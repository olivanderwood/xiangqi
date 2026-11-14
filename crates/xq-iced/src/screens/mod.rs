//! The app's screens. Every screen follows the module contract
//! documented in `crate::app`:
//!
//! ```ignore
//! pub struct State { /* ... */ }
//! pub enum Msg {
//!     Navigate(Screen), // intercepted by `App`, never handled here
//!     ShowSettings,     // intercepted by `App`, opens the dialog
//!     ShowAbout,        // intercepted by `App`, opens the dialog
//!     /* ... screen-specific variants ... */
//! }
//! impl State {
//!     pub fn new(shared: &Shared) -> Self;
//!     pub fn update(&mut self, msg: Msg, shared: &mut Shared) -> Task<Msg>;
//!     pub fn view(&self, shared: &Shared) -> Element<'_, Msg>;
//!     pub fn subscription(&self) -> Subscription<Msg>;
//! }
//! ```

pub mod history;
pub mod home;
pub mod learn;
pub mod learn_select;
pub mod play;
pub mod practice;
pub mod settings_dialog;
pub mod summary;
