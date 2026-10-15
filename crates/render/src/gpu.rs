//! wgpu renderer: draws a [`Scene`] into an offscreen texture (4× MSAA, depth), which the UI
//! shows as an image and the headless path reads back as PNG (MCP `render`, CLI).

use std::collections::HashMap;

use bytemuck::{Pod, Zeroable};
use glam::Mat4;
use wgpu::util::DeviceExt;

use crate::scene::{LineVertex, MeshVertex, Scene};

pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const SAMPLES: u32 = 4;

const SHADER: &str = r#"
struct Globals {
    view_proj: mat4x4<f32>,
    eye: vec4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;

struct MeshIn {
    @location(0) pos: vec3<f32>,
    @location(1) normal: vec3<f32>,
    @location(2) color: vec4<f32>,
};
struct MeshOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
    @location(2) world: vec3<f32>,
};

@vertex
fn vs_mesh(v: MeshIn) -> MeshOut {
    var o: MeshOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.normal = v.normal;
    o.color = v.color;
    o.world = v.pos;
    return o;
}

@fragment
fn fs_mesh(i: MeshOut, @builtin(front_facing) front: bool) -> @location(0) vec4<f32> {
    var n = normalize(i.normal);
    if (!front) { n = -n; }
    let view_dir = normalize(g.eye.xyz - i.world);
    // Headlight plus a fixed key light from above, like a CAD viewport.
    let head = max(dot(n, view_dir), 0.0);
    let key = max(dot(n, normalize(g.light.xyz)), 0.0);
    let ambient = 0.42;
    let diffuse = 0.48 * head + 0.22 * key;
    let h = normalize(view_dir + normalize(g.light.xyz));
    let spec = 0.10 * pow(max(dot(n, h), 0.0), 60.0);
    let rgb = i.color.rgb * (ambient + diffuse) + vec3<f32>(spec);
    return vec4<f32>(rgb, i.color.a);
}

struct LineIn {
    @location(0) pos: vec3<f32>,
    @location(1) color: vec4<f32>,
};
struct LineOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(v: LineIn) -> LineOut {
    var o: LineOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.color = v.color;
    return o;
}

@fragment
fn fs_line(i: LineOut) -> @location(0) vec4<f32> {
    return i.color;
}
"#;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
    light: [f32; 4],
}

struct GpuMesh {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    vertex_count: usize,
    edges: Option<(wgpu::Buffer, u32)>,
}

struct Targets {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    resolve_view: wgpu::TextureView,
}

#[derive(Debug, thiserror::Error)]
pub enum GpuError {
    #[error("no GPU adapter available: {0}")]
    NoAdapter(String),
    #[error("GPU device: {0}")]
    Device(String),
    #[error("readback failed: {0}")]
    Readback(String),
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    mesh_pipeline: wgpu::RenderPipeline,
    mesh_pipeline_transparent: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    meshes: HashMap<u64, GpuMesh>,
    targets: Option<Targets>,
}

fn pipeline(
    device: &wgpu::Device,
    layout: &wgpu::PipelineLayout,
    module: &wgpu::ShaderModule,
    lines: bool,
    transparent: bool,
) -> wgpu::RenderPipeline {
    let mesh_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
    let line_attrs = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
    let buffers = if lines {
        [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<LineVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &line_attrs,
        })]
    } else {
        [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<MeshVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attrs,
        })]
    };
    let blend = if transparent || lines { Some(wgpu::BlendState::ALPHA_BLENDING) } else { None };
    device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some(if lines { "lines" } else { "mesh" }),
        layout: Some(layout),
        vertex: wgpu::VertexState {
            module,
            entry_point: Some(if lines { "vs_line" } else { "vs_mesh" }),
            compilation_options: Default::default(),
            buffers: &buffers,
        },
        primitive: wgpu::PrimitiveState {
            topology: if lines { wgpu::PrimitiveTopology::LineList } else { wgpu::PrimitiveTopology::TriangleList },
            cull_mode: None,
            ..Default::default()
        },
        depth_stencil: Some(wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(!transparent),
            depth_compare: Some(wgpu::CompareFunction::LessEqual),
            stencil: Default::default(),
            // Push faces back a little so their own edges win the depth test.
            bias: if lines {
                Default::default()
            } else {
                wgpu::DepthBiasState { constant: 2, slope_scale: 1.5, clamp: 0.0 }
            },
        }),
        multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
        fragment: Some(wgpu::FragmentState {
            module,
            entry_point: Some(if lines { "fs_line" } else { "fs_mesh" }),
            compilation_options: Default::default(),
            targets: &[Some(wgpu::ColorTargetState {
                format: COLOR_FORMAT,
                blend,
                write_mask: wgpu::ColorWrites::ALL,
            })],
        }),
        multiview_mask: None,
        cache: None,
    })
}

