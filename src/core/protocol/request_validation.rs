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
pub enum OwnerValidationError {
    #[error("No matching owner prefix found")]
    NoMatch,

    #[error("Name cannot be empty")]
    StringEmpty,
}

impl OwnerValidationError {
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
            OwnerValidationError::NoMatch => format!(
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
            OwnerValidationError::StringEmpty => format!(
                "'{}' is not a valid {} name.",
                db_or_user.name(),
                db_or_user.lowercased_noun()
            )
            .to_string(),
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            OwnerValidationError::NoMatch => "no-match",
            OwnerValidationError::StringEmpty => "string-empty",
        }
    }
}

#[derive(Error, Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum AuthorizationError {
    #[error("Sanitization error: {0}")]
    SanitizationError(NameValidationError),

    #[error("Ownership error: {0}")]
    OwnershipError(OwnerValidationError),
    // AuthorizationHandlerError(String),
}

impl AuthorizationError {
    pub fn to_error_message(&self, db_or_user: DbOrUser) -> String {
        match self {
            AuthorizationError::SanitizationError(err) => err.to_error_message(db_or_user),
            AuthorizationError::OwnershipError(err) => err.to_error_message(db_or_user),
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
            AuthorizationError::SanitizationError(err) => {
                format!("sanitization-error/{}", err.error_type())
            }
            // TODO: maybe rename this to authorization error?
            AuthorizationError::OwnershipError(err) => {
                format!("ownership-error/{}", err.error_type())
            } // AuthorizationError::AuthorizationHandlerError(_) => {
              //     "authorization-handler-error".to_string()
              // }
        }
    }
}
