use crate::services::models::{Artwork, ArtworkSource, PlaybackSource, Track};
use sha1::{Digest, Sha1};
use std::error::Error;
use std::fs::File;
use std::path::{Path, PathBuf};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use walkdir::WalkDir;

pub struct FileScanner;

impl FileScanner {
    pub fn scan_directory(path: &Path) -> Result<Vec<PathBuf>, Box<dyn Error + Send + Sync>> {
        use rayon::prelude::*;

        let walker = WalkDir::new(path).follow_links(true).into_iter();
        let music_files: Vec<_> = walker
            .filter_map(Result::ok)
            .par_bridge()
            .filter(|entry| Self::is_music_file(entry.path()))
            .map(|entry| entry.path().to_owned())
            .collect();

        Ok(music_files)
    }

    fn is_music_file(path: &Path) -> bool {
        if let Some(extension) = path.extension() {
            matches!(
                extension.to_str().unwrap_or("").to_lowercase().as_str(),
                "mp3" | "flac" | "m4a" | "ogg" | "wav"
            )
        } else {
            false
        }
    }

    pub fn is_music_file_public(path: &Path) -> bool {
        Self::is_music_file(path)
    }

    pub fn process_file(path: &Path) -> Result<Track, Box<dyn Error + Send + Sync>> {
        // Check if file exists first
        if !path.exists() {
            return Err(Box::new(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("File not found: {:?}", path),
            )));
        }

        // Generate a unique ID for the track based on its path
        let mut hasher = Sha1::new();
        hasher.update(path.to_str().unwrap_or_default().as_bytes());
        let id = format!("{:x}", hasher.finalize());

        // Open the file
        let file = File::open(path)?;
        let file_metadata = file.metadata()?;
        let file_size = file_metadata.len();

        // Create a media source from the file
        let mss = MediaSourceStream::new(Box::new(file), Default::default());

        // Create probe hint and options
        let hint = Hint::new();
        let format_opts: FormatOptions = Default::default();
        let metadata_opts: MetadataOptions = Default::default();

        // Probe the media source
        let mut probed =
            symphonia::default::get_probe().format(&hint, mss, &format_opts, &metadata_opts)?;

        // Get default values
        let mut title = path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();
        let mut artist = String::from("Unknown Artist");
        let mut album = String::from("Unknown Album");
        let mut track_number = None;
        let mut disc_number = None;
        let mut release_year = None;
        let mut genre = None;
        let mut duration = 0;

        // Get format metadata
        if let Some(metadata) = probed.format.metadata().current() {
            for tag in metadata.tags() {
                match tag.std_key {
                    Some(symphonia::core::meta::StandardTagKey::TrackTitle) => {
                        title = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Artist) => {
                        artist = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Album) => {
                        album = tag.value.to_string();
                    }
                    Some(symphonia::core::meta::StandardTagKey::TrackNumber) => {
                        track_number = tag.value.to_string().parse().ok();
                    }
                    Some(symphonia::core::meta::StandardTagKey::DiscNumber) => {
                        disc_number = tag.value.to_string().parse().ok();
                    }
                    Some(symphonia::core::meta::StandardTagKey::Date) => {
                        release_year = tag
                            .value
                            .to_string()
                            .split('-')
                            .next()
                            .and_then(|y| y.parse().ok());
                    }
                    Some(symphonia::core::meta::StandardTagKey::Genre) => {
                        genre = Some(tag.value.to_string());
                    }
                    _ => {
                        // Handle non-standard tags
                        match tag.key.to_uppercase().as_str() {
                            "TITLE"
                                if title
                                    == path
                                        .file_stem()
                                        .and_then(|n| n.to_str())
                                        .unwrap_or("Unknown") =>
                            {
                                title = tag.value.to_string();
                            }
                            "ARTIST" if artist == "Unknown Artist" => {
                                artist = tag.value.to_string();
                            }
                            "ALBUM" if album == "Unknown Album" => {
                                album = tag.value.to_string();
                            }
                            "TRACKNUMBER" if track_number.is_none() => {
                                track_number = tag.value.to_string().parse().ok();
                            }
                            "DISCNUMBER" if disc_number.is_none() => {
                                disc_number = tag.value.to_string().parse().ok();
                            }
                            "DATE" if release_year.is_none() => {
                                release_year = tag
                                    .value
                                    .to_string()
                                    .split('-')
                                    .next()
                                    .and_then(|y| y.parse().ok());
                            }
                            "GENRE" if genre.is_none() => {
                                genre = Some(tag.value.to_string());
                            }
                            _ => {}
                        }
                    }
                }
            }
        }

        // Calculate duration
        if let Some(track) = probed.format.tracks().first() {
            if let Some(n_frames) = track.codec_params.n_frames {
                if let Some(sample_rate) = track.codec_params.sample_rate {
                    duration = (n_frames as f64 / sample_rate as f64) as u32;
                }
            }
        }

        // Extract artwork
        let mut artwork = Artwork {
            thumbnail: None,
            full_art: ArtworkSource::None,
        };

        // Try to get embedded artwork
        if let Some(visual_meta) = probed.format.metadata().current().and_then(|meta| {
            meta.visuals()
                .iter()
                .find(|v| v.media_type.starts_with("image/"))
        }) {
            artwork.thumbnail = Some(visual_meta.data.to_vec());
        } else {
            // Look for cover art files in the same directory
            if let Some(parent) = path.parent() {
                let cover_filenames = [
                    "cover.jpg",
                    "cover.jpeg",
                    "cover.png",
                    "folder.jpg",
                    "folder.jpeg",
                    "folder.png",
                    "album.jpg",
                    "album.jpeg",
                    "album.png",
                    "front.jpg",
                    "front.jpeg",
                    "front.png",
                ];

                for filename in cover_filenames.iter() {
                    let cover_path = parent.join(filename);
                    if cover_path.exists() {
                        artwork.full_art = ArtworkSource::Local { path: cover_path };
                        break;
                    }
                }
            }
        }

        // Get file format from extension
        let file_format = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("unknown")
            .to_lowercase();

        Ok(Track {
            id,
            title,
            artist,
            album,
            duration,
            track_number,
            disc_number,
            release_year,
            genre,
            artwork,
            source: PlaybackSource::Local {
                file_format,
                file_size,
                path: path.to_path_buf(),
            },
        })
    }
}
