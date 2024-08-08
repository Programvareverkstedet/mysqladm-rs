use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use nix::unistd::{getuid, Group, User};
use sqlx::{Connection, MySqlConnection};

#[cfg(not(target_os = "macos"))]
use std::ffi::CString;

/// Report the result status of a command.
/// This is used to display a status message to the user.
pub enum CommandStatus {
    /// The command was successful,
    /// and made modification to the database.
    SuccessfullyModified,

    /// The command was mostly successful,
    /// and modifications have been made to the database.
    /// However, some of the requested modifications failed.
    PartiallySuccessfullyModified,

    /// The command was successful,
    /// but no modifications were needed.
    NoModificationsNeeded,

    /// The command was successful,
    /// and made no modification to the database.
    NoModificationsIntended,

    /// The command was cancelled, either through a dialog or a signal.
    /// No modifications have been made to the database.
    Cancelled,
}

pub fn get_current_unix_user() -> anyhow::Result<User> {
    User::from_uid(getuid())
        .context("Failed to look up your UNIX username")
        .and_then(|u| u.ok_or(anyhow::anyhow!("Failed to look up your UNIX username")))
}

#[cfg(target_os = "macos")]
pub fn get_unix_groups(_user: &User) -> anyhow::Result<Vec<Group>> {
    // Return an empty list on macOS since there is no `getgrouplist` function
    Ok(vec![])
}

#[cfg(not(target_os = "macos"))]
pub fn get_unix_groups(user: &User) -> anyhow::Result<Vec<Group>> {
    let user_cstr =
        CString::new(user.name.as_bytes()).context("Failed to convert username to CStr")?;
    let groups = nix::unistd::getgrouplist(&user_cstr, user.gid)?
        .iter()
        .filter_map(|gid| match Group::from_gid(*gid) {
            Ok(Some(group)) => Some(group),
            Ok(None) => None,
            Err(e) => {
                log::warn!(
                    "Failed to look up group with GID {}: {}\nIgnoring...",
                    gid,
                    e
                );
                None
            }
        })
        .collect::<Vec<Group>>();

    Ok(groups)
}

/// This function creates a regex that matches items (users, databases)
/// that belong to the user or any of the user's groups.
pub fn create_user_group_matching_regex(user: &User) -> String {
    let groups = get_unix_groups(user).unwrap_or_default();

    if groups.is_empty() {
        format!("{}(_.+)?", user.name)
    } else {
        format!(
            "({}|{})(_.+)?",
            user.name,
            groups
                .iter()
                .map(|g| g.name.as_str())
                .collect::<Vec<_>>()
                .join("|")
        )
    }
}

/// This enum is used to differentiate between database and user operations.
/// Their output are very similar, but there are slight differences in the words used.
#[derive(Debug, PartialEq, Eq, Clone, Copy)]
pub enum DbOrUser {
    Database,
    User,
}

impl DbOrUser {
    pub fn lowercased(&self) -> String {
        match self {
            DbOrUser::Database => "database".to_string(),
            DbOrUser::User => "user".to_string(),
        }
    }

