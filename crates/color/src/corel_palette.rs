//! CorelDRAW colour palettes (`.xml`, read only), written from Corel's public description of the
//! palette file: a `<palette name>` holding `<colors>` whose `<page>`s list `<color cs tints name>`
//! entries, tints being the colour model's components from 0 to 1. Spot colours (such as the
//! palettes of a colour system a user owns) are `<colorspaces>` entries `<cs name>` that give the
//! ink's colour in one or more models; a palette colour whose `cs` names one is that ink at the
//! colour's tint.
//!
//! A page with a name becomes a colour group; the colours of unnamed pages are ungrouped.
//! Registration colours and models this reader doesn't know are left out.

use std::collections::HashMap;

use crate::{Color, Paint, Swatch, SwatchGroup, SwatchLibrary};

/// The most colours read from one palette.
const MAX_COLORS: usize = 100_000;
/// The largest palette file read (bytes).
const MAX_BYTES: usize = 64 << 20;
/// The most characters of a name that are kept.
const MAX_NAME: usize = 1024;

/// Does `text` look like a CorelDRAW palette? (Its first KiB has the `<palette` root.)
pub fn sniff(text: &str) -> bool {
    let head: String = text.chars().take(1024).collect();
    let t = head.trim_start_matches('\u{feff}').trim_start();
    t.starts_with('<') && head.contains("<palette")
}

/// Read a CorelDRAW palette; `fallback` names it when the file doesn't.
pub fn read(text: &str, fallback: &str) -> Result<SwatchLibrary, String> {
    if text.len() > MAX_BYTES {
        return Err("the palette file is too large".into());
    }
    let text = text.trim_start_matches('\u{feff}');
    let doc = roxmltree::Document::parse(text).map_err(|e| format!("not a CorelDRAW palette: {e}"))?;
    let root = doc.root_element();
    if !root.has_tag_name("palette") {
        return Err("not a CorelDRAW palette (no <palette>)".into());
    }
    let name = root.attribute("name").map(clean_name).filter(|n| !n.is_empty()).unwrap_or_else(|| fallback.to_string());
    // The spot inks: name → their colour in the first model this reader knows (RGB first, as
    // shown on screen, then CMYK, then any other).
    let mut inks: HashMap<String, Color> = HashMap::new();
    for cs in root.children().filter(|n| n.has_tag_name("colorspaces")).flat_map(|n| n.children()).filter(|n| n.has_tag_name("cs")) {
        let Some(ink) = cs.attribute("name") else { continue };
        let mut found: Vec<(u8, Color)> = cs
            .children()
            .filter(|n| n.has_tag_name("color"))
            .filter_map(|c| {
                let model = c.attribute("cs")?.to_ascii_uppercase();
                let color = model_color(&model, &tints(c.attribute("tints")?)?)?;
                let rank = match model.as_str() {
                    "RGB" => 0,
                    "CMYK" | "CMYK255" => 1,
                    _ => 2,
                };
                Some((rank, color))
            })
            .collect();
        found.sort_by_key(|(rank, _)| *rank);
        if let Some((_, c)) = found.first() {
            inks.insert(ink.to_string(), *c);
        }
    }
    let mut lib = SwatchLibrary { name, ..Default::default() };
    let mut taken = std::collections::HashSet::new();
    let mut count = 0usize;
    for (i, page) in root.children().filter(|n| n.has_tag_name("colors")).flat_map(|n| n.children()).filter(|n| n.has_tag_name("page")).enumerate() {
        let mut list = vec![];
        for c in page.children().filter(|n| n.has_tag_name("color")) {
            if count >= MAX_COLORS {
                break;
            }
            let Some(model) = c.attribute("cs") else { continue };
            let Some(values) = tints(c.attribute("tints").unwrap_or("")) else { continue };
            let (color, spot) = match inks.get(model) {
                Some(ink) => (tinted(*ink, values.first().copied().unwrap_or(1.0)), true),
                None => match model_color(&model.to_ascii_uppercase(), &values) {
                    Some(color) => (color, false),
                    None => continue,
                },
            };
            let base = c.attribute("name").map(clean_name).filter(|n| !n.is_empty()).unwrap_or_else(|| value_name(color));
            let name = unique(&mut taken, base);
            list.push(Swatch { name, paint: Paint::solid(color), global: spot, spot });
            count += 1;
        }
        match page.attribute("name").map(clean_name).filter(|n| !n.is_empty()) {
            Some(group) if !list.is_empty() => lib.groups.push(SwatchGroup { name: group, swatches: list }),
            _ if i == 0 || lib.groups.is_empty() => lib.swatches.extend(list),
            _ => lib.groups.push(SwatchGroup { name: format!("Page {}", i + 1), swatches: list }),
        }
    }
    if lib.is_empty() {
        return Err("the CorelDRAW palette has no colours this app reads".into());
    }
    Ok(lib)
}

