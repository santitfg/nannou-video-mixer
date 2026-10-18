use crate::modules::effects::shader::ShaderState;
use crate::modules::math::{
    lag::{LagMode, LagState},
    map::MapState,
    lfo::{LfoState, Wave},
    mult::MultState,
    number::NumberState,
};
use crate::modules::mix::blend::{BlendMode, BlendState};
use crate::modules::param::Param;
use crate::modules::output::OutputState;
use crate::modules::sources::camera::CameraState;
use crate::modules::sources::video::{LoopMode, VideoState};
use crate::modules::sources::wled::WledState;
use crate::modules::math::osc::OscState;
use crate::modules::{Connection, ModuleBody, ModuleKind, PlacedModule};
use crate::group::Group;
use crate::{canvas_rect, Model, ModuleChrome, SAVE_PATH};
use nokhwa::utils::CameraInfo;
use nannou::prelude::*;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

// Plain, serializable snapshots of the composition — deliberately separate
// from `PlacedModule`/`ModuleBody`, which hold runtime-only things (GPU
// textures, capture threads) that can't be serialized. A camera is saved
// by device name rather than its list index, since that index isn't
// guaranteed stable across runs.

#[derive(Serialize, Deserialize, Clone)]
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
    /// The category a Shader's list is limited to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shader_category: Option<String>,
    /// The module's number box (Blend's mix, Number's value, LFO's
    /// frequency) with its range. Older files only have `mix`/`value`/`freq`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    param: Option<SavedParam>,
    /// An LFO's waveform and whether it swings around zero.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wave: Option<Wave>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    centered: Option<bool>,
    /// A Video's file, loop mode, whether it's playing, and its playhead (s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    video_path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    video_loop: Option<LoopMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    playing: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    position: Option<f64>,
    /// Map's range fields (an old Clamp saved just min and max), and its
    /// clamp, autorange and center toggles.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    fields: Option<Vec<SavedParam>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    clamp: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    map_auto: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    map_center: Option<bool>,
    /// A WLED module's device (address) and the GPIO it outputs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wled_device: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    wled_pin: Option<u8>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    blend_mode: Option<BlendMode>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    lag_mode: Option<LagMode>,
    /// A Shader's property boxes, by property name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    shader_params: Option<BTreeMap<String, SavedParam>>,
}

#[derive(Serialize, Deserialize, Clone, Copy)]
struct SavedParam {
    value: f32,
    min: f32,
    max: f32,
    clamp: bool,
}

impl From<Param> for SavedParam {
    fn from(p: Param) -> Self {
        SavedParam { value: p.value, min: p.min, max: p.max, clamp: p.clamp }
    }
}

impl SavedParam {
    fn restore(self) -> Param {
        let mut p = Param::ranged(self.value, self.min, self.max);
        p.set_clamp(self.clamp);
        p.set(self.value);
        p
    }
}

fn restore_fields(fields: &mut [Param], saved: &Option<Vec<SavedParam>>) {
    for (field, saved) in fields.iter_mut().zip(saved.iter().flatten()) {
        *field = saved.restore();
    }
}

