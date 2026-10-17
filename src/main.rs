mod debug_log;
mod gpu;
mod graph;
mod modules;
mod save;
mod settings;

use modules::sources::camera::{discover_cameras, CameraWatcher, CameraWorker};
use modules::effects::shader_file::scan_library;
use modules::{
    body_color, has_config_row, has_output, kind_label, module_number, output_type,
    port_color, push_module as modules_push_module, reaches, title_buttons_for, Connection,
    ModuleBody, ModuleKind, PlacedModule, PortType, TitleButton, CATEGORIES, SHADER_PORT_STEP,
    SHADER_PORT_TOP,
};
use nannou::prelude::*;
use nokhwa::utils::CameraInfo;
use save::{load_composition, save_composition};
use settings::Settings;
use std::cell::Cell;
use std::time::{Duration, Instant};

const IDLE_TIMEOUT: Duration = Duration::from_millis(1500);
const SAVE_STATUS_TIMEOUT: Duration = Duration::from_millis(2500);
pub const SAVE_PATH: &str = "composition.json";
const PORT_RADIUS: f32 = 10.0;
const PORT_HIT_RADIUS: f32 = 18.0;
const SIDEBAR_WIDTH: f32 = 220.0;
const SIDEBAR_HEADER_H: f32 = 64.0;
const MODULE_W: f32 = 220.0;
const MINIMIZED_H: f32 = 34.0;
const PREVIEW_H: f32 = 90.0;
const DROPDOWN_ROW_H: f32 = 26.0;
const NUMBER_DRAG_SENSITIVITY: f32 = 0.02;
const FREQ_DRAG_SENSITIVITY: f32 = 0.01;
const SCROLL_LINE_PIXELS: f32 = 40.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Config,
    Fullscreen,
    FullscreenOverlay,
}

/// State every module has regardless of kind: whether it's collapsed to
/// just its title bar, whether its inline thumbnail is showing, and the
/// texture backing that thumbnail, handed over by `graph::run_graph` each
/// tick (the video itself never leaves the GPU).
pub struct ModuleChrome {
    pub minimized: bool,
    pub show_preview: bool,
    pub preview_texture: Option<gpu::DrawTex>,
}

impl ModuleChrome {
    pub fn new() -> Self {
        ModuleChrome {
            minimized: false,
            show_preview: false,
            preview_texture: None,
        }
    }
}

#[derive(Clone, Copy)]
enum Drag {
    Module { id: u64, offset: Vec2 },
    Cable { from: u64 },
    Slider { id: u64 },
    /// An open-ended drag-to-adjust value (Number's value, LFO's
    /// frequency): `last_x` is the mouse x from the previous move event,
    /// so each move applies a relative delta rather than an absolute
    /// position (unlike Slider, which is bounded 0..1).
    NumberKnob { id: u64, last_x: f32 },
    /// Panning the canvas (middle button, or left + Shift); `last` is the
    /// previous screen-space mouse position.
    Pan { last: Vec2 },
    /// Dragging one of the settings sliders (0 = scroll speed, 1 = drag speed).
    Setting { index: usize },
}

enum SidebarRow {
    Header {
        rect: Rect,
        label: &'static str,
    },
    Module {
        row_rect: Rect,
        plus_rect: Rect,
        label: &'static str,
        kind: ModuleKind,
    },
}

pub struct Model {
    view: View,
    last_mouse_move: Instant,
    mouse_down: bool,

    pub available_cameras: Vec<CameraInfo>,
    camera_watcher: CameraWatcher,
    pub blend_pipeline: gpu::BlendPipeline,
    pub yuyv_pipeline: gpu::YuyvPipeline,
    /// File names found in the shaders folder (rescanned whenever a Shader's dropdown opens).
    pub shader_library: Vec<String>,
    pub next_id: u64,
    pub modules: Vec<PlacedModule>,
    pub connections: Vec<Connection>,
    pub(crate) dragging: Option<Drag>,
    pub output_id: u64,
    /// Canvas offset: a module at world position `p` is drawn at `p + pan`.
    /// The canvas has no bounds; scrolling or dragging just moves this.
    pan: Vec2,
    settings: Settings,
    settings_open: bool,
    /// The hidden Debug section of the settings panel is expanded.
    debug_open: bool,
    status: Option<(String, bool, Instant)>,
    fps: FpsCounter,
    pub(crate) timings: Timings,
    /// Time spent in `view` since the last `update` (`view` only gets `&Model`).
    view_time: Cell<Duration>,
}

/// Frames per second over the last quarter second, as ready-to-draw text.
struct FpsCounter {
    since: Instant,
    frames: u32,
    label: String,
}

impl FpsCounter {
    fn new() -> Self {
        FpsCounter { since: Instant::now(), frames: 0, label: String::from("-- fps") }
    }

    fn tick(&mut self) {
        self.frames += 1;
        let elapsed = self.since.elapsed().as_secs_f32();
        if elapsed >= 0.25 {
            let fps = self.frames as f32 / elapsed;
            self.label = format!("{fps:.1} fps \u{00B7} {:.1} ms", 1000.0 / fps);
            self.since = Instant::now();
            self.frames = 0;
        }
    }
}

/// Where the main thread's time goes, summed and logged once a second.
pub(crate) struct Timings {
    since: Instant,
    frames: u32,
    update: Duration,
    pub(crate) uploads: Duration,
    view: Duration,
}

impl Timings {
    fn new() -> Self {
        Timings { since: Instant::now(), frames: 0, update: Duration::ZERO, uploads: Duration::ZERO, view: Duration::ZERO }
    }

    fn report_every_second(&mut self) {
        let elapsed = self.since.elapsed();
        if elapsed < Duration::from_secs(1) {
            return;
        }
        let n = self.frames.max(1) as f32;
        let ms = |d: Duration| d.as_secs_f32() * 1000.0 / n;
        dlog!(
            "main: {:.1} fps | update {:.2}ms (camera uploads {:.2}ms) | view {:.2}ms",
            self.frames as f32 / elapsed.as_secs_f32(),
            ms(self.update),
            ms(self.uploads),
            ms(self.view)
        );
        *self = Timings::new();
    }
}

/// The on/off rows under the settings sliders.
#[derive(Clone, Copy)]
enum SettingsToggle {
    ShowFps,
    /// Expands/collapses the rows below; not a setting itself.
    DebugSection,
    DebugLog,
    SerializeGrabs,
}

