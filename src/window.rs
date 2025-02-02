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
use crate::services::PlayableItem;
use adw::subclass::prelude::*;
use chrono::{DateTime, Utc};
use gdk_pixbuf::Pixbuf;
use glib::{clone, Propagation};
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::collections::HashMap;
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
        #[template_child]
        pub search_stack: TemplateChild<gtk::Stack>,
        #[template_child]
        pub empty_search_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub search_results_scroll: TemplateChild<gtk::ScrolledWindow>,
        #[template_child]
        pub search_results_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub current_song_artist: TemplateChild<gtk::Label>,
        #[template_child]
        pub content_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub top_result_box: TemplateChild<gtk::CenterBox>,
        #[template_child]
        pub tracks_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub artists_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub albums_box: TemplateChild<gtk::Box>,
        #[template_child]
        pub no_results_page: TemplateChild<adw::StatusPage>,
        #[template_child]
        pub artists_section: TemplateChild<gtk::Box>,
        #[template_child]
        pub albums_section: TemplateChild<gtk::Box>,
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
            self.search_stack
                .add_named(&self.no_results_page.get(), Some("no_results_page"));

            // Add scroll controller to search results
            let scroll_controller =
                gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
            scroll_controller.set_propagation_phase(gtk::PropagationPhase::Capture);

            let search_results_scroll = self.search_results_scroll.clone();
            scroll_controller.connect_scroll(move |_, _, dy| {
                let adj = search_results_scroll.vadjustment();
                let increment = dy * adj.step_increment() * 3.0; // Multiply by 3 for faster scrolling
                let new_value = adj.value() + increment;
                adj.set_value(new_value.clamp(adj.lower(), adj.upper() - adj.page_size()));
                Propagation::Stop
            });

            self.search_results_box.add_controller(scroll_controller);

            // Get a clone of the ScrolledWindow for the closure.
            let search_results_scroll = self.search_results_scroll.clone();

            // Create a function to add scroll controllers
            fn create_scroll_controller(
                search_results_scroll: &gtk::ScrolledWindow,
            ) -> gtk::EventControllerScroll {
                let scroll_window = search_results_scroll.clone();
                let controller =
                    gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
                controller.set_propagation_phase(gtk::PropagationPhase::Capture);
                controller.connect_scroll(move |_controller, _dx, dy| {
                    let adj = scroll_window.vadjustment();
                    let new_value = adj.value() + (dy * adj.step_increment());
                    adj.set_value(new_value.clamp(adj.lower(), adj.upper() - adj.page_size()));
                    Propagation::Stop
                });
                controller
            }

            // Attach controllers to result boxes
            for widget in [
                &*self.content_box,
                &*self.tracks_box,
                &*self.artists_box,
                &*self.albums_box,
            ] {
                let controller = create_scroll_controller(&search_results_scroll);
                widget.add_controller(controller);
            }

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

            // Set up initial state of search stack
            self.search_stack
                .add_named(&self.empty_search_page.get(), Some("empty_search_page"));
            self.search_stack.add_named(
                &self.search_results_scroll.get(),
                Some("search_results_scroll"),
            );
            self.search_stack
                .set_visible_child_name("empty_search_page");

            // Connect search entry focus and text change signals
            let search_entry = self.header_search_entry.clone();
            let main_stack = self.main_stack.clone();
            let sidebar_list = self.sidebar_list.clone();
            let home_button = self.home_button.clone();
            let search_stack = self.search_stack.clone();

            // Key controller setup
            let search_entry_key = search_entry.clone();
            let main_stack_key = main_stack.clone();
            let sidebar_list_key = sidebar_list.clone();
            let home_button_key = home_button.clone();

            let key_controller = gtk::EventControllerKey::new();
            key_controller.connect_key_pressed(move |controller, key, _keycode, _modifier| {
                // Don't hijack input if we're already typing in a text entry
                if let Some(widget) = controller.widget() {
                    if let Some(focused) = widget.root().and_then(|root| root.focus()) {
                        if focused.is_ancestor(&widget) {
                            if focused.is::<gtk::Entry>() || focused.is::<gtk::EditableLabel>() {
                                return glib::Propagation::Proceed;
                            }
                        }
                    }
                }

                // Only handle printable characters
                if let Some(char) = key.to_unicode() {
                    if char.is_alphanumeric() || char.is_ascii_punctuation() || char == ' ' {
                        // Set focus to search entry
                        search_entry_key.grab_focus();

                        // Insert the character
                        let current_text = search_entry_key.text();
                        search_entry_key.set_text(&format!("{}{}", current_text, char));

                        // Move cursor to end
                        search_entry_key.set_position(-1);

                        // Switch to search view
                        main_stack_key.set_visible_child_name("search");
                        home_button_key.remove_css_class("selected");
                        sidebar_list_key.unselect_all();

                        return glib::Propagation::Stop;
                    }
                }
                glib::Propagation::Proceed
            });

            self.obj().add_controller(key_controller);

            // Focus controller setup
            let focus_controller = gtk::EventControllerFocus::new();
            focus_controller.connect_enter(
                clone!(@strong main_stack, @strong home_button, @strong sidebar_list, @strong search_stack, @strong search_entry => move |_controller| {
                    main_stack.set_visible_child_name("search");
                    home_button.remove_css_class("selected");
                    sidebar_list.unselect_all();

                    // Only show empty search page if there's no search text
                    if search_entry.text().is_empty() {
                        search_stack.set_visible_child_name("empty_search_page");
                    }
                })
            );
            search_entry.add_controller(focus_controller);

            let artists_section = self.artists_section.clone();
            let albums_section = self.albums_section.clone();

            // When search text changes
            let service_manager = self.service_manager.clone();
            let top_result_box = self.top_result_box.clone();
            let tracks_box = self.tracks_box.clone();
            let artists_box = self.artists_box.clone();
            let albums_box = self.albums_box.clone();
            let search_stack = self.search_stack.clone();

            self.header_search_entry.connect_changed(move |entry| {
                let query = entry.text().to_string();

                // Clear previous results
                if let Some(child) = top_result_box.center_widget() {
                    top_result_box.set_center_widget(None::<&gtk::Widget>);
                }
                while let Some(child) = tracks_box.first_child() {
                    tracks_box.remove(&child);
                }
                while let Some(child) = artists_box.first_child() {
                    artists_box.remove(&child);
                }
                while let Some(child) = albums_box.first_child() {
                    albums_box.remove(&child);
                }

                if query.is_empty() {
                    search_stack.set_visible_child_name("empty_search_page");
                    return;
                }

                search_stack.set_visible_child_name("search_results_scroll");

                // Add placeholders initially
                let top_placeholder = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                top_placeholder.add_css_class("album-art");
                top_result_box.set_center_widget(Some(&top_placeholder));

                // Track placeholders
                for _ in 0..3 {
                    let track_placeholder = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                    track_placeholder.set_size_request(48, 48);
                    track_placeholder.add_css_class("album-art");
                    tracks_box.append(&track_placeholder);
                }

                // Artist placeholders
                for _ in 0..4 {
                    let artist_placeholder = gtk::Image::from_icon_name("avatar-default-symbolic");
                    artist_placeholder.set_size_request(150, 150);
                    artist_placeholder.add_css_class("album-art");
                    artist_placeholder.add_css_class("artist-image");
                    artists_box.append(&artist_placeholder);
                }

                // Album placeholders
                for _ in 0..4 {
                    let album_placeholder = gtk::Image::from_icon_name("audio-x-generic-symbolic");
                    album_placeholder.set_size_request(150, 150);
                    album_placeholder.add_css_class("album-art");
                    albums_box.append(&album_placeholder);
                }

                if let Some(manager) = service_manager.borrow().as_ref() {
                    let manager_clone = manager.clone();
                    gtk::glib::spawn_future_local(clone!(@strong top_result_box, @strong tracks_box, @strong artists_box, @strong albums_box, @strong artists_section, @strong albums_section, @strong search_stack => async move {
                        match manager_clone.search_all(&query).await {
                            Ok(tracks) => {
                                // Clear placeholders
                                if let Some(child) = top_result_box.center_widget() {
                                    top_result_box.set_center_widget(None::<&gtk::Widget>);
                                }
                                while let Some(child) = tracks_box.first_child() {
                                    tracks_box.remove(&child);
                                }
                                while let Some(child) = artists_box.first_child() {
                                    artists_box.remove(&child);
                                }
                                while let Some(child) = albums_box.first_child() {
                                    albums_box.remove(&child);
                                }

                                let mut artists = HashMap::new();
                                let mut albums = HashMap::new();
                                let mut valid_tracks = Vec::new();

                                let query_lower = query.to_lowercase();

                                // Process tracks
                                for track in tracks.iter() {
                                    valid_tracks.push(track);

                                    // Only add to artists if artist is valid
                                    if !track.track.artist.trim().is_empty() &&
                                       track.track.artist.to_lowercase() != "unknown artist" {
                                        artists.entry(track.track.artist.clone())
                                            .or_insert_with(|| track.track.artwork.clone());
                                    }

                                    // Only add to albums if both album and artist are valid
                                    if !track.track.album.trim().is_empty() &&
                                       !track.track.artist.trim().is_empty() &&
                                       track.track.album.to_lowercase() != "unknown album" &&
                                       track.track.artist.to_lowercase() != "unknown artist" {
                                        albums.entry((track.track.album.clone(), track.track.artist.clone()))
                                            .or_insert_with(|| track.track.artwork.clone());
                                    }
                                }

                                if valid_tracks.is_empty() {
                                    search_stack.set_visible_child_name("no_results_page");
                                    return;
                                }

                                search_stack.set_visible_child_name("search_results_scroll");

                                // Find best matches for top result
                                let mut best_track = (0, None::<&PlayableItem>);
                                let mut best_artist = (0, None::<&str>);
                                let mut best_album = (0, None::<(&str, &str)>);

                                for track in valid_tracks.iter() {
                                    let track_score = calculate_match_score(&track.track.title.to_lowercase(), &query_lower);
                                    if track_score > best_track.0 {
                                        best_track = (track_score, Some(track));
                                    }

                                    // Only score artist if valid
                                    if !track.track.artist.trim().is_empty() &&
                                       track.track.artist.to_lowercase() != "unknown artist" {
                                        let artist_score = calculate_match_score(&track.track.artist.to_lowercase(), &query_lower);
                                        if artist_score > best_artist.0 {
                                            best_artist = (artist_score, Some(&track.track.artist));
                                        }
                                    }

                                    // Only score album if valid
                                    if !track.track.album.trim().is_empty() &&
                                       !track.track.artist.trim().is_empty() &&
                                       track.track.album.to_lowercase() != "unknown album" &&
                                       track.track.artist.to_lowercase() != "unknown artist" {
                                        let album_score = calculate_match_score(&track.track.album.to_lowercase(), &query_lower);
                                        if album_score > best_album.0 {
                                            best_album = (album_score, Some((&track.track.album, &track.track.artist)));
                                        }
                                    }
                                }

                                // Choose top result based on highest score
                                let top_track_score = best_track.0;
                                let top_artist_score = best_artist.0;
                                let top_album_score = best_album.0;

                                if let Some(track) = best_track.1 {
                                    if top_track_score >= top_artist_score && top_track_score >= top_album_score {
                                        let card = create_track_card(&track.track, true);
                                        let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
                                        container.set_hexpand(false);
                                        container.set_halign(gtk::Align::Center);
                                        container.append(&card);
                                        top_result_box.set_hexpand(false);
                                        top_result_box.set_halign(gtk::Align::Center);
                                        top_result_box.set_center_widget(Some(&container));
                                    }
                                }

                                if let Some(artist) = best_artist.1 {
                                    if top_artist_score > top_track_score && top_artist_score >= top_album_score {
                                        if let Some(artwork) = artists.get(artist) {
                                            let card = create_artist_card(artist, Some(artwork), true);
                                            let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
                                            container.set_hexpand(false);
                                            container.set_halign(gtk::Align::Center);
                                            container.append(&card);
                                            top_result_box.set_hexpand(false);
                                            top_result_box.set_halign(gtk::Align::Center);
                                            top_result_box.set_center_widget(Some(&container));
                                        }
                                    }
                                }

                                if let Some((album, artist)) = best_album.1 {
                                    if top_album_score > top_track_score && top_album_score > top_artist_score {
                                        if let Some(artwork) = albums.get(&(album.to_string(), artist.to_string())) {
                                            let card = create_album_card(album, artist, Some(artwork), true);
                                            let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
                                            container.set_hexpand(false);
                                            container.set_halign(gtk::Align::Center);
                                            container.append(&card);
                                            top_result_box.set_hexpand(false);
                                            top_result_box.set_halign(gtk::Align::Center);
                                            top_result_box.set_center_widget(Some(&container));
                                        }
                                    }
                                }

                                // Add other tracks to tracks box
                                for (i, track) in valid_tracks.iter().enumerate() {
                                    if i < 5 {
                                        let card = create_track_card(&track.track, false);
                                        tracks_box.append(&card);
                                    }
                                }

                                // Convert to vectors and calculate scores
                                let mut artists_vec: Vec<_> = artists.into_iter().collect();
                                artists_vec.sort_by(|a, b| {
                                    let score_a = calculate_match_score(&a.0.to_lowercase(), &query_lower);
                                    let score_b = calculate_match_score(&b.0.to_lowercase(), &query_lower);
                                    score_b.cmp(&score_a).then(a.0.cmp(&b.0))
                                });

                                let mut albums_vec: Vec<_> = albums.into_iter().collect();
                                albums_vec.sort_by(|a, b| {
                                    let score_a = calculate_match_score(&(a.0).0.to_lowercase(), &query_lower);
                                    let score_b = calculate_match_score(&(b.0).0.to_lowercase(), &query_lower);
                                    score_b.cmp(&score_a).then((a.0).0.cmp(&(b.0).0))
                                });

                                // Create artist cards
                                if !artists_vec.is_empty() {
                                    artists_section.set_visible(true);
                                    for (artist_name, artwork) in artists_vec.iter().take(10) {
                                        let card = create_artist_card(&artist_name, Some(&artwork), false);
                                        artists_box.append(&card);
                                    }
                                } else {
                                    artists_section.set_visible(false);
                                }

                                // Create album cards
                                if !albums_vec.is_empty() {
                                    albums_section.set_visible(true);
                                    for ((album_name, artist_name), artwork) in albums_vec.iter().take(10) {
                                        let card = create_album_card(&album_name, &artist_name, Some(&artwork), false);
                                        albums_box.append(&card);
                                    }
                                } else {
                                    albums_section.set_visible(false);
                                }
                            }
                            Err(e) => eprintln!("Search error: {:?}", e),
                        }

                        fn calculate_match_score(text: &str, query: &str) -> i32 {
                            let mut score = 0;

                            if text == query {
                                score += 100;
                            }
                            else if text.starts_with(query) {
                                score += 75;
                            }
                            else if text.contains(query) {
                                score += 50;
                            }

                            score
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

    fn create_track_card(track: &Track, is_large: bool) -> gtk::Box {
        // Helper function to create a placeholder image with the right size
        fn create_placeholder_image(size: i32) -> gtk::Image {
            let image = gtk::Image::from_icon_name("audio-x-generic-symbolic");
            image.set_pixel_size(size);
            image.add_css_class("album-art");
            image
        }

        // Helper function to create artwork image
        fn create_artwork_image(artwork: &Artwork, size: i32) -> gtk::Image {
            match artwork {
                Artwork {
                    thumbnail: Some(data),
                    ..
                } => {
                    let bytes = glib::Bytes::from(data);
                    let stream = gio::MemoryInputStream::from_bytes(&bytes);
                    if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
                        if let Some(scaled) =
                            pixbuf.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear)
                        {
                            let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                            let image = gtk::Image::from_paintable(Some(&paintable));
                            image.add_css_class("album-art");
                            image
                        } else {
                            create_placeholder_image(size)
                        }
                    } else {
                        create_placeholder_image(size)
                    }
                }
                Artwork {
                    thumbnail: None,
                    full_art: ArtworkSource::Local { path },
                } => {
                    if let Ok(pixbuf) = Pixbuf::from_file(path) {
                        if let Some(scaled) =
                            pixbuf.scale_simple(size, size, gdk_pixbuf::InterpType::Bilinear)
                        {
                            let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                            let image = gtk::Image::from_paintable(Some(&paintable));
                            image.add_css_class("album-art");
                            image
                        } else {
                            create_placeholder_image(size)
                        }
                    } else {
                        create_placeholder_image(size)
                    }
                }
                _ => create_placeholder_image(size),
            }
        }

        if is_large {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            card.add_css_class("track-card");
            card.add_css_class("large-track");
            card.set_hexpand(false);
            card.set_halign(gtk::Align::Center);

            // Use larger size for main display
            let art = create_artwork_image(&track.artwork, 200);
            art.add_css_class("large-image");

            // Rest of the large card layout...
            let labels = gtk::Box::new(gtk::Orientation::Vertical, 8);
            labels.set_halign(gtk::Align::Center);
            labels.set_width_request(130);

            let title = gtk::Label::new(Some(&track.title));
            title.add_css_class("track-title");
            title.set_halign(gtk::Align::Center);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title.set_lines(2);
            title.set_max_width_chars(15);
            title.set_width_chars(15);
            title.set_justify(gtk::Justification::Center);
            title.set_hexpand(false);

            let type_label = gtk::Label::new(Some(&format!("Track • {}", track.artist)));
            type_label.add_css_class("type-label");
            type_label.set_halign(gtk::Align::Center);
            type_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            type_label.set_lines(1);
            type_label.set_max_width_chars(15);
            type_label.set_width_chars(15);
            type_label.set_justify(gtk::Justification::Center);
            type_label.set_hexpand(false);

            labels.append(&title);
            labels.append(&type_label);

            card.append(&art);
            card.append(&labels);

            // Add click handling
            let track_info = track.clone();
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!(
                    "Clicked on track: '{}' by '{}'",
                    track_info.title, track_info.artist
                );
            });
            card.add_controller(click_controller);

            card
        } else {
            let card = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            card.add_css_class("track-card");

            // Use smaller size for list items
            let art = create_artwork_image(&track.artwork, 48);
            art.add_css_class("small-image");

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);

            let title = gtk::Label::new(Some(&track.title));
            title.add_css_class("track-title");
            title.set_halign(gtk::Align::Start);

            let artist = gtk::Label::new(Some(&track.artist));
            artist.add_css_class("track-artist");
            artist.set_halign(gtk::Align::Start);

            labels.append(&title);
            labels.append(&artist);

            card.append(&art);
            card.append(&labels);

            // Add click handling
            let track_info = track.clone();
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!(
                    "Clicked on track: '{}' by '{}'",
                    track_info.title, track_info.artist
                );
            });
            card.add_controller(click_controller);

            card
        }
    }

    fn create_artist_card(name: &str, artwork: Option<&Artwork>, is_large: bool) -> gtk::Box {
        if is_large {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            card.add_css_class("track-card");
            card.add_css_class("large-track");
            card.set_hexpand(false);
            card.set_halign(gtk::Align::Center);

            let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
            container.set_hexpand(false);
            container.set_halign(gtk::Align::Center);

            let art = if let Some(artwork) = artwork {
                match artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => {
                        let bytes = glib::Bytes::from(data);
                        let stream = gio::MemoryInputStream::from_bytes(&bytes);
                        if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>)
                        {
                            if let Some(scaled) =
                                pixbuf.scale_simple(200, 200, gdk_pixbuf::InterpType::Bilinear)
                            {
                                let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                gtk::Image::from_paintable(Some(&paintable))
                            } else {
                                gtk::Image::from_icon_name("avatar-default-symbolic")
                            }
                        } else {
                            gtk::Image::from_icon_name("avatar-default-symbolic")
                        }
                    }
                    _ => gtk::Image::from_icon_name("avatar-default-symbolic"),
                }
            } else {
                gtk::Image::from_icon_name("avatar-default-symbolic")
            };
            art.add_css_class("large-image");

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 8);
            labels.set_halign(gtk::Align::Center);
            labels.set_width_request(130);

            let name_label = gtk::Label::new(Some(name));
            name_label.add_css_class("track-title");
            name_label.set_halign(gtk::Align::Center);
            name_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            name_label.set_lines(2);
            name_label.set_max_width_chars(15);
            name_label.set_width_chars(15);
            name_label.set_justify(gtk::Justification::Center);
            name_label.set_hexpand(false);

            let type_label = gtk::Label::new(Some("Artist"));
            type_label.add_css_class("type-label");
            type_label.set_halign(gtk::Align::Center);
            type_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            type_label.set_lines(1);
            type_label.set_max_width_chars(15);
            type_label.set_width_chars(15);
            type_label.set_justify(gtk::Justification::Center);
            type_label.set_hexpand(false);

            labels.append(&name_label);
            labels.append(&type_label);

            container.append(&art);
            container.append(&labels);
            card.append(&container);

            let artist_name = name.to_string();
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!("Clicked on artist: '{}'", artist_name);
            });
            card.add_controller(click_controller);

            card
        } else {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
            card.add_css_class("artist-card");
            card.set_hexpand(false);
            card.set_halign(gtk::Align::Center);

            let art = if let Some(artwork) = artwork {
                match artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => {
                        let bytes = glib::Bytes::from(data);
                        let stream = gio::MemoryInputStream::from_bytes(&bytes);
                        if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>)
                        {
                            if let Some(scaled) =
                                pixbuf.scale_simple(150, 150, gdk_pixbuf::InterpType::Bilinear)
                            {
                                let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                gtk::Image::from_paintable(Some(&paintable))
                            } else {
                                gtk::Image::from_icon_name("avatar-default-symbolic")
                            }
                        } else {
                            gtk::Image::from_icon_name("avatar-default-symbolic")
                        }
                    }
                    _ => gtk::Image::from_icon_name("avatar-default-symbolic"),
                }
            } else {
                gtk::Image::from_icon_name("avatar-default-symbolic")
            };
            art.add_css_class("artist-image");

            let name_label = gtk::Label::new(Some(name));
            name_label.add_css_class("artist-name");

            card.append(&art);
            card.append(&name_label);

            let artist_name = name.to_string();
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!("Clicked on artist: '{}'", artist_name);
            });
            card.add_controller(click_controller);

            card
        }
    }

    fn create_album_card(
        title: &str,
        artist: &str,
        artwork: Option<&Artwork>,
        is_large: bool,
    ) -> gtk::Box {
        if is_large {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
            card.add_css_class("track-card");
            card.add_css_class("large-track");
            card.set_hexpand(false);
            card.set_halign(gtk::Align::Center);

            let container = gtk::Box::new(gtk::Orientation::Vertical, 8);
            container.set_hexpand(false);
            container.set_halign(gtk::Align::Center);

            let art = if let Some(artwork) = artwork {
                match artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => {
                        let bytes = glib::Bytes::from(data);
                        let stream = gio::MemoryInputStream::from_bytes(&bytes);
                        if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>)
                        {
                            if let Some(scaled) =
                                pixbuf.scale_simple(200, 200, gdk_pixbuf::InterpType::Bilinear)
                            {
                                let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                gtk::Image::from_paintable(Some(&paintable))
                            } else {
                                gtk::Image::from_icon_name("audio-x-generic-symbolic")
                            }
                        } else {
                            gtk::Image::from_icon_name("audio-x-generic-symbolic")
                        }
                    }
                    _ => gtk::Image::from_icon_name("audio-x-generic-symbolic"),
                }
            } else {
                gtk::Image::from_icon_name("audio-x-generic-symbolic")
            };
            art.add_css_class("large-image");

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 8);
            labels.set_halign(gtk::Align::Center);
            labels.set_width_request(130);

            let title_label = gtk::Label::new(Some(title));
            title_label.add_css_class("track-title");
            title_label.set_halign(gtk::Align::Center);
            title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title_label.set_lines(2);
            title_label.set_max_width_chars(15);
            title_label.set_width_chars(15);
            title_label.set_justify(gtk::Justification::Center);
            title_label.set_hexpand(false);

            let type_label = gtk::Label::new(Some(&format!("Album • {}", artist)));
            type_label.add_css_class("type-label");
            type_label.set_halign(gtk::Align::Center);
            type_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            type_label.set_lines(1);
            type_label.set_max_width_chars(15);
            type_label.set_width_chars(15);
            type_label.set_justify(gtk::Justification::Center);
            type_label.set_hexpand(false);

            labels.append(&title_label);
            labels.append(&type_label);

            container.append(&art);
            container.append(&labels);
            card.append(&container);

            let album_info = (title.to_string(), artist.to_string());
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!("Clicked on album: '{}' by '{}'", album_info.0, album_info.1);
            });
            card.add_controller(click_controller);

            card
        } else {
            let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
            card.add_css_class("album-card");
            card.set_hexpand(false);
            card.set_halign(gtk::Align::Center);

            let art = if let Some(artwork) = artwork {
                match artwork {
                    Artwork {
                        thumbnail: Some(data),
                        ..
                    } => {
                        let bytes = glib::Bytes::from(data);
                        let stream = gio::MemoryInputStream::from_bytes(&bytes);
                        if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>)
                        {
                            if let Some(scaled) =
                                pixbuf.scale_simple(150, 150, gdk_pixbuf::InterpType::Bilinear)
                            {
                                let paintable = gtk::gdk::Texture::for_pixbuf(&scaled);
                                gtk::Image::from_paintable(Some(&paintable))
                            } else {
                                gtk::Image::from_icon_name("audio-x-generic-symbolic")
                            }
                        } else {
                            gtk::Image::from_icon_name("audio-x-generic-symbolic")
                        }
                    }
                    _ => gtk::Image::from_icon_name("audio-x-generic-symbolic"),
                }
            } else {
                gtk::Image::from_icon_name("audio-x-generic-symbolic")
            };
            art.add_css_class("album-image");

            let labels = gtk::Box::new(gtk::Orientation::Vertical, 4);
            labels.set_width_request(130); // Force fixed width for label container

            let title_label = gtk::Label::new(Some(title));
            title_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            title_label.set_lines(2);
            title_label.set_max_width_chars(15);
            title_label.set_width_chars(15); // Force width to be exactly 15 chars
            title_label.set_justify(gtk::Justification::Center);
            title_label.set_hexpand(false);
            title_label.add_css_class("album-title");

            let artist_label = gtk::Label::new(Some(artist));
            artist_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
            artist_label.set_lines(1);
            artist_label.set_max_width_chars(15);
            artist_label.set_width_chars(15); // Force width to be exactly 15 chars
            artist_label.set_justify(gtk::Justification::Center);
            artist_label.set_hexpand(false);
            artist_label.add_css_class("album-artist");
            artist_label.add_css_class("dim-label");

            labels.append(&title_label);
            labels.append(&artist_label);

            card.append(&art);
            card.append(&labels);

            let album_info = (title.to_string(), artist.to_string());
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!("Clicked on album: '{}' by '{}'", album_info.0, album_info.1);
            });
            card.add_controller(click_controller);

            card
        }
    }

    fn create_type_label(result_type: &str, artist: Option<&str>) -> gtk::Label {
        let label_text = match (result_type, artist) {
            ("Artist", _) => "Artist".to_string(),
            (type_name, Some(artist_name)) => format!("{} • {}", type_name, artist_name),
            (type_name, None) => type_name.to_string(),
        };

        let label = gtk::Label::new(Some(&label_text));
        label.add_css_class("type-label");
        label.set_halign(gtk::Align::Center);
        label
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
