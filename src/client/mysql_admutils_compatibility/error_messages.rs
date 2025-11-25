use crate::core::protocol::{
    CreateDatabaseError, CreateUserError, DropDatabaseError, DropUserError,
    GetDatabasesPrivilegeDataError, ListUsersError, request_validation::DbOrUser,
};

pub fn name_validation_error_to_error_message(name: &str, db_or_user: DbOrUser) -> String {
    let argv0 = std::env::args().next().unwrap_or_else(|| match db_or_user {
        DbOrUser::Database => "mysql-dbadm".to_string(),
        DbOrUser::User => "mysql-useradm".to_string(),
    });

    format!(
        concat!(
            "{}: {} name '{}' contains invalid characters.\n",
            "Only A-Z, a-z, 0-9, _ (underscore) and - (dash) permitted. Skipping.",
        ),
        argv0,
        db_or_user.capitalized(),
        name,
    )
}

pub fn owner_validation_error_message(name: &str, db_or_user: DbOrUser) -> String {
    format!(
        "You are not in charge of mysql-{}: '{}'.  Skipping.",
        db_or_user.lowercased(),
        name
    )
}

pub fn handle_create_user_error(error: CreateUserError, name: &str) {
    let argv0 = std::env::args()
        .next()
        .unwrap_or_else(|| "mysql-useradm".to_string());
    match error {
        CreateUserError::SanitizationError(_) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(name, DbOrUser::User)
            );
        }
        CreateUserError::OwnershipError(_) => {
            eprintln!("{}", owner_validation_error_message(name, DbOrUser::User));
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
        DropUserError::SanitizationError(_) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(name, DbOrUser::User)
            );
        }
        DropUserError::OwnershipError(_) => {
            eprintln!("{}", owner_validation_error_message(name, DbOrUser::User));
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
        ListUsersError::SanitizationError(_) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(name, DbOrUser::User)
            );
        }
        ListUsersError::OwnershipError(_) => {
            eprintln!("{}", owner_validation_error_message(name, DbOrUser::User));
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
        CreateDatabaseError::SanitizationError(_) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(name, DbOrUser::Database)
            );
        }
        CreateDatabaseError::OwnershipError(_) => {
            eprintln!(
                "{}",
                owner_validation_error_message(name, DbOrUser::Database)
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
        DropDatabaseError::SanitizationError(_) => {
            eprintln!(
                "{}",
                name_validation_error_to_error_message(name, DbOrUser::Database)
            );
        }
        DropDatabaseError::OwnershipError(_) => {
            eprintln!(
                "{}",
                owner_validation_error_message(name, DbOrUser::Database)
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
        GetDatabasesPrivilegeDataError::SanitizationError(_) => {
            name_validation_error_to_error_message(name, DbOrUser::Database)
        }
        GetDatabasesPrivilegeDataError::OwnershipError(_) => {
            owner_validation_error_message(name, DbOrUser::Database)
        }
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
