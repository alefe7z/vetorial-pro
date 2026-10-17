//! Color palette bars (Window → Color Palettes), as CorelDRAW docks them: strips of colour cells
//! along the window's edges. A click paints the selection's fill, a right-click its stroke; the
//! crossed cell is None. Each bar has an eyedropper that adds the colour clicked on the art to the
//! palette, and a menu to move it to another edge, edit it in the Palette Editor or close it.
//!
//! A bar shows a swatch library (built-in, User Defined or loaded: `swatch.library.list`) or
//! [`DOCUMENT`], the active document's own swatches. The bars are UI state, saved with the
//! preferences.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use vectorcraft_color::Color;
use vectorcraft_engine::cmd::swatchlib;

use crate::VectorcraftApp;
use crate::menus::Item;
use crate::theme::Tokens;
use crate::widgets;

/// The library key of the document palette: the active document's swatches.
pub const DOCUMENT: &str = "document";

/// The window edge a bar is docked on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum BarSide {
    #[default]
    Right,
    Left,
    Top,
    Bottom,
}

impl BarSide {
    pub const ALL: [BarSide; 4] = [BarSide::Right, BarSide::Left, BarSide::Top, BarSide::Bottom];

    pub fn id(self) -> &'static str {
        match self {
            BarSide::Right => "right",
            BarSide::Left => "left",
            BarSide::Top => "top",
            BarSide::Bottom => "bottom",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|b| b.id() == s)
    }
    fn label(self) -> &'static str {
        match self {
            BarSide::Right => "Dock Right",
            BarSide::Left => "Dock Left",
            BarSide::Top => "Dock Top",
            BarSide::Bottom => "Dock Bottom",
        }
    }
    fn vertical(self) -> bool {
        matches!(self, BarSide::Right | BarSide::Left)
    }
}

/// One docked palette bar.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct PaletteBar {
    /// A swatch library id, or [`DOCUMENT`].
    pub library: String,
    pub side: BarSide,
    /// Vetorial: the space between colours (points; 0, the default, puts them edge to edge).
    #[serde(default)]
    pub gap: f32,
    /// The size of a colour cell (points).
    #[serde(default = "default_size")]
    pub size: f32,
}

fn default_size() -> f32 {
    CELL
}

impl PaletteBar {
    pub fn new(library: &str, side: BarSide) -> Self {
        Self { library: library.to_string(), side, gap: 0.0, size: CELL }
    }
}

/// The colour cells' sizes and spacings a bar takes.
pub const SIZES: std::ops::RangeInclusive<f32> = 8.0..=48.0;
pub const GAPS: std::ops::RangeInclusive<f32> = 0.0..=20.0;

/// The bars of a first launch: the default CMYK palette on the right, the document palette at the
/// bottom.
pub fn defaults() -> Vec<PaletteBar> {
    vec![PaletteBar::new("default-cmyk", BarSide::Right), PaletteBar::new(DOCUMENT, BarSide::Bottom)]
}

/// A colour cell: the swatch's name, its colour and, for the document palette, the swatch it applies.
pub struct Cell {
    pub name: String,
    pub color: Color,
    pub swatch: Option<String>,
}

/// The name and the colours of palette `key`; `None` when there is no such library (or, for the
/// document palette, no document).
pub fn cells(app: &VectorcraftApp, key: &str) -> Option<(String, Vec<Cell>)> {
    if key == DOCUMENT {
        let doc = &app.session.active()?.doc;
        let cells = doc
            .swatches_iter()
            .filter(|w| !w.is_reserved())
            .filter_map(|w| Some(Cell { name: w.name.clone(), color: w.paint.color()?, swatch: Some(w.name.clone()) }))
            .chain(used_colors(app).into_iter().map(|c| Cell { name: values(&c), color: c, swatch: None }))
            .collect();
        return Some((tl!("Document Palette").to_string(), cells));
    }
    let (info, lib) = swatchlib::library(&app.session, key)?;
    let cells = lib.iter().filter_map(|w| Some(Cell { name: w.name.clone(), color: w.paint.color()?, swatch: None })).collect();
    Some((tl!(&info.name).to_string(), cells))
}

