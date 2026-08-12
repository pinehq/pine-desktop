use std::cell::RefCell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use pine_git::{FileStatus, Repository};
use sourceview5::prelude::*;

#[must_use]
pub fn build(
    project_root: &Path,
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
    branch_label: &gtk::Label,
) -> gtk::Box {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::Single);

    let status = gtk::Label::new(Some("Loading Git status…"));
    status.add_css_class("dim-label");
    status.set_wrap(true);
    status.set_xalign(0.0);

    let refresh = gtk::Button::builder()
        .icon_name("view-refresh-symbolic")
        .tooltip_text("Refresh Git status")
        .build();
    let heading = gtk::Label::builder()
        .label("Changes")
        .xalign(0.0)
        .hexpand(true)
        .build();
    heading.add_css_class("heading");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.append(&heading);
    header.append(&refresh);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(12);
    content.set_margin_end(12);
    content.set_margin_bottom(12);
    content.set_margin_start(12);
    content.append(&header);
    content.append(&status);
    content.append(&list);

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let scroller = gtk::ScrolledWindow::builder()
        .child(&content)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build();
    root.append(&scroller);

    let files = Rc::new(RefCell::new(Vec::<FileStatus>::new()));
    list.connect_row_activated({
        let files = files.clone();
        let project_root = project_root.to_path_buf();
        let parent = parent.clone();
        let toast_overlay = toast_overlay.clone();
        move |_, row| {
            let Ok(index) = usize::try_from(row.index()) else {
                return;
            };
            let Some(file) = files.borrow().get(index).cloned() else {
                return;
            };
            load_diff(&project_root, file, &parent, &toast_overlay);
        }
    });

    refresh.connect_clicked({
        let project_root = project_root.to_path_buf();
        let list = list.clone();
        let status = status.clone();
        let files = files.clone();
        let branch_label = branch_label.clone();
        move |_| {
            refresh_status(&project_root, &list, &status, &files, &branch_label);
        }
    });
    refresh_status(project_root, &list, &status, &files, branch_label);
    root
}

fn refresh_status(
    project_root: &Path,
    list: &gtk::ListBox,
    status_label: &gtk::Label,
    files: &Rc<RefCell<Vec<FileStatus>>>,
    branch_label: &gtk::Label,
) {
    status_label.set_label("Loading Git status…");
    let project_root = project_root.to_path_buf();
    let list = list.clone();
    let status_label = status_label.clone();
    let files = files.clone();
    let branch_label = branch_label.clone();

    gtk::glib::spawn_future_local(async move {
        let result = gtk::gio::spawn_blocking(move || {
            let repository = Repository::discover(&project_root)?;
            repository.status()
        })
        .await;

        clear_list(&list);
        match result {
            Ok(Ok(snapshot)) => {
                branch_label.set_label(snapshot.branch().unwrap_or("Detached HEAD"));
                let new_files = snapshot.files().to_vec();
                if new_files.is_empty() {
                    status_label.set_label("Working tree clean");
                } else {
                    status_label.set_label(&format!("{} changed files", new_files.len()));
                    for file in &new_files {
                        let title = file.path().display().to_string();
                        let subtitle = file.original_path().map_or_else(
                            || file.summary().to_owned(),
                            |original| format!("{} · from {}", file.summary(), original.display()),
                        );
                        let row = adw::ActionRow::builder()
                            .title(title)
                            .subtitle(subtitle)
                            .activatable(true)
                            .build();
                        list.append(&row);
                    }
                }
                *files.borrow_mut() = new_files;
            }
            Ok(Err(error)) => {
                branch_label.set_label("No repository");
                status_label.set_label(&error.to_string());
                files.borrow_mut().clear();
            }
            Err(_) => {
                branch_label.set_label("Git unavailable");
                status_label.set_label("Git status worker stopped unexpectedly");
                files.borrow_mut().clear();
            }
        }
    });
}

fn clear_list(list: &gtk::ListBox) {
    while let Some(row) = list.row_at_index(0) {
        list.remove(&row);
    }
}

fn load_diff(
    project_root: &Path,
    file: FileStatus,
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
) {
    let project_root = project_root.to_path_buf();
    let title = file.path().display().to_string();
    let parent = parent.clone();
    let toast_overlay = toast_overlay.clone();

    gtk::glib::spawn_future_local(async move {
        let result = gtk::gio::spawn_blocking(move || {
            let repository = Repository::discover(&project_root)?;
            repository.diff(&file)
        })
        .await;

        match result {
            Ok(Ok(diff)) if diff.is_empty() => {
                show_toast(&toast_overlay, "No working-tree diff for this file");
            }
            Ok(Ok(diff)) => show_diff_window(&parent, &title, &diff),
            Ok(Err(error)) => show_toast(&toast_overlay, &error.to_string()),
            Err(_) => show_toast(&toast_overlay, "Git diff worker stopped unexpectedly"),
        }
    });
}

fn show_diff_window(parent: &adw::ApplicationWindow, title: &str, diff: &str) {
    let buffer = sourceview5::Buffer::new(None);
    buffer.set_highlight_syntax(true);
    buffer.set_language(
        sourceview5::LanguageManager::default()
            .guess_language(Some(PathBuf::from("changes.diff")), None)
            .as_ref(),
    );
    buffer.set_text(diff);
    buffer.set_modified(false);

    let view = sourceview5::View::with_buffer(&buffer);
    view.set_editable(false);
    view.set_monospace(true);
    view.set_show_line_numbers(true);
    view.set_top_margin(12);
    view.set_bottom_margin(12);
    view.set_left_margin(8);
    view.set_right_margin(8);

    let scroller = gtk::ScrolledWindow::builder()
        .child(&view)
        .hexpand(true)
        .vexpand(true)
        .build();
    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&adw::WindowTitle::new(title, "Working tree diff")));
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&scroller));

    let window = adw::Window::builder()
        .default_height(720)
        .default_width(960)
        .modal(true)
        .transient_for(parent)
        .title(title)
        .content(&toolbar)
        .build();
    window.present();
}

pub fn show_toast(overlay: &adw::ToastOverlay, message: &str) {
    overlay.add_toast(adw::Toast::new(message));
}
