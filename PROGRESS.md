# FormCraft — PROGRESS

> Новая сессия: «Прочитай PLAN.md и PROGRESS.md, продолжай цикл». Этот файл + `git log` — всё состояние.
> Сессии обрываются на долгих командах: всё дольше ~1 мин запускать в фоне, PROGRESS.md обновлять после каждого шага.

## Сейчас
- **Фаза:** 0 — Окружение и выбор ядра
- **Текущий шаг:** spike OCCT (сборка), затем сверка STEP во FreeCAD и ADR 0001
- **Дальше:** Фаза 1 (каркас workspace)

## Фазы
| Фаза | Статус | Тег |
|---|---|---|
| 0 Окружение и ядро | в работе | — |
| 1 Каркас и оболочка UI | — | — |
| 2 Тело на экране | — | — |
| 3 Эскиз, фичи, таймлайн | — | — |
| 4 Команды, CLI, MCP | — | — |
| 5 Встроенный ИИ | — | — |
| 6 Плагины и скрипты | — | — |
| 7 Шлифовка и релиз | — | — |

## Фаза 0 — заметки
- Окружение: Rust 1.99 (rustup, winget), git 2.53, gh 2.101, cmake 4.4.3, VS 2026 Community + VS 2022 BuildTools (VC tools уже были), Ollama 0.40.4, FreeCAD 1.1, Python 3.14.2. GPU: RTX 4050 Laptop 6 ГБ, драйвер 596.49 → модели <10 ГБ: qwen2.5-coder:7b, qwen3:8b.
- `ollama pull` не работает: прокси/VPN в fake-IP режиме (CDN резолвится в 198.18.x, Ollama отказывается). Обход: GGUF с Hugging Face (`curl -I` → Location → curl напрямую, `-C -` докачка) в `C:\fc-models`, затем `ollama create` из Modelfile.
- Сборка из пути с кириллицей рискованна для MSVC/CMake → `CARGO_TARGET_DIR=C:/fc-target/<name>`.
- Spikes: `spikes/kernel-{truck,occt,fornjot}`, общий harness `spikes/common/expect.rs` (объём по мешу, bbox, аналитические эталоны).
  - truck 0.6/shapeops 0.4: 4/5 (box_hole, cyl_cut, revolve PASS; shell только эмуляцией булевой; fillet_all FAIL — API нет, в master только простое скругление одного ребра без углов). Revolve даёт обратную ориентацию (объём < 0).
  - Fornjot 0.49: нет булевых, скруглений, shell, STEP-экспорта (fj-export: 3MF/STL/OBJ) → не проходит по API.
  - OCCT (opencascade 0.3 + occt-sys 7.8.1): нужны `CMAKE_POLICY_VERSION_MINIMUM=3.5` (CMake 4) и патч `Standard_Handle.hxx` (на MSVC `Handle_X` — класс, а мост cxx ждёт typedef; патч `#if 0` в исходнике occt-sys в реестре cargo). Для продукта: вендорить occt-sys через `[patch]`.

## Журнал
- 2026-10-14: репозиторий, PLAN.md, PROGRESS.md; Rust; клон `../cadcraft-ref` изучен (CommandSpec-реестр, never-crash lints, xtask ci + layers, MCP stdio JSON-RPC).
- 2026-10-15: truck/Fornjot оценены, OCCT собирается.

## Требует человека
- Прокси/VPN в fake-IP режиме ломает `ollama pull`. Модели качаются вручную (см. выше). Если нужно `ollama pull`: исключить `*.r2.cloudflarestorage.com` и `registry.ollama.ai` из fake-IP.

## Тупики
(пусто)

## Делегировано Sonnet
(пусто)
