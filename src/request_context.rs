use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{
        request::Parts,
        uri::{self},
        StatusCode, Uri,
    },
    response::Redirect,
};

#[derive(Debug)]
pub struct RequestContext {
    /// The absolute path on the domain that the app is running under.
    script_name: Option<String>,
}

impl RequestContext {
    /// Constructs an relative url (no scheme or host).
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
                    .expect("should not faild to parse?"),
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

#[async_trait]
impl<S> FromRequestParts<S> for RequestContext {
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

        Ok(RequestContext { script_name })
    }
}

#[cfg(test)]
mod tests {
    use super::RequestContext;

    #[test]
    fn internal_url_construction_without_reverse_proxy() {
        let ctx = RequestContext { script_name: None };

        assert_eq!(ctx.internal("/foo/bar"), "/foo/bar");
    }

    #[test]
    fn internal_url_construction_with_reverse_proxy() {
        let ctx = RequestContext {
            script_name: Some("/sub/dir".to_owned()),
        };

        assert_eq!(ctx.internal("foo/bar"), "/sub/dir/foo/bar");
    }
}
