//! The containers: the RIFF tree (with its zlib-compressed `cmpr` lists) and the ZIP archive of
//! CorelDRAW X4 and later, with the external data streams of X6 and later.

use std::io::Read as _;

use crate::{Error, Limits};

/// A record of the RIFF tree.
#[derive(Clone, Debug)]
pub(crate) enum Record {
    /// A `LIST` (or the `RIFF` root) of `kind`.
    List { kind: [u8; 4], children: Vec<Record> },
    /// A data record.
    Leaf { id: [u8; 4], data: Vec<u8> },
}

/// A file opened down to its RIFF tree.
pub(crate) struct File {
    /// The version the RIFF form type states (`vrsn` may refine it).
    pub version: u32,
    /// The root's records.
    pub records: Vec<Record>,
    /// The external data streams (X6 and later: `content/data/*` in `dataFileList.dat` order).
    pub streams: Vec<Vec<u8>>,
    /// Data streams too large to read (embedded bitmaps, usually), left empty.
    pub skipped: Vec<String>,
}

fn u16_at(b: &[u8], at: usize) -> Option<u16> {
    let s = b.get(at..at.checked_add(2)?)?;
    Some(u16::from_le_bytes([s[0], s[1]]))
}

pub(crate) fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    let s = b.get(at..at.checked_add(4)?)?;
    Some(u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
}

/// The version a RIFF form type's last character names (` ` CorelDRAW 3, `5` 5, `A` 10, `E` X4…).
pub(crate) fn version_of(c: u8) -> Option<u32> {
    match c {
        b' ' => Some(300),
        b'1'..=b'9' => Some(100 * u32::from(c - b'0')),
        b'A'..=b'Z' => Some(100 * u32::from(c - 0x37)),
        b'a'..=b'z' => Some(100 * u32::from(c - 0x57)),
        _ => None,
    }
}

/// The version of a RIFF CorelDRAW stream (`None` when it isn't one).
fn riff_version(b: &[u8]) -> Option<u32> {
    if !b.starts_with(b"RIFF") || !b.get(8..11)?.eq_ignore_ascii_case(b"cdr") {
        return None;
    }
    version_of(*b.get(11)?)
}

/// Is this a CorelDRAW file (a RIFF CorelDRAW stream, or a ZIP archive holding one)?
pub fn is_cdr(bytes: &[u8]) -> bool {
    riff_version(bytes).is_some()
        || Zip::new(bytes).is_some_and(|z| z.entries.iter().any(|e| e.name == "content/riffData.cdr" || e.name == "content/root.dat"))
}

// ---------- ZIP ----------

struct Entry {
    name: String,
    method: u16,
    compressed: usize,
    size: usize,
    local: usize,
}

/// A ZIP archive's central directory.
struct Zip<'a> {
    bytes: &'a [u8],
    entries: Vec<Entry>,
}

impl<'a> Zip<'a> {
    fn new(bytes: &'a [u8]) -> Option<Self> {
        if !bytes.starts_with(b"PK\x03\x04") {
            return None;
        }
        // The end record is in the last 64 KiB (+ its own 22 bytes, the most a comment takes).
        let from = bytes.len().saturating_sub(65_557);
        let end = (from..bytes.len().saturating_sub(21)).rev().find(|&i| u32_at(bytes, i) == Some(0x0605_4b50))?;
        let count = usize::from(u16_at(bytes, end + 10)?);
        let mut at = u32_at(bytes, end + 16)? as usize;
        let mut entries = Vec::with_capacity(count.min(4096));
        for _ in 0..count {
            if u32_at(bytes, at)? != 0x0201_4b50 {
                break;
            }
            let name_len = usize::from(u16_at(bytes, at + 28)?);
            let skip = usize::from(u16_at(bytes, at + 30)?) + usize::from(u16_at(bytes, at + 32)?);
            let name = String::from_utf8_lossy(bytes.get(at + 46..at + 46 + name_len)?).replace('\\', "/");
            entries.push(Entry {
                name,
                method: u16_at(bytes, at + 10)?,
                compressed: u32_at(bytes, at + 20)? as usize,
                size: u32_at(bytes, at + 24)? as usize,
                local: u32_at(bytes, at + 42)? as usize,
            });
            at += 46 + name_len + skip;
        }
        Some(Self { bytes, entries })
    }

    fn find(&self, name: &str) -> Option<&Entry> {
        self.entries.iter().find(|e| e.name.eq_ignore_ascii_case(name))
    }

