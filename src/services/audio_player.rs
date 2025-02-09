use crate::services::local::LocalAudioBackend;
use crate::services::models::{PlayableItem, Track};
use async_trait::async_trait;
use parking_lot::RwLock;
use std::any::Any;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct AudioPlayer {
    backend: Arc<dyn AudioBackend>,
    queue: Arc<RwLock<Queue>>,
    current_track: Arc<RwLock<Option<Track>>>,
}

#[async_trait::async_trait]
pub trait AudioBackend: Send + Sync + std::fmt::Debug + Any {
    fn play(&self, track: &Track) -> Result<(), Box<dyn std::error::Error + Send + Sync>>;
    fn stop(&self);
    fn pause(&self);
    fn resume(&self);
    fn is_playing(&self) -> bool;
    fn get_position(&self) -> Option<Duration>;
    fn set_position(&self, position: Duration);
    fn get_duration(&self) -> Option<Duration>;
    fn set_volume(&self, volume: f64);

    fn as_any(&self) -> &(dyn Any + 'static);
}

impl AudioPlayer {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        let backend = Arc::new(LocalAudioBackend::new()?);

        Ok(Self {
            backend,
            queue: Arc::new(RwLock::new(Queue::new(Vec::new()))),
            current_track: Arc::new(RwLock::new(None)),
        })
    }

    pub fn load_queue(&self, tracks: Vec<PlayableItem>) {
        let mut queue = self.queue.write();
        *queue = Queue::new(tracks);
    }

    pub fn play(&self, track: &Track) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.backend.play(track)?;
        *self.current_track.write() = Some(track.clone());
        Ok(())
    }

    pub fn stop(&self) {
        self.backend.stop();
        *self.current_track.write() = None;
    }

    pub fn pause(&self) {
        self.backend.pause();
    }

    pub fn resume(&self) {
        self.backend.resume();
    }

    pub fn next(&self) -> Option<Track> {
        let mut queue = self.queue.write();
        if let Some(next_track) = queue.next() {
            let _ = self.play(&next_track);
            Some(next_track)
        } else {
            None
        }
    }

    pub fn previous(&self) -> Option<Track> {
        let mut queue = self.queue.write();
        if let Some(prev_track) = queue.previous() {
            let _ = self.play(&prev_track);
            Some(prev_track)
        } else {
            None
        }
    }

    pub fn get_queue(&self) -> Vec<PlayableItem> {
        self.queue.read().get_tracks().to_vec()
    }

    pub fn is_playing(&self) -> bool {
        self.backend.is_playing()
    }

    pub fn get_position(&self) -> Option<Duration> {
        self.backend.get_position()
    }

    pub fn set_position(&self, position: Duration) {
        self.backend.set_position(position)
    }

    pub fn get_duration(&self) -> Option<Duration> {
        self.backend.get_duration()
    }

    pub fn get_current_track(&self) -> Option<Track> {
        self.current_track.read().clone()
    }

    pub fn set_volume(&self, volume: f64) {
        self.backend.set_volume(volume);
    }
}

#[derive(Debug)]
pub struct Queue {
    tracks: Vec<PlayableItem>,
    current_index: Option<usize>,
}

impl Queue {
    pub fn new(tracks: Vec<PlayableItem>) -> Self {
        Self {
            tracks,
            current_index: None,
        }
    }

    pub fn next(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

        self.current_index = Some(match self.current_index {
            Some(idx) if idx + 1 < self.tracks.len() => idx + 1,
            _ => 0,
        });

        self.current_track().cloned()
    }

    pub fn previous(&mut self) -> Option<Track> {
        if self.tracks.is_empty() {
            return None;
        }

        self.current_index = Some(match self.current_index {
            Some(idx) if idx > 0 => idx - 1,
            _ => self.tracks.len() - 1,
        });

        self.current_track().cloned()
    }

    pub fn current_track(&self) -> Option<&Track> {
        self.current_index.map(|idx| &self.tracks[idx].track)
    }

    pub fn get_tracks(&self) -> &[PlayableItem] {
        &self.tracks
    }
}
