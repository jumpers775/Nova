use super::models::{SearchResults, SearchWeights};
use crate::services::PlayableItem;
use async_trait::async_trait;
use chrono::Utc;
use fuzzy_matcher::skim::SkimMatcherV2;
use fuzzy_matcher::FuzzyMatcher;
use std::collections::HashMap;
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;

use super::error::ServiceError;
use super::models::{Album, Artist, Artwork, ArtworkSource, PlaybackSource, Track};
use super::traits::MusicProvider;
mod scanner;

#[derive(Debug)]
pub struct LocalMusicProvider {
    music_dir: PathBuf,
}

impl LocalMusicProvider {
    pub fn new(music_dir: PathBuf) -> Self {
        Self { music_dir }
    }

    fn extract_artwork(probed: &mut Box<dyn FormatReader>) -> Option<Vec<u8>> {
        if let Some(metadata) = probed.metadata().current() {
            for visual in metadata.visuals() {
                return Some(visual.data.to_vec());
            }
        }
        None
    }

    fn find_album_art_file(track_path: &Path) -> Option<PathBuf> {
        let parent = track_path.parent()?;
        let common_names = ["cover", "folder", "album", "front"];
        let extensions = ["jpg", "jpeg", "png"];

        for name in common_names.iter() {
            for ext in extensions.iter() {
                let file_name = format!("{}.{}", name, ext);
                let path = parent.join(&file_name);
                if path.exists() {
                    return Some(path);
                }
            }
        }
        None
    }

    fn score_artist_match(&self, artist_name: &str, query: &str) -> f32 {
        let matcher = SkimMatcherV2::default().ignore_case().use_cache(true);
        let query = query.to_lowercase();
        let mut score = 0.0;

        // Direct name matching
        if artist_name.to_lowercase() == query {
            score += 1000.0;
        }

        // Fuzzy matching on name
        if let Some(name_score) = matcher.fuzzy_match(artist_name, &query) {
            score += name_score as f32 * 3.0;
        }

        // Contains matching
        if artist_name.to_lowercase().contains(&query) {
            score += 500.0;
        }

        score
    }

    // Helper method for scoring album matches
    fn score_album_match(&self, album: &str, artist: &str, query: &str) -> f32 {
        let matcher = SkimMatcherV2::default().ignore_case().use_cache(true);
        let query = query.to_lowercase();
        let mut score = 0.0;

        // Direct matches
        if album.to_lowercase() == query {
            score += 1000.0;
        }
        if artist.to_lowercase() == query {
            score += 500.0;
        }

        // Fuzzy matching
        if let Some(album_score) = matcher.fuzzy_match(album, &query) {
            score += album_score as f32 * 3.0;
        }
        if let Some(artist_score) = matcher.fuzzy_match(artist, &query) {
            score += artist_score as f32 * 2.0;
        }

        // Contains matching
        if album.to_lowercase().contains(&query) {
            score += 300.0;
        }
        if artist.to_lowercase().contains(&query) {
            score += 200.0;
        }

        score
    }
}

