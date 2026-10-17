# FormCraft — PROGRESS

> Новая сессия: «Прочитай PLAN.md и PROGRESS.md, продолжай цикл». Этот файл + `git log` — всё состояние.
> Сессии иногда обрываются: PROGRESS.md обновлять после каждого шага, сборки > 10 мин — фоном.

## Сейчас
- **Все фазы 0–7 закрыты** (теги phase-0 … phase-7). Итоговый отчёт — ниже. Дальше — Фаза 8 (опционально, после релиза) и пункты «Требует человека».
- **Новая сессия:** «Прочитай PLAN.md, UI_SPEC.md и PROGRESS.md, продолжай цикл» → берите `docs/ideas.md` и «Требует человека».
- **MCP к Claude Code:** `.mcp.json` → `target/mcp/formcraft-cli.exe mcp` (`cargo xtask mcp-install`).

## Итоговый отчёт (2026-10-17)

### Что работает
- **Моделирование:** эскизы (решатель ограничений, профили с отверстиями, эскиз на грани), выдавливание/вращение/отверстия (цековка, зенковка)/скругления/фаски/оболочка/Press Pull/булевы, параметры и выражения, таймлайн (маркер, подавление, перестановка, группы), undo/redo, селекторы граней и рёбер, переживающие перестройку. Golden-тесты на 10 деталей, объёмы аналитически.
- **Файлы:** `.fcraft`, STEP (экспорт; импорт как базового тела), STL, 3MF; FreeCAD открывает наш STEP.
- **Интерфейс по UI_SPEC.md:** раскладка и вид Fusion (app bar, toolbar, плавающий Browser, ViewCube, nav bar, таймлайн с группами, Feature dialog, marking menu-кольцо, S-box, палитра эскиза, тосты), светлая/тёмная темы, Inter, свои двухцветные иконки, RU/EN, настройки, журнал. Скриншоты всех сцен §8 + Add-Ins, сечение, настройки — `docs/images/ui/`.
- **Inspect:** измерение, анализ сечения (точная площадь + вид), свойства и масса по материалу.
- **ИИ:** панель Assistant, превью, Принять/Отклонить одним шагом undo с группой, все точки входа; Ollama/OpenAI-совместимый/Mock; промпт и схема из реестра; цикл исправления; спасение обрезанных ответов; логи.
- **Команды/CLI/MCP:** всё — команды с JSON-схемами (≈60 встроенных + команды плагинов), `formcraft-cli` (run, commands, info, convert, render, screenshot, ai, ai-bench, plugin, mcp), MCP-сервер (headless и к живому окну через control channel), подключён к Claude Code.
- **Плагины и скрипты:** WebAssembly-компоненты на wasmtime (песочница, лимиты, выгрузка при сбое), custom features с замороженным телом, установка из zip, диалог Scripts and Add-Ins, шаблоны `plugin new`, Python-клиент. Примеры: «Шестерня», «Масса по таблице материалов», пакетный экспорт STL.
- **Сборка:** `cargo xtask dist` → `FormCraft-0.1.0-windows-x64.zip` (48 МБ, проверен из чистой папки: CLI, приложение, встроенные add-ins) и `FormCraft-0.1.0-x64.msi` (38 МБ, WiX 5, per-user, структура проверена административной распаковкой); AppImage — скрипт + `release.yml` (на этой Windows-машине не собирался).

### Что не работает / не проверено
- **GPU для ИИ:** Ollama 0.40.4 не видит RTX 4050 → всё на CPU: 1–3 мин на деталь. Требование «< 60 с на 12 ГБ VRAM» здесь не проверяемо.
- **Совпадение с эталоном в ai-bench — 44 %:** модель часто строит валидное, но не то тело (пропущенные отверстия, высота).
- **qwen3:8b непригоден на CPU** (0 % под грамматикой, > 10 мин без неё) — ADR 0005.
- **MSI не устанавливался** на эту систему (только распаковка); AppImage не собирался (нужен Linux + linuxdeploy). GitHub Actions: первый запуск CI — после пуша 2026-10-17 (результат см. во вкладке Actions).
- Отложено (`docs/ideas.md`): панели и события Plugin API 1.1, бейджи ограничений эскиза, толстые рёбра выбора, окно выбора рамкой, анимации камеры, жест marking menu, Slice, минимальное расстояние между произвольными гранями.

