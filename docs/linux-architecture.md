# Архитектура нативной Linux-версии

- Статус: первый vertical slice
- Дата: 2026-08-10
- Целевая платформа: Ubuntu 24.04/26.04 LTS, Wayland first и X11 fallback
- UI toolkit: GTK4 + libadwaita

## 1. Цель

Создать быстрое нативное Linux-приложение, в котором пользователь может
одновременно видеть код, Git-контекст и несколько работающих CLI-агентов. Оно
должно сохранять терминальную модель самих агентов, не подменять их vendor-чатом
и не делать непроверяемых выводов из терминального текста.

Архитектура должна одновременно решать две задачи:

1. использовать сильные стороны Linux desktop: GTK, `/proc`, PTY, D-Bus, XDG и
   systemd;
2. не помещать продуктовую логику внутрь GTK-виджетов, чтобы общее ядро позже
   использовала Windows-версия.

## 2. Не-цели первой версии

- автономное планирование и запуск цепочек агентов;
- встроенный универсальный AI-чат;
- чтение приватных баз, transcript или истории поставщика агента;
- определение успеха по terminal output, времени или exit code;
- extension marketplace и совместимость с расширениями VS Code;
- удалённая cloud-синхронизация;
- сохранение живого PTY после полного завершения процесса приложения;
- Flatpak как единственный способ распространения.

## 3. Общая схема

```text
┌────────────────────── pine-linux (GTK4) ──────────────────────┐
│                                                               │
│  WorkspaceWindow   AgentInboxWindow   SettingsWindow          │
│         │                 │                  │                 │
│  PaneTree / Tabs    Task list         Preferences             │
│      ┌──┴──────────────┐                                      │
│      │                 │                                      │
│ GtkSourceView     PineTerminalWidget                           │
│                        │                                      │
│                  pine-terminal                                │
│              VTE MVP │ libghostty-vt target                   │
└────────────────────────┬──────────────────────────────────────┘
                         │ typed commands/events
┌────────────────────────┴──────────────────────────────────────┐
│                       pine-core                               │
│  TaskRegistry │ Routing │ Evidence │ ProjectRegistry │ Policy │
└──────────┬──────────────┬──────────────┬──────────────┬────────┘
           │              │              │              │
      pine-git        pine-lsp      pine-storage   pine-platform-linux
      git/worktree    JSON-RPC      SQLite/XDG     PTY,/proc,D-Bus
```

GTK-слой отвечает за представление и ввод. Доменное ядро принимает команды,
проверяет инварианты и публикует события. Платформенный слой предоставляет
процессы, файловые события, уведомления и доказательства идентичности процесса.

## 4. Предлагаемая структура репозитория

```text
pine-desktop/
├── Cargo.toml
├── crates/
│   ├── pine-core/             # чистая доменная модель
│   ├── pine-linux/            # GTK4 application и окна
│   ├── pine-editor/           # GtkSourceView, LSP presentation
│   ├── pine-terminal/         # backend-neutral terminal contract
│   ├── pine-git/              # status, diff, branch, worktree
│   ├── pine-lsp/              # LSP client и lifecycle серверов
│   ├── pine-adapters/         # каталоги и адаптеры CLI-агентов
│   ├── pine-storage/          # SQLite, миграции, XDG paths
│   └── pine-platform-linux/   # /proc, watchers, D-Bus, portals
├── ui/
│   ├── resources/             # GtkBuilder UI, CSS, icons
│   └── screenshots/
├── docs/
└── tests/
    ├── fixtures/
    └── integration/
```

Имена crate предварительные. Важна направленность зависимостей:
`pine-core` ничего не знает о GTK, Ghostty, `/proc` или конкретной базе данных.

## 5. UI и модель рабочего пространства

### 5.1 Окна

`GtkApplication` управляет одним экземпляром приложения и несколькими окнами:

- `WorkspaceWindow` — окно конкретного проекта/worktree;
- `AgentInboxWindow` — агрегированные задачи всех открытых проектов;
- `SettingsWindow` — редактор, терминал, агенты, уведомления и безопасность;
- позднее `QuickTerminalWindow` — глобальный быстрый терминал.

Закрытие окна проекта не должно автоматически означать завершение его задачи.
Маршрут задачи помечается как backgrounded, пока терминал и приложение живы.

### 5.2 Панели и вкладки

Рабочая область хранится как дерево:

```text
PaneNode = Leaf(Pane) | Split(Axis, ratio, left, right)
Pane     = ordered tabs + selected tab
Tab      = Editor | Diff | Terminal | MarkdownPreview
```