/// The saved box if there is one, else the old bare value on top of `default`.
fn restore_param(saved: Option<SavedParam>, legacy: Option<f32>, default: Param) -> Param {
    match (saved, legacy) {
        (Some(p), _) => p.restore(),
        (None, Some(v)) => {
            // Old files stored raw values; widen the range rather than
            // clamp one that doesn't fit.
            let mut p = default;
            p.set_range(p.min.min(v), p.max.max(v));
            p.set(v);
            p
        }
        (None, None) => default,
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct SavedConnection {
    from: u64,
    to: u64,
    to_input: usize,
}

/// A group frame: its members by module id.
#[derive(Serialize, Deserialize, Clone)]
struct SavedGroup {
    title: String,
    members: Vec<u64>,
    #[serde(default)]
    collapsed: bool,
}

#[derive(Serialize, Deserialize)]
struct SaveFile {
    /// The project's tab name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    name: Option<String>,
    output_id: u64,
    modules: Vec<SavedModule>,
    connections: Vec<SavedConnection>,
    #[serde(default)]
    groups: Vec<SavedGroup>,
}

/// A piece of a composition — some modules, the cables between them and
/// the groups they form — used by copy/paste, duplicate and palette presets.
#[derive(Serialize, Deserialize, Clone, Default)]
pub struct Snippet {
    modules: Vec<SavedModule>,
    connections: Vec<SavedConnection>,
    #[serde(default)]
    groups: Vec<SavedGroup>,
}

impl Snippet {
    pub fn is_empty(&self) -> bool {
        self.modules.is_empty()
    }

    /// Identifies the modules, their parameters and the cables between
    /// them, ignoring ids, positions, names and view state (previews, a
    /// video's playhead), so two copies of the same patch match.
    pub fn fingerprint(&self) -> String {
        let signature = |m: &SavedModule| {
            let mut value = serde_json::to_value(m).unwrap_or_default();
            if let Some(map) = value.as_object_mut() {
                for key in ["id", "pos", "minimized", "show_preview", "playing", "position"] {
                    map.remove(key);
                }
            }
            value.to_string()
        };
        let signatures: BTreeMap<u64, String> = self.modules.iter().map(|m| (m.id, signature(m))).collect();
        let mut modules: Vec<&String> = signatures.values().collect();
        modules.sort();
        let mut cables: Vec<String> = self
            .connections
            .iter()
            .filter_map(|c| Some(format!("{}>{}:{}", signatures.get(&c.from)?, signatures.get(&c.to)?, c.to_input)))
            .collect();
        cables.sort();
        format!("{modules:?}|{cables:?}")
    }
}

/// A group saved to the sidebar's personal palette.
#[derive(Serialize, Deserialize, Clone)]
pub struct Preset {
    pub name: String,
    pub snippet: Snippet,
}

pub const PRESETS_PATH: &str = "presets.json";

pub fn load_presets() -> Vec<Preset> {
    std::fs::read_to_string(PRESETS_PATH)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

pub fn save_presets(presets: &[Preset]) {
    if let Ok(json) = serde_json::to_string_pretty(presets) {
        let _ = std::fs::write(PRESETS_PATH, json);
    }
}

fn save_group(g: &Group) -> SavedGroup {
    SavedGroup { title: g.title.clone(), members: g.members.clone(), collapsed: g.collapsed }
}

fn save_connection(c: &Connection) -> SavedConnection {
    SavedConnection { from: c.from, to: c.to, to_input: c.to_input }
}

/// The modules `ids` (Output excluded: there's only ever one), the cables
/// running between them, and the groups made only of them.
pub fn snippet(model: &Model, ids: &[u64]) -> Snippet {
    let modules: Vec<SavedModule> = model
        .modules
        .iter()
        .filter(|m| ids.contains(&m.id) && m.kind != ModuleKind::Output)
        .map(save_module)
        .collect();
    let inside = |id: u64| modules.iter().any(|m| m.id == id);
    let connections = model.connections.iter().filter(|c| inside(c.from) && inside(c.to)).map(save_connection).collect();
    let groups = model
        .groups
        .iter()
        .filter(|g| g.members.iter().all(|id| inside(*id)))
        .map(save_group)
        .collect();
    Snippet { modules, connections, groups }
}

/// Inserts a copy of `snippet` with fresh ids, centered on `center`.
/// Returns the new modules' ids.
pub fn paste(model: &mut Model, snippet: &Snippet, center: Point2) -> Vec<u64> {
    if snippet.modules.is_empty() {
        return Vec::new();
    }
    let n = snippet.modules.len() as f32;
    let centroid = snippet.modules.iter().fold(Vec2::ZERO, |acc, m| acc + vec2(m.pos.0, m.pos.1)) / n;
    let mut new_ids = std::collections::HashMap::new();
    for saved in &snippet.modules {
        let id = model.next_id;
        model.next_id += 1;
        new_ids.insert(saved.id, id);
        let pos = vec2(saved.pos.0, saved.pos.1) - centroid + center;
        let body = restore_body(saved.clone(), &model.available_cameras);
        model.modules.push(PlacedModule { id, kind: saved.kind, pos, body });
    }
    for c in &snippet.connections {
        if let (Some(&from), Some(&to)) = (new_ids.get(&c.from), new_ids.get(&c.to)) {
            model.connections.push(Connection { from, to, to_input: c.to_input });
        }
    }
    for g in &snippet.groups {
        let members: Vec<u64> = g.members.iter().filter_map(|id| new_ids.get(id).copied()).collect();
        model.add_group(g.title.clone(), members, g.collapsed);
    }
    snippet.modules.iter().filter_map(|m| new_ids.get(&m.id).copied()).collect()
}

fn save_module(m: &PlacedModule) -> SavedModule {
    let chrome = m.body.chrome();
    let mut camera_name = None;
    let mut mix = None;
    let mut value = None;
    let mut freq = None;
    let mut shader_file = None;
    let mut shader_category = None;
    let mut param = None;
    let mut shader_params = None;
    let mut wave = None;
    let mut centered = None;
    let mut blend_mode = None;
    let mut lag_mode = None;
    let (mut video_path, mut video_loop, mut playing, mut position) = (None, None, None, None);
    let (mut fields, mut clamp, mut wled_device, mut wled_pin) = (None, None, None, None);
    let (mut map_auto, mut map_center) = (None, None);
    match &m.body {
        ModuleBody::Camera(cam) => {
            camera_name = cam.wanted.clone();
        }
        ModuleBody::Blend(blend) => {
            mix = Some(blend.mix.value);
            param = Some(blend.mix.into());
            blend_mode = Some(blend.mode);
        }
        ModuleBody::Number(number) => {
            value = Some(number.value.value);
            param = Some(number.value.into());
        }
        ModuleBody::Lfo(lfo) => {
            freq = Some(lfo.freq.value);
            param = Some(lfo.freq.into());
            wave = Some(lfo.wave);
            centered = Some(lfo.centered);
        }
        ModuleBody::Shader(shader) => {
            shader_file = shader.file.clone();
            shader_category = shader.category.clone();
            let mut tuned: BTreeMap<String, SavedParam> =
                shader.tuned.iter().map(|(k, p)| (k.clone(), (*p).into())).collect();
            for (prop, p) in shader.props().iter().zip(&shader.params) {
                tuned.insert(prop.name.clone(), (*p).into());
            }
            shader_params = (!tuned.is_empty()).then_some(tuned);
        }
        ModuleBody::Map(map) => {
            fields = Some(map.fields.iter().map(|p| (*p).into()).collect());
            clamp = Some(map.clamp);
            map_auto = Some(map.auto);
            map_center = Some(map.center);
        }
        ModuleBody::Wled(w) => {
            wled_device = w.device.map(|a| a.to_string());
            wled_pin = w.pin;
            param = Some(w.raw_max.into());
        }
        ModuleBody::Video(video) => {
            video_path = video.path.as_ref().map(|p| p.to_string_lossy().to_string());
            param = Some(video.speed.into());
            video_loop = Some(video.looping);
            playing = Some(video.playing);
            position = Some(video.position);
        }
        ModuleBody::Osc(osc) => {
            fields = Some(osc.fields.iter().map(|p| (*p).into()).collect());
        }
        ModuleBody::Lag(lag) => {
            param = Some(lag.time.into());
            lag_mode = Some(lag.mode);
        }
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
        shader_category,
        param,
        wave,
        centered,
        video_path,
        video_loop,
        playing,
        position,
        fields,
        clamp,
        map_auto,
        map_center,
        wled_device,
        wled_pin,
        blend_mode,
        lag_mode,
        shader_params,
    }
}

fn build_save_file(model: &Model) -> SaveFile {
    SaveFile {
        name: model.tabs.get(model.active_tab).map(|t| t.name.clone()),
        output_id: model.output_id,
        modules: model.modules.iter().map(save_module).collect(),
        connections: model.connections.iter().map(save_connection).collect(),
        groups: model.groups.iter().map(save_group).collect(),
    }
}

pub fn save_composition(model: &Model) -> std::io::Result<()> {
    save_composition_to(model, std::path::Path::new(SAVE_PATH))
}

pub fn save_composition_to(model: &Model, path: &std::path::Path) -> std::io::Result<()> {
    let file = build_save_file(model);
    let json = serde_json::to_string_pretty(&file).expect("composition is always serializable");
    std::fs::write(path, json)
}

/// Replaces the current composition with whatever's in `path`. Camera
/// modules are re-resolved by matching `camera_name` against the cameras
/// actually available right now — a device that isn't connected on this
/// machine just loads unselected rather than failing the whole load.
pub fn load_composition(model: &mut Model, win: Rect, path: &std::path::Path) -> Result<(), String> {
    let json = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    load_composition_json(model, win, &json)
}

/// Replaces the canvas with the composition in `json`.
pub fn load_composition_json(model: &mut Model, win: Rect, json: &str) -> Result<(), String> {
    let file: SaveFile = serde_json::from_str(json).map_err(|e| e.to_string())?;

    let mut modules = Vec::with_capacity(file.modules.len());
    for saved in file.modules {
        let (id, kind, pos) = (saved.id, saved.kind, pt2(saved.pos.0, saved.pos.1));
        let body = restore_body(saved, &model.available_cameras);
        modules.push(PlacedModule { id, kind, pos, body });
    }
    finish_load(model, win, modules, file.output_id, file.connections, file.groups);
    if let (Some(name), Some(tab)) = (file.name.filter(|n| !n.is_empty()), model.tabs.get_mut(model.active_tab)) {
        tab.name = name;
    }
    Ok(())
}

/// Rebuilds a module's runtime state from its snapshot. A camera is matched
/// by name against the cameras connected right now.
fn restore_body(saved: SavedModule, cameras: &[CameraInfo]) -> ModuleBody {
    let mut chrome = ModuleChrome::new();
    chrome.minimized = saved.minimized;
    // Only modules that carry video have a preview to show.
    chrome.show_preview = saved.show_preview && crate::modules::has_video_preview(saved.kind);

    let body = match saved.kind {
        ModuleKind::Camera => {
            let mut cam = CameraState::new();
            cam.chrome = chrome;
            if let Some(name) = &saved.camera_name {
                // Kept even if the device isn't plugged in right now:
                // it attaches when it shows up.
                cam.wanted = Some(name.clone());
                cam.selected = cameras
                    .iter()
                    .position(|info| info.human_name() == *name);
            }
            ModuleBody::Camera(cam)
        }
        ModuleKind::Blend | ModuleKind::Stack => {
            let mut blend = BlendState::new();
            blend.chrome = chrome;
            blend.mix = restore_param(saved.param, saved.mix, blend.mix);
            blend.mode = saved.blend_mode.unwrap_or_default();
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
            number.value = restore_param(saved.param, saved.value, number.value);
            ModuleBody::Number(number)
        }
        ModuleKind::Video => {
            let mut video = VideoState::new();
            video.chrome = chrome;
            if let Some(path) = &saved.video_path {
                video.open(path.into());
            }
            video.speed = restore_param(saved.param, None, video.speed);
            video.looping = saved.video_loop.unwrap_or_default();
            video.playing = saved.playing.unwrap_or(true);
            video.position = saved.position.unwrap_or(0.0).max(0.0);
            ModuleBody::Video(video)
        }
        ModuleKind::Map => {
            let mut map = MapState::new();
            map.chrome = chrome;
            match saved.fields.as_deref() {
                // A Clamp from before it was folded into Map: min and max only.
                Some([min, max]) => {
                    for (i, p) in [*min, *max, *min, *max].into_iter().enumerate() {
                        map.fields[i] = p.restore();
                    }
                    map.clamp = true;
                }
                _ => {
                    restore_fields(&mut map.fields, &saved.fields);
                    map.clamp = saved.clamp.unwrap_or(false);
                }
            }
            map.auto = saved.map_auto.unwrap_or(false);
            map.center = saved.map_center.unwrap_or(false);
            ModuleBody::Map(map)
        }
        ModuleKind::Wled => {
            let mut w = WledState::new();
            w.chrome = chrome;
            w.device = saved.wled_device.as_deref().and_then(|a| a.parse().ok());
            w.pin = saved.wled_pin;
            w.raw_max = restore_param(saved.param, None, w.raw_max);
            ModuleBody::Wled(w)
        }
        ModuleKind::Osc => {
            let mut osc = OscState::new();
            osc.chrome = chrome;
            restore_fields(&mut osc.fields, &saved.fields);
            ModuleBody::Osc(osc)
        }
        ModuleKind::Lag => {
            let mut lag = LagState::new();
            lag.chrome = chrome;
            lag.time = restore_param(saved.param, None, lag.time);
            lag.mode = saved.lag_mode.unwrap_or_default();
            ModuleBody::Lag(lag)
        }
        ModuleKind::Mult => {
            let mut mult = MultState::new();
            mult.chrome = chrome;
            ModuleBody::Mult(mult)
        }
        ModuleKind::Lfo => {
            let mut lfo = LfoState::new();
            lfo.chrome = chrome;
            lfo.freq = restore_param(saved.param, saved.freq, lfo.freq);
            lfo.wave = saved.wave.unwrap_or_default();
            lfo.centered = saved.centered.unwrap_or(false);
            ModuleBody::Lfo(lfo)
        }
        ModuleKind::Shader | ModuleKind::Generator | ModuleKind::Effect | ModuleKind::Feedback | ModuleKind::ShaderMix => {
            // Ports and defaults come from the file, so they're built
            // on the next tick, when the shader gets compiled.
            let mut shader = ShaderState::new();
            shader.chrome = chrome;
            shader.select(saved.shader_file);
            shader.category = saved.shader_category;
            // Applied by name once the file compiles.
            shader.tuned = saved
                .shader_params
                .unwrap_or_default()
                .into_iter()
                .map(|(k, p)| (k, p.restore()))
                .collect();
            ModuleBody::Shader(shader)
        }
    };
    body
}

fn finish_load(
    model: &mut Model,
    win: Rect,
    mut modules: Vec<PlacedModule>,
    output_id: u64,
    connections: Vec<SavedConnection>,
    groups: Vec<SavedGroup>,
) {
    let mut next_id = modules.iter().map(|m| m.id).max().map(|id| id + 1).unwrap_or(0);
    let mut output_id = output_id;

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

    let connections = connections
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
    model.selected.clear();
    model.groups.clear();
    for g in groups {
        model.add_group(g.title, g.members, g.collapsed);
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bundled_examples_parse() {
        for example in crate::examples::all() {
            let file: SaveFile = serde_json::from_str(example.json).unwrap_or_else(|e| panic!("{}: {e}", example.name));
            assert!(file.modules.iter().any(|m| m.kind == ModuleKind::Output), "{}: no Output", example.name);
            for c in &file.connections {
                assert!(file.modules.iter().any(|m| m.id == c.from), "{}: dangling cable", example.name);
                assert!(file.modules.iter().any(|m| m.id == c.to), "{}: dangling cable", example.name);
            }
        }
    }

    fn snippet_of(json: &str) -> Snippet {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn fingerprint_ignores_ids_and_positions_but_not_parameters() {
        let a = snippet_of(
            r#"{"modules":[{"id":1,"kind":"number","pos":[0,0],"minimized":false,"show_preview":true,"value":0.5},
                           {"id":2,"kind":"lag","pos":[300,0],"minimized":false,"show_preview":true}],
                "connections":[{"from":1,"to":2,"to_input":0}]}"#,
        );
        let moved = snippet_of(
            r#"{"modules":[{"id":9,"kind":"lag","pos":[5,5],"minimized":true,"show_preview":false},
                           {"id":7,"kind":"number","pos":[-40,80],"minimized":false,"show_preview":true,"value":0.5}],
                "connections":[{"from":7,"to":9,"to_input":0}]}"#,
        );
        let tuned = snippet_of(
            r#"{"modules":[{"id":1,"kind":"number","pos":[0,0],"minimized":false,"show_preview":true,"value":0.7},
                           {"id":2,"kind":"lag","pos":[300,0],"minimized":false,"show_preview":true}],
                "connections":[{"from":1,"to":2,"to_input":0}]}"#,
        );
        assert_eq!(a.fingerprint(), moved.fingerprint());
        assert_ne!(a.fingerprint(), tuned.fingerprint());
    }

    #[test]
    fn old_clamp_loads_as_a_clamping_map() {
        let saved: SavedModule = serde_json::from_str(
            r#"{"id":1,"kind":"clamp","pos":[0,0],"minimized":false,"show_preview":false,
                "fields":[{"value":0.2,"min":0,"max":1,"clamp":false},{"value":0.8,"min":0,"max":1,"clamp":false}]}"#,
        )
        .unwrap();
        let ModuleBody::Map(map) = restore_body(saved, &[]) else { panic!("not a Map") };
        assert!(map.clamp);
        assert_eq!(map.apply(0.5), 0.5);
        assert_eq!(map.apply(5.0), 0.8);
    }

    #[test]
    fn param_survives_json() {
        let mut p = Param::ranged(4.0, 2.0, 10.0);
        p.set_clamp(false);
        let json = serde_json::to_string(&SavedParam::from(p)).unwrap();
        let back: SavedParam = serde_json::from_str(&json).unwrap();
        assert_eq!(back.restore(), p);
    }

    #[test]
    fn legacy_values_load_without_clamping() {
        let p = restore_param(None, Some(3.0), Param::unit(0.0));
        assert_eq!(p.value, 3.0);
        assert_eq!((p.min, p.max), (0.0, 3.0));
        let q = restore_param(None, Some(0.25), Param::unit(0.5));
        assert_eq!((q.value, q.min, q.max), (0.25, 0.0, 1.0));
    }

    #[test]
    fn bundled_examples_still_parse() {
        for entry in std::fs::read_dir("examples").unwrap().flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let json = std::fs::read_to_string(&path).unwrap();
                serde_json::from_str::<SaveFile>(&json).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
            }
        }
    }
}
