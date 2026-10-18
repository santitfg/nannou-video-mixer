use super::shader_file::{
    build_glsl, parse_shader, props_size, shader_path, ParsedShader, PropKind, Property, FEEDBACK_BINDING,
    GLOBALS_SIZE, MAX_CHANNELS,
};
use crate::gpu::{clear_pass_ops, create_texture, ensure_target, uniform_buffer, VideoTex, FORMAT, POSITION_VS};
use crate::modules::param::Param;
use crate::modules::{Connection, ModuleBody, PlacedModule, PortType};
use std::collections::BTreeMap;
use crate::ModuleChrome;
use std::borrow::Cow;
use std::time::{Duration, Instant, SystemTime};
use wgpu_upstream as wgpu;

const DEFAULT_SIZE: (u32, u32) = (640, 360);
const RELOAD_CHECK_EVERY: Duration = Duration::from_millis(500);

/// SHADER: runs a Shadertoy-style fragment shader from the shaders folder.
/// Video inputs feed `iChannel0..3`; every `Properties` entry in the file
/// is a Number input (falling back to its default when unplugged).
pub struct ShaderState {
    pub chrome: ModuleChrome,
    /// File name inside the shaders folder.
    pub file: Option<String>,
    /// Limits the list to one category (the node came from the sidebar's
    /// category entry). `None` lists every shader of the node's role.
    pub category: Option<String>,
    pub dropdown_open: bool,
    /// The `Enum` property whose options dropdown is open.
    pub prop_dropdown: Option<usize>,
    pub compiled: Option<Compiled>,
    pub error: Option<String>,
    /// One number box per property, in `props` order.
    pub params: Vec<Param>,
    /// Boxes by property name, kept across recompiles (and filled from a
    /// saved composition) so tuning survives editing the file.
    pub tuned: BTreeMap<String, Param>,
    /// The values the properties had on the last rendered frame.
    pub live: Vec<f32>,
    needs_compile: bool,
    mtime: Option<SystemTime>,
    last_check: Instant,
    frame_index: i32,
}

impl ShaderState {
    pub fn new() -> Self {
        ShaderState {
            chrome: ModuleChrome::new(),
            file: None,
            category: None,
            dropdown_open: false,
            prop_dropdown: None,
            compiled: None,
            error: None,
            params: Vec::new(),
            tuned: BTreeMap::new(),
            live: Vec::new(),
            needs_compile: false,
            mtime: None,
            last_check: Instant::now(),
            frame_index: 0,
        }
    }

    pub fn select(&mut self, file: Option<String>) {
        self.needs_compile = file.is_some();
        self.file = file;
        self.compiled = None;
        self.error = None;
        self.prop_dropdown = None;
        self.params.clear();
        self.tuned.clear();
        self.live.clear();
        self.mtime = None;
    }

    pub fn channels(&self) -> usize {
        self.compiled.as_ref().map_or(0, |c| c.channels)
    }

    pub fn props(&self) -> &[Property] {
        self.compiled.as_ref().map_or(&[], |c| &c.props)
    }

    pub fn input_types(&self) -> Vec<PortType> {
        let mut types = vec![PortType::Video; self.channels()];
        types.extend(std::iter::repeat(PortType::Number).take(self.props().len()));
        types
    }

    /// The value property `i` renders with: its box's own value, or the
    /// wired signal mapped onto the box's range. Int properties round.
    pub fn prop_value(&self, i: usize, input: Option<f32>) -> f32 {
        let Some(param) = self.params.get(i) else { return 0.0 };
        let v = param.resolve(input);
        match self.props().get(i) {
            Some(p) if matches!(p.kind, PropKind::Int | PropKind::Enum(_)) => p.clamp(v),
            _ => v,
        }
    }

    /// Writes property `i`'s box back into `tuned`, after it was edited.
    pub fn remember(&mut self, i: usize) {
        if let (Some(prop), Some(param)) = (self.props().get(i), self.params.get(i)) {
            self.tuned.insert(prop.name.clone(), *param);
        }
    }

