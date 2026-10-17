//! GPU-resident video: every video module's output is a texture that stays
//! on the GPU from camera upload to the final draw. Modules record their
//! passes into one command encoder per tick (see `graph::run_graph`), so
//! there are no CPU round-trips between modules.
//!
//! Convention: textures are stored top row first (row 0 = top of the image).

use std::sync::Arc;
use wgpu_upstream as wgpu;

pub const FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

/// Vertex stage shared by every fullscreen pass: one oversized triangle,
/// with `uv` running (0,0) top-left to (1,1) bottom-right across the screen.
pub const FULLSCREEN_VS: &str = "
struct VOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs(@builtin(vertex_index) i: u32) -> VOut {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    var out: VOut;
    out.pos = vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
    out.uv = vec2<f32>(x, 1.0 - y);
    return out;
}
";

/// Same triangle with only a position output, for GLSL fragment stages
/// (which compute everything from `gl_FragCoord`).
pub const POSITION_VS: &str = "
@vertex
fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
    let x = f32((i << 1u) & 2u);
    let y = f32(i & 2u);
    return vec4<f32>(x * 2.0 - 1.0, y * 2.0 - 1.0, 0.0, 1.0);
}
";

/// A video frame living on the GPU. Cheap to clone (shares the texture).
#[derive(Clone)]
pub struct VideoTex {
    pub texture: Arc<wgpu::Texture>,
    pub width: u32,
    pub height: u32,
}

impl VideoTex {
    pub fn view(&self) -> wgpu::TextureView {
        self.texture.create_view(&wgpu::TextureViewDescriptor::default())
    }

    /// The same texture as something nannou's `Draw` can show. Drawn through
    /// an sRGB view of the (Unorm) storage so colors come out unchanged.
    pub fn for_draw(&self) -> nannou::wgpu::Texture {
        let descriptor = wgpu::TextureDescriptor {
            label: None,
            size: wgpu::Extent3d { width: self.width, height: self.height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        };
        nannou::wgpu::Texture::from_handle_and_descriptor(self.texture.clone(), descriptor)
    }

    /// A drawable view of this texture, reusing `cached` when it already
    /// points at the same texture so nothing is rebuilt per frame.
    pub fn to_draw(&self, cached: Option<DrawTex>) -> DrawTex {
        match cached {
            Some(c) if Arc::ptr_eq(&c.source, &self.texture) => c,
            _ => DrawTex { source: self.texture.clone(), view: self.for_draw().view().build() },
        }
    }
}

/// What the UI draws for a module: an sRGB view built once per texture.
#[derive(Clone)]
pub struct DrawTex {
    source: Arc<wgpu::Texture>,
    pub view: nannou::wgpu::TextureView,
}

pub fn create_texture(device: &wgpu::Device, width: u32, height: u32, usage: wgpu::TextureUsages) -> VideoTex {
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: FORMAT,
        usage,
        view_formats: &[wgpu::TextureFormat::Rgba8UnormSrgb],
    });
    VideoTex { texture: Arc::new(texture), width, height }
}

/// A module's render target: reused frame to frame, recreated only when
/// the resolution changes.
pub fn ensure_target(device: &wgpu::Device, slot: &mut Option<VideoTex>, width: u32, height: u32) -> VideoTex {
    let usage = wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC;
    ensure_texture(device, slot, width, height, usage)
}

/// The texture in `slot`, recreated only when the resolution changes.
pub fn ensure_texture(
    device: &wgpu::Device,
    slot: &mut Option<VideoTex>,
    width: u32,
    height: u32,
    usage: wgpu::TextureUsages,
) -> VideoTex {
    if let Some(t) = slot.as_ref().filter(|t| (t.width, t.height) == (width, height)) {
        return t.clone();
    }
    let t = create_texture(device, width, height, usage);
    *slot = Some(t.clone());
    t
}

/// One plane of a planar YUV frame (luma, or a chroma channel).
pub const PLANE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;

