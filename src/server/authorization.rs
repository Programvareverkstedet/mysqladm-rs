use std::{collections::HashSet, path::Path};

use anyhow::Context;
use nix::unistd::Group;

use crate::core::{
    common::UnixUser,
    protocol::{
        CheckAuthorizationError,
        request_validation::{GroupDenylist, validate_db_or_user_request},
    },
    types::DbOrUser,
};

pub async fn check_authorization(
    dbs_or_users: Vec<DbOrUser>,
    unix_user: &UnixUser,
    group_denylist: &GroupDenylist,
) -> std::collections::BTreeMap<DbOrUser, Result<(), CheckAuthorizationError>> {
    let mut results = std::collections::BTreeMap::new();

    for db_or_user in dbs_or_users {
        if let Err(err) = validate_db_or_user_request(&db_or_user, unix_user, group_denylist)
            .map_err(CheckAuthorizationError)
        {
            results.insert(db_or_user.clone(), Err(err));
            continue;
        }
        results.insert(db_or_user.clone(), Ok(()));
    }

    results
}

/// Reads and parses a group denylist file, returning a set of GUIDs
///
/// The format of the denylist file is expected to be one group name or GID per line.
/// Lines starting with '#' are treated as comments and ignored.
/// Empty lines are also ignored.
///
/// Each line looks like one of the following:
/// - `gid:1001`
/// - `group:admins`
pub fn read_and_parse_group_denylist(denylist_path: &Path) -> anyhow::Result<GroupDenylist> {
    let content = std::fs::read_to_string(denylist_path).context(format!(
        "Failed to read denylist file at {:?}",
        denylist_path
    ))?;

    let mut groups = HashSet::with_capacity(content.lines().count());

    for (line_number, line) in content.lines().enumerate() {
        let trimmed_line = line.trim();

        if trimmed_line.is_empty() || trimmed_line.starts_with('#') {
            continue;
        }

        let parts: Vec<&str> = trimmed_line.splitn(2, ':').collect();
        if parts.len() != 2 {
            anyhow::bail!(
                "Invalid format in denylist file at {:?} on line {}: {}",
                denylist_path,
                line_number + 1,
                line
            );
        }

        match parts[0] {
            "gid" => {
                let gid: u32 = parts[1].parse().with_context(|| {
                    format!(
                        "Invalid GID in denylist file at {:?} on line {}: {}",
                        denylist_path,
                        line_number + 1,
                        parts[1]
                    )
                })?;
                let group = Group::from_gid(nix::unistd::Gid::from_raw(gid))
                    .context(format!(
                        "Failed to get group for GID {} in denylist file at {:?} on line {}",
                        gid,
                        denylist_path,
                        line_number + 1
                    ))?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No group found for GID {} in denylist file at {:?} on line {}",
                            gid,
                            denylist_path,
                            line_number + 1
                        )
                    })?;
                groups.insert(group.gid.as_raw());
            }
            "group" => {
                let group = Group::from_name(parts[1])
                    .context(format!(
                        "Failed to get group for name '{}' in denylist file at {:?} on line {}",
                        parts[1],
                        denylist_path,
                        line_number + 1
                    ))?
                    .ok_or_else(|| {
                        anyhow::anyhow!(
                            "No group found for name '{}' in denylist file at {:?} on line {}",
                            parts[1],
                            denylist_path,
                            line_number + 1
                        )
                    })?;
                groups.insert(group.gid.as_raw());
            }
            _ => {
                anyhow::bail!(
                    "Invalid prefix '{}' in denylist file at {:?} on line {}: {}",
                    parts[0],
                    denylist_path,
                    line_number + 1,
                    line
                );
            }
        }
    }

    Ok(groups)
}
