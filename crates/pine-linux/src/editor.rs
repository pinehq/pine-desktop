use std::path::{Path, PathBuf};

use gtk::prelude::*;
use sourceview5::prelude::*;

const MAX_PREVIEW_BYTES: u64 = 2 * 1024 * 1024;

#[derive(Clone)]
pub struct EditorPanel {
    root: gtk::Box,
    buffer: sourceview5::Buffer,
    page: adw::TabPage,
}

impl EditorPanel {
    #[must_use]
    pub fn new(initial_path: Option<&Path>) -> Self {
        let buffer = sourceview5::Buffer::new(None);
        buffer.set_highlight_syntax(true);

        let view = sourceview5::View::with_buffer(&buffer);
        view.set_auto_indent(true);
        view.set_highlight_current_line(true);
        view.set_monospace(true);
        view.set_show_line_numbers(true);
        view.set_smart_backspace(true);
        view.set_tab_width(4);
        view.set_top_margin(12);
        view.set_bottom_margin(12);
        view.set_left_margin(8);
        view.set_right_margin(8);
        view.set_vexpand(true);
        view.set_hexpand(true);

        let scroller = gtk::ScrolledWindow::builder()
            .child(&view)
            .hexpand(true)
            .vexpand(true)
            .build();

        let tabs = adw::TabView::new();
        let page = tabs.append(&scroller);
        page.set_title("Welcome");

        let tab_bar = adw::TabBar::builder()
            .autohide(false)
            .expand_tabs(false)
            .view(&tabs)
            .build();

        let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
        root.append(&tab_bar);
        root.append(&tabs);

        let panel = Self { root, buffer, page };
        if let Some(path) = initial_path {
            panel.open_path(path);
        } else {
            panel.buffer.set_text(WELCOME_TEXT);
        }
        panel
    }

    #[must_use]
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn open_path(&self, path: &Path) {
        let result = read_preview(path);
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        self.page.set_title(title);

        match result {
            Ok(text) => {
                self.buffer.set_text(&text);
                let language = sourceview5::LanguageManager::default()
                    .guess_language(Some(PathBuf::from(path)), None);
                self.buffer.set_language(language.as_ref());
                self.buffer.set_modified(false);
            }
            Err(message) => {
                self.buffer.set_language(None);
                self.buffer
                    .set_text(&format!("Unable to open {title}\n\n{message}\n"));
                self.buffer.set_modified(false);
            }
        }
    }
}

fn read_preview(path: &Path) -> Result<String, String> {
    let metadata = std::fs::metadata(path).map_err(|error| error.to_string())?;
    if !metadata.is_file() {
        return Err("The selected entry is not a regular file.".into());
    }
    if metadata.len() > MAX_PREVIEW_BYTES {
        return Err("The file is larger than the 2 MiB MVP preview limit.".into());
    }
    std::fs::read_to_string(path).map_err(|error| error.to_string())
}

const WELCOME_TEXT: &str = r"# Pine for GNOME

This is the first native Linux vertical slice.

- Files and agent tasks stay visible in the sidebar.
- Code is editable in GtkSourceView.
- A real shell runs below in VTE.
- Ctrl+` toggles the terminal.

The terminal boundary is intentionally backend-neutral: VTE is the MVP backend,
while libghostty-vt plus a Pine GTK/GSK renderer is the target architecture.
";
