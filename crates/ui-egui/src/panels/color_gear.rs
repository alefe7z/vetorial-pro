//! Vetorial: the Color Gear tab of the right-hand dock, a colour-harmony wheel. Three wheels: RGB
//! (a hue/saturation disc with a brightness slider), RYB (the painter's ring, with a slider from
//! shade to tint) and Image (the main colours of a picture). Thirteen harmonies, each drawn as an
//! icon above the wheel: complementary, analogous (2, 3, 4), split complementary (with and without
//! the complement), triad, kite, square, rectangle (two ways), hexagon and shades. Drag any point
//! on the wheel to turn the harmony (and, on the RGB disc, set its saturation); ⇄ mirrors it and the
//! eyedropper takes the current fill as the base. A click on a colour paints the selection's fill,
//! a right click its stroke; Edit sets the base by value, Save keeps the colours as a palette, and
//! Palettes shows a saved one as a palette bar.
//! Commands: `colorGear.set`, `colorGear.pick`, `colorGear.apply`, `colorGear.save`,
//! `colorGear.image`.

use egui::{Color32, Pos2, Rect, Sense, Stroke, Ui, pos2, vec2};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::VectorcraftApp;
use crate::theme::Tokens;
use crate::widgets;

/// The panel's id (`window.panel`) and dock tab.
pub const ID: &str = "colorGear";

/// The wheel the colours come from.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Wheel {
    #[default]
    Rgb,
    Ryb,
    Image,
}

impl Wheel {
    const ALL: [Wheel; 3] = [Wheel::Rgb, Wheel::Ryb, Wheel::Image];
    fn id(self) -> &'static str {
        match self {
            Wheel::Rgb => "rgb",
            Wheel::Ryb => "ryb",
            Wheel::Image => "image",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Wheel::Rgb => "RGB",
            Wheel::Ryb => "RYB",
            Wheel::Image => "Image",
        }
    }
}

/// A harmony: id, English name and the hue offsets (degrees) of its colours from the base.
/// `shades` has no offsets: six steps of the base towards white.
pub const HARMONIES: &[(&str, &str, &[f32])] = &[
    ("complementary", "Complementary", &[0.0, 180.0]),
    ("analogous2", "Analogous (2)", &[0.0, -30.0]),
    ("analogous3", "Analogous (3)", &[0.0, -30.0, 30.0]),
    ("analogous4", "Analogous (4)", &[0.0, -30.0, -60.0, 30.0]),
    ("split", "Split Complementary", &[0.0, 150.0, 210.0]),
    ("splitComplement", "Split Complementary and Complement", &[0.0, 150.0, 180.0, 210.0]),
    ("triad", "Triad", &[0.0, 120.0, 240.0]),
    ("kite", "Kite", &[0.0, 120.0, 180.0, 240.0]),
    ("square", "Square", &[0.0, 90.0, 180.0, 270.0]),
    ("rectangle", "Rectangle", &[0.0, -60.0, 180.0, 120.0]),
    ("rectangleTilted", "Tilted Rectangle", &[0.0, 60.0, 180.0, 240.0]),
    ("hexagon", "Hexagon", &[0.0, 60.0, 120.0, 180.0, 240.0, 300.0]),
    ("shades", "Shades", &[]),
];

/// How many colours Shades and Image give.
const STEPS: usize = 6;

/// The panel's state, kept with the UI state.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Gear {
    pub wheel: Wheel,
    /// Index into [`HARMONIES`].
    pub harmony: usize,
    /// The base colour's hue on the current wheel, in degrees.
    pub hue: f32,
    /// RGB: the base's saturation (its distance from the disc's white centre), 0 to 1.
    pub saturation: f32,
    /// RGB: the brightness slider, 0 to 1.
    pub brightness: f32,
    /// RYB: the slider from shade (−1) through the pure colour (0) to tint (1).
    pub light: f32,
    /// ⇄: the harmony mirrored (its offsets turned the other way).
    pub mirror: bool,
    /// Image: the picture's main colours (8-bit RGB).
    pub image: Vec<[u8; 3]>,
}

impl Default for Gear {
    fn default() -> Self {
        // A warm yellow, as the wheel first shows.
        Gear { wheel: Wheel::Rgb, harmony: 0, hue: 53.0, saturation: 0.91, brightness: 1.0, light: 0.0, mirror: false, image: vec![] }
    }
}

