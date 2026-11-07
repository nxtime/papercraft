//! Offscreen layers: rounded clips, layer and background blur, and the blend modes that read what
//! is underneath. A group of items is drawn to its own texture, which is then blurred, masked and
//! composited over the target by a shader. Everything else still goes through one pass.

use super::gpu::{scissor, Cmd, GroupRun, Op, Quad, CanvasRenderer, MSAA};
use super::mesh::Vertex;
use super::scene::Group;
use crate::geom::Rect;
use crate::model::BlendMode;
use std::hash::{Hash, Hasher};
use std::sync::Arc;
use wgpu::util::DeviceExt;

/// Layers nested deeper than this are drawn straight into their parent.
const MAX_DEPTH: usize = 3;
/// A Gaussian reaches about this many sigmas.
const REACH: f64 = 3.0;
/// Past this sigma (physical px) a blur runs twice at sigma / sqrt 2: the sampling step grows
/// with sigma and one pass would show bands.
const TWO_PASS_SIGMA: f32 = 6.0;

const SHADER: &str = r#"
struct Post {
    rect: vec4<f32>,
    radii: vec4<f32>,
    dir: vec2<f32>,
    sigma: f32,
    mode: u32,
    blend: u32,
    flags: u32,
    size: vec2<f32>,
};
@group(0) @binding(0) var<uniform> p: Post;
@group(0) @binding(1) var t_layer: texture_2d<f32>;
@group(0) @binding(2) var t_back: texture_2d<f32>;
@group(0) @binding(3) var t_blur: texture_2d<f32>;
@group(0) @binding(4) var samp: sampler;

@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    return vec4<f32>(uv * 2.0 - 1.0, 0.0, 1.0);
}

fn blur(uv: vec2<f32>) -> vec4<f32> {
    let sigma = max(p.sigma, 0.3);
    let step = max(sigma * 2.5 / 20.0, 1.0);
    var acc = vec4<f32>(0.0);
    var total = 0.0;
    for (var i = -20; i <= 20; i++) {
        let o = f32(i) * step;
        let w = exp(-0.5 * o * o / (sigma * sigma));
        acc += w * textureSampleLevel(t_layer, samp, uv + p.dir * o, 0.0);
        total += w;
    }
    return acc / total;
}

// Signed distance to a rounded box with per-corner radii (tl, tr, br, bl), pixels, y down.
fn sd_rrect(pos: vec2<f32>) -> f32 {
    let half = p.rect.zw * 0.5;
    let q = pos - (p.rect.xy + half);
    var r = p.radii.x;
    if (q.x > 0.0 && q.y <= 0.0) { r = p.radii.y; }
    if (q.x > 0.0 && q.y > 0.0) { r = p.radii.z; }
    if (q.x <= 0.0 && q.y > 0.0) { r = p.radii.w; }
    let d = abs(q) - half + vec2<f32>(r);
    return min(max(d.x, d.y), 0.0) + length(max(d, vec2<f32>(0.0))) - r;
}

fn lum(c: vec3<f32>) -> f32 { return dot(c, vec3<f32>(0.3, 0.59, 0.11)); }
fn clip_color(c: vec3<f32>) -> vec3<f32> {
    let l = lum(c);
    let n = min(c.r, min(c.g, c.b));
    let x = max(c.r, max(c.g, c.b));
    var o = c;
    if (n < 0.0) { o = l + (o - l) * l / (l - n); }
    if (x > 1.0) { o = l + (o - l) * (1.0 - l) / (x - l); }
    return o;
}
fn set_lum(c: vec3<f32>, l: f32) -> vec3<f32> { return clip_color(c + (l - lum(c))); }
fn sat(c: vec3<f32>) -> f32 { return max(c.r, max(c.g, c.b)) - min(c.r, min(c.g, c.b)); }
fn set_sat(c: vec3<f32>, s: f32) -> vec3<f32> {
    let mx = max(c.r, max(c.g, c.b));
    let mn = min(c.r, min(c.g, c.b));
    if (mx > mn) { return (c - mn) * s / (mx - mn); }
    return vec3<f32>(0.0);
}
fn hard_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let t = 2.0 * s - 1.0;
    return select(b + t - b * t, b * 2.0 * s, s <= vec3<f32>(0.5));
}
fn soft_light(b: vec3<f32>, s: vec3<f32>) -> vec3<f32> {
    let d = select(sqrt(b), ((16.0 * b - 12.0) * b + 4.0) * b, b <= vec3<f32>(0.25));
    let lo = b - (1.0 - 2.0 * s) * b * (1.0 - b);
    let hi = b + (2.0 * s - 1.0) * (d - b);
    return select(hi, lo, s <= vec3<f32>(0.5));
}

