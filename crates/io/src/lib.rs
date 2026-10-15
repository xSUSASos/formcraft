//! File formats: STEP (through the kernel), binary STL, and later 3MF and `.fcraft`.
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
}

impl Format {
    pub fn from_path(p: &Path) -> Option<Format> {
        match p.extension()?.to_str()?.to_ascii_lowercase().as_str() {
            "step" | "stp" => Some(Format::Step),
            "stl" => Some(Format::Stl),
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
