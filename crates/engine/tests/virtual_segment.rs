//! Virtual Segment Delete (`path.deleteVirtualSegment` and the `virtualSegmentDelete` tool):
//! a click deletes the run of a path between the points where it meets other paths, a marquee
//! deletes the runs wholly inside it. Original tiny geometry.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_engine::{Session, ViewInfo};
use vectorcraft_geom::Point;
use vectorcraft_tools::{Cursor, Mods, PointerEvent, PointerKind};

fn session() -> Session {
    let mut s = Session::new();
    s.execute("file.new", &json!({"width": 400, "height": 400})).unwrap();
    s
}

fn line(s: &mut Session, a: (f64, f64), b: (f64, f64)) -> NodeId {
    let r = s.execute("shape.line", &json!({"x1": a.0, "y1": a.1, "x2": b.0, "y2": b.1})).unwrap();
    NodeId(r["id"].as_u64().unwrap())
}

/// A horizontal line from x 0 to 300 at y 100, crossed by verticals at x 100 and 200.
fn ladder() -> (Session, NodeId, NodeId, NodeId) {
    let mut s = session();
    let h = line(&mut s, (0.0, 100.0), (300.0, 100.0));
    let v1 = line(&mut s, (100.0, 0.0), (100.0, 200.0));
    let v2 = line(&mut s, (200.0, 0.0), (200.0, 200.0));
    s.execute("select.none", &json!({})).ok();
    (s, h, v1, v2)
}

/// Each subpath's x extent (rounded), sorted.
fn spans(s: &Session, id: NodeId) -> Vec<(i64, i64, bool)> {
    let path = outline(s, id);
    let mut v: Vec<(i64, i64, bool)> = path
        .subpaths
        .iter()
        .map(|sp| {
            let xs: Vec<f64> = sp.anchors.iter().map(|a| a.p.x).collect();
            (
                xs.iter().cloned().fold(f64::INFINITY, f64::min).round() as i64,
                xs.iter().cloned().fold(f64::NEG_INFINITY, f64::max).round() as i64,
                sp.closed,
            )
        })
        .collect();
    v.sort();
    v
}

fn outline(s: &Session, id: NodeId) -> vectorcraft_geom::PathData {
    let all = vectorcraft_tools::vsegment::paths(&s.doc().unwrap().doc);
    all.into_iter().find(|(i, _)| *i == id).unwrap().1
}

fn live(s: &Session, id: NodeId) -> bool {
    s.doc().unwrap().doc.node(id).is_some()
}

#[test]
fn a_click_deletes_the_run_between_two_crossings() {
    let (mut s, h, v1, v2) = ladder();
    let r = s.execute("path.deleteVirtualSegment", &json!({"point": [150, 101]})).unwrap();
    assert_eq!(r["deleted"], 1);
    assert_eq!(spans(&s, h), vec![(0, 100, false), (200, 300, false)], "the middle run goes, the ends stay one object");
    assert_eq!(spans(&s, v1), vec![(100, 100, false)]);
    assert_eq!(spans(&s, v1).len(), 1);
    assert!(live(&s, v2));
    // One undo step brings it back whole.
    s.execute("edit.undo", &json!({})).unwrap();
    assert_eq!(spans(&s, h), vec![(0, 300, false)]);
}

#[test]
fn an_end_run_goes_from_the_end_to_the_first_crossing() {
    let (mut s, h, ..) = ladder();
    s.execute("path.deleteVirtualSegment", &json!({"point": [40, 100]})).unwrap();
    assert_eq!(spans(&s, h), vec![(100, 300, false)]);
    // Again on what is left: now 100–200 is the end run.
    s.execute("path.deleteVirtualSegment", &json!({"point": [120, 100]})).unwrap();
    assert_eq!(spans(&s, h), vec![(200, 300, false)]);
}

#[test]
fn anchors_between_crossings_do_not_end_a_run() {
    let mut s = session();
    let r = s.execute("path.create", &json!({"anchors": [{"x": 0, "y": 100}, {"x": 150, "y": 100}, {"x": 300, "y": 100}]})).unwrap();
    let h = NodeId(r["id"].as_u64().unwrap());
    line(&mut s, (100.0, 0.0), (100.0, 200.0));
    line(&mut s, (200.0, 0.0), (200.0, 200.0));
    s.execute("path.deleteVirtualSegment", &json!({"point": [130, 100]})).unwrap();
    assert_eq!(spans(&s, h), vec![(0, 100, false), (200, 300, false)]);
}

