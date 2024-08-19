#[cfg(feature = "mysql-admutils-compatibility")]
use anyhow::anyhow;
#[cfg(feature = "mysql-admutils-compatibility")]
use std::{env, os::unix::fs::symlink, path::PathBuf};

fn generate_mysql_admutils_symlinks() -> anyhow::Result<()> {
    // NOTE: This is slightly illegal, and depends on implementation details.
    //       But it is only here for ease of testing the compatibility layer,
    //       and not critical in any way. Considering the code is never going
    //       to be used as a library, it should be fine.
    let target_profile_dir: PathBuf = PathBuf::from(env::var("OUT_DIR")?)
        .parent()
        .and_then(|p| p.parent())
        .and_then(|p| p.parent())
        .ok_or(anyhow!("Could not resolve target profile directory"))?
        .to_path_buf();

    if !target_profile_dir.exists() {
        std::fs::create_dir_all(&target_profile_dir)?;
    }

    if !target_profile_dir.join("mysql-useradm").exists() {
        symlink(
            target_profile_dir.join("mysqladm"),
            target_profile_dir.join("mysql-useradm"),
        )
        .ok();
    }

    if !target_profile_dir.join("mysql-dbadm").exists() {
        symlink(
            target_profile_dir.join("mysqladm"),
            target_profile_dir.join("mysql-dbadm"),
        )
        .ok();
    }

    Ok(())
}

fn main() -> anyhow::Result<()> {
    #[cfg(feature = "mysql-admutils-compatibility")]
    generate_mysql_admutils_symlinks()?;

    Ok(())
}
