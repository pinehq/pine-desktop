use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};

use adw::prelude::*;
use pine_core::{AgentKind, AgentTask, TaskId, TaskRegistry, TaskState};
use pine_terminal::TerminalLaunch;

use crate::editor::EditorPanel;
use crate::terminal::TerminalPanel;

thread_local! {
    static WORKSPACES: RefCell<HashMap<PathBuf, gtk::glib::WeakRef<adw::ApplicationWindow>>> =
        RefCell::new(HashMap::new());
}

pub fn present(application: &adw::Application) {
    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
    let hold = application.hold();
    let application = application.clone();
    gtk::glib::spawn_future_local(async move {
        let fallback = project_root.clone();
        let project_root = match gtk::gio::spawn_blocking(move || project_root.canonicalize()).await
        {
            Ok(Ok(project_root)) => project_root,
            _ => fallback,
        };
        present_project(&application, project_root);
        drop(hold);
    });
}

fn present_project(application: &adw::Application, project_root: PathBuf) {
    if activate_existing_workspace(&project_root) {
        return;
    }

    let tasks = sample_tasks();
    let toast_overlay = adw::ToastOverlay::new();
    let window = adw::ApplicationWindow::builder()
        .application(application)
        .default_height(900)
        .default_width(1440)
        .title("Pine")
        .build();
    let editor = EditorPanel::new(initial_document(&project_root).as_deref(), {
        let toast_overlay = toast_overlay.clone();
        move |message| crate::git::show_toast(&toast_overlay, &message)
    });
    let launch = TerminalLaunch::user_shell(&project_root).expect("current directory is absolute");
    let terminal = TerminalPanel::new(&launch);

    let split = gtk::Paned::new(gtk::Orientation::Vertical);
    set_default_workspace_split(&split);
    split.connect_map({
        let initialized = Cell::new(false);
        move |split| {
            if !initialized.replace(true) {
                set_default_workspace_split(split);
            }
        }
    });
    split.set_resize_start_child(true);
    split.set_resize_end_child(true);
    split.set_shrink_start_child(false);
    split.set_shrink_end_child(true);
    split.set_start_child(Some(editor.widget()));
    split.set_end_child(Some(terminal.widget()));
    editor.connect_document_visibility_changed({
        let split = split.clone();
        let editor = editor.widget().clone();
        let terminal = terminal.widget().clone();
        move |is_visible| {
            if is_visible {
                split.set_start_child(Some(&editor));
                set_default_workspace_split(&split);
            } else {
                split.set_start_child(None::<&gtk::Widget>);
                split.set_end_child(Some(&terminal));
            }
        }
    });

    let branch_label = gtk::Label::new(Some("Loading Git…"));
    let sidebar = crate::sidebar::build(
        &project_root,
        &editor,
        &tasks,
        &window,
        &toast_overlay,
        &branch_label,
    );
    let workspace = adw::OverlaySplitView::new();
    workspace.set_content(Some(&split));
    workspace.set_max_sidebar_width(320.0);
    workspace.set_min_sidebar_width(220.0);
    workspace.set_sidebar(Some(&sidebar));
    workspace.set_sidebar_width_fraction(0.24);
    workspace.set_show_sidebar(true);

    let header = build_header(&project_root, &workspace, &tasks);
    let status = build_status_bar(&branch_label);
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&workspace));
    toolbar.add_bottom_bar(&status);

    toast_overlay.set_child(Some(&toolbar));
    window.set_content(Some(&toast_overlay));
    window.connect_close_request({
        let editor = editor.clone();
        let toast_overlay = toast_overlay.clone();
        move |_| {
            if editor.has_unsaved_changes() {
                crate::git::show_toast(
                    &toast_overlay,
                    "Save the current file before closing this workspace",
                );
                gtk::glib::Propagation::Stop
            } else {
                gtk::glib::Propagation::Proceed
            }
        }
    });
    install_actions(
        application,
        &window,
        &split,
        terminal.widget(),
        &editor,
        &toast_overlay,
    );
    WORKSPACES.with(|workspaces| {
        workspaces
            .borrow_mut()
            .insert(project_root, window.downgrade());
    });
    window.present();
}

fn activate_existing_workspace(project_root: &Path) -> bool {
    WORKSPACES.with(|workspaces| {
        let Some(window) = workspaces
            .borrow()
            .get(project_root)
            .and_then(gtk::glib::WeakRef::upgrade)
        else {
            return false;
        };
        window.present();
        true
    })
}

