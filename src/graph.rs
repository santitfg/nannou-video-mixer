//! Evaluates the video graph once per tick, entirely on the GPU.
//!
//! Only modules something is looking at are evaluated: Output, plus any
//! module whose own preview is on, plus everything upstream of those. Each
//! module is visited once (inputs first), and all of their passes are
//! recorded into a single command encoder and submitted together.

use crate::modules::effects::shader::compile_pending;
use crate::modules::sources::camera::CameraState;
use crate::modules::sources::video::VideoState;
use crate::modules::{input_source, multi_input, ModuleKind, module_number, reaches, Connection, ModuleBody, PlacedModule};
use crate::gpu::VideoTex;
use crate::Model;
use nannou::prelude::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use wgpu_upstream as wgpu;

/// Tallest render, in pixels, of a shader that only feeds a preview thumbnail.
const PREVIEW_RENDER_H: u32 = 270;

/// Inputs before the module that reads them, each id once. Cycles are cut
/// wherever they're first re-entered.
fn evaluation_order(connections: &[Connection], roots: &[u64]) -> Vec<u64> {
    fn visit(connections: &[Connection], id: u64, seen: &mut HashSet<u64>, order: &mut Vec<u64>) {
        if !seen.insert(id) {
            return;
        }
        for c in connections.iter().filter(|c| c.to == id) {
            visit(connections, c.from, seen, order);
        }
        order.push(id);
    }
    let mut seen = HashSet::new();
    let mut order = Vec::new();
    for &root in roots {
        visit(connections, root, &mut seen, &mut order);
    }
    order
}

pub fn run_graph(app: &App, model: &mut Model, delta: f32) {
    let now = app.time;
    let window = app.main_window();
    let device = window.device();
    let queue = window.queue();

    compile_pending(device, &mut model.modules, &mut model.connections);

    let mut roots = vec![model.output_id];
    roots.extend(
        model
            .modules
            .iter()
            .filter(|m| m.body.chrome().show_preview)
            .map(|m| m.id),
    );
    let order = evaluation_order(&model.connections, &roots);
    // Modules only a thumbnail looks at render small: a preview is ~90 px
    // tall, so a full-size pass behind it is wasted GPU time.
    let feeds_output: HashSet<u64> = order
        .iter()
        .copied()
        .filter(|&id| reaches(&model.connections, id, model.output_id, &mut Vec::new()))
        .collect();

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("graph") });
    let mut results: HashMap<u64, VideoTex> = HashMap::new();
    let mut upload_time = Duration::ZERO;

    for id in order {
        let Some(idx) = model.modules.iter().position(|m| m.id == id) else { continue };
        let video_in = |results: &HashMap<u64, VideoTex>, input: usize| {
            input_source(&model.connections, id, input).and_then(|src| results.get(&src).cloned())
        };
        let number_in = |modules: &[PlacedModule], input: usize| {
            input_source(&model.connections, id, input)
                .and_then(|src| module_number(modules, &model.connections, src, now, &mut vec![id]))
        };

        let produced = match &model.modules[idx].body {
            ModuleBody::Camera(_) | ModuleBody::Video(_) => None, // handled below, needs &mut
            ModuleBody::Output(_) => video_in(&results, 0),
            ModuleBody::Blend(blend) if model.modules[idx].kind == ModuleKind::Stack => {
                let layers: Vec<VideoTex> = multi_input::sources(&model.connections, id, 0)
                    .into_iter()
                    .filter_map(|src| results.get(&src).cloned())
                    .collect();
                let mix = blend.mix.resolve(number_in(&model.modules, 1));
                let ModuleBody::Blend(blend) = &mut model.modules[idx].body else { unreachable!() };
                blend.record_stack(&model.blend_pipeline, device, queue, &mut encoder, layers, mix)
            }
            ModuleBody::Blend(blend) => {
                let a = video_in(&results, 0);
                let b = video_in(&results, 1);
                let mix = blend.mix.resolve(number_in(&model.modules, 2));
                let ModuleBody::Blend(blend) = &mut model.modules[idx].body else { unreachable!() };
                blend.record(&model.blend_pipeline, device, queue, &mut encoder, a, b, mix)
            }
            ModuleBody::Shader(shader) => {
                if !shader.is_ready() {
                    None
                } else {
                    let channels_n = shader.channels();
                    let channels: Vec<Option<VideoTex>> = (0..channels_n).map(|n| video_in(&results, n)).collect();
                    let values: Vec<f32> = (0..shader.props().len())
                        .map(|i| shader.prop_value(i, number_in(&model.modules, channels_n + i)))
                        .collect();
                    let ModuleBody::Shader(shader) = &mut model.modules[idx].body else { unreachable!() };
                    let cap = (!feeds_output.contains(&id)).then_some(PREVIEW_RENDER_H);
                    shader.record(device, queue, &mut encoder, &channels, values, now, delta, cap)
                }
            }
            ModuleBody::Number(_)
            | ModuleBody::Mult(_)
            | ModuleBody::Lfo(_)
            | ModuleBody::Lag(_)
            | ModuleBody::Map(_)
            | ModuleBody::Wled(_)
            | ModuleBody::Osc(_) => None,
        };

        let produced = match &mut model.modules[idx].body {
            ModuleBody::Camera(CameraState { worker: Some(worker), .. }) => {
                let t = Instant::now();
                let tex = worker.upload(device, queue, &mut encoder, &model.yuyv_pipeline);
                upload_time += t.elapsed();
                tex
            }
            ModuleBody::Video(VideoState { worker: Some(worker), .. }) => {
                let t = Instant::now();
                let tex = worker.upload(device, queue, &mut encoder, &model.yuv420_pipeline);
                upload_time += t.elapsed();
                tex
            }
            _ => produced,
        };
        if let Some(tex) = produced {
            results.insert(id, tex);
        }
    }
    queue.submit(Some(encoder.finish()));

    // Hand the textures to the UI: Output's feeds the fullscreen view,
    // the rest are inline thumbnails.
    let output_id = model.output_id;
    for module in &mut model.modules {
        let chrome = module.body.chrome_mut();
        let cached = chrome.preview_texture.take();
        chrome.preview_texture = if module.id == output_id || chrome.show_preview {
            results.get(&module.id).map(|t| t.to_draw(cached))
        } else {
            None
        };
    }
    model.timings.uploads += upload_time;
}
