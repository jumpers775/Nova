use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artwork {
    pub thumbnail: Option<Vec<u8>>,
    pub full_art: ArtworkSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ArtworkSource {
    Embedded {
        format: String, // "image/png", "image/jpeg", etc.
        data: Vec<u8>,  // External path/URL or raw bytes
    },
    Local {
        path: PathBuf,
    },
    Remote {
        url: String,
        cache_key: Option<String>,
    },
    None,
}

// Playback source information
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum PlaybackSource {
    Local {
        file_format: String,
        file_size: u64,
        path: PathBuf,
    },
    Spotify {
        track_id: String,
        url: String,
    },
    YouTube {
        video_id: String,
        stream_url: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Track {
    pub id: String, // Unique across all providers (e.g., hash of source)
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration: u32,
    pub track_number: Option<u32>,
    pub disc_number: Option<u32>,
    pub release_year: Option<u32>,
    pub genre: Option<String>,
    pub artwork: Artwork,
    pub source: PlaybackSource,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlayableItem {
    pub track: Track,
    pub provider: String,
    pub added_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub id: String,
    pub name: String,
    pub items: Vec<PlayableItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Album {
    pub id: String,
    pub title: String,
    pub artist: String,
    pub year: Option<u32>,
    pub art_url: Option<String>,
    pub tracks: Vec<String>, // Track IDs
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Artist {
    pub id: String,
    pub name: String,
    pub albums: Vec<String>, // Album IDs
}

#[derive(Debug, Clone)]
pub struct SearchResults {
    pub tracks: Vec<PlayableItem>,
    pub albums: Vec<Album>,
    pub artists: Vec<Artist>,
}

#[derive(Debug, Clone)]
pub struct SearchWeights {
    pub track_weight: f32,
    pub album_weight: f32,
    pub artist_weight: f32,
}

impl Default for SearchWeights {
    fn default() -> Self {
        Self {
            track_weight: 1.0,
            album_weight: 1.0,
            artist_weight: 1.0,
        }
    }
}

#[derive(Debug, Clone)]
pub struct ScoredResult {
    pub score: f32,
    pub result_type: SearchResultType,
}

#[derive(Debug, Clone)]
pub enum SearchResultType {
    Track(PlayableItem),
    Album(Album),
    Artist(Artist),
}
