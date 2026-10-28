//! Vetorial: the Tiling panel (Window › Tiling, Modifiers › Tiling…), laid out as Inkscape's Tiling
//! in its Path Effects panel and live like it: every change shows on the canvas at once.
//!
//! From the top: the effect's row (show or hide it, remove it), the sixteen mirroring modes, Rows
//! and Columns, Gap X and Gap Y (each the same everywhere or random) with the unit they show in,
//! Offset % (every other row or column), Scale % and Rotate ° with how they step (down the rows,
//! across the columns, both, every tile the same, random), then Mirror Transforms, Minimize Gaps,
//! Split Elements, Link Styles and Randomize. Without Tiling on the selection, a button applies it.
//!
//! The controls ([`controls`]) also make the Tiling effect dialog. Command: `tiling.open`.

use egui::{Color32, Painter, Rect, Sense, Shape, Stroke, StrokeKind, Ui, pos2, vec2};
use serde_json::{Map, Value, json};
use vectorcraft_doc::{NodeId, Unit};

use crate::VectorcraftApp;
use crate::theme::Tokens;
use crate::widgets;

/// The panel's id (`window.panel`).
pub const ID: &str = "tiling";
/// The Tiling effect.
pub const EFFECT: &str = "distort.tiling";

/// The mirroring modes in Inkscape's order: (rows X, rows Y, columns X, columns Y).
const MIRRORS: [[bool; 4]; 16] = [
    [false, false, false, false],
    [true, false, false, false],
    [true, true, false, false],
    [false, true, false, false],
    [false, false, true, true],
    [true, false, true, true],
    [true, true, true, true],
    [false, true, true, true],
    [false, false, true, false],
    [true, false, true, false],
    [true, true, true, false],
    [false, true, true, false],
    [false, false, false, true],
    [true, false, false, true],
    [true, true, false, true],
    [false, true, false, true],
];
/// The mirror flags' parameters, in [`MIRRORS`]' order.
const MIRROR_KEYS: [&str; 4] = ["mirrorRowsX", "mirrorRowsY", "mirrorColsX", "mirrorColsY"];

/// A selected object with Tiling: its id, the effect's position among its effects, its values
/// over the defaults and whether it shows.
pub struct Target {
    pub id: NodeId,
    pub index: usize,
    pub params: Value,
    pub visible: bool,
}

/// The selected objects with Tiling among their own effects (the first Tiling of each).
pub fn targets(app: &VectorcraftApp) -> Vec<Target> {
    let Some(st) = app.session.active() else { return vec![] };
    st.selection
        .objects
        .iter()
        .filter_map(|id| {
            let n = st.doc.node(*id)?;
            let (index, e) = n.appearance.effects.iter().enumerate().find(|(_, e)| e.id == EFFECT)?;
            Some(Target { id: *id, index, params: vectorcraft_effects::merged_params(EFFECT, &e.params), visible: e.visible })
        })
        .collect()
}

/// Run `f` as one undo step (unless an undo group is already open, as while scrubbing a field).
fn one_step(app: &mut VectorcraftApp, f: impl FnOnce(&mut VectorcraftApp) -> Result<(), String>) {
    let open = app.session.active().is_some_and(|st| st.undo_group.is_none());
    if open {
        app.session.begin_named_undo_group(Some("Tiling".into()));
    }
    let r = f(app);
    if open {
        app.session.end_undo_group(false);
    }
    if let Err(e) = r {
        app.status(e);
    }
}

/// Set `patch` on the Tiling of every target.
fn set(app: &mut VectorcraftApp, targets: &[Target], patch: Map<String, Value>) {
    let calls: Vec<Value> = targets.iter().map(|t| json!({ "ids": [t.id.0], "index": t.index, "item": null, "params": patch })).collect();
    one_step(app, |app| {
        for c in calls {
            app.run("effect.setParams", c)?;
        }
        Ok(())
    });
}

