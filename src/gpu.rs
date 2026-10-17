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

const BLEND_FS: &str = "
struct Params { mix_amount: f32, pad0: f32, pad1: f32, pad2: f32 };
@group(0) @binding(0) var<uniform> params: Params;
@group(0) @binding(1) var samp: sampler;
@group(0) @binding(2) var tex_a: texture_2d<f32>;
@group(0) @binding(3) var tex_b: texture_2d<f32>;

@fragment
fn fs(in: VOut) -> @location(0) vec4<f32> {
    // Sampling by normalized uv resamples whichever input has a different
    // resolution for free.
    let a = textureSample(tex_a, samp, in.uv);
    let b = textureSample(tex_b, samp, in.uv);
    return mix(a, b, params.mix_amount);
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

    /// Records `target = mix(a, b, uniform.mix)`. The mix amount must already
    /// have been written to `uniform` (first f32).
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
        let uniform = uniform_buffer(&device, 16);
        let mut bytes = [0u8; 16];
        bytes[..4].copy_from_slice(&0.25f32.to_ne_bytes());
        queue.write_buffer(&uniform, 0, &bytes);

        let mut slot = None;
        let target = ensure_target(&device, &mut slot, 4, 4);
        let mut encoder = device.create_command_encoder(&Default::default());
        pipeline.record(&device, &mut encoder, &uniform, &red, &blue, &target);
        queue.submit(Some(encoder.finish()));

        let out = read_back(&device, &queue, &target);
        for px in out.chunks(4) {
            assert!((px[0] as i32 - 191).abs() <= 2 && (px[2] as i32 - 64).abs() <= 2, "{px:?}");
        }
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

    #[test]
    fn texture_can_be_viewed_for_drawing() {
        let Some((device, _queue)) = device() else { return };
        let tex = create_texture(&device, 8, 8, wgpu::TextureUsages::TEXTURE_BINDING);
        let drawable = tex.for_draw();
        let _view = drawable.view().build(); // sRGB view over the Unorm storage
    }
}
