//! String-to-reference conversion inside `Option`.

/// Convert `Option<String>` to `Option<&str>`.
pub trait ToOptStr {
    /// Given an owned option of a string, converts it into an option of a reference.
    fn to_opt_str(&self) -> Option<&str>;
}

impl<T> ToOptStr for Option<T>
where
    T: AsRef<str>,
{
    #[inline(always)]
    fn to_opt_str(&self) -> Option<&str> {
        self.as_ref().map(AsRef::as_ref)
    }
}