/// `tiling.open`: Tiling on the selected objects that lack it, and this panel shown.
pub fn command(app: &mut VectorcraftApp, id: &str, _p: &Value) -> Option<Result<Value, String>> {
    if id != "tiling.open" {
        return None;
    }
    Some((|| {
        let have: Vec<NodeId> = targets(app).iter().map(|t| t.id).collect();
        let missing: Vec<u64> =
            app.session.active().map(|st| st.selection.objects.iter().filter(|id| !have.contains(id)).map(|id| id.0).collect()).unwrap_or_default();
        if !missing.is_empty() {
            app.run("effect.apply", json!({ "effect": EFFECT, "ids": missing, "item": null }))?;
        }
        if crate::menus::checked(app, "window.panel", &json!({ "panel": ID })) != Some(true) {
            app.run("window.panel", json!({ "panel": ID }))?;
        }
        Ok(json!({ "applied": missing }))
    })())
}

/// The panel.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let found = targets(app);
    let Some(first) = found.first() else {
        let any = app.session.active().is_some_and(|st| !st.selection.objects.is_empty());
        ui.label(
            egui::RichText::new(if any {
                tl!("The selection has no Tiling. Apply it to repeat the objects in a grid of tiles.")
            } else {
                tl!("Select an object, then apply Tiling to repeat it in a grid of tiles.")
            })
            .color(t.text_dim),
        );
        ui.add_space(8.0);
        if ui.add_enabled_ui(any, |ui| framed_button(ui, tl!("Apply Tiling"), None)).inner.clicked()
            && let Err(e) = app.run("tiling.open", json!({}))
        {
            app.status(e);
        }
        return;
    };
    // The effect's row: its name, show/hide and remove.
    let (visible, params) = (first.visible, first.params.clone());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), Sense::hover());
        crate::icons::paint(ui, "layout-grid", r, t.text);
        ui.label(egui::RichText::new(tl!("Tiling")).strong().color(t.text));
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if widgets::icon_button(ui, "trash-2", tl!("Remove Tiling"), false, 22.0).clicked() {
                let calls: Vec<Value> = found.iter().map(|t| json!({ "ids": [t.id.0], "index": t.index, "item": null })).collect();
                one_step(app, |app| {
                    for c in calls {
                        app.run("effect.remove", c)?;
                    }
                    Ok(())
                });
            }
            let eye = if visible { "eye" } else { "eye-off" };
            if widgets::icon_button(ui, eye, if visible { tl!("Hide Tiling") } else { tl!("Show Tiling") }, false, 22.0).clicked() {
                let calls: Vec<Value> =
                    found.iter().map(|t| json!({ "ids": [t.id.0], "index": t.index, "item": null, "visible": !visible })).collect();
                one_step(app, |app| {
                    for c in calls {
                        app.run("effect.setParams", c)?;
                    }
                    Ok(())
                });
            }
        });
    });
    ui.add_space(4.0);
    let unit = app.session.general_unit();
    if let Some(patch) = controls(ui, &params, unit, "tiling-panel") {
        let found = targets(app);
        set(app, &found, patch);
    }
}

/// The mirror flags of `p` (an older file's `mirror` folded in).
fn mirror_flags(p: &Value) -> [bool; 4] {
    let f = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    let mut out = MIRROR_KEYS.map(f);
    let old = p.get("mirror").and_then(Value::as_f64).unwrap_or(0.0).clamp(0.0, 15.0) as usize;
    let (cols, rows) = (old % 4, old / 4);
    out[0] |= rows & 1 != 0;
    out[1] |= rows & 2 != 0;
    out[2] |= cols & 1 != 0;
    out[3] |= cols & 2 != 0;
    out
}

/// How a Tiling scale or rotation steps: (value, English tooltip), in Inkscape's button order.
const STEPS: [(&str, &str); 5] = [
    ("rows", "Down the rows: the top row as it is, a step more on each row below"),
    ("columns", "Across the columns: the left column as it is, a step more on each column to the right"),
    ("both", "Diagonally: the top-left tile as it is, a step more on each row and column"),
    ("uniform", "Every tile the same"),
    ("random", "Random: each tile its own share (Randomize draws again)"),
];