fn settings_toggles(debug_open: bool) -> &'static [SettingsToggle] {
    use SettingsToggle::*;
    if debug_open {
        &[ShowFps, DebugSection, DebugLog, SerializeGrabs]
    } else {
        &[ShowFps, DebugSection]
    }
}

fn main() {
    nannou::app(model)
        .update(update)
        .event(event)
        .simple_window(view)
        .run();
}

fn model(app: &App) -> Model {
    let canvas = canvas_rect(app.window_rect());
    let settings = Settings::load();
    settings.apply_globals();
    let available_cameras = discover_cameras();
    dlog!("cameras found: {}", camera_names(&available_cameras));
    let mut model = Model {
        view: View::Config,
        last_mouse_move: Instant::now(),
        mouse_down: false,
        camera_watcher: CameraWatcher::spawn(available_cameras.clone()),
        available_cameras,
        blend_pipeline: gpu::BlendPipeline::new(app.main_window().device()),
        yuyv_pipeline: gpu::YuyvPipeline::new(app.main_window().device()),
        shader_library: scan_library(),
        next_id: 0,
        modules: Vec::new(),
        connections: Vec::new(),
        dragging: None,
        output_id: 0,
        pan: Vec2::ZERO,
        settings,
        settings_open: false,
        debug_open: false,
        status: None,
        fps: FpsCounter::new(),
        timings: Timings::new(),
        view_time: Cell::new(Duration::ZERO),
    };
    model.output_id = push_module(&mut model, ModuleKind::Output, canvas.xy());
    model
}

fn camera_names(cameras: &[CameraInfo]) -> String {
    cameras.iter().map(|c| c.human_name()).collect::<Vec<_>>().join(", ")
}

fn update(app: &App, model: &mut Model, update: Update) {
    let started = Instant::now();
    model.fps.tick();

    if model.view == View::FullscreenOverlay
        && !model.mouse_down
        && model.dragging.is_none()
        && model.last_mouse_move.elapsed() > IDLE_TIMEOUT
    {
        model.view = View::Fullscreen;
    }

    if let Some((_, _, at)) = &model.status {
        if at.elapsed() > SAVE_STATUS_TIMEOUT {
            model.status = None;
        }
    }

    // Cameras plugged in or out while running: adopt the new list and
    // re-attach every Camera module to its device by name.
    while let Ok(list) = model.camera_watcher.updates.try_recv() {
        dlog!("camera list changed: {}", camera_names(&list));
        let old = std::mem::replace(&mut model.available_cameras, list);
        for module in &mut model.modules {
            if let ModuleBody::Camera(cam) = &mut module.body {
                cam.relink(&old, &model.available_cameras);
            }
        }
    }

    // A capture thread that died (device yanked mid-stream) leaves its
    // module detached; the watcher re-attaches it if the camera returns.
    for module in &mut model.modules {
        if let ModuleBody::Camera(cam) = &mut module.body {
            if cam.worker.as_ref().is_some_and(|w| !w.is_alive()) {
                dlog!("camera module {}: capture thread ended, detaching", module.id);
                cam.worker = None;
                cam.selected = None;
            }
        }
    }

    // Only keep a camera's capture thread alive while it's actually in use
    // — either its own preview is on, or it's reachable (through any chain
    // of Blends etc.) from Output. Running an unused camera is wasteful
    // and, on real hardware, can make devices contend for shared
    // USB/driver resources.
    let output_id = model.output_id;
    let needed_ids: std::collections::HashSet<u64> = model
        .modules
        .iter()
        .filter(|m| reaches(&model.connections, m.id, output_id, &mut Vec::new()))
        .map(|m| m.id)
        .collect();

    for module in &mut model.modules {
        if let ModuleBody::Camera(cam) = &mut module.body {
            let needed = cam.chrome.show_preview || needed_ids.contains(&module.id);
            if needed && cam.worker.is_none() {
                if let Some(info) = cam.selected.and_then(|i| model.available_cameras.get(i)) {
                    cam.worker = Some(CameraWorker::spawn(info.index().clone()));
                }
            } else if !needed && cam.worker.is_some() {
                cam.worker = None;
            }
        }
    }

    graph::run_graph(app, model, update.since_last.as_secs_f32());

    let timings = &mut model.timings;
    timings.frames += 1;
    timings.update += started.elapsed();
    timings.view += model.view_time.take();
    timings.report_every_second();
}

fn event(app: &App, model: &mut Model, event: Event) {
    if let Event::WindowEvent {
        simple: Some(window_event),
        ..
    } = event
    {
        match window_event {
            WindowEvent::MouseMoved(screen_pos) => {
                model.last_mouse_move = Instant::now();
                // Everything on the canvas lives in world space.
                let pos = screen_pos - model.pan;
                if model.view == View::Fullscreen {
                    model.view = View::FullscreenOverlay;
                }
                if model.view == View::Config {
                    match model.dragging {
                        Some(Drag::Setting { index }) => {
                            let t = slider_value_at(settings_slider_rect(app.window_rect(), index), screen_pos.x);
                            set_setting(&mut model.settings, index, t);
                        }
                        Some(Drag::Pan { last }) => {
                            model.pan += (screen_pos - last) * model.settings.drag_speed;
                            model.dragging = Some(Drag::Pan { last: screen_pos });
                        }
                        Some(Drag::Module { id, offset }) => {
                            if let Some(m) = model.modules.iter_mut().find(|m| m.id == id) {
                                m.pos = pos + offset;
                            }
                        }
                        Some(Drag::Slider { id }) => {
                            if let Some(m) = model.modules.iter_mut().find(|m| m.id == id) {
                                let config = config_rect(module_body_rect(m));
                                if let ModuleBody::Blend(blend) = &mut m.body {
                                    blend.mix = slider_value_at(config, pos.x);
                                }
                            }
                        }
                        Some(Drag::NumberKnob { id, last_x }) => {
                            let delta = pos.x - last_x;
                            if let Some(m) = model.modules.iter_mut().find(|m| m.id == id) {
                                match &mut m.body {
                                    ModuleBody::Number(number) => {
                                        number.value += delta * NUMBER_DRAG_SENSITIVITY;
                                    }
                                    ModuleBody::Lfo(lfo) => {
                                        lfo.freq = (lfo.freq + delta * FREQ_DRAG_SENSITIVITY).max(0.0);
                                    }
                                    _ => {}
                                }
                            }
                            model.dragging = Some(Drag::NumberKnob { id, last_x: pos.x });
                        }
                        _ => {}
                    }
                }
            }
            WindowEvent::MousePressed(button) => {
                model.mouse_down = true;
                let screen = pt2(app.mouse.x, app.mouse.y);
                let can_pan = model.view == View::Config
                    && model.dragging.is_none()
                    && !sidebar_rect(app.window_rect()).contains(screen)
                    && !(model.settings_open && settings_panel_rect(app.window_rect(), model.debug_open).contains(screen));
                if button == MouseButton::Middle && can_pan {
                    model.dragging = Some(Drag::Pan { last: screen });
                } else if button == MouseButton::Left {
                    if can_pan && app.keys.mods.shift() {
                        model.dragging = Some(Drag::Pan { last: screen });
                    } else {
                        handle_press(app, model);
                    }
                }
            }
            WindowEvent::MouseReleased(button) => {
                model.mouse_down = false;
                if button == MouseButton::Left {
                    handle_release(app, model);
                } else if button == MouseButton::Middle && matches!(model.dragging, Some(Drag::Pan { .. })) {
                    model.dragging = None;
                }
            }
            WindowEvent::KeyPressed(Key::H) => {
                if model.view == View::Config {
                    center_on_modules(model, app.window_rect());
                }
            }
            WindowEvent::MouseWheel(delta, _) => {
                if model.view == View::Config {
                    let mut d = match delta {
                        MouseScrollDelta::LineDelta(x, y) => vec2(x, y) * SCROLL_LINE_PIXELS,
                        MouseScrollDelta::PixelDelta(p) => vec2(p.x as f32, p.y as f32),
                    };
                    // A plain mouse wheel only scrolls vertically; Shift sends it sideways.
                    if app.keys.mods.shift() && d.x == 0.0 {
                        d = vec2(d.y, 0.0);
                    }
                    model.pan += d * model.settings.scroll_speed;
                }
            }
            _ => {}
        }
    }
}