fn build_header(
    project_root: &Path,
    workspace: &adw::OverlaySplitView,
    tasks: &TaskRegistry,
) -> adw::HeaderBar {
    let project_name = project_root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("Workspace");
    let title = adw::WindowTitle::new(project_name, "Pine · GNOME workspace");

    let sidebar_button = gtk::ToggleButton::builder()
        .active(true)
        .icon_name("sidebar-show-symbolic")
        .tooltip_text("Toggle sidebar")
        .build();
    sidebar_button.connect_toggled({
        let workspace = workspace.clone();
        move |button| workspace.set_show_sidebar(button.is_active())
    });

    let terminal_button = gtk::Button::builder()
        .action_name("win.toggle-terminal")
        .icon_name("utilities-terminal-symbolic")
        .tooltip_text("Toggle terminal (Ctrl+`)")
        .build();

    let save_button = gtk::Button::builder()
        .action_name("win.save")
        .icon_name("document-save-symbolic")
        .tooltip_text("Save (Ctrl+S)")
        .build();

    let inbox = build_inbox_button(tasks);
    let menu = build_menu_button();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));
    header.pack_start(&sidebar_button);
    header.pack_end(&menu);
    header.pack_end(&inbox);
    header.pack_end(&terminal_button);
    header.pack_end(&save_button);
    header
}

fn build_inbox_button(tasks: &TaskRegistry) -> gtk::MenuButton {
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);

    let mut count = 0;
    for task in tasks.iter() {
        count += 1;
        let row = adw::ActionRow::builder()
            .title(task.title())
            .subtitle(format!("{} · {}", task.agent(), task.state().label()))
            .build();
        list.append(&row);
    }

    let heading = gtk::Label::builder()
        .label(format!("Agent Inbox · {count}"))
        .xalign(0.0)
        .build();
    heading.add_css_class("title-4");

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(16);
    content.set_margin_end(16);
    content.set_margin_bottom(16);
    content.set_margin_start(16);
    content.set_size_request(360, -1);
    content.append(&heading);
    content.append(&list);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));

    gtk::MenuButton::builder()
        .icon_name("mail-unread-symbolic")
        .popover(&popover)
        .tooltip_text("Agent Inbox")
        .build()
}

fn build_menu_button() -> gtk::MenuButton {
    let menu = gtk::gio::Menu::new();
    menu.append(Some("Open Folder…"), Some("win.open-folder"));
    menu.append(Some("Save"), Some("win.save"));
    menu.append(Some("Keyboard Shortcuts"), Some("win.shortcuts"));
    menu.append(Some("About Pine"), Some("win.about"));
    menu.append(Some("Quit"), Some("app.quit"));

    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Main menu")
        .build()
}

fn build_status_bar(branch: &gtk::Label) -> gtk::Box {
    let environment = gtk::Label::new(Some("Ubuntu 24.04+ · Wayland first"));
    environment.add_css_class("dim-label");
    environment.set_hexpand(true);
    environment.set_xalign(1.0);

    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.set_margin_top(6);
    bar.set_margin_end(12);
    bar.set_margin_bottom(6);
    bar.set_margin_start(12);
    bar.append(&gtk::Image::from_icon_name("org.gnome.Builder-symbolic"));
    bar.append(branch);
    bar.append(&environment);
    bar
}

fn install_actions(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    split: &gtk::Paned,
    terminal: &gtk::Box,
    editor: &EditorPanel,
    toast_overlay: &adw::ToastOverlay,
) {
    let toggle_terminal = gtk::gio::SimpleAction::new("toggle-terminal", None);
    toggle_terminal.connect_activate({
        let split = split.clone();
        let terminal = terminal.clone();
        move |_, _| {
            if split.start_child().is_none() {
                split.set_end_child(Some(&terminal));
                return;
            }
            if split.end_child().is_some() {
                split.set_end_child(None::<&gtk::Widget>);
            } else {
                split.set_end_child(Some(&terminal));
                set_default_workspace_split(&split);
            }
        }
    });
    window.add_action(&toggle_terminal);

    let save = gtk::gio::SimpleAction::new("save", None);
    save.connect_activate({
        let editor = editor.clone();
        move |_, _| editor.save()
    });
    window.add_action(&save);

    let open_folder = gtk::gio::SimpleAction::new("open-folder", None);
    open_folder.connect_activate({
        let application = application.clone();
        let window = window.clone();
        let toast_overlay = toast_overlay.clone();
        move |_, _| select_project_folder(&application, &window, &toast_overlay)
    });
    window.add_action(&open_folder);

    let shortcuts = gtk::gio::SimpleAction::new("shortcuts", None);
    shortcuts.connect_activate({
        let window = window.clone();
        move |_, _| show_shortcuts(&window)
    });
    window.add_action(&shortcuts);

    let about = gtk::gio::SimpleAction::new("about", None);
    about.connect_activate({
        let window = window.clone();
        move |_, _| {
            adw::AboutDialog::builder()
                .application_name("Pine")
                .application_icon("io.pinehq.Pine")
                .developer_name("Pine contributors")
                .version(env!("CARGO_PKG_VERSION"))
                .website("https://github.com/pinehq/pine-desktop")
                .build()
                .present(Some(&window));
        }
    });
    window.add_action(&about);

    if application.lookup_action("quit").is_none() {
        let quit = gtk::gio::SimpleAction::new("quit", None);
        quit.connect_activate({
            let application = application.clone();
            move |_, _| application.quit()
        });
        application.add_action(&quit);
    }

    application.set_accels_for_action("win.toggle-terminal", &["<Control>grave"]);
    application.set_accels_for_action("win.save", &["<Control>s"]);
    application.set_accels_for_action("win.open-folder", &["<Control><Shift>o"]);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
}