/// The Tiling controls for values `p` (over the defaults), gaps shown in the effect's own `unit`
/// or else `general`. Returns the values changed this frame.
pub fn controls(ui: &mut Ui, p: &Value, general: Unit, salt: &str) -> Option<Map<String, Value>> {
    const LABEL_W: f32 = 104.0;
    const FIELD_W: f32 = 66.0;
    let t = Tokens::get(ui.ctx());
    let mut patch = Map::new();
    let num = |k: &str, d: f64| p.get(k).and_then(Value::as_f64).filter(|v| v.is_finite()).unwrap_or(d);
    let txt = |k: &str, d: &'static str| p.get(k).and_then(Value::as_str).map(str::to_string).unwrap_or_else(|| d.to_string());
    let on = |k: &str| p.get(k).and_then(Value::as_bool).unwrap_or(false);
    let unit = Unit::named(&txt("unit", "")).unwrap_or(general);
    // Mirroring mode: two rows of eight, as Inkscape's.
    widgets::subheader(ui, "Mirroring mode");
    let flags = mirror_flags(p);
    let current = MIRRORS.iter().position(|m| *m == flags);
    egui::Frame::NONE.stroke(Stroke::new(1.0, t.divider)).corner_radius(4.0).inner_margin(egui::Margin::same(4)).show(ui, |ui| {
        for row in 0..2 {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 3.0;
                for half in 0..2 {
                    if half == 1 {
                        ui.add_space(6.0);
                    }
                    for k in 0..4 {
                        let index = half * 8 + row * 4 + k;
                        let Some(m) = MIRRORS.get(index).copied() else { continue };
                        let tip = mirror_tip(m);
                        if mirror_button(ui, m, current == Some(index)).on_hover_text(tip).clicked() {
                            for (key, v) in MIRROR_KEYS.iter().zip(m) {
                                patch.insert((*key).into(), json!(v));
                            }
                            patch.insert("mirror".into(), json!(0));
                        }
                    }
                }
            });
        }
    });
    ui.add_space(6.0);
    // Rows and Columns.
    ui.horizontal(|ui| {
        for (key, label) in [("rows", "Rows"), ("columns", "Columns")] {
            widgets::dim_label(ui, label);
            if let Some(v) = stepped(ui, (salt, key), num(key, 3.0), 1.0, 1.0..=200.0, &|ui, v| {
                widgets::range_field(ui, (salt, key, "f"), v, 1.0..=200.0, "", 0, 44.0)
            }) {
                patch.insert(key.into(), json!(v));
            }
            ui.add_space(6.0);
        }
    });
    // Gap X (with the unit) and Gap Y, each the same everywhere or random.
    for (key, random_key, label, normal_tip, random_tip) in [
        ("gapX", "gapXRandom", "Gap X", "All horizontal gaps the same width", "Random horizontal gaps (Randomize draws again)"),
        ("gapY", "gapYRandom", "Gap Y", "All vertical gaps the same height", "Random vertical gaps (Randomize draws again)"),
    ] {
        widgets::label_row(ui, label, LABEL_W, |ui| {
            let step = unit.to_pt(1.0);
            if let Some(v) =
                stepped(ui, (salt, key), num(key, 0.0), step, -1e6..=1e6, &|ui, v| widgets::num_field(ui, (salt, key, "f"), Some(v), unit, FIELD_W))
            {
                patch.insert(key.into(), json!(v));
            }
            let random = on(random_key);
            let horizontal = key == "gapX";
            if toggle(ui, !random, normal_tip, &|p, r, c| grid_icon(p, r, c, Icon::Uniform)).clicked() && random {
                patch.insert(random_key.into(), json!(false));
            }
            if toggle(ui, random, random_tip, &move |p, r, c| grid_icon(p, r, c, if horizontal { Icon::GapX } else { Icon::GapY })).clicked()
                && !random
            {
                patch.insert(random_key.into(), json!(true));
            }
            if horizontal {
                ui.add_space(4.0);
                egui::ComboBox::from_id_salt((salt, "unit")).width(52.0).selected_text(unit.suffix()).show_ui(ui, |ui| {
                    for u in Unit::ALL.into_iter().filter(|u| *u != Unit::FeetInches) {
                        if ui.selectable_label(u == unit, u.suffix()).on_hover_text(tl!(u.label())).clicked() {
                            patch.insert("unit".into(), json!(u.suffix()));
                        }
                    }
                });
            }
        });
    }
    // Offset %: every other row or column.
    widgets::label_row(ui, "Offset %", LABEL_W, |ui| {
        if let Some(v) = stepped(ui, (salt, "offset"), num("offset", 0.0), 1.0, -1000.0..=1000.0, &|ui, v| {
            widgets::range_field(ui, (salt, "offset", "f"), v, -1000.0..=1000.0, "", 2, FIELD_W)
        }) {
            patch.insert("offset".into(), json!(v));
        }
        let cols = txt("offsetAxis", "rows") == "columns";
        if toggle(ui, !cols, "Offset every other row", &|p, r, c| grid_icon(p, r, c, Icon::OffsetRows)).clicked() {
            patch.insert("offsetAxis".into(), json!("rows"));
        }
        if toggle(ui, cols, "Offset every other column", &|p, r, c| grid_icon(p, r, c, Icon::OffsetCols)).clicked() {
            patch.insert("offsetAxis".into(), json!("columns"));
        }
    });
    // Scale % and Rotate °, and how each steps.
    for (key, mode_key, label, range, rotation) in
        [("scale", "scaleMode", "Scale %", -100.0..=10_000.0, false), ("rotate", "rotateMode", "Rotate °", -36_000.0..=36_000.0, true)]
    {
        widgets::label_row(ui, label, LABEL_W, |ui| {
            let r = range.clone();
            if let Some(v) = stepped(ui, (salt, key), num(key, 0.0), 1.0, range.clone(), &|ui, v| {
                widgets::range_field(ui, (salt, key, "f"), v, r.clone(), "", 2, FIELD_W)
            }) {
                patch.insert(key.into(), json!(v));
            }
            let mode = txt(mode_key, "rows");
            for (value, tip) in STEPS {
                let icon = match value {
                    "rows" => Icon::StepRows,
                    "columns" => Icon::StepCols,
                    "both" => Icon::StepBoth,
                    "random" => Icon::StepRandom,
                    _ => Icon::Uniform,
                };
                if toggle(ui, mode == value, tip, &move |p, r, c| step_icon(p, r, c, icon, rotation)).clicked() {
                    patch.insert(mode_key.into(), json!(value));
                }
            }
        });
    }
    ui.add_space(4.0);
    // The options, and Randomize beside them.
    ui.horizontal_top(|ui| {
        ui.vertical(|ui| {
            for (key, label, tip) in [
                ("mirrorTransforms", "Mirror transforms", "The turned tiles of every other row, column or square turn the other way"),
                ("minimizeGaps", "Minimize gaps", "Each column and row takes the room its own tiles need, not the largest tile's"),
                ("split", "Split elements", "The tiles become separate objects: each can be selected, coloured and moved on its own"),
                ("linkStyles", "Link styles", "With Split elements, the tiles take the object's colours and style whenever it changes"),
            ] {
                if widgets::check_tip(ui, label, on(key), true, tl!(tip)) {
                    patch.insert(key.into(), json!(!on(key)));
                }
            }
        });
        ui.add_space(8.0);
        if randomize_button(ui).clicked() {
            patch.insert("seed".into(), json!(num("seed", 0.0) + 1.0));
        }
    });
    (!patch.is_empty()).then_some(patch)
}

