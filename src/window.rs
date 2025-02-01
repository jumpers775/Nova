/* window.rs
 *
 * Copyright 2025 Luca Mignatti
 *
 * This program is free software: you can redistribute it and/or modify
 * it under the terms of the GNU General Public License as published by
 * the Free Software Foundation, either version 3 of the License, or
 * (at your option) any later version.
 *
 * This program is distributed in the hope that it will be useful,
 * but WITHOUT ANY WARRANTY; without even the implied warranty of
 * MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
 * GNU General Public License for more details.
 *
 * You should have received a copy of the GNU General Public License
 * along with this program.  If not, see <https://www.gnu.org/licenses/>.
 *
 * SPDX-License-Identifier: GPL-3.0-or-later
 */
use crate::services::models::{Artwork, ArtworkSource, Track};
use adw::subclass::prelude::*;
use chrono::{DateTime, Utc};
use gdk_pixbuf::Pixbuf;
use glib::clone;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

mod imp {
    use super::*;
    use crate::services::{LocalMusicProvider, ServiceManager};
    use std::path::PathBuf;
    use std::sync::Arc;
    use tokio::runtime::Runtime;

    #[derive(Debug, Default, gtk::CompositeTemplate)]
    #[template(resource = "/com/lucamignatti/nova/window.ui")]
    pub struct NovaWindow {
        // Template widgets
        #[template_child]
        pub home_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub header_search_entry: TemplateChild<gtk::SearchEntry>,
        #[template_child]
        pub queue_flap: TemplateChild<adw::Flap>,
        #[template_child]
        pub main_stack: TemplateChild<adw::ViewStack>,
        #[template_child]
        pub artists_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub albums_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub playlists_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub liked_row: TemplateChild<adw::ActionRow>,
        #[template_child]
        pub queue_toggle: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub play_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub prev_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub next_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub shuffle_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub loop_button: TemplateChild<gtk::ToggleButton>,
        #[template_child]
        pub mute_button: TemplateChild<gtk::Button>,
        #[template_child]
        pub current_song: TemplateChild<gtk::Label>,
        #[template_child]
        pub current_album_art: TemplateChild<gtk::Image>,
        #[template_child]
        pub song_progress_bar: TemplateChild<gtk::Scale>,
        #[template_child]
        pub volume_scale: TemplateChild<gtk::Scale>,
        #[template_child]
        pub current_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub total_time_label: TemplateChild<gtk::Label>,
        #[template_child]
        pub sidebar_list: TemplateChild<gtk::ListBox>,
        pub service_manager: RefCell<Option<Arc<ServiceManager>>>,
        #[template_child]
        pub queue_list: TemplateChild<gtk::ListBox>,
        #[template_child(id = "search_results_box")]
        pub search_results_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub current_song_artist: TemplateChild<gtk::Label>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for NovaWindow {
        const NAME: &'static str = "NovaWindow";
        type Type = super::NovaWindow;
        type ParentType = adw::ApplicationWindow;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
        }

