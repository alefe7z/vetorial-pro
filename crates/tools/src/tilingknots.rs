//! Vetorial: Tiling's gap handles (Inkscape's Tiling knots). On a selected object with Tiling the
//! Selection tool shows its first cell as a dashed frame and two handles: one right of the first
//! tile's top-right corner (Gap X) and one below its bottom-left corner (Gap Y), half a gap out.
//! Dragging one sets that gap, live; Shift-clicking one sets it back to 0. The engine works out
//! the frames ([`TilingFrame`]) for the selection.

use serde_json::json;
use vectorcraft_doc::NodeId;
use vectorcraft_geom::{Point, Rect};

use crate::{Action, Cursor, Overlay, ToolContext};

/// The handles' colour (Inkscape's path-effect knots are pink).
const KNOT: [u8; 3] = [0xE0, 0x45, 0x7B];
/// How near (screen px) a press must come to a handle to take it.
const REACH_PX: f64 = 7.0;

/// A selected object's Tiling, as its handles need it.
#[derive(Clone, Debug, PartialEq)]
pub struct TilingFrame {
    pub id: NodeId,
    /// The effect's position among the object's own effects.
    pub index: usize,
    /// The first cell and the first tile, grown with the largest tile (see the effect's layout).
    pub cell: Rect,
    pub tile: Rect,
    pub grow: f64,
    pub gap_x: f64,
    pub gap_y: f64,
}

/// Which gap a handle sets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Axis {
    X,
    Y,
}

impl TilingFrame {
    fn handle(&self, axis: Axis) -> Point {
        match axis {
            Axis::X => Point::new(self.tile.x1 + self.gap_x * self.grow / 2.0, self.tile.y0),
            Axis::Y => Point::new(self.tile.x0, self.tile.y1 + self.gap_y * self.grow / 2.0),
        }
    }
    /// The gap that puts `axis`'s handle at `p`.
    fn gap_at(&self, axis: Axis, p: Point) -> f64 {
        let g = self.grow.max(1e-9);
        match axis {
            Axis::X => 2.0 * (p.x - self.tile.x1) / g,
            Axis::Y => 2.0 * (p.y - self.tile.y1) / g,
        }
    }
}

/// The handle of the selection's Tiling under `p`.
fn under(cx: &ToolContext, p: Point) -> Option<(TilingFrame, Axis)> {
    let reach = cx.tol(REACH_PX);
    cx.tiling.iter().find_map(|f| [Axis::X, Axis::Y].into_iter().find(|a| f.handle(*a).distance(p) <= reach).map(|a| (f.clone(), a)))
}

/// Is `p` over a gap handle? (The cursor it shows.)
pub fn cursor(cx: &ToolContext, p: Point) -> Option<Cursor> {
    under(cx, p).map(|(_, a)| if a == Axis::X { Cursor::ResizeH } else { Cursor::ResizeV })
}

/// A gap handle pressed (and maybe dragged).
#[derive(Clone, Debug)]
pub struct GapDrag {
    frame: TilingFrame,
    axis: Axis,
    moved: bool,
}

impl GapDrag {
    /// The handle a press at `p` takes, if any.
    pub fn hit(cx: &ToolContext, p: Point) -> Option<Self> {
        under(cx, p).map(|(frame, axis)| GapDrag { frame, axis, moved: false })
    }

    /// Has the drag changed the gap yet?
    pub fn adjusting(&self) -> bool {
        self.moved
    }

    fn set(&self, gap: f64) -> serde_json::Value {
        let key = if self.axis == Axis::X { "gapX" } else { "gapY" };
        json!({ "ids": [self.frame.id.0], "index": self.frame.index, "item": null, "params": { key: gap } })
    }

    /// The pointer dragged to `p`: the gap follows it.
    pub fn drag(&mut self, p: Point) -> Vec<Action> {
        let mut out = vec![];
        if !self.moved {
            self.moved = true;
            out.push(Action::Begin(if self.axis == Axis::X { "Gap X".into() } else { "Gap Y".into() }));
        }
        let gap = self.frame.gap_at(self.axis, p);
        if gap.is_finite() {
            out.push(Action::Preview("effect.setParams".into(), self.set(gap)));
        }
        out
    }

    /// The button released: a drag commits; a Shift-click sets the gap back to 0.
    pub fn finish(self, shift: bool) -> Vec<Action> {
        if self.moved {
            vec![Action::Commit]
        } else if shift {
            vec![Action::Exec("effect.setParams".into(), self.set(0.0))]
        } else {
            vec![]
        }
    }
}

/// The selection's Tiling frames and handles.
pub fn overlays(cx: &ToolContext) -> Vec<Overlay> {
    let mut o = vec![];
    for f in cx.tiling {
        o.push(Overlay::Path { path: vectorcraft_geom::shapes::rectangle(f.cell).to_bezpath(), color: KNOT, width: 1.0, dashed: true });
        for a in [Axis::X, Axis::Y] {
            o.push(Overlay::CurvePoint { p: f.handle(a), color: KNOT, filled: true, size: 7.0 });
        }
    }
    o
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testutil::*;

    #[test]
    fn a_gap_handle_drags_its_gap_and_shift_click_resets_it() {
        let (d, id) = doc_with_rect();
        let s = vectorcraft_doc::Selection::default();
        let p = paint();
        let frames = [TilingFrame {
            id,
            index: 0,
            cell: Rect::new(95.0, 95.0, 205.0, 205.0),
            tile: Rect::new(100.0, 100.0, 200.0, 200.0),
            grow: 1.0,
            gap_x: 10.0,
            gap_y: 10.0,
        }];
        let cx = ToolContext { tiling: &frames, ..cx(&d, &s, &p) };
        assert_eq!(cursor(&cx, Point::new(205.0, 100.0)), Some(Cursor::ResizeH));
        assert_eq!(cursor(&cx, Point::new(100.0, 205.0)), Some(Cursor::ResizeV));
        assert_eq!(cursor(&cx, Point::new(150.0, 150.0)), None);
        let mut g = GapDrag::hit(&cx, Point::new(205.0, 101.0)).unwrap();
        let a = g.drag(Point::new(215.0, 101.0));
        assert_eq!(a[0], Action::Begin("Gap X".into()));
        assert_eq!(a[1], Action::Preview("effect.setParams".into(), json!({"ids": [id.0], "index": 0, "item": null, "params": {"gapX": 30.0}})));
        assert_eq!(g.finish(false), vec![Action::Commit]);
        let g = GapDrag::hit(&cx, Point::new(100.0, 205.0)).unwrap();
        assert_eq!(
            g.finish(true),
            vec![Action::Exec("effect.setParams".into(), json!({"ids": [id.0], "index": 0, "item": null, "params": {"gapY": 0.0}}))]
        );
        assert_eq!(overlays(&cx).len(), 3);
    }
}
