pub mod effects;
pub mod math;
pub mod mix;
pub mod multi_input;
pub mod output;
pub mod param;
pub mod signal;
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
    select::SelectState,
};
use mix::blend::BlendState;
use nannou::prelude::*;
use output::OutputState;
pub use signal::Signal;
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
    Select,
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
            ("Select", ModuleKind::Select),
        ],
    ),
    // Filled from the shaders folder, by category: see `sidebar_rows`.
    ("Effects", &[]),
];

/// The kinds of signal a port can carry. Each palette gives video and
/// numbers their own color (see `ui::Theme::port`), independent of module
/// body colors; an array of numbers shares the number color and is told
/// apart by its shape (a ring around the port, a doubled cable).
///
/// As an input, `Numbers` means "a number or an array": the math modules
/// work on either. An array never lands on a plain `Number` input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PortType {
    Video,
    Number,
    Numbers,
}

impl PortType {
    /// Whether a cable carrying `signal` can plug into an input of this type.
    pub fn accepts(self, signal: PortType) -> bool {
        self == signal || (self == PortType::Numbers && signal == PortType::Number)
    }
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
        ModuleKind::Select => "Select",
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
        // Each takes a number or an array, and sends the same kind on.
        ModuleKind::Mult => &[PortType::Numbers, PortType::Numbers],
        ModuleKind::Lag | ModuleKind::Map => &[PortType::Numbers],
        // A multi-input pill: every cable's values, end to end.
        ModuleKind::Select => &[PortType::Numbers],
        ModuleKind::Wled => &[],
        // Speed, then scrub (a wired scrub drives the playhead directly).
        ModuleKind::Video => &[PortType::Number, PortType::Number],
    }
}

/// The type of a module's output ports, or `None` if it has no output. A
/// math module fed an array sends an array: see `signal_type` for what a
/// given port actually carries.
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
        | ModuleKind::Select => Some(PortType::Number),
        ModuleKind::Wled => Some(PortType::Numbers),
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
        | ModuleKind::Select
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
        ModuleKind::Select => SELECT_ROWS_TOP + SELECT_ROW_H + 12.0,
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
    Select(SelectState),
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
            ModuleBody::Select(s) => &s.chrome,
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
            ModuleBody::Select(s) => &mut s.chrome,
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
/// Each extra line of a wrapped WLED status pushes the rows down this much.
pub const WLED_STATUS_LINE_H: f32 = 14.0;

/// Where a WLED module's input rows start: below its status, however many
/// lines that wrapped to.
pub fn wled_rows_top(w: &WledState) -> f32 {
    WLED_ROWS_TOP + (w.status.len().max(1) - 1) as f32 * WLED_STATUS_LINE_H
}
/// Where a Select's element rows start (under its mode row), and how tall each is.
pub const SELECT_ROWS_TOP: f32 = 72.0;
pub const SELECT_ROW_H: f32 = 28.0;

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
            ModuleBody::Wled(w) => wled_rows_top(w) + w.inputs.len().max(1) as f32 * WLED_ROW_H + 40.0,
            ModuleBody::Select(s) => SELECT_ROWS_TOP + s.values.len().max(1) as f32 * SELECT_ROW_H + 12.0,
            _ => base_height(self.kind),
        }
    }
}

/// One cable: `from`'s output number `from_output` feeds `to`'s input
/// number `to_input`. An input can carry at most one connection (unless
/// it's a multi port); an output can fan out to many. Every module has one
/// output except Select, which has one per element it splits out.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Connection {
    pub from: u64,
    pub from_output: usize,
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
        ModuleKind::Select => ModuleBody::Select(SelectState::new()),
        ModuleKind::Shader | ModuleKind::Generator | ModuleKind::Effect | ModuleKind::Feedback | ModuleKind::ShaderMix => {
            ModuleBody::Shader(ShaderState::new())
        }
    };
    let id = *next_id;
    modules.push(PlacedModule { id, kind, pos, body });
    *next_id += 1;
    id
}

/// The module wired into `to`'s input `to_input`, if any.
pub(crate) fn input_source(connections: &[Connection], to: u64, to_input: usize) -> Option<u64> {
    input_port(connections, to, to_input).map(|(from, _)| from)
}

