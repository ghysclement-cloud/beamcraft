//! # bc-viewer — la vue 3D de BEAMCraft
//!
//! Dessine un [`bc_kernel::Model`] dans une texture hors écran (avec anticrénelage
//! MSAA ×4 et tampon de profondeur). L'application affiche ensuite cette texture
//! dans son interface. Ce crate ne connaît rien de l'interface : il peut aussi
//! servir à faire des captures d'écran dans les tests.

mod camera;

pub use camera::{Camera, StandardView};

use bc_kernel::{Aabb, Model};
use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

pub use wgpu;

/// Format dans lequel on dessine (les couleurs sont encodées en sRGB).
pub const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
/// Format de stockage de la texture finale : mêmes octets, mais lus « tels quels »
/// par l'interface (egui travaille en espace gamma). Sans ça, l'image paraît trop sombre.
pub const OUTPUT_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
const MSAA: u32 = 4;

/// Couleurs de la vue, pilotées par le thème de l'application.
#[derive(Debug, Clone, Copy)]
pub struct ViewStyle {
    pub background: [f32; 3],
    pub edge: [f32; 4],
    pub grid: [f32; 4],
    pub highlight: [f32; 3],
}

impl ViewStyle {
    pub fn dark() -> Self {
        ViewStyle {
            background: srgb(0x12, 0x16, 0x1B),
            edge: [0.03, 0.04, 0.05, 1.0],
            grid: rgba(srgb(0x2A, 0x31, 0x3A)),
            highlight: srgb(0x4C, 0x9A, 0xFF),
        }
    }
    pub fn light() -> Self {
        ViewStyle {
            background: srgb(0xED, 0xF0, 0xF4),
            edge: [0.10, 0.12, 0.15, 1.0],
            grid: rgba(srgb(0xC6, 0xCD, 0xD5)),
            highlight: srgb(0x1F, 0x6F, 0xD6),
        }
    }
}

fn rgba(c: [f32; 3]) -> [f32; 4] {
    [c[0], c[1], c[2], 1.0]
}

