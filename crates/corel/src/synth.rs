//! Builders of synthetic CorelDRAW files for tests, written from the public descriptions of the
//! format. Not an exporter: just enough records to exercise the reader. Coordinates are given in
//! points and written as 32-bit tenths of a micrometre (CorelDRAW 6 and later).

use std::io::Write as _;

/// Tenths of a micrometre per point.
const UNIT: f64 = 254_000.0 / 72.0;

fn coord(v: f64) -> [u8; 4] {
    ((v * UNIT).round() as i32).to_le_bytes()
}

/// A data record (padded to an even length).
pub fn leaf(id: &[u8; 4], data: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(data.len() + 9);
    out.extend_from_slice(id);
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(data);
    if data.len() % 2 == 1 {
        out.push(0);
    }
    out
}

/// A `LIST` of `kind` holding `children`.
pub fn list(kind: &[u8; 4], children: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = children.concat();
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"LIST");
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(&body);
    out
}

/// A whole RIFF file of version character `v` (`'B'`: CorelDRAW 11, version 1100).
pub fn riff(v: u8, children: &[Vec<u8>]) -> Vec<u8> {
    let body: Vec<u8> = children.concat();
    let mut out = Vec::with_capacity(body.len() + 12);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(body.len() as u32 + 4).to_le_bytes());
    out.extend_from_slice(b"CDR");
    out.push(v);
    out.extend_from_slice(&body);
    out
}

/// A `loda` record of object `kind` (1 rectangle, 2 ellipse, 3 curve, 0x14 polygon…) with its
/// arguments `(type, data)`.
pub fn loda(kind: u32, args: &[(u32, Vec<u8>)]) -> Vec<u8> {
    let mut data = vec![0u8; 20];
    let mut offsets = Vec::new();
    for (_, a) in args {
        offsets.push(data.len() as u32);
        data.extend_from_slice(a);
    }
    let start_args = data.len() as u32;
    for o in &offsets {
        data.extend_from_slice(&o.to_le_bytes());
    }
    let start_types = data.len() as u32;
    // Types are stored in the opposite order of the offsets.
    for (t, _) in args.iter().rev() {
        data.extend_from_slice(&t.to_le_bytes());
    }
    let len = data.len() as u32;
    for (i, v) in [len, args.len() as u32, start_args, start_types, kind].into_iter().enumerate() {
        data[4 * i..4 * i + 4].copy_from_slice(&v.to_le_bytes());
    }
    leaf(b"loda", &data)
}

/// Curve geometry: points `(x, y, type)` (0x00 move, 0x40 line, 0xc0 control, 0x80 curve end,
/// | 0x08 close).
pub fn curve(points: &[(f64, f64, u8)]) -> Vec<u8> {
    let mut out = (points.len() as u16).to_le_bytes().to_vec();
    out.extend_from_slice(&[0, 0]);
    for (x, y, _) in points {
        out.extend_from_slice(&coord(*x));
        out.extend_from_slice(&coord(*y));
    }
    out.extend(points.iter().map(|p| p.2));
    out
}

/// Rectangle geometry (versions 900 to 1499): corner at (0, 0) and (w, h), corner radius `r`.
pub fn rect(w: f64, h: f64, r: f64) -> Vec<u8> {
    [w, h, r, r, r, r].iter().flat_map(|v| coord(*v)).collect()
}

/// Ellipse geometry: in the box (0, 0)–(w, h), from angle `a1` to `a2` (degrees; equal: whole).
pub fn ellipse(w: f64, h: f64, a1: f64, a2: f64, pie: bool) -> Vec<u8> {
    let mut out = [coord(w), coord(h)].concat();
    for a in [a1, a2] {
        out.extend_from_slice(&((a * 1_000_000.0).round() as i32).to_le_bytes());
    }
    out.extend_from_slice(&u32::from(pie).to_le_bytes());
    out
}

