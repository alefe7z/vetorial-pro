//! CorelDRAW drawings (`.cdr`, `.cdt`) through Open, Place and templates, the thumbnail fallback,
//! and the other formats read for import: PostScript `.ps`, Targa, Netpbm, QOI, ICO and Farbfeld.

use super::*;
use serde_json::json;
use std::io::Cursor;
use vectorcraft_corel::synth;
use vectorcraft_doc::NodeKind;

fn paths(doc: &vectorcraft_doc::Document) -> Vec<vectorcraft_doc::Node> {
    let mut out = Vec::new();
    doc.walk(|n| {
        if matches!(n.kind, NodeKind::Path { .. }) {
            out.push(n.clone());
        }
    });
    out
}

#[test]
fn a_cdr_opens_as_editable_art_on_its_page() {
    let l = load("drawing.cdr", &synth::sample()).unwrap();
    assert_eq!(l.format.id, "cdr");
    assert!(!l.preview_only);
    let doc = &l.doc;
    assert_eq!(doc.artboards.len(), 1);
    let a = doc.artboards[0].rect;
    assert_eq!((a.width().round(), a.height().round()), (200.0, 100.0));
    let p = paths(doc);
    assert_eq!(p.len(), 1, "the master page's guide is left out");
    // CorelDRAW's (10, 0)–(60, 40) about the page's centre, y up → (110, 10)–(160, 50).
    let b = p[0].visual_bounds().unwrap();
    assert!((b.x0 - 109.0).abs() < 1.5 && (b.y0 - 9.0).abs() < 1.5 && (b.x1 - 161.0).abs() < 1.5 && (b.y1 - 51.0).abs() < 1.5, "{b:?}");
    let ap = &p[0].appearance;
    assert_eq!(ap.fill_paint(), vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(255, 0, 0)));
    assert_eq!(ap.stroke_paint(), vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(0, 0, 255)));
    assert!((ap.stroke_width() - 2.0).abs() < 0.01);
    assert!(l.warnings.iter().any(|w| w.starts_with("CorelDRAW:")));
}

#[test]
fn open_place_and_templates() {
    let bytes = synth::sample();
    let mut s = Session::new();
    // Any name: told by its content.
    let r = open_bytes(&mut s, "renamed.bin", &bytes, Some("/x/renamed.bin".into())).unwrap();
    assert_eq!(r["format"], "cdr");
    // Not written back: Save asks for a new file.
    assert!(s.doc().unwrap().path.is_none());
    let p = json!({"name":"art.cdr", "dataBase64":vectorcraft_format::base64_encode(&bytes)});
    s.execute("file.new", &json!({"width":300,"height":300})).unwrap();
    s.execute("file.place", &p).unwrap();
    let red = vectorcraft_color::Paint::solid(vectorcraft_color::Color::rgb8(255, 0, 0));
    assert!(paths(&s.doc().unwrap().doc).iter().any(|p| p.appearance.fill_paint() == red));
    // A template opens untitled.
    open_bytes(&mut s, "form.cdt", &bytes, Some("/x/form.cdt".into())).unwrap();
    assert!(s.doc().unwrap().path.is_none());
    assert!(!s.doc().unwrap().doc.template);
    for ext in ["cdr", "cdt", "ps", "tga", "pbm", "ppm", "qoi", "ico", "hdr", "exr", "dds", "ff"] {
        assert!(OPEN_EXTS.contains(&ext) && PLACE_EXTS.contains(&ext), "{ext}");
        assert!(format(ext).is_some_and(|f| f.read), "{ext}");
    }
    assert!(!SAVE_FORMATS.contains(&"cdr"));
    assert!(s.execute("document.export", &json!({"format":"cdr"})).is_err());
}

#[test]
fn zipped_cdr_files_open() {
    let z = synth::zip(&[("content/riffData.cdr", synth::sample())]);
    let l = load("x4.cdr", &z).unwrap();
    assert_eq!(l.format.id, "cdr");
    assert_eq!(paths(&l.doc).len(), 1);
}

