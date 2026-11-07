//! wgpu pipeline for canvas batches, plus offscreen rendering for screenshots and PNG export.

use super::blend::{blend_key, blend_state};
use super::mesh::Vertex;
use super::layers::{self, Layers};
use super::scene::{CanvasFrame, Group, Item, Mark};
use super::tess;
use crate::assets;
use crate::geom::Rect;
use std::collections::HashMap;
use std::time::Duration;
use web_time::Instant;

/// An image not drawn for this long leaves the GPU; drawing it again uploads it again.
const IMAGE_IDLE: Duration = Duration::from_secs(60);
use std::sync::Arc;
use wgpu::util::DeviceExt;

pub const MSAA: u32 = 4;

/// Colors are authored in sRGB and blended in gamma space (like egui), so targets are non-sRGB.
pub const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

const SHADER: &str = r#"
struct Globals {
    size_px: vec2<f32>,
    scale: f32,
    zoom: f32,
    pan: vec2<f32>,
    offset: vec2<f32>,
};
@group(0) @binding(0) var<uniform> g: Globals;
@group(1) @binding(0) var ramp_tex: texture_2d<f32>;
@group(1) @binding(1) var ramp_samp: sampler;
@group(2) @binding(0) var img_tex: texture_2d<f32>;
@group(2) @binding(1) var img_samp: sampler;

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) color: vec4<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) @interpolate(flat) kind: u32,
};

@vertex
fn vs(@location(0) pos: vec2<f32>, @location(1) color: vec4<f32>, @location(2) uv: vec2<f32>,
      @location(3) kind: u32) -> VsOut {
    let logical = (pos - g.pan) * g.zoom + g.offset;
    let px = logical * g.scale;
    var o: VsOut;
    o.pos = vec4<f32>(px.x / g.size_px.x * 2.0 - 1.0, 1.0 - px.y / g.size_px.y * 2.0, 0.0, 1.0);
    o.color = color;
    o.uv = uv;
    o.kind = kind;
    return o;
}

fn ramp(t: f32, row: u32) -> vec4<f32> {
    let v = (f32(row) + 0.5) / 256.0;
    return textureSampleLevel(ramp_tex, ramp_samp, vec2<f32>(clamp(t, 0.0, 1.0), v), 0.0);
}

@fragment
fn fs(i: VsOut) -> @location(0) vec4<f32> {
    let k = i.kind & 255u;
    let row = i.kind >> 8u;
    // Derivatives must be taken in uniform control flow (WebGPU rejects textureSample under `k`).
    let duv_dx = dpdx(i.uv);
    let duv_dy = dpdy(i.uv);
    var c = i.color;
    if (k == 1u) {
        let r = ramp(i.uv.x, row);
        c = vec4<f32>(r.rgb, r.a * i.color.a);
    } else if (k == 2u) {
        let r = ramp(length(i.uv), row);
        c = vec4<f32>(r.rgb, r.a * i.color.a);
    } else if (k >= 3u) {
        if (k == 3u && (i.uv.x < 0.0 || i.uv.y < 0.0 || i.uv.x > 1.0 || i.uv.y > 1.0)) {
            discard;
        }
        let t = textureSampleGrad(img_tex, img_samp, i.uv, duv_dx, duv_dy);
        c = vec4<f32>(t.rgb, t.a * i.color.a);
    }
    // Premultiplied output; every pipeline blends premultiplied.
    return vec4<f32>(c.rgb * c.a, c.a);
}
"#;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Globals {
    size_px: [f32; 2],
    scale: f32,
    zoom: f32,
    pan: [f32; 2],
    offset: [f32; 2],
}

/// (index range, world clip, blend key, image) of one cached world draw.
type WorldDraw = (std::ops::Range<u32>, Option<Rect>, usize, Option<String>);

/// One step of the cached world scene: a draw, or the start or end of an offscreen layer.
enum WorldCmd {
    Draw(WorldDraw),
    /// The group, the clip it sits in and the world bounds of what it draws.
    Begin(Arc<Group>, Option<Rect>, Rect),
    End,
}

