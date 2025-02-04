use super::error::ServiceError;
use super::models::{Album, Artist, PlayableItem, Track};
use super::traits::MusicProvider;
use crate::services::models::{SearchResults, SearchWeights};
use async_trait::async_trait;
use chrono::Utc;
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;

#[derive(Debug)]
pub struct ServiceManager {
    providers: Arc<RwLock<HashMap<String, Box<dyn MusicProvider + Send + Sync + 'static>>>>,
}

impl ServiceManager {
    pub fn new() -> Self {
        Self {
            providers: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    pub async fn register_provider(
        &self,
        name: &str,
        provider: Box<dyn MusicProvider + Send + Sync>,
    ) {
        let mut providers = self.providers.write().await;
        providers.insert(name.to_string(), provider);
    }

    pub async fn get_all_tracks(&self) -> Result<Vec<PlayableItem>, ServiceError> {
        let mut all_tracks = Vec::new();
        let providers = self.providers.read().await;

        for (provider_name, provider) in providers.iter() {
            match provider.get_tracks().await {
                Ok(tracks) => {
                    all_tracks.extend(tracks.into_iter().map(|track| PlayableItem {
                        track,
                        provider: provider_name.clone(),
                        added_at: Utc::now(),
                    }));
                }
                Err(e) => {
                    eprintln!("Error getting tracks from {}: {}", provider_name, e);
                }
            }
        }

        Ok(all_tracks)
    }

    pub async fn search_all(
        &self,
        query: &str,
        weights: Option<SearchWeights>,
        limit: usize,
        offset: usize,
    ) -> Result<SearchResults, ServiceError> {
        let weights = weights.unwrap_or_default();
        let providers = self.providers.read().await;
        let mut all_results = SearchResults {
            tracks: Vec::new(),
            albums: Vec::new(),
            artists: Vec::new(),
        };

        // Query all providers concurrently
        let mut futures = Vec::new();
        for (provider_name, provider) in providers.iter() {
            let provider_name = provider_name.clone();
            let query = query.to_string();
            let weights = weights.clone();

            let future = async move {
                match provider.search_all(&query, &weights, limit, offset).await {
                    Ok(results) => Some((provider_name, results)),
                    Err(e) => {
                        eprintln!("Error searching in {}: {}", provider_name, e);
                        None
                    }
                }
            };
            futures.push(future);
        }

        // Wait for all searches to complete
        for result in futures::future::join_all(futures).await {
            if let Some((provider_name, results)) = result {
                all_results.tracks.extend(results.tracks);
                all_results.albums.extend(results.albums);
                all_results.artists.extend(results.artists);
            }
        }

        Ok(all_results)
    }
}
