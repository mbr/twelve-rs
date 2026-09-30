//! Coordinates graceful Tokio service shutdown on `SIGTERM` or `SIGINT`.
//!
//! For an HTTP-only application, [`crate::serve()`] handles shutdown
//! automatically. When serving Axum directly, pass [`signal()`] to its
//! graceful-shutdown method.
//!
//! Use [`token()`] when multiple components need shutdown notification, or when
//! a cancellation token better fits your application. Create it once and
//! distribute clones to the components that should stop. For token-controlled
//! HTTP shutdown, use Axum directly: [`crate::serve()`] listens for signals
//! independently and does not observe your token.
//!
//! Both functions register signal handlers eagerly. Registration failures are
//! logged and ignored. `SIGQUIT` retains its default behavior.
//!
//! ```no_run
//! use axum::Router;
//! use tokio::net::TcpListener;
//! use twelve::shutdown;
//!
//! async fn serve(listener: TcpListener) -> std::io::Result<()> {
//!     axum::serve(listener, Router::new())
//!         .with_graceful_shutdown(shutdown::signal())
//!         .await
//! }
//! ```

use std::future::{pending, Future};

use tokio::signal::unix::{signal as register_unix_signal, Signal, SignalKind};
use tokio_util::sync::CancellationToken;
use tracing::{error, info};

/// Registers conventional termination handlers and waits for either signal.
///
/// Registration failures are logged and omitted from the returned waiter.
/// Intended for use with [`axum::serve::Serve::with_graceful_shutdown`].
/// For shared shutdown notification, see [`token()`].
///
/// # Panics
///
/// Panics if called outside a Tokio runtime with signal support.
pub fn signal() -> impl Future<Output = ()> {
    let mut terminate = register_signal(SignalKind::terminate(), "SIGTERM");
    let mut interrupt = register_signal(SignalKind::interrupt(), "SIGINT");

    async move {
        tokio::select! {
            _ = receive_signal(&mut terminate) => {
                info!(signal = "SIGTERM", "shutdown signal received");
            }
            _ = receive_signal(&mut interrupt) => {
                info!(signal = "SIGINT", "shutdown signal received");
            }
        }
    }
}

/// Returns a token cancelled on a termination signal.
///
/// Registers handlers eagerly and spawns a watcher that exits on a signal or
/// manual cancellation.
///
/// For an HTTP-only application, [`crate::serve()`] handles shutdown without
/// an explicit token.
///
/// # Panics
///
/// Panics if called outside a Tokio runtime with signal support.
pub fn token() -> CancellationToken {
    let signal = signal();
    let token = CancellationToken::new();
    tokio::spawn(cancel_on_signal(signal, token.clone()));
    token
}

/// Cancels the token on a signal or stops watching on manual cancellation.
async fn cancel_on_signal(signal: impl Future<Output = ()>, token: CancellationToken) {
    tokio::select! {
        () = signal => token.cancel(),
        () = token.cancelled() => {}
    }
}

/// Registers a shutdown signal while preserving startup on failure.
fn register_signal(kind: SignalKind, name: &'static str) -> Option<Signal> {
    match register_unix_signal(kind) {
        Ok(signal) => Some(signal),
        Err(error) => {
            error!(%error, signal = name, "failed to register shutdown signal");
            None
        }
    }
}

/// Waits for a registered signal or indefinitely when registration failed.
async fn receive_signal(signal: &mut Option<Signal>) {
    match signal {
        Some(signal) => {
            signal.recv().await;
        }
        None => pending().await,
    }
}

#[cfg(test)]
mod tests {
    use std::{
        future::{pending, Future},
        pin::pin,
        task::{Context, Waker},
    };

    use tokio::sync::oneshot;
    use tokio_util::sync::CancellationToken;

    use super::cancel_on_signal;

    /// Verifies that a signal cancels the shared token and completes the watcher.
    #[test]
    fn cancels_token_on_signal() {
        let token = CancellationToken::new();
        let (sender, receiver) = oneshot::channel();
        let mut watcher = pin!(cancel_on_signal(
            async { receiver.await.expect("signal sender should remain alive") },
            token.clone(),
        ));
        let mut context = Context::from_waker(Waker::noop());

        assert!(watcher.as_mut().poll(&mut context).is_pending());
        assert!(!token.is_cancelled());

        sender.send(()).expect("watcher should receive the signal");

        assert!(watcher.as_mut().poll(&mut context).is_ready());
        assert!(token.is_cancelled());
    }

    /// Verifies that manual cancellation stops a watcher with no signal.
    #[test]
    fn stops_watching_on_manual_cancellation() {
        let token = CancellationToken::new();
        let mut watcher = pin!(cancel_on_signal(pending(), token.clone()));
        let mut context = Context::from_waker(Waker::noop());

        assert!(watcher.as_mut().poll(&mut context).is_pending());

        token.cancel();

        assert!(watcher.as_mut().poll(&mut context).is_ready());
    }
}
