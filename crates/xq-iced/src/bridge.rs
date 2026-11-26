//! Bridge from the worker threads (`std::sync::mpsc`) to Iced
//! `Subscription`s.
//!
//! The workers in this crate (`ai_worker.rs`, `chessdb_worker.rs`)
//! are unchanged from the egui app: each owns a background thread
//! that answers on a `std::sync::mpsc::Receiver`. In egui the UI
//! polled that receiver once per frame; in Iced (Elm architecture)
//! the UI must not poll — values arrive as `Message`s pushed by a
//! [`Subscription`]. This module is the small adapter between the
//! two worlds.
//!
//! # How it works (and why this shape, iced 0.14)
//!
//! Iced 0.14 has no `subscription::channel` helper (that existed in
//! older versions), and `Subscription::run` / `run_with` only take
//! `fn` pointers, so a closure capturing the receiver cannot be
//! passed to them directly. The way to carry owned data into a
//! subscription is a custom [`Recipe`] via
//! [`from_recipe`](iced::advanced::subscription::from_recipe)
//! (re-exported from `iced::advanced::subscription`), which is what
//! this module does:
//!
//! 1. `from_mpsc` builds a `Recipe` holding the shared receiver and
//!    a stable `id` (the recipe's identity — see below).
//! 2. When the subscription activates, the recipe's `stream()`
//!    builds an [`iced::stream::channel`] stream and spawns ONE
//!    forwarder thread. That thread does the blocking `recv()` on
//!    the std channel and forwards each item with `try_send` into
//!    the async channel. The async side then just awaits
//!    `pending()` forever; it never blocks an executor thread.
//! 3. When the subscription is dropped, the async channel closes,
//!    the forwarder's next `try_send` fails with `Disconnected`,
//!    and the thread exits on its own. (If it is parked in a
//!    blocking `recv()` with no sender left, it exits as soon as
//!    the next item arrives or the worker thread ends.)
//!
//! # Rules for callers (P1/P2/P3 screens)
//!
//! - **One bridge per receiver, created once.** The `id` is the
//!   subscription's identity: iced keeps the stream alive as long
//!   as a subscription with the same hash is returned, and drops
//!   it when the id disappears or changes. Use a distinct
//!   `&'static str` per worker (`"ai"`, `"chessdb"`, …) and return
//!   the same bridge from `subscription()` on every call — do not
//!   mint a fresh id per frame.
//! - **Never subscribe to a bridge on wasm32.** The recipe's stream
//!   spawns a forwarder thread (`std::thread::spawn` panics on
//!   wasm). On wasm the screens drain their channels from the tick
//!   handler instead (the AI `WasmAiWorker` in `ai_worker.rs`; the
//!   ChessDB panel answers "unavailable" at once), and the
//!   Pikafish-install bridge does not exist there (`App` has no
//!   install receiver on wasm). `from_mpsc` still compiles for
//!   wasm — it just must never be subscribed to there.
//! - **The bridged receiver has exactly one consumer: the
//!   forwarder thread.** The thread holds the `Mutex` across the
//!   blocking `recv()` (a std `Receiver` is `Send` but not `Sync`,
//!   so it cannot be shared without the lock). Do NOT also call
//!   `try_recv()` on the same receiver while it is bridged — that
//!   would block on the mutex. To hand a worker's receiver to the
//!   bridge, move it out of the worker struct (e.g. destructure
//!   `AiWorker` at construction) and wrap it:
//!   `Arc::new(Mutex::new(rx))`.
//! - **Stale responses are still filtered by `req_id`.** The bridge
//!   delivers every response in order; the screens must keep
//!   ignoring responses whose `req_id` is not the one they are
//!   waiting for (`ai_worker::response_is_current` /
//!   `chessdb_worker::response_is_current`), exactly as the egui
//!   screens did.
//! - `T` must be `Send + 'static` (it crosses into the forwarder
//!   thread). `AiResponse` and `ChessDbResponse` both satisfy this.
//!
//! # Example
//!
//! ```ignore
//! use std::sync::{Arc, Mutex};
//!
//! // At startup, after creating the worker:
//! let AiWorker { tx, rx, stop, next_req_id, fallback_notice } = worker;
//! let ai_rx = Arc::new(Mutex::new(rx));
//!
//! // In `App::subscription`:
//! fn subscription(&self) -> Subscription<Message> {
//!     bridge::from_mpsc("ai", Arc::clone(&self.ai_rx)).map(Message::Ai)
//! }
//! ```

use std::hash::Hash;
use std::sync::mpsc::Receiver;
use std::sync::{Arc, Mutex};
/// Desktop only: the forwarder thread's retry sleep.
#[cfg(not(target_arch = "wasm32"))]
use std::time::Duration;

use iced::advanced::subscription::{from_recipe, EventStream, Hasher, Recipe};
/// Desktop only: the forwarder thread's stream machinery. On wasm
/// `stream()` is an empty stream and these are unused.
#[cfg(not(target_arch = "wasm32"))]
use iced::futures::future;
#[cfg(not(target_arch = "wasm32"))]
use iced::futures::stream::BoxStream;
#[cfg(not(target_arch = "wasm32"))]
use iced::stream;
use iced::Subscription;

