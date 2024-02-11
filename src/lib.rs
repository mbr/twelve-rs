#![doc = include_str!("../README.md")]

use std::net::{IpAddr, SocketAddr};

use axum::Router;
use serde::de::DeserializeOwned;
use tracing::info;

pub mod page;
pub mod util;

/// Perform initial setup
///
/// # Panics
///
/// Will panic if setting up the tracing subscriber fails.
pub fn setup<T: DeserializeOwned>() -> T {
    tracing_subscriber::fmt()
        .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
        .init();

    envy::from_env().expect("failed to parse configuration from environment")
}

pub async fn serve(port: u16, router: Router) {
    let listen_addr = SocketAddr::from((IpAddr::from([0, 0, 0, 0]), port));
    info!("listening on {}", listen_addr);

    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .expect("could not bind to port");
    axum::serve(listener, router)
        .with_graceful_shutdown(util::graceful_shutdown::setup_and_wait_for_shutdown())
        .await
        .expect("crashed");
}
