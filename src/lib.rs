#![doc = include_str!("../README.md")]

mod config;
pub mod page;
pub mod util;

use std::net::{IpAddr, SocketAddr};

use axum::{body::Body, middleware, Router};
use serde::de::DeserializeOwned;
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::info;

use crate::page::{MethodNotAllowed, NotFound};
pub use config::TwelveConfig;

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

pub async fn serve(router: Router) {
    with_cfg(TwelveConfig::from_env(), router).await
}

pub async fn with_cfg(cfg: TwelveConfig, router: Router) {
    let listen_addr = SocketAddr::from((IpAddr::from([0, 0, 0, 0]), cfg.port));
    info!("listening on {}", listen_addr);

    let service = router
        .nest_service("/_static", ServeDir::new(&cfg.static_dir))
        .layer(middleware::from_fn(MethodNotAllowed::middleware::<Body>))
        .fallback(NotFound::handler)
        .layer(TraceLayer::new_for_http());

    let listener = tokio::net::TcpListener::bind(listen_addr)
        .await
        .expect("could not bind to port");
    axum::serve(listener, service)
        .with_graceful_shutdown(util::graceful_shutdown::setup_and_wait_for_shutdown())
        .await
        .expect("crashed");
}
