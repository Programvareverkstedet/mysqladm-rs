use anyhow::Context;
use clap::Parser;
use prettytable::{Cell, Row, Table};
use sqlx::{Connection, MySqlConnection};

use crate::core::{self, database_operations::DatabasePrivileges};

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
    /// TODO: fix this help message.
    ///
    /// This command has two modes of operation:
    /// 1. Interactive mode: If the `-t` flag is used, the user will be prompted to edit the permissions using a text editor.
    /// 2. Non-interactive mode: If the `-t` flag is not used, the user can specify the permissions to change using the `-p` flag.
    ///
    /// In non-interactive mode, the `-p` flag should be followed by strings, each representing a single permission change.
    ///
    /// The permission arguments should be a string, formatted as `db:user:privileges`
    /// where privs are a string of characters, each representing a single permissions,
    /// with the exception of `A` which represents all permissions.
    ///
    /// The permission to character mapping is as follows:
    ///
    /// - `s` - SELECT
    /// - `i` - INSERT
    /// - `u` - UPDATE
    /// - `d` - DELETE
    /// - `c` - CREATE
    /// - `D` - DROP
    /// - `a` - ALTER
    /// - `I` - INDEX
    /// - `t` - CREATE TEMPORARY TABLES
    /// - `l` - LOCK TABLES
    /// - `r` - REFERENCES
    /// - `A` - ALL PRIVILEGES
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

    /// Whether to edit the permissions using a text editor.
    #[arg(short, long)]
    text: bool,

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
            core::database_operations::HUMAN_READABLE_DATABASE_PRIVILEGE_NAMES
                .iter()
                .map(|(name, _)| Cell::new(name))
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
        select_priv: "N".to_string(),
        insert_priv: "N".to_string(),
        update_priv: "N".to_string(),
        delete_priv: "N".to_string(),
        create_priv: "N".to_string(),
        drop_priv: "N".to_string(),
        alter_priv: "N".to_string(),
        index_priv: "N".to_string(),
        create_tmp_table_priv: "N".to_string(),
        lock_tables_priv: "N".to_string(),
        references_priv: "N".to_string(),
    };

    for char in privs.chars() {
        match char {
            's' => result.select_priv = "Y".to_string(),
            'i' => result.insert_priv = "Y".to_string(),
            'u' => result.update_priv = "Y".to_string(),
            'd' => result.delete_priv = "Y".to_string(),
            'c' => result.create_priv = "Y".to_string(),
            'D' => result.drop_priv = "Y".to_string(),
            'a' => result.alter_priv = "Y".to_string(),
            'I' => result.index_priv = "Y".to_string(),
            't' => result.create_tmp_table_priv = "Y".to_string(),
            'l' => result.lock_tables_priv = "Y".to_string(),
            'r' => result.references_priv = "Y".to_string(),
            'A' => {
                result.select_priv = "Y".to_string();
                result.insert_priv = "Y".to_string();
                result.update_priv = "Y".to_string();
                result.delete_priv = "Y".to_string();
                result.create_priv = "Y".to_string();
                result.drop_priv = "Y".to_string();
                result.alter_priv = "Y".to_string();
                result.index_priv = "Y".to_string();
                result.create_tmp_table_priv = "Y".to_string();
                result.lock_tables_priv = "Y".to_string();
                result.references_priv = "Y".to_string();
            }
            _ => anyhow::bail!("Invalid permission character: {}", char),
        }
    }

    Ok(result)
}

async fn edit_permissions(
    args: DatabaseEditPermArgs,
    conn: &mut MySqlConnection,
) -> anyhow::Result<()> {
    let _data = if let Some(name) = &args.name {
        core::database_operations::get_database_privileges(name, conn).await?
    } else {
        core::database_operations::get_all_database_privileges(conn).await?
    };

    if !args.text {
        let permissions_to_change: Vec<DatabasePrivileges> = if let Some(name) = args.name {
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
        };

        println!("{:#?}", permissions_to_change);
    } else {
        // TODO: debug assert that -p is not used with -t
    }

    // TODO: find the difference between the two vectors, and ask for confirmation before applying the changes.

    // TODO: apply the changes to the database.
    unimplemented!();
}
