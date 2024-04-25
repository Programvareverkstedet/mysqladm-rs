use anyhow::{anyhow, Context};
use clap::Parser;
use indoc::indoc;
use itertools::Itertools;
use prettytable::{Cell, Row, Table};
use rand::prelude::*;
use sqlx::{Connection, MySqlConnection};

use crate::core::{
    self,
    common::get_current_unix_user,
    database_operations::{
        apply_permission_diffs, db_priv_field_human_readable_name, diff_permissions, yn,
        DatabasePrivileges, DATABASE_PRIVILEGE_FIELDS,
    },
};

#[derive(Parser)]
pub struct DatabaseArgs {
    #[clap(subcommand)]
    subcmd: DatabaseCommand,
}

// TODO: Support batch creation/dropping,showing of databases,
//       using a comma-separated list of database names.

#[derive(Parser)]
enum DatabaseCommand {
    /// Create the DATABASE(S).
    #[command(alias = "add", alias = "c")]
    Create(DatabaseCreateArgs),

    /// Delete the DATABASE(S).
    #[command(alias = "remove", alias = "delete", alias = "rm", alias = "d")]
    Drop(DatabaseDropArgs),

    /// List the DATABASE(S) you own.
    #[command()]
    List(DatabaseListArgs),

    /// Give information about the DATABASE(S), or if none are given, all the ones you own.
    ///
    /// In particular, this will show the permissions for the database(s) owned by the current user.
    #[command(alias = "s")]
    ShowPerm(DatabaseShowPermArgs),

    /// Change permissions for the DATABASE(S). Run `edit-perm --help` for more information.
    ///
    /// This command has two modes of operation:
    /// 1. Interactive mode: If nothing else is specified, the user will be prompted to edit the permissions using a text editor.
    ///
    ///    Follow the instructions inside the editor for more information.
    ///
    /// 2. Non-interactive mode: If the `-p` flag is specified, the user can write permissions using arguments.
    ///
    ///    The permission arguments should be formatted as `<db>:<user>:<privileges>`
    ///    where the privileges are a string of characters, each representing a single permissions.
    ///    The character `A` is an exception, because it represents all permissions.
    ///
    ///    The permission to character mapping is as follows:
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
    #[command(display_name = "edit-perm", alias = "e", verbatim_doc_comment)]
    EditPerm(DatabaseEditPermArgs),
}

#[derive(Parser)]
struct DatabaseCreateArgs {
    /// The name of the database(s) to create.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
struct DatabaseDropArgs {
    /// The name of the database(s) to drop.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
struct DatabaseListArgs {
    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser)]
struct DatabaseShowPermArgs {
    /// The name of the database(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser)]
struct DatabaseEditPermArgs {
    /// The name of the database to edit permissions for.
    name: Option<String>,

    #[arg(short, long, value_name = "[DATABASE:]USER:PERMISSIONS", num_args = 0..)]
    perm: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,

    /// Specify the text editor to use for editing permissions.
    #[arg(short, long)]
    editor: Option<String>,

    /// Disable confirmation before saving changes.
    #[arg(short, long)]
    yes: bool,
}

pub async fn handle_command(args: DatabaseArgs, mut conn: MySqlConnection) -> anyhow::Result<()> {
    let result = match args.subcmd {
        DatabaseCommand::Create(args) => create_databases(args, &mut conn).await,
        DatabaseCommand::Drop(args) => drop_databases(args, &mut conn).await,
        DatabaseCommand::List(args) => list_databases(args, &mut conn).await,
        DatabaseCommand::ShowPerm(args) => show_databases(args, &mut conn).await,
        DatabaseCommand::EditPerm(args) => edit_permissions(args, &mut conn).await,
    };

    conn.close().await?;

    result
}

async fn create_databases(
    args: DatabaseCreateArgs,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = core::database_operations::create_database(&name, conn).await {
            eprintln!("Failed to create database '{}': {}", name, e);
            eprintln!("Skipping...");
        }
    }

    Ok(())
}

async fn drop_databases(args: DatabaseDropArgs, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    if args.name.is_empty() {
        anyhow::bail!("No database names provided");
    }

    for name in args.name {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        if let Err(e) = core::database_operations::drop_database(&name, conn).await {
            eprintln!("Failed to drop database '{}': {}", name, e);
            eprintln!("Skipping...");
        }
    }

    Ok(())
}

async fn list_databases(args: DatabaseListArgs, conn: &mut MySqlConnection) -> anyhow::Result<()> {
    let databases = core::database_operations::get_database_list(conn).await?;

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

async fn show_databases(
    args: DatabaseShowPermArgs,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let database_users_to_show = if args.name.is_empty() {
        core::database_operations::get_all_database_privileges(conn).await?
    } else {
        // TODO: This can be optimized by fetching all the database privileges in one query.
        let mut result = Vec::with_capacity(args.name.len());
        for name in args.name {
            match core::database_operations::get_database_privileges(&name, conn).await {
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
                row.select_priv,
                row.insert_priv,
                row.update_priv,
                row.delete_priv,
                row.create_priv,
                row.drop_priv,
                row.alter_priv,
                row.index_priv,
                row.create_tmp_table_priv,
                row.lock_tables_priv,
                row.references_priv
            ]);
        }
        table.printstd();
    }

    Ok(())
}

/// See documentation for `DatabaseCommand::EditPerm`.
fn parse_permission_table_cli_arg(arg: &str) -> anyhow::Result<DatabasePrivileges> {
    let parts: Vec<&str> = arg.split(':').collect();
    if parts.len() != 3 {
        anyhow::bail!("Invalid argument format. See `edit-perm --help` for more information.");
    }

    let db = parts[0].to_string();
    let user = parts[1].to_string();
    let privs = parts[2].to_string();

    let mut result = DatabasePrivileges {
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
            _ => anyhow::bail!("Invalid permission character: {}", char),
        }
    }

