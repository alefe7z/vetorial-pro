//! Vetorial: CorelDRAW-style curve tools, beside the Illustrator-style ones.
//!
//! - **Bézier** (`bezier`): the Pen in Corel mode ([`crate::pen::PenTool::corel`]). Click adds a
//!   cusp node, click-drag a smooth node with symmetric handles (Alt breaks them); a click on the
//!   first node closes the curve; a click on an end of an open curve goes on from it. There is no
//!   rubber band: the next segment appears when the next node is placed. A double-click, Enter or
//!   Esc ends the curve; the readout shows the length of the segment to the pointer and of the
//!   whole curve.
//! - **B-Spline** (`bspline`): each click places a control point and a smooth cubic B-spline runs
//!   inside the dashed control polygon, from the first point to the last. A click on the first
//!   point (three or more placed) closes it into a periodic loop; a drag after the press moves the
//!   point just placed; a double-click, Enter or Esc ends it; Backspace/Delete takes the last point
//!   back. The finished curve is an ordinary Bézier path (one `path.create`, one undo step).
//! - **Polyline** (`polyline`): a click adds a straight segment, a drag draws freehand from the
//!   last point; a click on the first point closes it; a double-click, Enter or Esc ends it.
//! - **3-Point Curve** (`threePointCurve`): drag from the start to the end point, release, then
//!   move to bend the curve and click: it passes through that point.
//!
//! Every point placed snaps to Smart Guides ([`DrawSnap`]) as the Pen's anchors do; Shift keeps a
//! segment at 45° steps.

use kurbo::Shape;
use serde_json::{Value, json};
use vectorcraft_geom::{Anchor, AnchorKind, BezPath, Point, SubPath};

use crate::draw2::FEEDBACK;
use crate::guides::{DrawSnap, Leave};
use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext, ToolKey};

/// Create a Corel-style tool by id (None = not one of these).
pub fn create(id: &str) -> Option<Box<dyn Tool>> {
    Some(match id {
        "bezier" => Box::new(crate::pen::PenTool::corel()),
        "bspline" => Box::new(BSplineTool::default()),
        "polyline" => Box::new(PolylineTool::default()),
        "threePointCurve" => Box::new(ThreePointCurveTool::default()),
        _ => return None,
    })
}

// ---------------------------------------------------------------- geometry

/// The cubic B-spline over the control points `ctrl`, as Bézier anchors. Open, it is clamped: it
/// starts at the first point heading to the second and ends at the last (two points make a line,
/// three a single curve); closed (three points or more), it is periodic, a smooth loop.
pub fn bspline(ctrl: &[Point], closed: bool) -> SubPath {
    let n = ctrl.len();
    if closed && n >= 3 {
        let at = |i: usize| ctrl.get(i % n).copied().unwrap_or(Point::ZERO);
        let anchors = (0..n)
            .map(|i| {
                let (a, b, c) = (at(i + n - 1), at(i), at(i + 1));
                let p = Point::new((a.x + 4.0 * b.x + c.x) / 6.0, (a.y + 4.0 * b.y + c.y) / 6.0);
                Anchor { p, h_in: a.lerp(b, 2.0 / 3.0), h_out: b.lerp(c, 1.0 / 3.0), kind: AnchorKind::Smooth }
            })
            .collect();
        return SubPath::new(anchors, true);
    }
    match ctrl {
        [] | [_] | [_, _] => SubPath::polyline(ctrl, false),
        [a, b, c] => {
            // The quadratic Bézier a-b-c, raised to a cubic.
            let first = Anchor { p: *a, h_in: *a, h_out: a.lerp(*b, 2.0 / 3.0), kind: AnchorKind::Corner };
            let last = Anchor { p: *c, h_in: c.lerp(*b, 2.0 / 3.0), h_out: *c, kind: AnchorKind::Corner };
            SubPath::new(vec![first, last], false)
        }
        _ => clamped(ctrl),
    }
}

