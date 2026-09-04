# Twelve-factor web applications

`twelve` provides reusable utilities for twelve-factor web applications built
with [`axum`](https://docs.rs/axum/latest/axum/).

> **Warning:** This crate is still exploratory. Minor releases before 1.0 may
> contain breaking API changes.

## Features

The `postgres` feature provides validated database configuration and SQLx pool
initialization. The `html` feature provides responses for traditional HTML
applications. Neither feature is enabled by default.

```toml
[dependencies]
twelve = { version = "0.4", features = ["postgres"] }
```

## Example

The following application loads its configuration, initializes tracing and
PostgreSQL, builds its routing explicitly, and serves requests with graceful
shutdown:

```ignore
use std::path::PathBuf;

use axum::{Json, Router, extract::State, routing::get};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use tower_http::{services::ServeDir, trace::TraceLayer};
use twelve::{
    config::{Core, DatabaseUrl},
    frontend::RouterExt,
};

#[derive(Deserialize)]
struct Config {
    #[serde(flatten)]
    core: Core,
    database_url: DatabaseUrl,
    frontend: PathBuf,
}

#[derive(Serialize)]
struct Pong {
    message: &'static str,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config: Config = twelve::config::from_args()?;
    twelve::logging::init(config.core.log_filter)?;

    let database = twelve::postgres::connect_and_migrate(
        config.database_url,
        &sqlx::migrate!(),
    )
    .await?;

    let api = Router::new()
        .route("/ping", get(ping))
        .with_frontend_version(&config.frontend);
    let frontend = Router::new()
        .fallback_service(
            ServeDir::new(config.frontend).append_index_html_on_directories(true),
        )
        .with_frontend_cache();
    let application = Router::new()
        .nest("/api", api)
        .merge(frontend)
        .layer(TraceLayer::new_for_http())
        .with_state(database);

    twelve::serve(&config.core.listen_address, application).await?;
    Ok(())
}

async fn ping(State(_database): State<PgPool>) -> Json<Pong> {
    Json(Pong { message: "pong" })
}
```
