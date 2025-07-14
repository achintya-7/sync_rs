mod database;
use database::Database;

mod event_queue;
use event_queue::EventQueue;
use notify::Watcher;

use std::{path::PathBuf, sync::Arc};

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

    let test_folder = start_test_folder();
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

fn start_test_folder() -> PathBuf {
    let test_folder = PathBuf::from("test");

    if !test_folder.exists() {
        std::fs::create_dir_all(&test_folder).expect("[MAIN] Failed to create test folder");
        println!("[MAIN] Test folder created at: {:?}", test_folder);
    }

    test_folder
}
