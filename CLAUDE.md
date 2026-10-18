# Vetorial Pro — instructions for agents

Vetorial Pro (a fork of VectorCraft, alefe7z/vetorial-pro) aims to be the most complete and powerful design app there is, and extremely customizable: anything in it can be changed, without limits. CorelDRAW-style behaviour is the main reference. Keep building in the same direction as so far: powerful, creative tools that work exactly like the CorelDRAW workflow the owner shows (videos, screenshots), then go beyond it, with every part of the interface customizable (bars, palettes, shortcuts, modes, colours). It's a Rust app (egui) for Windows, macOS, Linux and the web (WASM).

Old `.drawcraft` / `.vectorcraft` files and `"format": "drawcraft"` headers still open; keep them working.

## Owner's working rules
- **Talk to the owner in Brazilian Portuguese**, in plain words (they are not a programmer): say what changed in the app, not in the code.
- **Work on your own branch**; don't change `main` unless the owner says so.
- **Do what the owner asks**; when it's done, report and wait for the next request.
- **Build the .exe only when the owner asks.** Pushing never builds it (`.github/workflows/gerar-exe.yml` runs on `workflow_dispatch` only).

## Technical notes (how the code is organised)
- User-visible actions are commands: engine commands in `crates/engine/src/cmd/*`; UI-only commands in `crates/ui-egui/src/menus.rs` (`UI_COMMANDS`). The control channel and MCP reach all of them.
- Interface translations: `crates/ui-egui/src/i18n/*.tsv`.
- Avoid crashes in shipped code (a crash loses the user's unsaved work): prefer `Result`/`?` to `unwrap()`. The workspace clippy lints flag `unwrap`/`expect`/`panic!` outside tests.
- Checks: `cargo xtask ci` (fmt, clippy, tests, wasm).

## Running and looking at the app
- `cargo run --release -p vectorcraft -- --control 7979 [file.svg|file.vectorcraft]`.
- Drive it: JSON lines on `127.0.0.1:7979`, e.g. `{"id":1,"method":"engine.execute","params":{"command":"shape.rectangle","params":{"x":10,"y":10,"width":100,"height":50}}}` then `{"id":2,"method":"ui.screenshot","params":{"path":"/tmp/shot.png"}}`. Methods: `crates/ui-egui/src/control.rs`.
- For UI work, look at the result (`ui.screenshot`, read the PNG).
- Performance: `vectorcraft-cli perf`; `vectorcraft-cli bench FILE`.
- MCP: `vectorcraft-cli mcp` (see `docs/mcp.md`).
- Shell gotcha: `mv`/`cp` may be aliased interactive — use `/bin/mv -f` / `/bin/cp -f`.
