//! The RIFF tree read as a drawing: the styles (`fild`, `outl`), the page size (`mcfg`), then the
//! pages, layers, groups and objects (`loda` with its `trfd` transforms).

use std::collections::HashMap;
use std::f64::consts::PI;

use kurbo::{Affine, BezPath, PathEl, Point, Rect, RoundedRect, RoundedRectRadii, Shape as _, Vec2};

use crate::container::{File, Record, u32_at};
use crate::{Color, Drawing, Error, Layer, Limits, Object, Page, Shape, Stroke};

const TEXT: &str = "text isn't imported yet";
const BITMAPS: &str = "bitmaps aren't imported yet";
const GRADIENTS: &str = "gradient fills come in as a solid colour";
const PATTERNS: &str = "pattern, texture and PostScript fills come in as a solid colour";
const SPOT: &str = "palette and spot colours come in as grey";
const COLORS: &str = "colours of a model the reader doesn't know come in as black";
const STYLES: &str = "objects painted only through a style come in without fill or outline";
const TOO_MANY: &str = "the drawing has more shapes than VectorCraft reads: the rest are left out";
const UNKNOWN: &str = "objects of kinds the reader doesn't know are left out";

/// Points per inch.
const PT: f64 = 72.0;

/// A little-endian reader over a record's data.
struct R<'a> {
    b: &'a [u8],
    at: usize,
    /// Coordinates are 16-bit thousandths of an inch (before CorelDRAW 6), else 32-bit tenths of
    /// a micrometre.
    short: bool,
}

impl<'a> R<'a> {
    fn new(b: &'a [u8], at: usize, short: bool) -> Self {
        Self { b, at, short }
    }
    fn take(&mut self, n: usize) -> Option<&'a [u8]> {
        let s = self.b.get(self.at..self.at.checked_add(n)?)?;
        self.at += n;
        Some(s)
    }
    fn skip(&mut self, n: usize) -> Option<()> {
        self.take(n).map(|_| ())
    }
    fn u8(&mut self) -> Option<u8> {
        self.take(1).map(|s| s[0])
    }
    fn u16(&mut self) -> Option<u16> {
        self.take(2).map(|s| u16::from_le_bytes([s[0], s[1]]))
    }
    fn u32(&mut self) -> Option<u32> {
        self.take(4).map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
    }
    fn i32(&mut self) -> Option<i32> {
        self.u32().map(|v| v as i32)
    }
    fn f64(&mut self) -> Option<f64> {
        let s = self.take(8)?;
        let v = f64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]);
        v.is_finite().then_some(v)
    }
    /// The size of a coordinate.
    fn coord_size(&self) -> usize {
        if self.short { 2 } else { 4 }
    }
    /// A coordinate, in points.
    fn coord(&mut self) -> Option<f64> {
        if self.short { self.u16().map(|v| f64::from(v as i16) / 1000.0 * PT) } else { self.i32().map(|v| f64::from(v) / 254_000.0 * PT) }
    }
    /// An angle, in radians.
    fn angle(&mut self) -> Option<f64> {
        if self.short { self.u16().map(|v| f64::from(v as i16) * PI / 1800.0) } else { self.i32().map(|v| f64::from(v) * PI / 180_000_000.0) }
    }
    /// A 16.16 fixed-point number.
    fn fixed(&mut self) -> Option<f64> {
        let v = self.u32()?;
        Some(f64::from((v >> 16) as u16 as i16) + f64::from(v & 0xffff) / 65536.0)
    }
}

/// A fill style.
#[derive(Clone, Copy)]
enum Fill {
    None,
    Solid(Color),
}

/// What an object's `loda` record says.
#[derive(Default)]
struct Loda {
    path: Option<BezPath>,
    fill: Option<u32>,
    outline: Option<u32>,
    style: bool,
    polygon: Option<Polygon>,
}

/// A polygon's or star's repetition of its stored points.
struct Polygon {
    angles: u32,
    next: u32,
    scale: (f64, f64),
    center: (f64, f64),
}

struct Reader<'f> {
    version: u32,
    streams: &'f [Vec<u8>],
    fills: HashMap<u32, Fill>,
    outlines: HashMap<u32, Option<Stroke>>,
    page_size: Option<(f64, f64)>,
    warnings: Vec<String>,
    shapes: usize,
    limits: Limits,
}

