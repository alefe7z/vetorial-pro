//! Vetorial: the rest of the Color Gear tab, under the harmony wheel (scroll down for it):
//! - Color picker, three ways: Disc (a hue ring around a saturation/brightness disc), Classic
//!   (a saturation/brightness square and a hue strip) and Values (H, S, B, R, G, B sliders and the
//!   hexadecimal code). The picked colour paints the selection's fill or stroke, or becomes the
//!   harmony's base.
//! - History: the colours Color Gear painted with, newest first (Clear empties it).
//! - Palette from Image: the main colours of the last picture.
//! - Palettes: the saved palettes, Compact (a row of colours each) or as Cards (a palette's
//!   colours as named cards); + saves the harmony's colours as a new palette.
//!
//! A click on any colour paints the fill, a right click the stroke.
//! Commands: `colorGear.current`, `colorGear.use`, `colorGear.history`, `colorGear.view`.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use super::color_gear::{hsv, to_hsv};
use crate::VectorcraftApp;
use crate::theme::Tokens;

/// How many colours the history keeps.
const HISTORY: usize = 16;

/// The colour picker's three ways.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Picker {
    #[default]
    Disc,
    Classic,
    Values,
}

impl Picker {
    const ALL: [Picker; 3] = [Picker::Disc, Picker::Classic, Picker::Values];
    fn id(self) -> &'static str {
        match self {
            Picker::Disc => "disc",
            Picker::Classic => "classic",
            Picker::Values => "values",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Picker::Disc => "Disc",
            Picker::Classic => "Classic",
            Picker::Values => "Values",
        }
    }
}

/// The state of these sections, kept with the Color Gear tab's.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct More {
    pub picker: Picker,
    /// The picked colour, as hue (degrees), saturation and brightness (0 to 1): kept as HSB so a
    /// grey keeps its hue while the sliders move.
    pub hsb: [f32; 3],
    /// The colours painted with, newest first.
    pub history: Vec<[u8; 3]>,
    /// Palettes as Cards (else Compact).
    pub cards: bool,
    /// The palette the Cards show (a library id; empty: the first saved one).
    pub card_palette: String,
}

impl Default for More {
    fn default() -> Self {
        More { picker: Picker::Disc, hsb: [300.0, 0.6, 0.9], history: vec![], cards: false, card_palette: String::new() }
    }
}

impl More {
    pub fn rgb(&self) -> [f32; 3] {
        hsv(self.hsb[0], self.hsb[1], self.hsb[2])
    }
}

fn to8(c: [f32; 3]) -> [u8; 3] {
    c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8)
}

