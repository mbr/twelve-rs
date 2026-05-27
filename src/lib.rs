#![doc = include_str!("../README.md")]

pub mod page;
mod request_context;

pub use request_context::RequestContext;
use tracing::info;

/// Waits for SIGTERM, SIGINT, or SIGQUIT and returns.
///
/// Intended for use with [`axum::serve::Serve::with_graceful_shutdown`].
///
/// # Panics
///
/// Panics if signal handlers cannot be registered.
pub async fn shutdown_signal() {
    use tokio::signal::unix::{signal, SignalKind};

    let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM handler");
    let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT handler");
    let mut sigquit = signal(SignalKind::quit()).expect("failed to register SIGQUIT handler");

    tokio::select! {
        _ = sigterm.recv() => info!("received SIGTERM, shutting down"),
        _ = sigint.recv() => info!("received SIGINT, shutting down"),
        _ = sigquit.recv() => info!("received SIGQUIT, shutting down"),
    }
}
