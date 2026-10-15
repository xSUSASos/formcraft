# Фаза 4 — команды, CLI, MCP

Этот документ для тех, кто знает базовый Rust и прочитал `phase-0.md` … `phase-3.md`. Он
объясняет, как FormCraft управляется снаружи: из командной строки, из внешнего ИИ-агента по
протоколу MCP и через сетевой канал запущенного окна. Первоисточники: `PLAN.md` (Фаза 4),
`PROGRESS.md`, `docs/adr/0003-plugin-api-v1.md`, крейты `render`, `mcp`, `ui-egui`, `formcraft-cli`.

## Что сделано

- Рендер тел в PNG без окна (`crates/render/src/headless.rs`).
- CLI `formcraft-cli`: подкоманды `run`, `commands`, `info`, `convert`, `render`, `mcp`.
- Крейт `mcp`: сервер MCP (JSON-RPC 2.0 по stdio) с инструментами `execute`, `list_commands`,
  `inspect_model`, `render`, `list_selectors`, `run_script`, `new_design` и, если подключено живое
  приложение, `screenshot` и `ui_click`.
- Канал управления в приложении: `formcraft --control PORT` (`crates/ui-egui/src/control.rs`).
- `.mcp.json` в корне репозитория: Claude Code может подключить FormCraft как MCP-сервер.
- ADR 0003 «Plugin API v1» и черновик WIT-интерфейса, без реализации (она в Фазе 6).
- Приёмка: кронштейн построен через MCP в живом окне, грань выбрана через `ui_click`.

Снимок: `docs/images/phase4-mcp-remote.png` — кронштейн, собранный через MCP в запущенном
приложении; верхняя грань выбрана командой `ui_click` в мировых координатах.

## Ключевые понятия

**MCP (Model Context Protocol)** — открытый протокол, по которому ИИ-агент (например, Claude Code)
находит и вызывает инструменты внешней программы. Агент спрашивает у сервера список инструментов
с описаниями и схемами аргументов, затем вызывает их. FormCraft выступает сервером.

**JSON-RPC 2.0** — формат сообщений, на котором построен MCP. Три вида сообщений:

- *запрос* (request): `{"jsonrpc":"2.0","id":1,"method":"tools/list","params":{}}`, у него есть `id`
  и на него ждут ответ;
- *уведомление* (notification): то же самое без `id`, ответа нет (например,
  `notifications/initialized`);
- *ответ* (response): `{"jsonrpc":"2.0","id":1,"result":{...}}` или `"error":{"code":…,"message":…}`.

Коды ошибок, которые использует сервер (`crates/mcp/src/server.rs`): `-32700` — не разобрался JSON,
`-32600` — это не корректное сообщение (не объект, пустой пакет, нет `method`), `-32601` — такого
метода нет, `-32602` — плохие параметры (неизвестный инструмент, `arguments` не объект).

**Транспорт stdio.** Агент запускает `formcraft-cli mcp` как дочерний процесс и обменивается с ним
строками JSON через stdin/stdout. Поэтому **stdout занят протоколом**: любой посторонний `println!`
сломал бы обмен. Журнал и диагностика идут только в stderr.

**Инструменты и ресурсы.** *Инструмент* (tool) — действие с аргументами, которое может менять
состояние (`execute`). *Ресурс* (resource) — данные, которые можно прочитать по адресу, без
побочных эффектов: `formcraft://model`, `formcraft://commands`, `formcraft://selectors`.

**Всё есть команда.** Это правило из `CLAUDE.md` окупается именно здесь. Каждая операция — команда
с JSON-схемой в реестре движка. Кнопка тулбара, окно S, ИИ, MCP, CLI и будущие плагины зовут один и
тот же `Session::execute`. Поэтому новая команда автоматически появляется во всех входах, а
списки команд нигде не пишутся руками.

**Control channel** — TCP-сервер внутри запущенного приложения на 127.0.0.1. Через него внешняя
программа (в том числе MCP-мост) управляет тем же окном, которое видит человек.

## Рендер без окна

`crates/render/src/headless.rs`, функция `render_png`. Обычный рендерер рисует в окно через egui;
здесь `Renderer::headless()` создаёт wgpu-устройство без поверхности, тела тесселируются
(`formcraft_tess::mesh`), камера подгоняется под габариты (`cam.fit`), кадр рисуется в
текстуру, пиксели читаются обратно и кодируются в PNG (`encode_png`).

