use crate::core::common::UnixUser;

/// This function creates a regex that matches items (users, databases)
/// that belong to the user or any of the user's groups.
pub fn create_user_group_matching_regex(user: &UnixUser) -> String {
    if user.groups.is_empty() {
        format!("{}(_.+)?", user.username)
    } else {
        format!("({}|{})(_.+)?", user.username, user.groups.join("|"))
    }
}
