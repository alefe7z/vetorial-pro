//! Mirror Horizontally / Vertically (CorelDRAW's one-click mirror buttons).
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_engine::Session;

fn points(s: &Session, id: NodeId) -> Vec<(f64, f64)> {
    let mut v: Vec<(f64, f64)> =
        s.doc().unwrap().doc.node(id).unwrap().path_data().unwrap().subpaths[0].anchors.iter().map(|a| (a.p.x, a.p.y)).collect();
    v.sort_by(|a, b| a.partial_cmp(b).unwrap());
    v
}

#[test]
fn mirror_flips_the_selection_in_place() {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
    // A right triangle: its right angle at the bottom left.
    let r =
        s.execute("path.create", &json!({"anchors": [{"x": 100, "y": 100}, {"x": 100, "y": 200}, {"x": 160, "y": 200}], "closed": true})).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    s.execute("select.set", &json!({"ids": [id.0]})).unwrap();
    s.execute("object.mirrorHorizontal", &json!({})).unwrap();
    // Left to right about its centre (x 130): the right angle goes to the bottom right.
    assert_eq!(points(&s, id), vec![(100.0, 200.0), (160.0, 100.0), (160.0, 200.0)]);
    s.execute("object.mirrorVertical", &json!({})).unwrap();
    // Top to bottom about y 150.
    assert_eq!(points(&s, id), vec![(100.0, 100.0), (160.0, 100.0), (160.0, 200.0)]);
    // Each is one undo step.
    s.execute("edit.undo", &json!({})).unwrap();
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(points(&s, id), vec![(100.0, 100.0), (100.0, 200.0), (160.0, 200.0)]);
    // Nothing selected: not available.
    s.execute("select.none", &json!({})).unwrap();
    assert!(s.execute("object.mirrorHorizontal", &json!({})).is_err());
}
