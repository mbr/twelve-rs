//! Loads app configuration from a file or standard input.
//!
//! [`from_args()`] parses a TOML configuration file named on the command line,
//! or standard input if `-` is passed instead. Applications with flat
//! configuration may prefer [`envy`](https://docs.rs/envy).
//!
//! [`Core`] contains common configuration for most web applications and can be
//! flattened into an existing configuration type.
//!
//! ```no_run
//! use serde::Deserialize;
//! use twelve::config::{self, Core};
//!
//! #[derive(Deserialize)]
//! struct Config {
//!     #[serde(flatten)]
//!     core: Core,
//! }
//!
//! # fn main() -> Result<(), config::Error> {
//! let configuration: Config = config::from_args()?;
//! # let _ = configuration.core;
//! # Ok(())
//! # }
//! ```

use std::{
    env,
    ffi::OsString,
    fmt::{self, Display, Formatter},
    fs,
    io::{self, Read},
    net::{AddrParseError, SocketAddr},
    path::{Path, PathBuf},
    str::FromStr,
};

#[cfg(feature = "postgres")]
use sec::Secret;
use serde::{de::DeserializeOwned, Deserialize};
#[cfg(feature = "postgres")]
use sqlx::postgres::PgConnectOptions;
use thiserror::Error;
use tracing_subscriber::{filter::ParseError, EnvFilter};

/// Identifies the source of a configuration document.
#[derive(Debug, Eq, PartialEq)]
pub enum Location {
    /// Reads configuration from standard input.
    StandardInput,

    /// Reads configuration from a file.
    File(PathBuf),
}

impl Location {
    /// Reads the configuration document into a string.
    fn read_to_string(&self) -> io::Result<String> {
        match self {
            Self::StandardInput => {
                let mut serialized = String::new();
                io::stdin()
                    .lock()
                    .read_to_string(&mut serialized)
                    .map(|_| serialized)
            }
            Self::File(path) => fs::read_to_string(path),
        }
    }
}

impl From<PathBuf> for Location {
    /// Converts a path into a configuration location.
    fn from(path: PathBuf) -> Self {
        if path.as_path() == Path::new("-") {
            Self::StandardInput
        } else {
            Self::File(path)
        }
    }
}

impl From<OsString> for Location {
    /// Converts a process argument into a configuration location.
    fn from(value: OsString) -> Self {
        PathBuf::from(value).into()
    }
}

impl Display for Location {
    /// Formats the location for diagnostics.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::StandardInput => formatter.write_str("standard input"),
            Self::File(path) => path.display().fmt(formatter),
        }
    }
}

/// Identifies an HTTP listener.
///
/// Numeric IPv4 and IPv6 socket addresses select TCP. Absolute filesystem
/// paths select Unix-domain sockets. Hostnames and relative paths are not
/// accepted.
///
/// ```
/// use twelve::config::ListenAddress;
///
/// let ipv4: ListenAddress = "127.0.0.1:3000".parse()?;
/// let ipv6: ListenAddress = "[::1]:3000".parse()?;
/// let unix: ListenAddress = "/run/myapp/http.sock".parse()?;
///
/// assert!(matches!(ipv4, ListenAddress::Tcp(_)));
/// assert!(matches!(ipv6, ListenAddress::Tcp(_)));
/// assert!(matches!(unix, ListenAddress::Unix(_)));
/// # Ok::<(), twelve::config::ParseListenAddressError>(())
/// ```
#[derive(Debug, Deserialize, Eq, PartialEq)]
#[serde(try_from = "String")]
pub enum ListenAddress {
    /// Listens on a TCP socket.
    Tcp(SocketAddr),

    /// Listens on a Unix-domain socket.
    Unix(PathBuf),
}

impl FromStr for ListenAddress {
    type Err = ParseListenAddressError;

    /// Parses a TCP address or Unix socket path.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        if Path::new(value).is_absolute() {
            Ok(Self::Unix(PathBuf::from(value)))
        } else {
            value
                .parse()
                .map(Self::Tcp)
                .map_err(|source| ParseListenAddressError { source })
        }
    }
}

