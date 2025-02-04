use crate::services::models::SearchResults;
use crate::services::PlayableItem;
use crate::window::components::cards::{create_album_card, create_artist_card, create_track_card};
use crate::window::imp;
use adw::prelude::*;
use adw::subclass::prelude::*;
use adw::Animation;
use gtk::prelude::*;
use gtk::{gio, glib};
use std::collections::HashSet;

pub(crate) fn show_loading_state(this: &imp::NovaWindow) {
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

pub(crate) fn update_search_results(this: &imp::NovaWindow, results: &SearchResults, query: &str) {
    println!(
        "Updating search results with {} tracks, {} albums, {} artists",
        results.tracks.len(),
        results.albums.len(),
        results.artists.len()
    );

    if let Some(container) = this.spinner_container.take() {
        container.unparent();
    }

    let has_any_results =
        !results.tracks.is_empty() || !results.albums.is_empty() || !results.artists.is_empty();

    if !has_any_results {
        this.search_stack.set_visible_child_name("no_results_page");
        return;
    }

    this.search_stack
        .set_visible_child_name("search_results_scroll");

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

    // Make sections visible
    let top_section = this.top_result_box.parent().unwrap().parent().unwrap();
    top_section.set_visible(true);
    let track_section = this.tracks_box.parent().unwrap();
    track_section.set_visible(true);

    // Always show top result section if we have tracks
    if !results.tracks.is_empty() {
        let card = create_track_card(&results.tracks[0].track, true);
        this.top_result_box.set_center_widget(Some(&card));
        this.top_result_box.set_visible(true);
        this.top_result_box.parent().unwrap().set_visible(true);
    }

    // Update tracks section (always show if we have tracks)
    if !results.tracks.is_empty() {
        for track in results.tracks.iter().take(5) {
            let card = create_track_card(&track.track, false);
            this.tracks_box.append(&card);
        }
        this.tracks_box.set_visible(true);
    }

    // Update artists section (filter out Unknown Artist)
    let filtered_artists: Vec<_> = results
        .artists
        .iter()
        .filter(|artist| artist.name != "Unknown Artist")
        .collect();

    if !filtered_artists.is_empty() {
        for artist in filtered_artists.iter().take(6) {
            // Find matching track for artwork
            let artist_artwork = results
                .tracks
                .iter()
                .find(|t| t.track.artist == artist.name)
                .map(|t| &t.track.artwork);

            let card = create_artist_card(&artist.name, artist_artwork, false);
            this.artists_box.append(&card);
        }
        this.artists_section.set_visible(true);
    } else {
        this.artists_section.set_visible(false);
    }

    // Update albums section (filter out Unknown Album)
    let filtered_albums: Vec<_> = results
        .albums
        .iter()
        .filter(|album| album.title != "Unknown Album")
        .collect();

    if !filtered_albums.is_empty() {
        for album in filtered_albums.iter().take(6) {
            // Find matching track for artwork
            let album_artwork = results
                .tracks
                .iter()
                .find(|t| t.track.album == album.title && t.track.artist == album.artist)
                .map(|t| &t.track.artwork);

            let card = create_album_card(&album.title, &album.artist, album_artwork, false);
            this.albums_box.append(&card);
        }
        this.albums_section.set_visible(true);
    } else {
        this.albums_section.set_visible(false);
    }
}

pub(crate) fn create_loading_indicator() -> gtk::Box {
    let container = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    container.set_halign(gtk::Align::Center);
    container.set_valign(gtk::Align::Center);

    // Create three dots with staggered animations
    for i in 0..3 {
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.set_size_request(8, 8);
        dot.add_css_class("loading-dot");

        let animation = adw::TimedAnimation::builder()
            .widget(&dot)
            .value_from(0.0)
            .value_to(1.0)
            .duration(1000)
            .target(&adw::PropertyAnimationTarget::new(&dot, "margin-top"))
            .build();

        animation.set_repeat_count(u32::MAX);
        animation.set_easing(adw::Easing::EaseInOutSine);

        // Stagger the start of each dot's animation
        let anim = animation.clone();
        glib::timeout_add_local_once(
            std::time::Duration::from_millis(i as u64 * 200),
            move || {
                anim.play(); // This should now work
            },
        );

        container.append(&dot);
    }

    container
}