Это позволяет перемещать вкладки между панелями, делить область вертикально и
горизонтально и сохранять layout независимо от конкретных GTK-виджетов.

### 5.3 Боковая панель

Одна боковая область переключается между режимами:

- Files — дерево проекта;
- Git — изменённые файлы, staging и diff;
- Agents — задачи текущего проекта;
- Problems — LSP и локальные diagnostics.

Agent Inbox остаётся глобальным и не ограничивается активным проектом.

## 6. Редактор

### 6.1 Базовый компонент

Редактор строится на GtkSourceView 5. Он предоставляет нативное текстовое поле,
подсветку синтаксиса, номера строк, поиск, completion, snippets и интеграцию с
GTK accessibility. Pine добавляет поверх него:

- вкладки и preview tabs;
- сохранение и обнаружение внешних изменений;
- LSP completion, hover, definition, references, rename и code actions;
- diagnostics gutter и Problems panel;
- Git diff markers и переход между изменениями;
- bracket matching, indentation guides и folding;
- project search и quick open;
- отдельный diff view для проверенных изменений агента.

### 6.2 LSP

Каждый language server — отдельный дочерний процесс с JSON-RPC поверх stdio.
Lifecycle привязан к canonical project root, языку и конфигурации. Серверы не
запускаются из UI-потока.

Документы получают монотонную версию. Ответ LSP с устаревшей версией документа
не может обновить diagnostics или применить edit к новому содержимому.

Workspace edits проходят предварительную проверку:

- путь находится внутри разрешённого project/worktree root;
- версия открытого документа совпадает;
- изменения не перекрываются;
- пользователь подтверждает потенциально широкие операции.

## 7. Терминал

### 7.1 Выбор уровня интеграции

Ghostty разделяет два разных слоя. Верхнеуровневый `include/ghostty.h` —
внутренний embedder API, рассчитанный на macOS-приложение Ghostty. Он не является
поддерживаемой границей для внешнего GTK-приложения. Публичный `libghostty-vt`
предоставляет parser, terminal state и render state, но не готовый GTK-виджет,
GPU renderer, шрифты или полный PTY lifecycle.

Поэтому терминал развивается в два этапа:

1. MVP использует VTE: законченный нативный GTK terminal widget;
2. целевая реализация использует `libghostty-vt` и собственный Pine renderer на
   GTK/GSK после отдельного rendering prototype.

Подход повторяет архитектурную границу Zed: terminal engine хранит состояние, а
приложение рисует его собственной UI-системой. Оба backend скрыты за
`pine-terminal`; типы VTE, Ghostty и GTK не попадают в `pine-core`.

### 7.2 `PineTerminalWidget`

GTK-виджет терминала отвечает за:

- создание и уничтожение backend surface;
- передачу scale factor и размеров будущему renderer;
- focus, keyboard, IME, mouse и scroll events;
- clipboard, URL и drag-and-drop callbacks;
- синхронизацию темы, шрифта и DPI;
- публикацию title, bell, working directory и process-exit событий;
- корректное уничтожение backend после остановки callbacks.

Вызовы backend, требующие GTK, исполняются в GLib main context. Фоновые callback
не обращаются к GTK напрямую, а отправляют bounded event в UI-loop.

### 7.3 Процесс и PTY

Терминальная сессия запускается только из структурированной конфигурации:

```text
TerminalLaunch {
    project_root,
    worktree_root,
    cwd,
    executable,
    argv,
    bounded_environment,
    task_run_id?,
}
```

Команда никогда не собирается строковой конкатенацией и не передаётся через
неявный `sh -c`, если пользователь явно не выбрал shell command.

Если terminal backend не предоставляет достаточных process lifecycle callbacks,
он остаётся обычным терминалом, но не получает право присоединить процесс к
существующей задаче. Недостающий сигнал нельзя заменять парсингом вывода.

## 8. Доменная модель агентов

### 8.1 Идентичности

Разделяются четыре идентичности:

| Сущность | Назначение |
| --- | --- |
| `AgentTask` | долговременное намерение пользователя |
| `AgentRun` | одна попытка запуска или возобновления задачи |
| `TerminalSession` | конкретный PTY и терминальная вкладка |
| `ProcessIdentity` | PID + start time + executable + наблюдаемое поколение |

PID, тип агента, cwd или близость по времени не могут самостоятельно вернуть
процессу старую задачу. Каждый resume создаёт новый `AgentRun`.

### 8.2 Состояния

Минимальная модель lifecycle:

