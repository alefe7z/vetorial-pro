use kurbo::{PathEl, Shape as _};

use crate::synth::*;
use crate::*;

fn shapes(d: &Drawing) -> Vec<&Shape> {
    fn walk<'a>(o: &'a [Object], out: &mut Vec<&'a Shape>) {
        for o in o {
            match o {
                Object::Shape(s) => out.push(s),
                Object::Group(g) => walk(g, out),
            }
        }
    }
    let mut out = Vec::new();
    for l in d.pages.iter().flat_map(|p| &p.layers) {
        walk(&l.objects, &mut out);
    }
    out
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn reads_the_sample() {
    let bytes = sample();
    assert!(is_cdr(&bytes));
    let d = read(&bytes, Limits::default()).unwrap();
    assert_eq!(d.version, 1100);
    assert_eq!(d.page_size.map(|(w, h)| (w.round(), h.round())), Some((200.0, 100.0)));
    // The master page (with the guide) is left out.
    assert_eq!(d.pages.len(), 1);
    let s = shapes(&d);
    assert_eq!(s.len(), 1);
    let b = s[0].path.bounding_box();
    assert!(near(b.x0, 10.0) && near(b.y0, 0.0) && near(b.x1, 60.0) && near(b.y1, 40.0), "{b:?}");
    assert_eq!(s[0].fill, Some(Color::Rgb([255, 0, 0])));
    let st = s[0].stroke.as_ref().unwrap();
    assert_eq!(st.color, Color::Rgb([0, 0, 255]));
    assert!(near(st.width, 2.0));
    assert_eq!((st.cap, st.join), (1, 1));
}

#[test]
fn curves_lines_and_closing() {
    let pts = [(0.0, 0.0, 0x00), (10.0, 20.0, 0xc0), (20.0, 20.0, 0xc0), (30.0, 0.0, 0x80), (15.0, -10.0, 0x48)];
    let obj = object(loda(3, &[(0x1e, curve(&pts)), (0x14, 7u32.to_le_bytes().to_vec())]), None);
    let f = riff(b'B', &[fill(7, [1, 2, 3]), page(0, &[layer(&[obj])])]);
    let d = read(&f, Limits::default()).unwrap();
    let s = shapes(&d);
    let els = s[0].path.elements();
    assert!(matches!(els[0], PathEl::MoveTo(_)));
    assert!(matches!(els[1], PathEl::CurveTo(..)));
    assert!(matches!(els[2], PathEl::LineTo(_)));
    assert!(matches!(els[3], PathEl::ClosePath));
    assert_eq!(s[0].fill, Some(Color::Rgb([1, 2, 3])));
    assert_eq!(s[0].stroke, None);
}

#[test]
fn open_paths_are_not_filled() {
    let obj = object(loda(3, &[(0x1e, curve(&[(0.0, 0.0, 0), (10.0, 0.0, 0x40)])), (0x14, 7u32.to_le_bytes().to_vec())]), None);
    let d = read(&riff(b'B', &[fill(7, [9, 9, 9]), page(0, &[layer(&[obj])])]), Limits::default()).unwrap();
    assert_eq!(shapes(&d)[0].fill, None);
}

#[test]
fn ellipses_arcs_and_pies() {
    let whole = object(loda(2, &[(0x1e, ellipse(40.0, 20.0, 0.0, 0.0, false))]), None);
    let pie = object(loda(2, &[(0x1e, ellipse(40.0, 40.0, 0.0, 90.0, true))]), None);
    // The file lists the frontmost object first: the pie is drawn above the ellipse.
    let d = read(&riff(b'B', &[page(0, &[layer(&[pie, whole])])]), Limits::default()).unwrap();
    let s = shapes(&d);
    let b = s[0].path.bounding_box();
    assert!(near(b.x0, 0.0) && near(b.x1, 40.0) && near(b.y0, 0.0) && near(b.y1, 20.0), "{b:?}");
    // A quarter pie: from the centre (20, 20) to the right and up.
    let b = s[1].path.bounding_box();
    assert!(near(b.x0, 20.0) && near(b.x1, 40.0) && near(b.y0, 20.0) && near(b.y1, 40.0), "{b:?}");
    assert!(s[1].path.elements().iter().any(|e| matches!(e, PathEl::ClosePath)));
}

