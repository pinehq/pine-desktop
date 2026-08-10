# Pine Desktop agent guide

This file is a living contract for contributors and coding agents. Update it in
the same change whenever an architectural decision, supported version, build
command, or implementation status changes.

## Product direction

Pine Desktop is a native GNOME workspace for code, Git, terminals, and multiple
CLI-agent tasks. It is inspired by the macOS Pine product, but Linux interaction
and visuals follow GNOME HIG rather than copying Apple UI.

Keep these invariants:

- GTK objects stay in `pine-linux`; portable crates do not depend on GTK.
- A CLI agent remains a real process in a real PTY.
- Terminal output and process exit are observations, not proof of task success.
- Internal commands use structured executable/argument values, never implicit
  shell string interpolation.
- Long-running Git, LSP, storage, and filesystem work must not block GLib's main
  loop.
- Prefer Wayland and GNOME behavior while retaining GTK's X11 fallback.

## Supported baseline and N-1 policy

The native package baseline is Ubuntu 24.04 LTS; Ubuntu 26.04 LTS is also a
first-class target. Rust-side components follow the previous compatible
generation, with the latest patch in that generation:

- Rust 1.96.1 (previous stable toolchain)
- gtk4-rs 0.10.3
- libadwaita-rs 0.8.1, enabled through libadwaita 1.5
- sourceview5-rs 0.10.0
- vte4-rs 0.9.0 for the MVP terminal widget

The required system API floor matches Ubuntu 24.04: GTK 4.14, libadwaita 1.5,
GtkSourceView 5.12, and VTE 0.76. Exact Rust crate versions are pinned in the
workspace manifest. Update this section and `docs/development.md` together when
rolling the baseline forward.

Ubuntu 22.04 does not package the GTK4 VTE development library. Treat it as a
future bundled-runtime/Flatpak target, not a native-package build target.

## Terminal architecture

VTE is the replaceable MVP backend because it provides a complete native GTK
terminal widget. The target architecture is `libghostty-vt` plus a Pine-owned
GTK/GSK renderer, similar to Zed combining Alacritty's terminal state with its
own renderer.

Do not use Ghostty's top-level `include/ghostty.h` embedder API. Upstream marks
it internal and tailored to the macOS app. Only the public `libghostty-vt`
headers are eligible for future integration.

All backend-neutral launch and event contracts belong in `pine-terminal`.

## Repository map

- `crates/pine-core`: portable task and product model
- `crates/pine-terminal`: toolkit-independent terminal contracts
- `crates/pine-linux`: native GTK/libadwaita application
- `dev/container`: reproducible Ubuntu compile/test image
- `dev/lima`: reproducible Ubuntu GNOME visual smoke-test VM
- `docs/linux-architecture.md`: detailed intended architecture
- `docs/adr`: accepted architecture decisions

## Required checks

Run from the repository root:

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets
markdownlint-cli2 README.md AGENTS.md "docs/**/*.md"
```

Linux UI compilation also needs the Ubuntu development packages listed in
`docs/development.md`. CI tests both supported LTS releases and is the canonical
check when working from macOS.

For visual checks on macOS, use the Lima/VZ template documented there; do not
treat virtual GPU performance as release evidence.

## MVP status

Implemented in the first vertical slice:

- Rust workspace and portable task-state invariants
- native libadwaita workspace window and adaptive sidebar
- shallow project file navigation and editable GtkSourceView buffer
- real VTE shell behind the backend-neutral terminal launch model
- sample Agent Inbox and GNOME keyboard actions

Still placeholder or intentionally absent:

- Git service and diff view
- persistence and task/run/process routing
- LSP and background filesystem scanning
- `libghostty-vt` renderer
- packaging and desktop metadata
