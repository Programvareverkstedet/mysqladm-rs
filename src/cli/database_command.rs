use anyhow::{anyhow, Context};
use clap::Parser;
use dialoguer::Editor;
use indoc::indoc;
use itertools::Itertools;
use prettytable::{Cell, Row, Table};
use sqlx::{Connection, MySqlConnection};

use crate::core::{
    common::{close_database_connection, get_current_unix_user, yn},
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
) -> anyhow::Result<()> {
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
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = create_database(&name, connection).await {
            eprintln!("Failed to create database '{}': {}", name, e);
            eprintln!("Skipping...");
        }
    }

    Ok(())
}

async fn drop_databases(
    args: DatabaseDropArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = drop_database(&name, connection).await {
            eprintln!("Failed to drop database '{}': {}", name, e);
            eprintln!("Skipping...");
        }
    }

    Ok(())
}

async fn list_databases(
    args: DatabaseListArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let databases = get_database_list(connection).await?;

    if databases.is_empty() {
        println!("No databases to show.");
        return Ok(());
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&databases)?);
    } else {
        for db in databases {
            println!("{}", db);
        }
    }

    Ok(())
}

async fn show_database_privileges(
    args: DatabaseShowPrivsArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
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

    if database_users_to_show.is_empty() {
        println!("No database users to show.");
        return Ok(());
    }

    if args.json {
        println!("{}", serde_json::to_string_pretty(&database_users_to_show)?);
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

    Ok(())
}

/// See documentation for `DatabaseCommand::EditDbPrivs`.
fn parse_privilege_table_cli_arg(arg: &str) -> anyhow::Result<DatabasePrivilegeRow> {
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid argument format. See `edit-db-privs --help` for more information.");
    }

    let db = parts[0].to_string();
    let user = parts[1].to_string();
    let privs = parts[2].to_string();

    let mut result = DatabasePrivilegeRow {
        db,
        user,
        select_priv: false,
        insert_priv: false,
        update_priv: false,
        delete_priv: false,
        create_priv: false,
        drop_priv: false,
        alter_priv: false,
        index_priv: false,
        create_tmp_table_priv: false,
        lock_tables_priv: false,
        references_priv: false,
    };

    for char in privs.chars() {
        match char {
            's' => result.select_priv = true,
            'i' => result.insert_priv = true,
            'u' => result.update_priv = true,
            'd' => result.delete_priv = true,
            'c' => result.create_priv = true,
            'D' => result.drop_priv = true,
            'a' => result.alter_priv = true,
            'I' => result.index_priv = true,
            't' => result.create_tmp_table_priv = true,
            'l' => result.lock_tables_priv = true,
            'r' => result.references_priv = true,
            'A' => {
                result.select_priv = true;
                result.insert_priv = true;
                result.update_priv = true;
                result.delete_priv = true;
                result.create_priv = true;
                result.drop_priv = true;
                result.alter_priv = true;
                result.index_priv = true;
                result.create_tmp_table_priv = true;
                result.lock_tables_priv = true;
                result.references_priv = true;
            }
            _ => anyhow::bail!("Invalid privilege character: {}", char),
        }
    }

    Ok(result)
}

fn parse_privilege(yn: &str) -> anyhow::Result<bool> {
    match yn.to_ascii_lowercase().as_str() {
        "y" => Ok(true),
        "n" => Ok(false),
        _ => Err(anyhow!("Expected Y or N, found {}", yn)),
    }
}

fn parse_privilege_data_from_editor(content: String) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    content
        .trim()
        .split('\n')
        .map(|line| line.trim())
        .filter(|line| !(line.starts_with('#') || line.starts_with("//") || line == &""))
        .skip(1)
        .map(|line| {
            let line_parts: Vec<&str> = line.trim().split_ascii_whitespace().collect();
            if line_parts.len() != DATABASE_PRIVILEGE_FIELDS.len() {
                anyhow::bail!("")
            }

            Ok(DatabasePrivilegeRow {
                db: (*line_parts.first().unwrap()).to_owned(),
                user: (*line_parts.get(1).unwrap()).to_owned(),
                select_priv: parse_privilege(line_parts.get(2).unwrap())
                    .context("Could not parse SELECT privilege")?,
                insert_priv: parse_privilege(line_parts.get(3).unwrap())
                    .context("Could not parse INSERT privilege")?,
                update_priv: parse_privilege(line_parts.get(4).unwrap())
                    .context("Could not parse UPDATE privilege")?,
                delete_priv: parse_privilege(line_parts.get(5).unwrap())
                    .context("Could not parse DELETE privilege")?,
                create_priv: parse_privilege(line_parts.get(6).unwrap())
                    .context("Could not parse CREATE privilege")?,
                drop_priv: parse_privilege(line_parts.get(7).unwrap())
                    .context("Could not parse DROP privilege")?,
                alter_priv: parse_privilege(line_parts.get(8).unwrap())
                    .context("Could not parse ALTER privilege")?,
                index_priv: parse_privilege(line_parts.get(9).unwrap())
                    .context("Could not parse INDEX privilege")?,
                create_tmp_table_priv: parse_privilege(line_parts.get(10).unwrap())
                    .context("Could not parse CREATE TEMPORARY TABLE privilege")?,
                lock_tables_priv: parse_privilege(line_parts.get(11).unwrap())
                    .context("Could not parse LOCK TABLES privilege")?,
                references_priv: parse_privilege(line_parts.get(12).unwrap())
                    .context("Could not parse REFERENCES privilege")?,
            })
        })
        .collect::<anyhow::Result<Vec<DatabasePrivilegeRow>>>()
}