/// HSV (hue in degrees) to RGB, 0 to 1.
pub fn hsv(h: f32, s: f32, v: f32) -> [f32; 3] {
    let h = h.rem_euclid(360.0) / 60.0;
    let c = v * s;
    let x = c * (1.0 - (h % 2.0 - 1.0).abs());
    let (r, g, b) = match h as u32 {
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

/// RGB (0 to 1) to HSV (hue in degrees).
pub fn to_hsv([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let d = max - min;
    let h = if d <= f32::EPSILON {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    let s = if max <= f32::EPSILON { 0.0 } else { d / max };
    (h, s, max)
}

/// The painter's wheel, clockwise from the top: yellow, yellow-green, green, blue-green, blue,
/// blue-violet, violet, red-violet, red, red-orange, orange, yellow-orange.
const RYB: [[u8; 3]; 12] = [
    [0xf2, 0xe2, 0x00],
    [0xbb, 0xd6, 0x15],
    [0x6a, 0xb7, 0x34],
    [0x3c, 0x8e, 0xaf],
    [0x01, 0x3f, 0xea],
    [0x3a, 0x1b, 0xd6],
    [0x58, 0x00, 0xeb],
    [0xa5, 0x00, 0x7c],
    [0xeb, 0x16, 0x00],
    [0xe9, 0x49, 0x02],
    [0xe9, 0x8d, 0x03],
    [0xeb, 0xb0, 0x00],
];

/// The colour at angle `a` (degrees clockwise from the top) of the painter's wheel.
pub fn ryb(a: f32) -> [f32; 3] {
    let t = a.rem_euclid(360.0) / 30.0;
    let i = (t.floor() as usize) % 12;
    let f = t - t.floor();
    let (p, q) = (RYB[i], RYB[(i + 1) % 12]);
    [0, 1, 2].map(|k| (f32::from(p[k]) + (f32::from(q[k]) - f32::from(p[k])) * f) / 255.0)
}

/// The angle of the painter's wheel whose colour is nearest `c` in hue.
fn ryb_angle(c: [f32; 3]) -> f32 {
    let (h, _, _) = to_hsv(c);
    let dist = |a: f32| {
        let (hh, _, _) = to_hsv(ryb(a));
        let d = (hh - h).rem_euclid(360.0);
        d.min(360.0 - d)
    };
    (0..360).map(|a| a as f32).min_by(|a, b| dist(*a).total_cmp(&dist(*b))).unwrap_or(0.0)
}

fn mix(a: [f32; 3], b: [f32; 3], k: f32) -> [f32; 3] {
    [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * k)
}

/// The RYB slider applied to `c`: towards black below 0, towards white above.
fn lighten(c: [f32; 3], light: f32) -> [f32; 3] {
    if light < 0.0 { mix(c, [0.0; 3], -light * 0.85) } else { mix(c, [1.0; 3], light * 0.85) }
}

impl Gear {
    fn offsets(&self) -> &'static [f32] {
        HARMONIES.get(self.harmony).map_or(&[], |h| h.2)
    }
    fn shades(&self) -> bool {
        self.offsets().is_empty()
    }
    fn sign(&self) -> f32 {
        if self.mirror { -1.0 } else { 1.0 }
    }

    /// The harmony's colours (RGB, 0 to 1), the base first.
    pub fn colors(&self) -> Vec<[f32; 3]> {
        let n = STEPS as f32;
        match self.wheel {
            Wheel::Rgb if self.shades() => (0..STEPS).map(|i| hsv(self.hue, self.saturation * (1.0 - i as f32 / n), self.brightness)).collect(),
            Wheel::Rgb => self.offsets().iter().map(|o| hsv(self.hue + self.sign() * o, self.saturation, self.brightness)).collect(),
            Wheel::Ryb if self.shades() => {
                let base = lighten(ryb(self.hue), self.light);
                (0..STEPS).map(|i| mix(base, [1.0; 3], i as f32 / n * 0.85)).collect()
            }
            Wheel::Ryb => self.offsets().iter().map(|o| lighten(ryb(self.hue + self.sign() * o), self.light)).collect(),
            Wheel::Image => self.image.iter().map(|c| c.map(|v| f32::from(v) / 255.0)).collect(),
        }
    }

    /// Where each colour's point sits on a wheel of radius 1 (y down), the base first.
    fn points(&self) -> Vec<egui::Vec2> {
        match self.wheel {
            Wheel::Rgb => {
                let at = |h: f32, s: f32| {
                    let a = h.to_radians();
                    vec2(a.cos(), -a.sin()) * s
                };
                if self.shades() {
                    (0..STEPS).map(|i| at(self.hue, self.saturation * (1.0 - i as f32 / STEPS as f32))).collect()
                } else {
                    self.offsets().iter().map(|o| at(self.hue + self.sign() * o, self.saturation)).collect()
                }
            }
            Wheel::Ryb => {
                let at = |a: f32| {
                    let a = a.to_radians();
                    vec2(a.sin(), -a.cos()) * RING_MID
                };
                if self.shades() { vec![at(self.hue)] } else { self.offsets().iter().map(|o| at(self.hue + self.sign() * o)).collect() }
            }
            Wheel::Image => vec![],
        }
    }

    /// Point `i` dragged to `p` (on a wheel of radius 1): the harmony turns so it lands there (and
    /// on the RGB disc takes its distance from the centre as the saturation).
    fn drag(&mut self, i: usize, p: egui::Vec2) {
        let off = if self.shades() { 0.0 } else { self.offsets().get(i).copied().unwrap_or(0.0) * self.sign() };
        match self.wheel {
            Wheel::Rgb => {
                let h = (-p.y).atan2(p.x).to_degrees();
                self.hue = (h - off).rem_euclid(360.0);
                let r = p.length().min(1.0);
                self.saturation = if self.shades() && i > 0 { (r / (1.0 - i as f32 / STEPS as f32)).min(1.0) } else { r };
            }
            Wheel::Ryb => {
                let a = p.x.atan2(-p.y).to_degrees();
                self.hue = (a - off).rem_euclid(360.0);
            }
            Wheel::Image => {}
        }
    }

    /// The base taken from colour `c` (the eyedropper, Edit).
    fn set_base(&mut self, c: [f32; 3]) {
        let (h, s, v) = to_hsv(c);
        if self.wheel == Wheel::Ryb {
            self.hue = ryb_angle(c);
            self.light = 0.0;
        } else {
            self.hue = h;
            self.saturation = s;
            self.brightness = v;
        }
    }
}

/// The RYB ring's middle, as a fraction of the wheel's radius.
const RING_MID: f32 = 0.86;
const RING_IN: f32 = 0.72;

fn hex(c: [f32; 3]) -> String {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    format!("#{r:02X}{g:02X}{b:02X}")
}

fn c32(c: [f32; 3]) -> Color32 {
    let [r, g, b] = c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8);
    Color32::from_rgb(r, g, b)
}

fn parse_hex(s: &str) -> Option<[f32; 3]> {
    let h = s.trim().trim_start_matches('#');
    let byte = |i: usize| h.get(i..i + 2).and_then(|x| u8::from_str_radix(x, 16).ok());
    (h.len() == 6).then_some(())?;
    Some([byte(0)?, byte(2)?, byte(4)?].map(|v| f32::from(v) / 255.0))
}

fn state(g: &Gear) -> Value {
    json!({
        "wheel": g.wheel.id(),
        "harmony": HARMONIES.get(g.harmony).map_or("", |h| h.0),
        "hue": g.hue,
        "saturation": g.saturation,
        "brightness": g.brightness,
        "light": g.light,
        "mirror": g.mirror,
        "colors": g.colors().into_iter().map(hex).collect::<Vec<_>>(),
    })
}

fn unit(p: &Value, key: &str, lo: f32, hi: f32) -> Result<Option<f32>, String> {
    match p.get(key) {
        None => Ok(None),
        Some(v) => v
            .as_f64()
            .map(|x| x as f32)
            .filter(|x| x.is_finite() && (lo..=hi).contains(x))
            .map(Some)
            .ok_or(format!("{key} is a number from {lo} to {hi}")),
    }
}

/// At most this many pixels a side are read from a picture for its colours.
const MAX_IMAGE_SIDE: u32 = 16_384;

/// The main colours of a picture's `bytes`: up to [`STEPS`], the most common first.
pub fn image_colors(bytes: &[u8]) -> Result<Vec<[u8; 3]>, String> {
    let mut reader = image::ImageReader::new(std::io::Cursor::new(bytes)).with_guessed_format().map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(MAX_IMAGE_SIDE);
    limits.max_image_height = Some(MAX_IMAGE_SIDE);
    limits.max_alloc = Some(512 * 1024 * 1024);
    reader.limits(limits);
    let img = reader.decode().map_err(|e| format!("can't read the picture: {e}"))?;
    let small = if img.width() > 96 || img.height() > 96 { img.thumbnail(96, 96) } else { img }.to_rgba8();
    let px: Vec<[f32; 3]> = small.pixels().filter(|p| p.0[3] > 127).map(|p| [f32::from(p.0[0]), f32::from(p.0[1]), f32::from(p.0[2])]).collect();
    Ok(kmeans(&px, STEPS))
}

/// `k` clusters of `px` (8-bit RGB as floats): their centres, the biggest first. The first centre
/// is the pixels' mean, each next one the pixel farthest from those so far.
fn kmeans(px: &[[f32; 3]], k: usize) -> Vec<[u8; 3]> {
    if px.is_empty() {
        return vec![];
    }
    let d2 = |a: &[f32; 3], b: &[f32; 3]| (0..3).map(|i| (a[i] - b[i]).powi(2)).sum::<f32>();
    let n = px.len() as f32;
    let mean = [0, 1, 2].map(|i| px.iter().map(|p| p[i]).sum::<f32>() / n);
    let mut centres = vec![mean];
    while centres.len() < k {
        let far = px.iter().max_by(|a, b| {
            let da = centres.iter().map(|c| d2(a, c)).fold(f32::INFINITY, f32::min);
            let db = centres.iter().map(|c| d2(b, c)).fold(f32::INFINITY, f32::min);
            da.total_cmp(&db)
        });
        match far {
            Some(p) if centres.iter().all(|c| d2(p, c) > 1.0) => centres.push(*p),
            _ => break,
        }
    }
    let mut counts = vec![0usize; centres.len()];
    for _ in 0..12 {
        let mut sums = vec![[0.0f32; 3]; centres.len()];
        counts = vec![0; centres.len()];
        for p in px {
            let j = (0..centres.len()).min_by(|&a, &b| d2(p, &centres[a]).total_cmp(&d2(p, &centres[b]))).unwrap_or(0);
            counts[j] += 1;
            (0..3).for_each(|i| sums[j][i] += p[i]);
        }
        for (j, c) in centres.iter_mut().enumerate() {
            if counts[j] > 0 {
                *c = sums[j].map(|s| s / counts[j] as f32);
            }
        }
    }
    let mut order: Vec<usize> = (0..centres.len()).filter(|&j| counts[j] > 0).collect();
    order.sort_by(|a, b| counts[*b].cmp(&counts[*a]));
    order.into_iter().map(|j| centres[j].map(|v| v.round().clamp(0.0, 255.0) as u8)).collect()
}

pub fn command(app: &mut VectorcraftApp, id: &str, p: &Value) -> Option<Result<Value, String>> {
    let r = match id {
        "colorGear.set" => (|| {
            let mut g = app.ui.color_gear.clone();
            if let Some(w) = p.get("wheel") {
                let w = w.as_str().ok_or("wheel is rgb, ryb or image")?;
                g.wheel = Wheel::ALL.into_iter().find(|x| x.id() == w).ok_or(format!("wheel is rgb, ryb or image, not `{w}`"))?;
            }
            if let Some(h) = p.get("harmony") {
                let h = h.as_str().ok_or("harmony is a harmony id")?;
                g.harmony = HARMONIES.iter().position(|x| x.0 == h).ok_or(format!("no harmony `{h}`"))?;
            }
            if let Some(c) = p.get("color") {
                let c = c.as_str().and_then(parse_hex).ok_or("color is #rrggbb")?;
                g.set_base(c);
            }
            if let Some(h) = unit(p, "hue", -3600.0, 3600.0)? {
                g.hue = h.rem_euclid(360.0);
            }
            if let Some(s) = unit(p, "saturation", 0.0, 1.0)? {
                g.saturation = s;
            }
            if let Some(b) = unit(p, "brightness", 0.0, 1.0)? {
                g.brightness = b;
            }
            if let Some(l) = unit(p, "light", -1.0, 1.0)? {
                g.light = l;
            }
            if let Some(m) = p.get("mirror") {
                g.mirror = m.as_bool().ok_or("mirror is true or false")?;
            }
            app.ui.color_gear = g;
            Ok(state(&app.ui.color_gear))
        })(),
        "colorGear.pick" => (|| {
            let c = super::active_paint(app).color().ok_or("the current colour isn't a solid colour")?;
            app.ui.color_gear.set_base(c.to_rgb());
            Ok(state(&app.ui.color_gear))
        })(),
        "colorGear.apply" => (|| {
            let i = p.get("index").and_then(Value::as_u64).ok_or("missing index")?;
            let colors = app.ui.color_gear.colors();
            let c = usize::try_from(i).ok().and_then(|i| colors.get(i)).copied().ok_or(format!("no colour {i}"))?;
            let stroke = p.get("stroke").and_then(Value::as_bool).unwrap_or(false);
            app.run(if stroke { "paint.setStroke" } else { "paint.setFill" }, json!({ "color": hex(c) }))?;
            super::color_gear_more::remember(&mut app.ui.color_gear_more, c.map(|v| (v.clamp(0.0, 1.0) * 255.0).round() as u8));
            Ok(json!({ "color": hex(c) }))
        })(),
        "colorGear.save" => (|| {
            let colors: Vec<String> = app.ui.color_gear.colors().into_iter().map(hex).collect();
            if colors.is_empty() {
                return Err("no colours to save".into());
            }
            let name = p.get("name").and_then(Value::as_str).unwrap_or("Color Gear");
            app.run("swatch.library.new", json!({ "name": name, "colors": colors }))
        })(),
        "colorGear.image" => (|| {
            let bytes = match (p.get("dataBase64").and_then(Value::as_str), p.get("path").and_then(Value::as_str)) {
                (Some(b64), _) => vectorcraft_format::base64_decode(b64).ok_or("dataBase64 isn't base64")?,
                (None, path) => {
                    let path = match path {
                        Some(p) => p.to_string(),
                        None => {
                            let pick = crate::FilePick {
                                filters: vec![("Images", &["png", "jpg", "jpeg", "gif", "bmp", "webp", "tif", "tiff"])],
                                ..Default::default()
                            };
                            match crate::picks::open(app, &pick) {
                                Some(p) => p,
                                None => return Ok(Value::Null),
                            }
                        }
                    };
                    app.services.read.as_ref().ok_or("no file reader")?(&path)?
                }
            };
            let colors = image_colors(&bytes)?;
            if colors.is_empty() {
                return Err("the picture has no visible colours".into());
            }
            app.ui.color_gear.image = colors;
            app.ui.color_gear.wheel = Wheel::Image;
            Ok(state(&app.ui.color_gear))
        })(),
        _ => return None,
    };
    Some(r)
}

/// A harmony's icon: the twelve segments of a ring, those of its colours lit and joined.
fn harmony_icon(ui: &Ui, r: Rect, offsets: &[f32], lit: Color32, dim: Color32) {
    let p = ui.painter();
    let c = r.center();
    let rad = r.width() * 0.42;
    let at = |a: f32, k: f32| c + vec2(a.to_radians().sin(), -a.to_radians().cos()) * rad * k;
    let on = |k: usize| {
        let a = k as f32 * 30.0;
        if offsets.is_empty() {
            k == 7
        } else {
            offsets.iter().any(|o| ((o.rem_euclid(360.0) - a).abs()) < 1.0 || ((o.rem_euclid(360.0) - a).abs() - 360.0).abs() < 1.0)
        }
    };
    for k in 0..12 {
        let a = k as f32 * 30.0;
        let col = if on(k) { lit } else { dim };
        p.line_segment([at(a - 11.0, 1.0), at(a + 11.0, 1.0)], Stroke::new(if on(k) { 3.5 } else { 2.5 }, col));
    }
    let pts: Vec<Pos2> = offsets.iter().map(|o| at(*o, 0.82)).collect();
    if pts.len() == 2 {
        p.line_segment([pts[0], pts[1]], Stroke::new(1.2, lit));
    } else if pts.len() > 2 {
        let mut sorted = pts.clone();
        sorted.sort_by(|a, b| {
            let ang = |q: &Pos2| (q.x - c.x).atan2(-(q.y - c.y));
            ang(a).total_cmp(&ang(b))
        });
        // Analogous colours sit side by side: no closed outline for them.
        let spread = offsets.iter().cloned().fold(f32::MIN, f32::max) - offsets.iter().cloned().fold(f32::MAX, f32::min);
        if spread > 100.0 {
            p.add(egui::Shape::closed_line(sorted, Stroke::new(1.2, lit)));
        }
    }
}

/// The RGB disc (hue around, saturation out, at `brightness`) or the RYB ring, in `r`.
fn paint_wheel(ui: &Ui, r: Rect, g: &Gear) {
    let c = r.center();
    let rad = r.width() / 2.0;
    let mut mesh = egui::Mesh::default();
    const N: usize = 120;
    match g.wheel {
        Wheel::Rgb => {
            mesh.colored_vertex(c, c32(hsv(0.0, 0.0, g.brightness)));
            for i in 0..=N {
                let h = i as f32 * 360.0 / N as f32;
                let a = h.to_radians();
                mesh.colored_vertex(c + vec2(a.cos(), -a.sin()) * rad, c32(hsv(h, 1.0, g.brightness)));
            }
            for i in 1..=N as u32 {
                mesh.add_triangle(0, i, i + 1);
            }
        }
        Wheel::Ryb => {
            for i in 0..=N {
                let a = i as f32 * 360.0 / N as f32;
                let col = c32(lighten(ryb(a), g.light));
                let d = vec2(a.to_radians().sin(), -a.to_radians().cos());
                mesh.colored_vertex(c + d * rad, col);
                mesh.colored_vertex(c + d * rad * RING_IN, col);
            }
            for i in 0..N as u32 {
                let k = i * 2;
                mesh.add_triangle(k, k + 1, k + 2);
                mesh.add_triangle(k + 1, k + 3, k + 2);
            }
        }
        Wheel::Image => {}
    }
    ui.painter().add(egui::Shape::mesh(mesh));
}

/// A horizontal slider with a painted track: returns the new value (0 to 1) while dragged.
fn track(ui: &mut Ui, value: f32, left: Color32, right: Color32, width: f32) -> Option<f32> {
    let t = Tokens::get(ui.ctx());
    let (r, resp) = ui.allocate_exact_size(vec2(width, 22.0), Sense::click_and_drag());
    let bar = Rect::from_center_size(r.center(), vec2(r.width() - 16.0, 4.0));
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(bar.left_top(), left);
    mesh.colored_vertex(bar.right_top(), right);
    mesh.colored_vertex(bar.right_bottom(), right);
    mesh.colored_vertex(bar.left_bottom(), left);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    ui.painter().add(egui::Shape::mesh(mesh));
    let x = bar.left() + bar.width() * value.clamp(0.0, 1.0);
    ui.painter().circle(pos2(x, bar.center().y), 7.0, Color32::WHITE, Stroke::new(1.0, t.border));
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        return Some(((p.x - bar.left()) / bar.width()).clamp(0.0, 1.0));
    }
    None
}

fn set(app: &mut VectorcraftApp, p: Value) {
    if let Err(e) = app.run("colorGear.set", p) {
        app.ui.status = e;
    }
}

pub fn show(app: &mut VectorcraftApp, ui: &mut Ui) {
    let t = Tokens::get(ui.ctx());
    let width = ui.available_width().max(200.0);
    // Wheels, and Palettes on the right: the saved palettes, each shown as a palette bar.
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        for w in Wheel::ALL {
            let on = app.ui.color_gear.wheel == w;
            let text = egui::RichText::new(tl!(w.label())).size(12.0).color(if on { t.text_strong } else { t.text_dim });
            let fill = if on { t.panel_darker } else { t.input };
            if ui.add(egui::Button::new(text).fill(fill).min_size(vec2(((width - 80.0) / 3.0).max(48.0), 24.0))).clicked() {
                set(app, json!({ "wheel": w.id() }));
            }
        }
        ui.add_space(6.0);
        let resp = widgets::icon_button(ui, "palette", "Palettes", false, 24.0);
        egui::Popup::menu(&resp).show(|ui| {
            let libs: Vec<_> = vectorcraft_engine::cmd::swatchlib::libraries(&app.session).into_iter().filter(|l| l.category == "user").collect();
            if libs.is_empty() {
                ui.label(egui::RichText::new(tl!("No saved palettes yet")).color(t.text_dim));
            }
            for l in libs {
                if widgets::menu_item_name(ui, &l.name, true, false) {
                    app.run("window.colorPalette", json!({ "library": l.id, "show": true })).ok();
                    ui.close();
                }
            }
        });
    });
    ui.add_space(6.0);
    let gear = app.ui.color_gear.clone();
    if gear.wheel != Wheel::Image {
        // The harmonies, side by side (they scroll sideways in a narrow panel).
        egui::ScrollArea::horizontal().id_salt("color-gear-harmonies").show(ui, |ui| {
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 4.0;
                for (k, (id, label, offsets)) in HARMONIES.iter().enumerate() {
                    let (r, resp) = ui.allocate_exact_size(vec2(34.0, 34.0), Sense::click());
                    let on = gear.harmony == k;
                    if on {
                        ui.painter().circle_stroke(r.center(), 16.0, Stroke::new(1.5, t.text_strong));
                    }
                    let lit = if on || resp.hovered() { t.text_strong } else { t.text_dim };
                    harmony_icon(ui, r.shrink(4.0), offsets, lit, t.divider);
                    if resp.on_hover_text(tl!(*label)).clicked() {
                        set(app, json!({ "harmony": id }));
                    }
                }
            });
        });
        ui.add_space(4.0);
        wheel(app, ui, width);
    } else {
        // Image: a box to click for a picture.
        let (r, resp) = ui.allocate_exact_size(vec2(width, 160.0), Sense::click());
        ui.painter().rect_filled(r, 8.0, t.panel_darker);
        ui.painter().rect_stroke(r, 8.0, Stroke::new(1.0, if resp.hovered() { t.accent } else { t.border }), egui::StrokeKind::Inside);
        crate::icons::paint(ui, "image", Rect::from_center_size(r.center() - vec2(0.0, 14.0), vec2(22.0, 22.0)), t.text);
        ui.painter().text(
            r.center() + vec2(0.0, 14.0),
            egui::Align2::CENTER_CENTER,
            tl!("Click to choose a picture"),
            egui::FontId::proportional(12.0),
            t.text,
        );
        if resp.clicked()
            && let Err(e) = app.run("colorGear.image", json!({}))
        {
            app.ui.status = e;
        }
    }
    ui.add_space(8.0);
    swatches(app, ui, width);
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        let w = (width - 12.0) / 2.0;
        let edit = ui.add(egui::Button::new(tl!("Edit")).min_size(vec2(w, 30.0)).corner_radius(15.0).stroke(Stroke::new(1.2, t.accent)));
        egui::Popup::menu(&edit).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| edit_base(app, ui));
        if ui
            .add(
                egui::Button::new(crate::i18n::tr_ctx(crate::i18n::current(), "colorGear", "Save"))
                    .min_size(vec2(w, 30.0))
                    .corner_radius(15.0)
                    .stroke(Stroke::new(1.2, t.accent)),
            )
            .clicked()
        {
            match app.run("colorGear.save", json!({})) {
                Ok(r) => app.ui.status = format!("{} {}", tl!("Saved palette"), r["name"].as_str().unwrap_or_default()),
                Err(e) => app.ui.status = e,
            }
        }
    });
    // Vetorial: scroll down for the colour picker, history, palette from image and palettes.
    super::color_gear_more::show(app, ui, width);
}

