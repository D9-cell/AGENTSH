use std::fs;
use std::path::PathBuf;

use anyhow::{Context as AnyhowContext, Result};
use chrono::Utc;
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Turn {
    pub user_input: String,
    pub planned_commands: Vec<String>,
    pub executed: bool,
    pub explanation: String,
}

pub struct HistoryDb {
    conn: Connection,
}

impl HistoryDb {
    pub fn open() -> Result<Self> {
        let path = Self::default_db_path()?;
        Self::open_at(path)
    }

    pub fn open_at(path: PathBuf) -> Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create history directory {}", parent.display())
            })?;
        }

        let conn = Connection::open(&path)
            .with_context(|| format!("failed to open history database at {}", path.display()))?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    pub fn insert_turn(&self, turn: &Turn) -> Result<()> {
        let commands = serde_json::to_string(&turn.planned_commands)?;
        self.conn.execute(
            "INSERT INTO turns (ts, user_input, commands, explanation, approved) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                Utc::now().to_rfc3339(),
                turn.user_input,
                commands,
                turn.explanation,
                i64::from(turn.executed),
            ],
        )?;
        Ok(())
    }

    pub fn recent(&self, n: usize) -> Result<Vec<Turn>> {
        let mut statement = self.conn.prepare(
            "SELECT user_input, commands, explanation, approved
             FROM turns
             ORDER BY id DESC
             LIMIT ?1",
        )?;

        let rows = statement.query_map([n as i64], |row| {
            let commands: String = row.get(1)?;
            let planned_commands = serde_json::from_str::<Vec<String>>(&commands).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    commands.len(),
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;

            Ok(Turn {
                user_input: row.get(0)?,
                planned_commands,
                explanation: row.get(2)?,
                executed: row.get::<_, i64>(3)? != 0,
            })
        })?;

        let mut turns = Vec::new();
        for row in rows {
            turns.push(row?);
        }
        turns.reverse();
        Ok(turns)
    }

    pub fn all_commands(&self) -> Result<Vec<String>> {
        let mut statement = self.conn.prepare(
            "SELECT commands
             FROM turns
             ORDER BY id DESC",
        )?;

        let rows = statement.query_map([], |row| {
            let commands = row.get::<_, Option<String>>(0)?.unwrap_or_default();
            let planned_commands = serde_json::from_str::<Vec<String>>(&commands).map_err(|err| {
                rusqlite::Error::FromSqlConversionFailure(
                    commands.len(),
                    rusqlite::types::Type::Text,
                    Box::new(err),
                )
            })?;
            Ok(planned_commands)
        })?;

        let mut commands = Vec::new();
        for row in rows {
            commands.extend(row?);
        }

        Ok(commands)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS turns (
                id          INTEGER PRIMARY KEY AUTOINCREMENT,
                ts          TEXT NOT NULL,
                user_input  TEXT NOT NULL,
                commands    TEXT,
                explanation TEXT,
                approved    INTEGER NOT NULL
            );",
        )?;
        Ok(())
    }

    fn default_db_path() -> Result<PathBuf> {
        let home = dirs::home_dir().context("failed to resolve home directory")?;
        Ok(home.join(".agentsh").join("history.db"))
    }
}

#[cfg(test)]
fn unique_temp_db_path(test_name: &str) -> Result<PathBuf> {
    use std::time::{SystemTime, UNIX_EPOCH};

    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .context("system clock before unix epoch")?
        .as_nanos();
    Ok(std::env::temp_dir().join(format!("agentsh-{test_name}-{nanos}.db")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_recent_turns() {
        let path = unique_temp_db_path("history-roundtrip").unwrap();
        let db = HistoryDb::open_at(path.clone()).unwrap();
        let turn = Turn {
            user_input: "show me files".to_string(),
            planned_commands: vec!["find . -type f".to_string()],
            executed: true,
            explanation: "Listed files in the current directory.".to_string(),
        };

        db.insert_turn(&turn).unwrap();

        let recent = db.recent(1).unwrap();
        assert_eq!(recent, vec![turn]);

        let _ = fs::remove_file(path);
    }

    #[test]
    fn flattens_all_commands_in_recent_first_order() {
        let path = unique_temp_db_path("history-all-commands").unwrap();
        let db = HistoryDb::open_at(path.clone()).unwrap();

        db.insert_turn(&Turn {
            user_input: "first".to_string(),
            planned_commands: vec!["git status".to_string()],
            executed: true,
            explanation: String::new(),
        })
        .unwrap();

        db.insert_turn(&Turn {
            user_input: "second".to_string(),
            planned_commands: vec!["cargo test".to_string(), "cargo clippy".to_string()],
            executed: true,
            explanation: String::new(),
        })
        .unwrap();

        let commands = db.all_commands().unwrap();
        assert_eq!(commands, vec!["cargo test", "cargo clippy", "git status"]);

        let _ = fs::remove_file(path);
    }
}