// SPDX-License-Identifier: GPL-3.0-or-later
// Copyright (C) 2026 Lucas Bernardo
//! Renderizador do viewport: desenha a cena em uma textura própria (MSAA 4x + profundidade)
//! que depois é exibida pela interface.

use bytemuck::{Pod, Zeroable};
use eframe::egui_wgpu;
use eframe::wgpu;
use eframe::wgpu::util::DeviceExt;
use glam::Vec3;

use crate::geom::{EdgeId, FaceId, Mesh};

/// Seleção de faces/arestas dentro de uma malha.
#[derive(Default)]
pub struct Selection {
    pub edges: std::collections::HashSet<EdgeId>,
    pub faces: std::collections::HashSet<FaceId>,
}

/// Uma parte do modelo a desenhar (malha ativa, um grupo, o contexto esmaecido...).
pub struct Part<'a> {
    pub mesh: &'a Mesh,
    /// Seleção dentro desta malha (só a malha ativa).
    pub sel: Option<&'a Selection>,
    /// Grupo selecionado inteiro.
    pub all_selected: bool,
    /// Fora do contexto de edição (desenhado claro).
    pub faded: bool,
    /// Material do grupo, para faces sem material próprio.
    pub material: Option<[f32; 3]>,
    /// Textura do grupo, para faces sem material próprio.
    pub texture: Option<u32>,
    /// Mundo → coordenadas locais do objeto (a textura acompanha o grupo ao mover).
    pub uv_xf: glam::DAffine3,
}

/// Cor padrão da face da frente (sRGB).
pub const DEFAULT_FACE: [f32; 3] = [0.96, 0.96, 0.94];

const SAMPLES: u32 = 4;
const COLOR_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
pub struct Globals {
    pub view_proj: [[f32; 4]; 4],
    pub eye: [f32; 4],
    pub target: [f32; 4],
    pub viewport: [f32; 4],
    pub light_dir: [f32; 4],
    pub bias: [f32; 4],
    pub back_color: [f32; 4],
    pub bg_top: [f32; 4],
    pub bg_bottom: [f32; 4],
    pub shading: [f32; 4],
    /// Plano de seção (normal, d); ativo quando `shading.w` = 1.
    pub clip: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct FaceVertex {
    pos: [f32; 3],
    normal: [f32; 3],
    color: [f32; 4],
    uv: [f32; 2],
    /// 1 = face com textura.
    textured: f32,
}

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable, Debug)]
pub struct LineInstance {
    pub a: [f32; 3],
    pub b: [f32; 3],
    pub color: [f32; 4],
    /// largura (pt), sinal do bias (+1 frente / -1 atrás), tracejado (pt, 0 = contínuo), fade (0/1)
    pub params: [f32; 4],
}

impl LineInstance {
    pub fn new(a: Vec3, b: Vec3, srgb: [f32; 4], width: f32, bias: f32, dash: f32, fade: bool) -> Self {
        Self {
            a: a.into(),
            b: b.into(),
            color: srgb_to_linear4(srgb),
            params: [width, bias, dash, if fade { 1.0 } else { 0.0 }],
        }
    }
}

pub fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

pub fn srgb_to_linear4(c: [f32; 4]) -> [f32; 4] {
    [srgb_to_linear(c[0]), srgb_to_linear(c[1]), srgb_to_linear(c[2]), c[3]]
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum FaceStyle {
    Wireframe,
    HiddenLine,
    Shaded,
    ShadedTextured,
}

/// Buffer de GPU que cresce conforme a necessidade.
struct DynBuffer {
    buf: wgpu::Buffer,
    capacity: u64,
    len: u32,
    usage: wgpu::BufferUsages,
    label: &'static str,
}

impl DynBuffer {
    fn new(device: &wgpu::Device, label: &'static str, usage: wgpu::BufferUsages) -> Self {
        let capacity = 4096;
        Self {
            buf: device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: capacity,
                usage: usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            }),
            capacity,
            len: 0,
            usage,
            label,
        }
    }

    fn upload<T: Pod>(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, data: &[T]) {
        // Nunca passa do limite da placa de vídeo: o excedente deixa de ser desenhado.
        let max = device.limits().max_buffer_size;
        let fit = data.len().min((max / size_of::<T>().max(1) as u64) as usize);
        let data = &data[..fit];
        let bytes: &[u8] = bytemuck::cast_slice(data);
        if bytes.len() as u64 > self.capacity {
            self.capacity = (bytes.len() as u64).next_power_of_two().min(max / 4 * 4);
            self.buf = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(self.label),
                size: self.capacity,
                usage: self.usage | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
        }
        if !bytes.is_empty() {
            queue.write_buffer(&self.buf, 0, bytes);
        }
        self.len = data.len() as u32;
    }
}

