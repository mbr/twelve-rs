//! Opens PostgreSQL pools with application-supplied migrations.
//!
//! [`connect_and_migrate()`] constructs a default SQLx pool, establishes its
//! first connection, and applies an embedded [`Migrator`] before returning.
//! Applications retain ownership of their migrations, queries, and pool.
//!
//! ```ignore
//! # async fn example(
//! #     database_url: twelve::config::DatabaseUrl,
//! # ) -> Result<(), twelve::postgres::Error> {
//! let pool = twelve::postgres::connect_and_migrate(
//!     database_url,
//!     &sqlx::migrate!(),
//! )
//! .await?;
//! # drop(pool);
//! # Ok(())
//! # }
//! ```

use sqlx::{
    migrate::{MigrateError, Migrator},
    postgres::PgPoolOptions,
    PgPool,
};
use thiserror::Error;

use crate::config::DatabaseUrl;

/// Describes a failure to open a migrated PostgreSQL pool.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that a PostgreSQL connection could not be established.
    #[error("failed to connect to PostgreSQL")]
    Connect {
        /// Provides the underlying SQLx error.
        #[source]
        source: sqlx::Error,
    },

    /// Indicates that database migrations could not be applied.
    #[error("failed to migrate PostgreSQL")]
    Migrate {
        /// Provides the underlying migration error.
        #[source]
        source: MigrateError,
    },
}

/// Opens a PostgreSQL pool and applies the supplied migrations.
pub async fn connect_and_migrate(
    database_url: DatabaseUrl,
    migrator: &Migrator,
) -> Result<PgPool, Error> {
    let pool = PgPoolOptions::new()
        .connect_with(database_url.into_connect_options())
        .await
        .map_err(|source| Error::Connect { source })?;

    migrator
        .run(&pool)
        .await
        .map_err(|source| Error::Migrate { source })?;

    Ok(pool)
}