#[test]
fn a_path_nothing_crosses_goes_whole() {
    let mut s = session();
    let a = line(&mut s, (10.0, 10.0), (90.0, 10.0));
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 10]})).unwrap();
    assert!(!live(&s, a));
}

#[test]
fn a_closed_path_opens_where_the_run_was() {
    let mut s = session();
    let r = s.execute("shape.rectangle", &json!({"x": 0, "y": 0, "width": 200, "height": 100})).unwrap();
    let rect = NodeId(r["id"].as_u64().unwrap());
    line(&mut s, (100.0, -50.0), (100.0, 150.0));
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 0]})).unwrap();
    let p = outline(&s, rect);
    assert_eq!(p.subpaths.len(), 1);
    assert!(!p.subpaths[0].closed, "the rectangle is now an open path");
    let b = p.bounds().unwrap();
    // Corners are anchors, not crossings: the run is the whole left half, from the line round to it.
    assert!((b.x0 - 100.0).abs() < 0.1 && (b.x1 - 200.0).abs() < 0.1 && b.y0.abs() < 0.1 && (b.y1 - 100.0).abs() < 0.1, "{b:?}");
}

#[test]
fn a_marquee_deletes_only_the_runs_wholly_inside() {
    let (mut s, h, v1, v2) = ladder();
    let r = s.execute("path.deleteVirtualSegment", &json!({"rect": [90, 90, 210, 110]})).unwrap();
    assert_eq!(r["deleted"], 1, "{r}");
    assert_eq!(spans(&s, h), vec![(0, 100, false), (200, 300, false)]);
    assert!(live(&s, v1) && live(&s, v2));
    // A marquee taking in the whole of everything removes it all.
    s.execute("path.deleteVirtualSegment", &json!({"rect": [-10, -10, 310, 210]})).unwrap();
    assert!(!live(&s, h) && !live(&s, v1) && !live(&s, v2));
}

#[test]
fn hidden_and_locked_paths_are_left_alone() {
    let (mut s, h, ..) = ladder();
    s.execute("select.set", &json!({"ids": [h.0]})).unwrap();
    s.execute("object.lock", &json!({})).unwrap();
    assert!(s.execute("path.deleteVirtualSegment", &json!({"point": [150, 100]})).is_err());
    assert!(live(&s, h));
}

#[test]
fn bad_params_and_empty_places_are_errors() {
    let (mut s, ..) = ladder();
    for p in [json!({}), json!({"point": [1]}), json!({"rect": [0, 0, 1]}), json!({"point": ["a", 1]}), json!({"point": [350, 350]})] {
        assert!(s.execute("path.deleteVirtualSegment", &p).is_err(), "{p}");
    }
}

#[test]
fn the_tool_highlights_then_deletes_on_click_and_drag() {
    let (mut s, h, ..) = ladder();
    let view = ViewInfo { zoom: 1.0, smart_guides: false, snap_to_grid: false, snap_to_pixel: false, snap_to_point: false, ..Default::default() };
    s.select_tool("virtualSegmentDelete", view).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Move, 150.0, 300.0), view).unwrap();
    assert_eq!(s.cursor(Point::new(150.0, 300.0), Mods::default(), view), Cursor::VirtualSegment);
    s.pointer(&PointerEvent::new(PointerKind::Move, 150.0, 101.0), view).unwrap();
    assert_eq!(s.cursor(Point::new(150.0, 101.0), Mods::default(), view), Cursor::VirtualSegmentHot);
    s.pointer(&PointerEvent::new(PointerKind::Down, 150.0, 101.0), view).unwrap();
    s.pointer(&PointerEvent::new(PointerKind::Up, 150.0, 101.0), view).unwrap();
    assert_eq!(spans(&s, h), vec![(0, 100, false), (200, 300, false)]);
    // A drag around the right end run.
    for (k, x, y) in
        [(PointerKind::Down, 190.0, 80.0), (PointerKind::Drag, 250.0, 100.0), (PointerKind::Drag, 320.0, 120.0), (PointerKind::Up, 320.0, 120.0)]
    {
        s.pointer(&PointerEvent::new(k, x, y), view).unwrap();
    }
    assert_eq!(spans(&s, h), vec![(0, 100, false)]);
}

