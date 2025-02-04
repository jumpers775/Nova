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

pub(crate) fn update_search_results(this: &imp::NovaWindow, results: &[PlayableItem], query: &str) {
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
            let card = create_artist_card(&result.track.artist, Some(&result.track.artwork), false);
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
