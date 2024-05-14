# mysqladm-rs

Work in progress rewrite of https://git.pvv.ntnu.no/Projects/mysql-admutils

## Installation

The resulting binary will probably need to be marked as either SUID or SGID to work in a multi-user environment.
The UID/GID of the binary should have access to the config file, which contains secrets to log in to an admin-like MySQL user.
Preferrably, this UID/GID should not be root, in order to minimize the potential damage that can be done in case of security vulnerabilities in the program.

## Development and testing

Ensure you have a [rust toolchain](https://www.rust-lang.org/tools/install) installed.

In order to set up a test instance of mariadb in a docker container, run the following command:

```bash
docker run --rm --name mariadb -e MYSQL_ROOT_PASSWORD=secret -d mariadb:latest
```

You should then create a config file, and adjust the hostname to the IP address of the docker container.

```bash
cp ./example-config.toml ./config.toml
DOCKER_IP_ADDRESS="$(docker inspect -f '{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}' mariadb)"
sed -i "s/host = \"localhost\"/host = \"${DOCKER_IP_ADDRESS}\"/" ./config.toml
```

You should now be able to connect to the mariadb instance, after building the program and using arguments to specify the config file.

```bash
cargo run -- --config-file ./config.toml <args>

# example usage
cargo run -- --config-file ./config.toml db create "${USER}_testdb"
cargo run -- --config-file ./config.toml user create "${USER}_testuser"
cargo run -- --config-file ./config.toml db edit-perm -p "${USER}_testdb:${USER}_testuser:A"
cargo run -- --config-file ./config.toml db show-perm
```

To stop and remove the container, run the following command:

```bash
docker stop mariadb
```