/// An open cubic B-spline on a clamped uniform knot vector (four or more control points), split
/// into its Bézier segments by knot insertion.
fn clamped(ctrl: &[Point]) -> SubPath {
    const P: usize = 3;
    let n = ctrl.len();
    // n + 4 knots: 0 0 0 0 1 2 … n-4 n-3 n-3 n-3 n-3.
    let last = (n - P) as f64;
    let mut knots: Vec<f64> = std::iter::repeat_n(0.0, P + 1).chain((1..n - P).map(|k| k as f64)).chain(std::iter::repeat_n(last, P + 1)).collect();
    let mut pts = ctrl.to_vec();
    for k in 1..n - P {
        // Each interior knot once more and again: multiplicity 3 splits the curve there.
        for _ in 0..P - 1 {
            insert_knot(&mut pts, &mut knots, k as f64);
        }
    }
    let segments = (pts.len() - 1) / P;
    let mut anchors = Vec::with_capacity(segments + 1);
    for j in 0..=segments {
        let i = j * P;
        let p = pts.get(i).copied().unwrap_or(Point::ZERO);
        let h_in = if j == 0 { p } else { pts.get(i - 1).copied().unwrap_or(p) };
        let h_out = if j == segments { p } else { pts.get(i + 1).copied().unwrap_or(p) };
        let kind = if j == 0 || j == segments { AnchorKind::Corner } else { AnchorKind::Smooth };
        anchors.push(Anchor { p, h_in, h_out, kind });
    }
    SubPath::new(anchors, false)
}

/// Insert the knot `u` once into the cubic B-spline (`pts`, `knots`), keeping its shape (Boehm).
fn insert_knot(pts: &mut Vec<Point>, knots: &mut Vec<f64>, u: f64) {
    const P: usize = 3;
    // The span: the last knot at or before u that still starts a span.
    let Some(s) = knots.iter().rposition(|k| *k <= u).map(|s| s.min(knots.len().saturating_sub(P + 2))) else { return };
    let mult = knots.iter().filter(|k| (**k - u).abs() < 1e-12).count();
    if s < P || mult > P {
        return;
    }
    let mut out = Vec::with_capacity(pts.len() + 1);
    for i in 0..=pts.len() {
        let q = if i + P <= s {
            pts.get(i).copied()
        } else if i + mult > s {
            pts.get(i.wrapping_sub(1)).copied()
        } else {
            let (ki, kp) = (knots.get(i).copied().unwrap_or(0.0), knots.get(i + P).copied().unwrap_or(0.0));
            let a = if (kp - ki).abs() < 1e-12 { 0.0 } else { (u - ki) / (kp - ki) };
            match (i.checked_sub(1).and_then(|j| pts.get(j)), pts.get(i)) {
                (Some(a0), Some(a1)) => Some(a0.lerp(*a1, a)),
                _ => None,
            }
        };
        let Some(q) = q else { return };
        out.push(q);
    }
    *pts = out;
    knots.insert(s + 1, u);
}

/// `anchors` for `path.create`: points with their handles (none where a handle is on its anchor).
pub fn anchors_json(sp: &SubPath) -> Value {
    Value::Array(
        sp.anchors
            .iter()
            .map(|a| {
                let mut v = json!({"x": a.p.x, "y": a.p.y});
                if a.h_in.distance(a.p) > 1e-9 {
                    v["in"] = json!([a.h_in.x, a.h_in.y]);
                }
                if a.h_out.distance(a.p) > 1e-9 {
                    v["out"] = json!([a.h_out.x, a.h_out.y]);
                }
                if a.kind == AnchorKind::Smooth {
                    v["smooth"] = json!(true);
                }
                v
            })
            .collect(),
    )
}

/// The `path.create` that makes the finished curve.
fn create_path(sp: &SubPath) -> Action {
    Action::Exec("path.create".into(), json!({"anchors": anchors_json(sp), "closed": sp.closed}))
}

/// Ramer–Douglas–Peucker: the points of a freehand stroke a polyline needs within `tol`.
pub fn simplify(pts: &[Point], tol: f64) -> Vec<Point> {
    if pts.len() < 3 {
        return pts.to_vec();
    }
    let mut keep = vec![false; pts.len()];
    keep[0] = true;
    keep[pts.len() - 1] = true;
    let mut stack = vec![(0usize, pts.len() - 1)];
    while let Some((a, b)) = stack.pop() {
        let (pa, pb) = (pts[a], pts[b]);
        let (mut far, mut at) = (0.0, None);
        for (i, p) in pts.iter().enumerate().take(b).skip(a + 1) {
            let d = segment_distance(*p, pa, pb);
            if d > far {
                (far, at) = (d, Some(i));
            }
        }
        if let Some(i) = at
            && far > tol
        {
            keep[i] = true;
            stack.push((a, i));
            stack.push((i, b));
        }
    }
    pts.iter().zip(keep).filter_map(|(p, k)| k.then_some(*p)).collect()
}