impl Renderer {
    pub fn new(device: wgpu::Device, queue: wgpu::Queue) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("formcraft"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: globals.as_entire_binding() }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("formcraft"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let mesh_pipeline = pipeline(&device, &layout, &module, false, false);
        let mesh_pipeline_transparent = pipeline(&device, &layout, &module, false, true);
        let line_pipeline = pipeline(&device, &layout, &module, true, false);
        Renderer {
            device,
            queue,
            globals,
            bind_group,
            mesh_pipeline,
            mesh_pipeline_transparent,
            line_pipeline,
            meshes: HashMap::new(),
            targets: None,
        }
    }

    /// A renderer on its own device (no window): for PNG rendering in the CLI and MCP.
    pub fn headless() -> Result<Self, GpuError> {
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle_from_env());
        let adapter =
            pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                force_fallback_adapter: false,
                compatible_surface: None,
                ..Default::default()
            }))
            .or_else(|_| {
                pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
                    force_fallback_adapter: true,
                    ..Default::default()
                }))
            })
            .map_err(|e| GpuError::NoAdapter(e.to_string()))?;
        let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
            label: Some("formcraft-headless"),
            required_limits: wgpu::Limits::downlevel_defaults().using_resolution(adapter.limits()),
            ..Default::default()
        }))
        .map_err(|e| GpuError::Device(e.to_string()))?;
        Ok(Self::new(device, queue))
    }

    pub fn device(&self) -> &wgpu::Device {
        &self.device
    }

    /// Upload (or replace) the mesh for `key` (one per body).
    pub fn set_mesh(&mut self, key: u64, vertices: &[MeshVertex], indices: &[u32], edges: &[LineVertex]) {
        let vb = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh vertices"),
            contents: bytemuck::cast_slice(vertices),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        let ib = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("mesh indices"),
            contents: bytemuck::cast_slice(indices),
            usage: wgpu::BufferUsages::INDEX,
        });
        let eb = (!edges.is_empty()).then(|| {
            let b = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("edges"),
                contents: bytemuck::cast_slice(edges),
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });
            (b, edges.len() as u32)
        });
        self.meshes.insert(
            key,
            GpuMesh {
                vertices: vb,
                indices: ib,
                index_count: indices.len() as u32,
                vertex_count: vertices.len(),
                edges: eb,
            },
        );
    }

    /// Re-colour an uploaded mesh (selection/hover changes) without re-uploading geometry.
    pub fn recolor(&mut self, key: u64, vertices: &[MeshVertex], edges: &[LineVertex]) {
        if let Some(m) = self.meshes.get(&key) {
            if vertices.len() == m.vertex_count {
                self.queue.write_buffer(&m.vertices, 0, bytemuck::cast_slice(vertices));
            }
            if let Some((eb, n)) = &m.edges
                && edges.len() == *n as usize
            {
                self.queue.write_buffer(eb, 0, bytemuck::cast_slice(edges));
            }
        }
    }

    pub fn has_mesh(&self, key: u64) -> bool {
        self.meshes.contains_key(&key)
    }

    /// Drop meshes whose keys are not in `keep`.
    pub fn retain(&mut self, keep: &[u64]) {
        self.meshes.retain(|k, _| keep.contains(k));
    }

    fn ensure_targets(&mut self, size: [u32; 2]) {
        let size = [size[0].clamp(1, 8192), size[1].clamp(1, 8192)];
        if self.targets.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let extent = wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 };
        let make = |label, format, samples, usage| {
            self.device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: extent,
                mip_level_count: 1,
                sample_count: samples,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage,
                view_formats: &[],
            })
        };
        let msaa = make("msaa", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = make("depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolve = make(
            "resolve",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::TEXTURE_BINDING
                | wgpu::TextureUsages::COPY_SRC,
        );
        self.targets = Some(Targets {
            size,
            msaa: msaa.create_view(&Default::default()),
            depth: depth.create_view(&Default::default()),
            resolve_view: resolve.create_view(&Default::default()),
            resolve,
        });
    }

    /// Render the scene; returns the resolved colour view (valid until the next resize).
    pub fn render(&mut self, scene: &Scene, size: [u32; 2]) -> Option<&wgpu::TextureView> {
        self.ensure_targets(size);
        let aspect = size[0] as f64 / size[1].max(1) as f64;
        let vp: Mat4 = scene.camera.view_proj_f32(aspect);
        let eye = scene.camera.eye().as_vec3();
        let g = Globals {
            view_proj: vp.to_cols_array_2d(),
            eye: [eye.x, eye.y, eye.z, 1.0],
            light: [0.35, -0.45, 0.82, 0.0],
        };
        self.queue.write_buffer(&self.globals, 0, bytemuck::bytes_of(&g));
        let lines = (!scene.lines.is_empty()).then(|| {
            self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("scene lines"),
                contents: bytemuck::cast_slice(&scene.lines),
                usage: wgpu::BufferUsages::VERTEX,
            })
        });
        let t = self.targets.as_ref()?;
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("scene") });
        {
            let [r, gg, b, a] = scene.background;
            let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa,
                    depth_slice: None,
                    resolve_target: Some(&t.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: r as f64, g: gg as f64, b: b as f64, a: a as f64 }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            // Opaque bodies, then scene lines (grid), edges, then transparent previews.
            for pass_transparent in [false, true] {
                pass.set_pipeline(if pass_transparent { &self.mesh_pipeline_transparent } else { &self.mesh_pipeline });
                for item in scene.items.iter().filter(|i| i.visible && i.shaded && i.transparent == pass_transparent) {
                    if let Some(m) = self.meshes.get(&item.key) {
                        pass.set_vertex_buffer(0, m.vertices.slice(..));
                        pass.set_index_buffer(m.indices.slice(..), wgpu::IndexFormat::Uint32);
                        pass.draw_indexed(0..m.index_count, 0, 0..1);
                    }
                }
                if !pass_transparent {
                    pass.set_pipeline(&self.line_pipeline);
                    if let Some(lb) = &lines {
                        pass.set_vertex_buffer(0, lb.slice(..));
                        pass.draw(0..scene.lines.len() as u32, 0..1);
                    }
                    for item in scene.items.iter().filter(|i| i.visible && i.edges) {
                        if let Some((eb, n)) = self.meshes.get(&item.key).and_then(|m| m.edges.as_ref()) {
                            pass.set_vertex_buffer(0, eb.slice(..));
                            pass.draw(0..*n, 0..1);
                        }
                    }
                }
            }
        }
        self.queue.submit(Some(enc.finish()));
        Some(&t.resolve_view)
    }

    /// Render and read the image back as RGBA8 (row-major, top row first).
    pub fn render_rgba(&mut self, scene: &Scene, size: [u32; 2]) -> Result<Vec<u8>, GpuError> {
        self.render(scene, size).ok_or_else(|| GpuError::Readback("no render target".into()))?;
        let t = self.targets.as_ref().ok_or_else(|| GpuError::Readback("no render target".into()))?;
        let [w, h] = t.size;
        let row = w * 4;
        let padded = row.div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
        let buf = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (padded * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut enc = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("readback") });
        enc.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &t.resolve,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buf,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(h) },
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );
        self.queue.submit(Some(enc.finish()));
        let slice = buf.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let _ = tx.send(r);
        });
        self.device.poll(wgpu::PollType::wait_indefinitely()).map_err(|e| GpuError::Readback(e.to_string()))?;
        rx.recv().map_err(|e| GpuError::Readback(e.to_string()))?.map_err(|e| GpuError::Readback(e.to_string()))?;
        let data = slice.get_mapped_range().map_err(|e| GpuError::Readback(e.to_string()))?;
        let mut out = Vec::with_capacity((row * h) as usize);
        for y in 0..h as usize {
            let start = y * padded as usize;
            out.extend_from_slice(data.get(start..start + row as usize).unwrap_or_default());
        }
        drop(data);
        buf.unmap();
        Ok(out)
    }
}
