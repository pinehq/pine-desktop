use std::cell::{Cell, RefCell};
use std::path::{Path, PathBuf};
use std::rc::Rc;

use gtk::gio;
use gtk::prelude::*;
use sourceview5::prelude::*;

const MAX_PREVIEW_BYTES: usize = 2 * 1024 * 1024;

type Notify = Rc<dyn Fn(String)>;

#[derive(Clone)]
pub struct EditorPanel {
    root: gtk::Box,
    buffer: sourceview5::Buffer,
    view: sourceview5::View,
    tabs: adw::TabView,
    scroller: gtk::ScrolledWindow,
    current_path: Rc<RefCell<Option<PathBuf>>>,
    etag: Rc<RefCell<Option<String>>>,
    load_generation: Rc<Cell<u64>>,
    edit_generation: Rc<Cell<u64>>,
    notify: Notify,
}

impl EditorPanel {
    #[must_use]
    pub fn new<F>(initial_path: Option<&Path>, notify: F) -> Self
    where
        F: Fn(String) + 'static,
    {
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

        let notify: Notify = Rc::new(notify);
        let current_path = Rc::new(RefCell::new(None));
        let edit_generation = Rc::new(Cell::<u64>::new(0));
        buffer.connect_changed({
            let edit_generation = edit_generation.clone();
            move |_| edit_generation.set(edit_generation.get().wrapping_add(1))
        });
        buffer.connect_modified_changed({
            let current_path = current_path.clone();
            let tabs = tabs.clone();
            let scroller = scroller.clone();
            move |buffer| {
                update_page_title(
                    &tabs,
                    &scroller,
                    current_path.borrow().as_deref(),
                    buffer.is_modified(),
                );
            }
        });
        tabs.connect_close_page({
            let buffer = buffer.clone();
            let notify = notify.clone();
            move |_, _| {
                if buffer.is_modified() {
                    notify("Save the current file before closing its tab.".into());
                    gtk::glib::Propagation::Stop
                } else {
                    gtk::glib::Propagation::Proceed
                }
            }
        });

        let panel = Self {
            root,
            buffer,
            view: view.clone(),
            tabs,
            scroller,
            current_path,
            etag: Rc::new(RefCell::new(None)),
            load_generation: Rc::new(Cell::new(0)),
            edit_generation,
            notify,
        };
        if let Some(path) = initial_path {
            panel.open_path(path);
        } else {
            panel.buffer.set_text(WELCOME_TEXT);
            panel.buffer.set_modified(false);
            panel.view.set_editable(false);
        }
        panel
    }

    #[must_use]
    pub fn widget(&self) -> &gtk::Box {
        &self.root
    }

    pub fn connect_document_visibility_changed<F>(&self, callback: F)
    where
        F: Fn(bool) + 'static,
    {
        self.tabs
            .connect_n_pages_notify(move |tabs| callback(tabs.n_pages() > 0));
    }

    pub fn open_path(&self, path: &Path) {
        if self.buffer.is_modified() {
            (self.notify)("Save the current file before opening another one.".into());
            return;
        }

        let generation = self.load_generation.get().wrapping_add(1);
        self.load_generation.set(generation);
        let page = self.ensure_page();
        let title = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled");
        page.set_title(&format!("Loading {title}…"));
        self.view.set_editable(false);

        let file = gio::File::for_path(path);
        let path = path.to_path_buf();
        let buffer = self.buffer.clone();
        let view = self.view.clone();
        let tabs = self.tabs.clone();
        let scroller = self.scroller.clone();
        let current_path = self.current_path.clone();
        let etag = self.etag.clone();
        let load_generation = self.load_generation.clone();
        let notify = self.notify.clone();

        gtk::glib::spawn_future_local(async move {
            let result = load_text_file(&file).await;
            if load_generation.get() != generation {
                return;
            }

            match result {
                Ok((text, loaded_etag)) => {
                    let language = sourceview5::LanguageManager::default()
                        .guess_language(Some(path.clone()), None);
                    buffer.set_language(language.as_ref());
                    buffer.set_text(&text);
                    *current_path.borrow_mut() = Some(path);
                    *etag.borrow_mut() = loaded_etag;
                    buffer.set_modified(false);
                    view.set_editable(true);
                    update_page_title(&tabs, &scroller, current_path.borrow().as_deref(), false);
                }
                Err(message) => {
                    view.set_editable(current_path.borrow().is_some());
                    update_page_title(
                        &tabs,
                        &scroller,
                        current_path.borrow().as_deref(),
                        buffer.is_modified(),
                    );
                    notify(message);
                }
            }
        });
    }

