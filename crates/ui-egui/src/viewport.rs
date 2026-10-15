//! 3D viewport: keeps tessellated bodies in sync with the engine, renders them offscreen with
//! the wgpu renderer and shows the result as an egui texture; picks faces/edges/bodies.

use std::collections::HashMap;

use egui::{Color32, Rect, TextureId};
use formcraft_engine::doc::BodyId;
use formcraft_engine::regen::Model;
use formcraft_engine::{Pick, Session};
use formcraft_kernel::Mesh;
use formcraft_render::camera::Camera;
use formcraft_render::gpu::Renderer;
use formcraft_render::pick::{self, Hit};
use formcraft_render::scene::{self, GridColors, Item, Scene, linear};
use glam::DVec2;

use crate::layout::VisualStyle;
use crate::theme::Tokens;

fn rgba(c: Color32) -> [f32; 4] {
    linear([c.r(), c.g(), c.b(), c.a()])
}

/// Body colour (a neutral steel blue-grey, like a CAD default material).
const BODY: [u8; 4] = [0xb4, 0xbd, 0xc8, 0xff];

pub struct Viewport {
    gpu: Option<(egui_wgpu::RenderState, Renderer)>,
    texture: Option<TextureId>,
    /// CPU meshes per body (for picking and re-colouring).
    meshes: HashMap<u64, Mesh>,
    revision: Option<u64>,
    /// Selection/hover state the GPU colours were baked for.
    painted: Option<(Vec<Pick>, Option<Pick>, u64)>,
    last_size: [u32; 2],
    pub tessellation_error: Option<String>,
}

impl Viewport {
    pub fn new(state: Option<egui_wgpu::RenderState>) -> Self {
        let gpu = state.map(|s| {
            let r = Renderer::new(s.device.clone(), s.queue.clone());
            (s, r)
        });
        Viewport {
            gpu,
            texture: None,
            meshes: HashMap::new(),
            revision: None,
            painted: None,
            last_size: [0, 0],
            tessellation_error: None,
        }
    }

    /// Re-tessellate bodies when the shown model changed (`key` identifies it).
    fn sync(&mut self, s: &Session, model: &Model, key: u64) {
        if self.revision == Some(key) {
            return;
        }
        self.revision = Some(key);
        self.meshes.clear();
        self.tessellation_error = None;
        for b in &model.bodies {
            match formcraft_tess::mesh(s.kernel().as_ref(), &b.shape, formcraft_tess::Quality::Display) {
                Ok(m) => {
                    self.meshes.insert(b.id.0 as u64, m);
                }
                Err(e) => self.tessellation_error = Some(format!("{}: {e}", b.name)),
            }
        }
        self.painted = None;
    }

    /// Upload or re-colour meshes for the current selection/hover.
    fn upload(&mut self, s: &Session, t: &Tokens, key: u64) {
        let sel = s.selection();
        let state = (sel.items.clone(), sel.hover, key);
        if self.painted.as_ref() == Some(&state) {
            return;
        }
        let Some((_, r)) = self.gpu.as_mut() else { return };
        let base = linear(BODY);
        let edge = rgba(Color32::from_rgb(0x20, 0x24, 0x2a));
        let selected = rgba(t.accent);
        let hover = rgba(t.hover);
        for (key, m) in &self.meshes {
            let body = BodyId(*key as u32);
            let body_sel = sel.contains(&Pick::Body { body });
            let body_hover = sel.hover == Some(Pick::Body { body });
            let face = |f: u32| {
                if sel.contains(&Pick::Face { body, index: f }) || body_sel {
                    Some(selected)
                } else if sel.hover == Some(Pick::Face { body, index: f }) || body_hover {
                    Some(hover)
                } else {
                    None
                }
            };
            let edge_c = |e: u32| {
                if sel.contains(&Pick::Edge { body, index: e }) {
                    Some(selected)
                } else if sel.hover == Some(Pick::Edge { body, index: e }) {
                    Some(hover)
                } else {
                    None
                }
            };
            let (v, i) = scene::mesh_vertices(m, base, &face);
            let e = scene::edge_vertices(m, edge, &edge_c);
            if self.painted.as_ref().is_some_and(|p| p.2 == state.2) && r.has_mesh(*key) {
                r.recolor(*key, &v, &e);
            } else {
                r.set_mesh(*key, &v, &i, &e);
            }
        }
        let keys: Vec<u64> = self.meshes.keys().copied().collect();
        r.retain(&keys);
        self.painted = Some(state);
    }