struct Targets {
    size: [u32; 2],
    msaa_view: wgpu::TextureView,
    resolve_view: wgpu::TextureView,
    depth_view: wgpu::TextureView,
}

impl Targets {
    fn new(device: &wgpu::Device, size: [u32; 2]) -> Self {
        let extent = wgpu::Extent3d {
            width: size[0],
            height: size[1],
            depth_or_array_layers: 1,
        };
        let tex = |label, format, samples, usage| {
            device
                .create_texture(&wgpu::TextureDescriptor {
                    label: Some(label),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: samples,
                    dimension: wgpu::TextureDimension::D2,
                    format,
                    usage,
                    view_formats: &[],
                })
                .create_view(&wgpu::TextureViewDescriptor::default())
        };
        Self {
            size,
            msaa_view: tex("viewport_msaa", COLOR_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT),
            resolve_view: tex(
                "viewport_color",
                COLOR_FORMAT,
                1,
                wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
            ),
            depth_view: tex("viewport_depth", DEPTH_FORMAT, SAMPLES, wgpu::TextureUsages::RENDER_ATTACHMENT),
        }
    }
}

pub struct ViewportRenderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    globals_buf: wgpu::Buffer,
    bind_group: wgpu::BindGroup,
    bg_pipeline: wgpu::RenderPipeline,
    face_pipeline: wgpu::RenderPipeline,
    face_xray_pipeline: wgpu::RenderPipeline,
    hidden_line_pipeline: wgpu::RenderPipeline,
    tex_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// Textura branca 1×1 (faces sem textura).
    white: wgpu::BindGroup,
    /// Texturas já enviadas à GPU, por conteúdo.
    tex_cache: std::collections::HashMap<u64, wgpu::BindGroup>,
    /// Trechos do buffer de faces por textura (None = sem textura).
    batches: Vec<(Option<u64>, std::ops::Range<u32>)>,
    line_pipeline: wgpu::RenderPipeline,
    faces: DynBuffer,
    /// Arestas do modelo (frente) e linhas auxiliares (grade/eixos).
    edges: DynBuffer,
    overlay: DynBuffer,
    targets: Option<Targets>,
    texture_id: Option<egui::TextureId>,
}

use eframe::egui;

