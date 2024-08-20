use anyhow::Context;
use nix::unistd::{Group as LibcGroup, User as LibcUser};

#[cfg(not(target_os = "macos"))]
use std::ffi::CString;

pub const DEFAULT_CONFIG_PATH: &str = "/etc/mysqladm/config.toml";
pub const DEFAULT_SOCKET_PATH: &str = "/run/mysqladm/mysqladm.sock";

pub struct UnixUser {
    pub username: String,
    pub groups: Vec<String>,
}

// TODO: these functions are somewhat critical, and should have integration tests

#[cfg(target_os = "macos")]
fn get_unix_groups(_user: &LibcUser) -> anyhow::Result<Vec<LibcGroup>> {
    // Return an empty list on macOS since there is no `getgrouplist` function
    Ok(vec![])
}

#[cfg(not(target_os = "macos"))]
fn get_unix_groups(user: &LibcUser) -> anyhow::Result<Vec<LibcGroup>> {
    let user_cstr =
        CString::new(user.name.as_bytes()).context("Failed to convert username to CStr")?;
    let groups = nix::unistd::getgrouplist(&user_cstr, user.gid)?
        .iter()
        .filter_map(|gid| match LibcGroup::from_gid(*gid) {
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
        .collect::<Vec<LibcGroup>>();

    Ok(groups)
}

impl UnixUser {
    pub fn from_uid(uid: u32) -> anyhow::Result<Self> {
        let libc_uid = nix::unistd::Uid::from_raw(uid);
        let libc_user = LibcUser::from_uid(libc_uid)
            .context("Failed to look up your UNIX username")?
            .ok_or(anyhow::anyhow!("Failed to look up your UNIX username"))?;

        let groups = get_unix_groups(&libc_user)?;

        Ok(UnixUser {
            username: libc_user.name,
            groups: groups.iter().map(|g| g.name.to_owned()).collect(),
        })
    }

    pub fn from_enviroment() -> anyhow::Result<Self> {
        let libc_uid = nix::unistd::getuid();
        UnixUser::from_uid(libc_uid.as_raw())
    }
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
}
