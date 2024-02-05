//! String-to-reference conversion inside `Option`.

/// Convert `Option<String>` to `Option<&str>`.
pub trait ToOptStr {
    /// Given an owned option of a string, converts it into an option of a reference.
    fn to_opt_str(&self) -> Option<&str>;
}

impl ToOptStr for Option<String> {
    #[inline(always)]
    fn to_opt_str(&self) -> Option<&str> {
        Some(self.as_ref()?.as_str())
    }
}
