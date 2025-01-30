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
use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::cell::RefCell;
use std::rc::Rc;

mod imp {
    use super::*;

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

            // Set queue flap hidden by default
            self.queue_flap.set_reveal_flap(false);

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

            // Queue toggle
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
                // Temporarily hold our local variables
                let (is_muted_now, new_volume);
                {
                    // Begin borrow of volume_state
                    let mut state = volume_state_clone.borrow_mut();

                    if state.0 {
                        // Currently muted -> Unmute
                        // Use the stored volume_state's last known volume
                        is_muted_now = false;
                        new_volume = state.1;
                    } else {
                        // Currently unmuted -> Mute
                        // Save the current scale value for later unmute
                        is_muted_now = true;
                        state.1 = volume_scale.value();
                        new_volume = 0.0;
                    }

                    // Update the "muted" status in the RefCell
                    state.0 = is_muted_now;
                } // <-- End of borrow scope here.

                // Now that the RefCell is no longer borrowed, we can safely update the scale
                volume_scale.set_value(new_volume);
                volume_scale.set_sensitive(!is_muted_now);

                // Update the mute button icon
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
