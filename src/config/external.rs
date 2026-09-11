//! Loads strings from literal values, files, or environment variables.
//!
//! ```toml
//! api_token = { file = "/run/credentials/myapp.service/api-token" }
//! # Alternatively: api_token = "dummy-dev-key"
//! # Alternatively: api_token = { env = "API_TOKEN" }
//! ```
//!
//! For credentials, use `sec` with its `deserialize` feature:
//!
//! ```no_run
//! use sec::Secret;
//! use serde::Deserialize;
//! use twelve::config::external::External;
//!
//! #[derive(Debug, Deserialize)]
//! struct Config {
//!     api_token: Secret<External>,
//! }
//!
//! let config: Config = twelve::config::from_args()?;
//! let token: Secret<String> = config.api_token.try_map_revealed(External::load)?;
//! # Ok::<(), Box<dyn std::error::Error>>(())
//! ```
//!
//! Loading is explicit and preserves whitespace, including trailing newlines.
//! Files must contain UTF-8 text; relative paths use the working directory.
//! [`External`] itself does not redact values. A `Secret` wrapper redacts the
//! value, but not source excerpts produced by TOML parse errors.

use std::{env, fs, io, path::PathBuf};

use serde::Deserialize;
use thiserror::Error;

/// Selects a literal string or an external source to load explicitly.
#[derive(Clone, Debug, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub enum External {
    /// Reads the contents of a UTF-8 file.
    File(PathBuf),

    /// Reads a Unicode environment variable.
    Env(String),

    /// Contains the literal value.
    #[serde(untagged)]
    Value(String),
}

impl External {
    /// Resolves the source once, without trimming or expanding its contents.
    pub fn load(self) -> Result<String, LoadError> {
        match self {
            Self::Value(value) => Ok(value),
            Self::File(path) => {
                fs::read_to_string(&path).map_err(|source| LoadError::File { path, source })
            }
            Self::Env(name) => env::var(&name).map_err(|source| match source {
                env::VarError::NotPresent => LoadError::MissingEnv { name },
                env::VarError::NotUnicode(_) => LoadError::NonUnicodeEnv { name },
            }),
        }
    }
}

/// Describes a loading failure without retaining file or environment contents.
#[derive(Debug, Error)]
pub enum LoadError {
    /// Indicates that a file could not be read as UTF-8 text.
    #[error("failed to read external value from {path}")]
    File {
        /// Identifies the file.
        path: PathBuf,

        /// Provides the underlying input error.
        #[source]
        source: io::Error,
    },

    /// Indicates that an environment variable is absent.
    #[error("environment variable {name} is not set")]
    MissingEnv {
        /// Identifies the environment variable.
        name: String,
    },

    /// Indicates that an environment value is not Unicode.
    #[error("environment variable {name} is not Unicode")]
    NonUnicodeEnv {
        /// Identifies the environment variable.
        name: String,
    },
}