#[async_trait]
impl MusicProvider for LocalMusicProvider {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>> {
        let mut found_tracks: Vec<Track> = Vec::new();
        if self.music_dir.is_dir() {
            let music_files = scanner::FileScanner::scan_directory(&self.music_dir)?;
            for file in music_files {
                let file_size = match std::fs::metadata(&file) {
                    Ok(metadata) => metadata.len(),
                    Err(_) => continue, // Skip files we can't read metadata for
                };

                let src = match File::open(&file) {
                    Ok(file) => file,
                    Err(_) => continue, // Skip files we can't open
                };

                let mss = MediaSourceStream::new(Box::new(src), Default::default());
                let meta_opts: MetadataOptions = Default::default();
                let fmt_opts: FormatOptions = Default::default();
                let hint = Hint::new();

                let mut probed = match symphonia::default::get_probe()
                    .format(&hint, mss, &fmt_opts, &meta_opts)
                {
                    Ok(probed) => probed,
                    Err(_) => continue, // Skip files that can't be probed
                };

                // Get filename without extension as default title
                let default_title = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .map(|name| {
                        name.rfind('.')
                            .map(|i| name[..i].to_string())
                            .unwrap_or_else(|| name.to_string())
                    })
                    .unwrap_or_else(|| String::from("Unknown"));

                let mut title = default_title;
                let mut artist = String::from("Unknown Artist");
                let mut album = String::from("Unknown Album");
                let mut duration = 0;

                // Extract metadata if available
                let metadata = probed.format.metadata();
                if let Some(tags) = metadata.current() {
                    for tag in tags.tags() {
                        let key = tag.key.to_uppercase();
                        let value = tag.value.to_string();

                        match key.as_str() {
                            "TITLE" => {
                                if !value.is_empty() {
                                    title = value;
                                }
                            }
                            "ARTIST" => {
                                if !value.is_empty() {
                                    artist = value;
                                }
                            }
                            "ALBUM" => {
                                if !value.is_empty() {
                                    album = value;
                                }
                            }
                            _ => (),
                        }
                    }
                }

                // Get duration from properties
                if let Some(track) = probed.format.tracks().get(0) {
                    let params: &CodecParameters = &track.codec_params;
                    if let Some(samples) = params.n_frames {
                        if let Some(sample_rate) = params.sample_rate {
                            let duration_seconds = samples as f64 / sample_rate as f64;
                            duration = duration_seconds as u32;
                        }
                    }
                }

                // Try to extract embedded artwork
                let artwork = if let Some(embedded_art) =
                    Self::extract_artwork(&mut Box::new(probed.format))
                {
                    Artwork {
                        thumbnail: Some(embedded_art),
                        full_art: ArtworkSource::None,
                    }
                } else if let Some(art_path) = Self::find_album_art_file(&file) {
                    Artwork {
                        thumbnail: None,
                        full_art: ArtworkSource::Local { path: art_path },
                    }
                } else {
                    Artwork {
                        thumbnail: None,
                        full_art: ArtworkSource::None,
                    }
                };

                let track = Track {
                    id: file.to_str().unwrap_or_default().to_string(),
                    title,
                    artist,
                    album,
                    duration,
                    track_number: None,
                    disc_number: None,
                    release_year: None,
                    genre: None,
                    artwork,
                    source: PlaybackSource::Local {
                        file_format: file
                            .extension()
                            .and_then(|ext| ext.to_str())
                            .unwrap_or("unknown")
                            .to_string(),
                        file_size,
                        path: file.clone(),
                    },
                };

                found_tracks.push(track);
            }
        }

        Ok(found_tracks)
    }

    async fn get_artists(&self) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>> {
        let all_tracks = self.get_tracks().await?;
        let mut artists = std::collections::HashMap::new();

        for track in all_tracks {
            if !artists.contains_key(&track.artist) {
                artists.insert(
                    track.artist.clone(),
                    Artist {
                        id: track.artist.clone(),
                        name: track.artist,
                        albums: Vec::new(),
                    },
                );
            }
        }

        Ok(artists.into_values().collect())
    }