fn segment_distance(p: Point, a: Point, b: Point) -> f64 {
    let ab = b - a;
    let len2 = ab.hypot2();
    if len2 < 1e-18 {
        return p.distance(a);
    }
    let t = ((p - a).dot(ab) / len2).clamp(0.0, 1.0);
    p.distance(a + ab * t)
}

/// The quadratic through `a` and `b` passing through `m` halfway, as two cubic anchors.
pub fn three_point_curve(a: Point, b: Point, m: Point) -> SubPath {
    let mid = a.midpoint(b);
    let c = Point::new(2.0 * m.x - mid.x, 2.0 * m.y - mid.y);
    let first = Anchor { p: a, h_in: a, h_out: a.lerp(c, 2.0 / 3.0), kind: AnchorKind::Corner };
    let last = Anchor { p: b, h_in: b.lerp(c, 2.0 / 3.0), h_out: b, kind: AnchorKind::Corner };
    SubPath::new(vec![first, last], false)
}

/// `sp` as a path to draw or measure.
pub fn bez(sp: &SubPath) -> BezPath {
    let mut bp = BezPath::new();
    sp.to_bezpath_into(&mut bp);
    bp
}

/// The length of a path, to 0.01 pt.
pub fn length(bp: &BezPath) -> f64 {
    bp.perimeter(0.01)
}

/// The Corel-style readout by the pointer: the segment being drawn and the whole curve.
pub fn length_readout(cx: &ToolContext, at: Point, segment: f64, total: f64) -> Option<Overlay> {
    cx.measurement_labels.then(|| Overlay::Measure { p: at, text: format!("Segmento: {}\nComprimento total: {}", cx.len(segment), cx.len(total)) })
}

/// How near (document units) a press must be to the first point to close.
fn close_tol(cx: &ToolContext) -> f64 {
    cx.tol(6.0).max(cx.point_tol())
}

// ---------------------------------------------------------------- B-Spline

#[derive(Default)]
pub struct BSplineTool {
    /// The control points placed.
    pts: Vec<Point>,
    /// The last point is being dragged into place.
    dragging: bool,
    hover: Option<Point>,
    snap: DrawSnap,
}

impl BSplineTool {
    fn closes(&self, cx: &ToolContext, p: Point) -> bool {
        self.pts.len() >= 3 && self.pts.first().is_some_and(|f| f.distance(p) <= close_tol(cx))
    }

    /// The curve as drawn so far, through the pointer while it hovers.
    fn preview(&self, cx: &ToolContext) -> Option<SubPath> {
        let mut pts = self.pts.clone();
        let mut closed = false;
        if !self.dragging
            && let Some(h) = self.hover
        {
            if self.closes(cx, h) {
                closed = true;
            } else if pts.last().is_none_or(|l| l.distance(h) > 1e-9) {
                pts.push(h);
            }
        }
        (pts.len() >= 2).then(|| bspline(&pts, closed))
    }

    fn finish(&mut self, closed: bool) -> Vec<Action> {
        let pts = std::mem::take(&mut self.pts);
        self.dragging = false;
        self.snap.clear();
        if pts.len() < 2 {
            return vec![];
        }
        vec![create_path(&bspline(&pts, closed && pts.len() >= 3))]
    }
}

