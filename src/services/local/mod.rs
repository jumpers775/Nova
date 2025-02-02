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

    fn calculate_match_score(
        name: &str,
        artist: Option<&str>,
        album: Option<&str>,
        query: &str,
    ) -> i32 {
        let mut score = 0;
        let query = query.to_lowercase();
        let name = name.to_lowercase();

        // Score for name/title match
        if name == query {
            score += 300; // Exact match
        } else if name.starts_with(&query) {
            score += 200; // Starts with query
        } else if name.contains(&query) {
            score += 100; // Contains query
        }

        // Score for artist match if provided
        if let Some(artist) = artist {
            let artist = artist.to_lowercase();
            if artist == query {
                score += 200;
            } else if artist.starts_with(&query) {
                score += 150;
            } else if artist.contains(&query) {
                score += 75;
            }
        }

        // Score for album match if provided
        if let Some(album) = album {
            let album = album.to_lowercase();
            if album == query {
                score += 150;
            } else if album.starts_with(&query) {
                score += 100;
            } else if album.contains(&query) {
                score += 50;
            }
        }

        score
    }
}

#[async_trait]
impl MusicProvider for LocalMusicProvider {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error>> {
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
        let query = query.to_lowercase();

        let mut scored_tracks: Vec<(i32, Track)> = all_tracks
            .into_iter()
            .filter_map(|track| {
                let score = Self::calculate_match_score(
                    &track.title,
                    Some(&track.artist),
                    Some(&track.album),
                    &query,
                );
                if score > 0 {
                    Some((score, track))
                } else {
                    None
                }
            })
            .collect();

        scored_tracks.sort_by(|a, b| b.0.cmp(&a.0));

        Ok(scored_tracks
            .into_iter()
            .skip(offset)
            .take(limit)
            .map(|(_, track)| track)
            .collect())
    }
}