impl TryFrom<String> for ListenAddress {
    type Error = ParseListenAddressError;

    /// Parses an owned listener address.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl Display for ListenAddress {
    /// Formats the listener address.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Self::Tcp(address) => address.fmt(formatter),
            Self::Unix(path) => path.display().fmt(formatter),
        }
    }
}

/// Describes an invalid HTTP listener address.
#[derive(Debug, Error)]
#[error("expected a TCP socket address or absolute Unix socket path")]
pub struct ParseListenAddressError {
    /// Provides the underlying TCP address error.
    #[source]
    source: AddrParseError,
}

/// Holds a validated tracing filter.
///
/// The default enables informational events while limiting Axum and tower-http
/// internals to warnings and errors.
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct LogFilter(EnvFilter);

impl Default for LogFilter {
    /// Constructs the production-oriented default filter.
    fn default() -> Self {
        "info,tower_http=warn,axum=warn"
            .parse()
            .expect("default log filter should be valid")
    }
}

impl FromStr for LogFilter {
    type Err = ParseError;

    /// Parses a tracing filter.
    fn from_str(value: &str) -> Result<Self, Self::Err> {
        EnvFilter::try_new(value).map(Self)
    }
}

impl TryFrom<String> for LogFilter {
    type Error = ParseError;

    /// Parses an owned tracing filter.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

impl From<LogFilter> for EnvFilter {
    /// Extracts the validated tracing filter.
    fn from(filter: LogFilter) -> Self {
        filter.0
    }
}

impl Display for LogFilter {
    /// Formats the tracing filter.
    fn fmt(&self, formatter: &mut Formatter<'_>) -> fmt::Result {
        self.0.fmt(formatter)
    }
}

/// Holds validated PostgreSQL connection options without exposing credentials.
#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
#[derive(Clone, Debug, Deserialize)]
#[serde(try_from = "String")]
pub struct DatabaseUrl(Secret<PgConnectOptions>);

#[cfg(feature = "postgres")]
impl DatabaseUrl {
    /// Returns the parsed PostgreSQL connection options.
    pub fn into_connect_options(self) -> PgConnectOptions {
        self.0.reveal_into()
    }
}

#[cfg(feature = "postgres")]
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

#[cfg(feature = "postgres")]
impl TryFrom<String> for DatabaseUrl {
    type Error = ParseDatabaseUrlError;

    /// Parses owned PostgreSQL connection options from a URL.
    fn try_from(value: String) -> Result<Self, Self::Error> {
        value.parse()
    }
}

/// Describes an invalid PostgreSQL connection URL.
#[cfg(feature = "postgres")]
#[cfg_attr(docsrs, doc(cfg(feature = "postgres")))]
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

/// Provides configuration shared by web applications.
///
/// This can be flattened into application-specific Serde configuration.
#[derive(Debug, Deserialize)]
pub struct Core {
    /// Selects the address on which the HTTP server listens.
    pub listen_address: ListenAddress,

    /// Selects the tracing events emitted by the application.
    #[serde(default)]
    pub log_filter: LogFilter,
}

/// Describes a failure to resolve or load application configuration.
#[derive(Debug, Error)]
pub enum Error {
    /// Indicates that no configuration source was provided.
    #[error("configuration file path is required")]
    MissingPath,

    /// Indicates that more than one configuration source was provided.
    #[error("unexpected argument after configuration file path")]
    UnexpectedArgument,

    /// Indicates that the configuration source could not be read.
    #[error("failed to read configuration from {location}")]
    Read {
        /// Identifies the configuration source.
        location: Location,

        /// Provides the underlying input error.
        #[source]
        source: io::Error,
    },

    /// Indicates that the configuration could not be deserialized.
    #[error("failed to parse configuration from {location}")]
    Parse {
        /// Identifies the configuration source.
        location: Location,

        /// Provides the underlying TOML error.
        #[source]
        source: toml::de::Error,
    },
}

/// Loads application configuration from the sole process argument.
///
/// The argument identifies a TOML file, or standard input when it is `-`.
pub fn from_args<T>() -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let mut arguments = env::args_os().skip(1);
    let path = arguments.next().ok_or(Error::MissingPath)?;
    if arguments.next().is_some() {
        return Err(Error::UnexpectedArgument);
    }

