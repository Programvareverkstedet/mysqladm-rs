use crate::core::common::{
    get_current_unix_user, validate_name_or_error, validate_ownership_or_error, DbOrUser,
};

/// In contrast to the new implementation which reports errors on any invalid name
/// for any reason, mysql-admutils would only log the error and skip that particular
/// name. This function replicates that behavior.
pub fn filter_db_or_user_names(
    names: Vec<String>,
    db_or_user: DbOrUser,
) -> anyhow::Result<Vec<String>> {
    let unix_user = get_current_unix_user()?;
    let argv0 = std::env::args().next().unwrap_or_else(|| match db_or_user {
        DbOrUser::Database => "mysql-dbadm".to_string(),
        DbOrUser::User => "mysql-useradm".to_string(),
    });

    let filtered_names = names
        .into_iter()
        // NOTE: The original implementation would only copy the first 32 characters
        //       of the argument into it's internal buffer. We replicate that behavior
        //       here.
        .map(|name| name.chars().take(32).collect::<String>())
        .filter(|name| {
            if let Err(_err) = validate_ownership_or_error(name, &unix_user, db_or_user) {
                println!(
                    "You are not in charge of mysql-{}: '{}'.  Skipping.",
                    db_or_user.lowercased(),
                    name
                );
                return false;
            }
            true
        })
        .filter(|name| {
            // NOTE: while this also checks for the length of the name,
            //       the name is already truncated to 32 characters. So
            //       if there is an error, it's guaranteed to be due to
            //       invalid characters.
            if let Err(_err) = validate_name_or_error(name, db_or_user) {
                println!(
                    concat!(
                        "{}: {} name '{}' contains invalid characters.\n",
                        "Only A-Z, a-z, 0-9, _ (underscore) and - (dash) permitted. Skipping.",
                    ),
                    argv0,
                    db_or_user.capitalized(),
                    name
                );
                return false;
            }
            true
        })
        .collect();

    Ok(filtered_names)
}
