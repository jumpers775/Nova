use super::components::{
    cards::{create_album_card, create_artist_card, create_track_card, create_type_label},
    search::{create_loading_indicator, show_loading_state, update_search_results},
};
use super::utils::ui;
use crate::services::{LocalMusicProvider, ServiceManager};
use adw::prelude::*;
use adw::subclass::prelude::*;
use glib::Propagation;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use gtk::{gio, glib};
use std::cell::{Cell, RefCell};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

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
        self.setup_service_manager();
        self.setup_search();
        self.setup_navigation();
        self.setup_playback_controls();
        self.setup_volume_controls();
    }
}

impl NovaWindow {
    fn setup_service_manager(&self) {
        // Initialize ServiceManager
        if self.service_manager.borrow().is_none() {
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
    }

    fn setup_search(&self) {
        // Initialize search version
        self.search_version.set(0);

        // Add scroll controller
        let scroll_controller =
            gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
        scroll_controller.set_propagation_phase(gtk::PropagationPhase::Capture);

        let search_results_scroll = self.search_results_scroll.clone();
        scroll_controller.connect_scroll(move |_, _, dy| {
            let adj = search_results_scroll.vadjustment();
            let increment = dy * adj.step_increment() * 3.0;
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

        // Hide search results containers
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

        // Setup search entry handler
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

                // Show loading spinner if needed
                if !has_existing_results || is_empty_page {
                    this.search_stack
                        .set_visible_child_name("search_results_scroll");
                    show_loading_state(this);
                } else {
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

        // Connect search entry focus
        let focus_controller = gtk::EventControllerFocus::new();
        let main_stack = self.main_stack.clone();
        let home_button = self.home_button.clone();
        let sidebar_list = self.sidebar_list.clone();
        let search_stack = self.search_stack.clone();
        let search_entry = self.header_search_entry.clone();

        focus_controller.connect_enter(move |_| {
            main_stack.set_visible_child_name("search");
            home_button.remove_css_class("selected");
            sidebar_list.unselect_all();

            if search_entry.text().is_empty() {
                search_stack.set_visible_child_name("empty_search_page");
            }
        });
        self.header_search_entry.add_controller(focus_controller);
    }

    fn setup_navigation(&self) {
        // Set initial selection state
        let sidebar_list = self.sidebar_list.clone();
        let home_button = self.home_button.clone();

        glib::idle_add_local_once(move || {
            sidebar_list.unselect_all();
            home_button.add_css_class("selected");
        });

        // Setup home button navigation
        let main_stack = self.main_stack.clone();
        let home_button = self.home_button.clone();
        let sidebar_list = self.sidebar_list.clone();
        self.home_button.connect_clicked(move |button| {
            main_stack.set_visible_child_name("home");
            button.add_css_class("selected");
            sidebar_list.unselect_all();
        });

        // Setup ListBox navigation
        let main_stack = self.main_stack.clone(); // Clone again here
        let home_button = self.home_button.clone();
        self.sidebar_list.connect_row_activated(move |_, row| {
            let page_name = match row.index() {
                0 => "artists",
                1 => "albums",
                2 => "playlists",
                3 => "liked",
                _ => "home",
            };
            main_stack.set_visible_child_name(page_name);
            home_button.remove_css_class("selected");
        });

        // Queue toggle with flap
        let queue_flap = self.queue_flap.clone();
        self.queue_toggle.connect_toggled(move |button| {
            queue_flap.set_reveal_flap(button.is_active());
        });
    }

    fn setup_playback_controls(&self) {
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
        });

        // Previous and Next buttons
        self.prev_button.connect_clicked(move |_| {
            println!("Previous button clicked");
        });

        self.next_button.connect_clicked(move |_| {
            println!("Next button clicked");
        });

        // Shuffle button
        self.shuffle_button.connect_clicked(move |button| {
            if button.is_active() {
                button.add_css_class("active");
            } else {
                button.remove_css_class("active");
            }
        });

        // Loop button
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

        // Progress bar updates
        self.song_progress_bar.connect_value_changed(|scale| {
            println!("Progress: {}%", scale.value());
        });
    }

    fn setup_volume_controls(&self) {
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
    }
}
// Implement other traits
impl WidgetImpl for NovaWindow {}
impl WindowImpl for NovaWindow {}
impl ApplicationWindowImpl for NovaWindow {}
impl AdwApplicationWindowImpl for NovaWindow {}