/// The wheel with its points, ⇄ and the eyedropper, and its slider.
fn wheel(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let gear = app.ui.color_gear.clone();
    let side = width.min(280.0);
    let (outer, resp) = ui.allocate_exact_size(vec2(width, side), Sense::click_and_drag());
    let r = Rect::from_center_size(outer.center(), vec2(side - 24.0, side - 24.0));
    paint_wheel(ui, r, &gear);
    let rad = r.width() / 2.0;
    let pts: Vec<Pos2> = gear.points().into_iter().map(|v| r.center() + v * rad).collect();
    let colors = gear.colors();
    // The point being dragged, from the press on.
    let drag_id = ui.id().with("color-gear-drag");
    let rel = |p: Pos2| (p - r.center()) / rad;
    if resp.drag_started()
        && let Some(p) = resp.interact_pointer_pos()
    {
        let near = pts.iter().enumerate().map(|(i, q)| (i, q.distance(p))).min_by(|a, b| a.1.total_cmp(&b.1));
        let i = near.filter(|n| n.1 < 16.0).map_or(0, |n| n.0);
        ui.data_mut(|d| d.insert_temp(drag_id, i));
    }
    if (resp.dragged() || resp.clicked())
        && let Some(p) = resp.interact_pointer_pos()
    {
        let i: usize = ui.data(|d| d.get_temp(drag_id)).filter(|_| resp.dragged()).unwrap_or(0);
        let mut g = gear.clone();
        g.drag(i, rel(p));
        set(app, json!({ "hue": g.hue, "saturation": g.saturation }));
    }
    for (i, q) in pts.iter().enumerate().rev() {
        let col = colors.get(if gear.shades() && gear.wheel == Wheel::Ryb { 0 } else { i }).copied().map_or(Color32::GRAY, c32);
        if i == 0 {
            // The base: a white ring, the wheel showing through.
            ui.painter().circle_stroke(*q, 11.0, Stroke::new(2.5, Color32::WHITE));
            ui.painter().circle_stroke(*q, 13.0, Stroke::new(1.0, Color32::from_black_alpha(120)));
        } else {
            // The others: a darker dot of their colour, as in a shadow.
            let d = |v: u8| (f32::from(v) * 0.72) as u8;
            ui.painter().circle(*q, 9.0, Color32::from_rgb(d(col.r()), d(col.g()), d(col.b())), Stroke::new(1.0, Color32::from_black_alpha(160)));
        }
    }
    // ⇄ mirrors the harmony; the eyedropper takes the current colour.
    let swap = Rect::from_min_size(outer.left_top(), vec2(24.0, 24.0));
    let pick = Rect::from_min_size(outer.right_top() - vec2(24.0, 0.0), vec2(24.0, 24.0));
    let swap_resp = widgets::paint_icon_button(
        ui,
        ui.interact(swap, ui.id().with("cg-swap"), Sense::click()),
        "arrow-left-right",
        "Mirror the harmony",
        gear.mirror,
    );
    if swap_resp.clicked() {
        set(app, json!({ "mirror": !gear.mirror }));
    }
    let pick_resp =
        widgets::paint_icon_button(ui, ui.interact(pick, ui.id().with("cg-pick"), Sense::click()), "pipette", "Take the current colour", false);
    if pick_resp.clicked()
        && let Err(e) = app.run("colorGear.pick", json!({}))
    {
        app.ui.status = e;
    }
    ui.add_space(4.0);
    match gear.wheel {
        Wheel::Rgb => {
            let full = c32(hsv(gear.hue, gear.saturation, 1.0));
            if let Some(v) = track(ui, gear.brightness, Color32::BLACK, full, width) {
                set(app, json!({ "brightness": v }));
            }
        }
        Wheel::Ryb => {
            if let Some(v) = track(ui, (gear.light + 1.0) / 2.0, Color32::from_gray(30), Color32::WHITE, width) {
                set(app, json!({ "light": v * 2.0 - 1.0 }));
            }
        }
        Wheel::Image => {}
    }
}

