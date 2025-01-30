use async_trait::async_trait;
use std::error::Error;
use std::fs::File;
use std::path::PathBuf;
use symphonia::core::codecs::CodecParameters;
use symphonia::core::formats::FormatOptions;
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

                let track = Track {
                    id: file.to_str().unwrap_or_default().to_string(), // Simple ID for now
                    title,
                    artist,
                    album,
                    duration,
                    track_number: None,
                    disc_number: None,
                    release_year: None,
                    genre: None,
                    artwork: Artwork {
                        thumbnail: None,
                        full_art: ArtworkSource::None,
                    },
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

    async fn search(&self, _query: &str) -> Result<Vec<Track>, Box<dyn Error>> {
        todo!()
    }
}
