use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};

use crate::core::common::UnixUser;

/// This enum is used to differentiate between database and user operations.
/// Their output are very similar, but there are slight differences in the words used.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DbOrUser {
    Database,
    User,
}

impl DbOrUser {
    pub fn lowercased(&self) -> &'static str {
        match self {
            DbOrUser::Database => "database",
            DbOrUser::User => "user",
        }
    }

    pub fn capitalized(&self) -> &'static str {
        match self {
            DbOrUser::Database => "Database",
            DbOrUser::User => "User",
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum NameValidationError {
    EmptyString,
    InvalidCharacters,
    TooLong,
}

impl NameValidationError {
    pub fn to_error_message(self, name: &str, db_or_user: DbOrUser) -> String {
        match self {
            NameValidationError::EmptyString => {
                format!("{} name cannot be empty.", db_or_user.capitalized()).to_owned()
            }
            NameValidationError::TooLong => format!(
                "{} is too long. Maximum length is 64 characters.",
                db_or_user.capitalized()
            )
            .to_owned(),
            NameValidationError::InvalidCharacters => format!(
                indoc! {r#"
                  Invalid characters in {} name: '{}'

                  Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted.
                "#},
                db_or_user.lowercased(),
                name
            )
            .to_owned(),
        }
    }
}

impl OwnerValidationError {
    pub fn to_error_message(self, name: &str, db_or_user: DbOrUser) -> String {
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
                db_or_user.lowercased(),
                name,
                db_or_user.lowercased(),
                db_or_user.lowercased(),
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
                name,
                db_or_user.lowercased()
            )
            .to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq, Clone, Copy, Serialize, Deserialize)]
pub enum OwnerValidationError {
    // The name is valid, but none of the given prefixes matched the name
    NoMatch,

    // The name is empty, which is invalid
    StringEmpty,
}