fn handle_press(app: &App, model: &mut Model) {
    let win = app.window_rect();
    let mouse = pt2(app.mouse.x, app.mouse.y);

    match model.view {
        View::Config => handle_press_config(model, win, mouse),
        View::FullscreenOverlay => {
            if exit_button_rect(win).contains(mouse) {
                app.main_window().set_fullscreen(false);
                model.view = View::Config;
            }
        }
        View::Fullscreen => {}
    }

    if model.view == View::Config && start_button_rect(win).contains(mouse) {
        app.main_window().set_fullscreen(true);
        model.view = View::Fullscreen;
        model.last_mouse_move = Instant::now();
    }
}

fn handle_press_config(model: &mut Model, win: Rect, screen_mouse: Point2) {
    // Sidebar and buttons live in screen space; modules in world space.
    let mouse = screen_mouse - model.pan;

    // A click anywhere while a dropdown is open either picks an option or
    // dismisses it; it never falls through to anything underneath.
    if let Some(open_idx) = model.modules.iter().position(|m| match &m.body {
        ModuleBody::Camera(cam) => cam.dropdown_open,
        ModuleBody::Shader(s) => s.dropdown_open,
        _ => false,
    }) {
        let options = dropdown_labels(model, &model.modules[open_idx]);
        let selector = config_rect(module_body_rect(&model.modules[open_idx]));

        let mut choice = None;
        for row in 0..options.len() {
            if dropdown_option_rect(selector, row).contains(mouse) {
                choice = Some(if row == 0 { None } else { Some(row - 1) });
                break;
            }
        }

        let library = model.shader_library.clone();
        let available = model.available_cameras.clone();
        match &mut model.modules[open_idx].body {
            ModuleBody::Camera(cam) => {
                cam.dropdown_open = false;
                if let Some(pick) = choice {
                    cam.select(pick, &available);
                }
            }
            ModuleBody::Shader(shader) => {
                shader.dropdown_open = false;
                if let Some(pick) = choice {
                    shader.select(pick.and_then(|i| library.get(i).cloned()));
                }
            }
            _ => {}
        }
        return;
    }

    if settings_button_rect(win).contains(screen_mouse) {
        model.settings_open = !model.settings_open;
        return;
    }
    if model.settings_open && settings_panel_rect(win, model.debug_open).contains(screen_mouse) {
        for index in 0..2 {
            let rect = settings_slider_rect(win, index);
            if rect.pad(4.0).contains(screen_mouse) {
                set_setting(&mut model.settings, index, slider_value_at(rect, screen_mouse.x));
                model.dragging = Some(Drag::Setting { index });
            }
        }
        let toggles = settings_toggles(model.debug_open);
        if let Some(row) = (0..toggles.len()).find(|&row| settings_toggle_rect(win, row).contains(screen_mouse)) {
            let settings = &mut model.settings;
            match toggles[row] {
                SettingsToggle::DebugSection => model.debug_open = !model.debug_open,
                SettingsToggle::ShowFps => settings.show_fps = !settings.show_fps,
                SettingsToggle::DebugLog => settings.debug_log = !settings.debug_log,
                SettingsToggle::SerializeGrabs => {
                    settings.serialize_camera_grabs = !settings.serialize_camera_grabs;
                    dlog!("serialize camera grabs: {}", settings.serialize_camera_grabs);
                }
            }
            settings.apply_globals();
            settings.save();
        }
        return;
    }

    if save_button_rect(win).contains(screen_mouse) {
        let (message, is_error) = match save_composition(model) {
            Ok(()) => (format!("Saved to {SAVE_PATH}"), false),
            Err(err) => (format!("Save failed: {err}"), true),
        };
        model.status = Some((message, is_error, Instant::now()));
        return;
    }

    if load_button_rect(win).contains(screen_mouse) {
        if let Some(path) = rfd::FileDialog::new()
            .add_filter("Composition (*.json)", &["json"])
            .set_file_name(SAVE_PATH)
            .pick_file()
        {
            let (message, is_error) = match load_composition(model, win, &path) {
                Ok(()) => (format!("Loaded {}", path.display()), false),
                Err(err) => (format!("Load failed: {err}"), true),
            };
            model.status = Some((message, is_error, Instant::now()));
        }
        return;
    }

    for row in sidebar_rows(win) {
        if let SidebarRow::Module {
            plus_rect, kind, ..
        } = row
        {
            if plus_rect.contains(screen_mouse) {
                spawn_module(win, model, kind);
                return;
            }
        }
    }

    // Modules scrolled underneath the sidebar can't be grabbed through it.
    if sidebar_rect(win).contains(screen_mouse) {
        return;
    }

    let available = model.available_cameras.clone();

    for i in (0..model.modules.len()).rev() {
        let kind = model.modules[i].kind;
        let id = model.modules[i].id;
        let (minimized, show_preview) = {
            let chrome = model.modules[i].body.chrome();
            (chrome.minimized, chrome.show_preview)
        };
        let body = module_body_rect(&model.modules[i]);

        let mut hit_button = false;
        for (slot, button) in title_buttons_for(kind).iter().enumerate() {
            if title_button_rect(body, slot).contains(mouse) {
                match button {
                    TitleButton::Delete => {
                        model.modules.remove(i);
                        model.connections.retain(|c| c.from != id && c.to != id);
                    }
                    TitleButton::Minimize => {
                        model.modules[i].body.chrome_mut().minimized = !minimized;
                    }
                    TitleButton::Preview => {
                        model.modules[i].body.chrome_mut().show_preview = !show_preview;
                    }
                }
                hit_button = true;
                break;
            }
        }
        if hit_button {
            return;
        }

        if has_config_row(kind) && !minimized {
            let config = config_rect(body);
            if config.contains(mouse) {
                match &mut model.modules[i].body {
                    ModuleBody::Camera(cam) => {
                        if !available.is_empty() {
                            cam.dropdown_open = true;
                        }
                    }
                    ModuleBody::Blend(blend) => {
                        // While a Number is plugged into Mix the slider is
                        // just a readout; dragging it would do nothing.
                        let driven = model.connections.iter().any(|c| c.to == id && c.to_input == 2);
                        if !driven {
                            blend.mix = slider_value_at(config, mouse.x);
                            model.dragging = Some(Drag::Slider { id });
                        }
                    }
                    ModuleBody::Shader(shader) => {
                        shader.dropdown_open = true;
                        model.shader_library = scan_library();
                    }
                    ModuleBody::Number(_) | ModuleBody::Lfo(_) => {
                        model.dragging = Some(Drag::NumberKnob { id, last_x: mouse.x });
                    }
                    ModuleBody::Output(_) | ModuleBody::Mult(_) => {}
                }
                return;
            }
        }

        if has_output(kind) && mouse.distance(module_output_port(body)) <= PORT_HIT_RADIUS {
            model.dragging = Some(Drag::Cable { from: id });
            return;
        }

        let inputs = model.modules[i].input_types().len();
        let mut hit_input = false;
        for input_index in 0..inputs {
            if mouse.distance(module_input_port(kind, body, input_index, inputs)) <= PORT_HIT_RADIUS {
                // Pick up whatever's already plugged in here, if anything,
                // so it can be moved to a different input or unplugged.
                if let Some(pos) = model
                    .connections
                    .iter()
                    .position(|c| c.to == id && c.to_input == input_index)
                {
                    let from = model.connections.remove(pos).from;
                    model.dragging = Some(Drag::Cable { from });
                }
                hit_input = true;
                break;
            }
        }
        if hit_input {
            return;
        }

        if body.contains(mouse) {
            model.dragging = Some(Drag::Module {
                id,
                offset: model.modules[i].pos - mouse,
            });
            return;
        }
    }
}