/// Vetorial: read every component in place; an invalid component rejects the colour.
fn tints(s: &str) -> Option<Vec<f32>> {
    // A missing tint retains the default full-strength spot ink.
    if s.trim().is_empty() {
        return Some(Vec::new());
    }
    s.split(',').map(|v| v.trim().parse::<f32>().ok().filter(|v| v.is_finite())).collect()
}

fn clean_name(s: &str) -> String {
    s.chars().filter(|c| !c.is_control()).take(MAX_NAME).collect::<String>().trim().to_string()
}

fn unique(taken: &mut std::collections::HashSet<String>, base: String) -> String {
    if taken.insert(base.clone()) {
        return base;
    }
    let name = (2..).map(|i| format!("{base} {i}")).find(|n| !taken.contains(n)).unwrap_or(base);
    taken.insert(name.clone());
    name
}

/// A spot ink at `tint` (0..1): process inks scaled, other colours mixed with white.
fn tinted(c: Color, tint: f32) -> Color {
    let t = tint.clamp(0.0, 1.0);
    match c {
        Color::Cmyk { c, m, y, k } => Color::cmyk(c * t, m * t, y * t, k * t),
        Color::Gray { k } => Color::gray(k * t),
        _ if t >= 1.0 => c,
        _ => {
            let [r, g, b] = c.to_rgb_uncalibrated().map(|v| 1.0 - (1.0 - v) * t);
            Color::rgb(r, g, b)
        }
    }
}

/// The colour of model `model` (upper case) with tints `v` (0..1), `None` for a model this reader
/// doesn't know or too few tints.
fn model_color(model: &str, v: &[f32]) -> Option<Color> {
    let u = |i: usize| v.get(i).map(|x| x.clamp(0.0, 1.0));
    Some(match model {
        "CMYK" | "CMYK255" => Color::cmyk(u(0)?, u(1)?, u(2)?, u(3)?),
        "CMY" => Color::cmyk(u(0)?, u(1)?, u(2)?, 0.0),
        "RGB" => Color::rgb(u(0)?, u(1)?, u(2)?),
        "BGR" => Color::rgb(u(2)?, u(1)?, u(0)?),
        // Grayscale is a brightness: 1 is white.
        "GRAY" | "GRAYSCALE" => Color::gray(1.0 - u(0)?),
        "HSB" | "HSV" => {
            let [r, g, b] = hsv(u(0)?, u(1)?, u(2)?);
            Color::rgb(r, g, b)
        }
        "HLS" => {
            let [r, g, b] = hls(u(0)?, u(1)?, u(2)?);
            Color::rgb(r, g, b)
        }
        "YIQ" => {
            // Y 0..1; I and Q stored from 0 to 1 around their middle.
            let (y, i, q) = (u(0)?, u(1)? * 1.192 - 0.596, u(2)? * 1.046 - 0.523);
            Color::rgb(
                (y + 0.956 * i + 0.621 * q).clamp(0.0, 1.0),
                (y - 0.272 * i - 0.647 * q).clamp(0.0, 1.0),
                (y - 1.106 * i + 1.703 * q).clamp(0.0, 1.0),
            )
        }
        "LAB" => {
            let (l, a, b) = (*v.first()?, *v.get(1)?, *v.get(2)?);
            if (0.0..=1.0).contains(&l) && a.abs() <= 1.0 && b.abs() <= 1.0 {
                // Stored from 0 to 1: L over 0..100; a and b around their middle, or signed when
                // either is negative.
                let ab = |x: f32| if a < 0.0 || b < 0.0 { x * 128.0 } else { x * 255.0 - 128.0 };
                Color::lab(l * 100.0, ab(a), ab(b))
            } else {
                Color::lab(l.clamp(0.0, 100.0), a.clamp(-128.0, 127.0), b.clamp(-128.0, 127.0))
            }
        }
        _ => return None,
    })
}

fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h6 = (h.fract() * 6.0).clamp(0.0, 6.0);
    let c = v * s;
    let x = c * (1.0 - (h6 % 2.0 - 1.0).abs());
    let (r, g, b) = match h6 as u32 {
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

fn hls(h: f32, l: f32, s: f32) -> [f32; 3] {
    let v = if l <= 0.5 { l * (1.0 + s) } else { l + s - l * s };
    if v <= 0.0 {
        return [0.0; 3];
    }
    hsv(h, 2.0 * (v - l) / v, v)
}

/// A colour named by its values, for an unnamed palette entry.
fn value_name(c: Color) -> String {
    let p = |v: f32| (v * 100.0).round() as i32;
    match c {
        Color::Cmyk { c, m, y, k } => format!("C{} M{} Y{} K{}", p(c), p(m), p(y), p(k)),
        Color::Gray { k } => format!("K{}", p(k)),
        Color::Lab { l, a, b } => format!("L{} a{} b{}", l.round(), a.round(), b.round()),
        Color::Rgb { .. } => c.to_hex().to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"<?xml version="1.0"?>
<palette guid="00000000-0000-0000-0000-000000000000" name="My Palette">
  <colorspaces>
    <cs name="Ink 100" fixedID="3">
      <color cs="LAB" tints="0.9,0.5,0.8"/>
      <color cs="CMYK" tints="0,0.1,0.8,0"/>
    </cs>
  </colorspaces>
  <colors>
    <page>
      <color cs="CMYK" tints="0,0,0,1" name="Black"/>
      <color cs="RGB" tints="1,0,0" name="Red"/>
      <color cs="GRAY" tints="1"/>
      <color cs="Ink 100" tints="0.5" name="Ink 100 50%"/>
      <color cs="REGISTRATION" tints="1" name="Registration"/>
      <color cs="RGB" tints="0,1,0" name="Red"/>
    </page>
    <page name="Blues">
      <color cs="HSB" tints="0.6667,1,1" name="Blue"/>
    </page>
  </colors>
</palette>"#;

    #[test]
    fn reads_colours_spot_inks_and_pages() {
        assert!(sniff(SAMPLE));
        let lib = read(SAMPLE, "fallback").unwrap();
        assert_eq!(lib.name, "My Palette");
        let names: Vec<&str> = lib.swatches.iter().map(|w| w.name.as_str()).collect();
        assert_eq!(names, ["Black", "Red", "K0", "Ink 100 50%", "Red 2"]);
        assert_eq!(lib.swatches[0].paint.color(), Some(Color::cmyk(0.0, 0.0, 0.0, 1.0)));
        assert_eq!(lib.swatches[2].paint.color(), Some(Color::gray(0.0)), "grayscale 1 is white");
        let ink = &lib.swatches[3];
        assert!(ink.spot && ink.global);
        assert_eq!(ink.paint.color(), Some(Color::cmyk(0.0, 0.05, 0.4, 0.0)), "CMYK of the ink at 50 %");
        assert_eq!(lib.groups.len(), 1);
        assert_eq!(lib.groups[0].name, "Blues");
        let [r, g, b] = lib.groups[0].swatches[0].paint.color().unwrap().to_rgb_uncalibrated();
        assert!(r < 0.01 && g < 0.01 && b > 0.99, "{r} {g} {b}");
    }

    /// Vetorial: invalid components must not shift the remaining colour channels.
    #[test]
    fn invalid_tints_do_not_change_colour_channels() {
        for invalid in ["NaN", "inf", "-inf", "1e40", "oops", ""] {
            let xml = format!(
                r#"<palette><colors><page>
                <color cs="RGB" tints="{invalid},1,0,0" name="Invalid RGB"/>
                <color cs="CMYK" tints="0,{invalid},1,0,0" name="Invalid CMYK"/>
                <color cs="RGB" tints="0,1,0" name="Valid green"/>
                </page></colors></palette>"#
            );
            let lib = read(&xml, "test").unwrap();
            assert_eq!(lib.swatches.len(), 1, "{invalid}");
            assert_eq!(lib.swatches[0].name, "Valid green");
            assert_eq!(lib.swatches[0].paint.color(), Some(Color::rgb(0.0, 1.0, 0.0)));
        }
    }

    /// Vetorial: a malformed spot tint is not the same as an omitted tint.
    #[test]
    fn invalid_spot_tints_are_skipped_but_missing_tints_default() {
        let xml = r#"<palette>
            <colorspaces><cs name="Ink">
                <color cs="RGB" tints="NaN,1,0,0"/>
                <color cs="CMYK" tints="0,1,0,0"/>
            </cs></colorspaces>
            <colors><page>
                <color cs="Ink" tints="NaN" name="Invalid"/>
                <color cs="Ink" name="Full"/>
                <color cs="Ink" tints="0.5" name="Half"/>
            </page></colors>
        </palette>"#;
        let lib = read(xml, "test").unwrap();
        assert_eq!(lib.swatches.len(), 2);
        assert_eq!(lib.swatches[0].name, "Full");
        assert_eq!(lib.swatches[0].paint.color(), Some(Color::cmyk(0.0, 1.0, 0.0, 0.0)));
        assert_eq!(lib.swatches[1].paint.color(), Some(Color::cmyk(0.0, 0.5, 0.0, 0.0)));
        assert!(lib.swatches.iter().all(|s| s.spot && s.global));
    }

    #[test]
    fn rejects_other_files() {
        assert!(!sniff("GIMP Palette\n"));
        assert!(read("<svg/>", "x").is_err());
        assert!(read("<palette><colors/></palette>", "x").is_err());
        assert!(read("<palette", "x").is_err());
    }

    #[test]
    fn hostile_values_never_panic() {
        for tints in ["", "nan,inf,-inf", "1e40,-1e40,5", "0.5", ",,,,", "-1,-1,-1,-1"] {
            for cs in ["CMYK", "RGB", "LAB", "HSB", "HLS", "YIQ", "GRAY", "CMY", "BGR"] {
                let x =
                    format!(r#"<palette><colors><page><color cs="{cs}" tints="{tints}"/><color cs="RGB" tints="0,0,0"/></page></colors></palette>"#);
                let _ = read(&x, "x").unwrap();
            }
        }
    }
}
