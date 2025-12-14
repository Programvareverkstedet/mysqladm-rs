use std::collections::{BTreeMap, BTreeSet};

use anyhow::Context;
use clap::Parser;
use clap_complete::ArgValueCompleter;
use dialoguer::{Confirm, Editor};
use futures_util::SinkExt;
use nix::unistd::{User, getuid};
use tokio_stream::StreamExt;

use crate::{
    client::commands::erroneous_server_response,
    core::{
        completion::mysql_database_completer,
        database_privileges::{
            DatabasePrivilegeEditEntry, DatabasePrivilegeRow, DatabasePrivilegeRowDiff,
            DatabasePrivilegesDiff, create_or_modify_privilege_rows, diff_privileges,
            display_privilege_diffs, generate_editor_content_from_privilege_data,
            parse_privilege_data_from_editor_content, reduce_privilege_diffs,
        },
        protocol::{
            ClientToServerMessageStream, Request, Response,
            print_modify_database_privileges_output_status,
        },
        types::{MySQLDatabase, MySQLUser},
    },
};

#[derive(Parser, Debug, Clone)]
pub struct EditPrivsArgs {
    /// The MySQL database to edit privileges for
    #[cfg_attr(not(feature = "suid-sgid-mode"), arg(add = ArgValueCompleter::new(mysql_database_completer)))]
    #[arg(value_name = "DB_NAME")]
    pub name: Option<MySQLDatabase>,

    #[arg(
      short,
      long,
      value_name = "[DATABASE:]USER:[+-]PRIVILEGES",
      num_args = 0..,
      value_parser = DatabasePrivilegeEditEntry::parse_from_str,
    )]
    pub privs: Vec<DatabasePrivilegeEditEntry>,

    /// Print the information as JSON
    #[arg(short, long)]
    pub json: bool,

    /// Specify the text editor to use for editing privileges
    #[arg(
      short,
      long,
      value_name = "COMMAND",
      value_hint = clap::ValueHint::CommandString,
    )]
    pub editor: Option<String>,

    /// Disable interactive confirmation before saving changes
    #[arg(short, long)]
    pub yes: bool,
}

async fn users_exist(
    server_connection: &mut ClientToServerMessageStream,
    privilege_diff: &BTreeSet<DatabasePrivilegesDiff>,
) -> anyhow::Result<BTreeMap<MySQLUser, bool>> {
    let user_list = privilege_diff
        .iter()
        .map(|diff| diff.get_user_name().clone())
        .collect();

    let message = Request::ListUsers(Some(user_list));
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::ListUsers(user_map))) => user_map,
        response => {
            erroneous_server_response(response)?;
            // Unreachable, but needed to satisfy the type checker
            BTreeMap::new()
        }
    };

    let result = result
        .into_iter()
        .map(|(user, user_result)| (user, user_result.is_ok()))
        .collect();

    Ok(result)
}

async fn databases_exist(
    server_connection: &mut ClientToServerMessageStream,
    privilege_diff: &BTreeSet<DatabasePrivilegesDiff>,
) -> anyhow::Result<BTreeMap<MySQLDatabase, bool>> {
    let database_list = privilege_diff
        .iter()
        .map(|diff| diff.get_database_name().clone())
        .collect();

    let message = Request::ListDatabases(Some(database_list));
    server_connection.send(message).await?;

    let result = match server_connection.next().await {
        Some(Ok(Response::ListDatabases(database_map))) => database_map,
        response => {
            erroneous_server_response(response)?;
            // Unreachable, but needed to satisfy the type checker
            BTreeMap::new()
        }
    };

    let result = result
        .into_iter()
        .map(|(database, db_result)| (database, db_result.is_ok()))
        .collect();

    Ok(result)
}

pub async fn edit_database_privileges(
    args: EditPrivsArgs,
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

    let user_existence_map = users_exist(&mut server_connection, &diffs).await?;
    let database_existence_map = databases_exist(&mut server_connection, &diffs).await?;

    let diffs = reduce_privilege_diffs(&existing_privilege_rows, diffs)?
        .into_iter()
        .filter(|diff| {
            let database_name = diff.get_database_name();
            let username = diff.get_user_name();

            if let Some(false) = database_existence_map.get(database_name) {
                println!("Database '{}' does not exist.", database_name);
                println!("Skipping...");
                return false;
            }

            if let Some(false) = user_existence_map.get(username) {
                println!("User '{}' does not exist.", username);
                println!("Skipping...");
                return false;
            }

            true
        })
        .collect::<BTreeSet<_>>();

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
    args: &EditPrivsArgs,
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
