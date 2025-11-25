//! This module contains serialization and deserialization logic for
//! database privileges related CLI commands.

use super::diff::{DatabasePrivilegeChange, DatabasePrivilegeRowDiff};
use crate::core::types::{MySQLDatabase, MySQLUser};

/// This enum represents a part of a CLI argument for editing database privileges,
/// indicating whether privileges are to be added, set, or removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DatabasePrivilegeEditEntryType {
    Add,
    Set,
    Remove,
}

/// This struct represents a single CLI argument for editing database privileges.
///
/// This is typically parsed from a string looking like:
///
///   `[database_name:]username:[+|-]privileges`
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DatabasePrivilegeEditEntry {
    pub database: Option<MySQLDatabase>,
    pub user: MySQLUser,
    pub type_: DatabasePrivilegeEditEntryType,
    pub privileges: Vec<String>,
}

impl DatabasePrivilegeEditEntry {
    /// Parses a privilege edit entry from a string.
    ///
    /// The expected format is:
    ///
    ///   `[database_name:]username:[+|-]privileges`
    ///
    /// where:
    /// - database_name is optional, if omitted the entry applies to all databases
    /// - username is the name of the user to edit privileges for
    /// - privileges is a string of characters representing the privileges to add, set or remove
    /// - the `+` or `-` prefix indicates whether to add or remove the privileges, if omitted the privileges are set directly
    /// - privileges characters are: siudcDaAItlrA
    pub fn parse_from_str(arg: &str) -> anyhow::Result<DatabasePrivilegeEditEntry> {
        let parts: Vec<&str> = arg.split(':').collect();
        if parts.len() < 2 || parts.len() > 3 {
            anyhow::bail!("Invalid privilege edit entry format: {}", arg);
        }

        let (database, user, user_privs) = if parts.len() == 3 {
            (Some(parts[0].to_string()), parts[1].to_string(), parts[2])
        } else {
            (None, parts[0].to_string(), parts[1])
        };

        if user.is_empty() {
            anyhow::bail!("Username cannot be empty in privilege edit entry: {}", arg);
        }

        let (edit_type, privs_str) = if let Some(privs_str) = user_privs.strip_prefix('+') {
            (DatabasePrivilegeEditEntryType::Add, privs_str)
        } else if let Some(privs_str) = user_privs.strip_prefix('-') {
            (DatabasePrivilegeEditEntryType::Remove, privs_str)
        } else {
            (DatabasePrivilegeEditEntryType::Set, user_privs)
        };

        let privileges: Vec<String> = privs_str.chars().map(|c| c.to_string()).collect();
        if privileges.iter().any(|c| !"siudcDaAItlrA".contains(c)) {
            let invalid_chars: String = privileges
                .iter()
                .filter(|c| !"siudcDaAItlrA".contains(c.as_str()))
                .cloned()
                .collect();
            anyhow::bail!(
                "Invalid character(s) in privilege edit entry: {}",
                invalid_chars
            );
        }

        Ok(DatabasePrivilegeEditEntry {
            database: database.map(MySQLDatabase::from),
            user: MySQLUser::from(user),
            type_: edit_type,
            privileges,
        })
    }

