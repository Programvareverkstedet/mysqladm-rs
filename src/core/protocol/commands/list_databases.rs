use std::collections::BTreeMap;

use prettytable::{Cell, Row, Table};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::{
    core::{
        protocol::request_validation::{NameValidationError, OwnerValidationError},
        types::{DbOrUser, MySQLDatabase},
    },
    server::sql::database_operations::DatabaseRow,
};

pub type ListDatabasesRequest = Option<Vec<MySQLDatabase>>;

pub type ListDatabasesResponse = BTreeMap<MySQLDatabase, Result<DatabaseRow, ListDatabasesError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListDatabasesError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
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
        table.add_row(Row::new(vec![Cell::new("Database")]));
        for db in final_database_list {
            table.add_row(row![db.database]);
        }
        table.printstd();
    }
}

pub fn print_list_databases_output_status_json(output: &ListDatabasesResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(_row) => (
                name.to_string(),
                json!({
                  "status": "success",
                  // NOTE: there will likely be more data to include here in the future
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
            ListDatabasesError::SanitizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            ListDatabasesError::OwnershipError(err) => {
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

    pub fn error_type(&self) -> &'static str {
        match self {
            ListDatabasesError::SanitizationError(_) => "sanitization-error",
            ListDatabasesError::OwnershipError(_) => "ownership-error",
            ListDatabasesError::DatabaseDoesNotExist => "database-does-not-exist",
            ListDatabasesError::MySqlError(_) => "mysql-error",
        }
    }
}
