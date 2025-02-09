use crate::services::audio_player::AudioPlayer;
use crate::services::models::Track;
use gtk::glib;
use gtk::glib::ControlFlow;
use gtk::prelude::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Duration;

#[derive(Debug)]
pub struct Player {
    audio_player: Rc<AudioPlayer>,
    play_button: gtk::Button,
    mute_button: gtk::Button,
    volume_scale: gtk::Scale,
    current_song_label: gtk::Label,
    current_artist_label: gtk::Label,
    current_album_art: gtk::Image,
    is_playing: Rc<RefCell<bool>>,
    is_muted: Rc<RefCell<bool>>,
    last_volume: Rc<RefCell<f64>>,
    progress_bar: gtk::Scale,
    current_time_label: gtk::Label,
    total_time_label: gtk::Label,
    progress_update_source_id: RefCell<Option<glib::SourceId>>,
}

impl Clone for Player {
    fn clone(&self) -> Self {
        Self {
            audio_player: self.audio_player.clone(),
            play_button: self.play_button.clone(),
            mute_button: self.mute_button.clone(),
            volume_scale: self.volume_scale.clone(),
            current_song_label: self.current_song_label.clone(),
            current_artist_label: self.current_artist_label.clone(),
            current_album_art: self.current_album_art.clone(),
            is_playing: self.is_playing.clone(),
            is_muted: self.is_muted.clone(),
            last_volume: self.last_volume.clone(),
            progress_bar: self.progress_bar.clone(),
            current_time_label: self.current_time_label.clone(),
            total_time_label: self.total_time_label.clone(),
            progress_update_source_id: RefCell::new(None),
        }
    }
}

impl Player {
    pub fn new(
        audio_player: AudioPlayer,
        play_button: gtk::Button,
        mute_button: gtk::Button,
        volume_scale: gtk::Scale,
        current_song_label: gtk::Label,
        current_artist_label: gtk::Label,
        current_album_art: gtk::Image,
        progress_bar: gtk::Scale,
        current_time_label: gtk::Label,
        total_time_label: gtk::Label,
    ) -> Self {
        let audio_player = Rc::new(audio_player);
        let is_playing = Rc::new(RefCell::new(false));
        let is_muted = Rc::new(RefCell::new(false));
        let last_volume = Rc::new(RefCell::new(100.0));

        let player = Self {
            audio_player: audio_player.clone(),
            play_button: play_button.clone(),
            mute_button: mute_button.clone(),
            volume_scale: volume_scale.clone(),
            current_song_label,
            current_artist_label,
            current_album_art,
            is_playing: is_playing.clone(),
            is_muted: is_muted.clone(),
            last_volume: last_volume.clone(),
            progress_bar: progress_bar.clone(),
            current_time_label,
            total_time_label,
            progress_update_source_id: RefCell::new(None),
        };

        // Set initial volume
        volume_scale.set_value(100.0);

        // Set up volume scale handler
        let is_muted_clone = player.is_muted.clone();
        let last_volume_clone = last_volume.clone();
        let mute_button_clone = mute_button.clone();
        let audio_player_clone = audio_player.clone();
        volume_scale.connect_value_changed(move |scale| {
            let value = scale.value();

            // Update mute button icon based on volume level
            if !*is_muted_clone.borrow() {
                *last_volume_clone.borrow_mut() = value;
                audio_player_clone.set_volume(value / 100.0);

                let icon = match value {
                    v if v <= 0.0 => "audio-volume-muted-symbolic",
                    v if v <= 33.0 => "audio-volume-low-symbolic",
                    v if v <= 66.0 => "audio-volume-medium-symbolic",
                    _ => "audio-volume-high-symbolic",
                };
                mute_button_clone.set_icon_name(icon);
            }
        });

        // Set up mute button handler
        let volume_scale = volume_scale.clone();
        let audio_player_clone = audio_player.clone();
        mute_button.connect_clicked(move |button| {
            let mut muted = is_muted.borrow_mut();
            *muted = !*muted;

            if *muted {
                *last_volume.borrow_mut() = volume_scale.value();
                volume_scale.set_value(0.0);
                button.set_icon_name("audio-volume-muted-symbolic");
                audio_player_clone.set_volume(0.0);
            } else {
                let vol = *last_volume.borrow();
                volume_scale.set_value(vol);
                let icon = match vol {
                    v if v <= 0.0 => "audio-volume-muted-symbolic",
                    v if v <= 33.0 => "audio-volume-low-symbolic",
                    v if v <= 66.0 => "audio-volume-medium-symbolic",
                    _ => "audio-volume-high-symbolic",
                };
                button.set_icon_name(icon);
                audio_player_clone.set_volume(vol / 100.0);
            }
        });

        // Set up play button handler
        let audio_player_clone = audio_player.clone();
        play_button.connect_clicked(move |button| {
            let mut playing = is_playing.borrow_mut();
            *playing = !*playing;

            if *playing {
                button.set_icon_name("media-playback-pause-symbolic");
                if let Some(track) = audio_player_clone.get_current_track() {
                    audio_player_clone.resume();
                }
            } else {
                button.set_icon_name("media-playback-start-symbolic");
                audio_player_clone.pause();
            }
        });

        // Set up progress bar handler
        progress_bar.connect_change_value(move |_, _, value| {
            if let Some(duration) = audio_player.get_duration() {
                let position = Duration::from_secs_f64(value / 100.0 * duration.as_secs_f64());
                audio_player.set_position(position);
            }
            glib::Propagation::Proceed
        });

        // Initialize progress bar
        progress_bar.set_draw_value(false);
        progress_bar.set_range(0.0, 100.0);
        player.connect_progress_bar();

        player
    }

    fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{}:{:02}", mins, secs)
    }

    fn start_progress_updates(&self) {
        if self.progress_update_source_id.borrow().is_some() {
            return;
        }

        let player = Rc::new(self.clone());
        let weak_player = Rc::downgrade(&player);

        let source_id = glib::timeout_add_local(Duration::from_millis(250), move || {
            let player = match weak_player.upgrade() {
                Some(player) => player,
                None => return ControlFlow::Break,
            };

            if !*player.is_playing.borrow() || !player.audio_player.is_playing() {
                if *player.is_playing.borrow() {
                    player.next();
                }
                return ControlFlow::Break;
            }

            // Only update if scale is not being dragged
            if !player.progress_bar.has_focus() {
                // Move expensive operations to background
                let weak_player = weak_player.clone();
                glib::idle_add_local_once(move || {
                    if let Some(player) = weak_player.upgrade() {
                        if let Some(position) = player.audio_player.get_position() {
                            if let Some(duration) = player.audio_player.get_duration() {
                                let progress = (position.as_secs_f64() / duration.as_secs_f64() * 100.0).min(100.0);
                                player.progress_bar.set_value(progress);
                                player.current_time_label.set_text(&Player::format_duration(position));
                                player.total_time_label.set_text(&Player::format_duration(duration));
                            }
                        }
                    }
                });
            }
            ControlFlow::Continue
        });

        self.progress_update_source_id.replace(Some(source_id));
    }

    fn stop_progress_updates(&self) {
        if let Some(id) = self.progress_update_source_id.borrow_mut().take() {
            id.remove();
        }
        self.progress_bar.set_value(0.0);
        self.current_time_label.set_text("0:00");
        self.total_time_label.set_text("0:00");
    }

    pub fn play_track(
        &self,
        track: &Track,
    ) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        match self.audio_player.play(track) {
            Ok(_) => {
                self.set_playing(true);
                self.update_now_playing(track);
                self.start_progress_updates();
                Ok(())
            }
            Err(e) => {
                // Reset UI on error
                self.set_playing(false);
                self.stop_progress_updates();
                self.current_song_label.set_text("Error playing track");
                self.current_artist_label.set_text(&e.to_string());
                Err(e)
            }
        }
    }

    pub fn set_playing(&self, playing: bool) {
        *self.is_playing.borrow_mut() = playing;
        self.play_button.set_icon_name(if playing {
            self.start_progress_updates();
            "media-playback-pause-symbolic"
        } else {
            self.stop_progress_updates();
            "media-playback-start-symbolic"
        });
    }

    pub fn is_playing(&self) -> bool {
        *self.is_playing.borrow()
    }

    pub fn update_now_playing(&self, track: &Track) {
        self.current_song_label.set_text(&track.title);
        self.current_artist_label.set_text(&track.artist);

        // Update album art
        if let Some(data) = &track.artwork.thumbnail {
            let bytes = glib::Bytes::from(data);
            let stream = gtk::gio::MemoryInputStream::from_bytes(&bytes);
            if let Ok(pixbuf) =
                gdk_pixbuf::Pixbuf::from_stream(&stream, None::<&gtk::gio::Cancellable>)
            {
                if let Some(scaled) = pixbuf.scale_simple(96, 96, gdk_pixbuf::InterpType::Bilinear)
                {
                    let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                    self.current_album_art.set_paintable(Some(&paintable));
                    return;
                }
            }
        }

        // Fallback to default icon if no artwork
        self.current_album_art
            .set_icon_name(Some("audio-x-generic-symbolic"));
        self.current_album_art.set_pixel_size(96); // Ensure fallback icon is also large
    }

    pub fn next(&self) {
        if let Some(track) = self.audio_player.next() {
            if let Err(e) = self.play_track(&track) {
                println!("Error playing next track: {}", e);
            }
        }
    }

    pub fn previous(&self) {
        if let Some(track) = self.audio_player.previous() {
            if let Err(e) = self.play_track(&track) {
                println!("Error playing previous track: {}", e);
            }
        }
    }

    // Handle progress bar seeking
    pub fn connect_progress_bar(&self) {
        let progress_bar = self.progress_bar.clone();
        let audio_player = self.audio_player.clone();
        let is_dragging = Rc::new(RefCell::new(false));
        let is_dragging_clone = is_dragging.clone();
        
        // Handle dragging state
        progress_bar.connect_change_value(move |scale, scroll_type, value| {
            if scroll_type == gtk::ScrollType::Jump {
                *is_dragging_clone.borrow_mut() = true;
            }
            scale.set_value(value);
            glib::Propagation::Stop
        });

        // Handle seek on release
        progress_bar.connect_value_changed(move |scale| {
            if *is_dragging.borrow() {
                if let Some(duration) = audio_player.get_duration() {
                    let position = Duration::from_secs_f64(scale.value() / 100.0 * duration.as_secs_f64());
                    audio_player.set_position(position);
                }
                *is_dragging.borrow_mut() = false;
            }
        });
    }
}
