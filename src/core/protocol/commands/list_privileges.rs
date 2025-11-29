// TODO: merge all rows into a single collection.
//       they already contain which database they belong to.
//       no need to index by database name.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::core::{
    database_privileges::DatabasePrivilegeRow,
    protocol::request_validation::{NameValidationError, OwnerValidationError},
    types::{DbOrUser, MySQLDatabase},
};

pub type ListPrivilegesRequest = Option<Vec<MySQLDatabase>>;

pub type ListPrivilegesResponse =
    BTreeMap<MySQLDatabase, Result<Vec<DatabasePrivilegeRow>, GetDatabasesPrivilegeDataError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetDatabasesPrivilegeDataError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    MySqlError(String),
}

impl GetDatabasesPrivilegeDataError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            GetDatabasesPrivilegeDataError::SanitizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            GetDatabasesPrivilegeDataError::OwnershipError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            GetDatabasesPrivilegeDataError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            GetDatabasesPrivilegeDataError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}
