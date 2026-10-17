//! CorelDRAW documents (`.cdr`, templates `.cdt`): the pages, layers, groups, shapes, fills and
//! outlines that `vectorcraft-corel` reads, as editable VectorCraft art, one artboard per page
//! (side by side). What the reader leaves out is listed in the warnings. A file whose drawing
//! can't be read opens as its embedded thumbnail instead, with a warning that says why, and can't
//! be placed.

use std::sync::Arc;

use kurbo::{Affine, Rect, Shape as _};
use vectorcraft_color::{Color, Paint};
use vectorcraft_corel::{self as cdr, Object};
use vectorcraft_doc::appearance::{Appearance, AppearanceItem, Dash, FillLayer, LineCap, LineJoin, StrokeLayer};
use vectorcraft_doc::geom::PathData;
use vectorcraft_doc::{Artboard, Document, LayerColor, Node};

use super::load::{err, raster_doc};
use crate::Result;

/// Space between pages laid side by side, in points.
const GAP: f64 = 36.0;
/// What a hairline outline (width 0) is drawn with, in points.
const HAIRLINE: f64 = 0.25;

/// What a CorelDRAW file opened as.
pub(super) struct Imported {
    pub doc: Document,
    pub warnings: Vec<String>,
    /// Only the embedded thumbnail: never placed, used as a template or mined for styles.
    pub preview_only: bool,
}

pub(super) fn import(name: &str, bytes: &[u8]) -> Result<Imported> {
    let read = vectorcraft_corel::read(bytes, cdr::Limits::default());
    match read {
        Ok(d) if d.shape_count() > 0 => {
            let mut warnings: Vec<String> = d.warnings.iter().map(|w| format!("CorelDRAW: {w}.")).collect();
            warnings.push(
                "CorelDRAW: the format isn't published; shapes, solid fills and outlines are read, other content may be missing or approximated."
                    .into(),
            );
            Ok(Imported { doc: document(&d)?, warnings, preview_only: false })
        }
        other => {
            let why = match other {
                Ok(d) if !d.warnings.is_empty() => format!("nothing VectorCraft reads was found in it ({})", d.warnings.join("; ")),
                Ok(_) => "nothing VectorCraft reads was found in it".to_string(),
                Err(e) => e.to_string(),
            };
            let thumb = vectorcraft_corel::thumbnail(bytes).ok_or_else(|| err(format!("can't open `{name}`: {why}")))?;
            let doc = raster_doc(name, &thumb).map_err(|e| err(format!("can't open `{name}`: {why}; its thumbnail can't be read either ({e})")))?;
            Ok(Imported {
                doc,
                warnings: vec![format!(
                    "The CorelDRAW drawing could not be read ({why}). Opened only its embedded thumbnail: no layers, vectors or text. Export SVG, PDF or EPS from CorelDRAW for editable vectors."
                )],
                preview_only: true,
            })
        }
    }
}

fn color(c: cdr::Color) -> Color {
    match c {
        cdr::Color::Rgb([r, g, b]) => Color::rgb8(r, g, b),
        cdr::Color::Cmyk([c, m, y, k]) => Color::cmyk(c, m, y, k),
        cdr::Color::Gray(g) => Color::gray(1.0 - f32::from(g) / 255.0),
    }
}

