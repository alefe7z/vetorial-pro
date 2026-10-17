//! Vetorial: the page bar under the canvas, as CorelDRAW has it. Insert a page before, go to the
//! first / previous page, "N of M", next / last, insert a page after, then one tab per page
//! ("Page 2", or "2: name" when it has a name of its own). Clicking a tab goes to that page,
//! double-clicking renames it in place, dragging moves it; right-click (or the tab's ▾) opens
//! Rename, Insert After / Before, Duplicate, Delete and Switch Orientation. A page is an artboard;
//! the bar acts through `view.goToArtboard`, `page.*`, `artboard.setProps` and `artboard.reorder`.

use egui::{Color32, Sense, Stroke, Ui, vec2};
use serde_json::json;
use vectorcraft_engine::cmd::pages::is_default_name;

use crate::VectorcraftApp;
use crate::theme::{self, Tokens};
use crate::widgets;

/// The bar's height.
pub const HEIGHT: f32 = 24.0;

/// A tab being renamed in place: its page and the text typed so far.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Rename {
    pub page: usize,
    pub text: String,
}

/// Vetorial (CorelDRAW): with View › One Page at a Time on and more than one page, the page the
/// canvas shows alone, with the art around it; `None` shows every page and all the art.
pub fn single_page(app: &VectorcraftApp) -> Option<usize> {
    let n = app.session.active()?.doc.artboards.len();
    (app.ui.view.single_page && n > 1).then(|| app.view().map_or(0, |v| v.artboard).min(n - 1))
}

/// The page each object belongs to: the one nearest its centre (inside it: that one). Objects are
/// the layers' contents (a group as a whole), sublayers looked into.
pub fn owner(doc: &vectorcraft_doc::Document, b: vectorcraft_geom::Rect) -> usize {
    let c = b.center();
    let dist = |r: &vectorcraft_geom::Rect| {
        let dx = (r.x0 - c.x).max(0.0).max(c.x - r.x1);
        let dy = (r.y0 - c.y).max(0.0).max(c.y - r.y1);
        dx * dx + dy * dy
    };
    doc.artboards.iter().enumerate().min_by(|a, b| dist(&a.1.rect).total_cmp(&dist(&b.1.rect))).map_or(0, |(i, _)| i)
}

/// The objects that belong to pages other than `page` (the canvas doesn't draw them while it shows
/// one page at a time), worked out again only when the document or the page changes.
pub fn off_page(doc: &vectorcraft_doc::Document, revision: (u64, u64), page: usize) -> Vec<vectorcraft_doc::NodeId> {
    type Cached = Option<((u64, u64, usize), Vec<vectorcraft_doc::NodeId>)>;
    thread_local!(static CACHE: std::cell::RefCell<Cached> = const { std::cell::RefCell::new(None) });
    let key = (revision.0, revision.1, page);
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if let Some((k, ids)) = c.as_ref()
            && *k == key
        {
            return ids.clone();
        }
        fn walk(
            doc: &vectorcraft_doc::Document,
            nodes: &[std::sync::Arc<vectorcraft_doc::Node>],
            page: usize,
            out: &mut Vec<vectorcraft_doc::NodeId>,
        ) {
            for n in nodes {
                if n.is_layer() {
                    walk(doc, n.children().map_or(&[], Vec::as_slice), page, out);
                } else if let Some(b) = n.visual_bounds()
                    && owner(doc, b) != page
                {
                    out.push(n.id);
                }
            }
        }
        let mut ids = vec![];
        walk(doc, &doc.layers, page, &mut ids);
        *c = Some((key, ids.clone()));
        ids
    })
}

/// What the tab's label says: "Page 3", or "3: name".
pub fn tab_label(index: usize, name: &str) -> String {
    let n = (index + 1).to_string();
    if is_default_name(name) { crate::i18n::fmt(tl!("Page {n}"), &[("n", &n)]) } else { format!("{n}: {}", name.trim()) }
}

