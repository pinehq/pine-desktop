mod editor;
mod sidebar;
mod terminal;
mod terminal_theme;
mod window;

use adw::prelude::*;

fn main() -> gtk::glib::ExitCode {
    let application = adw::Application::builder()
        .application_id("io.pinehq.Pine")
        .build();

    application.connect_activate(window::present);
    application.run()
}