/// A number with − and + beside it (stepping by `step` within `range`): the field `field` draws.
fn stepped(
    ui: &mut Ui,
    id: impl std::hash::Hash + std::fmt::Debug,
    value: f64,
    step: f64,
    range: std::ops::RangeInclusive<f64>,
    field: &dyn Fn(&mut Ui, f64) -> Option<f64>,
) -> Option<f64> {
    let mut out = field(ui, value);
    ui.spacing_mut().item_spacing.x = 1.0;
    for (glyph, by) in [("−", -step), ("+", step)] {
        let r = ui.push_id((&id, glyph), |ui| step_button(ui, glyph)).inner;
        if r.clicked() {
            out = Some((value + by).clamp(*range.start(), *range.end()));
        }
    }
    ui.spacing_mut().item_spacing.x = 6.0;
    out.map(|v| v.clamp(*range.start(), *range.end()))
}

/// A small − or + button.
fn step_button(ui: &mut Ui, glyph: &str) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(18.0, 24.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(r, 3.0, t.hover);
    }
    ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, glyph, egui::FontId::proportional(15.0), t.text);
    resp
}

/// A toggle drawn by `draw` (in its rect, in the ink colour), filled with the accent when `on`.
fn toggle(ui: &mut Ui, on: bool, tip: &str, draw: &dyn Fn(&Painter, Rect, Color32)) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(26.0, 24.0), Sense::click());
    let fill = if on {
        t.accent
    } else if resp.hovered() {
        t.hover
    } else {
        t.button
    };
    ui.painter().rect_filled(r, 3.0, fill);
    draw(ui.painter(), r.shrink2(vec2(5.0, 4.0)), if on { Color32::WHITE } else { t.text });
    resp.on_hover_text(tl!(tip))
}

