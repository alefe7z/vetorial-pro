//! Vetorial: View › View Mode, CorelDRAW's ways of showing the drawing:
//! - Simple Wireframe: the outlines of the objects alone (their effects left out);
//! - Wireframe: every outline, effects included;
//! - Draft: colours without smoothing, gradients and patterns as one colour (fast);
//! - Normal: every colour and gradient, without edge smoothing;
//! - Enhanced: everything, smooth (the default);
//! - Pixels: as it will look as pixels (Pixel Preview);
//! - Simulate Overprint: inks that overprint mix as on press (Overprint Preview).
//!
//! Toggle View Mode (Shift+F9) goes back to the mode before. Commands: `view.mode`,
//! `view.modeToggle`.

use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::menus::Item;

/// The modes, top to bottom: id and English label.
pub const MODES: &[(&str, &str)] = &[
    ("simpleWireframe", "Simple Wireframe"),
    ("wireframe", "Wireframe"),
    ("draft", "Draft"),
    ("normal", "Normal"),
    ("enhanced", "Enhanced"),
    ("pixels", "Pixels"),
    ("simulateOverprint", "Simulate Overprint"),
];

/// The mode the view is in now.
pub fn current(app: &VectorcraftApp) -> &'static str {
    let v = &app.ui.view;
    if v.outline {
        return if v.simple_wire { "simpleWireframe" } else { "wireframe" };
    }
    if v.draft {
        "draft"
    } else if v.hard_edges {
        "normal"
    } else if v.pixel_preview {
        "pixels"
    } else if vectorcraft_render::proof::view().overprint {
        "simulateOverprint"
    } else {
        "enhanced"
    }
}

/// Put the view in mode `id`.
fn set(app: &mut VectorcraftApp, id: &str) -> Result<(), String> {
    let id = MODES
        .iter()
        .find(|(m, _)| *m == id)
        .map(|(m, _)| *m)
        .ok_or(format!("mode must be one of {}, not `{id}`", MODES.iter().map(|(m, _)| *m).collect::<Vec<_>>().join(", ")))?;
    let before = current(app);
    let overprint = id == "simulateOverprint";
    if vectorcraft_render::proof::view().overprint != overprint {
        app.run("view.overprintPreview", json!({ "on": overprint }))?;
    }
    let v = &mut app.ui.view;
    v.outline = matches!(id, "simpleWireframe" | "wireframe");
    v.simple_wire = id == "simpleWireframe";
    v.draft = id == "draft";
    v.hard_edges = matches!(id, "draft" | "normal");
    v.pixel_preview = id == "pixels";
    if before != id {
        v.prev_mode = Some(before.to_string());
    }
    Ok(())
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let r = match id {
        "view.mode" => (|| {
            let m = p.get("mode").and_then(Value::as_str).ok_or("missing mode")?;
            set(app, m)?;
            Ok(json!({ "mode": current(app) }))
        })(),
        "view.modeToggle" => (|| {
            // Back to the mode before; the first time, between Enhanced and Wireframe.
            let now = current(app);
            let to = match app.ui.view.prev_mode.clone() {
                Some(m) if m != now => m,
                _ if now == "wireframe" => "enhanced".to_string(),
                _ => "wireframe".to_string(),
            };
            set(app, &to)?;
            Ok(json!({ "mode": current(app) }))
        })(),
        _ => return None,
    };
    Some(r)
}

pub fn checked(app: &VectorcraftApp, id: &str, p: &Value) -> Option<bool> {
    (id == "view.mode").then(|| p.get("mode").and_then(Value::as_str) == Some(current(app)))
}

/// The View › View Mode submenu.
pub fn view_menu() -> Vec<Item> {
    let mut items: Vec<Item> = MODES.iter().map(|(m, label)| Item::Cmd(label, "view.mode", json!({ "mode": m }))).collect();
    items.push(Item::Sep);
    items.push(Item::Cmd("Toggle View Mode", "view.modeToggle", json!({})));
    items
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> VectorcraftApp {
        VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default())
    }

    #[test]
    fn each_mode_sets_the_view_and_shift_f9_goes_back() {
        let mut a = app();
        a.run("file.new", json!({"width": 100, "height": 100})).unwrap();
        assert_eq!(current(&a), "enhanced");
        for (m, _) in MODES {
            assert_eq!(a.run("view.mode", json!({ "mode": m })).unwrap(), json!({ "mode": m }));
            assert_eq!(checked(&a, "view.mode", &json!({ "mode": m })), Some(true));
        }
        a.run("view.mode", json!({"mode": "wireframe"})).unwrap();
        assert!(a.ui.view.outline && !a.ui.view.simple_wire);
        a.run("view.mode", json!({"mode": "simpleWireframe"})).unwrap();
        assert!(a.ui.view.outline && a.ui.view.simple_wire);
        a.run("view.mode", json!({"mode": "draft"})).unwrap();
        assert!(!a.ui.view.outline && a.ui.view.draft && a.ui.view.hard_edges);
        a.run("view.mode", json!({"mode": "pixels"})).unwrap();
        assert!(a.ui.view.pixel_preview && !a.ui.view.draft && !a.ui.view.hard_edges);
        a.run("view.mode", json!({"mode": "enhanced"})).unwrap();
        assert!(!a.ui.view.pixel_preview && !a.ui.view.outline);
        // Shift+F9: Enhanced ↔ the mode before.
        a.run("view.mode", json!({"mode": "wireframe"})).unwrap();
        assert_eq!(a.run("view.modeToggle", json!({})).unwrap(), json!({"mode": "enhanced"}));
        assert_eq!(a.run("view.modeToggle", json!({})).unwrap(), json!({"mode": "wireframe"}));
        assert!(a.run("view.mode", json!({"mode": "xray"})).is_err());
    }

    #[test]
    fn draft_paints_a_gradient_as_one_colour() {
        use vectorcraft_render::{RenderOptions, Renderer};
        let mut a = app();
        a.run("file.new", json!({"width": 100, "height": 100})).unwrap();
        a.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 100, "height": 100})).unwrap();
        a.run("paint.setFill", json!({"gradient": {"stops": [{"offset": 0, "color": "#ff0000"}, {"offset": 1, "color": "#0000ff"}]}})).unwrap();
        let doc = &a.session.active().unwrap().doc;
        let draw = |opts: RenderOptions| Renderer::new().render(doc, 100, 100, vectorcraft_geom::Affine::IDENTITY, &opts).pixels;
        let px = |img: &[u8], x: usize| img[(50 * 100 + x) * 4..(50 * 100 + x) * 4 + 3].to_vec();
        let smooth = draw(RenderOptions::default());
        assert_ne!(px(&smooth, 5), px(&smooth, 95), "the gradient runs across");
        let draft = draw(RenderOptions { draft: true, ..Default::default() });
        assert_eq!(px(&draft, 5), px(&draft, 95), "one colour in Draft");
    }
}
