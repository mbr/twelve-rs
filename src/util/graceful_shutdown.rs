//! Graceful shutdown handling
//!
//! An important aspect of an application is proper handling of shutdown requests, typically
//! performed by sending a `SIGTERM` to the running application.
//!
//! This is especially important when running the application inside a container in the
//! single-application-per-container style popularized by
//! [Docker](https://en.wikipedia.org/wiki/Docker_(software)): While applications that do not setup
//! a handler for `SIGTERM` will automatically have one set up for them that terminates when
//! receiving such a signal, any application running as init (PID 0) will need to explicitly
//! register a handler, otherwise the signal will be ignored. Since most idiomatic containers run on
//! conventional container runtimes cause their contained binary to be run indeed as PID 0, any
//! `SIGTERM` sent to them will be ignored by default.
//!
//! As a result, containers running these applications will usually not respond to a `SIGTERM` and
//! run into a timeout (which is usually defaulting to 10 seconds) before being sent a `SIGKILL` by
//! the container runtime instead, which cannot be ignored. This causes both a delay in shutting
//! down, as well as potentially unclean shutdowns.
//!
//! To fix this, a utility function is provided in [`setup_and_wait_for_shutdown`], which is meant
//! to be registered using [`axum::serve::Serve::with_graceful_shutdown`].

use std::future::Future;

use async_signal::Signal;
use futures_lite::StreamExt;
use tracing::{info, warn};

/// Setups a signal handler for `TERM`, `INT` and `QUIT` signals, and returns a future awaiting
/// them.
///
/// # Panics
///
/// Will panic is there are any issues registering the signal handlers.
pub fn setup_and_wait_for_shutdown() -> impl Future<Output = ()> {
    let mut signal_listener =
        async_signal::Signals::new(&[Signal::Term, Signal::Int, Signal::Quit])
            .expect("could not register signal handler for graceful shutdown");

    async move {
        match signal_listener.next().await {
            Some(Err(err)) => {
                warn!(%err, "signal handler error, shutting down");
            }
            Some(Ok(signal)) => {
                info!(?signal, "shutting down after signal")
            }
            None => {
                unreachable!()
            }
        }
    }
}
