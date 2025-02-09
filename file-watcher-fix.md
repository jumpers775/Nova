# Fix for UI Freezing Issue

## Current Problem
The file watcher is running on the GUI thread via `glib::spawn_future_local` and performing heavy operations (file scanning, metadata extraction) while holding locks. This causes the UI to freeze after ~6.4 seconds.

## Solution Architecture

### 1. Move File Watching to Background Thread

```rust
// In local/mod.rs
pub struct LocalMusicProvider {
    music_dir: PathBuf,
    db: Arc<RwLock<Database>>,
    watcher: Arc<RwLock<FileWatcher>>,
    event_sender: mpsc::Sender<FileEvent>,  // New field
}

impl LocalMusicProvider {
    pub async fn new(music_dir: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        let (event_sender, event_receiver) = mpsc::channel(100);
        
        // Create provider instance
        let provider = Self {
            music_dir,
            db: Arc::new(RwLock::new(Database::new()?)),
            watcher: Arc::new(RwLock::new(FileWatcher::new(music_dir.clone())?)),
            event_sender,
        };
        
        // Start background watcher
        let db = provider.db.clone();
        tokio::spawn(async move {
            while let Ok(event) = event_receiver.recv().await {
                Self::handle_file_event(event, &db).await;
            }
        });
        
        // Initial scan in background
        let db = provider.db.clone();
        let music_dir = provider.music_dir.clone();
        tokio::spawn(async move {
            if let Ok(files) = FileScanner::scan_directory(&music_dir) {
                Self::process_files_batch(&db, files).await;
            }
        });
        
        Ok(provider)
    }
    
    async fn handle_file_event(event: FileEvent, db: &Arc<RwLock<Database>>) {
        match event {
            FileEvent::Created(path) | FileEvent::Modified(path) => {
                if FileScanner::is_music_file_public(&path) {
                    if let Ok(track) = FileScanner::process_file(&path) {
                        let mut db = db.write().await;
                        let _ = db.insert_track(&track);
                    }
                }
            }
            FileEvent::Removed(path) => {
                let mut db = db.write().await;
                let _ = db.remove_track_by_path(&path);
            }
        }
    }
    
    async fn process_files_batch(db: &Arc<RwLock<Database>>, files: Vec<PathBuf>) {
        for chunk in files.chunks(5) {
            let mut tracks = Vec::with_capacity(chunk.len());
            for file in chunk {
                if let Ok(track) = FileScanner::process_file(file) {
                    tracks.push(track);
                }
            }
            
            if !tracks.is_empty() {
                if let Ok(db) = db.write().await {
                    let _ = db.batch_insert_tracks(&tracks);
                }
            }
            
            // Yield to allow other tasks to run
            tokio::task::yield_now().await;
        }
    }
}
```

### 2. Optimize File Processing

```rust
// In scanner.rs
impl FileScanner {
    pub fn process_file(path: &Path) -> Result<Track, Box<dyn Error + Send + Sync>> {
        // Add yielding points for heavy operations
        if !path.exists() {
            return Err("File not found".into());
        }
        
        tokio::task::yield_now().await;
        
        // Generate ID and basic file info
        let id = Self::generate_file_id(path)?;
        let (file_size, file_format) = Self::get_file_info(path)?;
        
        tokio::task::yield_now().await;
        
        // Extract metadata
        let metadata = Self::extract_metadata(path)?;
        
        tokio::task::yield_now().await;
        
        // Find artwork separately
        let artwork = Self::find_artwork(path)?;
        
        Ok(Track {
            id,
            title: metadata.title,
            artist: metadata.artist,
            album: metadata.album,
            duration: metadata.duration,
            track_number: metadata.track_number,
            disc_number: metadata.disc_number,
            release_year: metadata.release_year,
            genre: metadata.genre,
            artwork,
            source: PlaybackSource::Local {
                file_format,
                file_size,
                path: path.to_path_buf(),
            },
        })
    }
}
```

### 3. Update File Watcher Implementation

```rust
// In watcher.rs
impl FileWatcher {
    pub fn new(path: PathBuf, event_sender: mpsc::Sender<FileEvent>) -> notify::Result<Self> {
        let (tx, rx) = bounded(100);
        
        let mut watcher = notify::recommended_watcher(move |res| {
            if let Ok(event) = res {
                // Process events in background
                let tx = tx.clone();
                tokio::spawn(async move {
                    for path in event.paths {
                        if let Some(event) = Self::process_event(event.kind, path) {
                            let _ = tx.send(event);
                        }
                    }
                });
            }
        })?;
        
        watcher.watch(&path, RecursiveMode::Recursive)?;
        
        Ok(Self {
            _watcher: watcher,
            receiver: Arc::new(rx),
        })
    }
}
```

## Implementation Steps

1. Create new channel-based file event handling
2. Move file watcher to background thread
3. Add yielding points in file processing
4. Update LocalMusicProvider initialization
5. Test UI responsiveness
6. Verify file watching still works

## Success Metrics

1. UI remains responsive after launch
2. File changes are still detected and processed
3. Memory usage remains stable
4. CPU usage is distributed across cores
5. No visible lag in playback controls

Would you like me to proceed with implementing these changes?