use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::server::sql::database_operations::DatabaseRow;

pub type ListAllDatabasesResponse = Result<Vec<DatabaseRow>, ListAllDatabasesError>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllDatabasesError {
    #[error("MySQL error: {0}")]
    MySqlError(String),
}

impl ListAllDatabasesError {
    #[must_use]
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllDatabasesError::MySqlError(err) => format!("MySQL error: {err}"),
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn error_type(&self) -> String {
        match self {
            ListAllDatabasesError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
