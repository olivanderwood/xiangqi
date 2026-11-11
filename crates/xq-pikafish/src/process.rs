//! Child-process management for a UCI engine (integration spec §7.2).
//!
//! One [`Process`] owns the child, its stdin writer, a reader thread
//! pushing stdout lines into a channel, and a stderr drain thread. The
//! session (`uci.rs`) drives it; this module knows nothing about UCI.

use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

/// Events from the reader thread.
#[derive(Debug)]
pub enum UciEvent {
    /// One stdout line (without the trailing newline).
    Line(String),
    /// stdout hit EOF — the child exited or was killed. The exit code is
    /// available through [`Process::try_wait`].
    Exited,
    /// The reader thread hit an I/O error.
    ReadError(String),
}

/// A spawned UCI engine process.
pub struct Process {
    child: Child,
    stdin: ChildStdin,
    events: Receiver<UciEvent>,
    /// Last stderr lines (capped), for diagnostics when the child dies.
    stderr_tail: Arc<Mutex<Vec<String>>>,
}

impl Process {
    /// Spawn the engine. `cwd` is the engine directory (so a relative
    /// `EvalFile` would resolve there; we always pass an absolute path).
    pub fn spawn(exe: &Path, args: &[String], cwd: &Path) -> Result<Self, xq_engine::EngineError> {
        let mut cmd = Command::new(exe);
        cmd.args(args)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(cwd);
        #[cfg(windows)]
        cmd.creation_flags(0x0800_0000); // CREATE_NO_WINDOW: no console window
        let mut child = cmd
            .spawn()
            .map_err(|e| xq_engine::EngineError::Spawn(format!("{}: {e}", exe.display())))?;
        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = child.stdout.take().expect("stdout piped");
        let stderr = child.stderr.take().expect("stderr piped");
        let (tx, rx) = channel::<UciEvent>();
        // Reader thread: stdout lines -> channel.
        let reader_tx = tx.clone();
        std::thread::spawn(move || {
            let reader = BufReader::new(stdout);
            for line in reader.lines() {
                match line {
                    Ok(l) => {
                        if reader_tx.send(UciEvent::Line(l)).is_err() {
                            break;
                        }
                    }
                    Err(e) => {
                        let _ = reader_tx.send(UciEvent::ReadError(e.to_string()));
                        break;
                    }
                }
            }
            let _ = reader_tx.send(UciEvent::Exited);
        });
        // Stderr drain thread: keep the last lines for diagnostics.
        let stderr_tail: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
        let tail = Arc::clone(&stderr_tail);
        std::thread::spawn(move || {
            let reader = BufReader::new(stderr);
            for l in reader.lines().map_while(Result::ok) {
                let mut guard = tail.lock().expect("stderr tail lock");
                guard.push(l);
                if guard.len() > 100 {
                    guard.remove(0);
                }
            }
        });
        Ok(Self {
            child,
            stdin,
            events: rx,
            stderr_tail,
        })
    }

    /// Write one line to the child's stdin (adds the newline, flushes).
    pub fn send(&mut self, line: &str) -> Result<(), xq_engine::EngineError> {
        crate::log::log_trace(&format!("→ {line}"));
        self.stdin
            .write_all(line.as_bytes())
            .and_then(|()| self.stdin.write_all(b"\n"))
            .and_then(|()| self.stdin.flush())
            .map_err(|e| xq_engine::EngineError::Protocol(format!("ghi stdin: {e}")))
    }

    /// Non-blocking receive of the next event, if one is queued.
    pub fn try_recv_event(&self) -> Option<UciEvent> {
        self.events.try_recv().ok()
    }

    /// Receive the next event, waiting at most `timeout`.
    pub fn recv_event_timeout(&self, timeout: Duration) -> Option<UciEvent> {
        self.events.recv_timeout(timeout).ok()
    }

    /// Is the child still running?
    #[must_use]
    pub fn is_alive(&mut self) -> bool {
        matches!(self.child.try_wait(), Ok(None))
    }

    /// Exit status if the child has finished.
    pub fn try_wait(&mut self) -> Option<std::process::ExitStatus> {
        self.child.try_wait().ok().flatten()
    }

    /// Last stderr lines (newest last), for diagnostics.
    #[must_use]
    pub fn stderr_tail(&self) -> Vec<String> {
        self.stderr_tail.lock().expect("stderr tail lock").clone()
    }

    /// Ask the child to quit (`quit`), wait up to `timeout`, then kill it.
    pub fn quit(&mut self, timeout: Duration) {
        let _ = self.send("quit");
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Ok(Some(_)) = self.child.try_wait() {
                return;
            }
            if std::time::Instant::now() >= deadline {
                break;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Kill the child immediately and reap it.
    pub fn kill(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

impl Drop for Process {
    fn drop(&mut self) {
        // Best effort: a dropped session must not leak a child process.
        if matches!(self.child.try_wait(), Ok(None)) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }
}
