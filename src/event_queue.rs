use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use tokio::sync::mpsc;
use walkdir::WalkDir;

use crate::{
    database,
    sync_engine::{self, calculate_hash},
};
use sync_engine::FsEventKind;
use tokio::sync::Mutex;

#[derive(Debug)]
pub enum QueueEvent {
    FileChanged { path: PathBuf, kind: FsEventKind },
    FolderAdded { path: PathBuf },
    Shutdown,
}

#[derive(Debug, Clone)]
pub struct EventQueue {
    sender: mpsc::Sender<QueueEvent>,
}

impl EventQueue {
    pub fn new(buffer: usize) -> (Self, mpsc::Receiver<QueueEvent>) {
        let (sender, receiver) = mpsc::channel(buffer);
        (EventQueue { sender }, receiver)
    }

    pub async fn send(&self, event: QueueEvent) {
        let _ = self.sender.send(event).await;
    }
}

pub async fn start_event_loop(
    mut receiver: mpsc::Receiver<QueueEvent>,
    db: Arc<Mutex<database::Database>>,
    queue: EventQueue,
) {
    println!("[EVENT_QUEUE] Starting event loop...");

    while let Some(event) = receiver.recv().await {
        match event {
            QueueEvent::FileChanged { path, kind } => {
                handle_file_changed_event(path, kind, &db).await
            }
            QueueEvent::FolderAdded { path } => handle_folder_added_event(path, &db, &queue).await,
            QueueEvent::Shutdown => handle_shutdown_event().await,
        }
    }
}

async fn handle_file_changed_event(
    path: PathBuf,
    kind: FsEventKind,
    db: &Arc<Mutex<database::Database>>,
) {
    println!(
        "[EVENT_QUEUE] Handling file changed event: {:?}, kind: {:?}",
        path, kind
    );
    let db_guard = db.lock().await;

    // 1. Find the parent sync folder for this file path to get its ID.
    let mut parent = path.parent();
    let mut folder_info = None;
    while let Some(current_path) = parent {
        if let Ok(Some(info)) = db_guard.get_folder_by_path(current_path.to_str().unwrap()) {
            folder_info = Some(info);
            break;
        }
        parent = current_path.parent();
    }

    let (folder_id, base_path) = match folder_info {
        Some(info) => info,
        None => {
            eprintln!(
                "[HANDLER] No registered sync folder found for path: {:?}",
                path
            );
            return;
        }
    };

    // 2. Determine the file's path relative to the sync folder root.
    let relative_path = match path.strip_prefix(&base_path) {
        Ok(p) => p,
        Err(_) => {
            eprintln!("[HANDLER] Could not determine relative path for {:?}", path);
            return;
        }
    };

    match kind {
        FsEventKind::Create | FsEventKind::Modify => {
            if !path.is_file() {
                println!("[EVENT_QUEUE] Ignoring non-file event: {:?}", path);
                return;
            }

            let metadata = match path.metadata() {
                Ok(meta) => meta,
                Err(e) => {
                    eprintln!("[EVENT_QUEUE] Failed to get metadata for {:?}: {}", path, e);
                    return;
                }
            };

            let hash = match calculate_hash(&path) {
                Ok(hash) => hash,
                Err(e) => {
                    eprintln!(
                        "[EVENT_QUEUE] Failed to calculate hash for {:?}: {}",
                        path, e
                    );
                    return;
                }
            };

            let file_size = metadata.len();
            let modified_secs = metadata
                .modified()
                .unwrap_or_else(|_| std::time::SystemTime::now())
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as u64;

            if let Err(e) = db_guard.upsert_file_record(
                folder_id,
                relative_path,
                file_size,
                &hash,
                modified_secs,
            ) {
                eprintln!("[HANDLER] DB Error upserting file {:?}: {}", path, e);
            }
        }

        FsEventKind::Remove => {
            if let Err(e) = db_guard.remove_file_entry(folder_id, relative_path) {
                eprintln!("[HANDLER] DB Error deleting file {:?}: {}", path, e);
            }
        }
        _ => {
            println!("[EVENT_QUEUE] Unhandled file event kind: {:?}", kind);
        }
    }
}

async fn handle_folder_added_event(
    path: PathBuf,
    db: &Arc<Mutex<database::Database>>,
    queue: &EventQueue,
) {
    println!("[EVENT_QUEUE] Handling folder added event: {:?}", path);

    let db_guard = db.lock().await;

    // 1. Add the folder to the database.
    let folder_name = path.file_name().unwrap().to_str().unwrap();
    if let Err(e) = db_guard.add_folder(folder_name, path.to_str().unwrap()) {
        eprintln!("[HANDLER] DB Error adding folder {:?}: {}", path, e);
        return;
    }

    drop(db_guard);

    // 2. Scan the folder and add its files by sending events.
    for entry in WalkDir::new(&path).into_iter().filter_map(Result::ok) {
        if entry.file_type().is_file() {
            queue
                .send(QueueEvent::FileChanged {
                    path: entry.path().to_path_buf(),
                    kind: FsEventKind::Create,
                })
                .await;
        }
    }
}

async fn handle_shutdown_event() {
    println!("[EVENT_QUEUE] Handling shutdown event.");
}
