use crate::core::{
    protocol::{
        CreateDatabaseError, CreateUserError, DropDatabaseError, DropUserError,
        GetDatabasesPrivilegeDataError, ListUsersError, request_validation::AuthorizationError,
    },
    types::DbOrUser,
};

pub fn name_validation_error_to_error_message(db_or_user: DbOrUser) -> String {
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

pub fn owner_validation_error_message(db_or_user: DbOrUser) -> String {
    format!(
        "You are not in charge of mysql-{}: '{}'.  Skipping.",
        db_or_user.lowercased_noun(),
        db_or_user.name(),
    )
}

pub fn handle_create_user_error(error: CreateUserError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        CreateUserError::AuthorizationError(AuthorizationError::SanitizationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(DbOrUser::User(name.into()))
            );
        }
        CreateUserError::AuthorizationError(AuthorizationError::OwnershipError(_)) => {
            eprintln!(
                "{}",
                owner_validation_error_message(DbOrUser::User(name.into()))
            );
        }
        CreateUserError::MySqlError(_) | CreateUserError::UserAlreadyExists => {
            eprintln!("{}: Failed to create user '{}'.", argv0, name);
        }
    }
}

pub fn handle_drop_user_error(error: DropUserError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        DropUserError::AuthorizationError(AuthorizationError::SanitizationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(DbOrUser::User(name.into()))
            );
        }
        DropUserError::AuthorizationError(AuthorizationError::OwnershipError(_)) => {
            eprintln!(
                "{}",
                owner_validation_error_message(DbOrUser::User(name.into()))
            );
        }
        DropUserError::MySqlError(_) | DropUserError::UserDoesNotExist => {
            eprintln!("{}: Failed to delete user '{}'.", argv0, name);
        }
    }
}

pub fn handle_list_users_error(error: ListUsersError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        ListUsersError::AuthorizationError(AuthorizationError::SanitizationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(DbOrUser::User(name.into()))
            );
        }
        ListUsersError::AuthorizationError(AuthorizationError::OwnershipError(_)) => {
            eprintln!(
                "{}",
                owner_validation_error_message(DbOrUser::User(name.into()))
            );
        }
        ListUsersError::UserDoesNotExist => {
            eprintln!(
                "{}: User '{}' does not exist. You must create it first.",
                argv0, name,
            );
        }
        ListUsersError::MySqlError(_) => {
            eprintln!("{}: Failed to look up password for user '{}'", argv0, name);
        }
    }
}

// ----------------------------------------------------------------------------

pub fn handle_create_database_error(error: CreateDatabaseError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());
    match error {
        CreateDatabaseError::AuthorizationError(AuthorizationError::SanitizationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(DbOrUser::Database(name.into()))
            );
        }

        CreateDatabaseError::AuthorizationError(AuthorizationError::OwnershipError(_)) => {
            eprintln!(
                "{}",
                owner_validation_error_message(DbOrUser::Database(name.into()))
            );
        }
        CreateDatabaseError::MySqlError(_) => {
            eprintln!("{}: Cannot create database '{}'.", argv0, name);
        }
        CreateDatabaseError::DatabaseAlreadyExists => {
            eprintln!("{}: Database '{}' already exists.", argv0, name);
        }
    }
}

pub fn handle_drop_database_error(error: DropDatabaseError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());
    match error {
        DropDatabaseError::AuthorizationError(AuthorizationError::SanitizationError(_)) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(DbOrUser::Database(name.into()))
            );
        }
        DropDatabaseError::AuthorizationError(AuthorizationError::OwnershipError(_)) => {
            eprintln!(
                "{}",
                owner_validation_error_message(DbOrUser::Database(name.into()))
            );
        }
        DropDatabaseError::MySqlError(_) => {
            eprintln!("{}: Cannot drop database '{}'.", argv0, name);
        }
        DropDatabaseError::DatabaseDoesNotExist => {
            eprintln!("{}: Database '{}' doesn't exist.", argv0, name);
        }
    }
}

pub fn format_show_database_error_message(
    error: GetDatabasesPrivilegeDataError,
    name: &str,
) -> String {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-dbadm".to_string());

    match error {
        GetDatabasesPrivilegeDataError::AuthorizationError(
            AuthorizationError::SanitizationError(_),
        ) => name_validation_error_to_error_message(DbOrUser::Database(name.into())),
        GetDatabasesPrivilegeDataError::AuthorizationError(AuthorizationError::OwnershipError(
            _,
        )) => owner_validation_error_message(DbOrUser::Database(name.into())),
        GetDatabasesPrivilegeDataError::MySqlError(err) => {
            format!(
                "{}: Failed to look up privileges for database '{}': {}",
                argv0, name, err
            )
        }
        GetDatabasesPrivilegeDataError::DatabaseDoesNotExist => {
            format!("{}: Database '{}' doesn't exist.", argv0, name)
        }
    }
}