    Ok(result)
}

fn parse_permission(yn: &str) -> anyhow::Result<bool> {
    match yn.to_ascii_lowercase().as_str() {
        "y" => Ok(true),
        "n" => Ok(false),
        _ => Err(anyhow!("Expected Y or N, found {}", yn)),
    }
}

fn parse_permission_data_from_editor(content: String) -> anyhow::Result<Vec<DatabasePrivileges>> {
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

            Ok(DatabasePrivileges {
                db: (*line_parts.get(0).unwrap()).to_owned(),
                user: (*line_parts.get(1).unwrap()).to_owned(),
                select_priv: parse_permission(*line_parts.get(2).unwrap())
                    .context("Could not parse SELECT privilege")?,
                insert_priv: parse_permission(*line_parts.get(3).unwrap())
                    .context("Could not parse INSERT privilege")?,
                update_priv: parse_permission(*line_parts.get(4).unwrap())
                    .context("Could not parse UPDATE privilege")?,
                delete_priv: parse_permission(*line_parts.get(5).unwrap())
                    .context("Could not parse DELETE privilege")?,
                create_priv: parse_permission(*line_parts.get(6).unwrap())
                    .context("Could not parse CREATE privilege")?,
                drop_priv: parse_permission(*line_parts.get(7).unwrap())
                    .context("Could not parse DROP privilege")?,
                alter_priv: parse_permission(*line_parts.get(8).unwrap())
                    .context("Could not parse ALTER privilege")?,
                index_priv: parse_permission(*line_parts.get(9).unwrap())
                    .context("Could not parse INDEX privilege")?,
                create_tmp_table_priv: parse_permission(*line_parts.get(10).unwrap())
                    .context("Could not parse CREATE TEMPORARY TABLE privilege")?,
                lock_tables_priv: parse_permission(*line_parts.get(11).unwrap())
                    .context("Could not parse LOCK TABLES privilege")?,
                references_priv: parse_permission(*line_parts.get(12).unwrap())
                    .context("Could not parse REFERENCES privilege")?,
            })
        })
        .collect::<anyhow::Result<Vec<DatabasePrivileges>>>()
}

fn display_permissions_as_editor_line(privs: &DatabasePrivileges) -> String {
    vec![
        privs.db.as_str(),
        privs.user.as_str(),
        yn(privs.select_priv),
        yn(privs.insert_priv),
        yn(privs.update_priv),
        yn(privs.delete_priv),
        yn(privs.create_priv),
        yn(privs.drop_priv),
        yn(privs.alter_priv),
        yn(privs.index_priv),
        yn(privs.create_tmp_table_priv),
        yn(privs.lock_tables_priv),
        yn(privs.references_priv),
    ]
    .join("\t")
}

async fn edit_permissions(
    args: DatabaseEditPermArgs,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let permission_data = if let Some(name) = &args.name {
        core::database_operations::get_database_privileges(name, conn).await?
    } else {
        core::database_operations::get_all_database_privileges(conn).await?
    };

    let permissions_to_change = if !args.perm.is_empty() {
        if let Some(name) = args.name {
            args.perm
                .iter()
                .map(|perm| {
                    parse_permission_table_cli_arg(&format!("{}:{}", name, &perm))
                        .context(format!("Failed parsing database permissions: `{}`", &perm))
                })
                .collect::<anyhow::Result<Vec<DatabasePrivileges>>>()?
        } else {
            args.perm
                .iter()
                .map(|perm| {
                    parse_permission_table_cli_arg(perm)
                        .context(format!("Failed parsing database permissions: `{}`", &perm))
                })
                .collect::<anyhow::Result<Vec<DatabasePrivileges>>>()?
        }
    } else {
        let comment = indoc! {r#"
            # Welcome to the permission editor.
            # To add permissions
        "#};
        let header = DATABASE_PRIVILEGE_FIELDS
            .map(db_priv_field_human_readable_name)
            .join("\t");
        let example_line = {
            let unix_user = get_current_unix_user()?;
            let mut rng = thread_rng();
            let random_yes_nos = (0..(DATABASE_PRIVILEGE_FIELDS.len() - 2))
                .map(|_| ['Y', 'N'].choose(&mut rng).unwrap())
                .join("\t");
            format!(
                "# {}_db\t{}_user\t{}",
                unix_user.name, unix_user.name, random_yes_nos
            )
        };

        let result = edit::edit_with_builder(
            format!(
                "{}\n{}\n{}",
                comment,
                header,
                if permission_data.is_empty() {
                    example_line
                } else {
                    permission_data
                        .iter()
                        .map(display_permissions_as_editor_line)
                        .join("\n")
                }
            ),
            edit::Builder::new()
                .prefix("database-permissions")
                .suffix(".tsv")
                .rand_bytes(10),
        )?;

        parse_permission_data_from_editor(result)
            .context("Could not parse permission data from editor")?
    };

    let diffs = diff_permissions(permission_data, &permissions_to_change).await;

    if diffs.is_empty() {
        println!("No changes to make.");
        return Ok(());
    }

    // TODO: Add confirmation prompt.

    apply_permission_diffs(diffs, conn).await?;

    Ok(())
}