### ai-bench (50 кейсов, CPU, `docs/bench/`)
| Модель | valid | rebuilt | matched | simple valid | с/кейс |
|---|---|---|---|---|---|
| qwen2.5-coder:7b (первый прогон) | 68 % | 68 % | 42 % | 63 % | 161 |
| **qwen2.5-coder:7b** (итог, по умолчанию) | **82 %** | **82 %** | **44 %** | **80 %** | **111** |
| qwen3:8b, JSON-схема | 0 % | 0 % | 0 % | 0 % | 236 |
| qwen3:8b, свободный текст | таймаут > 600 с на ответ | | | | |

Приёмка на реальной модели: фланец Ø100/4×Ø9 на Ø80/10 — точный объём за 181 с; «шестерня z=20 m=2» — вызов add-in за 86 с.

### Тупики
См. раздел «Тупики / известные проблемы» ниже (STEP + libtest, кириллица в путях, гамма вьюпорта, параллельные сборки и инференс, qwen3 и грамматика, блокировка бинарника MCP-сервером).

### ADR
0001 ядро (OpenCascade за трейтом), 0002 лицензии зависимостей, 0003 Plugin API v1 (+ раздел «Реализация»), 0004 внешний вид по UI_SPEC.md, 0005 модель ассистента по умолчанию.

### Что сделал Sonnet
См. «Делегировано Sonnet» ниже: learn-доки фаз 0–7, иконки (дважды), golden-детали, shortcuts, 25 few-shot, 48 кейсов бенча, add-in «Масса», `docs/plugins/`, локализация. Каждый результат проверен CI, тестами и/или скриншотами.

### Итоги по объёму
49+ коммитов за 2026-10-14 … 17, ≈190 тестов (`#[test]`) + 10 проверок приёмки CLI + 3 STEP-теста; `cargo xtask ci` зелёный.

## Приёмка Фазы 7
- Inspect: `inspect_measures_properties_and_sections` (масса алюминия, зазор 20 мм между параллельными гранями, Ø10 отверстия, площади сечения 5000−25π и 1000 мм²); вид сечения — `docs/images/ui/10-section-light.png`.
- Импорт STEP как базового тела + 3MF: `formcraft-cli --test acceptance` → `step_import_is_a_base_body` (импорт → фаска → объём), `export_3mf`.
- Настройки, логи, RU/EN: `docs/images/ui/11-settings-ru-dark.png`, `3-extrude-ru.png`; журнал `…/FormCraft/logs/formcraft.log`.
- Сборки: portable zip и MSI собраны и проверены (см. отчёт); AppImage — скрипт и CI-job. README: сборка, запуск, Ollama и модель, add-ins, MCP.

## Приёмка UI-выравнивания (UI_SPEC.md §8)
Снимки: `formcraft-cli screenshot all <dir>` (1920×1080, обе темы) → копии 1280×720 в `docs/images/ui/` (`<сцена>-light|dark.png`); тест `every_spec_scene_renders` рендерит все 8 сцен в CI. До/после: `docs/images/ui/before/`.

| Сцена §8 | Статус |
|---|---|
| 1 Пустой документ | ✓ все зоны, градиент, сетка |
| 2 Тело выбрано | ✓ подсветка accent 45%, Browser раскрыт, тултип «Face · Body1», рамка фичи на таймлайне |
| 3 Выдавливание | ✓ Feature dialog §5.8, превью Join синим, манипулятор (под диалогом при этом ракурсе) |
| 4 Режим эскиза | ✓ палитра §5.13, FINISH SKETCH, заливка профиля, размеры; цвета определённости — частично (недоопределённые синие) |
| 5 Marking menu | ✓ кольцо 8 секторов, центр «повторить», список |
| 6 Shortcut box | ✓ 360px, избранное 6×2, фиолетовая строка ИИ |
| 7 ИИ-группа + Assistant | ✓ группа-папка ✦ на таймлайне; панель Assistant — в фазе 5 |
| 8 Ошибка + тост | ✓ красная иконка с подложкой, тост 4 с со «Show» |