        fn instance_init(obj: &glib::subclass::InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for NovaWindow {
        fn constructed(&self) {
            self.parent_constructed();

            let manager = ServiceManager::new();

            let music_dir = dirs::audio_dir().unwrap_or_else(|| {
                PathBuf::from(&format!("{}/Music", std::env::var("HOME").unwrap()))
            });
            let local_provider = Box::new(LocalMusicProvider::new(music_dir));

            let manager = Arc::new(manager);
            let manager_clone = manager.clone();

            gtk::glib::spawn_future_local(async move {
                manager_clone
                    .register_provider("local", local_provider)
                    .await;
            });

            self.service_manager.replace(Some(manager));

            // Connect search entry focus and text change signals
            let search_entry = self.header_search_entry.clone();
            let main_stack = self.main_stack.clone();
            let sidebar_list = self.sidebar_list.clone();
            let home_button = self.home_button.clone();
            let search_results_box = self.search_results_box.clone();

            // When search entry gains focus using an EventControllerFocus
            let focus_controller = gtk::EventControllerFocus::new();
            focus_controller.connect_enter(clone!(@strong main_stack, @strong home_button, @strong sidebar_list => move |_controller| {
                main_stack.set_visible_child_name("search");
                home_button.remove_css_class("selected");
                sidebar_list.unselect_all();
            }));
            search_entry.add_controller(focus_controller);

            // When search text changes
            let service_manager = self.service_manager.clone();
            let search_results_box = self.search_results_box.clone();
            let current_song = self.current_song.clone();
            let current_song_artist = self.current_song_artist.clone();
            let current_album_art = self.current_album_art.clone();

            self.header_search_entry.connect_changed(move |entry| {
                let query = entry.text().to_string();

                // Clear previous results using a while loop over first_child
                while let Some(child) = search_results_box.first_child() {
                    search_results_box.remove(&child);
                }

                if query.is_empty() {
                    return;
                }

                if let Some(manager) = service_manager.borrow().as_ref() {
                    let manager_clone = manager.clone();
                    let current_song = current_song.clone();
                    let current_song_artist = current_song_artist.clone();
                    let current_album_art = current_album_art.clone();

                    gtk::glib::spawn_future_local(clone!(@strong search_results_box => async move {
                        match manager_clone.search_all(&query).await {
                            Ok(tracks) => {
                                for (i, track) in tracks.iter().enumerate() {
                                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);

                                    // Album art handling
                                    let art = match &track.track.artwork {
                                        Artwork { thumbnail: Some(data), .. } => {
                                            let bytes = glib::Bytes::from(data);
                                            let stream = gio::MemoryInputStream::from_bytes(&bytes);
                                            if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
                                                let size = if i == 0 { 96 } else { 48 };
                                                if let Some(scaled) = pixbuf.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear) {
                                                    let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                                    let img = gtk::Image::from_paintable(Some(&paintable));
                                                    img.add_css_class("album-art");
                                                    img.add_css_class("rounded");
                                                    img.set_size_request(size, size);
                                                    img
                                                } else {
                                                    let img = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                                                    img.add_css_class("album-art");
                                                    img.add_css_class("rounded");
                                                    img
                                                }
                                            } else {
                                                let img = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                                                img.add_css_class("album-art");
                                                img.add_css_class("rounded");
                                                img
                                            }
                                        },
                                        Artwork { full_art: ArtworkSource::Local { path }, .. } => {
                                            if let Ok(pixbuf) = Pixbuf::from_file(path) {
                                                let size = if i == 0 { 96 } else { 48 };
                                                if let Some(scaled) = pixbuf.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear) {
                                                    let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                                    let img = gtk::Image::from_paintable(Some(&paintable));
                                                    img.add_css_class("album-art");
                                                    img.add_css_class("rounded");
                                                    img.set_size_request(size, size);
                                                    img
                                                } else {
                                                    let img = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                                                    img.add_css_class("album-art");
                                                    img.add_css_class("rounded");
                                                    img
                                                }
                                            } else {
                                                let img = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                                                img.add_css_class("album-art");
                                                img.add_css_class("rounded");
                                                img
                                            }
                                        },
                                        _ => {
                                            let img = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                                            img.add_css_class("album-art");
                                            img.add_css_class("rounded");
                                            img
                                        },
                                    };

                                    if i == 0 {
                                        row.add_css_class("large-suggestion");
                                    } else {
                                        row.add_css_class("small-suggestion");
                                    }

                                    row.append(&art);

                                    // Text box for title and artist
                                    let text_box = gtk::Box::new(gtk::Orientation::Vertical, 4);
                                    text_box.set_margin_start(12);

                                    let title = gtk::Label::new(Some(&track.track.title));
                                    title.set_halign(gtk::Align::Start);
                                    if i == 0 {
                                        title.add_css_class("title-1");
                                    } else {
                                        title.add_css_class("title-4");
                                    }

                                    let artist = gtk::Label::new(Some(&track.track.artist));
                                    artist.set_halign(gtk::Align::Start);
                                    artist.add_css_class("dim-label");
                                    if i == 0 {
                                        artist.add_css_class("title-3");
                                    } else {
                                        artist.add_css_class("caption");
                                    }

                                    text_box.append(&title);
                                    text_box.append(&artist);
                                    row.append(&text_box);

                                    // Make row clickable using a GestureClick
                                    let gesture = gtk::GestureClick::new();
                                    let track = track.track.clone();
                                    let current_song = current_song.clone();
                                    let current_song_artist = current_song_artist.clone();
                                    let current_album_art = current_album_art.clone();

                                    gesture.connect_pressed(move |_gesture, _n_press, _x, _y| {
                                        println!("Clicked track: {}", track.title);
                                        current_song.set_label(&track.title);
                                        current_song_artist.set_label(&track.artist);
                                        match &track.artwork {
                                            Artwork { thumbnail: Some(data), .. } => {
                                                let bytes = glib::Bytes::from(data);
                                                let stream = gio::MemoryInputStream::from_bytes(&bytes);
                                                if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
                                                    if let Some(scaled) = pixbuf.scale_simple(48, 48, gdk_pixbuf::InterpType::Bilinear) {
                                                        let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                                        current_album_art.set_paintable(Some(&paintable));
                                                    }
                                                }
                                            },
                                            Artwork { full_art: ArtworkSource::Local { path }, .. } => {
                                                if let Ok(pixbuf) = Pixbuf::from_file(path) {
                                                    if let Some(scaled) = pixbuf.scale_simple(48, 48, gdk_pixbuf::InterpType::Bilinear) {
                                                        let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                                        current_album_art.set_paintable(Some(&paintable));
                                                    }
                                                }
                                            },
                                            _ => current_album_art.set_icon_name(Some("audio-x-generic-symbolic")),
                                        }
                                    });
                                    row.add_controller(gesture);

                                    search_results_box.append(&row);
                                }
                            }
                            Err(e) => eprintln!("Search error: {:?}", e),
                        }
                    }));
                }
            });

            // Rest of the original constructor code...
            // Set initial selection state
            let sidebar_list = self.sidebar_list.clone();
            let home_button = self.home_button.clone();

            glib::idle_add_local_once(move || {
                sidebar_list.unselect_all();
                home_button.add_css_class("selected");
            });

            // Initialize volume
            self.volume_scale.set_value(100.0);
            self.mute_button.set_icon_name("audio-volume-high-symbolic");

            // Setup navigation
            let main_stack = self.main_stack.clone();

            // Home button
            let home_button = self.home_button.clone();
            let sidebar_list = self.sidebar_list.clone();
            let stack = main_stack.clone();
            self.home_button.connect_clicked(move |button| {
                stack.set_visible_child_name("home");
                button.add_css_class("selected");
                sidebar_list.unselect_all();
                println!("Navigated to Home");
            });

            // ListBox navigation
            let home_button = self.home_button.clone();
            let stack = main_stack.clone();
            self.sidebar_list.connect_row_activated(move |_, row| {
                let page_name = match row.index() {
                    0 => "artists",
                    1 => "albums",
                    2 => "playlists",
                    3 => "liked",
                    _ => "home",
                };
                stack.set_visible_child_name(page_name);
                home_button.remove_css_class("selected");
                println!("Navigated to {}", page_name);
            });

            // Queue toggle with flap
            let queue_flap = self.queue_flap.clone();
            self.queue_toggle.connect_toggled(move |button| {
                queue_flap.set_reveal_flap(button.is_active());
                println!("Queue toggle: {}", button.is_active());
            });

            // Volume control state
            let volume_state = Rc::new(RefCell::new((false, 100.0)));

            // Volume scale handler
            let mute_button = self.mute_button.clone();
            let volume_state_clone = volume_state.clone();
            self.volume_scale.connect_value_changed(move |scale| {
                let value = scale.value();
                println!("Volume: {}%", value);

                let (is_muted, _) = *volume_state_clone.borrow();
                if !is_muted {
                    let icon = match value {
                        v if v <= 0.0 => "audio-volume-muted-symbolic",
                        v if v <= 33.0 => "audio-volume-low-symbolic",
                        v if v <= 66.0 => "audio-volume-medium-symbolic",
                        _ => "audio-volume-high-symbolic",
                    };
                    mute_button.set_icon_name(icon);
                }
            });

            // Mute button handler
            let volume_scale = self.volume_scale.clone();
            let volume_state_clone = volume_state.clone();
            self.mute_button.connect_clicked(move |btn| {
                let (is_muted_now, new_volume);
                {
                    let mut state = volume_state_clone.borrow_mut();

                    if state.0 {
                        is_muted_now = false;
                        new_volume = state.1;
                    } else {
                        is_muted_now = true;
                        state.1 = volume_scale.value();
                        new_volume = 0.0;
                    }

                    state.0 = is_muted_now;
                }

                volume_scale.set_value(new_volume);
                volume_scale.set_sensitive(!is_muted_now);

                if is_muted_now {
                    btn.set_icon_name("audio-volume-muted-symbolic");
                } else {
                    btn.set_icon_name("audio-volume-high-symbolic");
                }
            });

            // Progress bar updates
            self.song_progress_bar.connect_value_changed(|scale| {
                println!("Progress: {}%", scale.value());
            });

            // Play button state
            let is_playing = Rc::new(RefCell::new(false));
            self.play_button.connect_clicked(move |button| {
                let mut playing = is_playing.borrow_mut();
                *playing = !*playing;
                button.set_icon_name(if *playing {
                    "media-playback-pause-symbolic"
                } else {
                    "media-playback-start-symbolic"
                });
                println!("Play button clicked");
            });

            self.prev_button.connect_clicked(move |_| {
                println!("Previous button clicked");
            });

            self.next_button.connect_clicked(move |_| {
                println!("Next button clicked");
            });

            self.shuffle_button.connect_clicked(move |button| {
                if button.is_active() {
                    button.add_css_class("active");
                } else {
                    button.remove_css_class("active");
                }
                println!("Shuffle button clicked");
            });

            // Loop button state
            #[derive(Debug, Clone, Copy, PartialEq, Eq)]
            enum LoopState {
                Off,
                Playlist,
                Song,
            }

            let loop_state = Rc::new(RefCell::new(LoopState::Off));
            let loop_button = self.loop_button.clone();
            let loop_state_for_cb = loop_state.clone();
            loop_button.connect_clicked(move |button| {
                let mut state = loop_state_for_cb.borrow_mut();
                *state = match *state {
                    LoopState::Off => {
                        button.set_icon_name("media-playlist-repeat-symbolic");
                        button.add_css_class("active");
                        button.set_active(true);
                        LoopState::Playlist
                    }
                    LoopState::Playlist => {
                        button.set_icon_name("media-playlist-repeat-song-symbolic");
                        button.add_css_class("active");
                        button.set_active(true);
                        LoopState::Song
                    }
                    LoopState::Song => {
                        button.set_icon_name("media-playlist-repeat-symbolic");
                        button.remove_css_class("active");
                        button.set_active(false);
                        LoopState::Off
                    }
                };
                println!("Loop state is now: {:?}", state);
            });

            // Initialize ServiceManager
            let manager = ServiceManager::new();

            // Create and register local provider
            let music_dir =
                dirs::audio_dir().unwrap_or_else(|| PathBuf::from("/var/home/luca/Music"));
            let local_provider = Box::new(LocalMusicProvider::new(music_dir));

            // Store manager in RefCell
            let manager = Arc::new(manager);
            let manager_clone = manager.clone();

            // Use tokio to register providers
            let rt = Runtime::new().unwrap();
            rt.spawn(async move {
                manager_clone
                    .register_provider("local", local_provider)
                    .await;
            });

            self.service_manager.replace(Some(manager));
        }
    }
    impl WidgetImpl for NovaWindow {}
    impl WindowImpl for NovaWindow {}
    impl ApplicationWindowImpl for NovaWindow {}
    impl AdwApplicationWindowImpl for NovaWindow {}
}

glib::wrapper! {
    pub struct NovaWindow(ObjectSubclass<imp::NovaWindow>)
        @extends gtk::Widget, gtk::Window, gtk::ApplicationWindow, adw::ApplicationWindow,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl NovaWindow {
    pub fn new<P: IsA<gtk::Application>>(application: &P) -> Self {
        glib::Object::builder()
            .property("application", application)
            .build()
    }

    fn set_page(&self, page_name: &str) {
        let imp = self.imp();
        imp.main_stack.set_visible_child_name(page_name);
    }
}

pub mod window {}