/// Draws a small picture centred on a point, in an ink colour.
type Glyph<'a> = &'a dyn Fn(&Painter, egui::Pos2, Color32);

/// A framed button with `label`, after the picture `icon` draws (in its left 30 points) if any.
fn framed_button(ui: &mut Ui, label: &str, icon: Option<Glyph>) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let ink = if ui.is_enabled() { t.text } else { t.text_disabled };
    let galley = ui.painter().layout_no_wrap(label.to_string(), egui::FontId::proportional(12.5), ink);
    let lead = if icon.is_some() { 30.0 } else { 12.0 };
    let (r, resp) = ui.allocate_exact_size(vec2(galley.size().x + lead + 10.0, 26.0), Sense::click());
    ui.painter().rect_filled(r, 4.0, if resp.hovered() && ui.is_enabled() { t.hover } else { t.button });
    ui.painter().rect_stroke(r, 4.0, Stroke::new(1.0, t.button_border), StrokeKind::Inside);
    if let Some(draw) = icon {
        draw(ui.painter(), pos2(r.left() + 15.0, r.center().y), ink);
    }
    ui.painter().galley(pos2(r.left() + lead, r.center().y - galley.size().y / 2.0), galley, ink);
    resp
}

/// Randomize: a die and the word.
fn randomize_button(ui: &mut Ui) -> egui::Response {
    framed_button(ui, tl!("Randomize"), Some(&die)).on_hover_text(tl!("A new random draw for the random scale, rotation and gaps"))
}

/// A die (a cube seen from a corner, with its pips) centred on `c`.
fn die(p: &Painter, c: egui::Pos2, ink: Color32) {
    let s = 7.0;
    let top = [c + vec2(0.0, -s), c + vec2(s * 0.87, -s * 0.5), c, c + vec2(-s * 0.87, -s * 0.5)];
    let left = [c + vec2(-s * 0.87, -s * 0.5), c, c + vec2(0.0, s), c + vec2(-s * 0.87, s * 0.5)];
    let right = [c, c + vec2(s * 0.87, -s * 0.5), c + vec2(s * 0.87, s * 0.5), c + vec2(0.0, s)];
    for face in [top, left, right] {
        p.add(Shape::closed_line(face.to_vec(), Stroke::new(1.1, ink)));
    }
    for q in [c + vec2(0.0, -s * 0.5), c + vec2(-s * 0.45, s * 0.25), c + vec2(s * 0.45, s * 0.25)] {
        p.circle_filled(q, 1.1, ink);
    }
}

