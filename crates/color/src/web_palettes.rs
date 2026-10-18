//! Vetorial: more palette files people download (palette sites, other paint and design apps),
//! each read from its public description:
//! - `.aco` color swatches (binary; version 1 and 2 with names; RGB, HSB, CMYK, Lab and grayscale),
//!   layout from the Photoshop File Formats Specification, "Color Swatches",
//!   <https://www.adobe.com/devnet-apps/photoshop/fileformatashtml/#50577411_pgfId-1055819>;
//! - `.act` color tables (binary: 256 RGB triples, then optionally a count and transparent index),
//!   same specification, "Color Table";
//! - RIFF `.pal` palettes (`RIFF`…`PAL data`, little-endian: version, count, then R G B flags),
//!   <https://www.aelius.com/njh/wavemetatools/doc/riffmci.pdf>;
//! - JASC `.pal` text palettes (`JASC-PAL`, version, count, then `r g b` lines);
//! - LibreOffice / OpenOffice `.soc` colour tables (XML `draw:color` elements with `draw:name` and
//!   `draw:color`);
//! - hex lists: `.hex` (one RRGGBB per line), paint.net `.txt` (AARRGGBB per line, `;` comments)
//!   and any text with `#rrggbb` / `#rgb` colours in it (CSS, SCSS, JSON exports of palette
//!   sites), a colour named after its line's `name:` when the line has one.

use crate::{Color, Paint, Swatch, SwatchLibrary};

/// At most this many colours are read from one file.
const MAX_COLORS: usize = 65_536;

/// Names handed out once each: a repeated name gets a number ("Red 2").
#[derive(Default)]
struct Names(std::collections::HashSet<String>);

impl Names {
    fn unique(&mut self, base: String) -> String {
        let base = if base.trim().is_empty() { "Color".to_string() } else { base.trim().to_string() };
        if self.0.insert(base.clone()) {
            return base;
        }
        // Each pass tries the next number, and only finitely many names are taken.
        let mut n = 2usize;
        loop {
            let name = format!("{base} {n}");
            if self.0.insert(name.clone()) {
                return name;
            }
            n += 1;
        }
    }
}

fn rgb_name(r: u8, g: u8, b: u8) -> String {
    format!("R={r} G={g} B={b}")
}

/// A library of `colors` (name, colour) called `name`.
fn library(name: &str, colors: Vec<(String, Color)>) -> SwatchLibrary {
    let mut names = Names::default();
    let swatches = colors
        .into_iter()
        .take(MAX_COLORS)
        .map(|(n, c)| Swatch { name: names.unique(n), paint: Paint::solid(c), global: false, spot: false })
        .collect();
    SwatchLibrary { name: name.into(), swatches, ..Default::default() }
}

fn be16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_be_bytes([x[0], x[1]]))
}

fn le16(b: &[u8], at: usize) -> Option<u16> {
    b.get(at..at + 2).map(|x| u16::from_le_bytes([x[0], x[1]]))
}

/// The binary palettes: `.aco`, RIFF `.pal`, `.act`. `None` when `bytes` are none of them.
pub fn read_binary(bytes: &[u8], name: &str) -> Option<Result<SwatchLibrary, String>> {
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"PAL ") {
        return Some(read_riff(bytes, name));
    }
    if let Some(colors) = read_aco(bytes) {
        return Some(Ok(library(name, colors)));
    }
    if (bytes.len() == 768 || bytes.len() == 772) && std::str::from_utf8(bytes).is_err() {
        return Some(Ok(read_act(bytes, name)));
    }
    None
}

/// Are `bytes` an `.aco` or RIFF palette (the ones whose layout shows what they are)?
pub fn sniff_binary(bytes: &[u8]) -> bool {
    (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"PAL ")) || read_aco(bytes).is_some()
}