    pub fn as_database_privileges_diff(
        &self,
        external_database_name: Option<&MySQLDatabase>,
    ) -> anyhow::Result<DatabasePrivilegeRowDiff> {
        let database = match self.database.as_ref() {
            Some(db) => db.clone(),
            None => {
                if let Some(external_db) = external_database_name {
                    external_db.clone()
                } else {
                    anyhow::bail!(
                        "Database name must be specified either in the privilege edit entry or as an external argument."
                    );
                }
            }
        };
        let mut diff;
        match self.type_ {
            DatabasePrivilegeEditEntryType::Set => {
                diff = DatabasePrivilegeRowDiff {
                    db: database,
                    user: self.user.clone(),
                    select_priv: Some(DatabasePrivilegeChange::YesToNo),
                    insert_priv: Some(DatabasePrivilegeChange::YesToNo),
                    update_priv: Some(DatabasePrivilegeChange::YesToNo),
                    delete_priv: Some(DatabasePrivilegeChange::YesToNo),
                    create_priv: Some(DatabasePrivilegeChange::YesToNo),
                    drop_priv: Some(DatabasePrivilegeChange::YesToNo),
                    alter_priv: Some(DatabasePrivilegeChange::YesToNo),
                    index_priv: Some(DatabasePrivilegeChange::YesToNo),
                    create_tmp_table_priv: Some(DatabasePrivilegeChange::YesToNo),
                    lock_tables_priv: Some(DatabasePrivilegeChange::YesToNo),
                    references_priv: Some(DatabasePrivilegeChange::YesToNo),
                };
                for priv_char in &self.privileges {
                    match priv_char.as_str() {
                        "s" => diff.select_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "i" => diff.insert_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "u" => diff.update_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "d" => diff.delete_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "c" => diff.create_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "D" => diff.drop_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "a" => diff.alter_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "I" => diff.index_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "t" => diff.create_tmp_table_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "l" => diff.lock_tables_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "r" => diff.references_priv = Some(DatabasePrivilegeChange::NoToYes),
                        "A" => {
                            diff.select_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.insert_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.update_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.delete_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.create_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.drop_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.alter_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.index_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.create_tmp_table_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.lock_tables_priv = Some(DatabasePrivilegeChange::NoToYes);
                            diff.references_priv = Some(DatabasePrivilegeChange::NoToYes);
                        }
                        _ => unreachable!(),
                    }
                }
            }
            DatabasePrivilegeEditEntryType::Add | DatabasePrivilegeEditEntryType::Remove => {
                diff = DatabasePrivilegeRowDiff {
                    db: database,
                    user: self.user.clone(),
                    select_priv: None,
                    insert_priv: None,
                    update_priv: None,
                    delete_priv: None,
                    create_priv: None,
                    drop_priv: None,
                    alter_priv: None,
                    index_priv: None,
                    create_tmp_table_priv: None,
                    lock_tables_priv: None,
                    references_priv: None,
                };
                let value = match self.type_ {
                    DatabasePrivilegeEditEntryType::Add => DatabasePrivilegeChange::NoToYes,
                    DatabasePrivilegeEditEntryType::Remove => DatabasePrivilegeChange::YesToNo,
                    _ => unreachable!(),
                };
                for priv_char in &self.privileges {
                    match priv_char.as_str() {
                        "s" => diff.select_priv = Some(value),
                        "i" => diff.insert_priv = Some(value),
                        "u" => diff.update_priv = Some(value),
                        "d" => diff.delete_priv = Some(value),
                        "c" => diff.create_priv = Some(value),
                        "D" => diff.drop_priv = Some(value),
                        "a" => diff.alter_priv = Some(value),
                        "I" => diff.index_priv = Some(value),
                        "t" => diff.create_tmp_table_priv = Some(value),
                        "l" => diff.lock_tables_priv = Some(value),
                        "r" => diff.references_priv = Some(value),
                        "A" => {
                            diff.select_priv = Some(value);
                            diff.insert_priv = Some(value);
                            diff.update_priv = Some(value);
                            diff.delete_priv = Some(value);
                            diff.create_priv = Some(value);
                            diff.drop_priv = Some(value);
                            diff.alter_priv = Some(value);
                            diff.index_priv = Some(value);
                            diff.create_tmp_table_priv = Some(value);
                            diff.lock_tables_priv = Some(value);
                            diff.references_priv = Some(value);
                        }
                        _ => unreachable!(),
                    }
                }
            }
        }

        Ok(diff)
    }
}

impl std::fmt::Display for DatabasePrivilegeEditEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if let Some(db) = &self.database {
            write!(f, "{}:, ", db)?;
        }
        write!(f, "{}: ", self.user)?;
        match self.type_ {
            DatabasePrivilegeEditEntryType::Add => write!(f, "+")?,
            DatabasePrivilegeEditEntryType::Set => {}
            DatabasePrivilegeEditEntryType::Remove => write!(f, "-")?,
        }
        for priv_char in &self.privileges {
            write!(f, "{}", priv_char)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_arg_parse_set_db_user_all() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:A");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: Some("db".into()),
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Set,
                privileges: vec!["A".into()],
            })
        );
    }

    #[test]
    fn test_cli_arg_parse_set_db_user_none() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: Some("db".into()),
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Set,
                privileges: vec![],
            })
        );
    }

    #[test]
    fn test_cli_arg_parse_set_db_user_misc() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:siud");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: Some("db".into()),
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Set,
                privileges: vec!["s".into(), "i".into(), "u".into(), "d".into()],
            })
        );
    }

    #[test]
    fn test_cli_arg_parse_set_user_nonexistent_misc() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("user:siud");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: None,
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Set,
                privileges: vec!["s".into(), "i".into(), "u".into(), "d".into()],
            }),
        );
    }

    #[test]
    fn test_cli_arg_parse_set_db_user_nonexistent_privilege() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:F");
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_arg_parse_set_user_empty_string() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("::");
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_arg_parse_set_db_user_empty_string() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db::");
        assert!(result.is_err());
    }

    #[test]
    fn test_cli_arg_parse_add_db_user_misc() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:+siud");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: Some("db".into()),
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Add,
                privileges: vec!["s".into(), "i".into(), "u".into(), "d".into()],
            })
        );
    }

    #[test]
    fn test_cli_arg_parse_remove_db_user_misc() {
        let result = DatabasePrivilegeEditEntry::parse_from_str("db:user:-siud");
        assert_eq!(
            result.ok(),
            Some(DatabasePrivilegeEditEntry {
                database: Some("db".into()),
                user: "user".into(),
                type_: DatabasePrivilegeEditEntryType::Remove,
                privileges: vec!["s".into(), "i".into(), "u".into(), "d".into()],
            }),
        );
    }
}
