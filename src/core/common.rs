use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use nix::unistd::{getuid, Group, User};

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
        .filter_map(|gid| {
            match Group::from_gid(*gid).map_err(|e| {
                log::trace!(
                    "Failed to look up group with GID {}: {}\nIgnoring...",
                    gid,
                    e
                );
                e
            }) {
                Ok(Some(group)) => Some(group),
                _ => None,
            }
        })
        .collect::<Vec<Group>>();

    Ok(groups)
}

pub fn validate_prefix_for_user<'a>(name: &'a str, user: &User) -> anyhow::Result<&'a str> {
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

pub fn quote_literal(s: &str) -> String {
    format!("'{}'", s.replace('\'', r"\'"))
}

pub fn quote_identifier(s: &str) -> String {
    format!("`{}`", s.replace('`', r"\`"))
}
