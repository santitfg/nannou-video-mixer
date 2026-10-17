pub mod effects;
pub mod math;
pub mod mix;
pub mod output;
pub mod sources;

use crate::ModuleChrome;
use effects::shader::ShaderState;
use math::{lfo::LfoState, mult::MultState, number::NumberState};
use mix::blend::BlendState;
use nannou::prelude::*;
use output::OutputState;
use serde::{Deserialize, Serialize};
use sources::camera::CameraState;

// --- Module kinds ------------------------------------------------------
//
// Every module shares the same "chrome" (minimize/preview/position/ports);
// only the per-kind config row and output computation differ. Adding a new
// module type means: one `ModuleBody` variant, one entry in the metadata
// functions below, and one match arm each in `module_frame`/`module_number`
// and the config row draw/click handling — everything else (dragging,
// ports, cabling, preview thumbnails) comes for free.

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModuleKind {
    Camera,
    Blend,
    Output,
    Number,
    Mult,
    Lfo,
    Shader,
}

pub const CATEGORIES: &[(&str, &[(&str, ModuleKind)])] = &[
    ("Sources", &[("Camera", ModuleKind::Camera)]),
    ("Mix", &[("Blend", ModuleKind::Blend)]),
    (
        "Math",
        &[
            ("Number", ModuleKind::Number),
            ("Mult", ModuleKind::Mult),
            ("LFO", ModuleKind::Lfo),
        ],
    ),
    ("Effects", &[("Shader", ModuleKind::Shader)]),
];

/// The two kinds of signal a port can carry. Colors are how a port/cable's
/// kind is shown — video ports/cables are yellow, number ones black —
/// independent of whatever color a module's body happens to be.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Video,
    Number,
}

pub fn port_color(port_type: PortType) -> Srgb<u8> {
    match port_type {
        PortType::Video => YELLOW,
        PortType::Number => BLACK,
    }
}

pub fn kind_label(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Camera => "Camera",
        ModuleKind::Blend => "Blend",
        ModuleKind::Output => "Output",
        ModuleKind::Number => "Number",
        ModuleKind::Mult => "Mult",
        ModuleKind::Lfo => "LFO",
        ModuleKind::Shader => "Shader",
    }
}

/// The type of each of a module's input ports, in order.
pub fn input_types(kind: ModuleKind) -> &'static [PortType] {
    match kind {
        // Shader's ports depend on the file it loaded: see `PlacedModule::input_types`.
        ModuleKind::Camera | ModuleKind::Number | ModuleKind::Lfo | ModuleKind::Shader => &[],
        // The third input drives the mix; it overrides the slider while plugged.
        ModuleKind::Blend => &[PortType::Video, PortType::Video, PortType::Number],
        ModuleKind::Output => &[PortType::Video],
        ModuleKind::Mult => &[PortType::Number, PortType::Number],
    }
}

/// The type of a module's single output port, or `None` if it has no output.
pub fn output_type(kind: ModuleKind) -> Option<PortType> {
    match kind {
        ModuleKind::Camera | ModuleKind::Blend | ModuleKind::Shader => Some(PortType::Video),
        ModuleKind::Output => None,
        ModuleKind::Number | ModuleKind::Mult | ModuleKind::Lfo => Some(PortType::Number),
    }
}

pub fn has_output(kind: ModuleKind) -> bool {
    output_type(kind).is_some()
}

/// Whether video is involved at all (an input or the output) — only those
/// modules get a Preview button, since there's nothing to show a thumbnail
/// of for a pure number module.
pub fn has_video_preview(kind: ModuleKind) -> bool {
    output_type(kind) == Some(PortType::Video) || input_types(kind).contains(&PortType::Video)
}

pub fn has_config_row(kind: ModuleKind) -> bool {
    match kind {
        ModuleKind::Camera
        | ModuleKind::Blend
        | ModuleKind::Number
        | ModuleKind::Lfo
        | ModuleKind::Mult
        | ModuleKind::Shader => true,
        ModuleKind::Output => false,
    }
}

pub fn is_deletable(kind: ModuleKind) -> bool {
    kind != ModuleKind::Output
}

pub fn base_height(kind: ModuleKind) -> f32 {
    if has_config_row(kind) {
        90.0
    } else {
        64.0
    }
}

pub fn body_color(kind: ModuleKind) -> Srgb<u8> {
    match kind {
        ModuleKind::Camera => STEELBLUE,
        ModuleKind::Blend => rgb8(130, 90, 160),
        ModuleKind::Shader => rgb8(70, 130, 100),
        ModuleKind::Output => DARKSLATEGRAY,
        ModuleKind::Number | ModuleKind::Mult | ModuleKind::Lfo => rgb8(180, 130, 60),
    }
}

#[derive(Clone, Copy)]
pub enum TitleButton {
    Delete,
    Minimize,
    Preview,
}

pub fn title_buttons_for(kind: ModuleKind) -> &'static [TitleButton] {
    match (is_deletable(kind), has_video_preview(kind)) {
        (true, true) => &[TitleButton::Delete, TitleButton::Minimize, TitleButton::Preview],
        (true, false) => &[TitleButton::Delete, TitleButton::Minimize],
        (false, true) => &[TitleButton::Minimize, TitleButton::Preview],
        (false, false) => &[TitleButton::Minimize],
    }
}

// --- Composition graph -------------------------------------------------------

pub enum ModuleBody {
    Camera(CameraState),
    Blend(BlendState),
    Output(OutputState),
    Number(NumberState),
    Mult(MultState),
    Lfo(LfoState),
    Shader(ShaderState),
}

