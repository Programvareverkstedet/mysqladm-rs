use std::collections::BTreeMap;

use itertools::Itertools;
use prettytable::Table;
use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::{
    core::{
        protocol::request_validation::AuthorizationError,
        types::{DbOrUser, MySQLDatabase},
    },
    server::sql::database_operations::DatabaseRow,
};

pub type ListDatabasesRequest = Option<Vec<MySQLDatabase>>;

pub type ListDatabasesResponse = BTreeMap<MySQLDatabase, Result<DatabaseRow, ListDatabasesError>>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListDatabasesError {
    #[error("Authorization error: {0}")]
    AuthorizationError(#[from] AuthorizationError),

    #[error("Database does not exist")]
    DatabaseDoesNotExist,

    #[error("MySQL error: {0}")]
    MySqlError(String),
}

pub fn print_list_databases_output_status(output: &ListDatabasesResponse) {
    let mut final_database_list: Vec<&DatabaseRow> = Vec::new();
    for (db_name, db_result) in output {
        match db_result {
            Ok(db_row) => final_database_list.push(db_row),
            Err(err) => {
                eprintln!("{}", err.to_error_message(db_name));
                eprintln!("Skipping...");
            }
        }
    }

    if final_database_list.is_empty() {
        println!("No databases to show.");
    } else {
        let mut table = Table::new();
        table.add_row(row![
            "Database",
            "Tables",
            "Users",
            "Collation",
            "Character Set",
            "Size (Bytes)"
        ]);
        for db in final_database_list {
            table.add_row(row![
                db.database,
                db.tables.join("\n"),
                db.users.iter().map(|user| user.as_str()).join("\n"),
                db.collation.as_deref().unwrap_or("N/A"),
                db.character_set.as_deref().unwrap_or("N/A"),
                db.size_bytes,
            ]);
        }

        table.printstd();
    }
}

pub fn print_list_databases_output_status_json(output: &ListDatabasesResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(row) => (
                name.to_string(),
                json!({
                  "status": "success",
                  "tables": row.tables,
                  "users": row.users,
                  "collation": row.collation,
                  "character_set": row.character_set,
                  "size_bytes": row.size_bytes,
                }),
            ),
            Err(err) => (
                name.to_string(),
                json!({
                  "status": "error",
                  "type": err.error_type(),
                  "error": err.to_error_message(name),
                }),
            ),
        })
        .collect::<serde_json::Map<_, _>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&value)
            .unwrap_or("Failed to serialize result to JSON".to_string())
    );
}

impl ListDatabasesError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            ListDatabasesError::AuthorizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            ListDatabasesError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            ListDatabasesError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> String {
        match self {
            ListDatabasesError::AuthorizationError(err) => err.error_type(),
            ListDatabasesError::DatabaseDoesNotExist => "database-does-not-exist".to_string(),
            ListDatabasesError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
