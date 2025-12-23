use crate::core::{
    protocol::{
        CreateDatabaseError, CreateUserError, DropDatabaseError, DropUserError,
        ListPrivilegesError, ListUsersError, request_validation::ValidationError,
    },
    types::DbOrUser,
};

pub fn name_validation_error_to_error_message(db_or_user: &DbOrUser) -> String {
    let argv0 = std::env::args().next().unwrap_or_else(|| match db_or_user {
        DbOrUser::Database(_) => "mysql-dbadm".to_string(),
        DbOrUser::User(_) => "mysql-useradm".to_string(),
    });

    format!(
        concat!(
            "{}: {} name '{}' contains invalid characters.\n",
            "Only A-Z, a-z, 0-9, _ (underscore) and - (dash) permitted. Skipping.",
        ),
        argv0,
        db_or_user.capitalized_noun(),
        db_or_user.name(),
    )
}

pub fn authorization_error_message(db_or_user: &DbOrUser) -> String {
    format!(
        "You are not in charge of mysql-{}: '{}'.  Skipping.",
        db_or_user.lowercased_noun(),
        db_or_user.name(),
    )
}

pub fn handle_create_user_error(error: &CreateUserError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        CreateUserError::ValidationError(ValidationError::NameValidationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(&DbOrUser::User(name.into()))
            );
        }
        CreateUserError::ValidationError(ValidationError::AuthorizationError(_)) => {
            eprintln!(
                "{}",
                authorization_error_message(&DbOrUser::User(name.into()))
            );
        }
        CreateUserError::MySqlError(_) | CreateUserError::UserAlreadyExists => {
            eprintln!("{argv0}: Failed to create user '{name}'.");
        }
    }
}

pub fn handle_drop_user_error(error: &DropUserError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        DropUserError::ValidationError(ValidationError::NameValidationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(&DbOrUser::User(name.into()))
            );
        }
        DropUserError::ValidationError(ValidationError::AuthorizationError(_)) => {
            eprintln!(
                "{}",
                authorization_error_message(&DbOrUser::User(name.into()))
            );
        }
        DropUserError::MySqlError(_) | DropUserError::UserDoesNotExist => {
            eprintln!("{argv0}: Failed to delete user '{name}'.");
        }
    }
}

pub fn handle_list_users_error(error: &ListUsersError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        ListUsersError::ValidationError(ValidationError::NameValidationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(&DbOrUser::User(name.into()))
            );
        }
        ListUsersError::ValidationError(ValidationError::AuthorizationError(_)) => {
            eprintln!(
                "{}",
                authorization_error_message(&DbOrUser::User(name.into()))
            );
        }
        ListUsersError::UserDoesNotExist => {
            eprintln!("{argv0}: User '{name}' does not exist. You must create it first.",);
        }
        ListUsersError::MySqlError(_) => {
            eprintln!("{argv0}: Failed to look up password for user '{name}'");
        }
    }
}

// ----------------------------------------------------------------------------

pub fn handle_create_database_error(error: &CreateDatabaseError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());
    match error {
        CreateDatabaseError::ValidationError(ValidationError::NameValidationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(&DbOrUser::Database(name.into()))
            );
        }

        CreateDatabaseError::ValidationError(ValidationError::AuthorizationError(_)) => {
            eprintln!(
                "{}",
                authorization_error_message(&DbOrUser::Database(name.into()))
            );
        }
        CreateDatabaseError::MySqlError(_) => {
            eprintln!("{argv0}: Cannot create database '{name}'.");
        }
        CreateDatabaseError::DatabaseAlreadyExists => {
            eprintln!("{argv0}: Database '{name}' already exists.");
        }
    }
}

pub fn handle_drop_database_error(error: &DropDatabaseError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());
    match error {
        DropDatabaseError::ValidationError(ValidationError::NameValidationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(&DbOrUser::Database(name.into()))
            );
        }
        DropDatabaseError::ValidationError(ValidationError::AuthorizationError(_)) => {
            eprintln!(
                "{}",
                authorization_error_message(&DbOrUser::Database(name.into()))
            );
        }
        DropDatabaseError::MySqlError(_) => {
            eprintln!("{argv0}: Cannot drop database '{name}'.");
        }
        DropDatabaseError::DatabaseDoesNotExist => {
            eprintln!("{argv0}: Database '{name}' doesn't exist.");
        }
    }
}

pub fn format_show_database_error_message(error: &ListPrivilegesError, name: &str) -> String {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());

    match error {
        ListPrivilegesError::ValidationError(ValidationError::NameValidationError(_)) => {
            name_validation_error_to_error_message(&DbOrUser::Database(name.into()))
        }
        ListPrivilegesError::ValidationError(ValidationError::AuthorizationError(_)) => {
            authorization_error_message(&DbOrUser::Database(name.into()))
        }
        ListPrivilegesError::MySqlError(err) => {
            format!("{argv0}: Failed to look up privileges for database '{name}': {err}")
        }
        ListPrivilegesError::DatabaseDoesNotExist => {
            format!("{argv0}: Database '{name}' doesn't exist.")
        }
    }
}