    /// Rebuilds `params` for freshly compiled props, reusing tuned boxes.
    fn rebuild_params(&mut self) {
        let props = self.props().to_vec();
        self.params = props.iter().map(|p| self.tuned.get(&p.name).copied().unwrap_or_else(|| p.param())).collect();
    }
}

pub struct Compiled {
    pub channels: usize,
    pub feedback: bool,
    pub props: Vec<Property>,
    gpu: Gpu,
}

struct Gpu {
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    globals: wgpu::Buffer,
    props: wgpu::Buffer,
    /// Bound in place of any channel with nothing plugged in.
    placeholder: VideoTex,
    /// Render targets. Shaders reading `iFeedback` alternate between the
    /// two, sampling last frame's while drawing into the other.
    targets: [Option<VideoTex>; 2],
    current: usize,
}

// --- Compilation ---------------------------------------------------------

/// Everything that can be checked without a GPU: the Properties header,
/// the GLSL itself (parsed and validated by naga).
fn front_end(source: &str) -> Result<(ParsedShader, naga::Module), String> {
    let parsed = parse_shader(source)?;
    let (glsl, prelude_lines) = build_glsl(&parsed);

    let module = naga::front::glsl::Frontend::default()
        .parse(&naga::front::glsl::Options::from(naga::ShaderStage::Fragment), &glsl)
        .map_err(|errors| {
            errors
                .iter()
                .map(|e| match e.meta.location(&glsl).line_number as usize {
                    line if line > prelude_lines => format!("line {}: {}", line - prelude_lines, e.kind),
                    _ => e.kind.to_string(),
                })
                .collect::<Vec<_>>()
                .join("\n")
        })?;
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::empty())
        .validate(&module)
        .map_err(|e| e.as_inner().to_string())?;
    Ok((parsed, module))
}

fn compile(device: &wgpu::Device, file: &str) -> Result<Compiled, String> {
    let source = std::fs::read_to_string(shader_path(file)).map_err(|e| format!("{file}: {e}"))?;
    compile_from_source(device, &source)
}

fn compile_from_source(device: &wgpu::Device, source: &str) -> Result<Compiled, String> {
    // naga's GLSL front end can panic on odd input: that's a shader error,
    // not a crash.
    let (parsed, module) = std::panic::catch_unwind(|| front_end(source))
        .map_err(|_| "the shader compiler crashed on this file".to_string())??;

    // wgpu reports validation failures through the device's error handler,
    // which panics by default; catch them in a scope and show them instead.
    device.push_error_scope(wgpu::ErrorFilter::Validation);
    let compiled = build_pipeline(device, parsed, module);
    match pollster::block_on(device.pop_error_scope()) {
        Some(err) => Err(err.to_string()),
        None => Ok(compiled),
    }
}

