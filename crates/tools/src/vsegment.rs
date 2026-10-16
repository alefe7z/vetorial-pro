//! Vetorial: the Virtual Segment Delete tool (`virtualSegmentDelete`), CorelDRAW's.
//!
//! A *virtual segment* is a run of a path between the points where it meets another path or
//! itself (or an end of an open path): anchors in between don't end it. Hovering one stands the
//! knife up and highlights it; a click deletes it; a drag deletes every virtual segment lying
//! wholly inside the marquee. Nothing needs to be selected: every visible, unlocked path counts,
//! as a target and as a cutter. Deleting opens a closed path there and splits an open one (the
//! pieces stay one object, a compound path); a path with nothing left goes. The tool emits
//! `path.deleteVirtualSegment`; the pieces come from the planar map Live Paint uses
//! ([`vectorcraft_pathops::live_paint`]).

use serde_json::json;
use vectorcraft_doc::{Document, NodeId};
use vectorcraft_geom::{BezPath, PathData, Point, Rect, SubPath};
use vectorcraft_pathops as po;

use crate::builder::shapes_for;
use crate::{Action, Cursor, Mods, Overlay, PointerEvent, PointerKind, Tool, ToolContext};

/// How close (screen px) the pointer must come to a path for a click to delete there.
pub const PICK_PX: f64 = 4.0;
/// Highlight of the virtual segment under the pointer.
pub const HIGHLIGHT: [u8; 3] = [0xff, 0x2a, 0x2a];
/// Most segments the cutters may have in all: past this only the nearest ones cut.
const MAX_SEGMENTS: usize = 20_000;

/// Paths with their outlines, as [`paths`] gives them.
pub type Paths = Vec<(NodeId, PathData)>;

/// Every visible, unlocked path (and compound path) of the document, with its outline: the
/// targets and the cutters.
pub fn paths(doc: &Document) -> Paths {
    let (shapes, leaves) = shapes_for(doc, &doc.selectable_art());
    leaves.into_iter().zip(shapes).filter(|(n, _)| n.visible && !n.locked).map(|(n, s)| (n.id, s.path)).collect()
}

/// The virtual segments of path `id` among `all` (from [`paths`]): its subpaths cut wherever they
/// meet another path or themselves. A subpath nothing crosses is one piece, whole.
pub fn pieces(all: &[(NodeId, PathData)], id: NodeId) -> Vec<SubPath> {
    let Some((_, target)) = all.iter().find(|(i, _)| *i == id) else { return Vec::new() };
    let Some(tb) = target.bounds() else { return Vec::new() };
    let near = tb.inflate(1e-6 * (1.0 + tb.width() + tb.height()), 1e-6 * (1.0 + tb.width() + tb.height()));
    let mut cutters: Vec<&PathData> = all.iter().filter(|(i, p)| *i != id && p.bounds().is_some_and(|b| overlaps(b, near))).map(|(_, p)| p).collect();
    // Many cutters: keep the closest (by bounds centre) within the budget.
    let budget = |cs: &[&PathData]| cs.iter().map(|p| p.subpaths.iter().map(SubPath::segment_count).sum::<usize>()).sum::<usize>();
    if budget(&cutters) > MAX_SEGMENTS {
        let c = tb.center();
        cutters.sort_by(|a, b| {
            let d = |p: &PathData| p.bounds().map_or(f64::INFINITY, |r| r.center().distance(c));
            d(a).total_cmp(&d(b))
        });
        while cutters.len() > 1 && budget(&cutters) > MAX_SEGMENTS {
            cutters.pop();
        }
    }
    // The target goes last (front-most): where it runs along a cutter, the shared piece is its.
    let key = cutters.len() as u64;
    let mut shapes: Vec<po::Shape> = cutters.iter().enumerate().map(|(i, p)| po::Shape::new((*p).clone(), Default::default(), i as u64)).collect();
    shapes.push(po::Shape::new(target.clone(), Default::default(), key));
    let (_, edges) = po::live_paint(&shapes);
    let mut out: Vec<SubPath> =
        edges.into_iter().filter(|e| e.key == key).flat_map(|e| e.path.subpaths).filter(|sp| sp.segment_count() > 0).collect();
    if out.is_empty() {
        // No map (degenerate input): each subpath is one virtual segment.
        out = target.subpaths.iter().filter(|sp| sp.segment_count() > 0).cloned().collect();
    }
    out
}

fn overlaps(a: Rect, b: Rect) -> bool {
    a.x0 <= b.x1 && b.x0 <= a.x1 && a.y0 <= b.y1 && b.y0 <= a.y1
}

/// The path whose outline passes nearest `p` within `tol` (the front-most of equals).
pub fn path_at(all: &[(NodeId, PathData)], p: Point, tol: f64) -> Option<NodeId> {
    let mut best: Option<(f64, NodeId)> = None;
    for (id, path) in all {
        if path.bounds().is_none_or(|b| !b.inflate(tol, tol).contains(p)) {
            continue;
        }
        if let Some((.., d)) = path.nearest(p)
            && d <= tol
            && best.is_none_or(|(bd, _)| d <= bd)
        {
            best = Some((d, *id));
        }
    }
    best.map(|(_, id)| id)
}