fn handle_release(app: &App, model: &mut Model) {
    if matches!(model.dragging, Some(Drag::Setting { .. })) {
        model.dragging = None;
        model.settings.save();
        return;
    }
    if matches!(model.dragging, Some(Drag::Pan { .. })) {
        model.dragging = None; // Shift+left pan ends with the left button
        return;
    }
    let drag = model.dragging.take();
    let Some(Drag::Cable { from }) = drag else {
        return;
    };

    let Some(from_type) = model
        .modules
        .iter()
        .find(|m| m.id == from)
        .and_then(|m| output_type(m.kind))
    else {
        return;
    };

    let mouse = pt2(app.mouse.x, app.mouse.y) - model.pan;
    let mut target = None;

    for module in &model.modules {
        let types = module.input_types();
        if types.is_empty() {
            continue;
        }
        let body = module_body_rect(module);
        for (input_index, port_type) in types.iter().enumerate() {
            // A cable can only land on an input of the same signal type —
            // dropping a video cable on a number input (or vice versa)
            // just fails to connect, same as dropping on empty space.
            if *port_type != from_type {
                continue;
            }
            if mouse.distance(module_input_port(module.kind, body, input_index, types.len())) <= PORT_HIT_RADIUS {
                target = Some((module.id, input_index));
                break;
            }
        }
        if target.is_some() {
            break;
        }
    }

    if let Some((to, to_input)) = target {
        model.connections.retain(|c| !(c.to == to && c.to_input == to_input));
        model.connections.push(Connection { from, to, to_input });
    }
    // Dropped in empty space (or on a mismatched port type): stays
    // disconnected (already removed from `connections` in handle_press
    // if it was picked up from an input).
}

fn push_module(model: &mut Model, kind: ModuleKind, pos: Point2) -> u64 {
    modules_push_module(&mut model.modules, &mut model.next_id, kind, pos)
}

fn spawn_module(win: Rect, model: &mut Model, kind: ModuleKind) {
    // Land in the middle of what's currently on screen, whatever the pan.
    let canvas = canvas_rect(win);
    let count = model.modules.len() as f32;
    let pos = canvas.xy() - model.pan + vec2(30.0 * count, -30.0 * count);
    push_module(model, kind, pos);
}

// --- Layout ------------------------------------------------------------

pub fn canvas_rect(win: Rect) -> Rect {
    win.pad_left(SIDEBAR_WIDTH)
}

fn sidebar_rect(win: Rect) -> Rect {
    Rect::from_corners(
        pt2(win.left(), win.bottom()),
        pt2(win.left() + SIDEBAR_WIDTH, win.top()),
    )
}

/// The Save button pinned above the category list, spanning the sidebar.
fn save_button_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    let w = (sidebar.w() - 16.0 - 8.0) / 2.0;
    Rect::from_x_y_w_h(sidebar.x() - w / 2.0 - 4.0, sidebar.top() - SIDEBAR_HEADER_H / 2.0, w, 32.0)
}

fn load_button_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    let w = (sidebar.w() - 16.0 - 8.0) / 2.0;
    Rect::from_x_y_w_h(sidebar.x() + w / 2.0 + 4.0, sidebar.top() - SIDEBAR_HEADER_H / 2.0, w, 32.0)
}

fn settings_button_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    Rect::from_x_y_w_h(sidebar.x(), sidebar.bottom() + 28.0, sidebar.w() - 16.0, 32.0)
}

const SETTINGS_TOGGLE_TOP: f32 = 150.0;
const SETTINGS_TOGGLE_STEP: f32 = 36.0;

