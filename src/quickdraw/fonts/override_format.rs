//! Stable on-disk schema for runtime font overrides.
//!
//! `systemless` ships with its own original bitmap glyphs in the
//! `pixel_font` modules. For higher-fidelity rendering an opt-in runtime override
//! path lets a host substitute authentic Mac bitmap fonts (Chicago,
//! Geneva, Monaco, …) without committing Apple-copyrighted data into
//! this repo. The override directory is one `.bin` per
//! `(font_id, size, style)` tuple in the format described below; the
//! consumer is [`load_directory`], called once at fonts/mod.rs
//! `LazyLock` init when `SYSTEMLESS_ORIGINAL_FONTS_DIR` points at the
//! directory.
//!
//! The schema is deliberately minimal: little-endian, `#[repr(C)]`, no
//! `serde`, no compression. Bumping `VERSION` is a hard reset.
//!
//! # Layout
//!
//! ```text
//! [BlobHeader]                              48 bytes
//! [GlyphEntry; glyph_count]                 12 bytes each
//! [u8; data_len]                            coverage bytes (0 or 255)
//! ```
//!
//! Glyph order starts with the ASCII printable 0x20..=0x7E entries, then
//! appends classic menu symbols that live below ASCII in the
//! System font: Command key (Mac Roman 0x11), checkmark (0x12), and Apple mark
//! (0x14). Missing glyphs are encoded as zero-sized entries with a positive
//! advance for spaces or `0` advance for "not in font".

use std::collections::HashMap;
use std::fs;
use std::io::{self, Read, Write};
use std::path::Path;

use super::{FontFace, FontMetrics, Glyph};

const MAGIC: &[u8; 4] = b"KFOV";
const VERSION: u16 = 1;
const HEADER_BYTES: usize = 48;
const GLYPH_ENTRY_BYTES: usize = 12;
pub const ASCII_GLYPH_COUNT: u16 = 95;
pub const COMMAND_SYMBOL_GLYPH_INDEX: usize = ASCII_GLYPH_COUNT as usize;
pub const CHECKMARK_SYMBOL_GLYPH_INDEX: usize = COMMAND_SYMBOL_GLYPH_INDEX + 1;
pub const APPLE_SYMBOL_GLYPH_INDEX: usize = CHECKMARK_SYMBOL_GLYPH_INDEX + 1;
pub const GLYPH_COUNT: u16 = ASCII_GLYPH_COUNT + 3;

pub const STYLE_PLAIN: u8 = 0;
pub const STYLE_BOLD: u8 = 1;
pub const STYLE_ITALIC: u8 = 2;
pub const STYLE_BOLD_ITALIC: u8 = 3;

pub struct Blob {
    pub font_id: i16,
    pub size: i16,
    pub style: u8,
    pub metrics: FontMetrics,
    pub glyphs: Vec<Glyph>,
    pub data: Vec<u8>,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    BadMagic,
    UnsupportedVersion(u16),
    Truncated { expected: usize, actual: usize },
    DataLenMismatch { header: usize, file: usize },
}

impl From<io::Error> for LoadError {
    fn from(e: io::Error) -> Self {
        LoadError::Io(e)
    }
}

impl std::fmt::Display for LoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LoadError::Io(e) => write!(f, "io: {e}"),
            LoadError::BadMagic => write!(f, "bad magic (not a KFOV blob)"),
            LoadError::UnsupportedVersion(v) => write!(f, "unsupported version {v}"),
            LoadError::Truncated { expected, actual } => {
                write!(f, "truncated: expected {expected} bytes, got {actual}")
            }
            LoadError::DataLenMismatch { header, file } => {
                write!(
                    f,
                    "data_len {header} disagrees with file size remainder {file}"
                )
            }
        }
    }
}

