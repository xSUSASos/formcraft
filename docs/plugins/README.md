# Расширения FormCraft: скрипты и add-in'ы

Руководство для авторов. Решения и мотивация — в [ADR 0003](../adr/0003-plugin-api-v1.md),
контракт — в [`crates/plugin-api/wit/formcraft.wit`](../../crates/plugin-api/wit/formcraft.wit).

## 1. Обзор: скрипты и add-in'ы

| | Скрипт | Add-in (плагин) |
|---|---|---|
| Что это | Внешняя программа (обычно Python), ведёт запущенный FormCraft по control channel | WebAssembly-компонент (`wasm32-wasip2`), загружается в приложение |
| Что умеет | Любые команды реестра, `model.inspect`, экспорт, рендер | Свои команды (кнопки, S-box, ИИ, MCP) и свои фичи на таймлайне с процедурной геометрией |
| Изоляция | Отдельный процесс, обычные права ОС | Песочница: нет файлов/сети/окружения, лимиты памяти и времени |
| Когда брать | Разовая автоматизация, пакетный экспорт, генерация моделей | Повторно используемая функция в интерфейсе, параметрическая фича, инструмент для ИИ |

Всё, что скрипты и add-in'ы делают с моделью, идёт через команды реестра — так же, как из UI, ИИ,
MCP и CLI. Поэтому работает undo, а схемы команд сразу видны ассистенту.

Три примера в репозитории (`examples/plugins/`):

- `gear` — фича «Шестерня» (`gear.spur`): эвольвентное колесо, custom feature.
- `material` — команда `material.mass`: масса видимых тел по таблице плотностей (без фич).
- `spin` — тестовый «плохой» плагин (`spin.forever`, `spin.hog`, `spin.panic`, `spin.box`): бесконечный
  цикл, 2 ГБ памяти, паника. Нужен, чтобы проверять песочницу; в обычной работе не ставьте.

Пример скрипта: `examples/scripts/export_stl.py`.

## 2. Быстрый старт: add-in на Rust

Нужны Rust и таргет: `rustup target add wasm32-wasip2`.

```text
formcraft-cli plugin new my_tool --lang rust
cd my_tool
cargo build --release --target wasm32-wasip2
```

Имя папки определяет id (приводится к нижнему регистру, все символы кроме букв и цифр становятся `_`;
id должен начинаться с буквы). Шаблон создаёт `Cargo.toml` (со своим пустым `[workspace]`),
`plugin.toml`, `src/lib.rs` (команда `my_tool.hello` и фича `my_tool.block`), `wit/formcraft.wit`,
`.gitignore`.

Скопируйте собранный компонент рядом с манифестом под именем из `wasm`:

```text
copy target\wasm32-wasip2\release\formcraft_addin_my_tool.wasm my_tool.wasm
```

Проверка без установки — запустить приложение с папкой add-in'а:

```text
formcraft --plugin C:\path\to\my_tool
```

Команды появятся в S-box, на панели (если задан `placement`) и в `formcraft-cli commands`.
Экспорты компонента: `wasm-tools component wit my_tool.wasm`.

Упаковка и установка:

1. Заархивируйте `plugin.toml` и `.wasm` в zip — в корень или в одну папку верхнего уровня
   (без `target/`; предел архива 1000 файлов и 256 МиБ).
2. Установите: `formcraft-cli plugin install my_tool.zip` (проверит загрузку и выведет список
   команд) или Scripts and Add-Ins (Shift+S) -> ADD-INS -> «Install from .zip…».

Add-in ложится в `<plugins>/<id>/`; повторная установка заменяет старую версию.

## 3. Манифест `plugin.toml`

```toml
id = "material"
name = "Масса по таблице материалов"
version = "1.0.0"
author = "FormCraft examples"      # необязательно
license = "MIT OR Apache-2.0"      # необязательно
description = "..."                # необязательно
api = "1.0"
wasm = "material.wasm"

[permissions]
files = []                         # папки, доступные плагину
network = false
```

