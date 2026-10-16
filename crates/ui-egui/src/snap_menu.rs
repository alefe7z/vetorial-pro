//! Vetorial: the Snap To menu ("Alinhar a"), as CorelDRAW has it. A button in the menu bar
//! turns all snapping off (remembering what was on) and back on; its drop-down ticks each kind
//! of target: the document grid, the baseline grid, the guidelines, the objects (their edges,
//! corners, centres, outlines and crossings), the page, pixels and the nodes (anchors). Snapped
//! points take the target's coordinates exactly. Commands: `view.snapTo` and `view.snapAll`.

use egui::{Stroke, Ui, vec2};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::state::ViewFlags;
use crate::theme::Tokens;

/// The menu's items, top to bottom: target id and English label.
pub const TARGETS: &[(&str, &str)] = &[
    ("grid", "Document Grid"),
    ("baseline", "Baseline Grid"),
    ("guides", "Guidelines"),
    ("objects", "Objects"),
    ("page", "Page"),
    ("pixels", "Pixels"),
    ("nodes", "Nodes"),
];

/// The flags the snapping state is made of, saved when Snap To goes off.
pub type Saved = [bool; 9];

fn save(v: &ViewFlags) -> Saved {
    [v.smart_guides, v.snap_to_point, v.snap_to_grid, v.snap_to_pixel, v.snap_objects, v.snap_nodes, v.snap_page, v.snap_guides, v.snap_baseline]
}

fn restore(v: &mut ViewFlags, s: Saved) {
    [v.smart_guides, v.snap_to_point, v.snap_to_grid, v.snap_to_pixel, v.snap_objects, v.snap_nodes, v.snap_page, v.snap_guides, v.snap_baseline] = s;
}

/// Whether target `t` snaps now (its tick in the menu).
pub fn is_on(v: &ViewFlags, t: &str) -> bool {
    let targets = v.smart_guides || v.snap_to_point;
    match t {
        "grid" => v.snap_to_grid,
        "pixels" => v.snap_to_pixel,
        "objects" => v.snap_objects && v.smart_guides,
        "page" => v.snap_page && v.smart_guides,
        "nodes" => v.snap_nodes && targets,
        "guides" => v.snap_guides && targets && v.guides,
        "baseline" => v.snap_baseline && targets,
        _ => false,
    }
}

/// Whether anything snaps (the Snap To button is on).
pub fn any_on(v: &ViewFlags) -> bool {
    TARGETS.iter().any(|(t, _)| is_on(v, t))
}

/// Tick or untick target `t`. Ticking one turns on what it needs to pull (Smart Guides, Show
/// Guides…).
fn set(v: &mut ViewFlags, t: &str, on: bool) -> Result<(), String> {
    match t {
        "grid" => v.snap_to_grid = on,
        "pixels" => v.snap_to_pixel = on,
        "objects" => v.snap_objects = on,
        "page" => v.snap_page = on,
        "nodes" => v.snap_nodes = on,
        "guides" => {
            v.snap_guides = on;
            if on {
                v.guides = true;
                v.snap_to_point = true;
            }
        }
        "baseline" => v.snap_baseline = on,
        other => return Err(format!("target must be one of grid, baseline, guides, objects, page, pixels, nodes, not `{other}`")),
    }
    if on && matches!(t, "objects" | "page" | "nodes" | "baseline") {
        v.smart_guides = true;
    }
    if on {
        v.snap_saved = None;
    }
    Ok(())
}

