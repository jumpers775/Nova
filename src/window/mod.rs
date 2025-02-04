mod components;
mod imp;
mod utils;

use adw::prelude::*;
use adw::subclass::prelude::*;
use gtk::prelude::*;
use gtk::{gio, glib};

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
        self.imp().main_stack.set_visible_child_name(page_name);
    }
}
