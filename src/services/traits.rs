use async_trait::async_trait;
use std::error::Error;

use super::models::{Album, Artist, Track};

#[async_trait]
pub trait MusicProvider: std::fmt::Debug + Send + Sync {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error>>;
    async fn get_albums(&self) -> Result<Vec<Album>, Box<dyn Error>>;
    async fn get_artists(&self) -> Result<Vec<Artist>, Box<dyn Error>>;
    async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error>>;
}