/// Run a page command, then show page `go(result)` (when given).
fn run(app: &mut VectorcraftApp, cmd: &str, params: serde_json::Value, go: impl FnOnce(&serde_json::Value) -> Option<usize>) {
    match app.run(cmd, params) {
        Ok(r) => {
            if let Some(i) = go(&r) {
                app.run("view.goToArtboard", json!({ "index": i })).ok();
            }
        }
        Err(e) => app.status(e),
    }
}

fn index_of(r: &serde_json::Value) -> Option<usize> {
    r.get("index").and_then(serde_json::Value::as_u64).map(|i| i as usize)
}

/// The tab menu's actions on page `i` of `n`.
fn menu(app: &mut VectorcraftApp, ui: &mut Ui, i: usize, n: usize) {
    if ui.button(tl!("Rename Page…")).clicked() {
        let name = app.session.active().and_then(|d| d.doc.artboards.get(i)).map(|a| a.name.clone()).unwrap_or_default();
        app.ui.page_rename = Some(Rename { page: i, text: if is_default_name(&name) { String::new() } else { name } });
        ui.close();
    }
    ui.separator();
    if ui.button(tl!("Insert Page After")).clicked() {
        run(app, "page.insert", json!({"index": i, "after": true}), index_of);
        ui.close();
    }
    if ui.button(tl!("Insert Page Before")).clicked() {
        run(app, "page.insert", json!({"index": i, "after": false}), index_of);
        ui.close();
    }
    if ui.button(tl!("Duplicate Page")).clicked() {
        run(app, "page.duplicate", json!({"index": i, "after": true}), index_of);
        ui.close();
    }
    if ui.add_enabled(n > 1, egui::Button::new(tl!("Delete Page"))).clicked() {
        run(app, "page.delete", json!({ "index": i }), |_| Some(i.min(n.saturating_sub(2))));
        ui.close();
    }
    ui.separator();
    let mut all = !app.ui.view.single_page;
    if ui.checkbox(&mut all, tl!("Show All Pages' Objects")).changed() {
        app.run("view.singlePage", json!({ "on": !all })).ok();
    }
    if ui.button(tl!("Switch Page Orientation")).clicked() {
        run(app, "page.toggleOrientation", json!({ "index": i }), |_| Some(i));
        ui.close();
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(names) = app.session.active().map(|d| d.doc.artboards.iter().map(|a| a.name.clone()).collect::<Vec<_>>()) else { return };
    let n = names.len();
    let t = Tokens::get(ui.ctx());
    let frame = egui::Frame::NONE.fill(t.panel).stroke(Stroke::new(1.0, t.border)).inner_margin(egui::Margin::symmetric(4, 0));
    egui::Panel::bottom("page_bar").exact_size(HEIGHT).resizable(false).frame(frame).show(ui, |ui| {
        ui.horizontal_centered(|ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            let cur = app.view().map_or(0, |v| v.artboard).min(n.saturating_sub(1));
            if widgets::icon_button(ui, "plus", tl!("Insert page before"), false, 18.0).clicked() {
                run(app, "page.insert", json!({"index": cur, "after": false}), index_of);
            }
            let mut go = None;
            for (icon, tip, to, on) in
                [("chevrons-left", tl!("First page"), "first", cur > 0), ("chevron-left", tl!("Previous page"), "previous", cur > 0)]
            {
                if widgets::icon_button_enabled(ui, icon, tip, false, on, 18.0).clicked() {
                    go = Some(to);
                }
            }
            let of = crate::i18n::fmt(tl!("{n} of {m}"), &[("n", &(cur + 1).to_string()), ("m", &n.to_string())]);
            ui.add_sized(vec2(48.0, HEIGHT - 4.0), egui::Label::new(egui::RichText::new(of).size(11.5).color(t.text)));
            for (icon, tip, to, on) in
                [("chevron-right", tl!("Next page"), "next", cur + 1 < n), ("chevrons-right", tl!("Last page"), "last", cur + 1 < n)]
            {
                if widgets::icon_button_enabled(ui, icon, tip, false, on, 18.0).clicked() {
                    go = Some(to);
                }
            }
            if widgets::icon_button(ui, "plus", tl!("Insert page after"), false, 18.0).clicked() {
                run(app, "page.insert", json!({"index": cur, "after": true}), index_of);
            }
            if let Some(to) = go
                && let Err(e) = app.run("view.goToArtboard", json!({ "index": to }))
            {
                app.status(e);
            }
            ui.add_space(6.0);
            egui::ScrollArea::horizontal().id_salt("page_tabs").scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden).show(
                ui,
                |ui| {
                    tabs(app, ui, &names, cur);
                },
            );
        });
    });
}

