use super::models::{Album, Artist, Track};
use crate::services::models::{SearchResults, SearchWeights};
use crate::services::PlayableItem;
use async_trait::async_trait;
use chrono::Utc;
use std::error::Error;

#[async_trait]
pub trait MusicProvider: std::fmt::Debug + Send + Sync {
    async fn get_tracks(&self) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>>;
    async fn get_albums(&self) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>>;
    async fn get_artists(&self) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>>;
    async fn search(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>>;

    async fn search_tracks(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Track>, Box<dyn Error + Send + Sync>>;

    async fn search_albums(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Album>, Box<dyn Error + Send + Sync>>;

    async fn search_artists(
        &self,
        query: &str,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<Artist>, Box<dyn Error + Send + Sync>>;

    async fn search_all(
        &self,
        query: &str,
        weights: &SearchWeights,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResults, Box<dyn Error + Send + Sync>>;
}
