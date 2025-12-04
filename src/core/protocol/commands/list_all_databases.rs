use serde::{Deserialize, Serialize};

use crate::server::sql::database_operations::DatabaseRow;

pub type ListAllDatabasesResponse = Result<Vec<DatabaseRow>, ListAllDatabasesError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllDatabasesError {
    MySqlError(String),
}

impl ListAllDatabasesError {
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllDatabasesError::MySqlError(err) => format!("MySQL error: {}", err),
        }
    }

    #[allow(dead_code)]
    pub fn error_type(&self) -> String {
        match self {
            ListAllDatabasesError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
