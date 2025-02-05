use crate::services::models::SearchResults;
use crate::services::{Album, Artist, PlayableItem, Track};
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

    // Sort and process tracks
    let mut tracks = results.tracks.clone();
    tracks.sort_by(|a, b| {
        score_track(&b.track, query)
            .partial_cmp(&score_track(&a.track, query))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort and filter artists
    let mut filtered_artists: Vec<_> = results
        .artists
        .iter()
        .filter(|artist| artist.name != "Unknown Artist")
        .collect();

    filtered_artists.sort_by(|a, b| {
        score_artist(b, query)
            .partial_cmp(&score_artist(a, query))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Sort and filter albums
    let mut filtered_albums: Vec<_> = results
        .albums
        .iter()
        .filter(|album| album.title != "Unknown Album")
        .collect();

    filtered_albums.sort_by(|a, b| {
        score_album(b, query)
            .partial_cmp(&score_album(a, query))
            .unwrap_or(std::cmp::Ordering::Equal)
    });

    // Show top result based on relevance scoring
    if let Some(top_result) = determine_top_result(results, query) {
        this.top_result_box.set_center_widget(Some(&top_result));
        this.top_result_box.set_visible(true);
        this.top_result_box.parent().unwrap().set_visible(true);
    }

    // Update tracks section
    if !tracks.is_empty() {
        for track in tracks.iter().take(5) {
            let card = create_track_card(&track.track, false);
            this.tracks_box.append(&card);
        }
        this.tracks_box.set_visible(true);
    }

    // Update artists section
    if !filtered_artists.is_empty() {
        for artist in filtered_artists.iter().take(6) {
            let card = create_artist_card(artist, false);
            this.artists_box.append(&card);
        }
        this.artists_section.set_visible(true);
    } else {
        this.artists_section.set_visible(false);
    }

    // Update albums section
    if !filtered_albums.is_empty() {
        for album in filtered_albums.iter().take(6) {
            let card = create_album_card(album, false);
            this.albums_box.append(&card);
        }
        this.albums_section.set_visible(true);
    } else {
        this.albums_section.set_visible(false);
    }
}

fn score_track(track: &Track, query: &str) -> f32 {
    let query = query.to_lowercase();

    // Primary matches (high weight for track-specific fields)
    let title_exact = if track.title.to_lowercase() == query {
        1200.0
    } else {
        0.0
    };
    let title_contains = if track.title.to_lowercase().contains(&query) {
        600.0
    } else {
        0.0
    };

    // Secondary matches (lower weight for related fields)
    let artist_exact = if track.artist.to_lowercase() == query {
        300.0
    } else {
        0.0
    };
    let artist_contains = if track.artist.to_lowercase().contains(&query) {
        150.0
    } else {
        0.0
    };
    let album_exact = if track.album.to_lowercase() == query {
        200.0
    } else {
        0.0
    };
    let album_contains = if track.album.to_lowercase().contains(&query) {
        100.0
    } else {
        0.0
    };

    title_exact + title_contains + artist_exact + artist_contains + album_exact + album_contains
}

fn score_artist(artist: &Artist, query: &str) -> f32 {
    let query = query.to_lowercase();

    // Primary matches (high weight for artist-specific fields)
    let name_exact = if artist.name.to_lowercase() == query {
        1200.0
    } else {
        0.0
    };
    let name_contains = if artist.name.to_lowercase().contains(&query) {
        600.0
    } else {
        0.0
    };

    name_exact + name_contains
}

fn score_album(album: &Album, query: &str) -> f32 {
    let query = query.to_lowercase();

    // Primary matches (high weight for album-specific fields)
    let title_exact = if album.title.to_lowercase() == query {
        1200.0
    } else {
        0.0
    };
    let title_contains = if album.title.to_lowercase().contains(&query) {
        600.0
    } else {
        0.0
    };

    // Secondary matches (lower weight for related fields)
    let artist_exact = if album.artist.to_lowercase() == query {
        300.0
    } else {
        0.0
    };
    let artist_contains = if album.artist.to_lowercase().contains(&query) {
        150.0
    } else {
        0.0
    };

    // Additional score for release year if query is a year
    let year_score = if let Some(year) = album.year {
        if query == year.to_string() {
            400.0
        } else {
            0.0
        }
    } else {
        0.0
    };

    title_exact + title_contains + artist_exact + artist_contains + year_score
}

fn determine_top_result(results: &SearchResults, query: &str) -> Option<gtk::Box> {
    let mut best_result = None;
    let mut best_score = -1.0;

    // Score tracks
    if let Some(track) = results.tracks.first() {
        let score = score_track(&track.track, query);
        if score > best_score {
            best_score = score;
            best_result = Some(create_track_card(&track.track, true));
        }
    }

    // Score artists
    if let Some(artist) = results.artists.first() {
        let score = score_artist(artist, query);
        if score > best_score {
            best_score = score;
            best_result = Some(create_artist_card(artist, true));
        }
    }

    // Score albums
    if let Some(album) = results.albums.first() {
        let score = score_album(album, query);
        if score > best_score {
            best_result = Some(create_album_card(album, true));
        }
    }

    best_result
}

fn score_item(primary: &str, secondary: &str, query: &str, weight: f32) -> f32 {
    let query = query.to_lowercase();
    let primary = primary.to_lowercase();
    let secondary = secondary.to_lowercase();

    let exact_match = if primary == query {
        1000.0 * weight
    } else {
        0.0
    };

    let contains = if primary.contains(&query) {
        500.0 * weight
    } else {
        0.0
    };

    let secondary_score = if !secondary.is_empty() {
        if secondary == query {
            250.0 * weight
        } else if secondary.contains(&query) {
            125.0 * weight
        } else {
            0.0
        }
    } else {
        0.0
    };

    exact_match + contains + secondary_score
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