/// The floating settings panel, pinned to the canvas' top-right corner; it
/// grows when the Debug section is expanded.
fn settings_panel_rect(win: Rect, debug_open: bool) -> Rect {
    let rows = settings_toggles(debug_open).len() as f32;
    let h = SETTINGS_TOGGLE_TOP + (rows - 1.0) * SETTINGS_TOGGLE_STEP + 32.0;
    Rect::from_w_h(300.0, h).top_right_of(canvas_rect(win)).shift(vec2(-16.0, -16.0))
}

fn settings_slider_rect(win: Rect, index: usize) -> Rect {
    let panel = settings_panel_rect(win, false); // rows only move the bottom edge
    Rect::from_x_y_w_h(panel.x(), panel.top() - 62.0 - index as f32 * 44.0, panel.w() - 32.0, 28.0)
}

fn settings_toggle_rect(win: Rect, row: usize) -> Rect {
    let panel = settings_panel_rect(win, false);
    let y = panel.top() - SETTINGS_TOGGLE_TOP - row as f32 * SETTINGS_TOGGLE_STEP;
    Rect::from_x_y_w_h(panel.x(), y, panel.w() - 32.0, 28.0)
}

/// Sliders are 0..1 on screen; this maps that onto the setting's real range.
fn set_setting(settings: &mut Settings, index: usize, t: f32) {
    let value = Settings::SPEED_MIN + t.clamp(0.0, 1.0) * (Settings::SPEED_MAX - Settings::SPEED_MIN);
    match index {
        0 => settings.scroll_speed = value,
        _ => settings.drag_speed = value,
    }
}

/// Pans so the box around every module sits in the middle of the canvas.
fn center_on_modules(model: &mut Model, win: Rect) {
    let mut bounds: Option<Rect> = None;
    for module in &model.modules {
        let body = module_body_rect(module);
        bounds = Some(match bounds {
            Some(b) => Rect::from_corners(
                pt2(b.left().min(body.left()), b.bottom().min(body.bottom())),
                pt2(b.right().max(body.right()), b.top().max(body.top())),
            ),
            None => body,
        });
    }
    if let Some(b) = bounds {
        model.pan = canvas_rect(win).xy() - b.xy();
    }
}

fn sidebar_rows(win: Rect) -> Vec<SidebarRow> {
    let sidebar = sidebar_rect(win);
    let mut rows = Vec::new();
    let mut y = sidebar.top() - SIDEBAR_HEADER_H - 24.0;

    for (category, mods) in CATEGORIES {
        let rect = Rect::from_x_y_w_h(sidebar.x(), y, sidebar.w() - 16.0, 30.0);
        rows.push(SidebarRow::Header {
            rect,
            label: category,
        });
        y -= 38.0;

        for (label, kind) in mods.iter() {
            let row_rect = Rect::from_x_y_w_h(sidebar.x() + 6.0, y, sidebar.w() - 28.0, 32.0);
            let plus_rect = Rect::from_w_h(24.0, 24.0).mid_right_of(row_rect).shift_x(-6.0);
            rows.push(SidebarRow::Module {
                row_rect,
                plus_rect,
                label,
                kind: *kind,
            });
            y -= 38.0;
        }

        y -= 14.0;
    }

    rows
}

/// Height of a module box: collapsed to just the title bar when minimized,
/// otherwise its normal content plus a preview strip if that's enabled.
fn body_height(minimized: bool, show_preview: bool, base: f32) -> f32 {
    if minimized {
        MINIMIZED_H
    } else if show_preview {
        base + PREVIEW_H + 8.0
    } else {
        base
    }
}

fn module_body_rect(module: &PlacedModule) -> Rect {
    let chrome = module.body.chrome();
    Rect::from_xy_wh(
        module.pos,
        vec2(MODULE_W, body_height(chrome.minimized, chrome.show_preview, module.base_height())),
    )
}

/// The one config-control slot beneath the title, shared by every kind
/// that has a config row (Camera's dropdown, Blend's slider, ...).
fn config_rect(body: Rect) -> Rect {
    Rect::from_x_y_w_h(body.x(), body.top() - 52.0, body.w() - 24.0, 32.0)
}

/// `base` is the module's height without the preview strip; the strip sits
/// in the extra room `body_height` adds below it.
fn preview_rect(body: Rect, base: f32, below_config_row: bool) -> Rect {
    let y = if below_config_row {
        body.top() - (base - 22.0) - 8.0 - PREVIEW_H / 2.0
    } else {
        body.top() - 34.0 - 8.0 - PREVIEW_H / 2.0
    };
    Rect::from_x_y_w_h(body.x(), y, body.w() - 24.0, PREVIEW_H)
}

fn dropdown_option_rect(selector: Rect, row: usize) -> Rect {
    let y = selector.bottom() - DROPDOWN_ROW_H / 2.0 - row as f32 * DROPDOWN_ROW_H;
    Rect::from_x_y_w_h(selector.x(), y, selector.w(), DROPDOWN_ROW_H)
}

fn slider_value_at(rect: Rect, mouse_x: f32) -> f32 {
    ((mouse_x - rect.left()) / rect.w()).clamp(0.0, 1.0)
}

fn title_button_rect(body: Rect, slot: usize) -> Rect {
    Rect::from_w_h(22.0, 22.0)
        .top_right_of(body)
        .shift(vec2(-8.0 - slot as f32 * 26.0, -8.0))
}

fn module_output_port(body: Rect) -> Point2 {
    pt2(body.right(), body.y())
}

/// Input ports are spaced evenly down the left edge; a single input sits
/// dead center, matching every module that used to have exactly one.
fn module_input_port(kind: ModuleKind, body: Rect, index: usize, total: usize) -> Point2 {
    // Shader ports hang below the file selector, one labelled row each.
    if kind == ModuleKind::Shader {
        if body.h() <= MINIMIZED_H {
            return pt2(body.left(), body.y());
        }
        return pt2(body.left(), body.top() - SHADER_PORT_TOP - index as f32 * SHADER_PORT_STEP);
    }
    if total <= 1 {
        return pt2(body.left(), body.y());
    }
    let step = body.h() / (total as f32 + 1.0);
    pt2(body.left(), body.top() - step * (index as f32 + 1.0))
}

fn start_button_rect(win: Rect) -> Rect {
    Rect::from_w_h(220.0, 50.0)
        .mid_bottom_of(canvas_rect(win))
        .shift_y(40.0)
}

