use super::error::ServiceError;
use super::models::{Album, Artist, PlayableItem, Track};
use super::traits::MusicProvider;
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

    pub async fn search_all(&self, query: &str) -> Result<Vec<PlayableItem>, ServiceError> {
        // TODO:
        // - Improve sorting algorithm
        // - Implement pagination

        const RESULTS_PER_PAGE: usize = 20;
        let offset = 0; // pagination not implemented

        let mut all_results = Vec::new();
        let providers = self.providers.read().await;

        for (provider_name, provider) in providers.iter() {
            match provider.search(query, RESULTS_PER_PAGE, offset).await {
                Ok(tracks) => {
                    all_results.extend(tracks.into_iter().map(|track| PlayableItem {
                        track,
                        provider: provider_name.clone(),
                        added_at: Utc::now(),
                    }));
                }
                Err(e) => {
                    eprintln!("Error searching in {}: {}", provider_name, e);
                }
            }
        }

        // Sort combined results.
        all_results.sort_by(|a, b| a.track.title.cmp(&b.track.title));

        Ok(all_results)
    }
}
