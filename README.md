[![Coverage](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/badges/for_the_badge.svg)](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/)
[![Docs](https://img.shields.io/badge/rust_docs-blue?style=for-the-badge&logo=rust)](https://pages.pvv.ntnu.no/Projects/muscl/main/docs/muscl/)

# muscl 💪

Dropping DBs (dumbbells) and having mysql spasms since 2024

## What is this?

This is a CLI tool that let's unprivileged users perform administrative operations on a MySQL DBMS, given the are authorized to perform the action on the database or database user in question.
The default authorization mechanism is to only let the user perform these actions on databases and database users that are prefixed with their username,
or with the name of any unix group that the user is a part of. i.e. `<user>_mydb`, `<user>_mydbuser`, or `<group>_myotherdb`.

The available administrative actions include:

- creating/listing/modifying/deleting databases and database users
- modifying privileges for a database user on a database
- changing the passwords of the database users
- locking and unlocking database users
- ... more to come

The software is designed to be run as a client and a server. The server has administrative access to the mysql server,
and is responsible for authorizing any requests from the clients.

This software is designed for multi-user servers, like tilde servers, university servers, etc.

## Installation and configuration

### Debian/Ubuntu

**TODO:** write this section once the package has been pushed to the gitea package repository.

### NixOS

For NixOS, there is a module available via the nix flake. You can include it in your configuration like this:

```nix
{
  inputs.nixpkgs.url = "github:NixOS/nixpkgs/nixos-XX.YY";

  inputs.muscl.url = "git+https://git.pvv.ntnu.no/Projects/muscle.git";
  inputs.muscl.inputs.nixpkgs.follows = "nixpkgs";

  ...
}
```

The module allows for easy setup on a local machine by enabling `services.muscl.createLocalDatabaseUser`.

### SUID/SGID mode

For backwards compatibility reasons, it is possible to run the program without a daemon by utilizing SUID/SGID.
In order to do this, you should set either the SUID/SGID bit and preferably make the executable owned by a non-privileged user.
If the database is running on the same machine, the user/group will need access to write and read from the database socket.
Otherwise, the only requirement is that the user/group is able to read the config file (typically `/etc/muscl/config.toml`).

## Development and testing

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
cargo run -- --config-file ./config.toml edit-privs -p "${USER}_testdb:${USER}_testuser:A"
cargo run -- --config-file ./config.toml show-privs
```

To stop and remove the container, run the following command:

```bash
docker stop mariadb
```

### Nix

If you have nix installed, you can easily test your changes in a NixOS vm by running:

```bash
nix run .#vm
```

You can configure the vm in `flake.nix`

## Filter logs by user with journalctl

If you want to filter the server logs by user, you can use journalctl's built-in filtering capabilities.

```bash
journalctl -eu muscl F_USER=<username>
```

## Compatibility mode with [mysql-admutils](https://git.pvv.ntnu.no/Projects/mysql-admutils)

If you enable the feature flag `mysql-admutils-compatibility` (enabled by default for now), the output directory will contain two symlinks to the `musl` binary: `mysql-dbadm` and `mysql-useradm`. When invoked through these symlinks, the binary will react to its `argv[0]` and behave accordingly. These modes strive to behave as similar as possible to the original programs.

```bash
cargo build
./target/debug/mysql-dbadm --help
./target/debug/mysql-useradm --help
```

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
