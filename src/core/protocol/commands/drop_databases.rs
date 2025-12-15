use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::core::{
    protocol::request_validation::AuthorizationError,
    types::{DbOrUser, MySQLDatabase},
};

pub type DropDatabasesRequest = Vec<MySQLDatabase>;

pub type DropDatabasesResponse = BTreeMap<MySQLDatabase, Result<(), DropDatabaseError>>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DropDatabaseError {
    #[error("Authorization error: {0}")]
    AuthorizationError(#[from] AuthorizationError),

    #[error("Database does not exist")]
    DatabaseDoesNotExist,

    #[error("MySQL error: {0}")]
    MySqlError(String),
}

pub fn print_drop_databases_output_status(output: &DropDatabasesResponse) {
    for (database_name, result) in output {
        match result {
            Ok(()) => {
                println!(
                    "Database '{}' dropped successfully.",
                    database_name.as_str()
                );
            }
            Err(err) => {
                println!("{}", err.to_error_message(database_name));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_drop_databases_output_status_json(output: &DropDatabasesResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(()) => (name.to_string(), json!({ "status": "success" })),
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

impl DropDatabaseError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            DropDatabaseError::AuthorizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            DropDatabaseError::DatabaseDoesNotExist => {
                format!("Database {} does not exist.", database_name)
            }
            DropDatabaseError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> String {
        match self {
            DropDatabaseError::AuthorizationError(err) => err.error_type(),
            DropDatabaseError::DatabaseDoesNotExist => "database-does-not-exist".to_string(),
            DropDatabaseError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