| Ключ | Правила |
|---|---|
| `id` | Строчные ASCII-буквы, цифры, `_`, `-`; начинается с буквы; до 32 байт. Префикс всех команд (`material.mass`) |
| `name` | 1..80 символов |
| `version` | Строка, показывается в диалоге |
| `api` | `"мажор.минор"`. Мажор должен совпасть с хостом (сейчас 1), минор не больше хостового (0). Иначе: «plugin `x` is built for API 2.0, this FormCraft supports API 1.x» |
| `wasm` | Имя `.wasm` файла рядом с манифестом, без путей |
| `permissions.files` | Абсолютные пути или относительные от папки плагина; см. раздел 6 |
| `permissions.network` | В API v1 должен быть `false`; `true` — манифест отклоняется |

Неизвестные ключи запрещены (опечатка = ошибка), манифест не больше 64 КиБ.

## 4. Справочник WIT API (пакет `formcraft:plugin@1.0.0`)

Мир `plugin`. JSON ходит строками (`type json = string`), хост проверяет их по схемам.

### Экспорты плагина

- `init() -> result<tuple<list<command-spec>, list<feature-spec>>, string>` — вызывается один раз после
  загрузки (лимит 10 с). Возвращает команды и фичи для регистрации. Ошибка — плагин не загружен.
- `run-command(id, params) -> result<json, string>` — вызвана ваша команда. `params` уже проверены по
  `params-schema`. Результат — JSON-строка: объект возвращается как есть, остальное оборачивается в
  `{"result": ...}`, не-JSON текст становится строкой. `Err(текст)` — ошибка команды (плагин остаётся
  загруженным).
- `regenerate(kind, params) -> result<solid, string>` — построить тело custom feature (раздел 5).

### `command-spec`

| Поле | Смысл |
|---|---|
| `id` | Должен начинаться с `<id плагина>.` |
| `label`, `description` | Подпись и описание (описание читает ИИ) |
| `params-schema` | JSON schema параметров (JSON-объект). Из неё строятся диалог и промпт ИИ |
| `placement` | `option<placement>`: `tab`, `group`, `pinned` — место на панели |
| `shortcut` | `option<string>` — горячая клавиша |
| `ai` | Показывать ли команду ассистенту и MCP |

### `feature-spec`

`kind`, `label`, `description`, `params-schema`, `placement`, `ai`. Хост регистрирует команду
`<plugin>.<kind>` (например `gear.spur`).

### Интерфейс `host` (что плагин может вызвать)

- `execute(command, params) -> result<json, string>` — выполнить любую зарегистрированную команду
  (встроенную или другого плагина). Работает на текущей сессии, поэтому undo/redo как обычно.
  Доступна только пока выполняется команда плагина; из `init` и `regenerate` вернёт ошибку.
- `inspect() -> json` — результат `model.inspect`: `bodies[]` (`name`, `volume` в мм³, `visible`, ...),
  фичи со статусом, параметры. Вне команды — `"{}"`.
- `log(level, message)` — 0 trace, 1 debug, 2 info, 3 warn, 4 error. Сообщение (до 2000 символов)
  попадает в лог плагина (последние 500 строк; кнопка «Log» в диалоге Scripts and Add-Ins).
- `read-file(path)` / `write-file(path, data)` — только внутри папок из `[permissions] files`; `..` в
  пути запрещён; чтение до 64 МиБ.

Плагин не может через `execute` вызвать собственную команду (повторный вход) — получит ошибку.

### Интерфейс `geometry` (для custom features)

Тела — ресурс `solid`, им владеет хост; плагин держит только хэндл.

- `make-box(corner, size)`, `make-cylinder(base, axis, radius, height)`.
- `extrude(plane, outer, holes, distance)` — выдавить полигоны на плоскости (`plane`: `origin`, `x-dir`,
  `normal`) вдоль нормали; первый контур внешний (против часовой), остальные — отверстия. Дуги плагин
  заменяет точками сам. Контур: 3..100000 точек, координаты конечные, `distance` не ноль.
- Методы `solid`: `mass-props()` (объём, площадь, bbox), `transformed(offset, origin, axis, angle-deg)`,
  `fuse(other)`, `cut(tool)`, `common(other)`.

