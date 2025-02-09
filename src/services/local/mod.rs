mod audio;
mod database;
mod scanner;
mod watcher;

use super::error::ServiceError;
use super::models::{Artwork, ArtworkSource, PlaybackSource, SearchWeights};
use super::traits::MusicProvider;
use crate::services::models::{Album, Artist, PlayableItem, SearchResults, Track};

use crate::services::local::database::Database;
use crate::services::local::scanner::FileScanner;
use crate::services::local::watcher::{FileEvent, FileWatcher};
use async_trait::async_trait;
use chrono::Utc;
use crossbeam_channel::RecvTimeoutError;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use gtk::glib;
use gtk::prelude::*;
use notify;
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rayon::prelude::*;
use rusqlite::{params, OptionalExtension};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::{mpsc, RwLock};

pub use audio::LocalAudioBackend;

#[derive(Debug, Clone)]
pub struct LocalMusicProvider {
    music_dir: PathBuf,
    db: Arc<RwLock<Database>>,
    event_sender: mpsc::Sender<FileEvent>,
}

impl LocalMusicProvider {
    pub async fn new(music_dir: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        println!(
            "Initializing LocalMusicProvider with directory: {:?}",
            music_dir
        );

        // Create channels for file events
        let (event_sender, mut event_receiver) = mpsc::channel(100);

        // Create database and watcher
        let db = Arc::new(RwLock::new(Database::new()?));
        let _watcher = FileWatcher::new(music_dir.clone(), event_sender.clone())?;

        let provider = Self {
            music_dir: music_dir.clone(),
            db: db.clone(),
            event_sender,
        };

        // Start background event processor
        let db_clone = db.clone();
        tokio::spawn(async move {
            println!("Starting file event processor");
            while let Some(event) = event_receiver.recv().await {
                Self::handle_file_event(&event, &db_clone).await;
            }
        });

        // Start initial scan in background
        let db_clone = db.clone();
        tokio::spawn(async move {
            println!("Starting music directory scan...");
            if let Ok(files) = FileScanner::scan_directory(&music_dir) {
                println!("Found {} music files", files.len());
                Self::process_files_batch(&files, &db_clone).await;
            }
        });

        Ok(provider)
    }

    pub async fn rescan_library(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("Rescanning music directory: {:?}", self.music_dir);

        // Scan files
        let files = FileScanner::scan_directory(&self.music_dir)?;
        println!("Found {} music files", files.len());

        // Process files in background
        Self::process_files_batch(&files, &self.db).await;
        println!("Rescan complete");

        Ok(())
    }

    async fn handle_file_event(event: &FileEvent, db: &Arc<RwLock<Database>>) {
        match event {
            FileEvent::Created(path) | FileEvent::Modified(path) => {
                if FileScanner::is_music_file_public(path) {
                    tokio::task::yield_now().await;
                    if let Ok(track) = FileScanner::process_file(path).await {
                        let mut db = db.write().await;
                        if let Err(e) = db.insert_track(&track) {
                            eprintln!("Error inserting track: {}", e);
                        }
                    }
                }
            }
            FileEvent::Removed(path) => {
                if path.extension().map_or(false, |ext| {
                    matches!(
                        ext.to_str().unwrap_or("").to_lowercase().as_str(),
                        "mp3" | "flac" | "m4a" | "ogg" | "wav"
                    )
                }) {
                    let mut db = db.write().await;
                    if let Err(e) = db.remove_track_by_path(path) {
                        eprintln!("Error removing track: {}", e);
                    }
                }
            }
        }
    }

    async fn process_files_batch(files: &[PathBuf], db: &Arc<RwLock<Database>>) {
        for chunk in files.chunks(5) {
            let mut tracks = Vec::with_capacity(chunk.len());
            
            for file in chunk {
                tokio::task::yield_now().await;
                if let Ok(track) = FileScanner::process_file(file).await {
                    tracks.push(track);
                }
            }

            if !tracks.is_empty() {
                let mut db = db.write().await;
                if let Err(e) = db.batch_insert_tracks(&tracks) {
                    eprintln!("Error inserting tracks batch: {}", e);
                }
            }
            
            // Yield to allow other tasks to run
            tokio::task::yield_now().await;
        }
    }
}

#[async_trait]
impl MusicProvider for LocalMusicProvider {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.get_all_tracks()
    }

    async fn get_artists(&self) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.get_all_artists()
    }

    async fn get_albums(&self) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.get_all_albums()
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>> {
        self.search_tracks(query, limit, offset).await
    }

    async fn search_tracks(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.search_tracks(query, limit, offset)
    }

    async fn search_albums(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.search_albums(query, limit, offset)
    }

    async fn search_artists(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;
        db.search_artists(query, limit, offset)
    }

    async fn search_all(
        &self,
        query: &str,
        weights: &SearchWeights,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResults, Box<dyn Error + Send + Sync>> {
        let db = self.db.read().await;

        let tracks = db.search_tracks(query, limit, offset)?;
        let albums = db.search_albums(query, limit, offset)?;
        let artists = db.search_artists(query, limit, offset)?;

        Ok(SearchResults {
            tracks: tracks
                .into_iter()
                .map(|track| PlayableItem {
                    track,
                    provider: "local".to_string(),
                    added_at: chrono::Utc::now(),
                })
                .collect(),
            albums,
            artists,
        })
    }
}
