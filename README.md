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

## Documentation

- [Installation and configuration](docs/installation.md)
- [Development and testing](docs/development.md)
- [Compiling and packaging](docs/compiling.md)
- [Compatibility mode with mysql-admutils](docs/mysql-admutils-compatibility.md)
- [Use with NixOS](docs/nixos.md)
- [SUID/SGID mode](docs/suid-sgid-mode.md)
