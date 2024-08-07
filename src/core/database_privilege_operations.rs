//! Database privilege operations
//!
//! This module contains functions for querying, modifying,
//! displaying and comparing database privileges.
//!
//! A lot of the complexity comes from two core components:
//!
//! - The privilege editor that needs to be able to print
//!   an editable table of privileges and reparse the content
//!   after the user has made manual changes.
//!
//! - The comparison functionality that tells the user what
//!   changes will be made when applying a set of changes
//!   to the list of database privileges.

use std::collections::HashMap;

use anyhow::Context;
use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlRow, prelude::*, MySqlConnection};

use crate::core::{
    common::{
        create_user_group_matching_regex, get_current_unix_user, quote_identifier, rev_yn, yn,
    },
    database_operations::validate_database_name,
};

/// This is the list of fields that are used to fetch the db + user + privileges
/// from the `db` table in the database. If you need to add or remove privilege
/// fields, this is a good place to start.
pub const DATABASE_PRIVILEGE_FIELDS: [&str; 13] = [
    "db",
    "user",
    "select_priv",
    "insert_priv",
    "update_priv",
    "delete_priv",
    "create_priv",
    "drop_priv",
    "alter_priv",
    "index_priv",
    "create_tmp_table_priv",
    "lock_tables_priv",
    "references_priv",
];

pub fn db_priv_field_human_readable_name(name: &str) -> String {
    match name {
        "db" => "Database".to_owned(),
        "user" => "User".to_owned(),
        "select_priv" => "Select".to_owned(),
        "insert_priv" => "Insert".to_owned(),
        "update_priv" => "Update".to_owned(),
        "delete_priv" => "Delete".to_owned(),
        "create_priv" => "Create".to_owned(),
        "drop_priv" => "Drop".to_owned(),
        "alter_priv" => "Alter".to_owned(),
        "index_priv" => "Index".to_owned(),
        "create_tmp_table_priv" => "Temp".to_owned(),
        "lock_tables_priv" => "Lock".to_owned(),
        "references_priv" => "References".to_owned(),
        _ => format!("Unknown({})", name),
    }
}

/// This struct represents the set of privileges for a single user on a single database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivilegeRow {
    pub db: String,
    pub user: String,
    pub select_priv: bool,
    pub insert_priv: bool,
    pub update_priv: bool,
    pub delete_priv: bool,
    pub create_priv: bool,
    pub drop_priv: bool,
    pub alter_priv: bool,
    pub index_priv: bool,
    pub create_tmp_table_priv: bool,
    pub lock_tables_priv: bool,
    pub references_priv: bool,
}

impl DatabasePrivilegeRow {
    pub fn get_privilege_by_name(&self, name: &str) -> bool {
        match name {
            "select_priv" => self.select_priv,
            "insert_priv" => self.insert_priv,
            "update_priv" => self.update_priv,
            "delete_priv" => self.delete_priv,
            "create_priv" => self.create_priv,
            "drop_priv" => self.drop_priv,
            "alter_priv" => self.alter_priv,
            "index_priv" => self.index_priv,
            "create_tmp_table_priv" => self.create_tmp_table_priv,
            "lock_tables_priv" => self.lock_tables_priv,
            "references_priv" => self.references_priv,
            _ => false,
        }
    }

    pub fn diff(&self, other: &DatabasePrivilegeRow) -> DatabasePrivilegeRowDiff {
        debug_assert!(self.db == other.db && self.user == other.user);

        DatabasePrivilegeRowDiff {
            db: self.db.clone(),
            user: self.user.clone(),
            diff: DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .skip(2)
                .filter_map(|field| {
                    DatabasePrivilegeChange::new(
                        self.get_privilege_by_name(field),
                        other.get_privilege_by_name(field),
                        field,
                    )
                })
                .collect(),
        }
    }
}

