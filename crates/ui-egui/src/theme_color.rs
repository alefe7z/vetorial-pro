//! Vetorial: Window › Theme Color, the interface in any colour: the panels, bars, fields and
//! borders take shades of the chosen theme colour (with dark text on a light one), and selected
//! items, focused fields and active tabs the chosen accent colour. Both are kept with the
//! preferences (`themeColor`, `accentColor`). Command: `window.themeColor` (`{theme?, accent?}`;
//! without either it opens the picker window).

use serde_json::{Value, json};

use crate::VectorcraftApp;

/// Ready-made theme colours: (#rrggbb, English name).
pub const PRESETS: &[(&str, &str)] = &[
    ("#1e2a3a", "Navy"),
    ("#2b3a2e", "Forest"),
    ("#3a2430", "Wine"),
    ("#30284a", "Violet"),
    ("#2a3540", "Slate"),
    ("#3b3026", "Coffee"),
    ("#d9d2c5", "Sand"),
    ("#cfdbe6", "Sky"),
];

/// Ready-made accent colours.
pub const ACCENTS: &[&str] = &["#378ef0", "#e0457b", "#2eb84b", "#f0a330", "#9b59f0", "#16b5b5", "#e5502b", "#c8c8c8"];

fn state(app: &VectorcraftApp) -> Value {
    json!({ "theme": app.session.prefs.theme_color, "accent": app.session.prefs.accent_color })
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    if id != "window.themeColor" {
        return None;
    }
    let (theme, accent) = (p.get("theme"), p.get("accent"));
    if theme.is_none() && accent.is_none() {
        app.ui.theme_color_picker = true;
        return Some(Ok(state(app)));
    }
    let r = (|| {
        for (key, v) in [("themeColor", theme), ("accentColor", accent)] {
            if let Some(v) = v {
                app.run("prefs.set", json!({ "key": key, "value": v }))?;
            }
        }
        Ok(state(app))
    })();
    Some(r)
}

/// Ticked while the interface has a colour of its own.
pub fn checked(app: &VectorcraftApp, id: &str, _p: &Value) -> Option<bool> {
    (id == "window.themeColor").then_some(!app.session.prefs.theme_color.is_empty() || !app.session.prefs.accent_color.is_empty())
}

fn hex(c: egui::Color32) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b())
}

/// A row of colour buttons; returns the one clicked.
fn swatches<'a>(ui: &mut egui::Ui, colors: impl Iterator<Item = (&'a str, Option<&'a str>)>) -> Option<&'a str> {
    let mut picked = None;
    ui.horizontal_wrapped(|ui| {
        for (c, name) in colors {
            let Some(fill) = crate::prefs_dialog::hex_color(c) else { continue };
            let resp = ui.add(egui::Button::new("").fill(fill).min_size(egui::vec2(26.0, 22.0)));
            let resp = match name {
                Some(n) => resp.on_hover_text(tl!(n)),
                None => resp,
            };
            if resp.clicked() {
                picked = Some(c);
            }
        }
    });
    picked
}

/// The picker window, while it is open.
pub fn show(app: &mut VectorcraftApp, ctx: &egui::Context) {
    if !app.ui.theme_color_picker {
        return;
    }
    let mut open = true;
    let t = crate::theme::Tokens::get(ctx);
    let mut set: Option<(&str, String)> = None;
    egui::Window::new(tl!("Theme Color")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        ui.set_max_width(300.0);
        ui.label(egui::RichText::new(tl!("Theme Color")).strong());
        if let Some(c) = swatches(ui, PRESETS.iter().map(|(c, n)| (*c, Some(*n)))) {
            set = Some(("theme", c.to_string()));
        }
        let mut c = t.panel;
        if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
            set = Some(("theme", hex(c)));
        }
        if ui.button(tl!("Standard (UI Brightness)")).clicked() {
            set = Some(("theme", String::new()));
        }
        ui.separator();
        ui.label(egui::RichText::new(tl!("Accent Color")).strong());
        if let Some(c) = swatches(ui, ACCENTS.iter().map(|c| (*c, None))) {
            set = Some(("accent", c.to_string()));
        }
        let mut a = t.accent;
        if egui::color_picker::color_picker_color32(ui, &mut a, egui::color_picker::Alpha::Opaque) {
            set = Some(("accent", hex(a)));
        }
        if ui.button(tl!("Standard Blue")).clicked() {
            set = Some(("accent", String::new()));
        }
    });
    if let Some((key, v)) = set
        && let Err(e) = app.run("window.themeColor", json!({ key: v }))
    {
        app.ui.status = e;
    }
    app.ui.theme_color_picker = open;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_interface_takes_any_theme_and_accent_colour() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        assert_eq!(checked(&app, "window.themeColor", &json!({})), Some(false));
        let r = app.run("window.themeColor", json!({"theme": "#1E2A3A", "accent": "e0457b"})).unwrap();
        assert_eq!(r, json!({"theme": "#1e2a3a", "accent": "#e0457b"}));
        assert_eq!(checked(&app, "window.themeColor", &json!({})), Some(true));
        assert!(app.run("window.themeColor", json!({"theme": "bluish"})).is_err());
        // The tokens follow: the panels are the theme colour, the accent the accent.
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        crate::prefs_dialog::apply_runtime(&mut app, &ctx);
        let t = crate::theme::Tokens::get(&ctx);
        assert_eq!(t.panel, egui::Color32::from_rgb(0x1e, 0x2a, 0x3a));
        assert_eq!(t.accent, egui::Color32::from_rgb(0xe0, 0x45, 0x7b));
        assert!(t.dark, "light text on a dark theme");
        // A light theme turns the text dark; empty puts the standard look back.
        app.run("window.themeColor", json!({"theme": "#d9d2c5"})).unwrap();
        crate::prefs_dialog::apply_runtime(&mut app, &ctx);
        assert!(!crate::theme::Tokens::get(&ctx).dark);
        app.run("window.themeColor", json!({"theme": "", "accent": ""})).unwrap();
        crate::prefs_dialog::apply_runtime(&mut app, &ctx);
        let std = crate::theme::Tokens::for_brightness(app.ui.brightness);
        assert_eq!(crate::theme::Tokens::get(&ctx).panel, std.panel);
        assert_eq!(crate::theme::Tokens::get(&ctx).accent, std.accent);
        // No colour: the picker opens.
        app.run("window.themeColor", json!({})).unwrap();
        assert!(app.ui.theme_color_picker);
        ctx.run_ui(Default::default(), |ui| show(&mut app, ui.ctx())).textures_delta.clear();
    }
}
