# Compatibility mode with [mysql-admutils](https://git.pvv.ntnu.no/Projects/mysql-admutils)

If you enable the feature flag `mysql-admutils-compatibility` (enabled by default for now), the output directory will contain two symlinks to the `musl` binary: `mysql-dbadm` and `mysql-useradm`. When invoked through these symlinks, the binary will react to its `argv[0]` and behave accordingly. These modes strive to behave as similar as possible to the original programs.

```bash
cargo build
./target/debug/mysql-dbadm --help
./target/debug/mysql-useradm --help
```

These symlinks are also included in the deb packages.

### Known deviations from the original programs

- Added flags for database configuration, not present in the original programs
- `--help` output is formatted by clap in a modern style.
- `mysql-dbadm edit-perm` uses the new implementation. Parsing the old logic was
  too complex to be worth porting, and since the action is inherently interactive,
  there shoulnd't have been any (or at least very few) scripts relying on the old
  command API or behavior.
- The new tools use the modern implementation to find it's configuration. If you compiled
  the old programs with `--sysconfdir=<somewhere>`, you might have to provide `--config-file`
  where the old program would just work by itself.
- The order in which some things are validated (e.g. whether you own a user, whether the
  contains illegal characters, whether the user does or does not exist) might be different
  from the original program, leading to the same command giving the errors in a different order.
