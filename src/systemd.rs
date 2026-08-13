//! Integrates application lifecycle with systemd-compatible service managers.

use std::io;

use sd_notify::NotifyState;

/// Reports that application startup is complete.
///
/// This succeeds without sending anything when the process has no notification
/// socket.
pub fn ready() -> io::Result<()> {
    sd_notify::notify(false, &[NotifyState::Ready])
}

/// Reports that application startup is complete with a descriptive status.
///
/// This succeeds without sending anything when the process has no notification
/// socket.
pub fn ready_with_status(status: &str) -> io::Result<()> {
    sd_notify::notify(false, &[NotifyState::Ready, NotifyState::Status(status)])
}