Параметры (`Options`): `width`/`height` (по умолчанию 1024×768, зажимаются в 16…4096), `view`
(`iso` по умолчанию, `top`, `bottom`, `front`, `back`, `left`, `right`), `edges`, `grid`. Для
всех видов, кроме `iso`, камера ортографическая. Создание GPU-устройства медленное и может
не удаться (нет видеокарты), поэтому оно ленивое (`control::renderer`), а ошибка превращается в
понятное сообщение «rendering is not available», а не в падение.

## Общие методы сессии

`crates/render/src/control.rs`, функция `session_call`. Это набор методов, одинаковый для
headless-MCP и для приложения:

| Метод | Что делает |
|---|---|
| `engine.execute {command, params}` | выполнить команду реестра |
| `engine.commands` | список команд со схемами |
| `model.inspect` | тела (объём, габариты, число граней), таймлайн, параметры |
| `selectors.list` | как ссылаться на грани и рёбра |
| `ui.render {width, height, view, edges, grid, path}` | PNG: `{pngBase64, width, height, bodies}` |

Методы живого окна добавляются поверх: `ui.screenshot`, `ui.click`, `ui.state`.

## Как устроен MCP-сервер

- `crates/mcp/src/server.rs`: цикл `serve` читает строки из stdin и пишет ответы в stdout.
  `handle_line` разбирает JSON (поддержан и пакет-массив), `handle` отличает запрос от
  уведомления, `request` обрабатывает методы `initialize`, `ping`, `tools/list`, `tools/call`,
  `resources/list`, `resources/read`. Длинные строки (больше 64 МБ) отклоняются как враждебные.
- `initialize` — рукопожатие. Сервер отвечает версией протокола (`2025-06-18`, также понимает
  `2025-03-26` и `2024-11-05`), своими возможностями и текстом `instructions`: краткой памяткой
  для модели (единицы мм, Z вверх, пример цепочки команд).
- `crates/mcp/src/tools.rs`: описания инструментов (`definitions`) и их выполнение (`call`).
  Инструмент — тонкая обёртка над методом сессии: `execute` → `engine.execute`,
  `inspect_model` → `model.inspect`, `render` → `ui.render` и т. д. `run_script` принимает
  скрипт построчно (`команда {json}`, тот же формат, что у `formcraft-cli run`) и останавливается
  на первой ошибке.
- **Ошибка команды — не ошибка протокола.** Если `solid.fillet` отказал (например, радиус
  слишком велик), инструмент возвращает обычный результат с `"isError": true` и текстом: модель
  видит причину и исправляется. Код `-32602` только для неизвестного инструмента или
  неправильной формы аргументов.
- **Картинки.** Результат `render` содержит блок `{"type":"image","data":<base64>,"mimeType":"image/png"}`.
  Base64 кодирует байты PNG в текст из букв, цифр, `+`, `/`, поэтому картинка проходит через
  JSON. Функция `base64` написана вручную (`render/src/control.rs`), чтобы не тянуть зависимость;
  тест `base64_matches_rfc4648` сверяет её с примерами из RFC 4648.

### Два бэкенда

Сервер не знает, где живёт модель. Он работает через трейт `Backend` (`crates/mcp/src/backend.rs`)
с методами `call`, `has_ui`, `describe`:

- **`Headless`** — своя `Session` внутри процесса, окна нет. Инструментов `screenshot` и
  `ui_click` в списке нет, потому что `has_ui()` ложно.
- **`Remote`** — TCP-клиент к приложению (`127.0.0.1:PORT`). Строка запроса:
  `{"id":N,"method":…,"params":…}`, ответ: `{"id":N,"result":…}` или `{"id":N,"error":"…"}`.
  Появляются `screenshot` и `ui_click`.

## Канал управления в приложении

`formcraft --control 7878` (`apps/formcraft/src/main.rs`) вызывает `control::listen`
(`crates/ui-egui/src/control.rs`). Привязывается только 127.0.0.1, то есть снаружи компьютера
канал недоступен.

