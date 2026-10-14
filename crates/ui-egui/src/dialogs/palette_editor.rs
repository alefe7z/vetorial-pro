//! Palette Editor (Window → Color Palettes → Palette Editor…), as CorelDRAW's: pick a palette, click
//! a colour to edit its name and values (CMYK or RGB), add, change and delete colours; make, rename
//! and delete palettes. Changes apply at once: the document palette through the swatch commands
//! (undoable), User Defined palettes through `swatch.library.edit` (saved to their files). Built-in
//! and loaded palettes are read only: Make Editable Copy copies one into the user library folder.
//!
//! Fields: `library` (the palette's key: a library id or `document`), `selected` (a colour's name),
//! `name` (the colour's name being edited), `model` (`cmyk` or `rgb`), `c` `m` `y` `k` (percent),
//! `r` `g` `b` (0..255), `paletteName` (Rename's).

use serde_json::{Value, json};
use vectorcraft_color::Color;
use vectorcraft_engine::cmd::swatchlib;

use super::DialogSpec;
use crate::palette_bars::{self, DOCUMENT};
use crate::state::Dialog;
use crate::{VectorcraftApp, widgets};

pub const KIND: &str = "paletteEditor";

pub(super) const SPEC: DialogSpec = DialogSpec {
    heading: |_| tl!("Palette Editor").into(),
    body,
    confirm: |app, _| close(app),
    ok: None,
    min_width: 460.0,
    max_width: Some(560.0),
    ..DialogSpec::FORM
};

fn close(app: &mut VectorcraftApp) -> Result<Value, String> {
    app.ui.dialog = None;
    Ok(Value::Null)
}

/// Open the editor on palette `key` (default: the first bar's, else the document palette).
pub fn open(app: &mut VectorcraftApp, key: Option<&str>) {
    let key = key.map(str::to_string).or_else(|| app.ui.palette_bars.first().map(|b| b.library.clone())).unwrap_or_else(|| DOCUMENT.into());
    let mut d = Dialog::new(KIND, json!({ "library": key, "model": "cmyk" }));
    select(app, &mut d, None);
    app.ui.dialog = Some(d);
}

fn set(d: &mut Dialog, k: &str, v: Value) {
    d.fields.insert(k.into(), v);
}

fn num(d: &Dialog, k: &str) -> f64 {
    d.fields.get(k).and_then(Value::as_f64).unwrap_or(0.0)
}

/// Select colour `name` of the palette (the first colour when `None` or gone): its values fill the
/// fields.
fn select(app: &VectorcraftApp, d: &mut Dialog, name: Option<&str>) {
    let Some((_, cells)) = palette_bars::cells(app, &d.str("library")) else { return };
    let cell = name.and_then(|n| cells.iter().find(|c| c.name == n)).or_else(|| cells.first());
    let Some(cell) = cell else {
        set(d, "selected", Value::Null);
        return;
    };
    set(d, "selected", json!(cell.name));
    set(d, "name", json!(cell.name));
    load_color(d, &cell.color);
}

fn load_color(d: &mut Dialog, c: &Color) {
    match *c {
        Color::Cmyk { c, m, y, k } => {
            set(d, "model", json!("cmyk"));
            for (key, v) in [("c", c), ("m", m), ("y", y), ("k", k)] {
                set(d, key, json!((v as f64 * 100.0).round()));
            }
        }
        _ => {
            set(d, "model", json!("rgb"));
            let rgb = c.to_rgb_uncalibrated();
            for (key, v) in ["r", "g", "b"].iter().zip(rgb) {
                set(d, key, json!((v.clamp(0.0, 1.0) as f64 * 255.0).round()));
            }
        }
    }
}

/// The colour the fields describe, as a `color` param.
fn field_color(d: &Dialog) -> Value {
    if d.str("model") == "rgb" {
        let v = |k: &str| num(d, k).clamp(0.0, 255.0).round() as u8;
        json!(format!("#{:02x}{:02x}{:02x}", v("r"), v("g"), v("b")))
    } else {
        // Fractions: color_value reads values of 1 and under as 0..1.
        let v = |k: &str| num(d, k).clamp(0.0, 100.0) / 100.0;
        json!({ "c": v("c"), "m": v("m"), "y": v("y"), "k": v("k") })
    }
}