#[test]
fn round_rectangles() {
    let obj = object(loda(1, &[(0x1e, rect(100.0, 50.0, 10.0))]), None);
    let d = read(&riff(b'B', &[page(0, &[layer(&[obj])])]), Limits::default()).unwrap();
    let s = shapes(&d);
    assert!(s[0].path.elements().iter().any(|e| matches!(e, PathEl::CurveTo(..))));
    assert!((s[0].path.area().abs() - (100.0 * 50.0 - (4.0 - std::f64::consts::PI) * 100.0)).abs() < 0.5);
}

#[test]
fn groups_and_transforms() {
    let a = object(loda(1, &[(0x1e, rect(10.0, 10.0, 0.0))]), Some([2.0, 0.0, 0.0, 2.0, 0.0, 0.0]));
    let group = list(b"grp ", &[list(b"lgob", &[list(b"trfl", &[trfd([1.0, 0.0, 0.0, 1.0, 5.0, 5.0])])]), a]);
    let d = read(&riff(b'B', &[page(0, &[layer(&[group])])]), Limits::default()).unwrap();
    let Object::Group(g) = &d.pages[0].layers[0].objects[0] else { panic!("not a group") };
    let Object::Shape(s) = &g[0] else { panic!("not a shape") };
    // Scaled by its own transform, then moved by its group's.
    let b = s.path.bounding_box();
    assert!(near(b.x0, 5.0) && near(b.x1, 25.0) && near(b.y1, 25.0), "{b:?}");
}

#[test]
fn stars() {
    // A five-pointed star drawn in one run, every second vertex: one stored edge, turned 4 times.
    let edge = curve(&[(0.0, 1.0, 0x00), ((0.8 * std::f64::consts::PI).sin(), (0.8 * std::f64::consts::PI).cos(), 0x40)]);
    let obj = object(loda(0x14, &[(0x1e, edge), (0x2af8, polygon_transform(5, 2, 10.0, (50.0, 50.0)))]), None);
    let d = read(&riff(b'B', &[page(0, &[layer(&[obj])])]), Limits::default()).unwrap();
    let s = shapes(&d);
    let b = s[0].path.bounding_box();
    let side = 10.0 * (0.4 * std::f64::consts::PI).sin();
    assert!(near(b.y1, 60.0) && near(b.x1, 50.0 + side) && near(b.x0, 50.0 - side), "{b:?}");
    assert!(s[0].path.elements().len() >= 10);
}

#[test]
fn compressed_lists() {
    let obj = object(loda(1, &[(0x1e, rect(30.0, 30.0, 0.0)), (0x14, 3u32.to_le_bytes().to_vec())]), None);
    let fild = fill(3, [0, 128, 0]);
    // The fill style lives in a compressed list, its length an index into the block table.
    let packed = cmpr(&[(b"fild", fild[8..].to_vec())]);
    let d = read(&riff(b'B', &[packed, page(0, &[layer(&[obj])])]), Limits::default()).unwrap();
    assert_eq!(shapes(&d)[0].fill, Some(Color::Rgb([0, 128, 0])));
}

#[test]
fn dashed_outlines() {
    let obj = object(loda(3, &[(0x1e, curve(&[(0.0, 0.0, 0), (10.0, 0.0, 0x40)])), (0x0a, 4u32.to_le_bytes().to_vec())]), None);
    let d = read(&riff(b'B', &[outline(4, 2.0, [0, 0, 0], &[3, 1]), page(0, &[layer(&[obj])])]), Limits::default()).unwrap();
    let st = shapes(&d)[0].stroke.clone().unwrap();
    assert_eq!(st.dashes.len(), 2);
    assert!(near(st.dashes[0], 6.0) && near(st.dashes[1], 2.0));
}

#[test]
fn zipped_files_and_their_thumbnail() {
    let bmp = {
        let mut b = b"BM".to_vec();
        b.extend_from_slice(&[0; 12]);
        b.extend(dib([1, 2, 3]));
        b
    };
    let z = zip(&[("content/riffData.cdr", sample()), ("metadata/thumbnails/thumbnail.bmp", bmp.clone())]);
    assert!(is_cdr(&z));
    let d = read(&z, Limits::default()).unwrap();
    assert_eq!(shapes(&d).len(), 1);
    assert_eq!(thumbnail(&z), Some(bmp));
    // A ZIP that isn't CorelDRAW's.
    assert!(!is_cdr(&zip(&[("word/document.xml", b"<w/>".to_vec())])));
}

