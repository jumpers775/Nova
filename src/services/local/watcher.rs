use crossbeam_channel::{bounded, Sender};
use gtk::glib;
use notify::{
    event::{CreateKind, ModifyKind, RemoveKind},
    Config, Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use parking_lot::RwLock;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum FileEvent {
    Created(PathBuf),
    Modified(PathBuf),
    Removed(PathBuf),
}

#[derive(Debug)]
pub struct FileWatcher {
    _watcher: RecommendedWatcher,
    receiver: Arc<crossbeam_channel::Receiver<FileEvent>>,
}

impl FileWatcher {
    pub fn new(path: PathBuf) -> notify::Result<Self> {
        let (tx, rx) = bounded(100);
        let tx = Arc::new(tx);

        println!("Initializing file watcher for path: {:?}", path);

        let tx_clone = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                println!("Raw watcher event: {:?}", event);

                for path in event.paths {
                    let event = match event.kind {
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
                        // Schedule the event processing on the main thread
                        let tx = tx_clone.clone();
                        glib::idle_add_local_once(move || {
                            let _ = tx.send(event);
                        });
                    }
                }
            } else if let Err(e) = res {
                eprintln!("Watch error: {:?}", e);
            }
        })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;
        println!("File watcher initialized successfully");

        Ok(Self {
            _watcher: watcher,
            receiver: Arc::new(rx),
        })
    }

    pub fn try_receive(&self) -> Option<FileEvent> {
        self.receiver.try_recv().ok()
    }
}

impl Clone for FileWatcher {
    fn clone(&self) -> Self {
        Self {
            _watcher: notify::recommended_watcher(|_| {}).expect("Failed to create watcher"),
            receiver: self.receiver.clone(),
        }
    }
}
