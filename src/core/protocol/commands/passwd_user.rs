use serde::{Deserialize, Serialize};

use crate::core::{
    protocol::request_validation::{DbOrUser, NameValidationError, OwnerValidationError},
    types::MySQLUser,
};

pub type SetUserPasswordRequest = (MySQLUser, String);

pub type SetUserPasswordResponse = Result<(), SetPasswordError>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SetPasswordError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    MySqlError(String),
}

pub fn print_set_password_output_status(output: &SetUserPasswordResponse, username: &MySQLUser) {
    match output {
        Ok(()) => {
            println!("Password for user '{}' set successfully.", username);
        }
        Err(err) => {
            println!("{}", err.to_error_message(username));
            println!("Skipping...");
        }
    }
}

impl SetPasswordError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            SetPasswordError::SanitizationError(err) => {
                err.to_error_message(username, DbOrUser::User)
            }
            SetPasswordError::OwnershipError(err) => err.to_error_message(username, DbOrUser::User),
            SetPasswordError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            SetPasswordError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }
}
