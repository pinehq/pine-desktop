use std::path::{Path, PathBuf};

use adw::prelude::*;
use pine_core::{AgentKind, AgentTask, TaskId, TaskRegistry, TaskState};
use pine_terminal::TerminalLaunch;

use crate::editor::EditorPanel;
use crate::terminal::TerminalPanel;

pub fn present(application: &adw::Application) {
    if let Some(window) = application.active_window() {
        window.present();
        return;
    }

    let project_root = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/tmp"));
    let tasks = sample_tasks();
    let editor = EditorPanel::new(initial_document(&project_root).as_deref());
    let launch = TerminalLaunch::user_shell(&project_root).expect("current directory is absolute");
    let terminal = TerminalPanel::new(&launch);

    let split = gtk::Paned::new(gtk::Orientation::Vertical);
    split.set_position(600);
    split.set_resize_start_child(true);
    split.set_resize_end_child(true);
    split.set_shrink_start_child(false);
    split.set_shrink_end_child(true);
    split.set_start_child(Some(editor.widget()));
    split.set_end_child(Some(terminal.widget()));

    let sidebar = crate::sidebar::build(&project_root, &editor, &tasks);
    let workspace = adw::OverlaySplitView::new();
    workspace.set_content(Some(&split));
    workspace.set_max_sidebar_width(320.0);
    workspace.set_min_sidebar_width(220.0);
    workspace.set_sidebar(Some(&sidebar));
    workspace.set_sidebar_width_fraction(0.24);
    workspace.set_show_sidebar(true);

    let header = build_header(&project_root, &workspace, &tasks);
    let status = build_status_bar();
    let toolbar = adw::ToolbarView::new();
    toolbar.add_top_bar(&header);
    toolbar.set_content(Some(&workspace));
    toolbar.add_bottom_bar(&status);

    let window = adw::ApplicationWindow::builder()
        .application(application)
        .content(&toolbar)
        .default_height(900)
        .default_width(1440)
        .title("Pine")
        .build();

    install_actions(application, &window, &split, terminal.widget());
    window.present();
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

    let inbox = build_inbox_button(tasks);
    let menu = build_menu_button();

    let header = adw::HeaderBar::new();
    header.set_title_widget(Some(&title));
    header.pack_start(&sidebar_button);
    header.pack_end(&menu);
    header.pack_end(&inbox);
    header.pack_end(&terminal_button);
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
    menu.append(Some("Keyboard Shortcuts"), Some("win.shortcuts"));
    menu.append(Some("About Pine"), Some("win.about"));
    menu.append(Some("Quit"), Some("app.quit"));

    gtk::MenuButton::builder()
        .icon_name("open-menu-symbolic")
        .menu_model(&menu)
        .tooltip_text("Main menu")
        .build()
}

fn build_status_bar() -> gtk::Box {
    let branch = gtk::Label::new(Some("main"));
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
    bar.append(&branch);
    bar.append(&environment);
    bar
}

fn install_actions(
    application: &adw::Application,
    window: &adw::ApplicationWindow,
    split: &gtk::Paned,
    terminal: &gtk::Box,
) {
    let toggle_terminal = gtk::gio::SimpleAction::new("toggle-terminal", None);
    toggle_terminal.connect_activate({
        let split = split.clone();
        let terminal = terminal.clone();
        move |_, _| {
            if split.end_child().is_some() {
                split.set_end_child(None::<&gtk::Widget>);
            } else {
                split.set_end_child(Some(&terminal));
                split.set_position(600);
            }
        }
    });
    window.add_action(&toggle_terminal);

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

    let quit = gtk::gio::SimpleAction::new("quit", None);
    quit.connect_activate({
        let application = application.clone();
        move |_, _| application.quit()
    });
    application.add_action(&quit);

    application.set_accels_for_action("win.toggle-terminal", &["<Control>grave"]);
    application.set_accels_for_action("app.quit", &["<Control>q"]);
}

fn show_shortcuts(window: &adw::ApplicationWindow) {
    let terminal = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control>grave")
        .title("Toggle terminal")
        .build();
    let quit = gtk::ShortcutsShortcut::builder()
        .accelerator("<Control>q")
        .title("Quit")
        .build();
    let group = gtk::ShortcutsGroup::builder().title("Workspace").build();
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
        .transition(build, TaskState::Working)
        .expect("valid sample transition");
    tasks
        .transition(review, TaskState::Working)
        .expect("valid sample transition");
    tasks
        .transition(review, TaskState::WaitingForUser)
        .expect("valid sample transition");
    tasks
}
