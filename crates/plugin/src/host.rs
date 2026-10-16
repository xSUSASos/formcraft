//! The host side of the WIT world: `host` (commands, model, log, files) and `geometry`
//! (solids as resources backed by the kernel).

use std::path::PathBuf;
use std::sync::Arc;

use formcraft_engine::Session;
use formcraft_kernel::{DVec2, DVec3, Kernel, Loop2, Plane, Profile, Region, Segment2, Shape};
use serde_json::{Value, json};
use wasmtime::StoreLimits;
use wasmtime::component::{HasSelf, Linker, Resource, ResourceTable};
use wasmtime_wasi::{WasiCtx, WasiCtxView, WasiView};

wasmtime::component::bindgen!({
    path: "../plugin-api/wit",
    world: "plugin",
    with: {
        "formcraft:plugin/geometry.solid": SolidRep,
    },
});

pub use formcraft::plugin::types;
pub type Bindings = Plugin;

/// A solid owned by the host; the plugin only holds a handle.
pub struct SolidRep(pub Shape);

pub struct HostState {
    pub table: ResourceTable,
    pub wasi: WasiCtx,
    pub limits: StoreLimits,
    /// The session, lent for the duration of a plugin command.
    pub session: Option<Session>,
    pub kernel: Arc<dyn Kernel>,
    /// Folders the plugin may read and write (manifest permissions).
    pub files: Vec<PathBuf>,
    pub log: Vec<String>,
}

const LOG_LINES: usize = 500;
const MAX_FILE: u64 = 64 << 20;
const MAX_POINTS: usize = 100_000;

impl WasiView for HostState {
    fn ctx(&mut self) -> WasiCtxView<'_> {
        WasiCtxView { ctx: &mut self.wasi, table: &mut self.table }
    }
}

pub fn add_to_linker(l: &mut Linker<HostState>) -> wasmtime::Result<()> {
    Plugin::add_to_linker::<HostState, HasSelf<HostState>>(l, |s| s)
}

/// Move a returned solid out of the resource table.
pub fn take_solid(s: &mut HostState, r: Resource<SolidRep>) -> wasmtime::Result<Shape> {
    Ok(s.table.delete(r)?.0)
}

impl HostState {
    fn push_log(&mut self, line: String) {
        if self.log.len() >= LOG_LINES {
            self.log.remove(0);
        }
        self.log.push(line);
    }

    fn allowed(&self, path: &str) -> Result<PathBuf, String> {
        let p = PathBuf::from(path);
        if p.components().any(|c| matches!(c, std::path::Component::ParentDir)) {
            return Err("`..` is not allowed in plugin paths".into());
        }
        if self.files.iter().any(|root| p.starts_with(root)) {
            Ok(p)
        } else {
            Err(format!("no permission for `{path}` (see [permissions] files in plugin.toml)"))
        }
    }

    fn solid(&mut self, shape: Shape) -> Result<Resource<SolidRep>, String> {
        self.table.push(SolidRep(shape)).map_err(|e| e.to_string())
    }

    fn shape(&self, r: &Resource<SolidRep>) -> Result<Shape, String> {
        self.table.get(r).map(|s| s.0.clone()).map_err(|e| e.to_string())
    }
}

fn v3(v: types::Vec3) -> Result<DVec3, String> {
    let d = DVec3::new(v.x, v.y, v.z);
    if d.is_finite() { Ok(d) } else { Err("coordinates must be finite".into()) }
}

fn loop2(points: &[types::Point2]) -> Result<Loop2, String> {
    if points.len() < 3 || points.len() > MAX_POINTS {
        return Err(format!("a polygon needs 3..{MAX_POINTS} points"));
    }
    let pts: Vec<DVec2> = points.iter().map(|p| DVec2::new(p.x, p.y)).collect();
    if pts.iter().any(|p| !p.is_finite()) {
        return Err("polygon points must be finite".into());
    }
    let segs = (0..pts.len())
        .filter_map(|i| {
            let (a, b) = (pts.get(i)?, pts.get((i + 1) % pts.len())?);
            (a.distance(*b) > 1e-9).then_some(Segment2::Line { a: *a, b: *b })
        })
        .collect();
    Ok(Loop2::new(segs))
}

impl formcraft::plugin::types::Host for HostState {}

impl formcraft::plugin::host::Host for HostState {
    fn execute(&mut self, command: String, params: String) -> Result<String, String> {
        let p: Value = serde_json::from_str(&params).map_err(|e| format!("params: {e}"))?;
        let s = self.session.as_mut().ok_or("the model is only available while a plugin command runs")?;
        s.execute(&command, &p).map(|v| v.to_string()).map_err(|e| e.to_string())
    }

    fn inspect(&mut self) -> String {
        match self.session.as_mut() {
            Some(s) => s.execute("model.inspect", &json!({})).map(|v| v.to_string()).unwrap_or_default(),
            None => "{}".into(),
        }
    }

    fn log(&mut self, level: u8, message: String) {
        let tag = ["trace", "debug", "info", "warn", "error"].get(level as usize).copied().unwrap_or("info");
        let msg: String = message.chars().take(2000).collect();
        log::info!("plugin [{tag}] {msg}");
        self.push_log(format!("[{tag}] {msg}"));
    }