fn select_project_folder(
    application: &adw::Application,
    parent: &adw::ApplicationWindow,
    toast_overlay: &adw::ToastOverlay,
) {
    let dialog = gtk::FileDialog::builder()
        .accept_label("Open")
        .modal(true)
        .title("Open Project Folder")
        .build();
    dialog.select_folder(Some(parent), None::<&gtk::gio::Cancellable>, {
        let application = application.clone();
        let toast_overlay = toast_overlay.clone();
        move |result| match result {
            Ok(folder) => {
                let Some(path) = folder.path() else {
                    crate::git::show_toast(
                        &toast_overlay,
                        "Pine currently supports local project folders only",
                    );
                    return;
                };
                let application = application.clone();
                let toast_overlay = toast_overlay.clone();
                gtk::glib::spawn_future_local(async move {
                    match gtk::gio::spawn_blocking(move || path.canonicalize()).await {
                        Ok(Ok(project_root)) => present_project(&application, project_root),
                        Ok(Err(error)) => crate::git::show_toast(
                            &toast_overlay,
                            &format!("Unable to open project folder: {error}"),
                        ),
                        Err(_) => crate::git::show_toast(
                            &toast_overlay,
                            "Project folder worker stopped unexpectedly",
                        ),
                    }
                });
            }
            Err(error) if error.matches(gtk::gio::IOErrorEnum::Cancelled) => {}
            Err(error) => crate::git::show_toast(
                &toast_overlay,
                &format!("Unable to choose a project folder: {error}"),
            ),
        }
    });
}

fn set_default_workspace_split(split: &gtk::Paned) {
    const EDITOR_PERCENT: i32 = 60;
    const FALLBACK_EDITOR_HEIGHT: i32 = 500;

    let available_height = split.height();
    let editor_height = if available_height > 0 {
        available_height * EDITOR_PERCENT / 100
    } else {
        FALLBACK_EDITOR_HEIGHT
    };
    split.set_position(editor_height);
}

fn show_shortcuts(window: &adw::ApplicationWindow) {
    let open_folder = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control><Shift>o")
        .title("Open project folder")
        .build();
    let save = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control>s")
        .title("Save current file")
        .build();
    let terminal = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control>grave")
        .title("Toggle terminal")
        .build();
    let quit = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control>q")
        .title("Quit")
        .build();
    let group = gtk::ShortcutsGroup::builder().title("Workspace").build();
    group.add_shortcut(&open_folder);
    group.add_shortcut(&save);
    group.add_shortcut(&terminal);
    group.add_shortcut(&quit);

    let section = gtk::ShortcutsSection::builder().title("Workspace").build();
    section.add_group(&group);

    let dialog = gtk::ShortcutsWindow::builder()
        .modal(true)
        .transient_for(window)
        .build();
    dialog.add_section(&section);
    dialog.present();
}

fn initial_document(project_root: &Path) -> Option<PathBuf> {
    let readme = project_root.join("README.md");
    readme.is_file().then_some(readme)
}

fn sample_tasks() -> TaskRegistry {
    let mut tasks = TaskRegistry::new();
    let build = TaskId::new(1);
    let review = TaskId::new(2);
    let explore = TaskId::new(3);

    tasks
        .insert(AgentTask::new(build, "Native GNOME MVP", AgentKind::Codex))
        .expect("unique sample task");
    tasks
        .insert(AgentTask::new(
            review,
            "Review terminal contract",
            AgentKind::ClaudeCode,
        ))
        .expect("unique sample task");
    tasks
        .insert(AgentTask::new(
            explore,
            "Explore agent workflow",
            AgentKind::Pi,
        ))
        .expect("unique sample task");
    tasks
        .transition(build, TaskState::Working)
        .expect("valid sample transition");
    tasks
        .transition(review, TaskState::Working)
        .expect("valid sample transition");
    tasks
        .transition(review, TaskState::WaitingForUser)
        .expect("valid sample transition");
    tasks
        .transition(explore, TaskState::Working)
        .expect("valid sample transition");
    tasks
}