/// A mirroring mode's tooltip: which rows and columns flip which way.
fn mirror_tip(m: [bool; 4]) -> String {
    let names = ["Rows flip left to right", "Rows flip top to bottom", "Columns flip left to right", "Columns flip top to bottom"];
    let parts: Vec<&str> = names.iter().zip(m).filter(|(_, on)| *on).map(|(n, _)| tl!(n)).collect();
    if parts.is_empty() { tl!("No mirroring").to_string() } else { parts.join(" · ") }
}

/// A mirroring mode's button: four tiles of a corner triangle, flipped as mode `m` flips them.
fn mirror_button(ui: &mut Ui, m: [bool; 4], on: bool) -> egui::Response {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(28.0, 28.0), Sense::click());
    let fill = if on {
        t.accent
    } else if resp.hovered() {
        t.hover
    } else {
        t.button
    };
    ui.painter().rect_filled(r, 4.0, fill);
    let cell = (r.width() - 8.0) / 2.0;
    let odd = |n: usize| n % 2 == 1;
    for (row, col) in [(0usize, 0usize), (0, 1), (1, 0), (1, 1)] {
        let flip = |by_rows: bool, by_cols: bool| match (by_rows, by_cols) {
            (true, true) => odd(row + col),
            (true, false) => odd(row),
            (false, true) => odd(col),
            _ => false,
        };
        let (fx, fy) = (flip(m[0], m[2]), flip(m[1], m[3]));
        let o = r.min + vec2(4.0 + col as f32 * cell, 4.0 + row as f32 * cell);
        let q = |x: f32, y: f32| o + vec2(if fx { cell - x } else { x }, if fy { cell - y } else { y });
        let ink = if (row + col) % 2 == 0 { Color32::WHITE } else { Color32::from_rgb(0xE0, 0x45, 0x7B) };
        ui.painter().add(Shape::convex_polygon(vec![q(1.0, 1.0), q(cell - 1.0, 1.0), q(1.0, cell - 1.0)], ink, Stroke::NONE));
    }
    resp
}

/// The little pictures on the toggles: a 3 × 3 grid of squares, varied as the option varies them.
#[derive(Clone, Copy, PartialEq)]
enum Icon {
    /// Every square the same.
    Uniform,
    /// Random gaps between the columns...
    GapX,
    /// ...or the rows.
    GapY,
    /// The middle row slides sideways...
    OffsetRows,
    /// ...or the middle column down.
    OffsetCols,
    /// Growing down the rows.
    StepRows,
    /// Growing across the columns.
    StepCols,
    /// Growing diagonally.
    StepBoth,
    /// Random sizes.
    StepRandom,
}

/// A small, fixed "random" share for square `k` of an icon.
fn jitter(k: usize) -> f32 {
    [0.55, 0.1, 0.85, 0.35, 0.7, 0.2, 0.95, 0.45, 0.05].get(k).copied().unwrap_or(0.5)
}

/// Draw `icon` as a 3 × 3 grid in `r`.
fn grid_icon(p: &Painter, r: Rect, c: Color32, icon: Icon) {
    let side = r.width().min(r.height());
    let o = r.center() - vec2(side, side) / 2.0;
    let pitch = side / 3.0;
    let sq = pitch * 0.72;
    for i in 0..3 {
        for j in 0..3 {
            let k = i * 3 + j;
            let mut at = o + vec2(pitch * (j as f32 + 0.5), pitch * (i as f32 + 0.5));
            match icon {
                Icon::GapX if j > 0 => at.x -= pitch * 0.3 * jitter(k),
                Icon::GapY if i > 0 => at.y -= pitch * 0.3 * jitter(k),
                Icon::OffsetRows if i == 1 => at.x += pitch * 0.35,
                Icon::OffsetCols if j == 1 => at.y += pitch * 0.35,
                _ => {}
            }
            p.rect_filled(Rect::from_center_size(at, vec2(sq, sq)), 0.5, c);
        }
    }
}