/// Read the drawing a CorelDRAW file's RIFF tree holds.
pub(crate) fn drawing(file: &File, limits: Limits) -> Result<Drawing, Error> {
    let mut r = Reader {
        version: file.version,
        streams: &file.streams,
        fills: HashMap::new(),
        outlines: HashMap::new(),
        page_size: None,
        warnings: Vec::new(),
        shapes: 0,
        limits,
    };
    for name in &file.skipped {
        r.warn(&format!("part `{name}` of the file is too large to read and was left out (embedded bitmaps aren't imported)"));
    }
    r.styles(&file.records);
    let mut pages = Vec::new();
    r.pages(&file.records, &mut pages);
    Ok(Drawing { version: r.version, page_size: r.page_size, pages, warnings: r.warnings })
}

/// The records directly in `records` or in lists below them that don't start another object,
/// group, layer or page, with id `id`.
fn own<'r>(records: &'r [Record], id: &[u8; 4], out: &mut Vec<&'r [u8]>) {
    for r in records {
        match r {
            Record::Leaf { id: i, data } if i == id => out.push(data),
            Record::List { kind, children } if !matches!(kind, b"obj " | b"grp " | b"lnkg" | b"layr" | b"page") => own(children, id, out),
            _ => {}
        }
    }
}

impl Reader<'_> {
    fn warn(&mut self, w: &str) {
        if !self.warnings.iter().any(|x| x == w) {
            self.warnings.push(w.to_string());
        }
    }

    fn short(&self) -> bool {
        self.version < 600
    }

    /// A record's data, from its external stream in CorelDRAW X6 and later (a 16-byte record
    /// naming the stream, the length and the offset).
    fn data<'d>(&'d self, data: &'d [u8]) -> &'d [u8] {
        if self.version < 1600 || data.len() != 16 {
            return data;
        }
        let (Some(index), Some(len), Some(at)) = (u32_at(data, 0), u32_at(data, 4), u32_at(data, 8)) else { return data };
        if index == u32::MAX {
            return data;
        }
        let (len, at) = (len as usize, at as usize);
        self.streams.get(index as usize).and_then(|s| s.get(at..at.checked_add(len)?)).unwrap_or(&[])
    }

    // ---------- styles ----------

    /// The version, styles and page size anywhere in the tree.
    fn styles(&mut self, records: &[Record]) {
        for r in records {
            match r {
                Record::List { children, .. } => self.styles(children),
                Record::Leaf { id, data } => match id {
                    b"vrsn" => {
                        if let Some(v) = R::new(data, 0, false).u16().map(u32::from).filter(|v| (300..=4000).contains(v)) {
                            self.version = v;
                        }
                    }
                    b"fild" => {
                        let data = self.data(data).to_vec();
                        if let Some((id, f)) = self.fill(&data) {
                            self.fills.entry(id).or_insert(f);
                        }
                    }
                    b"outl" => {
                        let data = self.data(data).to_vec();
                        if let Some((id, s)) = self.outline(&data) {
                            self.outlines.entry(id).or_insert(s);
                        }
                    }
                    b"mcfg" if self.page_size.is_none() => {
                        let data = self.data(data).to_vec();
                        self.page_size = self.mcfg(&data);
                    }
                    _ => {}
                },
            }
        }
    }

    fn mcfg(&self, data: &[u8]) -> Option<(f64, f64)> {
        let v = self.version;
        let mut r = R::new(data, 0, self.short());
        r.skip(match v {
            1300.. => 12,
            900..=1299 => 4,
            600..=699 => 0x1c,
            _ => 0,
        })?;
        let (w, h) = if v < 400 {
            r.skip(2)?;
            let (x0, y0, x1, y1) = (r.coord()?, r.coord()?, r.coord()?, r.coord()?);
            ((x1 - x0).abs(), (y1 - y0).abs())
        } else {
            (r.coord()?.abs(), r.coord()?.abs())
        };
        let ok = |s: f64| s.is_finite() && (1.0..=1.0e6).contains(&s);
        (ok(w) && ok(h)).then_some((w, h))
    }

    /// A colour at `r`'s position (`r` moves past it).
    fn color(&mut self, r: &mut R) -> Option<Color> {
        let v = self.version;
        let (model, value) = if v >= 500 {
            let mut model = r.u16()?;
            r.skip(2)?; // palette
            r.skip(4)?;
            let value = r.u32()?;
            if (model == 1 && v >= 1300) || model == 0x1e {
                model = 0x19;
            }
            (model, value)
        } else if v >= 400 {
            let model = r.u16()?;
            let (c, m, y, k) = (r.u16()? & 0xff, r.u16()? & 0xff, r.u16()? & 0xff, r.u16()? & 0xff);
            r.skip(2)?;
            let _ = model; // CMYK on a 0–100 scale, whatever the model says
            (2, u32::from(c) | u32::from(m) << 8 | u32::from(y) << 16 | u32::from(k) << 24)
        } else {
            (u16::from(r.u8()?), r.u32()?)
        };
        Some(self.color_of(model, value))
    }

    fn color_of(&mut self, model: u16, value: u32) -> Color {
        let [c0, c1, c2, c3] = value.to_le_bytes();
        let pct = |b: u8| (f32::from(b) / 100.0).clamp(0.0, 1.0);
        let unit = |b: u8| f32::from(b) / 255.0;
        let hue = || f64::from(u16::from(c1) << 8 | u16::from(c0)) % 360.0;
        match model {
            0x01 | 0x02 | 0x15 => Color::Cmyk([pct(c0), pct(c1), pct(c2), pct(c3)]),
            0x03 | 0x11 => Color::Cmyk([unit(c0), unit(c1), unit(c2), unit(c3)]),
            0x04 => Color::Rgb([255 - c0, 255 - c1, 255 - c2]),
            0x05 => Color::Rgb([c2, c1, c0]),
            0x06 => Color::Rgb(hsv(hue(), f64::from(c2) / 255.0, f64::from(c3) / 255.0)),
            0x07 => Color::Rgb(hls(hue(), f64::from(c2) / 255.0, f64::from(c3) / 255.0)),
            0x08 => Color::Gray(if c0 == 0 { 0 } else { 255 }),
            0x09 => Color::Gray(c0),
            0x0c => Color::Rgb(lab(f64::from(c0) * 100.0 / 255.0, f64::from(c1 as i8), f64::from(c2 as i8))),
            0x12 => Color::Rgb(lab(f64::from(c0) * 100.0 / 255.0, f64::from(c1) - 128.0, f64::from(c2) - 128.0)),
            0x14 => {
                let t = pct(c0);
                Color::Cmyk([t; 4])
            }
            0x19 => {
                self.warn(SPOT);
                Color::Gray(128)
            }
            _ => {
                self.warn(COLORS);
                Color::BLACK
            }
        }
    }

    fn fill(&mut self, data: &[u8]) -> Option<(u32, Fill)> {
        let v = self.version;
        let mut r = R::new(data, 0, self.short());
        let id = r.u32()?;
        if v >= 1300 {
            r.skip(8)?;
        }
        let kind = r.u16()?;
        let fill = match kind {
            0 => Fill::None,
            1 => {
                r.skip(if v >= 1300 { 13 } else { 2 })?;
                Fill::Solid(self.color(&mut r)?)
            }
            2 => {
                self.warn(GRADIENTS);
                Fill::Solid(Color::Gray(160))
            }
            7 | 9 | 10 | 11 => {
                self.warn(PATTERNS);
                Fill::Solid(Color::Gray(160))
            }
            _ => Fill::None,
        };
        Some((id, fill))
    }

    fn outline(&mut self, data: &[u8]) -> Option<(u32, Option<Stroke>)> {
        let v = self.version;
        let mut r = R::new(data, 0, self.short());
        let id = r.u32()?;
        if v >= 1300 {
            // Blocks of (id, length) until the one with id 1, which holds the outline.
            let mut found = false;
            for _ in 0..256 {
                let (block, len) = (r.u32()?, r.u32()?);
                if block == 1 {
                    found = true;
                    break;
                }
                r.skip(len as usize)?;
            }
            if !found {
                return None;
            }
        }
        let kind = r.u16()?;
        let cap = r.u16()?;
        let join = r.u16()?;
        if (600..1300).contains(&v) {
            r.skip(2)?;
        }
        let width = r.coord()?.abs();
        let _stretch = r.u16()?;
        if v >= 600 {
            r.skip(2)?;
        }
        r.angle()?;
        r.skip(match v {
            1300.. => 46,
            600.. => 52,
            _ => 0,
        })?;
        let color = self.color(&mut r)?;
        r.skip(if v < 600 { 10 } else { 16 })?;
        let unit = if width > 0.0 { width } else { 1.0 };
        let dashes = match r.u16() {
            Some(n) => (0..n.min(64)).map_while(|_| r.u16()).map(|d| f64::from(d) * unit).collect::<Vec<_>>(),
            None => Vec::new(),
        };
        let dashes = if dashes.len() >= 2 && dashes.iter().any(|d| *d > 0.0) { dashes } else { Vec::new() };
        if kind & 1 != 0 {
            return Some((id, None));
        }
        let small = |v: u16| u8::try_from(v).ok().filter(|v| *v <= 2).unwrap_or(0);
        Some((
            id,
            Some(Stroke { color, width: if width.is_finite() { width.min(10_000.0) } else { 0.0 }, cap: small(cap), join: small(join), dashes }),
        ))
    }

    // ---------- pages ----------

    fn pages(&mut self, records: &[Record], out: &mut Vec<Page>) {
        for r in records {
            let Record::List { kind, children } = r else { continue };
            match kind {
                b"page" => {
                    if let Some(p) = self.page(children) {
                        out.push(p);
                    }
                }
                b"obj " | b"grp " | b"lnkg" | b"layr" => {}
                _ => self.pages(children, out),
            }
        }
    }

    fn page(&mut self, children: &[Record]) -> Option<Page> {
        // A master page (its flags in 0x00ff0000) holds the guides, the desktop and the grid.
        let flags = children.iter().find_map(|r| match r {
            Record::Leaf { id: [b'f', b'l', b'g', b's'], data } => u32_at(self.data(data), 0),
            _ => None,
        });
        if self.version >= 400 && flags.is_some_and(|f| f & 0x00ff_0000 != 0) {
            return None;
        }
        let mut layers = Vec::new();
        self.layers(children, &mut layers);
        if layers.is_empty() {
            let mut objects = Vec::new();
            self.collect(children, &mut objects, None);
            layers.push(Layer { name: "Layer 1".into(), objects });
        }
        layers.retain(|l| !l.objects.is_empty());
        (!layers.is_empty()).then_some(Page { layers })
    }

    fn layers(&mut self, records: &[Record], out: &mut Vec<Layer>) {
        for r in records {
            let Record::List { kind, children } = r else { continue };
            match kind {
                b"layr" => {
                    let mut objects = Vec::new();
                    self.collect(children, &mut objects, None);
                    out.push(Layer { name: format!("Layer {}", out.len() + 1), objects });
                }
                b"obj " | b"grp " | b"lnkg" | b"page" => {}
                _ => self.layers(children, out),
            }
        }
    }

    /// The objects and groups in `records`; `group`: the innermost group's transform.
    fn collect(&mut self, records: &[Record], out: &mut Vec<Object>, group: Option<Affine>) {
        for r in records {
            let Record::List { kind, children } = r else { continue };
            match kind {
                b"obj " => {
                    if let Some(o) = self.object(children, group) {
                        out.push(o);
                    }
                }
                b"grp " | b"lnkg" => {
                    let own = self.transform(children);
                    let mut kids = Vec::new();
                    self.collect(children, &mut kids, Some(own));
                    if !kids.is_empty() {
                        out.push(Object::Group(kids));
                    }
                }
                b"page" | b"layr" => {}
                _ => self.collect(children, out, group),
            }
        }
    }

    /// The transforms of `records`' own `trfd` records, composed (the first applies first).
    fn transform(&self, records: &[Record]) -> Affine {
        let mut leaves = Vec::new();
        own(records, b"trfd", &mut leaves);
        let mut xf = Affine::IDENTITY;
        for leaf in leaves {
            for t in self.trfd(self.data(leaf)) {
                xf = t * xf;
            }
        }
        xf
    }

    fn trfd(&self, data: &[u8]) -> Vec<Affine> {
        let v = self.version;
        let mut out = Vec::new();
        let (Some(n), Some(start)) = (u32_at(data, 4), u32_at(data, 8)) else { return out };
        for i in 0..(n as usize).min(64) {
            let Some(at) = start.checked_add(4 * i as u32).and_then(|a| u32_at(data, a as usize)) else { break };
            let mut r = R::new(data, at as usize, self.short());
            let t = (|| {
                if v >= 1300 {
                    r.skip(8)?;
                }
                if r.u16()? != 0x08 {
                    return None;
                }
                if v >= 600 {
                    r.skip(6)?;
                }
                let div = if v >= 600 { 254_000.0 } else { 1000.0 };
                let [a, c, e, b, d, f] = if v >= 500 {
                    [r.f64()?, r.f64()?, r.f64()? / div, r.f64()?, r.f64()?, r.f64()? / div]
                } else {
                    [r.fixed()?, r.fixed()?, f64::from(r.i32()?) / 1000.0, r.fixed()?, r.fixed()?, f64::from(r.i32()?) / 1000.0]
                };
                let t = Affine::new([a, b, c, d, e * PT, f * PT]);
                t.as_coeffs().iter().all(|x| x.is_finite() && x.abs() < 1.0e9).then_some(t)
            })();
            out.extend(t);
        }
        out
    }

    fn object(&mut self, children: &[Record], group: Option<Affine>) -> Option<Object> {
        let mut lodas = Vec::new();
        own(children, b"loda", &mut lodas);
        let data = self.data(lodas.first()?).to_vec();
        let loda = self.loda(&data)?;
        let mut path = loda.path?;
        if let Some(p) = &loda.polygon {
            path = polygon(&path, p);
        }
        if path.elements().len() < 2 {
            return None;
        }
        if self.shapes >= self.limits.max_shapes {
            self.warn(TOO_MANY);
            return None;
        }
        self.shapes += 1;
        path.apply_affine(group.unwrap_or(Affine::IDENTITY) * self.transform(children));
        let closed = path.elements().iter().any(|e| matches!(e, PathEl::ClosePath));
        let fill = match loda.fill.and_then(|id| self.fills.get(&id)) {
            Some(Fill::Solid(c)) if closed => Some(*c),
            _ => None,
        };
        let stroke = loda.outline.and_then(|id| self.outlines.get(&id).cloned()).flatten();
        if loda.style && loda.fill.is_none() && loda.outline.is_none() {
            self.warn(STYLES);
        }
        Some(Object::Shape(Shape { path, fill, stroke }))
    }

    fn loda(&mut self, data: &[u8]) -> Option<Loda> {
        let v = self.version;
        let n = u32_at(data, 4)? as usize;
        let args = u32_at(data, 8)? as usize;
        let types = u32_at(data, 12)? as usize;
        let kind = u32_at(data, 16)?;
        let n = n.min(data.len().saturating_sub(args) / 4).min(4096);
        let mut out = Loda::default();
        for i in 0..n {
            let Some(at) = u32_at(data, args + 4 * i) else { break };
            // The types are stored in the opposite order of the offsets.
            let Some(ty) = u32_at(data, types + 4 * (n - 1 - i)) else { break };
            let at = at as usize;
            match ty {
                0x1e => out.path = self.geometry(data, at, kind),
                0x14 if v >= 400 => out.fill = u32_at(data, at),
                0x0a if v >= 400 => out.outline = u32_at(data, at),
                0xc8 => out.style = true,
                0x2af8 => out.polygon = self.polygon_transform(data, at),
                _ => {}
            }
        }
        Some(out)
    }

    fn geometry(&mut self, data: &[u8], at: usize, kind: u32) -> Option<BezPath> {
        let old = self.version < 400;
        match (kind, old) {
            (1, false) | (0, true) => self.rectangle(data, at),
            (2, false) | (1, true) => self.ellipse(data, at),
            (3, false) | (2, true) | (0x14, _) => {
                let mut r = R::new(data, at, self.short());
                let count = r.u16()?;
                r.skip(2)?;
                self.points(&mut r, usize::from(count))
            }
            (0x25, _) => {
                let mut r = R::new(data, at, self.short());
                r.skip(4)?;
                let count = usize::from(r.u16()?) + usize::from(r.u16()?);
                r.skip(16)?;
                self.points(&mut r, count)
            }
            (4 | 6, false) | (3 | 5, true) => {
                self.warn(TEXT);
                None
            }
            (5, false) | (4, true) => {
                self.warn(BITMAPS);
                None
            }
            _ => {
                self.warn(UNKNOWN);
                None
            }
        }
    }

    /// `count` points (all coordinates, then a type byte each) as a path.
    fn points(&mut self, r: &mut R, count: usize) -> Option<BezPath> {
        let size = 2 * r.coord_size() + 1;
        let count = count.min(r.b.len().saturating_sub(r.at) / size);
        let mut pts = Vec::with_capacity(count);
        for _ in 0..count {
            pts.push(Point::new(r.coord()?, r.coord()?));
        }
        let types = r.take(count)?;
        let mut bp = BezPath::new();
        let mut ctrl: Vec<Point> = Vec::new();
        // The current subpath's start, and whether it is open.
        let mut start: Option<Point> = None;
        let mut open = false;
        for (p, t) in pts.into_iter().zip(types) {
            let close = t & 0x08 != 0;
            match t & 0xc0 {
                0x00 => {
                    if close && open {
                        bp.close_path();
                    }
                    bp.move_to(p);
                    start = Some(p);
                    open = true;
                    ctrl.clear();
                    continue;
                }
                0xc0 => {
                    ctrl.push(p);
                    continue;
                }
                _ => {}
            }
            if !open {
                // Drawing on after a close starts again from the subpath's start.
                bp.move_to(start.unwrap_or(p));
                start = start.or(Some(p));
                open = true;
            }
            match ctrl[..] {
                [a, b, ..] if t & 0xc0 == 0x80 => bp.curve_to(a, b, p),
                _ => bp.line_to(p),
            }
            ctrl.clear();
            if close {
                bp.close_path();
                open = false;
            }
        }
        (!bp.elements().is_empty()).then_some(bp)
    }

    fn rectangle(&mut self, data: &[u8], at: usize) -> Option<BezPath> {
        let mut r = R::new(data, at, self.short());
        let (w, h, radii) = if self.version < 1500 {
            let (w, h) = (r.coord()?, r.coord()?);
            let r3 = r.coord()?;
            let rest = if self.version >= 900 { [r.coord()?, r.coord()?, r.coord()?] } else { [r3; 3] };
            (w, h, [rest[2], rest[1], rest[0], r3])
        } else {
            let unit = 254_000.0 / PT;
            let (w, h) = (r.f64()? / unit, r.f64()? / unit);
            r.skip(16)?;
            let relative = r.u8()? == 0;
            let scale = if relative { w.abs().min(h.abs()) / 2.0 } else { 1.0 / unit };
            let at = |o: usize| R::new(data, at + o, false).f64().map(|v| v * scale);
            (w, h, [at(112).unwrap_or(0.0), at(88).unwrap_or(0.0), at(64).unwrap_or(0.0), at(40).unwrap_or(0.0)])
        };
        let rect = Rect::new(0.0, 0.0, w, h).abs();
        let most = rect.width().min(rect.height()) / 2.0;
        let c = |v: f64| if v.is_finite() { v.abs().min(most) } else { 0.0 };
        let radii = RoundedRectRadii::new(c(radii[0]), c(radii[1]), c(radii[2]), c(radii[3]));
        Some(RoundedRect::from_rect(rect, radii).to_path(0.01))
    }

    fn ellipse(&mut self, data: &[u8], at: usize) -> Option<BezPath> {
        let mut r = R::new(data, at, self.short());
        let (x, y) = (r.coord()?, r.coord()?);
        let (a1, a2) = (r.angle()?, r.angle()?);
        let pie = r.u32().unwrap_or(0) != 0;
        let center = Point::new(x / 2.0, y / 2.0);
        let radii = Vec2::new(center.x.abs(), center.y.abs());
        let turn = |a: f64| a.rem_euclid(2.0 * PI);
        let (a1, mut a2) = (turn(a1), turn(a2));
        if (a1 - a2).abs() < 1e-9 {
            return Some(kurbo::Ellipse::new(center, radii, 0.0).to_path(0.01));
        }
        if a2 < a1 {
            a2 += 2.0 * PI;
        }
        let arc = kurbo::Arc::new(center, radii, a1, a2 - a1, 0.0);
        let mut bp = BezPath::new();
        bp.move_to(center + Vec2::new(radii.x * a1.cos(), radii.y * a1.sin()));
        arc.to_cubic_beziers(0.01, |p1, p2, p| bp.curve_to(p1, p2, p));
        if pie {
            bp.line_to(center);
            bp.close_path();
        }
        Some(bp)
    }

    fn polygon_transform(&self, data: &[u8], at: usize) -> Option<Polygon> {
        let mut r = R::new(data, at, self.short());
        if self.version < 1300 {
            r.skip(4)?;
        }
        let angles = r.u32()?;
        let mut next = r.u32()?;
        if next <= 1 {
            next = r.u32()?;
        } else {
            r.skip(4)?;
        }
        if self.version >= 1300 {
            r.skip(4)?;
        }
        let scale = (r.f64()?, r.f64()?);
        let center = (r.coord()?, r.coord()?);
        (angles > 0 && angles <= 4096).then_some(Polygon { angles, next, scale, center })
    }
}

