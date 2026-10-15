//! Native `.fcraft` files: a zip archive with `document.json` (the timeline — the source of
//! truth), `meta.json` (format version, app version) and an optional `preview.png`.

use std::io::{Read, Write};
use std::path::Path;

use formcraft_doc::Document;
use serde_json::json;

use crate::{IoError, Result};

pub const FORMAT_VERSION: u32 = 1;
/// Largest `document.json` accepted (hostile files).
const MAX_JSON: u64 = 64 * 1024 * 1024;
const MAX_PREVIEW: u64 = 16 * 1024 * 1024;

pub fn save(doc: &Document, preview_png: Option<&[u8]>, path: &Path) -> Result<()> {
    let json = doc.to_json().map_err(|e| IoError::Format(e.to_string()))?;
    let tmp = path.with_extension("fcraft.tmp");
    {
        let f = std::fs::File::create(&tmp)?;
        let mut z = zip::ZipWriter::new(f);
        let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        let meta = json!({"format": "formcraft", "version": FORMAT_VERSION, "app": env!("CARGO_PKG_VERSION")});
        z.start_file("meta.json", opts).map_err(zerr)?;
        z.write_all(meta.to_string().as_bytes())?;
        z.start_file("document.json", opts).map_err(zerr)?;
        z.write_all(json.as_bytes())?;
        if let Some(png) = preview_png {
            z.start_file("preview.png", zip::write::SimpleFileOptions::default()).map_err(zerr)?;
            z.write_all(png)?;
        }
        z.finish().map_err(zerr)?;
    }
    // Write-then-rename: a crash while saving never destroys the previous file.
    std::fs::rename(&tmp, path)?;
    Ok(())
}

fn zerr(e: zip::result::ZipError) -> IoError {
    IoError::Format(format!("zip: {e}"))
}

fn read_entry(z: &mut zip::ZipArchive<std::fs::File>, name: &str, limit: u64) -> Result<Option<Vec<u8>>> {
    let entry = match z.by_name(name) {
        Ok(e) => e,
        Err(zip::result::ZipError::FileNotFound) => return Ok(None),
        Err(e) => return Err(zerr(e)),
    };
    if entry.size() > limit {
        return Err(IoError::Format(format!("{name} is too large")));
    }
    let mut buf = Vec::new();
    entry.take(limit + 1).read_to_end(&mut buf)?;
    if buf.len() as u64 > limit {
        return Err(IoError::Format(format!("{name} is too large")));
    }
    Ok(Some(buf))
}

/// Open a `.fcraft` file: the document and the preview (if any).
pub fn open(path: &Path) -> Result<(Document, Option<Vec<u8>>)> {
    let f = std::fs::File::open(path)?;
    let mut z = zip::ZipArchive::new(f).map_err(zerr)?;
    if let Some(meta) = read_entry(&mut z, "meta.json", 64 * 1024)? {
        let v: serde_json::Value =
            serde_json::from_slice(&meta).map_err(|e| IoError::Format(format!("meta.json: {e}")))?;
        let ver = v.get("version").and_then(serde_json::Value::as_u64).unwrap_or(0);
        if ver > FORMAT_VERSION as u64 {
            return Err(IoError::Format(format!("file format {ver} is newer than this FormCraft ({FORMAT_VERSION})")));
        }
    }
    let json = read_entry(&mut z, "document.json", MAX_JSON)?
        .ok_or_else(|| IoError::Format("not a FormCraft file (no document.json)".into()))?;
    let text = String::from_utf8(json).map_err(|_| IoError::Format("document.json is not UTF-8".into()))?;
    let doc = Document::from_json(&text).map_err(|e| IoError::Format(e.to_string()))?;
    let preview = read_entry(&mut z, "preview.png", MAX_PREVIEW)?;
    Ok((doc, preview))
}

#[cfg(test)]
mod tests {
    use formcraft_doc::{BodyOp, FeatureKind, Vec3Expr};

    use super::*;

    #[test]
    fn round_trip_and_garbage() {
        let dir = std::env::temp_dir().join(format!("fc-fcraft-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut d = Document::new("Bracket");
        d.set_parameter("L", "60".into(), Some("length")).unwrap();
        d.add_feature(FeatureKind::Box {
            corner: Vec3Expr::num(0.0, 0.0, 0.0),
            size: Vec3Expr(["L".into(), "40".into(), "5".into()]),
            op: BodyOp::NewBody,
        })
        .unwrap();
        let p = dir.join("b.fcraft");
        save(&d, Some(b"\x89PNG fake"), &p).unwrap();
        let (back, prev) = open(&p).unwrap();
        assert_eq!(back, d);
        assert_eq!(prev.as_deref(), Some(&b"\x89PNG fake"[..]));
        std::fs::write(dir.join("bad.fcraft"), b"not a zip").unwrap();
        assert!(open(&dir.join("bad.fcraft")).is_err());
        assert!(open(&dir.join("missing.fcraft")).is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }
}