    pub fn save(&self) {
        let Some(path) = self.current_path.borrow().clone() else {
            (self.notify)("Open a file before saving.".into());
            return;
        };
        let (start, end) = self.buffer.bounds();
        let contents = self.buffer.text(&start, &end, true).to_string();
        let saved_generation = self.edit_generation.get();
        let expected_etag = self.etag.borrow().clone();
        let file = gio::File::for_path(&path);
        let buffer = self.buffer.clone();
        let current_path = self.current_path.clone();
        let etag = self.etag.clone();
        let edit_generation = self.edit_generation.clone();
        let notify = self.notify.clone();

        gtk::glib::spawn_future_local(async move {
            match file
                .replace_contents_future(
                    contents.into_bytes(),
                    expected_etag.as_deref(),
                    false,
                    gio::FileCreateFlags::NONE,
                )
                .await
            {
                Ok((_contents, new_etag)) => {
                    if current_path.borrow().as_deref() != Some(path.as_path()) {
                        return;
                    }
                    *etag.borrow_mut() = new_etag.map(|value| value.to_string());
                    if edit_generation.get() == saved_generation {
                        buffer.set_modified(false);
                        notify(format!("Saved {}", path.display()));
                    } else {
                        notify("Saved snapshot; newer edits remain unsaved.".into());
                    }
                }
                Err((_contents, error)) => {
                    notify(format!(
                        "Could not save {}. The file may have changed on disk: {error}",
                        path.display()
                    ));
                }
            }
        });
    }

    #[must_use]
    pub fn has_unsaved_changes(&self) -> bool {
        self.buffer.is_modified()
    }

    fn ensure_page(&self) -> adw::TabPage {
        if self.tabs.n_pages() == 0 {
            self.tabs.append(&self.scroller)
        } else {
            self.tabs.page(&self.scroller)
        }
    }
}

async fn load_text_file(file: &gio::File) -> Result<(String, Option<String>), String> {
    let info = file
        .query_info_future(
            "standard::size,standard::type",
            gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
            gtk::glib::Priority::default(),
        )
        .await
        .map_err(|error| format!("Unable to inspect the selected file: {error}"))?;
    if info.file_type() != gio::FileType::Regular {
        return Err("The selected entry is not a regular file.".into());
    }
    if usize::try_from(info.size()).map_or(true, |size| size > MAX_PREVIEW_BYTES) {
        return Err("The file is larger than the 2 MiB MVP preview limit.".into());
    }

    let (bytes, etag) = file
        .load_contents_future()
        .await
        .map_err(|error| format!("Unable to read the selected file: {error}"))?;
    if bytes.len() > MAX_PREVIEW_BYTES {
        return Err("The file grew beyond the 2 MiB MVP preview limit.".into());
    }
    let text = String::from_utf8(bytes.to_vec())
        .map_err(|_| "The selected file is not valid UTF-8.".to_owned())?;
    Ok((text, etag.map(|value| value.to_string())))
}

fn update_page_title(
    tabs: &adw::TabView,
    scroller: &gtk::ScrolledWindow,
    path: Option<&Path>,
    modified: bool,
) {
    if tabs.n_pages() == 0 {
        return;
    }
    let title = path
        .and_then(Path::file_name)
        .and_then(|name| name.to_str())
        .unwrap_or("Welcome");
    let title = if modified {
        format!("• {title}")
    } else {
        title.to_owned()
    };
    tabs.page(scroller).set_title(&title);
}

const WELCOME_TEXT: &str = r"# Pine for GNOME

This is the first native Linux vertical slice.

- Files and agent tasks stay visible in the sidebar.
- Code is editable in GtkSourceView.
- A real shell runs below in VTE.
- Ctrl+S saves the current file.
- Ctrl+Shift+O opens another project.
- Ctrl+` toggles the terminal.

The terminal boundary is intentionally backend-neutral: VTE is the MVP backend,
while libghostty-vt plus a Pine GTK/GSK renderer is the target architecture.
";
