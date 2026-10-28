//! Vetorial: Tiling (Inkscape's Tiling path effect, matched to how it behaves): the object
//! repeated in `rows` × `columns` tiles, the first where the object is.
//!
//! - The grid's cells are the object's bounds plus the gaps (`gapX`, `gapY`), as large as the
//!   largest tile; each gap can instead be random (`gapXRandom`, `gapYRandom`): every tile but the
//!   first moves back by a random share of it.
//! - `offset` % slides every other row sideways (`offsetAxis` "rows") or every other column down
//!   ("columns"), by that share of a cell.
//! - `scale` % is added to the tiles' size and `rotate` turns them (degrees, clockwise as in
//!   Inkscape), as the modes say: "rows" grows a step on each row down (the top row as it is),
//!   "columns" a step on each column across, "both" on each step of either, "uniform" every tile
//!   the same, "random" each tile a random share (Randomize, `seed`, draws again).
//! - The mirror flags flip the tiles of every other row (`mirrorRowsX` left to right,
//!   `mirrorRowsY` top to bottom) or column (`mirrorColsX`, `mirrorColsY`); rows and columns on the
//!   same axis flip a checkerboard. `mirrorTransforms` turns the flipped rows' (or columns') tiles
//!   the other way.
//! - `minimizeGaps` sizes each column and row by its own tiles (also when they turn), not the
//!   largest's.
//! - `split` leaves the first tile here: the others are separate objects the engine keeps
//!   (Split Elements).
//!
//! Older files' `mirror` (0..15: every other column flips as `mirror % 4`, every other row as
//! `mirror / 4`; 1 left to right, 2 top to bottom) and step mode "none" (no change) still read.

use serde_json::Value;
use vectorcraft_geom::{Affine, PathData, Point, Rect, Vec2};

use crate::util::*;

/// Most tiles the Tiling effect makes (rows × columns).
pub const MAX_TILES: usize = 40_000;

/// Where Tiling puts its tiles.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub rows: usize,
    pub cols: usize,
    /// Each tile's transform of the object, row by row (the first is row 0, column 0).
    pub tiles: Vec<Affine>,
    /// The first cell: the object's bounds and half of each gap around them, grown to the largest
    /// tile's size about the object's centre (the dashed frame shown on the canvas).
    pub cell: Rect,
    /// The object's bounds grown the same way: the gap handles sit at its corners.
    pub tile: Rect,
    /// How much the cell grew (the largest tile's scale).
    pub grow: f64,
}

impl Layout {
    /// Where the Gap X handle is: right of the first tile's top-right corner by half its gap.
    pub fn gap_x_handle(&self, gap_x: f64) -> Point {
        Point::new(self.tile.x1 + gap_x * self.grow / 2.0, self.tile.y0)
    }
    /// Where the Gap Y handle is: below the first tile's bottom-left corner by half its gap.
    pub fn gap_y_handle(&self, gap_y: f64) -> Point {
        Point::new(self.tile.x0, self.tile.y1 + gap_y * self.grow / 2.0)
    }
    /// The Gap X that puts its handle at page x `x`.
    pub fn gap_x_at(&self, x: f64) -> f64 {
        2.0 * (x - self.tile.x1) / self.grow.max(1e-9)
    }
    /// The Gap Y that puts its handle at page y `y`.
    pub fn gap_y_at(&self, y: f64) -> f64 {
        2.0 * (y - self.tile.y1) / self.grow.max(1e-9)
    }
}

/// How a scale or rotation steps from tile to tile.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Step {
    /// A step on each row down (Inkscape's Interpolate Y, the default).
    Rows,
    /// A step on each column across (Interpolate X).
    Columns,
    /// A step on each row and column (Interpolate both).
    Both,
    /// Every tile the same (No interpolation).
    Uniform,
    /// A random share for each tile.
    Random,
    /// Older files: no change.
    Off,
}

fn step(p: &Value, key: &str) -> Step {
    match text(p, key, "rows") {
        "columns" => Step::Columns,
        "both" => Step::Both,
        "uniform" => Step::Uniform,
        "random" => Step::Random,
        "none" => Step::Off,
        _ => Step::Rows,
    }
}

/// A random number in 0..1 for tile `k`, one draw per `salt`.
fn draw(seed: u64, k: usize, salt: u64) -> f64 {
    (noise(seed, k as u64, salt, 0x711E) + 1.0) / 2.0
}

