//! Vetorial: CorelDRAW's Treat All Objects as Filled. On, a click anywhere inside an object picks
//! it (and a drag moves it), even without a fill; off, an object without a fill is picked on its
//! outline only. The switch sits at the bottom left of the window (the status bar), showing the
//! owner's two icons: a filled, striped square (on) or a hollow square with an X (off). Command:
//! `select.treatAllAsFilled`; preference `treatAllAsFilled`.

use egui::{Color32, Pos2, Rect, Sense, Shape, Stroke, Ui, Vec2, pos2, vec2};
use serde_json::{Value, json};

use crate::VectorcraftApp;

/// Is the option on?
pub fn on(app: &VectorcraftApp) -> bool {
    app.session.prefs.treat_all_as_filled
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "select.treatAllAsFilled" {
        return None;
    }
    let to = match p.get("on") {
        None | Some(Value::Null) => !on(app),
        Some(v) => match v.as_bool() {
            Some(b) => b,
            None => return Some(Err("on is true or false".into())),
        },
    };
    Some(app.run("prefs.set", json!({ "key": "treatAllAsFilled", "value": to })).map(|_| json!({ "on": to })))
}

pub fn checked(app: &VectorcraftApp, id: &str, _p: &Value) -> Option<bool> {
    (id == "select.treatAllAsFilled").then(|| on(app))
}

/// The pink and the blue of the "on" icon's stripes.
const PINK: Color32 = Color32::from_rgb(0xEC, 0x00, 0x8C);
const BLUE: Color32 = Color32::from_rgb(0x00, 0xAE, 0xEF);
/// The little lights beside the icon: green on, red off.
const LIGHT_ON: Color32 = Color32::from_rgb(0x55, 0xFF, 0x0F);
const LIGHT_OFF: Color32 = Color32::from_rgb(0xED, 0x1C, 0x24);

/// The icon in `r`: on, a rounded square of pink and blue stripes with a dark rim; off, a hollow
/// rounded square with an X, in `ink`.
pub fn paint_icon(p: &egui::Painter, r: Rect, on: bool, ink: Color32) {
    let round = r.width().min(r.height()) * 0.28;
    if on {
        p.rect_filled(r, round, PINK);
        // Blue stripes slanting across, clipped to the square.
        let clip = p.with_clip_rect(r.shrink(1.0));
        let w = r.width();
        let band = w * 0.055;
        for (x0, x1) in [(-0.2, 0.25), (0.4, 0.75), (0.08, 0.68), (0.9, 1.25)] {
            let a = pos2(r.left() + x0 * w, r.top());
            let b = pos2(r.left() + x1 * w, r.bottom());
            let n = vec2(band, 0.0);
            clip.add(Shape::convex_polygon(vec![a - n, a + n, b + n, b - n], BLUE, Stroke::NONE));
        }
        p.rect_stroke(r, round, Stroke::new(1.5, Color32::from_gray(30)), egui::StrokeKind::Inside);
    } else {
        p.rect_stroke(r, round, Stroke::new(1.3, ink), egui::StrokeKind::Inside);
        let k = r.shrink(r.width() * 0.26);
        p.line_segment([k.left_top(), k.right_bottom()], Stroke::new(1.8, ink));
        p.line_segment([k.right_top(), k.left_bottom()], Stroke::new(1.8, ink));
    }
}

/// The switch in the status bar: the light and the icon; a click turns the option on or off.
pub fn status_switch(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = crate::theme::Tokens::get(ui.ctx());
    let is_on = on(app);
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 18.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let p = ui.painter();
        if resp.hovered() {
            p.rect_filled(rect, 3.0, t.hover);
        }
        let light: Pos2 = pos2(rect.left() + 4.0, rect.center().y);
        p.circle_filled(light, 2.6, if is_on { LIGHT_ON } else { LIGHT_OFF });
        let icon = Rect::from_center_size(pos2(rect.left() + 19.0, rect.center().y), Vec2::new(17.0, 15.0));
        paint_icon(p, icon, is_on, t.text);
    }
    let tip = if is_on {
        tl!("Treat All Objects as Filled: on (a click inside any object picks it)")
    } else {
        tl!("Treat All Objects as Filled: off (an object without a fill is picked on its outline)")
    };
    if resp.on_hover_text(tip).clicked()
        && let Err(e) = app.run("select.treatAllAsFilled", json!({}))
    {
        app.status(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_switch_toggles_the_preference_and_the_click_inside() {
        let mut a = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        a.run("file.new", json!({"width": 300, "height": 300})).unwrap();
        assert!(!on(&a), "off by default, as in CorelDRAW");
        assert_eq!(a.run("select.treatAllAsFilled", json!({})).unwrap(), json!({"on": true}));
        assert!(a.session.prefs.treat_all_as_filled);
        assert_eq!(checked(&a, "select.treatAllAsFilled", &json!({})), Some(true));
        assert_eq!(a.run("select.treatAllAsFilled", json!({"on": false})).unwrap(), json!({"on": false}));
        assert!(a.run("select.treatAllAsFilled", json!({"on": "yes"})).is_err());
    }
}
