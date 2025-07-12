mod database;
use database::Database;

mod event_queue;
use event_queue::{EventQueue, QueueEvent};

use crate::{event_queue::start_event_loop, sync_engine::FsEventKind};
use std::path::PathBuf;
pub mod sync_engine;

#[tokio::main]
async fn main() {
    let db = Database::new().expect("Failed to initialize database");
    println!("[MAIN] Database initialized successfully.");

    let device_id = db
        .get_or_create_device_id()
        .expect("[MAIN] Failed to get or create device ID");

    println!("[MAIN] Device ID: {}", device_id);

    let (queue, receiver) = EventQueue::new(100);

    let event_loop_handle = tokio::spawn(event_queue::start_event_loop(receiver));

    // Send test events
    send_test_events(&queue).await;

    let _ = event_loop_handle.await;
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
