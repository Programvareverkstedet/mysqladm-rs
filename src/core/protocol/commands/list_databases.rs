use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{
    core::{
        protocol::request_validation::{DbOrUser, NameValidationError, OwnerValidationError},
        types::MySQLDatabase,
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

impl ListDatabasesError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            ListDatabasesError::SanitizationError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ListDatabasesError::OwnershipError(err) => {
                err.to_error_message(database_name, DbOrUser::Database)
            }
            ListDatabasesError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            ListDatabasesError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}
