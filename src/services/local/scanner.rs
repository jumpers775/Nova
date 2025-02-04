use std::error::Error;
use std::path::Path;
use walkdir::WalkDir;

pub struct FileScanner;

impl FileScanner {
    pub fn scan_directory(
        path: &Path,
    ) -> Result<Vec<std::path::PathBuf>, Box<dyn Error + Send + Sync>> {
        let mut music_files = Vec::new();

        for entry in WalkDir::new(path)
            .follow_links(true)
            .into_iter()
            .filter_map(|e| e.ok())
        {
            let path = entry.path();
            if Self::is_music_file(path) {
                music_files.push(path.to_path_buf());
            }
        }

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
}
