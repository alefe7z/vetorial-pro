//! A bounded reader for CorelDRAW documents (`.cdr`, templates `.cdt`).
//!
//! CorelDRAW's format is not published: this reader follows public descriptions of it. A file is
//! a RIFF tree (CorelDRAW 3 to X3), or a ZIP archive holding that tree as `content/riffData.cdr`
//! (X4, X5) or `content/root.dat` with its records' data in `content/data/*` (X6 and later).
//! [`read`] turns it into pages of layers of shapes, in points, with CorelDRAW's axes: the origin
//! at the page's centre and y pointing up. [`thumbnail`] gives the preview every file carries.
//!
//! Everything read from the file is untrusted: sizes and counts are capped ([`Limits`]), and
//! nothing here panics.

mod container;
mod parse;
#[cfg(any(test, feature = "synth"))]
pub mod synth;
#[cfg(test)]
mod tests;

use kurbo::BezPath;

pub use container::is_cdr;

/// Caps on what one file may make the reader do.
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    /// The most bytes any one stream (a ZIP entry, a decompressed list) may hold.
    pub max_stream: usize,
    /// The most records the RIFF tree may hold.
    pub max_records: usize,
    /// The deepest the RIFF tree may nest.
    pub max_depth: usize,
    /// The most shapes the drawing may hold.
    pub max_shapes: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Self { max_stream: 256 << 20, max_records: 2_000_000, max_depth: 64, max_shapes: 500_000 }
    }
}

/// Why a file can't be read.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    /// Not a CorelDRAW file.
    NotCdr,
    /// A CorelDRAW format this reader doesn't know (CorelDRAW 1 and 2).
    Unsupported(String),
    /// The file is damaged or cut short.
    Damaged(String),
    /// The file asks for more than [`Limits`] allows.
    TooLarge(String),
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::NotCdr => f.write_str("not a CorelDRAW file"),
            Error::Unsupported(w) => write!(f, "unsupported CorelDRAW file: {w}"),
            Error::Damaged(w) => write!(f, "damaged CorelDRAW file: {w}"),
            Error::TooLarge(w) => write!(f, "CorelDRAW file too large: {w}"),
        }
    }
}

impl std::error::Error for Error {}

/// A colour as the file states it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Color {
    /// 0–255 per channel.
    Rgb([u8; 3]),
    /// Ink amounts 0–1.
    Cmyk([f32; 4]),
    /// 0 (black) – 255 (white).
    Gray(u8),
}

impl Color {
    pub const BLACK: Color = Color::Cmyk([0.0, 0.0, 0.0, 1.0]);
}

/// An object's outline.
#[derive(Clone, Debug, PartialEq)]
pub struct Stroke {
    pub color: Color,
    /// Points; 0 is a hairline.
    pub width: f64,
    /// 0 butt, 1 round, 2 square.
    pub cap: u8,
    /// 0 miter, 1 round, 2 bevel.
    pub join: u8,
    /// Dash and gap lengths in points (empty: solid).
    pub dashes: Vec<f64>,
}

/// One drawn object.
#[derive(Clone, Debug, PartialEq)]
pub struct Shape {
    /// In points, page centre at the origin, y up.
    pub path: BezPath,
    pub fill: Option<Color>,
    pub stroke: Option<Stroke>,
}

/// A shape or a group of objects.
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    Shape(Shape),
    Group(Vec<Object>),
}

/// A layer of a page, bottom to top.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layer {
    pub name: String,
    pub objects: Vec<Object>,
}

/// One page.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Page {
    /// Bottom layer first.
    pub layers: Vec<Layer>,
}

/// A CorelDRAW document as far as it could be read.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Drawing {
    /// The format version (CorelDRAW 9 is 900, X3 1300, 2017 1900…).
    pub version: u32,
    /// The page size in points, when the file states one.
    pub page_size: Option<(f64, f64)>,
    /// Pages with something on them (master pages left out).
    pub pages: Vec<Page>,
    /// What wasn't read or was approximated, once each.
    pub warnings: Vec<String>,
}

impl Drawing {
    /// The number of shapes on every page.
    pub fn shape_count(&self) -> usize {
        fn count(o: &[Object]) -> usize {
            o.iter().map(|o| if let Object::Group(g) = o { count(g) } else { 1 }).sum()
        }
        self.pages.iter().flat_map(|p| &p.layers).map(|l| count(&l.objects)).sum()
    }
}

/// Read a CorelDRAW file.
pub fn read(bytes: &[u8], limits: Limits) -> Result<Drawing, Error> {
    let file = container::open(bytes, limits)?;
    parse::drawing(&file, limits)
}

/// The preview picture a CorelDRAW file carries, as a complete BMP or PNG file (`None` when it
/// has none that can be found).
pub fn thumbnail(bytes: &[u8]) -> Option<Vec<u8>> {
    container::thumbnail(bytes, Limits::default())
}