Чек-лист: канвас ≥ 80% окна ✓ (Browser, ViewCube, nav bar, диалоги — плавающие карточки поверх); панели не закрывают центр ✓; toolbar 32px с подписями и группами капсом ▾ ✓; таймлайн внизу, ViewCube справа сверху, nav bar по центру снизу ✓; цвета только из токенов ✓ (литералы — `WHITE` на кнопках OK/FINISH и галочке, `TRANSPARENT`); иконок/логотипов/текстов Autodesk нет ✓.

Осталось (не блокирует, в `docs/ideas.md`-уровне до фазы 7): бейджи ограничений 14px и цвета полной определённости эскиза; рёбра выбора 2.5px (нужны экранные квады в рендере); окно выбора рамкой; анимации камеры/меню (§6); жест marking menu; схлопывание групп toolbar на узком окне; локализация подписей (ВЕРХ/ПЕРЕД… — фаза 7); табличные цифры (не поддерживает egui).

## UI: расхождения со спецификацией (UI_SPEC.md) — исходный список
Снято 2026-10-16 `formcraft-cli screenshot all <dir>` (1920×1080, обе темы), копии светлой темы — `docs/images/ui/before/` (960×540). Фазы 1–3 закрыты; правки только внешнего вида и раскладки — задача «UI-выравнивание» (шаги ниже). Будущие зоны (Assistant, Scripts and Add-Ins, Inspect, тосты) делаются сразу по спецификации в своих фазах.

