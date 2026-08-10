use gtk::prelude::*;
use pine_terminal::{BackendKind, TerminalLaunch};
use vte4::prelude::*;

pub struct TerminalPanel {
    root: gtk::Box,
}

impl TerminalPanel {
    #[must_use]
    pub fn new(launch: &TerminalLaunch) -> Self {
        let terminal = vte4::Terminal::new();
        terminal.set_allow_hyperlink(true);
        terminal.set_audible_bell(false);
        terminal.set_bold_is_bright(true);
        terminal.set_mouse_autohide(true);
        terminal.set_scroll_on_keystroke(true);
        terminal.set_scrollback_lines(10_000);
        terminal.set_hexpand(true);
        terminal.set_vexpand(true);

        let backend = BackendKind::VteMvp;
        let title = gtk::Label::builder()
            .label(format!("Terminal · {backend:?}"))
            .xalign(0.0)
            .hexpand(true)
            .build();
        title.add_css_class("heading");

        let status = gtk::Label::builder()
            .label("Starting shell…")
            .xalign(1.0)
            .build();
        status.add_css_class("dim-label");

        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        bar.set_margin_top(6);
        bar.set_margin_end(12);
        bar.set_margin_bottom(6);
        bar.set_margin_start(12);
        bar.append(&title);
        bar.append(&status);

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&bar);
        root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        root.append(&terminal);

        terminal.connect_child_exited({
            let status = status.clone();
            move |_, exit_status| {
                status.set_label(&format!("Exited · {exit_status} · unverified"));
            }
        });

        spawn_shell(&terminal, launch, &status);
        Self { root }
    }

    #[must_use]
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }
}

fn spawn_shell(terminal: &vte4::Terminal, launch: &TerminalLaunch, status: &gtk::Label) {
    let executable = launch.executable().to_string_lossy().into_owned();
    let cwd = launch.cwd().to_string_lossy().into_owned();

    let mut arguments = Vec::with_capacity(launch.arguments().len() + 1);
    arguments.push(executable);
    arguments.extend(launch.arguments().iter().cloned());
    let argv: Vec<&str> = arguments.iter().map(String::as_str).collect();

    let environment: Vec<String> = std::env::vars_os()
        .filter_map(|(key, value)| Some(format!("{}={}", key.to_str()?, value.to_str()?)))
        .collect();
    let envv: Vec<&str> = environment.iter().map(String::as_str).collect();

    terminal.spawn_async(
        vte4::PtyFlags::DEFAULT,
        Some(&cwd),
        &argv,
        &envv,
        gtk::glib::SpawnFlags::SEARCH_PATH,
        || {},
        -1,
        None::<&gtk::gio::Cancellable>,
        {
            let status = status.clone();
            move |result| match result {
                Ok(_) => status.set_label("Running"),
                Err(error) => status.set_label(&format!("Failed to start: {error}")),
            }
        },
    );
}
