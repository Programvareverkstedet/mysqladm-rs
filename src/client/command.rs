use std::collections::BTreeSet;

use anyhow::Context;
use clap::Parser;
use dialoguer::{Confirm, Editor, Password};
use futures_util::{SinkExt, StreamExt};
use nix::unistd::{User, getuid};
use prettytable::{Cell, Row, Table};

use crate::{
    core::{
        common::yn,
        database_privileges::{
            DATABASE_PRIVILEGE_FIELDS, DatabasePrivilegeEditEntry, DatabasePrivilegeRow,
            DatabasePrivilegeRowDiff, DatabasePrivilegesDiff, create_or_modify_privilege_rows,
            db_priv_field_human_readable_name, diff_privileges, display_privilege_diffs,
            generate_editor_content_from_privilege_data, parse_privilege_data_from_editor_content,
            reduce_privilege_diffs,
        },
        protocol::{
            ClientToServerMessageStream, ListUsersError, MySQLDatabase, MySQLUser, Request,
            Response, print_create_databases_output_status,
            print_create_databases_output_status_json, print_create_users_output_status,
            print_create_users_output_status_json, print_drop_databases_output_status,
            print_drop_databases_output_status_json, print_drop_users_output_status,
            print_drop_users_output_status_json, print_lock_users_output_status,
            print_lock_users_output_status_json, print_modify_database_privileges_output_status,
            print_set_password_output_status, print_unlock_users_output_status,
            print_unlock_users_output_status_json,
        },
    },
};

#[derive(Parser, Debug, Clone)]
pub enum ClientCommand {
    /// Create one or more databases
    #[command()]
    CreateDb(DatabaseCreateArgs),

    /// Delete one or more databases
    #[command()]
    DropDb(DatabaseDropArgs),

    /// Print information about one or more databases
    ///
    /// If no database name is provided, all databases you have access will be shown.
    #[command()]
    ShowDb(DatabaseShowArgs),

    /// Print user privileges for one or more databases
    ///
    /// If no database names are provided, all databases you have access to will be shown.
    #[command()]
    ShowDbPrivs(DatabaseShowPrivsArgs),

    /// Change user privileges for one or more databases. See `edit-db-privs --help` for details.
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
    ///   e.g. `edit-db-privs my_db -p my_user+siu` is equivalent to `edit-db-privs -p my_db:my_user:siu`.
    ///   While it doesn't make much of a difference for a single edit, it can be useful for editing multiple users
    ///   on the same database at once.
    ///
    ///   Example usage of non-interactive mode:
    ///
    ///     Enable privileges `SELECT`, `INSERT`, and `UPDATE` for user `my_user` on database `my_db`:
    ///
    ///       `mysqladm edit-db-privs -p my_db:my_user:siu`
    ///
    ///     Enable all privileges for user `my_other_user` on database `my_other_db`:
    ///
    ///       `mysqladm edit-db-privs -p my_other_db:my_other_user:A`
    ///
    ///     Set miscellaneous privileges for multiple users on database `my_db`:
    ///
    ///       `mysqladm edit-db-privs my_db -p my_user:siu my_other_user:ct``
    ///
    #[command(verbatim_doc_comment)]
    EditDbPrivs(DatabaseEditPrivsArgs),

    /// Create one or more users
    #[command()]
    CreateUser(UserCreateArgs),

    /// Delete one or more users
    #[command()]
    DropUser(UserDeleteArgs),

    /// Change the MySQL password for a user
    #[command()]
    PasswdUser(UserPasswdArgs),

    /// Print information about one or more users
    ///
    /// If no username is provided, all users you have access will be shown.
    #[command()]
    ShowUser(UserShowArgs),

    /// Lock account for one or more users
    #[command()]
    LockUser(UserLockArgs),