/// The colours the active document's art uses that no swatch holds (the document palette shows them
/// after its swatches), worked out again only when the document changes.
fn used_colors(app: &VectorcraftApp) -> Vec<Color> {
    type Cached = Option<((u64, u64), Vec<Color>)>;
    thread_local!(static CACHE: std::cell::RefCell<Cached> = const { std::cell::RefCell::new(None) });
    let Some(st) = app.session.active() else { return vec![] };
    let key = (st.uid, st.revision);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        match c.as_ref() {
            Some((k, colors)) if *k == key => colors.clone(),
            _ => {
                let colors = vectorcraft_engine::cmd::swatch::used_colors(&st.doc, 2000);
                *c = Some((key, colors.clone()));
                colors
            }
        }
    })
}

/// Can palette `key` be changed (the document palette, or a User Defined library that isn't read
/// only)?
pub fn editable(app: &VectorcraftApp, key: &str) -> bool {
    if key == DOCUMENT {
        return app.session.active().is_some();
    }
    swatchlib::libraries(&app.session).iter().any(|l| l.id == key && l.category == "user")
        && app.session.swatch_libraries.path(key).is_some_and(|p| {
            let ext = p.rsplit_once('.').map_or("", |(_, e)| e).to_ascii_lowercase();
            matches!(ext.as_str(), "vcswatches" | "gpl" | "ase")
        })
}

/// The id prefix of the Window → Color Palettes → User Defined slots.
pub(crate) const USER_SLOT: &str = "window.userColorPalette";

/// The User Defined library slot `id` (`window.userColorPalette3`) stands for.
pub(crate) fn user_library(app: &VectorcraftApp, id: &str) -> Option<swatchlib::LibraryInfo> {
    crate::panels::library_panel::user_slot(id, USER_SLOT, swatchlib::libraries(&app.session))
}

/// Window → Color Palettes.
pub(crate) fn window_menu() -> Vec<Item> {
    const SLOTS: [&str; 10] = [
        "window.userColorPalette1",
        "window.userColorPalette2",
        "window.userColorPalette3",
        "window.userColorPalette4",
        "window.userColorPalette5",
        "window.userColorPalette6",
        "window.userColorPalette7",
        "window.userColorPalette8",
        "window.userColorPalette9",
        "window.userColorPalette10",
    ];
    let bar = |label: &'static str, id: &'static str| Item::Cmd(label, "window.colorPalette", json!({ "library": id }));
    let mut items = vec![
        Item::Cmd("Close All Palettes", "window.colorPalettes.closeAll", Value::Null),
        Item::Sep,
        bar("Document Palette", DOCUMENT),
        bar("Default CMYK Palette", "default-cmyk"),
        bar("Default RGB Palette", "default-rgb"),
        Item::Sep,
    ];
    items.extend(
        vectorcraft_color::libraries::SWATCH_LIBRARIES
            .iter()
            .filter(|b| !matches!(b.id, "default-cmyk" | "default-rgb"))
            .map(|b| Item::Cmd(b.name, "window.colorPalette", json!({ "library": b.id }))),
    );
    items.extend([
        Item::Sep,
        Item::Sub("User Defined", SLOTS.iter().map(|id| Item::Cmd("User Palette", id, Value::Null)).collect()),
        Item::Sep,
        Item::Cmd("Open Palette…", "window.colorPalettes.open", Value::Null),
        Item::Sep,
        Item::Cmd("Create Palette from Selection", "window.colorPalettes.fromSelection", Value::Null),
        Item::Cmd("Create Palette from Document", "window.colorPalettes.fromDocument", Value::Null),
        Item::Sep,
        Item::Cmd("Palette Editor…", "window.colorPalettes.editor", Value::Null),
    ]);
    items
}

/// Is palette `key` shown as a bar?
fn shown(app: &VectorcraftApp, key: &str) -> bool {
    app.ui.palette_bars.iter().any(|b| b.library == key)
}

/// Show palette `key` as a bar (on the right, Corel's default) or, with `on: false`, close it.
fn set_shown(app: &mut VectorcraftApp, key: &str, on: bool, side: Option<BarSide>) {
    app.ui.palette_bars_set = true;
    match app.ui.palette_bars.iter_mut().find(|b| b.library == key) {
        Some(b) if on => {
            if let Some(side) = side {
                b.side = side;
            }
        }
        Some(_) => app.ui.palette_bars.retain(|b| b.library != key),
        None if on => app.ui.palette_bars.push(PaletteBar::new(key, side.unwrap_or_default())),
        None => {}
    }
}