/// The harmony's colours with their values under them: a click paints the fill, a right click
/// the stroke.
fn swatches(app: &mut VectorcraftApp, ui: &mut Ui, width: f32) {
    let t = Tokens::get(ui.ctx());
    let colors = app.ui.color_gear.colors();
    if colors.is_empty() {
        return;
    }
    let n = colors.len() as f32;
    let cell = ((width - 6.0 * (n - 1.0)) / n).min(44.0);
    ui.horizontal(|ui| {
        ui.add_space(((width - (cell * n + 6.0 * (n - 1.0))) / 2.0).max(0.0));
        ui.spacing_mut().item_spacing.x = 6.0;
        for (i, c) in colors.iter().enumerate() {
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 2.0;
                let (r, resp) = ui.allocate_exact_size(vec2(cell, cell), Sense::click());
                ui.painter().rect_filled(r, 8.0, c32(*c));
                if resp.hovered() {
                    ui.painter().rect_stroke(r, 8.0, Stroke::new(1.5, t.text_strong), egui::StrokeKind::Inside);
                }
                let label = hex(*c);
                let (lr, _) = ui.allocate_exact_size(vec2(cell, 12.0), Sense::hover());
                ui.painter().text(lr.center(), egui::Align2::CENTER_CENTER, &label, egui::FontId::proportional(8.5), t.text_dim);
                let resp = resp.on_hover_text(format!("{label}\n{}", tl!("Click: fill · Right click: stroke")));
                let stroke = if resp.clicked() {
                    Some(false)
                } else if resp.secondary_clicked() {
                    Some(true)
                } else {
                    None
                };
                if let Some(stroke) = stroke
                    && app.session.active().is_some()
                    && let Err(e) = app.run("colorGear.apply", json!({ "index": i, "stroke": stroke }))
                {
                    app.ui.status = e;
                }
            });
        }
    });
}

