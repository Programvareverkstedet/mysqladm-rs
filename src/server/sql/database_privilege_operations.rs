// TODO: fix comment
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

use std::collections::{BTreeMap, BTreeSet};

use indoc::indoc;
use itertools::Itertools;
use serde::{Deserialize, Serialize};
use sqlx::{mysql::MySqlRow, prelude::*, MySqlConnection};

use crate::{
    core::{
        common::{rev_yn, yn, UnixUser},
        database_privileges::{DatabasePrivilegeChange, DatabasePrivilegesDiff},
        protocol::{
            DiffDoesNotApplyError, GetAllDatabasesPrivilegeData, GetAllDatabasesPrivilegeDataError,
            GetDatabasesPrivilegeData, GetDatabasesPrivilegeDataError,
            ModifyDatabasePrivilegesError, ModifyDatabasePrivilegesOutput,
        },
    },
    server::{
        common::create_user_group_matching_regex,
        input_sanitization::{quote_identifier, validate_name, validate_ownership_by_unix_user},
        sql::database_operations::unsafe_database_exists,
    },
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

// NOTE: ord is needed for BTreeSet to accept the type, but it
//       doesn't have any natural implementation semantics.

/// This struct represents the set of privileges for a single user on a single database.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, PartialOrd, Ord)]
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
}

#[inline]
fn get_mysql_row_priv_field(row: &MySqlRow, position: usize) -> Result<bool, sqlx::Error> {
    let field = DATABASE_PRIVILEGE_FIELDS[position];
    let value = row.try_get(position)?;
    match rev_yn(value) {
        Some(val) => Ok(val),
        _ => {
            log::warn!(r#"Invalid value for privilege "{}": '{}'"#, field, value);
            Ok(false)
        }
    }
}

impl FromRow<'_, MySqlRow> for DatabasePrivilegeRow {
    fn from_row(row: &MySqlRow) -> Result<Self, sqlx::Error> {
        Ok(Self {
            db: row.try_get("db")?,
            user: row.try_get("user")?,
            select_priv: get_mysql_row_priv_field(row, 2)?,
            insert_priv: get_mysql_row_priv_field(row, 3)?,
            update_priv: get_mysql_row_priv_field(row, 4)?,
            delete_priv: get_mysql_row_priv_field(row, 5)?,
            create_priv: get_mysql_row_priv_field(row, 6)?,
            drop_priv: get_mysql_row_priv_field(row, 7)?,
            alter_priv: get_mysql_row_priv_field(row, 8)?,
            index_priv: get_mysql_row_priv_field(row, 9)?,
            create_tmp_table_priv: get_mysql_row_priv_field(row, 10)?,
            lock_tables_priv: get_mysql_row_priv_field(row, 11)?,
            references_priv: get_mysql_row_priv_field(row, 12)?,
        })
    }
}

// NOTE: this function is unsafe because it does no input validation.
/// Get all users + privileges for a single database.
async fn unsafe_get_database_privileges(
    database_name: &str,
    connection: &mut MySqlConnection,
) -> Result<Vec<DatabasePrivilegeRow>, sqlx::Error> {
    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ?",
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| quote_identifier(field))
            .join(","),
    ))
    .bind(database_name)
    .fetch_all(connection)
    .await;

    if let Err(e) = &result {
        log::error!(
            "Failed to get database privileges for '{}': {}",
            &database_name,
            e
        );
    }

    result
}

// NOTE: this function is unsafe because it does no input validation.
/// Get all users + privileges for a single database-user pair.
pub async fn unsafe_get_database_privileges_for_db_user_pair(
    database_name: &str,
    user_name: &str,
    connection: &mut MySqlConnection,
) -> Result<Option<DatabasePrivilegeRow>, sqlx::Error> {
    let result = sqlx::query_as::<_, DatabasePrivilegeRow>(&format!(
        "SELECT {} FROM `db` WHERE `db` = ? AND `user` = ?",
        DATABASE_PRIVILEGE_FIELDS
            .iter()
            .map(|field| quote_identifier(field))
            .join(","),
    ))
    .bind(database_name)
    .bind(user_name)
    .fetch_optional(connection)
    .await;

    if let Err(e) = &result {
        log::error!(
            "Failed to get database privileges for '{}.{}': {}",
            &database_name,
            &user_name,
            e
        );
    }

    result
}