/// The library key of a `library` param: an id, a name, or [`DOCUMENT`].
fn library_key(app: &VectorcraftApp, p: &Value) -> Result<String, String> {
    let key = p.get("library").and_then(Value::as_str).ok_or("missing `library`")?;
    if key == DOCUMENT {
        return Ok(key.to_string());
    }
    swatchlib::library(&app.session, key).map(|(info, _)| info.id).ok_or_else(|| format!("no swatch library `{key}` (see swatch.library.list)"))
}

/// The Window → Color Palettes commands and the bars' own (`ui.paletteBar.*`); `None` for any
/// other id.
pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let r = match id {
        "window.colorPalette" => (|| {
            let key = library_key(app, p)?;
            let on = p.get("show").and_then(Value::as_bool).unwrap_or(!shown(app, &key));
            let side = p
                .get("side")
                .and_then(Value::as_str)
                .map(|s| BarSide::parse(s).ok_or(format!("side is right, left, top or bottom, not `{s}`")))
                .transpose()?;
            set_shown(app, &key, on, side);
            Ok(json!({ "library": key, "shown": on }))
        })(),
        "window.colorPalettes.closeAll" => {
            app.ui.palette_bars.clear();
            app.ui.palette_bars_set = true;
            Ok(Value::Null)
        }
        "window.colorPalettes.open" => (|| {
            let path = match p.get("path").and_then(Value::as_str) {
                Some(path) => path.to_string(),
                None => {
                    let pick = crate::FilePick { filters: vec![("Color palettes", swatchlib::LIBRARY_EXTS)], ..Default::default() };
                    match crate::picks::open(app, &pick) {
                        Some(path) => path,
                        None => return Ok(Value::Null),
                    }
                }
            };
            let r = app.run("swatch.library.load", json!({ "path": path }))?;
            let key = r["library"].as_str().unwrap_or_default().to_string();
            set_shown(app, &key, true, None);
            Ok(r)
        })(),
        "window.colorPalettes.fromSelection" | "window.colorPalettes.fromDocument" => (|| {
            let from = if id.ends_with("Selection") { "selection" } else { "document" };
            let name = p.get("name").and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| {
                let title = app.session.active().map(|d| d.title()).unwrap_or_default();
                let stem = title.rsplit_once('.').map_or(title.as_str(), |(s, _)| s).to_string();
                if stem.is_empty() { tl!("New Palette").to_string() } else { stem }
            });
            let r = app.run("swatch.library.new", json!({ "name": name, "from": from }))?;
            let key = r["library"].as_str().unwrap_or_default().to_string();
            set_shown(app, &key, true, None);
            crate::dialogs::palette_editor::open(app, Some(&key));
            Ok(r)
        })(),
        "window.colorPalettes.editor" => {
            let key = p.get("library").and_then(Value::as_str).map(str::to_string);
            crate::dialogs::palette_editor::open(app, key.as_deref());
            Ok(Value::Null)
        }
        "ui.paletteBar.set" => (|| {
            let key = library_key(app, p)?;
            let num = |k: &str, range: &std::ops::RangeInclusive<f32>| -> Result<Option<f32>, String> {
                match p.get(k) {
                    None => Ok(None),
                    Some(v) => v
                        .as_f64()
                        .map(|f| f as f32)
                        .filter(|f| range.contains(f))
                        .map(Some)
                        .ok_or_else(|| format!("{k} is {} to {} pt", range.start(), range.end())),
                }
            };
            let (gap, size) = (num("gap", &GAPS)?, num("size", &SIZES)?);
            let bar = app.ui.palette_bars.iter_mut().find(|b| b.library == key).ok_or("that palette isn't shown")?;
            if let Some(g) = gap {
                bar.gap = g;
            }
            if let Some(z) = size {
                bar.size = z;
            }
            let (gap, size) = (bar.gap, bar.size);
            app.ui.palette_bars_set = true;
            Ok(json!({ "library": key, "gap": gap, "size": size }))
        })(),
        "ui.paletteBar.eyedropper" => (|| {
            let key = library_key(app, p)?;
            eyedropper(app, &key)
        })(),
        id if id.starts_with(USER_SLOT) => match user_library(app, id) {
            Some(l) => {
                let on = !shown(app, &l.id);
                set_shown(app, &l.id, on, None);
                Ok(json!({ "library": l.id, "shown": on }))
            }
            None => Err("no such user palette".into()),
        },
        _ => return None,
    };
    Some(r)
}