// W3C compositing blend functions on straight (not premultiplied) colors; modes follow
// `BlendMode::ALL`.
fn blend_fn(mode: u32, cb: vec3<f32>, cs: vec3<f32>) -> vec3<f32> {
    let one = vec3<f32>(1.0);
    switch mode {
        case 1u: { return cb * cs; }
        case 2u: { return cb + cs - cb * cs; }
        case 3u: { return hard_light(cs, cb); }
        case 4u: { return min(cb, cs); }
        case 5u: { return max(cb, cs); }
        case 6u: {
            let v = min(one, cb / (one - cs));
            return select(select(v, one, cs >= one), vec3<f32>(0.0), cb <= vec3<f32>(0.0));
        }
        case 7u: {
            let v = one - min(one, (one - cb) / cs);
            return select(select(v, vec3<f32>(0.0), cs <= vec3<f32>(0.0)), one, cb >= one);
        }
        case 8u: { return hard_light(cb, cs); }
        case 9u: { return soft_light(cb, cs); }
        case 10u: { return abs(cb - cs); }
        case 11u: { return cb + cs - 2.0 * cb * cs; }
        case 12u: { return set_lum(set_sat(cs, sat(cb)), lum(cb)); }
        case 13u: { return set_lum(set_sat(cb, sat(cs)), lum(cb)); }
        case 14u: { return set_lum(cs, lum(cb)); }
        case 15u: { return set_lum(cb, lum(cs)); }
        default: { return cs; }
    }
}

@fragment
fn fs(@builtin(position) frag: vec4<f32>) -> @location(0) vec4<f32> {
    let uv = frag.xy / p.size;
    if (p.mode == 0u) { return blur(uv); }
    var l = textureSampleLevel(t_layer, samp, uv, 0.0);
    let m = clamp(0.5 - sd_rrect(frag.xy), 0.0, 1.0);
    if ((p.flags & 1u) != 0u) { l = l * m; }
    if (p.mode == 1u) { return l; }
    var b = textureSampleLevel(t_back, samp, uv, 0.0);
    if ((p.flags & 2u) != 0u) { b = mix(b, textureSampleLevel(t_blur, samp, uv, 0.0), m); }
    let ab = b.a;
    let a_s = l.a;
    let cb = b.rgb / max(ab, 0.0001);
    let cs = l.rgb / max(a_s, 0.0001);
    let rgb = l.rgb * (1.0 - ab) + b.rgb * (1.0 - a_s) + a_s * ab * blend_fn(p.blend, cb, cs);
    return vec4<f32>(rgb, a_s + ab - a_s * ab);
}
"#;

const MODE_BLUR: u32 = 0;
const MODE_OVER: u32 = 1;
const MODE_REPLACE: u32 = 2;
const FLAG_MASK: u32 = 1;
const FLAG_BACKDROP: u32 = 2;

#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Post {
    rect: [f32; 4],
    radii: [f32; 4],
    dir: [f32; 2],
    sigma: f32,
    mode: u32,
    blend: u32,
    flags: u32,
    size: [f32; 2],
}

struct Tex {
    _tex: wgpu::Texture,
    view: wgpu::TextureView,
}

/// A render target with its own multisampled attachment and two resolved textures: a pass
/// resolves into the one not being read.
struct Level {
    msaa: wgpu::TextureView,
    res: [Tex; 2],
    /// Index of the latest resolved texture.
    cur: usize,
    started: bool,
    pending: Vec<Op>,
    run: Option<GroupRun>,
}

pub(super) struct Layers {
    layout: wgpu::BindGroupLayout,
    blur: wgpu::RenderPipeline,
    over: wgpu::RenderPipeline,
    replace: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    blank: Tex,
    size: [u32; 2],
    levels: Vec<Level>,
    /// Blur ping-pong textures: two for the layer, two for the backdrop.
    scratch: Vec<Tex>,
}

/// Key of a group for the world cache's signature.
pub(super) fn group_key(g: &Group) -> usize {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for v in [g.shape.x, g.shape.y, g.shape.w, g.shape.h, g.blur, g.backdrop_blur] {
        v.to_bits().hash(&mut h);
    }
    g.radii.iter().for_each(|r| r.to_bits().hash(&mut h));
    (g.mask, g.blend as u8).hash(&mut h);
    // Bit 0 stays free for `Mark::End`.
    (h.finish() as usize) | 2
}