/// Items the forwarder buffers before the UI consumes them. Worker
/// answers arrive in small bursts (`Info` lines, then the final
/// answer), so 100 is far above anything a single search produces;
/// if the buffer ever fills, the forwarder waits instead of
/// dropping — see `forward`.
const DEFAULT_BUFFER: usize = 100;

/// Turn a shared std-mpsc receiver into an Iced [`Subscription`]
/// that yields each received item as a message.
///
/// See the module docs for the rules: one bridge per receiver,
/// stable `id`, and no other consumer of the same receiver while
/// the subscription is alive.
pub fn from_mpsc<T>(id: &'static str, rx: Arc<Mutex<Receiver<T>>>) -> Subscription<T>
where
    T: Send + 'static,
{
    from_mpsc_buffered(id, rx, DEFAULT_BUFFER)
}

/// Same as [`from_mpsc`], with an explicit async-channel buffer
/// size. Only needed if a worker can burst more than
/// [`DEFAULT_BUFFER`] items before the UI drains them.
pub fn from_mpsc_buffered<T>(
    id: &'static str,
    rx: Arc<Mutex<Receiver<T>>>,
    buffer: usize,
) -> Subscription<T>
where
    T: Send + 'static,
{
    from_recipe(MpscRecipe {
        id,
        rx,
        buffer: buffer.max(1),
    })
}

/// The [`Recipe`] behind [`from_mpsc`]: it owns the shared receiver
/// and spawns the forwarder thread when its stream starts.
///
/// The `allow(dead_code)` is for wasm, where the fields exist only
/// so `from_mpsc` keeps compiling (`stream()` yields nothing there).
#[cfg_attr(target_arch = "wasm32", allow(dead_code))]
struct MpscRecipe<T> {
    id: &'static str,
    rx: Arc<Mutex<Receiver<T>>>,
    buffer: usize,
}

impl<T> Recipe for MpscRecipe<T>
where
    T: Send + 'static,
{
    type Output = T;

    fn hash(&self, state: &mut Hasher) {
        std::any::TypeId::of::<Self>().hash(state);
        self.id.hash(state);
    }

    /// wasm32: never subscribed to (see the module docs — the
    /// forwarder thread would panic there), so this is a stream
    /// that yields nothing. It only exists so the crate still
    /// compiles for wasm. The return type is spelled out instead
    /// of the `BoxStream` alias: the trait's `BoxStream` is
    /// `iced_futures`' platform one (`LocalBoxStream`, not `Send`,
    /// on wasm), while `iced::futures::stream::BoxStream` is the
    /// `futures` crate's (always `Send`) — the two only agree on
    /// native targets.
    #[cfg(target_arch = "wasm32")]
    fn stream(
        self: Box<Self>,
        _input: EventStream,
    ) -> std::pin::Pin<Box<dyn iced::futures::Stream<Item = Self::Output> + 'static>> {
        let _ = self;
        Box::pin(iced::futures::stream::empty())
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn stream(self: Box<Self>, _input: EventStream) -> BoxStream<'static, Self::Output> {
        let rx = self.rx;
        let buffer = self.buffer;
        Box::pin(stream::channel(buffer, async move |sender| {
            // The blocking `recv()` must not run on the async
            // executor: hand the sender to a plain thread. The
            // sender is `Send` when `T: Send`, so this is safe.
            std::thread::spawn(move || forward(rx, sender));
            // Park the async side forever; the stream only ends
            // when the subscription (and with it this future) is
            // dropped, which closes the channel and lets the
            // forwarder thread exit.
            future::pending::<()>().await;
        }))
    }
}

/// Forwarder thread body: blocking-receive from the std channel and
/// push each item into the async channel. Exits when the worker's
/// sender is gone or the subscription side has closed.
/// Desktop only (wasm has no forwarder thread).
#[cfg(not(target_arch = "wasm32"))]
fn forward<T>(rx: Arc<Mutex<Receiver<T>>>, mut sender: iced::futures::channel::mpsc::Sender<T>)
where
    T: Send,
{
    loop {
        // The mutex is held across the blocking `recv()` on purpose:
        // a std `Receiver` is not `Sync`, and this thread is the
        // receiver's only consumer while bridged (see module docs).
        let item = {
            let Ok(guard) = rx.lock() else {
                break; // poisoned — nothing sane left to do
            };
            guard.recv()
        };
        let Ok(mut item) = item else {
            break; // all senders dropped — the worker is gone
        };
        // `try_send` fails with `Full` when the UI has not drained
        // the buffer yet: wait briefly and retry with the same item
        // instead of dropping it. Only `Disconnected` ends the loop.
        loop {
            match sender.try_send(item) {
                Ok(()) => break,
                Err(err) => {
                    if err.is_disconnected() {
                        return;
                    }
                    item = err.into_inner();
                    std::thread::sleep(Duration::from_millis(1));
                }
            }
        }
    }
}
