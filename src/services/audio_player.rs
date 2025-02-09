use crate::services::models::{PlayableItem, Track};
use async_trait::async_trait;
use gtk::glib;
use parking_lot::RwLock;
use rodio::{Decoder, OutputStream, OutputStreamHandle, Sink, Source};
use std::any::Any;
use std::cell::RefCell;
use std::fs::File;
use std::io::BufReader;
use std::sync::Arc;
use std::time::Duration;
use std::time::Instant;

thread_local! {
    static AUDIO_STREAM: RefCell<Option<(OutputStream, OutputStreamHandle)>> = RefCell::new(None);
}

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
        // Use as_any() directly from the AudioBackend trait
        if let Some(backend) = self.backend.as_any().downcast_ref::<LocalAudioBackend>() {
            backend.set_volume(volume);
        }
    }
}

pub struct LocalAudioBackend {
    sink: Arc<RwLock<Option<Sink>>>,
    is_playing: Arc<RwLock<bool>>,
    current_duration: Arc<RwLock<Option<Duration>>>,
    start_time: Arc<RwLock<Option<Instant>>>,
}

impl std::fmt::Debug for LocalAudioBackend {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalAudioBackend")
            .field("is_playing", &self.is_playing)
            .field("current_duration", &self.current_duration)
            .field("start_time", &self.start_time)
            .finish()
    }
}

impl LocalAudioBackend {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Initialize stream on the main thread
        glib::idle_add_local(|| {
            if AUDIO_STREAM.with(|s| s.borrow().is_none()) {
                if let Ok((stream, handle)) = OutputStream::try_default() {
                    AUDIO_STREAM.with(|s| *s.borrow_mut() = Some((stream, handle)));
                }
            }
            glib::ControlFlow::Break
        });

        Ok(Self {
            sink: Arc::new(RwLock::new(None)),
            is_playing: Arc::new(RwLock::new(false)),
            current_duration: Arc::new(RwLock::new(None)),
            start_time: Arc::new(RwLock::new(None)),
        })
    }

    fn get_stream_handle() -> Option<OutputStreamHandle> {
        AUDIO_STREAM.with(|s| s.borrow().as_ref().map(|(_, handle)| handle.clone()))
    }

    fn set_volume(&self, vol: f64) {
        if let Some(sink) = &*self.sink.read() {
            sink.set_volume(vol as f32);
        }
    }
}

impl AudioBackend for LocalAudioBackend {
    fn play(&self, track: &Track) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let stream_handle = Self::get_stream_handle()
            .ok_or_else(|| "No audio output stream available".to_string())?;

        // Stop any currently playing audio
        self.stop();

        // Get the file path from the track's source
        if let crate::services::models::PlaybackSource::Local { path, .. } = &track.source {
            // Open the audio file
            let file = File::open(path)?;
            let reader = BufReader::new(file);

            // Create a new sink
            let sink = Sink::try_new(&stream_handle)?;

            // Decode and append the audio to the sink
            let source = Decoder::new(reader)?;

            // Store the duration
            let duration = source.total_duration();
            *self.current_duration.write() = duration;

            sink.append(source);

            // Store the sink and start playback
            *self.sink.write() = Some(sink);
            *self.is_playing.write() = true;
            *self.start_time.write() = Some(Instant::now());

            Ok(())
        } else {
            Err("Not a local audio source".into())
        }
    }

    fn stop(&self) {
        if let Some(sink) = self.sink.write().take() {
            sink.stop();
        }
        *self.is_playing.write() = false;
        *self.current_duration.write() = None;
        *self.start_time.write() = None;
    }

    fn pause(&self) {
        if let Some(sink) = &*self.sink.read() {
            sink.pause();
            *self.is_playing.write() = false;
        }
    }

    fn resume(&self) {
        if let Some(sink) = &*self.sink.read() {
            sink.play();
            *self.is_playing.write() = true;
            *self.start_time.write() = Some(Instant::now());
        }
    }

    fn is_playing(&self) -> bool {
        *self.is_playing.read()
    }

    fn get_position(&self) -> Option<Duration> {
        if !*self.is_playing.read() {
            return None;
        }

        self.start_time.read().map(|start| start.elapsed())
    }

    fn set_position(&self, _position: Duration) {
        // Note: rodio doesn't support seeking directly
    }

    fn get_duration(&self) -> Option<Duration> {
        *self.current_duration.read()
    }

    fn set_volume(&self, volume: f64) {
        if let Some(sink) = &*self.sink.read() {
            sink.set_volume(volume as f32);
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) {
        self
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
