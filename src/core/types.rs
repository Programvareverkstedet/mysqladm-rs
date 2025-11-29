use std::{
    fmt,
    ops::{Deref, DerefMut},
    str::FromStr,
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct MySQLUser(String);

impl FromStr for MySQLUser {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MySQLUser(s.to_string()))
    }
}

impl Deref for MySQLUser {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MySQLUser {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for MySQLUser {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MySQLUser {
    fn from(s: &str) -> Self {
        MySQLUser(s.to_string())
    }
}

impl From<String> for MySQLUser {
    fn from(s: String) -> Self {
        MySQLUser(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default)]
pub struct MySQLDatabase(String);

impl FromStr for MySQLDatabase {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(MySQLDatabase(s.to_string()))
    }
}

impl Deref for MySQLDatabase {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for MySQLDatabase {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for MySQLDatabase {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl From<&str> for MySQLDatabase {
    fn from(s: &str) -> Self {
        MySQLDatabase(s.to_string())
    }
}

impl From<String> for MySQLDatabase {
    fn from(s: String) -> Self {
        MySQLDatabase(s)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DbOrUser {
    Database(MySQLDatabase),
    User(MySQLUser),
}

impl DbOrUser {
    pub fn lowercased_noun(&self) -> &'static str {
        match self {
            DbOrUser::Database(_) => "database",
            DbOrUser::User(_) => "user",
        }
    }

    pub fn capitalized_noun(&self) -> &'static str {
        match self {
            DbOrUser::Database(_) => "Database",
            DbOrUser::User(_) => "User",
        }
    }

    pub fn name(&self) -> &str {
        match self {
            DbOrUser::Database(db) => db.as_str(),
            DbOrUser::User(user) => user.as_str(),
        }
    }
}
