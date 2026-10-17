//! Vetorial: the program's modes, switched from the menu bar (Vector, CAD, PAINT…). Each mode
//! will bring its own tools; for now only Vector has any, the others show an empty Tools panel.
//! Command: `app.mode {mode}`.

use egui::{Color32, Sense, Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::Tokens;

/// The modes, left to right: id and label.
pub const MODES: &[(&str, &str)] = &[("vector", "Vector"), ("cad", "CAD"), ("paint", "PAINT"), ("paint2", "PAINT")];

/// The mode the program starts in, the main one.
pub const MAIN: &str = "vector";

/// How wide the mode switcher is in the menu bar.
pub const WIDTH: f32 = 4.0 * STEP + 8.0;
/// One mode button and the gap after it.
const STEP: f32 = 64.0;

/// Does the current mode have the vector tools?
pub fn vector(app: &VectorcraftApp) -> bool {
    app.ui.app_mode == MAIN || app.ui.app_mode.is_empty()
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "app.mode" {
        return None;
    }
    Some(match p.get("mode").and_then(Value::as_str) {
        Some(m) if MODES.iter().any(|(id, _)| *id == m) => {
            app.ui.app_mode = m.to_string();
            Ok(json!({ "mode": m }))
        }
        Some(m) => Err(format!("mode is one of vector, cad, paint, paint2, not `{m}`")),
        None => Err("missing mode".into()),
    })
}

pub fn checked(app: &VectorcraftApp, id: &str, p: &Value) -> Option<bool> {
    (id == "app.mode").then(|| p.get("mode").and_then(Value::as_str) == Some(app.ui.app_mode.as_str()))
}

/// The switcher: one rounded button per mode, a green light on the current one and red on the
/// others.
pub fn bar(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let h = 24.0;
    let (frame, _) = ui.allocate_exact_size(vec2(WIDTH, h + 6.0), Sense::hover());
    ui.painter().rect_filled(frame, 6.0, t.tab_strip);
    ui.painter().rect_stroke(frame, 6.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
    let mut x = frame.left() + 4.0;
    for (id, label) in MODES {
        let r = egui::Rect::from_min_size(egui::pos2(x, frame.top() + 3.0), vec2(STEP - 4.0, h));
        let resp = ui.interact(r, ui.id().with(("mode", id)), Sense::click());
        let on = app.ui.app_mode == *id || (app.ui.app_mode.is_empty() && *id == MAIN);
        let fill = if on {
            t.panel
        } else if resp.hovered() {
            t.hover
        } else {
            t.tab_strip.gamma_multiply(1.4)
        };
        ui.painter().rect_filled(r, 5.0, fill);
        ui.painter().rect_stroke(r, 5.0, Stroke::new(1.0, t.border), egui::StrokeKind::Inside);
        let light = if on { Color32::from_rgb(0x9b, 0xe6, 0x2e) } else { Color32::from_rgb(0xe0, 0x1b, 0x1b) };
        ui.painter().circle_filled(r.left_top() + vec2(8.0, 7.0), 3.0, light);
        let font = if on { crate::theme::semibold(12.5) } else { egui::FontId::proportional(12.0) };
        ui.painter().text(r.center() + vec2(3.0, 0.0), egui::Align2::CENTER_CENTER, tl!(*label), font, if on { t.text_strong } else { t.text_dim });
        if resp.clicked() {
            app.run("app.mode", json!({ "mode": id })).ok();
        }
        x += STEP;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn modes_switch_and_only_vector_has_tools() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        assert!(vector(&app), "Vector is the main mode");
        assert_eq!(app.run("app.mode", json!({"mode": "cad"})).unwrap(), json!({"mode": "cad"}));
        assert!(!vector(&app));
        assert_eq!(checked(&app, "app.mode", &json!({"mode": "cad"})), Some(true));
        assert!(app.run("app.mode", json!({"mode": "photo"})).is_err());
        app.run("app.mode", json!({"mode": "vector"})).unwrap();
        assert!(vector(&app));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        ctx.run_ui(Default::default(), |ui| bar(&mut app, ui)).textures_delta.clear();
    }
}