/// The mirror flags (rows X, rows Y, columns X, columns Y), from the four flags or an older
/// file's `mirror`.
fn mirror_flags(p: &Value) -> [bool; 4] {
    let mut f = [flag(p, "mirrorRowsX", false), flag(p, "mirrorRowsY", false), flag(p, "mirrorColsX", false), flag(p, "mirrorColsY", false)];
    let old = num(p, "mirror", 0.0).clamp(0.0, 15.0) as usize;
    let (cols, rows) = (old % 4, old / 4);
    f[0] |= rows & 1 != 0;
    f[1] |= rows & 2 != 0;
    f[2] |= cols & 1 != 0;
    f[3] |= cols & 2 != 0;
    f
}

/// The first cell of Tiling with params `p` for an object of bounds `b` (the frame the gap handles
/// go on): (cell, tile, grow) as in [`Layout`], without working out every tile.
pub fn frame(b: Rect, p: &Value) -> Option<(Rect, Rect, f64)> {
    let rows = num(p, "rows", 3.0).clamp(1.0, 200.0) as usize;
    let cols = num(p, "columns", 3.0).clamp(1.0, 200.0) as usize;
    if rows * cols > MAX_TILES || !(b.width() > 0.0 || b.height() > 0.0) {
        return None;
    }
    let scale = num(p, "scale", 0.0).clamp(-100.0, 10_000.0) / 100.0;
    let steps = match step(p, "scaleMode") {
        Step::Rows => rows - 1,
        Step::Columns => cols - 1,
        Step::Both => rows + cols - 2,
        Step::Random => 1,
        Step::Uniform => return Some(cells(b, p, (1.0 + scale).max(0.01))),
        Step::Off => 0,
    };
    Some(cells(b, p, (1.0 + scale * steps as f64).max(1.0)))
}

/// The first cell and tile of an object of bounds `b` grown by `grow` (see [`Layout`]).
fn cells(b: Rect, p: &Value, grow: f64) -> (Rect, Rect, f64) {
    let gx = num(p, "gapX", 0.0).clamp(-1e6, 1e6);
    let gy = num(p, "gapY", 0.0).clamp(-1e6, 1e6);
    let about = |r: Rect| Rect::from_center_size(b.center(), (r.width() * grow, r.height() * grow));
    (about(b.inflate(gx / 2.0, gy / 2.0)), about(b), grow)
}

