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
use rusqlite::params;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::RwLock;

#[derive(Debug, Clone)]
pub struct LocalMusicProvider {
    music_dir: PathBuf,
    db: Arc<RwLock<Database>>,
    watcher: Arc<RwLock<FileWatcher>>,
}

impl LocalMusicProvider {
    pub fn new(music_dir: PathBuf) -> Result<Self, Box<dyn Error + Send + Sync>> {
        println!(
            "Initializing LocalMusicProvider with directory: {:?}",
            music_dir
        );

        println!("Creating persistent database");
        let db = Database::new()?;
        let watcher = FileWatcher::new(music_dir.clone())?;

        // Create provider first
        let provider = Self {
            music_dir,
            db: Arc::new(RwLock::new(db)),
            watcher: Arc::new(RwLock::new(watcher)),
        };

        // Scan files in parallel
        println!("Starting music directory scan...");
        let files = FileScanner::scan_directory(&provider.music_dir)?;
        println!("Found {} music files", files.len());

        // Process files in chunks to avoid overwhelming the database
        const CHUNK_SIZE: usize = 50;

        for chunk in files.chunks(CHUNK_SIZE) {
            let tracks: Vec<_> = chunk
                .par_iter()
                .filter_map(|file| match FileScanner::process_file(file) {
                    Ok(track) => Some(track),
                    Err(e) => {
                        eprintln!("Error processing file {:?}: {}", file, e);
                        None
                    }
                })
                .collect();

            // Insert chunk of tracks
            if let Err(e) = provider.db.blocking_read().batch_insert_tracks(&tracks) {
                eprintln!("Error inserting tracks: {}", e);
            }
        }

        println!("Successfully processed all tracks");

        // Start watching for changes
        let provider_clone = provider.clone();
        glib::MainContext::default().spawn_local(async move {
            loop {
                let watcher = provider_clone.watcher.read().await;
                match watcher.try_receive() {
                    Some(FileEvent::Created(path)) | Some(FileEvent::Modified(path)) => {
                        if FileScanner::is_music_file_public(&path) {
                            match FileScanner::process_file(&path) {
                                Ok(track) => {
                                    let mut db = provider_clone.db.write().await;
                                    if let Err(e) = db.insert_track(&track) {
                                        eprintln!("Error updating track in database: {}", e);
                                    }
                                }
                                Err(e) => {
                                    eprintln!("Error processing file {:?}: {}", path, e);
                                }
                            }
                        }
                    }
                    Some(FileEvent::Removed(path)) => {
                        let mut db = provider_clone.db.write().await;
                        if let Err(e) = db.remove_track_by_path(&path) {
                            eprintln!("Error removing track from database: {}", e);
                        }
                    }
                    None => {
                        // No events to process, sleep for a longer time
                        glib::timeout_future(Duration::from_millis(500)).await;
                    }
                }
            }
        });

        Ok(provider)
    }

    pub async fn rescan_library(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        println!("Rescanning music directory: {:?}", self.music_dir);

        // Create new database
        let mut new_db = Database::new()?;

        // Scan files
        let files = FileScanner::scan_directory(&self.music_dir)?;
        println!("Found {} music files", files.len());

        // Process files in parallel batches
        let tracks: Vec<_> = files
            .par_iter()
            .filter_map(|file| match FileScanner::process_file(file) {
                Ok(track) => Some(track),
                Err(e) => {
                    eprintln!("Error processing file {:?}: {}", file, e);
                    None
                }
            })
            .collect();

        // Batch insert tracks
        new_db.batch_insert_tracks(&tracks)?;

        // Replace old database with new one
        let mut db = self.db.write().await;
        *db = new_db;
        println!("Successfully processed {} tracks", tracks.len());

        Ok(())
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