    /// Draw the scene into `rect`. Returns false when there is no GPU (caller draws a fallback).
    pub fn paint(
        &mut self,
        ui: &mut egui::Ui,
        rect: Rect,
        s: &Session,
        shown: Option<(&Model, u64)>,
        cam: &Camera,
        t: &Tokens,
        grid: bool,
        style: VisualStyle,
    ) -> bool {
        let (model, key) = shown.unwrap_or((s.model(), s.revision()));
        self.sync(s, model, key);
        if self.gpu.is_none() {
            return false;
        }
        self.upload(s, t, key);
        let Some((state, r)) = self.gpu.as_mut() else { return false };
        let ppp = ui.ctx().pixels_per_point();
        let size = [(rect.width() * ppp).round().max(1.0) as u32, (rect.height() * ppp).round().max(1.0) as u32];
        let grid_colors = GridColors {
            minor: rgba(t.grid_minor),
            major: rgba(t.grid_major),
            x: rgba(t.axis_x),
            y: rgba(t.axis_y),
            z: rgba(t.axis_z),
        };
        let items = model
            .bodies
            .iter()
            .map(|b| Item {
                key: b.id.0 as u64,
                visible: b.visible,
                shaded: style != VisualStyle::Wireframe,
                edges: style != VisualStyle::Shaded,
                transparent: false,
            })
            .collect();
        // Transparent clear: the egui gradient behind the canvas shows through.
        let scene = Scene {
            camera: *cam,
            background: [0.0, 0.0, 0.0, 0.0],
            lines: scene::grid_lines(cam, &grid_colors, grid),
            items,
        };
        let Some(view) = r.render(&scene, size) else { return false };
        let mut egui_r = state.renderer.write();
        let id = match self.texture {
            Some(id) => {
                egui_r.update_egui_texture_from_wgpu_texture(r_device(state), view, wgpu_filter(), id);
                id
            }
            None => egui_r.register_native_texture(r_device(state), view, wgpu_filter()),
        };
        drop(egui_r);
        self.texture = Some(id);
        self.last_size = size;
        ui.painter().image(id, rect, Rect::from_min_max(egui::pos2(0.0, 0.0), egui::pos2(1.0, 1.0)), Color32::WHITE);
        true
    }

    /// What is under `pixel` (canvas coordinates in points).
    pub fn pick(&self, cam: &Camera, pixel: DVec2, size: DVec2, s: &Session) -> Option<Pick> {
        let visible: Vec<u64> = s.model().bodies.iter().filter(|b| b.visible).map(|b| b.id.0 as u64).collect();
        let meshes: Vec<(u64, &Mesh)> =
            self.meshes.iter().filter(|(k, _)| visible.contains(k)).map(|(k, m)| (*k, m)).collect();
        pick::pick(cam, pixel, size, &meshes, 6.0).map(|h| match h {
            Hit::Face { key, face, .. } => Pick::Face { body: BodyId(key as u32), index: face },
            Hit::Edge { key, edge, .. } => Pick::Edge { body: BodyId(key as u32), index: edge },
        })
    }
}

fn r_device(s: &egui_wgpu::RenderState) -> &egui_wgpu::wgpu::Device {
    &s.device
}

fn wgpu_filter() -> egui_wgpu::wgpu::FilterMode {
    egui_wgpu::wgpu::FilterMode::Linear
}
