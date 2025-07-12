use std::path::PathBuf;
use tokio::sync::mpsc;

use crate::sync_engine;
use sync_engine::FsEventKind;

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

pub async fn start_event_loop(mut reciever: mpsc::Receiver<QueueEvent>) {
    println!("[EVENT_QUEUE] Starting event loop...");

    while let Some(event) = reciever.recv().await {
        match event {
            QueueEvent::FileChanged { path, kind } => {
                println!("[EVENT_QUEUE] File changed: {:?}, kind: {:?}", path, kind);
            }
            QueueEvent::FolderAdded { path } => {
                println!("[EVENT_QUEUE] Folder added: {:?}", path);
            }
            QueueEvent::Shutdown => {
                println!("[EVENT_QUEUE] Shutting down...");
            }
        }
    }
}