    /// An entry's bytes (stored or deflated), at most `cap`.
    fn extract(&self, e: &Entry, cap: usize) -> Result<Vec<u8>, Error> {
        let damaged = || Error::Damaged(format!("can't read `{}` in the archive", e.name));
        if e.size > cap {
            return Err(Error::TooLarge(format!("`{}` holds {} bytes", e.name, e.size)));
        }
        if u32_at(self.bytes, e.local) != Some(0x0403_4b50) {
            return Err(damaged());
        }
        let skip =
            usize::from(u16_at(self.bytes, e.local + 26).ok_or_else(damaged)?) + usize::from(u16_at(self.bytes, e.local + 28).ok_or_else(damaged)?);
        let start = e.local + 30 + skip;
        let data = self.bytes.get(start..start.saturating_add(e.compressed).min(self.bytes.len())).ok_or_else(damaged)?;
        match e.method {
            0 => Ok(data.get(..e.size.min(data.len())).unwrap_or(data).to_vec()),
            8 => {
                let mut out = Vec::with_capacity(e.size.min(cap));
                flate2::read::DeflateDecoder::new(data).take(cap as u64).read_to_end(&mut out).map_err(|_| damaged())?;
                Ok(out)
            }
            m => Err(Error::Unsupported(format!("`{}` is compressed with method {m}", e.name))),
        }
    }

    fn read(&self, name: &str, cap: usize) -> Option<Result<Vec<u8>, Error>> {
        self.find(name).map(|e| self.extract(e, cap))
    }
}

// ---------- RIFF ----------

/// Inflate zlib (or raw deflate) data → (bytes, input bytes consumed), at most `cap` bytes.
fn inflate(data: &[u8], cap: usize) -> Option<(Vec<u8>, usize)> {
    let limit = (cap as u64).saturating_add(1);
    let mut out = Vec::new();
    let mut z = flate2::bufread::ZlibDecoder::new(data);
    if (&mut z).take(limit).read_to_end(&mut out).is_ok() && !out.is_empty() && out.len() <= cap {
        return Some((out, data.len() - z.get_ref().len()));
    }
    out.clear();
    let mut d = flate2::bufread::DeflateDecoder::new(data);
    if (&mut d).take(limit).read_to_end(&mut out).is_ok() && !out.is_empty() && out.len() <= cap {
        return Some((out, data.len() - d.get_ref().len()));
    }
    None
}

struct Riff {
    limits: Limits,
    count: usize,
}

impl Riff {
    /// The records in `data`. In a compressed list, a length smaller than the block table is an
    /// index into it.
    fn records(&mut self, data: &[u8], blocks: Option<&[u32]>, depth: usize) -> Result<Vec<Record>, Error> {
        if depth > self.limits.max_depth {
            return Err(Error::TooLarge("its records nest too deep".into()));
        }
        let mut out = Vec::new();
        let mut at = 0;
        loop {
            // Padding (and writers' alignment) between records is zero bytes.
            while data.get(at) == Some(&0) {
                at += 1;
            }
            let (Some(id), Some(len)) = (data.get(at..at + 4), u32_at(data, at + 4)) else { break };
            if !id.iter().all(|c| c.is_ascii_graphic() || *c == b' ') {
                break;
            }
            let len = match blocks {
                Some(b) if (len as usize) < b.len() => b.get(len as usize).copied().unwrap_or(len),
                _ => len,
            } as usize;
            let start = at + 8;
            let end = start.saturating_add(len).min(data.len());
            let body = data.get(start..end).unwrap_or(&[]);
            self.count += 1;
            if self.count > self.limits.max_records {
                return Err(Error::TooLarge("too many records".into()));
            }
            let id: [u8; 4] = [id[0], id[1], id[2], id[3]];
            if &id == b"LIST" || &id == b"RIFF" {
                let kind = body.get(..4).map_or([0; 4], |k| [k[0], k[1], k[2], k[3]]);
                let children =
                    if &kind == b"cmpr" { self.cmpr(body, depth)? } else { self.records(body.get(4..).unwrap_or(&[]), blocks, depth + 1)? };
                out.push(Record::List { kind, children });
            } else {
                out.push(Record::Leaf { id, data: body.to_vec() });
            }
            if end <= at {
                break;
            }
            at = end;
        }
        Ok(out)
    }

