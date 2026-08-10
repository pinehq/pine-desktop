use std::path::{Path, PathBuf};
use std::rc::Rc;

use adw::prelude::*;
use pine_core::{AgentTask, TaskRegistry};

use crate::editor::EditorPanel;

#[must_use]
pub fn build(project_root: &Path, editor: &EditorPanel, tasks: &TaskRegistry) -> gtk::Box {
    let stack = gtk::Stack::builder()
        .transition_type(gtk::StackTransitionType::Crossfade)
        .vexpand(true)
        .build();

    stack.add_titled(
        &build_file_list(project_root, editor),
        Some("files"),
        "Files",
    );
    stack.add_titled(&build_git_placeholder(), Some("git"), "Git");
    stack.add_titled(&build_agent_list(tasks), Some("agents"), "Agents");

    let switcher = gtk::StackSwitcher::builder()
        .halign(gtk::Align::Center)
        .stack(&stack)
        .build();

    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.set_width_request(260);
    root.append(&switcher);
    root.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    root.append(&stack);
    root
}

fn build_file_list(project_root: &Path, editor: &EditorPanel) -> gtk::ScrolledWindow {
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);

    let paths = Rc::new(project_entries(project_root));
    for path in paths.iter() {
        let is_directory = path.is_dir();
        let icon = gtk::Image::from_icon_name(if is_directory {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        });
        let label = gtk::Label::builder()
            .label(
                path.file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Unknown"),
            )
            .xalign(0.0)
            .hexpand(true)
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .build();

        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.set_margin_top(7);
        content.set_margin_end(8);
        content.set_margin_bottom(7);
        content.set_margin_start(8);
        content.append(&icon);
        content.append(&label);
        list.append(&content);
    }

    list.connect_row_activated({
        let editor = editor.clone();
        move |_, row| {
            let Ok(index) = usize::try_from(row.index()) else {
                return;
            };
            if let Some(path) = paths.get(index).filter(|path| path.is_file()) {
                editor.open_path(path);
            }
        }
    });

    gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}

fn project_entries(project_root: &Path) -> Vec<PathBuf> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(project_root)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.file_name().is_some_and(|name| name != ".git"))
        .collect();

    entries.sort_by(|left, right| {
        right
            .is_dir()
            .cmp(&left.is_dir())
            .then_with(|| left.file_name().cmp(&right.file_name()))
    });
    entries
}

fn build_git_placeholder() -> adw::StatusPage {
    adw::StatusPage::builder()
        .icon_name("org.gnome.Builder-symbolic")
        .title("Git status")
        .description("The Git service is the next vertical slice.")
        .build()
}

fn build_agent_list(tasks: &TaskRegistry) -> gtk::ScrolledWindow {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_margin_top(12);
    list.set_margin_end(12);
    list.set_margin_bottom(12);
    list.set_margin_start(12);
    list.set_selection_mode(gtk::SelectionMode::None);

    let mut tasks: Vec<&AgentTask> = tasks.iter().collect();
    tasks.sort_by_key(|task| task.id().to_string());
    for task in tasks {
        let row = adw::ActionRow::builder()
            .title(task.title())
            .subtitle(format!("{} · {}", task.agent(), task.state().label()))
            .build();
        list.append(&row);
    }

    gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}
