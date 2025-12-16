# Development and testing

Ensure you have a [rust toolchain](https://www.rust-lang.org/tools/install) installed.

In order to set up a test instance of mariadb in a docker container, run the following command:

```bash
docker run --rm --name mariadb -e MYSQL_ROOT_PASSWORD=secret -p 3306:3306 -d mariadb:latest
```

This will start a mariadb instance with the root password `secret`, and expose the port 3306 on the host machine.

Run the following command to create a configuration file with the default settings:

```bash
cp ./assets/example-config.toml ./config.toml
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

## Development using Nix

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
