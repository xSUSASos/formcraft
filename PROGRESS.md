# FormCraft — PROGRESS

> Новая сессия: «Прочитай PLAN.md и PROGRESS.md, продолжай цикл». Этот файл + `git log` — всё состояние.
> Сессии иногда обрываются: PROGRESS.md обновлять после каждого шага, долгие (> 10 мин) сборки
> запускать фоном.

## Сейчас
- **Фаза:** 1 — Каркас и оболочка интерфейса (почти закрыта)
- **Сделано в фазе 1:** workspace, xtask ci, CI workflow, вся раскладка п. 2 (app bar, toolbar с вкладками и группами, browser dock/float/close, canvas с сеткой и осями, ViewCube 26 видов + Home, navigation bar, timeline, свёрнутые Text Commands), темы, сохранение раскладки, Reset Layout, скриншоты в `docs/images/`.
- **Осталось в фазе 1:** иконки по ТЗ (делегировать Sonnet), docs/learn/phase-1.md (делегировать), проверка приёмки и запись в этот файл, `git tag phase-1`.
- **Дальше:** Фаза 2 — тела на экране: рендер мешей через wgpu callback, выбор граней/рёбер/тел, команды примитивов и булевых, STEP/STL экспорт, `formcraft-cli run --save`.

## Как собирать и проверять
- `cargo xtask ci` — fmt, clippy -D warnings, тесты, слои, cargo deny. Обычный `target/` (OCCT собирается там, первая сборка 30–60 мин).
- Скриншоты: `target/debug/formcraft.exe --screenshot C:/fc-target/x.png [--theme light] [--float-browser] [--text-commands] [--view top]`, потом смотреть PNG.
- STEP во FreeCAD: `tools/freecad_check.py` (`FC_FILES="a.step;b.step" freecadcmd script` — только ASCII-пути, `C:/fc-target/...`).
- Ollama: сервер `%LOCALAPPDATA%\Programs\Ollama\ollama.exe serve`, модель `qwen2.5-coder:7b` создана из GGUF (`models/*.Modelfile`, копия в `C:/fc-models`).

## Фазы
| Фаза | Статус | Тег |
|---|---|---|
| 0 Окружение и ядро | готово: ADR 0001, `Kernel` + `occt`, golden-тесты | phase-0 |
| 1 Каркас и оболочка UI | почти готово | — |
| 2 Тело на экране | — | — |
| 3 Эскиз, фичи, таймлайн | — | — |
| 4 Команды, CLI, MCP | — | — |
| 5 Встроенный ИИ | — | — |
| 6 Плагины и скрипты | — | — |
| 7 Шлифовка и релиз | — | — |

## Приёмка Фазы 0
- ADR 0001 с таблицей «прошёл/не прошёл»: `docs/adr/0001-geometry-kernel.md` — OCCT 5/5, STEP во FreeCAD чистый; truck 3/5 с ошибками STEP; Fornjot 0/5.
- Ядро за трейтом: `crates/kernel` (`Kernel`, `Shape`, `Profile`, `Mesh`) + `crates/occt`.
- `cargo test -p formcraft-occt`: 16 passed (golden: box_hole, fillet_all, cyl_cut, revolve, shell, union/intersect, chamfer, slot с дугами, нормали, рёбра, тесселяция, BRep, compound, transform, hostile inputs, параллельность) + `tests/step.rs`: 3 passed (round trip в кириллической папке, отсутствующий и мусорный файл).
- `cargo xtask ci`: «CI summary: all 5 steps passed».

## Журнал
- 2026-10-14: репозиторий, PLAN.md, PROGRESS.md; Rust; клон `../cadcraft-ref` изучен (CommandSpec-реестр, never-crash lints, xtask ci + layers, MCP stdio JSON-RPC).
- 2026-10-15: spike: OCCT 5/5, truck 3/5, Fornjot 0/5 → ADR 0001 (OCCT за трейтом, своя обвязка — opencascade-rs роняет процесс на исключениях OCCT).
- 2026-10-15: `crates/occt` — свой cxx-мост, FC_GUARD, глобальный мьютекс, проверка результатов (BRepCheck, объём, bbox для shell). Реестр команд в `engine` (регистрация в рантайме, поиск, panic guard). UI-оболочка в стиле Fusion. ADR 0002 (лицензии). CI зелёный. Коммит 96c0b73.

## Требует человека
- Прокси/VPN в fake-IP режиме ломает `ollama pull` (CDN → 198.18.x). Модели качаются вручную с Hugging Face (`docs/environment.md`). Чтобы `ollama pull` заработал: исключить `registry.ollama.ai` и `*.r2.cloudflarestorage.com` из fake-IP.
- Ollama 0.40.4 не находит RTX 4050 (`initial_count=0`), инференс идёт на CPU (~14 с на короткий ответ 7B). Проверить: настройки графики Windows → «Высокая производительность» для `ollama.exe`/`ollama app.exe`, либо обновить Ollama. Драйверы не трогали (запрещено).
- qwen3-8b GGUF докачивается скриптом `C:/fc-models/dl.sh` (после докачки: `ollama create qwen3:8b -f C:/fc-models/qwen3-8b.Modelfile`).

## Тупики / известные проблемы
- libtest + STEP: тестовый бинарник после любого STEP-обмена падает с access violation при выходе процесса (все тесты уже прошли). Обычные программы (пример с main, приложение) выходят чисто, в т.ч. с потоком и `process::exit`. Причину не нашли за отведённое время (кандидат — порядок статических деструкторов OCCT при статической компоновке). Обход: STEP-тесты — `crates/occt/tests/step.rs` с `harness = false`.
- bat-файлы с кириллическим путём и `chcp 65001` пропускают строки (`set CARGO_TARGET_DIR` не применился) — не использовать.

## Делегировано Sonnet
- `docs/learn/phase-0.md` — конспект фазы 0 по моему плану (первая попытка упала по сети, вторая успешна). Проверено: структура, числа сверены с ADR; одна устаревшая строка про `CARGO_TARGET_DIR` исправлена мной.