/// One `.aco` colour: its colour space and four values.
fn aco_color(space: u16, v: [u16; 4]) -> Option<Color> {
    let u = |x: u16| f32::from(x) / 65535.0;
    Some(match space {
        0 => Color::rgb(u(v[0]), u(v[1]), u(v[2])),
        1 => {
            let (h, s, b) = (u(v[0]) * 360.0, u(v[1]), u(v[2]));
            let [r, g, bl] = hsb_to_rgb(h, s, b);
            Color::rgb(r, g, bl)
        }
        // Ink is stored inverted: 0 is 100 %.
        2 => Color::cmyk(1.0 - u(v[0]), 1.0 - u(v[1]), 1.0 - u(v[2]), 1.0 - u(v[3])),
        7 => Color::lab(f32::from(v[0]) / 100.0, f32::from(v[1] as i16) / 100.0, f32::from(v[2] as i16) / 100.0),
        8 => Color::gray((f32::from(v[0]) / 10000.0).clamp(0.0, 1.0)),
        _ => return None,
    })
}

fn hsb_to_rgb(h: f32, s: f32, v: f32) -> [f32; 3] {
    let c = v * s;
    let hp = (h.rem_euclid(360.0)) / 60.0;
    let x = c * (1.0 - (hp % 2.0 - 1.0).abs());
    let (r, g, b) = match hp as u32 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    [r + m, g + m, b + m]
}

/// The colours of an `.aco` file (its version 2 section, with names, when it has one), or `None`
/// when `bytes` aren't one: a version 1 section must fill the file or be followed by a version 2
/// section, whose names must fit it exactly.
fn read_aco(bytes: &[u8]) -> Option<Vec<(String, Color)>> {
    let section = |at: usize, version: u16| -> Option<(Vec<(String, Color)>, usize)> {
        if be16(bytes, at)? != version {
            return None;
        }
        let count = usize::from(be16(bytes, at + 2)?);
        let mut p = at + 4;
        let mut out = Vec::with_capacity(count.min(MAX_COLORS));
        for _ in 0..count {
            let space = be16(bytes, p)?;
            let v = [be16(bytes, p + 2)?, be16(bytes, p + 4)?, be16(bytes, p + 6)?, be16(bytes, p + 8)?];
            p += 10;
            let mut name = String::new();
            if version == 2 {
                be16(bytes, p)?; // always 0
                let n = usize::from(be16(bytes, p + 2)?);
                p += 4;
                let units: Vec<u16> = (0..n).map(|k| be16(bytes, p + 2 * k)).collect::<Option<_>>()?;
                p += 2 * n;
                name = String::from_utf16_lossy(&units).trim_end_matches('\0').trim().to_string();
            }
            if let Some(c) = aco_color(space, v) {
                out.push((name, c));
            }
        }
        Some((out, p))
    };
    let (v1, end) = section(0, 1).or_else(|| section(0, 2))?;
    if end == bytes.len() {
        return Some(v1.into_iter().map(|(n, c)| (if n.is_empty() { super_name(c) } else { n }, c)).collect());
    }
    let (v2, end2) = section(end, 2)?;
    (end2 == bytes.len()).then(|| v2.into_iter().map(|(n, c)| (if n.is_empty() { super_name(c) } else { n }, c)).collect())
}

/// A colour's name from its values.
fn super_name(c: Color) -> String {
    let pct = |x: f32| (x * 100.0).round() as i32;
    let byte = |x: f32| (x.clamp(0.0, 1.0) * 255.0).round() as i32;
    match c {
        Color::Rgb { r, g, b } => format!("R={} G={} B={}", byte(r), byte(g), byte(b)),
        Color::Cmyk { c, m, y, k } => format!("C={} M={} Y={} K={}", pct(c), pct(m), pct(y), pct(k)),
        Color::Gray { k } => format!("Gray K={}", pct(k)),
        Color::Lab { l, a, b } => format!("L={} a={} b={}", l.round() + 0.0, a.round() + 0.0, b.round() + 0.0),
    }
}

