use crate::services::models::Track;
use crate::services::audio_player::AudioBackend;
use async_trait::async_trait;
use gstreamer as gst;
use gstreamer::prelude::*;
use gst::glib;
use parking_lot::RwLock;
use std::any::Any;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

#[derive(Debug)]
pub struct LocalAudioBackend {
    pipeline: Arc<RwLock<Option<gst::Element>>>,
    is_playing: Arc<RwLock<bool>>,
    current_duration: Arc<RwLock<Option<Duration>>>,
    current_path: Arc<RwLock<Option<PathBuf>>>,
}

impl LocalAudioBackend {
    pub fn new() -> Result<Self, Box<dyn std::error::Error + Send + Sync>> {
        // Initialize GStreamer
        gst::init()?;

        Ok(Self {
            pipeline: Arc::new(RwLock::new(None)),
            is_playing: Arc::new(RwLock::new(false)),
            current_duration: Arc::new(RwLock::new(None)),
            current_path: Arc::new(RwLock::new(None)),
        })
    }

    fn setup_pipeline(&self, uri: &str) -> Result<gst::Element, Box<dyn std::error::Error + Send + Sync>> {
        // Create playbin element
        let playbin = gst::ElementFactory::make("playbin")
            .name("player")
            .build()
            .map_err(|e| format!("Failed to create playbin: {}", e))?;

        // Set up the bus message handling
        let pipeline_weak = playbin.downgrade();
        let is_playing = Arc::clone(&self.is_playing);
        playbin
            .bus()
            .unwrap()
            .add_watch(move |_, msg| {
                if let Some(pipeline) = pipeline_weak.upgrade() {
                    match msg.view() {
                        gst::MessageView::Error(err) => {
                            eprintln!(
                                "GStreamer error from {:?}: {} ({:?})",
                                err.src().map(|s| s.path_string()),
                                err.error(),
                                err.debug()
                            );
                            pipeline.set_state(gst::State::Null).unwrap();
                            *is_playing.write() = false;
                        }
                        gst::MessageView::Eos(_) => {
                            pipeline.set_state(gst::State::Null).unwrap();
                            *is_playing.write() = false;
                        }
                        gst::MessageView::StateChanged(state) => {
                            // Compare the source object with our pipeline
                            let is_our_pipeline = state
                                .src()
                                .map(|s| s.type_() == pipeline.type_())
                                .unwrap_or(false);
                            
                            if is_our_pipeline {
                                println!(
                                    "Pipeline state changed from {:?} to {:?}",
                                    state.old(),
                                    state.current()
                                );
                            }
                        }
                        _ => (),
                    }
                }
                gst::glib::ControlFlow::Continue
            })
            .expect("Failed to add bus watch");

        // Set up audio properties
        playbin.set_property("uri", uri);
        playbin.set_property("volume", 1.0);

        // Configure audio sink
        let audio_sink = gst::ElementFactory::make("autoaudiosink")
            .build()
            .map_err(|e| format!("Failed to create audio sink: {}", e))?;

        playbin.set_property("audio-sink", &audio_sink);

        Ok(playbin)
    }

    fn get_position_from_pipeline(pipeline: &gst::Element) -> Option<Duration> {
        let position = pipeline.query_position::<gst::ClockTime>();
        position.map(|p| Duration::from_nanos(p.nseconds()))
    }

    fn get_duration_from_pipeline(pipeline: &gst::Element) -> Option<Duration> {
        let duration = pipeline.query_duration::<gst::ClockTime>();
        duration.map(|d| Duration::from_nanos(d.nseconds()))
    }

    fn ensure_state_change(
        pipeline: &gst::Element,
        state: gst::State,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        let state_change = pipeline.set_state(state)?;
        match state_change {
            gst::StateChangeSuccess::Success => Ok(()),
            gst::StateChangeSuccess::Async => {
                // Wait for state change to complete with timeout
                let timeout = gst::ClockTime::from_seconds(1);
                let (change_result, current, pending) = pipeline.state(timeout);
                
                match change_result {
                    Ok(gst::StateChangeSuccess::Success) if current == state => Ok(()),
                    Ok(success) => Err(format!(
                        "Unexpected state change result: {:?}, current: {:?}, pending: {:?}",
                        success, current, pending
                    ).into()),
                    Err(err) => Err(format!(
                        "State change error: {:?}, current: {:?}, pending: {:?}",
                        err, current, pending
                    ).into()),
                }
            }
            gst::StateChangeSuccess::NoPreroll => Ok(()), // Acceptable for live sources
            _ => Err(format!("Failed to change state to {:?}", state).into()),
        }
    }
}

impl AudioBackend for LocalAudioBackend {
    fn play(&self, track: &Track) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        // Stop any currently playing audio
        self.stop();

        // Get the file path from the track's source
        if let crate::services::models::PlaybackSource::Local { path, .. } = &track.source {
            // Update the current path
            *self.current_path.write() = Some(path.clone());

            // Create properly encoded URI from path
            let uri = glib::filename_to_uri(path, None)
                .map_err(|e| format!("Failed to create URI from path: {}", e))?;

            // Setup new pipeline
            let pipeline = self.setup_pipeline(&uri)?;

            // Set to playing state
            Self::ensure_state_change(&pipeline, gst::State::Playing)?;

            // Store pipeline and update state
            *self.pipeline.write() = Some(pipeline);
            *self.is_playing.write() = true;

            // Get and store duration
            if let Some(pipeline) = &*self.pipeline.read() {
                *self.current_duration.write() = Self::get_duration_from_pipeline(pipeline);
            }

            Ok(())
        } else {
            Err("Not a local audio source".into())
        }
    }

    fn stop(&self) {
        if let Some(pipeline) = self.pipeline.write().take() {
            let _ = Self::ensure_state_change(&pipeline, gst::State::Null);
        }
        *self.is_playing.write() = false;
        *self.current_duration.write() = None;
    }

    fn pause(&self) {
        if let Some(pipeline) = &*self.pipeline.read() {
            if let Ok(()) = Self::ensure_state_change(pipeline, gst::State::Paused) {
                *self.is_playing.write() = false;
            }
        }
    }

    fn resume(&self) {
        if let Some(pipeline) = &*self.pipeline.read() {
            if let Ok(()) = Self::ensure_state_change(pipeline, gst::State::Playing) {
                *self.is_playing.write() = true;
            }
        }
    }

    fn is_playing(&self) -> bool {
        *self.is_playing.read()
    }

    fn get_position(&self) -> Option<Duration> {
        if let Some(pipeline) = &*self.pipeline.read() {
            Self::get_position_from_pipeline(pipeline)
        } else {
            None
        }
    }

    fn set_position(&self, position: Duration) {
        if let Some(pipeline) = &*self.pipeline.read() {
            let position = position.as_nanos() as u64;
            let _ = pipeline.seek_simple(
                gst::SeekFlags::FLUSH | gst::SeekFlags::KEY_UNIT | gst::SeekFlags::ACCURATE,
                gst::ClockTime::from_nseconds(position),
            );
        }
    }

    fn get_duration(&self) -> Option<Duration> {
        *self.current_duration.read()
    }

    fn set_volume(&self, volume: f64) {
        if let Some(pipeline) = &*self.pipeline.read() {
            pipeline.set_property("volume", volume.clamp(0.0, 1.0));
        }
    }

    fn as_any(&self) -> &(dyn Any + 'static) {
        self
    }
}