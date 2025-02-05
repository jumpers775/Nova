use crate::services::models::{Album, Artist, Artwork, ArtworkSource, PlaybackSource, Track};
use r2d2::Pool;
use r2d2_sqlite::SqliteConnectionManager;
use rusqlite::{params, OptionalExtension};
use sha1::{Digest, Sha1};
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Debug)]
pub struct Database {
    pool: Arc<Pool<SqliteConnectionManager>>,
}

impl Database {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        println!("Initializing in-memory database");

        // Initialize in-memory database
        let manager = SqliteConnectionManager::memory()
            .with_flags(
                rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE
                    | rusqlite::OpenFlags::SQLITE_OPEN_CREATE
                    | rusqlite::OpenFlags::SQLITE_OPEN_SHARED_CACHE,
            )
            .with_init(|conn| {
                conn.execute_batch(
                    "PRAGMA journal_mode = OFF;  -- No need for journaling in memory
                     PRAGMA synchronous = OFF;   -- No need for fsync
                     PRAGMA temp_store = MEMORY;
                     PRAGMA cache_size = 10000;  -- Increased cache size for memory
                     PRAGMA busy_timeout = 60000;",
                )?;
                Ok(())
            });

        // Create pool with appropriate size
        let pool = Pool::builder()
            .max_size(4)
            .min_idle(Some(1))
            .build(manager)?;

        let db = Self {
            pool: Arc::new(pool),
        };

        // Initialize schema in a transaction
        {
            let mut conn = db.pool.get()?;
            let tx = conn.transaction()?;

            // Create tables
            tx.execute_batch(
                "CREATE TABLE IF NOT EXISTS tracks (
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
                    artwork_data BLOB,
                    artwork_path TEXT,
                    UNIQUE(title, artist)
                );

                CREATE TABLE IF NOT EXISTS artists (
                    id TEXT PRIMARY KEY,
                    name TEXT NOT NULL UNIQUE,
                    artwork_data BLOB,
                    artwork_path TEXT
                );",
            )?;