fn hex8(c: [u8; 3]) -> String {
    format!("#{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn c32(c: [u8; 3]) -> Color32 {
    Color32::from_rgb(c[0], c[1], c[2])
}

fn parse(s: &str) -> Option<[u8; 3]> {
    let h = s.trim().trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    (h.len() == 6).then_some(())?;
    Some([byte(0)?, byte(2)?, byte(4)?])
}

/// Push `c` to the front of the history (once).
pub fn remember(more: &mut More, c: [u8; 3]) {
    more.history.retain(|h| *h != c);
    more.history.insert(0, c);
    more.history.truncate(HISTORY);
}

/// A colour's English name: its hue's name, "Dark" or "Light" before it when it is so; greys by
/// lightness.
pub fn color_name(c: [u8; 3]) -> String {
    let (h, s, v) = to_hsv(c.map(|x| f32::from(x) / 255.0));
    if v < 0.12 {
        return "Black".into();
    }
    if s < 0.12 {
        return if v > 0.92 {
            "White".into()
        } else if v > 0.6 {
            "Light Gray".into()
        } else if v > 0.3 {
            "Gray".into()
        } else {
            "Dark Gray".into()
        };
    }
    const HUES: &[(f32, &str)] = &[
        (12.0, "Red"),
        (40.0, "Orange"),
        (65.0, "Yellow"),
        (95.0, "Lime"),
        (150.0, "Green"),
        (175.0, "Teal"),
        (195.0, "Cyan"),
        (215.0, "Cyan Blue"),
        (245.0, "Blue"),
        (265.0, "Indigo"),
        (290.0, "Purple"),
        (320.0, "Magenta"),
        (345.0, "Pink"),
        (361.0, "Red"),
    ];
    let base = HUES.iter().find(|(top, _)| h < *top).map_or("Red", |(_, n)| *n);
    if v < 0.5 {
        format!("Dark {base}")
    } else if s < 0.45 && v > 0.75 {
        format!("Light {base}")
    } else {
        base.to_string()
    }
}

/// [`color_name`] in the interface's language.
fn shown_name(c: [u8; 3]) -> String {
    let n = color_name(c);
    if let Some(base) = n.strip_prefix("Dark ").filter(|b| *b != "Gray") {
        tl!("Dark {color}").replace("{color}", tl!(base))
    } else if let Some(base) = n.strip_prefix("Light ").filter(|b| *b != "Gray") {
        tl!("Light {color}").replace("{color}", tl!(base))
    } else {
        tl!(&n).to_string()
    }
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let r = match id {
        "colorGear.current" => (|| {
            let m = &mut app.ui.color_gear_more;
            if let Some(c) = p.get("color") {
                let c = c.as_str().and_then(parse).ok_or("color is #rrggbb")?;
                let (h, s, v) = to_hsv(c.map(|x| f32::from(x) / 255.0));
                // A grey keeps the hue it had.
                m.hsb = [if s > 0.0 { h } else { m.hsb[0] }, s, v];
            }
            for (i, key) in ["hue", "saturation", "brightness"].into_iter().enumerate() {
                if let Some(v) = p.get(key) {
                    let hi = if i == 0 { 360.0 } else { 1.0 };
                    let v = v.as_f64().map(|x| x as f32).filter(|x| x.is_finite() && (0.0..=hi).contains(x)).ok_or(format!("{key} is 0 to {hi}"))?;
                    if let Some(slot) = m.hsb.get_mut(i) {
                        *slot = v;
                    }
                }
            }
            Ok(json!({ "color": hex8(to8(m.rgb())), "hsb": m.hsb }))
        })(),
        "colorGear.use" => (|| {
            let c = match p.get("color") {
                Some(c) => c.as_str().and_then(parse).ok_or("color is #rrggbb")?,
                None => to8(app.ui.color_gear_more.rgb()),
            };
            if p.get("base").and_then(Value::as_bool).unwrap_or(false) {
                app.run("colorGear.set", json!({ "color": hex8(c) }))?;
            } else {
                let stroke = p.get("stroke").and_then(Value::as_bool).unwrap_or(false);
                app.run(if stroke { "paint.setStroke" } else { "paint.setFill" }, json!({ "color": hex8(c) }))?;
            }
            remember(&mut app.ui.color_gear_more, c);
            Ok(json!({ "color": hex8(c) }))
        })(),
        "colorGear.history" => {
            if p.get("clear").and_then(Value::as_bool).unwrap_or(false) {
                app.ui.color_gear_more.history.clear();
            }
            Ok(json!({ "history": app.ui.color_gear_more.history.iter().map(|c| hex8(*c)).collect::<Vec<_>>() }))
        }
        "colorGear.view" => (|| {
            let m = &mut app.ui.color_gear_more;
            if let Some(v) = p.get("picker") {
                let v = v.as_str().ok_or("picker is disc, classic or values")?;
                m.picker = Picker::ALL.into_iter().find(|x| x.id() == v).ok_or(format!("picker is disc, classic or values, not `{v}`"))?;
            }
            if let Some(c) = p.get("cards") {
                m.cards = c.as_bool().ok_or("cards is true or false")?;
            }
            if let Some(l) = p.get("palette") {
                m.card_palette = l.as_str().ok_or("palette is a library id")?.to_string();
            }
            Ok(json!({ "picker": m.picker.id(), "cards": m.cards, "palette": m.card_palette }))
        })(),
        _ => return None,
    };
    Some(r)
}

fn run(app: &mut VectorcraftApp, id: &str, p: Value) {
    if let Err(e) = app.run(id, p) {
        app.ui.status = e;
    }
}

/// A clickable colour: a click paints the fill, a right click the stroke.
fn chip(app: &mut VectorcraftApp, ui: &mut Ui, c: [u8; 3], size: egui::Vec2, round: f32) {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(size, Sense::click());
    ui.painter().rect_filled(r, round, c32(c));
    if resp.hovered() {
        ui.painter().rect_stroke(r, round, Stroke::new(1.5, t.text_strong), egui::StrokeKind::Inside);
    }
    let resp = resp.on_hover_text(format!("{} · {}\n{}", shown_name(c), hex8(c), tl!("Click: fill · Right click: stroke")));
    let stroke = if resp.clicked() {
        Some(false)
    } else if resp.secondary_clicked() {
        Some(true)
    } else {
        None
    };
    if let Some(stroke) = stroke
        && app.session.active().is_some()
    {
        run(app, "colorGear.use", json!({ "color": hex8(c), "stroke": stroke }));
    }
}

/// A row (wrapping) of clickable colours.
fn chips(app: &mut VectorcraftApp, ui: &mut Ui, colors: &[[u8; 3]], width: f32) {
    let per = 8.0;
    let cell = ((width - (per - 1.0) * 3.0) / per).floor().max(12.0);
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(3.0, 3.0);
        for c in colors {
            chip(app, ui, *c, vec2(cell, cell * 0.8), 3.0);
        }
    });
}

