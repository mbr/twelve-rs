pub trait ToOptStr {
    fn to_opt_str(&self) -> Option<&str>;
}

impl ToOptStr for Option<String> {
    #[inline]
    fn to_opt_str(&self) -> Option<&str> {
        Some(self.as_ref()?.as_str())
    }
}