    async fn get_albums(&self) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>> {
        let all_tracks = self.get_tracks().await?;
        let mut albums = std::collections::HashMap::new();

        for track in all_tracks {
            let album_key = format!("{}-{}", track.album, track.artist);
            if !albums.contains_key(&album_key) {
                albums.insert(
                    album_key.clone(),
                    Album {
                        id: album_key,
                        title: track.album,
                        artist: track.artist,
                        year: track.release_year,
                        art_url: None,
                        tracks: Vec::new(),
                    },
                );
            }
        }

        Ok(albums.into_values().collect())
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
        let all_tracks = self.get_tracks().await?;
        let query = query.to_lowercase();
        let query_words: Vec<&str> = query.split_whitespace().collect();

        let matcher = SkimMatcherV2::default().ignore_case().use_cache(true);

        let mut scored_tracks: Vec<(i64, Track)> = all_tracks
            .into_iter()
            .filter_map(|track| {
                let mut score = 0i64;

                // Check each word in the query separately
                for word in &query_words {
                    // Title matching
                    if let Some(title_score) = matcher.fuzzy_match(&track.title, word) {
                        score += title_score * 3;
                    }
                    // Simple contains check for more leniency
                    if track.title.to_lowercase().contains(word) {
                        score += 500;
                    }

                    // Artist matching
                    if let Some(artist_score) = matcher.fuzzy_match(&track.artist, word) {
                        score += artist_score * 2;
                    }
                    if track.artist.to_lowercase().contains(word) {
                        score += 300;
                    }

                    // Album matching
                    if let Some(album_score) = matcher.fuzzy_match(&track.album, word) {
                        score += album_score;
                    }
                    if track.album.to_lowercase().contains(word) {
                        score += 200;
                    }
                }

                // Bonus points for matching more query words
                let matched_words = query_words
                    .iter()
                    .filter(|&&word| {
                        track.title.to_lowercase().contains(word)
                            || track.artist.to_lowercase().contains(word)
                            || track.album.to_lowercase().contains(word)
                    })
                    .count();

                if matched_words > 0 {
                    score += (matched_words as i64) * 1000;
                }

                // Even if the fuzzy match score is low, if we match any words, include it
                if score > 0 || matched_words > 0 {
                    Some((score, track))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (highest first)
        scored_tracks.sort_by(|a, b| b.0.cmp(&a.0));

        // Apply pagination
        Ok(scored_tracks
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, track)| track)
            .collect())
    }

    async fn search_albums(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>> {
        let all_tracks = self.get_tracks().await?;

        let mut albums: std::collections::HashMap<String, Album> = std::collections::HashMap::new();
        for track in all_tracks {
            let album_key = format!("{}-{}", track.album, track.artist);
            if !albums.contains_key(&album_key) {
                albums.insert(
                    album_key.clone(),
                    Album {
                        id: album_key,
                        title: track.album,
                        artist: track.artist,
                        year: track.release_year,
                        art_url: None,
                        tracks: Vec::new(),
                    },
                );
            }
        }

        let mut scored_albums: Vec<(f32, Album)> = albums
            .into_iter()
            .map(|(_, album)| {
                let score = self.score_album_match(&album.title, &album.artist, query);
                (score, album)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored_albums.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored_albums
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, album)| album)
            .collect())
    }

    async fn search_artists(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>> {
        let all_tracks = self.get_tracks().await?;

        let mut artists: std::collections::HashMap<String, Artist> =
            std::collections::HashMap::new();
        for track in all_tracks {
            if !artists.contains_key(&track.artist) {
                artists.insert(
                    track.artist.clone(),
                    Artist {
                        id: track.artist.clone(),
                        name: track.artist,
                        albums: Vec::new(),
                    },
                );
            }
        }

        let mut scored_artists: Vec<(f32, Artist)> = artists
            .into_iter()
            .map(|(_, artist)| {
                let score = self.score_artist_match(&artist.name, query);
                (score, artist)
            })
            .filter(|(score, _)| *score > 0.0)
            .collect();

        scored_artists.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        Ok(scored_artists
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, artist)| artist)
            .collect())
    }

    async fn search_all(
        &self,
        query: &str,
        weights: &SearchWeights,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResults, Box<dyn Error + Send + Sync>> {
        let (tracks, albums, artists) = futures::join!(
            self.search_tracks(query, limit, offset),
            self.search_albums(query, limit, offset),
            self.search_artists(query, limit, offset),
        );

        Ok(SearchResults {
            tracks: tracks?
                .into_iter()
                .map(|track| PlayableItem {
                    track,
                    provider: "local".to_string(),
                    added_at: chrono::Utc::now(),
                })
                .collect(),
            albums: albums?,
            artists: artists?,
        })
    }
}