fn document(d: &cdr::Drawing) -> Result<Document> {
    let mut doc = Document::new(1.0, 1.0);
    doc.layers.clear();
    let mut artboards = Vec::new();
    let mut x = 0.0;
    let many = d.pages.len() > 1;
    for (i, page) in d.pages.iter().enumerate() {
        // The page's size, else its art's bounds (CorelDRAW's origin is the page's centre, y up).
        let bounds = page_bounds(page);
        let (w, h) = d.page_size.unwrap_or_else(|| {
            let b = bounds.unwrap_or(Rect::new(-306.0, -396.0, 306.0, 396.0));
            (2.0 * b.x0.abs().max(b.x1.abs()).max(1.0), 2.0 * b.y0.abs().max(b.y1.abs()).max(1.0))
        });
        // The art around the page (CorelDRAW's desktop) keeps clear of the page before: the page
        // goes far enough right that its art's left edge clears the last page's art (Vetorial).
        let reach_left = bounds.map_or(0.0, |b| (w / 2.0 + b.x0).min(0.0));
        x -= reach_left;
        let to_doc = Affine::new([1.0, 0.0, 0.0, -1.0, x + w / 2.0, h / 2.0]);
        let name = if many { format!("Page {}", i + 1) } else { "Page 1".to_string() };
        artboards.push(Artboard { id: i as u32 + 1, name, rect: Rect::new(x, 0.0, x + w, h), show_center_mark: false, show_cross_hairs: false });
        for layer in &page.layers {
            let children: Vec<Arc<Node>> = layer.objects.iter().filter_map(|o| node(&mut doc, o, to_doc)).collect();
            let label = if many { format!("Page {} {}", i + 1, layer.name) } else { layer.name.clone() };
            let id = doc.alloc_id();
            let mut l = Node::layer(id, &label, LayerColor::Preset((doc.layers.len() % 9) as u8));
            if let Some(c) = l.children_mut() {
                *c = children;
            }
            doc.layers.push(Arc::new(l));
        }
        // Past the page and the art to its right.
        let reach_right = bounds.map_or(w, |b| (w / 2.0 + b.x1).max(w));
        x += reach_right + GAP;
    }
    if doc.layers.is_empty() {
        return Err(err("the CorelDRAW document has no art"));
    }
    doc.artboards = artboards;
    Ok(doc)
}

fn page_bounds(page: &cdr::Page) -> Option<Rect> {
    fn walk(o: &[Object], acc: &mut Option<Rect>) {
        for o in o {
            match o {
                Object::Shape(s) => {
                    let b = s.path.bounding_box();
                    *acc = Some(acc.map_or(b, |a| a.union(b)));
                }
                Object::Group(g) => walk(g, acc),
            }
        }
    }
    let mut acc = None;
    for l in &page.layers {
        walk(&l.objects, &mut acc);
    }
    acc.filter(|r| [r.x0, r.y0, r.x1, r.y1].iter().all(|v| v.is_finite()))
}

fn node(doc: &mut Document, o: &Object, to_doc: Affine) -> Option<Arc<Node>> {
    match o {
        Object::Group(g) => {
            let kids: Vec<Arc<Node>> = g.iter().filter_map(|o| node(doc, o, to_doc)).collect();
            (!kids.is_empty()).then(|| Arc::new(Node::group(doc.alloc_id(), kids)))
        }
        Object::Shape(s) => {
            let bp = to_doc * s.path.clone();
            let b = bp.bounding_box();
            if ![b.x0, b.y0, b.x1, b.y1].iter().all(|v| v.is_finite() && v.abs() < 1.0e7) {
                return None;
            }
            let mut items = Vec::new();
            if let Some(f) = s.fill {
                items.push(AppearanceItem::Fill(FillLayer::new(Paint::solid(color(f)))));
            }
            if let Some(st) = &s.stroke {
                let mut layer = StrokeLayer::new(Paint::solid(color(st.color)), if st.width > 0.0 { st.width } else { HAIRLINE });
                layer.cap = match st.cap {
                    1 => LineCap::Round,
                    2 => LineCap::Square,
                    _ => LineCap::Butt,
                };
                layer.join = match st.join {
                    1 => LineJoin::Round,
                    2 => LineJoin::Bevel,
                    _ => LineJoin::Miter,
                };
                if !st.dashes.is_empty() {
                    layer.dash = Some(Dash { pattern: st.dashes.clone(), offset: 0.0, align_corners: false });
                }
                items.push(AppearanceItem::Stroke(layer));
            }
            Some(Arc::new(Node::path(doc.alloc_id(), PathData::from_bezpath(&bp), Appearance { items, ..Appearance::default() })))
        }
    }
}