impl FromRow<'_, MySqlRow> for DatabasePrivilegeRow {
    fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            db: row.try_get("db")?,
            user: row.try_get("user")?,
            select_priv: row.try_get("select_priv").map(rev_yn)?,
            insert_priv: row.try_get("insert_priv").map(rev_yn)?,
            update_priv: row.try_get("update_priv").map(rev_yn)?,
            delete_priv: row.try_get("delete_priv").map(rev_yn)?,
            create_priv: row.try_get("create_priv").map(rev_yn)?,
            drop_priv: row.try_get("drop_priv").map(rev_yn)?,
            alter_priv: row.try_get("alter_priv").map(rev_yn)?,
            index_priv: row.try_get("index_priv").map(rev_yn)?,
            create_tmp_table_priv: row.try_get("create_tmp_table_priv").map(rev_yn)?,
            lock_tables_priv: row.try_get("lock_tables_priv").map(rev_yn)?,
            references_priv: row.try_get("references_priv").map(rev_yn)?,
        })
    }
}

pub async fn get_database_privileges(
    database_name: &str,
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = get_current_unix_user()?;
    validate_database_name(database_name, &unix_user)?;

    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ?",
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| quote_identifier(field))
            .join(","),
    ))
    .bind(database_name)
    .fetch_all(connection)
    .await
    .context("Failed to show database")?;

    Ok(result)
}

pub async fn get_all_database_privileges(
    connection: &mut MySqlConnection,
) -> anyhow::Result<Vec<DatabasePrivilegeRow>> {
    let unix_user = get_current_unix_user()?;

    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
        indoc! {r#"
          SELECT {} FROM `db` WHERE `db` IN
          (SELECT DISTINCT `SCHEMA_NAME` AS `database`
            FROM `information_schema`.`SCHEMATA`
            WHERE `SCHEMA_NAME` NOT IN ('information_schema', 'performance_schema', 'mysql', 'sys')
              AND `SCHEMA_NAME` REGEXP ?)
        "#},
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| format!("`{field}`"))
            .join(","),
    ))
    .bind(create_user_group_matching_regex(&unix_user))
    .fetch_all(connection)
    .await
    .context("Failed to show databases")?;

    Ok(result)
}

/*******************/
/* PRIVILEGE DIFFS */
/*******************/

/// This struct represents encapsulates the differences between two
/// instances of privilege sets for a single user on a single database.
///
/// The `User` and `Database` are the same for both instances.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DatabasePrivilegeRowDiff {
    pub db: String,
    pub user: String,
    pub diff: Vec<DatabasePrivilegeChange>,
}

/// This enum represents a change in a single privilege.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegeChange {
    YesToNo(String),
    NoToYes(String),
}

