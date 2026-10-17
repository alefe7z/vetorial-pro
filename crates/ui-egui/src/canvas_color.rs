//! Vetorial: View › Canvas Color, the colour around the pages (CorelDRAW's desktop is white): the
//! named choices of User Interface › Canvas Color, and any colour from a picker window.
//! Commands: `view.canvasColor` (`{color?}`; without one it opens the picker).

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::menus::Item;

/// The named choices, as the preference stores them, with their menu labels.
pub const NAMED: &[(&str, &str)] = &[
    ("white", "White"),
    ("lightGray", "Light Gray"),
    ("mediumGray", "Medium Gray"),
    ("darkGray", "Dark Gray"),
    ("matchUi", "Match User Interface Brightness"),
];

/// The View › Canvas Color submenu.
pub fn view_menu() -> Vec<Item> {
    let mut items: Vec<Item> = NAMED.iter().map(|(v, label)| Item::Cmd(label, "view.canvasColor", json!({ "color": v }))).collect();
    items.push(Item::Sep);
    items.push(Item::Cmd("Choose Color…", "view.canvasColor", json!({})));
    items
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "view.canvasColor" {
        return None;
    }
    Some(match p.get("color") {
        Some(c) => app.run("prefs.set", json!({ "key": "canvasColor", "value": c })).map(|_| json!({ "color": app.session.prefs.canvas_color })),
        None => {
            app.ui.canvas_color_picker = true;
            Ok(json!({ "color": app.session.prefs.canvas_color }))
        }
    })
}

/// The ticked item: the stored choice (a picked colour ticks Choose Color…).
pub fn checked(app: &VectorcraftApp, id: &str, p: &Value) -> Option<bool> {
    if id != "view.canvasColor" {
        return None;
    }
    let cur = app.session.prefs.canvas_color.as_str();
    Some(match p.get("color").and_then(Value::as_str) {
        Some(v) => v == cur,
        None => cur.starts_with('#'),
    })
}

/// The picker window, while it is open.
pub fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.ui.canvas_color_picker {
        return;
    }
    let mut open = true;
    let theme = crate::theme::Tokens::get(ctx).pasteboard;
    let mut c = crate::prefs_dialog::canvas_color(&app.session.prefs.canvas_color).unwrap_or(theme);
    egui::Window::new(tl!("Canvas Color")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.horizontal(|ui| {
            for (v, label) in NAMED.iter().take(4) {
                let swatch = crate::prefs_dialog::canvas_color(v).unwrap_or(theme);
                if ui.add(egui::Button::new("").fill(swatch).min_size(egui::vec2(28.0, 22.0))).on_hover_text(tl!(*label)).clicked() {
                    app.run("view.canvasColor", json!({ "color": v })).ok();
                }
            }
            if ui.button(tl!("Match User Interface Brightness")).clicked() {
                app.run("view.canvasColor", json!({ "color": "matchUi" })).ok();
            }
        });
        ui.separator();
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            let hex = format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b());
            app.run("view.canvasColor", json!({ "color": hex })).ok();
        }
    });
    app.ui.canvas_color_picker = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_canvas_takes_white_or_any_colour() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        assert_eq!(app.run("view.canvasColor", json!({"color": "white"})).unwrap(), json!({"color": "white"}));
        assert_eq!(checked(&app, "view.canvasColor", &json!({"color": "white"})), Some(true));
        assert_eq!(app.run("view.canvasColor", json!({"color": "#12AB34"})).unwrap(), json!({"color": "#12ab34"}));
        assert_eq!(checked(&app, "view.canvasColor", &json!({})), Some(true), "Choose Color… ticks a picked colour");
        assert!(app.run("view.canvasColor", json!({"color": "pinkish"})).is_err());
        assert!(!app.ui.canvas_color_picker);
        app.run("view.canvasColor", json!({})).unwrap();
        assert!(app.ui.canvas_color_picker, "no colour: the picker opens");
        let ctx = egui::Context::default();
        ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx())).textures_delta.clear();
    }
}