impl ModuleBody {
    pub fn chrome(&self) -> &ModuleChrome {
        match self {
            ModuleBody::Camera(s) => &s.chrome,
            ModuleBody::Blend(s) => &s.chrome,
            ModuleBody::Output(s) => &s.chrome,
            ModuleBody::Number(s) => &s.chrome,
            ModuleBody::Mult(s) => &s.chrome,
            ModuleBody::Lfo(s) => &s.chrome,
            ModuleBody::Shader(s) => &s.chrome,
        }
    }

    pub fn chrome_mut(&mut self) -> &mut ModuleChrome {
        match self {
            ModuleBody::Camera(s) => &mut s.chrome,
            ModuleBody::Blend(s) => &mut s.chrome,
            ModuleBody::Output(s) => &mut s.chrome,
            ModuleBody::Number(s) => &mut s.chrome,
            ModuleBody::Mult(s) => &mut s.chrome,
            ModuleBody::Lfo(s) => &mut s.chrome,
            ModuleBody::Shader(s) => &mut s.chrome,
        }
    }
}

pub struct PlacedModule {
    pub id: u64,
    pub kind: ModuleKind,
    pub pos: Point2,
    pub body: ModuleBody,
}

/// Vertical room per input port on a Shader module (its ports carry labels).
pub const SHADER_PORT_STEP: f32 = 24.0;
/// Where a Shader's first port sits, measured down from the top of the box.
pub const SHADER_PORT_TOP: f32 = 84.0;

impl PlacedModule {
    /// The type of each input port, in order. Fixed per kind, except Shader,
    /// whose ports come from the file it loaded.
    pub fn input_types(&self) -> Vec<PortType> {
        match &self.body {
            ModuleBody::Shader(s) => s.input_types(),
            _ => input_types(self.kind).to_vec(),
        }
    }

    pub fn base_height(&self) -> f32 {
        match &self.body {
            ModuleBody::Shader(s) => {
                let ports = s.input_types().len() as f32;
                (SHADER_PORT_TOP + ports * SHADER_PORT_STEP + 4.0).max(base_height(self.kind))
            }
            _ => base_height(self.kind),
        }
    }
}

/// One cable: `from`'s output feeds `to`'s input number `to_input`. An
/// input can carry at most one connection; an output can fan out to many.
pub struct Connection {
    pub from: u64,
    pub to: u64,
    pub to_input: usize,
}

pub fn push_module(modules: &mut Vec<PlacedModule>, next_id: &mut u64, kind: ModuleKind, pos: Point2) -> u64 {
    let body = match kind {
        ModuleKind::Camera => ModuleBody::Camera(CameraState::new()),
        ModuleKind::Blend => ModuleBody::Blend(BlendState::new()),
        ModuleKind::Output => ModuleBody::Output(OutputState::new()),
        ModuleKind::Number => ModuleBody::Number(NumberState::new()),
        ModuleKind::Mult => ModuleBody::Mult(MultState::new()),
        ModuleKind::Lfo => ModuleBody::Lfo(LfoState::new()),
        ModuleKind::Shader => ModuleBody::Shader(ShaderState::new()),
    };
    let id = *next_id;
    modules.push(PlacedModule { id, kind, pos, body });
    *next_id += 1;
    id
}

pub(crate) fn input_source(connections: &[Connection], to: u64, to_input: usize) -> Option<u64> {
    connections
        .iter()
        .find(|c| c.to == to && c.to_input == to_input)
        .map(|c| c.from)
}

/// Computes the current number a module's output carries: Number returns
/// its own value, LFO its current oscillation, Mult the product of its
/// two inputs (or whichever one is connected, if only one is). Math is
/// cheap enough that, unlike video frames, this is just recomputed fresh
/// every time it's needed rather than cached.
pub fn module_number(
    modules: &[PlacedModule],
    connections: &[Connection],
    id: u64,
    now: f32,
    visiting: &mut Vec<u64>,
) -> Option<f32> {
    if visiting.contains(&id) {
        return None;
    }
    visiting.push(id);

    let result = modules.iter().find(|m| m.id == id).and_then(|m| match &m.body {
        ModuleBody::Number(number) => Some(number.value),
        ModuleBody::Lfo(lfo) => {
            let phase = 2.0 * std::f32::consts::PI * lfo.freq * now;
            Some((phase.sin() + 1.0) / 2.0)
        }
        ModuleBody::Mult(_) => {
            let a = input_source(connections, id, 0)
                .and_then(|src| module_number(modules, connections, src, now, visiting));
            let b = input_source(connections, id, 1)
                .and_then(|src| module_number(modules, connections, src, now, visiting));
            match (a, b) {
                (Some(a), Some(b)) => Some(a * b),
                (Some(a), None) | (None, Some(a)) => Some(a),
                (None, None) => None,
            }
        }
        ModuleBody::Camera(_) | ModuleBody::Blend(_) | ModuleBody::Output(_) | ModuleBody::Shader(_) => None,
    });

    visiting.pop();
    result
}

/// True if `id`'s output is wired, through any chain of modules, into
/// `target`'s input.
pub fn reaches(connections: &[Connection], from: u64, target: u64, visiting: &mut Vec<u64>) -> bool {
    if from == target {
        return true;
    }
    if visiting.contains(&from) {
        return false;
    }
    visiting.push(from);
    connections.iter().any(|c| c.from == from && reaches(connections, c.to, target, visiting))
}