/// Convertit une couleur sRGB 8 bits en valeurs linéaires (ce qu'attend le GPU).
fn srgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let f = |c: u8| {
        let c = c as f32 / 255.0;
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(r), f(g), f(b)]
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    eye: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct MeshVertex {
    position: [f32; 3],
    normal: [f32; 3],
    color: [f32; 3],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct LineVertex {
    position: [f32; 3],
    color: [f32; 4],
}

struct GpuPart {
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    edges: wgpu::Buffer,
    edge_count: u32,
    uniform: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    visible: bool,
    highlighted: bool,
}

struct Targets {
    size: [u32; 2],
    msaa_color: wgpu::TextureView,
    depth: wgpu::TextureView,
    resolve: wgpu::Texture,
    /// Vue sRGB : cible du rendu.
    resolve_view: wgpu::TextureView,
    /// Vue « brute » : ce qu'on donne à l'interface.
    output_view: wgpu::TextureView,
}

/// La vue 3D.
pub struct Viewport {
    pub camera: Camera,
    pub style: ViewStyle,
    mesh_pipeline: wgpu::RenderPipeline,
    line_pipeline: wgpu::RenderPipeline,
    globals_buf: wgpu::Buffer,
    globals_bg: wgpu::BindGroup,
    part_layout: wgpu::BindGroupLayout,
    parts: Vec<GpuPart>,
    grid: Option<(wgpu::Buffer, u32)>,
    grid_edge_color: [f32; 4],
    bounds: Aabb,
    targets: Option<Targets>,
}

impl Viewport {
    pub fn new(device: &wgpu::Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bc-viewer shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let uniform_entry = |vis| wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: vis,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        };
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[uniform_entry(wgpu::ShaderStages::VERTEX_FRAGMENT)],
        });
        let part_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("part"),
            entries: &[uniform_entry(wgpu::ShaderStages::FRAGMENT)],
        });

        let globals_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let globals_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &globals_layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });

        let mesh_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("mesh"),
            bind_group_layouts: &[Some(&globals_layout), Some(&part_layout)],
            ..Default::default()
        });
        let line_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("line"),
            bind_group_layouts: &[Some(&globals_layout)],
            ..Default::default()
        });

        let multisample = wgpu::MultisampleState {
            count: MSAA,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let color_target = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(wgpu::BlendState::ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let mesh_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("mesh"),
            layout: Some(&mesh_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_mesh"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<MeshVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x3],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_mesh"),
                compilation_options: Default::default(),
                targets: &color_target,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                cull_mode: None,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::Less),
                stencil: Default::default(),
                // Recule légèrement les surfaces pour que les arêtes restent visibles.
                bias: wgpu::DepthBiasState {
                    constant: 2,
                    slope_scale: 1.5,
                    clamp: 0.0,
                },
            }),
            multisample,
            multiview_mask: None,
            cache: None,
        });

        let line_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("line"),
            layout: Some(&line_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_line"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<LineVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x4],
                })],
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_line"),
                compilation_options: Default::default(),
                targets: &color_target,
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineList,
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(true),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample,
            multiview_mask: None,
            cache: None,
        });

        Viewport {
            camera: Camera::default(),
            style: ViewStyle::dark(),
            mesh_pipeline,
            line_pipeline,
            globals_buf,
            globals_bg,
            part_layout,
            parts: Vec::new(),
            grid: None,
            grid_edge_color: [0.0; 4],
            bounds: Aabb::EMPTY,
            targets: None,
        }
    }

    /// Envoie un modèle à la carte graphique et cadre la vue dessus.
    pub fn set_model(&mut self, device: &wgpu::Device, model: &Model) {
        self.parts.clear();
        self.grid_edge_color = self.style.edge;
        for part in &model.parts {
            let m = &part.mesh;
            let verts: Vec<MeshVertex> = (0..m.positions.len())
                .map(|i| MeshVertex {
                    position: m.positions[i],
                    normal: m.normals[i],
                    color: srgb_f(m.colors[i]),
                })
                .collect();
            let edges: Vec<LineVertex> = m
                .edges
                .iter()
                .map(|p| LineVertex {
                    position: *p,
                    color: self.style.edge,
                })
                .collect();
            let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("part vertices"),
                contents: bytemuck::cast_slice(&verts),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("part indices"),
                contents: bytemuck::cast_slice(&m.indices),
                usage: wgpu::BufferUsages::INDEX,
            });
            let placeholder = [LineVertex::zeroed(); 2];
            let edges_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("part edges"),
                contents: bytemuck::cast_slice(if edges.is_empty() { &placeholder[..] } else { &edges }),
                usage: wgpu::BufferUsages::VERTEX,
            });
            let uniform = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("part uniform"),
                contents: bytemuck::cast_slice(&[0f32; 4]),
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });
            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("part"),
                layout: &self.part_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform.as_entire_binding(),
                }],
            });
            self.parts.push(GpuPart {
                vertices,
                indices,
                index_count: m.indices.len() as u32,
                edges: edges_buf,
                edge_count: edges.len() as u32,
                uniform,
                bind_group,
                visible: true,
                highlighted: false,
            });
        }
        self.bounds = model.bounds;
        self.grid = Some(build_grid(device, &model.bounds, self.style.grid));
        self.camera.fit(&model.bounds);
    }

    /// Vide la vue.
    pub fn clear(&mut self) {
        self.parts.clear();
        self.grid = None;
        self.bounds = Aabb::EMPTY;
    }

    pub fn set_part_visible(&mut self, index: usize, visible: bool) {
        if let Some(p) = self.parts.get_mut(index) {
            p.visible = visible;
        }
    }

    pub fn set_highlight(&mut self, index: Option<usize>) {
        for (i, p) in self.parts.iter_mut().enumerate() {
            p.highlighted = Some(i) == index;
        }
    }

    pub fn fit(&mut self) {
        self.camera.fit(&self.bounds);
    }

    /// Dessine la scène dans une texture de `size` pixels et la renvoie.
    pub fn render(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, size: [u32; 2]) -> &wgpu::TextureView {
        let size = [size[0].max(1), size[1].max(1)];
        if self.targets.as_ref().map(|t| t.size) != Some(size) {
            self.targets = Some(create_targets(device, size));
        }

        // Si le thème a changé, recolorer la grille et les arêtes.
        if self.grid_edge_color != self.style.edge && !self.bounds.is_empty() {
            self.grid = Some(build_grid(device, &self.bounds, self.style.grid));
            self.grid_edge_color = self.style.edge;
            // Les arêtes gardent leur couleur d'origine (sombre) : lisible sur les deux thèmes.
        }

        let aspect = size[0] as f32 / size[1] as f32;
        let globals = Globals {
            view_proj: self.camera.view_proj(aspect).to_cols_array_2d(),
            eye: self.camera.eye().extend(1.0).to_array(),
        };
        queue.write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(&globals));
        let hl = self.style.highlight;
        for p in &self.parts {
            let tint = if p.highlighted {
                [hl[0], hl[1], hl[2], 0.55]
            } else {
                [0.0; 4]
            };
            queue.write_buffer(&p.uniform, 0, bytemuck::cast_slice(&tint));
        }

        let t = self.targets.as_ref().unwrap();
        let bg = self.style.background;
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("bc-viewer"),
        });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("scene"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &t.msaa_color,
                    resolve_target: Some(&t.resolve_view),
                    depth_slice: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg[0] as f64,
                            g: bg[1] as f64,
                            b: bg[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &t.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });

            pass.set_bind_group(0, &self.globals_bg, &[]);

            pass.set_pipeline(&self.mesh_pipeline);
            for p in self.parts.iter().filter(|p| p.visible) {
                pass.set_bind_group(1, &p.bind_group, &[]);
                pass.set_vertex_buffer(0, p.vertices.slice(..));
                pass.set_index_buffer(p.indices.slice(..), wgpu::IndexFormat::Uint32);
                pass.draw_indexed(0..p.index_count, 0, 0..1);
            }

            pass.set_pipeline(&self.line_pipeline);
            if let Some((grid, n)) = &self.grid {
                pass.set_vertex_buffer(0, grid.slice(..));
                pass.draw(0..*n, 0..1);
            }
            for p in self.parts.iter().filter(|p| p.visible && p.edge_count > 0) {
                pass.set_vertex_buffer(0, p.edges.slice(..));
                pass.draw(0..p.edge_count, 0..1);
            }
        }
        queue.submit(Some(encoder.finish()));
        &self.targets.as_ref().unwrap().output_view
    }

    /// La texture finale (pour une capture d'écran par exemple).
    pub fn output_texture(&self) -> Option<&wgpu::Texture> {
        self.targets.as_ref().map(|t| &t.resolve)
    }
}

