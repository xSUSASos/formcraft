//! Binary STL.

use std::io::Write;
use std::path::Path;

use formcraft_kernel::Mesh;

use crate::{IoError, Result};

/// Bytes of a binary STL for `mesh` (80-byte header, count, 50 bytes per triangle).
pub fn to_bytes(mesh: &Mesh) -> Result<Vec<u8>> {
    let tris = mesh.triangle_count();
    let count = u32::try_from(tris).map_err(|_| IoError::Format("too many triangles for STL".into()))?;
    let mut out = Vec::with_capacity(84 + 50 * tris);
    let mut header = [0u8; 80];
    let tag = b"FormCraft binary STL";
    header[..tag.len()].copy_from_slice(tag);
    out.extend_from_slice(&header);
    out.extend_from_slice(&count.to_le_bytes());
    let p = |i: u32| mesh.positions.get(i as usize).copied().unwrap_or([0.0; 3]);
    for t in mesh.indices.as_chunks::<3>().0 {
        let (a, b, c) = (p(t[0]), p(t[1]), p(t[2]));
        let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
        let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
        let n = [u[1] * v[2] - u[2] * v[1], u[2] * v[0] - u[0] * v[2], u[0] * v[1] - u[1] * v[0]];
        let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
        let n = if len > 0.0 { [n[0] / len, n[1] / len, n[2] / len] } else { [0.0; 3] };
        for v in [n, a, b, c] {
            for f in v {
                out.extend_from_slice(&f.to_le_bytes());
            }
        }
        out.extend_from_slice(&[0, 0]);
    }
    Ok(out)
}

pub fn write(mesh: &Mesh, path: &Path) -> Result<()> {
    let bytes = to_bytes(mesh)?;
    let mut f = std::fs::File::create(path)?;
    f.write_all(&bytes)?;
    Ok(())
}

fn f32_at(rec: &[u8], k: usize) -> f32 {
    rec.get(k * 4..k * 4 + 4).and_then(|s| <[u8; 4]>::try_from(s).ok()).map(f32::from_le_bytes).unwrap_or(0.0)
}

/// Parse a binary STL into a mesh (tests and import checks). Hostile input is rejected.
pub fn from_bytes(b: &[u8]) -> Result<Mesh> {
    let count = b
        .get(80..84)
        .and_then(|s| <[u8; 4]>::try_from(s).ok())
        .map(u32::from_le_bytes)
        .ok_or_else(|| IoError::Format("STL too short".into()))?;
    let need = (count as usize).checked_mul(50).and_then(|n| n.checked_add(84));
    if need != Some(b.len()) {
        return Err(IoError::Format("STL size does not match its triangle count".into()));
    }
    let mut m = Mesh::default();
    for i in 0..count as usize {
        let rec = b.get(84 + i * 50..84 + i * 50 + 50).ok_or_else(|| IoError::Format("truncated STL".into()))?;
        for v in 0..3 {
            let base = 3 + v * 3;
            m.positions.push([f32_at(rec, base), f32_at(rec, base + 1), f32_at(rec, base + 2)]);
            m.indices.push((i * 3 + v) as u32);
        }
        m.triangle_face.push(0);
    }
    Ok(m)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trip_keeps_volume() {
        let positions = vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let indices = vec![0, 2, 1, 0, 1, 3, 0, 3, 2, 1, 2, 3];
        let m = Mesh { positions, indices, triangle_face: vec![0; 4], ..Default::default() };
        let b = to_bytes(&m).unwrap();
        assert_eq!(b.len(), 84 + 4 * 50);
        let back = from_bytes(&b).unwrap();
        assert!((back.volume() - m.volume()).abs() < 1e-6);
        assert!((m.volume() - 1.0 / 6.0).abs() < 1e-6);
    }

    #[test]
    fn hostile_stl_is_rejected() {
        assert!(from_bytes(&[0; 10]).is_err());
        let mut b = vec![0u8; 84];
        b[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(from_bytes(&b).is_err());
    }
}
