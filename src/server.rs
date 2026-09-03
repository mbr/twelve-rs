//! Runs Axum routers with Twelve's listener and shutdown handling.

use std::io;

use axum::Router;
use thiserror::Error;
use tracing::info;

use crate::{
    config::ListenAddress,
    listener::{self, Listener},
    shutdown,
};

/// Runs an Axum router until process shutdown.
pub async fn serve(listen_address: &ListenAddress, application: Router) -> Result<(), ServeError> {
    let listener = Listener::bind(listen_address).await?;

    info!(address = %listener.local_address(), "HTTP server listening");

    axum::serve(listener, application)
        .with_graceful_shutdown(shutdown::signal())
        .await
        .map_err(|source| ServeError::Serve { source })?;

    info!("HTTP server stopped");
    Ok(())
}

/// Describes an HTTP server failure.
#[derive(Debug, Error)]
pub enum ServeError {
    /// Indicates that the configured listener could not be opened.
    #[error(transparent)]
    Listener(#[from] listener::Error),

    /// Indicates that Axum failed while serving requests.
    #[error("failed to serve HTTP")]
    Serve {
        /// Provides the underlying I/O error.
        #[source]
        source: io::Error,
    },
}