/// A section's title, with an optional action on the right.
fn title(ui: &mut Ui, text: &str, action: Option<&str>) -> bool {
    let t = Tokens::get(ui.ctx());
    ui.add_space(10.0);
    let mut clicked = false;
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(text).strong().color(t.text_strong));
        if let Some(a) = action {
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                clicked = ui.small_button(a).clicked();
            });
        }
    });
    ui.add_space(2.0);
    clicked
}

/// The sections, under the harmony wheel.
pub fn show(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    ui.add_space(12.0);
    ui.separator();
    // The picker, its three ways as tabs.
    title(ui, tl!("Color Picker"), None);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for p in Picker::ALL {
            let on = app.ui.color_gear_more.picker == p;
            let text = egui::RichText::new(tl!(p.label())).size(12.0).color(if on { t.text_strong } else { t.text_dim });
            if ui.add(egui::Button::new(text).fill(if on { t.panel_darker } else { t.input }).min_size(vec2(width / 3.0, 24.0))).clicked() {
                run(app, "colorGear.view", json!({ "picker": p.id() }));
            }
        }
    });
    ui.add_space(6.0);
    match app.ui.color_gear_more.picker {
        Picker::Disc => disc(app, ui, width),
        Picker::Classic => classic(app, ui, width),
        Picker::Values => values(app, ui, width),
    }
    // The picked colour and what to do with it.
    ui.add_space(6.0);
    let cur = to8(app.ui.color_gear_more.rgb());
    ui.horizontal(|ui| {
        let (r, _) = ui.allocate_exact_size(vec2(40.0, 28.0), Sense::hover());
        ui.painter().rect_filled(r, 6.0, c32(cur));
        ui.painter().rect_stroke(r, 6.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        ui.vertical(|ui| {
            ui.label(egui::RichText::new(shown_name(cur)).color(t.text_strong));
            ui.label(egui::RichText::new(hex8(cur)).size(11.0).color(t.text_dim));
        });
    });
    ui.horizontal_wrapped(|ui| {
        let on_doc = app.session.active().is_some();
        if ui.add_enabled(on_doc, egui::Button::new(tl!("Fill"))).clicked() {
            run(app, "colorGear.use", json!({}));
        }
        if ui.add_enabled(on_doc, egui::Button::new(tl!("Stroke"))).clicked() {
            run(app, "colorGear.use", json!({ "stroke": true }));
        }
        if ui.button(tl!("Use in the Harmony")).clicked() {
            run(app, "colorGear.use", json!({ "base": true }));
        }
    });
    // History.
    if title(ui, tl!("History"), Some(tl!("Clear"))) {
        run(app, "colorGear.history", json!({ "clear": true }));
    }
    let history = app.ui.color_gear_more.history.clone();
    if history.is_empty() {
        ui.label(egui::RichText::new(tl!("The colours you paint with appear here")).size(11.0).color(t.text_dim));
    } else {
        chips(app, ui, &history, width);
    }
    // Palette from the last picture.
    if title(ui, tl!("Palette from Image"), Some(tl!("Choose…"))) {
        run(app, "colorGear.image", json!({}));
    }
    let image = app.ui.color_gear.image.clone();
    if image.is_empty() {
        ui.label(egui::RichText::new(tl!("Choose a picture to take its colours")).size(11.0).color(t.text_dim));
    } else {
        chips(app, ui, &image, width);
    }
    palettes(app, ui, width);
}

