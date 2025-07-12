use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs;
use std::fs::File;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use walkdir::WalkDir;

#[derive(Debug, Clone)]
pub enum FsEventKind {
    Create,
    Modify,
    Remove,
    Rename {
        old_path: PathBuf,
        new_path: PathBuf,
    },
}

/// Represents a file or directory entry with metadata.
#[derive(Debug, Clone)]
pub struct FileEntry {
    pub path: PathBuf,
    pub last_modified: SystemTime,
    pub size: u64,
    pub hash: Option<String>, // Placeholder for file hash
    pub version: u64,
}

/// Represents a folder being synced.
#[derive(Debug)]
pub struct SyncFolder {
    pub id: String, // In production, use a UUID
    pub name: String,
    pub path: PathBuf,
    pub files: HashMap<PathBuf, FileEntry>,
}

impl SyncFolder {
    pub fn new(id: String, name: String, path: PathBuf) -> Self {
        Self {
            id,
            name,
            path,
            files: HashMap::new(),
        }
    }
}

/// The SyncEngine manages all syncing logic and state.
pub struct SyncEngine {
    pub folders: Vec<SyncFolder>,
}

impl SyncEngine {
    pub fn new() -> Self {
        Self {
            folders: Vec::new(),
        }
    }

    pub fn add_folder(&mut self, folder: SyncFolder) -> io::Result<()> {
        self.folders.push(folder);
        Ok(())
    }

    /// Scans a folder, updating its file map with metadata and hashes.
    /// Returns a Result for error handling.
    pub fn scan_folder(&mut self, folder_index: usize) -> io::Result<()> {
        let folder = &mut self.folders[folder_index];
        folder.files.clear();

        for entry in WalkDir::new(&folder.path)
            .into_iter()
            .filter_map(Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let path = entry.path().to_path_buf();
            let meta = entry.metadata()?;
            let hash = Some(calculate_hash(&path)?);
            let last_modified = meta.modified().unwrap_or(UNIX_EPOCH);
            let size = meta.len();

            let file_entry = FileEntry {
                path: path.clone(),
                last_modified,
                size,
                hash,
                version: 1, // starts with version 1
            };

            folder.files.insert(path, file_entry);
        }

        Ok(())
    }

    pub fn handle_fs_change(
        &mut self,
        folder_index: usize,
        event_path: &Path,
        event_kind: FsEventKind,
    ) -> io::Result<()> {
        let folder = &mut self.folders[folder_index];

        match event_kind {
            FsEventKind::Create | FsEventKind::Modify => {
                if event_path.is_file() {
                    let meta = fs::metadata(event_path)?;
                    let hash = Some(calculate_hash(event_path)?);
                    let last_modified = meta.modified().unwrap_or(UNIX_EPOCH);
                    let size = meta.len();

                    let file_entry = FileEntry {
                        path: event_path.to_path_buf(),
                        last_modified,
                        size,
                        hash,
                        version: 1,
                    };

                    folder.files.insert(event_path.to_path_buf(), file_entry);
                }
            }
            FsEventKind::Remove => {
                folder.files.remove(event_path);
            }
            FsEventKind::Rename { old_path, new_path } => {
                if let Some(file_entry) = folder.files.remove(&old_path) {
                    let mut new_entry = file_entry.clone();
                    new_entry.path = new_path.clone();
                    folder.files.insert(new_path, new_entry);
                }
            }
        }
        Ok(())
    }
}

pub fn calculate_hash(file_path: &Path) -> io::Result<String> {
    let mut file = File::open(file_path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0; 8192]; // 8KB buffer for reading the file

    loop {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read == 0 {
            break;
        }
        hasher.update(&buffer[..bytes_read]);
    }

    Ok(format!("{:x}", hasher.finalize()))
}
