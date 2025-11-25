[![Coverage](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/badges/for_the_badge.svg)](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/)
[![Docs](https://img.shields.io/badge/rust_docs-blue?style=for-the-badge&logo=rust)](https://pages.pvv.ntnu.no/Projects/muscl/main/docs/muscl/)

# muscl

Healing mysql spasms since 2024

## What is this?

This is a CLI tool that let's normal users perform administrative operations on a MySQL DBMS, with some restrictions.
The default restriction is to only let the user perform these actions on databases and database users that are prefixed with their username,
or with the name of any unix group that the user is a part of. i.e. `<user>_mydb`, `<user>_mydbuser`, or `<group>_myotherdb`.

The administrative actions available to the user includes:

- creating/listing/modifying/deleting databases and database users
- modifying database user privileges
- changing the passwords of the database users
- locking and unlocking database user accounts
- ... more to come

The software is split into a client and a server. The server has administrative access to the mysql server,
and is responsible for checking client authorization for the different types of actions the client might request.

This is designed for (and is only really useful for) multi-user servers, like tilde servers, university unix servers, etc.

## Installation

The resulting binary will probably need to be marked as either SUID or SGID to work in a multi-user environment.
The UID/GID of the binary should have access to the config file, which contains secrets to log in to an admin-like MySQL user.
Preferrably, this UID/GID should not be root, in order to minimize the potential damage that can be done in case of security vulnerabilities in the program.

## Development and testing

### Nix

If you have nix installed, you can test your changes in a NixOS vm by running:

```bash
nix run .#vm
```

### General setup

Ensure you have a [rust toolchain](https://www.rust-lang.org/tools/install) installed.

In order to set up a test instance of mariadb in a docker container, run the following command:

```bash
docker run --rm --name mariadb -e MYSQL_ROOT_PASSWORD=secret -p 3306:3306 -d mariadb:latest
```

This will start a mariadb instance with the root password `secret`, and expose the port 3306 on the host machine.


Run the following command to create a configuration file with the default settings:

```bash
cp ./example-config.toml ./config.toml
```

If you used the docker command above, you can use these settings as is, but if you are running mariadb/mysql on another host, port or with another password, adjust the corresponding fields in `config.toml`.
This file will contain your database password, but is ignored by git, so it will not be committed to the repository.

You should now be able to connect to the mariadb instance, after building the program and using arguments to specify the config file.

```bash
cargo run -- --config-file ./config.toml <args>

# example usage
cargo run -- --config-file ./config.toml create-db "${USER}_testdb"
cargo run -- --config-file ./config.toml create-user "${USER}_testuser"
cargo run -- --config-file ./config.toml edit-db-privs -p "${USER}_testdb:${USER}_testuser:A"
cargo run -- --config-file ./config.toml show-db-privs
```

To stop and remove the container, run the following command:

```bash
docker stop mariadb
```

## Compatibility mode with [mysql-admutils](https://git.pvv.ntnu.no/Projects/mysql-admutils)

If you enable the feature flag `mysql-admutils-compatibility` (enabled by default), the output directory will contain two symlinks to the binary, `mysql-dbadm` and `mysql-useradm`. In the same fashion as busybox, the binary will react to its `argv[0]` and behave as if it was called with the corresponding name. While the internal functionality is written in rust, these modes strive to behave as similar as possible to the original programs.

```bash
cargo build
./target/debug/mysql-dbadm --help
./target/debug/mysql-useradm --help
```

### Known deviations from the original programs

- Added flags for database configuration, not present in the original programs
- `--help` output is formatted by clap in a modern style.
- `mysql-dbadm edit-perm` uses the new implementation. The idea was that the parsing
  logic was too complex to be worth porting, and there wouldn't be any scripts depending
  on this command anyway. As such, the new implementation is more user-friendly and only
  brings positive changes.
- The new tools use the modern implementation to find it's configuration. If you compiled
  the old programs with `--sysconfdir=<somewhere>`, you might have to provide `--config-file`
  where the old program would just work by itself.
- The order in which some things are validated (e.g. whether you own a user, whether the
  contains illegal characters, whether the user does or does not exist) might be different
  from the original program, leading to the same command giving the errors in a different order.