    pub fn capitalized(&self) -> String {
        match self {
            DbOrUser::Database => "Database".to_string(),
            DbOrUser::User => "User".to_string(),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum NameValidationResult {
    Valid,
    EmptyString,
    InvalidCharacters,
    TooLong,
}

pub fn validate_name(name: &str) -> NameValidationResult {
    if name.is_empty() {
        NameValidationResult::EmptyString
    } else if name.len() > 64 {
        NameValidationResult::TooLong
    } else if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        NameValidationResult::InvalidCharacters
    } else {
        NameValidationResult::Valid
    }
}

pub fn validate_name_or_error(name: &str, db_or_user: DbOrUser) -> anyhow::Result<()> {
    match validate_name(name) {
        NameValidationResult::Valid => Ok(()),
        NameValidationResult::EmptyString => {
            anyhow::bail!("{} name cannot be empty.", db_or_user.capitalized())
        }
        NameValidationResult::TooLong => anyhow::bail!(
            "{} is too long. Maximum length is 64 characters.",
            db_or_user.capitalized()
        ),
        NameValidationResult::InvalidCharacters => anyhow::bail!(
            indoc! {r#"
              Invalid characters in {} name: '{}'

              Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted.
            "#},
            db_or_user.lowercased(),
            name
        ),
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum OwnerValidationResult {
    // The name is valid and matches one of the given prefixes
    Match,

    // The name is valid, but none of the given prefixes matched the name
    NoMatch,

    // The name is empty, which is invalid
    StringEmpty,

    // The name is in the format "_<postfix>", which is invalid
    MissingPrefix,

    // The name is in the format "<prefix>_", which is invalid
    MissingPostfix,
}

/// Core logic for validating the ownership of a database name.
/// This function checks if the given name matches any of the given prefixes.
/// These prefixes will in most cases be the user's unix username and any
/// unix groups the user is a member of.
pub fn validate_ownership_by_prefixes(name: &str, prefixes: &[String]) -> OwnerValidationResult {
    if name.is_empty() {
        return OwnerValidationResult::StringEmpty;
    }

    if name.starts_with('_') {
        return OwnerValidationResult::MissingPrefix;
    }

    let (prefix, _) = match name.split_once('_') {
        Some(pair) => pair,
        None => return OwnerValidationResult::MissingPostfix,
    };

    if prefixes.iter().any(|g| g == prefix) {
        OwnerValidationResult::Match
    } else {
        OwnerValidationResult::NoMatch
    }
}

/// Validate the ownership of a database name or database user name.
/// This function takes the name of a database or user and a unix user,
/// for which it fetches the user's groups. It then checks if the name
/// is prefixed with the user's username or any of the user's groups.
pub fn validate_ownership_or_error<'a>(
    name: &'a str,
    user: &User,
    db_or_user: DbOrUser,
) -> anyhow::Result<&'a str> {
    let user_groups = get_unix_groups(user)?;
    let prefixes = std::iter::once(user.name.clone())
        .chain(user_groups.iter().map(|g| g.name.clone()))
        .collect::<Vec<String>>();

    match validate_ownership_by_prefixes(name, &prefixes) {
        OwnerValidationResult::Match => Ok(name),
        OwnerValidationResult::NoMatch => {
            anyhow::bail!(
                indoc! {r#"
                  Invalid {} name prefix: '{}' does not match your username or any of your groups.
                  Are you sure you are allowed to create {} names with this prefix?

                  Allowed prefixes:
                    - {}
                  {}
                "#},
                db_or_user.lowercased(),
                name,
                db_or_user.lowercased(),
                user.name,
                user_groups
                    .iter()
                    .filter(|g| g.name != user.name)
                    .map(|g| format!("  - {}", g.name))
                    .sorted()
                    .join("\n"),
            );
        }
        _ => anyhow::bail!(
            "'{}' is not a valid {} name.",
            name,
            db_or_user.lowercased()
        ),
    }
}

/// Gracefully close a MySQL connection.
pub async fn close_database_connection(connection: MySqlConnection) {
    if let Err(e) = connection
        .close()
        .await
        .context("Failed to close connection properly")
    {
        eprintln!("{}", e);
        eprintln!("Ignoring...");
    }
}

#[inline]
pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"\'"))
}

#[inline]
pub fn quote_identifier(s: &str) -> String {
    format!("`{}`", s.replace('`', r"\`"))
}

#[inline]
pub(crate) fn yn(b: bool) -> &'static str {
    if b {
        "Y"
    } else {
        "N"
    }
}

#[inline]
pub(crate) fn rev_yn(s: &str) -> Option<bool> {
    match s.to_lowercase().as_str() {
        "y" => Some(true),
        "n" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn test_yn() {
        assert_eq!(yn(true), "Y");
        assert_eq!(yn(false), "N");
    }

    #[test]
    fn test_rev_yn() {
        assert_eq!(rev_yn("Y"), Some(true));
        assert_eq!(rev_yn("y"), Some(true));
        assert_eq!(rev_yn("N"), Some(false));
        assert_eq!(rev_yn("n"), Some(false));
        assert_eq!(rev_yn("X"), None);
    }

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
        assert_eq!(validate_name(""), NameValidationResult::EmptyString);
        assert_eq!(
            validate_name("abcdefghijklmnopqrstuvwxyz"),
            NameValidationResult::Valid
        );
        assert_eq!(
            validate_name("ABCDEFGHIJKLMNOPQRSTUVWXYZ"),
            NameValidationResult::Valid
        );
        assert_eq!(validate_name("0123456789_-"), NameValidationResult::Valid);

        for c in "\n\t\r !@#$%^&*()+=[]{}|;:,.<>?/".chars() {
            assert_eq!(
                validate_name(&c.to_string()),
                NameValidationResult::InvalidCharacters
            );
        }

        assert_eq!(validate_name(&"a".repeat(64)), NameValidationResult::Valid);

        assert_eq!(
            validate_name(&"a".repeat(65)),
            NameValidationResult::TooLong
        );
    }

    #[test]
    fn test_validate_owner_by_prefixes() {
        let prefixes = vec!["user".to_string(), "group".to_string()];

        assert_eq!(
            validate_ownership_by_prefixes("", &prefixes),
            OwnerValidationResult::StringEmpty
        );

        assert_eq!(
            validate_ownership_by_prefixes("user", &prefixes),
            OwnerValidationResult::MissingPostfix
        );
        assert_eq!(
            validate_ownership_by_prefixes("something", &prefixes),
            OwnerValidationResult::MissingPostfix
        );
        assert_eq!(
            validate_ownership_by_prefixes("user-testdb", &prefixes),
            OwnerValidationResult::MissingPostfix
        );

        assert_eq!(
            validate_ownership_by_prefixes("_testdb", &prefixes),
            OwnerValidationResult::MissingPrefix
        );

        assert_eq!(
            validate_ownership_by_prefixes("user_testdb", &prefixes),
            OwnerValidationResult::Match
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_testdb", &prefixes),
            OwnerValidationResult::Match
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_test_db", &prefixes),
            OwnerValidationResult::Match
        );
        assert_eq!(
            validate_ownership_by_prefixes("group_test-db", &prefixes),
            OwnerValidationResult::Match
        );

        assert_eq!(
            validate_ownership_by_prefixes("nonexistent_testdb", &prefixes),
            OwnerValidationResult::NoMatch
        );
    }
}