/// The page tabs.
fn tabs(app: &mut VectorcraftApp, ui: &mut Ui, names: &[String], cur: usize) {
    let t = Tokens::get(ui.ctx());
    let n = names.len();
    let mut rects = Vec::with_capacity(n);
    let mut dragged: Option<(usize, f32)> = None;
    for (i, name) in names.iter().enumerate() {
        // Renaming this tab: a text field in its place.
        if let Some(mut r) = app.ui.page_rename.clone().filter(|r| r.page == i) {
            let resp = ui.add(egui::TextEdit::singleline(&mut r.text).desired_width(110.0).font(theme::semibold(11.5)));
            if !resp.has_focus() && !resp.lost_focus() {
                resp.request_focus();
            }
            let enter = ui.input(|i| i.key_pressed(egui::Key::Enter));
            let escape = ui.input(|i| i.key_pressed(egui::Key::Escape));
            if escape {
                app.ui.page_rename = None;
            } else if enter || resp.lost_focus() {
                app.ui.page_rename = None;
                let typed = r.text.trim().to_string();
                let name = if typed.is_empty() { format!("Page {}", i + 1) } else { typed };
                if let Err(e) = app.run("artboard.setProps", json!({"index": i, "name": name})) {
                    app.status(e);
                }
            } else {
                app.ui.page_rename = Some(r);
            }
            rects.push(resp.rect);
            continue;
        }
        let label = tab_label(i, name);
        let active = i == cur;
        let color = if active { Color32::WHITE } else { t.text };
        let galley = ui.painter().layout_no_wrap(label, theme::semibold(11.5), color);
        let w = (galley.size().x + 34.0).max(84.0);
        let (rect, resp) = ui.allocate_exact_size(vec2(w, HEIGHT - 4.0), Sense::click_and_drag());
        let fill = if active {
            t.accent
        } else if resp.hovered() {
            t.hover
        } else {
            t.tab_strip
        };
        ui.painter().rect_filled(rect, 3.0, fill);
        ui.painter().galley(
            egui::pos2(
                rect.center().x - galley.size().x / 2.0 - if active || resp.hovered() { 6.0 } else { 0.0 },
                rect.center().y - galley.size().y / 2.0,
            ),
            galley,
            color,
        );
        // The ▾ of the active or hovered tab opens its menu.
        let arrow = egui::Rect::from_center_size(egui::pos2(rect.right() - 10.0, rect.center().y), vec2(16.0, 16.0));
        let arrow_resp = ui.interact(arrow, resp.id.with("arrow"), Sense::click());
        if active || resp.hovered() || arrow_resp.hovered() {
            crate::icons::paint(ui, "chevron-down", arrow.shrink(2.0), color);
        }
        egui::Popup::menu(&arrow_resp).show(|ui| menu(app, ui, i, n));
        if resp.double_clicked() {
            app.ui.page_rename = Some(Rename { page: i, text: if is_default_name(name) { String::new() } else { name.clone() } });
        } else if resp.clicked() && !active {
            app.run("view.goToArtboard", json!({ "index": i })).ok();
        }
        resp.context_menu(|ui| menu(app, ui, i, n));
        if resp.dragged() || resp.drag_stopped() {
            if let Some(p) = ui.input(|inp| inp.pointer.interact_pos()) {
                dragged = Some((i, p.x));
                if resp.dragged() {
                    ui.painter().rect_stroke(rect, 3.0, Stroke::new(1.5, t.accent), egui::StrokeKind::Inside);
                }
            }
            if resp.drag_stopped()
                && let Some((from, x)) = dragged
            {
                let to = drop_target(&rects, rect, x, from, names.len());
                if to != from {
                    match app.run("artboard.reorder", json!({"index": from, "to": to})) {
                        Ok(_) => {
                            app.run("view.goToArtboard", json!({ "index": to })).ok();
                        }
                        Err(e) => app.status(e),
                    }
                }
            }
        }
        rects.push(rect);
    }
}