/// A polygon transform (versions 600 to 1299): `angles` vertices, every `next` one joined, the
/// stored points scaled by `scale` and moved to `center` (points).
pub fn polygon_transform(angles: u32, next: u32, scale: f64, center: (f64, f64)) -> Vec<u8> {
    let mut out = vec![0; 4];
    out.extend_from_slice(&angles.to_le_bytes());
    out.extend_from_slice(&next.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&scale.to_le_bytes());
    out.extend_from_slice(&scale.to_le_bytes());
    out.extend_from_slice(&coord(center.0));
    out.extend_from_slice(&coord(center.1));
    out
}

/// A `trfd` record (versions 600 to 1299): x' = a·x + c·y + e, y' = b·x + d·y + f (e, f in points).
pub fn trfd([a, b, c, d, e, f]: [f64; 6]) -> Vec<u8> {
    let mut data = Vec::new();
    data.extend_from_slice(&0u32.to_le_bytes());
    data.extend_from_slice(&1u32.to_le_bytes());
    data.extend_from_slice(&12u32.to_le_bytes());
    data.extend_from_slice(&16u32.to_le_bytes());
    data.extend_from_slice(&8u16.to_le_bytes());
    data.extend_from_slice(&[0; 6]);
    for v in [a, c, e * UNIT, b, d, f * UNIT] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    leaf(b"trfd", &data)
}

/// An object: its `loda` and its transform list.
pub fn object(loda: Vec<u8>, transform: Option<[f64; 6]>) -> Vec<u8> {
    let mut lgob = vec![loda];
    if let Some(t) = transform {
        lgob.push(list(b"trfl", &[trfd(t)]));
    }
    list(b"obj ", &[list(b"lgob", &lgob)])
}

fn color_rgb([r, g, b]: [u8; 3]) -> Vec<u8> {
    let mut out = 5u16.to_le_bytes().to_vec();
    out.extend_from_slice(&[0; 6]);
    out.extend_from_slice(&u32::from_le_bytes([b, g, r, 0]).to_le_bytes());
    out
}

/// A solid RGB fill style (versions 500 to 1299).
pub fn fill(id: u32, rgb: [u8; 3]) -> Vec<u8> {
    let mut data = id.to_le_bytes().to_vec();
    data.extend_from_slice(&1u16.to_le_bytes());
    data.extend_from_slice(&[0; 2]);
    data.extend(color_rgb(rgb));
    leaf(b"fild", &data)
}

/// An outline style (versions 600 to 1299): width in points, RGB colour, dashes in widths.
pub fn outline(id: u32, width: f64, rgb: [u8; 3], dashes: &[u16]) -> Vec<u8> {
    let mut data = id.to_le_bytes().to_vec();
    for v in [0u16, 1, 1, 0] {
        data.extend_from_slice(&v.to_le_bytes());
    }
    data.extend_from_slice(&coord(width));
    data.extend_from_slice(&100u16.to_le_bytes());
    data.extend_from_slice(&[0; 2]);
    data.extend_from_slice(&[0; 4]);
    data.extend_from_slice(&[0; 52]);
    data.extend(color_rgb(rgb));
    data.extend_from_slice(&[0; 16]);
    data.extend_from_slice(&(dashes.len() as u16).to_le_bytes());
    for d in dashes {
        data.extend_from_slice(&d.to_le_bytes());
    }
    leaf(b"outl", &data)
}

/// The page size (versions 900 to 1299), in points.
pub fn mcfg(w: f64, h: f64) -> Vec<u8> {
    leaf(b"mcfg", &[vec![0; 4], coord(w).to_vec(), coord(h).to_vec()].concat())
}

/// A page with `flags` (0x00ff0000 set: a master page) and its layers.
pub fn page(flags: u32, layers: &[Vec<u8>]) -> Vec<u8> {
    let mut children = vec![leaf(b"flgs", &flags.to_le_bytes())];
    children.extend_from_slice(layers);
    list(b"page", &children)
}

/// A layer of objects.
pub fn layer(objects: &[Vec<u8>]) -> Vec<u8> {
    layer_flags(0, objects)
}

/// A layer with `flags` (in a master page, `0x0a` in the low byte: its guides layer).
pub fn layer_flags(flags: u32, objects: &[Vec<u8>]) -> Vec<u8> {
    let mut children = vec![leaf(b"flgs", &flags.to_le_bytes())];
    children.extend_from_slice(objects);
    list(b"layr", &children)
}

