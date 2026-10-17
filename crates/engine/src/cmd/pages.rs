//! Vetorial: CorelDRAW-style page commands for the page bar under the canvas. A page is an
//! artboard; these insert one before or after a page (listed there, drawn right of the last),
//! duplicate one with its art, delete one with its art, and swap a page's orientation. Rename is
//! `artboard.setProps {index, name}` and moving a tab is `artboard.reorder`.

use serde_json::{Value, json};
use vectorcraft_doc::{Artboard, NodeId, Scaling};
use vectorcraft_geom::{Affine, Rect, Vec2};

use super::panelcmds::{artboard_art, copy_artboard};
use super::*;
use crate::EngineError;

pub fn specs() -> Vec<CommandSpec> {
    vec![
        cmd!(
            "page.insert",
            "Insert Page",
            [],
            None,
            "{index?: the page it goes beside (0-based; default 0), after?: bool (default true; false: before it), count?: pages (1–100, default 1)} new pages the size of that one, listed after (or before) it and drawn right of the last page; one undo step → {index: the first new page's number}",
            has_doc,
            insert
        ),
        cmd!(
            "page.duplicate",
            "Duplicate Page",
            [],
            None,
            "{index?: page (default 0), after?: bool (default true), content?: bool (default true: the art fully inside it is copied too; false: an empty page)} a copy of the page, listed after (or before) it and drawn right of the last page → {index, ids: the art's copies}",
            has_doc,
            duplicate
        ),
        cmd!(
            "page.delete",
            "Delete Page",
            [],
            None,
            "{index} delete the page with the art fully inside it (locked and hidden art only with prefs moveLockedWithArtboard) and its guides; a document keeps at least one page",
            has_doc,
            delete
        ),
        cmd!(
            "page.toggleOrientation",
            "Switch Page Orientation",
            [],
            None,
            "{index?: page (default 0)} swap the page's width and height about its top-left corner (portrait ↔ landscape); the pages to its right in its row move over with their art so none overlaps → {width, height}",
            has_doc,
            toggle_orientation
        ),
    ]
}

/// The space between a new page and the pages and art before it.
const GAP: f64 = 36.0;

/// Where a new page's left edge goes: right of every page and all the art, and as far again past
/// the art as the art reaches past the pages, so the art around the last page stays nearer to it
/// than to the new one (each object belongs to the page nearest it; CorelDRAW keeps each page's
/// surroundings apart).
fn new_page_x(d: &vectorcraft_doc::Document) -> f64 {
    let pages = d.artboards.iter().map(|a| a.rect.x1).fold(f64::NEG_INFINITY, f64::max);
    let art = vectorcraft_render::encode::art_bounds(d).map_or(pages, |b| b.x1);
    if !pages.is_finite() {
        return if art.is_finite() { art + GAP } else { 0.0 };
    }
    let reach = (art - pages).max(0.0);
    pages + 2.0 * reach + GAP
}

/// The page `p["index"]` names (default 0) in `n` pages.
fn page(p: &Value, n: usize, cmd: &str) -> Result<usize> {
    let i = match p.get("index") {
        None | Some(Value::Null) => 0,
        Some(v) => v.as_u64().ok_or_else(|| bad(cmd, "index is a page number"))? as usize,
    };
    if i >= n {
        return Err(EngineError::Other("no such page".into()));
    }
    Ok(i)
}

/// Where a new page goes in the list: after page `i` or before it.
fn slot(p: &Value, i: usize) -> usize {
    if bool_or(p, "after", true) { i + 1 } else { i }
}

/// A free default name: "Page N", the lowest N no page has.
fn default_name(boards: &[Artboard]) -> String {
    (1..).map(|n| format!("Page {n}")).find(|name| boards.iter().all(|a| a.name != *name)).unwrap_or_default()
}

/// Whether `name` is a default one ("Page 3", "Artboard 2" or none): the page bar then shows the
/// page's number instead.
pub fn is_default_name(name: &str) -> bool {
    let name = name.trim();
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    name.is_empty() || name.strip_prefix("Page ").is_some_and(digits) || name.strip_prefix("Artboard ").is_some_and(digits)
}

fn insert(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "page.insert";
    let n = s.doc()?.doc.artboards.len();
    let count = p.get("count").and_then(Value::as_u64).unwrap_or(1);
    if !(1..=100).contains(&count) {
        return Err(bad(C, "count is 1 to 100"));
    }
    let (i, at) = if n == 0 { (0, 0) } else { page(p, n, C).map(|i| (i, slot(p, i)))? };
    let index = s.edit("Insert Page", |d, _| {
        let size = d.artboards.get(i).map(|a| a.rect).unwrap_or(Rect::new(0.0, 0.0, 612.0, 792.0));
        for k in 0..count as usize {
            let x = new_page_x(d);
            let rect = Rect::new(x, size.y0, x + size.width(), size.y1);
            let a = Artboard { id: d.next_artboard_id(), name: default_name(&d.artboards), rect, show_center_mark: false, show_cross_hairs: false };
            d.artboards.insert((at + k).min(d.artboards.len()), a);
        }
        Ok(at.min(d.artboards.len().saturating_sub(1)))
    })?;
    Ok(json!({ "index": index }))
}

