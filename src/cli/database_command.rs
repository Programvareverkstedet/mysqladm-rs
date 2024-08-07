use anyhow::Context;
use clap::Parser;
use dialoguer::Editor;
use prettytable::{Cell, Row, Table};
use sqlx::{Connection, MySqlConnection};

use crate::core::{
    common::{close_database_connection, get_current_unix_user, yn, CommandStatus},
    database_operations::*,
    database_privilege_operations::*,
    user_operations::user_exists,
};

#[derive(Parser)]
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

#[derive(Parser)]
pub struct DatabaseCreateArgs {
    /// The name of the database(s) to create.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct DatabaseDropArgs {
    /// The name of the database(s) to drop.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct DatabaseListArgs {
    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser)]
pub struct DatabaseShowPrivsArgs {
    /// The name of the database(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser)]
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
    mut connection: MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    let result = connection
        .transaction(|txn| {
            Box::pin(async move {
                match command {
                    DatabaseCommand::CreateDb(args) => create_databases(args, txn).await,
                    DatabaseCommand::DropDb(args) => drop_databases(args, txn).await,
                    DatabaseCommand::ListDb(args) => list_databases(args, txn).await,
                    DatabaseCommand::ShowDbPrivs(args) => show_database_privileges(args, txn).await,
                    DatabaseCommand::EditDbPrivs(args) => edit_privileges(args, txn).await,
                }
            })
        })
        .await;

    close_database_connection(connection).await;

    result
}

async fn create_databases(
    args: DatabaseCreateArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let mut result = CommandStatus::SuccessfullyModified;

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = create_database(&name, connection).await {
            eprintln!("Failed to create database '{}': {}", name, e);
            eprintln!("Skipping...");
            result = CommandStatus::PartiallySuccessfullyModified;
        } else {
            println!("Database '{}' created.", name);
        }
    }

    Ok(result)
}

async fn drop_databases(
    args: DatabaseDropArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    let mut result = CommandStatus::SuccessfullyModified;

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = drop_database(&name, connection).await {
            eprintln!("Failed to drop database '{}': {}", name, e);
            eprintln!("Skipping...");
            result = CommandStatus::PartiallySuccessfullyModified;
        } else {
            println!("Database '{}' dropped.", name);
        }
    }

    Ok(result)
}

async fn list_databases(
    args: DatabaseListArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    let databases = get_database_list(connection).await?;

    if args.json {
        println!("{}", serde_json::to_string_pretty(&databases)?);
        return Ok(CommandStatus::NoModificationsIntended);
    }

    if databases.is_empty() {
        println!("No databases to show.");
    } else {
        for db in databases {
            println!("{}", db);
        }
    }

    Ok(CommandStatus::NoModificationsIntended)
}

async fn show_database_privileges(
    args: DatabaseShowPrivsArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    let database_users_to_show = if args.name.is_empty() {
        get_all_database_privileges(connection).await?
    } else {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        let mut result = Vec::with_capacity(args.name.len());
        for name in args.name {
            match get_database_privileges(&name, connection).await {
                Ok(db) => result.extend(db),
                Err(e) => {
                    eprintln!("Failed to show database '{}': {}", name, e);
                    eprintln!("Skipping...");
                }
            }
        }
        result
    };

    if args.json {
        println!("{}", serde_json::to_string_pretty(&database_users_to_show)?);
        return Ok(CommandStatus::NoModificationsIntended);
    }

    if database_users_to_show.is_empty() {
        println!("No database users to show.");
    } else {
        let mut table = Table::new();
        table.add_row(Row::new(
            DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .map(db_priv_field_human_readable_name)
                .map(|name| Cell::new(&name))
                .collect(),
        ));

        for row in database_users_to_show {
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

    Ok(CommandStatus::NoModificationsIntended)
}

pub async fn edit_privileges(
    args: DatabaseEditPrivsArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<CommandStatus> {
    let privilege_data = if let Some(name) = &args.name {
        get_database_privileges(name, connection).await?
    } else {
        get_all_database_privileges(connection).await?
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

    for row in privileges_to_change.iter() {
        if !user_exists(&row.user, connection).await? {
            // TODO: allow user to return and correct their mistake
            anyhow::bail!("User {} does not exist", row.user);
        }
    }

    let diffs = diff_privileges(privilege_data, &privileges_to_change);

    if diffs.is_empty() {
        println!("No changes to make.");
        return Ok(CommandStatus::NoModificationsNeeded);
    }

    // TODO: Add confirmation prompt.

    apply_privilege_diffs(diffs, connection).await?;

    Ok(CommandStatus::SuccessfullyModified)
}

pub fn parse_privilege_tables_from_args(
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

pub fn edit_privileges_with_editor(
    privilege_data: &[DatabasePrivilegeRow],
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = get_current_unix_user()?;

    let editor_content =
        generate_editor_content_from_privilege_data(privilege_data, &unix_user.name);

    // TODO: handle errors better here
    let result = Editor::new()
        .extension("tsv")
        .edit(&editor_content)?
        .unwrap();

    parse_privilege_data_from_editor_content(result)
        .context("Could not parse privilege data from editor")
}
