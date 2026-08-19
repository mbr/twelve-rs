//! Waits for conventional Unix termination signals for graceful shutdown.
//!
//! This module is useful for long-running Tokio services that should stop
//! accepting work on `SIGTERM` or `SIGINT` while allowing in-flight work to
//! complete. [`signal()`] registers both handlers eagerly and resolves when the
//! first registered signal arrives. Registration failures are logged instead
//! of preventing startup, and `SIGQUIT` retains its default behavior.
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
use tracing::{error, info};

/// Registers conventional termination handlers and waits for either signal.
///
/// Registration failures are logged and omitted from the returned waiter.
/// Intended for use with [`axum::serve::Serve::with_graceful_shutdown`].
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
