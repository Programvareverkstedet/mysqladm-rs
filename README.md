[![Coverage](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/badges/for_the_badge.svg)](https://pages.pvv.ntnu.no/Projects/muscl/main/coverage/)
[![Docs](https://img.shields.io/badge/rust_docs-blue?style=for-the-badge&logo=rust)](https://pages.pvv.ntnu.no/Projects/muscl/main/docs/muscl/)

# muscl 💪

Dropping DBs (dumbbells) and having MySQL spasms since 2024

## What is this?

`muscl is a secure MySQL administration tool for multi-user systems.
It allows unprivileged users to manage their own databases and database users without granting them direct access to the MySQL server.
Authorization is handled by a prefix-based model tied to Unix users and groups, making it ideal for shared hosting environments, like university servers, tilde servers, or similar.

When a user requests an administrative operation, the `muscl` daemon verifies authenticates the user through unix socket peer credentials,
and then checks the requested item name against the user's username and group list for authorization.
The default authorization mechanism only allows the user to manage items prefixed with either their username or a group name.
For example, a user would be allowed to manage items like `<user>_mydb`, `<user>_mydbuser`, or `<group>_myotherdb`.

The available administrative operations include:

- creating/listing/modifying/deleting databases and database users
- modifying privileges for a database user on a database
- changing the passwords of the database users
- locking and unlocking database users
- ... and more

The software is designed to be run as a client and a server. The clients are run by the unprivileged users,
and does not have direct access to the MySQL server. Instead, they communicate with the muscl server
over a IPC, which then performs the requested operations on behalf of the clients.

## Documentation

- [Installation and configuration](docs/installation.md)
- [Development and testing](docs/development.md)
- [Compiling and packaging](docs/compiling.md)
- [Compatibility mode with mysql-admutils](docs/mysql-admutils-compatibility.md)
- [Use with NixOS](docs/nixos.md)
- [SUID/SGID mode](docs/suid-sgid-mode.md)
