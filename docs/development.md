# Локальная разработка

## Базовая среда

Native MVP поддерживает Ubuntu 24.04 и 26.04 LTS. Ubuntu 24.04 задаёт минимальный
system API; Ubuntu 26.04 проверяет работу с текущим GNOME stack. Workspace
закрепляет предыдущие совместимые поколения Rust bindings на их последних
patch-версиях:

| Компонент | Версия |
| --- | --- |
| Rust | 1.96.1 |
| GTK system API | 4.14 |
| libadwaita system API | 1.5 |
| GtkSourceView system API | 5.12 |
| VTE system API | 0.76 |
| gtk4-rs | 0.10.3 |
| libadwaita-rs | 0.8.1 |
| sourceview5-rs | 0.10.0 |
| vte4-rs | 0.9.0 |

Исключение из простого semver N‑1 — `libghostty-vt`: его внешний API пока не
является частью MVP и будет закреплён на точной revision только вместе с первым
renderer prototype. Внутренний `include/ghostty.h` использовать нельзя.

## Ubuntu 24.04 и 26.04

Обе поддерживаемые LTS используют одинаковые package names:

```sh
sudo apt-get update
sudo apt-get install \
  build-essential \
  ca-certificates \
  libadwaita-1-dev \
  libgtk-4-dev \
  libgtksourceview-5-dev \
  libvte-2.91-gtk4-dev \
  pkg-config
```

Rust toolchain выбирается автоматически из `rust-toolchain.toml`. После
установки rustup:

```sh
cargo run -p pine-linux
```

## Проверки

```sh
cargo fmt --all --check
cargo test --workspace
cargo clippy --workspace --all-targets
markdownlint-cli2 README.md AGENTS.md "docs/**/*.md"
```

На macOS можно запускать unit tests переносимых crates, но `pine-linux`
линкуется только там, где установлены GTK, libadwaita, GtkSourceView и VTE.

Ту же минимальную сборку можно воспроизвести в container:

```sh
docker build \
  --build-arg UBUNTU_VERSION=24.04 \
  --file dev/container/Ubuntu.Dockerfile \
  --tag pine-dev:ubuntu-24.04 \
  dev/container
docker run --rm --volume "$(pwd):/workspace" pine-dev:ubuntu-24.04
```

## Ubuntu 22.04

В Ubuntu 22.04 доступны GTK 4.6, libadwaita 1.1 и GtkSourceView 5.4, но в
штатном репозитории нет GTK4-варианта VTE development package. Поэтому native
`.deb`, собранный только из системных зависимостей, пока начинается с Ubuntu
24.04.

Поддержка 22.04 технически возможна через Flatpak, AppImage или другой package с
собственным GTK/VTE runtime. Это отдельный packaging этап: приложение не должно
молча терять встроенный терминал на старой системе.

## GNOME VM на macOS через Lima

Для визуальной smoke-проверки используется Ubuntu 26.04 с GNOME 50. CI отдельно
компилирует минимальный Ubuntu 24.04 system API.

Нужны Lima 2.2 или новее и macOS 13.5 или новее. Шаблон использует нативный
Apple Virtualization.framework, `virtiofs` и встроенное display window:

```sh
brew install lima
limactl start \
  --name pine-gnome \
  --mount "$(pwd):w" \
  dev/lima/ubuntu-gnome.yaml
```

Первый запуск загружает Ubuntu и устанавливает минимальный GNOME desktop, поэтому
занимает заметное время. После автоматического входа открыть Terminal и выполнить
в примонтированном каталоге проекта:

```sh
cargo run -p pine-linux
```

Управление VM:

```sh
limactl stop pine-gnome
limactl start pine-gnome
limactl delete pine-gnome
```

VM подходит для проверки layout, тем, focus и Wayland-поведения. Она не является
точным измерением GPU-производительности реального Linux desktop.
