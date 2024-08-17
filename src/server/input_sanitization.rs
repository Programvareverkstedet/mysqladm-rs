use crate::core::{
    common::UnixUser,
    protocol::server_responses::{NameValidationError, OwnerValidationError},
};

const MAX_NAME_LENGTH: usize = 64;

pub fn validate_name(name: &str) -> Result<(), NameValidationError> {
    if name.is_empty() {
        Err(NameValidationError::EmptyString)
    } else if name.len() > MAX_NAME_LENGTH {
        Err(NameValidationError::TooLong)
    } else if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Err(NameValidationError::InvalidCharacters)
    } else {
        Ok(())
    }
}

pub fn validate_ownership_by_unix_user(
    name: &str,
    user: &UnixUser,
) -> Result<(), OwnerValidationError> {
    let prefixes = std::iter::once(user.username.clone())
        .chain(user.groups.iter().cloned())
        .collect::<Vec<String>>();

    validate_ownership_by_prefixes(name, &prefixes)
}

/// Core logic for validating the ownership of a database name.
/// This function checks if the given name matches any of the given prefixes.
/// These prefixes will in most cases be the user's unix username and any
/// unix groups the user is a member of.
pub fn validate_ownership_by_prefixes(
    name: &str,
    prefixes: &[String],
) -> Result<(), OwnerValidationError> {
    if name.is_empty() {
        return Err(OwnerValidationError::StringEmpty);
    }

    if prefixes
        .iter()
        .filter(|p| name.starts_with(*p))
        .collect::<Vec<_>>()
        .is_empty()
    {
        return Err(OwnerValidationError::NoMatch);
    };

    Ok(())
}

#[inline]
pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"\'"))
}

#[inline]
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

    #[test]
    fn test_validate_name() {
        assert_eq!(validate_name(""), Err(NameValidationError::EmptyString));
        assert_eq!(validate_name("abcdefghijklmnopqrstuvwxyz"), Ok(()));
        assert_eq!(validate_name("ABCDEFGHIJKLMNOPQRSTUVWXYZ"), Ok(()));
        assert_eq!(validate_name("0123456789_-"), Ok(()));

        for c in "\n\t\r !@#$%^&*()+=[]{}|;:,.<>?/".chars() {
            assert_eq!(
                validate_name(&c.to_string()),
                Err(NameValidationError::InvalidCharacters)
            );
        }

        assert_eq!(validate_name(&"a".repeat(MAX_NAME_LENGTH)), Ok(()));

        assert_eq!(
            validate_name(&"a".repeat(MAX_NAME_LENGTH + 1)),
            Err(NameValidationError::TooLong)
        );
    }

    #[test]
    fn test_validate_owner_by_prefixes() {
        let prefixes = vec!["user".to_string(), "group".to_string()];

        assert_eq!(
            validate_ownership_by_prefixes("", &prefixes),
            Err(OwnerValidationError::StringEmpty)
        );

        assert_eq!(
            validate_ownership_by_prefixes("user_testdb", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_testdb", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_test_db", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_test-db", &prefixes),
            Ok(())
        );

        assert_eq!(
            validate_ownership_by_prefixes("nonexistent_testdb", &prefixes),
            Err(OwnerValidationError::NoMatch)
        );
    }
}