#[test]
fn an_unreadable_drawing_opens_as_its_thumbnail_and_is_not_placed() {
    // Only a master page: nothing to draw, but a thumbnail.
    let guide = synth::object(synth::loda(3, &[(0x1e, synth::curve(&[(0.0, 0.0, 0), (5.0, 0.0, 0x40)]))]), None);
    let f = synth::riff(
        b'B',
        &[synth::leaf(b"DISP", &[vec![0; 4], synth::dib([0, 255, 0])].concat()), synth::page(0x00ff_0000, &[synth::layer(&[guide])])],
    );
    let l = load("thumb.cdr", &f).unwrap();
    assert!(l.preview_only);
    assert!(l.warnings[0].contains("embedded thumbnail"), "{}", l.warnings[0]);
    assert_eq!((l.doc.artboards[0].rect.width(), l.doc.artboards[0].rect.height()), (2.0, 2.0));
    let mut s = Session::new();
    s.execute("file.new", &json!({"width":100,"height":100})).unwrap();
    let p = json!({"name":"thumb.cdr", "dataBase64":vectorcraft_format::base64_encode(&f)});
    let e = s.execute("file.place", &p).unwrap_err().to_string();
    assert!(e.contains("CorelDRAW") && e.contains("File › Open"), "{e}");
    // Neither drawing nor thumbnail: an error that says why.
    let bare = synth::riff(b'B', &[synth::page(0x00ff_0000, &[])]);
    assert!(load("bare.cdr", &bare).is_err());
}

#[test]
fn damaged_cdr_files_never_panic() {
    let whole = synth::sample();
    for cut in (0..whole.len()).step_by(11) {
        let _ = load("cut.cdr", &whole[..cut]);
    }
}

#[test]
fn postscript_programs_open() {
    let ps = b"%!PS-Adobe-3.0\n%%BoundingBox: 0 0 200 100\n%%EndComments\nnewpath 10 10 moveto 110 10 lineto 110 60 lineto closepath 1 0 0 setrgbcolor fill\nshowpage\n%%EOF\n";
    let mut s = Session::new();
    let r = open_bytes(&mut s, "page.ps", ps, Some("/x/page.ps".into())).unwrap();
    assert_eq!(r["format"], "ps");
    assert!(!paths(&s.doc().unwrap().doc).is_empty());
    // PostScript isn't written back over the file.
    assert!(s.doc().unwrap().path.is_none());
}

fn encoded(f: image::ImageFormat) -> Vec<u8> {
    let im = image::RgbaImage::from_fn(3, 2, |x, _| image::Rgba([40 * x as u8, 100, 200, 255]));
    let mut b = Vec::new();
    let im = image::DynamicImage::ImageRgba8(im);
    let im = match f {
        image::ImageFormat::Farbfeld => image::DynamicImage::ImageRgba16(im.to_rgba16()),
        image::ImageFormat::Pnm => image::DynamicImage::ImageRgb8(im.to_rgb8()),
        _ => im,
    };
    im.write_to(&mut Cursor::new(&mut b), f).unwrap();
    b
}

#[test]
fn more_raster_formats_open_and_place() {
    for (name, f) in [
        ("a.tga", image::ImageFormat::Tga),
        ("a.ppm", image::ImageFormat::Pnm),
        ("a.qoi", image::ImageFormat::Qoi),
        ("a.ico", image::ImageFormat::Ico),
        ("a.ff", image::ImageFormat::Farbfeld),
    ] {
        let bytes = encoded(f);
        let l = load(name, &bytes).unwrap_or_else(|e| panic!("{name}: {e}"));
        assert!(l.format.raster, "{name}");
        assert_eq!((l.doc.artboards[0].rect.width(), l.doc.artboards[0].rect.height()), (3.0, 2.0), "{name}");
        // Stored as PNG.
        assert_eq!(l.doc.images.values().next().unwrap().mime, "image/png", "{name}");
        let mut s = Session::new();
        s.execute("file.new", &json!({"width":100,"height":100})).unwrap();
        s.execute("file.place", &json!({"name": name, "dataBase64": vectorcraft_format::base64_encode(&bytes)}))
            .unwrap_or_else(|e| panic!("{name}: {e}"));
    }
}