/// World bounds of a group: what it drew (the rounded clip's shape limits it, a background blur
/// shows the whole shape), grown by the blur's reach.
pub(super) fn group_bounds(g: &Group, content: Option<Rect>) -> Rect {
    let r = match (content, g.mask) {
        (Some(c), true) => c.intersection(&g.shape).unwrap_or(g.shape),
        (None, _) => g.shape,
        (Some(c), false) if g.backdrop_blur > 0.0 => c.union(&g.shape),
        (Some(c), false) => c,
    };
    r.inflate(REACH * g.blur)
}

pub(super) fn mesh_bounds(vs: &[Vertex]) -> Option<Rect> {
    let first = vs.first()?;
    let (mut x0, mut y0, mut x1, mut y1) = (first.pos[0], first.pos[1], first.pos[0], first.pos[1]);
    for v in vs {
        x0 = x0.min(v.pos[0]);
        y0 = y0.min(v.pos[1]);
        x1 = x1.max(v.pos[0]);
        y1 = y1.max(v.pos[1]);
    }
    Some(Rect::new(x0 as f64, y0 as f64, (x1 - x0) as f64, (y1 - y0) as f64))
}

/// A group's screen-space parameters. `rect` is the composite area, `area` the viewport and
/// clip the blurs may read within, `shape` the node's rect (logical px).
pub(super) fn group_run(
    g: &Arc<Group>,
    rect: Rect,
    area: Rect,
    shape: Rect,
    px_per_world: f32,
    scale: f32,
) -> GroupRun {
    let sigma = g.blur as f32 * px_per_world;
    let backdrop_sigma = g.backdrop_blur as f32 * px_per_world;
    // Blurs read past the composite area; the backdrop one does so from the whole screen.
    let reach = (REACH as f32 * sigma.max(backdrop_sigma) / scale) as f64;
    let wide = rect.inflate(reach).intersection(&area).unwrap_or(rect);
    let s = scale as f64;
    let max_r = (shape.w.min(shape.h) * 0.5 * s) as f32;
    GroupRun {
        group: g.clone(),
        rect,
        wide,
        sigma,
        backdrop_sigma,
        shape: [(shape.x * s) as f32, (shape.y * s) as f32, (shape.w * s) as f32, (shape.h * s) as f32],
        radii: g.radii.map(|r| ((r * px_per_world as f64) as f32).clamp(0.0, max_r)),
    }
}

fn blend_index(b: BlendMode) -> u32 {
    BlendMode::ALL.iter().position(|x| *x == b).unwrap_or(0) as u32
}

