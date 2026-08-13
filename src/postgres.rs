//! Provides PostgreSQL configuration types.

use std::str::FromStr;

use sec::Secret;
use serde::Deserialize;
use sqlx::postgres::PgConnectOptions;
use thiserror::Error;

/// Holds validated PostgreSQL connection options without exposing credentials.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct DatabaseUrl(Secret<PgConnectOptions>);

impl DatabaseUrl {
    /// Returns the parsed PostgreSQL connection options.
    pub fn into_connect_options(self) -> PgConnectOptions {
        self.0.reveal_into()
    }
}

impl FromStr for DatabaseUrl {
    type Err = ParseDatabaseUrlError;

    /// Parses PostgreSQL connection options from a URL.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        let scheme = value
            .split_once(':')
            .map(|(scheme, _)| scheme)
            .ok_or(ParseDatabaseUrlError::Scheme)?;
        if !scheme.eq_ignore_ascii_case("postgres") && !scheme.eq_ignore_ascii_case("postgresql") {
            return Err(ParseDatabaseUrlError::Scheme);
        }

        value
            .parse()
            .map(Secret::new)
            .map(Self)
            .map_err(|source| ParseDatabaseUrlError::Invalid { source })
    }
}

impl TryFrom<String> for DatabaseUrl {
    type Error = ParseDatabaseUrlError;

    /// Parses owned PostgreSQL connection options from a URL.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// Describes an invalid PostgreSQL connection URL.
#[derive(Debug, Error)]
pub enum ParseDatabaseUrlError {
    /// Indicates that the URL does not select PostgreSQL.
    #[error("expected a postgres or postgresql URL")]
    Scheme,

    /// Indicates that the PostgreSQL URL is malformed.
    #[error("invalid PostgreSQL connection URL")]
    Invalid {
        /// Provides the underlying SQLx configuration error.
        #[source]
        source: sqlx::Error,
    },
}

#[cfg(test)]
mod tests {
    use super::DatabaseUrl;

    /// Validates connection URLs without exposing credentials through diagnostics.
    #[test]
    fn validates_and_redacts_database_urls() {
        let url: DatabaseUrl = "postgresql://user:password@localhost/database"
            .parse()
            .expect("database URL should parse");

        assert_eq!(format!("{url:?}"), "DatabaseUrl(...)");
        assert!("http://localhost/database".parse::<DatabaseUrl>().is_err());
    }
}
