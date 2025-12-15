use crate::core::{
    common::UnixUser,
    protocol::{CheckAuthorizationError, request_validation::validate_db_or_user_request},
    types::DbOrUser,
};

pub async fn check_authorization(
    dbs_or_users: Vec<DbOrUser>,
    unix_user: &UnixUser,
) -> std::collections::BTreeMap<DbOrUser, Result<(), CheckAuthorizationError>> {
    let mut results = std::collections::BTreeMap::new();

    for db_or_user in dbs_or_users {
        if let Err(err) =
            validate_db_or_user_request(&db_or_user, unix_user).map_err(CheckAuthorizationError)
        {
            results.insert(db_or_user.clone(), Err(err));
            continue;
        }

        results.insert(db_or_user.clone(), Ok(()));
    }

    results
}