/// Draw a scale (`rotation` false) or rotation step `icon`: squares that grow, or turn, as it says.
fn step_icon(p: &Painter, r: Rect, c: Color32, icon: Icon, rotation: bool) {
    let side = r.width().min(r.height());
    let o = r.center() - vec2(side, side) / 2.0;
    let pitch = side / 3.0;
    for i in 0..3 {
        for j in 0..3 {
            let k = i * 3 + j;
            let share = match icon {
                Icon::StepRows => i as f32 / 2.0,
                Icon::StepCols => j as f32 / 2.0,
                Icon::StepBoth => (i + j) as f32 / 4.0,
                Icon::StepRandom => jitter(k),
                _ => 0.0,
            };
            let at = o + vec2(pitch * (j as f32 + 0.5), pitch * (i as f32 + 0.5));
            if rotation {
                // A short bar per tile, turned as the tile turns (all alike for Uniform).
                let a = if icon == Icon::Uniform { -std::f32::consts::FRAC_PI_6 } else { -share * std::f32::consts::FRAC_PI_2 };
                let d = vec2(a.cos(), a.sin()) * pitch * 0.42;
                p.line_segment([at - d, at + d], Stroke::new(1.6, c));
            } else {
                let s = pitch * (0.35 + 0.55 * if icon == Icon::Uniform { 0.75 } else { share });
                p.rect_filled(Rect::from_center_size(at, vec2(s, s)), 0.5, c);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> VectorcraftApp {
        let mut a = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        a.run("file.new", json!({"width": 400, "height": 400})).unwrap();
        a
    }

    #[test]
    fn opening_applies_tiling_once_and_shows_the_panel() {
        let mut a = app();
        a.run("shape.rectangle", json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
        assert_eq!(a.run("tiling.open", json!({})).unwrap()["applied"].as_array().map(Vec::len), Some(1));
        assert_eq!(targets(&a).len(), 1);
        assert_eq!(crate::menus::checked(&a, "window.panel", &json!({ "panel": ID })), Some(true));
        // Again: nothing more applied, the panel stays.
        assert_eq!(a.run("tiling.open", json!({})).unwrap()["applied"], json!([]));
        assert_eq!(crate::menus::checked(&a, "window.panel", &json!({ "panel": ID })), Some(true));
        // A change from the panel is one undo step on the effect.
        let found = targets(&a);
        let mut patch = Map::new();
        patch.insert("rows".into(), json!(5));
        patch.insert("mirrorRowsX".into(), json!(true));
        set(&mut a, &found, patch);
        let p = &targets(&a)[0].params;
        assert_eq!((p["rows"].as_f64(), p["mirrorRowsX"].as_bool()), (Some(5.0), Some(true)));
        a.run("edit.undo", json!({})).unwrap();
        assert_eq!(targets(&a)[0].params["rows"].as_f64(), Some(3.0));
    }

    #[test]
    fn the_mirror_table_and_an_older_files_mirror() {
        // Every mode once, the first none.
        let mut seen = MIRRORS.to_vec();
        seen.sort();
        seen.dedup();
        assert_eq!(seen.len(), 16);
        assert_eq!(MIRRORS[0], [false; 4]);
        // An older `mirror: 1` flipped every other column left to right.
        assert_eq!(mirror_flags(&json!({"mirror": 1})), [false, false, true, false]);
        assert_eq!(mirror_flags(&json!({"mirror": 6, "mirrorRowsY": true})), [true, true, false, true]);
    }
}
