pub mod effects;
pub mod math;
pub mod mix;
pub mod multi_input;
pub mod output;
pub mod param;
pub mod sources;

use crate::ModuleChrome;
use effects::shader::ShaderState;
use effects::shader_file::ShaderRole;
use math::{
    lag::LagState,
    lfo::LfoState,
    map::MapState,
    mult::MultState,
    number::NumberState,
    osc::OscState,
};
use mix::blend::BlendState;
use nannou::prelude::*;
use output::OutputState;
use serde::{Deserialize, Serialize};
use sources::camera::CameraState;
use sources::video::VideoState;
use sources::wled::WledState;

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
    Video,
    Blend,
    Output,
    Number,
    Mult,
    Lfo,
    Lag,
    /// Old compositions saved Clamp as its own kind: it loads as a Map.
    #[serde(alias = "clamp")]
    Map,
    Wled,
    /// Legacy catch-all shader node (lists every shader); kept so old
    /// compositions load. New nodes use the role-specific kinds below.
    Shader,
    Generator,
    Effect,
    Feedback,
    ShaderMix,
    /// Blend with one multi-cable video input (any number of layers).
    Stack,
    Osc,
}

impl ModuleKind {
    /// Whether this kind is backed by a shader file.
    pub fn is_shader(self) -> bool {
        self.shader_role().is_some()
    }

    /// Which shaders this kind lists in its dropdown.
    pub fn shader_role(self) -> Option<ShaderRole> {
        match self {
            ModuleKind::Shader => Some(ShaderRole::Any),
            ModuleKind::Generator => Some(ShaderRole::Generator),
            ModuleKind::Effect => Some(ShaderRole::Effect),
            ModuleKind::Feedback => Some(ShaderRole::Feedback),
            ModuleKind::ShaderMix => Some(ShaderRole::Mix),
            _ => None,
        }
    }

    /// The node to create for a shader file of the given role.
    pub fn for_shader_role(role: ShaderRole) -> ModuleKind {
        match role {
            ShaderRole::Any => ModuleKind::Shader,
            ShaderRole::Generator => ModuleKind::Generator,
            ShaderRole::Effect => ModuleKind::Effect,
            ShaderRole::Feedback => ModuleKind::Feedback,
            ShaderRole::Mix => ModuleKind::ShaderMix,
        }
    }
}

pub const CATEGORIES: &[(&str, &[(&str, ModuleKind)])] = &[
    (
        "Sources",
        &[
            ("Camera", ModuleKind::Camera),
            ("Video", ModuleKind::Video),
            ("WLED", ModuleKind::Wled),
        ],
    ),
    ("Mix", &[("Blend", ModuleKind::Blend), ("Stack", ModuleKind::Stack)]),
    (
        "Math",
        &[
            ("Number", ModuleKind::Number),
            ("Mult", ModuleKind::Mult),
            ("LFO", ModuleKind::Lfo),
            ("Lag", ModuleKind::Lag),
            ("Map", ModuleKind::Map),
        ],
    ),
    // Filled from the shaders folder, by category: see `sidebar_rows`.
    ("Effects", &[]),
];

/// The two kinds of signal a port can carry. Each palette gives them their
/// own color (see `ui::Theme::port`), independent of module body colors.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Video,
    Number,
}

pub fn kind_label(kind: ModuleKind) -> &'static str {
    match kind {
        ModuleKind::Camera => "Camera",
        ModuleKind::Video => "Video",
        ModuleKind::Blend => "Blend",
        ModuleKind::Output => "Output",
        ModuleKind::Number => "Number",
        ModuleKind::Mult => "Mult",
        ModuleKind::Lfo => "LFO",
        ModuleKind::Lag => "Lag",
        ModuleKind::Map => "Map",
        ModuleKind::Wled => "WLED",
        ModuleKind::Shader => "Shader",
        ModuleKind::Generator => "Generator",
        ModuleKind::Effect => "Effect",
        ModuleKind::Feedback => "Feedback",
        ModuleKind::ShaderMix => "Shader Mix",
        ModuleKind::Stack => "Stack",
        ModuleKind::Osc => "OSC",
    }
}