/// The virtual segment under `p` (within `tol`): its path and the piece.
pub fn segment_at(all: &[(NodeId, PathData)], p: Point, tol: f64) -> Option<(NodeId, SubPath)> {
    let id = path_at(all, p, tol)?;
    let piece = pieces(all, id)
        .into_iter()
        .filter_map(|sp| {
            let d = PathData::single(sp.clone()).nearest(p)?.4;
            Some((d, sp))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))?
        .1;
    Some((id, piece))
}

/// Does `sp` lie wholly inside `r`?
fn inside(sp: &SubPath, r: Rect) -> bool {
    let mut bp = BezPath::new();
    sp.to_bezpath_into(&mut bp);
    let b = kurbo::Shape::bounding_box(&bp);
    b.x0 >= r.x0 && b.x1 <= r.x1 && b.y0 >= r.y0 && b.y1 <= r.y1
}

/// The virtual segments lying wholly inside `r`, by path.
pub fn segments_in(all: &[(NodeId, PathData)], r: Rect) -> Vec<(NodeId, Vec<SubPath>)> {
    all.iter()
        .filter(|(_, p)| p.bounds().is_some_and(|b| overlaps(b, r)))
        .filter_map(|(id, _)| {
            let inner: Vec<SubPath> = pieces(all, *id).into_iter().filter(|sp| inside(sp, r)).collect();
            (!inner.is_empty()).then_some((*id, inner))
        })
        .collect()
}

/// The tool. Hover: the virtual segment under the pointer is highlighted and the knife stands up.
/// Press-release in place deletes it; a drag deletes what the marquee encloses.
#[derive(Default)]
pub struct VirtualSegmentTool {
    /// The paths, cached for the document revision they were read at.
    cache: Option<((u64, u64), Paths)>,
    /// The highlighted piece (document coordinates).
    hover: Option<SubPath>,
    /// Where a press started, and where the pointer is now (a marquee once it moves).
    press: Option<(Point, Point)>,
    dragging: bool,
}

impl VirtualSegmentTool {
    fn paths(&mut self, cx: &ToolContext) -> &[(NodeId, PathData)] {
        if self.cache.as_ref().is_none_or(|(rev, _)| *rev != cx.revision) {
            self.cache = Some((cx.revision, paths(cx.doc)));
        }
        self.cache.as_ref().map(|(_, p)| p.as_slice()).unwrap_or(&[])
    }

    fn marquee(&self) -> Option<Rect> {
        let (a, b) = self.press?;
        self.dragging.then(|| Rect::from_points(a, b))
    }
}

impl Tool for VirtualSegmentTool {
    fn id(&self) -> &'static str {
        "virtualSegmentDelete"
    }

    fn pointer(&mut self, cx: &ToolContext, ev: &PointerEvent) -> Vec<Action> {
        let tol = cx.tol(PICK_PX);
        match ev.kind {
            PointerKind::Move => {
                let all = self.paths(cx).to_vec();
                self.hover = segment_at(&all, ev.pos, tol).map(|(_, sp)| sp);
                vec![]
            }
            PointerKind::Down => {
                self.press = Some((ev.pos, ev.pos));
                self.dragging = false;
                vec![]
            }
            PointerKind::Drag => {
                if let Some((a, _)) = self.press {
                    // A few pixels' wobble is still a click.
                    if !self.dragging && a.distance(ev.pos) * cx.zoom.max(1e-9) >= 3.0 {
                        self.dragging = true;
                        self.hover = None;
                    }
                    self.press = Some((a, ev.pos));
                }
                vec![]
            }
            PointerKind::Up => {
                let Some((a, b)) = self.press.take() else { return vec![] };
                let dragged = std::mem::take(&mut self.dragging);
                self.hover = None;
                self.cache = None;
                if dragged {
                    let r = Rect::from_points(a, b);
                    vec![Action::Exec("path.deleteVirtualSegment".into(), json!({ "rect": [r.x0, r.y0, r.x1, r.y1] }))]
                } else {
                    vec![Action::Exec("path.deleteVirtualSegment".into(), json!({ "point": [a.x, a.y], "tolerance": tol }))]
                }
            }
            _ => vec![],
        }
    }

    fn overlays(&self, _cx: &ToolContext) -> Vec<Overlay> {
        let mut out = vec![];
        if let Some(r) = self.marquee() {
            out.push(Overlay::Marquee(r));
        }
        if let Some(sp) = &self.hover {
            let mut path = BezPath::new();
            sp.to_bezpath_into(&mut path);
            out.push(Overlay::Path { path, color: HIGHLIGHT, width: 2.0, dashed: false });
        }
        out
    }

    fn cursor(&self, _cx: &ToolContext, _p: Point, _m: Mods) -> Cursor {
        if self.hover.is_some() && !self.dragging { Cursor::VirtualSegmentHot } else { Cursor::VirtualSegment }
    }

    fn deactivate(&mut self, _cx: &ToolContext) -> Vec<Action> {
        self.press = None;
        self.dragging = false;
        self.hover = None;
        vec![]
    }
}