/// Where a tab dropped at x lands: the number of other tabs whose centre lies left of x. Tabs to
/// the right of the dragged one aren't laid out yet, so their places are guessed from its width.
fn drop_target(before: &[egui::Rect], own: egui::Rect, x: f32, from: usize, n: usize) -> usize {
    let left = before.iter().filter(|r| r.center().x < x).count();
    if left < from {
        return left;
    }
    let step = own.width() + 2.0;
    let right = ((x - own.right()) / step + 0.5).floor().max(0.0) as usize;
    (from + right).min(n.saturating_sub(1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One headless frame of the page bar → its 18-point buttons and its tabs, left to right.
    fn frame(app: &mut VectorcraftApp, ctx: &egui::Context, events: Vec<egui::Event>) -> (Vec<egui::Rect>, Vec<egui::Rect>) {
        let screen = egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(1200.0, 300.0));
        let mut out = ctx.run_ui(egui::RawInput { screen_rect: Some(screen), events, ..Default::default() }, |ui| show(app, ui));
        out.textures_delta.clear();
        let all: Vec<egui::Rect> = ctx.viewport(|vp| vp.prev_pass.widgets.layers().flat_map(|(_, w)| w.iter()).map(|w| w.rect).collect());
        let pick = |f: &dyn Fn(&egui::Rect) -> bool| {
            let mut v: Vec<egui::Rect> = all.iter().filter(|r| f(r)).copied().collect();
            v.sort_by(|a, b| a.left().total_cmp(&b.left()));
            v.dedup();
            v
        };
        (pick(&|r| r.size() == vec2(18.0, 18.0)), pick(&|r| r.height() == HEIGHT - 4.0 && r.width() >= 84.0))
    }

    fn click(app: &mut VectorcraftApp, ctx: &egui::Context, r: egui::Rect, clicks: usize) {
        let b =
            |pressed| egui::Event::PointerButton { pos: r.center(), button: egui::PointerButton::Primary, pressed, modifiers: Default::default() };
        frame(app, ctx, vec![egui::Event::PointerMoved(r.center())]);
        for _ in 0..clicks {
            frame(app, ctx, vec![b(true)]);
            frame(app, ctx, vec![b(false)]);
        }
    }

    #[test]
    fn the_bar_inserts_goes_to_and_renames_pages() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
        app.run("artboard.new", json!({})).unwrap();
        app.run("artboard.new", json!({})).unwrap();
        app.canvas_rect = Some(egui::Rect::from_min_size(egui::Pos2::ZERO, vec2(460.0, 260.0)));
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        let (buttons, tabs) = frame(&mut app, &ctx, vec![]);
        assert_eq!((buttons.len(), tabs.len()), (6, 3), "{buttons:?} {tabs:?}");
        let pages = |app: &VectorcraftApp| app.session.active().unwrap().doc.artboards.len();
        let cur = |app: &VectorcraftApp| app.view().unwrap().artboard;
        // + after page 1: a new page 2, shown.
        click(&mut app, &ctx, buttons[5], 1);
        assert_eq!((pages(&app), cur(&app)), (4, 1));
        // A tab goes to its page.
        let (_, tabs) = frame(&mut app, &ctx, vec![]);
        assert_eq!(tabs.len(), 4);
        click(&mut app, &ctx, tabs[3], 1);
        assert_eq!(cur(&app), 3);
        // Last / first.
        click(&mut app, &ctx, buttons[1], 1);
        assert_eq!(cur(&app), 0);
        // + before page 1.
        click(&mut app, &ctx, buttons[0], 1);
        assert_eq!((pages(&app), cur(&app)), (5, 0));
        // Rename in place: type, Enter.
        app.ui.page_rename = Some(Rename { page: 2, text: String::new() });
        frame(&mut app, &ctx, vec![]);
        frame(&mut app, &ctx, vec![egui::Event::Text("adizero".into())]);
        frame(
            &mut app,
            &ctx,
            vec![egui::Event::Key { key: egui::Key::Enter, physical_key: None, pressed: true, repeat: false, modifiers: Default::default() }],
        );
        assert_eq!(app.ui.page_rename, None);
        assert_eq!(app.session.active().unwrap().doc.artboards[2].name, "adizero");
        assert_eq!(tab_label(2, "adizero"), "3: adizero");
    }

    /// Vetorial: one page at a time, the canvas leaves out the art of the other pages; the option
    /// off (or a single page), everything shows.
    #[test]
    fn one_page_at_a_time_hides_the_other_pages_art() {
        let mut app = VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default());
        app.run("file.new", json!({"width": 200, "height": 100})).unwrap();
        let near_first = app.run("shape.rectangle", json!({"x": -300, "y": 10, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap();
        assert_eq!(single_page(&app), None, "one page: nothing to hide");
        app.run("page.insert", json!({})).unwrap();
        let second = app.session.active().unwrap().doc.artboards[1].rect;
        let near_second =
            app.run("shape.rectangle", json!({"x": second.x1 + 400.0, "y": 10, "width": 50, "height": 50})).unwrap()["id"].as_u64().unwrap();
        let st = app.session.active().unwrap();
        let (doc, rev) = (st.doc.clone(), (st.uid, st.revision));
        app.run("view.goToArtboard", json!({"index": 0})).unwrap();
        assert_eq!(single_page(&app), Some(0));
        let hidden: Vec<u64> = off_page(&doc, rev, 0).iter().map(|i| i.0).collect();
        assert_eq!(hidden, vec![near_second], "page 1 shows its own desktop art, not page 2's");
        let hidden: Vec<u64> = off_page(&doc, rev, 1).iter().map(|i| i.0).collect();
        assert_eq!(hidden, vec![near_first]);
        app.run("view.singlePage", json!({"on": false})).unwrap();
        assert_eq!(single_page(&app), None, "Show All Pages' Objects");
        assert_eq!(app.run("view.singlePage", json!({})).unwrap(), json!({"on": true}));
    }

    #[test]
    fn tab_labels() {
        assert_eq!(tab_label(0, "Page 1"), "Page 1");
        assert_eq!(tab_label(4, "Artboard 2"), "Page 5");
        assert_eq!(tab_label(1, "adizero"), "2: adizero");
    }

    #[test]
    fn drop_targets() {
        let r = |x: f32| egui::Rect::from_min_size(egui::pos2(x, 0.0), vec2(80.0, 20.0));
        let before = [r(0.0), r(82.0)];
        // Tab 3 (at 164) dropped at x: left of the first tab's centre → first, and so on.
        let out: Vec<usize> = [10.0, 100.0, 200.0, 300.0, 900.0].iter().map(|x| drop_target(&before, r(164.0), *x, 2, 5)).collect();
        assert_eq!(out, vec![0, 1, 2, 3, 4]);
    }
}
