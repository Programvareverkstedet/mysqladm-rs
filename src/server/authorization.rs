use crate::{
    core::{
        common::UnixUser,
        protocol::{CheckAuthorizationError, request_validation::AuthorizationError},
        types::DbOrUser,
    },
    server::input_sanitization::{validate_name, validate_ownership_by_unix_user},
};

pub async fn check_authorization(
    dbs_or_users: Vec<DbOrUser>,
    unix_user: &UnixUser,
) -> std::collections::BTreeMap<DbOrUser, Result<(), CheckAuthorizationError>> {
    let mut results = std::collections::BTreeMap::new();

    for db_or_user in dbs_or_users {
        if let Err(err) = validate_name(db_or_user.name())
            .map_err(AuthorizationError::SanitizationError)
            .map_err(CheckAuthorizationError)
        {
            results.insert(db_or_user.clone(), Err(err));
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(db_or_user.name(), unix_user)
            .map_err(AuthorizationError::OwnershipError)
            .map_err(CheckAuthorizationError)
        {
            results.insert(db_or_user.clone(), Err(err));
            continue;
        }

        results.insert(db_or_user.clone(), Ok(()));
    }

    results
}
