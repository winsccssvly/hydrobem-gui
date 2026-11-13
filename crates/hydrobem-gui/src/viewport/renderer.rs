//! The wgpu renderer of the viewport. The scene is drawn into an offscreen target of its own
//! (4x MSAA with a depth buffer), then copied into egui's pass: the viewport does not depend on
//! eframe's depth or multisampling settings. Opaque triangles write depth; translucent ones are
//! sorted back to front and drawn after them without writing depth; outlines write depth, so the
//! faces at their depth leave them visible; markers are depth tested; the axes and the picked point are drawn on top.

use std::sync::Arc;

use eframe::egui;
use eframe::egui_wgpu::{self, CallbackResources, CallbackTrait, ScreenDescriptor};
use eframe::wgpu::{self, util::DeviceExt};
use gui_core::scene::{LineVertex, Scene, Vertex};

use super::camera::Mat4;

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

const SHADER: &str = r#"
struct Uniforms {
    view_projection: mat4x4<f32>,
    light: vec4<f32>,
};
@group(0) @binding(0) var<uniform> u: Uniforms;

struct MeshOut {
    @builtin(position) position: vec4<f32>,
    @location(0) normal: vec3<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_mesh(@location(0) position: vec3<f32>, @location(1) normal: vec3<f32>, @location(2) color: vec4<f32>) -> MeshOut {
    var out: MeshOut;
    out.position = u.view_projection * vec4<f32>(position, 1.0);
    out.normal = normal;
    out.color = color;
    return out;
}

// Two-sided diffuse lighting: panels are seen from both sides.
@fragment
fn fs_mesh(in: MeshOut) -> @location(0) vec4<f32> {
    let diffuse = abs(dot(normalize(in.normal), u.light.xyz));
    return vec4<f32>(min(in.color.rgb * (0.46 + 0.62 * diffuse), vec3<f32>(1.0)), in.color.a);
}

struct LineOut {
    @builtin(position) position: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_line(@location(0) position: vec3<f32>, @location(1) color: vec4<f32>) -> LineOut {
    var out: LineOut;
    out.position = u.view_projection * vec4<f32>(position, 1.0);
    out.color = color;
    return out;
}

@fragment
fn fs_line(in: LineOut) -> @location(0) vec4<f32> {
    return in.color;
}

// The offscreen image into egui's pass: one triangle over the viewport.
struct BlitOut {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> BlitOut {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var out: BlitOut;
    out.position = vec4<f32>(uv.x * 2.0 - 1.0, 1.0 - uv.y * 2.0, 0.0, 1.0);
    out.uv = uv;
    return out;
}

@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var image_sampler: sampler;

@fragment
fn fs_blit(in: BlitOut) -> @location(0) vec4<f32> {
    return textureSample(image, image_sampler, in.uv);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct GpuLine {
    position: [f32; 3],
    color: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Uniforms {
    view_projection: Mat4,
    light: [f32; 4],
}

fn mesh_vertices(v: &[Vertex]) -> Vec<GpuVertex> {
    v.iter().map(|v| GpuVertex { position: v.position, normal: v.normal, color: v.color }).collect()
}

fn line_vertices(v: &[LineVertex]) -> Vec<GpuLine> {
    v.iter().map(|v| GpuLine { position: v.position, color: v.color }).collect()
}

struct Buffer {
    buffer: Option<wgpu::Buffer>,
    count: u32,
}

impl Buffer {
    fn empty() -> Buffer {
        Buffer { buffer: None, count: 0 }
    }

    fn upload<T: bytemuck::Pod>(device: &wgpu::Device, data: &[T]) -> Buffer {
        if data.is_empty() {
            return Buffer::empty();
        }
        let buffer = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("viewport vertices"),
            contents: bytemuck::cast_slice(data),
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
        });
        Buffer { buffer: Some(buffer), count: data.len() as u32 }
    }

    fn draw<'a>(&'a self, pass: &mut wgpu::RenderPass<'a>, pipeline: &'a wgpu::RenderPipeline) {
        if let Some(b) = &self.buffer {
            pass.set_pipeline(pipeline);
            pass.set_vertex_buffer(0, b.slice(..));
            pass.draw(0..self.count, 0..1);
        }
    }
}

struct Target {
    size: [u32; 2],
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolved: wgpu::TextureView,
    blit_group: wgpu::BindGroup,
}

/// The GPU resources of the viewport, kept in egui's callback resources.
pub struct Resources {
    mesh_opaque: wgpu::RenderPipeline,
    mesh_transparent: wgpu::RenderPipeline,
    lines: wgpu::RenderPipeline,
    lines_on_top: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    blit_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    uniforms: wgpu::Buffer,
    uniform_group: wgpu::BindGroup,
    target: Option<Target>,
    scene_id: u64,
    opaque: Buffer,
    transparent: Buffer,
    transparent_data: Vec<GpuVertex>,
    sorted_for: Option<[f32; 3]>,
    edges: Buffer,
    markers: Buffer,
    axes: Buffer,
    highlight: Buffer,
    highlight_faces: Buffer,
}

impl Resources {
    /// Creates the pipelines; `target_format` is egui's.
    pub fn new(device: &wgpu::Device, target_format: wgpu::TextureFormat) -> Resources {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport shader"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let uniform_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport uniforms"),
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
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport layout"),
            bind_group_layouts: &[Some(&uniform_layout)],
            immediate_size: 0,
        });
        let mesh_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4];
        let line_attributes = wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4];
        let mesh_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuVertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &mesh_attributes,
        })];
        let line_buffers = [Some(wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GpuLine>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &line_attributes,
        })];
        let blend = Some(wgpu::BlendState::ALPHA_BLENDING);
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[Option<wgpu::VertexBufferLayout>],
                        topology: wgpu::PrimitiveTopology,
                        depth_write: bool,
                        depth_test: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    compilation_options: Default::default(),
                    buffers,
                },
                primitive: wgpu::PrimitiveState { topology, cull_mode: None, ..Default::default() },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(if !depth_test {
                        wgpu::CompareFunction::Always
                    } else if depth_write {
                        wgpu::CompareFunction::LessEqual
                    } else {
                        // translucent faces do not cover the outlines drawn at their depth
                        wgpu::CompareFunction::Less
                    }),
                    stencil: wgpu::StencilState::default(),
                    bias: wgpu::DepthBiasState::default(),
                }),
                multisample: wgpu::MultisampleState { count: SAMPLES, ..Default::default() },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
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
        };
        let tri = wgpu::PrimitiveTopology::TriangleList;
        let line = wgpu::PrimitiveTopology::LineList;
        let mesh_opaque = pipeline("viewport opaque", "vs_mesh", "fs_mesh", &mesh_buffers, tri, true, true);
        let mesh_transparent = pipeline("viewport transparent", "vs_mesh", "fs_mesh", &mesh_buffers, tri, false, true);
        let lines = pipeline("viewport lines", "vs_line", "fs_line", &line_buffers, line, true, true);
        let lines_on_top = pipeline("viewport lines on top", "vs_line", "fs_line", &line_buffers, line, false, false);

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("viewport blit"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport blit layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });
        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("viewport blit"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_blit"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_blit"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: target_format,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("viewport sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("viewport uniforms"),
            size: std::mem::size_of::<Uniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let uniform_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport uniforms"),
            layout: &uniform_layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: uniforms.as_entire_binding() }],
        });
        Resources {
            mesh_opaque,
            mesh_transparent,
            lines,
            lines_on_top,
            blit,
            blit_layout,
            sampler,
            uniforms,
            uniform_group,
            target: None,
            scene_id: u64::MAX,
            opaque: Buffer::empty(),
            transparent: Buffer::empty(),
            transparent_data: Vec::new(),
            sorted_for: None,
            edges: Buffer::empty(),
            markers: Buffer::empty(),
            axes: Buffer::empty(),
            highlight: Buffer::empty(),
            highlight_faces: Buffer::empty(),
        }
    }

    fn ensure_target(&mut self, device: &wgpu::Device, size: [u32; 2]) {
        if self.target.as_ref().is_some_and(|t| t.size == size) {
            return;
        }
        let texture = |label: &str, format: wgpu::TextureFormat, samples: u32, usage: wgpu::TextureUsages| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        let msaa = texture("viewport msaa", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let depth = texture("viewport depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT);
        let resolved = texture(
            "viewport image",
            COLOR_FORMAT,
            1,
            wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        );
        let blit_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("viewport blit"),
            layout: &self.blit_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&resolved) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
            ],
        });
        self.target = Some(Target { size, msaa, depth, resolved, blit_group });
    }

    fn upload_scene(&mut self, device: &wgpu::Device, id: u64, scene: &Scene) {
        if self.scene_id == id {
            return;
        }
        self.scene_id = id;
        self.opaque = Buffer::upload(device, &mesh_vertices(&scene.opaque));
        self.transparent_data = mesh_vertices(&scene.transparent);
        self.transparent = Buffer::upload(device, &self.transparent_data);
        self.sorted_for = None;
        self.edges = Buffer::upload(device, &line_vertices(&scene.edges));
        self.markers = Buffer::upload(device, &line_vertices(&scene.markers));
        self.axes = Buffer::upload(device, &line_vertices(&scene.axes));
    }

    /// Back-to-front order of the translucent triangles for the eye.
    fn sort_transparent(&mut self, queue: &wgpu::Queue, eye: [f32; 3]) {
        if self.sorted_for == Some(eye) || self.transparent.buffer.is_none() {
            return;
        }
        self.sorted_for = Some(eye);
        let distance = |t: &[GpuVertex]| {
            let c: [f32; 3] = std::array::from_fn(|i| (t[0].position[i] + t[1].position[i] + t[2].position[i]) / 3.0);
            (c[0] - eye[0]).powi(2) + (c[1] - eye[1]).powi(2) + (c[2] - eye[2]).powi(2)
        };
        let mut triangles: Vec<(f32, [GpuVertex; 3])> =
            self.transparent_data.as_chunks::<3>().0.iter().map(|t| (distance(t), *t)).collect();
        triangles.sort_by(|a, b| b.0.total_cmp(&a.0));
        let sorted: Vec<GpuVertex> = triangles.into_iter().flat_map(|(_, t)| t).collect();
        if let Some(b) = &self.transparent.buffer {
            queue.write_buffer(b, 0, bytemuck::cast_slice(&sorted));
        }
    }
}

