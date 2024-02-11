//! String-to-reference conversion inside `Option`.

/// Acquire an `Option<&str>` from given value.
pub trait AsOptStr {
    /// Given value, converts it into an option of a string reference.
    fn as_opt_str(&self) -> Option<&str>;
}

impl<T> AsOptStr for Option<T>
where
    T: AsRef<str>,
{
    #[inline(always)]
    fn as_opt_str(&self) -> Option<&str> {
        self.as_ref().map(AsRef::as_ref)
    }
}