/// Edit: the base colour by value (a picker and its hex code).
fn edit_base(app: &mut VectorcraftApp, ui: &mut Ui) {
    let Some(base) = app.ui.color_gear.colors().first().copied() else {
        ui.label(tl!("Choose a picture first"));
        return;
    };
    if app.ui.color_gear.wheel == Wheel::Image {
        ui.label(tl!("The picture's colours can't be edited; choose RGB or RYB"));
        return;
    }
    let mut c = c32(base);
    if egui::color_picker::color_picker_color32(ui, &mut c, egui::color_picker::Alpha::Opaque) {
        set(app, json!({ "color": format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()) }));
    }
    ui.horizontal(|ui| {
        ui.label("#");
        let id = ui.id().with("cg-hex");
        let mut text: String = ui.data(|d| d.get_temp(id)).unwrap_or_else(|| hex(base).trim_start_matches('#').to_string());
        let resp = ui.add(egui::TextEdit::singleline(&mut text).desired_width(80.0));
        if resp.changed() {
            if parse_hex(&text).is_some() {
                set(app, json!({ "color": format!("#{}", text.trim().trim_start_matches('#')) }));
            }
            ui.data_mut(|d| d.insert_temp(id, text));
        } else if !resp.has_focus() {
            ui.data_mut(|d| d.remove::<String>(id));
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app() -> VectorcraftApp {
        VectorcraftApp::new(vectorcraft_engine::Session::new(), Default::default())
    }

    #[test]
    fn harmonies_give_their_colours() {
        let mut a = app();
        let r = a.run("colorGear.set", json!({"wheel": "rgb", "harmony": "complementary", "color": "#ff0000"})).unwrap();
        assert_eq!(r["colors"], json!(["#FF0000", "#00FFFF"]));
        let r = a.run("colorGear.set", json!({"harmony": "triad"})).unwrap();
        assert_eq!(r["colors"], json!(["#FF0000", "#00FF00", "#0000FF"]));
        let r = a.run("colorGear.set", json!({"harmony": "analogous2", "mirror": true})).unwrap();
        assert_eq!(r["colors"], json!(["#FF0000", "#FF8000"]), "mirrored: the other side");
        let r = a.run("colorGear.set", json!({"harmony": "shades", "mirror": false})).unwrap();
        let cs = r["colors"].as_array().unwrap();
        assert_eq!(cs.len(), 6);
        assert_eq!(cs[0], "#FF0000");
        assert!(cs[5].as_str().unwrap() > "#FF0000", "towards white: {cs:?}");
        let r = a.run("colorGear.set", json!({"brightness": 0.5})).unwrap();
        assert_eq!(r["colors"][0], "#800000");
        assert_eq!(a.run("colorGear.set", json!({"harmony": "hexagon"})).unwrap()["colors"].as_array().unwrap().len(), 6);
        assert!(a.run("colorGear.set", json!({"harmony": "zigzag"})).is_err());
        assert!(a.run("colorGear.set", json!({"saturation": 2})).is_err());
        assert!(a.run("colorGear.set", json!({"wheel": "cmyk"})).is_err());
    }

    #[test]
    fn the_ryb_wheel_and_its_slider() {
        let mut a = app();
        let r = a.run("colorGear.set", json!({"wheel": "ryb", "harmony": "complementary", "hue": 0})).unwrap();
        // Yellow's complement on the painter's wheel is violet.
        assert_eq!(r["colors"][0], "#F2E200");
        assert_eq!(r["colors"][1], "#5800EB");
        let dark = a.run("colorGear.set", json!({"light": -1})).unwrap();
        let light = a.run("colorGear.set", json!({"light": 1})).unwrap();
        assert!(dark["colors"][0].as_str().unwrap() < "#F2E200" && light["colors"][0].as_str().unwrap() > "#F2E200");
    }

    #[test]
    fn dragging_a_point_turns_the_harmony() {
        let mut g = Gear { harmony: 0, ..Default::default() };
        // The complement dragged to the right (red's place) puts the base on cyan.
        g.drag(1, vec2(0.5, 0.0));
        assert!((g.hue - 180.0).abs() < 0.01 && (g.saturation - 0.5).abs() < 0.01, "{g:?}");
        g.wheel = Wheel::Ryb;
        g.drag(0, vec2(0.0, -0.8));
        assert!(g.hue.abs() < 0.01, "top of the ring is yellow: {g:?}");
    }

    #[test]
    fn colours_paint_the_selection_and_save_as_a_palette() {
        let mut a = app();
        a.run("file.new", json!({"width": 100, "height": 100})).unwrap();
        a.run("shape.rectangle", json!({"x": 0, "y": 0, "width": 10, "height": 10})).unwrap();
        a.run("colorGear.set", json!({"wheel": "rgb", "harmony": "complementary", "color": "#ff0000"})).unwrap();
        a.run("colorGear.apply", json!({"index": 1})).unwrap();
        let fill = super::super::active_paint(&a).color().unwrap().to_rgba8(1.0);
        assert_eq!(&fill[..3], &[0, 255, 255]);
        assert!(a.run("colorGear.apply", json!({"index": 9})).is_err());
        // The eyedropper takes it back as the base.
        a.run("colorGear.set", json!({"color": "#123456"})).unwrap();
        assert_eq!(a.run("colorGear.pick", json!({})).unwrap()["colors"][0], "#00FFFF");
    }

    #[test]
    fn pictures_give_their_main_colours() {
        let mut img = image::RgbaImage::new(20, 10);
        for (x, _, p) in img.enumerate_pixels_mut() {
            *p = if x < 15 { image::Rgba([200, 30, 30, 255]) } else { image::Rgba([20, 40, 220, 255]) };
        }
        let mut png = vec![];
        image::DynamicImage::ImageRgba8(img).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let mut a = app();
        let r = a.run("colorGear.image", json!({"dataBase64": vectorcraft_format::base64_encode(&png)})).unwrap();
        assert_eq!(r["wheel"], "image");
        assert_eq!(r["colors"][0], "#C81E1E", "the most common first: {r}");
        assert!(r["colors"].as_array().unwrap().iter().any(|c| c == "#1428DC"), "{r}");
        assert!(a.run("colorGear.image", json!({"dataBase64": vectorcraft_format::base64_encode(b"nope")})).is_err());
    }

    #[test]
    fn the_panel_draws() {
        let mut a = app();
        let ctx = egui::Context::default();
        crate::theme::install_fonts(&ctx);
        for w in ["rgb", "ryb", "image"] {
            a.ui.color_gear.wheel = Wheel::ALL.into_iter().find(|x| x.id() == w).unwrap();
            ctx.run_ui(Default::default(), |ui| show(&mut a, ui)).textures_delta.clear();
        }
    }
}