/// Is the menu item checked (a shown palette)? `None` for other ids.
pub fn checked(app: &VectorcraftApp, id: &str, p: &Value) -> Option<bool> {
    match id {
        "window.colorPalette" => Some(library_key(app, p).is_ok_and(|k| shown(app, &k))),
        id if id.starts_with(USER_SLOT) => Some(user_library(app, id).is_some_and(|l| shown(app, &l.id))),
        _ => None,
    }
}

/// The eyedropper of palette `key`: the next click on the art adds its colour to the palette. A
/// read-only palette is first copied into the user library folder, and the bar shows the copy.
fn eyedropper(app: &mut VectorcraftApp, key: &str) -> Result<Value, String> {
    if app.session.active().is_none() {
        return Err("open a document to sample its colours".into());
    }
    let key = if editable(app, key) {
        key.to_string()
    } else {
        let name = cells(app, key).map(|(n, _)| n).unwrap_or_default();
        let copy = crate::i18n::fmt(tl!("{name} (copy)"), &[("name", &name)]);
        let r = app.run("swatch.library.new", json!({ "name": copy, "from": key }))?;
        let new = r["library"].as_str().unwrap_or_default().to_string();
        for b in app.ui.palette_bars.iter_mut().filter(|b| b.library == key) {
            b.library = new.clone();
        }
        app.ui.palette_bars_set = true;
        new
    };
    let back = app.session.tool_id().to_string();
    let back = if back == "eyedropper" { "selection".to_string() } else { back };
    app.select_tool("eyedropper");
    app.session.set_tool_option("palette", &json!({ "library": key, "back": back }));
    app.status(tl!("Click a colour on the art to add it to the palette").to_string());
    Ok(json!({ "library": key }))
}

/// Cell size along and across a bar.
const CELL: f32 = 18.0;
const GAP: f32 = 2.0;

/// Draw every docked bar (before the dock and the canvas, so they sit at the window's edges).
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let bars = app.ui.palette_bars.clone();
    for (i, bar) in bars.iter().enumerate() {
        let t = Tokens::get(ui.ctx());
        let frame = egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).inner_margin(egui::Margin::same(3));
        let id = egui::Id::new(("palette_bar", i, bar.library.as_str()));
        let panel = match bar.side {
            BarSide::Right => egui::Panel::right(id),
            BarSide::Left => egui::Panel::left(id),
            BarSide::Top => egui::Panel::top(id),
            BarSide::Bottom => egui::Panel::bottom(id),
        };
        panel.exact_size(bar.size.max(CELL) + 8.0).resizable(false).frame(frame).show(ui, |ui| strip(app, ui, bar, id));
    }
}