impl Tool for BSplineTool {
    fn id(&self) -> &'static str {
        "bspline"
    }
    fn busy(&self) -> bool {
        !self.pts.is_empty()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let from = self.pts.last().map(|l| Leave::segment(cx, *l, ev.mods.shift));
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(self.snap.hover(cx, ev.pos, &[], from.as_ref()));
                vec![]
            }
            PointerKind::Down => {
                if self.closes(cx, ev.pos) {
                    return self.finish(true);
                }
                let p = self.snap.press(cx, ev.pos, &[], from.as_ref());
                self.pts.push(p);
                self.hover = Some(p);
                self.dragging = true;
                vec![]
            }
            PointerKind::Drag => {
                if self.dragging {
                    let before = self.pts.len().checked_sub(2).and_then(|i| self.pts.get(i)).map(|l| Leave::segment(cx, *l, ev.mods.shift));
                    let p = self.snap.drag(cx, ev.pos, before.as_ref());
                    if let Some(last) = self.pts.last_mut() {
                        *last = p;
                    }
                    self.hover = Some(p);
                }
                vec![]
            }
            PointerKind::Up => {
                self.dragging = false;
                self.snap.clear();
                vec![]
            }
            PointerKind::DoubleClick => {
                // The double-click's second press placed a point on the one before it.
                if let [.., a, b] = self.pts.as_slice()
                    && a.distance(*b) <= close_tol(cx)
                {
                    self.pts.pop();
                }
                self.finish(false)
            }
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        match key {
            ToolKey::Enter | ToolKey::Escape => self.finish(false),
            ToolKey::Backspace | ToolKey::Delete => {
                self.pts.pop();
                vec![]
            }
            _ => vec![],
        }
    }
    fn claims_key(&self, _cx: &ToolContext, key: ToolKey) -> bool {
        !self.pts.is_empty() && matches!(key, ToolKey::Backspace | ToolKey::Delete | ToolKey::Enter | ToolKey::Escape)
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.finish(false)
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let mut o = vec![];
        let mut poly = self.pts.clone();
        if !self.dragging
            && let Some(h) = self.hover
            && !self.pts.is_empty()
        {
            poly.push(if self.closes(cx, h) { self.pts.first().copied().unwrap_or(h) } else { h });
        }
        for w in poly.windows(2) {
            if let [a, b] = w {
                o.push(Overlay::Line { a: *a, b: *b, color: FEEDBACK, dashed: true });
            }
        }
        if let Some(sp) = self.preview(cx) {
            let bp = bez(&sp);
            if let (Some(h), Some(l)) = (self.hover, self.pts.last())
                && let Some(m) = length_readout(cx, h, h.distance(*l), length(&bp))
            {
                o.push(m);
            }
            o.push(Overlay::Path { path: bp, color: FEEDBACK, width: 1.0, dashed: false });
        }
        let n = self.pts.len();
        for (i, p) in self.pts.iter().enumerate() {
            o.push(Overlay::CurvePoint { p: *p, color: FEEDBACK, filled: i + 1 == n, size: 6.0 });
        }
        o.extend_from_slice(self.snap.guides());
        o
    }
    fn cursor(&self, cx: &ToolContext, p: Point, _m: Mods) -> Cursor {
        if self.closes(cx, p) { Cursor::PenClose } else { Cursor::Crosshair }
    }
}

// ---------------------------------------------------------------- Polyline

#[derive(Default)]
pub struct PolylineTool {
    /// The nodes placed.
    pts: Vec<Point>,
    /// The press of the gesture going on and whether it turned into a freehand drag.
    press: Option<Point>,
    free: bool,
    /// The freehand stroke of the drag, from the press.
    stroke: Vec<Point>,
    hover: Option<Point>,
    snap: DrawSnap,
}

impl PolylineTool {
    fn closes(&self, cx: &ToolContext, p: Point) -> bool {
        self.pts.len() >= 2 && self.pts.first().is_some_and(|f| f.distance(p) <= close_tol(cx))
    }

    fn finish(&mut self, closed: bool) -> Vec<Action> {
        let pts = std::mem::take(&mut self.pts);
        (self.press, self.free) = (None, false);
        self.stroke.clear();
        self.snap.clear();
        if pts.len() < 2 {
            return vec![];
        }
        vec![create_path(&SubPath::polyline(&pts, closed && pts.len() >= 3))]
    }

    fn drawn(&self) -> Vec<Point> {
        let mut pts = self.pts.clone();
        if self.free {
            pts.extend(self.stroke.iter().skip(1));
        }
        pts
    }
}