Все операции проверяются ядром: плохая геометрия — `err`, а не падение.

## 5. Custom features

Фича плагина — запись таймлайна `Custom { plugin, kind, params }`. Команда `<plugin>.<kind>` добавляет
её (как родную: `operation`/`target` для new/join/cut/intersect), а при каждой перестройке хост
вызывает `regenerate`.

- **Детерминизм.** `regenerate` вызывается на каждой перестройке и при открытии файла; одинаковые
  `params` должны давать одинаковое тело. Никаких случайных чисел, времени, внешнего состояния.
- **Параметры уже вычислены.** Пользователь вводит выражения (`"teeth"`, `"L/2"`), `regenerate` получает
  JSON с готовыми значениями. В схеме разрешайте `["number", "string"]`, как в `gear`. Числа всё равно
  проверяйте сами: конечные, положительные, в разумном диапазоне.
- **Замороженное тело.** Последнее успешное тело сохраняется (BREP) в документе. Если плагина нет,
  он выгружен или `regenerate` вернул ошибку/упал, фича строится из замороженного тела и получает
  предупреждение на таймлайне. Документ открывается и без плагина.
- **Правка** идёт через ту же команду (диалог фичи); смена параметра запускает `regenerate`.

## 6. Песочница, лимиты, разрешения

- Отдельный `Store` (wasmtime) на каждый плагин. WASI без файлов, сети и переменных окружения.
- **Память:** 256 МиБ на плагин.
- **Время:** epoch interruption, тик 10 мс; дедлайн 2 с на вызов (`run-command`, `regenerate`), 10 с на `init`.
- **Trap, паника, таймаут, превышение памяти** — плагин выгружается: его команды снимаются, фичи
  переходят на замороженные тела, причина попадает в статус и лог, приложение продолжает работать.
  Вернуть плагин: Reload в диалоге.
- **Файлы:** только папки из `[permissions] files` (относительные считаются от папки плагина), через
  `host.read-file/write-file`.
- **Сеть:** недоступна.
- **Версии:** `api` сверяется при загрузке; несовместимый плагин не грузится с понятным сообщением.
- **Установка из zip:** `plugin.toml` в корне или в одной верхней папке; пути проверяются (только
  обычные компоненты пути: без `..` и абсолютных), не более 1000 файлов и 256 МиБ, файл из `wasm`
  обязан существовать; распаковка идёт во временную папку, потом заменяет `<plugins>/<id>/`.

Где лежат add-in'ы: `%APPDATA%/FormCraft/plugins` на Windows, `$XDG_CONFIG_HOME/formcraft/plugins` или
`~/.config/formcraft/plugins` в других ОС. Переменная `FORMCRAFT_PLUGINS` переопределяет папку.
Скрипты — в соседней папке `scripts`.

## 7. Интеграция с ИИ

Команды и фичи с `ai: true` попадают в промпт ассистента и в MCP `list_commands` автоматически — все
читают один реестр команд. Что для этого сделать:

- Пишите содержательное `description` с примером («Example: teeth 20, module 2, thickness 10, bore 8»).
- Описывайте параметры в схеме: `type`, `enum`, `description`, `required`. Хорошая схема делает
  ответы модели точнее (см. `material`: `enum` материалов).
- Возвращайте из команды понятный `message` и числа в JSON.
- `ai: false` — команда доступна только человеку (UI, S-box, CLI, скрипты); ассистент отклонит её
  («is not available to the assistant»).

## 8. Скрипты (Python)

Пакет `python/formcraft` — чистый Python, только стандартная библиотека. Скрипт управляет запущенным
FormCraft через control channel (loopback, строки JSON `{id, method, params}` -> `{id, result|error}`).

Запуск приложения с каналом: `formcraft --control 7878`. Порт клиент берёт из `FORMCRAFT_PORT`
(по умолчанию 7878).