/// The hue ring around a saturation (across) and brightness (up) disc.
fn disc(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    let side = width.min(240.0);
    let (outer, resp) = ui.allocate_exact_size(vec2(width, side), Sense::click_and_drag());
    let c = outer.center();
    let r_out = side / 2.0 - 4.0;
    let r_in = r_out * 0.78;
    let r_disc = r_in - 8.0;
    let [h, s, v] = app.ui.color_gear_more.hsb;
    // The ring: every hue, red on the right going round anticlockwise.
    let mut mesh = egui::Mesh::default();
    const N: u32 = 96;
    for i in 0..=N {
        let a = i as f32 / N as f32 * std::f32::consts::TAU;
        let col = c32(to8(hsv(a.to_degrees(), 1.0, 1.0)));
        let d = vec2(a.cos(), -a.sin());
        mesh.colored_vertex(c + d * r_out, col);
        mesh.colored_vertex(c + d * r_in, col);
    }
    for i in 0..N {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    // The disc: saturation across, brightness up, at this hue (rings of vertices, so its edge is
    // round).
    const RINGS: u32 = 16;
    const SPOKES: u32 = 72;
    let colour_at = |p: Pos2| {
        let (x, y) = (((p.x - c.x) / r_disc + 1.0) / 2.0, ((p.y - c.y) / r_disc + 1.0) / 2.0);
        c32(to8(hsv(h, x.clamp(0.0, 1.0), 1.0 - y.clamp(0.0, 1.0))))
    };
    let centre = mesh.vertices.len() as u32;
    mesh.colored_vertex(c, colour_at(c));
    for k in 1..=RINGS {
        let rr = r_disc * k as f32 / RINGS as f32;
        for j in 0..SPOKES {
            let a = j as f32 / SPOKES as f32 * std::f32::consts::TAU;
            let p = c + vec2(a.cos(), a.sin()) * rr;
            mesh.colored_vertex(p, colour_at(p));
        }
    }
    let at = |k: u32, j: u32| centre + 1 + (k - 1) * SPOKES + j % SPOKES;
    for j in 0..SPOKES {
        mesh.add_triangle(centre, at(1, j), at(1, j + 1));
    }
    for k in 1..RINGS {
        for j in 0..SPOKES {
            mesh.add_triangle(at(k, j), at(k + 1, j), at(k + 1, j + 1));
            mesh.add_triangle(at(k, j), at(k + 1, j + 1), at(k, j + 1));
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().circle_stroke(c, r_disc, Stroke::new(1.0, t.border));
    // The markers: the hue on the ring, the colour in the disc.
    let ha = h.to_radians();
    let ring_at = c + vec2(ha.cos(), -ha.sin()) * (r_in + r_out) / 2.0;
    let disc_at = c + vec2((s * 2.0 - 1.0) * r_disc, ((1.0 - v) * 2.0 - 1.0) * r_disc);
    ui.painter().circle_stroke(ring_at, (r_out - r_in) / 2.0 + 2.0, Stroke::new(2.5, Color32::WHITE));
    ui.painter().circle(disc_at, 7.0, c32(to8(hsv(h, s, v))), Stroke::new(2.0, Color32::WHITE));
    // A press on the ring turns the hue; in the disc, it sets saturation and brightness.
    let mode_id = ui.id().with("cg-disc-mode");
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        ui.data_mut(|d| d.insert_temp(mode_id, p.distance(c) > r_in - 4.0));
    }
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let ring = if resp.dragged() { ui.data(|d| d.get_temp(mode_id)).unwrap_or(false) } else { p.distance(c) > r_in - 4.0 };
        let d = p - c;
        if ring {
            let hue = (-d.y).atan2(d.x).to_degrees().rem_euclid(360.0);
            run(app, "colorGear.current", json!({ "hue": hue }));
        } else {
            let sx = ((d.x / r_disc + 1.0) / 2.0).clamp(0.0, 1.0);
            let by = 1.0 - ((d.y / r_disc + 1.0) / 2.0).clamp(0.0, 1.0);
            run(app, "colorGear.current", json!({ "saturation": sx, "brightness": by }));
        }
    }
}

/// A saturation (across) and brightness (up) square, and a hue strip under it.
fn classic(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    let [h, s, v] = app.ui.color_gear_more.hsb;
    let (sq, resp) = ui.allocate_exact_size(vec2(width, (width * 0.7).min(200.0)), Sense::click_and_drag());
    let mut mesh = egui::Mesh::default();
    const G: u32 = 16;
    for j in 0..=G {
        for i in 0..=G {
            let (x, y) = (i as f32 / G as f32, j as f32 / G as f32);
            mesh.colored_vertex(pos2(sq.left() + x * sq.width(), sq.top() + y * sq.height()), c32(to8(hsv(h, x, 1.0 - y))));
        }
    }
    for j in 0..G {
        for i in 0..G {
            let a = j * (G + 1) + i;
            mesh.add_triangle(a, a + 1, a + G + 2);
            mesh.add_triangle(a, a + G + 2, a + G + 1);
        }
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    ui.painter().rect_stroke(sq, 4.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let at = pos2(sq.left() + s * sq.width(), sq.top() + (1.0 - v) * sq.height());
    ui.painter().circle(at, 7.0, c32(to8(hsv(h, s, v))), Stroke::new(2.0, Color32::WHITE));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let sx = ((p.x - sq.left()) / sq.width()).clamp(0.0, 1.0);
        let by = 1.0 - ((p.y - sq.top()) / sq.height()).clamp(0.0, 1.0);
        run(app, "colorGear.current", json!({ "saturation": sx, "brightness": by }));
    }
    ui.add_space(6.0);
    if let Some(x) = strip(ui, width, h / 360.0, |x| hsv(x * 360.0, 1.0, 1.0)) {
        run(app, "colorGear.current", json!({ "hue": x * 360.0 }));
    }
}

/// A slider whose track shows `color(x)` from 0 to 1: the new value while dragged.
fn strip(ui: &mut Ui, width: f32, value: f32, color: impl Fn(f32) -> [f32; 3]) -> Option<f32> {
    let (r, resp) = ui.allocate_exact_size(vec2(width, 18.0), Sense::click_and_drag());
    let bar = Rect::from_center_size(r.center(), vec2(r.width() - 14.0, 8.0));
    let mut mesh = egui::Mesh::default();
    const N: u32 = 24;
    for i in 0..=N {
        let x = i as f32 / N as f32;
        let col = c32(to8(color(x)));
        let px = bar.left() + x * bar.width();
        mesh.colored_vertex(pos2(px, bar.top()), col);
        mesh.colored_vertex(pos2(px, bar.bottom()), col);
    }
    for i in 0..N {
        let k = i * 2;
        mesh.add_triangle(k, k + 1, k + 2);
        mesh.add_triangle(k + 1, k + 3, k + 2);
    }
    ui.painter().add(egui::Shape::mesh(mesh));
    let knob: Pos2 = pos2(bar.left() + bar.width() * value.clamp(0.0, 1.0), bar.center().y);
    ui.painter().circle(knob, 7.0, Color32::WHITE, Stroke::new(1.0, Color32::from_black_alpha(140)));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        return Some(((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0));
    }
    None
}

/// H, S, B and R, G, B sliders with their numbers, and the hexadecimal code.
fn values(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    let [h, s, v] = app.ui.color_gear_more.hsb;
    let rgb = app.ui.color_gear_more.rgb();
    let track = (width - 70.0).max(80.0);
    let row = |ui: &mut Ui, label: &str, f: &mut dyn FnMut(&mut Ui)| {
        ui.horizontal(|ui| {
            ui.add_sized(vec2(14.0, 18.0), egui::Label::new(egui::RichText::new(label).color(t.text_dim)));
            f(ui);
        });
    };
    let mut set: Option<Value> = None;
    row(ui, "H", &mut |ui| {
        if let Some(x) = strip(ui, track, h / 360.0, |x| hsv(x * 360.0, 1.0, 1.0)) {
            set = Some(json!({ "hue": x * 360.0 }));
        }
        let mut n = h.round();
        if ui.add(egui::DragValue::new(&mut n).range(0.0..=360.0)).changed() {
            set = Some(json!({ "hue": n }));
        }
    });
    row(ui, "S", &mut |ui| {
        if let Some(x) = strip(ui, track, s, |x| hsv(h, x, v.max(0.2))) {
            set = Some(json!({ "saturation": x }));
        }
        let mut n = (s * 100.0).round();
        if ui.add(egui::DragValue::new(&mut n).range(0.0..=100.0).suffix("%")).changed() {
            set = Some(json!({ "saturation": n / 100.0 }));
        }
    });
    row(ui, "B", &mut |ui| {
        if let Some(x) = strip(ui, track, v, |x| hsv(h, s, x)) {
            set = Some(json!({ "brightness": x }));
        }
        let mut n = (v * 100.0).round();
        if ui.add(egui::DragValue::new(&mut n).range(0.0..=100.0).suffix("%")).changed() {
            set = Some(json!({ "brightness": n / 100.0 }));
        }
    });
    let c8 = to8(rgb);
    for (k, label) in ["R", "G", "B"].into_iter().enumerate() {
        row(ui, label, &mut |ui| {
            let with = |x: f32| {
                let mut c = rgb;
                if let Some(slot) = c.get_mut(k) {
                    *slot = x;
                }
                c
            };
            let cur = rgb.get(k).copied().unwrap_or(0.0);
            let mut next: Option<[u8; 3]> = None;
            if let Some(x) = strip(ui, track, cur, with) {
                next = Some(to8(with(x)));
            }
            let mut n = c8.get(k).copied().unwrap_or(0);
            if ui.add(egui::DragValue::new(&mut n).range(0..=255)).changed() {
                let mut c = c8;
                if let Some(slot) = c.get_mut(k) {
                    *slot = n;
                }
                next = Some(c);
            }
            if let Some(c) = next {
                set = Some(json!({ "color": hex8(c) }));
            }
        });
    }
    ui.add_space(4.0);
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Hexadecimal")).color(t.text_dim));
        let id = ui.id().with("cg-more-hex");
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| hex8(c8));
        let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(90.0));
        if resp.changed() {
            if parse(&text).is_some() {
                set = Some(json!({ "color": text.trim() }));
            }
            ui.data_mut(|d| d.insert_temp(id, text));
        } else if !resp.has_focus() {
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
    if let Some(p) = set {
        run(app, "colorGear.current", p);
    }
}

/// The saved palettes, Compact or as Cards.
fn palettes(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    if title(ui, tl!("Palettes"), Some("+")) {
        match app.run("colorGear.save", json!({})) {
            Ok(r) => app.ui.status = format!("{} {}", tl!("Saved palette"), r["name"].as_str().unwrap_or_default()),
            Err(e) => app.ui.status = e,
        }
    }
    let cards = app.ui.color_gear_more.cards;
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for (on, label, value) in [(!cards, tl!("Compact"), false), (cards, tl!("Cards"), true)] {
            let text = egui::RichText::new(label).size(12.0).color(if on { Color32::WHITE } else { t.text_dim });
            if ui.add(egui::Button::new(text).fill(if on { t.accent } else { t.input }).min_size(vec2(width / 2.0, 24.0))).clicked() {
                run(app, "colorGear.view", json!({ "cards": value }));
            }
        }
    });
    ui.add_space(4.0);
    let libs: Vec<(String, String, Vec<[u8; 3]>)> = vectorcraft_engine::cmd::swatchlib::libraries(&app.session)
        .into_iter()
        .filter(|l| l.category == "user")
        .filter_map(|l| {
            let (_, lib) = vectorcraft_engine::cmd::swatchlib::library(&app.session, &l.id)?;
            let colors: Vec<[u8; 3]> = lib
                .swatches
                .iter()
                .chain(lib.groups.iter().flat_map(|g| g.swatches.iter()))
                .filter_map(|s| s.paint.color())
                .map(|c| to8(c.to_rgb()))
                .take(64)
                .collect();
            Some((l.id.clone(), l.name.clone(), colors))
        })
        .collect();
    if libs.is_empty() {
        ui.label(egui::RichText::new(tl!("Save a harmony with + or Save to make your first palette")).size(11.0).color(t.text_dim));
        return;
    }
    if !cards {
        for (id, name, colors) in &libs {
            egui::Frame::NONE.fill(t.panel_darker).corner_radius(6.0).inner_margin(6.0).show(ui, |ui| {
                ui.set_width(width - 12.0);
                ui.horizontal(|ui| {
                    ui.label(egui::RichText::new(name).strong());
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        let resp = ui.small_button("…");
                        egui::Popup::menu(&resp).show(|ui| {
                            if ui.button(tl!("Show as a Palette Bar")).clicked() {
                                run(app, "window.colorPalette", json!({ "library": id, "show": true }));
                                ui.close();
                            }
                            if ui.button(tl!("Show as Cards")).clicked() {
                                run(app, "colorGear.view", json!({ "cards": true, "palette": id }));
                                ui.close();
                            }
                        });
                    });
                });
                chips(app, ui, colors, width - 12.0);
            });
            ui.add_space(4.0);
        }
        return;
    }
    // Cards: one palette's colours, named.
    let chosen = libs.iter().find(|l| l.0 == app.ui.color_gear_more.card_palette).or_else(|| libs.first());
    let Some((cid, cname, colors)) = chosen.cloned() else { return };
    egui::ComboBox::from_id_salt("cg-card-palette").selected_text(cname).width(width - 8.0).show_ui(ui, |ui| {
        for (id, name, _) in &libs {
            if ui.selectable_label(*id == cid, name).clicked() {
                run(app, "colorGear.view", json!({ "palette": id }));
            }
        }
    });
    ui.add_space(4.0);
    let cols = 3.0;
    let w = ((width - (cols - 1.0) * 4.0) / cols).floor();
    ui.horizontal_wrapped(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        for c in colors {
            let before = ui.cursor().min;
            chip(app, ui, c, vec2(w, w * 0.62), 4.0);
            let r = Rect::from_min_size(before, vec2(w, w * 0.62));
            let (_, _, lum) = to_hsv(c.map(|x| f32::from(x) / 255.0));
            let dark_text = 0.299 * f32::from(c[0]) + 0.587 * f32::from(c[1]) + 0.114 * f32::from(c[2]) > 150.0 && lum > 0.0;
            let ink = if dark_text { Color32::from_gray(20) } else { Color32::WHITE };
            ui.painter().text(r.left_bottom() + vec2(6.0, -6.0), egui::Align2::LEFT_BOTTOM, shown_name(c), egui::FontId::proportional(10.5), ink);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> VectorcraftApp {
        VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default())
    }

    #[test]
    fn colours_get_names() {
        assert_eq!(color_name([0, 0, 0]), "Black");
        assert_eq!(color_name([255, 255, 255]), "White");
        assert_eq!(color_name([128, 128, 128]), "Gray");
        assert_eq!(color_name([255, 0, 0]), "Red");
        assert_eq!(color_name([255, 0, 255]), "Magenta");
        assert_eq!(color_name([40, 10, 120]), "Dark Indigo");
        assert_eq!(color_name([30, 140, 230]), "Cyan Blue");
        assert_eq!(color_name([250, 180, 240]), "Light Magenta");
    }

    #[test]
    fn the_picker_paints_and_keeps_a_history() {
        let mut a = app();
        a.run("file.new", json!({"width": 100, "height": 100})).unwrap();
        a.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        assert_eq!(a.run("colorGear.current", json!({"color": "#ff0000"})).unwrap()["color"], "#FF0000");
        assert_eq!(a.run("colorGear.current", json!({"hue": 120})).unwrap()["color"], "#00FF00");
        a.run("colorGear.use", json!({})).unwrap();
        let fill = super::super::active_paint(&a).color().unwrap().to_rgba8(1.0);
        assert_eq!(&fill[..3], &[0, 255, 0]);
        a.run("colorGear.use", json!({"color": "#123456", "stroke": true})).unwrap();
        assert_eq!(a.run("colorGear.history", json!({})).unwrap()["history"], json!(["#123456", "#00FF00"]));
        // Again: it moves to the front, once.
        a.run("colorGear.use", json!({"color": "#00ff00"})).unwrap();
        assert_eq!(a.run("colorGear.history", json!({})).unwrap()["history"], json!(["#00FF00", "#123456"]));
        assert_eq!(a.run("colorGear.history", json!({"clear": true})).unwrap()["history"], json!([]));
        // As the harmony's base.
        a.run("colorGear.use", json!({"color": "#0000ff", "base": true})).unwrap();
        assert_eq!(a.run("colorGear.set", json!({"wheel": "rgb"})).unwrap()["colors"][0], "#0000FF");
        assert!(a.run("colorGear.current", json!({"saturation": 3})).is_err());
        assert!(a.run("colorGear.view", json!({"picker": "spiral"})).is_err());
        assert_eq!(a.run("colorGear.view", json!({"picker": "values", "cards": true})).unwrap()["picker"], "values");
    }

    #[test]
    fn every_way_draws() {
        let mut a = app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        a.ui.color_gear_more.history = vec![[10, 20, 30], [200, 100, 50]];
        for (p, cards) in [(Picker::Disc, false), (Picker::Classic, true), (Picker::Values, false)] {
            a.ui.color_gear_more.picker = p;
            a.ui.color_gear_more.cards = cards;
            ctx.run_ui(Default::default(), |ui| show(&mut a, ui, 280.0)).textures_delta.clear();
        }
    }
}