/// The type of each of a module's input ports, in order.
pub fn input_types(kind: ModuleKind) -> &'static [PortType] {
    match kind {
        // Shader's ports depend on the file it loaded: see `PlacedModule::input_types`.
        ModuleKind::Camera
        | ModuleKind::Number
        | ModuleKind::Lfo
        | ModuleKind::Shader
        | ModuleKind::Generator
        | ModuleKind::Effect
        | ModuleKind::Feedback
        | ModuleKind::ShaderMix
        | ModuleKind::Osc => &[],
        // The third input drives the mix; it overrides the slider while plugged.
        ModuleKind::Blend => &[PortType::Video, PortType::Video, PortType::Number],
        // Port 0 takes any number of cables (see `multi_input`); port 1 drives the mix.
        ModuleKind::Stack => &[PortType::Video, PortType::Number],
        ModuleKind::Output => &[PortType::Video],
        ModuleKind::Mult => &[PortType::Number, PortType::Number],
        ModuleKind::Lag | ModuleKind::Map => &[PortType::Number],
        ModuleKind::Wled => &[],
        // Speed, then scrub (a wired scrub drives the playhead directly).
        ModuleKind::Video => &[PortType::Number, PortType::Number],
    }
}

/// The type of a module's single output port, or `None` if it has no output.
pub fn output_type(kind: ModuleKind) -> Option<PortType> {
    match kind {
        ModuleKind::Camera
        | ModuleKind::Video
        | ModuleKind::Blend
        | ModuleKind::Stack
        | ModuleKind::Shader
        | ModuleKind::Generator
        | ModuleKind::Effect
        | ModuleKind::Feedback
        | ModuleKind::ShaderMix => Some(PortType::Video),
        ModuleKind::Output => None,
        ModuleKind::Number
        | ModuleKind::Mult
        | ModuleKind::Lfo
        | ModuleKind::Lag
        | ModuleKind::Map
        | ModuleKind::Osc
        | ModuleKind::Wled => Some(PortType::Number),
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
        | ModuleKind::Video
        | ModuleKind::Blend
        | ModuleKind::Stack
        | ModuleKind::Osc
        | ModuleKind::Number
        | ModuleKind::Lfo
        | ModuleKind::Lag
        | ModuleKind::Map
        | ModuleKind::Wled
        | ModuleKind::Mult
        | ModuleKind::Shader
        | ModuleKind::Generator
        | ModuleKind::Effect
        | ModuleKind::Feedback
        | ModuleKind::ShaderMix => true,
        ModuleKind::Output => false,
    }
}

pub fn is_deletable(kind: ModuleKind) -> bool {
    kind != ModuleKind::Output
}