fn build_pipeline(device: &wgpu::Device, parsed: ParsedShader, module: naga::Module) -> Compiled {
    let fragment = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader-fragment"),
        source: wgpu::ShaderSource::Naga(Cow::Owned(module)),
    });
    let vertex = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("shader-vertex"),
        source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(POSITION_VS)),
    });

    let uniform = |binding: u32| wgpu::BindGroupLayoutEntry {
        binding,
        visibility: wgpu::ShaderStages::FRAGMENT,
        ty: wgpu::BindingType::Buffer {
            ty: wgpu::BufferBindingType::Uniform,
            has_dynamic_offset: false,
            min_binding_size: None,
        },
        count: None,
    };
    let mut entries = vec![
        uniform(0),
        uniform(1),
        wgpu::BindGroupLayoutEntry {
            binding: 2,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
            count: None,
        },
    ];
    for binding in (3..3 + MAX_CHANNELS as u32).chain([FEEDBACK_BINDING]) {
        entries.push(wgpu::BindGroupLayoutEntry {
            binding,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Texture {
                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                view_dimension: wgpu::TextureViewDimension::D2,
                multisampled: false,
            },
            count: None,
        });
    }
    let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("shader-bind-layout"),
        entries: &entries,
    });
    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("shader-pipeline-layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });
    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("shader-pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &vertex,
            entry_point: "vs",
            buffers: &[],
        },
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        fragment: Some(wgpu::FragmentState {
            module: &fragment,
            entry_point: "main",
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
    // Freshly created textures are zero-filled, so this reads as transparent black.
    let placeholder = create_texture(device, 1, 1, wgpu::TextureUsages::TEXTURE_BINDING);

    Compiled {
        channels: parsed.channels,
        feedback: parsed.feedback,
        gpu: Gpu {
            pipeline,
            layout,
            sampler,
            globals: uniform_buffer(device, GLOBALS_SIZE),
            props: uniform_buffer(device, props_size(&parsed.props)),
            placeholder,
            targets: [None, None],
            current: 0,
        },
        props: parsed.props,
    }
}

// --- Per-tick work ---------------------------------------------------------

/// Recompiles every Shader that asked for it (new selection) or whose file
/// changed on disk, and drops cables left dangling by a changed port list.
pub fn compile_pending(device: &wgpu::Device, modules: &mut [PlacedModule], connections: &mut Vec<Connection>) {
    for idx in 0..modules.len() {
        let id = modules[idx].id;
        let mut ports_changed = false;
        if let ModuleBody::Shader(s) = &mut modules[idx].body {
            if s.last_check.elapsed() > RELOAD_CHECK_EVERY {
                s.last_check = Instant::now();
                let mtime = s
                    .file
                    .as_ref()
                    .and_then(|f| std::fs::metadata(shader_path(f)).ok())
                    .and_then(|md| md.modified().ok());
                if (s.compiled.is_some() || s.error.is_some()) && mtime != s.mtime {
                    s.needs_compile = true;
                }
            }
            if s.needs_compile {
                s.needs_compile = false;
                if let Some(file) = s.file.clone() {
                    s.mtime = std::fs::metadata(shader_path(&file)).ok().and_then(|md| md.modified().ok());
                    let started = Instant::now();
                    match compile(device, &file) {
                        Ok(compiled) => {
                            crate::dlog!("shader {file}: compiled in {:.1}ms", started.elapsed().as_secs_f32() * 1000.0);
                            s.compiled = Some(compiled);
                            s.error = None;
                            s.rebuild_params();
                            ports_changed = true;
                        }
                        // Keep running the last good version on a typo.
                        Err(err) => {
                            crate::dlog!("shader {file}: {}", err.lines().next().unwrap_or(""));
                            s.error = Some(err);
                        }
                    }
                }
            }
            if s.file.is_none() {
                ports_changed = true;
            }
        }
        if ports_changed {
            let count = modules[idx].input_types().len();
            connections.retain(|c| c.to != id || c.to_input < count);
        }
    }
}

impl ShaderState {
    pub fn is_ready(&self) -> bool {
        self.compiled.is_some()
    }

    /// Records this shader's pass into `encoder`. `channels` holds what's
    /// plugged into each `iChannelN`; `values` the already-resolved property
    /// values (wired number or default).
    pub fn record(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        channels: &[Option<VideoTex>],
        values: Vec<f32>,
        time: f32,
        delta: f32,
        max_height: Option<u32>,
    ) -> Option<VideoTex> {
        let frame = self.frame_index;
        self.frame_index = self.frame_index.wrapping_add(1);
        let compiled = self.compiled.as_mut()?;

        let (width, height) = channels
            .iter()
            .flatten()
            .map(|t| (t.width, t.height))
            .next()
            .unwrap_or(DEFAULT_SIZE);
        // Thumbnail-only passes shrink to `max_height`, keeping the aspect.
        let (width, height) = match max_height {
            Some(cap) if height > cap => (((width as u64 * cap as u64) / height as u64).max(1) as u32, cap),
            _ => (width, height),
        };
        let gpu = &mut compiled.gpu;
        // Feedback shaders draw into the other target each frame; a size
        // change restarts both (black) so they never disagree.
        let previous = if compiled.feedback {
            let prev = gpu.targets[gpu.current].clone().filter(|t| (t.width, t.height) == (width, height));
            if prev.is_none() {
                gpu.targets = [None, None];
            }
            gpu.current = 1 - gpu.current;
            prev
        } else {
            None
        };
        let target = ensure_target(device, &mut gpu.targets[gpu.current], width, height);

        // Uniforms. Layout mirrors the blocks in `build_glsl` (std140).
        let push_f = |buf: &mut Vec<u8>, v: f32| buf.extend_from_slice(&v.to_ne_bytes());
        let mut globals = Vec::with_capacity(GLOBALS_SIZE);
        for v in [width as f32, height as f32, 1.0, 0.0] {
            push_f(&mut globals, v); // iResolution
        }
        for _ in 0..8 {
            push_f(&mut globals, 0.0); // iMouse, iDate (not supported yet)
        }
        for n in 0..MAX_CHANNELS {
            let res = channels
                .get(n)
                .and_then(|c| c.as_ref())
                .map_or([0.0; 4], |t| [t.width as f32, t.height as f32, 1.0, 0.0]);
            for v in res {
                push_f(&mut globals, v); // iChannelResolution[n]
            }
        }
        push_f(&mut globals, time);
        push_f(&mut globals, delta);
        globals.extend_from_slice(&frame.to_ne_bytes());
        push_f(&mut globals, if delta > 0.0 { 1.0 / delta } else { 0.0 });
        queue.write_buffer(&compiled.gpu.globals, 0, &globals);

        let mut props = vec![0u8; props_size(&compiled.props)];
        for (i, (prop, value)) in compiled.props.iter().zip(&values).enumerate() {
            let at = 16 + i * 4;
            let bytes = match prop.kind {
                PropKind::Int | PropKind::Enum(_) => (*value as i32).to_ne_bytes(),
                _ => value.to_ne_bytes(),
            };
            props[at..at + 4].copy_from_slice(&bytes);
        }
        queue.write_buffer(&compiled.gpu.props, 0, &props);
        self.live = values;

        let gpu = &compiled.gpu;
        let views: Vec<wgpu::TextureView> = (0..MAX_CHANNELS)
            .map(|n| match channels.get(n).and_then(|c| c.as_ref()) {
                Some(t) if n < compiled.channels => t.view(),
                _ => gpu.placeholder.view(),
            })
            .collect();
        let mut entries = vec![
            wgpu::BindGroupEntry { binding: 0, resource: gpu.globals.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: gpu.props.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&gpu.sampler) },
        ];
        for (n, view) in views.iter().enumerate() {
            entries.push(wgpu::BindGroupEntry {
                binding: 3 + n as u32,
                resource: wgpu::BindingResource::TextureView(view),
            });
        }
        let feedback_view = previous.as_ref().unwrap_or(&gpu.placeholder).view();
        entries.push(wgpu::BindGroupEntry {
            binding: FEEDBACK_BINDING,
            resource: wgpu::BindingResource::TextureView(&feedback_view),
        });
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &gpu.layout,
            entries: &entries,
        });

        let target_view = target.view();
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("shader"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &target_view,
                resolve_target: None,
                ops: clear_pass_ops(),
            })],
            depth_stencil_attachment: None,
        });
        pass.set_pipeline(&gpu.pipeline);
        pass.set_bind_group(0, &bind_group, &[]);
        pass.draw(0..3, 0..1);
        drop(pass);
        Some(target)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gpu::upload;
    use crate::modules::effects::shader_file::scan_library;

    fn gpu() -> Option<(wgpu::Device, wgpu::Queue)> {
        let instance = wgpu::Instance::default();
        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default(), None)).ok()
    }

    fn run(device: &wgpu::Device, queue: &wgpu::Queue, compiled: Compiled, inputs: &[Option<VideoTex>]) -> Vec<u8> {
        let mut state = ShaderState::new();
        let values = compiled.props.iter().map(|p| p.default).collect();
        state.compiled = Some(compiled);
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let out = state.record(device, queue, &mut encoder, inputs, values, 1.0, 0.016, None).unwrap();
        queue.submit(Some(encoder.finish()));
        crate::gpu::read_back(device, queue, &out)
    }

    #[test]
    fn bundled_shaders_compile_and_render() {
        let library: Vec<String> = scan_library().into_iter().map(|i| i.file).collect();
        assert!(!library.is_empty(), "no shaders found in the shaders folder");

        let Some((device, queue)) = gpu() else {
            eprintln!("no GPU adapter; checking the front end only");
            for file in &library {
                let src = std::fs::read_to_string(shader_path(file)).unwrap();
                front_end(&src).unwrap_or_else(|e| panic!("{file}: {e}"));
            }
            return;
        };

        // A horizontal red ramp.
        let (w, h) = (64u32, 32u32);
        let mut rgba = Vec::new();
        for _ in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 255 / (w - 1)) as u8, 0, 0, 255]);
            }
        }
        let input = upload(&device, &queue, w, h, &rgba);

        for file in &library {
            let compiled = compile(&device, file).unwrap_or_else(|e| panic!("{file}: {e}"));
            let channels = vec![Some(input.clone()); compiled.channels];
            let n = (compiled.channels, compiled.props.len());
            let out = run(&device, &queue, compiled, &channels);
            assert!(out.iter().any(|&b| b != 0), "{file}: rendered all black");
            assert!(out.chunks(4).all(|px| px[3] == 255), "{file}: alpha not opaque");
            eprintln!("{file}: ok ({} channels, {} props)", n.0, n.1);
        }
    }

    /// `texture(iChannel0, fragCoord / iResolution.xy)` must hand the image
    /// back unchanged: Shadertoy's bottom-left origin and our top-first
    /// storage cancel out.
    #[test]
    fn sampling_keeps_orientation() {
        let Some((device, queue)) = gpu() else { return };
        let (w, h) = (16u32, 16u32);
        let mut rgba = Vec::new();
        for y in 0..h {
            for x in 0..w {
                rgba.extend_from_slice(&[(x * 16) as u8, (y * 16) as u8, 0, 255]);
            }
        }
        let input = upload(&device, &queue, w, h, &rgba);

        let src = "void mainImage(out vec4 c, in vec2 p) { c = texture(iChannel0, p / iResolution.xy); }";
        let compiled = compile_from_source(&device, src).unwrap();
        let out = run(&device, &queue, compiled, &[Some(input)]);
        for (a, b) in out.chunks(4).zip(rgba.chunks(4)) {
            assert!((a[0] as i32 - b[0] as i32).abs() <= 2 && (a[1] as i32 - b[1] as i32).abs() <= 2, "{a:?} vs {b:?}");
        }
    }

    /// A shader adding its own previous frame keeps getting brighter,
    /// which only happens if the two feedback targets really alternate.
    #[test]
    fn feedback_reads_previous_frame() {
        let Some((device, queue)) = gpu() else { return };
        let src = "void mainImage(out vec4 c, in vec2 p) { c = vec4(texture(iFeedback, p / iResolution.xy).rgb + 0.1, 1.0); }";
        let mut state = ShaderState::new();
        state.compiled = Some(compile_from_source(&device, src).unwrap());
        let mut reds = Vec::new();
        for _ in 0..3 {
            let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
            let out = state.record(&device, &queue, &mut encoder, &[], vec![], 0.0, 0.016, None).unwrap();
            queue.submit(Some(encoder.finish()));
            reds.push(crate::gpu::read_back(&device, &queue, &out)[0]);
        }
        assert!(reds[0] < reds[1] && reds[1] < reds[2], "{reds:?}");
    }

    #[test]
    fn texel_fetch_and_lod_compile() {
        let src = "void mainImage(out vec4 c, in vec2 p) { c = texelFetch(iChannel0, ivec2(p), 0) + textureLod(iChannel1, p / iResolution.xy, 0.0); }";
        front_end(src).unwrap();
    }

    #[test]
    fn reports_glsl_errors_with_user_line_numbers() {
        let err = front_end("void mainImage(out vec4 c, in vec2 p) {\n  c = vec4(notDeclared);\n}").err().unwrap();
        assert!(err.contains("line 2"), "got: {err}");
    }
}