fn duplicate(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "page.duplicate";
    let i = page(p, s.doc()?.doc.artboards.len(), C)?;
    let at = slot(p, i);
    let art = artboard_art(s, i, bool_or(p, "content", true))?;
    let sc = Scaling { patterns: super::object::transform_patterns(s, p, &art)?, ..Scaling::default() };
    let ids = s.edit("Duplicate Page", |d, _| {
        let dv = d.artboards.get(i).map(|a| Vec2::new(new_page_x(d) - a.rect.x0, 0.0)).unwrap_or_default();
        let default = d.artboards.get(i).is_some_and(|a| is_default_name(&a.name));
        let (index, ids) = copy_artboard(d, i, dv, &art, sc)?;
        let mut copy = d.artboards.remove(index);
        if default {
            copy.name = default_name(&d.artboards);
        }
        d.artboards.insert(at.min(d.artboards.len()), copy);
        Ok(ids)
    })?;
    Ok(json!({ "index": at, "ids": ids.iter().map(|i| i.0).collect::<Vec<_>>() }))
}

fn delete(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "page.delete";
    if p.get("index").is_none() {
        return Err(bad(C, "missing index"));
    }
    let n = s.doc()?.doc.artboards.len();
    let i = page(p, n, C)?;
    if n <= 1 {
        return Err(EngineError::Other("a document needs at least one page".into()));
    }
    let art = artboard_art(s, i, true)?;
    s.edit("Delete Page", |d, sel| {
        if i >= d.artboards.len() {
            return Err(EngineError::Other("no such page".into()));
        }
        let gone = d.artboards.remove(i).id;
        d.retain_guides(sel, |_, g| g.artboard != Some(gone));
        d.remove_many(&art)?;
        sel.prune(d);
        Ok(())
    })?;
    ok()
}