/// One bar's contents: the eyedropper, the menu, the None cell, then the colours, scrolled by the
/// arrows or the mouse wheel when they don't fit.
fn strip(app: &mut VectorcraftApp, ui: &mut Ui, bar: &PaletteBar, id: egui::Id) {
    let t = Tokens::get(ui.ctx());
    let Some((name, cells)) = cells(app, &bar.library) else {
        // A library that is gone (a deleted User Defined file) shows an empty bar with its menu.
        bar_menu(app, ui, bar, "—");
        return;
    };
    let vertical = bar.side.vertical();
    let full = ui.available_rect_before_wrap();
    // The fixed head: eyedropper and menu.
    let head = if vertical {
        ui.vertical_centered(|ui| {
            pick_button(app, ui, bar);
            bar_menu(app, ui, bar, &name);
        })
        .response
        .rect
    } else {
        ui.horizontal_centered(|ui| {
            pick_button(app, ui, bar);
            bar_menu(app, ui, bar, &name);
        })
        .response
        .rect
    };
    let start = if vertical { head.bottom() + GAP } else { head.right() + GAP };
    // Where the cells start (the tests click them there).
    ui.ctx().data_mut(|d| d.insert_temp(id.with("start"), start));
    let end = if vertical { full.bottom() } else { full.right() };
    let span = (end - start).max(0.0);
    // The colours' own size and spacing (the bar's settings); the head keeps its own.
    let cell = bar.size.clamp(*SIZES.start(), *SIZES.end());
    let step = cell + bar.gap.clamp(*GAPS.start(), *GAPS.end());
    // One slot each for None and the colours; arrows take a slot at each end when they don't fit.
    let total = cells.len() + 1;
    let fits = (span / step).floor() as usize;
    let scrolling = total > fits;
    let room = if scrolling { fits.saturating_sub(2) } else { fits };
    let key = id.with("offset");
    let mut offset: usize = ui.ctx().data(|d| d.get_temp(key)).unwrap_or(0);
    let max_offset = total.saturating_sub(room.max(1));
    let area = Rect::from_min_max(if vertical { pos2(full.left(), start) } else { pos2(start, full.top()) }, full.max);
    let hover = ui.rect_contains_pointer(area);
    if scrolling && hover {
        let d = ui.input(|i| i.smooth_scroll_delta);
        let delta = if vertical { d.y } else { d.y + d.x };
        if delta < -1.0 {
            offset += 1;
        } else if delta > 1.0 {
            offset = offset.saturating_sub(1);
        }
    }
    offset = offset.min(max_offset);
    // Slot `k`'s rect along the bar.
    let slot = |k: usize| {
        let a = start + k as f32 * step;
        if vertical {
            Rect::from_min_size(pos2(full.center().x - cell / 2.0, a), vec2(cell, cell))
        } else {
            Rect::from_min_size(pos2(a, full.center().y - cell / 2.0), vec2(cell, cell))
        }
    };
    let mut k = 0;
    if scrolling {
        let r = slot(0);
        let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tl!("Scroll"));
        arrow(ui, r, vertical, true, offset > 0, &t);
        if resp.clicked() {
            offset = offset.saturating_sub(room.max(1));
        }
        k = 1;
    }
    for item in offset..(offset + room).min(total) {
        let r = slot(k);
        k += 1;
        if item == 0 {
            let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tl!("None (click: fill, right-click: stroke)"));
            none_cell(ui, r, &t);
            if resp.clicked() {
                paint(app, "paint.setFill", json!({"none": true}));
            } else if resp.secondary_clicked() {
                paint(app, "paint.setStroke", json!({"none": true}));
            }
            continue;
        }
        let Some(c) = cells.get(item - 1) else { continue };
        let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(format!("{}\n{}", c.name, values(&c.color)));
        let [red, green, blue] = c.color.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
        // Edge to edge (no gap) the colours touch with no outline, as CorelDRAW's do.
        let round = if bar.gap > 0.0 { 1.0 } else { 0.0 };
        ui.painter().rect_filled(r, round, Color32::from_rgb(red, green, blue));
        if resp.hovered() {
            ui.painter().rect_stroke(r, round, Stroke::new(1.5, t.text), egui::StrokeKind::Inside);
        } else if bar.gap > 0.0 {
            ui.painter().rect_stroke(r, round, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        }
        let params = match &c.swatch {
            Some(s) => json!({ "swatch": s }),
            None => json!({ "color": c.color }),
        };
        if resp.clicked() {
            paint(app, "paint.setFill", params);
        } else if resp.secondary_clicked() {
            paint(app, "paint.setStroke", params);
        }
    }
    if scrolling {
        let r = slot(room + 1);
        let resp = ui.allocate_rect(r, Sense::click()).on_hover_text(tl!("Scroll"));
        arrow(ui, r, vertical, false, offset < max_offset, &t);
        if resp.clicked() {
            offset = (offset + room.max(1)).min(max_offset);
        }
    }
    ui.ctx().data_mut(|d| d.insert_temp(key, offset));
}

/// Paint the selection (and the defaults); a failure (no document) shows in the status bar.
fn paint(app: &mut VectorcraftApp, cmd: &str, p: Value) {
    if app.session.active().is_none() {
        return;
    }
    if let Err(e) = app.run(cmd, p) {
        app.status(e);
    }
}

fn pick_button(app: &mut VectorcraftApp, ui: &mut Ui, bar: &PaletteBar) {
    let picking = app.session.tool_id() == "eyedropper" && app.session.tool_options()["palette"]["library"].as_str() == Some(bar.library.as_str());
    if widgets::icon_button(ui, "pipette", tl!("Add a colour from the art to this palette"), picking, CELL).clicked()
        && let Err(e) = app.run("ui.paletteBar.eyedropper", json!({ "library": bar.library }))
    {
        app.status(e);
    }
}

