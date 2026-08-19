//! Builds internal links for applications mounted below a reverse-proxy path.
//!
//! This module is useful when generated links and redirects must remain below
//! an externally visible path prefix. The [`Mount`] extractor reads
//! `X-Script-Name` and prepends it without rewriting request routing. A trusted
//! reverse proxy should remove client-supplied values before setting the
//! header.
//!
//! ```
//! use axum::response::Redirect;
//! use twelve::mount::Mount;
//!
//! async fn account(mount: Mount) -> Redirect {
//!     mount.redirect_to("/account")
//! }
//! ```

use axum::{
    extract::FromRequestParts,
    http::{
        request::Parts,
        uri::{self},
        StatusCode, Uri,
    },
    response::Redirect,
};

/// Provides request-aware links for applications below a proxy path prefix.
#[derive(Debug)]
pub struct Mount {
    /// The absolute path on the domain that the app is running under.
    script_name: Option<String>,
}

impl Mount {
    /// Constructs a relative URL (no scheme or host).
    ///
    /// # Panics
    ///
    /// Will panic if generated Uris are invalid.
    // TODO: Log warning, generate different Uri, or ensure this can never fail?
    pub fn internal<S: AsRef<str>>(&self, path: S) -> String {
        let mut parts: uri::Parts = Default::default();

        if let Some(ref script_name) = self.script_name {
            parts.path_and_query = Some(
                (script_name.clone() + "/" + path.as_ref())
                    .parse()
                    .expect("should not fail to parse"),
            );
        } else {
            parts.path_and_query = Some(
                path.as_ref()
                    .parse()
                    .expect("tried to generate invalid Uri"),
            );
        }

        // TODO: Set scheme from reverse proxy headers if available.

        Uri::from_parts(parts)
            .expect("should not fail to construct relative uri")
            .to_string()
    }

    #[inline(always)]
    pub fn redirect_to(&self, path: &str) -> Redirect {
        Redirect::to(&self.internal(path))
    }
}

impl<S: Send + Sync> FromRequestParts<S> for Mount {
    type Rejection = StatusCode;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let script_name = if let Some(script_name_header) = parts.headers.get("X-Script-Name") {
            Some(
                script_name_header
                    .to_str()
                    .map_err(|_| StatusCode::BAD_GATEWAY)?
                    .to_owned(),
            )
        } else {
            None
        };

        Ok(Mount { script_name })
    }
}

#[cfg(test)]
mod tests {
    use super::Mount;

    #[test]
    fn internal_url_construction_without_reverse_proxy() {
        let mount = Mount { script_name: None };

        assert_eq!(mount.internal("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn internal_url_construction_with_reverse_proxy() {
        let mount = Mount {
            script_name: Some("/sub/dir".to_owned()),
        };

        assert_eq!(mount.internal("foo/bar"), "/sub/dir/foo/bar");
    }
}
