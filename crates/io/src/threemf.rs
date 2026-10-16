//! 3MF export (3D Manufacturing Format core spec): a zip package with one mesh object per body,
//! millimetres, shared (deduplicated) vertices so the meshes are watertight for slicers.

use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use formcraft_kernel::Mesh;

use crate::{IoError, Result};

const CONTENT_TYPES: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Types xmlns="http://schemas.openxmlformats.org/package/2006/content-types">
  <Default Extension="rels" ContentType="application/vnd.openxmlformats-package.relationships+xml"/>
  <Default Extension="model" ContentType="application/vnd.ms-package.3dmanufacturing-3dmodel+xml"/>
</Types>
"#;

const RELS: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<Relationships xmlns="http://schemas.openxmlformats.org/package/2006/relationships">
  <Relationship Target="/3D/3dmodel.model" Id="rel0" Type="http://schemas.microsoft.com/3dmanufacturing/2013/01/3dmodel"/>
</Relationships>
"#;

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

/// Vertices merged at 1e-6 mm; triangles that collapse after merging are dropped.
fn indexed(mesh: &Mesh) -> (Vec<[f32; 3]>, Vec<[u32; 3]>) {
    let key = |p: &[f32; 3]| p.map(|c| (f64::from(c) * 1e6).round() as i64);
    let mut map: HashMap<[i64; 3], u32> = HashMap::new();
    let mut verts = Vec::new();
    let mut remap = Vec::with_capacity(mesh.positions.len());
    for p in &mesh.positions {
        let id = *map.entry(key(p)).or_insert_with(|| {
            verts.push(*p);
            (verts.len() - 1) as u32
        });
        remap.push(id);
    }
    let tris = mesh
        .indices
        .as_chunks::<3>()
        .0
        .iter()
        .filter_map(|&[i0, i1, i2]| {
            let (a, b, c) = (*remap.get(i0 as usize)?, *remap.get(i1 as usize)?, *remap.get(i2 as usize)?);
            (a != b && b != c && a != c).then_some([a, b, c])
        })
        .collect();
    (verts, tris)
}

/// The `3D/3dmodel.model` document for named meshes.
pub fn model_xml(objects: &[(String, Mesh)]) -> String {
    let mut x = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<model unit=\"millimeter\" xml:lang=\"en-US\" \
         xmlns=\"http://schemas.microsoft.com/3dmanufacturing/core/2015/02\">\n\
         <metadata name=\"Application\">FormCraft</metadata>\n<resources>\n",
    );
    for (i, (name, mesh)) in objects.iter().enumerate() {
        let (verts, tris) = indexed(mesh);
        x.push_str(&format!(
            "<object id=\"{}\" type=\"model\" name=\"{}\"><mesh><vertices>\n",
            i + 1,
            xml_escape(name)
        ));
        for v in &verts {
            x.push_str(&format!("<vertex x=\"{}\" y=\"{}\" z=\"{}\"/>\n", v[0], v[1], v[2]));
        }
        x.push_str("</vertices><triangles>\n");
        for t in &tris {
            x.push_str(&format!("<triangle v1=\"{}\" v2=\"{}\" v3=\"{}\"/>\n", t[0], t[1], t[2]));
        }
        x.push_str("</triangles></mesh></object>\n");
    }
    x.push_str("</resources>\n<build>\n");
    for i in 0..objects.len() {
        x.push_str(&format!("<item objectid=\"{}\"/>\n", i + 1));
    }
    x.push_str("</build>\n</model>\n");
    x
}

pub fn write(objects: &[(String, Mesh)], path: &Path) -> Result<()> {
    let tmp = path.with_extension("3mf.tmp");
    let f = std::fs::File::create(&tmp)?;
    let mut z = zip::ZipWriter::new(f);
    let opts = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    let err = |e: zip::result::ZipError| IoError::Format(format!("3mf: {e}"));
    z.start_file("[Content_Types].xml", opts).map_err(err)?;
    z.write_all(CONTENT_TYPES.as_bytes())?;
    z.start_file("_rels/.rels", opts).map_err(err)?;
    z.write_all(RELS.as_bytes())?;
    z.start_file("3D/3dmodel.model", opts).map_err(err)?;
    z.write_all(model_xml(objects).as_bytes())?;
    z.finish().map_err(err)?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}