            // Create indexes
            tx.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_tracks_title ON tracks(title);
                 CREATE INDEX IF NOT EXISTS idx_tracks_artist ON tracks(artist);
                 CREATE INDEX IF NOT EXISTS idx_tracks_album ON tracks(album);
                 CREATE INDEX IF NOT EXISTS idx_albums_title ON albums(title);
                 CREATE INDEX IF NOT EXISTS idx_artists_name ON artists(name);
                 CREATE INDEX IF NOT EXISTS idx_tracks_search ON tracks(title, artist, album);
                 CREATE INDEX IF NOT EXISTS idx_albums_search ON albums(title, artist);
                 CREATE INDEX IF NOT EXISTS idx_artists_search ON artists(name);",
            )?;

            tx.commit()?;
        }

        // Now initialize artwork
        db.initialize_artwork()?;

        println!("In-memory database initialized successfully");
        Ok(db)
    }

    fn get_connection(
        &self,
    ) -> Result<r2d2::PooledConnection<SqliteConnectionManager>, Box<dyn Error + Send + Sync>> {
        Ok(self.pool.get()?)
    }

    fn initialize(&self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        println!("Initializing database tables and indexes");
        let mut conn = self.pool.get()?;

        // First create tables if they don't exist
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
                artwork_data BLOB,
                artwork_path TEXT,
                UNIQUE(title, artist)
            );

            CREATE TABLE IF NOT EXISTS artists (
                id TEXT PRIMARY KEY,
                name TEXT NOT NULL UNIQUE,
                artwork_data BLOB,
                artwork_path TEXT
            );
        ",
        )?;

        // Function to check if a column exists in a table
        fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
            conn.query_row(
                "SELECT 1 FROM pragma_table_info(?) WHERE name = ?",
                params![table, column],
                |_row| Ok(true),
            )
            .unwrap_or(false)
        }

        // Add artwork columns to tracks if they don't exist
        if !column_exists(&conn, "tracks", "artwork_data") {
            conn.execute("ALTER TABLE tracks ADD COLUMN artwork_data BLOB", [])?;
        }
        if !column_exists(&conn, "tracks", "artwork_path") {
            conn.execute("ALTER TABLE tracks ADD COLUMN artwork_path TEXT", [])?;
        }

        // Add artwork columns to albums if they don't exist
        if !column_exists(&conn, "albums", "artwork_data") {
            conn.execute("ALTER TABLE albums ADD COLUMN artwork_data BLOB", [])?;
        }
        if !column_exists(&conn, "albums", "artwork_path") {
            conn.execute("ALTER TABLE albums ADD COLUMN artwork_path TEXT", [])?;
        }

        // Add artwork columns to artists if they don't exist
        if !column_exists(&conn, "artists", "artwork_data") {
            conn.execute("ALTER TABLE artists ADD COLUMN artwork_data BLOB", [])?;
        }
        if !column_exists(&conn, "artists", "artwork_path") {
            conn.execute("ALTER TABLE artists ADD COLUMN artwork_path TEXT", [])?;
        }

        // Create indexes
        conn.execute_batch(
            "
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
            "SELECT id, title, artist, album, duration, track_number, disc_number, release_year, genre, file_path, file_format, file_size, artwork_data, artwork_path
            FROM tracks
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
                            full_art: match row.get::<_, Option<String>>(13)? {
                                Some(path) if !path.is_empty() => ArtworkSource::Local {
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
        let mut stmt = conn.prepare("SELECT id, title, artist, album, duration, track_number, disc_number, release_year, genre, file_path, file_format, file_size, artwork_data, artwork_path FROM tracks")?;
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
                        full_art: match row.get::<_, Option<String>>(13)? {
                            Some(path) if !path.is_empty() => ArtworkSource::Local {
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
            "INSERT OR REPLACE INTO artists (id, name, artwork_data, artwork_path) VALUES (?, ?, ?, ?)",
            params![artist.id, artist.name, match &artist.artwork {
                Some(Artwork { thumbnail: Some(data), .. }) => Some(data as &[u8]),
                _ => None,
            }, match &artist.artwork {
                Some(Artwork { full_art: ArtworkSource::Local { path }, .. }) => path.to_str().unwrap_or_default(),
                _ => "",
            }],
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
            "INSERT OR REPLACE INTO albums (id, title, artist, year, artwork_data, artwork_path) VALUES (?, ?, ?, ?, ?, ?)",
            params![
                album.id,
                album.title,
                album.artist,
                album.year,
                match &album.artwork {
                    Some(Artwork { thumbnail: Some(data), .. }) => Some(data as &[u8]),
                    _ => None,
                },
                match &album.artwork {
                    Some(Artwork { full_art: ArtworkSource::Local { path }, .. }) => path.to_str().unwrap_or_default(),
                    _ => "",
                }
            ],
        )?;
        Ok(())
    }

    pub fn get_all_artists(&self) -> Result<Vec<Artist>, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let mut stmt = conn.prepare(
            "SELECT a.id, a.name, COALESCE(a.artwork_data, t.artwork_data) as final_artwork_data,
                    COALESCE(a.artwork_path, t.artwork_path) as final_artwork_path
             FROM artists a
             LEFT JOIN tracks t ON a.name = t.artist
             WHERE a.name != 'Unknown Artist'
             GROUP BY a.id",
        )?;

        let artists: Vec<Artist> = stmt
            .query_map([], |row| {
                Ok(Artist {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    albums: Vec::new(),
                    artwork: Some(Artwork {
                        thumbnail: row.get(2)?,
                        full_art: match row.get::<_, Option<String>>(3)? {
                            Some(path) => ArtworkSource::Local {
                                path: PathBuf::from(path),
                            },
                            None => ArtworkSource::None,
                        },
                    }),
                })
            })?
            .filter_map(Result::ok)
            .collect();

        Ok(artists)
    }

    pub fn get_all_albums(&self) -> Result<Vec<Album>, Box<dyn std::error::Error + Send + Sync>> {
        let mut conn = self.get_connection()?;
        let tx = conn.transaction()?;

        let sql = "SELECT a.id, a.title, a.artist, a.year,
                   COALESCE(a.artwork_data, (
                       SELECT t.artwork_data
                       FROM tracks t
                       WHERE t.album = a.title AND t.artist = a.artist
                       AND t.artwork_data IS NOT NULL
                       ORDER BY t.track_number ASC
                       LIMIT 1
                   )) as final_artwork_data,
                   COALESCE(a.artwork_path, (
                       SELECT t.artwork_path
                       FROM tracks t
                       WHERE t.album = a.title AND t.artist = a.artist
                       AND t.artwork_path IS NOT NULL
                       ORDER BY t.track_number ASC
                       LIMIT 1
                   )) as final_artwork_path
            FROM albums a
            WHERE a.title != 'Unknown Album'";

        let mut stmt = tx.prepare(sql)?;
        let albums = stmt
            .query_map([], |row| {
                Ok(Album {
                    id: row.get(0)?,
                    title: row.get(1)?,
                    artist: row.get(2)?,
                    year: row.get(3)?,
                    art_url: None,
                    tracks: Vec::new(),
                    artwork: Some(Artwork {
                        thumbnail: row.get(4)?,
                        full_art: match row.get::<_, Option<String>>(5)? {
                            Some(path) => ArtworkSource::Local {
                                path: PathBuf::from(path),
                            },
                            None => ArtworkSource::None,
                        },
                    }),
                })
            })?
            .filter_map(Result::ok)
            .collect::<Vec<_>>();

        drop(stmt);
        tx.commit()?;

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
        let mut stmt = conn.prepare(
            "SELECT a.id, a.name,
                    COALESCE(a.artwork_data, (
                        SELECT t.artwork_data
                        FROM tracks t
                        WHERE t.artist = a.name
                        ORDER BY t.track_number ASC
                        LIMIT 1
                    )) as final_artwork_data,
                    COALESCE(a.artwork_path, (
                        SELECT t.artwork_path
                        FROM tracks t
                        WHERE t.artist = a.name
                        ORDER BY t.track_number ASC
                        LIMIT 1
                    )) as final_artwork_path
             FROM artists a
             WHERE a.name LIKE ?1
             AND a.name != 'Unknown Artist'
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
                        artwork: Some(Artwork {
                            thumbnail: row.get(2)?,
                            full_art: match row.get::<_, Option<String>>(3)? {
                                Some(path) => ArtworkSource::Local {
                                    path: PathBuf::from(path),
                                },
                                None => ArtworkSource::None,
                            },
                        }),
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
        let mut stmt = conn.prepare(
            "SELECT a.id, a.title, a.artist, a.year,
                    COALESCE(a.artwork_data, (
                        SELECT t.artwork_data
                        FROM tracks t
                        WHERE t.album = a.title AND t.artist = a.artist
                        ORDER BY t.track_number ASC
                        LIMIT 1
                    )) as final_artwork_data,
                    COALESCE(a.artwork_path, (
                        SELECT t.artwork_path
                        FROM tracks t
                        WHERE t.album = a.title AND t.artist = a.artist
                        ORDER BY t.track_number ASC
                        LIMIT 1
                    )) as final_artwork_path
             FROM albums a
             WHERE (a.title LIKE ?1 OR a.artist LIKE ?1)
             AND a.title != 'Unknown Album'
             GROUP BY a.id
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
                        artwork: Some(Artwork {
                            thumbnail: row.get::<_, Option<Vec<u8>>>(4)?,
                            full_art: match row.get::<_, Option<String>>(5)? {
                                Some(path) => ArtworkSource::Local {
                                    path: PathBuf::from(path),
                                },
                                None => ArtworkSource::None,
                            },
                        }),
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
        let tx = conn.transaction()?;

        // Create SHA1 hash properly
        let mut hasher = Sha1::new();
        hasher.update(artist.as_bytes());
        let artist_id = format!("{:x}", hasher.finalize());

        tx.execute(
            "INSERT OR IGNORE INTO artists (id, name, artwork_data, artwork_path)
             VALUES (?, ?, NULL, NULL)",
            params![artist_id, artist],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn ensure_album(
        &self,
        title: &str,
        artist: &str,
        year: Option<u32>,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // Create SHA1 hash in Rust
        let mut hasher = Sha1::new();
        hasher.update(format!("{}:{}", title, artist).as_bytes());
        let album_id = format!("{:x}", hasher.finalize());

        tx.execute(
            "INSERT OR IGNORE INTO albums (id, title, artist, year, artwork_data, artwork_path)
             VALUES (?, ?, ?, ?, NULL, NULL)",
            params![album_id, title, artist, year],
        )?;

        tx.commit()?;
        Ok(())
    }

    pub fn update_artist_artwork(
        &self,
        artist_name: &str,
        artwork: &Artwork,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE artists SET
                artwork_data = ?,
                artwork_path = ?
             WHERE name = ?",
            params![
                match &artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => Some(data as &[u8]),
                    _ => None,
                },
                match &artwork.full_art {
                    ArtworkSource::Local { path } => path.to_str().unwrap_or_default(),
                    _ => "",
                },
                artist_name,
            ],
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn update_album_artwork(
        &self,
        title: &str,
        artist: &str,
        artwork: &Artwork,
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        tx.execute(
            "UPDATE albums SET
                artwork_data = ?,
                artwork_path = ?
             WHERE title = ? AND artist = ?",
            params![
                match &artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => Some(data as &[u8]),
                    _ => None,
                },
                match &artwork.full_art {
                    ArtworkSource::Local { path } => path.to_str().unwrap_or_default(),
                    _ => "",
                },
                title,
                artist,
            ],
        )?;

        // Update artwork for all tracks of this album as well
        tx.execute(
            "UPDATE tracks SET
                artwork_data = ?,
                artwork_path = ?
             WHERE album = ? AND artist = ?",
            params![
                match &artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => Some(data as &[u8]),
                    _ => None,
                },
                match &artwork.full_art {
                    ArtworkSource::Local { path } => path.to_str().unwrap_or_default(),
                    _ => "",
                },
                title,
                artist,
            ],
        )?;

        tx.commit()?;
        Ok(())
    }

    fn initialize_artwork(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // Process albums
        {
            let mut albums_query = tx.prepare(
                "SELECT DISTINCT t.album, t.artist, t.artwork_data, t.artwork_path
                 FROM tracks t
                 WHERE t.artwork_data IS NOT NULL OR t.artwork_path IS NOT NULL",
            )?;

            let album_rows = albums_query.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,          // album
                    row.get::<_, String>(1)?,          // artist
                    row.get::<_, Option<Vec<u8>>>(2)?, // artwork_data
                    row.get::<_, Option<String>>(3)?,  // artwork_path
                ))
            })?;

            // Process each album
            for result in album_rows {
                let (album, artist, artwork_data, artwork_path) = result?;

                // Create album ID using Rust's SHA1
                let mut hasher = Sha1::new();
                hasher.update(format!("{}:{}", album, artist).as_bytes());
                let album_id = format!("{:x}", hasher.finalize());

                // Update album entry
                tx.execute(
                    "INSERT OR REPLACE INTO albums (id, title, artist, artwork_data, artwork_path)
                     VALUES (?, ?, ?, ?, ?)",
                    params![album_id, album, artist, artwork_data, artwork_path],
                )?;
            }
        } // albums_query is dropped here

        // Process artists
        {
            let mut artists_query = tx.prepare(
                "SELECT DISTINCT t.artist, t.artwork_data, t.artwork_path
                 FROM tracks t
                 WHERE t.artwork_data IS NOT NULL OR t.artwork_path IS NOT NULL",
            )?;

            let artist_rows = artists_query.query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,          // artist
                    row.get::<_, Option<Vec<u8>>>(1)?, // artwork_data
                    row.get::<_, Option<String>>(2)?,  // artwork_path
                ))
            })?;

            // Process each artist
            for result in artist_rows {
                let (artist, artwork_data, artwork_path) = result?;

                // Create artist ID using Rust's SHA1
                let mut hasher = Sha1::new();
                hasher.update(artist.as_bytes());
                let artist_id = format!("{:x}", hasher.finalize());

                // Update artist entry
                tx.execute(
                    "INSERT OR REPLACE INTO artists (id, name, artwork_data, artwork_path)
                     VALUES (?, ?, ?, ?)",
                    params![artist_id, artist, artwork_data, artwork_path],
                )?;
            }
        } // artists_query is dropped here

        tx.commit()?;
        Ok(())
    }

    pub fn batch_insert_tracks(
        &self,
        tracks: &[Track],
    ) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        conn.execute_batch("PRAGMA busy_timeout = 60000;")?;
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
        let tx = conn.transaction()?;

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

        tx.commit()?;

        println!(
            "Successfully inserted track: {} - {}",
            track.title, track.artist
        );
        Ok(())
    }

    pub fn remove_track_by_path(&self, path: &Path) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // Get track info before deletion
        let track_info: Option<(String, String)> = tx
            .query_row(
                "SELECT artist, album FROM tracks WHERE file_path = ?",
                params![path.to_str().unwrap_or_default()],
                |row| Ok((row.get(0)?, row.get(1)?)),
            )
            .optional()?;

        // Delete the track
        tx.execute(
            "DELETE FROM tracks WHERE file_path = ?",
            params![path.to_str().unwrap_or_default()],
        )?;

        // If we found track info, clean up orphaned albums and artists
        if let Some((artist, album)) = track_info {
            // Check if this was the last track from this album
            let album_track_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM tracks WHERE album = ? AND artist = ?",
                params![album, artist],
                |row| row.get(0),
            )?;

            if album_track_count == 0 {
                // Delete the album if no tracks remain
                tx.execute(
                    "DELETE FROM albums WHERE title = ? AND artist = ?",
                    params![album, artist],
                )?;
            }

            // Check if this was the last track from this artist
            let artist_track_count: i64 = tx.query_row(
                "SELECT COUNT(*) FROM tracks WHERE artist = ?",
                params![artist],
                |row| row.get(0),
            )?;

            if artist_track_count == 0 {
                // Delete the artist if no tracks remain
                tx.execute("DELETE FROM artists WHERE name = ?", params![artist])?;
            }
        }

        tx.commit()?;
        println!("Successfully removed track at path: {:?}", path);
        Ok(())
    }

    pub fn cleanup_database(&self) -> Result<(), Box<dyn Error + Send + Sync>> {
        let mut conn = self.pool.get()?;
        let tx = conn.transaction()?;

        // Remove tracks with non-existent files
        let tracks: Vec<(String,)> = tx
            .prepare("SELECT file_path FROM tracks")?
            .query_map([], |row| Ok((row.get(0)?,)))?
            .filter_map(Result::ok)
            .collect();

        for (path,) in tracks {
            if !std::path::Path::new(&path).exists() {
                println!("Removing track with missing file: {}", path);
                self.remove_track_by_path(std::path::Path::new(&path))?;
            }
        }

        tx.commit()?;
        Ok(())
    }
}