/// The bar's menu: its name, the edges it docks on, Palette Editor…, Close.
fn bar_menu(app: &mut VectorcraftApp, ui: &mut Ui, bar: &PaletteBar, name: &str) {
    let resp = widgets::icon_button(ui, "ellipsis", name, false, CELL);
    egui::Popup::menu(&resp).show(|ui| {
        ui.label(egui::RichText::new(name).strong());
        ui.separator();
        for side in BarSide::ALL {
            if widgets::menu_item(ui, tl!(side.label()), true, bar.side == side) {
                set_shown(app, &bar.library, true, Some(side));
            }
        }
        ui.separator();
        // Vetorial: the colours' size and spacing.
        let (mut size, mut gap) = (bar.size, bar.gap);
        ui.horizontal(|ui| {
            ui.label(tl!("Color size"));
            if ui.add(egui::DragValue::new(&mut size).range(SIZES).speed(0.2).suffix(" pt").max_decimals(1)).changed() {
                app.run("ui.paletteBar.set", json!({ "library": bar.library, "size": size })).ok();
            }
        });
        ui.horizontal(|ui| {
            ui.label(tl!("Spacing"));
            if ui.add(egui::DragValue::new(&mut gap).range(GAPS).speed(0.1).suffix(" pt").max_decimals(1)).changed() {
                app.run("ui.paletteBar.set", json!({ "library": bar.library, "gap": gap })).ok();
            }
        });
        // Another palette beside this one: as many as wanted.
        ui.menu_button(tl!("Open Another Palette"), |ui| {
            let mut libs: Vec<(String, String)> = vec![(DOCUMENT.to_string(), tl!("Document Palette").to_string())];
            libs.extend(
                swatchlib::libraries(&app.session)
                    .into_iter()
                    .filter(|l| l.category != "gradients")
                    .map(|l| (l.id.clone(), tl!(&l.name).to_string())),
            );
            egui::ScrollArea::vertical().max_height(360.0).show(ui, |ui| {
                for (id, name) in libs {
                    let on = shown(app, &id);
                    if widgets::menu_item_name(ui, &name, true, on) {
                        set_shown(app, &id, !on, Some(bar.side));
                        ui.close();
                    }
                }
            });
        });
        ui.separator();
        if widgets::menu_item(ui, tl!("Palette Editor…"), true, false) {
            crate::dialogs::palette_editor::open(app, Some(&bar.library));
        }
        if widgets::menu_item(ui, tl!("Close Palette"), true, false) {
            set_shown(app, &bar.library, false, None);
        }
    });
}