impl Tool for PolylineTool {
    fn id(&self) -> &'static str {
        "polyline"
    }
    fn busy(&self) -> bool {
        !self.pts.is_empty()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let from = self.pts.last().map(|l| Leave::segment(cx, *l, ev.mods.shift));
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(self.snap.hover(cx, ev.pos, &[], from.as_ref()));
                vec![]
            }
            PointerKind::Down => {
                if self.closes(cx, ev.pos) {
                    return self.finish(true);
                }
                let p = self.snap.press(cx, ev.pos, &[], from.as_ref());
                // A press on the last node goes on from it (a drag from there draws freehand).
                let p = match self.pts.last() {
                    Some(l) if l.distance(p) <= close_tol(cx) => *l,
                    _ => {
                        self.pts.push(p);
                        p
                    }
                };
                (self.press, self.free) = (Some(p), false);
                self.stroke = vec![p];
                self.hover = Some(p);
                vec![]
            }
            PointerKind::Drag => {
                let Some(start) = self.press else { return vec![] };
                if !self.free && ev.pos.distance(start) > cx.tol(4.0) {
                    self.free = true;
                }
                if self.free && self.stroke.last().is_none_or(|l| l.distance(ev.pos) >= cx.tol(1.5)) {
                    self.stroke.push(ev.pos);
                }
                self.hover = Some(ev.pos);
                vec![]
            }
            PointerKind::Up => {
                if self.press.take().is_some() && self.free {
                    let kept = simplify(&self.stroke, cx.tol(1.0));
                    self.pts.extend(kept.into_iter().skip(1));
                }
                self.free = false;
                self.stroke.clear();
                self.snap.clear();
                vec![]
            }
            PointerKind::DoubleClick => {
                if let [.., a, b] = self.pts.as_slice()
                    && a.distance(*b) <= close_tol(cx)
                {
                    self.pts.pop();
                }
                self.finish(false)
            }
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        match key {
            ToolKey::Enter | ToolKey::Escape => self.finish(false),
            ToolKey::Backspace | ToolKey::Delete => {
                self.pts.pop();
                vec![]
            }
            _ => vec![],
        }
    }
    fn claims_key(&self, _cx: &ToolContext, key: ToolKey) -> bool {
        !self.pts.is_empty() && matches!(key, ToolKey::Backspace | ToolKey::Delete | ToolKey::Enter | ToolKey::Escape)
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.finish(false)
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let mut o = vec![];
        let mut pts = self.drawn();
        if self.press.is_none()
            && let (Some(h), false) = (self.hover, pts.is_empty())
        {
            pts.push(if self.closes(cx, h) { self.pts.first().copied().unwrap_or(h) } else { h });
        }
        let bp = crate::draw2::polyline(&pts);
        if let (Some(h), Some(l)) = (self.hover, self.drawn().last())
            && let Some(m) = length_readout(cx, h, h.distance(*l), length(&bp))
        {
            o.push(m);
        }
        o.push(Overlay::Path { path: bp, color: FEEDBACK, width: 1.0, dashed: false });
        for p in &self.pts {
            o.push(Overlay::Anchor { p: *p, color: FEEDBACK, filled: false, size: 5.0 });
        }
        o.extend_from_slice(self.snap.guides());
        o
    }
    fn cursor(&self, cx: &ToolContext, p: Point, _m: Mods) -> Cursor {
        if self.closes(cx, p) { Cursor::PenClose } else { Cursor::Crosshair }
    }
}

// ---------------------------------------------------------------- 3-Point Curve

#[derive(Default)]
pub struct ThreePointCurveTool {
    /// The start point, then the end point once the drag ended: the next click bends the curve.
    start: Option<Point>,
    end: Option<Point>,
    dragging: bool,
    hover: Option<Point>,
    snap: DrawSnap,
}

impl ThreePointCurveTool {
    fn reset(&mut self) {
        (self.start, self.end, self.dragging) = (None, None, false);
        self.snap.clear();
    }

    /// The curve through the pointer, while it waits for the click that bends it.
    fn bent(&self) -> Option<SubPath> {
        let (a, b) = (self.start?, self.end?);
        (!self.dragging).then(|| three_point_curve(a, b, self.hover.unwrap_or(a.midpoint(b))))
    }
}