fn toggle_orientation(s: &mut Session, p: &Value) -> Result<Value> {
    const C: &str = "page.toggleOrientation";
    let d0 = &s.doc()?.doc;
    let i = page(p, d0.artboards.len(), C)?;
    let was = d0.artboards.get(i).map(|a| a.rect).ok_or_else(|| EngineError::Other("no such page".into()))?;
    let now = Rect::new(was.x0, was.y0, was.x0 + was.height(), was.y0 + was.width());
    // Grown wider: the pages right of it in its row (and their art) move over by the growth.
    let grow = now.width() - was.width();
    let shifted: Vec<(usize, Vec<NodeId>)> = if grow > 1e-9 {
        d0.artboards
            .iter()
            .enumerate()
            .filter(|(k, a)| *k != i && a.rect.x0 >= was.x1 - 1e-6 && a.rect.y0 < now.y1.max(was.y1) && a.rect.y1 > was.y0)
            .map(|(k, a)| (k, d0.art_on_artboard(a.rect, s.prefs.move_locked_with_artboard)))
            .collect()
    } else {
        vec![]
    };
    s.edit("Switch Page Orientation", |d, _| {
        let dv = Vec2::new(grow, 0.0);
        let mut moved: Vec<NodeId> = vec![];
        for (k, art) in &shifted {
            let Some(a) = d.artboards.get_mut(*k) else { continue };
            a.rect = a.rect + dv;
            let id = a.id;
            d.move_artboard_guides(id, dv);
            for n in art {
                // An object on two pages moves once.
                if !moved.contains(n)
                    && let Some(node) = d.node_mut(*n)
                {
                    node.transform(Affine::translate(dv), Scaling::default());
                    moved.push(*n);
                }
            }
        }
        let a = d.artboards.get_mut(i).ok_or_else(|| EngineError::Other("no such page".into()))?;
        a.rect = now;
        Ok(())
    })?;
    Ok(json!({ "width": now.width(), "height": now.height() }))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(pages: usize) -> Session {
        let mut s = Session::new();
        s.execute("file.new", &json!({"width": 200, "height": 100})).unwrap();
        for _ in 1..pages {
            s.execute("artboard.new", &json!({})).unwrap();
        }
        s
    }

    fn boards(s: &Session) -> Vec<Artboard> {
        s.doc().unwrap().doc.artboards.clone()
    }

    #[test]
    fn insert_goes_after_or_before_and_never_overlaps() {
        let mut s = session(2);
        let ids: Vec<u32> = boards(&s).iter().map(|a| a.id).collect();
        assert_eq!(s.execute("page.insert", &json!({"index": 0})).unwrap()["index"], 1);
        let b = boards(&s);
        assert_eq!(b.len(), 3);
        assert_eq!((b[0].id, b[2].id), (ids[0], ids[1]), "listed after page 1");
        assert!(b[1].rect.x0 > b[2].rect.x1, "drawn right of the last");
        assert_eq!((b[1].rect.width(), b[1].rect.height()), (200.0, 100.0));
        s.execute("page.insert", &json!({"index": 0, "after": false, "count": 2})).unwrap();
        let b = boards(&s);
        assert_eq!(b.len(), 5);
        assert_eq!(b[2].id, ids[0]);
        let names: std::collections::HashSet<_> = b.iter().map(|a| a.name.clone()).collect();
        assert_eq!(names.len(), 5, "names are distinct");
        // One undo step.
        s.execute("edit.undo", &json!({})).unwrap();
        assert_eq!(boards(&s).len(), 3);
        assert!(s.execute("page.insert", &json!({"index": 9})).is_err());
        assert!(s.execute("page.insert", &json!({"count": 0})).is_err());
    }

    #[test]
    fn duplicate_copies_the_art_and_lists_the_copy_after() {
        let mut s = session(2);
        s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 50, "height": 50})).unwrap();
        let r = s.execute("page.duplicate", &json!({"index": 0})).unwrap();
        assert_eq!(r["index"], 1);
        assert_eq!(r["ids"].as_array().unwrap().len(), 1);
        let b = boards(&s);
        assert_eq!(b.len(), 3);
        assert!(b[1].rect.x0 > b[2].rect.x1);
        assert!(is_default_name(&b[1].name));
        let empty = s.execute("page.duplicate", &json!({"index": 0, "content": false, "after": false})).unwrap();
        assert_eq!(empty["ids"].as_array().unwrap().len(), 0);
        assert_eq!(empty["index"], 0);
    }

    #[test]
    fn delete_takes_the_pages_art_and_keeps_one_page() {
        let mut s = session(2);
        let right = boards(&s)[1].rect;
        s.execute("shape.rectangle", &json!({"x": right.x0 + 10.0, "y": 10, "width": 20, "height": 20})).unwrap();
        s.execute("shape.rectangle", &json!({"x": 10, "y": 10, "width": 20, "height": 20})).unwrap();
        let count = |s: &Session| s.doc().unwrap().doc.selectable_art().len();
        assert_eq!(count(&s), 2);
        s.execute("page.delete", &json!({"index": 1})).unwrap();
        assert_eq!(boards(&s).len(), 1);
        assert_eq!(count(&s), 1);
        assert!(s.execute("page.delete", &json!({"index": 0})).is_err());
        assert!(s.execute("page.delete", &json!({})).is_err());
    }

    #[test]
    fn orientation_swaps_and_pushes_the_next_pages_over() {
        let mut s = session(2);
        let next = boards(&s)[1].rect;
        let r = s.execute("shape.rectangle", &json!({"x": next.x0 + 10.0, "y": 10, "width": 20, "height": 20})).unwrap();
        let id = NodeId(r["id"].as_u64().unwrap());
        let r = s.execute("page.toggleOrientation", &json!({"index": 0})).unwrap();
        assert_eq!((r["width"].as_f64(), r["height"].as_f64()), (Some(100.0), Some(200.0)));
        // Narrower: nothing moves.
        assert_eq!(boards(&s)[1].rect, next);
        s.execute("page.toggleOrientation", &json!({"index": 0})).unwrap();
        assert_eq!(boards(&s)[0].rect, Rect::new(0.0, 0.0, 200.0, 100.0));
        // Page 2 to portrait and back to landscape: no page to its right, so nothing else moves.
        s.execute("page.toggleOrientation", &json!({"index": 1})).unwrap();
        s.execute("page.toggleOrientation", &json!({"index": 0})).unwrap();
        s.execute("page.toggleOrientation", &json!({"index": 0})).unwrap();
        let b = boards(&s);
        assert!(b[1].rect.x0 >= b[0].rect.x1, "{b:?}");
        let x = s.doc().unwrap().doc.node(id).unwrap().visual_bounds().unwrap().x0;
        assert!((x - (b[1].rect.x0 + 10.0)).abs() < 1.0, "the art moved with its page");
    }

    #[test]
    fn a_new_page_clears_the_art_around_the_last() {
        let mut s = session(1);
        // Art on the desktop, well right of the page.
        s.execute("shape.rectangle", &json!({"x": 900, "y": 10, "width": 100, "height": 50})).unwrap();
        s.execute("page.insert", &json!({})).unwrap();
        let second = boards(&s)[1].rect;
        assert!(second.x0 >= 1000.0, "{second:?}");
        // The art stays nearer the first page than the new one.
        assert!(second.x0 - 1000.0 > 900.0 - boards(&s)[0].rect.x1, "{second:?}");
        let dup = s.execute("page.duplicate", &json!({"index": 0})).unwrap();
        let i = dup["index"].as_u64().unwrap() as usize;
        assert!(boards(&s)[i].rect.x0 > boards(&s)[2].rect.x1 || i == 1, "{:?}", boards(&s));
    }

    #[test]
    fn default_names() {
        for n in ["", "Page 3", "Artboard 12"] {
            assert!(is_default_name(n), "{n}");
        }
        for n in ["adizero", "Page", "Page 2b", "Artboard x"] {
            assert!(!is_default_name(n), "{n}");
        }
    }
}