/// Couleur sRGB (0..1) → linéaire.
fn srgb_f(c: [f32; 3]) -> [f32; 3] {
    let f = |c: f32| {
        if c <= 0.04045 {
            c / 12.92
        } else {
            ((c + 0.055) / 1.055).powf(2.4)
        }
    };
    [f(c[0]), f(c[1]), f(c[2])]
}

fn create_targets(device: &wgpu::Device, size: [u32; 2]) -> Targets {
    let extent = wgpu::Extent3d {
        width: size[0],
        height: size[1],
        depth_or_array_layers: 1,
    };
    let tex = |label, format, samples, usage, view_formats: &[wgpu::TextureFormat]| {
        device.create_texture(&wgpu::TextureDescriptor {
            label: Some(label),
            size: extent,
            mip_level_count: 1,
            sample_count: samples,
            dimension: wgpu::TextureDimension::D2,
            format,
            usage,
            view_formats,
        })
    };
    let msaa_color = tex(
        "msaa color",
        COLOR_FORMAT,
        MSAA,
        wgpu::TextureUsages::RENDER_ATTACHMENT,
        &[],
    )
    .create_view(&Default::default());
    let depth =
        tex("depth", DEPTH_FORMAT, MSAA, wgpu::TextureUsages::RENDER_ATTACHMENT, &[]).create_view(&Default::default());
    let resolve = tex(
        "resolve",
        OUTPUT_FORMAT,
        1,
        wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        &[COLOR_FORMAT],
    );
    let resolve_view = resolve.create_view(&wgpu::TextureViewDescriptor {
        format: Some(COLOR_FORMAT),
        ..Default::default()
    });
    let output_view = resolve.create_view(&Default::default());
    Targets {
        size,
        msaa_color,
        depth,
        resolve,
        resolve_view,
        output_view,
    }
}

/// Grille au sol sous le modèle, avec un pas « rond » (100, 500, 1 000 mm…).
fn build_grid(device: &wgpu::Device, b: &Aabb, color: [f32; 4]) -> (wgpu::Buffer, u32) {
    let s = b.size();
    let extent = s[0].max(s[1]).max(1000.0) * 0.75 + 1000.0;
    let step = [100.0f32, 250.0, 500.0, 1000.0, 2000.0, 5000.0]
        .into_iter()
        .find(|st| extent * 2.0 / st <= 40.0)
        .unwrap_or(10000.0);
    let c = b.center();
    let z = b.min[2];
    let (cx, cy) = ((c[0] / step).round() * step, (c[1] / step).round() * step);
    let n = (extent / step).ceil() as i32;
    let half = n as f32 * step;
    let mut v = Vec::new();
    for i in -n..=n {
        let o = i as f32 * step;
        let k = if color[0] > 0.3 { 0.55 } else { 1.8 }; // axe central plus marqué
        let col = if i == 0 {
            [color[0] * k, color[1] * k, color[2] * k, 1.0]
        } else {
            color
        };
        v.push(LineVertex {
            position: [cx + o, cy - half, z],
            color: col,
        });
        v.push(LineVertex {
            position: [cx + o, cy + half, z],
            color: col,
        });
        v.push(LineVertex {
            position: [cx - half, cy + o, z],
            color: col,
        });
        v.push(LineVertex {
            position: [cx + half, cy + o, z],
            color: col,
        });
    }
    let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: Some("grid"),
        contents: bytemuck::cast_slice(&v),
        usage: wgpu::BufferUsages::VERTEX,
    });
    (buf, v.len() as u32)
}
