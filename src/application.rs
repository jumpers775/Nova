/* application.rs
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

use adw::prelude::*;
use adw::subclass::prelude::*;
use gettextrs::gettext;
use gtk::{gio, glib};

use crate::config::VERSION;
use crate::NovaWindow;

mod imp {
    use super::*;

    #[derive(Debug, Default)]
    pub struct NovaApplication {}

    #[glib::object_subclass]
    impl ObjectSubclass for NovaApplication {
        const NAME: &'static str = "NovaApplication";
        type Type = super::NovaApplication;
        type ParentType = adw::Application;
    }

    impl ObjectImpl for NovaApplication {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.setup_gactions();
            obj.set_accels_for_action("app.quit", &["<primary>q"]);
        }
    }

    impl ApplicationImpl for NovaApplication {
        // We connect to the activate callback to create a window when the application
        // has been launched. Additionally, this callback notifies us when the user
        // tries to launch a "second instance" of the application. When they try
        // to do that, we'll just present any existing window.
        fn activate(&self) {
            let application = self.obj();
            // Get the current window or create one if necessary
            let window = application.active_window().unwrap_or_else(|| {
                let window = NovaWindow::new(&*application);
                window.upcast()
            });

            // Ask the window manager/compositor to present the window
            window.present();
        }
    }

    impl GtkApplicationImpl for NovaApplication {}
    impl AdwApplicationImpl for NovaApplication {}
}

glib::wrapper! {
    pub struct NovaApplication(ObjectSubclass<imp::NovaApplication>)
        @extends gio::Application, gtk::Application, adw::Application,
        @implements gio::ActionGroup, gio::ActionMap;
}

impl NovaApplication {
    pub fn new(application_id: &str, flags: &gio::ApplicationFlags) -> Self {
        glib::Object::builder()
            .property("application-id", application_id)
            .property("flags", flags)
            .build()
    }

    fn setup_gactions(&self) {
        let quit_action = gio::ActionEntry::builder("quit")
            .activate(move |app: &Self, _, _| app.quit())
            .build();
        let about_action = gio::ActionEntry::builder("about")
            .activate(move |app: &Self, _, _| app.show_about())
            .build();
        // Add preferences action
        let preferences_action = gio::ActionEntry::builder("preferences")
            .activate(|app: &Self, _, _| {
                println!("Preferences action activated");
                // Add actual preferences implementation later
            })
            .build();
        self.add_action_entries([quit_action, about_action, preferences_action]);
        self.set_accels_for_action("app.preferences", &["<primary>comma"]);
    }

    fn show_about(&self) {
        let window = self.active_window().unwrap();
        let about = adw::AboutDialog::builder()
            .application_name("nova")
            .application_icon("com.lucamignatti.nova")
            .developer_name("Luca Mignatti")
            .version(VERSION)
            .developers(vec!["Luca Mignatti"])
            // Translators: Replace "translator-credits" with your name/username, and optionally an email or URL.
            .translator_credits(&gettext("translator-credits"))
            .copyright("© 2025 Luca Mignatti")
            .build();

        about.present(Some(&window));
    }
}