/// A path crossing itself like a fish (CorelDRAW's own demo): one tail goes, then the other, and
/// the loop stays, its ends meeting where the crossing was.
#[test]
fn a_self_crossing_path_loses_its_tails_one_by_one() {
    let mut s = session();
    let pts = [(0, 0), (300, 300), (400, 200), (300, 100), (0, 300)];
    let anchors: Vec<_> = pts.iter().map(|(x, y)| json!({"x": x, "y": y})).collect();
    let r = s.execute("path.create", &json!({ "anchors": anchors })).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    // The tails meet the loop at (180, 180).
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 50]})).unwrap();
    assert!(live(&s, id));
    let b = outline(&s, id).bounds().unwrap();
    assert!((b.x0 - 0.0).abs() < 0.5 && (b.y0 - 100.0).abs() < 0.5, "the first tail went: {b:?}");
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 266.7]})).unwrap();
    assert!(live(&s, id), "the loop stays");
    let p = outline(&s, id);
    let b = p.bounds().unwrap();
    assert!((b.x0 - 180.0).abs() < 0.5 && (b.x1 - 400.0).abs() < 0.5 && (b.y0 - 100.0).abs() < 0.5 && (b.y1 - 300.0).abs() < 0.5, "{b:?}");
}

/// Clicking the fish's loop takes only the loop: both tails stay, one object.
#[test]
fn a_self_crossing_paths_loop_goes_alone() {
    let mut s = session();
    let pts = [(0, 0), (300, 300), (400, 200), (300, 100), (0, 300)];
    let anchors: Vec<_> = pts.iter().map(|(x, y)| json!({"x": x, "y": y})).collect();
    let r = s.execute("path.create", &json!({ "anchors": anchors })).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    s.execute("path.deleteVirtualSegment", &json!({"point": [400, 200]})).unwrap();
    assert_eq!(spans(&s, id), vec![(0, 180, false), (0, 180, false)], "the two tails");
    // Then each tail on its own.
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 50]})).unwrap();
    assert_eq!(spans(&s, id), vec![(0, 180, false)]);
    s.execute("path.deleteVirtualSegment", &json!({"point": [50, 266.7]})).unwrap();
    assert!(!live(&s, id));
}

/// A closed figure eight: clicking one lobe leaves the other, open where they met.
#[test]
fn a_closed_figure_eight_loses_one_lobe() {
    let mut s = session();
    let pts = [(0, 0), (200, 200), (200, 0), (0, 200)];
    let anchors: Vec<_> = pts.iter().map(|(x, y)| json!({"x": x, "y": y})).collect();
    let r = s.execute("path.create", &json!({ "anchors": anchors, "closed": true })).unwrap();
    let id = NodeId(r["id"].as_u64().unwrap());
    // The lobes meet at (100, 100); the right lobe is (100,100)→(200,200)→(200,0)→(100,100).
    s.execute("path.deleteVirtualSegment", &json!({"point": [200, 100]})).unwrap();
    let p = outline(&s, id);
    let b = p.bounds().unwrap();
    assert!((b.x0 - 0.0).abs() < 0.5 && (b.x1 - 100.0).abs() < 0.5, "the left lobe stays: {b:?}");
}

/// The same with curves, as the fish is drawn in practice.
#[test]
fn a_curved_fish_loses_its_tails_one_by_one() {
    let mut s = session();
    let d = "M 0 300 C 150 300 250 100 350 100 C 450 100 450 300 350 300 C 250 300 150 100 0 100";
    let id = NodeId(s.execute("path.create", &json!({ "d": d })).unwrap()["id"].as_u64().unwrap());
    s.execute("path.deleteVirtualSegment", &json!({"point": [10, 300]})).unwrap();
    s.execute("path.deleteVirtualSegment", &json!({"point": [10, 100]})).unwrap();
    assert!(live(&s, id), "the loop stays");
    let p = outline(&s, id);
    assert_eq!(p.subpaths.len(), 1);
    let b = p.bounds().unwrap();
    // The curves cross halfway along, at (193.75, 200).
    assert!((b.x0 - 193.75).abs() < 0.01 && (b.x1 - 425.0).abs() < 0.01, "{b:?}");
    let sp = &p.subpaths[0];
    let (first, last) = (sp.anchors.first().unwrap().p, sp.anchors.last().unwrap().p);
    assert!(first.distance(last) < 1e-6, "its ends meet where the crossing was: {first:?} {last:?}");
}