    load_location(path.into())
}

/// Loads application configuration from a TOML file or standard input.
///
/// A path equal to `-` reads the document from standard input.
pub fn load<T>(path: &Path) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    load_location(path.to_owned().into())
}

/// Loads application configuration from a resolved location.
fn load_location<T>(location: Location) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    let serialized = match location.read_to_string() {
        Ok(serialized) => serialized,
        Err(source) => return Err(Error::Read { location, source }),
    };

    deserialize(&serialized, location)
}

/// Deserializes a TOML document with source-aware diagnostics.
fn deserialize<T>(serialized: &str, location: Location) -> Result<T, Error>
where
    T: DeserializeOwned,
{
    toml::from_str(serialized).map_err(|source| Error::Parse { location, source })
}

#[cfg(test)]
mod tests {
    use std::{net::SocketAddr, path::PathBuf};

    use serde::Deserialize;

    #[cfg(feature = "postgres")]
    use super::DatabaseUrl;
    use super::{deserialize, Core, ListenAddress, Location};

    /// Provides application-specific fields around shared configuration.
    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Config {
        /// Provides shared web application configuration.
        #[serde(flatten)]
        core: Core,

        /// Selects an application-specific frontend directory.
        frontend: PathBuf,
    }

    /// Deserializes flattened shared configuration.
    #[test]
    fn deserializes_flattened_core_configuration() {
        let config: Config = deserialize(
            concat!(
                "listen_address = '127.0.0.1:3000'\n",
                "log_filter = 'twelve=debug,tower_http=info'\n",
                "frontend = '/srv/frontend'\n",
            ),
            Location::File(PathBuf::from("test")),
        )
        .expect("configuration should deserialize");

        assert_eq!(
            config.core.listen_address,
            ListenAddress::Tcp(SocketAddr::from(([127, 0, 0, 1], 3000)))
        );
        assert_eq!(
            config.core.log_filter.to_string(),
            "tower_http=info,twelve=debug"
        );
        assert_eq!(config.frontend, PathBuf::from("/srv/frontend"));
    }

    /// Uses the production log filter when it is omitted.
    #[test]
    fn defaults_log_filter() {
        let config: Config = deserialize(
            concat!(
                "listen_address = '127.0.0.1:3000'\n",
                "frontend = '/srv/frontend'\n",
            ),
            Location::File(PathBuf::from("test")),
        )
        .expect("configuration should deserialize");

        assert_eq!(
            config.core.log_filter.to_string(),
            "tower_http=warn,axum=warn,info"
        );
    }

    /// Parses each supported listener address family.
    #[test]
    fn parses_listener_addresses() {
        let ipv6: ListenAddress = "[::1]:3000".parse().expect("IPv6 listener should parse");
        let unix: ListenAddress = "/run/myapp/http.sock"
            .parse()
            .expect("Unix listener should parse");

        assert_eq!(
            ipv6,
            ListenAddress::Tcp(SocketAddr::from(([0, 0, 0, 0, 0, 0, 0, 1], 3000)))
        );
        assert_eq!(
            unix,
            ListenAddress::Unix(PathBuf::from("/run/myapp/http.sock"))
        );
        assert!("systemd".parse::<ListenAddress>().is_err());
        assert!("myapp.sock".parse::<ListenAddress>().is_err());
    }

    /// Validates connection URLs without exposing credentials through diagnostics.
    #[cfg(feature = "postgres")]
    #[test]
    fn validates_and_redacts_database_urls() {
        let url: DatabaseUrl = "postgresql://user:password@localhost/database"
            .parse()
            .expect("database URL should parse");

        assert_eq!(format!("{url:?}"), "DatabaseUrl(...)");
        assert!("http://localhost/database".parse::<DatabaseUrl>().is_err());
    }
}
