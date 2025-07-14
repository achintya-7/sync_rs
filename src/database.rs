use rusqlite::{Connection, Result, params};
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use crate::sync_engine::FileEntry;

const DB_PATH: &str = "sync_rs.db";

#[derive(Debug)]
pub struct Database {
    conn: rusqlite::Connection,
}

impl Database {
    pub fn new() -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(DB_PATH)?;
        let db = Self { conn };
        db.initialize()?;
        Ok(db)
    }

    fn initialize(&self) -> Result<(), rusqlite::Error> {
        self.conn.execute_batch(
            "BEGIN;
            CREATE TABLE IF NOT EXISTS settings (
                key TEXT PRIMARY KEY,
                value TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS synced_folders (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                name TEXT NOT NULL UNIQUE,
                local_path TEXT NOT NULL UNIQUE,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP
            );

            CREATE TABLE IF NOT EXISTS file_index (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                folder_id INTEGER NOT NULL,
                relative_path TEXT NOT NULL,
                last_modified_secs INTEGER NOT NULL,
                size_bytes INTEGER NOT NULL,
                sha256_hash TEXT,
                version INTEGER NOT NULL DEFAULT 1,
                last_synced_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                UNIQUE(folder_id, relative_path),
                FOREIGN KEY(folder_id) REFERENCES synced_folders(id) ON DELETE CASCADE
            );

            COMMIT;",
        )?;

        Ok(())
    }

    pub fn get_or_create_device_id(&self) -> Result<String, rusqlite::Error> {
        let query: &str = "SELECT value FROM settings WHERE key = 'device_id'";
        let query_result = self.conn.query_row(query, [], |row| row.get(0));

        match query_result {
            Ok(device_id) => Ok(device_id),

            Err(rusqlite::Error::QueryReturnedNoRows) => {
                let new_device_id = uuid::Uuid::new_v4().to_string();

                self.conn.execute(
                    "INSERT INTO settings (key, value) VALUES (?1, ?2)",
                    rusqlite::params!["device_id", new_device_id],
                )?;

                Ok(new_device_id)
            }

            Err(e) => Err(e),
        }
    }

    pub fn add_folder(&self, name: &str, path: &str) -> Result<i64, rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO synced_folders (name, local_path) VALUES (?1, ?2)",
            rusqlite::params![name, path],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn get_folders_and_files(
        &self,
        folder_id: i64,
        folder_base_path: &Path,
    ) -> Result<HashMap<PathBuf, FileEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT relative_path, last_modified_secs, size_bytes, sha256_hash, version
             FROM file_index WHERE folder_id = ?1",
        )?;

        let rows = stmt.query_map(params![folder_id], |row| {
            let relattive_path_str: String = row.get(0)?;
            let relative_path = PathBuf::from(relattive_path_str);
            let absolute_path = PathBuf::from(folder_base_path).join(&relative_path);
            let last_modified_secs: i64 = row.get(1)?;

            Ok((
                absolute_path.clone(),
                FileEntry {
                    path: absolute_path,
                    last_modified: std::time::UNIX_EPOCH
                        + std::time::Duration::from_secs(last_modified_secs as u64),
                    size: row.get(2)?,
                    hash: row.get(3)?,
                    version: row.get(4)?,
                },
            ))
        })?;

        let mut files_map = HashMap::new();
        for file in rows {
            let (path, file_entry) = file?;
            files_map.insert(path, file_entry);
        }

        Ok(files_map)
    }

    pub fn get_folder_by_path(
        &self,
        path_str: &str,
    ) -> Result<Option<(i64, PathBuf)>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, local_path FROM synced_folders WHERE local_path = ?1")?;
        let mut rows = stmt.query_map(params![path_str], |row| {
            Ok((row.get(0)?, row.get::<_, String>(1)?.into()))
        })?;

        match rows.next() {
            Some(Ok(row)) => Ok(Some(row)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub fn get_all_synced_folders(&self) -> Result<Vec<(i64, PathBuf)>, rusqlite::Error> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, local_path FROM synced_folders")?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get::<_, String>(1)?.into())))?;

        let mut folders = Vec::new();
        for row in rows {
            folders.push(row?);
        }
        Ok(folders)
    }

    pub fn upsert_file_record(
        &self,
        folder_id: i64,
        relative_path: &Path,
        size_bytes: u64,
        sha256_hash: &str,
        modified_secs: u64,
    ) -> Result<(), rusqlite::Error> {
        self.conn.execute(
            "INSERT INTO file_index (folder_id, relative_path, last_modified_secs, size_bytes, sha256_hash, version)
             VALUES (?1, ?2, ?3, ?4, ?5, 1)
             ON CONFLICT(folder_id, relative_path) DO UPDATE SET
                last_modified_secs = excluded.last_modified_secs,
                size_bytes = excluded.size_bytes,
                sha256_hash = excluded.sha256_hash,
                version = version + 1,
                last_synced_at = CURRENT_TIMESTAMP",
            params![
                folder_id,
                relative_path.to_str().expect("Path contains invalid UTF-8"),
                modified_secs,
                size_bytes,
                sha256_hash
            ],
        )?;
        Ok(())
    }

    pub fn remove_file_entry(&self, folder_id: i64, file_name: &Path) -> Result<()> {
        self.conn.execute(
            "DELETE FROM file_index WHERE folder_id = ?1 AND relative_path = ?2",
            params![folder_id, file_name.to_str().unwrap()],
        )?;
        Ok(())
    }
}