impl Tool for ThreePointCurveTool {
    fn id(&self) -> &'static str {
        "threePointCurve"
    }
    fn busy(&self) -> bool {
        self.start.is_some()
    }
    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        match ev.kind {
            PointerKind::Move => {
                self.hover = Some(self.snap.hover(cx, ev.pos, &[], None));
                vec![]
            }
            PointerKind::Down => {
                if let (Some(a), Some(b)) = (self.start, self.end) {
                    let m = self.snap.press(cx, ev.pos, &[], None);
                    self.reset();
                    return vec![create_path(&three_point_curve(a, b, m))];
                }
                let p = self.snap.press(cx, ev.pos, &[], None);
                (self.start, self.end, self.dragging) = (Some(p), Some(p), true);
                vec![]
            }
            PointerKind::Drag => {
                if self.dragging
                    && let Some(a) = self.start
                {
                    let leave = Leave::segment(cx, a, ev.mods.shift);
                    self.end = Some(self.snap.drag(cx, ev.pos, Some(&leave)));
                }
                vec![]
            }
            PointerKind::Up => {
                if self.dragging {
                    self.dragging = false;
                    self.snap.clear();
                    // A click without a drag makes no curve.
                    if let (Some(a), Some(b)) = (self.start, self.end)
                        && a.distance(b) <= cx.tol(3.0)
                    {
                        self.reset();
                    }
                }
                vec![]
            }
            PointerKind::DoubleClick => vec![],
        }
    }
    fn key(&mut self, _cx: &ToolContext, key: ToolKey, _m: Mods) -> Vec<Action> {
        match key {
            ToolKey::Escape => {
                self.reset();
                vec![]
            }
            ToolKey::Enter => {
                let Some(sp) = self.bent() else { return vec![] };
                self.reset();
                vec![create_path(&sp)]
            }
            _ => vec![],
        }
    }
    fn claims_key(&self, _cx: &ToolContext, key: ToolKey) -> bool {
        self.start.is_some() && matches!(key, ToolKey::Enter | ToolKey::Escape)
    }
    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.reset();
        vec![]
    }
    fn overlays(&self, cx: &ToolContext) -> Vec<Overlay> {
        let mut o = vec![];
        let (Some(a), Some(b)) = (self.start, self.end) else {
            o.extend_from_slice(self.snap.guides());
            return o;
        };
        let bp = match self.bent() {
            Some(sp) => bez(&sp),
            None => crate::draw2::polyline(&[a, b]),
        };
        if let Some(m) = length_readout(cx, if self.dragging { b } else { self.hover.unwrap_or(b) }, a.distance(b), length(&bp)) {
            o.push(m);
        }
        o.push(Overlay::Path { path: bp, color: FEEDBACK, width: 1.0, dashed: false });
        o.push(Overlay::Anchor { p: a, color: FEEDBACK, filled: false, size: 5.0 });
        o.push(Overlay::Anchor { p: b, color: FEEDBACK, filled: false, size: 5.0 });
        o.extend_from_slice(self.snap.guides());
        o
    }
    fn cursor(&self, _cx: &ToolContext, _p: Point, _m: Mods) -> Cursor {
        Cursor::Crosshair
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::{cx, doc_with_rect, paint};
    use kurbo::ParamCurve;
    use vectorcraft_doc::Selection;

    fn pt(x: f64, y: f64) -> Point {
        Point::new(x, y)
    }

    /// The point at parameter t of the B-spline evaluated straight from its definition (de Boor),
    /// for checking the Bézier conversion.
    fn de_boor(ctrl: &[Point], knots: &[f64], u: f64) -> Point {
        let p = 3;
        let n = ctrl.len();
        let k = (p..n).rev().find(|&i| knots[i] <= u).unwrap_or(p);
        let mut d: Vec<Point> = (0..=p).map(|j| ctrl[j + k - p]).collect();
        for r in 1..=p {
            for j in (r..=p).rev() {
                let i = j + k - p;
                let den = knots[i + p + 1 - r] - knots[i];
                let a = if den == 0.0 { 0.0 } else { (u - knots[i]) / den };
                d[j] = d[j - 1].lerp(d[j], a);
            }
        }
        d[p]
    }

    #[test]
    fn open_bspline_matches_de_boor_and_hits_its_ends() {
        let ctrl = [pt(0.0, 0.0), pt(40.0, 80.0), pt(100.0, 90.0), pt(140.0, 10.0), pt(200.0, 60.0), pt(230.0, 0.0)];
        let sp = bspline(&ctrl, false);
        let n = ctrl.len();
        assert_eq!(sp.anchors.len(), n - 2, "one Bézier segment per knot span");
        assert_eq!(sp.anchors[0].p, ctrl[0]);
        assert_eq!(sp.anchors.last().unwrap().p, ctrl[n - 1]);
        let knots: Vec<f64> = [0.0; 4].into_iter().chain((1..n - 3).map(|k| k as f64)).chain([(n - 3) as f64; 4]).collect();
        for seg in 0..sp.segment_count() {
            for t in [0.0, 0.25, 0.5, 0.75, 1.0] {
                let got = sp.segment(seg).eval(t);
                let want = de_boor(&ctrl, &knots, seg as f64 + t - if seg + 1 == sp.segment_count() && t == 1.0 { 1e-12 } else { 0.0 });
                assert!(got.distance(want) < 1e-6, "segment {seg} t {t}: {got:?} vs {want:?}");
            }
        }
        // Smooth inside: the handles of every inner anchor are collinear.
        for a in &sp.anchors[1..sp.anchors.len() - 1] {
            let (i, o) = (a.p - a.h_in, a.h_out - a.p);
            assert!(i.cross(o).abs() < 1e-6 && i.dot(o) > 0.0);
            assert_eq!(a.kind, AnchorKind::Smooth);
        }
    }

    #[test]
    fn small_and_closed_bsplines() {
        assert_eq!(bspline(&[pt(0.0, 0.0), pt(10.0, 0.0)], false).anchors.len(), 2);
        let q = bspline(&[pt(0.0, 0.0), pt(50.0, 100.0), pt(100.0, 0.0)], false);
        assert_eq!(q.anchors.len(), 2);
        // The quadratic's apex at t = 0.5 is halfway to its control point.
        assert!(q.segment(0).eval(0.5).distance(pt(50.0, 50.0)) < 1e-9);
        let ring = bspline(&[pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 100.0), pt(0.0, 100.0)], true);
        assert!(ring.closed && ring.anchors.len() == 4);
        // A square's B-spline loop is symmetric about its centre.
        let c = pt(50.0, 50.0);
        let r0 = ring.anchors[0].p.distance(c);
        assert!(ring.anchors.iter().all(|a| (a.p.distance(c) - r0).abs() < 1e-9));
        assert!(bspline(&[], false).anchors.is_empty());
    }

    #[test]
    fn three_point_curve_passes_through_the_bend() {
        let sp = three_point_curve(pt(0.0, 0.0), pt(100.0, 0.0), pt(50.0, 40.0));
        assert!(sp.segment(0).eval(0.5).distance(pt(50.0, 40.0)) < 1e-9);
        assert_eq!(sp.anchors[0].p, pt(0.0, 0.0));
    }

    #[test]
    fn simplify_keeps_corners_and_drops_the_rest() {
        let line: Vec<Point> = (0..=10).map(|i| pt(f64::from(i) * 10.0, 0.0)).chain((1..=10).map(|i| pt(100.0, f64::from(i) * 10.0))).collect();
        assert_eq!(simplify(&line, 0.5), vec![pt(0.0, 0.0), pt(100.0, 0.0), pt(100.0, 100.0)]);
    }

    fn ev(kind: PointerKind, x: f64, y: f64) -> PointerEvent {
        PointerEvent::new(kind, x, y)
    }

    fn click(t: &mut dyn Tool, c: &ToolContext, x: f64, y: f64) -> Vec<Action> {
        let mut a = t.pointer(c, &ev(PointerKind::Down, x, y));
        a.extend(t.pointer(c, &ev(PointerKind::Up, x, y)));
        a
    }

    fn created(a: &[Action]) -> Option<&Value> {
        a.iter().find_map(|a| match a {
            Action::Exec(c, v) if c == "path.create" => Some(v),
            _ => None,
        })
    }

    #[test]
    fn bspline_tool_clicks_points_and_ends_on_a_double_click() {
        let (d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let mut t = create("bspline").unwrap();
        assert_eq!(t.id(), "bspline");
        for (x, y) in [(10.0, 400.0), (60.0, 300.0), (120.0, 420.0), (180.0, 300.0)] {
            assert!(click(t.as_mut(), &c, x, y).is_empty());
        }
        assert!(t.busy());
        t.pointer(&c, &ev(PointerKind::Move, 230.0, 420.0));
        assert!(t.overlays(&c).iter().any(|o| matches!(o, Overlay::Measure { .. })), "length readout");
        assert!(t.overlays(&c).iter().any(|o| matches!(o, Overlay::Line { dashed: true, .. })), "control polygon");
        // A double-click: its second press lands on the last point, which it doesn't add twice.
        let mut a = click(t.as_mut(), &c, 230.0, 420.0);
        a.extend(click(t.as_mut(), &c, 230.0, 420.0));
        a.extend(t.pointer(&c, &ev(PointerKind::DoubleClick, 230.0, 420.0)));
        let v = created(&a).expect("the curve is made");
        assert_eq!(v["closed"], false);
        assert_eq!(v["anchors"].as_array().unwrap().len(), 5 - 2, "five control points");
        assert_eq!(v["anchors"][0]["x"], 10.0);
        assert!(!t.busy());
    }

    #[test]
    fn bspline_tool_closes_on_the_first_point_and_takes_points_back() {
        let (d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let mut t = create("bspline").unwrap();
        for (x, y) in [(10.0, 400.0), (100.0, 300.0), (200.0, 400.0), (300.0, 300.0)] {
            click(t.as_mut(), &c, x, y);
        }
        assert!(t.key(&c, ToolKey::Backspace, Mods::default()).is_empty());
        assert_eq!(t.cursor(&c, pt(10.0, 400.0), Mods::default()), Cursor::PenClose);
        let a = click(t.as_mut(), &c, 11.0, 401.0);
        let v = created(&a).expect("closing makes the curve");
        assert_eq!(v["closed"], true);
        assert_eq!(v["anchors"].as_array().unwrap().len(), 3, "a periodic loop: one anchor per point");
    }

    #[test]
    fn polyline_clicks_and_drags() {
        let (d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let mut t = create("polyline").unwrap();
        click(t.as_mut(), &c, 0.0, 400.0);
        click(t.as_mut(), &c, 100.0, 400.0);
        // A freehand stretch: a right angle drawn by dragging.
        t.pointer(&c, &ev(PointerKind::Down, 100.0, 400.0));
        for i in 1..=20 {
            t.pointer(&c, &ev(PointerKind::Drag, 100.0, 400.0 - f64::from(i) * 5.0));
        }
        for i in 1..=20 {
            t.pointer(&c, &ev(PointerKind::Drag, 100.0 + f64::from(i) * 5.0, 300.0));
        }
        t.pointer(&c, &ev(PointerKind::Up, 200.0, 300.0));
        let a = t.key(&c, ToolKey::Enter, Mods::default());
        let v = created(&a).expect("Enter makes the polyline");
        let xs: Vec<(f64, f64)> = v["anchors"].as_array().unwrap().iter().map(|a| (a["x"].as_f64().unwrap(), a["y"].as_f64().unwrap())).collect();
        assert_eq!(xs, vec![(0.0, 400.0), (100.0, 400.0), (100.0, 300.0), (200.0, 300.0)]);
    }

    #[test]
    fn three_point_curve_tool_drag_then_click() {
        let (d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let mut t = create("threePointCurve").unwrap();
        t.pointer(&c, &ev(PointerKind::Down, 0.0, 400.0));
        t.pointer(&c, &ev(PointerKind::Drag, 50.0, 400.0));
        t.pointer(&c, &ev(PointerKind::Drag, 200.0, 400.0));
        assert!(t.pointer(&c, &ev(PointerKind::Up, 200.0, 400.0)).is_empty());
        t.pointer(&c, &ev(PointerKind::Move, 100.0, 350.0));
        let a = click(t.as_mut(), &c, 100.0, 350.0);
        let v = created(&a).expect("the click bends and makes the curve");
        assert_eq!(v["anchors"].as_array().unwrap().len(), 2);
        assert_eq!(v["anchors"][1]["x"], 200.0);
        assert!(!t.busy());
    }

    #[test]
    fn bezier_is_the_pen_without_rubber_band_ending_on_double_click() {
        let mut t = create("bezier").unwrap();
        assert_eq!(t.id(), "bezier");
        let (d, _) = doc_with_rect();
        let (s, p) = (Selection::default(), paint());
        let c = cx(&d, &s, &p);
        let a = t.pointer(&c, &ev(PointerKind::Down, 10.0, 400.0));
        assert!(matches!(a.as_slice(), [Action::Begin(_), Action::Preview(cmd, _)] if cmd == "path.create"), "{a:?}");
        t.pointer(&c, &ev(PointerKind::Up, 10.0, 400.0));
        t.pointer(&c, &ev(PointerKind::Move, 100.0, 300.0));
        assert!(!t.overlays(&c).iter().any(|o| matches!(o, Overlay::Path { .. })), "no rubber band");
        assert!(t.pointer(&c, &ev(PointerKind::DoubleClick, 10.0, 400.0)).is_empty());
        assert!(!t.busy());
    }
}