/// An `.act` table: 256 RGB triples, then optionally a big-endian count of the colours used.
fn read_act(bytes: &[u8], name: &str) -> SwatchLibrary {
    let count = if bytes.len() == 772 { be16(bytes, 768).map_or(256, usize::from).min(256) } else { 256 };
    let colors = bytes
        .as_chunks::<3>()
        .0
        .iter()
        .take(count)
        .map(|c| {
            let (r, g, b) = (c[0], c[1], c[2]);
            (rgb_name(r, g, b), Color::rgb8(r, g, b))
        })
        .collect();
    library(name, colors)
}

/// A RIFF palette: its `data` chunk holds a version, a count and R G B flags entries.
fn read_riff(bytes: &[u8], name: &str) -> Result<SwatchLibrary, String> {
    let mut p = 12;
    while p + 8 <= bytes.len() {
        let id = bytes.get(p..p + 4).ok_or("the palette is cut short")?;
        let size = bytes.get(p + 4..p + 8).map_or(0, |x| u32::from_le_bytes([x[0], x[1], x[2], x[3]])) as usize;
        if id == b"data" {
            let count = usize::from(le16(bytes, p + 10).ok_or("the palette is cut short")?);
            let start = p + 12;
            let colors = (0..count)
                .map_while(|k| bytes.get(start + 4 * k..start + 4 * k + 3))
                .map(|c| (rgb_name(c[0], c[1], c[2]), Color::rgb8(c[0], c[1], c[2])))
                .collect();
            return Ok(library(name, colors));
        }
        p = p.saturating_add(8).saturating_add(size).saturating_add(size & 1);
    }
    Err("the palette has no colours".into())
}

/// The text palettes: JASC `.pal`, LibreOffice `.soc`, hex lists and text with `#rrggbb` colours.
/// `None` when `text` has no colour in any of them.
pub fn read_text(text: &str, name: &str) -> Option<Result<SwatchLibrary, String>> {
    let text = text.trim_start_matches('\u{feff}');
    let mut lines = text.lines().map(str::trim);
    if lines.next() == Some("JASC-PAL") {
        let colors: Vec<(String, Color)> = lines
            .skip(2)
            .filter_map(|l| {
                let v: Vec<u8> = l.split_whitespace().take(3).map_while(|x| x.parse().ok()).collect();
                let [r, g, b] = v[..] else { return None };
                Some((rgb_name(r, g, b), Color::rgb8(r, g, b)))
            })
            .collect();
        return Some(Ok(library(name, colors)));
    }
    if text.contains("draw:color") && text.trim_start().starts_with('<') {
        return Some(read_soc(text, name));
    }
    let colors = hex_colors(text);
    (!colors.is_empty()).then(|| Ok(library(name, colors)))
}

/// Is `text` a JASC or LibreOffice palette (the text palettes whose start shows what they are)?
pub fn sniff_text(text: &str) -> bool {
    let t = text.trim_start_matches('\u{feff}').trim_start();
    t.starts_with("JASC-PAL") || (t.starts_with('<') && t.contains("office:color-table"))
}

fn read_soc(text: &str, name: &str) -> Result<SwatchLibrary, String> {
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not a colour table: {e}"))?;
    let colors = doc
        .descendants()
        .filter(|n| n.tag_name().name() == "color")
        .filter_map(|n| {
            let attr = |key: &str| n.attributes().find(|a| a.name() == key).map(|a| a.value().to_string());
            let c = parse_hex(attr("color")?.trim().trim_start_matches('#'))?;
            Some((attr("name").unwrap_or_default(), c))
        })
        .collect::<Vec<_>>();
    if colors.is_empty() { Err("the colour table has no colours".into()) } else { Ok(library(name, colors)) }
}