impl ViewportRenderer {
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        let device = rs.device.clone();
        let queue = rs.queue.clone();

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("viewport_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let globals_buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("globals"),
            contents: bytemuck::bytes_of(&Globals::zeroed()),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        });
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals_layout"),
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
            label: Some("globals_bg"),
            layout: &bgl,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: globals_buf.as_entire_binding(),
            }],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("viewport_layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let tex_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("texture_layout"),
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
        let face_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("face_layout"),
            bind_group_layouts: &[Some(&bgl), Some(&tex_layout)],
            immediate_size: 0,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("texture_sampler"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });

        let depth = |write: bool, compare: wgpu::CompareFunction| wgpu::DepthStencilState {
            format: DEPTH_FORMAT,
            depth_write_enabled: Some(write),
            depth_compare: Some(compare),
            stencil: wgpu::StencilState::default(),
            bias: wgpu::DepthBiasState::default(),
        };
        let multisample = wgpu::MultisampleState {
            count: SAMPLES,
            mask: !0,
            alpha_to_coverage_enabled: false,
        };
        let primitive = wgpu::PrimitiveState {
            topology: wgpu::PrimitiveTopology::TriangleList,
            cull_mode: None,
            ..Default::default()
        };
        let opaque = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: None,
            write_mask: wgpu::ColorWrites::ALL,
        })];
        let blended = [Some(wgpu::ColorTargetState {
            format: COLOR_FORMAT,
            blend: Some(wgpu::BlendState::PREMULTIPLIED_ALPHA_BLENDING),
            write_mask: wgpu::ColorWrites::ALL,
        })];

        let bg_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("background"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_bg"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            primitive,
            depth_stencil: Some(depth(false, wgpu::CompareFunction::Always)),
            multisample,
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_bg"),
                compilation_options: Default::default(),
                targets: &opaque,
            }),
            multiview_mask: None,
            cache: None,
        });

        let face_attrs =
            wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x2, 4 => Float32];
        let face_pipe = |label: &str, xray: bool| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&face_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_face"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<FaceVertex>() as u64,
                        step_mode: wgpu::VertexStepMode::Vertex,
                        attributes: &face_attrs,
                    })],
                },
                primitive,
                // Raio-X: faces translúcidas que não escondem nada atrás delas.
                depth_stencil: Some(depth(!xray, wgpu::CompareFunction::Greater)),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs_face"),
                    compilation_options: Default::default(),
                    targets: if xray { &blended } else { &opaque },
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let face_pipeline = face_pipe("faces", false);
        let face_xray_pipeline = face_pipe("faces_xray", true);

        let line_attrs = wgpu::vertex_attr_array![
            0 => Float32x3, 1 => Float32x3, 2 => Float32x4, 3 => Float32x4
        ];
        let line_pipe = |label: &str, fs: &str, compare: wgpu::CompareFunction| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_line"),
                    compilation_options: Default::default(),
                    buffers: &[Some(wgpu::VertexBufferLayout {
                        array_stride: std::mem::size_of::<LineInstance>() as u64,
                        step_mode: wgpu::VertexStepMode::Instance,
                        attributes: &line_attrs,
                    })],
                },
                primitive,
                depth_stencil: Some(depth(false, compare)),
                multisample,
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    compilation_options: Default::default(),
                    targets: &blended,
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let line_pipeline = line_pipe("lines", "fs_line", wgpu::CompareFunction::Greater);
        // Arestas posteriores: só o que está atrás de alguma face (profundidade menor).
        let hidden_line_pipeline = line_pipe("hidden_lines", "fs_line_hidden", wgpu::CompareFunction::Less);

        Self {
            faces: DynBuffer::new(&device, "faces", wgpu::BufferUsages::VERTEX),
            edges: DynBuffer::new(&device, "edges", wgpu::BufferUsages::VERTEX),
            overlay: DynBuffer::new(&device, "overlay", wgpu::BufferUsages::VERTEX),
            white: make_texture_bind_group(&device, &queue, &tex_layout, &sampler, &crate::textures::Image::new(1, 1, vec![255; 4])),
            tex_layout,
            sampler,
            tex_cache: Default::default(),
            batches: Vec::new(),
            device,
            queue,
            globals_buf,
            bind_group,
            bg_pipeline,
            face_pipeline,
            face_xray_pipeline,
            hidden_line_pipeline,
            line_pipeline,
            targets: None,
            texture_id: None,
        }
    }

    /// Envia a geometria da malha para a GPU (quando ela ou a seleção mudam).
    /// Envia a geometria de todas as partes do modelo para a GPU (quando algo muda).
    pub fn upload(&mut self, parts: &[Part], textures: &[crate::textures::Texture]) {
        // Faces agrupadas por textura (um desenho por textura).
        let mut groups: std::collections::BTreeMap<Option<u32>, Vec<FaceVertex>> = Default::default();
        let mut edges = Vec::new();
        for part in parts {
            let mesh = part.mesh;
            for (id, face) in &mesh.faces {
                let n = face.normal.as_vec3().to_array();
                let own = face.material.is_some();
                let m = face.material.or(part.material).unwrap_or(DEFAULT_FACE);
                let tex = if own { face.texture } else { part.texture }.filter(|t| (*t as usize) < textures.len());
                // alfa marca o estado para o shader: 1 normal, 0,5 selecionada, 0 esmaecida.
                let alpha = if part.faded {
                    0.0
                } else if part.all_selected || part.sel.is_some_and(|s| s.faces.contains(&id)) {
                    0.5
                } else {
                    1.0
                };
                let c = srgb_to_linear4([m[0], m[1], m[2], 1.0]);
                let c = [c[0], c[1], c[2], alpha];
                // UV nas coordenadas locais do objeto (a textura acompanha o grupo).
                let ln = (part.uv_xf.matrix3.inverse().transpose() * face.normal).normalize_or(face.normal);
                let size = tex.map(|t| textures[t as usize].size).unwrap_or([1.0, 1.0]);
                let out = groups.entry(tex).or_default();
                for tri in mesh.face_triangles(id) {
                    for p in tri {
                        let uv = crate::textures::uv(part.uv_xf.transform_point3(p), ln, size);
                        out.push(FaceVertex {
                            pos: p.as_vec3().to_array(),
                            normal: n,
                            color: c,
                            uv,
                            textured: if tex.is_some() { 1.0 } else { 0.0 },
                        });
                    }
                }
            }
            for e in mesh.edges.keys() {
                if mesh.edges[e].soft {
                    continue;
                }
                let (a, b) = mesh.edge_points(e);
                let (a, b) = (a.as_vec3(), b.as_vec3());
                let selected = part.all_selected || part.sel.is_some_and(|s| s.edges.contains(&e));
                edges.push(if part.faded {
                    LineInstance::new(a, b, [0.62, 0.62, 0.62, 0.7], 1.0, 1.0, 0.0, false)
                } else if selected {
                    LineInstance::new(a, b, [0.1, 0.35, 1.0, 1.0], 2.4, 1.0, 0.0, false)
                } else {
                    LineInstance::new(a, b, [0.0, 0.0, 0.0, 1.0], 1.3, 1.0, 0.0, false)
                });
            }
        }
        let mut verts = Vec::new();
        self.batches.clear();
        for (tex, vs) in groups {
            let start = verts.len() as u32;
            verts.extend(vs);
            let key = tex.map(|t| {
                let texture = &textures[t as usize];
                if !self.tex_cache.contains_key(&texture.uid) {
                    let bg = make_texture_bind_group(&self.device, &self.queue, &self.tex_layout, &self.sampler, &texture.image);
                    self.tex_cache.insert(texture.uid, bg);
                }
                texture.uid
            });
            self.batches.push((key, start..verts.len() as u32));
        }
        self.faces.upload(&self.device, &self.queue, &verts);
        self.edges.upload(&self.device, &self.queue, &edges);
    }

    pub fn triangle_count(&self) -> u32 {
        self.faces.len / 3
    }

    pub fn set_overlay(&mut self, lines: &[LineInstance]) {
        self.overlay.upload(&self.device, &self.queue, lines);
    }

    /// Renderiza e devolve a textura para exibir na interface.
    pub fn render(
        &mut self,
        egui_renderer: &mut egui_wgpu::Renderer,
        size_px: [u32; 2],
        globals: &Globals,
        style: FaceStyle,
        xray: bool,
        back_edges: bool,
    ) -> egui::TextureId {
        let size = [size_px[0].max(1), size_px[1].max(1)];
        if self.targets.as_ref().map(|t| t.size) != Some(size) {
            let targets = Targets::new(&self.device, size);
            match self.texture_id {
                Some(id) => egui_renderer.update_egui_texture_from_wgpu_texture(
                    &self.device,
                    &targets.resolve_view,
                    wgpu::FilterMode::Nearest,
                    id,
                ),
                None => {
                    self.texture_id = Some(egui_renderer.register_native_texture(
                        &self.device,
                        &targets.resolve_view,
                        wgpu::FilterMode::Nearest,
                    ))
                }
            }
            self.targets = Some(targets);
        }
        let targets = self.targets.as_ref().expect("targets criados acima");

        self.queue
            .write_buffer(&self.globals_buf, 0, bytemuck::bytes_of(globals));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                label: Some("viewport"),
            });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("viewport_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &targets.msaa_view,
                    depth_slice: None,
                    resolve_target: Some(&targets.resolve_view),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::WHITE),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &targets.depth_view,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(0.0), // Z reverso: 0 = infinito
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_bind_group(0, &self.bind_group, &[]);

            pass.set_pipeline(&self.bg_pipeline);
            pass.draw(0..3, 0..1);

            if style != FaceStyle::Wireframe && self.faces.len > 0 {
                pass.set_pipeline(if xray { &self.face_xray_pipeline } else { &self.face_pipeline });
                pass.set_vertex_buffer(0, self.faces.buf.slice(..));
                for (key, range) in &self.batches {
                    let bg = key.and_then(|k| self.tex_cache.get(&k)).unwrap_or(&self.white);
                    pass.set_bind_group(1, bg, &[]);
                    pass.draw(range.clone(), 0..1);
                }
            }

            if back_edges && !xray && style != FaceStyle::Wireframe && self.edges.len > 0 {
                pass.set_pipeline(&self.hidden_line_pipeline);
                pass.set_vertex_buffer(0, self.edges.buf.slice(..));
                pass.draw(0..6, 0..self.edges.len);
            }

            pass.set_pipeline(&self.line_pipeline);
            if self.overlay.len > 0 {
                pass.set_vertex_buffer(0, self.overlay.buf.slice(..));
                pass.draw(0..6, 0..self.overlay.len);
            }
            if self.edges.len > 0 {
                pass.set_vertex_buffer(0, self.edges.buf.slice(..));
                pass.draw(0..6, 0..self.edges.len);
            }
        }
        self.queue.submit([encoder.finish()]);

        self.texture_id.expect("textura registrada acima")
    }
}

/// Envia uma imagem (com mipmaps) e monta o grupo de recursos do shader.
fn make_texture_bind_group(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    layout: &wgpu::BindGroupLayout,
    sampler: &wgpu::Sampler,
    img: &crate::textures::Image,
) -> wgpu::BindGroup {
    let mips = crate::textures::mipmaps(img);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("material_texture"),
        size: wgpu::Extent3d { width: img.w, height: img.h, depth_or_array_layers: 1 },
        mip_level_count: mips.len() as u32,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    for (level, (w, h, data)) in mips.iter().enumerate() {
        queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &texture,
                mip_level: level as u32,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            data,
            wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(4 * w), rows_per_image: Some(*h) },
            wgpu::Extent3d { width: *w, height: *h, depth_or_array_layers: 1 },
        );
    }
    let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("material_texture_bg"),
        layout,
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view) },
            wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(sampler) },
        ],
    })
}