/// Snap To off (everything unticked, what was on remembered) or on (what was on before, else the
/// usual objects, nodes, page and guidelines).
fn set_all(v: &mut ViewFlags, on: bool) {
    if on {
        if any_on(v) {
            return;
        }
        match v.snap_saved.take() {
            Some(s) if s.iter().any(|b| *b) => restore(v, s),
            _ => {
                restore(v, [true, true, false, false, true, true, true, true, false]);
                v.guides = true;
            }
        }
    } else if any_on(v) {
        v.snap_saved = Some(save(v));
        restore(v, [false; 9]);
    }
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let r = match id {
        "view.snapTo" => (|| {
            let t = p.get("target").and_then(Value::as_str).ok_or("missing target")?;
            if let Some(step) = p.get("step") {
                let step = step.as_f64().filter(|s| s.is_finite() && (0.01..=10_000.0).contains(s)).ok_or("step is 0.01 to 10000 pt")?;
                app.ui.view.baseline_step = step;
            }
            let on = match p.get("on") {
                Some(v) => v.as_bool().ok_or("on is true or false")?,
                None if p.get("step").is_some() => is_on(&app.ui.view, t),
                None => !is_on(&app.ui.view, t),
            };
            set(&mut app.ui.view, t, on)?;
            Ok(json!({ "target": t, "on": is_on(&app.ui.view, t) }))
        })(),
        "view.snapAll" => (|| {
            let on = match p.get("on") {
                Some(v) => v.as_bool().ok_or("on is true or false")?,
                None => !any_on(&app.ui.view),
            };
            set_all(&mut app.ui.view, on);
            Ok(json!({ "on": any_on(&app.ui.view) }))
        })(),
        _ => return None,
    };
    Some(r)
}

pub fn checked(app: &VectorcraftApp, id: &str, p: &Value) -> Option<bool> {
    match id {
        "view.snapTo" => Some(p.get("target").and_then(Value::as_str).is_some_and(|t| is_on(&app.ui.view, t))),
        "view.snapAll" => Some(any_on(&app.ui.view)),
        _ => None,
    }
}

/// The View › Snap To submenu.
pub fn view_menu() -> Vec<crate::menus::Item> {
    let mut items = vec![crate::menus::Item::Cmd("Snap To (All)", "view.snapAll", json!({})), crate::menus::Item::Sep];
    items.extend(TARGETS.iter().map(|(t, label)| crate::menus::Item::Cmd(label, "view.snapTo", json!({ "target": t }))));
    items
}

/// How wide the Snap To button is in the menu bar.
pub const WIDTH: f32 = 128.0;

/// The Snap To button for the menu bar: the on/off switch and its drop-down.
pub fn button(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let on = any_on(&app.ui.view);
    // The switch: a round light, green with snapping on, red with it off.
    let (r, resp) = ui.allocate_exact_size(vec2(20.0, 20.0), egui::Sense::click());
    let light = if on { egui::Color32::from_rgb(0x2e, 0xb8, 0x4b) } else { egui::Color32::from_rgb(0xe5, 0x2b, 0x2b) };
    ui.painter().circle_filled(r.center(), 8.0, light);
    if on {
        ui.painter().line_segment([r.center() + vec2(-4.0, 0.0), r.center() + vec2(-1.0, 3.5)], Stroke::new(2.0, egui::Color32::WHITE));
        ui.painter().line_segment([r.center() + vec2(-1.0, 3.5), r.center() + vec2(4.5, -3.5)], Stroke::new(2.0, egui::Color32::WHITE));
    } else {
        ui.painter().line_segment([r.center() + vec2(-4.5, 0.0), r.center() + vec2(4.5, 0.0)], Stroke::new(2.5, egui::Color32::WHITE));
    }
    let tip = if on { tl!("Snap To: on (click to turn all snapping off)") } else { tl!("Snap To: off (click to turn snapping back on)") };
    if resp.on_hover_text(tip).clicked() {
        app.run("view.snapAll", json!({})).ok();
    }
    // The drop-down.
    let label = egui::RichText::new(format!("{} ⏷", tl!("Snap To"))).size(12.0).color(if on { t.text } else { t.text_dim });
    let resp = ui.add(egui::Button::new(label).fill(t.tab_strip).stroke(Stroke::new(1.0, t.border)));
    egui::Popup::menu(&resp).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| menu(app, ui));
}