    /// Unlock account for one or more users
    #[command()]
    UnlockUser(UserUnlockArgs),
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseCreateArgs {
    /// The name of the database(s) to create
    #[arg(num_args = 1..)]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseDropArgs {
    /// The name of the database(s) to drop
    #[arg(num_args = 1..)]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseShowArgs {
    /// The name of the database(s) to show
    #[arg(num_args = 0..)]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseShowPrivsArgs {
    /// The name of the database(s) to show
    #[arg(num_args = 0..)]
    name: Vec<MySQLDatabase>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseEditPrivsArgs {
    /// The name of the database to edit privileges for
    pub name: Option<MySQLDatabase>,

    #[arg(short, long, value_name = "[DATABASE:]USER:[+-]PRIVILEGES", num_args = 0.., value_parser = DatabasePrivilegeEditEntry::parse_from_str)]
    pub privs: Vec<DatabasePrivilegeEditEntry>,

    /// Print the information as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Specify the text editor to use for editing privileges
    #[arg(short, long)]
    pub editor: Option<String>,

    /// Disable interactive confirmation before saving changes
    #[arg(short, long)]
    pub yes: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserCreateArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Do not ask for a password, leave it unset
    #[clap(long)]
    no_password: bool,

    /// Print the information as JSON
    ///
    /// Note that this implies `--no-password`, since the command will become non-interactive.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserDeleteArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserPasswdArgs {
    username: MySQLUser,

    #[clap(short, long)]
    password_file: Option<String>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserShowArgs {
    #[arg(num_args = 0..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserLockArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct UserUnlockArgs {
    #[arg(num_args = 1..)]
    username: Vec<MySQLUser>,

    /// Print the information as JSON
    #[arg(short, long)]
    json: bool,
}

pub async fn handle_command(
    command: ClientCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        ClientCommand::CreateDb(args) => create_databases(args, server_connection).await,
        ClientCommand::DropDb(args) => drop_databases(args, server_connection).await,
        ClientCommand::ShowDb(args) => show_databases(args, server_connection).await,
        ClientCommand::ShowDbPrivs(args) => show_database_privileges(args, server_connection).await,
        ClientCommand::EditDbPrivs(args) => edit_database_privileges(args, server_connection).await,
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

async fn create_databases(
    args: DatabaseCreateArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let message = Request::CreateDatabases(args.name.to_owned());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_create_databases_output_status_json(&result);
    } else {
        print_create_databases_output_status(&result);
    }

    Ok(())
}

async fn drop_databases(
    args: DatabaseDropArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let message = Request::DropDatabases(args.name.to_owned());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::DropDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_drop_databases_output_status_json(&result);
    } else {
        print_drop_databases_output_status(&result);
    };

    Ok(())
}

async fn show_databases(
    args: DatabaseShowArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.name.is_empty() {
        Request::ListDatabases(None)
    } else {
        Request::ListDatabases(Some(args.name.to_owned()))
    };

    server_connection.send(message).await?;

    // TODO: collect errors for json output.

    let database_list = match server_connection.next().await {
        Some(Ok(Response::ListDatabases(databases))) => databases
            .into_iter()
            .filter_map(|(database_name, result)| match result {
                Ok(database_row) => Some(database_row),
                Err(err) => {
                    eprintln!("{}", err.to_error_message(&database_name));
                    eprintln!("Skipping...");
                    println!();
                    None
                }
            })
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllDatabases(database_list))) => match database_list {
            Ok(list) => list,
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(
                    anyhow::anyhow!(err.to_error_message()).context("Failed to list databases")
                );
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&database_list)?);
    } else if database_list.is_empty() {
        println!("No databases to show.");
    } else {
        let mut table = Table::new();
        table.add_row(Row::new(vec![Cell::new("Database")]));
        for db in database_list {
            table.add_row(row![db.database]);
        }
        table.printstd();
    }

    Ok(())
}

async fn show_database_privileges(
    args: DatabaseShowPrivsArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.name.is_empty() {
        Request::ListPrivileges(None)
    } else {
        Request::ListPrivileges(Some(args.name.to_owned()))
    };
    server_connection.send(message).await?;

    let privilege_data = match server_connection.next().await {
        Some(Ok(Response::ListPrivileges(databases))) => databases
            .into_iter()
            .filter_map(|(database_name, result)| match result {
                Ok(privileges) => Some(privileges),
                Err(err) => {
                    eprintln!("{}", err.to_error_message(&database_name));
                    eprintln!("Skipping...");
                    println!();
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllPrivileges(privilege_rows))) => match privilege_rows {
            Ok(list) => list,
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(anyhow::anyhow!(err.to_error_message())
                    .context("Failed to list database privileges"));
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&privilege_data)?);
    } else if privilege_data.is_empty() {
        println!("No database privileges to show.");
    } else {
        let mut table = Table::new();
        table.add_row(Row::new(
            DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .map(db_priv_field_human_readable_name)
                .map(|name| Cell::new(&name))
                .collect(),
        ));

        for row in privilege_data {
            table.add_row(row![
                row.db,
                row.user,
                c->yn(row.select_priv),
                c->yn(row.insert_priv),
                c->yn(row.update_priv),
                c->yn(row.delete_priv),
                c->yn(row.create_priv),
                c->yn(row.drop_priv),
                c->yn(row.alter_priv),
                c->yn(row.index_priv),
                c->yn(row.create_tmp_table_priv),
                c->yn(row.lock_tables_priv),
                c->yn(row.references_priv),
            ]);
        }
        table.printstd();
    }

    Ok(())
}

