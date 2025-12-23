pub mod database_operations;
pub mod database_privilege_operations;
pub mod user_operations;

#[inline]
#[must_use]
pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"\'"))
}

#[inline]
#[must_use]
pub fn quote_identifier(s: &str) -> String {
    format!("`{}`", s.replace('`', r"\`"))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_quote_literal() {
        let payload = "' OR 1=1 --";
        assert_eq!(quote_literal(payload), r#"'\' OR 1=1 --'"#);
    }

    #[test]
    fn test_quote_identifier() {
        let payload = "` OR 1=1 --";
        assert_eq!(quote_identifier(payload), r#"`\` OR 1=1 --`"#);
    }
}