#[test]
fn riff_thumbnail_is_a_bmp() {
    let t = thumbnail(&sample()).unwrap();
    assert!(t.starts_with(b"BM"));
    assert_eq!(u32::from_le_bytes([t[10], t[11], t[12], t[13]]), 54);
}

#[test]
fn x6_external_streams() {
    // CorelDRAW X6 (version 1600): a record of 16 bytes points into an external stream.
    let geometry = loda(3, &[(0x1e, curve(&[(0.0, 0.0, 0), (20.0, 10.0, 0x40)]))]);
    let body = geometry[8..].to_vec();
    let mut stream = vec![0xEE; 7];
    stream.extend_from_slice(&body);
    let mut pointer = Vec::new();
    for v in [0u32, body.len() as u32, 7, 0] {
        pointer.extend_from_slice(&v.to_le_bytes());
    }
    let root = riff(b'G', &[page(0, &[layer(&[list(b"obj ", &[list(b"lgob", &[leaf(b"loda", &pointer)])])])])]);
    let z = zip(&[("content/root.dat", root.clone()), ("content/dataFileList.dat", b"s1.dat\n".to_vec()), ("content/data/s1.dat", stream.clone())]);
    let d = read(&z, Limits::default()).unwrap();
    assert_eq!(d.version, 1600);
    let b = shapes(&d)[0].path.bounding_box();
    assert!(near(b.x1, 20.0) && near(b.y1, 10.0), "{b:?}");
    // A stream past the limit (a file's bitmaps) is left out with a warning; the drawing still opens.
    let z = zip(&[
        ("content/root.dat", root),
        ("content/dataFileList.dat", b"s1.dat\nBitmaps.dat\n".to_vec()),
        ("content/data/s1.dat", stream),
        ("content/data/Bitmaps.dat", vec![0; 5000]),
    ]);
    let d = read(&z, Limits { max_stream: 4096, ..Limits::default() }).unwrap();
    assert_eq!(shapes(&d).len(), 1);
    assert!(d.warnings.iter().any(|w| w.contains("Bitmaps.dat") && w.contains("too large")), "{:?}", d.warnings);
}

#[test]
fn text_and_bitmaps_are_listed() {
    let text = object(loda(4, &[(0x1e, vec![0; 16])]), None);
    let d = read(&riff(b'B', &[page(0, &[layer(&[text])])]), Limits::default()).unwrap();
    assert!(d.warnings.iter().any(|w| w.contains("text")));
    assert_eq!(d.shape_count(), 0);
}

#[test]
fn rejects_other_files() {
    assert_eq!(read(b"%PDF-1.4", Limits::default()), Err(Error::NotCdr));
    assert!(matches!(read(b"WL\x05\x00rest", Limits::default()), Err(Error::Unsupported(_))));
    assert!(!is_cdr(b"RIFF\0\0\0\0WAVE"));
}

#[test]
fn damaged_files_never_panic() {
    let whole = sample();
    let z = zip(&[("content/riffData.cdr", whole.clone())]);
    for f in [&whole, &z] {
        for cut in (0..f.len()).step_by(7) {
            let _ = read(&f[..cut], Limits::default());
            let _ = thumbnail(&f[..cut]);
        }
        // Flipped bytes everywhere.
        let mut seed = 0x2545_f491u32;
        for _ in 0..400 {
            let mut g = f.clone();
            for _ in 0..8 {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                let i = seed as usize % g.len();
                g[i] = (seed >> 8) as u8;
            }
            let _ = read(&g, Limits::default());
            let _ = thumbnail(&g);
        }
    }
}

#[test]
fn shape_limit() {
    let obj = || object(loda(1, &[(0x1e, rect(1.0, 1.0, 0.0))]), None);
    let f = riff(b'B', &[page(0, &[layer(&[obj(), obj(), obj()])])]);
    let d = read(&f, Limits { max_shapes: 2, ..Limits::default() }).unwrap();
    assert_eq!(d.shape_count(), 2);
    assert!(!d.warnings.is_empty());
}