| Зона (§) | Есть | Не так | Нет |
|---|---|---|---|
| Токены (§2–3, §7) | `theme.rs`, почти все цвета в нём | имена и значения не из спецификации; тёмная тема по умолчанию (нужна светлая); размеры/отступы — магические числа в виджетах; 3 цвета вне токенов (sketch_mode, viewport рёбра, viewcube) | токены размеров (4px-сетка, скругления 4/6/3, шрифты 10/11/12), состояния иконок |
| Шрифт (§3) | шрифт egui по умолчанию | — | Inter (OFL), табличные цифры |
| Application bar (§5.1) | имя, File/View/Help, undo/redo | светлая в светлой теме (нужна тёмная `#2B2F33`, 36px); меню текстом вместо «гамбургера» | вкладки документов (точка «не сохранено», ✕, +), сохранить, поиск, ✦ИИ, настройки, справка справа |
| Toolbar (§5.2) | DESIGN ▾, вкладки, группы с подписями ▾ | иконки ~20px без подписей (нужны 32px с подписью), высота ~70 без разделения 28+68, подчёркивание вкладки | SELECT-группа, схлопывание групп на узком окне, расширенные тултипы |
| Browser (§5.3) | дерево, глаза, Origin/Sketches/Bodies | закреплённая панель на всю высоту, отрезает канвас; нет тени/карточки | плавающая карточка 240px с отступом 8px и тенью, сворачивание по шапке, «Параметры ИИ-групп» |
| Canvas (§5.4) | градиент, сетка, MSAA | тело тёмно-серое (нужно `#B8BEC4`), выбор — насыщенная синяя заливка (нужно accent 45% + ребро 2.5px); градиент светлой темы почти плоский | тултип «Грань/Ребро/Тело» у курсора, окно выбора (рамка), анимация камеры 300мс |
| ViewCube (§5.5) | куб, грани, Home | ~140px (нужно 100), без полупрозрачности в покое; в виде «сверху» рисуется плоским квадратом | подсветка рёбер/углов accent, стрелки поворота, компас-кольцо |
| Navigation bar (§5.6) | карточка по центру снизу, иконки | высота/иконки меньше спецификации | выпадающие Orbit ▾, Fit ▾, Display Settings ▾ (стили, перспектива), Grid ▾, Viewports |
| Timeline (§5.7) | кнопки ⏮◀▶⏭, иконки фич, маркер, красная рамка ошибки, фиолетовая полоса группы | иконки ~20px (нужно 24), нет подложки у ошибки, группа — полоса вместо папки | группа-папка с ✦, раскрытие скобкой, жёлтое предупреждение, шестерёнка справа, красная подсветка недопустимого места при перетаскивании |
| Feature dialog (§5.8) | карточка справа, поля из схемы, OK/Cancel, превью, Enter/Esc | заголовок по центру, нет иконки и «?»; подписи без колонки 110px; ширина ~340; значения enum сырые (`one_side`, `new_body`); поле без единиц; в сцене 3 пустая дистанция — превью нет | селекшен-поле «Выбрать / 1 выбрано / ✕», иконки операций с цветами превью, подгруппы ▸, подсказка серым внизу |
| Манипуляторы (§5.9) | стрелка, кольцо, поле значения на канвасе | размеры/цвета не из токенов | утолщение при наведении, сектор угла |
| Marking menu (§5.10) | 8 пунктов вокруг курсора + список ниже | пункты — «таблетки» без кругового меню, центр — точка | секторное кольцо, центр «повторить», контекст для грани (✦ Изменить с ИИ…), жест |
| Shortcut box (§5.11) | поле поиска, строка ИИ | ширина ~310 (нужно 360); строка «✦ Сделать с ИИ» нечитаема (синий текст на синем выделении) и не фиолетовая | сетка избранного 6×2, путь «Solid › Create», клавиша справа, булавка |
| Sketch palette (§5.13) | карточка справа, инструменты, ограничения, Finish Sketch | ограничения — серый список текстом (нужна сетка значков 6×2), профиль залит серо-синим (нужно `#FFD580` 30%), цвета геометрии не по §2.4 | Options (Look At, Sketch Grid, Snap, Show Constraints/Dimensions, Slice), бейджи ограничений 14px у геометрии |
| Sketch toolbar (§5.2) | вкладка SKETCH, зелёная галочка | Finish Sketch — маленькая иконка в группе | большая кнопка FINISH SKETCH справа, цветная подсветка контекстной вкладки |
| Text commands (§5.14) | свёрнутая полоса, консоль | «▸ TEXT COMMANDS» вместо «▴», консоль не моноширинная | — |
| Иконки (§4) | 20+ своих иконок в коде | одноцветные, не двухцветные; размеры 20px | `docs/icons.md`, состояния normal/hover/pressed/disabled |
| Анимации (§6) | — | — | hover 80мс, fade 120мс, тосты ошибок regen (4с, «Показать») |
| Доступность (§7) | — | — | подписи для скринридера, видимый фокус |

### Задача «UI-выравнивание» (до продолжения фазы 5)
1. Токены по §2–3 (цвета + размеры + шрифт Inter, светлая тема по умолчанию), все виджеты на токенах; ADR «Внешний вид по UI_SPEC.md».
2. `docs/icons.md` (ТЗ на иконки по §4) → Sonnet рисует двухцветные иконки 16/20/24/32px; ревью и интеграция.
3. Каркас: тёмный app bar 36px с вкладками документа и правыми кнопками; toolbar 28+68 с кнопками 32px и подписями; канвас на всё окно, Browser — плавающая карточка 240px.
4. Канвас и 3D-цвета по §2.3 (тело, выбор, наведение, превью Join/Cut/New), тултип у курсора; ViewCube 100px полупрозрачный; navigation bar 36px.
5. Timeline 40px: иконки 24px, ошибки с подложкой, группа-папка ✦ со скобкой, шестерёнка.
6. Feature dialog 300px по §5.8; marking menu с кольцом; shortcut box 360px с избранным и фиолетовой строкой ИИ; sketch palette по §5.13 и FINISH SKETCH; text commands ▴ моноширинная.
7. Скриншот-тесты всех сцен §8 в обеих темах + чек-лист §8 → `docs/images/ui/`.