```text
Draft
  → Working
  → WaitingForUser
  → ExitedUnknown
  → CompletedVerified | FailedVerified
  → Paused | Canceled
```

`ExitedUnknown` принципиально отличается от `CompletedVerified`: исчезновение
процесса или exit code являются наблюдением, но не доказательством выполнения
пользовательской задачи.

### 8.3 Agent Inbox

Inbox сортирует задачи по необходимости действия:

1. ожидает пользователя;
2. подтверждённо завершилась с ошибкой;
3. подтверждённо завершилась и ещё не прочитана;
4. работает;
5. приостановлена или потеряла живой маршрут;
6. история.

Нажатие на задачу поздно разрешает маршрут до точного открытого проекта,
worktree, терминала, run и process generation. Неоднозначный или устаревший
маршрут безопасно отклоняется.

## 9. Обнаружение процессов в Linux

`pine-platform-linux` использует `/proc`, не разбирая terminal output:

- `/proc/<pid>/exe` для фактического executable;
- `/proc/<pid>/stat` для parent PID и process start time;
- дерево потомков PTY/shell для обнаружения агента;
- canonical executable basename и каталог разрешённых aliases;
- монотонный generation внутри конкретной terminal session.

Чтение `/proc` подвержено гонкам. До и после получения пути сверяется start time;
смешанные поколения PID отбрасываются. Lookalike-процессы остаются generic.

В будущем Pine может запускать task process в отдельном systemd user scope или
cgroup. Это упростит групповую отмену и containment, но не заменит проверку
идентичности и не входит в MVP.

## 10. Git и worktree

Git-команды запускаются напрямую с массивом аргументов и фиксированной рабочей
директорией. Shell interpolation запрещена для внутренних операций.

При создании параллельной задачи:

1. проверяется repository root и состояние основной рабочей копии;
2. создаётся предсказуемое имя branch/worktree;
3. новый worktree размещается в управляемой директории внутри репозитория или в
   явно выбранном пользователем корне;
4. задача получает canonical repository и worktree identity;
5. terminal cwd устанавливается в новый worktree;
6. удаление worktree всегда требует отдельного явного действия.

Сравнение и undo не должны затрагивать unrelated changes или staging state.
При расхождении ожидаемых blob identity операция прекращается и показывает diff.

## 11. Хранение

Используются XDG-директории:

```text
$XDG_CONFIG_HOME/pine/       настройки пользователя
$XDG_STATE_HOME/pine/        SQLite и долговременное состояние
$XDG_CACHE_HOME/pine/        восстанавливаемые кэши
$XDG_RUNTIME_DIR/pine/       сокеты и runtime-only данные
```

SQLite хранит проекты, задачи, runs, маршруты, layout и миграции схемы. Записи
изменяются транзакционно. База и директории создаются с правами только для
владельца.

Запрещено сохранять в task database:

- prompts и terminal transcript;
- file contents и выделенный текст;
- токены, credentials и environment;
- приватные vendor session identifiers;
- абсолютные пути в пользовательских уведомлениях.

Название и objective задачи сохраняются только как явно введённые пользователем
ограниченные строки.

## 12. Конкурентность

Используются два execution domain:

- GLib main loop владеет GTK-объектами и presentation state;
- Tokio runtime выполняет Git, LSP, filesystem, persistence и process polling.

Связь идёт через typed bounded channels. Каждый долгий запрос получает identity
и generation token. Устаревший результат не может обновить новый проект, tab,
document или task run.

Ни один Git, filesystem scan, SQLite migration или LSP read не выполняется в
GTK main loop.

## 13. Безопасность и границы доверия

- Terminal text является presentation data, а не управляющим протоколом.
- Process exit не является доказательством успеха.
- Неизвестный executable не получает capability только из-за похожего имени.
- Любой путь от агента считается недоверенным до canonicalization и проверки
  containment внутри project/worktree root.
- Запуск команды, применение patch, cleanup worktree и undo имеют раздельные
  разрешения.
- Notification не содержит transcript, credentials, абсолютных путей или
  vendor identifiers.
- Адаптер не получает произвольный доступ к GTK, базе задач или routing state.

Структурированные adapter events потребуют отдельного версионированного и
аутентифицированного протокола. До его появления агенты остаются detected-only.

## 14. Linux desktop integration

GTK/GDK абстрагирует Wayland и X11, но релизные тесты должны отдельно проверять:

