use clap::Parser;
use sqlx::MySqlConnection;

use crate::{
    cli::{database_command, mysql_admutils_compatibility::common::filter_db_or_user_names},
    core::{
        common::{yn, DbOrUser},
        config::{create_mysql_connection_from_config, get_config, GlobalConfigArgs},
        database_operations::{create_database, drop_database, get_database_list},
        database_privilege_operations,
    },
};

const HELP_DB_PERM: &str = r#"
Edit permissions for the DATABASE(s). Running this command will
spawn the editor stored in the $EDITOR environment variable.
(pico will be used if the variable is unset)

The file should contain one line per user, starting with the
username and followed by ten Y/N-values seperated by whitespace.
Lines starting with # are ignored.

The Y/N-values corresponds to the following mysql privileges:
  Select     - Enables use of SELECT
  Insert     - Enables use of INSERT
  Update     - Enables use of UPDATE
  Delete     - Enables use of DELETE
  Create     - Enables use of CREATE TABLE
  Drop       - Enables use of DROP TABLE
  Alter      - Enables use of ALTER TABLE
  Index      - Enables use of CREATE INDEX and DROP INDEX
  Temp       - Enables use of CREATE TEMPORARY TABLE
  Lock       - Enables use of LOCK TABLE
  References - Enables use of REFERENCES
"#;

#[derive(Parser)]
pub struct Args {
    #[command(subcommand)]
    pub command: Option<Command>,

    #[command(flatten)]
    config_overrides: GlobalConfigArgs,

    /// Print help for the 'editperm' subcommand.
    #[arg(long, global = true)]
    pub help_editperm: bool,
}

// NOTE: mysql-dbadm explicitly calls privileges "permissions".
//       This is something we're trying to move away from.
//       See https://git.pvv.ntnu.no/Projects/mysqladm-rs/issues/29

/// Create, drop or edit permissions for the DATABASE(s),
/// as determined by the COMMAND.
///
/// This is a compatibility layer for the mysql-dbadm command.
/// Please consider using the newer mysqladm command instead.
#[derive(Parser)]
#[command(version, about, disable_help_subcommand = true, verbatim_doc_comment)]
pub enum Command {
    /// create the DATABASE(s).
    Create(CreateArgs),

    /// delete the DATABASE(s).
    Drop(DatabaseDropArgs),

    /// give information about the DATABASE(s), or, if
    /// none are given, all the ones you own.
    Show(DatabaseShowArgs),

    // TODO: make this output more verbatim_doc_comment-like,
    //       without messing up the indentation.
    /// change permissions for the DATABASE(s). Your
    /// favorite editor will be started, allowing you
    /// to make changes to the permission table.
    /// Run 'mysql-dbadm --help-editperm' for more
    /// information.
    EditPerm(EditPermArgs),
}

#[derive(Parser)]
pub struct CreateArgs {
    /// The name of the DATABASE(s) to create.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct DatabaseDropArgs {
    /// The name of the DATABASE(s) to drop.
    #[arg(num_args = 1..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct DatabaseShowArgs {
    /// The name of the DATABASE(s) to show.
    #[arg(num_args = 0..)]
    name: Vec<String>,
}

#[derive(Parser)]
pub struct EditPermArgs {
    /// The name of the DATABASE to edit permissions for.
    pub database: String,
}

pub async fn main() -> anyhow::Result<()> {
    let args: Args = Args::parse();

    if args.help_editperm {
        println!("{}", HELP_DB_PERM);
        return Ok(());
    }

    let command = match args.command {
        Some(command) => command,
        None => {
            println!(
                "Try `{} --help' for more information.",
                std::env::args().next().unwrap_or("mysql-dbadm".to_string())
            );
            return Ok(());
        }
    };

    let config = get_config(args.config_overrides)?;
    let mut connection = create_mysql_connection_from_config(config.mysql).await?;

    match command {
        Command::Create(args) => {
            let filtered_names = filter_db_or_user_names(args.name, DbOrUser::Database)?;
            for name in filtered_names {
                create_database(&name, &mut connection).await?;
                println!("Database {} created.", name);
            }
        }
        Command::Drop(args) => {
            let filtered_names = filter_db_or_user_names(args.name, DbOrUser::Database)?;
            for name in filtered_names {
                drop_database(&name, &mut connection).await?;
                println!("Database {} dropped.", name);
            }
        }
        Command::Show(args) => {
            let names = if args.name.is_empty() {
                get_database_list(&mut connection).await?
            } else {
                filter_db_or_user_names(args.name, DbOrUser::Database)?
            };

            for name in names {
                show_db(&name, &mut connection).await?;
            }
        }
        Command::EditPerm(args) => {
            // TODO: This does not accurately replicate the behavior of the old implementation.
            //       Hopefully, not many people rely on this in an automated fashion, as it
            //       is made to be interactive in nature. However, we should still try to
            //        replicate the old behavior as closely as possible.
            let edit_privileges_args = database_command::DatabaseEditPrivsArgs {
                name: Some(args.database),
                privs: vec![],
                json: false,
                editor: None,
                yes: false,
            };

            database_command::edit_privileges(edit_privileges_args, &mut connection).await?;
        }
    }

    Ok(())
}

async fn show_db(name: &str, connection: &mut MySqlConnection) -> anyhow::Result<()> {
    // NOTE: mysql-dbadm show has a quirk where valid database names
    //       for non-existent databases will report with no users.
    //       This function should *not* check for db existence, only
    //       validate the names.
    let privileges = database_privilege_operations::get_database_privileges(name, connection)
        .await
        .unwrap_or(vec![]);

    println!(
        concat!(
            "Database '{}':\n",
            "# User                Select  Insert  Update  Delete  Create   Drop   Alter   Index    Temp    Lock  References\n",
            "# ----------------    ------  ------  ------  ------  ------   ----   -----   -----    ----    ----  ----------"
        ),
        name,
    );
    if privileges.is_empty() {
        println!("# (no permissions currently granted to any users)");
    } else {
        for privilege in privileges {
            println!(
                "  {:<16}      {:<7} {:<7} {:<7} {:<7} {:<7} {:<7} {:<7} {:<7} {:<7} {:<7} {}",
                privilege.user,
                yn(privilege.select_priv),
                yn(privilege.insert_priv),
                yn(privilege.update_priv),
                yn(privilege.delete_priv),
                yn(privilege.create_priv),
                yn(privilege.drop_priv),
                yn(privilege.alter_priv),
                yn(privilege.index_priv),
                yn(privilege.create_tmp_table_priv),
                yn(privilege.lock_tables_priv),
                yn(privilege.references_priv)
            );
        }
    }

    Ok(())
}