## Как собирать и проверять
- `cargo xtask ci` — fmt, clippy -D warnings, тесты, слои, cargo deny. Обычный `target/` (первая сборка OCCT 30–60 мин).
- Тесты: 81 обычных + `crates/occt/tests/step.rs` (3) + `apps/formcraft-cli/tests/acceptance.rs` (6), последние два — `harness = false`.
- Скриншоты UI без окна: `target/debug/formcraft-cli.exe screenshot all C:/fc-target/ui [--theme light|dark|both] [--size 1920x1080]` (сцены UI_SPEC §8, `crates/ui-egui/src/screens.rs`); тест `every_spec_scene_renders`.
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
| 4 Команды, CLI, MCP | готово | phase-4 |
| 5 Встроенный ИИ | готово | phase-5 |
| 6 Плагины и скрипты | готово | phase-6 |
| 7 Шлифовка и релиз | готово | phase-7 |

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

## Приёмка Фазы 4
- MCP по stdio, headless: `cargo test -p formcraft-mcp` — 5 passed (жизненный цикл, кронштейн через run_script + inspect_model с точным объёмом, render → PNG, ошибки протокола).
- Живое приложение: `formcraft --control 7878` + `formcraft-cli mcp --port 7878` ← JSON-RPC из файла: run_script (кронштейн) → inspect_model (bbox 60×40×5) → ui_click {world 30,30,5} → `"hit": true, "Selected: face 4 of Body1"` → screenshot (1440×900, `docs/images/phase4-mcp-remote.png`) → render (640×480). Тест `control_channel_drives_the_app` — то же по TCP в CI.
- «Claude Code через MCP строит кронштейн и проверяет через inspect_model и render»: выполнено (JSON-RPC в `formcraft-cli mcp` и `mcp --port` к живому окну, ответы выше).
- ADR 0003 «Plugin API v1» + `crates/plugin-api/wit/formcraft.wit` (дизайн, реализация в фазе 6).
- `.mcp.json` в корне: `target/debug/formcraft-cli.exe mcp` (новая сессия Claude Code подхватит после одобрения).

## Приёмка Фазы 5
- «Фланец Ø100, 4 отверстия Ø9 на Ø80, толщина 10» → корректная группа фич: реальная модель qwen2.5-coder:7b (Ollama, **CPU**): `formcraft-cli ai "фланец Ø100, 4 отверстия Ø9 на Ø80, толщина 10"` → Sketch1 + Extrude1 + Hole1 (4 точки на окружности Ø80), параметры flange_d/flange_t/bolt_circle_d/hole_d; объём 75995.1263 = π·50²·10 − 4·π·4.5²·10 точно, 5 цилиндрических граней; `docs/images/phase5-flange-ai.png`. Время **181 с на CPU** — требование «< 60 с» относится к GPU 12 ГБ; Ollama не видит RTX 4050 («Требует человека»), поэтому на этой машине не проверяемо.
- ai-bench ≥ 70 % валидных на простых: **80 %** (qwen2.5-coder:7b, все 50 кейсов: valid 82 %, rebuilt 82 %, matched 44 %, 111 с/кейс на CPU). Выбор модели — ADR 0005 (qwen3:8b: 0 % под грамматикой, > 10 мин на ответ без неё на CPU). Отчёты — `docs/bench/`.
- Mock в CI: `cargo test -p formcraft-ai` (17 тестов: цикл исправления, отказ после 3 попыток, группа и один шаг undo, промпт из реестра, fake-Ollama по HTTP, спасение обрезанного ответа, бенч на «оракуле»), `cargo test -p formcraft-ui-egui assistant` (фон → превью → Принять/Отклонить).
- Точки входа: S-box «Сделать с ИИ», Solid › Create › Generate, marking menu «Изменить с ИИ…» (с выбором), ✦ в app bar, «Объяснить» на таймлайне; панель Assistant по UI_SPEC §5.12 (`docs/images/ui/7-ai-group-*.png`). Логи `…/FormCraft/logs/ai/*.jsonl`.

