# Compatibility mode with [mysql-admutils](https://git.pvv.ntnu.no/Projects/mysql-admutils)

If you enable the feature flag `mysql-admutils-compatibility` (enabled by default for now), the output directory will contain two symlinks to the `musl` binary: `mysql-dbadm` and `mysql-useradm`. When invoked through these symlinks, the binary will react to its `argv[0]` and behave accordingly. These modes strive to behave as similar as possible to the original programs.

```bash
cargo build
./target/debug/mysql-dbadm --help
./target/debug/mysql-useradm --help
```

These symlinks are also included in the deb packages.

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