pub fn base_height(kind: ModuleKind) -> f32 {
    match kind {
        // A second row with a mode selector (LFO wave, blend mode, lag curve).
        ModuleKind::Lfo | ModuleKind::Blend | ModuleKind::Stack | ModuleKind::Lag => 126.0,
        // Port and scale boxes, then the last-message readout.
        ModuleKind::Osc => 126.0,
        // File, speed, transport and the scrub bar.
        ModuleKind::Video => 180.0,
        // Two rows of range fields, bias and calibrate, the toggles, the readout.
        ModuleKind::Map => 198.0,
        // Grows with its inputs: see `PlacedModule::base_height`.
        ModuleKind::Wled => WLED_ROWS_TOP + 40.0,
        _ if has_config_row(kind) => 90.0,
        _ => 64.0,
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
    Video(VideoState),
    Blend(BlendState),
    Output(OutputState),
    Number(NumberState),
    Mult(MultState),
    Lfo(LfoState),
    Lag(LagState),
    Map(MapState),
    Wled(WledState),
    Shader(ShaderState),
    Osc(OscState),
}

impl ModuleBody {
    pub fn chrome(&self) -> &ModuleChrome {
        match self {
            ModuleBody::Camera(s) => &s.chrome,
            ModuleBody::Video(s) => &s.chrome,
            ModuleBody::Blend(s) => &s.chrome,
            ModuleBody::Output(s) => &s.chrome,
            ModuleBody::Number(s) => &s.chrome,
            ModuleBody::Mult(s) => &s.chrome,
            ModuleBody::Lfo(s) => &s.chrome,
            ModuleBody::Lag(s) => &s.chrome,
            ModuleBody::Map(s) => &s.chrome,
            ModuleBody::Wled(s) => &s.chrome,
            ModuleBody::Shader(s) => &s.chrome,
            ModuleBody::Osc(s) => &s.chrome,
        }
    }

    pub fn chrome_mut(&mut self) -> &mut ModuleChrome {
        match self {
            ModuleBody::Camera(s) => &mut s.chrome,
            ModuleBody::Video(s) => &mut s.chrome,
            ModuleBody::Blend(s) => &mut s.chrome,
            ModuleBody::Output(s) => &mut s.chrome,
            ModuleBody::Number(s) => &mut s.chrome,
            ModuleBody::Mult(s) => &mut s.chrome,
            ModuleBody::Lfo(s) => &mut s.chrome,
            ModuleBody::Lag(s) => &mut s.chrome,
            ModuleBody::Map(s) => &mut s.chrome,
            ModuleBody::Wled(s) => &mut s.chrome,
            ModuleBody::Shader(s) => &mut s.chrome,
            ModuleBody::Osc(s) => &mut s.chrome,
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
/// How far a Shader's ports move up when it has no selector row.
pub const SHADER_PICKER_H: f32 = 36.0;
/// Where a WLED module's input rows start, and how tall each is.
pub const WLED_ROWS_TOP: f32 = 96.0;
pub const WLED_ROW_H: f32 = 26.0;

impl PlacedModule {
    /// The type of each input port, in order. Fixed per kind, except Shader,
    /// whose ports come from the file it loaded.
    pub fn input_types(&self) -> Vec<PortType> {
        match &self.body {
            ModuleBody::Shader(s) => s.input_types(),
            _ => input_types(self.kind).to_vec(),
        }
    }

    /// How much higher than usual this module's input ports sit (a shader
    /// with a single choice has no selector row).
    pub fn port_shift(&self) -> f32 {
        match &self.body {
            ModuleBody::Shader(s) if s.single_choice && s.file.is_some() => SHADER_PICKER_H,
            _ => 0.0,
        }
    }

    pub fn base_height(&self) -> f32 {
        match &self.body {
            ModuleBody::Shader(s) => {
                let ports = s.input_types().len() as f32;
                let shift = self.port_shift();
                (SHADER_PORT_TOP - shift + ports * SHADER_PORT_STEP + 4.0).max(base_height(self.kind) - shift)
            }
            ModuleBody::Wled(w) => WLED_ROWS_TOP + w.inputs.len().max(1) as f32 * WLED_ROW_H + 40.0,
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
        ModuleKind::Video => ModuleBody::Video(VideoState::new()),
        ModuleKind::Blend | ModuleKind::Stack => ModuleBody::Blend(BlendState::new()),
        ModuleKind::Osc => ModuleBody::Osc(OscState::new()),
        ModuleKind::Output => ModuleBody::Output(OutputState::new()),
        ModuleKind::Number => ModuleBody::Number(NumberState::new()),
        ModuleKind::Mult => ModuleBody::Mult(MultState::new()),
        ModuleKind::Lfo => ModuleBody::Lfo(LfoState::new()),
        ModuleKind::Lag => ModuleBody::Lag(LagState::new()),
        ModuleKind::Map => ModuleBody::Map(MapState::new()),
        ModuleKind::Wled => ModuleBody::Wled(WledState::new()),
        ModuleKind::Shader | ModuleKind::Generator | ModuleKind::Effect | ModuleKind::Feedback | ModuleKind::ShaderMix => {
            ModuleBody::Shader(ShaderState::new())
        }
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

/// Computes the current number a module's output carries (normalized,
/// 0..1 by convention): Number where its value sits in its range, LFO its
/// current oscillation, Mult the product of its two inputs (or whichever
/// one is connected, if only one is). Math is
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
        ModuleBody::Number(number) => Some(number.value.output_norm()),
        ModuleBody::Lfo(lfo) => Some(lfo.sample(now, id)),
        // Advanced once per tick in `update` (it has state).
        ModuleBody::Lag(lag) => lag.output(),
        ModuleBody::Map(map) => input_source(connections, id, 0)
            .and_then(|src| module_number(modules, connections, src, now, visiting))
            .map(|x| map.apply(x)),
        // Refreshed from the device every tick in `update`.
        ModuleBody::Wled(wled) => wled.output(),
        // Updated from the socket every tick in `update`.
        ModuleBody::Osc(osc) => osc.output(),
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
        ModuleBody::Camera(_) | ModuleBody::Video(_) | ModuleBody::Blend(_) | ModuleBody::Output(_) | ModuleBody::Shader(_) => {
            None
        }
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