/// One frame of the viewport.
pub struct ViewportCallback {
    pub scene: Arc<Scene>,
    pub scene_id: u64,
    pub view_projection: Mat4,
    pub eye: [f32; 3],
    pub light: [f32; 3],
    pub background: [f64; 3],
    pub highlight: Vec<LineVertex>,
    /// Translucent faces over the scene (highlighted panels).
    pub highlight_faces: Vec<Vertex>,
    /// The viewport in physical pixels.
    pub size: [u32; 2],
}

impl CallbackTrait for ViewportCallback {
    fn prepare(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        screen: &ScreenDescriptor,
        encoder: &mut wgpu::CommandEncoder,
        resources: &mut CallbackResources,
    ) -> Vec<wgpu::CommandBuffer> {
        let Some(r) = resources.get_mut::<Resources>() else { return Vec::new() };
        let _ = screen;
        r.ensure_target(device, self.size);
        r.upload_scene(device, self.scene_id, &self.scene);
        r.sort_transparent(queue, self.eye);
        r.highlight = Buffer::upload(device, &line_vertices(&self.highlight));
        r.highlight_faces = Buffer::upload(device, &mesh_vertices(&self.highlight_faces));
        queue.write_buffer(
            &r.uniforms,
            0,
            bytemuck::bytes_of(&Uniforms {
                view_projection: self.view_projection,
                light: [self.light[0], self.light[1], self.light[2], 0.0],
            }),
        );
        let Some(target) = &r.target else { return Vec::new() };
        let [cr, cg, cb] = self.background;
        let mut pass = encoder
            .begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &target.msaa,
                    depth_slice: None,
                    resolve_target: Some(&target.resolved),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: cr, g: cg, b: cb, a: 1.0 }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &target.depth,
                    depth_ops: Some(wgpu::Operations { load: wgpu::LoadOp::Clear(1.0), store: wgpu::StoreOp::Discard }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            })
            .forget_lifetime();
        pass.set_bind_group(0, &r.uniform_group, &[]);
        r.opaque.draw(&mut pass, &r.mesh_opaque);
        r.edges.draw(&mut pass, &r.lines);
        r.transparent.draw(&mut pass, &r.mesh_transparent);
        r.highlight_faces.draw(&mut pass, &r.mesh_transparent);
        r.markers.draw(&mut pass, &r.lines);
        r.axes.draw(&mut pass, &r.lines_on_top);
        r.highlight.draw(&mut pass, &r.lines_on_top);
        Vec::new()
    }

    fn paint(
        &self,
        _info: egui::PaintCallbackInfo,
        pass: &mut wgpu::RenderPass<'static>,
        resources: &CallbackResources,
    ) {
        let Some(r) = resources.get::<Resources>() else { return };
        let Some(target) = &r.target else { return };
        pass.set_pipeline(&r.blit);
        pass.set_bind_group(0, &target.blit_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Registers the viewport's GPU resources with egui's renderer.
pub fn install(render_state: &egui_wgpu::RenderState) {
    let resources = Resources::new(&render_state.device, render_state.target_format);
    render_state.renderer.write().callback_resources.insert(resources);
}
