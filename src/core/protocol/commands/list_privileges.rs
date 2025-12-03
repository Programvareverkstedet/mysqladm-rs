// TODO: merge all rows into a single collection.
//       they already contain which database they belong to.
//       no need to index by database name.

use std::collections::BTreeMap;

use itertools::Itertools;
use prettytable::{Cell, Row, Table};
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::core::{
    common::yn,
    database_privileges::{
        DATABASE_PRIVILEGE_FIELDS, DatabasePrivilegeRow, db_priv_field_human_readable_name,
    },
    protocol::request_validation::{NameValidationError, OwnerValidationError},
    types::{DbOrUser, MySQLDatabase},
};

pub type ListPrivilegesRequest = Option<Vec<MySQLDatabase>>;

pub type ListPrivilegesResponse =
    BTreeMap<MySQLDatabase, Result<Vec<DatabasePrivilegeRow>, GetDatabasesPrivilegeDataError>>;

pub fn print_list_privileges_output_status(output: &ListPrivilegesResponse) {
    let mut final_privs_map: BTreeMap<MySQLDatabase, Vec<DatabasePrivilegeRow>> = BTreeMap::new();
    for (db_name, db_result) in output {
        match db_result {
            Ok(db_rows) => {
                final_privs_map.insert(db_name.clone(), db_rows.clone());
            }
            Err(err) => {
                eprintln!("{}", err.to_error_message(db_name));
                eprintln!("Skipping...");
            }
        }
    }

    if final_privs_map.is_empty() {
        println!("No privileges to show.");
    } else {
        let mut table = Table::new();

        table.add_row(Row::new(
            DATABASE_PRIVILEGE_FIELDS
                .into_iter()
                .map(db_priv_field_human_readable_name)
                .map(|name| Cell::new(&name))
                .collect(),
        ));

        for (_database, rows) in final_privs_map {
            for row in rows.iter() {
                table.add_row(row![
                    row.db,
                    row.user,
                    c->yn(row.select_priv),
                    c->yn(row.insert_priv),
                    c->yn(row.update_priv),
                    c->yn(row.delete_priv),
                    c->yn(row.create_priv),
                    c->yn(row.drop_priv),
                    c->yn(row.alter_priv),
                    c->yn(row.index_priv),
                    c->yn(row.create_tmp_table_priv),
                    c->yn(row.lock_tables_priv),
                    c->yn(row.references_priv),
                ]);
            }
        }

        table.printstd();
    }
}

pub fn print_list_privileges_output_status_json(output: &ListPrivilegesResponse) {
    let value = output
        .iter()
        .map(|(name, result)| match result {
            Ok(row) => (
                name.to_string(),
                json!({
                  "status": "success",
                  "value": row.iter().into_group_map_by(|priv_row| priv_row.user.clone()),
                }),
            ),
            Err(err) => (
                name.to_string(),
                json!({
                  "status": "error",
                  "type": err.error_type(),
                  "error": err.to_error_message(name),
                }),
            ),
        })
        .collect::<serde_json::Map<_, _>>();
    println!(
        "{}",
        serde_json::to_string_pretty(&value)
            .unwrap_or("Failed to serialize result to JSON".to_string())
    );
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum GetDatabasesPrivilegeDataError {
    SanitizationError(NameValidationError),
    OwnershipError(OwnerValidationError),
    DatabaseDoesNotExist,
    MySqlError(String),
}

impl GetDatabasesPrivilegeDataError {
    pub fn to_error_message(&self, database_name: &MySQLDatabase) -> String {
        match self {
            GetDatabasesPrivilegeDataError::SanitizationError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            GetDatabasesPrivilegeDataError::OwnershipError(err) => {
                err.to_error_message(DbOrUser::Database(database_name.clone()))
            }
            GetDatabasesPrivilegeDataError::DatabaseDoesNotExist => {
                format!("Database '{}' does not exist.", database_name)
            }
            GetDatabasesPrivilegeDataError::MySqlError(err) => {
                format!("MySQL error: {}", err)
            }
        }
    }

    pub fn error_type(&self) -> &'static str {
        match self {
            GetDatabasesPrivilegeDataError::SanitizationError(_) => "sanitization-error",
            GetDatabasesPrivilegeDataError::OwnershipError(_) => "ownership-error",
            GetDatabasesPrivilegeDataError::DatabaseDoesNotExist => "database-does-not-exist",
            GetDatabasesPrivilegeDataError::MySqlError(_) => "mysql-error",
        }
    }
}
