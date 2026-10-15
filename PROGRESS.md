# FormCraft — PROGRESS

> Новая сессия: «Прочитай PLAN.md и PROGRESS.md, продолжай цикл». Этот файл + `git log` — всё состояние.
> Сессии иногда обрываются: PROGRESS.md обновлять после каждого шага, сборки > 10 мин — фоном.

## Сейчас
- **Фаза:** 4 — Команды, CLI, MCP (в работе). Фаза 3 закрыта, тег phase-3.
- **Сделано в фазе 4:** шаги 1–3 — `render::headless` (PNG), CLI `convert`/`render`/`mcp [--port N]`, крейт `mcp` (stdio JSON-RPC, инструменты execute/list_commands/inspect_model/render/list_selectors/run_script/new_design; screenshot/ui_click для Remote). Тесты mcp 5/5, CI зелёный.
- **Следующий шаг:** 4 — control channel в приложении (`formcraft --control PORT`: engine.execute, model.inspect, ui.render, ui.screenshot, ui.click; бэкенд Remote уже ждёт этот протокол в `crates/mcp/src/backend.rs`), затем 5–7.
- **План фазы 4:**
  1. `render::headless`: PNG тел (headless wgpu) — для `formcraft-cli render` и MCP `render`.
  2. CLI: `convert` (STEP→STL/STEP), `render`, `mcp`.
  3. Крейт `mcp`: JSON-RPC 2.0 по stdio, инструменты `execute`, `list_commands`, `inspect_model`, `render`, `list_selectors`, `screenshot`, `ui_click`; бэкенды Headless (сессия в процессе) и Remote (TCP control channel запущенного приложения).
  4. Control channel в приложении: `formcraft --control PORT` (127.0.0.1, JSON-строки: engine.execute, model.inspect, ui.screenshot, ui.click, ui.render).
  5. `.mcp.json` в репо (formcraft-cli mcp) + проверка: построить кронштейн через MCP, проверить inspect_model и render.
  6. ADR 0003 «Plugin API v1» (WIT), без реализации.
  7. docs/learn/phase-4.md, тег phase-4.

## Как собирать и проверять
- `cargo xtask ci` — fmt, clippy -D warnings, тесты, слои, cargo deny. Обычный `target/` (первая сборка OCCT 30–60 мин).
- Тесты: 81 обычных + `crates/occt/tests/step.rs` (3) + `apps/formcraft-cli/tests/acceptance.rs` (6), последние два — `harness = false`.
- Приложение: `target/debug/formcraft.exe [--script examples/bracket.fcs] [--screenshot C:/fc-target/x.png] [--theme light] [--float-browser] [--text-commands] [--view top]`, потом смотреть PNG.
- CLI: `target/debug/formcraft-cli.exe run examples/bracket.fcs --save C:/fc-target/b.step`, `commands`, `info file.step`.
- FreeCAD: `FC_FILES="C:/fc-target/b.step" "%LOCALAPPDATA%/Programs/FreeCAD 1.1/bin/freecadcmd.exe" C:/fc-target/fcc.py` (`fcc.py` = копия `tools/freecad_check.py`; только ASCII-пути). Или `FREECAD_CMD=... cargo test -p formcraft-cli --test acceptance`.
- Ollama: `%LOCALAPPDATA%\Programs\Ollama\ollama.exe serve`; модель `qwen2.5-coder:7b` создана из GGUF (`models/*.Modelfile`, копия в `C:/fc-models`). Работает на CPU.

## Фазы
| Фаза | Статус | Тег |
|---|---|---|
| 0 Окружение и ядро | готово | phase-0 |
| 1 Каркас и оболочка UI | готово | phase-1 |
| 2 Тело на экране | готово | phase-2 |
| 3 Эскиз, фичи, таймлайн | готово | phase-3 |
| 4 Команды, CLI, MCP | в работе (шаги 1–3) | — |
| 5 Встроенный ИИ | — | — |
| 6 Плагины и скрипты | — | — |
| 7 Шлифовка и релиз | — | — |

## Приёмка Фазы 0
- ADR 0001: OCCT 5/5, STEP во FreeCAD чистый; truck 3/5 с ошибками STEP; Fornjot 0/5. Ядро за трейтом `Kernel` (`crates/kernel`) + `crates/occt` (своя обвязка).
- `cargo test -p formcraft-occt`: 16 + `tests/step.rs` 3 — passed.

## Приёмка Фазы 1
- Окно открывается: скриншоты `docs/images/phase1-dark.png` (вся раскладка п. 2), `docs/images/phase1-light-floating-top.png` (светлая тема, browser плавающий, вид сверху, Text Commands раскрыта). Сравнение с п. 2 PLAN: app bar ✓, toolbar с вкладками Solid/Sketch/Inspect/Tools и группами с выпадающими списками ✓, browser (Document Settings, Named Views, Origin с глазами, Sketches, Bodies) ✓, canvas с сеткой ✓, ViewCube ✓, navigation bar ✓, timeline внизу ✓, Text Commands свёрнута ✓.
- Панели двигаются/сбрасываются, орбита/пан/зум, ViewCube: `cargo test -p formcraft-ui-egui` — тесты `shift_middle_drag_orbits`, `middle_drag_pans`, `wheel_zooms_in_and_out`, `view_cube_top_face_sets_top_view`, `browser_float_close_and_reset`, `text_command_line_runs_commands`, `both_themes_render` — ok.
- GitHub Actions: `.github/workflows/ci.yml` (Windows + Linux) написан, но не запускался — нет удалённого репозитория (см. «Требует человека»).