/// The most elements a polygon's outline may have.
const MAX_POLYGON: usize = 16_384;

/// A polygon or star from its stored points: copies turned round the origin, then scaled and moved.
fn polygon(base: &BezPath, p: &Polygon) -> BezPath {
    let step = 2.0 * PI / f64::from(p.angles);
    let mut out = BezPath::new();
    let append = |out: &mut BezPath, angle: f64, joined: bool| {
        let rot = Affine::rotate(angle);
        for el in base.elements() {
            if out.elements().len() >= MAX_POLYGON {
                return;
            }
            match *el {
                PathEl::MoveTo(q) if joined => out.line_to(rot * q),
                PathEl::MoveTo(q) => out.move_to(rot * q),
                PathEl::LineTo(q) => out.line_to(rot * q),
                PathEl::QuadTo(a, q) => out.quad_to(rot * a, rot * q),
                PathEl::CurveTo(a, b, q) => out.curve_to(rot * a, rot * b, rot * q),
                PathEl::ClosePath => {}
            }
        }
    };
    if p.next == 0 {
        append(&mut out, 0.0, false);
        out.close_path();
    } else if !p.angles.is_multiple_of(p.next) {
        // One run through every vertex, `next` vertices at a time.
        for i in 0..p.angles {
            append(&mut out, f64::from(i) * f64::from(p.next) * step, i > 0);
        }
        out.close_path();
    } else {
        // `next` separate polygons, each turned a step from the one before.
        let per = p.angles / p.next;
        for g in 0..p.next {
            for k in 0..per {
                append(&mut out, (f64::from(g) + f64::from(k) * f64::from(p.next)) * step, k > 0);
            }
            out.close_path();
        }
    }
    let (sx, sy) = p.scale;
    out.apply_affine(Affine::new([sx, 0.0, 0.0, sy, p.center.0, p.center.1]));
    out
}