fn exit_button_rect(win: Rect) -> Rect {
    Rect::from_w_h(180.0, 44.0)
        .top_right_of(win)
        .shift(vec2(-110.0, -40.0))
}

// --- Drawing -----------------------------------------------------------

fn view(app: &App, model: &Model, render_frame: nannou::Frame) {
    let started = Instant::now();
    let draw = app.draw();
    let win = app.window_rect();

    match model.view {
        View::Config => {
            draw.background().color(DIMGRAY);

            let world = draw.translate(model.pan.extend(0.0));
            for module in &model.modules {
                draw_module(&world, model, module, app.time);
            }
            draw_cables(&world, app, model);

            draw_button(&draw, start_button_rect(win), "Start Fullscreen");
            draw_sidebar(&draw, win, model);
            if model.settings_open {
                draw_settings_panel(&draw, win, &model.settings, model.debug_open);
            }
            if model.settings.debug_log {
                draw_debug_log(&draw, win);
            }

            // Open dropdown renders last so its options sit on top of everything.
            for module in &model.modules {
                let open = match &module.body {
                    ModuleBody::Camera(cam) => cam.dropdown_open,
                    ModuleBody::Shader(shader) => shader.dropdown_open,
                    _ => false,
                };
                if open {
                    draw_dropdown_options(&world, module_body_rect(module), &dropdown_labels(model, module));
                }
            }
        }
        View::Fullscreen | View::FullscreenOverlay => {
            draw.background().color(BLACK);
            draw_output_signal(&draw, win, model);

            if model.view == View::FullscreenOverlay {
                draw_button(&draw, exit_button_rect(win), "Exit Fullscreen");
            }
        }
    }

    if model.settings.show_fps {
        let area = if model.view == View::Config { canvas_rect(win) } else { win };
        draw_fps(&draw, area, &model.fps.label);
    }

    draw.to_frame(app, &render_frame).unwrap();
    model.view_time.set(model.view_time.get() + started.elapsed());
}

fn draw_fps(draw: &Draw, area: Rect, label: &str) {
    let rect = Rect::from_w_h(150.0, 22.0).top_left_of(area).shift(vec2(8.0, -8.0));
    draw.rect().xy(rect.xy()).wh(rect.wh()).color(rgba(0.0, 0.0, 0.0, 0.6));
    draw.text(label)
        .xy(rect.xy() + vec2(6.0, 0.0))
        .wh(rect.wh())
        .left_justify()
        .color(LIMEGREEN)
        .font_size(12);
}

const DEBUG_LOG_LINES: usize = 14;
const DEBUG_LOG_LINE_H: f32 = 14.0;

/// The newest debug-log lines, in a translucent strip above the Start button.
fn draw_debug_log(draw: &Draw, win: Rect) {
    let canvas = canvas_rect(win);
    let h = DEBUG_LOG_LINES as f32 * DEBUG_LOG_LINE_H + 16.0;
    let panel = Rect::from_w_h(canvas.w() - 32.0, h).mid_bottom_of(canvas).shift_y(84.0);
    draw.rect().xy(panel.xy()).wh(panel.wh()).color(rgba(0.0, 0.0, 0.0, 0.75));
    for (i, line) in debug_log::recent(DEBUG_LOG_LINES).iter().enumerate() {
        let y = panel.top() - 8.0 - DEBUG_LOG_LINE_H / 2.0 - i as f32 * DEBUG_LOG_LINE_H;
        draw.text(line)
            .xy(pt2(panel.x(), y))
            .wh(vec2(panel.w() - 16.0, DEBUG_LOG_LINE_H))
            .left_justify()
            .no_line_wrap()
            .color(LIGHTGRAY)
            .font_size(11);
    }
}

fn draw_sidebar(draw: &Draw, win: Rect, model: &Model) {
    let sidebar = sidebar_rect(win);
    draw.rect()
        .xy(sidebar.xy())
        .wh(sidebar.wh())
        .color(rgb(0.13, 0.13, 0.13));

    let save_rect = save_button_rect(win);
    draw_button(draw, save_rect, "Save");

    let load_rect = load_button_rect(win);
    draw_button(draw, load_rect, "Load");
    draw_button(draw, settings_button_rect(win), if model.settings_open { "Settings \u{25BE}" } else { "Settings" });

    if let Some((message, is_error, _)) = &model.status {
        draw.text(message)
            .xy(pt2(sidebar.x(), save_rect.y() - 24.0))
            .wh(vec2(sidebar.w() - 16.0, 20.0))
            .color(if *is_error { CRIMSON } else { LIMEGREEN })
            .font_size(12);
    }

    for row in sidebar_rows(win) {
        match row {
            SidebarRow::Header { rect, label } => {
                draw.rect()
                    .xy(rect.xy())
                    .wh(rect.wh())
                    .color(rgb(0.22, 0.22, 0.22));
                draw.text(label).xy(rect.xy()).color(WHITE).font_size(16);
            }
            SidebarRow::Module {
                row_rect,
                plus_rect,
                label,
                ..
            } => {
                draw.text(label)
                    .xy(row_rect.xy() - vec2(14.0, 0.0))
                    .color(LIGHTGRAY)
                    .font_size(14);
                draw.rect()
                    .xy(plus_rect.xy())
                    .wh(plus_rect.wh())
                    .color(STEELBLUE);
                draw.text("+").xy(plus_rect.xy()).color(WHITE).font_size(16);
            }
        }
    }
}

fn draw_title_buttons(draw: &Draw, body: Rect, buttons: &[TitleButton]) {
    for (slot, button) in buttons.iter().enumerate() {
        let rect = title_button_rect(body, slot);
        let (color, glyph) = match button {
            TitleButton::Delete => (CRIMSON, "x"),
            TitleButton::Minimize => (STEELBLUE, "_"),
            TitleButton::Preview => (STEELBLUE, "P"),
        };
        draw.rect().xy(rect.xy()).wh(rect.wh()).color(color);
        draw.text(glyph).xy(rect.xy()).color(WHITE).font_size(13);
    }
}