## Приёмка Фазы 2
- `formcraft-cli run examples/bracket.fcs --save bracket.step` → `apps/formcraft-cli/tests/acceptance.rs`: run_and_save_step/stl, step_volume, step_bbox, step_one_valid_solid — ok.
- FreeCAD 1.1.4: `FCCHECK {"file": "bracket.step", "ok": true, "solids": 1, "faces": 8, "volume": 11657.881, "bbox": [0.0, 0.0, 0.0, 60.0, 40.0, 5.0]}`, точное значение 11657.8806 — совпадает.
- Рендер с рёбрами, выбор и подсветка: `docs/images/phase2-bracket-selection.png` (выделена грань), тест `clicking_the_canvas_picks_faces`, `crates/render/src/gpu_tests.rs` (офскрин-рендер на локальном GPU).
- Тела в Browser (глаз = `body.visibility`), примитивы и булевы — команды `solid.box/cylinder/sphere/combine`, экспорт STEP/STL — `doc.export` и File → Export.

## Приёмка Фазы 3
- «Собрать кронштейн мышью, изменить параметр → корректное перестроение»: `cargo test -p formcraft-ui-egui acceptance_bracket_with_the_mouse` — ok (S/R/D/E/F, клики по canvas, размер `L` в поле на холсте, диалоги Extrude/Fillet, `L` 60→80 → объём (L·30 − (4−π)·9)·5 точно).
- Golden-тесты на 10 эталонных деталей: `cargo test -p formcraft-engine --test golden` — 10 passed (plate, bushing, flange, l_bracket, washer, hex_nut, shaft_keyway, enclosure, pulley, rounded_plate; объёмы аналитически).
- Фичи: `crates/regen/src/tests3.rs` 12 golden-тестов (эскиз+выдавливание, симметрия, through all, revolve, fillet при смене параметров, chamfer, shell, цековка, зенковка, press pull, эскиз на грани, сигнатура грани).
- UI: скриншоты `docs/images/phase3-*.png` (режим эскиза, диалог+таймлайн, marking menu, манипулятор). Сравнение с п. 2: feature dialog справа ✓, манипуляторы ✓, marking menu ✓, S-box ✓, sketch palette ✓, таймлайн с иконками/маркером/ошибками ✓.
- `.fcraft`: тест `save_and_open_round_trip`.

## Журнал
- 2026-10-14: репозиторий, PLAN, PROGRESS; Rust; изучен `../cadcraft-ref`.
- 2026-10-15: spike ядер → ADR 0001; своя обвязка OCCT; реестр команд; UI-оболочка; ADR 0002. Тег phase-0.
- 2026-10-15: doc/regen/engine (undo, атомарные правки, команды), io (STL, STEP), CLI, GPU-вьюпорт с выбором. 20 иконок от Sonnet. Теги phase-1, phase-2.
- 2026-10-15/16: фаза 3: UI (диалог из JSON-схемы с живым превью, таймлайн, режим эскиза, marking menu, S-box, манипуляторы), .fcraft, приёмка, golden.
- 2026-10-15: sketch (решатель, профили), фичи фазы 3 с селекторами. Найдено: шум решателя 1e-12 ломал скругления OCCT → снап координат к 1e-9. Числовые углы/центры в `sketch.create` закрепляются, иначе прямоугольник растёт в обе стороны при смене параметра.

## Требует человека
- **GitHub:** создать репозиторий и запушить (`git remote add origin …; git push -u origin main --tags`), чтобы заработал CI в `.github/workflows/ci.yml`. Публиковать без разрешения не стал.
- Прокси/VPN в fake-IP режиме ломает `ollama pull` (CDN → 198.18.x). Модели качаются вручную с Hugging Face (`docs/environment.md`). Чтобы `ollama pull` заработал: исключить `registry.ollama.ai` и `*.r2.cloudflarestorage.com` из fake-IP.
- Ollama 0.40.4 не находит RTX 4050 (`initial_count=0`), инференс на CPU (~14 с на короткий ответ 7B). Проверить: настройки графики Windows → «Высокая производительность» для `ollama.exe`, либо обновить Ollama. Драйверы не трогали.
- qwen3-8b GGUF: докачка скриптом `C:/fc-models/dl.sh` (запуск: `bash /c/fc-models/dl.sh`), затем `ollama create qwen3:8b -f C:/fc-models/qwen3-8b.Modelfile`.

## Тупики / известные проблемы
- libtest + STEP: тестовый бинарник после STEP-обмена падает с access violation при выходе процесса (тесты уже прошли). Обычные программы выходят чисто. Причину не нашли (кандидат — порядок статических деструкторов OCCT). Обход: STEP-тесты с `harness = false`.
- bat-файлы с кириллическим путём и `chcp 65001` пропускают строки — не использовать.
- Рёбра рисуются линиями толщиной 1 px (LineList); выделенное ребро видно слабо. Улучшить в фазе 3 (экранные квады для толстых линий).

## Делегировано Sonnet
- `docs/learn/phase-0.md` — проверено, одна строка исправлена.
- 20 иконок тулбара в `crates/ui-egui/src/icons.rs` — проверено: clippy/CI зелёные, на скриншоте выглядят корректно.
- `docs/learn/phase-1.md`, `docs/learn/phase-2.md` — проверены (структура, ключевые факты), без правок.
- 10 golden-деталей (`examples/golden`, `crates/engine/tests/golden.rs`) — проверено: все зелёные, выборочно пересчитан фланец.
- `docs/shortcuts.md`, `docs/learn/phase-3.md` — проверены, закоммичены.
