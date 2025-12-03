use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{
    protocol::request_validation::{NameValidationError, OwnerValidationError},
    types::{DbOrUser, MySQLUser},
};

pub type UnlockUsersRequest = Vec<MySQLUser>;

pub type UnlockUsersResponse = BTreeMap<MySQLUser, Result<(), UnlockUserError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum UnlockUserError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    UserDoesNotExist,
    UserIsAlreadyUnlocked,
    MySqlError(String),
}

pub fn print_unlock_users_output_status(output: &UnlockUsersResponse) {
    for (username, result) in output {
        match result {
            Ok(()) => {
                println!("User '{}' unlocked successfully.", username);
            }
            Err(err) => {
                println!("{}", err.to_error_message(username));
                println!("Skipping...");
            }
        }
        println!();
    }
}

pub fn print_unlock_users_output_status_json(output: &UnlockUsersResponse) {
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

impl UnlockUserError {
    pub fn to_error_message(&self, username: &MySQLUser) -> String {
        match self {
            UnlockUserError::SanitizationError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            UnlockUserError::OwnershipError(err) => {
                err.to_error_message(DbOrUser::User(username.clone()))
            }
            UnlockUserError::UserDoesNotExist => {
                format!("User '{}' does not exist.", username)
            }
            UnlockUserError::UserIsAlreadyUnlocked => {
                format!("User '{}' is already unlocked.", username)
            }
            UnlockUserError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            UnlockUserError::SanitizationError(_) => "sanitization-error",
            UnlockUserError::OwnershipError(_) => "ownership-error",
            UnlockUserError::UserDoesNotExist => "user-does-not-exist",
            UnlockUserError::UserIsAlreadyUnlocked => "user-is-already-unlocked",
            UnlockUserError::MySqlError(_) => "mysql-error",
        }
    }
}