    fn read_file(&mut self, path: String) -> Result<Vec<u8>, String> {
        let p = self.allowed(&path)?;
        let meta = std::fs::metadata(&p).map_err(|e| e.to_string())?;
        if meta.len() > MAX_FILE {
            return Err("file too large".into());
        }
        std::fs::read(p).map_err(|e| e.to_string())
    }

    fn write_file(&mut self, path: String, data: Vec<u8>) -> Result<(), String> {
        let p = self.allowed(&path)?;
        std::fs::write(p, data).map_err(|e| e.to_string())
    }
}

impl formcraft::plugin::geometry::Host for HostState {
    fn make_box(&mut self, corner: types::Vec3, size: types::Vec3) -> Result<Resource<SolidRep>, String> {
        let (c, s) = (v3(corner)?, v3(size)?);
        let shape = self.kernel.make_box(c.min(c + s), s.abs()).map_err(|e| e.to_string())?;
        self.solid(shape)
    }

    fn make_cylinder(
        &mut self,
        base: types::Vec3,
        axis: types::Vec3,
        radius: f64,
        height: f64,
    ) -> Result<Resource<SolidRep>, String> {
        let shape = self.kernel.make_cylinder(v3(base)?, v3(axis)?, radius, height).map_err(|e| e.to_string())?;
        self.solid(shape)
    }

    fn extrude(
        &mut self,
        p: types::Plane,
        outer: Vec<types::Point2>,
        holes: Vec<Vec<types::Point2>>,
        distance: f64,
    ) -> Result<Resource<SolidRep>, String> {
        if !distance.is_finite() || distance.abs() < 1e-9 {
            return Err("distance must be a non-zero number".into());
        }
        let plane = Plane::new(v3(p.origin)?, v3(p.x_dir)?, v3(p.normal)?).map_err(|e| e.to_string())?;
        let region = Region { outer: loop2(&outer)?, holes: holes.iter().map(|h| loop2(h)).collect::<Result<_, _>>()? };
        let profile = Profile { plane, regions: vec![region] };
        let shape = self.kernel.extrude(&profile, plane.normal * distance).map_err(|e| e.to_string())?;
        self.solid(shape)
    }
}

impl formcraft::plugin::geometry::HostSolid for HostState {
    fn mass_props(&mut self, me: Resource<SolidRep>) -> Result<types::MassProps, String> {
        let m = self.kernel.mass_props(&self.shape(&me)?).map_err(|e| e.to_string())?;
        let v = |d: DVec3| types::Vec3 { x: d.x, y: d.y, z: d.z };
        Ok(types::MassProps { volume: m.volume, area: m.area, bbox_min: v(m.bbox.min), bbox_max: v(m.bbox.max) })
    }

    fn transformed(
        &mut self,
        me: Resource<SolidRep>,
        offset: types::Vec3,
        origin: types::Vec3,
        axis: types::Vec3,
        angle_deg: f64,
    ) -> Result<Resource<SolidRep>, String> {
        let (o, c, a) = (v3(offset)?, v3(origin)?, v3(axis)?);
        if !angle_deg.is_finite() {
            return Err("angle must be finite".into());
        }
        let m = if a.length() > 1e-12 && angle_deg != 0.0 {
            formcraft_kernel::DMat4::from_translation(c)
                * formcraft_kernel::DMat4::from_axis_angle(a.normalize(), angle_deg.to_radians())
                * formcraft_kernel::DMat4::from_translation(-c)
                * formcraft_kernel::DMat4::from_translation(o)
        } else {
            formcraft_kernel::DMat4::from_translation(o)
        };
        let shape = self.kernel.transform(&self.shape(&me)?, &m).map_err(|e| e.to_string())?;
        self.solid(shape)
    }

    fn fuse(&mut self, me: Resource<SolidRep>, other: Resource<SolidRep>) -> Result<Resource<SolidRep>, String> {
        self.boolean(me, other, formcraft_kernel::BoolOp::Union)
    }

    fn cut(&mut self, me: Resource<SolidRep>, tool: Resource<SolidRep>) -> Result<Resource<SolidRep>, String> {
        self.boolean(me, tool, formcraft_kernel::BoolOp::Cut)
    }

    fn common(&mut self, me: Resource<SolidRep>, other: Resource<SolidRep>) -> Result<Resource<SolidRep>, String> {
        self.boolean(me, other, formcraft_kernel::BoolOp::Intersect)
    }

    fn drop(&mut self, rep: Resource<SolidRep>) -> wasmtime::Result<()> {
        self.table.delete(rep)?;
        Ok(())
    }
}

impl HostState {
    fn boolean(
        &mut self,
        a: Resource<SolidRep>,
        b: Resource<SolidRep>,
        op: formcraft_kernel::BoolOp,
    ) -> Result<Resource<SolidRep>, String> {
        let (x, y) = (self.shape(&a)?, self.shape(&b)?);
        let shape = self.kernel.boolean(op, &x, &y).map_err(|e| e.to_string())?;
        self.solid(shape)
    }
}