/// A single-channel texture for one YUV plane, recreated only on resize.
pub fn ensure_plane(device: &wgpu::Device, slot: &mut Option<VideoTex>, width: u32, height: u32) -> VideoTex {
    if let Some(t) = slot.as_ref().filter(|t| (t.width, t.height) == (width, height)) {
        return t.clone();
    }
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("yuv-plane"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: PLANE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    let t = VideoTex { texture: Arc::new(texture), width, height };
    *slot = Some(t.clone());
    t
}

/// Writes tightly packed bytes (one per pixel) into a plane texture.
pub fn write_plane(queue: &wgpu::Queue, tex: &VideoTex, bytes: &[u8]) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        bytes,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(tex.width), rows_per_image: Some(tex.height) },
        wgpu::Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
    );
}

pub fn clear_pass_ops() -> wgpu::Operations<wgpu::Color> {
    wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLACK), store: true }
}

pub fn uniform_buffer(device: &wgpu::Device, size: usize) -> wgpu::Buffer {
    device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: size as u64,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    })
}

// --- Blend ---------------------------------------------------------------

/// `mode` follows `BlendMode`'s order (see mix/blend.rs). `mix_amount` is
/// B's opacity over A, except Wipe (curtain position) and Dissolve
/// (noise threshold).
const BLEND_FS: &str = "
struct Params { mix_amount: f32, mode: f32, pad1: f32, pad2: f32 };
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex_a: texture_2d<f32>;
@group(0) @binding(3) var tex_b: texture_2d<f32>;

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

fn hash(p: vec2<f32>) -> f32 {
    return fract(sin(dot(p, vec2<f32>(12.9898, 78.233))) * 43758.5453);
}

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    // Sampling by normalized uv resamples whichever input has a different
    // resolution for free.
    let a = textureSample(tex_a, samp, in.uv).rgb;
    let b = textureSample(tex_b, samp, in.uv).rgb;
    let t = params.mix_amount;
    var out = a;
    switch i32(params.mode + 0.5) {
        case 1: { out = mix(a, min(a + b, vec3<f32>(1.0)), t); }
        case 2: { out = mix(a, max(a - b, vec3<f32>(0.0)), t); }
        case 3: { out = mix(a, a * b, t); }
        case 4: { out = mix(a, 1.0 - (1.0 - a) * (1.0 - b), t); }
        case 5: {
            let lo = 2.0 * a * b;
            let hi = 1.0 - 2.0 * (1.0 - a) * (1.0 - b);
            out = mix(a, select(hi, lo, a < vec3<f32>(0.5)), t);
        }
        case 6: { out = mix(a, abs(a - b), t); }
        case 7: { out = mix(a, max(a, b), t); }
        case 8: { out = mix(a, min(a, b), t); }
        case 9: { out = mix(a, b, smoothstep(0.25, 0.35, luma(b)) * t); }
        case 10: { out = select(a, b, in.uv.x < t); }
        case 11: { out = select(a, b, hash(floor(in.pos.xy)) < t); }
        default: { out = mix(a, b, t); }
    }
    return vec4<f32>(out, 1.0);
}
";

/// The one pipeline every Blend module shares.
pub struct BlendPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl BlendPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("blend"),
            source: wgpu::ShaderSource::Wgsl(format!("{FULLSCREEN_VS}{BLEND_FS}").into()),
        });
        let texture_entry = |binding| wgpu::BindGroupLayoutEntry {
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
            label: Some("blend-layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture_entry(2),
                texture_entry(3),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("blend"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: "vs", buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        BlendPipeline { pipeline, layout, sampler }
    }

    /// Records `target = blend(a, b)`. The mix amount and mode must already
    /// have been written to `uniform` (first two f32s).
    pub fn record(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &wgpu::Buffer,
        a: &VideoTex,
        b: &VideoTex,
        target: &VideoTex,
    ) {
        let (view_a, view_b, view_t) = (a.view(), b.view(), target.view());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&view_a) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&view_b) },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("blend"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view_t,
                resolve_target: None,
                ops: clear_pass_ops(),
            })],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

