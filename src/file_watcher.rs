use crate::event_queue::{EventQueue, QueueEvent};
use crate::sync_engine::FsEventKind;
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use std::path::PathBuf;

/// Starts an async file watcher and forwards events to the event queue.
pub async fn start_file_watcher(folder: PathBuf, event_queue: EventQueue) -> NotifyResult<()> {
    // Use a tokio channel for async communication
    let (tx, mut rx) = tokio::sync::mpsc::channel::<Event>(100);

    // Get a handle to the current Tokio runtime
    let handle = tokio::runtime::Handle::current();

    // The watcher closure runs in notify's thread, but we use the runtime handle
    let mut watcher = RecommendedWatcher::new(
        move |res| {
            if let Ok(event) = res {
                let tx = tx.clone();
                let handle = handle.clone();
                // Spawn the async task using the runtime handle
                handle.spawn(async move {
                    let _ = tx.send(event).await;
                });
            }
        },
        notify::Config::default(),
    )?;

    watcher.watch(&folder, RecursiveMode::Recursive)?;

    println!("[WATCHER] Watching folder: {:?}", folder);

    // Spawn a task to process file events
    let processor_handle = tokio::spawn({
        let event_queue = event_queue.clone();
        async move {
            while let Some(event) = rx.recv().await {
                for path in event.paths {
                    if let Some(q_event) = map_notify_event(path, &event.kind) {
                        let queue = event_queue.clone();
                        queue.send(q_event).await;
                    }
                }
            }
        }
    });

    // Keep the watcher alive by spawning a task that holds onto it
    tokio::spawn(async move {
        // This task holds the watcher and keeps it alive
        let _watcher = watcher;

        // Wait for the processor to finish (which should never happen normally)
        let _ = processor_handle.await;

        println!("[WATCHER] File watcher stopped");
    });

    Ok(())
}

fn map_notify_event(path: PathBuf, kind: &EventKind) -> Option<QueueEvent> {
    match kind {
        EventKind::Modify(_) => Some(QueueEvent::FileChanged {
            path,
            kind: FsEventKind::Modify,
        }),
        EventKind::Create(_) => Some(QueueEvent::FileChanged {
            path,
            kind: FsEventKind::Create,
        }),
        EventKind::Remove(_) => Some(QueueEvent::FileChanged {
            path,
            kind: FsEventKind::Remove,
        }),
        _ => None,
    }
}
