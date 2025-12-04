use serde::{Deserialize, Serialize};

use crate::core::database_privileges::DatabasePrivilegeRow;

pub type ListAllPrivilegesResponse =
    Result<Vec<DatabasePrivilegeRow>, GetAllDatabasesPrivilegeDataError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetAllDatabasesPrivilegeDataError {
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