fn draw_module(draw: &Draw, model: &Model, module: &PlacedModule, now: f32) {
    let chrome = module.body.chrome();
    let body = module_body_rect(module);

    draw.rect().xy(body.xy()).wh(body.wh()).color(body_color(module.kind));
    draw.rect()
        .xy(body.xy())
        .wh(body.wh())
        .no_fill()
        .stroke(BLACK)
        .stroke_weight(2.0);

    let title_y = if chrome.minimized { body.y() } else { body.top() - 18.0 };
    draw.text(kind_label(module.kind))
        .xy(pt2(body.x(), title_y))
        .color(WHITE)
        .font_size(16);

    draw_title_buttons(draw, body, title_buttons_for(module.kind));

    if !chrome.minimized {
        if has_config_row(module.kind) {
            let config = config_rect(body);
            match &module.body {
                ModuleBody::Camera(cam) => {
                    draw.rect().xy(config.xy()).wh(config.wh()).color(SNOW);
                    draw.text(&format!("{} \u{25BE}", cam.label(&model.available_cameras)))
                        .xy(config.xy())
                        .wh(config.wh())
                        .color(BLACK)
                        .font_size(12);
                }
                ModuleBody::Blend(blend) => {
                    let source = model
                        .connections
                        .iter()
                        .find(|c| c.to == module.id && c.to_input == 2)
                        .map(|c| c.from);
                    let mix = source
                        .and_then(|src| {
                            module_number(&model.modules, &model.connections, src, now, &mut vec![module.id])
                        })
                        .unwrap_or(blend.mix);
                    draw_slider(draw, config, mix, source.is_some());
                }
                ModuleBody::Shader(shader) => {
                    draw_value_box(
                        draw,
                        config,
                        &format!("{} \u{25BE}", shader.file.as_deref().unwrap_or("None")),
                    );
                }
                ModuleBody::Number(number) => {
                    draw_value_box(draw, config, &format!("{:.2}", number.value));
                }
                ModuleBody::Lfo(lfo) => {
                    let value = module_number(&model.modules, &model.connections, module.id, now, &mut Vec::new())
                        .unwrap_or(0.0);
                    draw_value_box(draw, config, &format!("{:.2} Hz = {:.2}", lfo.freq, value));
                }
                ModuleBody::Mult(_) => {
                    let value = module_number(&model.modules, &model.connections, module.id, now, &mut Vec::new());
                    let text = match value {
                        Some(v) => format!("= {:.2}", v),
                        None => "= \u{2014}".to_string(),
                    };
                    draw_value_box(draw, config, &text);
                }
                ModuleBody::Output(_) => {}
            }
        }

        if chrome.show_preview {
            let preview = preview_rect(body, module.base_height(), has_config_row(module.kind));
            draw_preview(draw, preview, chrome.preview_texture.as_ref());
        }
    }

    if let Some(port_type) = output_type(module.kind) {
        let connected = model.connections.iter().any(|c| c.from == module.id);
        draw_port(draw, module_output_port(body), port_type, connected);
    }

    let types = module.input_types();
    for (i, port_type) in types.iter().enumerate() {
        let connected = model.connections.iter().any(|c| c.to == module.id && c.to_input == i);
        draw_port(draw, module_input_port(module.kind, body, i, types.len()), *port_type, connected);
    }

    if let (ModuleBody::Shader(shader), false) = (&module.body, chrome.minimized) {
        for (i, label) in shader.port_labels().iter().enumerate() {
            let port = module_input_port(module.kind, body, i, types.len());
            draw.text(label)
                .xy(port + vec2(16.0 + 80.0, 0.0))
                .wh(vec2(160.0, 18.0))
                .left_justify()
                .color(WHITE)
                .font_size(11);
        }
        if let Some(error) = &shader.error {
            // Sits under the ports; the full text is also readable by
            // fixing the file, since the shader reloads on save.
            let ports_bottom = body.top() - SHADER_PORT_TOP - types.len() as f32 * SHADER_PORT_STEP;
            draw.text(error)
                .xy(pt2(body.x(), ports_bottom - 30.0))
                .wh(vec2(body.w() - 16.0, 56.0))
                .left_justify()
                .color(rgb(1.0, 0.7, 0.7))
                .font_size(10);
        }
    }
}

/// A port is always filled with its signal-type color (video = yellow,
/// number = black) so the type is visible regardless of connection state;
/// connected ports are solid, unconnected ones are hollow rings.
fn draw_port(draw: &Draw, pos: Point2, port_type: PortType, connected: bool) {
    let color = port_color(port_type);
    if connected {
        draw.ellipse().xy(pos).radius(PORT_RADIUS).color(color);
    } else {
        draw.ellipse().xy(pos).radius(PORT_RADIUS).color(WHITE);
        draw.ellipse()
            .xy(pos)
            .radius(PORT_RADIUS)
            .no_fill()
            .stroke(color)
            .stroke_weight(2.5);
    }
}

fn draw_settings_panel(draw: &Draw, win: Rect, settings: &Settings, debug_open: bool) {
    let panel = settings_panel_rect(win, debug_open);
    draw.rect().xy(panel.xy()).wh(panel.wh()).color(rgb(0.13, 0.13, 0.13));
    draw.rect().xy(panel.xy()).wh(panel.wh()).no_fill().stroke(BLACK).stroke_weight(2.0);
    draw.text("Settings").xy(pt2(panel.x(), panel.top() - 18.0)).color(WHITE).font_size(16);

    let range = Settings::SPEED_MAX - Settings::SPEED_MIN;
    let rows = [("Scroll speed", settings.scroll_speed), ("Drag speed", settings.drag_speed)];
    for (index, (label, value)) in rows.iter().enumerate() {
        let rect = settings_slider_rect(win, index);
        let t = (value - Settings::SPEED_MIN) / range;
        draw_labeled_slider(draw, rect, t, &format!("{label} {:.2}x", value), false);
    }

    for (row, toggle) in settings_toggles(debug_open).iter().enumerate() {
        let rect = settings_toggle_rect(win, row);
        let (label, on) = match toggle {
            SettingsToggle::ShowFps => ("Show FPS", settings.show_fps),
            SettingsToggle::DebugSection => {
                let label = if debug_open { "Debug \u{25BE}" } else { "Debug \u{25B8}" };
                draw.text(label).xy(rect.xy()).wh(rect.wh()).left_justify().color(GRAY).font_size(12);
                continue;
            }
            SettingsToggle::DebugLog => ("Debug log", settings.debug_log),
            SettingsToggle::SerializeGrabs => ("Serialize camera grabs", settings.serialize_camera_grabs),
        };
        draw_toggle(draw, rect, label, on);
    }
}

fn draw_toggle(draw: &Draw, rect: Rect, label: &str, on: bool) {
    draw_value_box(draw, rect, "");
    draw.text(label)
        .xy(rect.xy() + vec2(8.0, 0.0))
        .wh(rect.wh())
        .left_justify()
        .color(BLACK)
        .font_size(12);
    let check = Rect::from_w_h(16.0, 16.0).mid_right_of(rect).shift_x(-8.0);
    draw.rect()
        .xy(check.xy())
        .wh(check.wh())
        .color(if on { STEELBLUE } else { WHITE })
        .stroke(rgb(0.4, 0.4, 0.4))
        .stroke_weight(1.0);
}