/// The display colour of the fields, for the preview chip.
fn preview(d: &Dialog) -> egui::Color32 {
    let c = vectorcraft_engine::cmd::color_value(&field_color(d)).unwrap_or_default();
    let [r, g, b] = c.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    egui::Color32::from_rgb(r, g, b)
}

/// Run an edit; its error goes to the status bar.
fn run(app: &mut VectorcraftApp, cmd: &str, p: Value) -> Option<Value> {
    match app.run(cmd, p) {
        Ok(v) => Some(v),
        Err(e) => {
            app.status(e);
            None
        }
    }
}

fn body(app: &mut VectorcraftApp, ui: &mut egui::Ui, d: &mut Dialog) -> bool {
    let key = d.str("library");
    // The palette picker: the document palette, then every library.
    let mut keys = vec![DOCUMENT.to_string()];
    let mut names = vec![tl!("Document Palette").to_string()];
    for l in swatchlib::libraries(&app.session).into_iter().filter(|l| l.category != "gradients") {
        names.push(if l.category == "builtIn" { tl!(&l.name).to_string() } else { l.name.clone() });
        keys.push(l.id);
    }
    let current = keys.iter().position(|k| *k == key).and_then(|i| names.get(i)).cloned().unwrap_or_else(|| "—".into());
    let options: Vec<&str> = names.iter().map(String::as_str).collect();
    ui.horizontal(|ui| {
        widgets::dim_label(ui, tl!("Palette:"));
        if let Some(i) = widgets::dropdown_names(ui, "palette-editor-library", &current, &options, 260.0)
            && let Some(k) = keys.get(i)
        {
            set(d, "library", json!(k));
            select(app, d, None);
        }
        let shown = app.ui.palette_bars.iter().any(|b| b.library == key);
        if widgets::check(ui, tl!("Show as Bar"), shown, true) {
            let _ = app.run("window.colorPalette", json!({ "library": key, "show": !shown }));
        }
    });
    let editable = palette_bars::editable(app, &key);
    ui.horizontal(|ui| {
        if widgets::flat_button(ui, tl!("New Palette"), 110.0).clicked()
            && let Some(r) = run(app, "swatch.library.new", json!({ "name": tl!("New Palette") }))
        {
            set(d, "library", r["library"].clone());
            select(app, d, None);
        }
        if !editable && key != DOCUMENT && widgets::flat_button(ui, tl!("Make Editable Copy"), 150.0).clicked() {
            let name = palette_bars::cells(app, &key).map(|(n, _)| n).unwrap_or_default();
            let copy = crate::i18n::fmt(tl!("{name} (copy)"), &[("name", &name)]);
            if let Some(r) = run(app, "swatch.library.new", json!({ "name": copy, "from": key })) {
                let new = r["library"].as_str().unwrap_or_default().to_string();
                for b in app.ui.palette_bars.iter_mut().filter(|b| b.library == key) {
                    b.library = new.clone();
                }
                set(d, "library", json!(new));
                select(app, d, None);
            }
        }
        if editable
            && key != DOCUMENT
            && widgets::flat_button(ui, tl!("Delete Palette"), 120.0).clicked()
            && run(app, "swatch.library.delete", json!({ "library": key })).is_some()
        {
            app.ui.palette_bars.retain(|b| b.library != key);
            set(d, "library", json!(DOCUMENT));
            select(app, d, None);
        }
    });
    if editable && key != DOCUMENT {
        ui.horizontal(|ui| {
            if !d.fields.contains_key("paletteName") {
                let name = palette_bars::cells(app, &key).map(|(n, _)| n).unwrap_or_default();
                set(d, "paletteName", json!(name));
            }
            super::form::text(ui, d, "paletteName", 200.0);
            if widgets::flat_button(ui, tl!("Rename Palette"), 130.0).clicked() {
                run(app, "swatch.library.edit", json!({ "library": key, "op": "rename", "newName": d.str("paletteName") }));
            }
        });
    } else if !editable {
        widgets::dim_label(ui, tl!("This palette is read only: make an editable copy to change it."));
    }
    ui.add_space(6.0);
    // The colours: a click selects one.
    let cells = palette_bars::cells(app, &key).map(|(_, c)| c).unwrap_or_default();
    let selected = d.str("selected");
    egui::ScrollArea::vertical().max_height(180.0).id_salt("palette-editor-cells").show(ui, |ui| {
        ui.horizontal_wrapped(|ui| {
            ui.spacing_mut().item_spacing = egui::vec2(3.0, 3.0);
            for c in &cells {
                let (rect, resp) = ui.allocate_exact_size(egui::vec2(20.0, 20.0), egui::Sense::click());
                let [r, g, b] = c.color.to_rgb().map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
                ui.painter().rect_filled(rect, 2.0, egui::Color32::from_rgb(r, g, b));
                let t = crate::theme::Tokens::get(ui.ctx());
                let ring = if c.name == selected { egui::Stroke::new(2.0, t.accent) } else { egui::Stroke::new(1.0, t.border) };
                ui.painter().rect_stroke(rect, 2.0, ring, egui::StrokeKind::Inside);
                let resp = resp.on_hover_text(format!("{}\n{}", c.name, palette_bars::values(&c.color)));
                if resp.clicked() {
                    set(d, "selected", json!(c.name));
                    set(d, "name", json!(c.name));
                    load_color(d, &c.color);
                }
            }
        });
    });
    ui.add_space(6.0);
    ui.separator();
    // The colour being edited.
    ui.horizontal(|ui| {
        let (rect, _) = ui.allocate_exact_size(egui::vec2(36.0, 36.0), egui::Sense::hover());
        ui.painter().rect_filled(rect, 3.0, preview(d));
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                widgets::dim_label(ui, tl!("Name:"));
                super::form::text(ui, d, "name", 200.0);
            });
            ui.horizontal(|ui| {
                let models = ["CMYK", "RGB"];
                let rgb = d.str("model") == "rgb";
                if let Some(i) = widgets::dropdown(ui, "palette-editor-model", models[usize::from(rgb)], &models, 70.0) {
                    // Switch the model keeping the colour.
                    let c = vectorcraft_engine::cmd::color_value(&field_color(d)).unwrap_or_default();
                    if i == 1 {
                        load_color(d, &Color::rgb(0.0, 0.0, 0.0));
                        let rgb = c.to_rgb_uncalibrated();
                        for (k, v) in ["r", "g", "b"].iter().zip(rgb) {
                            set(d, k, json!((v.clamp(0.0, 1.0) as f64 * 255.0).round()));
                        }
                    } else {
                        let [c, m, y, k] = c.to_cmyk();
                        load_color(d, &Color::cmyk(c, m, y, k));
                    }
                }
                let keys: &[(&str, f64)] = if d.str("model") == "rgb" {
                    &[("r", 255.0), ("g", 255.0), ("b", 255.0)]
                } else {
                    &[("c", 100.0), ("m", 100.0), ("y", 100.0), ("k", 100.0)]
                };
                for &(k, max) in keys {
                    widgets::dim_label(ui, &k.to_uppercase());
                    if let Some(v) = widgets::plain_field(ui, ("palette-editor", k), num(d, k), "", 0, 44.0) {
                        set(d, k, json!(v.clamp(0.0, max).round()));
                    }
                }
            });
        });
    });
    ui.add_space(4.0);
    let has_selection = !selected.is_empty() && cells.iter().any(|c| c.name == selected);
    ui.add_enabled_ui(editable, |ui| {
        ui.horizontal(|ui| {
            if widgets::flat_button(ui, tl!("Add Color"), 100.0).clicked() && editable {
                let name = d.str("name");
                let color = field_color(d);
                let r = if key == DOCUMENT {
                    let mut p = json!({ "color": color });
                    if !name.trim().is_empty() && !cells.iter().any(|c| c.name == name) {
                        p["name"] = json!(name);
                    }
                    run(app, "swatch.new", p)
                } else {
                    let mut p = json!({ "library": key, "op": "add", "color": color });
                    if !name.trim().is_empty() && !cells.iter().any(|c| c.name == name) {
                        p["name"] = json!(name);
                    }
                    run(app, "swatch.library.edit", p)
                };
                if let Some(r) = r {
                    select(app, d, r["name"].as_str());
                }
            }
            if widgets::flat_button(ui, tl!("Apply Changes"), 120.0).clicked() && editable && has_selection {
                let name = d.str("name");
                let new_name = (name.trim() != selected && !name.trim().is_empty()).then(|| name.trim().to_string());
                let color = field_color(d);
                let r = if key == DOCUMENT {
                    let mut p = json!({ "name": selected, "color": color });
                    if let Some(n) = &new_name {
                        p["newName"] = json!(n);
                    }
                    run(app, "swatch.edit", p)
                } else {
                    let mut p = json!({ "library": key, "op": "set", "name": selected, "color": color });
                    if let Some(n) = &new_name {
                        p["newName"] = json!(n);
                    }
                    run(app, "swatch.library.edit", p)
                };
                if let Some(r) = r {
                    select(app, d, r["name"].as_str());
                }
            }
            if widgets::flat_button(ui, tl!("Delete Color"), 110.0).clicked() && editable && has_selection {
                let ok = if key == DOCUMENT {
                    run(app, "swatch.delete", json!({ "name": selected }))
                } else {
                    run(app, "swatch.library.edit", json!({ "library": key, "op": "delete", "name": selected }))
                };
                if ok.is_some() {
                    select(app, d, None);
                }
            }
        })
    });
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use vectorcraft_engine::Session;

    fn app() -> VectorcraftApp {
        let mut app = VectorcraftApp::new(Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 200})).unwrap();
        app
    }

    #[test]
    fn fields_hold_cmyk_and_rgb_colours() {
        let mut d = Dialog::new(KIND, json!({}));
        load_color(&mut d, &Color::cmyk(0.1, 0.25, 0.0, 1.0));
        assert_eq!((d.str("model"), num(&d, "m"), num(&d, "k")), ("cmyk".to_string(), 25.0, 100.0));
        assert_eq!(field_color(&d), json!({"c": 0.1, "m": 0.25, "y": 0.0, "k": 1.0}));
        // 1 % stays 1 %: fractions are sent.
        set(&mut d, "k", json!(1.0));
        assert_eq!(vectorcraft_engine::cmd::color_value(&field_color(&d)), Some(Color::cmyk(0.1, 0.25, 0.0, 0.01)));
        load_color(&mut d, &Color::rgb8(255, 128, 0));
        assert_eq!(field_color(&d), json!("#ff8000"));
    }

    #[test]
    fn opens_on_a_palette_with_its_first_colour_selected() {
        let mut app = app();
        app.run("swatch.new", json!({"name": "Laranja", "color": "#ff8000"})).unwrap();
        app.run("window.colorPalettes.editor", json!({"library": "default-rgb"})).unwrap();
        let d = app.ui.dialog.clone().unwrap();
        assert_eq!((d.kind.as_str(), d.str("library"), d.str("selected")), (KIND, "default-rgb".to_string(), "R0 G0 B0".to_string()));
        open(&mut app, Some(DOCUMENT));
        let d = app.ui.dialog.as_ref().unwrap();
        assert!(!d.str("selected").is_empty());
        // A headless frame draws it.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for _ in 0..2 {
            let mut out = ctx.run_ui(Default::default(), |ui| crate::dialogs::show(&mut app, ui.ctx()));
            out.textures_delta.clear();
        }
        assert!(app.ui.dialog.is_some());
    }
}