#[cfg(test)]
mod tests {
    use std::{env, fs, io::ErrorKind, process::Command};
    #[cfg(unix)]
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};

    use sec::Secret;
    use serde::Deserialize;
    use tempfile::tempdir;

    use super::{External, LoadError};

    /// Holds a credential source without exposing its literal value.
    #[derive(Debug, Deserialize)]
    struct Config {
        /// Selects the credential source.
        token: Secret<External>,
    }

    /// Deserializes sources without loading them and redacts wrapped values.
    #[test]
    fn deserializes_sources() {
        for (value, expected) in [
            ("'dummy-token'", External::Value("dummy-token".into())),
            ("{ file = 'token.txt' }", External::File("token.txt".into())),
            ("{ env = 'API_TOKEN' }", External::Env("API_TOKEN".into())),
        ] {
            let config: Config = toml::from_str(&format!("token = {value}"))
                .expect("source should deserialize without loading");
            assert_eq!(format!("{config:?}"), "Config { token: ... }");
            assert_eq!(config.token.reveal_into(), expected);
        }
    }

    /// Rejects ambiguous sources, unknown fields, and non-string values.
    #[test]
    fn rejects_invalid_sources() {
        for value in [
            "{}",
            "{ file = 'token', env = 'TOKEN' }",
            "{ file = 'token', typo = true }",
            "{ env = 'TOKEN', typo = true }",
            "{ value = 'token' }",
            "{ file = 42 }",
            "{ env = 42 }",
            "42",
            "true",
            "['token']",
        ] {
            assert!(toml::from_str::<Config>(&format!("token = {value}")).is_err());
        }
    }

    /// Loads literal and file contents exactly while keeping results wrapped.
    #[test]
    fn loads_values_and_files() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("token");
        let value = " token\r\n";
        fs::write(&path, value).expect("token file should be written");

        for source in [External::Value(value.into()), External::File(path.clone())] {
            let token = Secret::new(source)
                .try_map_revealed(External::load)
                .expect("token should load");
            assert_eq!(format!("{token:?}"), "...");
            assert_eq!(token.reveal_str(), value);
        }

        fs::write(&path, "").expect("empty file should be written");
        assert_eq!(
            External::File(path).load().expect("empty file should load"),
            ""
        );
        assert_eq!(
            External::Value(String::new())
                .load()
                .expect("empty literal should load"),
            ""
        );
    }

    /// Reports file failures without retaining invalid file contents.
    #[test]
    fn reports_file_errors() {
        let directory = tempdir().expect("temporary directory should be created");
        let path = directory.path().join("token");
        let error = External::File(path.clone())
            .load()
            .expect_err("missing file should fail");
        assert!(
            matches!(error, LoadError::File { source, .. } if source.kind() == ErrorKind::NotFound)
        );

        fs::write(&path, b"private-token\xff").expect("invalid UTF-8 file should be written");
        let error = External::File(path)
            .load()
            .expect_err("invalid UTF-8 should fail");
        assert!(!format!("{error:?} {error}").contains("private-token"));
        assert!(
            matches!(error, LoadError::File { source, .. } if source.kind() == ErrorKind::InvalidData)
        );
    }

    /// Tests environment sources in a subprocess without mutating shared state.
    #[test]
    fn loads_environment() {
        if env::var_os("TWELVE_EXTERNAL_TEST_CHILD").is_none() {
            let mut command =
                Command::new(env::current_exe().expect("test executable should exist"));
            command
                .args(["--exact", "config::external::tests::loads_environment"])
                .env("TWELVE_EXTERNAL_TEST_CHILD", "1")
                .env("TWELVE_EXTERNAL_TEST_VALUE", " token\n")
                .env("TWELVE_EXTERNAL_TEST_EMPTY", "")
                .env_remove("TWELVE_EXTERNAL_TEST_MISSING");
            #[cfg(unix)]
            command.env(
                "TWELVE_EXTERNAL_TEST_INVALID",
                OsString::from_vec(b"private-token\xff".to_vec()),
            );
            let output = command.output().expect("test subprocess should run");
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }

        assert_eq!(
            External::Env("TWELVE_EXTERNAL_TEST_VALUE".into())
                .load()
                .expect("variable should load"),
            " token\n"
        );
        assert_eq!(
            External::Env("TWELVE_EXTERNAL_TEST_EMPTY".into())
                .load()
                .expect("empty variable should load"),
            ""
        );
        assert!(matches!(
            External::Env("TWELVE_EXTERNAL_TEST_MISSING".into()).load(),
            Err(LoadError::MissingEnv { .. })
        ));
        #[cfg(unix)]
        {
            let error = External::Env("TWELVE_EXTERNAL_TEST_INVALID".into())
                .load()
                .expect_err("non-Unicode variable should fail");
            assert!(matches!(error, LoadError::NonUnicodeEnv { .. }));
            assert!(!format!("{error:?} {error}").contains("private-token"));
        }
    }
}