    /// A `cmpr` list (`body` from its kind on): its records, zlib-compressed, with their lengths in
    /// a compressed block table after them.
    fn cmpr(&mut self, body: &[u8], depth: usize) -> Result<Vec<Record>, Error> {
        let damaged = || Error::Damaged("a compressed list can't be read".into());
        let size = u32_at(body, 4).ok_or_else(damaged)? as usize;
        if body.get(20..24) != Some(b"CPng") || u16_at(body, 24) != Some(1) || u16_at(body, 26) != Some(4) {
            return Err(damaged());
        }
        let packed = body.get(28..).ok_or_else(damaged)?;
        let (data, used) = inflate(packed, self.limits.max_stream).ok_or_else(damaged)?;
        // The block table follows the records' stream.
        let blocks = [used, size, size.saturating_sub(8)].into_iter().find_map(|from| {
            let (t, _) = inflate(packed.get(from..)?, self.limits.max_stream)?;
            (t.len() % 4 == 0 && !t.is_empty()).then(|| t.chunks_exact(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect::<Vec<_>>())
        });
        self.records(&data, Some(blocks.as_deref().unwrap_or(&[])), depth + 1)
    }
}

/// Open a CorelDRAW file down to its RIFF tree.
pub(crate) fn open(bytes: &[u8], limits: Limits) -> Result<File, Error> {
    let mut streams = Vec::new();
    let mut skipped = Vec::new();
    let riff: std::borrow::Cow<[u8]> = if bytes.starts_with(b"RIFF") {
        bytes.into()
    } else if let Some(zip) = Zip::new(bytes) {
        match zip.read("content/riffData.cdr", limits.max_stream) {
            Some(r) => r?.into(),
            None => {
                let root = zip.read("content/root.dat", limits.max_stream).ok_or(Error::NotCdr)??;
                if let Some(list) = zip.read("content/dataFileList.dat", 1 << 20).transpose()? {
                    for name in list.split(|c| *c == b'\n').map(|n| String::from_utf8_lossy(n).trim().to_string()).filter(|n| !n.is_empty()) {
                        // A stream past the limit (the file's bitmaps, which aren't read) is left
                        // empty: the drawing doesn't need it.
                        let s = match zip.read(&format!("content/data/{name}"), limits.max_stream).transpose() {
                            Ok(s) => s.unwrap_or_default(),
                            Err(Error::TooLarge(_)) => {
                                skipped.push(name);
                                vec![]
                            }
                            Err(e) => return Err(e),
                        };
                        streams.push(s);
                        if streams.len() > 65_536 {
                            return Err(Error::TooLarge("too many data streams".into()));
                        }
                    }
                }
                root.into()
            }
        }
    } else if bytes.starts_with(b"WL") {
        return Err(Error::Unsupported("CorelDRAW 1 and 2 files aren't read".into()));
    } else {
        return Err(Error::NotCdr);
    };
    let version = riff_version(&riff).ok_or(Error::NotCdr)?;
    let mut parser = Riff { limits, count: 0 };
    let mut records = parser.records(&riff, None, 0)?;
    let records = match records.pop() {
        Some(Record::List { children, .. }) if records.is_empty() => children,
        _ => return Err(Error::Damaged("no RIFF tree".into())),
    };
    Ok(File { version, records, streams, skipped })
}

/// The preview picture of a CorelDRAW file, as a complete BMP or PNG file.
pub(crate) fn thumbnail(bytes: &[u8], limits: Limits) -> Option<Vec<u8>> {
    if let Some(zip) = Zip::new(bytes) {
        let e = zip.find("metadata/thumbnails/thumbnail.bmp").or_else(|| {
            zip.entries.iter().find(|e| {
                let n = e.name.to_ascii_lowercase();
                n.starts_with("metadata/thumbnails/") && (n.ends_with(".bmp") || n.ends_with(".png"))
            })
        })?;
        return zip.extract(e, 64 << 20).ok();
    }
    // RIFF: a `DISP` record holds a Windows DIB after a 4-byte field.
    riff_version(bytes)?;
    let mut parser = Riff { limits, count: 0 };
    let records = parser.records(bytes, None, 0).ok()?;
    let Some(Record::List { children, .. }) = records.first() else { return None };
    let dib = children.iter().find_map(|r| match r {
        Record::Leaf { id: [b'D', b'I', b'S', b'P'], data } => data.get(4..),
        _ => None,
    })?;
    bmp_from_dib(dib)
}

/// A BMP file from a Windows DIB (a header, its palette, the pixels).
pub(crate) fn bmp_from_dib(dib: &[u8]) -> Option<Vec<u8>> {
    let header = u32_at(dib, 0)? as usize;
    if !(12..=124).contains(&header) {
        return None;
    }
    let (bits, used) = if header == 12 { (u16_at(dib, 10)?, 0) } else { (u16_at(dib, 14)?, u32_at(dib, 32)? as usize) };
    let entry = if header == 12 { 3 } else { 4 };
    let palette = match (used, bits) {
        (0, 1 | 4 | 8) => 1usize << bits,
        (n, _) if n <= 256 => n,
        _ => return None,
    };
    let offset = 14 + header + palette * entry;
    let size = u32::try_from(14 + dib.len()).ok()?;
    let mut out = Vec::with_capacity(14 + dib.len());
    out.extend_from_slice(b"BM");
    out.extend_from_slice(&size.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&u32::try_from(offset).ok()?.to_le_bytes());
    out.extend_from_slice(dib);
    Some(out)
}
