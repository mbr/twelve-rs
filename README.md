# Twelve factor webapp crate

`twelve` provides reusable utilities for twelve-factor web applications built
with [`axum`](https://docs.rs/axum/latest/axum/).

> **Warning:** Parts of this crate are still exploratory. Expect substantial API breakage between major versions.

## Example

The following application loads its configuration, initializes tracing and
PostgreSQL, builds its routing explicitly, and serves requests with graceful
shutdown:

```ignore
use std::path::PathBuf;

use axum::{Router, extract::State, routing::get};
use serde::Deserialize;
use sqlx::PgPool;
use tower_http::services::ServeDir;
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
        .route("/status", get(status))
        .with_frontend_version(&config.frontend);
    let frontend = Router::new()
        .fallback_service(
            ServeDir::new(config.frontend).append_index_html_on_directories(true),
        )
        .with_frontend_cache();
    let application = Router::new()
        .nest("/api", api)
        .merge(frontend)
        .with_state(database);

    twelve::serve(&config.core.listen_address, application).await?;
    Ok(())
}

async fn status(State(_database): State<PgPool>) -> &'static str {
    "ok"
}
```
