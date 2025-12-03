use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{
    protocol::request_validation::{NameValidationError, OwnerValidationError},
    types::DbOrUser,
};

pub type CheckAuthorizationRequest = Vec<DbOrUser>;

pub type CheckAuthorizationResponse = BTreeMap<DbOrUser, Result<(), CheckAuthorizationError>>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum CheckAuthorizationError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    // AuthorizationHandlerError(String),
}

pub fn print_check_authorization_output_status(output: &CheckAuthorizationResponse) {
    for (db_or_user, result) in output {
        match result {
            Ok(()) => {
                println!("'{}': OK", db_or_user.name());
            }
            Err(err) => {
                println!(
                    "'{}': {}",
                    db_or_user.name(),
                    err.to_error_message(db_or_user)
                );
            }
        }
    }
}

pub fn print_check_authorization_output_status_json(output: &CheckAuthorizationResponse) {
    let value = output
        .iter()
        .map(|(db_or_user, result)| match result {
            Ok(()) => (
                db_or_user.name().to_string(),
                json!({ "status": "success" }),
            ),
            Err(err) => (
                db_or_user.name().to_string(),
                json!({
                  "status": "error",
                  "type": err.error_type(),
                  "error": err.to_error_message(db_or_user),
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

impl CheckAuthorizationError {
    pub fn to_error_message(&self, db_or_user: &DbOrUser) -> String {
        match self {
            CheckAuthorizationError::SanitizationError(err) => {
                err.to_error_message(db_or_user.clone())
            }
            CheckAuthorizationError::OwnershipError(err) => {
                err.to_error_message(db_or_user.clone())
            } // CheckAuthorizationError::AuthorizationHandlerError(msg) => {
              //     format!(
              //         "Authorization handler error for '{}': {}",
              //         db_or_user.name(),
              //         msg
              //     )
              // }
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            CheckAuthorizationError::SanitizationError(_) => "sanitization-error",
            CheckAuthorizationError::OwnershipError(_) => "ownership-error",
            // CheckAuthorizationError::AuthorizationHandlerError(_) => "authorization-handler-error",
        }
    }
}
