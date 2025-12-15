use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::{common::UnixUser, types::DbOrUser};

#[derive(Error, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum NameValidationError {
    #[error("Name cannot be empty.")]
    EmptyString,

    #[error(
        "Name contains invalid characters. Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted."
    )]
    InvalidCharacters,

    #[error("Name is too long. Maximum length is 64 characters.")]
    TooLong,
}

impl NameValidationError {
    pub fn to_error_message(self, db_or_user: DbOrUser) -> String {
        match self {
            NameValidationError::EmptyString => {
                format!("{} name cannot be empty.", db_or_user.capitalized_noun()).to_owned()
            }
            NameValidationError::TooLong => format!(
                "{} is too long. Maximum length is 64 characters.",
                db_or_user.capitalized_noun()
            )
            .to_owned(),
            NameValidationError::InvalidCharacters => format!(
                indoc! {r#"
                  Invalid characters in {} name: '{}'

                  Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted.
                "#},
                db_or_user.lowercased_noun(),
                db_or_user.name(),
            )
            .to_owned(),
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            NameValidationError::EmptyString => "empty-string",
            NameValidationError::InvalidCharacters => "invalid-characters",
            NameValidationError::TooLong => "too-long",
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum AuthorizationError {
    #[error("No matching owner prefix found")]
    NoMatch,

    // TODO: I don't think this should ever happen?
    #[error("Name cannot be empty")]
    StringEmpty,
}

impl AuthorizationError {
    pub fn to_error_message(self, db_or_user: DbOrUser) -> String {
        let user = UnixUser::from_enviroment();

        let UnixUser {
            username,
            mut groups,
        } = user.unwrap_or(UnixUser {
            username: "???".to_string(),
            groups: vec![],
        });

        groups.sort();

        match self {
            AuthorizationError::NoMatch => format!(
                indoc! {r#"
                  Invalid {} name prefix: '{}' does not match your username or any of your groups.
                  Are you sure you are allowed to create {} names with this prefix?
                  The format should be: <prefix>_<{} name>

                  Allowed prefixes:
                    - {}
                  {}
                "#},
                db_or_user.lowercased_noun(),
                db_or_user.name(),
                db_or_user.lowercased_noun(),
                db_or_user.lowercased_noun(),
                username,
                groups
                    .into_iter()
                    .filter(|g| g != &username)
                    .map(|g| format!("  - {}", g))
                    .join("\n"),
            )
            .to_owned(),
            AuthorizationError::StringEmpty => format!(
                "'{}' is not a valid {} name.",
                db_or_user.name(),
                db_or_user.lowercased_noun()
            )
            .to_string(),
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            AuthorizationError::NoMatch => "no-match",
            AuthorizationError::StringEmpty => "string-empty",
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone, Serialize, Deserialize)]
pub enum ValidationError {
    #[error("Name validation error: {0}")]
    NameValidationError(NameValidationError),

    #[error("Authorization error: {0}")]
    AuthorizationError(AuthorizationError),
    // AuthorizationHandlerError(String),
}

impl ValidationError {
    pub fn to_error_message(&self, db_or_user: DbOrUser) -> String {
        match self {
            ValidationError::NameValidationError(err) => err.to_error_message(db_or_user),
            ValidationError::AuthorizationError(err) => err.to_error_message(db_or_user),
            // AuthorizationError::AuthorizationHandlerError(msg) => {
            //     format!(
            //         "Authorization handler error for '{}': {}",
            //         db_or_user.name(),
            //         msg
            //     )
            // }
        }
    }

    pub fn error_type(&self) -> String {
        match self {
            ValidationError::NameValidationError(err) => {
                format!("name-validation-error/{}", err.error_type())
            }
            ValidationError::AuthorizationError(err) => {
                format!("authorization-error/{}", err.error_type())
            } // AuthorizationError::AuthorizationHandlerError(_) => {
              //     "authorization-handler-error".to_string()
              // }
        }
    }
}

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

pub fn validate_authorization_by_unix_user(
    name: &str,
    user: &UnixUser,
) -> Result<(), AuthorizationError> {
    let prefixes = std::iter::once(user.username.to_owned())
        .chain(user.groups.iter().cloned())
        .collect::<Vec<String>>();

    validate_authorization_by_prefixes(name, &prefixes)
}

/// Core logic for validating the ownership of a database name.
/// This function checks if the given name matches any of the given prefixes.
/// These prefixes will in most cases be the user's unix username and any
/// unix groups the user is a member of.
pub fn validate_authorization_by_prefixes(
    name: &str,
    prefixes: &[String],
) -> Result<(), AuthorizationError> {
    if name.is_empty() {
        return Err(AuthorizationError::StringEmpty);
    }

    if prefixes
        .iter()
        .filter(|p| name.starts_with(&(p.to_string() + "_")))
        .collect::<Vec<_>>()
        .is_empty()
    {
        return Err(AuthorizationError::NoMatch);
    };

    Ok(())
}

pub fn validate_db_or_user_request(
    db_or_user: &DbOrUser,
    unix_user: &UnixUser,
) -> Result<(), ValidationError> {
    validate_name(db_or_user.name()).map_err(ValidationError::NameValidationError)?;

    validate_authorization_by_unix_user(db_or_user.name(), unix_user)
        .map_err(ValidationError::AuthorizationError)?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

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
    fn test_validate_authorization_by_prefixes() {
        let prefixes = vec!["user".to_string(), "group".to_string()];

        assert_eq!(
            validate_authorization_by_prefixes("", &prefixes),
            Err(AuthorizationError::StringEmpty)
        );

        assert_eq!(
            validate_authorization_by_prefixes("user_testdb", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_authorization_by_prefixes("group_testdb", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_authorization_by_prefixes("group_test_db", &prefixes),
            Ok(())
        );
        assert_eq!(
            validate_authorization_by_prefixes("group_test-db", &prefixes),
            Ok(())
        );

        assert_eq!(
            validate_authorization_by_prefixes("nonexistent_testdb", &prefixes),
            Err(AuthorizationError::NoMatch)
        );
    }
}
