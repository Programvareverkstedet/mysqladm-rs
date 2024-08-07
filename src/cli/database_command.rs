use anyhow::{anyhow, Context};
use clap::Parser;
use indoc::indoc;
use itertools::Itertools;
use prettytable::{Cell, Row, Table};
use sqlx::MySqlConnection;

use crate::core::{
    self,
    common::{close_database_connection, get_current_unix_user},
    database_operations::{
        apply_permission_diffs, db_priv_field_human_readable_name, diff_permissions, yn,
        DatabasePrivileges, DATABASE_PRIVILEGE_FIELDS,
    },
    user_operations::user_exists,
};

// TODO: Support batch creation/dropping,showing of databases,
//       using a comma-separated list of database names.

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

    /// List user permissions for one or more databases
    ///
    /// If no database names are provided, it will show permissions for all databases you have access to.
    #[command()]
    ShowDbPerm(DatabaseShowPermArgs),

    /// Change user permissions for one or more databases. See `edit-db-perm --help` for details.
    ///
    /// This command has two modes of operation:
    ///
    /// 1. Interactive mode: If nothing else is specified, the user will be prompted to edit the permissions using a text editor.
    ///
    ///    You can configure your preferred text editor by setting the `VISUAL` or `EDITOR` environment variables.
    ///
    ///    Follow the instructions inside the editor for more information.
    ///
    /// 2. Non-interactive mode: If the `-p` flag is specified, the user can write permissions using arguments.
    ///
    ///    The permission arguments should be formatted as `<db>:<user>:<privileges>`
    ///    where the privileges are a string of characters, each representing a single permissions.
    ///    The character `A` is an exception, because it represents all permissions.
    ///
    ///    The character to permission mapping is declared as follows:
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
    ///   If you provide a database name, you can omit it from the permission arguments.
    ///
    ///   Example usage of non-interactive mode:
    ///
    ///     Set permissions `SELECT`, `INSERT`, and `UPDATE` for user `my_user` on database `my_db`:
    ///
    ///       mysqladm edit-db-perm -p my_db:my_user:siu
    ///
    ///     Set all permissions for user `my_other_user` on database `my_other_db`:
    ///
    ///       mysqladm edit-db-perm -p my_other_db:my_other_user:A
    ///
    ///     Set miscellaneous permissions for multiple users on database `my_db`:
    ///
    ///       mysqladm edit-db-perm my_db -p my_user:siu my_other_user:ct
    ///
    #[command(verbatim_doc_comment)]
    EditDbPerm(DatabaseEditPermArgs),
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
pub struct DatabaseShowPermArgs {
    /// The name of the database(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    json: bool,
}

#[derive(Parser)]
pub struct DatabaseEditPermArgs {
    /// The name of the database to edit permissions for.
    pub name: Option<String>,

    #[arg(short, long, value_name = "[DATABASE:]USER:PERMISSIONS", num_args = 0..)]
    pub perm: Vec<String>,

    /// Whether to output the information in JSON format.
    #[arg(short, long)]
    pub json: bool,

    /// Specify the text editor to use for editing permissions.
    #[arg(short, long)]
    pub editor: Option<String>,

    /// Disable interactive confirmation before saving changes.
    #[arg(short, long)]
    pub yes: bool,
}

pub async fn handle_command(
    command: DatabaseCommand,
    mut conn: MySqlConnection,
) -> anyhow::Result<()> {
    let result = match command {
        DatabaseCommand::CreateDb(args) => create_databases(args, &mut conn).await,
        DatabaseCommand::DropDb(args) => drop_databases(args, &mut conn).await,
        DatabaseCommand::ListDb(args) => list_databases(args, &mut conn).await,
        DatabaseCommand::ShowDbPerm(args) => show_databases(args, &mut conn).await,
        DatabaseCommand::EditDbPerm(args) => edit_permissions(args, &mut conn).await,
    };

    close_database_connection(conn).await;

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
                db: (*line_parts.first().unwrap()).to_owned(),
                user: (*line_parts.get(1).unwrap()).to_owned(),
                select_priv: parse_permission(line_parts.get(2).unwrap())
                    .context("Could not parse SELECT privilege")?,
                insert_priv: parse_permission(line_parts.get(3).unwrap())
                    .context("Could not parse INSERT privilege")?,
                update_priv: parse_permission(line_parts.get(4).unwrap())
                    .context("Could not parse UPDATE privilege")?,
                delete_priv: parse_permission(line_parts.get(5).unwrap())
                    .context("Could not parse DELETE privilege")?,
                create_priv: parse_permission(line_parts.get(6).unwrap())
                    .context("Could not parse CREATE privilege")?,
                drop_priv: parse_permission(line_parts.get(7).unwrap())
                    .context("Could not parse DROP privilege")?,
                alter_priv: parse_permission(line_parts.get(8).unwrap())
                    .context("Could not parse ALTER privilege")?,
                index_priv: parse_permission(line_parts.get(9).unwrap())
                    .context("Could not parse INDEX privilege")?,
                create_tmp_table_priv: parse_permission(line_parts.get(10).unwrap())
                    .context("Could not parse CREATE TEMPORARY TABLE privilege")?,
                lock_tables_priv: parse_permission(line_parts.get(11).unwrap())
                    .context("Could not parse LOCK TABLES privilege")?,
                references_priv: parse_permission(line_parts.get(12).unwrap())
                    .context("Could not parse REFERENCES privilege")?,
            })
        })
        .collect::<anyhow::Result<Vec<DatabasePrivileges>>>()
}

fn format_privileges_line(
    privs: &DatabasePrivileges,
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

pub async fn edit_permissions(
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
            # Each line defines what permissions a single user has on a single database.
            # The first two columns respectively represent the database name and the user, and the remaining columns are the permissions.
            # If the user should have a permission, write 'Y', otherwise write 'N'.
            #
            # Lines starting with '#' are comments and will be ignored.
        "#};

        let unix_user = get_current_unix_user()?;
        let example_user = format!("{}_user", unix_user.name);
        let example_db = format!("{}_db", unix_user.name);

        let longest_username = permission_data
            .iter()
            .map(|p| p.user.len())
            .max()
            .unwrap_or(example_user.len());

        let longest_database_name = permission_data
            .iter()
            .map(|p| p.db.len())
            .max()
            .unwrap_or(example_db.len());

        let mut header: Vec<_> = DATABASE_PRIVILEGE_FIELDS
            .into_iter()
            .map(db_priv_field_human_readable_name)
            .collect();

        // Pad the first two columns with spaces to align the permissions.
        header[0] = format!("{:width$}", header[0], width = longest_database_name);
        header[1] = format!("{:width$}", header[1], width = longest_username);

        let example_line = format_privileges_line(
            &DatabasePrivileges {
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

        let result = edit::edit_with_builder(
            format!(
                "{}\n{}\n{}",
                comment,
                header.join(" "),
                if permission_data.is_empty() {
                    format!("# {}", example_line)
                } else {
                    permission_data
                        .iter()
                        .map(|perm| {
                            format_privileges_line(perm, longest_username, longest_database_name)
                        })
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

    for row in permissions_to_change.iter() {
        if !user_exists(&row.user, conn).await? {
            // TODO: allow user to return and correct their mistake
            anyhow::bail!("User {} does not exist", row.user);
        }
    }

    let diffs = diff_permissions(permission_data, &permissions_to_change).await;

    if diffs.is_empty() {
        println!("No changes to make.");
        return Ok(());
    }

    // TODO: Add confirmation prompt.

    apply_permission_diffs(diffs, conn).await?;

    Ok(())
}