/// Test helper: copies a texture back to the CPU as tightly packed RGBA.
#[cfg(test)]
pub fn read_back(device: &wgpu::Device, queue: &wgpu::Queue, tex: &VideoTex) -> Vec<u8> {
    let padded = (tex.width * 4).div_ceil(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT) * wgpu::COPY_BYTES_PER_ROW_ALIGNMENT;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: (padded * tex.height) as u64,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::ImageCopyTexture {
            texture: &tex.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::ImageCopyBuffer {
            buffer: &buffer,
            layout: wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(padded), rows_per_image: Some(tex.height) },
        },
        wgpu::Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
    );
    queue.submit(Some(encoder.finish()));
    let slice = buffer.slice(..);
    let (tx, rx) = std::sync::mpsc::channel();
    slice.map_async(wgpu::MapMode::Read, move |r| {
        let _ = tx.send(r);
    });
    device.poll(wgpu::Maintain::Wait);
    rx.recv().unwrap().unwrap();
    let mapped = slice.get_mapped_range();
    let row = (tex.width * 4) as usize;
    let mut out = Vec::new();
    for y in 0..tex.height as usize {
        out.extend_from_slice(&mapped[y * padded as usize..y * padded as usize + row]);
    }
    out
}

/// Test helper: uploads tightly packed RGBA as a sampled texture.
#[cfg(test)]
pub fn upload(device: &wgpu::Device, queue: &wgpu::Queue, width: u32, height: u32, rgba: &[u8]) -> VideoTex {
    let tex = create_texture(device, width, height, wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST);
    write_rgba(queue, &tex, rgba);
    tex
}

pub fn write_rgba(queue: &wgpu::Queue, tex: &VideoTex, rgba: &[u8]) {
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex.texture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        rgba,
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(tex.width * 4), rows_per_image: Some(tex.height) },
        wgpu::Extent3d { width: tex.width, height: tex.height, depth_or_array_layers: 1 },
    );
}

// --- YUV 4:2:0 -> RGBA ------------------------------------------------------

/// Video files decode to planar YUV 4:2:0 (a full-size Y plane, quarter-size
/// U and V); this turns the three planes into RGBA. `params.x` = full-range
/// levels, `params.y` = BT.709 (else BT.601) colors.
const YUV420_FS: &str = "
@group(0) @binding(0) var<uniform> params: vec4<f32>;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex_y: texture_2d<f32>;
@group(0) @binding(3) var tex_u: texture_2d<f32>;
@group(0) @binding(4) var tex_v: texture_2d<f32>;

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    var y = textureSample(tex_y, samp, in.uv).r;
    var u = textureSample(tex_u, samp, in.uv).r - 0.5;
    var v = textureSample(tex_v, samp, in.uv).r - 0.5;
    if (params.x < 0.5) {
        y = (y - 16.0 / 255.0) * (255.0 / 219.0);
        u = u * (255.0 / 224.0);
        v = v * (255.0 / 224.0);
    }
    var rgb: vec3<f32>;
    if (params.y > 0.5) {
        rgb = vec3<f32>(y + 1.5748 * v, y - 0.1873 * u - 0.4681 * v, y + 1.8556 * u);
    } else {
        rgb = vec3<f32>(y + 1.402 * v, y - 0.344136 * u - 0.714136 * v, y + 1.772 * u);
    }
    return vec4<f32>(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0)), 1.0);
}
";

pub struct Yuv420Pipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
}

impl Yuv420Pipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuv420"),
            source: wgpu::ShaderSource::Wgsl(format!("{FULLSCREEN_VS}{YUV420_FS}").into()),
        });
        let texture = |binding| wgpu::BindGroupLayoutEntry {
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
            label: Some("yuv420-layout"),
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
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
                texture(2),
                texture(3),
                texture(4),
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("yuv420"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: "vs", buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState { format: FORMAT, blend: None, write_mask: wgpu::ColorWrites::ALL })],
            }),
            multiview: None,
        });
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        Yuv420Pipeline { pipeline, layout, sampler }
    }

    /// Records `target = rgb(planes)`. `uniform` is the caller's own 16-byte
    /// buffer (a shared one would be overwritten by the last video in the
    /// frame before any pass runs).
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        uniform: &wgpu::Buffer,
        planes: [&VideoTex; 3],
        target: &VideoTex,
        full_range: bool,
        bt709: bool,
    ) {
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&(full_range as u8 as f32).to_ne_bytes());
        bytes[4..8].copy_from_slice(&(bt709 as u8 as f32).to_ne_bytes());
        queue.write_buffer(uniform, 0, &bytes);
        let views = planes.map(|p| p.view());
        let view_t = target.view();
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: uniform.as_entire_binding() },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&self.sampler) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&views[0]) },
                wgpu::BindGroupEntry { binding: 3, resource: wgpu::BindingResource::TextureView(&views[1]) },
                wgpu::BindGroupEntry { binding: 4, resource: wgpu::BindingResource::TextureView(&views[2]) },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("yuv420"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment { view: &view_t, resolve_target: None, ops: clear_pass_ops() })],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

