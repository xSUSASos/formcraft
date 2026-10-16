# Окружение разработки

Записано в Фазе 0 (2026-10-15). Машина разработчика:

| Компонент | Версия | Как ставили |
|---|---|---|
| ОС | Windows 11 Pro 10.0.26200 | — |
| GPU | NVIDIA GeForce RTX 4050 Laptop, 6 ГБ VRAM, драйвер 596.49 | не трогали |
| rustc / cargo | 1.99.0 (stable-x86_64-pc-windows-msvc) | `winget install Rustlang.Rustup`, `rustup default stable` |
| MSVC | VS 2026 Community 18.10 (MSVC 14.51) + VS 2022 Build Tools 17.14 | уже стояли |
| CMake | 4.4.3 | уже стоял |
| Git / gh | 2.53.0 / 2.101.0 | уже стояли |
| Ollama | 0.40.4 | `winget install Ollama.Ollama` |
| FreeCAD | 1.1.4 (`%LOCALAPPDATA%\Programs\FreeCAD 1.1\bin\freecadcmd.exe`) | `winget install FreeCAD.FreeCAD` |
| Python | 3.14.2 | уже стоял (≥ 3.11 нужен только в Фазе 6) |
| cargo-deny | 0.20.2 | `cargo install cargo-deny --locked` |
| cargo-nextest | см. ниже | `cargo install cargo-nextest --locked` |

## Локальные модели (6 ГБ VRAM → класс < 10 ГБ)

Кандидаты для `ai-bench`: `qwen2.5-coder:7b`, `qwen3:8b` (Q4_K_M).

`ollama pull` на этой машине не работает: сетевой прокси работает в режиме fake-IP, CDN реестра
резолвится в 198.18.0.0/15, и Ollama отказывается следовать редиректу на «непубличный» адрес.
Обход — GGUF с Hugging Face и `ollama create`:

```bash
L=$(curl -sI https://huggingface.co/Qwen/Qwen2.5-Coder-7B-Instruct-GGUF/resolve/main/qwen2.5-coder-7b-instruct-q4_k_m.gguf | grep -i ^location | cut -d' ' -f2 | tr -d '\r')
curl -C - -o C:/fc-models/qwen2.5-coder-7b-q4_k_m.gguf "$L"
ollama create qwen2.5-coder:7b -f models/qwen2.5-coder-7b.Modelfile
```

## Особенности сборки на Windows

- Путь репозитория содержит кириллицу. Cargo, CMake и MSVC (сборка OCCT в обычном `target/`)
  с ним работают. Не работают FreeCADCmd, `ollama create -f` и bat-файлы `cmd.exe` с `chcp 65001`
  (строки пропускаются): для них файлы кладутся в `C:\fc-target` / `C:\fc-models`.
- Первая сборка OCCT (occt-sys) — 30–60 минут, дальше инкрементально.
- После любого STEP-обмена тестовый бинарник libtest падает с access violation при выходе
  процесса (все тесты уже прошли); обычные программы выходят чисто. Поэтому STEP-тесты —
  `crates/occt/tests/step.rs` с `harness = false`.
- Ollama 0.40.4 не видит RTX 4050 (`initial_count=0` при discovery), модели идут на CPU.
- CMake 4 отказывается от `cmake_minimum_required` < 3.5 в исходниках OCCT:
  `CMAKE_POLICY_VERSION_MINIMUM=3.5` (задано в `.cargo/config.toml`).

## Фаза 6: плагины и скрипты (2026-10-16)
- `rustup target add wasm32-wasip2` — установлен. Rust ≥ 1.82 собирает компоненты WASI 0.2 для
  `wasm32-wasip2` напрямую (`wit-bindgen` в крейте плагина), поэтому `cargo-component` не нужен.
- `wasm-tools` — `cargo install --locked wasm-tools` (проверка/печать WIT компонента).
- Python 3.14.2 (`%LOCALAPPDATA%\Python\bin`) — для пакета `formcraft` (скрипты по control channel).
- Ollama: модели `qwen2.5-coder:7b` и `qwen3:8b` (создан из GGUF 2026-10-16), обе на CPU.

## Linux (CI)

`build-essential pkg-config libssl-dev cmake clang libxkbcommon-dev libwayland-dev libx11-dev
libxcursor-dev libxrandr-dev libxi-dev libgl1-mesa-dev libgtk-3-dev` (eframe/wgpu/rfd), Rust через
rustup, Ollama через официальный install-скрипт (только для ручного ai-bench).
