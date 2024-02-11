use std::{
    net::{IpAddr, SocketAddr},
    path::PathBuf,
};

use axum::{body::Body, middleware, Router};
use serde::Deserialize;
use sqlx::{postgres::PgPoolOptions, PgPool};
use tower_http::{services::ServeDir, trace::TraceLayer};
use tracing::info;

use crate::{
    page::{MethodNotAllowed, NotFound},
    util,
};

#[derive(Debug)]
pub struct AppBuilder {
    pub config: Config,
}

impl AppBuilder {
    pub async fn begin() -> (PgPool, Self) {
        tracing_subscriber::fmt()
            .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
            .init();

        let config = Config::from_env();

        let pool = PgPoolOptions::new()
            .max_connections(5)
            .connect(&config.database_url)
            .await
            .expect("could not connect to database");

        (pool, Self { config })
    }

    pub async fn serve(self, router: Router) -> anyhow::Result<()> {
        let listen_addr = SocketAddr::from((IpAddr::from([0, 0, 0, 0]), self.config.port));
        info!("listening on {}", listen_addr);

        let service = router
            .nest_service("/_static", ServeDir::new(&self.config.static_dir))
            .layer(middleware::from_fn(MethodNotAllowed::middleware::<Body>))
            .fallback(NotFound::handler)
            .layer(TraceLayer::new_for_http());

        let listener = tokio::net::TcpListener::bind(listen_addr)
            .await
            .expect("could not bind to port");
        axum::serve(listener, service)
            .with_graceful_shutdown(util::graceful_shutdown::setup_and_wait_for_shutdown())
            .await?;

        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub struct Config {
    #[serde(default = "default_static_dir")]
    pub static_dir: PathBuf,
    pub database_url: String,
    #[serde(default = "default_port")]
    pub port: u16,
}

impl Config {
    #[inline]
    pub fn from_env() -> Self {
        envy::from_env().expect("could not parse configuration from environment")
    }
}

#[inline(always)]
fn default_static_dir() -> PathBuf {
    "./_static".into()
}

#[inline(always)]
fn default_port() -> u16 {
    3000
}