```python
import formcraft

with formcraft.connect() as fc:          # connect(port=None, host="127.0.0.1", timeout=300)
    fc.parameter("plate_length", 80)
    fc.execute("solid.box", size=["plate_length", 50, 6])
    for b in fc.bodies():
        print(b["name"], b["volume"])
    fc.export("C:/out/part.stl")
```

| Метод | Назначение |
|---|---|
| `connect(port, host, timeout)` / `Client(...)` | Подключение (контекстный менеджер) |
| `execute(command, **params)` | Любая команда реестра, в том числе команды add-in'ов |
| `commands()` | Список команд со схемами |
| `inspect()`, `bodies()` | `model.inspect` / список тел |
| `parameter(name, expression, comment=None)` | `doc.parameter` |
| `new(name=None)`, `open(path)`, `save(path)` | Документ |
| `export(path, bodies=None)` | STEP/STL по расширению; `bodies` — id или имена |
| `render(path, view="iso", width=1024, height=768)` | PNG через `ui.render` |
| `call(method, params)` | Сырой вызов протокола |

Ошибки приложения поднимают `formcraft.FormCraftError`. Методы протокола: `engine.execute`,
`engine.commands`, `model.inspect`, `ui.render` и другие (см. ADR 0003).

**Запуск из диалога.** Scripts and Add-Ins (Shift+S) -> вкладка SCRIPTS: список `.py` из папки
`scripts` (и из `--scripts DIR`), кнопки «Run» и «Run file…». При первом запуске приложение само
поднимает control channel на свободном порту и стартует `python <скрипт>` (на Windows `python`, иначе
`python3`) с `FORMCRAFT_PORT`; рабочая папка — папка скрипта. Вывод показывается в диалоге; код
выхода 0 — «Script finished».

Заготовка скрипта: `formcraft-cli plugin new report --lang python-script` (создаёт `report.py`).
Пример: `examples/scripts/export_stl.py <папка>` — каждое видимое тело в свой STL.

MCP использует тот же канал: `formcraft-cli mcp --port 7878` подключается к приложению,
запущенному с `--control 7878`.

## 9. Справочник флагов

`formcraft` (приложение): `--plugin DIR` (загрузить add-in из папки, повторяемо), `--scripts DIR` (ещё
одна папка скриптов для вкладки SCRIPTS), `--control PORT` (control channel на 127.0.0.1).

`formcraft-cli`: `plugin install <addin.zip>`, `plugin list`, `plugin new <name> [--lang rust|python-script]`;
глобально `--plugin <dir>` (повторяемо) и `--no-plugins` (не грузить установленные). Например:
`formcraft-cli --plugin examples/plugins/material commands material`.

## 10. Если что-то не работает

| Симптом | Причина / что делать |
|---|---|
| `plugin.toml: bad id ...` | id не из `a-z0-9_-`, не с буквы или длиннее 32 |
| `built for API X, this FormCraft supports API 1.x` | Пересоберите под API 1.0 или обновите приложение |
| `not a valid component` | Собран не под `wasm32-wasip2`; проверьте `wasm-tools component wit x.wasm` |
| `command ... must start with <id>.` | Все id команд начинаются с id плагина и точки |
| `parameter schema is not JSON` / `must be an object` | `params-schema` — валидный JSON-объект |
| `... failed to start` | Ошибка при инстанцировании компонента |
| `plugin ... timed out after 2000 ms` | Вызов дольше 2 с; уберите тяжёлые циклы |
| `plugin ... crashed: ...` | Паника или trap; смотрите Log в диалоге, плагин выгружен — Reload |
| `plugin ... is busy (re-entrant call)` | Команда плагина через `execute` вызвала собственную команду |
| `no permission for ...` | Путь вне `[permissions] files` или с `..` |
| `unsafe path in the archive` | В zip есть `..` или абсолютные пути |
| `... is missing` при установке | Файла из `wasm` нет в архиве |
| Предупреждение у фичи на таймлайне | Плагин не загружен или `regenerate` вернул ошибку; используется замороженное тело |
| Скрипт: `cannot start python` | Python нет в PATH |
| Скрипт: connection refused | Приложение запущено без `--control`; из диалога порт передаётся сам |
