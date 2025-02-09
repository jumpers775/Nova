use gtk::glib;
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;

#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
}

#[derive(Debug)]
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    event_sender: mpsc::Sender<FileEvent>,
}

impl FileWatcher {
    pub fn new(path: PathBuf, event_sender: mpsc::Sender<FileEvent>) -> notify::Result<Self> {
        println!("Initializing file watcher for path: {:?}", path);

        let event_sender_clone = event_sender.clone();
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                println!("Raw watcher event: {:?}", event);

                // Process events in background
                let event_sender = event_sender_clone.clone();
                let paths = event.paths.clone();
                let kind = event.kind.clone();
                
                tokio::spawn(async move {
                    for path in paths {
                        let event = match kind {
                            EventKind::Create(_) => {
                                if path.exists() {
                                    Some(FileEvent::Created(path))
                                } else {
                                    None
                                }
                            }
                            EventKind::Modify(_) => {
                                if path.exists() {
                                    Some(FileEvent::Modified(path))
                                } else {
                                    Some(FileEvent::Removed(path))
                                }
                            }
                            EventKind::Remove(_) => Some(FileEvent::Removed(path)),
                            _ => None,
                        };

                        if let Some(event) = event {
                            let _ = event_sender.send(event).await;
                        }
                    }
                });
            } else if let Err(e) = res {
                eprintln!("Watch error: {:?}", e);
            }
        })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;
        println!("File watcher initialized successfully");

        Ok(Self {
            _watcher: watcher,
            event_sender,
        })
    }

}

// FileWatcher is not Clone anymore since it owns a unique event sender
