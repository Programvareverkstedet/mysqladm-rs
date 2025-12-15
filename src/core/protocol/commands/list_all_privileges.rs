use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::database_privileges::DatabasePrivilegeRow;

pub type ListAllPrivilegesResponse =
    Result<Vec<DatabasePrivilegeRow>, GetAllDatabasesPrivilegeDataError>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetAllDatabasesPrivilegeDataError {
    #[error("MySQL error: {0}")]
    MySqlError(String),
}

impl GetAllDatabasesPrivilegeDataError {
    pub fn to_error_message(&self) -> String {
        match self {
            GetAllDatabasesPrivilegeDataError::MySqlError(err) => format!("MySQL error: {}", err),
        }
    }

    #[allow(dead_code)]
    pub fn error_type(&self) -> String {
        match self {
            GetAllDatabasesPrivilegeDataError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