- fractional scaling;
- несколько мониторов с разным scale factor;
- IME и compose/dead keys;
- clipboard и primary selection;
- drag-and-drop вкладок и файлов;
- accessibility через AT-SPI;
- OpenGL/Vulkan fallback и software rendering;
- GNOME Shell как основную среду;
- KDE и тайлинговые compositor как совместимый, но не визуально основной путь.

File chooser и уведомления используют D-Bus/xdg-desktop-portal там, где это
доступно, с безопасным fallback для обычной desktop-сессии.

## 15. Распространение

Рекомендуемый порядок:

1. development build из исходников;
2. `.deb` и `.rpm` для основных дистрибутивов;
3. AppImage или другой self-contained build;
4. Flatpak после проектирования доступа к host CLI, Git, language servers,
   `/proc` и пользовательским worktree.

Sandboxed package не должен выглядеть поддерживаемым, если он не может надёжно
запускать установленных пользователем агентов и проверять их процессы.

## 16. Тестирование

### Unit

- state machine задач и runs;
- route resolution и stale generation;
- parsing Git/LSP;
- canonical path containment;
- adapter catalogs и bounded input;
- SQLite migrations.

### Integration

- временные Git-репозитории и worktree;
- настоящий PTY с тестовым child process;
- process replacement и PID reuse simulations;
- LSP fixtures с reorder и stale responses;
- contract tests для VTE adapter и будущего `libghostty-vt` adapter.

### UI

- keyboard-only workflow;
- focus между editor и terminal;
- split/tab drag-and-drop;
- Inbox routing;
- light/dark/high-contrast темы;
- screenshot regression на Wayland и X11.

### Performance

- ввод в редакторе и терминале не блокируется фоновыми задачами;
- большие diff и репозитории загружаются постепенно;
- filesystem events coalesce;
- Agent Inbox обновляется инкрементально.

## 17. Этапы реализации

### Этап A: vertical slice

- GTK4 application и одно workspace window;
- shallow file list;
- один GtkSourceView tab;
- один встроенный VTE terminal;
- backend-neutral terminal launch contract;
- базовый Agent Inbox.

### Этап B: рабочий редактор

- tabs и split tree;
- Git status/diff;
- LSP diagnostics/navigation;
- file watching и external change review.

### Этап C: agent workflow

- detector catalog;
- `Task → Run → Terminal → Process`;
- project-local task list;
- глобальный Agent Inbox;
- desktop notifications.

### Этап D: параллельная работа

- worktree creation;
- task comparison;
- verified diff и completion brief;
- безопасный cleanup/undo.

### Этап E: долговременная платформа

- systemd user supervisor при подтверждённой необходимости;
- структурированные authenticated adapters;
- выделение Windows platform boundary;
- Windows frontend поверх общего ядра.

## 18. Открытые решения

- архитектура GTK/GSK renderer поверх `libghostty-vt`;
- момент замены VTE после rendering prototype;
- нужен ли daemon до первой публичной версии;
- допустима ли первая версия без Flatpak;
- формат пользовательских задач и keybindings;
- поддерживаемые CLI-агенты первого релиза.

## 19. Версионная политика

Минимальная native-система — Ubuntu 24.04 LTS: GTK 4.14, libadwaita 1.5,
GtkSourceView 5.12 и VTE 0.76. Ubuntu 26.04 проверяется как текущая LTS. N‑1
применяется к Rust toolchain и поколениям bindings: Rust 1.96.1, gtk4-rs 0.10.3,
libadwaita-rs 0.8.1, sourceview5-rs 0.10.0 и vte4-rs 0.9.0.

Ubuntu 22.04 не поставляет GTK4 VTE development package. Её поддержка возможна
через Flatpak или другой bundled runtime, но не входит в native-package MVP.
Повышение baseline меняет manifest, CI, `AGENTS.md` и документацию одной
транзакцией.

## 20. Ссылки

- [Pine](https://github.com/batonogov/pine)
- [Ghostty architecture](https://ghostty.org/docs/about)
- [Ghostty public VT headers](https://github.com/ghostty-org/ghostty/tree/main/include/ghostty/vt)
- [Zed terminal](https://zed.dev/features)
- [GTK4 documentation](https://docs.gtk.org/gtk4/)
- [GtkApplication](https://docs.gtk.org/gtk4/class.Application.html)
- [GtkSourceView 5](https://gnome.pages.gitlab.gnome.org/gtksourceview/gtksourceview5/)
- [XDG Base Directory Specification](https://specifications.freedesktop.org/basedir-spec/latest/)
- [Git worktree](https://git-scm.com/docs/git-worktree)
- [Language Server Protocol](https://microsoft.github.io/language-server-protocol/)
