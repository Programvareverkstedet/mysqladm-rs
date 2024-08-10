use anyhow::Context;
use clap::Parser;
use dialoguer::{Confirm, Editor};
use futures_util::{SinkExt, StreamExt};
use nix::unistd::{getuid, User};
use prettytable::{Cell, Row, Table};

use crate::{
    cli::common::erroneous_server_response,
    core::{
        common::yn,
        database_privileges::{
            db_priv_field_human_readable_name, diff_privileges, display_privilege_diffs,
            generate_editor_content_from_privilege_data, parse_privilege_data_from_editor_content,
            parse_privilege_table_cli_arg,
        },
        protocol::{
            print_create_databases_output_status, print_drop_databases_output_status,
            print_modify_database_privileges_output_status, ClientToServerMessageStream, Request,
            Response,
        },
    },
    server::sql::database_privilege_operations::{DatabasePrivilegeRow, DATABASE_PRIVILEGE_FIELDS},
};

#[derive(Parser, Debug, Clone)]
// #[command(next_help_heading = Some(DATABASE_COMMAND_HEADER))]
pub enum DatabaseCommand {
    /// Create one or more databases
    #[command()]
    CreateDb(DatabaseCreateArgs),

    /// Delete one or more databases
    #[command()]
    DropDb(DatabaseDropArgs),

    /// List all databases you have access to
    #[command()]
    ListDb(DatabaseListArgs),

    /// List user privileges for one or more databases
    ///
    /// If no database names are provided, it will show privileges for all databases you have access to.
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
    ///    The privilege arguments should be formatted as `<db>:<user>:<privileges>`
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
    ///   e.g. `edit-db-privs my_db -p my_user:siu` is equivalent to `edit-db-privs -p my_db:my_user:siu`.
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
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseCreateArgs {
    /// The name of the database(s) to create.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseDropArgs {
    /// The name of the database(s) to drop.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseListArgs {
    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseShowPrivsArgs {
    /// The name of the database(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser, Debug, Clone)]
pub struct DatabaseEditPrivsArgs {
    /// The name of the database to edit privileges for.
    pub name: Option<String>,

    #[arg(short, long, value_name = "[DATABASE:]USER:PRIVILEGES", num_args = 0..)]
    pub privs: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    pub json: bool,

    /// Specify the text editor to use for editing privileges
    #[arg(short, long)]
    pub editor: Option<String>,

    /// Disable interactive confirmation before saving changes.
    #[arg(short, long)]
    pub yes: bool,
}

pub async fn handle_command(
    command: DatabaseCommand,
    server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    match command {
        DatabaseCommand::CreateDb(args) => create_databases(args, server_connection).await,
        DatabaseCommand::DropDb(args) => drop_databases(args, server_connection).await,
        DatabaseCommand::ListDb(args) => list_databases(args, server_connection).await,
        DatabaseCommand::ShowDbPrivs(args) => {
            show_database_privileges(args, server_connection).await
        }
        DatabaseCommand::EditDbPrivs(args) => {
            edit_database_privileges(args, server_connection).await
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

    let message = Request::CreateDatabases(args.name.clone());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::CreateDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_create_databases_output_status(&result);

    Ok(())
}

async fn drop_databases(
    args: DatabaseDropArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let message = Request::DropDatabases(args.name.clone());
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::DropDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    print_drop_databases_output_status(&result);

    Ok(())
}

async fn list_databases(
    args: DatabaseListArgs,
    mut server_connection: ClientToServerMessageStream,
) -> anyhow::Result<()> {
    let message = Request::ListDatabases;
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::ListAllDatabases(result))) => result,
        response => return erroneous_server_response(response),
    };

    server_connection.send(Request::Exit).await?;

    let database_list = match result {
        Ok(list) => list,
        Err(err) => {
            return Err(anyhow::anyhow!(err.to_error_message()).context("Failed to list databases"))
        }
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&database_list)?);
    } else if database_list.is_empty() {
        println!("No databases to show.");
    } else {
        for db in database_list {
            println!("{}", db);
        }
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
        Request::ListPrivileges(Some(args.name.clone()))
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
    let message = Request::ListPrivileges(args.name.clone().map(|name| vec![name]));

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

    // TODO: The data from args should not be absolute.
    //       In the current implementation, the user would need to
    //       provide all privileges for all users on all databases.
    //       The intended effect is to modify the privileges which have
    //       matching users and databases, as well as add any
    //       new db-user pairs. This makes it impossible to remove
    //       privileges, but that is an issue for another day.
    let privileges_to_change = if !args.privs.is_empty() {
        parse_privilege_tables_from_args(&args)?
    } else {
        edit_privileges_with_editor(&privilege_data)?
    };

    let diffs = diff_privileges(&privilege_data, &privileges_to_change);

    if diffs.is_empty() {
        println!("No changes to make.");
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

    // TODO: allow user to return and correct their mistake
    print_modify_database_privileges_output_status(&result);

    server_connection.send(Request::Exit).await?;

    Ok(())
}

fn parse_privilege_tables_from_args(
    args: &DatabaseEditPrivsArgs,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    debug_assert!(!args.privs.is_empty());
    let result = if let Some(name) = &args.name {
        args.privs
            .iter()
            .map(|p| {
                parse_privilege_table_cli_arg(&format!("{}:{}", name, &p))
                    .context(format!("Failed parsing database privileges: `{}`", &p))
            })
            .collect::<anyhow::Result<Vec<DatabasePrivilegeRow>>>()?
    } else {
        args.privs
            .iter()
            .map(|p| {
                parse_privilege_table_cli_arg(p)
                    .context(format!("Failed parsing database privileges: `{}`", &p))
            })
            .collect::<anyhow::Result<Vec<DatabasePrivilegeRow>>>()?
    };
    Ok(result)
}

fn edit_privileges_with_editor(
    privilege_data: &[DatabasePrivilegeRow],
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = User::from_uid(getuid())
        .context("Failed to look up your UNIX username")
        .and_then(|u| u.ok_or(anyhow::anyhow!("Failed to look up your UNIX username")))?;

    let editor_content =
        generate_editor_content_from_privilege_data(privilege_data, &unix_user.name);

    // TODO: handle errors better here
    let result = Editor::new().extension("tsv").edit(&editor_content)?;

    match result {
        None => Ok(privilege_data.to_vec()),
        Some(result) => parse_privilege_data_from_editor_content(result)
            .context("Could not parse privilege data from editor"),
    }
}