#[test]
fn x6_inline_values_the_desktop_and_placed_objects() {
    // CorelDRAW 2020 and later: short values (flags) sit in the 16-byte record itself (stream
    // 0xffffffff, their length, the value); the art may be on the master page's desktop layer
    // (flags 0x08), with its guides (0x0a) and grid (0x5a) left out.
    let inline = |v: u32| [u32::MAX.to_le_bytes(), 4u32.to_le_bytes(), v.to_le_bytes(), [0; 4]].concat();
    let layer_of = |flags: u32, objects: &[Vec<u8>]| {
        let mut children = vec![leaf(b"flgs", &inline(flags))];
        children.extend_from_slice(objects);
        list(b"layr", &children)
    };
    let guide = object(loda(3, &[(0x1e, curve(&[(-100.0, 0.0, 0x00), (100.0, 0.0, 0x40)]))]), None);
    let placed = object(loda(3, &[(0x1e, curve(&[(0.0, 0.0, 0x00), (10.0, 10.0, 0x40)]))]), None);
    let group = list(b"grp ", &[list(b"lgob", &[leaf(b"trfd", &[0; 4])]), placed]);
    let master = list(
        b"page",
        &[
            leaf(b"flgs", &inline(0x9001_0040)),
            layer_of(0x9801_000a, std::slice::from_ref(&guide)),
            layer_of(0x9801_0008, &[group]),
            layer_of(0x9801_015a, &[guide]),
        ],
    );
    let own = object(loda(3, &[(0x1e, curve(&[(0.0, 0.0, 0x00), (5.0, 5.0, 0x40)]))]), None);
    let page1 = list(b"page", &[leaf(b"flgs", &inline(0x9000_0040)), layer_of(0x9800_0000, &[own])]);
    let root = riff(b'G', &[leaf(b"vrsn", &[u32::MAX.to_le_bytes(), 2u32.to_le_bytes(), 1600u32.to_le_bytes(), [0; 4]].concat()), master, page1]);
    let d = read(&zip(&[("content/root.dat", root)]), Limits::default()).unwrap();
    assert_eq!(d.version, 1600);
    assert_eq!(d.pages.len(), 1, "the desktop's art joins the first page: {d:?}");
    let s = shapes(&d);
    assert_eq!(s.len(), 2, "the guides and the grid are left out");
    let b = s[0].path.bounding_box();
    assert!(near(b.x1, 10.0), "the desktop layer is below the page's own: {b:?}");
}

/// A real file, read from `CDR_FILE` (never committed: files users send stay local). Prints what
/// was read: `CDR_FILE=/path/x.cdr cargo test -p vectorcraft-corel real_file -- --ignored --nocapture`.
#[test]
#[ignore]
fn real_file() {
    let Ok(path) = std::env::var("CDR_FILE") else { return };
    let bytes = std::fs::read(path).unwrap();
    let d = read(&bytes, Limits::default()).unwrap();
    println!("version {} page {:?}", d.version, d.page_size);
    for (i, p) in d.pages.iter().enumerate() {
        let count: usize = p.layers.iter().map(|l| l.objects.len()).sum();
        println!("page {i}: {} layers, {count} top objects", p.layers.len());
    }
    let s = shapes(&d);
    println!("{} shapes", s.len());
    for sh in s.iter().take(8) {
        println!("  {:?} fill {:?} stroke {:?}", sh.path.bounding_box(), sh.fill, sh.stroke.as_ref().map(|s| (s.width, s.color)));
    }
    let mut widths = std::collections::BTreeMap::new();
    for sh in &s {
        if let Some(st) = &sh.stroke {
            *widths.entry(format!("{:.3} {:?}", st.width, st.color)).or_insert(0) += 1;
        }
    }
    println!("strokes {widths:?}");
    let all = s.iter().map(|sh| sh.path.bounding_box()).reduce(|a, b| a.union(b));
    println!("all {all:?}");
    for sh in s.iter().filter(|sh| sh.path.bounding_box().area() > 20000.0) {
        let b = sh.path.bounding_box();
        println!("big {:.0},{:.0} – {:.0},{:.0} fill {:?}", b.x0, b.y0, b.x1, b.y1, sh.fill);
    }
    println!("warnings: {:?}", d.warnings);
}
