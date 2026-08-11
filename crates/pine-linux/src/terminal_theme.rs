use adw::prelude::*;
use gtk::gdk;
use vte4::prelude::*;

// The paired Pine palette is adapted for VTE from the MIT-licensed original:
// https://github.com/batonogov/pine/blob/7fbe71781a905fb653b06949d967c8fc5faa095d/Pine/TerminalPalette.swift
// VTE keeps bright ANSI slots distinct, so no SwiftTerm-specific slot collapse
// workaround is needed here.

struct TerminalColorScheme {
    foreground: &'static str,
    background: &'static str,
    cursor: &'static str,
    selection: &'static str,
    ansi: [&'static str; 16],
}

const LIGHT: TerminalColorScheme = TerminalColorScheme {
    foreground: "#4C4F69",
    background: "#EFF1F5",
    cursor: "#DC8A78",
    selection: "#CCD0E1",
    ansi: [
        "#6C6F85", "#D20F39", "#3F9E2B", "#C07A19", "#1E66F5", "#CC67B1", "#179299", "#7C7F89",
        "#6C6F85", "#D20F39", "#3F9E2B", "#C07A19", "#1E66F5", "#CC67B1", "#179299", "#878A93",
    ],
};

const DARK: TerminalColorScheme = TerminalColorScheme {
    foreground: "#ABB2BF",
    background: "#282C34",
    cursor: "#FFFFFF",
    selection: "#3E4455",
    ansi: [
        "#5C6370", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#ABB2BF",
        "#5C6370", "#E06C75", "#98C379", "#E5C07B", "#61AFEF", "#C678DD", "#56B6C2", "#FFFFFF",
    ],
};

pub fn follow_system(terminal: &vte4::Terminal) {
    let style_manager = adw::StyleManager::default();
    apply(terminal, style_manager.is_dark());

    let terminal = terminal.downgrade();
    style_manager.connect_dark_notify(move |style_manager| {
        if let Some(terminal) = terminal.upgrade() {
            apply(&terminal, style_manager.is_dark());
        }
    });
}

fn apply(terminal: &vte4::Terminal, is_dark: bool) {
    let scheme = if is_dark { &DARK } else { &LIGHT };
    let foreground = parse_color(scheme.foreground);
    let background = parse_color(scheme.background);
    let cursor = parse_color(scheme.cursor);
    let selection = parse_color(scheme.selection);
    let palette: Vec<gdk::RGBA> = scheme.ansi.iter().map(|value| parse_color(value)).collect();
    let palette: Vec<&gdk::RGBA> = palette.iter().collect();

    terminal.set_colors(Some(&foreground), Some(&background), &palette);
    terminal.set_color_cursor(Some(&cursor));
    terminal.set_color_highlight(Some(&selection));
    terminal.set_color_highlight_foreground(Some(&foreground));
}

fn parse_color(value: &str) -> gdk::RGBA {
    gdk::RGBA::parse(value).expect("built-in terminal colors are valid")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn built_in_colors_are_valid() {
        for scheme in [&LIGHT, &DARK] {
            for value in [
                scheme.foreground,
                scheme.background,
                scheme.cursor,
                scheme.selection,
            ]
            .into_iter()
            .chain(scheme.ansi)
            {
                assert!(gdk::RGBA::parse(value).is_ok(), "invalid color {value}");
            }
        }
    }
}