## Приёмка Фазы 6
- «Шестерня» устанавливается из zip: `install_from_zip_checks_paths`; кнопка Spur Gear в Solid › Create и диалог из схемы плагина: `gear_addin_button_dialog_and_timeline`, `docs/images/ui/9-addins-dark.png`; фича на таймлайне перестраивается при смене параметра: `gear_plugin_adds_a_parametric_feature` (z 20 → 30, правка через feature.edit, выгрузка → замороженное тело с предупреждением).
- ИИ по запросу «шестерня z=20 m=2» вызывает команду плагина — **реальная модель**: `formcraft-cli --plugin examples/plugins/gear ai "шестерня z=20 m=2"` → одна фича `gear.spur` (Custom, ok), Ø вершин 44 = m·(z+2), 86 с на CPU; плюс тест на Mock `assistant_uses_the_gear_addin`.
- Python-скрипт экспортирует все тела в STL: `examples/scripts/export_stl.py` против `formcraft --control` — два тела → Body1.stl, Body2.stl (прогон 2026-10-16).
- Плагин с бесконечным циклом выгружается по таймауту без падения: `misbehaving_plugins_are_unloaded_not_fatal` (2 с, сессия цела, команды сняты; также 2 ГБ памяти и panic).
- Шаблоны `formcraft-cli plugin new --lang rust|python-script`, `docs/plugins/README.md`, три примера (gear, material, export_stl.py), ADR 0003 с отличиями реализации.

## Журнал
- 2026-10-14: репозиторий, PLAN, PROGRESS; Rust; изучен `../cadcraft-ref`.
- 2026-10-15: spike ядер → ADR 0001; своя обвязка OCCT; реестр команд; UI-оболочка; ADR 0002. Тег phase-0.
- 2026-10-15: doc/regen/engine (undo, атомарные правки, команды), io (STL, STEP), CLI, GPU-вьюпорт с выбором. 20 иконок от Sonnet. Теги phase-1, phase-2.
- 2026-10-15/16: фаза 3: UI (диалог из JSON-схемы с живым превью, таймлайн, режим эскиза, marking menu, S-box, манипуляторы), .fcraft, приёмка, golden.
- 2026-10-15: sketch (решатель, профили), фичи фазы 3 с селекторами. Найдено: шум решателя 1e-12 ломал скругления OCCT → снап координат к 1e-9. Числовые углы/центры в `sketch.create` закрепляются, иначе прямоугольник растёт в обе стороны при смене параметра.

- 2026-10-16: UI_SPEC.md: скриншот-конвейер без окна, токены и Inter (ADR 0004), каркас Fusion (app bar, toolbar, плавающий Browser, таймлайн-группы), иконки (Sonnet), диалог, marking menu, S-box, палитра эскиза, тосты; исправлена гамма вьюпорта (всё 3D было слишком тёмным).
- 2026-10-16: фаза 4: headless-рендер, CLI convert/render/mcp, MCP-сервер, control channel приложения, ADR 0003. Тег phase-4.

## Требует человека
- **WiX 7** требует принять OSMF EULA (`wix eula accept wix7`); сборка MSI переведена на WiX 5.0.2 (`dotnet tool install --global wix --version 5.0.2`), так что это не обязательно. Лицензию за вас не принимал.
- **Установка MSI** на эту машину не выполнялась (меняет профиль пользователя) — проверьте вручную: `msiexec /i dist\FormCraft-0.1.0-x64.msi`.
- **GitHub:** приватный репозиторий https://github.com/xSUSASos/formcraft создан и запушен 2026-10-17 (main + теги phase-0…phase-7); CI в `.github/workflows/ci.yml` запускается на push, `release.yml` — на теги `v*`/`phase-7`.
- Прокси/VPN в fake-IP режиме ломает `ollama pull` (CDN → 198.18.x). Модели качаются вручную с Hugging Face (`docs/environment.md`). Чтобы `ollama pull` заработал: исключить `registry.ollama.ai` и `*.r2.cloudflarestorage.com` из fake-IP.
- Ollama 0.40.4 не находит RTX 4050 (`initial_count=0`), инференс на CPU (~14 с на короткий ответ 7B). Проверить: настройки графики Windows → «Высокая производительность» для `ollama.exe`, либо обновить Ollama. Драйверы не трогали.
- qwen3-8b GGUF: докачка скриптом `C:/fc-models/dl.sh` (запуск: `bash /c/fc-models/dl.sh`), затем `ollama create qwen3:8b -f C:/fc-models/qwen3-8b.Modelfile`.

