use tracing::{info, warn};

/// Setups a signal handler for `TERM`, `INT` and `QUIT` signals, and returns a future awaiting
/// them.
///
/// # Panics
///
/// Will panic is there are any issues registering the signal handlers.
pub fn setup_and_wait_for_shutdown() -> impl Future<Output = ()> {
    let mut signal_listener =
        async_signal::Signals::new(&[Signal::Term, Signal::Int, Signal::Quit])?;

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
