mod create_db;
mod create_user;
mod drop_db;
mod drop_user;
mod edit_privs;
mod lock_user;
mod passwd_user;
mod show_db;
mod show_privs;
mod show_user;
mod unlock_user;

pub use create_db::*;
pub use create_user::*;
pub use drop_db::*;
pub use drop_user::*;
pub use edit_privs::*;
pub use lock_user::*;
pub use passwd_user::*;
pub use show_db::*;
pub use show_privs::*;
pub use show_user::*;
pub use unlock_user::*;

use clap::Parser;

use crate::core::protocol::{ClientToServerMessageStream, Response};

#[derive(Parser, Debug, Clone)]
pub enum ClientCommand {
    /// Create one or more databases
    #[command()]
    CreateDb(CreateDbArgs),

    /// Delete one or more databases
    #[command()]
    DropDb(DropDbArgs),

    /// Print information about one or more databases
    ///
    /// If no database name is provided, all databases you have access will be shown.
    #[command()]
    ShowDb(ShowDbArgs),

    /// Print user privileges for one or more databases
    ///
    /// If no database names are provided, all databases you have access to will be shown.
    #[command()]
    ShowPrivs(ShowPrivsArgs),

    /// Change user privileges for one or more databases. See `edit-privs --help` for details.
    ///
    /// This command has two modes of operation:
    ///
    /// 1. Interactive mode: If nothing else is specified, the user will be prompted to edit the privileges using a text editor.
    ///
    ///    You can configure your preferred text editor by setting the `VISUAL` or `EDITOR` environment variables.
    ///
    ///    Follow the instructions inside the editor for more information.
    ///
    /// 2. Non-interactive mode: If the `-p` flag is specified, the user can write privileges using arguments.
    ///
    ///    The privilege arguments should be formatted as `<db>:<user>+<privileges>-<privileges>`
    ///    where the privileges are a string of characters, each representing a single privilege.
    ///    The character `A` is an exception - it represents all privileges.
    ///
    ///    The character-to-privilege mapping is defined as follows:
    ///
    ///    - `s` - SELECT
    ///    - `i` - INSERT
    ///    - `u` - UPDATE
    ///    - `d` - DELETE
    ///    - `c` - CREATE
    ///    - `D` - DROP
    ///    - `a` - ALTER
    ///    - `I` - INDEX
    ///    - `t` - CREATE TEMPORARY TABLES
    ///    - `l` - LOCK TABLES
    ///    - `r` - REFERENCES
    ///    - `A` - ALL PRIVILEGES
    ///
    ///   If you provide a database name, you can omit it from the privilege string,
    ///   e.g. `edit-privs my_db -p my_user+siu` is equivalent to `edit-privs -p my_db:my_user:siu`.
    ///   While it doesn't make much of a difference for a single edit, it can be useful for editing multiple users
    ///   on the same database at once.
    ///
    ///   Example usage of non-interactive mode:
    ///
    ///     Enable privileges `SELECT`, `INSERT`, and `UPDATE` for user `my_user` on database `my_db`:
    ///
    ///       `muscl edit-privs -p my_db:my_user:siu`
    ///
    ///     Enable all privileges for user `my_other_user` on database `my_other_db`:
    ///
    ///       `muscl edit-privs -p my_other_db:my_other_user:A`
    ///
    ///     Set miscellaneous privileges for multiple users on database `my_db`:
    ///
    ///       `muscl edit-privs my_db -p my_user:siu my_other_user:ct``
    ///
    #[command(verbatim_doc_comment)]
    EditPrivs(EditPrivsArgs),

    /// Create one or more users
    #[command()]
    CreateUser(CreateUserArgs),

    /// Delete one or more users
    #[command()]
    DropUser(DropUserArgs),

    /// Change the MySQL password for a user
    #[command()]
    PasswdUser(PasswdUserArgs),

    /// Print information about one or more users
    ///
    /// If no username is provided, all users you have access will be shown.
    #[command()]
    ShowUser(ShowUserArgs),

    /// Lock account for one or more users
    #[command()]
    LockUser(LockUserArgs),

    /// Unlock account for one or more users
    #[command()]
    UnlockUser(UnlockUserArgs),
}

pub async fn handle_command(
    command: ClientCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        ClientCommand::CreateDb(args) => create_databases(args, server_connection).await,
        ClientCommand::DropDb(args) => drop_databases(args, server_connection).await,
        ClientCommand::ShowDb(args) => show_databases(args, server_connection).await,
        ClientCommand::ShowPrivs(args) => show_database_privileges(args, server_connection).await,
        ClientCommand::EditPrivs(args) => edit_database_privileges(args, server_connection).await,
        ClientCommand::CreateUser(args) => create_users(args, server_connection).await,
        ClientCommand::DropUser(args) => drop_users(args, server_connection).await,
        ClientCommand::PasswdUser(args) => passwd_user(args, server_connection).await,
        ClientCommand::ShowUser(args) => show_users(args, server_connection).await,
        ClientCommand::LockUser(args) => lock_users(args, server_connection).await,
        ClientCommand::UnlockUser(args) => unlock_users(args, server_connection).await,
    }
}

pub fn erroneous_server_response(
    response: Option<Result<Response, std::io::Error>>,
) -> anyhow::Result<()> {
    match response {
        Some(Ok(Response::Error(e))) => {
            anyhow::bail!("Server returned error: {}", e);
        }
        Some(Err(e)) => {
            anyhow::bail!(e);
        }
        Some(response) => {
            anyhow::bail!("Unexpected response from server: {:?}", response);
        }
        None => {
            anyhow::bail!("No response from server");
        }
    }
}
