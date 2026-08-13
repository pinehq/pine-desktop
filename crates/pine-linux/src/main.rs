//! Native GNOME workspace for CLI agents, code, Git, and terminals.
//!
//! `pine-linux` is the GTK4/libadwaita application binary; portable domain
//! logic lives in `pine-core`, `pine-git`, and `pine-terminal`.

mod editor;
mod git;
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