fn read_u16(buf: &[u8], off: usize) -> u16 {
    u16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_i16(buf: &[u8], off: usize) -> i16 {
    i16::from_le_bytes([buf[off], buf[off + 1]])
}

fn read_u32(buf: &[u8], off: usize) -> u32 {
    u32::from_le_bytes([buf[off], buf[off + 1], buf[off + 2], buf[off + 3]])
}

pub fn read_blob(bytes: &[u8]) -> Result<Blob, LoadError> {
    if bytes.len() < HEADER_BYTES {
        return Err(LoadError::Truncated {
            expected: HEADER_BYTES,
            actual: bytes.len(),
        });
    }
    if &bytes[0..4] != MAGIC {
        return Err(LoadError::BadMagic);
    }
    let version = read_u16(bytes, 4);
    if version != VERSION {
        return Err(LoadError::UnsupportedVersion(version));
    }
    let font_id = read_i16(bytes, 6);
    let size = read_i16(bytes, 8);
    let style = bytes[10];
    // bytes[11] is padding
    let glyph_count = read_u16(bytes, 12) as usize;
    let ascent = read_i16(bytes, 14);
    let descent = read_i16(bytes, 16);
    let wid_max = read_i16(bytes, 18);
    let leading = read_i16(bytes, 20);
    let data_len = read_u32(bytes, 24) as usize;
    // bytes[28..48] reserved zero — currently unused, future-proof padding.

    let entries_off = HEADER_BYTES;
    let entries_end = entries_off + glyph_count * GLYPH_ENTRY_BYTES;
    let data_end = entries_end + data_len;
    if bytes.len() < data_end {
        return Err(LoadError::Truncated {
            expected: data_end,
            actual: bytes.len(),
        });
    }
    if bytes.len() != data_end {
        return Err(LoadError::DataLenMismatch {
            header: data_len,
            file: bytes.len() - entries_end,
        });
    }

    let mut glyphs = Vec::with_capacity(glyph_count);
    for i in 0..glyph_count {
        let o = entries_off + i * GLYPH_ENTRY_BYTES;
        let width = bytes[o];
        let height = bytes[o + 1];
        let advance = bytes[o + 2];
        let origin_x = bytes[o + 3] as i8;
        let origin_y = bytes[o + 4] as i8;
        // bytes[o+5..o+8] padding to align u32
        let data_offset = read_u32(bytes, o + 8) as usize;
        glyphs.push(Glyph {
            width,
            height,
            advance,
            origin_x,
            origin_y,
            data_offset,
        });
    }

    let data = bytes[entries_end..data_end].to_vec();

    Ok(Blob {
        font_id,
        size,
        style,
        metrics: FontMetrics {
            ascent,
            descent,
            wid_max,
            leading,
        },
        glyphs,
        data,
    })
}

pub fn write_blob<W: Write>(w: &mut W, blob: &Blob) -> io::Result<()> {
    let mut header = [0u8; HEADER_BYTES];
    header[0..4].copy_from_slice(MAGIC);
    header[4..6].copy_from_slice(&VERSION.to_le_bytes());
    header[6..8].copy_from_slice(&blob.font_id.to_le_bytes());
    header[8..10].copy_from_slice(&blob.size.to_le_bytes());
    header[10] = blob.style;
    // header[11] padding
    let glyph_count = blob.glyphs.len() as u16;
    header[12..14].copy_from_slice(&glyph_count.to_le_bytes());
    header[14..16].copy_from_slice(&blob.metrics.ascent.to_le_bytes());
    header[16..18].copy_from_slice(&blob.metrics.descent.to_le_bytes());
    header[18..20].copy_from_slice(&blob.metrics.wid_max.to_le_bytes());
    header[20..22].copy_from_slice(&blob.metrics.leading.to_le_bytes());
    let data_len = blob.data.len() as u32;
    header[24..28].copy_from_slice(&data_len.to_le_bytes());
    w.write_all(&header)?;

    for g in &blob.glyphs {
        let mut entry = [0u8; GLYPH_ENTRY_BYTES];
        entry[0] = g.width;
        entry[1] = g.height;
        entry[2] = g.advance;
        entry[3] = g.origin_x as u8;
        entry[4] = g.origin_y as u8;
        // entry[5..8] padding
        entry[8..12].copy_from_slice(&(g.data_offset as u32).to_le_bytes());
        w.write_all(&entry)?;
    }

    w.write_all(&blob.data)?;
    Ok(())
}

/// Read every `*.bin` blob in `dir`, decode, and convert to leak-allocated
/// [`FontFace`]s keyed on `(font_id, size)`. Style != [`STYLE_PLAIN`] entries
/// are skipped (italic synthesis still goes through `ITALIC_TABLE`); they
/// remain reserved for a future bump that wires styled blobs directly.
///
/// Returns an empty map if the directory does not exist; per-file errors
/// are logged via `eprintln!` and the offending file is skipped — one
/// bad blob does not poison the rest.
pub fn load_directory(dir: &Path) -> HashMap<(i16, i16), &'static FontFace> {
    let mut out = HashMap::new();
    let read = match fs::read_dir(dir) {
        Ok(rd) => rd,
        Err(_) => return out,
    };
    for entry in read.flatten() {
        let path = entry.path();
        if path.extension().map(|e| e != "bin").unwrap_or(true) {
            continue;
        }
        let mut bytes = Vec::new();
        if let Err(e) = fs::File::open(&path).and_then(|mut f| f.read_to_end(&mut bytes)) {
            eprintln!("font override: failed to read {}: {e}", path.display());
            continue;
        }
        let blob = match read_blob(&bytes) {
            Ok(b) => b,
            Err(e) => {
                eprintln!("font override: skip {}: {e}", path.display());
                continue;
            }
        };
        if blob.style != STYLE_PLAIN {
            continue;
        }
        let face = FontFace {
            font_id: blob.font_id,
            size: blob.size,
            metrics: blob.metrics,
            glyphs: Box::leak(blob.glyphs.into_boxed_slice()),
            data: Box::leak(blob.data.into_boxed_slice()),
        };
        let leaked: &'static FontFace = Box::leak(Box::new(face));
        out.insert((leaked.font_id, leaked.size), leaked);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_blob() -> Blob {
        Blob {
            font_id: 4,
            size: 9,
            style: STYLE_PLAIN,
            metrics: FontMetrics {
                ascent: 7,
                descent: 2,
                wid_max: 6,
                leading: 1,
            },
            glyphs: vec![
                Glyph {
                    width: 0,
                    height: 0,
                    advance: 5,
                    origin_x: 0,
                    origin_y: 0,
                    data_offset: 0,
                },
                Glyph {
                    width: 1,
                    height: 1,
                    advance: 4,
                    origin_x: 0,
                    origin_y: -7,
                    data_offset: 0,
                },
            ],
            data: vec![255],
        }
    }

    #[test]
    fn roundtrip_blob() {
        let blob = sample_blob();
        let mut buf = Vec::new();
        write_blob(&mut buf, &blob).unwrap();
        let back = read_blob(&buf).unwrap();
        assert_eq!(back.font_id, blob.font_id);
        assert_eq!(back.size, blob.size);
        assert_eq!(back.style, blob.style);
        assert_eq!(back.metrics.ascent, blob.metrics.ascent);
        assert_eq!(back.metrics.descent, blob.metrics.descent);
        assert_eq!(back.metrics.wid_max, blob.metrics.wid_max);
        assert_eq!(back.metrics.leading, blob.metrics.leading);
        assert_eq!(back.glyphs.len(), blob.glyphs.len());
        assert_eq!(back.glyphs[1].advance, 4);
        assert_eq!(back.glyphs[1].origin_y, -7);
        assert_eq!(back.data, blob.data);
    }

    #[test]
    fn rejects_bad_magic() {
        let mut buf = Vec::new();
        write_blob(&mut buf, &sample_blob()).unwrap();
        buf[0] = b'X';
        assert!(matches!(read_blob(&buf), Err(LoadError::BadMagic)));
    }

    #[test]
    fn rejects_truncated() {
        let mut buf = Vec::new();
        write_blob(&mut buf, &sample_blob()).unwrap();
        buf.truncate(20);
        assert!(matches!(read_blob(&buf), Err(LoadError::Truncated { .. })));
    }

    #[test]
    fn rejects_wrong_version() {
        let mut buf = Vec::new();
        write_blob(&mut buf, &sample_blob()).unwrap();
        buf[4] = 99;
        assert!(matches!(
            read_blob(&buf),
            Err(LoadError::UnsupportedVersion(99))
        ));
    }

    #[test]
    fn load_directory_missing_returns_empty() {
        let missing = std::env::temp_dir().join("systemless-overrides-does-not-exist-xyzzy");
        let map = load_directory(&missing);
        assert!(map.is_empty());
    }
}