pub async fn edit_database_privileges(
    args: DatabaseEditPrivsArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = Request::ListPrivileges(args.name.to_owned().map(|name| vec![name]));

    server_connection.send(message).await?;

    let existing_privilege_rows = match server_connection.next().await {
        Some(Ok(Response::ListPrivileges(databases))) => databases
            .into_iter()
            .filter_map(|(database_name, result)| match result {
                Ok(privileges) => Some(privileges),
                Err(err) => {
                    eprintln!("{}", err.to_error_message(&database_name));
                    eprintln!("Skipping...");
                    println!();
                    None
                }
            })
            .flatten()
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllPrivileges(privilege_rows))) => match privilege_rows {
            Ok(list) => list,
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(anyhow::anyhow!(err.to_error_message())
                    .context("Failed to list database privileges"));
            }
        },
        response => return erroneous_server_response(response),
    };

    let diffs: BTreeSet<DatabasePrivilegesDiff> = if !args.privs.is_empty() {
        let privileges_to_change = parse_privilege_tables_from_args(&args)?;
        create_or_modify_privilege_rows(&existing_privilege_rows, &privileges_to_change)?
    } else {
        let privileges_to_change =
            edit_privileges_with_editor(&existing_privilege_rows, args.name.as_ref())?;
        diff_privileges(&existing_privilege_rows, &privileges_to_change)
    };
    let diffs = reduce_privilege_diffs(&existing_privilege_rows, diffs)?;

    if diffs.is_empty() {
        println!("No changes to make.");
        server_connection.send(Request::Exit).await?;
        return Ok(());
    }

    println!("The following changes will be made:\n");
    println!("{}", display_privilege_diffs(&diffs));

    if !args.yes
        && !Confirm::new()
            .with_prompt("Do you want to apply these changes?")
            .default(false)
            .show_default(true)
            .interact()?
    {
        server_connection.send(Request::Exit).await?;
        return Ok(());
    }

    let message = Request::ModifyPrivileges(diffs);
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::ModifyPrivileges(result))) => result,
        response => return erroneous_server_response(response),
    };

    print_modify_database_privileges_output_status(&result);

    server_connection.send(Request::Exit).await?;

    Ok(())
}

fn parse_privilege_tables_from_args(
    args: &DatabaseEditPrivsArgs,
) -> anyhow::Result<BTreeSet<DatabasePrivilegeRowDiff>> {
    debug_assert!(!args.privs.is_empty());
    args.privs
        .iter()
        .map(|priv_edit_entry| {
            priv_edit_entry
                .as_database_privileges_diff(args.name.as_ref())
                .context(format!(
                    "Failed parsing database privileges: `{}`",
                    priv_edit_entry
                ))
        })
        .collect::<anyhow::Result<BTreeSet<DatabasePrivilegeRowDiff>>>()
}

fn edit_privileges_with_editor(
    privilege_data: &[DatabasePrivilegeRow],
    database_name: Option<&MySQLDatabase>,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = User::from_uid(getuid())
        .context("Failed to look up your UNIX username")
        .and_then(|u| u.ok_or(anyhow::anyhow!("Failed to look up your UNIX username")))?;

    let editor_content =
        generate_editor_content_from_privilege_data(privilege_data, &unix_user.name, database_name);

    // TODO: handle errors better here
    let result = Editor::new().extension("tsv").edit(&editor_content)?;

    match result {
        None => Ok(privilege_data.to_vec()),
        Some(result) => parse_privilege_data_from_editor_content(result)
            .context("Could not parse privilege data from editor"),
    }
}

