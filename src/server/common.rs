use crate::core::common::UnixUser;
use sqlx::prelude::*;

/// This function creates a regex that matches items (users, databases)
/// that belong to the user or any of the user's groups.
pub fn create_user_group_matching_regex(user: &UnixUser) -> String {
    if user.groups.is_empty() {
        format!("{}_.+", user.username)
    } else {
        format!("({}|{})_.+", user.username, user.groups.join("|"))
    }
}

/// Some mysql versions with some collations mark some columns as binary fields,
/// which in the current version of sqlx is not parsable as string.
/// See: https://github.com/launchbadge/sqlx/issues/3387
#[inline]
pub fn try_get_with_binary_fallback(
    row: &sqlx::mysql::MySqlRow,
    column: &str,
) -> Result<String, sqlx::Error> {
    row.try_get(column).or_else(|_| {
        row.try_get::<Vec<u8>, _>(column)
            .map(|v| String::from_utf8_lossy(&v).to_string())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use regex::Regex;

    #[test]
    fn test_create_user_group_matching_regex() {
        let user = UnixUser {
            username: "user".to_owned(),
            groups: vec!["group1".to_owned(), "group2".to_owned()],
        };

        let regex = create_user_group_matching_regex(&user);
        let re = Regex::new(&regex).unwrap();

        assert!(re.is_match("user_something"));
        assert!(re.is_match("group1_something"));
        assert!(re.is_match("group2_something"));

        assert!(!re.is_match("other_something"));
        assert!(!re.is_match("user"));
        assert!(!re.is_match("usersomething"));
    }
}
