mod check_auth;
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

pub use check_auth::*;
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

use clap::Subcommand;
use futures_util::SinkExt;
use itertools::Itertools;
use tokio_stream::StreamExt;

use crate::core::protocol::{ClientToServerMessageStream, Request, Response};

const EDIT_PRIVS_EXAMPLES: &str = color_print::cstr!(
    r#"
<bold><underline>Examples:</underline></bold>
  # Open interactive editor to edit privileges
  muscl edit-privs

  # Set privileges `SELECT`, `INSERT`, and `UPDATE` for user `my_user` on database `my_db`
  muscl edit-privs my_db my_user siu

  # Set all privileges for user `my_other_user` on database `my_other_db`
  muscl edit-privs my_other_db my_other_user A

  # Add the `DELETE` privilege for user `my_user` on database `my_db`
  muscl edit-privs my_db my_user +d

  # Set miscellaneous privileges for multiple users on database `my_db`
  muscl edit-privs -p my_db:my_user:siu -p my_db:my_other_user:+ct -p my_db:yet_another_user:-d
"#
);

#[derive(Subcommand, Debug, Clone)]
#[command(subcommand_required = true)]
pub enum ClientCommand {
    /// Check whether you are authorized to manage the specified databases or users.
    CheckAuth(CheckAuthArgs),

    /// Create one or more databases
    CreateDb(CreateDbArgs),

    /// Delete one or more databases
    DropDb(DropDbArgs),

    /// Print information about one or more databases
    ///
    /// If no database name is provided, all databases you have access will be shown.
    ShowDb(ShowDbArgs),

    /// Print user privileges for one or more databases
    ///
    /// If no database names are provided, all databases you have access to will be shown.
    ShowPrivs(ShowPrivsArgs),

    /// Change user privileges for one or more databases. See `edit-privs --help` for details.
    ///
    /// This command has three modes of operation:
    ///
    /// 1. Interactive mode:
    ///
    ///    If no arguments are provided, the user will be prompted to edit the privileges using a text editor.
    ///
    ///    You can configure your preferred text editor by setting the `VISUAL` or `EDITOR` environment variables.
    ///
    ///    Follow the instructions inside the editor for more information.
    ///
    /// 2. Non-interactive human-friendly mode:
    ///
    ///    You can provide the command with three positional arguments:
    ///
    ///    - `<DB_NAME>`: The name of the database for which you want to edit privileges.
    ///    - `<USER_NAME>`: The name of the user whose privileges you want to edit.
    ///    - `<[+-]PRIVILEGES>`: A string representing the privileges to set for the user.
    ///
    ///    The `<[+-]PRIVILEGES>` argument is a string of characters, each representing a single privilege.
    ///    The character `A` is an exception - it represents all privileges.
    ///    The optional leading character can be either `+` to grant additional privileges or `-` to revoke privileges.
    ///    If omitted, the privileges will be set exactly as specified, removing any privileges not listed, and adding any that are.
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
    /// 3. Non-interactive batch mode:
    ///
    ///    By using the `-p` flag, you can provide multiple privilege edits in a single command.
    ///
    ///    The flag value should be formatted as `DB_NAME:USER_NAME:[+-]PRIVILEGES`
    ///    where the privileges are a string of characters, each representing a single privilege.
    ///    (See the character-to-privilege mapping above.)
    ///
    #[command(
        verbatim_doc_comment,
        override_usage = "muscl edit-privs [OPTIONS] [ -p <DB_NAME:USER_NAME:[+-]PRIVILEGES>... | <DB_NAME> <USER_NAME> <[+-]PRIVILEGES> ]",
        after_long_help = EDIT_PRIVS_EXAMPLES,
    )]
    EditPrivs(EditPrivsArgs),

    /// Create one or more users
    CreateUser(CreateUserArgs),

    /// Delete one or more users
    DropUser(DropUserArgs),

    /// Change the MySQL password for a user
    PasswdUser(PasswdUserArgs),

    /// Print information about one or more users
    ///
    /// If no username is provided, all users you have access will be shown.
    ShowUser(ShowUserArgs),

    /// Lock account for one or more users
    LockUser(LockUserArgs),

    /// Unlock account for one or more users
    UnlockUser(UnlockUserArgs),
}

pub async fn handle_command(
    command: ClientCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        ClientCommand::CheckAuth(args) => check_authorization(args, server_connection).await,
        ClientCommand::CreateDb(args) => create_databases(args, server_connection).await,
        ClientCommand::DropDb(args) => drop_databases(args, server_connection).await,
        ClientCommand::ShowDb(args) => show_databases(args, server_connection).await,
        ClientCommand::ShowPrivs(args) => show_database_privileges(args, server_connection).await,
        ClientCommand::EditPrivs(args) => {
            edit_database_privileges(args, None, server_connection).await
        }
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

pub async fn print_authorization_owner_hint(
    server_connection: &mut ClientToServerMessageStream,
) -> anyhow::Result<()> {
    server_connection
        .send(Request::ListValidNamePrefixes)
        .await?;

    let response = match server_connection.next().await {
        Some(Ok(Response::ListValidNamePrefixes(prefixes))) => prefixes,
        response => return erroneous_server_response(response),
    };

    println!(
        "Note: You are allowed to manage databases and users with the following prefixes:\n{}",
        response.into_iter().map(|p| format!(" - {}", p)).join("\n")
    );

    Ok(())
}
