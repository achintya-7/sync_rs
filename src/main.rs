mod database;
use database::Database;

mod event_queue;
use event_queue::{EventQueue, QueueEvent};
use notify::{Event as NotifyEvent, RecursiveMode, Result as NotifyResult, Watcher};

use crate::sync_engine::FsEventKind;
use std::{
    path::{Path, PathBuf},
    sync::{Arc, mpsc},
};

use tokio::sync::Mutex as TokioMutex;

pub mod sync_engine;

mod file_watcher;

#[tokio::main]
async fn main() {
    let db = Arc::new(TokioMutex::new(
        Database::new().expect("Failed to initialize database"),
    ));
    println!("[MAIN] Database initialized successfully.");

    let device_id = db
        .lock()
        .await
        .get_or_create_device_id()
        .expect("[MAIN] Failed to get or create device ID");

    println!("[MAIN] Device ID: {}", device_id);

    let (queue, receiver) = EventQueue::new(100);

    let event_loop_handle = tokio::spawn(event_queue::start_event_loop(
        receiver,
        db.clone(),
        queue.clone(),
    ));

    let test_folder = PathBuf::from("test");
    file_watcher::start_file_watcher(test_folder, queue.clone())
        .await
        .expect("[MAIN] Failed to start file watcher");

    println!("[MAIN] File watcher started. Waiting for events... (Press Ctrl+C to exit)");

    // Wait for the event loop to finish (which won't happen unless there's an error)
    // This keeps the program running indefinitely
    if let Err(e) = event_loop_handle.await {
        eprintln!("[MAIN] Event loop error: {:?}", e);
    }
}

// Send a test event
async fn send_test_events(queue: &EventQueue) {
    queue
        .send(QueueEvent::FileChanged {
            path: PathBuf::from("test_sync/file.txt"),
            kind: FsEventKind::Modify,
        })
        .await;

    queue
        .send(QueueEvent::FolderAdded {
            path: PathBuf::from("test_sync"),
        })
        .await;

    queue.send(QueueEvent::Shutdown).await;
}

fn notify_test() -> NotifyResult<()> {
    let (tx, rx) = mpsc::channel::<NotifyResult<NotifyEvent>>();

    // Use recommended_watcher() to automatically select the best implementation
    // for your platform. The `EventHandler` passed to this constructor can be a
    // closure, a `std::sync::mpsc::Sender`, a `crossbeam_channel::Sender`, or
    // another type the trait is implemented for.
    let mut watcher = notify::recommended_watcher(tx)?;

    // Add a path to be watched. All files and directories at that path and
    // below will be monitored for changes.
    watcher.watch(Path::new("test"), RecursiveMode::Recursive)?;
    // Block forever, printing out events as they come in
    for res in rx {
        match res {
            Ok(event) => println!("event: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

    Ok(())
}