// --- YUYV -> RGBA ----------------------------------------------------------

/// Camera YUYV (4:2:2) frames are uploaded as-is into a half-width RGBA
/// texture — one texel per `Y0 U Y1 V` group — and converted here, instead
/// of on the capture thread's CPU. Same integer BT.601 math as nokhwa's
/// `yuyv444_to_rgb`, so colors match the CPU path.
const YUYV_FS: &str = "
@group(0) @binding(0) var src: texture_2d<f32>;

@fragment
fn fs(@builtin(position) pos: vec4<f32>) -> @location(0) vec4<f32> {
    let x = u32(pos.x);
    let t = textureLoad(src, vec2<u32>(x / 2u, u32(pos.y)), 0) * 255.0;
    let luma = select(t.r, t.b, (x & 1u) == 1u);
    let c = (luma - 16.0) * 298.0;
    let d = t.g - 128.0;
    let e = t.a - 128.0;
    let rgb = vec3<f32>(c + 409.0 * e, c - 100.0 * d - 208.0 * e, c + 516.0 * d) + 128.0;
    return vec4<f32>(clamp(floor(rgb / 256.0), vec3<f32>(0.0), vec3<f32>(255.0)) / 255.0, 1.0);
}
";

pub struct YuyvPipeline {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
}

impl YuyvPipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("yuyv"),
            source: wgpu::ShaderSource::Wgsl(format!("{POSITION_VS}{YUYV_FS}").into()),
        });
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("yuyv-layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::FRAGMENT,
                ty: wgpu::BindingType::Texture {
                    sample_type: wgpu::TextureSampleType::Float { filterable: false },
                    view_dimension: wgpu::TextureViewDimension::D2,
                    multisampled: false,
                },
                count: None,
            }],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("yuyv"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState { module: &module, entry_point: "vs", buffers: &[] },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: "fs",
                targets: &[Some(wgpu::ColorTargetState {
                    format: FORMAT,
                    blend: None,
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });
        YuyvPipeline { pipeline, layout }
    }

    /// Records `target = rgba(src)`; `src` is the half-width packed texture.
    pub fn record(&self, device: &wgpu::Device, encoder: &mut wgpu::CommandEncoder, src: &VideoTex, target: &VideoTex) {
        let (view_src, view_t) = (src.view(), target.view());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&view_src) }],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("yuyv"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &view_t,
                resolve_target: None,
                ops: clear_pass_ops(),
            })],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device() -> Option<(wgpu::Device, wgpu::Queue)> {
        let adapter = pollster::block_on(wgpu::Instance::default().request_adapter(&Default::default()))?;
        pollster::block_on(adapter.request_device(&Default::default(), None)).ok()
    }

    #[test]
    fn blend_mixes_and_resamples() {
        let Some((device, queue)) = device() else { return };
        let red = upload(&device, &queue, 4, 4, &[255, 0, 0, 255].repeat(16));
        let blue = upload(&device, &queue, 2, 2, &[0, 0, 255, 255].repeat(4)); // different size
        let pipeline = BlendPipeline::new(&device);
        let out = run_blend(&device, &queue, &pipeline, &red, &blue, 0.25, 0.0);
        for px in out.chunks(4) {
            assert!((px[0] as i32 - 191).abs() <= 2 && (px[2] as i32 - 64).abs() <= 2, "{px:?}");
        }
    }

    fn run_blend(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        pipeline: &BlendPipeline,
        a: &VideoTex,
        b: &VideoTex,
        mix: f32,
        mode: f32,
    ) -> Vec<u8> {
        let uniform = uniform_buffer(device, 16);
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&mix.to_ne_bytes());
        bytes[4..8].copy_from_slice(&mode.to_ne_bytes());
        queue.write_buffer(&uniform, 0, &bytes);
        let mut slot = None;
        let target = ensure_target(device, &mut slot, a.width, a.height);
        let mut encoder = device.create_command_encoder(&Default::default());
        pipeline.record(device, &mut encoder, &uniform, a, b, &target);
        queue.submit(Some(encoder.finish()));
        read_back(device, queue, &target)
    }

    #[test]
    fn blend_modes() {
        let Some((device, queue)) = device() else { return };
        let pipeline = BlendPipeline::new(&device);
        let a = upload(&device, &queue, 4, 4, &[200, 100, 50, 255].repeat(16));
        let b = upload(&device, &queue, 4, 4, &[100, 150, 50, 255].repeat(16));
        let px = |mode: f32| run_blend(&device, &queue, &pipeline, &a, &b, 1.0, mode)[..3].to_vec();
        let near = |got: Vec<u8>, want: [i32; 3]| {
            assert!(got.iter().zip(want).all(|(g, w)| (*g as i32 - w).abs() <= 2), "{got:?} vs {want:?}");
        };
        near(px(1.0), [255, 250, 100]); // add
        near(px(3.0), [78, 59, 10]); // multiply
        near(px(6.0), [100, 50, 0]); // difference
        // Wipe at 0.5: left half B, right half A.
        let out = run_blend(&device, &queue, &pipeline, &a, &b, 0.5, 10.0);
        assert_eq!(&out[..3], &[100, 150, 50]);
        assert_eq!(&out[12..15], &[200, 100, 50]);
    }

    /// The GPU conversion must match nokhwa's CPU one byte for byte (±1).
    #[test]
    fn yuyv_converts_like_nokhwa() {
        let Some((device, queue)) = device() else { return };
        let (w, h) = (8u32, 4u32);
        let yuyv: Vec<u8> = (0..w * h * 2).map(|i| (i * 37 % 256) as u8).collect();
        let packed = upload(&device, &queue, w / 2, h, &yuyv);
        let mut slot = None;
        let target = ensure_target(&device, &mut slot, w, h);
        let mut encoder = device.create_command_encoder(&Default::default());
        YuyvPipeline::new(&device).record(&device, &mut encoder, &packed, &target);
        queue.submit(Some(encoder.finish()));

        let mut expected = vec![0u8; (w * h * 4) as usize];
        nokhwa::utils::buf_yuyv422_to_rgb(&yuyv, &mut expected, true).unwrap();
        let out = read_back(&device, &queue, &target);
        for (a, b) in out.iter().zip(&expected) {
            assert!((*a as i32 - *b as i32).abs() <= 1, "gpu {a} vs cpu {b}");
        }
    }

    /// Mid-gray luma with neutral chroma is gray; full-range white is white;
    /// a strong V is red-dominant.
    #[test]
    fn yuv420_converts_planes() {
        let Some((device, queue)) = device() else { return };
        let pipeline = Yuv420Pipeline::new(&device);
        let uniform = uniform_buffer(&device, 16);
        let run = |y: u8, u: u8, v: u8, full: bool| {
            let (mut sy, mut su, mut sv) = (None, None, None);
            let py = ensure_plane(&device, &mut sy, 4, 4);
            let pu = ensure_plane(&device, &mut su, 2, 2);
            let pv = ensure_plane(&device, &mut sv, 2, 2);
            write_plane(&queue, &py, &[y; 16]);
            write_plane(&queue, &pu, &[u; 4]);
            write_plane(&queue, &pv, &[v; 4]);
            let mut slot = None;
            let target = ensure_target(&device, &mut slot, 4, 4);
            let mut encoder = device.create_command_encoder(&Default::default());
            pipeline.record(&device, &queue, &mut encoder, &uniform, [&py, &pu, &pv], &target, full, true);
            queue.submit(Some(encoder.finish()));
            read_back(&device, &queue, &target)[..3].to_vec()
        };
        let gray = run(126, 128, 128, false);
        assert!(gray.iter().all(|c| (*c as i32 - 128).abs() <= 3), "{gray:?}");
        let white = run(255, 128, 128, true);
        assert!(white.iter().all(|c| *c >= 252), "{white:?}");
        let red = run(80, 100, 230, false);
        assert!(red[0] > 150 && red[1] < 60, "{red:?}");
    }

    #[test]
    fn texture_can_be_viewed_for_drawing() {
        let Some((device, _queue)) = device() else { return };
        let tex = create_texture(&device, 8, 8, wgpu::TextureUsages::TEXTURE_BINDING);
        let drawable = tex.for_draw();
        let _view = drawable.view().build(); // sRGB view over the Unorm storage
    }
}
