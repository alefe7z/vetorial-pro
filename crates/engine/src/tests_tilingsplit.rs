//! Vetorial: Tiling's Split Elements: the tiles but the first become objects of their own that
//! follow the object, keep their own colours unless Link Styles, and go with the effect.

use serde_json::{Value, json};
use vectorcraft_geom::Rect;

use super::*;

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 800, "height": 600})).unwrap();
    s
}

/// The satellites the first Tiling on `id` lists.
fn satellites(s: &Session, id: NodeId) -> Vec<NodeId> {
    let n = s.doc().unwrap().doc.node(id).unwrap();
    let e = n.appearance.effects.iter().find(|e| e.id == "distort.tiling").unwrap();
    e.params.get("satellites").and_then(Value::as_array).map(|a| a.iter().filter_map(Value::as_u64).map(NodeId).collect()).unwrap_or_default()
}

fn bounds(s: &Session, id: NodeId) -> Rect {
    s.doc().unwrap().doc.node(id).unwrap().geometric_bounds().unwrap()
}

fn fill(s: &Session, id: NodeId) -> String {
    let n = s.doc().unwrap().doc.node(id).unwrap();
    format!("{:?}", n.appearance.fill_paint())
}

#[test]
fn split_tiles_are_objects_that_follow_the_object() {
    let mut s = session();
    let r = NodeId(s.execute("shape.rectangle", &json!({"x": 100, "y": 100, "width": 50, "height": 40})).unwrap()["id"].as_u64().unwrap());
    s.execute("effect.apply", &json!({"effect": "distort.tiling", "ids": [r.0], "item": null, "params": {"rows": 2, "columns": 3}})).unwrap();
    assert!(satellites(&s, r).is_empty(), "not split: one object");
    let count = |s: &Session| s.doc().unwrap().doc.node_count();
    let before = count(&s);
    // Split: five objects beside it, in their tiles' places, as one undo step.
    s.execute("effect.setParams", &json!({"ids": [r.0], "index": 0, "item": null, "params": {"split": true}})).unwrap();
    let sats = satellites(&s, r);
    assert_eq!(sats.len(), 5);
    assert_eq!(count(&s), before + 5);
    assert_eq!(bounds(&s, sats[0]), Rect::new(150.0, 100.0, 200.0, 140.0));
    assert_eq!(bounds(&s, sats[4]), Rect::new(200.0, 140.0, 250.0, 180.0));
    let n = s.doc().unwrap().doc.node(sats[0]).unwrap();
    assert!(n.appearance.effects.iter().all(|e| e.id != "distort.tiling"), "a tile has no Tiling of its own");
    // One tile recoloured keeps its colour when the object changes...
    s.execute("paint.setFill", &json!({"ids": [sats[1].0], "color": "#ff0000"})).unwrap();
    let red = fill(&s, sats[1]);
    s.execute("select.set", &json!({"ids": [r.0]})).unwrap();
    s.execute("object.move", &json!({"dx": 10, "dy": 0})).unwrap();
    assert_eq!(satellites(&s, r), sats, "the same objects, made again");
    assert_eq!(bounds(&s, sats[0]).x0, 160.0, "it follows the object");
    assert_eq!(fill(&s, sats[1]), red);
    // ...unless Link Styles: then every tile takes the object's.
    s.execute("effect.setParams", &json!({"ids": [r.0], "index": 0, "item": null, "params": {"linkStyles": true}})).unwrap();
    assert_eq!(fill(&s, sats[1]), fill(&s, r));
    // Fewer tiles: the extra objects go.
    s.execute("effect.setParams", &json!({"ids": [r.0], "index": 0, "item": null, "params": {"rows": 1}})).unwrap();
    assert_eq!(satellites(&s, r).len(), 2);
    assert!(s.doc().unwrap().doc.node(sats[4]).is_none());
    // Undo brings them back.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(satellites(&s, r).len(), 5);
    assert!(s.doc().unwrap().doc.node(sats[4]).is_some());
    // A copy makes tiles of its own.
    s.execute("select.set", &json!({"ids": [r.0]})).unwrap();
    let dup = NodeId(s.execute("edit.duplicate", &json!({})).unwrap()["ids"][0].as_u64().unwrap());
    let theirs = satellites(&s, dup);
    assert_eq!(theirs.len(), 5);
    assert!(theirs.iter().all(|t| !sats.contains(t)), "the original keeps its own: {theirs:?} {sats:?}");
    assert_eq!(satellites(&s, r), sats);
    // Split off: the tiles go.
    s.execute("effect.setParams", &json!({"ids": [r.0], "index": 0, "item": null, "params": {"split": false}})).unwrap();
    assert!(satellites(&s, r).is_empty() && sats.iter().all(|t| s.doc().unwrap().doc.node(*t).is_none()));
    // Removing the copy's Tiling takes its tiles; deleting an object takes them too.
    s.execute("effect.remove", &json!({"ids": [dup.0], "index": 0, "item": null})).unwrap();
    assert!(theirs.iter().all(|t| s.doc().unwrap().doc.node(*t).is_none()));
    s.execute("effect.setParams", &json!({"ids": [r.0], "index": 0, "item": null, "params": {"split": true}})).unwrap();
    let again = satellites(&s, r);
    s.execute("select.set", &json!({"ids": [r.0]})).unwrap();
    s.execute("edit.clear", &json!({})).unwrap();
    assert!(again.iter().all(|t| s.doc().unwrap().doc.node(*t).is_none()));
}