/// The None cell: white, crossed in red.
fn none_cell(ui: &Ui, r: Rect, t: &Tokens) {
    let p = ui.painter();
    p.rect_filled(r, 1.0, Color32::WHITE);
    p.line_segment([r.left_bottom() + vec2(2.0, -2.0), r.right_top() + vec2(-2.0, 2.0)], Stroke::new(1.5, Color32::from_rgb(220, 40, 40)));
    p.rect_stroke(r, 1.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
}

/// A scroll arrow: towards the start (`back`) or the end of the bar.
fn arrow(ui: &Ui, r: Rect, vertical: bool, back: bool, enabled: bool, t: &Tokens) {
    let c = r.center();
    let s = 4.0;
    let pts: [Pos2; 3] = match (vertical, back) {
        (true, true) => [c + vec2(-s, s / 2.0), c + vec2(s, s / 2.0), c + vec2(0.0, -s / 2.0)],
        (true, false) => [c + vec2(-s, -s / 2.0), c + vec2(s, -s / 2.0), c + vec2(0.0, s / 2.0)],
        (false, true) => [c + vec2(s / 2.0, -s), c + vec2(s / 2.0, s), c + vec2(-s / 2.0, 0.0)],
        (false, false) => [c + vec2(-s / 2.0, -s), c + vec2(-s / 2.0, s), c + vec2(s / 2.0, 0.0)],
    };
    let color = if enabled { t.text } else { t.text_disabled };
    ui.painter().add(egui::Shape::convex_polygon(pts.to_vec(), color, Stroke::NONE));
}

/// A colour's values as a tooltip shows them.
pub fn values(c: &Color) -> String {
    let p = |v: f32| (v * 100.0).round() as i32;
    match *c {
        Color::Cmyk { c, m, y, k } => format!("C{} M{} Y{} K{}", p(c), p(m), p(y), p(k)),
        Color::Gray { k } => format!("K{}", p(k)),
        Color::Lab { l, a, b } => format!("L{} a{} b{}", l.round(), a.round(), b.round()),
        Color::Rgb { r, g, b } => {
            let v = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as u8;
            format!("R{} G{} B{}", v(r), v(g), v(b))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Event, PointerButton};
    use vectorcraft_engine::Session;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        let id = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap()["id"].clone();
        app.run("select.set", json!({ "ids": [id] })).unwrap();
        app
    }

    /// One 800×600 frame of the bars with `events`.
    fn frame(ctx: &egui::Context, app: &mut VectorcraftApp, events: Vec<Event>) {
        let raw = egui::RawInput { screen_rect: Some(Rect::from_min_size(Pos2::ZERO, vec2(800.0, 600.0))), events, ..Default::default() };
        let mut out = ctx.run_ui(raw, |ui| show(app, ui));
        out.textures_delta.clear();
    }

    /// Click `button` at `pos` over three frames (hover, press, release).
    fn click(ctx: &egui::Context, app: &mut VectorcraftApp, pos: Pos2, button: PointerButton) {
        let press = |pressed| Event::PointerButton { pos, button, pressed, modifiers: Default::default() };
        for e in [Event::PointerMoved(pos), press(true), press(false)] {
            frame(ctx, app, vec![e]);
        }
    }

    fn selected_paint(app: &VectorcraftApp, stroke: bool) -> vectorcraft_color::Paint {
        let st = app.session.active().unwrap();
        let n = st.doc.node(st.selection.objects[0]).unwrap();
        if stroke { n.appearance.stroke_paint() } else { n.appearance.fill_paint() }
    }

    #[test]
    fn the_menu_shows_and_closes_bars_on_any_edge() {
        let mut app = app();
        assert!(app.ui.palette_bars.is_empty());
        app.run("window.colorPalette", json!({"library": "default-rgb"})).unwrap();
        assert_eq!(crate::menus::checked(&app, "window.colorPalette", &json!({"library": "default-rgb"})), Some(true));
        assert!(app.run("window.colorPalette", json!({"library": "default-rgb", "side": "middle"})).is_err());
        app.run("window.colorPalette", json!({"library": "default-rgb", "show": true, "side": "top"})).unwrap();
        assert_eq!(app.ui.palette_bars, vec![PaletteBar::new("default-rgb", BarSide::Top)]);
        app.run("window.colorPalette", json!({"library": "document", "show": true})).unwrap();
        assert!(app.run("window.colorPalette", json!({"library": "nope"})).is_err());
        app.run("window.colorPalettes.closeAll", json!({})).unwrap();
        assert!(app.ui.palette_bars.is_empty() && app.ui.palette_bars_set);
        assert_eq!(defaults()[0].library, "default-cmyk");
    }

    #[test]
    fn a_click_paints_the_fill_and_a_right_click_the_stroke() {
        let mut app = app();
        app.ui.palette_bars = vec![PaletteBar::new("default-cmyk", BarSide::Right)];
        let ctx = egui::Context::default();
        frame(&ctx, &mut app, vec![]);
        // The right bar: eyedropper, menu, then None and the colours, CELL + GAP apart.
        let x = 800.0 - (CELL + 8.0) / 2.0;
        let id = egui::Id::new(("palette_bar", 0usize, "default-cmyk"));
        let y0: f32 = ctx.data(|d| d.get_temp(id.with("start"))).unwrap();
        let slot = |k: f32| pos2(x, y0 + k * (CELL + GAP) + CELL / 2.0);
        // The 72 colours don't fit in 600 px: slot 0 is the scroll arrow, 1 None, 2 the first
        // colour (CMYK black).
        click(&ctx, &mut app, slot(2.0), PointerButton::Primary);
        assert_eq!(selected_paint(&app, false).color(), Some(Color::cmyk(0.0, 0.0, 0.0, 1.0)));
        click(&ctx, &mut app, slot(3.0), PointerButton::Secondary);
        assert_eq!(selected_paint(&app, true).color(), Some(Color::cmyk(0.0, 0.0, 0.0, 0.9)));
        click(&ctx, &mut app, slot(1.0), PointerButton::Primary);
        assert!(selected_paint(&app, false).is_none());
        // The far arrow scrolls a page on: slot 2 is then a later colour.
        let id = id.with("offset");
        let last = (600.0 - 3.0 - y0) / (CELL + GAP);
        click(&ctx, &mut app, slot(last.floor() - 1.0), PointerButton::Primary);
        assert!(ctx.data(|d| d.get_temp::<usize>(id)).unwrap() > 0);
    }

    #[test]
    fn the_eyedropper_samples_into_the_palette_or_an_editable_copy() {
        let mut app = app();
        app.ui.palette_bars = vec![PaletteBar::new(DOCUMENT, BarSide::Bottom)];
        app.run("ui.paletteBar.eyedropper", json!({"library": "document"})).unwrap();
        assert_eq!(app.session.tool_id(), "eyedropper");
        assert_eq!(app.session.tool_options()["palette"], json!({"library": "document", "back": "selection"}));
        // A built-in palette is copied first (here: no user library folder, so it fails and says so).
        assert!(app.run("ui.paletteBar.eyedropper", json!({"library": "default-cmyk"})).is_err());
        let dir = std::env::temp_dir().join(format!("vc-palette-bars-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        app.session.swatch_libraries.set_user_dir(Some(dir.to_string_lossy().to_string()));
        app.ui.palette_bars.push(PaletteBar::new("default-cmyk", BarSide::Right));
        let r = app.run("ui.paletteBar.eyedropper", json!({"library": "default-cmyk"})).unwrap();
        let copy = r["library"].as_str().unwrap().to_string();
        assert!(copy.starts_with("user/"), "{r}");
        assert!(editable(&app, &copy));
        assert_eq!(app.ui.palette_bars[1].library, copy, "the bar shows the copy");
        assert_eq!(cells(&app, &copy).unwrap().1.len(), cells(&app, "default-cmyk").unwrap().1.len());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn the_document_palette_lists_the_colours_the_art_uses() {
        let mut app = app();
        let r = app.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
        app.run("select.set", json!({"ids": [r["id"]]})).unwrap();
        app.run("paint.setFill", json!({"color": "#12ab34"})).unwrap();
        let (_, before) = cells(&app, DOCUMENT).unwrap();
        let green = Color::from_hex("#12ab34").unwrap();
        assert!(before.iter().any(|c| c.color == green && c.swatch.is_none()), "the fill colour, no swatch for it");
        // Made a swatch, it shows once, as the swatch.
        app.run("swatch.addUsedColors", json!({})).unwrap();
        let (_, after) = cells(&app, DOCUMENT).unwrap();
        assert_eq!(after.iter().filter(|c| c.color == green).count(), 1);
    }

    #[test]
    fn bars_take_any_spacing_and_size() {
        let mut app = app();
        app.ui.palette_bars = vec![PaletteBar::new("default-cmyk", BarSide::Right)];
        // Edge to edge by default.
        assert_eq!((app.ui.palette_bars[0].gap, app.ui.palette_bars[0].size), (0.0, CELL));
        let r = app.run("ui.paletteBar.set", json!({"library": "default-cmyk", "gap": 4, "size": 30})).unwrap();
        assert_eq!(r, json!({"library": "default-cmyk", "gap": 4.0, "size": 30.0}));
        assert!(app.run("ui.paletteBar.set", json!({"library": "default-cmyk", "gap": 99})).is_err());
        assert!(app.run("ui.paletteBar.set", json!({"library": "default-rgb", "gap": 1})).is_err(), "not shown");
        // Several bars at once, each its own.
        app.run("window.colorPalette", json!({"library": "default-rgb", "show": true, "side": "bottom"})).unwrap();
        app.run("window.colorPalette", json!({"library": "document", "show": true, "side": "bottom"})).unwrap();
        assert_eq!(app.ui.palette_bars.len(), 3);
        assert_eq!(app.ui.palette_bars[1].gap, 0.0);
        // Old saved bars (no gap, no size) read edge to edge at the usual size.
        let old: PaletteBar = serde_json::from_value(json!({"library": "x", "side": "right"})).unwrap();
        assert_eq!((old.gap, old.size), (0.0, CELL));
        // The cells sit `size + gap` apart in a frame.
        let ctx = egui::Context::default();
        app.ui.palette_bars.truncate(1);
        frame(&ctx, &mut app, vec![]);
        frame(&ctx, &mut app, vec![]);
    }

    #[test]
    fn the_document_palette_lists_the_documents_colours() {
        let mut app = app();
        app.run("swatch.new", json!({"name": "Laranja", "color": "#ff8800"})).unwrap();
        let (_, cells) = cells(&app, DOCUMENT).unwrap();
        assert!(cells.iter().any(|c| c.name == "Laranja" && c.swatch.as_deref() == Some("Laranja")));
        assert!(!cells.iter().any(|c| c.name == vectorcraft_color::swatch::REGISTRATION));
        assert!(editable(&app, DOCUMENT) && !editable(&app, "default-rgb"));
    }
}
