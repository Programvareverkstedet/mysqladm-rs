use crate::core::protocol::{MySQLDatabase, MySQLUser};

#[inline]
pub fn trim_db_name_to_32_chars(db_name: &MySQLDatabase) -> MySQLDatabase {
    db_name.chars().take(32).collect::<String>().into()
}

#[inline]
pub fn trim_user_name_to_32_chars(user_name: &MySQLUser) -> MySQLUser {
    user_name.chars().take(32).collect::<String>().into()
}