/// The module and output port wired into `to`'s input `to_input`, if any.
pub(crate) fn input_port(connections: &[Connection], to: u64, to_input: usize) -> Option<(u64, usize)> {
    connections
        .iter()
        .find(|c| c.to == to && c.to_input == to_input)
        .map(|c| (c.from, c.from_output))
}

/// What's arriving at `to`'s input `to_input`: a number or an array.
pub fn input_signal(
    modules: &[PlacedModule],
    connections: &[Connection],
    to: u64,
    to_input: usize,
    now: f32,
    visiting: &mut Vec<u64>,
) -> Option<Signal> {
    let (from, output) = input_port(connections, to, to_input)?;
    module_signal(modules, connections, from, output, now, visiting)
}

/// The single number arriving at `to`'s input `to_input`. An array (which
/// never plugs into a plain number input) reads as nothing.
pub fn input_number(
    modules: &[PlacedModule],
    connections: &[Connection],
    to: u64,
    to_input: usize,
    now: f32,
    visiting: &mut Vec<u64>,
) -> Option<f32> {
    input_signal(modules, connections, to, to_input, now, visiting)?.as_one()
}

/// A Select's input: every cable on its pill, in plugging order, laid end
/// to end.
pub fn select_input(modules: &[PlacedModule], connections: &[Connection], id: u64, now: f32, visiting: &mut Vec<u64>) -> Vec<f32> {
    multi_input::source_ports(connections, id, 0)
        .into_iter()
        .filter_map(|(from, output)| module_signal(modules, connections, from, output, now, visiting))
        .flat_map(|s| s.values().to_vec())
        .collect()
}

