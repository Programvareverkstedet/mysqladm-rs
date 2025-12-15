use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;
use thiserror::Error;

use crate::core::{
    protocol::request_validation::ValidationError,
    types::{DbOrUser, MySQLDatabase},
};

pub type CreateDatabasesRequest = Vec<MySQLDatabase>;

pub type CreateDatabasesResponse = BTreeMap<MySQLDatabase, Result<(), CreateDatabaseError>>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CreateDatabaseError {
    #[error("Validation error: {0}")]
    ValidationError(#[from] ValidationError),

    #[error("Database already exists")]
    DatabaseAlreadyExists,

    #[error("MySQL error: {0}")]
    MySqlError(String),
}

pub fn print_create_databases_output_status(output: &CreateDatabasesResponse) {
    for (database_name, result) in output {
        match result {
            Ok(()) => {
                println!("Database '{}' created successfully.", database_name);
            }
            Err(err) => {
                println!("{}", err.to_error_message(database_name));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_create_databases_output_status_json(output: &CreateDatabasesResponse) {
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

impl CreateDatabaseError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            CreateDatabaseError::ValidationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            CreateDatabaseError::DatabaseAlreadyExists => {
                format!("Database {} already exists.", database_name)
            }
            CreateDatabaseError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> String {
        match self {
            CreateDatabaseError::ValidationError(err) => err.error_type(),
            CreateDatabaseError::DatabaseAlreadyExists => "database-already-exists".to_string(),
            CreateDatabaseError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