fn format_privileges_line(
    privs: &DatabasePrivilegeRow,
    username_len: usize,
    database_name_len: usize,
) -> String {
    // Format a privileges line by padding each value with spaces
    // The first two fields are padded to the length of the longest username and database name
    // The remaining fields are padded to the length of the corresponding field name

    DATABASE_PRIVILEGE_FIELDS
        .into_iter()
        .map(|field| match field {
            "db" => format!("{:width$}", privs.db, width = database_name_len),
            "user" => format!("{:width$}", privs.user, width = username_len),
            privilege => format!(
                "{:width$}",
                yn(privs.get_privilege_by_name(privilege)),
                width = db_priv_field_human_readable_name(privilege).len()
            ),
        })
        .join(" ")
        .trim()
        .to_string()
}

pub async fn edit_privileges(
    args: DatabaseEditPrivsArgs,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let privilege_data = if let Some(name) = &args.name {
        get_database_privileges(name, connection).await?
    } else {
        get_all_database_privileges(connection).await?
    };

    let privileges_to_change = if !args.privs.is_empty() {
        if let Some(name) = args.name {
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
        }
    } else {
        let comment = indoc! {r#"
            # Welcome to the privilege editor.
            # Each line defines what privileges a single user has on a single database.
            # The first two columns respectively represent the database name and the user, and the remaining columns are the privileges.
            # If the user should have a certain privilege, write 'Y', otherwise write 'N'.
            #
            # Lines starting with '#' are comments and will be ignored.
        "#};

        let unix_user = get_current_unix_user()?;
        let example_user = format!("{}_user", unix_user.name);
        let example_db = format!("{}_db", unix_user.name);

        let longest_username = privilege_data
            .iter()
            .map(|p| p.user.len())
            .max()
            .unwrap_or(example_user.len());

        let longest_database_name = privilege_data
            .iter()
            .map(|p| p.db.len())
            .max()
            .unwrap_or(example_db.len());

        let mut header: Vec<_> = DATABASE_PRIVILEGE_FIELDS
            .into_iter()
            .map(db_priv_field_human_readable_name)
            .collect();

        // Pad the first two columns with spaces to align the privileges.
        header[0] = format!("{:width$}", header[0], width = longest_database_name);
        header[1] = format!("{:width$}", header[1], width = longest_username);

        let example_line = format_privileges_line(
            &DatabasePrivilegeRow {
                db: example_db,
                user: example_user,
                select_priv: true,
                insert_priv: true,
                update_priv: true,
                delete_priv: true,
                create_priv: false,
                drop_priv: false,
                alter_priv: false,
                index_priv: false,
                create_tmp_table_priv: false,
                lock_tables_priv: false,
                references_priv: false,
            },
            longest_username,
            longest_database_name,
        );

        // TODO: handle errors better here
        let result = Editor::new()
            .extension("tsv")
            .edit(
                format!(
                    "{}\n{}\n{}",
                    comment,
                    header.join(" "),
                    if privilege_data.is_empty() {
                        format!("# {}", example_line)
                    } else {
                        privilege_data
                            .iter()
                            .map(|privs| {
                                format_privileges_line(
                                    privs,
                                    longest_username,
                                    longest_database_name,
                                )
                            })
                            .join("\n")
                    }
                )
                .as_str(),
            )?
            .unwrap();

        parse_privilege_data_from_editor(result)
            .context("Could not parse privilege data from editor")?
    };

    for row in privileges_to_change.iter() {
        if !user_exists(&row.user, connection).await? {
            // TODO: allow user to return and correct their mistake
            anyhow::bail!("User {} does not exist", row.user);
        }
    }

    let diffs = diff_privileges(privilege_data, &privileges_to_change).await;

    if diffs.is_empty() {
        println!("No changes to make.");
        return Ok(());
    }

    // TODO: Add confirmation prompt.

    apply_privilege_diffs(diffs, connection).await?;

    Ok(())
}
