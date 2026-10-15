# Vetorial Pro app icon

**Mark:** a white serif "V" on a violet rounded square: the Vetorial Pro logo, designed by the
project owner.

**Palette:**

| Colour | Hex | Used for |
|---|---|---|
| Violet | `#5614C9` | the rounded square |
| White | `#FFFFFF` | the "V" |

**Tile:** `viewBox="0 0 512 512"`, the owner's artwork scaled from its 13598.65-unit drawing into the
512 tile, corners as drawn (`rx` 1171.46 of 13598.65, about 44 at 512). The macOS files add Apple's
transparent margin (tile = 824/1024 of the canvas); Windows and Linux use the tile edge to edge.

**Provenance:** the owner's own logo (`vetorialprologo.svg`, made in CorelDRAW), kept as its two shapes
with the editor's metadata removed. `vectorcraft.svg` is the canonical artwork (the file names keep
the upstream project's names so packaging finds them). Licence: see `LICENSE.txt`.

## Files

| File | What | Used by |
|---|---|---|
| `vectorcraft.svg` | master vector | source for everything below |
| `vectorcraft-small.svg` | the same vector (kept for the scalable Linux icon) | `hicolor/scalable` |
| `vectorcraft-1024.png` | full tile, 1024 px | docs, store listings |
| `vectorcraft-macos-512.png` | tile with Apple margin, 512 px | runtime Dock icon (`apps/vectorcraft/src/main.rs`) |
| `vectorcraft.icns` | macOS icon set, 16 to 1024 px | `cargo xtask bundle` (`CFBundleIconFile`) |
| `vectorcraft.ico` | Windows icon, 16 to 256 px | `apps/vectorcraft/build.rs` (embedded in the `.exe`) |
| `hicolor/<size>/apps/ai.storyteller.vectorcraft.png` | Linux theme icons, 16 to 512 px; the 256 one is also the runtime icon on Windows and Linux, the 128 one the brand mark in the app bar and About box (`crates/ui-egui/src/brand.rs`) | `packaging/linux/ai.storyteller.vectorcraft.desktop` |
| `hicolor/scalable/apps/ai.storyteller.vectorcraft.svg` | Linux scalable icon | as above |

## How it reaches each OS

- **macOS:** the app sets the Dock / app-switcher icon at runtime (`ViewportBuilder::with_icon`); the
  `.app` from `cargo xtask bundle` carries `vectorcraft.icns`.
- **Windows:** `build.rs` embeds `vectorcraft.ico` and VERSIONINFO with `winresource` (taskbar, Start
  menu, Explorer, Alt-Tab); the runtime icon covers the title bar.
- **Linux:** install `packaging/linux/ai.storyteller.vectorcraft.desktop` to `share/applications/` and
  `hicolor/` to `share/icons/`. The app sets its Wayland app ID / X11 class to
  `ai.storyteller.vectorcraft`, so the dock matches the window to the launcher.

## Regenerate

Edit or replace `vectorcraft.svg` (and `vectorcraft-small.svg`), then run `packaging/icons.sh`
(needs `resvg`; `iconutil` on macOS for the `.icns`). It writes every PNG, the `.ico` (via
`cargo xtask ico`) and the `.icns`.