async fn create_users(
    args: UserCreateArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::CreateUsers(args.username.to_owned());
    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(anyhow::Error::from(err).context("Failed to communicate with server"));
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    if args.json {
        print_create_users_output_status_json(&result);
    } else {
        print_create_users_output_status(&result);

        let successfully_created_users = result
            .iter()
            .filter_map(|(username, result)| result.as_ref().ok().map(|_| username))
            .collect::<Vec<_>>();

        for username in successfully_created_users {
            if !args.no_password
                && Confirm::new()
                    .with_prompt(format!(
                        "Do you want to set a password for user '{}'?",
                        username
                    ))
                    .default(false)
                    .interact()?
            {
                let password = read_password_from_stdin_with_double_check(username)?;
                let message = Request::PasswdUser(username.to_owned(), password);

                if let Err(err) = server_connection.send(message).await {
                    server_connection.close().await.ok();
                    anyhow::bail!(err);
                }

                match server_connection.next().await {
                    Some(Ok(Response::PasswdUser(result))) => {
                        print_set_password_output_status(&result, username)
                    }
                    response => return erroneous_server_response(response),
                }

                println!();
            }
        }
    }

    server_connection.send(Request::Exit).await?;

    Ok(())
}

async fn drop_users(
    args: UserDeleteArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::DropUsers(args.username.to_owned());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::DropUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_drop_users_output_status_json(&result);
    } else {
        print_drop_users_output_status(&result);
    }

    Ok(())
}

pub fn read_password_from_stdin_with_double_check(username: &MySQLUser) -> anyhow::Result<String> {
    Password::new()
        .with_prompt(format!("New MySQL password for user '{}'", username))
        .with_confirmation(
            format!("Retype new MySQL password for user '{}'", username),
            "Passwords do not match",
        )
        .interact()
        .map_err(Into::into)
}

async fn passwd_user(
    args: UserPasswdArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    // TODO: create a "user" exists check" command
    let message = Request::ListUsers(Some(vec![args.username.to_owned()]));
    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }
    let response = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users,
        response => return erroneous_server_response(response),
    };
    match response
        .get(&args.username)
        .unwrap_or(&Err(ListUsersError::UserDoesNotExist))
    {
        Ok(_) => {}
        Err(err) => {
            server_connection.send(Request::Exit).await?;
            server_connection.close().await.ok();
            anyhow::bail!("{}", err.to_error_message(&args.username));
        }
    }

    let password = if let Some(password_file) = args.password_file {
        std::fs::read_to_string(password_file)
            .context("Failed to read password file")?
            .trim()
            .to_string()
    } else {
        read_password_from_stdin_with_double_check(&args.username)?
    };

    let message = Request::PasswdUser(args.username.to_owned(), password);

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::PasswdUser(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_set_password_output_status(&result, &args.username);

    Ok(())
}

async fn show_users(
    args: UserShowArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = if args.username.is_empty() {
        Request::ListUsers(None)
    } else {
        Request::ListUsers(Some(args.username.to_owned()))
    };

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let users = match server_connection.next().await {
        Some(Ok(Response::ListUsers(users))) => users
            .into_iter()
            .filter_map(|(username, result)| match result {
                Ok(user) => Some(user),
                Err(err) => {
                    eprintln!("{}", err.to_error_message(&username));
                    eprintln!("Skipping...");
                    None
                }
            })
            .collect::<Vec<_>>(),
        Some(Ok(Response::ListAllUsers(users))) => match users {
            Ok(users) => users,
            Err(err) => {
                server_connection.send(Request::Exit).await?;
                return Err(
                    anyhow::anyhow!(err.to_error_message()).context("Failed to list all users")
                );
            }
        },
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        println!(
            "{}",
            serde_json::to_string_pretty(&users).context("Failed to serialize users to JSON")?
        );
    } else if users.is_empty() {
        println!("No users to show.");
    } else {
        let mut table = prettytable::Table::new();
        table.add_row(row![
            "User",
            "Password is set",
            "Locked",
            "Databases where user has privileges"
        ]);
        for user in users {
            table.add_row(row![
                user.user,
                user.has_password,
                user.is_locked,
                user.databases.join("\n")
            ]);
        }
        table.printstd();
    }

    Ok(())
}

async fn lock_users(
    args: UserLockArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::LockUsers(args.username.to_owned());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::LockUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_lock_users_output_status_json(&result);
    } else {
        print_lock_users_output_status(&result);
    }

    Ok(())
}

async fn unlock_users(
    args: UserUnlockArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.username.is_empty() {
        anyhow::bail!("No usernames provided");
    }

    let message = Request::UnlockUsers(args.username.to_owned());

    if let Err(err) = server_connection.send(message).await {
        server_connection.close().await.ok();
        anyhow::bail!(err);
    }

    let result = match server_connection.next().await {
        Some(Ok(Response::UnlockUsers(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    if args.json {
        print_unlock_users_output_status_json(&result);
    } else {
        print_unlock_users_output_status(&result);
    }

    Ok(())
}