## Тупики / известные проблемы
- libtest + STEP: тестовый бинарник после STEP-обмена падает с access violation при выходе процесса (тесты уже прошли). Обычные программы выходят чисто. Причину не нашли (кандидат — порядок статических деструкторов OCCT). Обход: STEP-тесты с `harness = false`.
- bat-файлы с кириллическим путём и `chcp 65001` пропускают строки — не использовать.
- Рёбра рисуются линиями толщиной 1 px (LineList); выделенное ребро видно слабо. Улучшить в фазе 3 (экранные квады для толстых линий).
- egui + sRGB-текстура: вьюпорт отдавал egui вид Rgba8UnormSrgb → двойная линеаризация, всё 3D темнее в ~3 раза. Решение: отдельный вид Rgba8Unorm той же текстуры для egui.
- Тяжёлая сборка (wasmtime, OCCT) параллельно с CPU-инференсом Ollama (15 ГБ ОЗУ) роняла раннер модели и искажала замеры — бенч гонять без параллельных сборок; `CARGO_BUILD_JOBS=2..3` во время инференса.
- qwen3:8b + грамматика JSON-схемы (Ollama `format`) → вырождение (табы, пустые `params`); без грамматики на CPU > 10 мин на ответ. Отложено до GPU (ADR 0005).
- Запущенный MCP-сервер (`.mcp.json`) держит `target/debug/formcraft-cli.exe` — сборка падала «Отказано в доступе». Решение: сервер запускается из копии `target/mcp/` (`cargo xtask mcp-install`), бенч — из `target/bench/`.
- Python-heredoc'и в bash с `
`/`\` портили экранирование — правки больших фрагментов делать файлами-скриптами с raw-строками.

## Делегировано Sonnet
- `docs/learn/phase-0.md` — проверено, одна строка исправлена.
- Иконки по `docs/icons.md` (двухцветные, состояния, ~70 id, тесты `every_icon_paints`, `accents_by_category`) — проверено: CI зелёный, просмотр снимков всех сцен; добавлен `plus`.
- 20 иконок тулбара в `crates/ui-egui/src/icons.rs` — проверено: clippy/CI зелёные, на скриншоте выглядят корректно.
- `docs/learn/phase-1.md`, `docs/learn/phase-2.md` — проверены (структура, ключевые факты), без правок.
- 10 golden-деталей (`examples/golden`, `crates/engine/tests/golden.rs`) — проверено: все зелёные, выборочно пересчитан фланец.
- `docs/shortcuts.md`, `docs/learn/phase-3.md` — проверены, закоммичены.
- `docs/learn/phase-4.md` — проверено: флаги и команды сверены с кодом, примеры из «Как проверить» прогнаны.
- 25 few-shot примеров ИИ (`crates/ai/fewshot/`) — проверено тестом `every_fewshot_example_builds_a_valid_body`, выборочно пересчитана гайка (Rₐ = 13/√3).
- 48 кейсов ai-bench (`crates/ai/bench/cases.json`) с эталонными планами и аналитическими объёмами — проверено тестом `bench_references_match_their_expectations`.
- `docs/learn/phase-5.md`, `phase-6.md`, `phase-7.md` — сверены с кодом, команды из «Как проверить» выборочно прогнаны.
- Add-in «Масса по таблице материалов» (`examples/plugins/material`) и `docs/plugins/README.md` — проверено тестом `material_addin_computes_mass` (270 г на 100 см³ алюминия).
- Локализация RU/EN (≈170 строк, таблица ≈350 записей) — проверено: CI зелёный, скриншоты на русском просмотрены.
