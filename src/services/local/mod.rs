use async_trait::async_trait;
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
}

#[async_trait]
impl MusicProvider for LocalMusicProvider {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error>> {
        let mut found_tracks: Vec<Track> = Vec::new();
        if self.music_dir.is_dir() {
            let music_files = scanner::FileScanner::scan_directory(&self.music_dir)?;
            for file in music_files {
                // Get file size before opening file for symphonia
                let file_size = std::fs::metadata(&file)?.len();

                let src = File::open(&file)?;
                let mss = MediaSourceStream::new(Box::new(src), Default::default());
                let meta_opts: MetadataOptions = Default::default();
                let fmt_opts: FormatOptions = Default::default();
                let hint = Hint::new();
                let mut probed = symphonia::default::get_probe()
                    .format(&hint, mss, &fmt_opts, &meta_opts)
                    .map_err(|e| Box::new(ServiceError::ProviderError(e.to_string())))?;

                // Default values
                let mut title = file
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Unknown")
                    .to_string();
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
                            "TITLE" => title = value,
                            "ARTIST" => artist = value,
                            "ALBUM" => album = value,
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

    async fn get_albums(&self) -> Result<Vec<Album>, Box<dyn Error>> {
        todo!()
    }

    async fn get_artists(&self) -> Result<Vec<Artist>, Box<dyn Error>> {
        todo!()
    }

    async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error>> {
        let all_tracks = self.get_tracks().await?;

        // Convert query to lowercase for case-insensitive matching
        let query = query.to_lowercase();

        // Score and sort tracks
        let mut scored_tracks: Vec<(f32, Track)> = all_tracks
            .into_iter()
            .filter_map(|track| {
                let mut score = 0.0;

                // Score based on title match
                if track.title.to_lowercase().contains(&query) {
                    score += 1.0;
                    // Bonus for exact match
                    if track.title.to_lowercase() == query {
                        score += 0.5;
                    }
                    // Bonus for start match
                    if track.title.to_lowercase().starts_with(&query) {
                        score += 0.3;
                    }
                }

                // Score based on artist match
                if track.artist.to_lowercase().contains(&query) {
                    score += 0.8;
                    if track.artist.to_lowercase() == query {
                        score += 0.4;
                    }
                }

                // Score based on album match
                if track.album.to_lowercase().contains(&query) {
                    score += 0.6;
                    if track.album.to_lowercase() == query {
                        score += 0.3;
                    }
                }

                if score > 0.0 {
                    Some((score, track))
                } else {
                    None
                }
            })
            .collect();

        // Sort by score (highest first)
        scored_tracks.sort_by(|a, b| b.0.partial_cmp(&a.0).unwrap_or(std::cmp::Ordering::Equal));

        // Return paginated results
        Ok(scored_tracks
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, track)| track)
            .collect())
    }
}