**Почему всё выполняется в UI-потоке.** `Session` и состояние приложения не рассчитаны на доступ
из нескольких потоков, а блокировки вокруг всей сессии усложнили бы код. Поэтому потоки соединений
только читают строку, разбирают JSON и кладут `Request` в очередь `mpsc`, сам же ждут ответ в
одноразовом канале (таймаут 120 с). UI-поток в каждом кадре вызывает `serve_control`
(`crates/ui-egui/src/app.rs`), забирает запросы, выполняет `control_call` и отправляет ответ.
Чтобы приложение, которое ничего не рисует, проснулось, поток соединения после постановки в
очередь вызывает `wake` — это `ctx.request_repaint()`.

**Скриншот.** Картинку окна умеет делать только egui, и только между кадрами. Поэтому
`ui.screenshot` не отвечает сразу: запрос откладывается в `pending_shots`, отправляется
`ViewportCommand::Screenshot`, и когда в одном из следующих кадров приходит событие
`Event::Screenshot` (помеченное маркером `ControlShot`, чтобы не спутать со скриншотом по
кнопке), результат кодируется в PNG и уходит в ответ. Пока снимка нет, запрашивается новый кадр.

**Клик.** `ui.click {x, y, z?, space, button}` (`control_click`):

- `space: "screen"` (по умолчанию) — пиксели окна, как на скриншоте; делятся на
  `pixels_per_point` и переводятся в координаты холста;
- `space: "world"` — точка модели `(x, y, z)`; камера проецирует её на экран
  (`camera.project`). Так надёжнее, чем угадывать пиксели: «кликнуть в центр верхней грани»
  значит `{x:30, y:30, z:5, space:"world"}`;
- `button: "left"` выбирает то, что под курсором (как клик в режиме Select), ответ содержит
  `hit` и описание выбора; `"right"` открывает маркирующее меню. Нечисловые и бесконечные
  координаты, точки вне холста и клики в режиме эскиза возвращают ошибку.

Тест `control_channel_drives_the_app` (`crates/ui-egui/src/app_tests.rs`) делает всё это по TCP
из отдельного потока: создаёт коробку, проверяет объём 12000, кликает по верхней грани, затем
проверяет ошибки (плохой клик, неизвестный метод, невалидный JSON).

## CLI

`apps/formcraft-cli/src/main.rs`:

| Команда | Назначение |
|---|---|
| `run <script\|-> [--save out.step\|out.stl] [--quiet]` | выполнить скрипт, вывести `model.inspect` |
| `commands [filter]` | каталог команд со схемами |
| `info <file.step\|file.fcraft>` | свойства файла |
| `convert <in> <out.step\|out.stl>` | конвертация (вход `.step`, `.fcraft` или `.fcs`) |
| `render <in> <out.png> [--view V] [--size WxH] [--no-grid] [--no-edges]` | PNG без окна |
| `mcp [--port N]` | MCP-сервер на stdio; с `--port` подключается к `formcraft --control N` |

`.mcp.json` в корне регистрирует сервер `formcraft` как `target/debug/formcraft-cli.exe mcp`
(headless). Новая сессия Claude Code подхватит его после одобрения пользователем.

## Расширяемость и ADR 0003

Реестр команд в движке заполняется в рантайме, с полем `source` у каждой команды и снятием по
источнику. ADR 0003 использует это: команды плагина получат `source = plugin:<id>` и сразу
попадут в окно S, `list_commands` MCP и промпт ИИ, потому что все читают реестр.

План плагинов (реализация в Фазе 6): WebAssembly-компоненты в `wasmtime`. *Component model* — это
стандарт, где интерфейс между хостом и модулем описывается на языке **WIT** (`world`,
`interface`, `resource`, типизированные функции), а привязки к Rust, C, Go и JS генерируются
автоматически. Черновик лежит в `crates/plugin-api/wit/formcraft.wit` и пока не компилируется.
Идеи: параметры и результаты команд идут JSON-строками и проверяются хостом по схеме; геометрия —
ресурс `solid`, которым владеет хост; плагин меняет модель только через `host.execute` (undo
работает как всегда); лимиты памяти и времени, падение плагина не роняет приложение. Скрипты
(Python) пойдут через тот же control channel, поэтому его протокол теперь считается публичным API.

## Куда смотреть в коде