/// The current number (or array) on output `output` of module `id`
/// (normalized, 0..1 by convention): Number where its value sits in its
/// range, LFO its current oscillation, Mult the product of its two inputs
/// (or whichever one is connected, if only one is), WLED all its inputs.
/// Math is cheap enough that, unlike video frames, this is just recomputed
/// fresh every time it's needed rather than cached.
pub fn module_signal(
    modules: &[PlacedModule],
    connections: &[Connection],
    id: u64,
    output: usize,
    now: f32,
    visiting: &mut Vec<u64>,
) -> Option<Signal> {
    if visiting.contains(&id) {
        return None;
    }
    visiting.push(id);

    let one = |x: Option<f32>| if output == 0 { x.map(Signal::One) } else { None };
    let result = modules.iter().find(|m| m.id == id).and_then(|m| match &m.body {
        ModuleBody::Number(number) => one(Some(number.value.output_norm())),
        ModuleBody::Lfo(lfo) => one(Some(lfo.sample(now, id))),
        // Advanced once per tick in `update` (it has state).
        ModuleBody::Lag(lag) => lag.output().filter(|_| output == 0),
        ModuleBody::Map(map) => input_signal(modules, connections, id, 0, now, visiting).map(|x| x.map(|x| map.apply(x))),
        // Refreshed from the device every tick in `update`.
        ModuleBody::Wled(wled) => (output == 0).then(|| Signal::Many(wled.output())),
        // Updated from the socket every tick in `update`.
        ModuleBody::Osc(osc) => one(osc.output()),
        ModuleBody::Select(select) => select.apply(&select_input(modules, connections, id, now, visiting), output),
        ModuleBody::Mult(_) => {
            let a = input_signal(modules, connections, id, 0, now, visiting);
            let b = input_signal(modules, connections, id, 1, now, visiting);
            match (a, b) {
                (Some(a), Some(b)) => Some(a.zip_with(&b, |a, b| a * b)),
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

/// The single number on module `id`'s first output (nothing if it's an array).
pub fn module_number(
    modules: &[PlacedModule],
    connections: &[Connection],
    id: u64,
    now: f32,
    visiting: &mut Vec<u64>,
) -> Option<f32> {
    module_signal(modules, connections, id, 0, now, visiting)?.as_one()
}

/// What output `output` of module `id` carries: an array if it's a WLED,
/// a Select in Combine mode, or a math module with an array wired into it.
/// Follows the cables upstream (computed from the wiring, not the values,
/// so it holds even before a device reports anything).
pub fn signal_type(modules: &[PlacedModule], connections: &[Connection], id: u64, output: usize, visiting: &mut Vec<u64>) -> Option<PortType> {
    let module = modules.iter().find(|m| m.id == id)?;
    let base = output_type(module.kind)?;
    if visiting.contains(&id) {
        return Some(base);
    }
    visiting.push(id);
    let result = match &module.body {
        ModuleBody::Select(s) => Some(if s.combine && output == 0 { PortType::Numbers } else { PortType::Number }),
        ModuleBody::Map(_) | ModuleBody::Lag(_) | ModuleBody::Mult(_) => {
            let any_array = (0..input_types(module.kind).len()).any(|n| {
                input_port(connections, id, n)
                    .and_then(|(from, out)| signal_type(modules, connections, from, out, visiting))
                    == Some(PortType::Numbers)
            });
            Some(if any_array { PortType::Numbers } else { PortType::Number })
        }
        _ => Some(base),
    };
    visiting.pop();
    result
}

/// Cables whose source sends something their input can't take (a math
/// module turned into an array by a new cable upstream, still wired into a
/// plain number input), as indexes into `connections`.
pub fn mismatched(modules: &[PlacedModule], connections: &[Connection]) -> Vec<usize> {
    connections
        .iter()
        .enumerate()
        .filter(|(_, c)| {
            let Some(to) = modules.iter().find(|m| m.id == c.to) else { return false };
            let Some(&input) = to.input_types().get(c.to_input) else { return false };
            signal_type(modules, connections, c.from, c.from_output, &mut Vec::new()).is_some_and(|s| !input.accepts(s))
        })
        .map(|(i, _)| i)
        .collect()
}

/// Unplugs every cable `mismatched` finds (repeatedly: unplugging one can
/// change what's downstream of it).
pub fn drop_mismatched(modules: &[PlacedModule], connections: &mut Vec<Connection>) {
    loop {
        let bad = mismatched(modules, connections);
        if bad.is_empty() {
            return;
        }
        for i in bad.into_iter().rev() {
            connections.remove(i);
        }
    }
}

/// Labels for the elements of what output `output` of `id` carries: WLED
/// names its inputs by GPIO, a Select passes its own on, and math modules
/// keep their input's. Anything else is numbered from 1.
pub fn signal_labels(modules: &[PlacedModule], connections: &[Connection], id: u64, output: usize, len: usize, visiting: &mut Vec<u64>) -> Vec<String> {
    let numbered = |from: usize, n: usize| (from..from + n).map(|i| (i + 1).to_string()).collect::<Vec<_>>();
    let Some(module) = modules.iter().find(|m| m.id == id) else { return numbered(0, len) };
    if visiting.contains(&id) {
        return numbered(0, len);
    }
    visiting.push(id);
    let mut labels = match &module.body {
        ModuleBody::Wled(w) => w.labels(),
        ModuleBody::Select(s) if s.combine => s
            .labels
            .iter()
            .enumerate()
            .filter(|(i, _)| s.selected.iter().all(|x| !x) || s.is_selected(*i))
            .map(|(_, l)| l.clone())
            .collect(),
        ModuleBody::Select(s) => s.labels.get(output).cloned().into_iter().collect(),
        ModuleBody::Map(_) | ModuleBody::Lag(_) | ModuleBody::Mult(_) => {
            // The first input carrying an array names the elements.
            (0..input_types(module.kind).len())
                .filter_map(|n| input_port(connections, id, n))
                .find(|&(from, out)| signal_type(modules, connections, from, out, &mut Vec::new()) == Some(PortType::Numbers))
                .map(|(from, out)| signal_labels(modules, connections, from, out, len, visiting))
                .unwrap_or_default()
        }
        _ => Vec::new(),
    };
    visiting.pop();
    let have = labels.len();
    if have < len {
        labels.extend(numbered(have, len - have));
    }
    labels.truncate(len);
    labels
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

#[cfg(test)]
mod tests {
    use super::*;
    use sources::wled::InputView;

    fn wled_with(values: &[f32]) -> WledState {
        let mut w = WledState::new();
        w.inputs = values
            .iter()
            .enumerate()
            .map(|(i, &value)| InputView { gpio: 30 + i as u8, kind: "Analog", value, raw: 0, active: false })
            .collect();
        w
    }

    fn placed(id: u64, kind: ModuleKind, body: ModuleBody) -> PlacedModule {
        PlacedModule { id, kind, pos: pt2(0.0, 0.0), body }
    }

    fn cable(from: u64, from_output: usize, to: u64, to_input: usize) -> Connection {
        Connection { from, from_output, to, to_input }
    }

    #[test]
    fn map_scales_a_whole_array_and_select_splits_it() {
        let mut map = MapState::new();
        map.fields[3].set(2.0); // 0..1 -> 0..2
        let mut select = SelectState::new();
        select.toggle(2);
        let modules = vec![
            placed(1, ModuleKind::Wled, ModuleBody::Wled(wled_with(&[0.1, 0.2, 0.4]))),
            placed(2, ModuleKind::Map, ModuleBody::Map(map)),
            placed(3, ModuleKind::Select, ModuleBody::Select(select)),
        ];
        let connections = vec![cable(1, 0, 2, 0), cable(2, 0, 3, 0)];
        assert_eq!(module_signal(&modules, &connections, 2, 0, 0.0, &mut Vec::new()), Some(Signal::Many(vec![0.2, 0.4, 0.8])));
        assert_eq!(signal_type(&modules, &connections, 2, 0, &mut Vec::new()), Some(PortType::Numbers));
        assert_eq!(module_signal(&modules, &connections, 3, 2, 0.0, &mut Vec::new()), Some(Signal::One(0.8)));
        assert_eq!(module_signal(&modules, &connections, 3, 0, 0.0, &mut Vec::new()), None, "element 0 isn't picked");
        assert_eq!(signal_labels(&modules, &connections, 2, 0, 3, &mut Vec::new())[2], "32 \u{00B7} Analog");
    }

    #[test]
    fn mult_broadcasts_a_number_over_an_array() {
        let mut number = NumberState::new();
        number.value.set(0.5);
        let modules = vec![
            placed(1, ModuleKind::Wled, ModuleBody::Wled(wled_with(&[0.2, 1.0]))),
            placed(2, ModuleKind::Number, ModuleBody::Number(number)),
            placed(3, ModuleKind::Mult, ModuleBody::Mult(MultState::new())),
        ];
        let connections = vec![cable(1, 0, 3, 0), cable(2, 0, 3, 1)];
        assert_eq!(module_signal(&modules, &connections, 3, 0, 0.0, &mut Vec::new()), Some(Signal::Many(vec![0.1, 0.5])));
        assert_eq!(module_number(&modules, &connections, 3, 0.0, &mut Vec::new()), None, "an array isn't a single number");
    }

    #[test]
    fn select_packs_single_numbers_into_an_array() {
        let (mut a, mut b) = (NumberState::new(), NumberState::new());
        a.value.set(0.25);
        b.value.set(0.75);
        let mut select = SelectState::new();
        select.combine = true;
        let modules = vec![
            placed(1, ModuleKind::Number, ModuleBody::Number(a)),
            placed(2, ModuleKind::Number, ModuleBody::Number(b)),
            placed(3, ModuleKind::Select, ModuleBody::Select(select)),
        ];
        let connections = vec![cable(2, 0, 3, 0), cable(1, 0, 3, 0)];
        assert_eq!(module_signal(&modules, &connections, 3, 0, 0.0, &mut Vec::new()), Some(Signal::Many(vec![0.75, 0.25])));
        assert_eq!(signal_type(&modules, &connections, 3, 0, &mut Vec::new()), Some(PortType::Numbers));
    }

    #[test]
    fn an_array_reaching_a_plain_number_input_is_unplugged() {
        let modules = vec![
            placed(1, ModuleKind::Wled, ModuleBody::Wled(wled_with(&[0.2]))),
            placed(2, ModuleKind::Map, ModuleBody::Map(MapState::new())),
            placed(3, ModuleKind::Blend, ModuleBody::Blend(BlendState::new())),
        ];
        // Map -> Blend's mix was fine while Map got a single number; now it gets an array.
        let mut connections = vec![cable(2, 0, 3, 2), cable(1, 0, 2, 0)];
        drop_mismatched(&modules, &mut connections);
        assert_eq!(connections, [cable(1, 0, 2, 0)]);
        assert!(PortType::Numbers.accepts(PortType::Number) && !PortType::Number.accepts(PortType::Numbers));
    }
}
