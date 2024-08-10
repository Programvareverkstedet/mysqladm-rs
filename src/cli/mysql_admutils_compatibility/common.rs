#[inline]
pub fn trim_to_32_chars(name: &str) -> String {
    name.chars().take(32).collect()
}
