use crate::services::models::{Album, Artist, Artwork, ArtworkSource, PlaybackSource, Track};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::params;
use sha1::{Digest, Sha1};
use std::error::Error;
use std::path::Path;
use std::sync::Arc;

#[derive(Debug)]
pub struct Database {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

impl Database {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Get application data directory
        let data_dir = dirs::data_dir()
            .ok_or("Could not find data directory")?
            .join("nova");

        // Create directory if it doesn't exist
        std::fs::create_dir_all(&data_dir)?;

        let db_path = data_dir.join("library.db");
        println!("Using database at: {:?}", db_path);

        // Initialize the database with optimized settings
        {
            let conn = rusqlite::Connection::open(&db_path)?;
            conn.execute_batch(
                "PRAGMA journal_mode = WAL;
                 PRAGMA synchronous = NORMAL;
                 PRAGMA temp_store = MEMORY;
                 PRAGMA mmap_size = 30000000000;
                 PRAGMA page_size = 4096;
                 PRAGMA cache_size = -2000;
                 PRAGMA busy_timeout = 10000;",
            )?;
        }

        // Create SQLite connection manager with the optimized database
        let manager = SqliteConnectionManager::file(&db_path).with_flags(
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE,
        );

        // Create pool with appropriate size and timeout
        let pool = Pool::builder().max_size(4).build(manager)?;

        let db = Self {
            pool: Arc::new(pool),
        };

        db.initialize()?;
        println!("Database initialized");
        Ok(db)
    }

    fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Initializing database tables and indexes");
        let mut conn = self.pool.get()?;

        conn.execute_batch(
            "
            CREATE TABLE IF NOT EXISTS tracks (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                album TEXT NOT NULL,
                duration INTEGER NOT NULL,
                track_number INTEGER,
                disc_number INTEGER,
                release_year INTEGER,
                genre TEXT,
                file_path TEXT NOT NULL,
                file_format TEXT NOT NULL,
                file_size INTEGER NOT NULL,
                artwork_data BLOB,
                artwork_path TEXT
            );

            CREATE TABLE IF NOT EXISTS albums (
                id TEXT PRIMARY KEY,
                title TEXT NOT NULL,
                artist TEXT NOT NULL,
                year INTEGER,
                UNIQUE(title, artist)
            );

            CREATE TABLE IF NOT EXISTS artists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE
            );

            CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
            CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
            CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
            CREATE INDEX IF NOT EXISTS idx_albums_title ON albums(title);
            CREATE INDEX IF NOT EXISTS idx_artists_name ON artists(name);
            CREATE INDEX IF NOT EXISTS idx_tracks_search ON tracks(title, artist, album);
            CREATE INDEX IF NOT EXISTS idx_albums_search ON albums(title, artist);
            CREATE INDEX IF NOT EXISTS idx_artists_search ON artists(name);
        ",
        )?;

        println!("Created all tables and indexes");

        Ok(())
    }

    pub fn search_tracks(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
        println!(
            "Searching tracks with query: '{}' (limit: {}, offset: {})",
            query, limit, offset
        );
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare(
            "SELECT * FROM tracks
            WHERE title LIKE ?1 OR artist LIKE ?1 OR album LIKE ?1
            LIMIT ?2 OFFSET ?3",
        )?;

        let search_pattern = format!("%{}%", query);
        println!("Using search pattern: {}", search_pattern);
        let tracks: Vec<Track> = stmt
            .query_map(
                params![search_pattern, limit as i64, offset as i64],
                |row| {
                    Ok(Track {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        album: row.get(3)?,
                        duration: row.get(4)?,
                        track_number: row.get(5)?,
                        disc_number: row.get(6)?,
                        release_year: row.get(7)?,
                        genre: row.get(8)?,
                        artwork: Artwork {
                            thumbnail: row.get(12)?,
                            full_art: match row.get::<_, String>(13)? {
                                path if !path.is_empty() => ArtworkSource::Local {
                                    path: Path::new(&path).to_path_buf(),
                                },
                                _ => ArtworkSource::None,
                            },
                        },
                        source: PlaybackSource::Local {
                            file_format: row.get(10)?,
                            file_size: row.get(11)?,
                            path: Path::new(&row.get::<_, String>(9)?).to_path_buf(),
                        },
                    })
                },
            )?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} tracks", tracks.len());
        Ok(tracks)
    }

    pub fn get_all_tracks(&self) -> Result<Vec<Track>, Box<dyn std::error::Error + Send + Sync>> {
        println!("Getting all tracks");
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare("SELECT * FROM tracks")?;
        let tracks: Vec<Track> = stmt
            .query_map([], |row| {
                Ok(Track {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    album: row.get(3)?,
                    duration: row.get(4)?,
                    track_number: row.get(5)?,
                    disc_number: row.get(6)?,
                    release_year: row.get(7)?,
                    genre: row.get(8)?,
                    artwork: Artwork {
                        thumbnail: row.get(12)?,
                        full_art: match row.get::<_, String>(13)? {
                            path if !path.is_empty() => ArtworkSource::Local {
                                path: Path::new(&path).to_path_buf(),
                            },
                            _ => ArtworkSource::None,
                        },
                    },
                    source: PlaybackSource::Local {
                        file_format: row.get(10)?,
                        file_size: row.get(11)?,
                        path: Path::new(&row.get::<_, String>(9)?).to_path_buf(),
                    },
                })
            })?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} total tracks", tracks.len());
        Ok(tracks)
    }

    pub fn insert_artist(
        &self,
        artist: &Artist,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Inserting artist: {}", artist.name);
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        conn.execute(
            "INSERT OR REPLACE INTO artists (id, name) VALUES (?, ?)",
            params![artist.id, artist.name],
        )?;
        Ok(())
    }

    pub fn insert_album(
        &self,
        album: &Album,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Inserting album: {} by {}", album.title, album.artist);
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        conn.execute(
            "INSERT OR REPLACE INTO albums (id, title, artist, year) VALUES (?, ?, ?, ?)",
            params![album.id, album.title, album.artist, album.year],
        )?;
        Ok(())
    }

    pub fn get_all_artists(&self) -> Result<Vec<Artist>, Box<dyn std::error::Error + Send + Sync>> {
        println!("Getting all artists");
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare("SELECT * FROM artists WHERE name != 'Unknown Artist'")?;
        let artists: Vec<Artist> = stmt
            .query_map([], |row| {
                Ok(Artist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    albums: Vec::new(), // Albums will be populated separately
                })
            })?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} total artists", artists.len());
        Ok(artists)
    }

    pub fn get_all_albums(&self) -> Result<Vec<Album>, Box<dyn std::error::Error + Send + Sync>> {
        println!("Getting all albums");
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare("SELECT * FROM albums WHERE title != 'Unknown Album'")?;
        let albums: Vec<Album> = stmt
            .query_map([], |row| {
                Ok(Album {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    year: row.get(3)?,
                    art_url: None,
                    tracks: Vec::new(), // Tracks will be populated separately
                })
            })?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} total albums", albums.len());
        Ok(albums)
    }

    pub fn search_artists(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Artist>, Box<dyn std::error::Error + Send + Sync>> {
        println!("Searching artists with query: {}", query);
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare(
            "SELECT * FROM artists
             WHERE name LIKE ?1
             AND name != 'Unknown Artist'
             LIMIT ?2 OFFSET ?3",
        )?;

        let search_pattern = format!("%{}%", query);
        println!("Using search pattern: {}", search_pattern);
        let artists: Vec<Artist> = stmt
            .query_map(
                params![search_pattern, limit as i64, offset as i64],
                |row| {
                    Ok(Artist {
                        id: row.get(0)?,
                        name: row.get(1)?,
                        albums: Vec::new(),
                    })
                },
            )?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} artists", artists.len());
        Ok(artists)
    }

    pub fn search_albums(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Album>, Box<dyn std::error::Error + Send + Sync>> {
        println!("Searching albums with query: {}", query);
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;
        let mut stmt = conn.prepare(
            "SELECT * FROM albums
             WHERE (title LIKE ?1 OR artist LIKE ?1)
             AND title != 'Unknown Album'
             LIMIT ?2 OFFSET ?3",
        )?;

        let search_pattern = format!("%{}%", query);
        println!("Using search pattern: {}", search_pattern);
        let albums: Vec<Album> = stmt
            .query_map(
                params![search_pattern, limit as i64, offset as i64],
                |row| {
                    Ok(Album {
                        id: row.get(0)?,
                        title: row.get(1)?,
                        artist: row.get(2)?,
                        year: row.get(3)?,
                        art_url: None,
                        tracks: Vec::new(),
                    })
                },
            )?
            .filter_map(Result::ok)
            .collect();

        println!("Found {} albums", albums.len());
        Ok(albums)
    }

    fn ensure_artist(&self, artist: &str) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;

        // Create SHA1 hash properly
        let mut hasher = Sha1::new();
        hasher.update(artist.as_bytes());
        let artist_id = format!("{:x}", hasher.finalize());

        conn.execute(
            "INSERT OR IGNORE INTO artists (id, name) VALUES (?, ?)",
            params![artist_id, artist],
        )?;

        Ok(())
    }

    fn ensure_album(
        &self,
        title: &str,
        artist: &str,
        year: Option<u32>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;

        // Create SHA1 hash properly
        let mut hasher = Sha1::new();
        hasher.update(format!("{}:{}", artist, title).as_bytes());
        let album_id = format!("{:x}", hasher.finalize());

        conn.execute(
            "INSERT OR IGNORE INTO albums (id, title, artist, year) VALUES (?, ?, ?, ?)",
            params![album_id, title, artist, year],
        )?;

        Ok(())
    }

    pub fn batch_insert_tracks(
        &self,
        tracks: &[Track],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;

        let tx = conn.transaction()?;

        for track in tracks {
            // First ensure artist exists
            self.ensure_artist(&track.artist)?;

            // Then ensure album exists
            self.ensure_album(&track.album, &track.artist, track.release_year)?;

            // Insert track
            tx.execute(
                "INSERT OR REPLACE INTO tracks (
                    id, title, artist, album, duration, track_number, disc_number,
                    release_year, genre, file_path, file_format, file_size,
                    artwork_data, artwork_path
                ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
                params![
                    track.id,
                    track.title,
                    track.artist,
                    track.album,
                    track.duration,
                    track.track_number,
                    track.disc_number,
                    track.release_year,
                    track.genre,
                    match &track.source {
                        PlaybackSource::Local { path, .. } => path.to_str().unwrap_or_default(),
                        _ => "",
                    },
                    match &track.source {
                        PlaybackSource::Local { file_format, .. } => file_format,
                        _ => "",
                    },
                    match &track.source {
                        PlaybackSource::Local { file_size, .. } => file_size,
                        _ => &0,
                    },
                    match &track.artwork {
                        Artwork {
                            thumbnail: Some(data),
                            ..
                        } => Some(data as &[u8]),
                        _ => None,
                    },
                    match &track.artwork.full_art {
                        ArtworkSource::Local { path } => path.to_str().unwrap_or_default(),
                        _ => "",
                    },
                ],
            )?;
        }

        tx.commit()?;
        Ok(())
    }

    pub fn insert_track(&self, track: &Track) -> Result<(), Box<dyn Error + Send + Sync>> {
        // First ensure artist exists
        self.ensure_artist(&track.artist)?;

        // Then ensure album exists
        self.ensure_album(&track.album, &track.artist, track.release_year)?;

        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;

        conn.execute(
            "INSERT OR REPLACE INTO tracks (
                id, title, artist, album, duration, track_number, disc_number,
                release_year, genre, file_path, file_format, file_size,
                artwork_data, artwork_path
            ) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
            params![
                track.id,
                track.title,
                track.artist,
                track.album,
                track.duration,
                track.track_number,
                track.disc_number,
                track.release_year,
                track.genre,
                match &track.source {
                    PlaybackSource::Local { path, .. } => path.to_str().unwrap_or_default(),
                    _ => "",
                },
                match &track.source {
                    PlaybackSource::Local { file_format, .. } => file_format,
                    _ => "",
                },
                match &track.source {
                    PlaybackSource::Local { file_size, .. } => file_size,
                    _ => &0,
                },
                match &track.artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => Some(data as &[u8]),
                    _ => None,
                },
                match &track.artwork.full_art {
                    ArtworkSource::Local { path } => path.to_str().unwrap_or_default(),
                    _ => "",
                },
            ],
        )?;

        println!(
            "Successfully inserted track: {} - {}",
            track.title, track.artist
        );
        Ok(())
    }

    pub fn remove_track_by_path(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 10000;")?;

        conn.execute(
            "DELETE FROM tracks WHERE file_path = ?",
            params![path.to_str().unwrap_or_default()],
        )?;
        Ok(())
    }
}
