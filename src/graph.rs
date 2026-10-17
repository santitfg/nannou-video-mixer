//! Evaluates the video graph once per tick, entirely on the GPU.
//!
//! Only modules something is looking at are evaluated: Output, plus any
//! module whose own preview is on, plus everything upstream of those. Each
//! module is visited once (inputs first), and all of their passes are
//! recorded into a single command encoder and submitted together.

use crate::modules::effects::shader::compile_pending;
use crate::modules::sources::camera::CameraState;
use crate::modules::{input_source, module_number, Connection, ModuleBody, PlacedModule};
use crate::gpu::VideoTex;
use crate::Model;
use nannou::prelude::*;
use std::collections::{HashMap, HashSet};
use wgpu_upstream as wgpu;

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

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("graph") });
    let mut results: HashMap<u64, VideoTex> = HashMap::new();

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
            ModuleBody::Camera(_) => None, // handled below, needs &mut
            ModuleBody::Output(_) => video_in(&results, 0),
            ModuleBody::Blend(blend) => {
                let a = video_in(&results, 0);
                let b = video_in(&results, 1);
                let mix = number_in(&model.modules, 2).unwrap_or(blend.mix);
                let ModuleBody::Blend(blend) = &mut model.modules[idx].body else { unreachable!() };
                blend.record(&model.blend_pipeline, device, queue, &mut encoder, a, b, mix)
            }
            ModuleBody::Shader(shader) => {
                if !shader.is_ready() {
                    None
                } else {
                    let channels_n = shader.channels();
                    let props: Vec<_> = shader.props().to_vec();
                    let channels: Vec<Option<VideoTex>> = (0..channels_n).map(|n| video_in(&results, n)).collect();
                    let values: Vec<f32> = props
                        .iter()
                        .enumerate()
                        .map(|(i, p)| p.clamp(number_in(&model.modules, channels_n + i).unwrap_or(p.default)))
                        .collect();
                    let ModuleBody::Shader(shader) = &mut model.modules[idx].body else { unreachable!() };
                    shader.record(device, queue, &mut encoder, &channels, values, now, delta)
                }
            }
            ModuleBody::Number(_) | ModuleBody::Mult(_) | ModuleBody::Lfo(_) => None,
        };

        let produced = match &mut model.modules[idx].body {
            ModuleBody::Camera(CameraState { worker: Some(worker), gpu, .. }) => worker.upload(device, queue, gpu),
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
        chrome.preview_texture = if module.id == output_id || chrome.show_preview {
            results.get(&module.id).map(|t| t.for_draw())
        } else {
            None
        };
    }
}