/// `driven`: the value comes from a connected Number, so it's a readout
/// (drawn dimmer) rather than something to drag.
fn draw_slider(draw: &Draw, rect: Rect, mix: f32, driven: bool) {
    let label = format!("Mix {:.2}{}", mix, if driven { " (in)" } else { "" });
    draw_labeled_slider(draw, rect, mix, &label, driven);
}

fn draw_labeled_slider(draw: &Draw, rect: Rect, mix: f32, label: &str, driven: bool) {
    draw.rect().xy(rect.xy()).wh(rect.wh()).color(SNOW);
    let fill_w = (rect.w() * mix).max(2.0);
    let fill = Rect::from_x_y_w_h(rect.left() + fill_w / 2.0, rect.y(), fill_w, rect.h());
    draw.rect()
        .xy(fill.xy())
        .wh(fill.wh())
        .color(if driven { rgb8(150, 150, 150) } else { STEELBLUE });
    draw.rect()
        .xy(rect.xy())
        .wh(rect.wh())
        .no_fill()
        .stroke(rgb(0.4, 0.4, 0.4))
        .stroke_weight(1.0);
    draw.text(label)
        .xy(rect.xy())
        .color(BLACK)
        .font_size(12);
}

/// A plain value readout, used for Number's and LFO's drag-to-adjust
/// controls and Mult's live (read-only) result.
fn draw_value_box(draw: &Draw, rect: Rect, text: &str) {
    draw.rect().xy(rect.xy()).wh(rect.wh()).color(SNOW);
    draw.rect()
        .xy(rect.xy())
        .wh(rect.wh())
        .no_fill()
        .stroke(rgb(0.4, 0.4, 0.4))
        .stroke_weight(1.0);
    draw.text(text).xy(rect.xy()).color(BLACK).font_size(13);
}

/// Row labels for a module's open dropdown: "None" first, then each choice.
fn dropdown_labels(model: &Model, module: &PlacedModule) -> Vec<String> {
    let choices: Vec<String> = match &module.body {
        ModuleBody::Camera(_) => model
            .available_cameras
            .iter()
            .map(|c| c.human_name().to_string())
            .collect(),
        ModuleBody::Shader(_) => model.shader_library.clone(),
        _ => Vec::new(),
    };
    std::iter::once("None".to_string()).chain(choices).collect()
}

fn draw_dropdown_options(draw: &Draw, body: Rect, labels: &[String]) {
    let selector = config_rect(body);

    for (row, label) in labels.iter().enumerate() {
        let rect = dropdown_option_rect(selector, row);
        draw.rect().xy(rect.xy()).wh(rect.wh()).color(WHITE);
        draw.rect()
            .xy(rect.xy())
            .wh(rect.wh())
            .no_fill()
            .stroke(rgb(0.6, 0.6, 0.6))
            .stroke_weight(1.0);
        draw.text(label)
            .xy(rect.xy())
            .wh(rect.wh())
            .color(BLACK)
            .font_size(12);
    }
}

fn draw_preview(draw: &Draw, rect: Rect, texture: Option<&gpu::DrawTex>) {
    draw.rect().xy(rect.xy()).wh(rect.wh()).color(BLACK);
    match texture {
        Some(texture) => {
            let [img_w, img_h] = texture.view.size();
            let scale = (rect.w() / img_w as f32).min(rect.h() / img_h as f32);
            draw.texture(&texture.view)
                .xy(rect.xy())
                .w_h(img_w as f32 * scale, img_h as f32 * scale);
        }
        None => {
            draw.text("No Signal")
                .xy(rect.xy())
                .color(GRAY)
                .font_size(14);
        }
    }
    draw.rect()
        .xy(rect.xy())
        .wh(rect.wh())
        .no_fill()
        .stroke(rgb(0.4, 0.4, 0.4))
        .stroke_weight(1.0);
}

fn draw_cables(draw: &Draw, app: &App, model: &Model) {
    for c in &model.connections {
        let from = model.modules.iter().find(|m| m.id == c.from);
        let to = model.modules.iter().find(|m| m.id == c.to);
        if let (Some(from), Some(to)) = (from, to) {
            let from_body = module_body_rect(from);
            let to_body = module_body_rect(to);
            let to_inputs = to.input_types().len();
            if c.to_input >= to_inputs {
                continue;
            }
            let color = output_type(from.kind).map(port_color).unwrap_or(LIGHTGRAY);

            draw.line()
                .start(module_output_port(from_body))
                .end(module_input_port(to.kind, to_body, c.to_input, to_inputs))
                .weight(4.0)
                .color(color);
        }
    }

    if let Some(Drag::Cable { from }) = &model.dragging {
        if let Some(module) = model.modules.iter().find(|m| m.id == *from) {
            let body = module_body_rect(module);
            let color = output_type(module.kind).map(port_color).unwrap_or(LIGHTGRAY);
            draw.line()
                .start(module_output_port(body))
                .end(pt2(app.mouse.x, app.mouse.y) - model.pan)
                .weight(4.0)
                .color(color);
        }
    }
}

fn draw_button(draw: &Draw, rect: Rect, label: &str) {
    draw.rect().xy(rect.xy()).wh(rect.wh()).color(SNOW);
    draw.rect()
        .xy(rect.xy())
        .wh(rect.wh())
        .no_fill()
        .stroke(BLACK)
        .stroke_weight(2.0);
    draw.text(label).xy(rect.xy()).color(BLACK).font_size(18);
}

/// Draws whatever Output currently carries: its cached preview texture
/// (kept fresh unconditionally — see `refresh_one_texture`), or a "no
/// signal" placeholder if nothing's reached it yet.
fn draw_output_signal(draw: &Draw, win: Rect, model: &Model) {
    let texture = model
        .modules
        .iter()
        .find(|m| m.id == model.output_id)
        .and_then(|m| m.body.chrome().preview_texture.as_ref());

    match texture {
        Some(texture) => {
            let [img_w, img_h] = texture.view.size();
            let scale = (win.w() / img_w as f32).min(win.h() / img_h as f32);
            draw.texture(&texture.view)
                .w_h(img_w as f32 * scale, img_h as f32 * scale);
        }
        None => {
            draw.text("No Signal").color(GRAY).font_size(32);
        }
    }
}
