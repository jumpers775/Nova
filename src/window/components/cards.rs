use crate::services::models::{Artwork, ArtworkSource, Track};
use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use gtk::{gio, glib, pango};

pub(crate) fn create_track_card(track: &Track, is_large: bool) -> gtk::Box {
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

pub(crate) fn create_artist_card(
    name: &str,
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
                    if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
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
                    if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
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

pub(crate) fn create_album_card(
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
                    if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
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
                    if let Ok(pixbuf) = Pixbuf::from_stream(&stream, None::<&gio::Cancellable>) {
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

pub(crate) fn create_type_label(result_type: &str, artist: Option<&str>) -> gtk::Label {
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
