//! File formats: STEP (through the kernel), binary STL, 3MF and `.fcraft`.
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod fcraft;
pub mod stl;
pub mod threemf;

use std::path::Path;

use formcraft_kernel::{Kernel, Shape};

#[derive(Debug, thiserror::Error)]
pub enum IoError {
    #[error("{0}")]
    Kernel(#[from] formcraft_kernel::KernelError),
    #[error("{0}")]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Format(String),
}

pub type Result<T> = std::result::Result<T, IoError>;

/// Export formats chosen by file extension.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Step,
    Stl,
    ThreeMf,
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "step" | "stp" => Some(Format::Step),
            "stl" => Some(Format::Stl),
            "3mf" => Some(Format::ThreeMf),
            _ => None,
        }
    }
}

/// Write bodies to a file; the format follows the extension.
pub fn export(kernel: &dyn Kernel, bodies: &[Shape], path: &Path) -> Result<()> {
    if bodies.is_empty() {
        return Err(IoError::Format("nothing to export: the design has no visible bodies".into()));
    }
    match Format::from_path(path) {
        Some(Format::Step) => {
            let shape = if let [one] = bodies { one.clone() } else { kernel.compound(bodies)? };
            kernel.write_step(&shape, path)?;
        }
        Some(Format::Stl) => {
            let mut mesh = formcraft_kernel::Mesh::default();
            for b in bodies {
                mesh.append(&formcraft_tess::mesh(kernel, b, formcraft_tess::Quality::Export)?);
            }
            stl::write(&mesh, path)?;
        }
        Some(Format::ThreeMf) => {
            let mut objects = Vec::new();
            for (i, b) in bodies.iter().enumerate() {
                objects.push((
                    format!("Body{}", i + 1),
                    formcraft_tess::mesh(kernel, b, formcraft_tess::Quality::Export)?,
                ));
            }
            threemf::write(&objects, path)?;
        }
        None => return Err(IoError::Format(format!("unsupported file type: {}", path.display()))),
    }
    Ok(())
}

/// Read a STEP file into its solids.
pub fn import_step(kernel: &dyn Kernel, path: &Path) -> Result<Vec<Shape>> {
    let s = kernel.read_step(path)?;
    let solids = kernel.solids(&s)?;
    if solids.is_empty() { Err(IoError::Format("the file contains no solids".into())) } else { Ok(solids) }
}

#[cfg(test)]
mod threemf_tests {
    use formcraft_kernel::Mesh;

    #[test]
    fn model_shares_vertices_and_escapes_names() {
        // two triangles of a square with duplicated corners
        let p = |x: f32, y: f32| [x, y, 0.0];
        let m = Mesh {
            positions: vec![p(0., 0.), p(1., 0.), p(1., 1.), p(0., 0.), p(1., 1.), p(0., 1.)],
            indices: vec![0, 1, 2, 3, 4, 5],
            ..Default::default()
        };
        let x = super::threemf::model_xml(&[("a<b".into(), m)]);
        assert_eq!(x.matches("<vertex ").count(), 4, "duplicates merged");
        assert_eq!(x.matches("<triangle ").count(), 2);
        assert!(x.contains("name=\"a&lt;b\""));
        assert!(x.contains("unit=\"millimeter\""));
    }
}