| Что | Где |
|---|---|
| PNG без окна | `crates/render/src/headless.rs` |
| Общие методы сессии | `crates/render/src/control.rs` |
| JSON-RPC и жизненный цикл MCP | `crates/mcp/src/server.rs` |
| Инструменты MCP | `crates/mcp/src/tools.rs` |
| Бэкенды Headless / Remote | `crates/mcp/src/backend.rs` |
| Тесты MCP | `crates/mcp/src/tests.rs` |
| TCP-канал приложения | `crates/ui-egui/src/control.rs` |
| `serve_control`, `control_call`, `control_click` | `crates/ui-egui/src/app.rs` |
| Подкоманды CLI | `apps/formcraft-cli/src/main.rs` |
| Подключение к Claude Code | `.mcp.json` |
| Plugin API | `docs/adr/0003-plugin-api-v1.md`, `crates/plugin-api/wit/formcraft.wit` |

## Как проверить

Тесты (первая сборка долгая, OCCT собирается 30–60 мин):

```
cargo test -p formcraft-mcp
cargo test -p formcraft-ui-egui control_channel
cargo xtask ci
```

`formcraft-mcp` проверяет жизненный цикл, сборку кронштейна через `run_script` с точным объёмом,
рендер PNG (или понятную ошибку без GPU) и коды ошибок протокола.

Рендер эталонной детали без окна:

```
target/debug/formcraft-cli.exe render examples/golden/flange.fcs out.png --size 640x480
target/debug/formcraft-cli.exe render examples/golden/flange.fcs top.png --view top --no-grid
```

MCP вручную, headless. Каждая строка — одно сообщение, ответ приходит строкой в stdout:

```
echo '{"jsonrpc":"2.0","id":1,"method":"tools/list"}' | target/debug/formcraft-cli.exe mcp
```

С живым окном. В первом терминале:

```
target/debug/formcraft.exe --control 7878
```

Во втором запустить мост и подать запросы, например из файла:

```
target/debug/formcraft-cli.exe mcp --port 7878 < requests.jsonl
```

Строки в `requests.jsonl` (сначала `initialize`, потом вызовы):

```
{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{}}}
{"jsonrpc":"2.0","method":"notifications/initialized"}
{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"execute","arguments":{"command":"solid.box","params":{"size":[40,30,10]}}}}
{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"inspect_model","arguments":{}}}
{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"ui_click","arguments":{"x":20,"y":15,"z":10,"space":"world"}}}
```

В окне после этого коробка появится на холсте, а верхняя
грань будет выбрана. Для Claude Code достаточно открыть репозиторий и одобрить сервер `formcraft`
из `.mcp.json`.

## Что дальше

- Шаг 7 Фазы 4: этот документ и тег `phase-4`.
- Фаза 5: встроенный ИИ. Он берёт команды и их схемы из того же реестра, что и MCP, поэтому
  структурный вывод модели — это вызовы `engine.execute`.
- Фаза 6: хост плагинов по ADR 0003, `FeatureKind::Custom`, Python-пакет `formcraft` поверх
  control channel.

## Проверь себя

1. Чем отличаются запрос, уведомление и ответ в JSON-RPC?
2. Почему сервер MCP по stdio не должен печатать ничего лишнего в stdout?
3. Чем инструмент MCP отличается от ресурса?
4. Почему запросы control channel выполняются в UI-потоке и зачем нужен `request_repaint`?
5. Почему `ui.screenshot` отвечает не сразу?
6. Что возвращает инструмент, если команда движка вернула ошибку?

<details>
<summary>Ответы</summary>

1. У запроса есть `id` и на него ждут ответ; у уведомления `id` нет, ответа не будет; ответ
   содержит `id` запроса и либо `result`, либо `error`.
2. Stdout — это сам канал протокола. Посторонний текст разрушил бы разбор сообщений на стороне
   агента. Журнал идёт в stderr.
3. Инструмент — действие с аргументами, может менять модель. Ресурс — данные для чтения по
   адресу (`formcraft://model`).
4. Сессия не рассчитана на несколько потоков. Потоки соединений только ставят запрос в очередь
   `mpsc`; UI-поток исполняет его между кадрами. `request_repaint` будит приложение, если оно
   простаивает и не рисует новые кадры.
5. Снимок окна делает egui между кадрами по `ViewportCommand::Screenshot`. Запрос ждёт события
   `Event::Screenshot` и отвечает, когда оно пришло.
6. Обычный результат с `isError: true` и текстом ошибки, чтобы модель могла исправиться. Ошибка
   протокола (`-32602`) только для неизвестного инструмента или неверной формы аргументов.

</details>