fn zlib(data: &[u8]) -> Vec<u8> {
    let mut e = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::default());
    let _ = e.write_all(data);
    e.finish().unwrap_or_default()
}

/// A `cmpr` list holding `records` (data records and lists, as built here), whose lengths are
/// written as indices into its block table.
pub fn cmpr(records: &[(&[u8; 4], Vec<u8>)]) -> Vec<u8> {
    let mut inner = Vec::new();
    let mut blocks = Vec::new();
    for (i, (id, data)) in records.iter().enumerate() {
        inner.extend_from_slice(*id);
        inner.extend_from_slice(&(i as u32).to_le_bytes());
        inner.extend_from_slice(data);
        blocks.extend_from_slice(&(data.len() as u32).to_le_bytes());
    }
    let (packed, table) = (zlib(&inner), zlib(&blocks));
    let mut body = b"cmpr".to_vec();
    body.extend_from_slice(&(packed.len() as u32).to_le_bytes());
    body.extend_from_slice(&(inner.len() as u32).to_le_bytes());
    body.extend_from_slice(&(table.len() as u32).to_le_bytes());
    body.extend_from_slice(&(blocks.len() as u32).to_le_bytes());
    body.extend_from_slice(b"CPng");
    body.extend_from_slice(&1u16.to_le_bytes());
    body.extend_from_slice(&4u16.to_le_bytes());
    body.extend_from_slice(&packed);
    body.extend_from_slice(&table);
    let mut out = b"LIST".to_vec();
    out.extend_from_slice(&(body.len() as u32).to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// A store-only ZIP archive of `files`.
pub fn zip(files: &[(&str, Vec<u8>)]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, bytes) in files {
        let offset = out.len() as u32;
        let mut crc = flate2::Crc::new();
        crc.update(bytes);
        let mut shared = Vec::new();
        for v in [0u16, 0, 0, 0] {
            shared.extend_from_slice(&v.to_le_bytes());
        }
        for v in [crc.sum(), bytes.len() as u32, bytes.len() as u32] {
            shared.extend_from_slice(&v.to_le_bytes());
        }
        shared.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&shared);
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(bytes);
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&shared);
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(files.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// A 2 × 2 24-bit Windows DIB (a header and pixels, no file header) of one colour.
pub fn dib([r, g, b]: [u8; 3]) -> Vec<u8> {
    let mut out = Vec::new();
    for v in [40u32, 2, 2] {
        out.extend_from_slice(&v.to_le_bytes());
    }
    out.extend_from_slice(&1u16.to_le_bytes());
    out.extend_from_slice(&24u16.to_le_bytes());
    out.extend_from_slice(&[0; 24]);
    for _ in 0..2 {
        out.extend_from_slice(&[b, g, r, b, g, r, 0, 0]);
    }
    out
}

/// A simple CorelDRAW 11 file: a 200 × 100 pt page with a red rectangle (blue 2 pt outline) moved
/// 10 pt right, on top of a master page holding a guide.
pub fn sample() -> Vec<u8> {
    let rect = object(
        loda(1, &[(0x1e, rect(50.0, 40.0, 0.0)), (0x14, 1u32.to_le_bytes().to_vec()), (0x0a, 2u32.to_le_bytes().to_vec())]),
        Some([1.0, 0.0, 0.0, 1.0, 10.0, 0.0]),
    );
    let guide = object(loda(3, &[(0x1e, curve(&[(-100.0, 0.0, 0x00), (100.0, 0.0, 0x40)])), (0x0a, 2u32.to_le_bytes().to_vec())]), None);
    let doc = list(b"doc ", &[mcfg(200.0, 100.0), list(b"filc", &[fill(1, [255, 0, 0])]), list(b"outc", &[outline(2, 2.0, [0, 0, 255], &[])])]);
    riff(
        b'B',
        &[
            leaf(b"vrsn", &1100u16.to_le_bytes()),
            leaf(b"DISP", &[vec![0; 4], dib([0, 255, 0])].concat()),
            doc,
            page(0x00ff_0000, &[layer_flags(0x9801_000a, &[guide])]),
            page(0, &[layer(&[rect])]),
        ],
    )
}
