use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::server::sql::user_operations::DatabaseUser;

pub type ListAllUsersResponse = Result<Vec<DatabaseUser>, ListAllUsersError>;

#[derive(Error, Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ListAllUsersError {
    #[error("MySQL error: {0}")]
    MySqlError(String),
}

impl ListAllUsersError {
    #[must_use]
    pub fn to_error_message(&self) -> String {
        match self {
            ListAllUsersError::MySqlError(err) => format!("MySQL error: {err}"),
        }
    }

    #[allow(dead_code)]
    #[must_use]
    pub fn error_type(&self) -> String {
        match self {
            ListAllUsersError::MySqlError(_) => "mysql-error".to_string(),
        }
    }
}