impl Layers {
    pub(super) fn new(device: &wgpu::Device, format: wgpu::TextureFormat) -> Layers {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("layers"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let tex_entry = |binding| wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        };
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("layers"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                tex_entry(1),
                tex_entry(2),
                tex_entry(3),
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("layers"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |samples: u32, blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("layers"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs"),
                    compilation_options: Default::default(),
                    buffers: &[],
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: samples,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some("fs"),
                    compilation_options: Default::default(),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let premultiplied = wgpu::BlendComponent {
            src_factor: wgpu::BlendFactor::One,
            dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
            operation: wgpu::BlendOperation::Add,
        };
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("layers"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Layers {
            blur: pipeline(1, None),
            over: pipeline(
                MSAA,
                Some(wgpu::BlendState { color: premultiplied, alpha: premultiplied }),
            ),
            replace: pipeline(MSAA, None),
            layout,
            sampler,
            blank: target_tex(device, format, [1, 1]),
            size: [0, 0],
            levels: Vec::new(),
            scratch: Vec::new(),
        }
    }

    fn resize(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, size: [u32; 2]) {
        if self.size == size {
            return;
        }
        self.size = size;
        self.levels.clear();
        self.scratch = (0..4).map(|_| target_tex(device, format, size)).collect();
    }

    fn level(&mut self, device: &wgpu::Device, format: wgpu::TextureFormat, depth: usize) {
        while self.levels.len() <= depth {
            let size = self.size;
            let msaa = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("layer msaa"),
                size: wgpu::Extent3d { width: size[0], height: size[1], depth_or_array_layers: 1 },
                mip_level_count: 1,
                sample_count: MSAA,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            });
            self.levels.push(Level {
                msaa: msaa.create_view(&Default::default()),
                res: [target_tex(device, format, size), target_tex(device, format, size)],
                cur: 1,
                started: false,
                pending: Vec::new(),
                run: None,
            });
        }
        let l = &mut self.levels[depth];
        l.cur = 1;
        l.started = false;
        l.pending.clear();
        l.run = None;
    }

    fn bind_group(
        &self,
        device: &wgpu::Device,
        post: &Post,
        layer: &wgpu::TextureView,
        back: &wgpu::TextureView,
        blurred: &wgpu::TextureView,
    ) -> wgpu::BindGroup {
        let buf = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("layers"),
            contents: bytemuck::bytes_of(post),
            usage: wgpu::BufferUsages::UNIFORM,
        });
        let tex = |binding, view| wgpu::BindGroupEntry {
            binding,
            resource: wgpu::BindingResource::TextureView(view),
        };
        device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("layers"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: buf.as_entire_binding() },
                tex(1, layer),
                tex(2, back),
                tex(3, blurred),
                wgpu::BindGroupEntry {
                    binding: 4,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        })
    }

    /// Gaussian-blur `src` into scratch texture `first + 1` (`first` is the horizontal pass's
    /// target); returns that index.
    #[allow(clippy::too_many_arguments)]
    fn blur_into(
        &self,
        device: &wgpu::Device,
        enc: &mut wgpu::CommandEncoder,
        src: &wgpu::TextureView,
        first: usize,
        sigma: f32,
        wide: Rect,
        scale: f32,
    ) -> usize {
        let twice = sigma > TWO_PASS_SIGMA;
        let each = if twice { sigma / std::f32::consts::SQRT_2 } else { sigma };
        let [w, h] = self.size;
        let (a, b) = (&self.scratch[first], &self.scratch[first + 1]);
        let mut from = src;
        for _ in 0..if twice { 2 } else { 1 } {
            for (to, dir) in [(a, [1.0 / w as f32, 0.0]), (b, [0.0, 1.0 / h as f32])] {
                let post = Post {
                    rect: [0.0; 4],
                    radii: [0.0; 4],
                    dir,
                    sigma: each,
                    mode: MODE_BLUR,
                    blend: 0,
                    flags: 0,
                    size: [w as f32, h as f32],
                };
                let bg = self.bind_group(device, &post, from, &self.blank.view, &self.blank.view);
                let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
                    label: Some("blur"),
                    color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                        view: &to.view,
                        depth_slice: None,
                        resolve_target: None,
                        ops: wgpu::Operations {
                            load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                            store: wgpu::StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    timestamp_writes: None,
                    occlusion_query_set: None,
                    multiview_mask: None,
                });
                let (x, y, sw, sh) = scissor(Some(wide), self.size, scale);
                if sw > 0 && sh > 0 {
                    pass.set_pipeline(&self.blur);
                    pass.set_bind_group(0, &bg, &[]);
                    pass.set_scissor_rect(x, y, sw, sh);
                    pass.draw(0..3, 0..1);
                }
                drop(pass);
                from = &to.view;
            }
        }
        first + 1
    }
}

fn target_tex(device: &wgpu::Device, format: wgpu::TextureFormat, size: [u32; 2]) -> Tex {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("layer"),
        size: wgpu::Extent3d {
            width: size[0].max(1),
            height: size[1].max(1),
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    });
    Tex { view: tex.create_view(&Default::default()), _tex: tex }
}

/// Encode the pending ops of `lvl` as one pass, resolving into its spare texture (or into
/// `final_target` for the last pass of the frame).
#[allow(clippy::too_many_arguments)]
fn flush(
    r: &CanvasRenderer,
    enc: &mut wgpu::CommandEncoder,
    lvl: &mut Level,
    size: [u32; 2],
    scale: f32,
    clear: [f64; 4],
    final_target: Option<&wgpu::TextureView>,
    sbuf: &Option<(wgpu::Buffer, wgpu::Buffer)>,
) {
    if final_target.is_none() && lvl.started && lvl.pending.is_empty() {
        return;
    }
    let next = 1 - lvl.cur;
    let load = if lvl.started {
        wgpu::LoadOp::Load
    } else {
        wgpu::LoadOp::Clear(wgpu::Color { r: clear[0], g: clear[1], b: clear[2], a: clear[3] })
    };
    let mut pass = enc.begin_render_pass(&wgpu::RenderPassDescriptor {
        label: Some("layer"),
        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
            view: &lvl.msaa,
            depth_slice: None,
            resolve_target: Some(final_target.unwrap_or(&lvl.res[next].view)),
            ops: wgpu::Operations {
                load,
                store: if final_target.is_some() { wgpu::StoreOp::Discard } else { wgpu::StoreOp::Store },
            },
        })],
        depth_stencil_attachment: None,
        timestamp_writes: None,
        occlusion_query_set: None,
        multiview_mask: None,
    });
    pass.set_bind_group(1, &r.ramp_bg, &[]);
    r.encode(&mut pass, &lvl.pending, sbuf, size, scale);
    drop(pass);
    lvl.pending.clear();
    lvl.started = true;
    if final_target.is_none() {
        lvl.cur = next;
    }
}

