use crossbeam_channel::{bounded, Sender};
use notify::{
    Event, EventKind, RecommendedWatcher, RecursiveMode, Result as NotifyResult, Watcher,
};
use std::path::PathBuf;
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
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

        let tx_clone = tx.clone();
        let mut watcher = notify::recommended_watcher(move |res: NotifyResult<Event>| {
            if let Ok(event) = res {
                match event.kind {
                    EventKind::Create(_) => {
                        for path in event.paths {
                            if path.exists() {
                                let _ = tx_clone.send(FileEvent::Created(path));
                            }
                        }
                    }
                    EventKind::Modify(_) => {
                        for path in event.paths {
                            if path.exists() {
                                let _ = tx_clone.send(FileEvent::Modified(path));
                            }
                        }
                    }
                    EventKind::Remove(_) => {
                        for path in event.paths {
                            let _ = tx_clone.send(FileEvent::Removed(path));
                        }
                    }
                    _ => {} // Ignore other events
                }
            }
        })?;

        watcher.watch(&path, RecursiveMode::Recursive)?;

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
