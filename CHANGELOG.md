# Changelog

## v1.0.0 - Initial Release

This is the initial release of `muscl`.

### Features ported from [`mysql-admutils`](https://git.pvv.ntnu.no/Projects/mysql-admutils)

- All commands
- Support for starting internal server with SUID/SGID
- Best-effort CLI interface backwards compatibility (see deviation notes for details)
- Best-effort stdout/stderr output backwards compatibility (see line above)
- Privilege editor

### New features and changes from `mysql-admutils`

- Changed programming language from `C` to `Rust`, for better or for worse
- Combined the functionality of both `mysql-dbadm` and `mysql-useradm` into a single executable.
- Switched to a server+client architecture. With this change comes:
  - Added security against SUID/SGID-related vulnerabilities.
  - Logging and debug information for system administrators.
  - A limitation on the maximum number of connections to the database.
  - A lot of sandboxing and hardening for the server-side, limiting the amount
    of damage that can be done if compromised, and further increasing security.
- Added `--json` flag for several commands
- Added `check-auth` command, for testing whether you are allowed to manage certain databases or users
- Added `lock-user`/`unlock-user` which let's you temporarily disable a database user.
- Added dynamic shell completions, aware of which databases and users exist.
- Changed the name length limit from `32` characters to `64` characters.
- Added `-p`/`--privs` flag for editing privileges using only commandline flags.
  The flag acts similarly to `chmod` with `+` and `-` variants for adding and removing privileges.
  See `muscl edit-privs --help` for more information.
- Changed handling of database user passwords:
  - Prompting for passwords will now hide what you write
  - Allow providing passwords through files and stdin
- Respect `$VISUAL` in addition to `$EDITOR` when launching the privilege editor.
- Use a constant template for the privilege editor instead of providing random privileges on first use.
- Display the diff before committing privilege changes.
- Generally more detailed error reporting:
  - On entering database or user names you do not own, suggest valid names
  - Instead of silently trimming database/user names when too long, report as error
  - When there are other name validation errors, report exactly what went wrong instead of a generic message
  - Add new errors related to failures inbetween the client and the server
- Package and distribute software:
  - Provide `.deb` packages
  - Provide systemd units
  - Provide nix-flake with packages, overlays and NixOS modules.

### Known deviations from `mysql-admutils`' behaviour

- `--help` output is formatted by clap in a different style.
- `mysql-dbadm edit-perm` uses the new privilege editor implementation. Replicating
  the old behaviour
  there shoulnd't have been any (or at least very few) scripts relying on the old
  command API or behavior.
- The new tools use the new implementation to find it's configuration file, and uses the
  new configuration format. See the example config and installation instructions for more
  information about how to configure the software.
- The order in which input is validated (e.g. whether you own a user, whether the
  contains illegal characters, whether the user does or does not exist) might be different
  from the original program, leading to the same command reporting different errors.
- Arguments are de-duplicated, meaning that if you run something like
  `mysql-dbadm create user_db1 user_db2 user_db1`, the program will only try to create
  the `user_db1` once. The old program would attempt to create it twice, failing the second time.