/// The drop-down's contents.
fn menu(app: &mut VectorcraftApp, ui: &mut Ui) {
    ui.set_min_width(220.0);
    let t = Tokens::get(ui.ctx());
    let mut all = any_on(&app.ui.view);
    if ui.checkbox(&mut all, egui::RichText::new(tl!("Snap To")).strong()).changed() {
        app.run("view.snapAll", json!({ "on": all })).ok();
    }
    ui.separator();
    for (target, label) in TARGETS {
        let mut on = is_on(&app.ui.view, target);
        if ui.checkbox(&mut on, tl!(*label)).changed() {
            app.run("view.snapTo", json!({ "target": target, "on": on })).ok();
        }
    }
    ui.separator();
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Baseline spacing")).color(t.text_dim));
        let mut step = app.ui.view.baseline_step;
        if ui.add(egui::DragValue::new(&mut step).range(0.01..=10_000.0).speed(0.1).suffix(" pt").max_decimals(4)).changed() {
            app.run("view.snapTo", json!({ "target": "baseline", "step": step })).ok();
        }
    });
    ui.horizontal(|ui| {
        ui.label(egui::RichText::new(tl!("Snapping distance")).color(t.text_dim));
        let mut tol = app.session.prefs.snapping_tolerance;
        if ui.add(egui::DragValue::new(&mut tol).range(0.0..=40.0).speed(0.1).suffix(" px").max_decimals(1)).changed() {
            app.run("prefs.set", json!({ "key": "snappingTolerance", "value": tol })).ok();
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
    fn snap_all_turns_everything_off_and_back() {
        let mut a = app();
        assert!(any_on(&a.ui.view));
        a.run("view.snapTo", json!({"target": "nodes", "on": false})).unwrap();
        let before = save(&a.ui.view);
        assert_eq!(a.run("view.snapAll", json!({})).unwrap(), json!({"on": false}));
        assert!(TARGETS.iter().all(|(t, _)| !is_on(&a.ui.view, t)));
        let info = a.view_info();
        assert!(!info.smart_guides && !info.snap_to_point && !info.snap_to_grid && !info.snap_to_pixel);
        // Back on: what was on before, nodes still off.
        assert_eq!(a.run("view.snapAll", json!({})).unwrap(), json!({"on": true}));
        assert_eq!(save(&a.ui.view), before);
        assert!(!is_on(&a.ui.view, "nodes") && is_on(&a.ui.view, "objects"));
        assert_eq!(a.run("view.snapAll", json!({"on": true})).unwrap(), json!({"on": true}));
    }

    #[test]
    fn ticking_targets_reaches_the_tools() {
        let mut a = app();
        a.run("view.snapAll", json!({"on": false})).unwrap();
        // Ticking Objects alone brings Smart Guides back with only objects pulling.
        assert_eq!(a.run("view.snapTo", json!({"target": "objects"})).unwrap(), json!({"target": "objects", "on": true}));
        let k = a.view_info().snap_kinds;
        assert!(a.view_info().smart_guides && k.objects && !k.nodes && !k.page && !k.guides && !k.baseline);
        a.run("view.snapTo", json!({"target": "baseline", "on": true, "step": 12})).unwrap();
        let k = a.view_info().snap_kinds;
        assert!(k.baseline && k.baseline_step == 12.0);
        a.run("view.snapTo", json!({"target": "grid"})).unwrap();
        assert!(a.view_info().snap_to_grid);
        assert!(a.run("view.snapTo", json!({"target": "sideways"})).is_err());
        assert!(a.run("view.snapTo", json!({"target": "baseline", "step": 0})).is_err());
        assert_eq!(checked(&a, "view.snapTo", &json!({"target": "grid"})), Some(true));
        assert_eq!(checked(&a, "view.snapAll", &json!({})), Some(true));
    }
}
