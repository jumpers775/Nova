use crate::services::models::{Artwork, ArtworkSource};
use gdk_pixbuf::Pixbuf;
use gtk::prelude::*;
use gtk::{gio, glib};

pub(crate) fn create_artwork_image(artwork: &Artwork, size: i32) -> gtk::Image {
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

pub(crate) fn create_placeholder_image(size: i32) -> gtk::Image {
    let image = gtk::Image::from_icon_name("audio-x-generic-symbolic");
    image.set_pixel_size(size);
    image.add_css_class("album-art");
    image
}