pub async fn get_databases_privilege_data(
    database_names: Vec<String>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> GetDatabasesPrivilegeData {
    let mut results = BTreeMap::new();

    for database_name in database_names.iter() {
        if let Err(err) = validate_name(database_name) {
            results.insert(
                database_name.clone(),
                Err(GetDatabasesPrivilegeDataError::SanitizationError(err)),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(database_name, unix_user) {
            results.insert(
                database_name.clone(),
                Err(GetDatabasesPrivilegeDataError::OwnershipError(err)),
            );
            continue;
        }

        if !unsafe_database_exists(database_name, connection)
            .await
            .unwrap()
        {
            results.insert(
                database_name.clone(),
                Err(GetDatabasesPrivilegeDataError::DatabaseDoesNotExist),
            );
            continue;
        }

        let result = unsafe_get_database_privileges(database_name, connection)
            .await
            .map_err(|e| GetDatabasesPrivilegeDataError::MySqlError(e.to_string()));

        results.insert(database_name.clone(), result);
    }

    debug_assert!(database_names.len() == results.len());

    results
}

/// Get all database + user + privileges pairs that are owned by the current user.
pub async fn get_all_database_privileges(
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> GetAllDatabasesPrivilegeData {
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
            .map(|field| quote_identifier(field))
            .join(","),
    ))
    .bind(create_user_group_matching_regex(unix_user))
    .fetch_all(connection)
    .await
    .map_err(|e| GetAllDatabasesPrivilegeDataError::MySqlError(e.to_string()));

    if let Err(e) = &result {
        log::error!("Failed to get all database privileges: {:?}", e);
    }

    result
}

async fn unsafe_apply_privilege_diff(
    database_privilege_diff: &DatabasePrivilegesDiff,
    connection: &mut MySqlConnection,
) -> Result<(), sqlx::Error> {
    let result = match database_privilege_diff {
        DatabasePrivilegesDiff::New(p) => {
            let tables = DATABASE_PRIVILEGE_FIELDS
                .iter()
                .map(|field| quote_identifier(field))
                .join(",");

            let question_marks = std::iter::repeat("?")
                .take(DATABASE_PRIVILEGE_FIELDS.len())
                .join(",");

            sqlx::query(
                format!("INSERT INTO `db` ({}) VALUES ({})", tables, question_marks).as_str(),
            )
            .bind(p.db.to_string())
            .bind(p.user.to_string())
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
            .execute(connection)
            .await
            .map(|_| ())
        }
        DatabasePrivilegesDiff::Modified(p) => {
            let changes = p
                .diff
                .iter()
                .map(|diff| match diff {
                    DatabasePrivilegeChange::YesToNo(name) => {
                        format!("{} = 'N'", quote_identifier(name))
                    }
                    DatabasePrivilegeChange::NoToYes(name) => {
                        format!("{} = 'Y'", quote_identifier(name))
                    }
                })
                .join(",");

            sqlx::query(
                format!("UPDATE `db` SET {} WHERE `db` = ? AND `user` = ?", changes).as_str(),
            )
            .bind(p.db.to_string())
            .bind(p.user.to_string())
            .execute(connection)
            .await
            .map(|_| ())
        }
        DatabasePrivilegesDiff::Deleted(p) => {
            sqlx::query("DELETE FROM `db` WHERE `db` = ? AND `user` = ?")
                .bind(p.db.to_string())
                .bind(p.user.to_string())
                .execute(connection)
                .await
                .map(|_| ())
        }
    };

    if let Err(e) = &result {
        log::error!("Failed to apply database privilege diff: {}", e);
    }

    result
}

async fn validate_diff(
    diff: &DatabasePrivilegesDiff,
    connection: &mut MySqlConnection,
) -> Result<(), ModifyDatabasePrivilegesError> {
    let privilege_row = unsafe_get_database_privileges_for_db_user_pair(
        diff.get_database_name(),
        diff.get_user_name(),
        connection,
    )
    .await;

    let privilege_row = match privilege_row {
        Ok(privilege_row) => privilege_row,
        Err(e) => return Err(ModifyDatabasePrivilegesError::MySqlError(e.to_string())),
    };

    let result = match diff {
        DatabasePrivilegesDiff::New(_) => {
            if privilege_row.is_some() {
                Err(ModifyDatabasePrivilegesError::DiffDoesNotApply(
                    DiffDoesNotApplyError::RowAlreadyExists(
                        diff.get_user_name().to_string(),
                        diff.get_database_name().to_string(),
                    ),
                ))
            } else {
                Ok(())
            }
        }
        DatabasePrivilegesDiff::Modified(_) if privilege_row.is_none() => {
            Err(ModifyDatabasePrivilegesError::DiffDoesNotApply(
                DiffDoesNotApplyError::RowDoesNotExist(
                    diff.get_user_name().to_string(),
                    diff.get_database_name().to_string(),
                ),
            ))
        }
        DatabasePrivilegesDiff::Modified(row_diff) => {
            let row = privilege_row.unwrap();

            let error_exists = row_diff.diff.iter().any(|change| match change {
                DatabasePrivilegeChange::YesToNo(name) => !row.get_privilege_by_name(name),
                DatabasePrivilegeChange::NoToYes(name) => row.get_privilege_by_name(name),
            });

            if error_exists {
                Err(ModifyDatabasePrivilegesError::DiffDoesNotApply(
                    DiffDoesNotApplyError::RowPrivilegeChangeDoesNotApply(row_diff.clone(), row),
                ))
            } else {
                Ok(())
            }
        }
        DatabasePrivilegesDiff::Deleted(_) => {
            if privilege_row.is_none() {
                Err(ModifyDatabasePrivilegesError::DiffDoesNotApply(
                    DiffDoesNotApplyError::RowDoesNotExist(
                        diff.get_user_name().to_string(),
                        diff.get_database_name().to_string(),
                    ),
                ))
            } else {
                Ok(())
            }
        }
    };

    result
}

/// Uses the result of [`diff_privileges`] to modify privileges in the database.
pub async fn apply_privilege_diffs(
    database_privilege_diffs: BTreeSet<DatabasePrivilegesDiff>,
    unix_user: &UnixUser,
    connection: &mut MySqlConnection,
) -> ModifyDatabasePrivilegesOutput {
    let mut results: BTreeMap<(String, String), _> = BTreeMap::new();

    for diff in database_privilege_diffs {
        let key = (
            diff.get_database_name().to_string(),
            diff.get_user_name().to_string(),
        );
        if let Err(err) = validate_name(diff.get_database_name()) {
            results.insert(
                key,
                Err(ModifyDatabasePrivilegesError::DatabaseSanitizationError(
                    err,
                )),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(diff.get_database_name(), unix_user) {
            results.insert(
                key,
                Err(ModifyDatabasePrivilegesError::DatabaseOwnershipError(err)),
            );
            continue;
        }

        if let Err(err) = validate_name(diff.get_user_name()) {
            results.insert(
                key,
                Err(ModifyDatabasePrivilegesError::UserSanitizationError(err)),
            );
            continue;
        }

        if let Err(err) = validate_ownership_by_unix_user(diff.get_user_name(), unix_user) {
            results.insert(
                key,
                Err(ModifyDatabasePrivilegesError::UserOwnershipError(err)),
            );
            continue;
        }

        if !unsafe_database_exists(diff.get_database_name(), connection)
            .await
            .unwrap()
        {
            results.insert(
                key,
                Err(ModifyDatabasePrivilegesError::DatabaseDoesNotExist),
            );
            continue;
        }

        if let Err(err) = validate_diff(&diff, connection).await {
            results.insert(key, Err(err));
            continue;
        }

        let result = unsafe_apply_privilege_diff(&diff, connection)
            .await
            .map_err(|e| ModifyDatabasePrivilegesError::MySqlError(e.to_string()));

        results.insert(key, result);
    }

    results
}
