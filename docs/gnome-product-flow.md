# Продуктовый flow Pine для GNOME

- Статус: продуктовый контракт, MVP реализован частично
- Дата сверки с оригинальным Pine: 2026-08-11
- Исходный snapshot: `batonogov/pine@7fbe71781a905fb653b06949d967c8fc5faa095d`

## 1. Что переносим

Linux-версия переносит не внешний вид macOS, а модель работы оригинального
[Pine](https://github.com/batonogov/pine): агент остаётся в настоящем терминале,
код остаётся рядом, а приложение связывает проекты, panes, терминальные сессии и
долговременные задачи.

Контракт составлен по [описанию продукта](https://github.com/batonogov/pine/blob/7fbe71781a905fb653b06949d967c8fc5faa095d/README.md),
[pane-модели](https://github.com/batonogov/pine/blob/7fbe71781a905fb653b06949d967c8fc5faa095d/Pine/PaneManager.swift),
[модели Agent Inbox](https://github.com/batonogov/pine/blob/7fbe71781a905fb653b06949d967c8fc5faa095d/Pine/Agent/AgentInboxModels.swift)
и [матрице совместимости агентов](https://github.com/batonogov/pine/blob/7fbe71781a905fb653b06949d967c8fc5faa095d/docs/agent-compatibility.md).

Не переносим буквально:

- Liquid Glass, traffic-light controls и AppKit chrome;
- Finder-термины и macOS menu-bar conventions;
- глобальный перехват клавиш способами, несовместимыми с Wayland;
- предположение, что process exit или terminal text доказывают успех задачи.

## 2. Верхнеуровневый путь

### 2.1 Запуск

Welcome window показывает одно основное действие «Открыть папку», список недавних
проектов и вход в глобальный Agent Inbox. В GNOME это обычное
`AdwApplicationWindow`: primary action в content area, recent projects в
`GtkListView`, глобальные действия в `AdwHeaderBar`.

Выбор папки открывает отдельное workspace window для canonical project root.
Повторное открытие того же root активирует существующее окно. Сессия
восстанавливает файлы, panes и выбранные вкладки, но не выдаёт мёртвый PTY за
живую агентскую сессию.

### 2.2 Workspace

Широкое окно состоит из utility sidebar и рабочей области. Для sidebar подходит
`AdwOverlaySplitView`: на нормальной ширине он находится рядом с content, на
узкой — открывается поверх него. Внутри sidebar переключаются Files, Git, Agents
и Problems. Header bar содержит project/worktree title, поиск, terminal action и
Agent Inbox с attention badge.

Рабочая область — не фиксированные «редактор сверху, терминал снизу», а дерево:

```text
PaneNode = Leaf(PaneContent) | Split(Axis, ratio, first, second)
PaneContent = EditorTabs | TerminalTabs | Diff | MarkdownPreview
```

GTK-представление использует рекурсивные `GtkPaned`, а набор вкладок leaf —
`AdwTabView`. Модель дерева и правила переходов живут вне GTK, чтобы layout можно
было тестировать без display server.

При первом открытии вертикального editor/terminal split терминал получает 40%
доступной высоты, editor — 60%. После этого пользовательский divider остаётся
источником истины до структурного изменения layout.

### 2.3 Файлы и preview

Выбор и открытие — разные действия:

1. Одинарный click или Space показывает transient preview и оставляет focus в
   Files.
2. Enter, двойной click или явное Open закрепляет вкладку и переводит focus в
   editor.
3. Редактирование transient preview автоматически закрепляет его.
4. Следующий preview переиспользует незакреплённую preview-вкладку.

В GNOME Files строится на `GtkTreeListModel` + `GtkListView`. Стандартная
keyboard navigation виджета сохраняется; дополнительным действиям задаются
accessible labels/help. Rename использует F2, а не macOS Return convention.

## 3. Editor ↔ terminal

Ключевой flow оригинального Pine сохраняется как layout-инвариант:

```text
EmptyEditor
  ├─ open file ────────────────> Editor
  └─ open terminal ────────────> TerminalOnly

Editor
  └─ open terminal ────────────> Editor + Terminal

Editor + Terminal
  ├─ close last editor tab ────> TerminalOnly
  └─ close last terminal tab ──> Editor

TerminalOnly
  ├─ open file ────────────────> Editor + Terminal
  └─ close last terminal tab ──> EmptyEditor
```

Правила для дерева с несколькими panes:

- пустой editor leaf удаляется, если в дереве есть другой leaf;
- его sibling занимает освободившееся пространство без пустого placeholder;
- единственный оставшийся leaf не удаляется: workspace никогда не бывает без
  content destination;
- открытие файла в terminals-only layout создаёт editor leaf над активным или
  первым terminal leaf;
- закрытие последней terminal tab удаляет terminal leaf и останавливает её
  процесс только после подтверждения, если внутри есть foreground process;
- maximize — состояние представления существующего leaf, а не отдельное дерево;
- размеры split сохраняются, но восстановление не должно возвращать удалённый
  пустой leaf.

В текущем MVP реализован вертикальный частный случай: закрытие последней
editor-вкладки разворачивает VTE, а выбор файла в sidebar возвращает editor.
Полное рекурсивное pane tree — следующий layout slice.

Terminal theme по умолчанию следует системной теме GNOME без перезапуска PTY:
контрастная Catppuccin Latte variant используется для light mode, One Dark — для
dark mode. Меняются background, foreground, cursor, selection и все 16 ANSI
слотов; scrollback и дочерний shell сохраняются.

## 4. Terminal-first agent flow

Пользователь открывает terminal tab в project/worktree cwd и запускает `codex`,
`claude`, `pi` или другой CLI. Terminal остаётся исходным TUI агента; Pine не
рисует поверх него vendor chat.

Минимальная безопасная последовательность:

```text
Shell
  → exact executable observed in PTY process tree
  → AgentTask + AgentRun + TerminalSession + ProcessIdentity route
  → Working
  → process disappeared
  → ExitedUnknown
```

`WaitingForUser`, `CompletedVerified` и `FailedVerified` требуют отдельного
доверенного события. Текст терминала, PID, exit code и совпадение по времени не
повышают уровень доверия. Неизвестный executable остаётся generic CLI process.

VM устанавливает Codex, Claude Code и Pi для ручной проверки в VTE. Это пока не
означает, что MVP умеет автоматически запускать, обнаруживать или возобновлять
их: launch adapters и Linux `/proc` detector остаются отдельным slice.

## 5. Agent Inbox

Agent Inbox — отдельное application-level окно, а не список, принадлежащий
текущему workspace. Оно агрегирует задачи разных проектов и сохраняет порядок
секций:

1. Needs Attention;
2. Failed;
3. Completed · Unread;
4. Working;
5. History.

Строка показывает только безопасные metadata: title, agent, project/worktree,
state, freshness и unread marker. Prompt, terminal transcript, token, абсолютный
home path и vendor session identifier не являются presentation data.

Activation разрешает маршрут в момент click: точные project, worktree, task,
run, terminal tab и process generation. Живой маршрут активирует окно, pane и
tab. Устаревший или неоднозначный маршрут показывает recover/start-new-session
actions, но не фокусирует приблизительно похожий процесс.

В GNOME Inbox реализуется отдельным `AdwApplicationWindow` с sectioned
`GtkListView`; header-bar button и Welcome открывают одно и то же окно.
Уведомления идут через `GNotification`, а click использует тот же exact route,
что и строка Inbox.

## 6. GNOME mapping

| Намерение Pine | Нативный GNOME surface |
| --- | --- |
| Project window | `AdwApplicationWindow` + `AdwToolbarView` |
| Utility sidebar | adaptive `AdwOverlaySplitView` |
| Editor/terminal split tree | recursive `GtkPaned` |
| Tabs внутри pane | `AdwTabView` + `AdwTabBar` |
| Code buffer | GtkSourceView 5 |
| Terminal MVP | VTE GTK4 |
| Agent Inbox | отдельное `AdwApplicationWindow`, sectioned `GtkListView` |
| Confirmation | `AdwAlertDialog` |
| Non-blocking feedback | `AdwToastOverlay` |
| Desktop notification | `GNotification` |
| Quick terminal | Wayland portal/system shortcut, не X11 grab |

libadwaita применяется на API floor Ubuntu 24.04. Новые widgets из main docs не
используются, пока их версия не входит в поддерживаемый system API.

## 7. Keyboard и focus

Смысл shortcut сохраняется, но модификаторы соответствуют GNOME и terminal TUI:

| Действие | GNOME shortcut |
| --- | --- |
| Quick Open | `Ctrl+P` |
| Закрыть editor tab | `Ctrl+W` при focus в editor |
| Показать/focus terminal | `Ctrl+grave` |
| Новая terminal tab | `Ctrl+Shift+T` |
| Закрыть terminal tab | `Ctrl+Shift+W` |
| Переименовать файл | `F2` |
| Agent Inbox | настраиваемое app action, без глобального key grab |

Открытие файла явным действием передаёт focus редактору; preview сохраняет focus
в sidebar; создание terminal tab передаёт focus VTE; возврат из Agent Inbox
фокусирует точный routed terminal. Escape закрывает transient overlay/dialog и
возвращает прежний focus owner.

## 8. Проверяемые сценарии

Каждый layout slice должен иметь model tests и visual smoke в Ubuntu VM:

- editor + terminal → закрыть последнюю editor tab → terminal занимает content;
- terminals-only → открыть файл из Files → editor появляется над terminal;
- закрыть последний terminal с foreground process → confirmation → empty editor;
- отменить confirmation → layout и процесс не меняются;
- transient preview → выбрать другой файл → одна preview tab переиспользована;
- transient preview → редактировать → tab закреплена;
- две задачи в разных проектах → Inbox route открывает точный terminal tab;
- PID/process replacement → старый Inbox route не активирует новый process;
- keyboard-only и Orca → все panes, tabs, sidebar и Inbox достижимы;
- narrow window → sidebar становится overlay, editor/terminal state не теряется.

Официальные GNOME reference points: [adaptive layouts](https://gnome.pages.gitlab.gnome.org/libadwaita/doc/1.8/adaptive-layouts.html),
[header bars](https://developer.gnome.org/hig/patterns/containers/header-bars.html)
и [GTK accessibility](https://docs.gtk.org/gtk4/section-accessibility.html).
