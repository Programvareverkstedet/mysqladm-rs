use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use nix::unistd::{getuid, Group, User};
use sqlx::{Connection, MySqlConnection};

#[cfg(not(target_os = "macos"))]
use std::ffi::CString;

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

pub fn validate_name_token(name: &str) -> anyhow::Result<()> {
    if name.is_empty() {
        anyhow::bail!("Database name cannot be empty.");
    }

    if name.len() > 64 {
        anyhow::bail!("Database name is too long. Maximum length is 64 characters.");
    }

    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        anyhow::bail!(
            indoc! {r#"
              Invalid characters in name: '{}'

              Only A-Z, a-z, 0-9, _ (underscore) and - (dash) are permitted.
            "#},
            name
        );
    }

    Ok(())
}

pub fn validate_ownership_by_user_prefix<'a>(
    name: &'a str,
    user: &User,
) -> anyhow::Result<&'a str> {
    let user_groups = get_unix_groups(user)?;

    let mut split_name = name.split('_');

    let prefix = split_name
        .next()
        .ok_or(anyhow::anyhow!(indoc! {r#"
              Failed to find prefix.
            "#},))
        .and_then(|prefix| {
            if user.name == prefix || user_groups.iter().any(|g| g.name == prefix) {
                Ok(prefix)
            } else {
                anyhow::bail!(
                    indoc! {r#"
                      Invalid prefix: '{}' does not match your username or any of your groups.
                      Are you sure you are allowed to create databases or users with this prefix?

                      Allowed prefixes:
                        - {}
                      {}
                    "#},
                    prefix,
                    user.name,
                    user_groups
                        .iter()
                        .filter(|g| g.name != user.name)
                        .map(|g| format!("  - {}", g.name))
                        .sorted()
                        .join("\n"),
                );
            }
        })?;

    if !split_name.next().is_some_and(|s| !s.is_empty()) {
        anyhow::bail!(
            indoc! {r#"
              Missing the rest of the name after the user/group prefix.

              The name should be in the format: '{}_<name>'
            "#},
            prefix
        );
    }

    Ok(prefix)
}

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