/// The layout of Tiling with params `p` for an object of bounds `b`; `None` when it makes no
/// tiles (empty bounds, too many tiles).
pub fn layout(b: Rect, p: &Value) -> Option<Layout> {
    let rows = num(p, "rows", 3.0).clamp(1.0, 200.0) as usize;
    let cols = num(p, "columns", 3.0).clamp(1.0, 200.0) as usize;
    if rows * cols > MAX_TILES || !(b.width() > 0.0 || b.height() > 0.0) {
        return None;
    }
    let gx = num(p, "gapX", 0.0).clamp(-1e6, 1e6);
    let gy = num(p, "gapY", 0.0).clamp(-1e6, 1e6);
    let (gx_random, gy_random) = (flag(p, "gapXRandom", false), flag(p, "gapYRandom", false));
    let offset = num(p, "offset", 0.0).clamp(-1000.0, 1000.0) / 100.0;
    let offset_cols = text(p, "offsetAxis", "rows") == "columns";
    let scale = num(p, "scale", 0.0).clamp(-100.0, 10_000.0) / 100.0;
    let rotate = num(p, "rotate", 0.0).clamp(-36_000.0, 36_000.0);
    let (smode, rmode) = (step(p, "scaleMode"), step(p, "rotateMode"));
    let [rx, ry, cx, cy] = mirror_flags(p);
    let mirror_turns = flag(p, "mirrorTransforms", false);
    let tight = flag(p, "minimizeGaps", false);
    let seed = seed(p);
    let c0 = b.center();
    let odd = |n: usize| n % 2 == 1;
    // Each tile's own transform about the object's centre, and its scale.
    let mut own: Vec<(Affine, f64)> = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let k = i * cols + j;
            let steps = |m: Step| match m {
                Step::Rows => i as f64,
                Step::Columns => j as f64,
                Step::Both => (i + j) as f64,
                Step::Uniform => 1.0,
                Step::Random | Step::Off => 0.0,
            };
            let s = match smode {
                Step::Random => {
                    let (lo, hi) = ((1.0 + scale).min(1.0), (1.0 + scale).max(1.0));
                    lo + (hi - lo) * draw(seed, k, 1)
                }
                m => 1.0 + scale * steps(m),
            }
            .max(0.01);
            let mut a = match rmode {
                Step::Random => (draw(seed, k, 2) - draw(seed, k, 3)) * rotate,
                m => rotate * steps(m),
            };
            // Mirror Transforms: the tiles of every other row (column, or square) turn back.
            let back = match rmode {
                Step::Columns => odd(i),
                Step::Rows => odd(j),
                Step::Both => odd(i + j),
                _ => false,
            };
            if mirror_turns && back {
                a = -a;
            }
            let flip = |by_rows: bool, by_cols: bool| match (by_rows, by_cols) {
                (true, true) => odd(i + j),
                (true, false) => odd(i),
                (false, true) => odd(j),
                _ => false,
            };
            let mx = if flip(rx, cx) { -1.0 } else { 1.0 };
            let my = if flip(ry, cy) { -1.0 } else { 1.0 };
            let m = Affine::translate(c0.to_vec2())
                * Affine::scale_non_uniform(mx, my)
                * Affine::rotate(a.to_radians())
                * Affine::scale(s)
                * Affine::translate(-c0.to_vec2());
            own.push((m, s));
        }
    }
    // The cells grow with the largest tile (shrink with a uniform scale below 100 %).
    let (cell, tile_box, grow) = frame(b, p)?;
    let (cell_w, cell_h) = ((b.width() + gx) * grow, (b.height() + gy) * grow);
    let tile = |i: usize, j: usize| own.get(i * cols + j);
    // Minimize Gaps: each column as wide and each row as tall as its own tiles (and their gaps).
    let (col_w, row_h): (Vec<f64>, Vec<f64>) = if tight {
        let size = |i: usize, j: usize| tile(i, j).map(|(m, s)| (m.transform_rect_bbox(b), *s));
        let cw = (0..cols).map(|j| (0..rows).filter_map(|i| size(i, j)).map(|(r, s)| r.width() + gx * s).fold(f64::MIN, f64::max).max(0.0)).collect();
        let rh =
            (0..rows).map(|i| (0..cols).filter_map(|j| size(i, j)).map(|(r, s)| r.height() + gy * s).fold(f64::MIN, f64::max).max(0.0)).collect();
        (cw, rh)
    } else {
        (vec![cell_w; cols], vec![cell_h; rows])
    };
    // The centres of the columns and rows, from the first tile's.
    let starts = |sizes: &[f64]| -> Vec<f64> {
        let mut at = 0.0;
        sizes
            .iter()
            .enumerate()
            .map(|(n, s)| {
                if n > 0 {
                    at += (sizes.get(n - 1).copied().unwrap_or(0.0) + s) / 2.0;
                }
                at
            })
            .collect()
    };
    let (xs, ys) = (starts(&col_w), starts(&row_h));
    let mut tiles = Vec::with_capacity(rows * cols);
    for i in 0..rows {
        for j in 0..cols {
            let k = i * cols + j;
            let Some((m, _)) = tile(i, j) else { continue };
            let (w, h) = (col_w.get(j).copied().unwrap_or(cell_w), row_h.get(i).copied().unwrap_or(cell_h));
            let mut d = Vec2::new(xs.get(j).copied().unwrap_or(0.0), ys.get(i).copied().unwrap_or(0.0));
            if offset != 0.0 {
                if offset_cols && odd(j) {
                    d.y += h * offset;
                } else if !offset_cols && odd(i) {
                    d.x += w * offset;
                }
            }
            if k > 0 {
                if gx_random {
                    d.x -= draw(seed, k, 4) * gx;
                }
                if gy_random {
                    d.y -= draw(seed, k, 5) * gy;
                }
            }
            tiles.push(Affine::translate(d) * *m);
        }
    }
    Some(Layout { rows, cols, tiles, cell, tile: tile_box, grow })
}

/// Tiling of `path` (an object of bounds `b`): every tile, or with `split` the first only.
pub fn tiling(path: &PathData, b: Rect, p: &Value) -> PathData {
    let Some(l) = layout(b, p) else { return path.clone() };
    if flag(p, "split", false) {
        return l.tiles.first().map_or_else(|| path.clone(), |m| path.transformed(*m));
    }
    let mut out = PathData::new(Vec::with_capacity(path.subpaths.len() * l.tiles.len()));
    for m in &l.tiles {
        out.subpaths.extend(path.transformed(*m).subpaths);
    }
    out
}
