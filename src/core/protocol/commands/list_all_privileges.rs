use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::core::database_privileges::DatabasePrivilegeRow;

pub type ListAllPrivilegesResponse = Result<Vec<DatabasePrivilegeRow>, ListAllPrivilegesError>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllPrivilegesError {
    #[error("MySQL error: {0}")]
    MySqlError(String),
}

impl ListAllPrivilegesError {
    #[must_use]
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllPrivilegesError::MySqlError(err) => format!("MySQL error: {err}"),
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn error_type(&self) -> String {
        match self {
            ListAllPrivilegesError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