impl CanvasRenderer {
    /// The frame with offscreen layers: each group is drawn to its own level, then blurred and
    /// composited into the level below. `cmds` is the whole frame, root draws included.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn render_layered(
        &mut self,
        device: &wgpu::Device,
        enc: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        size: [u32; 2],
        scale: f32,
        clear: [f64; 4],
        cmds: Vec<Cmd>,
        sbuf: &Option<(wgpu::Buffer, wgpu::Buffer)>,
    ) {
        let format = self.format;
        let mut layers = self.layers.take().unwrap_or_else(|| Layers::new(device, format));
        layers.resize(device, format, size);
        layers.level(device, format, 0);
        let (mut depth, mut skipped) = (0usize, 0usize);
        for cmd in cmds {
            match cmd {
                Cmd::Op(op) => layers.levels[depth].pending.push(op),
                Cmd::Begin(run) => {
                    if depth >= MAX_DEPTH {
                        skipped += 1;
                    } else {
                        depth += 1;
                        layers.level(device, format, depth);
                        layers.levels[depth].run = Some(run);
                    }
                }
                Cmd::End if skipped > 0 => skipped -= 1,
                Cmd::End if depth > 0 => {
                    finish_group(self, &mut layers, device, enc, depth, size, scale, clear, sbuf);
                    depth -= 1;
                }
                Cmd::End => {}
            }
        }
        flush(self, enc, &mut layers.levels[0], size, scale, clear, Some(target), sbuf);
        self.layers = Some(layers);
    }
}

/// Close the group at `depth`: blur its layer, then composite it into the level below.
#[allow(clippy::too_many_arguments)]
fn finish_group(
    r: &CanvasRenderer,
    layers: &mut Layers,
    device: &wgpu::Device,
    enc: &mut wgpu::CommandEncoder,
    depth: usize,
    size: [u32; 2],
    scale: f32,
    clear: [f64; 4],
    sbuf: &Option<(wgpu::Buffer, wgpu::Buffer)>,
) {
    // Levels are taken out so the blur helpers can borrow `layers` whole.
    let mut levels = std::mem::take(&mut layers.levels);
    let (below, top) = levels.split_at_mut(depth);
    let (parent, lvl) = (&mut below[depth - 1], &mut top[0]);
    let Some(run) = lvl.run.take() else {
        layers.levels = levels;
        return;
    };
    flush(r, enc, lvl, size, scale, [0.0; 4], None, sbuf);
    // Only the root level starts from the frame's clear color.
    let parent_clear = if depth == 1 { clear } else { [0.0; 4] };
    let layer = &lvl.res[lvl.cur].view;
    let blurred_layer = (run.sigma > 0.0).then(|| layers.blur_into(device, enc, layer, 0, run.sigma, run.wide, scale));
    let layer = blurred_layer.map_or(layer, |i| &layers.scratch[i].view);

    let g = &run.group;
    let replace = !g.blend.fixed_function() || g.blend != BlendMode::Normal || run.backdrop_sigma > 0.0;
    let mut post = Post {
        rect: run.shape,
        radii: run.radii,
        dir: [0.0; 2],
        sigma: 0.0,
        mode: if replace { MODE_REPLACE } else { MODE_OVER },
        blend: blend_index(g.blend),
        flags: if g.mask { FLAG_MASK } else { 0 },
        size: [size[0] as f32, size[1] as f32],
    };
    let bg = if replace {
        // The backdrop is the parent as drawn so far.
        flush(r, enc, parent, size, scale, parent_clear, None, sbuf);
        let back = &parent.res[parent.cur].view;
        let blurred_back = (run.backdrop_sigma > 0.0)
            .then(|| layers.blur_into(device, enc, back, 2, run.backdrop_sigma, run.wide, scale));
        if blurred_back.is_some() {
            post.flags |= FLAG_BACKDROP;
        }
        let blurred = blurred_back.map_or(&layers.blank.view, |i| &layers.scratch[i].view);
        layers.bind_group(device, &post, layer, back, blurred)
    } else {
        layers.bind_group(device, &post, layer, &layers.blank.view, &layers.blank.view)
    };
    parent.pending.push(Op::Quad(Quad {
        bg,
        pipeline: if replace { layers.replace.clone() } else { layers.over.clone() },
        clip: run.rect,
    }));
    // Flushed at once: the next layer at this depth reuses the textures the quad reads.
    flush(r, enc, parent, size, scale, parent_clear, None, sbuf);
    layers.levels = levels;
}
