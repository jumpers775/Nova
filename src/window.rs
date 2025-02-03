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
use adw::prelude::*;
use adw::subclass::prelude::*;
use chrono::{DateTime, Utc};
use gdk_pixbuf::Pixbuf;
use glib::clone::Downgrade;
use glib::subclass::prelude::*;
use glib::{ControlFlow, Propagation, SourceId};
use gtk::gio;
use gtk::glib::{self, clone, timeout_add_local};
use gtk::prelude::*;
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::collections::HashSet;
use std::rc::Rc;
use std::time::Duration;

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
        pub search_version: Cell<u32>,
        pub current_search_handle: RefCell<Option<glib::JoinHandle<()>>>,
        pub spinner_container: RefCell<Option<gtk::Box>>,
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
            let container = gtk::Box::new(gtk::Orientation::Vertical, 12);
            container.set_hexpand(true);
            container.set_vexpand(true);
            container.set_halign(gtk::Align::Center);
            container.set_valign(gtk::Align::Center);

            let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
            content.set_halign(gtk::Align::Center);
            content.set_valign(gtk::Align::Center);
            content.add_css_class("track-card");
            content.add_css_class("large-track");

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

            content.append(&art);
            content.append(&labels);

            // Add click handling
            let track_info = track.clone();
            let click_controller = gtk::GestureClick::new();
            click_controller.connect_released(move |_, _, _, _| {
                println!(
                    "Clicked on track: '{}' by '{}'",
                    track_info.title, track_info.artist
                );
            });
            content.add_controller(click_controller);

            container.append(&content);
            container
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

            if self.service_manager.borrow().is_none() {
                // Initialize ServiceManager
                let manager = ServiceManager::new();

                let music_dir = dirs::audio_dir().unwrap_or_else(|| {
                    PathBuf::from(&format!("{}/Music", std::env::var("HOME").unwrap()))
                });
                let local_provider = Box::new(LocalMusicProvider::new(music_dir));

                let manager = Arc::new(manager);
                let manager_clone = manager.clone();

                gtk::glib::MainContext::default().spawn_local(async move {
                    manager_clone
                        .register_provider("local", local_provider)
                        .await;
                });

                self.service_manager.replace(Some(manager));
            }

            // Initialize new state
            self.search_version.set(0);

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

            // Initialize search stack
            self.search_stack
                .add_named(&self.empty_search_page.get(), Some("empty_search_page"));
            self.search_stack.add_named(
                &self.search_results_scroll.get(),
                Some("search_results_scroll"),
            );
            self.search_stack
                .add_named(&self.no_results_page.get(), Some("no_results_page"));
            self.search_stack
                .set_visible_child_name("empty_search_page");

            // Ensure search results containers start hidden
            if let Some(parent) = self.top_result_box.parent() {
                parent.set_visible(false);
            }
            if let Some(parent) = self.tracks_box.parent() {
                parent.set_visible(false);
            }
            self.artists_section.set_visible(false);
            self.albums_section.set_visible(false);

            // Set up global key controller
            let obj_weak = self.obj().downgrade();
            let key_controller = gtk::EventControllerKey::new();
            key_controller.connect_key_pressed(move |controller, key, _, _| {
                if let Some(obj) = obj_weak.upgrade() {
                    let this = obj.imp();

                    // Don't handle if already in text entry
                    if let Some(widget) = controller.widget() {
                        if let Some(focused) = widget.root().and_then(|root| root.focus()) {
                            if focused.is::<gtk::Entry>() || focused.is::<gtk::EditableLabel>() {
                                return Propagation::Proceed;
                            }
                        }
                    }

                    // Handle printable characters
                    if let Some(ch) = key.to_unicode() {
                        if ch.is_alphanumeric() || ch.is_ascii_punctuation() || ch == ' ' {
                            this.main_stack.set_visible_child_name("search");
                            this.header_search_entry.grab_focus();
                            this.header_search_entry.set_text(&ch.to_string());
                            this.header_search_entry.set_position(-1);
                            this.home_button.remove_css_class("selected");
                            this.sidebar_list.unselect_all();
                            return Propagation::Stop;
                        }
                    }
                }
                Propagation::Proceed
            });
            self.obj().add_controller(key_controller);

            // Set up search entry handler
            let obj_weak = self.obj().downgrade();
            self.header_search_entry.connect_changed(move |entry| {
                if let Some(obj) = obj_weak.upgrade() {
                    let this = obj.imp();
                    let query = entry.text().to_string();

                    // Switch to search view and update state
                    this.main_stack.set_visible_child_name("search");
                    this.home_button.remove_css_class("selected");
                    this.sidebar_list.unselect_all();

                    // Increment version to invalidate previous searches
                    let current_version = this.search_version.get() + 1;
                    this.search_version.set(current_version);

                    // Handle empty query
                    if query.is_empty() {
                        this.search_stack
                            .set_visible_child_name("empty_search_page");
                        return;
                    }

                    // Check if we have any existing visible results
                    let has_existing_results = this.top_result_box.center_widget().is_some()
                        || this.tracks_box.first_child().is_some()
                        || this.artists_box.first_child().is_some()
                        || this.albums_box.first_child().is_some();

                    // Check if we're currently on the empty search page
                    let is_empty_page = this
                        .search_stack
                        .visible_child_name()
                        .map_or(true, |name| name == "empty_search_page");

                    // Show loading spinner if:
                    // 1. We have no existing results, OR
                    // 2. We're coming from the empty search page
                    if !has_existing_results || is_empty_page {
                        this.search_stack
                            .set_visible_child_name("search_results_scroll");
                        show_loading_state(this);
                    } else {
                        // We have existing results and aren't coming from empty page,
                        // just ensure we're showing the search results view
                        this.search_stack
                            .set_visible_child_name("search_results_scroll");
                    }

                    // Cancel previous search if running
                    if let Some(handle) = this.current_search_handle.take() {
                        handle.abort();
                    }

                    // Create new search with delay
                    let obj_weak = obj_weak.clone();
                    let query = query.clone();
                    let handle = glib::MainContext::default().spawn_local(async move {
                        // Wait for debounce period
                        glib::timeout_future(Duration::from_millis(300)).await;

                        if let Some(obj) = obj_weak.upgrade() {
                            let this = obj.imp();

                            // Check if this search is still relevant
                            if this.search_version.get() != current_version {
                                return;
                            }

                            // Perform search
                            if let Some(manager) = this.service_manager.borrow().as_ref() {
                                match manager.search_all(&query).await {
                                    Ok(results) => {
                                        // Verify search is still relevant
                                        if this.search_version.get() != current_version {
                                            return;
                                        }

                                        let results: Vec<_> = results.into_iter().collect();
                                        let obj_weak = obj_weak.clone();
                                        glib::MainContext::default().spawn_local(async move {
                                            if let Some(obj) = obj_weak.upgrade() {
                                                let this = obj.imp();
                                                update_search_results(this, &results, &query);
                                            }
                                        });
                                    }
                                    Err(e) => {
                                        eprintln!("Search error: {}", e);
                                        if this.search_version.get() == current_version {
                                            let obj_weak = obj_weak.clone();
                                            glib::MainContext::default().spawn_local(async move {
                                                if let Some(obj) = obj_weak.upgrade() {
                                                    let this = obj.imp();
                                                    this.search_stack
                                                        .set_visible_child_name("no_results_page");
                                                }
                                            });
                                        }
                                    }
                                }
                            }
                        }
                    });

                    this.current_search_handle.replace(Some(handle));
                }
            });

            // Connect search entry focus and text change signals
            let search_entry = self.header_search_entry.clone();
            let main_stack = self.main_stack.clone();
            let sidebar_list = self.sidebar_list.clone();
            let home_button = self.home_button.clone();
            let search_stack = self.search_stack.clone();

            // When search entry gains focus using an EventControllerFocus
            let focus_controller = gtk::EventControllerFocus::new();
            focus_controller.connect_enter(clone!(@strong main_stack, @strong home_button, @strong sidebar_list, @strong search_stack, @strong search_entry => move |_controller| {
                main_stack.set_visible_child_name("search");
                home_button.remove_css_class("selected");
                sidebar_list.unselect_all();

                // Only show empty search page if there's no text
                if search_entry.text().is_empty() {
                    search_stack.set_visible_child_name("empty_search_page");
                }
            }));
            search_entry.add_controller(focus_controller);

            // Add debug prints
            println!("Window constructed");
            println!(
                "Service manager initialized: {}",
                self.service_manager.borrow().is_some()
            );

            // Set initial selection state
            let sidebar_list = self.sidebar_list.clone();
            let home_button = self.home_button.clone();

            glib::idle_add_local_once(move || {
                sidebar_list.unselect_all();
                home_button.add_css_class("selected");
            });

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

            // Initialize volume
            self.volume_scale.set_value(100.0);
            self.mute_button.set_icon_name("audio-volume-high-symbolic");

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
        }
    }

    fn create_loading_indicator() -> gtk::Box {
        let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        container.set_halign(gtk::Align::Center);
        container.set_valign(gtk::Align::Center);

        // Create three dots with staggered animations
        for i in 0..3 {
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.set_size_request(8, 8);
            dot.add_css_class("loading-dot");

            let animation = adw::TimedAnimation::new(
                &dot,
                0.0,
                1.0,
                1000, // Duration in milliseconds as u32
                adw::PropertyAnimationTarget::new(&dot, "margin-top"), // Removed reference
            );
            animation.set_repeat_count(u32::MAX);
            animation.set_easing(adw::Easing::EaseInOutSine);

            // Stagger the start of each dot's animation
            let anim = animation.clone();
            glib::timeout_add_local_once(
                std::time::Duration::from_millis(i as u64 * 200),
                move || {
                    anim.play();
                },
            );

            container.append(&dot);
        }

        container
    }

    fn show_loading_state(this: &imp::NovaWindow) {
        // Clear any existing spinner
        if let Some(container) = this.spinner_container.take() {
            container.unparent();
        }

        // Ensure we're showing the search results scroll
        this.search_stack
            .set_visible_child_name("search_results_scroll");

        // Hide ALL content including section headers and labels
        if let Some(parent) = this.top_result_box.parent() {
            parent.set_visible(false);
        }
        if let Some(parent) = this.tracks_box.parent() {
            parent.set_visible(false);
        }
        this.artists_section.set_visible(false);
        this.albums_section.set_visible(false);

        // Create spinner with vertical centering
        let spinner = gtk::Spinner::new();
        spinner.set_size_request(48, 48);
        spinner.start();

        let container = gtk::Box::new(gtk::Orientation::Vertical, 0);
        container.set_hexpand(true);
        container.set_vexpand(true);
        container.set_halign(gtk::Align::Center);
        container.set_valign(gtk::Align::Center);
        container.append(&spinner);

        // Store reference and make visible immediately
        this.search_results_box.append(&container);
        this.spinner_container.replace(Some(container));
    }

    fn update_search_results(this: &imp::NovaWindow, results: &[PlayableItem], query: &str) {
        println!("Updating search results with {} items", results.len());

        if let Some(container) = this.spinner_container.take() {
            container.unparent();
        }

        this.top_result_box.set_visible(true);
        this.tracks_box.set_visible(true);
        this.top_result_box.parent().unwrap().set_visible(true);
        this.tracks_box.parent().unwrap().set_visible(true);
        this.artists_section.set_visible(!results.is_empty());
        this.albums_section.set_visible(!results.is_empty());

        // Sort results by relevance score
        let mut sorted_results = results.to_vec();
        sorted_results.sort_by_cached_key(|item| {
            let title_match = item.track.title.to_lowercase() == query.to_lowercase();
            let title_contains = item
                .track
                .title
                .to_lowercase()
                .contains(&query.to_lowercase());
            let artist_match = item
                .track
                .artist
                .to_lowercase()
                .contains(&query.to_lowercase());
            let album_match = item
                .track
                .album
                .to_lowercase()
                .contains(&query.to_lowercase());

            let mut score = 0;

            // Exact matches get highest priority
            if title_match {
                score += 1000;
            }
            if title_contains {
                score += 100;
            }
            if artist_match {
                score += 50;
            }
            if album_match {
                score += 25;
            }

            -score // Negative for reverse sort (higher scores first)
        });

        // Clear previous results
        if let Some(child) = this.top_result_box.center_widget() {
            this.top_result_box.set_center_widget(None::<&gtk::Widget>);
        }
        while let Some(child) = this.tracks_box.first_child() {
            this.tracks_box.remove(&child);
        }
        while let Some(child) = this.artists_box.first_child() {
            this.artists_box.remove(&child);
        }
        while let Some(child) = this.albums_box.first_child() {
            this.albums_box.remove(&child);
        }

        if sorted_results.is_empty() {
            this.search_stack.set_visible_child_name("no_results_page");
            this.artists_section.set_visible(false);
            this.albums_section.set_visible(false);
            return;
        }

        // Use HashSet to track unique artists and albums
        let mut artists = HashSet::new();
        let mut albums = HashSet::new();
        let mut top_tracks = Vec::new();

        // Process results in sorted order
        for result in &sorted_results {
            // Add to tracks if we haven't hit the limit
            if top_tracks.len() < 5 {
                top_tracks.push(result);
            }

            // Add to artists if unique and not unknown
            if artists.len() < 6
                && !result.track.artist.eq_ignore_ascii_case("Unknown Artist")
                && artists.insert(result.track.artist.clone())
            {
                let card =
                    create_artist_card(&result.track.artist, Some(&result.track.artwork), false);
                this.artists_box.append(&card);
            }

            // Add to albums if unique and not unknown
            let album_key = format!("{} - {}", result.track.album, result.track.artist);
            if albums.len() < 6
                && !result.track.album.eq_ignore_ascii_case("Unknown Album")
                && albums.insert(album_key)
            {
                let card = create_album_card(
                    &result.track.album,
                    &result.track.artist,
                    Some(&result.track.artwork),
                    false,
                );
                this.albums_box.append(&card);
            }
        }

        // Update visibility of sections
        this.artists_section.set_visible(!artists.is_empty());
        this.albums_section.set_visible(!albums.is_empty());

        // Update top result
        if let Some(top_result) = top_tracks.first() {
            let card = create_track_card(&top_result.track, true);
            this.top_result_box.set_center_widget(Some(&card));
        }

        // Update tracks
        for track in top_tracks {
            let card = create_track_card(&track.track, false);
            this.tracks_box.append(&card);
        }

        // Show results
        this.search_stack
            .set_visible_child_name("search_results_scroll");
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