fn hsv(h: f64, s: f64, v: f64) -> [u8; 3] {
    let c = v * s;
    let x = c * (1.0 - ((h / 60.0) % 2.0 - 1.0).abs());
    let (r, g, b) = match (h / 60.0) as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [byte(r + m), byte(g + m), byte(b + m)]
}

fn hls(h: f64, l: f64, s: f64) -> [u8; 3] {
    let v = l + s * l.min(1.0 - l);
    let sv = if v > 0.0 { 2.0 * (1.0 - l / v) } else { 0.0 };
    hsv(h, sv, v)
}

/// CIE L*a*b* (D50) as sRGB.
fn lab(l: f64, a: f64, b: f64) -> [u8; 3] {
    let fy = (l + 16.0) / 116.0;
    let (fx, fz) = (fy + a / 500.0, fy - b / 200.0);
    let inv = |t: f64| if t > 6.0 / 29.0 { t * t * t } else { 3.0 * (6.0f64 / 29.0).powi(2) * (t - 4.0 / 29.0) };
    let (x, y, z) = (0.9642 * inv(fx), inv(fy), 0.8249 * inv(fz));
    // D50 XYZ → linear sRGB (Bradford-adapted).
    let r = 3.1339 * x - 1.6169 * y - 0.4906 * z;
    let g = -0.9788 * x + 1.9161 * y + 0.0334 * z;
    let bl = 0.0719 * x - 0.2290 * y + 1.4052 * z;
    let gamma = |c: f64| if c <= 0.003_130_8 { 12.92 * c } else { 1.055 * c.powf(1.0 / 2.4) - 0.055 };
    [byte(gamma(r)), byte(gamma(g)), byte(gamma(bl))]
}

fn byte(v: f64) -> u8 {
    if v.is_finite() { (v.clamp(0.0, 1.0) * 255.0).round() as u8 } else { 0 }
}
