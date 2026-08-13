#![doc = include_str!("../README.md")]

use std::future::{pending, Future};

use tokio::signal::unix::{signal, Signal, SignalKind};
use tracing::{error, info};

pub mod config;
pub mod page;
mod request_context;

pub use request_context::RequestContext;

/// Registers conventional termination handlers and waits for either signal.
///
/// Registration failures are logged and omitted from the returned waiter.
/// Intended for use with [`axum::serve::Serve::with_graceful_shutdown`].
///
/// # Panics
///
/// Panics if called outside a Tokio runtime with signal support.
pub fn shutdown_signal() -> impl Future<Output = ()> {
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
    match signal(kind) {
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