/// `rrggbb`, `rgb` or `aarrggbb` hex digits as a colour.
fn parse_hex(h: &str) -> Option<Color> {
    if !h.chars().all(|c| c.is_ascii_hexdigit()) {
        return None;
    }
    let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    match h.len() {
        6 => Some(Color::rgb8(byte(0)?, byte(2)?, byte(4)?)),
        8 => Some(Color::rgb8(byte(2)?, byte(4)?, byte(6)?)),
        3 => {
            let d = |i: usize| h.get(i..i + 1).and_then(|x| u8::from_str_radix(x, 16).ok()).map(|v| v * 17);
            Some(Color::rgb8(d(0)?, d(1)?, d(2)?))
        }
        _ => None,
    }
}

/// The colours of a hex list or of any text: a line that is only `rrggbb` / `#rrggbb` /
/// `aarrggbb` is a colour; elsewhere each `#rrggbb` or `#rgb` is one, named after the line's
/// `name:` (or `name =`, `"name":`) when it holds one colour. `;` and `//` lines are comments.
fn hex_colors(text: &str) -> Vec<(String, Color)> {
    let mut out = vec![];
    for line in text.lines().map(str::trim) {
        if out.len() >= MAX_COLORS {
            break;
        }
        if line.is_empty() || line.starts_with(';') || line.starts_with("//") {
            continue;
        }
        let bare = line.trim_start_matches('#');
        if (bare.len() == 6 || (bare.len() == 8 && !line.starts_with('#')))
            && let Some(c) = parse_hex(bare)
        {
            out.push((super_name(c), c));
            continue;
        }
        let mut found = vec![];
        let b = line.as_bytes();
        let mut i = 0;
        while i < b.len() {
            if b[i] == b'#' {
                let n = b[i + 1..].iter().take_while(|c| c.is_ascii_hexdigit()).count();
                if (n == 6 || n == 3)
                    && let Some(c) = line.get(i + 1..i + 1 + n).and_then(parse_hex)
                {
                    found.push(c);
                }
                i += 1 + n;
            } else {
                i += 1;
            }
        }
        let label = line
            .split_once(':')
            .or_else(|| line.split_once('='))
            .map(|(k, _)| k.trim().trim_matches(|c: char| c == '"' || c == '\'' || c == '-' || c == '$' || c == '@').trim().to_string())
            .filter(|k| !k.is_empty() && !k.contains('#') && k.len() <= 64);
        let one = found.len() == 1;
        for c in found {
            let name = match &label {
                Some(l) if one => l.clone(),
                _ => super_name(c),
            };
            out.push((name, c));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(lib: &SwatchLibrary) -> Vec<(String, [u8; 3])> {
        lib.swatches
            .iter()
            .map(|s| {
                let Some(Color::Rgb { r, g, b }) = s.paint.color() else { panic!("{s:?}") };
                let q = |x: f32| (x * 255.0).round() as u8;
                (s.name.clone(), [q(r), q(g), q(b)])
            })
            .collect()
    }

    fn aco(v2: bool, colors: &[(u16, [u16; 4], &str)]) -> Vec<u8> {
        let mut b = vec![];
        b.extend(1u16.to_be_bytes());
        b.extend((colors.len() as u16).to_be_bytes());
        for (s, v, _) in colors {
            b.extend(s.to_be_bytes());
            v.iter().for_each(|x| b.extend(x.to_be_bytes()));
        }
        if v2 {
            b.extend(2u16.to_be_bytes());
            b.extend((colors.len() as u16).to_be_bytes());
            for (s, v, n) in colors {
                b.extend(s.to_be_bytes());
                v.iter().for_each(|x| b.extend(x.to_be_bytes()));
                let units: Vec<u16> = n.encode_utf16().chain([0]).collect();
                b.extend(0u16.to_be_bytes());
                b.extend((units.len() as u16).to_be_bytes());
                units.iter().for_each(|u| b.extend(u.to_be_bytes()));
            }
        }
        b
    }

    #[test]
    fn aco_files_are_read_with_their_names() {
        let file = aco(true, &[(0, [65535, 0, 0, 0], "Red"), (2, [0, 65535, 65535, 65535], "Cyan"), (99, [0; 4], "Odd")]);
        assert!(sniff_binary(&file));
        let lib = read_binary(&file, "mine").unwrap().unwrap();
        assert_eq!(lib.swatches.len(), 2, "an unknown colour space is skipped");
        assert_eq!(lib.swatches[0].name, "Red");
        assert_eq!(lib.swatches[0].paint.color(), Some(Color::rgb(1.0, 0.0, 0.0)));
        assert_eq!(lib.swatches[1].paint.color(), Some(Color::cmyk(1.0, 0.0, 0.0, 0.0)));
        // Version 1 alone: named by value.
        let lib = read_binary(&aco(false, &[(0, [0, 65535, 0, 0], "")]), "mine").unwrap().unwrap();
        assert_eq!(rgb(&lib), [("R=0 G=255 B=0".to_string(), [0, 255, 0])]);
        // Cut short: not an .aco.
        let mut cut = aco(true, &[(0, [1, 2, 3, 4], "Abc")]);
        cut.pop();
        assert!(read_aco(&cut).is_none());
    }

    #[test]
    fn act_and_riff_palettes_are_read() {
        let mut act = vec![0u8; 772];
        act[0..6].copy_from_slice(&[255, 0, 0, 0, 0, 255]);
        act[768..770].copy_from_slice(&2u16.to_be_bytes());
        let lib = read_binary(&act, "t").unwrap().unwrap();
        assert_eq!(rgb(&lib).iter().map(|c| c.1).collect::<Vec<_>>(), [[255, 0, 0], [0, 0, 255]]);
        let mut riff = b"RIFF\0\0\0\0PAL data".to_vec();
        riff.extend(12u32.to_le_bytes());
        riff.extend(0x0300u16.to_le_bytes());
        riff.extend(2u16.to_le_bytes());
        riff.extend([10, 20, 30, 0, 40, 50, 60, 0]);
        assert!(sniff_binary(&riff));
        let lib = read_binary(&riff, "t").unwrap().unwrap();
        assert_eq!(rgb(&lib).iter().map(|c| c.1).collect::<Vec<_>>(), [[10, 20, 30], [40, 50, 60]]);
    }

    #[test]
    fn text_palettes_are_read() {
        let lib = read_text("JASC-PAL\r\n0100\r\n2\r\n255 0 0\r\n0 128 255\r\n", "j").unwrap().unwrap();
        assert_eq!(rgb(&lib).iter().map(|c| c.1).collect::<Vec<_>>(), [[255, 0, 0], [0, 128, 255]]);
        let soc = r##"<?xml version="1.0"?><office:color-table xmlns:office="urn:o" xmlns:draw="urn:d"><draw:color draw:name="Sun" draw:color="#ffcc00"/></office:color-table>"##;
        assert!(sniff_text(soc));
        assert_eq!(rgb(&read_text(soc, "s").unwrap().unwrap()), [("Sun".to_string(), [255, 204, 0])]);
        // A .hex list and a paint.net palette.
        let lib = read_text("ff0000\n#00ff00\n", "h").unwrap().unwrap();
        assert_eq!(lib.swatches.len(), 2);
        let lib = read_text("; paint.net Palette File\nFF0000FF\n", "p").unwrap().unwrap();
        assert_eq!(rgb(&lib).iter().map(|c| c.1).collect::<Vec<_>>(), [[0, 0, 255]]);
        // CSS from a palette site: named after the property.
        let css = ":root {\n  --sunset-orange: #ff5733;\n  --sky: #3cf;\n}\n";
        assert_eq!(rgb(&read_text(css, "c").unwrap().unwrap()), [("sunset-orange".to_string(), [255, 87, 51]), ("sky".to_string(), [51, 204, 255])]);
        let json = r##"{"colors": ["#112233", "#445566"]}"##;
        assert_eq!(read_text(json, "x").unwrap().unwrap().swatches.len(), 2);
        assert!(read_text("no colours here", "x").is_none());
    }
}
