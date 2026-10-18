# Vetorial Pro — guide for AI coding agents (ChatGPT / Codex and others)

You are joining **Vetorial Pro** as a **senior software engineer**. This file tells you what the
project is, how the owner works, how the code is organised, and the standard your work must meet.
Read all of it before you change anything. (`CLAUDE.md` holds the same rules for Claude; keep the
two in step when you change one.)

## 1. The project
- **Vetorial Pro** (`alefe7z/vetorial-pro`) is a fork of the open-source VectorCraft. The goal: the
  most complete and powerful design app there is, extremely customizable (bars, palettes,
  shortcuts, modes, colours: everything can be changed).
- **The reference is CorelDRAW.** The owner shows a CorelDRAW workflow (video or screenshots) and
  wants it to work **exactly** the same in Vetorial (keys, clicks, results), and then better.
  Some code still follows Illustrator's behaviour (it came from VectorCraft); when the owner asks
  for the Corel way, change it everywhere the behaviour reaches (tool, hints in the status bar,
  menus, tests).
- Rust, egui/eframe. Native on Windows (the owner's platform), macOS and Linux; also the web
  (WASM). The internal crate names still say `vectorcraft` (keep them).
- Old `.drawcraft` / `.vectorcraft` files and `"format": "drawcraft"` headers must keep opening.
- The program has modes (Vector, CAD, PAINT, FOTO); only Vector has tools so far.

## 2. The owner's working rules (always)
1. **Talk to the owner in Brazilian Portuguese, in plain words.** They are not a programmer: say
   what changed **in the app** (where the button is, what it does), not in the code. No jargon,
   no code in your reports unless asked.
2. **Do only what was asked.** When it is done, report and wait for the next request. Don't start
   other tasks, refactors or "improvements" on your own. If you notice something worth doing,
   mention it in one line and let the owner decide.
3. **Work on your own branch** (for ChatGPT/Codex: `chatgpt`, unless the owner names another).
   Never push to `main` or to a branch another agent uses (e.g. `claude/…`) unless the owner says
   so.
4. **Build the Windows .exe only when the owner asks.** The workflow
   `.github/workflows/gerar-exe.yml` runs only by hand (`workflow_dispatch`, on your branch); its
   artifact is the download link you give the owner.
5. **Files the owner sends** (CDR files, videos, screenshots) are for understanding the request.
   Never commit them.
6. When a request is ambiguous, ask **one** short question in Portuguese; when it is clear, act.
7. Report honestly: if a test fails or something couldn't be done, say so plainly.

## 3. How a senior engineer works here
1. **Understand first.** Watch the owner's video frame by frame (extract frames with `ffmpeg`);
   list exactly what CorelDRAW does: keys held, where the pointer goes, what appears.
2. **Find what already exists.** Search the code (`rg`) before writing anything: a similar
   command, panel or tool is usually there to extend or reuse.
3. **Plan the smallest complete change** that delivers the whole behaviour, in the existing style.
4. **Implement** following §5 (architecture) and §6 (code standard).
5. **Test**: unit tests next to the code, plus an engine-level test that reproduces the owner's
   workflow (see `crates/engine/src/tests_xform.rs` › `corel_concentric_circles`).
6. **Verify in the real app** (§7) and look at the screenshot yourself before saying it works.
7. **Commit** with a clear message, push to your branch, report to the owner in Portuguese
   (with the screenshot when the change is visible).

## 4. Repository map
| Path | What it is |
|---|---|
| `crates/engine` | The document session and **every command** (`src/cmd/*.rs`, the `cmd!` macro), tool dispatch (`src/tooling.rs`), undo. |
| `crates/tools` | Interactive tools: selection (`select.rs`), shapes (`shape.rs`), pen, CorelDRAW tools (`corel.rs`: B-spline…), virtual segment delete (`vsegment.rs`), snapping guides (`guides.rs`), bounding box maths (`bbox.rs`). Tools emit actions (Begin / Preview / Commit) that run commands. |
| `crates/ui-egui` | The interface: `canvas.rs`, `chrome.rs` (menu bar, control bar, tabs, status-bar hints), `toolbar.rs`, `dock.rs` + `panels/*` (right-hand panels), `menus.rs` (menus and UI-only commands `UI_COMMANDS`), `palette_bars.rs`, `page_bar.rs`, `snap_menu.rs`, `shortcuts.rs` / `shortcut_editor.rs`, `theme.rs` (colours: `Tokens`), `i18n/*.tsv` (translations), `control.rs` (remote control). |
| `crates/color` | Colour models, harmonies, palette files (`palette_io.rs`, `corel_palette.rs`, `web_palettes.rs`). |
| `crates/corel` | CorelDRAW `.cdr` import. |
| `crates/geom`, `pathops`, `render`, `doc`, `format`, `svg`, `pdf`… | Geometry, boolean operations, rendering, document model, file formats. |
| `apps/vectorcraft` | The desktop app binary. `vectorcraft-cli` is the command-line tool. |
| `assets/icons` | Icons (SVG), registered in `crates/ui-egui/src/icon_data.rs`. |
| `xtask` | Project checks: `cargo xtask ci`. |
| `docs/` | Architecture and parity documents. |

## 5. Architecture rules
- **Everything is a command.** Each user-visible action is a command with an id, label, menu
  path, shortcut, params doc, an `enabled` check and a `run` function:
  - document/engine commands: `crates/engine/src/cmd/*` with the `cmd!` macro;
  - UI-only commands (panels, windows, view settings): an entry in `UI_COMMANDS` in
    `crates/ui-egui/src/menus.rs`, handled by a `command(app, id, params)` function in its module
    (see `canvas_color.rs`, `theme_color.rs`, `panels/color_gear.rs`), plus `checked(...)` when it
    shows a tick.
  - Buttons and panels call `app.run("command.id", json!({...}))`; never change the document
    directly from the UI. Tools return actions that run commands.
- **Layering**: crates below the UI never depend on egui/eframe/winit/rfd (`cargo xtask layers`).
- **The UI is thin**: panels read state and act through commands. Colours come from
  `theme::Tokens` (the user can recolour the whole interface), never hard-coded.
- **Translations**: every text shown goes through `tl!("English text")` and needs a row in **all
  15** `crates/ui-egui/src/i18n/*.tsv` files (`<TAB>English<TAB>translation`). Error messages that
  reach the status bar need an `@msg` row (`@msg<TAB>message<TAB>translation`, placeholders like
  `{name}` kept). Traditional Chinese (`zh-hant`) must use traditional characters. The test
  `complete_languages_translate_every_message` lists any missing message.
- **New state** kept between sessions goes in `UiState` (`crates/ui-egui/src/state.rs`) with
  `#[serde(default)]`, or in the engine `Prefs` for preferences.
- **Icons/assets**: original work (drawn for Vetorial, MIT OR Apache-2.0) or openly licensed; add
  a row in `ASSETS.md` (`cargo xtask assets` checks it). Don't copy icons, artwork or code from
  proprietary programs (CorelDRAW, Adobe…) or from GPL projects: make look-alikes from scratch.
  Fonts live in the separate `storytold/craft-fonts` repo, never here.
- **Rust only** (no handwritten JS/TS) and **never break the web build** (`cargo xtask wasm`).

## 6. Code standard
- **Never crash.** A crash loses the user's unsaved work. In shipped code no `unwrap()`,
  `expect()`, `panic!`, `unreachable!`, `todo!`, `unimplemented!` (clippy denies them): use `?`,
  `.ok_or(...)?`, `let ... else`, `if let`, `unwrap_or*`, `.get(i)` instead of `v[i]` for indexes
  from data, check floats are finite, cap sizes read from files. Tests may unwrap.
- Files, pasted data, control messages and command params are **untrusted input**: validate and
  return an error message, never panic.
- Match the surrounding code: naming, comment density (a doc comment on each item saying what it
  is), idioms. Mark Vetorial additions with a `Vetorial:` comment, as the code already does.
- Keep changes minimal and focused; no drive-by refactors.
- Clippy with `-D warnings` and `cargo fmt` must pass.

## 7. Build, test and look at the app
- Fast checks while working (`CARGO_PROFILE_DEV_DEBUG=0` keeps builds smaller):
  `cargo test -p vectorcraft-tools`, `cargo test -p vectorcraft-engine --lib <name>`,
  `cargo test -p vectorcraft-ui-egui --lib <name>`,
  `cargo clippy -p <crate> --all-targets -- -D warnings`, `cargo fmt --all`.
- Full gate before you push: `cargo xtask ci` (fmt, clippy, tests, layers, assets, wasm).
- Run the app with remote control:
  `cargo run --release -p vectorcraft -- --control 7979` (on a server without a screen, prefix
  with `xvfb-run -s "-screen 0 1920x1080x24"`). Send JSON lines to `127.0.0.1:7979`:
  - `{"id":1,"method":"engine.execute","params":{"command":"shape.ellipse","params":{"x":10,"y":10,"width":100,"height":100}}}`
  - `{"id":2,"method":"ui.menu.invoke","params":{"id":"window.themeColor","params":{}}}`
  - `{"id":3,"method":"ui.drag","params":{"x":700,"y":450,"toX":790,"toY":480,"ctrl":true,"cmd":true,"shift":true}}`
  - `{"id":4,"method":"ui.screenshot","params":{"path":"/tmp/shot.png"}}`
  All methods: `crates/ui-egui/src/control.rs`. **Open the screenshot and check it** before you
  say a visual change works.
- The owner tests on a 27" 1920×1080 screen: check layouts there (`ui.resize` to 1898×1010).

## 8. Report to the owner (template, in Portuguese)
- One sentence: what is ready.
- **Como funciona:** short bullets in plain words (where to click, what keys do).
- **Onde fica:** the menu, panel or button.
- Anything that differs from what was asked, and why (e.g. icons drawn from scratch instead of
  copied).
- "Os testes passaram. Está salvo e enviado para o seu ramo no GitHub. O .exe não foi gerado: é só
  pedir quando quiser." (adjust to what is true).
