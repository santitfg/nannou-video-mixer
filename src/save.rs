use crate::modules::effects::shader::ShaderState;
use crate::modules::math::{lfo::LfoState, mult::MultState, number::NumberState};
use crate::modules::mix::blend::BlendState;
use crate::modules::output::OutputState;
use crate::modules::sources::camera::CameraState;
use crate::modules::{Connection, ModuleBody, ModuleKind, PlacedModule};
use crate::{canvas_rect, Model, ModuleChrome, SAVE_PATH};
use nannou::prelude::*;
use serde::{Deserialize, Serialize};

// Plain, serializable snapshots of the composition — deliberately separate
// from `PlacedModule`/`ModuleBody`, which hold runtime-only things (GPU
// textures, capture threads) that can't be serialized. A camera is saved
// by device name rather than its list index, since that index isn't
// guaranteed stable across runs.

#[derive(Serialize, Deserialize)]
struct SavedModule {
    id: u64,
    kind: ModuleKind,
    pos: (f32, f32),
    minimized: bool,
    show_preview: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    camera_name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    mix: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<f32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    freq: Option<f32>,
    /// A Shader's file name inside the shaders folder.
    #[serde(skip_serializing_if = "Option::is_none")]
    shader_file: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct SavedConnection {
    from: u64,
    to: u64,
    to_input: usize,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    output_id: u64,
    modules: Vec<SavedModule>,
    connections: Vec<SavedConnection>,
}

fn build_save_file(model: &Model) -> SaveFile {
    let modules = model
        .modules
        .iter()
        .map(|m| {
            let chrome = m.body.chrome();
            let mut camera_name = None;
            let mut mix = None;
            let mut value = None;
            let mut freq = None;
            let mut shader_file = None;
            match &m.body {
                ModuleBody::Camera(cam) => {
                    camera_name = cam.wanted.clone();
                }
                ModuleBody::Blend(blend) => mix = Some(blend.mix),
                ModuleBody::Number(number) => value = Some(number.value),
                ModuleBody::Lfo(lfo) => freq = Some(lfo.freq),
                ModuleBody::Shader(shader) => shader_file = shader.file.clone(),
                ModuleBody::Output(_) | ModuleBody::Mult(_) => {}
            };
            SavedModule {
                id: m.id,
                kind: m.kind,
                pos: (m.pos.x, m.pos.y),
                minimized: chrome.minimized,
                show_preview: chrome.show_preview,
                camera_name,
                mix,
                value,
                freq,
                shader_file,
            }
        })
        .collect();

    let connections = model
        .connections
        .iter()
        .map(|c| SavedConnection {
            from: c.from,
            to: c.to,
            to_input: c.to_input,
        })
        .collect();

    SaveFile {
        output_id: model.output_id,
        modules,
        connections,
    }
}

pub fn save_composition(model: &Model) -> std::io::Result<()> {
    let file = build_save_file(model);
    let json = serde_json::to_string_pretty(&file).expect("composition is always serializable");
    std::fs::write(SAVE_PATH, json)
}

/// Replaces the current composition with whatever's in `path`. Camera
/// modules are re-resolved by matching `camera_name` against the cameras
/// actually available right now — a device that isn't connected on this
/// machine just loads unselected rather than failing the whole load.
pub fn load_composition(model: &mut Model, win: Rect, path: &std::path::Path) -> Result<(), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let file: SaveFile = serde_json::from_str(&json).map_err(|e| e.to_string())?;

    let mut modules = Vec::with_capacity(file.modules.len());
    for saved in file.modules {
        let mut chrome = ModuleChrome::new();
        chrome.minimized = saved.minimized;
        chrome.show_preview = saved.show_preview;

        let body = match saved.kind {
            ModuleKind::Camera => {
                let mut cam = CameraState::new();
                cam.chrome = chrome;
                if let Some(name) = &saved.camera_name {
                    // Kept even if the device isn't plugged in right now:
                    // it attaches when it shows up.
                    cam.wanted = Some(name.clone());
                    cam.selected = model
                        .available_cameras
                        .iter()
                        .position(|info| info.human_name() == *name);
                }
                ModuleBody::Camera(cam)
            }
            ModuleKind::Blend => {
                let mut blend = BlendState::new();
                blend.chrome = chrome;
                blend.mix = saved.mix.unwrap_or(0.5).clamp(0.0, 1.0);
                ModuleBody::Blend(blend)
            }
            ModuleKind::Output => {
                let mut output = OutputState::new();
                output.chrome = chrome;
                ModuleBody::Output(output)
            }
            ModuleKind::Number => {
                let mut number = NumberState::new();
                number.chrome = chrome;
                number.value = saved.value.unwrap_or(0.0);
                ModuleBody::Number(number)
            }
            ModuleKind::Mult => {
                let mut mult = MultState::new();
                mult.chrome = chrome;
                ModuleBody::Mult(mult)
            }
            ModuleKind::Lfo => {
                let mut lfo = LfoState::new();
                lfo.chrome = chrome;
                lfo.freq = saved.freq.unwrap_or(1.0).max(0.0);
                ModuleBody::Lfo(lfo)
            }
            ModuleKind::Shader => {
                // Ports and defaults come from the file, so they're built
                // on the next tick, when the shader gets compiled.
                let mut shader = ShaderState::new();
                shader.chrome = chrome;
                shader.select(saved.shader_file);
                ModuleBody::Shader(shader)
            }
        };

        modules.push(PlacedModule {
            id: saved.id,
            kind: saved.kind,
            pos: pt2(saved.pos.0, saved.pos.1),
            body,
        });
    }

    let mut next_id = modules.iter().map(|m| m.id).max().map(|id| id + 1).unwrap_or(0);
    let mut output_id = file.output_id;

    // Self-produced files always include Output, but guard against a
    // hand-edited file that doesn't, so the "exactly one pinned Output"
    // invariant always holds.
    if !modules.iter().any(|m| m.id == output_id && m.kind == ModuleKind::Output) {
        output_id = next_id;
        next_id += 1;
        modules.push(PlacedModule {
            id: output_id,
            kind: ModuleKind::Output,
            pos: canvas_rect(win).xy(),
            body: ModuleBody::Output(OutputState::new()),
        });
    }

    let connections = file
        .connections
        .into_iter()
        .map(|c| Connection {
            from: c.from,
            to: c.to,
            to_input: c.to_input,
        })
        .collect();

    model.modules = modules;
    model.connections = connections;
    model.next_id = next_id;
    model.output_id = output_id;
    model.dragging = None;

    Ok(())
}