impl DatabasePrivilegeChange {
    pub fn new(p1: bool, p2: bool, name: &str) -> Option<DatabasePrivilegeChange> {
        match (p1, p2) {
            (true, false) => Some(DatabasePrivilegeChange::YesToNo(name.to_owned())),
            (false, true) => Some(DatabasePrivilegeChange::NoToYes(name.to_owned())),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DatabasePrivilegesDiff {
    New(DatabasePrivilegeRow),
    Modified(DatabasePrivilegeRowDiff),
    Deleted(DatabasePrivilegeRow),
}

pub async fn diff_privileges(
    from: Vec<DatabasePrivilegeRow>,
    to: &[DatabasePrivilegeRow],
) -> Vec<DatabasePrivilegesDiff> {
    let from_lookup_table: HashMap<(String, String), DatabasePrivilegeRow> = HashMap::from_iter(
        from.iter()
            .cloned()
            .map(|p| ((p.db.clone(), p.user.clone()), p)),
    );

    let to_lookup_table: HashMap<(String, String), DatabasePrivilegeRow> = HashMap::from_iter(
        to.iter()
            .cloned()
            .map(|p| ((p.db.clone(), p.user.clone()), p)),
    );

    let mut result = vec![];

    for p in to {
        if let Some(old_p) = from_lookup_table.get(&(p.db.clone(), p.user.clone())) {
            let diff = old_p.diff(p);
            if !diff.diff.is_empty() {
                result.push(DatabasePrivilegesDiff::Modified(diff));
            }
        } else {
            result.push(DatabasePrivilegesDiff::New(p.clone()));
        }
    }

    for p in from {
        if !to_lookup_table.contains_key(&(p.db.clone(), p.user.clone())) {
            result.push(DatabasePrivilegesDiff::Deleted(p));
        }
    }

    result
}

pub async fn apply_privilege_diffs(
    diffs: Vec<DatabasePrivilegesDiff>,
    connection: &mut MySqlConnection,
) -> anyhow::Result<()> {
    for diff in diffs {
        match diff {
            DatabasePrivilegesDiff::New(p) => {
                let tables = DATABASE_PRIVILEGE_FIELDS
                    .iter()
                    .map(|field| format!("`{field}`"))
                    .join(",");

                let question_marks = std::iter::repeat("?")
                    .take(DATABASE_PRIVILEGE_FIELDS.len())
                    .join(",");

                sqlx::query(
                    format!("INSERT INTO `db` ({}) VALUES ({})", tables, question_marks).as_str(),
                )
                .bind(p.db)
                .bind(p.user)
                .bind(yn(p.select_priv))
                .bind(yn(p.insert_priv))
                .bind(yn(p.update_priv))
                .bind(yn(p.delete_priv))
                .bind(yn(p.create_priv))
                .bind(yn(p.drop_priv))
                .bind(yn(p.alter_priv))
                .bind(yn(p.index_priv))
                .bind(yn(p.create_tmp_table_priv))
                .bind(yn(p.lock_tables_priv))
                .bind(yn(p.references_priv))
                .execute(&mut *connection)
                .await?;
            }
            DatabasePrivilegesDiff::Modified(p) => {
                let tables = p
                    .diff
                    .iter()
                    .map(|diff| match diff {
                        DatabasePrivilegeChange::YesToNo(name) => format!("`{}` = 'N'", name),
                        DatabasePrivilegeChange::NoToYes(name) => format!("`{}` = 'Y'", name),
                    })
                    .join(",");

                sqlx::query(
                    format!("UPDATE `db` SET {} WHERE `db` = ? AND `user` = ?", tables).as_str(),
                )
                .bind(p.db)
                .bind(p.user)
                .execute(&mut *connection)
                .await?;
            }
            DatabasePrivilegesDiff::Deleted(p) => {
                sqlx::query("DELETE FROM `db` WHERE `db` = ? AND `user` = ?")
                    .bind(p.db)
                    .bind(p.user)
                    .execute(&mut *connection)
                    .await?;
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_database_privilege_change_creation() {
        assert_eq!(
            DatabasePrivilegeChange::new(true, false, "test"),
            Some(DatabasePrivilegeChange::YesToNo("test".to_owned()))
        );
        assert_eq!(
            DatabasePrivilegeChange::new(false, true, "test"),
            Some(DatabasePrivilegeChange::NoToYes("test".to_owned()))
        );
        assert_eq!(DatabasePrivilegeChange::new(true, true, "test"), None);
        assert_eq!(DatabasePrivilegeChange::new(false, false, "test"), None);
    }

    #[tokio::test]
    async fn test_diff_privileges() {
        let from = vec![DatabasePrivilegeRow {
            db: "db".to_owned(),
            user: "user".to_owned(),
            select_priv: true,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: true,
            drop_priv: true,
            alter_priv: true,
            index_priv: true,
            create_tmp_table_priv: true,
            lock_tables_priv: true,
            references_priv: true,
        }];

        let to = vec![DatabasePrivilegeRow {
            db: "db".to_owned(),
            user: "user".to_owned(),
            select_priv: false,
            insert_priv: true,
            update_priv: true,
            delete_priv: true,
            create_priv: true,
            drop_priv: true,
            alter_priv: true,
            index_priv: true,
            create_tmp_table_priv: true,
            lock_tables_priv: true,
            references_priv: true,
        }];

        let diffs = diff_privileges(from, &to).await;

        assert_eq!(
            diffs,
            vec![DatabasePrivilegesDiff::Modified(DatabasePrivilegeRowDiff {
                db: "db".to_owned(),
                user: "user".to_owned(),
                diff: vec![DatabasePrivilegeChange::YesToNo("select_priv".to_owned())],
            })]
        );

        assert!(matches!(&diffs[0], DatabasePrivilegesDiff::Modified(_)));
    }
}
