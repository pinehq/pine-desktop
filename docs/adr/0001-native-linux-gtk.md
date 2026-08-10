# ADR 0001: нативная Linux-оболочка на GTK4

- Статус: принято для начального проектирования
- Дата: 2026-08-10

## Контекст

Исходный Pine — нативное macOS-приложение на SwiftUI/AppKit. Его продуктовая
модель подходит для Linux, но UI, текстовый стек, terminal embedding, process
APIs, notifications и packaging зависят от macOS и не переносятся напрямую.

Linux-версия должна:

- выглядеть и вести себя как Linux desktop application;
- встраивать быстрый полноценный терминал;
- поддерживать редактор, LSP, Git и несколько project windows;
- оставить доменную логику пригодной для будущей Windows-версии;
- не зависеть от Electron runtime;
- не смешивать terminal presentation с доверенными agent events.

Главная развилка — общий web UI для всех платформ или отдельная нативная
оболочка поверх общего ядра.

## Решение

Linux frontend строится на:

- GTK4 и libadwaita;
- Rust bindings `gtk4-rs`;
- GtkSourceView 5 для редактора;
- полном `libghostty` через C ABI для терминала;
- Rust для общего доменного и сервисного ядра.

GTK остаётся только presentation/platform слоем. Task registry, routing, Git,
LSP, evidence, adapters и persistence не зависят от GTK.

`libghostty` фиксируется на точной upstream revision и скрывается за внутренним
`pine-terminal` API. Типы и callbacks Ghostty не пересекают границу терминального
crate. Обновление revision требует contract, integration и UI tests.

## Обоснование

### GTK4

GTK предоставляет нативные окна, ввод, clipboard, accessibility, Wayland/X11 и
desktop integration. Linux frontend самого Ghostty также использует GTK, поэтому
эта связка имеет наименьшее архитектурное расхождение с терминальным движком.

### GtkSourceView

GtkSourceView расширяет нативный `GtkTextView` функциями редактора. Он позволяет
получить единое поведение focus, IME, clipboard, themes и accessibility без
WebView. Недостающая IDE-функциональность добавляется через Pine LSP и Git layers.

### Rust

Rust подходит для конкурентных фоновых сервисов, process supervision, SQLite,
Git/LSP parsing и будущего Windows platform layer. C ABI позволяет использовать
`libghostty`, не перенося остальную программу на Zig.

### Отдельные нативные оболочки

Общее доменное ядро важнее общего дерева UI. Linux может использовать GTK, а
Windows — WinUI или другой Windows-native toolkit. Это дороже одного web UI, но
уменьшает проблемы с terminal surface embedding, focus, IME и accessibility.

## Рассмотренные альтернативы

### Перенос SwiftUI/AppKit-кода

Отклонено: UI и большая часть системных интеграций привязаны к macOS. Полезны
продуктовые модели, ADR, сценарии и тестовые инварианты, а не platform-код.

### Electron

Отклонено: увеличивает runtime и расход памяти, а терминал и редактор становятся
браузерными компонентами. Это расходится с целью нативного быстрого приложения.

### Tauri + Monaco + xterm.js

Не выбрано основным путём. Tauri легче Electron и ускоряет общий frontend, но
`libghostty` требует нативной render surface. Смешение WebView, нативного GPU
виджета и GTK усложняет focus, clipping, drag-and-drop, IME и accessibility.

Tauri остаётся допустимым fallback, если стоимость двух нативных оболочек
окажется выше продуктовой ценности.

### Полностью Zig-приложение

Не выбрано: интеграция с internals Ghostty была бы проще, но доменное ядро,
SQLite, LSP, Windows platform work и прикладная экосистема важнее минимального
FFI. Взаимодействие с Ghostty ограничивается стабильной C-границей.

### Qt

Не выбрано: Qt даёт переносимый UI, но увеличивает расхождение с GTK frontend
Ghostty и нативной GNOME/Linux-интеграцией. Может быть пересмотрено, если единый
Linux/Windows UI станет обязательным требованием.

### Внешнее окно Ghostty

Отклонено: отдельный процесс не даёт встроенных split-панелей, точного focus,
маршрутизации Inbox до terminal tab и общего lifecycle рабочей области.

## Последствия

Положительные:

- нативный интерфейс и desktop integration;
- полноценный GPU-терминал без `xterm.js`;
- единый GTK input/focus/accessibility pipeline;
- переносимое доменное ядро;
- возможность отдельного качественного Windows frontend.

Отрицательные:

- Linux и Windows потребуют разные UI-слои;
- `libghostty` API пока меняется и требует pinned revision;
- сборке нужен Zig toolchain;
- Rust ↔ C/Zig FFI добавляет unsafe boundary;
- GtkSourceView потребует собственной LSP-интеграции и части IDE-функций;
- Wayland, X11, Mesa, fractional scaling и разные desktop environments
  увеличивают тестовую матрицу.

## Ограничения реализации

1. GTK-типы не входят в `pine-core`.
2. Весь Ghostty FFI находится в одном crate.
3. Нельзя определять agent lifecycle из terminal text.
4. Долгие операции не выполняются в GLib main loop.
5. Каждый async result проверяет project/document/task generation.
6. Packaging не ослабляет process и filesystem проверки незаметно для
   пользователя.
7. Обновление GTK, GtkSourceView или Ghostty не принимается без regression tests.

## Условия пересмотра

Решение пересматривается, если выполняется хотя бы одно условие:

- полный `libghostty` нельзя устойчиво встроить в сторонний GTK widget;
- GtkSourceView не обеспечивает требуемую производительность или correctness;
- поддержка двух нативных UI блокирует поставку Windows-версии;
- sandbox packaging делает host CLI workflow практически невозможным;
- upstream API Ghostty меняется так, что pinning и адаптация становятся
  несопоставимы с ценностью библиотеки.

В таком случае отдельно оцениваются Tauri/Monaco, Qt или собственный renderer на
`libghostty-vt`; изменение оформляется новым ADR, а не неявной заменой стека.