pub(super) struct Draw {
    range: std::ops::Range<u32>,
    /// Screen-space scissor (logical px), already intersected with the viewport.
    clip: Option<Rect>,
    blend: usize,
    image: Option<String>,
    world: bool,
}

/// A full-screen triangle that composites a layer (scissored to `clip`, logical px).
pub(super) struct Quad {
    pub bg: wgpu::BindGroup,
    pub pipeline: wgpu::RenderPipeline,
    pub clip: Rect,
}

pub(super) enum Op {
    Draw(Draw),
    Quad(Quad),
}

/// A layer about to be drawn: how to composite it, in screen terms.
pub(super) struct GroupRun {
    pub group: Arc<Group>,
    /// Area the composite covers (logical px, inside the viewport and the clip).
    pub rect: Rect,
    /// `rect` grown by the blur reach: the area blurs read and write.
    pub wide: Rect,
    /// Gaussian sigmas, physical px.
    pub sigma: f32,
    pub backdrop_sigma: f32,
    /// The node's shape and corner radii, physical px.
    pub shape: [f32; 4],
    pub radii: [f32; 4],
}

pub(super) enum Cmd {
    Op(Op),
    Begin(GroupRun),
    End,
}

/// Cached world-space geometry: reused while the item list is unchanged (panning, idle).
struct WorldBuffers {
    signature: Vec<(usize, Option<[u64; 4]>, usize)>,
    vb: Option<wgpu::Buffer>,
    ib: Option<wgpu::Buffer>,
    cmds: Vec<WorldCmd>,
}

pub struct CanvasRenderer {
    pub(super) pipelines: Vec<wgpu::RenderPipeline>,
    pub(super) layers: Option<Layers>,
    world_globals: wgpu::Buffer,
    screen_globals: wgpu::Buffer,
    pub(super) world_bg: wgpu::BindGroup,
    pub(super) screen_bg: wgpu::BindGroup,
    ramp_tex: wgpu::Texture,
    pub(super) ramp_bg: wgpu::BindGroup,
    ramp_version: u64,
    img_layout: wgpu::BindGroupLayout,
    img_sampler: wgpu::Sampler,
    pub(super) blank_img: wgpu::BindGroup,
    /// Uploaded images and when each was last drawn.
    pub(super) images: HashMap<String, (wgpu::BindGroup, Instant)>,
    world: Option<WorldBuffers>,
    msaa: Option<(wgpu::TextureView, [u32; 2])>,
    pub(super) format: wgpu::TextureFormat,
    /// World geometry uploads (for tests: panning must not re-upload).
    pub uploads: u64,
}

fn texture_2d(device: &wgpu::Device, label: &str, w: u32, h: u32, mips: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some(label),
        size: wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
        mip_level_count: mips,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    })
}

