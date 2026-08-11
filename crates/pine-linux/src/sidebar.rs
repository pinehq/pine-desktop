use std::path::{Path, PathBuf};

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
    let root = directory_model(project_root);
    let tree = gtk::TreeListModel::new(root, false, false, |item| {
        let item = item.downcast_ref::<gtk::glib::BoxedAnyObject>()?;
        let node = item.try_borrow::<FileNode>().ok()?;
        node.is_directory
            .then(|| directory_model(&node.path).upcast())
    });
    let selection = gtk::SingleSelection::new(Some(tree.clone()));
    selection.set_autoselect(false);
    selection.set_can_unselect(true);

    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let expander = gtk::TreeExpander::new();
        expander.set_indent_for_icon(true);
        list_item.set_child(Some(&expander));
    });
    factory.connect_bind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        let Some(row) = list_item.item().and_downcast::<gtk::TreeListRow>() else {
            return;
        };
        let Some(item) = row.item().and_downcast::<gtk::glib::BoxedAnyObject>() else {
            return;
        };
        let Ok(node) = item.try_borrow::<FileNode>() else {
            return;
        };
        let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() else {
            return;
        };

        let icon_name = if node.is_directory {
            "folder-symbolic"
        } else {
            "text-x-generic-symbolic"
        };
        let icon = gtk::Image::from_icon_name(icon_name);
        let label = gtk::Label::builder()
            .label(
                node.path
                    .file_name()
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
        content.append(&icon);
        content.append(&label);

        expander.set_list_row(Some(&row));
        expander.set_child(Some(&content));
    });
    factory.connect_unbind(|_, object| {
        let Some(list_item) = object.downcast_ref::<gtk::ListItem>() else {
            return;
        };
        if let Some(expander) = list_item.child().and_downcast::<gtk::TreeExpander>() {
            expander.set_list_row(None);
            expander.set_child(None::<&gtk::Widget>);
        }
    });

    let list = gtk::ListView::new(Some(selection), Some(factory));
    list.add_css_class("navigation-sidebar");
    list.set_single_click_activate(true);
    list.connect_activate({
        let editor = editor.clone();
        let tree = tree.clone();
        move |_, position| {
            let Some(row) = tree.row(position) else {
                return;
            };
            let Some(item) = row.item().and_downcast::<gtk::glib::BoxedAnyObject>() else {
                return;
            };
            let Ok(node) = item.try_borrow::<FileNode>() else {
                return;
            };
            if node.is_directory {
                row.set_expanded(!row.is_expanded());
            } else {
                editor.open_path(&node.path);
            }
        }
    });

    gtk::ScrolledWindow::builder()
        .child(&list)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .vexpand(true)
        .build()
}

#[derive(Clone, Debug)]
struct FileNode {
    path: PathBuf,
    is_directory: bool,
}

fn directory_model(directory_path: &Path) -> gtk::gio::ListStore {
    let model = gtk::gio::ListStore::new::<gtk::glib::BoxedAnyObject>();
    let directory_path = directory_path.to_path_buf();
    let directory = gtk::gio::File::for_path(&directory_path);
    let destination = model.clone();

    gtk::glib::spawn_future_local(async move {
        let Ok(enumerator) = directory
            .enumerate_children_future(
                "standard::name,standard::type",
                gtk::gio::FileQueryInfoFlags::NOFOLLOW_SYMLINKS,
                gtk::glib::Priority::default(),
            )
            .await
        else {
            return;
        };

        let mut nodes = Vec::new();
        loop {
            let Ok(batch) = enumerator
                .next_files_future(128, gtk::glib::Priority::default())
                .await
            else {
                return;
            };
            if batch.is_empty() {
                break;
            }
            nodes.extend(batch.into_iter().filter_map(|info| {
                let name = info.name();
                (name.as_os_str() != ".git").then(|| FileNode {
                    path: directory_path.join(name),
                    is_directory: info.file_type() == gtk::gio::FileType::Directory,
                })
            }));
        }

        nodes.sort_by(|left, right| {
            right
                .is_directory
                .cmp(&left.is_directory)
                .then_with(|| left.path.file_name().cmp(&right.path.file_name()))
        });
        for node in nodes {
            destination.append(&gtk::glib::BoxedAnyObject::new(node));
        }
    });

    model
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