fn write_texture(queue: &wgpu::Queue, tex: &wgpu::Texture, mip: u32, img: &image::RgbaImage) {
    queue.write_texture(
        wgpu::TexelCopyTextureInfo {
            texture: tex,
            mip_level: mip,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        img.as_raw(),
        wgpu::TexelCopyBufferLayout {
            offset: 0,
            bytes_per_row: Some(img.width() * 4),
            rows_per_image: Some(img.height()),
        },
        wgpu::Extent3d {
            width: img.width(),
            height: img.height(),
            depth_or_array_layers: 1,
        },
    );
}

impl CanvasRenderer {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> CanvasRenderer {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("canvas"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let globals_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("canvas globals"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });
        let tex_layout = |label| {
            device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some(label),
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
            })
        };
        let ramp_layout = tex_layout("ramp");
        let img_layout = tex_layout("image");
        let mk_globals = |label| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: std::mem::size_of::<Globals>() as u64,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            })
        };
        let world_globals = mk_globals("world globals");
        let screen_globals = mk_globals("screen globals");
        let mk_bg = |buf: &wgpu::Buffer| {
            device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("globals"),
                layout: &globals_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: buf.as_entire_binding(),
                }],
            })
        };
        let world_bg = mk_bg(&world_globals);
        let screen_bg = mk_bg(&screen_globals);
        let ramp_tex = texture_2d(device, "gradient ramps", tess::ATLAS_W, tess::ATLAS_H, 1);
        let ramp_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("ramp"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let ramp_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ramp"),
            layout: &ramp_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &ramp_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&ramp_sampler),
                },
            ],
        });
        let img_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("image"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::Repeat,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            ..Default::default()
        });
        let blank = texture_2d(device, "blank", 1, 1, 1);
        let blank_img = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("blank image"),
            layout: &img_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &blank.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&img_sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("canvas"),
            bind_group_layouts: &[Some(&globals_layout), Some(&ramp_layout), Some(&img_layout)],
            immediate_size: 0,
        });
        let pipelines = (0..5)
            .map(|key| {
                device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("canvas"),
                    layout: Some(&layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs"),
                        compilation_options: Default::default(),
                        buffers: &[Some(wgpu::VertexBufferLayout {
                            array_stride: std::mem::size_of::<Vertex>() as u64,
                            step_mode: wgpu::VertexStepMode::Vertex,
                            attributes: &wgpu::vertex_attr_array![
                                0 => Float32x2, 1 => Unorm8x4, 2 => Float32x2, 3 => Uint32
                            ],
                        })],
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState {
                        count: MSAA,
                        mask: !0,
                        alpha_to_coverage_enabled: false,
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs"),
                        compilation_options: Default::default(),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: Some(blend_state(key)),
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                    }),
                    multiview_mask: None,
                    cache: None,
                })
            })
            .collect();
        CanvasRenderer {
            pipelines,
            layers: None,
            world_globals,
            screen_globals,
            world_bg,
            screen_bg,
            ramp_tex,
            ramp_bg,
            ramp_version: 0,
            img_layout,
            img_sampler,
            blank_img,
            images: HashMap::new(),
            world: None,
            msaa: None,
            format,
            uploads: 0,
        }
    }

    fn msaa_view(&mut self, device: &wgpu::Device, size: [u32; 2]) -> &wgpu::TextureView {
        if self.msaa.as_ref().is_none_or(|(_, s)| *s != size) {
            let tex = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("canvas msaa"),
                size: wgpu::Extent3d {
                    width: size[0],
                    height: size[1],
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: MSAA,
                dimension: wgpu::TextureDimension::D2,
                format: self.format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.msaa = Some((tex.create_view(&Default::default()), size));
        }
        &self.msaa.as_ref().unwrap().0
    }

    fn sync_ramps(&mut self, queue: &wgpu::Queue) {
        let a = tess::atlas().lock().unwrap();
        if a.version == self.ramp_version {
            return;
        }
        self.ramp_version = a.version;
        let img = image::RgbaImage::from_raw(tess::ATLAS_W, tess::ATLAS_H, a.pixels.clone())
            .expect("atlas size");
        write_texture(queue, &self.ramp_tex, 0, &img);
    }

    fn image_bg(&mut self, device: &wgpu::Device, queue: &wgpu::Queue, id: &str) {
        if let Some((_, used)) = self.images.get_mut(id) {
            *used = Instant::now();
            return;
        }
        let Some(rgba) = assets::get(id).and_then(|a| a.rgba()) else { return };
        let (width, height) = rgba.dimensions();
        // Mip chain so minified images stay smooth.
        let mut levels = vec![rgba];
        while levels.last().unwrap().width() > 1 || levels.last().unwrap().height() > 1 {
            let l = levels.last().unwrap();
            let (w, h) = ((l.width() / 2).max(1), (l.height() / 2).max(1));
            levels.push(image::imageops::resize(
                l,
                w,
                h,
                image::imageops::FilterType::Triangle,
            ));
            if levels.len() > 14 {
                break;
            }
        }
        let tex = texture_2d(device, "image", width, height, levels.len() as u32);
        for (i, l) in levels.iter().enumerate() {
            write_texture(queue, &tex, i as u32, l);
        }
        let bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("image"),
            layout: &self.img_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(
                        &tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.img_sampler),
                },
            ],
        });
        self.images.insert(id.to_string(), (bg, Instant::now()));
    }

    /// Rebuild the world vertex/index buffers when the item list changed.
    fn sync_world(&mut self, device: &wgpu::Device, items: &[Item]) {
        let sig: Vec<(usize, Option<[u64; 4]>, usize)> = items
            .iter()
            .map(|it| {
                let id = match &it.mark {
                    // Groups are rebuilt every frame: compare what they hold, not the pointer.
                    Some(Mark::Begin(g)) => layers::group_key(g),
                    Some(Mark::End) => 1,
                    None => Arc::as_ptr(&it.pieces) as usize,
                };
                (
                    id,
                    it.clip
                        .map(|c| [c.x.to_bits(), c.y.to_bits(), c.w.to_bits(), c.h.to_bits()]),
                    blend_key(it.blend),
                )
            })
            .collect();
        if self.world.as_ref().is_some_and(|w| w.signature == sig) {
            return;
        }
        self.uploads += 1;
        let mut vertices: Vec<Vertex> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        let mut cmds: Vec<WorldCmd> = Vec::new();
        // Open groups: where their Begin sits and the bounds of what they drew so far.
        let mut open: Vec<(usize, Option<Rect>)> = Vec::new();
        for it in items {
            match &it.mark {
                Some(Mark::Begin(g)) => {
                    open.push((cmds.len(), None));
                    cmds.push(WorldCmd::Begin(g.clone(), it.clip, g.outline_bounds()));
                    continue;
                }
                Some(Mark::End) => {
                    if let Some((at, content)) = open.pop() {
                        let bounds = match &mut cmds[at] {
                            WorldCmd::Begin(g, _, bounds) => {
                                *bounds = layers::group_bounds(g, content);
                                *bounds
                            }
                            _ => continue,
                        };
                        if let Some((_, parent)) = open.last_mut() {
                            *parent = Some(parent.map_or(bounds, |p| p.union(&bounds)));
                        }
                        cmds.push(WorldCmd::End);
                    }
                    continue;
                }
                None => {}
            }
            let bk = blend_key(it.blend);
            for p in it.pieces.iter() {
                if let Some((_, content)) = open.last_mut() {
                    if let Some(r) = layers::mesh_bounds(&p.mesh.vertices) {
                        *content = Some(content.map_or(r, |c| c.union(&r)));
                    }
                }
                let base = vertices.len() as u32;
                let start = indices.len() as u32;
                vertices.extend_from_slice(&p.mesh.vertices);
                indices.extend(p.mesh.indices.iter().map(|i| i + base));
                let end = indices.len() as u32;
                match cmds.last_mut() {
                    Some(WorldCmd::Draw(d))
                        if d.1 == it.clip && d.2 == bk && d.3 == p.image && d.0.end == start =>
                    {
                        d.0.end = end
                    }
                    _ => cmds.push(WorldCmd::Draw((start..end, it.clip, bk, p.image.clone()))),
                }
            }
        }
        let mk = |data: &[u8], usage, label| {
            device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some(label),
                contents: data,
                usage,
            })
        };
        let (vb, ib) = if indices.is_empty() {
            (None, None)
        } else {
            (
                Some(mk(
                    bytemuck::cast_slice(&vertices),
                    wgpu::BufferUsages::VERTEX,
                    "world vertices",
                )),
                Some(mk(
                    bytemuck::cast_slice(&indices),
                    wgpu::BufferUsages::INDEX,
                    "world indices",
                )),
            )
        };
        self.world = Some(WorldBuffers {
            signature: sig,
            vb,
            ib,
            cmds,
        });
    }

    /// Record a pass that clears `target` and draws a canvas frame.
    /// Logical px × `scale` = physical px.
    #[allow(clippy::too_many_arguments)]
    pub fn render(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scale: f32,
        frame: &CanvasFrame,
        clear: [f64; 4],
    ) {
        let cam = frame.camera;
        let world = Globals {
            size_px: [size[0] as f32, size[1] as f32],
            scale,
            zoom: cam.zoom as f32,
            pan: [cam.pan.x as f32, cam.pan.y as f32],
            offset: [frame.offset.x as f32, frame.offset.y as f32],
        };
        let screen = Globals {
            zoom: 1.0,
            pan: [0.0; 2],
            offset: [0.0; 2],
            ..world
        };
        queue.write_buffer(&self.world_globals, 0, bytemuck::bytes_of(&world));
        queue.write_buffer(&self.screen_globals, 0, bytemuck::bytes_of(&screen));
        self.sync_ramps(queue);
        for it in &frame.items {
            for p in it.pieces.iter() {
                if let Some(id) = &p.image {
                    self.image_bg(device, queue, id);
                }
            }
        }
        self.images.retain(|_, (_, used)| used.elapsed() < IMAGE_IDLE);
        self.sync_world(device, &frame.items);

        // Screen-space batches (underlay + overlay) are small: rebuilt every frame.
        let mut sverts: Vec<Vertex> = Vec::new();
        let mut sidx: Vec<u32> = Vec::new();
        let mut under = Vec::new();
        let mut over = Vec::new();
        for (list, out) in [(&frame.under, &mut under), (&frame.over, &mut over)] {
            for b in list.iter() {
                let base = sverts.len() as u32;
                let start = sidx.len() as u32;
                sverts.extend_from_slice(&b.mesh.vertices);
                sidx.extend(b.mesh.indices.iter().map(|i| i + base));
                out.push(Draw {
                    range: start..sidx.len() as u32,
                    clip: b.clip,
                    blend: 0,
                    image: None,
                    world: false,
                });
            }
        }
        let sbuf = (!sidx.is_empty()).then(|| {
            (
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("screen vertices"),
                    contents: bytemuck::cast_slice(&sverts),
                    usage: wgpu::BufferUsages::VERTEX,
                }),
                device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some("screen indices"),
                    contents: bytemuck::cast_slice(&sidx),
                    usage: wgpu::BufferUsages::INDEX,
                }),
            )
        });
        let vp = frame.viewport;
        let to_screen = |r: &Rect| {
            Rect::new(
                (r.x - cam.pan.x) * cam.zoom + frame.offset.x,
                (r.y - cam.pan.y) * cam.zoom + frame.offset.y,
                r.w * cam.zoom,
                r.h * cam.zoom,
            )
        };
        let px_per_world = cam.zoom as f32 * scale;
        let mut cmds: Vec<Cmd> = under.into_iter().map(|d| Cmd::Op(Op::Draw(d))).collect();
        let mut layered = false;
        for c in self.world.iter().flat_map(|w| w.cmds.iter()) {
            let area = |clip: &Option<Rect>| match clip {
                Some(c) => vp.intersection(&to_screen(c)),
                None => Some(vp),
            };
            match c {
                WorldCmd::Draw((range, clip, blend, image)) => {
                    let Some(sc) = area(clip) else { continue };
                    cmds.push(Cmd::Op(Op::Draw(Draw {
                        range: range.clone(),
                        clip: Some(sc),
                        blend: *blend,
                        image: image.clone(),
                        world: true,
                    })));
                }
                WorldCmd::Begin(group, clip, bounds) => {
                    layered = true;
                    // A group clipped away entirely still needs its Begin and End balanced.
                    let empty = Rect::new(0.0, 0.0, 0.0, 0.0);
                    let area = area(clip).unwrap_or(empty);
                    let rect = area.intersection(&to_screen(bounds)).unwrap_or(empty);
                    cmds.push(Cmd::Begin(layers::group_run(
                        group,
                        rect,
                        area,
                        to_screen(&group.shape),
                        px_per_world,
                        scale,
                    )));
                }
                WorldCmd::End => cmds.push(Cmd::End),
            }
        }
        cmds.extend(over.into_iter().map(|d| Cmd::Op(Op::Draw(d))));

        if layered {
            self.render_layered(device, encoder, target, size, scale, clear, cmds, &sbuf);
            return;
        }
        let ops: Vec<Op> = cmds
            .into_iter()
            .filter_map(|c| match c {
                Cmd::Op(o) => Some(o),
                _ => None,
            })
            .collect();
        let msaa = self.msaa_view(device, size).clone();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("canvas"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &msaa,
                depth_slice: None,
                resolve_target: Some(target),
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color {
                        r: clear[0],
                        g: clear[1],
                        b: clear[2],
                        a: clear[3],
                    }),
                    store: wgpu::StoreOp::Discard,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_bind_group(1, &self.ramp_bg, &[]);
        self.encode(&mut pass, &ops, &sbuf, size, scale);
    }

    /// Record `ops` into `pass` (the ramp bind group must be set).
    pub(super) fn encode(
        &self,
        pass: &mut wgpu::RenderPass<'_>,
        ops: &[Op],
        sbuf: &Option<(wgpu::Buffer, wgpu::Buffer)>,
        size: [u32; 2],
        scale: f32,
    ) {
        let mut current_pipeline = usize::MAX;
        for op in ops {
            let d = match op {
                Op::Draw(d) => d,
                Op::Quad(q) => {
                    let (x, y, w, h) = scissor(Some(q.clip), size, scale);
                    if w > 0 && h > 0 {
                        pass.set_pipeline(&q.pipeline);
                        pass.set_bind_group(0, &q.bg, &[]);
                        pass.set_scissor_rect(x, y, w, h);
                        pass.draw(0..3, 0..1);
                    }
                    // The quad's pipeline has another layout: set the canvas state again.
                    pass.set_bind_group(1, &self.ramp_bg, &[]);
                    current_pipeline = usize::MAX;
                    continue;
                }
            };
            let (vb, ib) = if d.world {
                match self
                    .world
                    .as_ref()
                    .and_then(|w| w.vb.as_ref().zip(w.ib.as_ref()))
                {
                    Some(b) => b,
                    None => continue,
                }
            } else {
                match sbuf {
                    Some((v, i)) => (v, i),
                    None => continue,
                }
            };
            let (x, y, w, h) = scissor(d.clip, size, scale);
            if w == 0 || h == 0 || d.range.is_empty() {
                continue;
            }
            if current_pipeline != d.blend {
                pass.set_pipeline(&self.pipelines[d.blend]);
                current_pipeline = d.blend;
            }
            pass.set_bind_group(
                0,
                if d.world {
                    &self.world_bg
                } else {
                    &self.screen_bg
                },
                &[],
            );
            let img = d
                .image
                .as_ref()
                .and_then(|i| self.images.get(i))
                .map_or(&self.blank_img, |(bg, _)| bg);
            pass.set_bind_group(2, img, &[]);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.set_scissor_rect(x, y, w, h);
            pass.draw_indexed(d.range.clone(), 0, 0..1);
        }
    }
}

/// A logical-px rect as a scissor in physical px, kept inside the target.
pub(super) fn scissor(c: Option<Rect>, size: [u32; 2], scale: f32) -> (u32, u32, u32, u32) {
    let Some(c) = c else { return (0, 0, size[0], size[1]) };
    let s = scale as f64;
    let x0 = (c.x * s).floor().clamp(0.0, size[0] as f64);
    let y0 = (c.y * s).floor().clamp(0.0, size[1] as f64);
    let x1 = (c.right() * s).ceil().clamp(0.0, size[0] as f64);
    let y1 = (c.bottom() * s).ceil().clamp(0.0, size[1] as f64);
    (x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32)
}
