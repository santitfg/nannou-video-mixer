// Release builds on Windows open no console window next to the app (the debug
// log is still shown inside the app, from Settings).
#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod debug_log;
mod examples;
mod gpu;
mod graph;
mod group;
mod modules;
mod project;
mod save;
mod settings;
mod sync;
mod ui;

use modules::effects::shader_file::{scan_library, ShaderInfo, ShaderRole};
use modules::math::lag::LagMode;
use modules::math::lfo::Wave;
use modules::mix::blend::BlendMode;
use modules::param::Param;
use modules::sources::camera::{discover_cameras, CameraWatcher, CameraWorker};
use modules::sources::video::{fmt_time, LoopMode, VideoState, VideoWorker};
use modules::sources::wled::{short_name, Link, WledState};
use modules::{multi_input, wled_rows_top, Signal, SELECT_ROWS_TOP, SELECT_ROW_H, WLED_ROW_H, WLED_STATUS_LINE_H};
use std::net::SocketAddr;
use std::sync::mpsc;
use wled_pots::{DeviceDiscovery, DiscoveredDevice, MdnsDiscovery, WledManager};
use modules::{
    drop_mismatched, has_config_row, has_output, input_number, input_port, input_signal, input_source, kind_label,
    module_number, module_signal, output_type, select_input, signal_labels, signal_type,
    push_module as modules_push_module, reaches, title_buttons_for, Connection, ModuleBody,
    ModuleKind, PlacedModule, PortType, TitleButton, CATEGORIES, SHADER_PORT_STEP, SHADER_PORT_TOP,
};
use nannou::prelude::*;
use nannou::winit::window::CursorIcon;
use nokhwa::utils::CameraInfo;
use group::{Group, GroupButton};
use save::{load_composition, save_composition, save_composition_to, Preset, Snippet};
use settings::Settings;
use std::cell::Cell;
use std::time::{Duration, Instant};
use ui::{fmt_num, Palette, Theme, ValueBox};

const IDLE_TIMEOUT: Duration = Duration::from_millis(1500);
const SAVE_STATUS_TIMEOUT: Duration = Duration::from_millis(2500);
const ZOOM_INDICATOR_TIMEOUT: Duration = Duration::from_millis(1200);
pub const SAVE_PATH: &str = "composition.json";
/// The session autosaved on exit (and periodically) for "open last session".
pub const SESSION_PATH: &str = "last_session.json";
const AUTOSAVE_EVERY: Duration = Duration::from_secs(30);
const PORT_RADIUS: f32 = 9.0;
const PORT_HIT_RADIUS: f32 = 18.0;
const SIDEBAR_WIDTH: f32 = 220.0;
const SIDEBAR_HEADER_H: f32 = 64.0;
/// The Modules / Examples tab strip under the Save and Load buttons.
const SIDEBAR_TABS_H: f32 = 36.0;
const MODULE_W: f32 = 220.0;
const MODULE_RADIUS: f32 = 10.0;
/// An `update` slower than this is logged (Settings → Debug).
const SLOW_UPDATE: Duration = Duration::from_millis(50);
const MINIMIZED_H: f32 = 34.0;
const PREVIEW_H: f32 = 90.0;
/// A minimized module with its preview on: the title band plus a small thumbnail.
const MINI_PREVIEW_H: f32 = 45.0;
const MINIMIZED_PREVIEW_H: f32 = MINIMIZED_H + MINI_PREVIEW_H + 8.0;
const DROPDOWN_ROW_H: f32 = 26.0;
const PROP_BOX_H: f32 = 20.0;
/// Longer dropdowns scroll (mouse wheel) instead of running off screen.
const DROPDOWN_MAX_ROWS: usize = 10;
const SCROLL_LINE_PIXELS: f32 = 40.0;
const ZOOM_MIN: f32 = 0.25;
const ZOOM_MAX: f32 = 2.0;
const ZOOM_KEY_STEP: f32 = 1.2;
/// A press that moves less than this (screen px) before release is a click.
const CLICK_SLOP: f32 = 3.0;
/// Shift while dragging a number box: this much finer.
const FINE_DRAG: f32 = 0.1;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum View {
    Config,
    Fullscreen,
    /// Fullscreen with the cursor and Exit button showing (only when the
    /// `show_exit_button` setting is on).
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

/// Which number box of a module: its config-row box, or one of a Shader's
/// property boxes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Main,
    Prop(usize),
    /// One of a module's own fixed fields (Map's ranges and bias, WLED's Raw max).
    Field(usize),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct ParamTarget {
    module: u64,
    slot: Slot,
}

/// Which part of a number box is being typed into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditField {
    Value,
    Min,
    Max,
}

/// What a text edit types into.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum EditTarget {
    Param(ParamTarget, EditField),
    GroupTitle(u64),
    /// A project tab's name (index into `tabs`).
    TabName(usize),
    /// A saved group's name in the sidebar (index into `presets`).
    PresetName(usize),
    /// The WLED panel's "add by IP / host name" field.
    WledAddress,
}

struct TextEdit {
    target: EditTarget,
    buffer: String,
    /// Where typing lands, in characters from the start of `buffer`.
    cursor: usize,
    /// The other end of the selection (the caret is the first): the
    /// characters between them are replaced by whatever is typed.
    anchor: Option<usize>,
}

impl TextEdit {
    /// Starts editing `buffer` with the caret at its end; `select_all`
    /// selects it so the first keystroke replaces it.
    fn new(target: EditTarget, buffer: String, select_all: bool) -> Self {
        let cursor = buffer.chars().count();
        TextEdit { target, buffer, cursor, anchor: select_all.then_some(0) }
    }

    fn len(&self) -> usize {
        self.buffer.chars().count()
    }

    fn byte_at(&self, chars: usize) -> usize {
        self.buffer.char_indices().nth(chars).map_or(self.buffer.len(), |(i, _)| i)
    }

    /// The selected range, in characters, if any.
    fn selection(&self) -> Option<(usize, usize)> {
        self.anchor.filter(|a| *a != self.cursor).map(|a| (a.min(self.cursor), a.max(self.cursor)))
    }

    fn delete_selection(&mut self) -> bool {
        let Some((a, b)) = self.selection() else {
            self.anchor = None;
            return false;
        };
        let (from, to) = (self.byte_at(a), self.byte_at(b));
        self.buffer.replace_range(from..to, "");
        self.cursor = a;
        self.anchor = None;
        true
    }

    fn insert(&mut self, ch: char) {
        self.delete_selection();
        let at = self.byte_at(self.cursor);
        self.buffer.insert(at, ch);
        self.cursor += 1;
    }

    fn backspace(&mut self) {
        if !self.delete_selection() && self.cursor > 0 {
            let at = self.byte_at(self.cursor - 1);
            self.buffer.remove(at);
            self.cursor -= 1;
        }
    }

    fn delete(&mut self) {
        if !self.delete_selection() && self.cursor < self.len() {
            let at = self.byte_at(self.cursor);
            self.buffer.remove(at);
        }
    }

    /// Moves the caret; `extend` (Shift) grows the selection instead of dropping it.
    fn move_cursor(&mut self, to: usize, extend: bool) {
        if extend {
            self.anchor.get_or_insert(self.cursor);
        } else {
            self.anchor = None;
        }
        self.cursor = to.min(self.len());
    }

    /// Left/Right without Shift: a selection collapses to its near edge.
    fn step(&mut self, delta: isize, extend: bool) {
        match (self.selection(), extend) {
            (Some((a, b)), false) => self.move_cursor(if delta < 0 { a } else { b }, false),
            _ => self.move_cursor(self.cursor.saturating_add_signed(delta), extend),
        }
    }

    fn selected_text(&self) -> Option<String> {
        let (a, b) = self.selection()?;
        Some(self.buffer.chars().skip(a).take(b - a).collect())
    }

    fn select_all(&mut self) {
        self.anchor = Some(0);
        self.cursor = self.len();
    }

    /// A click: caret here, selection starts here (a drag extends it).
    fn click_at(&mut self, at: usize) {
        self.cursor = at.min(self.len());
        self.anchor = Some(self.cursor);
    }
}

#[derive(Clone, Copy)]
enum Drag {
    /// `start` is where the module sat when grabbed (to tell a click from a drag).
    Module { id: u64, offset: Vec2, start: Point2 },
    /// A cable leaving output `output` of module `from`.
    Cable { from: u64, output: usize },
    /// Drag-to-adjust on a number box. `last_x` is the previous world x;
    /// `moved` the total screen distance, to tell a click from a drag.
    Param { target: ParamTarget, last_x: f32, moved: f32 },
    /// Panning the canvas (middle button, or left + Shift); `last` is the
    /// previous screen-space mouse position.
    Pan { last: Vec2 },
    /// Dragging one of the settings sliders (0 = scroll speed, 1 = drag speed).
    Setting { index: usize },
    /// Rubber-band selection from `start` (world space); `additive` keeps
    /// what was already selected (Cmd/Ctrl held).
    Marquee { start: Point2, additive: bool },
    /// Moving a whole group by its title bar (or its collapsed box).
    Group { id: u64, last: Point2 },
    /// Scrubbing a video's position bar.
    Scrub { id: u64 },
    /// Dragging across the text being edited to select part of it.
    TextSelect,
}

enum SidebarRow {
    Header {
        rect: Rect,
        label: String,
        /// A heading inside a section (a shader category): smaller and dimmer.
        sub: bool,
    },
    Module {
        row_rect: Rect,
        plus_rect: Rect,
        label: String,
        action: SidebarAction,
    },
    /// Dim help text (e.g. how to fill an empty palette).
    Hint {
        rect: Rect,
        text: &'static str,
    },
}

#[derive(Clone)]
enum SidebarAction {
    Spawn(ModuleKind),
    /// Spawns the shader node of this role, its list limited to one category.
    ShaderCategory { category: String, role: ShaderRole },
    /// A saved group from the personal palette, by index into `presets`.
    Preset(usize),
    /// A bundled example, by index into `Model::examples`.
    Example(usize),
}

pub struct Model {
    view: View,
    last_mouse_move: Instant,
    mouse_down: bool,

    pub available_cameras: Vec<CameraInfo>,
    camera_watcher: CameraWatcher,
    pub blend_pipeline: gpu::BlendPipeline,
    pub yuyv_pipeline: gpu::YuyvPipeline,
    pub yuv420_pipeline: gpu::Yuv420Pipeline,
    /// File names found in the shaders folder (rescanned whenever a Shader's dropdown opens).
    pub shader_library: Vec<ShaderInfo>,
    pub next_id: u64,
    pub modules: Vec<PlacedModule>,
    pub connections: Vec<Connection>,
    pub(crate) dragging: Option<Drag>,
    pub output_id: u64,
    /// Canvas transform: a module at world position `p` is drawn at
    /// `p * zoom + pan`. The canvas has no bounds.
    pan: Vec2,
    zoom: f32,
    zoom_changed: Option<Instant>,
    settings: Settings,
    settings_open: bool,
    help_open: bool,
    /// The hidden Debug section of the settings panel is expanded.
    debug_open: bool,
    palette_open: bool,
    /// The number box whose options popover is open.
    param_menu: Option<ParamTarget>,
    editing: Option<TextEdit>,
    /// Selected modules, in selection order.
    selected: Vec<u64>,
    pub(crate) groups: Vec<Group>,
    next_group_id: u64,
    clipboard: Option<Snippet>,
    /// The personal palette: saved groups, shown in the sidebar.
    presets: Vec<Preset>,
    /// Open projects as tabs; the active one's data is in the fields above.
    tabs: Vec<project::ProjectTab>,
    active_tab: usize,
    sidebar_scroll: f32,
    /// The sidebar shows the bundled examples instead of the module list.
    examples_tab: bool,
    /// Bundled and read-only: adding one to the canvas copies it.
    examples: Vec<examples::Example>,
    /// First visible row of the open module dropdown (fractional while scrolling).
    dropdown_scroll: f32,
    /// Right-click menu on the canvas, at this screen position.
    canvas_menu: Option<Point2>,
    /// Right-click menu on a saved group (sidebar) or a group frame (canvas).
    item_menu: Option<ItemMenu>,
    /// The Stack whose layer order is being edited, and where its menu sits.
    stack_menu: Option<StackMenu>,
    /// WLED controllers: discovery + one poller thread each.
    wled: WledManager,
    wled_open: bool,
    /// Devices found by a background scan or host-name lookup.
    wled_found: (mpsc::Sender<Vec<DiscoveredDevice>>, mpsc::Receiver<Vec<DiscoveredDevice>>),
    /// File dialogs run on a thread of their own; answers come back here.
    dialog: (mpsc::Sender<Option<DialogReply>>, mpsc::Receiver<Option<DialogReply>>),
    dialog_open: bool,
    last_autosave: Instant,
    /// The project file Save writes to in project mode (the last one saved or loaded).
    project: Option<std::path::PathBuf>,
    wled_scanning: bool,
    /// When and where the last left click landed, for double-clicks.
    last_click: Option<(Instant, Point2)>,
    cursor: CursorIcon,
    status: Option<(String, bool, Instant)>,
    fps: FpsCounter,
    pub(crate) timings: Timings,
    /// Time spent in `view` since the last `update` (`view` only gets `&Model`).
    view_time: Cell<Duration>,
    /// Start-in-fullscreen is applied on the first update tick.
    fullscreen_pending: bool,
}

impl Model {
    /// Frames `members` in a new group (taking them out of any other one).
    pub fn add_group(&mut self, title: String, members: Vec<u64>, collapsed: bool) -> u64 {
        for g in &mut self.groups {
            g.members.retain(|id| !members.contains(id));
        }
        self.groups.retain(|g| !g.members.is_empty());
        let id = self.next_group_id;
        self.next_group_id += 1;
        if !members.is_empty() {
            self.groups.push(Group { id, title, members, collapsed });
        }
        id
    }

    fn group_of(&self, module: u64) -> Option<&Group> {
        self.groups.iter().find(|g| g.members.contains(&module))
    }

    /// Hidden inside a collapsed group.
    fn hidden(&self, module: u64) -> bool {
        self.group_of(module).is_some_and(|g| g.collapsed)
    }

    fn group_frame(&self, group: &Group) -> Option<Rect> {
        let bodies: Vec<Rect> = self
            .modules
            .iter()
            .filter(|m| group.members.contains(&m.id))
            .map(module_body_rect)
            .collect();
        group::frame_rect(group.collapsed, &bodies)
    }

    fn notify(&mut self, message: impl Into<String>) {
        self.status = Some((message.into(), false, Instant::now()));
    }
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

/// The rows under the settings sliders.
#[derive(Clone, Copy, PartialEq, Eq)]
enum SettingsRow {
    ShowFps,
    ExitButton,
    ProjectMode,
    RestoreSession,
    StartFullscreen,
    Palette,
    /// Expands/collapses the rows below; not a setting itself.
    DebugSection,
    DebugLog,
    SerializeGrabs,
}

fn settings_rows(debug_open: bool) -> &'static [SettingsRow] {
    use SettingsRow::*;
    if debug_open {
        &[ShowFps, ExitButton, ProjectMode, RestoreSession, StartFullscreen, Palette, DebugSection, DebugLog, SerializeGrabs]
    } else {
        &[ShowFps, ExitButton, ProjectMode, RestoreSession, StartFullscreen, Palette, DebugSection]
    }
}

fn main() {
    nannou::app(model).update(update).event(event).exit(exit).run();
}

fn model(app: &App) -> Model {
    app.new_window()
        .size(1280, 800)
        .title("Video Mixer")
        .view(view)
        .raw_event(raw_window_event)
        .build()
        .unwrap();
    // Esc leaves fullscreen / closes popups; it must never quit the app.
    app.set_exit_on_escape(false);

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
        yuv420_pipeline: gpu::Yuv420Pipeline::new(app.main_window().device()),
        shader_library: scan_library(),
        next_id: 0,
        modules: Vec::new(),
        connections: Vec::new(),
        dragging: None,
        output_id: 0,
        pan: Vec2::ZERO,
        zoom: 1.0,
        zoom_changed: None,
        settings: settings.clone(),
        settings_open: false,
        help_open: false,
        debug_open: false,
        palette_open: false,
        param_menu: None,
        editing: None,
        selected: Vec::new(),
        groups: Vec::new(),
        next_group_id: 0,
        clipboard: None,
        presets: save::load_presets(),
        tabs: vec![project::ProjectTab::new("Untitled")],
        active_tab: 0,
        sidebar_scroll: 0.0,
        examples_tab: false,
        examples: examples::all(),
        dropdown_scroll: 0.0,
        canvas_menu: None,
        item_menu: None,
        stack_menu: None,
        wled: start_wled(&settings),
        wled_open: false,
        wled_found: mpsc::channel(),
        dialog: mpsc::channel(),
        dialog_open: false,
        last_autosave: Instant::now(),
        project: None,
        wled_scanning: false,
        last_click: None,
        cursor: CursorIcon::Default,
        status: None,
        fps: FpsCounter::new(),
        timings: Timings::new(),
        view_time: Cell::new(Duration::ZERO),
        fullscreen_pending: false,
    };
    model.output_id = push_module(&mut model, ModuleKind::Output, canvas.xy());
    if model.settings.restore_session && std::path::Path::new(SESSION_PATH).exists() {
        match load_composition(&mut model, app.window_rect(), std::path::Path::new(SESSION_PATH)) {
            Ok(()) => model.notify("Reopened the last session"),
            Err(err) => model.status = Some((format!("Couldn't reopen the last session: {err}"), true, Instant::now())),
        }
    }
    // Asking for fullscreen before the event loop runs leaves the window at
    // its default size, so it's done on the first update instead.
    model.fullscreen_pending = model.settings.start_fullscreen;
    model
}

/// Keeps the session for the next start.
fn exit(_app: &App, model: Model) {
    if model.settings.restore_session {
        let _ = save_composition_to(&model, std::path::Path::new(SESSION_PATH));
    }
}

fn camera_names(cameras: &[CameraInfo]) -> String {
    cameras.iter().map(|c| c.human_name()).collect::<Vec<_>>().join(", ")
}

fn update(app: &App, model: &mut Model, update: Update) {
    let started = Instant::now();
    model.fps.tick();

    if model.fullscreen_pending {
        model.fullscreen_pending = false;
        enter_fullscreen(app, model);
    }

    if model.view == View::FullscreenOverlay
        && !model.mouse_down
        && model.last_mouse_move.elapsed() > IDLE_TIMEOUT
    {
        model.view = View::Fullscreen;
        app.main_window().set_cursor_visible(false);
    }

    if let Some((_, _, at)) = &model.status {
        if at.elapsed() > SAVE_STATUS_TIMEOUT {
            model.status = None;
        }
    }

    if model.view == View::Config {
        let wanted = cursor_for(app, model);
        if wanted != model.cursor {
            model.cursor = wanted;
            app.main_window().set_cursor_icon(wanted);
        }
    }

    // A shader node whose list holds one shader has nothing to pick.
    for module in &mut model.modules {
        if let (ModuleBody::Shader(sh), Some(role)) = (&mut module.body, module.kind.shader_role()) {
            sh.single_choice = sh.category.as_deref().is_some_and(|c| {
                model.shader_library.iter().filter(|i| i.category == c && role.lists(i.role)).count() == 1
            });
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

    // A capture thread that died (device yanked mid-stream) or stopped
    // delivering frames (a hung driver call) is dropped; the module keeps its
    // device and retries after a growing cooldown.
    for module in &mut model.modules {
        if let ModuleBody::Camera(cam) = &mut module.body {
            if let Some(w) = &cam.worker {
                if w.has_frames() {
                    cam.failures = 0;
                }
                let dead = !w.is_alive();
                if dead || w.stalled() {
                    dlog!("camera module {}: capture {}, restarting", module.id, if dead { "ended" } else { "stalled" });
                    cam.worker = None;
                    cam.chrome.preview_texture = None;
                    cam.failures = (cam.failures + 1).min(5);
                    cam.retry_at = Some(Instant::now() + Duration::from_secs(1 << (cam.failures - 1)));
                }
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
            if needed && cam.worker.is_none() && cam.retry_at.map_or(true, |t| Instant::now() >= t) {
                if let Some(info) = cam.selected.and_then(|i| model.available_cameras.get(i)) {
                    cam.worker = Some(CameraWorker::spawn(info.index().clone()));
                }
            } else if !needed && cam.worker.is_some() {
                cam.worker = None;
            }
        }
        // Videos too: an unused one frees its decoder and frame cache.
        if let ModuleBody::Video(video) = &mut module.body {
            let needed = video.chrome.show_preview || needed_ids.contains(&module.id);
            // A decoder that crashed (without an error to show) is replaced
            // after a short pause.
            if video.worker.as_ref().is_some_and(|w| !w.is_alive() && w.error().is_none()) {
                dlog!("video module {}: decoder ended, restarting", module.id);
                video.worker = None;
                video.retry_at = Some(Instant::now() + Duration::from_secs(2));
            }
            if needed && video.worker.is_none() && video.retry_at.map_or(true, |t| Instant::now() >= t) {
                if let Some(path) = video.path.clone() {
                    video.worker = Some(VideoWorker::spawn(path));
                }
            } else if !needed && video.worker.is_some() {
                video.worker = None;
            }
        }
    }

    poll_dialogs(app, model);
    if model.settings.restore_session && model.last_autosave.elapsed() > AUTOSAVE_EVERY {
        model.last_autosave = Instant::now();
        let _ = save_composition_to(model, std::path::Path::new(SESSION_PATH));
    }
    step_lags(app, model, update.since_last.as_secs_f32());
    step_maps(app, model);
    step_selects(app, model);
    step_videos(app, model, update.since_last.as_secs_f64());
    step_wled(model);
    step_osc(model);
    graph::run_graph(app, model, update.since_last.as_secs_f32());

    let timings = &mut model.timings;
    timings.frames += 1;
    timings.update += started.elapsed();
    timings.view += model.view_time.take();
    timings.report_every_second();
    if started.elapsed() > SLOW_UPDATE {
        dlog!("slow update: {:.0}ms", started.elapsed().as_secs_f32() * 1000.0);
    }
}

fn start_wled(settings: &Settings) -> WledManager {
    let manager = WledManager::new(Box::new(MdnsDiscovery), Duration::from_millis(settings.wled_poll_ms.max(10)));
    for device in &settings.wled_devices {
        if let Ok(addr) = device.addr.parse::<SocketAddr>() {
            manager.add_discovered(DiscoveredDevice { name: device.name.clone(), addr, mac: None });
        }
    }
    manager
}

/// Tracked WLED devices, by name, with their address.
fn wled_devices(model: &Model) -> Vec<(SocketAddr, String)> {
    let mut list: Vec<(SocketAddr, String)> = model
        .wled
        .devices()
        .into_iter()
        .map(|addr| (addr, model.wled.device_name(addr).map_or_else(|| addr.to_string(), |n| short_name(&n).to_string())))
        .collect();
    list.sort_by(|a, b| a.1.cmp(&b.1).then(a.0.cmp(&b.0)));
    list
}

/// Picks up scan/lookup results, refreshes every WLED module from its
/// device's latest reading, and remembers the device list.
fn step_wled(model: &mut Model) {
    while let Ok(found) = model.wled_found.1.try_recv() {
        model.wled_scanning = false;
        let n = found.len();
        for device in found {
            dlog!("WLED: tracking {} at {}", device.name, device.addr);
            model.wled.add_discovered(device);
        }
        model.notify(if n == 0 { "No WLED devices found".to_string() } else { format!("Found {n} WLED device{}", if n == 1 { "" } else { "s" }) });
    }
    let poll = model.wled.poll_interval();
    for m in &mut model.modules {
        if let ModuleBody::Wled(w) = &mut m.body {
            let reading = w.device.and_then(|addr| model.wled.reading(addr));
            w.refresh(reading.as_ref(), poll);
        }
    }
    let devices: Vec<settings::WledDevice> =
        wled_devices(model).into_iter().map(|(addr, name)| settings::WledDevice { addr: addr.to_string(), name }).collect();
    if devices != model.settings.wled_devices {
        model.settings.wled_devices = devices;
        model.settings.save();
    }
}

/// Looks for WLED devices over mDNS on a background thread.
fn scan_wled(model: &mut Model) {
    if model.wled_scanning {
        return;
    }
    model.wled_scanning = true;
    let tx = model.wled_found.0.clone();
    std::thread::spawn(move || {
        let found = MdnsDiscovery.discover(Duration::from_secs(3));
        let _ = tx.send(found);
    });
}

/// Adds a device typed into the panel: "192.168.1.50", "…:8080" or a
/// host name (resolved on a background thread; it can be slow).
fn add_wled_address(model: &mut Model, text: &str) {
    if text.is_empty() {
        return;
    }
    let direct = text.parse::<SocketAddr>().ok().or_else(|| text.parse::<std::net::IpAddr>().ok().map(|ip| SocketAddr::new(ip, 80)));
    if let Some(addr) = direct {
        model.wled.add_manual(addr);
        return;
    }
    let tx = model.wled_found.0.clone();
    let host = text.to_string();
    model.wled_scanning = true;
    std::thread::spawn(move || {
        use std::net::ToSocketAddrs;
        let target = if host.contains(':') { host.clone() } else { format!("{host}:80") };
        let found = target
            .to_socket_addrs()
            .ok()
            .and_then(|mut addrs| addrs.find(|a| a.is_ipv4()))
            .map(|addr| vec![DiscoveredDevice { name: host, addr, mac: None }])
            .unwrap_or_default();
        let _ = tx.send(found);
    });
}

/// Reads the sockets of the OSC modules.
fn step_osc(model: &mut Model) {
    for m in &mut model.modules {
        if let ModuleBody::Osc(osc) = &mut m.body {
            osc.poll();
        }
    }
}

/// Moves every video's playhead (speed and scrub can come from cables).
fn step_videos(app: &App, model: &mut Model, dt: f64) {
    let inputs: Vec<(usize, f32, Option<f32>)> = model
        .modules
        .iter()
        .enumerate()
        .filter_map(|(i, m)| {
            let ModuleBody::Video(video) = &m.body else { return None };
            let signal = |input: usize| input_number(&model.modules, &model.connections, m.id, input, app.time, &mut vec![m.id]);
            Some((i, video.speed.resolve(signal(0)), signal(1)))
        })
        .collect();
    for (i, speed, scrub) in inputs {
        if let ModuleBody::Video(video) = &mut model.modules[i].body {
            video.advance(dt, speed, scrub);
        }
    }
}

/// Lags have state: each one moves towards its input once per tick.
fn step_lags(app: &App, model: &mut Model, dt: f32) {
    let targets: Vec<(usize, Option<Signal>)> = model
        .modules
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.body, ModuleBody::Lag(_)))
        .map(|(i, m)| (i, input_signal(&model.modules, &model.connections, m.id, 0, app.time, &mut vec![m.id])))
        .collect();
    for (i, target) in targets {
        if let ModuleBody::Lag(lag) = &mut model.modules[i].body {
            lag.step(target, dt);
        }
    }
}

/// Maps in autorange mode learn their From range from their input.
fn step_maps(app: &App, model: &mut Model) {
    let inputs: Vec<(usize, Option<Signal>)> = model
        .modules
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(&m.body, ModuleBody::Map(map) if map.auto))
        .map(|(i, m)| (i, input_signal(&model.modules, &model.connections, m.id, 0, app.time, &mut vec![m.id])))
        .collect();
    for (i, input) in inputs {
        if let ModuleBody::Map(map) = &mut model.modules[i].body {
            map.learn(input.as_ref().map_or(&[], |s| s.values()));
        }
    }
}

/// Selects keep a copy of their input (and what each element is called)
/// for drawing their rows; one loaded from an old composition picks its
/// GPIO here once the WLED reports it.
fn step_selects(app: &App, model: &mut Model) {
    let fresh: Vec<(usize, Vec<f32>, Vec<String>)> = model
        .modules
        .iter()
        .enumerate()
        .filter(|(_, m)| matches!(m.body, ModuleBody::Select(_)))
        .map(|(i, m)| {
            let (modules, connections) = (&model.modules, &model.connections);
            let values = select_input(modules, connections, m.id, app.time, &mut vec![m.id]);
            let labels = multi_input::source_ports(connections, m.id, 0)
                .into_iter()
                .flat_map(|(from, out)| {
                    let len = module_signal(modules, connections, from, out, app.time, &mut vec![m.id]).map_or(0, |s| s.values().len());
                    signal_labels(modules, connections, from, out, len, &mut vec![m.id])
                })
                .collect();
            (i, values, labels)
        })
        .collect();
    for (i, values, labels) in fresh {
        let id = model.modules[i].id;
        let ModuleBody::Select(select) = &mut model.modules[i].body else { continue };
        select.values = values;
        select.labels = labels;
        let Some(gpio) = select.pending_gpio else { continue };
        let prefix = format!("{gpio} \u{00B7}");
        if let Some(e) = select.labels.iter().position(|l| l.starts_with(&prefix)) {
            select.pending_gpio = None;
            select.selected = vec![false; e + 1];
            select.selected[e] = true;
            for c in model.connections.iter_mut().filter(|c| c.from == id) {
                c.from_output = e;
            }
        }
    }
}

/// A number or an array, for readouts: an array shows its length and first values.
fn fmt_signal(signal: &Signal) -> String {
    match signal {
        Signal::One(x) => fmt_num(*x),
        Signal::Many(v) => {
            let first: Vec<String> = v.iter().take(2).map(|x| fmt_num(*x)).collect();
            format!("[{}] {}{}", v.len(), first.join(" "), if v.len() > 2 { "\u{2026}" } else { "" })
        }
    }
}

/// Trackpad pinch, which nannou's simplified events don't carry.
fn raw_window_event(app: &App, model: &mut Model, event: &nannou::winit::event::WindowEvent) {
    if let nannou::winit::event::WindowEvent::TouchpadMagnify { delta, .. } = event {
        if model.view == View::Config {
            zoom_at(model, pt2(app.mouse.x, app.mouse.y), 1.0 + *delta as f32);
        }
    }
}

fn event(app: &App, model: &mut Model, event: Event) {
    let Event::WindowEvent { simple: Some(window_event), .. } = event else { return };
    match window_event {
        WindowEvent::MouseMoved(screen_pos) => mouse_moved(app, model, screen_pos),
        WindowEvent::MousePressed(button) => {
            model.mouse_down = true;
            if model.view != View::Config {
                // Any click leaves fullscreen.
                exit_fullscreen(app, model);
                return;
            }
            let screen = pt2(app.mouse.x, app.mouse.y);
            let win = app.window_rect();
            let over_ui = sidebar_rect(win).contains(screen)
                || over_panel(model, win, screen);
            let can_pan = model.dragging.is_none() && !over_ui && model.param_menu.is_none();
            match button {
                MouseButton::Middle if can_pan => model.dragging = Some(Drag::Pan { last: screen }),
                MouseButton::Left if can_pan && app.keys.mods.shift() && over_empty_canvas(model, win, screen) => {
                    commit_edit(model);
                    model.dragging = Some(Drag::Pan { last: screen });
                }
                MouseButton::Left => handle_press(app, model, win, screen),
                MouseButton::Right => handle_right_press(model, win, screen),
                _ => {}
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
        WindowEvent::ReceivedCharacter(ch) => {
            if let Some(edit) = &mut model.editing {
                if let Some(ch) = accepted_char(edit.target, ch) {
                    edit.insert(ch);
                }
            }
        }
        WindowEvent::KeyPressed(key) => key_pressed(app, model, key),
        WindowEvent::DroppedFile(path) if model.view == View::Config && is_video_file(&path) => {
            // A video dropped on the window becomes a Video module there.
            let pos = to_world(model, pt2(app.mouse.x, app.mouse.y));
            let id = push_module(model, ModuleKind::Video, pos);
            if let Some(ModuleBody::Video(video)) = model.modules.iter_mut().find(|m| m.id == id).map(|m| &mut m.body) {
                video.open(path);
            }
            model.selected = vec![id];
        }
        WindowEvent::MouseWheel(delta, _) => {
            if model.view != View::Config {
                return;
            }
            let is_wheel = matches!(delta, MouseScrollDelta::LineDelta(..));
            let mut d = match delta {
                MouseScrollDelta::LineDelta(x, y) => vec2(x, y) * SCROLL_LINE_PIXELS,
                MouseScrollDelta::PixelDelta(p) => vec2(p.x as f32, p.y as f32),
            };
            let mods = app.keys.mods;
            let screen = pt2(app.mouse.x, app.mouse.y);
            let win = app.window_rect();
            if sidebar_rect(win).contains(screen) {
                scroll_sidebar(model, win, -d.y);
                return;
            }
            // An open list scrolls itself; with any popup open the canvas stays put.
            if let Some(i) = open_dropdown(model) {
                let n = dropdown_labels(model, &model.modules[i]).len();
                let max = n.saturating_sub(DROPDOWN_MAX_ROWS) as f32;
                model.dropdown_scroll = (model.dropdown_scroll - d.y / DROPDOWN_ROW_H).clamp(0.0, max);
                return;
            }
            if popup_open(model) {
                return;
            }
            // The mouse wheel zooms around the cursor (Shift: scroll sideways
            // instead); a trackpad scrolls, and zooms with Cmd/Ctrl held.
            if (is_wheel && !mods.shift()) || mods.logo() || mods.ctrl() {
                zoom_at(model, screen, (d.y * 0.005).exp());
                return;
            }
            if mods.shift() && d.x == 0.0 {
                d = vec2(d.y, 0.0);
            }
            model.pan += d * model.settings.scroll_speed;
        }
        _ => {}
    }
}

fn mouse_moved(app: &App, model: &mut Model, screen_pos: Point2) {
    model.last_mouse_move = Instant::now();
    match model.view {
        View::Fullscreen if model.settings.show_exit_button => {
            model.view = View::FullscreenOverlay;
            app.main_window().set_cursor_visible(true);
            return;
        }
        View::Fullscreen | View::FullscreenOverlay => return,
        View::Config => {}
    }
    let pos = to_world(model, screen_pos);
    match model.dragging {
        Some(Drag::Setting { index }) => {
            let t = slider_value_at(settings_slider_rect(app.window_rect(), index), screen_pos.x);
            set_setting(&mut model.settings, index, t);
        }
        Some(Drag::TextSelect) => {
            if let Some(at) = caret_for_pointer(model, app.window_rect(), screen_pos, false) {
                if let Some(edit) = model.editing.as_mut() {
                    edit.cursor = at.min(edit.len());
                }
            }
        }
        Some(Drag::Pan { last }) => {
            model.pan += (screen_pos - last) * model.settings.drag_speed;
            model.dragging = Some(Drag::Pan { last: screen_pos });
        }
        Some(Drag::Module { id, offset, .. }) => {
            // The grabbed module leads; the rest of the selection follows.
            let Some(lead) = model.modules.iter().find(|m| m.id == id) else { return };
            let delta = pos + offset - lead.pos;
            let selected = model.selected.clone();
            for m in model.modules.iter_mut().filter(|m| m.id == id || selected.contains(&m.id)) {
                m.pos += delta;
            }
        }
        Some(Drag::Scrub { id }) => {
            if let Some(m) = model.modules.iter_mut().find(|m| m.id == id) {
                let bar = video_scrub_rect(module_body_rect(m));
                if let ModuleBody::Video(video) = &mut m.body {
                    video.seek_fraction(slider_value_at(bar, pos.x));
                }
            }
        }
        Some(Drag::Group { id, last }) => {
            let delta = pos - last;
            if let Some(members) = model.groups.iter().find(|g| g.id == id).map(|g| g.members.clone()) {
                for m in model.modules.iter_mut().filter(|m| members.contains(&m.id)) {
                    m.pos += delta;
                }
            }
            model.dragging = Some(Drag::Group { id, last: pos });
        }
        Some(Drag::Param { target, last_x, moved }) => {
            let dx = pos.x - last_x;
            let moved = moved + dx.abs() * model.zoom;
            // Small jitters before a click registers as a drag are ignored,
            // so clicking to type doesn't nudge the value first.
            if moved >= CLICK_SLOP {
                let width = param_rect_of(model, target).map_or(MODULE_W, |r| r.w());
                let fine = if app.keys.mods.shift() { FINE_DRAG } else { 1.0 };
                edit_param(model, target, |p| p.nudge(dx / width * fine));
            }
            model.dragging = Some(Drag::Param { target, last_x: pos.x, moved });
        }
        _ => {}
    }
}

/// The character a text field of this kind takes for `ch`, if any.
fn accepted_char(target: EditTarget, ch: char) -> Option<char> {
    match target {
        EditTarget::Param(..) => {
            let ch = if ch == ',' { '.' } else { ch };
            (ch.is_ascii_digit() || ch == '.' || ch == '-').then_some(ch)
        }
        EditTarget::GroupTitle(_) | EditTarget::PresetName(_) | EditTarget::TabName(_) => (!ch.is_control()).then_some(ch),
        EditTarget::WledAddress => (ch.is_ascii_alphanumeric() || ".:-_".contains(ch)).then_some(ch),
    }
}

fn clipboard_text() -> Option<String> {
    arboard::Clipboard::new().ok()?.get_text().ok()
}

fn set_clipboard_text(text: String) {
    if let Ok(mut clipboard) = arboard::Clipboard::new() {
        let _ = clipboard.set_text(text);
    }
}

fn key_pressed(app: &App, model: &mut Model, key: Key) {
    // Typing into a box swallows every key.
    if model.editing.is_some() {
        match key {
            Key::Return | Key::NumpadEnter | Key::Tab => commit_edit(model),
            Key::Escape => model.editing = None,
            Key::Back => model.editing.as_mut().unwrap().backspace(),
            Key::Delete => model.editing.as_mut().unwrap().delete(),
            Key::Left => model.editing.as_mut().unwrap().step(-1, app.keys.mods.shift()),
            Key::Right => model.editing.as_mut().unwrap().step(1, app.keys.mods.shift()),
            Key::Home => model.editing.as_mut().unwrap().move_cursor(0, app.keys.mods.shift()),
            Key::End => model.editing.as_mut().unwrap().move_cursor(usize::MAX, app.keys.mods.shift()),
            Key::A if app.keys.mods.logo() || app.keys.mods.ctrl() => model.editing.as_mut().unwrap().select_all(),
            Key::C | Key::X if app.keys.mods.logo() || app.keys.mods.ctrl() => {
                let edit = model.editing.as_mut().unwrap();
                if let Some(text) = edit.selected_text() {
                    set_clipboard_text(text);
                    if key == Key::X {
                        edit.delete_selection();
                    }
                }
            }
            Key::V if app.keys.mods.logo() || app.keys.mods.ctrl() => {
                if let (Some(text), Some(edit)) = (clipboard_text(), model.editing.as_mut()) {
                    let target = edit.target;
                    for ch in text.chars().filter_map(|c| accepted_char(target, c)) {
                        edit.insert(ch);
                    }
                }
            }
            _ => {}
        }
        return;
    }

    if model.view != View::Config {
        if matches!(key, Key::Escape | Key::Space | Key::Return | Key::NumpadEnter | Key::F) {
            exit_fullscreen(app, model);
        }
        return;
    }

    let win = app.window_rect();
    let canvas_center = canvas_rect(win).xy();
    let mods = app.keys.mods;
    if mods.logo() || mods.ctrl() {
        match key {
            Key::C => copy_selection(model),
            Key::X => {
                copy_selection(model);
                let ids = model.selected.clone();
                delete_modules(model, &ids);
            }
            Key::V => paste_clipboard(app, model),
            Key::D => duplicate_selection(model),
            Key::A => model.selected = model.modules.iter().filter(|m| !model.hidden(m.id)).map(|m| m.id).collect(),
            Key::G if mods.shift() => ungroup_selection(model),
            Key::G => group_selection(model),
            _ => {}
        }
        return;
    }
    match key {
        Key::F => enter_fullscreen(app, model),
        Key::D => duplicate_selection(model),
        Key::L => arrange(model, Arrangement::Auto),
        Key::P => toggle_previews(model),
        Key::M => toggle_minimized(model),
        Key::Delete | Key::Back => {
            let ids = model.selected.clone();
            delete_modules(model, &ids);
        }
        Key::H => {
            model.zoom = 1.0;
            center_on_modules(model, win);
        }
        Key::Key0 | Key::Numpad0 => zoom_at(model, canvas_center, 1.0 / model.zoom),
        Key::Equals | Key::Plus | Key::NumpadAdd => zoom_at(model, canvas_center, ZOOM_KEY_STEP),
        Key::Minus | Key::NumpadSubtract => zoom_at(model, canvas_center, 1.0 / ZOOM_KEY_STEP),
        Key::F1 => model.help_open = !model.help_open,
        Key::Escape => {
            if model.help_open {
                model.help_open = false;
            } else if popup_open(model) {
                close_popups(model);
            } else {
                model.selected.clear();
            }
        }
        _ => {}
    }
}

fn close_popups(model: &mut Model) {
    model.param_menu = None;
    model.palette_open = false;
    model.canvas_menu = None;
    model.item_menu = None;
    model.stack_menu = None;
    model.dropdown_scroll = 0.0;
    for module in &mut model.modules {
        match &mut module.body {
            ModuleBody::Camera(cam) => cam.dropdown_open = false,
            ModuleBody::Shader(shader) => {
                shader.dropdown_open = false;
                shader.prop_dropdown = None;
            }
            ModuleBody::Lfo(lfo) => lfo.dropdown_open = false,
            ModuleBody::Blend(blend) => blend.dropdown_open = false,
            ModuleBody::Lag(lag) => lag.dropdown_open = false,
            ModuleBody::Video(video) => video.dropdown_open = false,
            ModuleBody::Wled(w) => w.dropdown_open = false,
            _ => {}
        }
    }
}

fn enter_fullscreen(app: &App, model: &mut Model) {
    commit_edit(model);
    close_popups(model);
    model.dragging = None;
    let window = app.main_window();
    window.set_fullscreen(true);
    window.set_cursor_visible(false);
    model.view = View::Fullscreen;
    model.last_mouse_move = Instant::now();
}

fn exit_fullscreen(app: &App, model: &mut Model) {
    let window = app.main_window();
    window.set_fullscreen(false);
    window.set_cursor_visible(true);
    model.view = View::Config;
    model.dragging = None;
}

// --- Canvas transform ----------------------------------------------------------

fn to_world(model: &Model, screen: Point2) -> Point2 {
    (screen - model.pan) / model.zoom
}

fn to_screen_rect(model: &Model, world: Rect) -> Rect {
    Rect::from_xy_wh(world.xy() * model.zoom + model.pan, world.wh() * model.zoom)
}

/// Zooms by `factor`, keeping whatever is under `screen` in place.
fn zoom_at(model: &mut Model, screen: Point2, factor: f32) {
    let old = model.zoom;
    let new = (old * factor).clamp(ZOOM_MIN, ZOOM_MAX);
    if (new - old).abs() < f32::EPSILON {
        return;
    }
    model.pan = screen - (screen - model.pan) * (new / old);
    model.zoom = new;
    model.zoom_changed = Some(Instant::now());
}

// --- Number boxes --------------------------------------------------------------

/// The number boxes a module shows, in drawing order.
fn param_slots(module: &PlacedModule) -> Vec<Slot> {
    match &module.body {
        ModuleBody::Blend(_) | ModuleBody::Number(_) | ModuleBody::Lfo(_) | ModuleBody::Lag(_) | ModuleBody::Video(_) => {
            vec![Slot::Main]
        }
        ModuleBody::Shader(s) => (0..s.params.len()).map(Slot::Prop).collect(),
        ModuleBody::Map(m) => (0..m.fields.len()).map(Slot::Field).collect(),
        ModuleBody::Wled(_) => vec![Slot::Field(0)],
        ModuleBody::Osc(o) => (0..o.fields.len()).map(Slot::Field).collect(),
        _ => Vec::new(),
    }
}

fn param_of(module: &PlacedModule, slot: Slot) -> Option<Param> {
    match (&module.body, slot) {
        (ModuleBody::Blend(b), Slot::Main) => Some(b.mix),
        (ModuleBody::Number(n), Slot::Main) => Some(n.value),
        (ModuleBody::Lfo(l), Slot::Main) => Some(l.freq),
        (ModuleBody::Lag(l), Slot::Main) => Some(l.time),
        (ModuleBody::Video(v), Slot::Main) => Some(v.speed),
        (ModuleBody::Shader(s), Slot::Prop(i)) => s.params.get(i).copied(),
        (ModuleBody::Map(m), Slot::Field(i)) => m.fields.get(i).copied(),
        (ModuleBody::Wled(w), Slot::Field(0)) => Some(w.raw_max),
        (ModuleBody::Osc(o), Slot::Field(i)) => o.fields.get(i).copied(),
        _ => None,
    }
}

/// Applies `f` to a number box (remembering a Shader's tuning by name).
fn edit_param(model: &mut Model, target: ParamTarget, f: impl FnOnce(&mut Param)) {
    let Some(module) = model.modules.iter_mut().find(|m| m.id == target.module) else { return };
    match (&mut module.body, target.slot) {
        (ModuleBody::Blend(b), Slot::Main) => f(&mut b.mix),
        (ModuleBody::Number(n), Slot::Main) => f(&mut n.value),
        (ModuleBody::Lfo(l), Slot::Main) => f(&mut l.freq),
        (ModuleBody::Lag(l), Slot::Main) => f(&mut l.time),
        (ModuleBody::Video(v), Slot::Main) => f(&mut v.speed),
        (ModuleBody::Map(m), Slot::Field(i)) if i < m.fields.len() => f(&mut m.fields[i]),
        (ModuleBody::Wled(w), Slot::Field(0)) => f(&mut w.raw_max),
        (ModuleBody::Osc(o), Slot::Field(i)) if i < o.fields.len() => f(&mut o.fields[i]),
        (ModuleBody::Shader(s), Slot::Prop(i)) => {
            if let Some(p) = s.params.get_mut(i) {
                f(p);
                s.remember(i);
            }
        }
        _ => {}
    }
}

/// The input port feeding a number box, if it has one.
fn param_input(module: &PlacedModule, slot: Slot) -> Option<usize> {
    match (&module.body, slot) {
        // A Stack has no second layer port: its mix input follows the layers.
        (ModuleBody::Blend(_), Slot::Main) => Some(if module.kind == ModuleKind::Stack { 1 } else { 2 }),
        (ModuleBody::Video(_), Slot::Main) => Some(0),
        (ModuleBody::Shader(s), Slot::Prop(i)) => Some(s.channels() + i),
        _ => None,
    }
}

/// The signal wired into a number box right now, if anything is.
fn param_signal(model: &Model, module: &PlacedModule, slot: Slot, now: f32) -> Option<f32> {
    let input = param_input(module, slot)?;
    input_number(&model.modules, &model.connections, module.id, input, now, &mut vec![module.id])
}

/// Everything needed to draw a number box.
struct ParamView {
    param: Param,
    shown: f32,
    driven: bool,
    text: String,
}

fn param_view(model: &Model, module: &PlacedModule, slot: Slot, now: f32) -> Option<ParamView> {
    let param = param_of(module, slot)?;
    let signal = param_signal(model, module, slot, now);
    let driven = signal.is_some();
    let shown = match (&module.body, slot) {
        (ModuleBody::Shader(s), Slot::Prop(i)) => s.prop_value(i, signal),
        _ => param.resolve(signal),
    };
    let suffix = if driven { " (in)" } else { "" };
    let text = match (&module.body, slot) {
        (ModuleBody::Blend(_), _) => format!("Mix {}{suffix}", fmt_num(shown)),
        (ModuleBody::Lfo(_), _) => {
            let out = module_number(&model.modules, &model.connections, module.id, now, &mut Vec::new()).unwrap_or(0.0);
            format!("{} Hz \u{2192} {}", fmt_num(shown), fmt_num(out))
        }
        (ModuleBody::Lag(lag), _) => {
            let out = lag.output().map_or("\u{2014}".to_string(), |s| fmt_signal(&s));
            format!("{} s \u{2192} {out}", fmt_num(shown))
        }
        (ModuleBody::Shader(s), Slot::Prop(i)) => {
            let prop = s.props().get(i);
            let label = prop.map_or("", |p| p.label.as_str());
            match prop.and_then(|p| p.option_name(shown)) {
                Some(option) => format!("{label}: {option}{suffix}"),
                None => format!("{label} {}{suffix}", fmt_num(shown)),
            }
        }
        (ModuleBody::Video(_), _) => format!("Speed {}\u{00D7}{suffix}", fmt_num(shown)),
        (ModuleBody::Map(_), Slot::Field(i)) => format!("{} {}", modules::math::map::MAP_LABELS[i], fmt_num(shown)),
        (ModuleBody::Wled(_), _) => format!("Raw max {:.0}", shown),
        (ModuleBody::Osc(_), Slot::Field(0)) => format!("Port {:.0}", shown.round()),
        (ModuleBody::Osc(_), Slot::Field(i)) => format!("{} {}", modules::math::osc::OSC_LABELS[i.min(2)], fmt_num(shown)),
        _ => format!("{}{suffix}", fmt_num(shown)),
    };
    Some(ParamView { param, shown, driven, text })
}

/// Title for a number box's options popover.
fn param_title(module: &PlacedModule, slot: Slot) -> String {
    match (&module.body, slot) {
        (ModuleBody::Blend(_), _) => "Blend \u{00B7} Mix".into(),
        (ModuleBody::Lfo(_), _) => "LFO \u{00B7} Frequency (Hz)".into(),
        (ModuleBody::Lag(_), _) => "Lag \u{00B7} Time (s)".into(),
        (ModuleBody::Video(_), _) => "Video \u{00B7} Speed (\u{00D7})".into(),
        (ModuleBody::Map(_), Slot::Field(i)) => {
            ["Map \u{00B7} From min", "Map \u{00B7} From max", "Map \u{00B7} To min", "Map \u{00B7} To max", "Map \u{00B7} Bias (added to the input)"][i.min(4)].into()
        }
        (ModuleBody::Wled(_), _) => "WLED \u{00B7} Raw max (full-scale reading)".into(),
        (ModuleBody::Osc(_), Slot::Field(i)) => {
            ["OSC \u{00B7} UDP port", "OSC \u{00B7} Value mapped to 0", "OSC \u{00B7} Value mapped to 1"][i.min(2)].into()
        }
        (ModuleBody::Shader(s), Slot::Prop(i)) => {
            format!("Shader \u{00B7} {}", s.props().get(i).map_or("", |p| p.label.as_str()))
        }
        _ => kind_label(module.kind).into(),
    }
}

fn param_rect(module: &PlacedModule, slot: Slot) -> Rect {
    let body = module_body_rect(module);
    match slot {
        Slot::Main if module.kind == ModuleKind::Video => video_speed_rect(body),
        Slot::Main => config_rect(body),
        Slot::Field(i) => field_rect(module, body, i),
        Slot::Prop(i) => {
            let inputs = module.input_types().len();
            let channels = inputs.saturating_sub(param_slots(module).len());
            let port = module_input_port(module, body, channels + i, inputs);
            Rect::from_corners(
                pt2(body.left() + 20.0, port.y - PROP_BOX_H / 2.0),
                pt2(body.right() - 12.0, port.y + PROP_BOX_H / 2.0),
            )
        }
    }
}

fn param_rect_of(model: &Model, target: ParamTarget) -> Option<Rect> {
    model.modules.iter().find(|m| m.id == target.module).map(|m| param_rect(m, target.slot))
}

/// The number box under `world`, respecting module stacking.
fn param_at(model: &Model, world: Point2) -> Option<ParamTarget> {
    for module in model.modules.iter().rev() {
        if model.hidden(module.id) {
            continue;
        }
        if module.body.chrome().minimized {
            if module_body_rect(module).contains(world) {
                return None;
            }
            continue;
        }
        for slot in param_slots(module) {
            if param_rect(module, slot).contains(world) {
                return Some(ParamTarget { module: module.id, slot });
            }
        }
        if module_body_rect(module).contains(world) {
            return None;
        }
    }
    None
}

fn start_edit(model: &mut Model, target: ParamTarget, field: EditField) {
    let Some(param) = model.modules.iter().find(|m| m.id == target.module).and_then(|m| param_of(m, target.slot))
    else {
        return;
    };
    let value = match field {
        EditField::Value => param.value,
        EditField::Min => param.min,
        EditField::Max => param.max,
    };
    model.editing = Some(TextEdit::new(EditTarget::Param(target, field), fmt_num(value), false));
}

/// Applies what was typed (an unparsable number is just dropped).
fn commit_edit(model: &mut Model) {
    let Some(edit) = model.editing.take() else { return };
    match edit.target {
        EditTarget::Param(target, field) => {
            let Ok(v) = edit.buffer.trim().parse::<f32>() else { return };
            edit_param(model, target, |p| match field {
                EditField::Value => p.set(v),
                EditField::Min => p.set_range(v, p.max),
                EditField::Max => p.set_range(p.min, v),
            });
        }
        EditTarget::WledAddress => add_wled_address(model, edit.buffer.trim()),
        EditTarget::GroupTitle(id) => {
            let title = edit.buffer.trim();
            if let (false, Some(g)) = (title.is_empty(), model.groups.iter_mut().find(|g| g.id == id)) {
                g.title = title.to_string();
            }
        }
        EditTarget::TabName(i) => {
            let name = edit.buffer.trim();
            if let (false, Some(tab)) = (name.is_empty(), model.tabs.get_mut(i)) {
                tab.name = name.to_string();
            }
        }
        EditTarget::PresetName(i) => {
            let name = edit.buffer.trim();
            if let (false, Some(p)) = (name.is_empty(), model.presets.get_mut(i)) {
                p.name = name.to_string();
                save::save_presets(&model.presets);
            }
        }
    }
}

/// What's typed into `field` of `target`, if that's the field being edited.
fn editing_text(model: &Model, target: ParamTarget, field: EditField) -> Option<ui::EditView<'_>> {
    model
        .editing
        .as_ref()
        .filter(|e| e.target == EditTarget::Param(target, field))
        .map(|e| ui::EditView { text: &e.buffer, cursor: e.cursor, selection: e.selection() })
}

/// Caret position for a pointer over a field drawn with `label_left`.
fn left_aligned_caret(text: &str, field: Rect, p: Point2, font: u32, inside_only: bool) -> Option<usize> {
    if inside_only && !field.contains(p) {
        return None;
    }
    Some(ui::caret_at(text, font, p.x - (field.left() + 10.0)))
}

/// The caret position under the pointer for the text being edited (a
/// number box or a tab name). With `inside_only`, `None` when the pointer
/// is outside its field; a drag passes false to keep following the pointer.
fn caret_for_pointer(model: &Model, win: Rect, screen: Point2, inside_only: bool) -> Option<usize> {
    let edit = model.editing.as_ref()?;
    match edit.target {
        EditTarget::Param(target, field) => {
            let (rect, p, font) = match param_menu_layout(model, win) {
                Some((_, layout)) => {
                    let rect = match field {
                        EditField::Value => layout.value,
                        EditField::Min => layout.min,
                        EditField::Max => layout.max,
                    };
                    (rect, screen, 12)
                }
                None if field == EditField::Value => {
                    let rect = param_rect_of(model, target)?;
                    (rect, to_world(model, screen), if target.slot == Slot::Main { 13 } else { 11 })
                }
                None => return None,
            };
            if inside_only && !rect.contains(p) {
                return None;
            }
            let x = p.x - (rect.x() - ui::text_width(&edit.buffer, font) / 2.0);
            Some(ui::caret_at(&edit.buffer, font, x))
        }
        EditTarget::TabName(i) => project::caret_for(model, win, i, screen, inside_only),
        EditTarget::GroupTitle(id) => {
            let group = model.groups.iter().find(|g| g.id == id)?;
            let frame = model.group_frame(group)?;
            let field = group::title_rect(frame).pad_right(group::BUTTONS.len() as f32 * 24.0 + 8.0).pad_left(4.0);
            left_aligned_caret(&edit.buffer, field, to_world(model, screen), 13, inside_only)
        }
        EditTarget::PresetName(i) => {
            let field = sidebar_items(model, win).into_iter().find_map(|row| match row {
                SidebarRow::Module { row_rect, action: SidebarAction::Preset(p), .. } if p == i => Some(row_rect),
                _ => None,
            })?;
            left_aligned_caret(&edit.buffer, field.pad_left(18.0).pad_right(28.0).pad_left(4.0), screen, 13, inside_only)
        }
        EditTarget::WledAddress => left_aligned_caret(&edit.buffer, wled_layout(model, win).address, screen, 12, inside_only),
    }
}

fn edit_view(edit: &TextEdit) -> ui::EditView<'_> {
    ui::EditView { text: &edit.buffer, cursor: edit.cursor, selection: edit.selection() }
}

fn editing_preset(model: &Model, index: usize) -> Option<ui::EditView<'_>> {
    model.editing.as_ref().filter(|e| e.target == EditTarget::PresetName(index)).map(edit_view)
}

fn editing_title(model: &Model, group: u64) -> Option<ui::EditView<'_>> {
    model.editing.as_ref().filter(|e| e.target == EditTarget::GroupTitle(group)).map(edit_view)
}

// --- Selection, clipboard, groups ------------------------------------------------

fn copy_selection(model: &mut Model) {
    let snippet = save::snippet(model, &model.selected);
    if snippet.is_empty() {
        return;
    }
    let n = model.selected.iter().filter(|id| **id != model.output_id).count();
    model.clipboard = Some(snippet);
    model.notify(format!("Copied {n} module{}", if n == 1 { "" } else { "s" }));
}

/// Pastes under the mouse (or mid-canvas if the mouse is elsewhere) and
/// selects what was pasted.
fn paste_clipboard(app: &App, model: &mut Model) {
    let Some(snippet) = model.clipboard.clone() else { return };
    let win = app.window_rect();
    let mouse = pt2(app.mouse.x, app.mouse.y);
    let at = if over_empty_canvas(model, win, mouse) || canvas_rect(win).contains(mouse) && !sidebar_rect(win).contains(mouse) {
        to_world(model, mouse)
    } else {
        to_world(model, canvas_rect(win).xy())
    };
    model.selected = save::paste(model, &snippet, at);
}

fn duplicate_selection(model: &mut Model) {
    let snippet = save::snippet(model, &model.selected);
    if snippet.is_empty() {
        return;
    }
    let bodies: Vec<Point2> = model.modules.iter().filter(|m| model.selected.contains(&m.id)).map(|m| m.pos).collect();
    let center = bodies.iter().fold(Vec2::ZERO, |a, p| a + *p) / bodies.len().max(1) as f32;
    model.selected = save::paste(model, &snippet, center + vec2(40.0, -40.0));
}

fn group_selection(model: &mut Model) {
    let members: Vec<u64> = model.selected.clone();
    if members.is_empty() {
        return;
    }
    let title = format!("Group {}", model.groups.len() + 1);
    model.add_group(title, members, false);
    model.notify("Grouped. Double-click the title to rename it; the star saves it to My groups.");
}

fn ungroup_selection(model: &mut Model) {
    let selected = model.selected.clone();
    model.groups.retain(|g| !g.members.iter().any(|id| selected.contains(id)));
}

/// Deletes modules (never Output), their cables, and empty groups.
/// Before module `id` goes: whatever feeds its top input (the first one of
/// the same signal type as its output) is wired to everything its output fed.
fn bridge_over(model: &mut Model, id: u64) {
    let Some(module) = model.modules.iter().find(|m| m.id == id) else { return };
    let Some(signal) = output_type(module.kind) else { return };
    let types = module.input_types();
    let upstream = (0..types.len())
        .filter(|&n| types[n].accepts(signal))
        .find_map(|n| multi_input::source_ports(&model.connections, id, n).first().copied());
    let Some((upstream, upstream_output)) = upstream else { return };
    let downstream: Vec<(u64, usize)> =
        model.connections.iter().filter(|c| c.from == id).map(|c| (c.to, c.to_input)).collect();
    for (to, to_input) in downstream {
        if to == upstream {
            continue;
        }
        let Some(kind) = model.modules.iter().find(|m| m.id == to).map(|m| m.kind) else { continue };
        // A multi port keeps the order: the bridge takes the deleted cable's place.
        if multi_input::is_multi(kind, to_input) {
            if let Some(pos) = model.connections.iter().position(|c| c.from == id && c.to == to && c.to_input == to_input) {
                if model.connections.iter().any(|c| c.from == upstream && c.from_output == upstream_output && c.to == to && c.to_input == to_input) {
                    continue;
                }
                model.connections[pos].from = upstream;
                model.connections[pos].from_output = upstream_output;
            }
        } else {
            multi_input::connect(&mut model.connections, kind, upstream, upstream_output, to, to_input);
        }
    }
}

fn delete_modules(model: &mut Model, ids: &[u64]) {
    let output_id = model.output_id;
    let doomed: Vec<u64> = ids.iter().copied().filter(|id| *id != output_id).collect();
    if doomed.is_empty() {
        return;
    }
    // Take them out one at a time, each leaving its upstream wired to its
    // downstream so the path from the top input to the output survives.
    for &id in &doomed {
        bridge_over(model, id);
        model.connections.retain(|c| c.from != id && c.to != id);
    }
    model.modules.retain(|m| !doomed.contains(&m.id));
    // A bridged array may have landed on a plain number input.
    drop_mismatched(&model.modules, &mut model.connections);
    for g in &mut model.groups {
        g.members.retain(|id| !doomed.contains(id));
    }
    model.groups.retain(|g| !g.members.is_empty());
    model.selected.retain(|id| !doomed.contains(id));
    if model.param_menu.is_some_and(|t| doomed.contains(&t.module)) {
        model.param_menu = None;
    }
    if matches!(model.editing, Some(TextEdit { target: EditTarget::Param(t, _), .. }) if doomed.contains(&t.module)) {
        model.editing = None;
    }
}

/// Saves group `id` to My groups. A favorite with the same modules and
/// parameters isn't added twice: it just takes the group's current name.
/// Anything else that differs becomes a new favorite.
fn save_group_to_palette(model: &mut Model, id: u64) {
    let Some(g) = model.groups.iter().find(|g| g.id == id) else { return };
    let name = g.title.clone();
    let snippet = save::snippet(model, &g.members);
    if snippet.is_empty() {
        return;
    }
    let print = snippet.fingerprint();
    let message = match model.presets.iter_mut().find(|p| p.snippet.fingerprint() == print) {
        Some(existing) if existing.name == name => format!("\u{201C}{name}\u{201D} is already in My groups"),
        Some(existing) => {
            let old = std::mem::replace(&mut existing.name, name.clone());
            existing.snippet = snippet;
            format!("Renamed favorite \u{201C}{old}\u{201D} to \u{201C}{name}\u{201D}")
        }
        None => {
            model.presets.push(Preset { name: name.clone(), snippet });
            format!("Saved \u{201C}{name}\u{201D} to My groups")
        }
    };
    save::save_presets(&model.presets);
    model.notify(message);
}

/// What a right-click menu on a saved group or a group frame is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemTarget {
    Preset(usize),
    Group(u64),
    /// Save would replace `composition.json`: overwrite it or pick a name.
    SaveConflict,
    /// A project tab (index into `tabs`).
    Tab(usize),
}

struct ItemMenu {
    at: Point2,
    target: ItemTarget,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ItemCommand {
    InsertPreset(usize),
    RenamePreset(usize),
    MovePresetUp(usize),
    MovePresetDown(usize),
    DeletePreset(usize),
    RenameGroup(u64),
    SyncGroup(u64),
    ToggleGroup(u64),
    UngroupGroup(u64),
    SaveOverwrite,
    SaveAsOtherName,
    RenameTab(usize),
    CloseTab(usize),
    Dismiss,
}

fn item_menu_items(model: &Model, target: ItemTarget) -> Vec<(&'static str, ItemCommand)> {
    use ItemCommand::*;
    match target {
        ItemTarget::Preset(i) => {
            let mut items = vec![("Add to canvas", InsertPreset(i)), ("Rename", RenamePreset(i))];
            if i > 0 {
                items.push(("Move up", MovePresetUp(i)));
            }
            if i + 1 < model.presets.len() {
                items.push(("Move down", MovePresetDown(i)));
            }
            items.push(("Delete", DeletePreset(i)));
            items
        }
        ItemTarget::Tab(i) => vec![("Rename", RenameTab(i)), ("Close", CloseTab(i))],
        ItemTarget::SaveConflict => vec![
            ("Overwrite existing file", SaveOverwrite),
            ("Save with another name\u{2026}", SaveAsOtherName),
            ("Cancel", Dismiss),
        ],
        ItemTarget::Group(id) => {
            let collapsed = model.groups.iter().find(|g| g.id == id).is_some_and(|g| g.collapsed);
            vec![
                ("Rename", RenameGroup(id)),
                ("Sync with favorite", SyncGroup(id)),
                (if collapsed { "Expand" } else { "Collapse" }, ToggleGroup(id)),
                ("Ungroup", UngroupGroup(id)),
            ]
        }
    }
}

fn run_item_command(model: &mut Model, win: Rect, command: ItemCommand) {
    match command {
        ItemCommand::InsertPreset(i) => {
            if let Some(snippet) = model.presets.get(i).map(|p| p.snippet.clone()) {
                let at = to_world(model, canvas_rect(win).xy());
                model.selected = save::paste(model, &snippet, at);
            }
        }
        ItemCommand::RenamePreset(i) => {
            if let Some(name) = model.presets.get(i).map(|p| p.name.clone()) {
                model.editing = Some(TextEdit::new(EditTarget::PresetName(i), name, true));
                model.notify("Type the new name, Enter to confirm, Esc to cancel");
            }
        }
        ItemCommand::MovePresetUp(i) if i > 0 && i < model.presets.len() => {
            model.presets.swap(i, i - 1);
            save::save_presets(&model.presets);
        }
        ItemCommand::MovePresetDown(i) if i + 1 < model.presets.len() => {
            model.presets.swap(i, i + 1);
            save::save_presets(&model.presets);
        }
        ItemCommand::DeletePreset(i) if i < model.presets.len() => {
            let preset = model.presets.remove(i);
            save::save_presets(&model.presets);
            model.notify(format!("Removed \u{201C}{}\u{201D} from My groups", preset.name));
        }
        ItemCommand::RenameGroup(id) => start_group_rename(model, id),
        ItemCommand::SyncGroup(id) => save_group_to_palette(model, id),
        ItemCommand::ToggleGroup(id) => {
            if let Some(g) = model.groups.iter_mut().find(|g| g.id == id) {
                g.collapsed = !g.collapsed;
            }
        }
        ItemCommand::UngroupGroup(id) => model.groups.retain(|g| g.id != id),
        ItemCommand::SaveOverwrite => {
            let (message, is_error) = match save_composition(model) {
                Ok(()) => (format!("Saved to {SAVE_PATH}"), false),
                Err(err) => (format!("Save failed: {err}"), true),
            };
            model.status = Some((message, is_error, Instant::now()));
        }
        ItemCommand::SaveAsOtherName => open_dialog(model, || {
            pollster::block_on(
                rfd::AsyncFileDialog::new().add_filter("Composition (*.json)", &["json"]).set_file_name(SAVE_PATH).save_file(),
            )
            .map(|f| DialogReply::SaveProject(f.path().to_path_buf()))
        }),
        ItemCommand::RenameTab(i) => project::start_rename(model, i),
        ItemCommand::CloseTab(i) => project::close_tab(model, win, i),
        ItemCommand::Dismiss => {}
        _ => {}
    }
}

fn start_group_rename(model: &mut Model, id: u64) {
    if let Some(title) = model.groups.iter().find(|g| g.id == id).map(|g| g.title.clone()) {
        model.editing = Some(TextEdit::new(EditTarget::GroupTitle(id), title, true));
        model.notify("Type the new name, Enter to confirm, Esc to cancel");
    }
}

/// Where a click lands on nothing: no module, group, sidebar or panel.
fn over_empty_canvas(model: &Model, win: Rect, screen: Point2) -> bool {
    if sidebar_rect(win).contains(screen) || over_panel(model, win, screen) {
        return false;
    }
    let world = to_world(model, screen);
    let on_module = model.modules.iter().any(|m| !model.hidden(m.id) && module_body_rect(m).contains(world));
    let on_group = model.groups.iter().filter_map(|g| model.group_frame(g).map(|f| (g, f))).any(|(g, f)| {
        if g.collapsed {
            f.contains(world)
        } else {
            group::title_rect(f).contains(world)
        }
    });
    !on_module && !on_group
}

/// The options popover's parts, in screen space, next to the box it edits.
struct MenuLayout {
    panel: Rect,
    value: Rect,
    min: Rect,
    max: Rect,
    clamp: Rect,
    reset: Rect,
}

fn menu_layout(win: Rect, anchor: Rect) -> MenuLayout {
    let (w, h) = (240.0, 214.0);
    let mut x = anchor.right() + 10.0 + w / 2.0;
    if x + w / 2.0 > win.right() - 8.0 {
        x = anchor.left() - 10.0 - w / 2.0;
    }
    let y = (anchor.top() - h / 2.0).clamp(win.bottom() + h / 2.0 + 8.0, win.top() - h / 2.0 - 8.0);
    let panel = Rect::from_x_y_w_h(x, y, w, h);
    let row = |i: f32| Rect::from_x_y_w_h(panel.x(), panel.top() - 52.0 - i * 36.0, panel.w() - 24.0, 28.0);
    let field = |r: Rect| r.pad_left(56.0);
    MenuLayout {
        panel,
        value: field(row(0.0)),
        min: field(row(1.0)),
        max: field(row(2.0)),
        clamp: row(3.0),
        reset: row(4.0),
    }
}

fn param_menu_layout(model: &Model, win: Rect) -> Option<(ParamTarget, MenuLayout)> {
    let target = model.param_menu?;
    let anchor = to_screen_rect(model, param_rect_of(model, target)?);
    Some((target, menu_layout(win, anchor)))
}

// --- Input -----------------------------------------------------------------------

fn handle_press(app: &App, model: &mut Model, win: Rect, mouse: Point2) {
    // A click inside the tab name being edited moves the caret instead of ending the edit.
    if let Some(at) = caret_for_pointer(model, win, mouse, true) {
        let double = model.last_click.is_some_and(|(t, p)| t.elapsed() < Duration::from_millis(400) && p.distance(mouse) < 6.0);
        model.last_click = Some((Instant::now(), mouse));
        if let Some(edit) = model.editing.as_mut() {
            if double {
                edit.select_all();
            } else {
                edit.click_at(at);
                model.dragging = Some(Drag::TextSelect);
            }
        }
        return;
    }
    commit_edit(model);
    if start_button_rect(win).contains(mouse) && !popup_open(model) {
        enter_fullscreen(app, model);
        return;
    }
    let mods = app.keys.mods;
    handle_press_config(model, win, mouse, mods.shift(), mods.logo() || mods.ctrl(), app.time);
}

fn popup_open(model: &Model) -> bool {
    model.param_menu.is_some()
        || model.palette_open
        || model.canvas_menu.is_some()
        || model.item_menu.is_some()
        || model.stack_menu.is_some()
        || open_dropdown(model).is_some()
}

fn open_dropdown(model: &Model) -> Option<usize> {
    model.modules.iter().position(|m| match &m.body {
        ModuleBody::Camera(cam) => cam.dropdown_open,
        ModuleBody::Shader(s) => s.dropdown_open || s.prop_dropdown.is_some(),
        ModuleBody::Lfo(l) => l.dropdown_open,
        ModuleBody::Blend(b) => b.dropdown_open,
        ModuleBody::Lag(l) => l.dropdown_open,
        ModuleBody::Video(v) => v.dropdown_open,
        ModuleBody::Wled(w) => w.dropdown_open,
        _ => false,
    })
}

fn handle_right_press(model: &mut Model, win: Rect, screen: Point2) {
    commit_edit(model);
    close_popups(model);
    if project::bar_rect(win).contains(screen) {
        if let Some(i) = project::tab_at(model, win, screen) {
            model.item_menu = Some(ItemMenu { at: screen, target: ItemTarget::Tab(i) });
        }
        return;
    }
    // Right-clicking a saved group in the sidebar opens its options.
    if sidebar_rect(win).contains(screen) {
        let hit = sidebar_items(model, win).into_iter().find_map(|row| match row {
            SidebarRow::Module { row_rect, action, .. } if row_rect.contains(screen) && sidebar_list_rect(win).contains(screen) => {
                Some(action)
            }
            _ => None,
        });
        if let Some(SidebarAction::Preset(i)) = hit {
            if i < model.presets.len() {
                model.item_menu = Some(ItemMenu { at: screen, target: ItemTarget::Preset(i) });
            }
        }
        return;
    }
    let world = to_world(model, screen);
    model.param_menu = param_at(model, world);
    if model.param_menu.is_some() {
        return;
    }
    // A group's title bar (or its collapsed box): the group's options.
    let on_module = model.modules.iter().any(|m| !model.hidden(m.id) && module_body_rect(m).contains(world));
    if !on_module {
        let hit = model.groups.iter().rev().find(|g| {
            model.group_frame(g).is_some_and(|f| if g.collapsed { f.contains(world) } else { group::title_rect(f).contains(world) })
        });
        if let Some(g) = hit {
            model.item_menu = Some(ItemMenu { at: screen, target: ItemTarget::Group(g.id) });
            return;
        }
    }
    // Anywhere else on the canvas: the context menu. Right-clicking an
    // unselected module selects it first, so the menu acts on it.
    if let Some(m) = model.modules.iter().rev().find(|m| !model.hidden(m.id) && module_body_rect(m).contains(world)) {
        if !model.selected.contains(&m.id) {
            model.selected = vec![m.id];
        }
    }
    model.canvas_menu = Some(screen);
}

/// The canvas context menu's entries.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CanvasCommand {
    Copy,
    Duplicate,
    Group,
    Delete,
    Paste,
    AutoLayout,
    ArrangeHorizontal,
    ArrangeVertical,
    SelectAll,
    TogglePreviews,
    ToggleMinimized,
}

impl CanvasCommand {
    /// Label, then the shortcut after a tab (drawn right-aligned).
    fn label(self) -> &'static str {
        match self {
            CanvasCommand::Copy => "Copy\tCmd+C",
            CanvasCommand::Duplicate => "Duplicate\tD",
            CanvasCommand::Group => "Group\tCmd+G",
            CanvasCommand::Delete => "Delete\tDel",
            CanvasCommand::Paste => "Paste here\tCmd+V",
            CanvasCommand::AutoLayout => "Auto layout\tL",
            CanvasCommand::ArrangeHorizontal => "Arrange in a row",
            CanvasCommand::ArrangeVertical => "Arrange in a column",
            CanvasCommand::SelectAll => "Select all\tCmd+A",
            CanvasCommand::TogglePreviews => "Hide / show previews\tP",
            CanvasCommand::ToggleMinimized => "Minimize / expand all\tM",
        }
    }
}

fn canvas_menu_items(model: &Model) -> Vec<CanvasCommand> {
    use CanvasCommand::*;
    let mut items = Vec::new();
    if !model.selected.is_empty() {
        items.extend([Copy, Duplicate, Group, Delete]);
    }
    if model.clipboard.is_some() {
        items.push(Paste);
    }
    items.extend([AutoLayout, ArrangeHorizontal, ArrangeVertical, SelectAll, TogglePreviews, ToggleMinimized]);
    items
}

const CANVAS_MENU_W: f32 = 230.0;

/// Row `i` of the context menu opened at `at`, kept inside the window.
fn canvas_menu_row(win: Rect, at: Point2, n: usize, i: usize) -> Rect {
    let h = n as f32 * DROPDOWN_ROW_H;
    let left = at.x.min(win.right() - CANVAS_MENU_W - 8.0);
    let top = at.y.max(win.bottom() + h + 8.0);
    Rect::from_x_y_w_h(left + CANVAS_MENU_W / 2.0, top - DROPDOWN_ROW_H / 2.0 - i as f32 * DROPDOWN_ROW_H, CANVAS_MENU_W, DROPDOWN_ROW_H)
}

fn run_canvas_command(model: &mut Model, command: CanvasCommand, at_world: Point2) {
    match command {
        CanvasCommand::Copy => copy_selection(model),
        CanvasCommand::Duplicate => duplicate_selection(model),
        CanvasCommand::Group => group_selection(model),
        CanvasCommand::Delete => {
            let ids = model.selected.clone();
            delete_modules(model, &ids);
        }
        CanvasCommand::Paste => {
            if let Some(snippet) = model.clipboard.clone() {
                model.selected = save::paste(model, &snippet, at_world);
            }
        }
        CanvasCommand::AutoLayout => arrange(model, Arrangement::Auto),
        CanvasCommand::ArrangeHorizontal => arrange(model, Arrangement::Row),
        CanvasCommand::ArrangeVertical => arrange(model, Arrangement::Column),
        CanvasCommand::SelectAll => model.selected = model.modules.iter().filter(|m| !model.hidden(m.id)).map(|m| m.id).collect(),
        CanvasCommand::TogglePreviews => toggle_previews(model),
        CanvasCommand::ToggleMinimized => toggle_minimized(model),
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Arrangement {
    /// Columns by signal flow: sources on the left, Output on the right.
    Auto,
    Row,
    Column,
}

const LAYOUT_GAP_X: f32 = 70.0;
const LAYOUT_GAP_Y: f32 = 28.0;

/// The modules layout commands act on: the selection when it has two or
/// more, otherwise everything visible.
fn layout_targets(model: &Model) -> Vec<u64> {
    let visible = |id: &u64| !model.hidden(*id);
    if model.selected.len() >= 2 {
        model.selected.iter().copied().filter(visible).collect()
    } else {
        model.modules.iter().map(|m| m.id).filter(visible).collect()
    }
}

fn arrange(model: &mut Model, how: Arrangement) {
    let ids = layout_targets(model);
    if ids.is_empty() {
        return;
    }
    let rect_of = |model: &Model, id: u64| model.modules.iter().find(|m| m.id == id).map(module_body_rect).unwrap();
    // Anchor everything at the current top-left of the set.
    let rects: Vec<Rect> = ids.iter().map(|id| rect_of(model, *id)).collect();
    let left = rects.iter().map(|r| r.left()).fold(f32::MAX, f32::min);
    let top = rects.iter().map(|r| r.top()).fold(f32::MIN, f32::max);

    let mut columns: Vec<Vec<u64>> = match how {
        Arrangement::Row => {
            let mut sorted = ids.clone();
            sorted.sort_by(|a, b| rect_of(model, *a).x().total_cmp(&rect_of(model, *b).x()));
            sorted.into_iter().map(|id| vec![id]).collect()
        }
        Arrangement::Column => {
            let mut sorted = ids.clone();
            sorted.sort_by(|a, b| rect_of(model, *b).y().total_cmp(&rect_of(model, *a).y()));
            vec![sorted]
        }
        Arrangement::Auto => {
            // Depth = longest chain of cables feeding a module (within the set).
            let mut depth: std::collections::HashMap<u64, usize> = ids.iter().map(|id| (*id, 0)).collect();
            for _ in 0..ids.len() {
                let mut changed = false;
                for c in &model.connections {
                    if let (Some(&d_from), Some(&d_to)) = (depth.get(&c.from), depth.get(&c.to)) {
                        if d_to < d_from + 1 && d_from + 1 < ids.len() {
                            depth.insert(c.to, d_from + 1);
                            changed = true;
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
            let max = depth.values().copied().max().unwrap_or(0);
            let mut columns = vec![Vec::new(); max + 1];
            for id in &ids {
                columns[depth[id]].push(*id);
            }
            for column in &mut columns {
                column.sort_by(|a, b| rect_of(model, *b).y().total_cmp(&rect_of(model, *a).y()));
            }
            columns.retain(|c| !c.is_empty());
            columns
        }
    };

    let mut x = left;
    for column in &mut columns {
        let mut y = top;
        for id in column.iter() {
            let r = rect_of(model, *id);
            if let Some(m) = model.modules.iter_mut().find(|m| m.id == *id) {
                m.pos = pt2(x + r.w() / 2.0, y - r.h() / 2.0);
            }
            y -= r.h() + LAYOUT_GAP_Y;
        }
        let widest = column.iter().map(|id| rect_of(model, *id).w()).fold(0.0, f32::max);
        x += widest + LAYOUT_GAP_X;
    }
}

/// Hides every preview if any is showing; otherwise shows them all.
fn toggle_previews(model: &mut Model) {
    let ids = layout_targets(model);
    let any_on = model.modules.iter().any(|m| ids.contains(&m.id) && m.body.chrome().show_preview);
    for m in model.modules.iter_mut().filter(|m| ids.contains(&m.id) && modules::has_video_preview(m.kind)) {
        m.body.chrome_mut().show_preview = !any_on;
    }
}

/// Minimizes everything if anything is expanded; otherwise expands all.
fn toggle_minimized(model: &mut Model) {
    let ids = layout_targets(model);
    let any_open = model.modules.iter().any(|m| ids.contains(&m.id) && !m.body.chrome().minimized);
    for m in model.modules.iter_mut().filter(|m| ids.contains(&m.id)) {
        m.body.chrome_mut().minimized = any_open;
    }
}

fn handle_press_config(model: &mut Model, win: Rect, screen_mouse: Point2, shift: bool, cmd: bool, now: f32) {
    // Sidebar, buttons and popovers live in screen space; modules in world space.
    let mouse = to_world(model, screen_mouse);
    let double_click = model
        .last_click
        .is_some_and(|(at, pos)| at.elapsed() < Duration::from_millis(400) && pos.distance(screen_mouse) < 6.0);
    model.last_click = Some((Instant::now(), screen_mouse));

    if let Some(menu) = model.stack_menu.take() {
        let n = multi_input::sources(&model.connections, menu.module, 0).len();
        let (panel, rows) = stack_menu_layout(win, menu.at, n);
        if panel.contains(screen_mouse) {
            for (k, row) in rows.iter().enumerate() {
                if row.up.contains(screen_mouse) {
                    move_layer(model, menu.module, k, -1);
                } else if row.down.contains(screen_mouse) {
                    move_layer(model, menu.module, k, 1);
                } else if row.remove.contains(screen_mouse) {
                    remove_layer(model, menu.module, k);
                }
            }
            // Stays open for more changes while there are layers to order.
            if !multi_input::sources(&model.connections, menu.module, 0).is_empty() {
                model.stack_menu = Some(menu);
            }
        }
        return;
    }

    if let Some(menu) = model.item_menu.take() {
        let items = item_menu_items(model, menu.target);
        if let Some(i) = (0..items.len()).find(|&i| canvas_menu_row(win, menu.at, items.len(), i).contains(screen_mouse)) {
            run_item_command(model, win, items[i].1);
        }
        return;
    }

    if let Some(at) = model.canvas_menu.take() {
        let items = canvas_menu_items(model);
        if let Some(i) = (0..items.len()).find(|&i| canvas_menu_row(win, at, items.len(), i).contains(screen_mouse)) {
            run_canvas_command(model, items[i], to_world(model, at));
        }
        return;
    }

    if project::press(model, win, screen_mouse, double_click) {
        return;
    }

    // An open popup takes the click: it either hits one of its controls or
    // closes; it never falls through to anything underneath.
    if let Some((target, layout)) = param_menu_layout(model, win) {
        if !layout.panel.contains(screen_mouse) {
            model.param_menu = None;
            return;
        }
        if layout.value.contains(screen_mouse) {
            start_edit(model, target, EditField::Value);
        } else if layout.min.contains(screen_mouse) {
            start_edit(model, target, EditField::Min);
        } else if layout.max.contains(screen_mouse) {
            start_edit(model, target, EditField::Max);
        } else if layout.clamp.contains(screen_mouse) {
            edit_param(model, target, |p| p.set_clamp(!p.clamp));
        } else if layout.reset.contains(screen_mouse) {
            let default = default_param(model, target);
            edit_param(model, target, |p| *p = default);
        }
        return;
    }

    if let Some(open_idx) = open_dropdown(model) {
        let options = dropdown_labels(model, &model.modules[open_idx]);
        let selector = dropdown_anchor(&model.modules[open_idx]);
        let visible = dropdown_window(model, options.len());
        let first = visible.start;
        let row = visible.clone().find(|&row| dropdown_option_rect(selector, row - first).contains(mouse));
        // Camera/Shader lists start with "None"; mode lists don't.
        let choice = row.map(|row| if row == 0 { None } else { Some(row - 1) });

        // An Enum property's options: pick one.
        if let ModuleBody::Shader(shader) = &mut model.modules[open_idx].body {
            if let Some(prop) = shader.prop_dropdown.take() {
                model.dropdown_scroll = 0.0;
                let target = ParamTarget { module: model.modules[open_idx].id, slot: Slot::Prop(prop) };
                if let Some(r) = row {
                    edit_param(model, target, |p| p.set(r as f32));
                }
                return;
            }
        }

        // Shader rows: category headings do nothing and keep the list open.
        let shader_rows = match (&model.modules[open_idx].body, model.modules[open_idx].kind.shader_role()) {
            (ModuleBody::Shader(sh), Some(role)) => shader_rows(model, role, sh.category.as_deref()),
            _ => Vec::new(),
        };
        if let Some(ShaderRow::Header(_)) = row.and_then(|r| shader_rows.get(r)) {
            return;
        }

        model.dropdown_scroll = 0.0;
        let devices = wled_devices(model);
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
                match row.and_then(|r| shader_rows.get(r)) {
                    Some(ShaderRow::None) => shader.select(None),
                    Some(ShaderRow::File(file)) => shader.select(Some(file.clone())),
                    _ => {}
                }
            }
            ModuleBody::Lfo(lfo) => {
                lfo.dropdown_open = false;
                if let Some(wave) = row.and_then(|r| Wave::ALL.get(r)) {
                    lfo.wave = *wave;
                }
            }
            ModuleBody::Blend(blend) => {
                blend.dropdown_open = false;
                if let Some(mode) = row.and_then(|r| BlendMode::ALL.get(r)) {
                    blend.mode = *mode;
                }
            }
            ModuleBody::Lag(lag) => {
                lag.dropdown_open = false;
                if let Some(mode) = row.and_then(|r| LagMode::ALL.get(r)) {
                    lag.mode = *mode;
                }
            }
            ModuleBody::Video(video) => {
                video.dropdown_open = false;
                if let Some(mode) = row.and_then(|r| LoopMode::ALL.get(r)) {
                    video.looping = *mode;
                }
            }
            ModuleBody::Wled(w) => {
                w.dropdown_open = false;
                if let Some(pick) = choice {
                    let device = pick.and_then(|i| devices.get(i)).map(|(addr, _)| *addr);
                    if device != w.device {
                        w.device = device;
                        w.pin = None;
                    }
                }
            }
            _ => {}
        }
        return;
    }

    if model.palette_open {
        model.palette_open = false;
        if let Some(i) = (0..Palette::ALL.len()).find(|&i| palette_option_rect(win, model.debug_open, i).contains(screen_mouse)) {
            model.settings.palette = Palette::ALL[i];
            model.settings.save();
        }
        return;
    }

    if settings_button_rect(win).contains(screen_mouse) {
        model.settings_open = !model.settings_open;
        model.wled_open = false;
        model.help_open = false;
        return;
    }
    if help_button_rect(win).contains(screen_mouse) {
        model.help_open = !model.help_open;
        model.settings_open = false;
        model.wled_open = false;
        return;
    }
    if wled_button_rect(win).contains(screen_mouse) {
        model.wled_open = !model.wled_open;
        model.settings_open = false;
        model.help_open = false;
        return;
    }
    if model.help_open && help_panel_rect(win).contains(screen_mouse) {
        return;
    }
    if model.wled_open && wled_layout(model, win).panel.contains(screen_mouse) {
        wled_panel_press(model, win, screen_mouse);
        return;
    }
    if model.settings_open && settings_panel_rect(win, model.debug_open).contains(screen_mouse) {
        for index in 0..2 {
            let rect = settings_slider_rect(win, index);
            if rect.pad(-4.0).contains(screen_mouse) {
                set_setting(&mut model.settings, index, slider_value_at(rect, screen_mouse.x));
                model.dragging = Some(Drag::Setting { index });
            }
        }
        let rows = settings_rows(model.debug_open);
        if let Some(row) = (0..rows.len()).find(|&row| settings_row_rect(win, row).contains(screen_mouse)) {
            let settings = &mut model.settings;
            match rows[row] {
                SettingsRow::DebugSection => model.debug_open = !model.debug_open,
                SettingsRow::Palette => model.palette_open = true,
                SettingsRow::ShowFps => settings.show_fps = !settings.show_fps,
                SettingsRow::ExitButton => settings.show_exit_button = !settings.show_exit_button,
                SettingsRow::ProjectMode => settings.project_mode = !settings.project_mode,
                SettingsRow::RestoreSession => settings.restore_session = !settings.restore_session,
                SettingsRow::StartFullscreen => settings.start_fullscreen = !settings.start_fullscreen,
                SettingsRow::DebugLog => settings.debug_log = !settings.debug_log,
                SettingsRow::SerializeGrabs => {
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
        // Project mode asks where to save and under what name.
        if model.settings.project_mode {
            let current = model.project.clone();
            open_dialog(model, move || {
                let mut dialog = rfd::AsyncFileDialog::new().add_filter("Project (*.json)", &["json"]);
                dialog = match current.as_deref().and_then(|p| p.file_name()).and_then(|n| n.to_str()) {
                    Some(name) => dialog.set_file_name(name),
                    None => dialog.set_file_name("project.json"),
                };
                if let Some(dir) = current.as_deref().and_then(|p| p.parent()) {
                    dialog = dialog.set_directory(dir);
                }
                pollster::block_on(dialog.save_file()).map(|f| DialogReply::SaveProject(f.path().to_path_buf()))
            });
            return;
        }
        // Sample mode: don't silently replace an existing file.
        if std::path::Path::new(SAVE_PATH).exists() {
            let at = save_button_rect(win).xy();
            model.item_menu = Some(ItemMenu { at, target: ItemTarget::SaveConflict });
            return;
        }
        let (message, is_error) = match save_composition(model) {
            Ok(()) => (format!("Saved to {SAVE_PATH}"), false),
            Err(err) => (format!("Save failed: {err}"), true),
        };
        model.status = Some((message, is_error, Instant::now()));
        return;
    }

    if load_button_rect(win).contains(screen_mouse) {
        open_dialog(model, || {
            pollster::block_on(
                rfd::AsyncFileDialog::new()
                    .add_filter("Composition (*.json)", &["json"])
                    .set_file_name(SAVE_PATH)
                    .pick_file(),
            )
            .map(|f| DialogReply::Composition(f.path().to_path_buf()))
        });
        return;
    }

    for i in 0..2 {
        if sidebar_tab_rect(win, i).contains(screen_mouse) {
            if model.examples_tab != (i == 1) {
                model.examples_tab = i == 1;
                model.sidebar_scroll = 0.0;
            }
            return;
        }
    }
    if sidebar_list_rect(win).contains(screen_mouse) {
        let hit = sidebar_items(model, win).into_iter().find_map(|row| match row {
            SidebarRow::Module { row_rect, action, .. } if row_rect.contains(screen_mouse) => Some(action),
            _ => None,
        });
        match hit {
            Some(SidebarAction::Spawn(kind)) => {
                model.selected = vec![spawn_module(win, model, kind)];
            }
            Some(SidebarAction::ShaderCategory { category, role }) => {
                let id = spawn_module(win, model, ModuleKind::for_shader_role(role));
                // A category with a single shader has nothing to choose: open it.
                let mut files = model.shader_library.iter().filter(|i| i.category == category && i.role == role);
                let only = match (files.next(), files.next()) {
                    (Some(info), None) => Some(info.file.clone()),
                    _ => None,
                };
                if let Some(ModuleBody::Shader(shader)) = model.modules.iter_mut().find(|m| m.id == id).map(|m| &mut m.body) {
                    shader.category = Some(category);
                    if only.is_some() {
                        shader.select(only);
                    }
                }
                model.selected = vec![id];
            }
            Some(SidebarAction::Example(i)) => {
                // Examples are whole projects: each opens in a tab of its own.
                let Some((name, json)) = model.examples.get(i).map(|e| (e.name.clone(), e.json)) else { return };
                match project::open(model, win, name.clone(), |m| save::load_composition_json(m, win, json)) {
                    Ok(()) => model.notify(format!("Opened example \u{201C}{name}\u{201D}")),
                    Err(err) => model.status = Some((format!("Example {name}: {err}"), true, Instant::now())),
                }
            }
            Some(SidebarAction::Preset(i)) => {
                if let Some(snippet) = model.presets.get(i).map(|p| p.snippet.clone()) {
                    let at = to_world(model, canvas_rect(win).xy());
                    model.selected = save::paste(model, &snippet, at);
                }
            }
            None => {}
        }
        return;
    }

    // Modules scrolled underneath the sidebar can't be grabbed through it.
    if sidebar_rect(win).contains(screen_mouse) {
        return;
    }

    if let Some(target) = param_at(model, mouse) {
        let driven = model
            .modules
            .iter()
            .find(|m| m.id == target.module)
            .is_some_and(|m| param_signal(model, m, target.slot, 0.0).is_some());
        // A wired box is a readout; right-click still opens its options.
        if driven {
            return;
        }
        if is_enum_param(model, target) {
            if let (Slot::Prop(i), Some(ModuleBody::Shader(s))) =
                (target.slot, model.modules.iter_mut().find(|m| m.id == target.module).map(|m| &mut m.body))
            {
                s.prop_dropdown = Some(i);
            }
        } else {
            model.dragging = Some(Drag::Param { target, last_x: mouse.x, moved: 0.0 });
        }
        return;
    }

    let available = model.available_cameras.clone();

    for i in (0..model.modules.len()).rev() {
        let kind = model.modules[i].kind;
        let id = model.modules[i].id;
        if model.hidden(id) {
            continue;
        }
        let (minimized, show_preview) = {
            let chrome = model.modules[i].body.chrome();
            (chrome.minimized, chrome.show_preview)
        };
        let body = module_body_rect(&model.modules[i]);

        if let Some(slot) = (0..title_buttons_for(kind).len()).find(|&slot| title_button_rect(body, slot).contains(mouse)) {
            match title_buttons_for(kind)[slot] {
                TitleButton::Delete => delete_modules(model, &[id]),
                TitleButton::Minimize => model.modules[i].body.chrome_mut().minimized = !minimized,
                TitleButton::Preview => model.modules[i].body.chrome_mut().show_preview = !show_preview,
            }
            return;
        }

        if has_mode_row(kind) && !minimized {
            let on_mode = mode_rect(kind, body).contains(mouse);
            let on_center = lfo_center_rect(body).contains(mouse);
            match &mut model.modules[i].body {
                ModuleBody::Lfo(lfo) if on_center => lfo.centered = !lfo.centered,
                ModuleBody::Lfo(lfo) if on_mode => lfo.dropdown_open = true,
                ModuleBody::Blend(blend) if on_mode => blend.dropdown_open = true,
                ModuleBody::Lag(lag) if on_mode => lag.dropdown_open = true,
                _ => {}
            }
            if on_mode || (kind == ModuleKind::Lfo && on_center) {
                return;
            }
        }

        if let (Some(button), false, true) = (map_button_at(body, mouse), minimized, matches!(model.modules[i].body, ModuleBody::Map(_))) {
            let id = model.modules[i].id;
            // An array calibrates on its average: they share one bias.
            let input = input_signal(&model.modules, &model.connections, id, 0, now, &mut vec![id])
                .map(|s| s.values().iter().sum::<f32>() / s.values().len().max(1) as f32);
            if let ModuleBody::Map(map) = &mut model.modules[i].body {
                match button {
                    0 => {
                        if let Some(x) = input {
                            map.calibrate(x);
                        }
                    }
                    1 => map.clamp = !map.clamp,
                    2 => {
                        map.auto = !map.auto;
                        map.relearn();
                    }
                    _ => map.center = !map.center,
                }
            }
            return;
        }
        if !minimized && matches!(model.modules[i].body, ModuleBody::Wled(_)) {
            if config_rect(body).contains(mouse) {
                // No devices yet: the panel is where to get some.
                if model.wled.devices().is_empty() {
                    model.wled_open = true;
                    model.settings_open = false;
                } else if let ModuleBody::Wled(w) = &mut model.modules[i].body {
                    w.dropdown_open = true;
                }
                return;
            }
        }
        if !minimized && output_at(&model.modules[i], mouse).is_none() {
            if let ModuleBody::Select(select) = &mut model.modules[i].body {
                if let Some(k) = (0..2).find(|&k| select_mode_rect(body, k).contains(mouse)) {
                    // Switching mode changes what the outputs carry: their cables go.
                    if select.combine != (k == 1) {
                        select.combine = k == 1;
                        model.connections.retain(|c| c.from != id);
                    }
                    return;
                }
                if let Some(e) = (0..select.values.len()).find(|&e| select_row_rect(body, e).contains(mouse)) {
                    select.toggle(e);
                    // An element that's no longer picked has no port to hold a cable.
                    if !select.combine && !select.is_selected(e) {
                        model.connections.retain(|c| !(c.from == id && c.from_output == e));
                    }
                    return;
                }
            }
        }

        if let (ModuleBody::Video(video), false) = (&mut model.modules[i].body, minimized) {
            if config_rect(body).contains(mouse) {
                let module = model.modules[i].id;
                open_dialog(model, move || {
                    pollster::block_on(rfd::AsyncFileDialog::new().add_filter("Video / image", VIDEO_EXTENSIONS).pick_file())
                        .map(|f| DialogReply::Video { module, path: f.path().to_path_buf() })
                });
                return;
            }
            if video_play_rect(body).contains(mouse) {
                // Play from the start again after a "Once" run ended.
                if !video.playing && video.looping == LoopMode::Once && video.position + 0.05 >= video.duration() {
                    video.position = 0.0;
                }
                video.playing = !video.playing;
                return;
            }
            if !video.is_still() && video_loop_rect(body).contains(mouse) {
                video.dropdown_open = true;
                return;
            }
            if video_scrub_rect(body).pad(-4.0).contains(mouse) {
                video.seek_fraction(slider_value_at(video_scrub_rect(body), mouse.x));
                model.dragging = Some(Drag::Scrub { id });
                return;
            }
        }

        if has_config_row(kind) && !minimized && config_rect(body).contains(mouse) {
            match &mut model.modules[i].body {
                ModuleBody::Camera(cam) => {
                    if !available.is_empty() {
                        cam.dropdown_open = true;
                    }
                }
                ModuleBody::Shader(shader) if shader.file.is_none() || !shader.single_choice => {
                    shader.dropdown_open = true;
                    model.shader_library = scan_library();
                }
                _ => {}
            }
            return;
        }

        if let Some(output) = output_at(&model.modules[i], mouse) {
            model.dragging = Some(Drag::Cable { from: id, output });
            return;
        }

        let inputs = model.modules[i].input_types().len();
        if let Some(input_index) = (0..inputs).find(|&n| port_hit(&model.modules[i], body, n, inputs, mouse)) {
            // A multi-input pill opens its order menu instead of picking a cable up.
            if multi_input::is_multi(kind, input_index) {
                if !multi_input::sources(&model.connections, id, input_index).is_empty() {
                    model.stack_menu = Some(StackMenu { module: id, at: screen_mouse });
                }
                return;
            }
            // Pick up whatever's already plugged in here, if anything,
            // so it can be moved to a different input or unplugged.
            if let Some(pos) = model.connections.iter().rposition(|c| c.to == id && c.to_input == input_index) {
                let c = model.connections.remove(pos);
                model.dragging = Some(Drag::Cable { from: c.from, output: c.from_output });
            }
            return;
        }

        if body.contains(mouse) {
            // Shift toggles a module in or out of the selection; a plain
            // click on an unselected module selects just it. Dragging moves
            // the whole selection.
            if shift {
                if let Some(pos) = model.selected.iter().position(|s| *s == id) {
                    model.selected.remove(pos);
                    return;
                }
                model.selected.push(id);
            } else if !model.selected.contains(&id) {
                model.selected = vec![id];
            }
            // Bring the grabbed module to the front.
            let module = model.modules.remove(i);
            let offset = module.pos - mouse;
            let start = module.pos;
            model.modules.push(module);
            model.dragging = Some(Drag::Module { id, offset, start });
            return;
        }
    }

    // Group frames: title-bar buttons, then the title bar (or a collapsed
    // box) itself, which selects the members and drags them together.
    for gi in (0..model.groups.len()).rev() {
        let Some(frame) = model.group_frame(&model.groups[gi]) else { continue };
        let gid = model.groups[gi].id;
        if let Some(slot) = (0..group::BUTTONS.len()).find(|&s| group::button_rect(frame, s).contains(mouse)) {
            match group::BUTTONS[slot] {
                GroupButton::Ungroup => {
                    model.groups.remove(gi);
                }
                GroupButton::Collapse => {
                    let g = &mut model.groups[gi];
                    g.collapsed = !g.collapsed;
                }
                GroupButton::SaveToPalette => save_group_to_palette(model, gid),
            }
            return;
        }
        let grab = if model.groups[gi].collapsed { frame } else { group::title_rect(frame) };
        if grab.contains(mouse) {
            if double_click {
                start_group_rename(model, gid);
                return;
            }
            let members = model.groups[gi].members.clone();
            if shift {
                model.selected.extend(members.iter().filter(|m| !model.selected.contains(m)).copied().collect::<Vec<_>>());
            } else {
                model.selected = members;
            }
            model.dragging = Some(Drag::Group { id: gid, last: mouse });
            return;
        }
    }

    // Empty canvas: start a selection rectangle (Cmd/Ctrl adds to it).
    model.settings_open = false;
    model.wled_open = false;
    model.dragging = Some(Drag::Marquee { start: mouse, additive: cmd || shift });
}

/// The selection rectangle being dragged, in world space.
fn marquee_rect(model: &Model, mouse_screen: Point2) -> Option<Rect> {
    match model.dragging {
        Some(Drag::Marquee { start, .. }) => Some(Rect::from_corners(start, to_world(model, mouse_screen))),
        _ => None,
    }
}

/// What Reset puts back: the box a fresh module (or the shader file) has.
fn default_param(model: &Model, target: ParamTarget) -> Param {
    let module = model.modules.iter().find(|m| m.id == target.module);
    match (module.map(|m| &m.body), target.slot) {
        (Some(ModuleBody::Lfo(_)), _) => modules::math::lfo::LfoState::new().freq,
        (Some(ModuleBody::Lag(_)), _) => modules::math::lag::LagState::new().time,
        (Some(ModuleBody::Video(_)), _) => VideoState::new().speed,
        (Some(ModuleBody::Map(_)), Slot::Field(i)) => modules::math::map::MapState::new().fields[i.min(4)],
        (Some(ModuleBody::Wled(_)), _) => WledState::new().raw_max,
        (Some(ModuleBody::Osc(_)), Slot::Field(i)) => modules::math::osc::OscState::new().fields[i.min(2)],
        (Some(ModuleBody::Blend(_)), _) => modules::mix::blend::BlendState::new().mix,
        (Some(ModuleBody::Shader(s)), Slot::Prop(i)) => s.props().get(i).map_or(Param::unit(0.0), |p| p.param()),
        _ => Param::unit(0.0),
    }
}

fn handle_release(app: &App, model: &mut Model) {
    match model.dragging {
        Some(Drag::Setting { .. }) => {
            model.dragging = None;
            model.settings.save();
            return;
        }
        Some(Drag::Pan { .. }) => {
            model.dragging = None; // Shift+left pan ends with the left button
            return;
        }
        Some(Drag::Param { target, moved, .. }) => {
            model.dragging = None;
            if moved < CLICK_SLOP {
                start_edit(model, target, EditField::Value);
            }
            return;
        }
        Some(Drag::Marquee { additive, .. }) => {
            let rect = marquee_rect(model, pt2(app.mouse.x, app.mouse.y)).unwrap();
            model.dragging = None;
            let tiny = rect.w() * model.zoom < CLICK_SLOP && rect.h() * model.zoom < CLICK_SLOP;
            let hits: Vec<u64> = if tiny {
                Vec::new() // a plain click on empty canvas
            } else {
                model
                    .modules
                    .iter()
                    .filter(|m| !model.hidden(m.id) && module_body_rect(m).overlap(rect).is_some())
                    .map(|m| m.id)
                    .collect()
            };
            if !additive {
                model.selected.clear();
            }
            for id in hits {
                if !model.selected.contains(&id) {
                    model.selected.push(id);
                }
            }
            return;
        }
        Some(Drag::Group { .. } | Drag::Scrub { .. }) => {
            model.dragging = None;
            return;
        }
        Some(Drag::Module { id, start, .. }) => {
            model.dragging = None;
            if let Some((index, input)) = insert_candidate(model, id, start) {
                insert_into_cable(model, id, index, input);
            }
            let mut moved = model.selected.clone();
            if !moved.contains(&id) {
                moved.push(id);
            }
            join_group_under(model, &moved);
            push_overlapping(model, &moved);
            return;
        }
        _ => {}
    }
    let drag = model.dragging.take();
    let Some(Drag::Cable { from, output }) = drag else {
        return;
    };

    let Some(from_type) = port_signal(model, from, output) else {
        return;
    };

    let mouse = to_world(model, pt2(app.mouse.x, app.mouse.y));
    let mut target = None;

    for module in model.modules.iter().filter(|m| !model.hidden(m.id)) {
        let types = module.input_types();
        let body = module_body_rect(module);
        for (input_index, port_type) in types.iter().enumerate() {
            // A cable can only land on an input that takes its signal —
            // dropping a video cable on a number input (or an array on a
            // plain number input) just fails to connect, same as dropping
            // on empty space.
            if !port_type.accepts(from_type) {
                continue;
            }
            if port_hit(module, body, input_index, types.len(), mouse) {
                target = Some((module.id, input_index));
                break;
            }
        }
        if target.is_some() {
            break;
        }
    }

    // Dropped on another cable of the same signal type: it takes that
    // cable's place on the input it feeds.
    if target.is_none() {
        if let Some(i) = cable_drop_target(model, from, output, mouse) {
            let c = model.connections.remove(i);
            target = Some((c.to, c.to_input));
        }
    }

    if let Some((to, to_input)) = target {
        if let Some(kind) = model.modules.iter().find(|m| m.id == to).map(|m| m.kind) {
            multi_input::connect(&mut model.connections, kind, from, output, to, to_input);
            // A math module that now gets an array sends one on: whatever
            // plain number input it fed can't take it any more.
            drop_mismatched(&model.modules, &mut model.connections);
        }
    }
    // Dropped in empty space (or on a mismatched port type): stays
    // disconnected (already removed from `connections` in handle_press
    // if it was picked up from an input).
}

/// The mouse cursor for whatever is under it.
fn cursor_for(app: &App, model: &Model) -> CursorIcon {
    match model.dragging {
        Some(Drag::Pan { .. } | Drag::Module { .. } | Drag::Group { .. }) => return CursorIcon::Grabbing,
        Some(Drag::Marquee { .. }) => return CursorIcon::Crosshair,
        Some(Drag::TextSelect) => return CursorIcon::Text,
        Some(Drag::Param { .. } | Drag::Setting { .. } | Drag::Scrub { .. }) => return CursorIcon::EwResize,
        Some(Drag::Cable { .. }) => return CursorIcon::Crosshair,
        None => {}
    }
    if model.editing.is_some() {
        return CursorIcon::Default;
    }
    let win = app.window_rect();
    let screen = pt2(app.mouse.x, app.mouse.y);
    let world = to_world(model, screen);
    if popup_open(model) {
        return CursorIcon::Hand;
    }
    let buttons = [start_button_rect(win), save_button_rect(win), load_button_rect(win), settings_button_rect(win), help_button_rect(win), wled_button_rect(win)];
    let sidebar_hit = sidebar_list_rect(win).contains(screen)
        && sidebar_items(model, win)
            .iter()
            .any(|r| matches!(r, SidebarRow::Module { row_rect, .. } if row_rect.contains(screen)));
    if buttons.iter().any(|b| b.contains(screen)) || sidebar_hit {
        return CursorIcon::Hand;
    }
    if over_panel(model, win, screen) {
        return CursorIcon::Default;
    }
    if sidebar_rect(win).contains(screen) {
        return CursorIcon::Default;
    }
    if let Some(target) = param_at(model, world) {
        return if is_enum_param(model, target) { CursorIcon::Hand } else { CursorIcon::EwResize };
    }
    for module in model.modules.iter().rev().filter(|m| !model.hidden(m.id)) {
        let body = module_body_rect(module);
        let on_title_button = (0..title_buttons_for(module.kind).len()).any(|s| title_button_rect(body, s).contains(world));
        let open = !module.body.chrome().minimized;
        let on_selector = open
            && match &module.body {
                ModuleBody::Camera(_) => config_rect(body).contains(world),
                ModuleBody::Shader(sh) => (sh.file.is_none() || !sh.single_choice) && config_rect(body).contains(world),
                ModuleBody::Lfo(_) => mode_rect(module.kind, body).contains(world) || lfo_center_rect(body).contains(world),
                ModuleBody::Wled(_) => config_rect(body).contains(world),
                ModuleBody::Select(s) => {
                    (0..2).any(|k| select_mode_rect(body, k).contains(world))
                        || (0..s.values.len()).any(|e| select_row_rect(body, e).contains(world))
                }
                ModuleBody::Map(_) => map_button_at(body, world).is_some(),
                ModuleBody::Video(_) => [config_rect(body), video_play_rect(body), video_loop_rect(body), video_scrub_rect(body).pad(-4.0)]
                    .iter()
                    .any(|r| r.contains(world)),
                _ if has_mode_row(module.kind) => mode_rect(module.kind, body).contains(world),
                _ => false,
            };
        let inputs = module.input_types().len();
        let on_port = output_at(module, world).is_some()
            || (0..inputs).any(|n| port_hit(module, body, n, inputs, world));
        if on_title_button || on_selector || on_port {
            return CursorIcon::Hand;
        }
        if body.contains(world) {
            return CursorIcon::Grab;
        }
    }
    for g in model.groups.iter().rev() {
        let Some(frame) = model.group_frame(g) else { continue };
        if (0..group::BUTTONS.len()).any(|s| group::button_rect(frame, s).contains(world)) {
            return CursorIcon::Hand;
        }
        let grab = if g.collapsed { frame } else { group::title_rect(frame) };
        if grab.contains(world) {
            return CursorIcon::Grab;
        }
    }
    CursorIcon::Default
}

/// A Shader property declared as `Enum(...)`: its box is a dropdown.
fn is_enum_param(model: &Model, target: ParamTarget) -> bool {
    let Slot::Prop(i) = target.slot else { return false };
    model.modules.iter().find(|m| m.id == target.module).is_some_and(|m| match &m.body {
        ModuleBody::Shader(s) => s.props().get(i).is_some_and(|p| matches!(p.kind, modules::effects::shader_file::PropKind::Enum(_))),
        _ => false,
    })
}

fn push_module(model: &mut Model, kind: ModuleKind, pos: Point2) -> u64 {
    modules_push_module(&mut model.modules, &mut model.next_id, kind, pos)
}

fn spawn_module(win: Rect, model: &mut Model, kind: ModuleKind) -> u64 {
    // Land in the middle of what's currently on screen, whatever the pan/zoom.
    let canvas = canvas_rect(win);
    let count = (model.modules.len() % 8) as f32;
    let pos = to_world(model, canvas.xy()) + vec2(30.0 * count, -30.0 * count);
    push_module(model, kind, pos)
}

/// "wave_distort.glsl" -> "Wave Distort".
fn shader_display_name(file: &str) -> String {
    let stem = file.rsplit_once('.').map_or(file, |(stem, _)| stem);
    stem.split(['_', '-', ' '])
        .filter(|w| !w.is_empty())
        .map(|w| {
            let mut chars = w.chars();
            chars.next().map_or(String::new(), |c| c.to_uppercase().chain(chars).collect())
        })
        .collect::<Vec<_>>()
        .join(" ")
}

// --- Layout ------------------------------------------------------------

pub fn canvas_rect(win: Rect) -> Rect {
    win.pad_left(SIDEBAR_WIDTH)
}

fn sidebar_rect(win: Rect) -> Rect {
    Rect::from_corners(pt2(win.left(), win.bottom()), pt2(win.left() + SIDEBAR_WIDTH, win.top()))
}

/// The Save/Load buttons pinned above the category list.
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
    let w = (sidebar.w() - 16.0 - 8.0) * 0.6;
    Rect::from_x_y_w_h(sidebar.left() + 8.0 + w / 2.0, sidebar.bottom() + 28.0, w, 32.0)
}

/// Help sits beside Settings.
fn help_button_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    let settings = settings_button_rect(win);
    let w = sidebar.w() - 16.0 - 8.0 - settings.w();
    Rect::from_x_y_w_h(sidebar.right() - 8.0 - w / 2.0, settings.y(), w, 32.0)
}

const SETTINGS_ROW_TOP: f32 = 150.0;
const SETTINGS_ROW_STEP: f32 = 36.0;

/// The floating settings panel, pinned to the canvas' top-right corner; it
/// grows when the Debug section is expanded.
fn settings_panel_rect(win: Rect, debug_open: bool) -> Rect {
    let rows = settings_rows(debug_open).len() as f32;
    let h = SETTINGS_ROW_TOP + (rows - 1.0) * SETTINGS_ROW_STEP + 32.0;
    Rect::from_w_h(300.0, h).top_right_of(canvas_rect(win)).shift(vec2(-16.0, -16.0))
}

fn settings_slider_rect(win: Rect, index: usize) -> Rect {
    let panel = settings_panel_rect(win, false); // rows only move the bottom edge
    Rect::from_x_y_w_h(panel.x(), panel.top() - 62.0 - index as f32 * 44.0, panel.w() - 32.0, 28.0)
}

fn settings_row_rect(win: Rect, row: usize) -> Rect {
    let panel = settings_panel_rect(win, false);
    let y = panel.top() - SETTINGS_ROW_TOP - row as f32 * SETTINGS_ROW_STEP;
    Rect::from_x_y_w_h(panel.x(), y, panel.w() - 32.0, 28.0)
}

fn palette_option_rect(win: Rect, debug_open: bool, i: usize) -> Rect {
    let row = settings_rows(debug_open).iter().position(|r| *r == SettingsRow::Palette).unwrap_or(0);
    let selector = settings_row_rect(win, row).pad_left(110.0);
    dropdown_option_rect(selector, i)
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
        model.pan = canvas_rect(win).xy() - b.xy() * model.zoom;
    }
}

fn sidebar_items(model: &Model, win: Rect) -> Vec<SidebarRow> {
    sidebar_rows(win, &model.shader_library, &model.presets, model.examples_tab.then_some(&model.examples[..]), model.sidebar_scroll)
}

/// The scrollable middle of the sidebar, between Save/Load and Settings.
fn wled_button_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    Rect::from_x_y_w_h(sidebar.x(), settings_button_rect(win).y() + 40.0, sidebar.w() - 16.0, 32.0)
}

fn sidebar_list_rect(win: Rect) -> Rect {
    let sidebar = sidebar_rect(win);
    Rect::from_corners(
        pt2(sidebar.left(), wled_button_rect(win).top() + 8.0),
        pt2(sidebar.right(), sidebar.top() - SIDEBAR_HEADER_H - SIDEBAR_TABS_H),
    )
}

/// Tab 0 is the module list, tab 1 the bundled examples.
fn sidebar_tab_rect(win: Rect, i: usize) -> Rect {
    let sidebar = sidebar_rect(win);
    let w = (sidebar.w() - 16.0 - 4.0) / 2.0;
    let x = sidebar.x() + if i == 0 { -(w / 2.0 + 2.0) } else { w / 2.0 + 2.0 };
    Rect::from_x_y_w_h(x, sidebar.top() - SIDEBAR_HEADER_H - SIDEBAR_TABS_H / 2.0 - 2.0, w, 26.0)
}

fn sidebar_rows(win: Rect, library: &[ShaderInfo], presets: &[Preset], examples: Option<&[examples::Example]>, scroll: f32) -> Vec<SidebarRow> {
    let sidebar = sidebar_rect(win);
    let mut rows = Vec::new();
    let mut y = sidebar.top() - SIDEBAR_HEADER_H - SIDEBAR_TABS_H - 24.0 + scroll;
    let row = |y: f32, h: f32| Rect::from_x_y_w_h(sidebar.x(), y, sidebar.w() - 16.0, h);
    let plus = |r: Rect| Rect::from_w_h(24.0, 24.0).mid_right_of(r).shift_x(-4.0);

    if let Some(examples) = examples {
        for (heading, kind, hint) in [
            ("By module", examples::ExampleKind::Box, "Each one opens as its own project"),
            ("Full projects", examples::ExampleKind::Composition, "Each one opens as its own project"),
        ] {
            rows.push(SidebarRow::Header { rect: row(y, 26.0), label: heading.to_string(), sub: false });
            y -= 28.0;
            rows.push(SidebarRow::Hint { rect: row(y - 4.0, 18.0), text: hint });
            y -= 28.0;
            for (i, example) in examples.iter().enumerate().filter(|(_, e)| e.kind == kind) {
                let row_rect = row(y, 28.0);
                rows.push(SidebarRow::Module { row_rect, plus_rect: plus(row_rect), label: example.name.clone(), action: SidebarAction::Example(i) });
                y -= 32.0;
            }
            y -= 12.0;
        }
        return rows;
    }

    for (category, mods) in CATEGORIES {
        rows.push(SidebarRow::Header { rect: row(y, 26.0), label: category.to_string(), sub: false });
        y -= 34.0;

        for (label, kind) in mods.iter() {
            let row_rect = row(y, 32.0);
            rows.push(SidebarRow::Module { row_rect, plus_rect: plus(row_rect), label: label.to_string(), action: SidebarAction::Spawn(*kind) });
            y -= 36.0;
        }

        // Every shader is an effect of its own, listed under its category.
        // One node per shader category (and role); the shader is picked
        // from the node's own list.
        if *category == "Effects" || *category == "Mix" {
            let mut seen: Vec<(&str, ShaderRole)> = Vec::new();
            for info in library {
                // Mixing shaders live under Mix, everything else under Effects.
                if (info.role == ShaderRole::Mix) != (*category == "Mix") {
                    continue;
                }
                if seen.contains(&(info.category.as_str(), info.role)) {
                    continue;
                }
                seen.push((&info.category, info.role));
                let row_rect = row(y, 28.0);
                rows.push(SidebarRow::Module {
                    row_rect,
                    plus_rect: plus(row_rect),
                    label: info.category.clone(),
                    action: SidebarAction::ShaderCategory { category: info.category.clone(), role: info.role },
                });
                y -= 32.0;
            }
        }

        y -= 12.0;
    }

    rows.push(SidebarRow::Header { rect: row(y, 26.0), label: "My groups".to_string(), sub: false });
    y -= 34.0;
    if presets.is_empty() {
        rows.push(SidebarRow::Hint { rect: row(y - 8.0, 44.0), text: "Select modules, Cmd/Ctrl+G,\nthen the star on the group" });
    }
    for (i, preset) in presets.iter().enumerate() {
        let row_rect = row(y, 28.0);
        rows.push(SidebarRow::Module { row_rect, plus_rect: plus(row_rect), label: preset.name.clone(), action: SidebarAction::Preset(i) });
        y -= 32.0;
    }

    rows
}

/// Scrolls the sidebar list by `dy` pixels, within its content.
fn scroll_sidebar(model: &mut Model, win: Rect, dy: f32) {
    let rows = sidebar_rows(win, &model.shader_library, &model.presets, model.examples_tab.then_some(&model.examples[..]), 0.0);
    let bottom = rows
        .iter()
        .map(|r| match r {
            SidebarRow::Header { rect, .. } | SidebarRow::Hint { rect, .. } => rect.bottom(),
            SidebarRow::Module { row_rect, .. } => row_rect.bottom(),
        })
        .fold(f32::MAX, f32::min);
    let max = (sidebar_list_rect(win).bottom() - bottom + 8.0).max(0.0);
    model.sidebar_scroll = (model.sidebar_scroll + dy).clamp(0.0, max);
}

/// Height of a module box: collapsed to just the title bar when minimized,
/// otherwise its normal content plus a preview strip if that's enabled.
fn body_height(minimized: bool, show_preview: bool, base: f32) -> f32 {
    if minimized {
        if show_preview {
            MINIMIZED_PREVIEW_H
        } else {
            MINIMIZED_H
        }
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
    Rect::from_x_y_w_h(body.x(), body.top() - 52.0, body.w() - 24.0, 30.0)
}

/// Modules with a second row holding a mode selector.
fn has_mode_row(kind: ModuleKind) -> bool {
    matches!(kind, ModuleKind::Lfo | ModuleKind::Blend | ModuleKind::Stack | ModuleKind::Lag)
}

/// The second row's mode selector (LFO wave, blend mode, lag curve); the
/// LFO's leaves room for its center toggle...
fn mode_rect(kind: ModuleKind, body: Rect) -> Rect {
    let row = config_rect(body).shift_y(-36.0);
    if kind == ModuleKind::Lfo {
        row.pad_right(48.0)
    } else {
        row
    }
}

/// ...and the toggle that centers its output around zero.
fn lfo_center_rect(body: Rect) -> Rect {
    let row = config_rect(body).shift_y(-36.0);
    Rect::from_w_h(42.0, row.h()).mid_right_of(row)
}

/// The box an open dropdown hangs from.
fn dropdown_anchor(module: &PlacedModule) -> Rect {
    let body = module_body_rect(module);
    if let ModuleBody::Shader(s) = &module.body {
        if let Some(i) = s.prop_dropdown {
            return param_rect(module, Slot::Prop(i));
        }
    }
    if module.kind == ModuleKind::Video {
        video_loop_rect(body)
    } else if has_mode_row(module.kind) {
        mode_rect(module.kind, body)
    } else {
        config_rect(body)
    }
}

/// Map's range and bias fields sit two per row from the config row down;
/// WLED's Raw max sits under its input rows.
fn field_rect(module: &PlacedModule, body: Rect, i: usize) -> Rect {
    if let ModuleBody::Wled(w) = &module.body {
        return wled_raw_max_rect(body, wled_rows_top(w), w.inputs.len());
    }
    let row = config_rect(body).shift_y(-36.0 * (i / 2) as f32);
    let half = (row.w() - 6.0) / 2.0;
    let x = if i % 2 == 0 { row.left() + half / 2.0 } else { row.right() - half / 2.0 };
    Rect::from_x_y_w_h(x, row.y(), half, row.h())
}

/// Map's buttons: Cal beside the bias box, then Clamp / Auto / Center in a row.
// "Center" (button 3, `MapState::center`) still works in the code but is hidden until it is tested.
const MAP_BUTTONS: [&str; 3] = ["Cal", "Clamp", "Auto"];

fn map_button_rect(body: Rect, i: usize) -> Rect {
    if i == 0 {
        let row = config_rect(body).shift_y(-72.0);
        let half = (row.w() - 6.0) / 2.0;
        return Rect::from_x_y_w_h(row.right() - half / 2.0, row.y(), half, row.h());
    }
    let row = config_rect(body).shift_y(-108.0);
    let w = (row.w() - 6.0) / 2.0;
    Rect::from_x_y_w_h(row.left() + w / 2.0 + (i - 1) as f32 * (w + 6.0), row.y(), w, row.h())
}

fn map_button_at(body: Rect, p: Point2) -> Option<usize> {
    (0..MAP_BUTTONS.len()).find(|&i| map_button_rect(body, i).contains(p))
}

fn math_readout_rect(module: &PlacedModule, body: Rect) -> Rect {
    match module.kind {
        ModuleKind::Map => {
            config_rect(body).shift_y(-144.0)
        }
        ModuleKind::Osc => {
            let row = config_rect(body).shift_y(-36.0);
            let half = (row.w() - 6.0) / 2.0;
            Rect::from_x_y_w_h(row.right() - half / 2.0, row.y(), half, row.h())
        }
        _ => config_rect(body).shift_y(-36.0),
    }
}

/// The status box under the selector: as tall as its wrapped text.
fn wled_status_rect(body: Rect, lines: usize) -> Rect {
    let h = lines.max(1) as f32 * WLED_STATUS_LINE_H + 6.0;
    Rect::from_x_y_w_h(body.x(), body.top() - 71.0 - h / 2.0, body.w() - 24.0, h)
}

/// Input row `i`; `top` is where the rows start (see `wled_rows_top`).
fn wled_input_rect(body: Rect, top: f32, i: usize) -> Rect {
    let y = body.top() - top - (i as f32 + 0.5) * WLED_ROW_H;
    Rect::from_x_y_w_h(body.x(), y, body.w() - 24.0, WLED_ROW_H - 4.0)
}

/// Select's mode pills (Split, Combine), clear of its input pill.
fn select_mode_rect(body: Rect, k: usize) -> Rect {
    let row = config_rect(body).pad_left(14.0);
    let w = (row.w() - 6.0) / 2.0;
    Rect::from_x_y_w_h(row.left() + w / 2.0 + k as f32 * (w + 6.0), row.y(), w, row.h() - 6.0)
}

/// One of Select's element rows; a picked one has its port at the row's right end.
fn select_row_rect(body: Rect, i: usize) -> Rect {
    let y = body.top() - SELECT_ROWS_TOP - (i as f32 + 0.5) * SELECT_ROW_H;
    Rect::from_x_y_w_h(body.x() + 6.0, y, body.w() - 36.0, SELECT_ROW_H - 4.0)
}

fn wled_raw_max_rect(body: Rect, top: f32, inputs: usize) -> Rect {
    let y = body.top() - top - inputs.max(1) as f32 * WLED_ROW_H - 18.0;
    Rect::from_x_y_w_h(body.x(), y, body.w() - 24.0, 22.0)
}

// --- WLED panel ------------------------------------------------------------------

const WLED_PANEL_W: f32 = 340.0;
const WLED_DEVICE_ROW_H: f32 = 46.0;

/// The WLED panel's parts (screen space), pinned where Settings opens.
struct WledLayout {
    panel: Rect,
    /// Each tracked device's row and its remove button.
    devices: Vec<(SocketAddr, String, Rect, Rect)>,
    hint: Option<Rect>,
    address: Rect,
    add: Rect,
    scan: Rect,
    polls: Vec<(u64, Rect)>,
}

fn wled_layout(model: &Model, win: Rect) -> WledLayout {
    let list = wled_devices(model);
    let rows = list.len().max(1) as f32;
    let h = 44.0 + rows * WLED_DEVICE_ROW_H + 40.0 + 40.0 + 24.0 + 36.0 + 12.0;
    let panel = Rect::from_w_h(WLED_PANEL_W, h).top_right_of(canvas_rect(win)).shift(vec2(-16.0, -16.0));
    let inner_w = panel.w() - 24.0;
    let mut y = panel.top() - 44.0;
    let mut devices = Vec::new();
    for (addr, name) in list.iter() {
        let row = Rect::from_x_y_w_h(panel.x(), y - WLED_DEVICE_ROW_H / 2.0, inner_w, WLED_DEVICE_ROW_H - 6.0);
        let remove = Rect::from_w_h(22.0, 22.0).mid_right_of(row).shift_x(-8.0);
        devices.push((*addr, name.clone(), row, remove));
        y -= WLED_DEVICE_ROW_H;
    }
    let hint = list.is_empty().then(|| {
        let r = Rect::from_x_y_w_h(panel.x(), y - WLED_DEVICE_ROW_H / 2.0, inner_w, WLED_DEVICE_ROW_H - 6.0);
        y -= WLED_DEVICE_ROW_H;
        r
    });
    y -= 20.0;
    let field_row = Rect::from_x_y_w_h(panel.x(), y, inner_w, 30.0);
    let add = Rect::from_w_h(64.0, 30.0).mid_right_of(field_row);
    let address = field_row.pad_right(70.0);
    y -= 40.0;
    let scan = Rect::from_x_y_w_h(panel.x(), y, inner_w, 30.0);
    y -= 40.0;
    let n = Settings::WLED_POLL_CHOICES.len() as f32;
    let pill_w = (inner_w - 56.0 - (n - 1.0) * 4.0) / n;
    let polls = Settings::WLED_POLL_CHOICES
        .iter()
        .enumerate()
        .map(|(i, ms)| (*ms, Rect::from_x_y_w_h(panel.left() + 12.0 + 56.0 + pill_w / 2.0 + i as f32 * (pill_w + 4.0), y, pill_w, 26.0)))
        .collect();
    WledLayout { panel, devices, hint, address, add, scan, polls }
}

/// Over the open Settings or WLED panel.
fn over_panel(model: &Model, win: Rect, screen: Point2) -> bool {
    (model.settings_open && settings_panel_rect(win, model.debug_open).contains(screen))
        || (model.help_open && help_panel_rect(win).contains(screen))
        || (model.wled_open && wled_layout(model, win).panel.contains(screen))
}

fn link_color(t: &Theme, link: Option<&Link>) -> ui::Color {
    match link {
        Some(Link::Online { .. }) => t.ok,
        Some(Link::Error(_)) => t.error,
        Some(Link::Connecting | Link::Stale { .. }) => t.video_port,
        None => t.text_dim,
    }
}

fn draw_wled_panel(draw: &Draw, t: &Theme, win: Rect, model: &Model, mouse: Point2) {
    let layout = wled_layout(model, win);
    ui::panel(draw, t, layout.panel, "WLED controllers");
    let poll = model.wled.poll_interval();
    for (addr, name, row, remove) in &layout.devices {
        ui::rounded(draw, *row, ui::RADIUS, if row.contains(mouse) { t.button_hover } else { t.button });
        let reading = model.wled.reading(*addr);
        let link = reading.as_ref().map(|r| Link::of(r, poll));
        draw.ellipse().x_y(row.left() + 14.0, row.y() + 7.0).radius(5.0).color(link_color(t, link.as_ref()));
        let top = Rect::from_corners(pt2(row.left() + 18.0, row.y()), pt2(remove.left() - 4.0, row.top()));
        ui::label_left(draw, top, name, t.text, 13);
        let inputs = reading.as_ref().map_or(0, |r| r.pins.iter().filter(|p| p.raw.is_some() || p.button_type.is_some_and(|b| b >= 2)).count());
        let detail = match &link {
            Some(Link::Error(_)) | None => format!("{addr} \u{00B7} {}", link.as_ref().map_or("not tracked".into(), |l| l.text())),
            Some(l) => format!("{addr} \u{00B7} {inputs} input{} \u{00B7} {}", if inputs == 1 { "" } else { "s" }, l.text()),
        };
        let bottom = Rect::from_corners(pt2(row.left() + 18.0, row.bottom()), pt2(remove.left() - 4.0, row.y()));
        let color = if matches!(link, Some(Link::Error(_))) { t.error } else { t.text_dim };
        ui::label_left(draw, bottom, &detail, color, 10);
        ui::rounded(draw, *remove, 5.0, if remove.contains(mouse) { t.danger } else { rgba8(0, 0, 0, 50) });
        ui::label(draw, *remove, "\u{00D7}", t.text, 13);
    }
    if let Some(hint) = layout.hint {
        draw.text("No controllers yet: scan the network, or type an IP address below.")
            .xy(hint.xy())
            .wh(hint.pad_left(10.0).wh())
            .left_justify()
            .color(t.text_dim)
            .font_size(11);
    }

    let caret = (model.fps.since.elapsed().as_millis() / 500) % 2 == 0;
    let typed = model.editing.as_ref().filter(|e| e.target == EditTarget::WledAddress).map(edit_view);
    let field = layout.address;
    ui::rounded(draw, field, ui::RADIUS, t.field);
    ui::rounded_outline(draw, field, ui::RADIUS, if typed.is_some() { t.accent } else { t.border }, if typed.is_some() { 2.0 } else { 1.0 });
    match typed {
        Some(view) => ui::edit_label_left(draw, t, field, view, t.field_text, 12, caret),
        None => ui::label_left(draw, field, "IP address or host name", t.fill_driven, 12),
    }
    ui::button(draw, t, layout.add, "Add", layout.add.contains(mouse));
    let scan_label = if model.wled_scanning { "Scanning\u{2026}" } else { "Scan network (mDNS)" };
    ui::button(draw, t, layout.scan, scan_label, layout.scan.contains(mouse) && !model.wled_scanning);

    if let Some((_, first)) = layout.polls.first() {
        let label = Rect::from_corners(pt2(layout.panel.left() + 6.0, first.bottom()), pt2(first.left(), first.top()));
        ui::label_left(draw, label, "Poll", t.text_dim, 12);
    }
    for (ms, rect) in &layout.polls {
        let on = Duration::from_millis(*ms) == poll;
        ui::pill_sized(draw, t, *rect, &format!("{ms} ms"), on, rect.contains(mouse), 11);
    }
}

/// A click inside the WLED panel.
fn wled_panel_press(model: &mut Model, win: Rect, screen: Point2) {
    let layout = wled_layout(model, win);
    if let Some((addr, ..)) = layout.devices.iter().find(|(_, _, _, remove)| remove.contains(screen)) {
        model.wled.remove(*addr);
        return;
    }
    if layout.address.contains(screen) {
        model.editing = Some(TextEdit::new(EditTarget::WledAddress, String::new(), false));
        return;
    }
    if layout.add.contains(screen) {
        // Adding with the field still open: take what's typed.
        if let Some(edit) = model.editing.take().filter(|e| e.target == EditTarget::WledAddress) {
            add_wled_address(model, edit.buffer.trim());
        }
        return;
    }
    if layout.scan.contains(screen) {
        scan_wled(model);
        return;
    }
    if let Some((ms, _)) = layout.polls.iter().find(|(_, r)| r.contains(screen)) {
        model.wled.set_poll_interval(Duration::from_millis(*ms));
        model.settings.wled_poll_ms = *ms;
        model.settings.save();
    }
}

// The Video module's rows, under the file selector (the config row).
fn video_speed_rect(body: Rect) -> Rect {
    config_rect(body).shift_y(-36.0)
}

fn video_transport_row(body: Rect) -> Rect {
    config_rect(body).shift_y(-72.0)
}

fn video_play_rect(body: Rect) -> Rect {
    let row = video_transport_row(body);
    Rect::from_w_h(36.0, row.h()).mid_left_of(row)
}

fn video_loop_rect(body: Rect) -> Rect {
    let row = video_transport_row(body);
    Rect::from_corners(pt2(row.left() + 42.0, row.bottom()), pt2(row.left() + 140.0, row.top()))
}

fn video_time_rect(body: Rect) -> Rect {
    let row = video_transport_row(body);
    Rect::from_corners(pt2(row.left() + 144.0, row.bottom()), row.top_right())
}

fn video_scrub_rect(body: Rect) -> Rect {
    let row = video_transport_row(body);
    Rect::from_x_y_w_h(row.x(), row.bottom() - 14.0, row.w(), 10.0)
}

const VIDEO_EXTENSIONS: &[&str] =
    &["mp4", "mov", "m4v", "mkv", "avi", "webm", "mpg", "mpeg", "mts", "m2ts", "ts", "wmv", "flv", "mxf", "gif",
      // Still images: a one-frame "video".
      "png", "jpg", "jpeg", "bmp", "tif", "tiff", "webp"];

fn is_video_file(path: &std::path::Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| VIDEO_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
}

/// What a file dialog (run off the main thread, so the mixer keeps drawing
/// and rendering while it's open) picked.
enum DialogReply {
    Composition(std::path::PathBuf),
    SaveProject(std::path::PathBuf),
    Video { module: u64, path: std::path::PathBuf },
}

/// Runs `pick` on its own thread; its answer (`None` if cancelled) arrives
/// in `update`. One dialog at a time.
fn open_dialog(model: &mut Model, pick: impl FnOnce() -> Option<DialogReply> + Send + 'static) {
    if model.dialog_open {
        return;
    }
    model.dialog_open = true;
    let tx = model.dialog.0.clone();
    std::thread::spawn(move || {
        let _ = tx.send(pick());
    });
}

/// Makes `path` the current project and shows its name in the window title.
fn set_project(app: &App, model: &mut Model, path: std::path::PathBuf) {
    let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("project").to_string();
    app.main_window().set_title(&format!("Video Mixer \u{2014} {name}"));
    model.project = Some(path);
}

/// Applies a finished dialog.
fn poll_dialogs(app: &App, model: &mut Model) {
    while let Ok(reply) = model.dialog.1.try_recv() {
        model.dialog_open = false;
        match reply {
            Some(DialogReply::SaveProject(mut path)) => {
                if path.extension().is_none() {
                    path.set_extension("json");
                }
                let (message, is_error) = match save_composition_to(model, &path) {
                    Ok(()) => {
                        if model.settings.project_mode {
                            set_project(app, model, path.clone());
                        }
                        (format!("Saved {}", path.display()), false)
                    }
                    Err(err) => (format!("Save failed: {err}"), true),
                };
                model.status = Some((message, is_error, Instant::now()));
            }
            Some(DialogReply::Composition(path)) => {
                let win = app.window_rect();
                let name = path.file_stem().and_then(|n| n.to_str()).unwrap_or("project").to_string();
                let (message, is_error) = match project::open(model, win, name, |m| load_composition(m, win, &path)) {
                    Ok(()) => {
                        if model.settings.project_mode {
                            set_project(app, model, path.clone());
                        }
                        (format!("Loaded {}", path.display()), false)
                    }
                    Err(err) => (format!("Load failed: {err}"), true),
                };
                model.status = Some((message, is_error, Instant::now()));
            }
            Some(DialogReply::Video { module, path }) => {
                if let Some(ModuleBody::Video(video)) = model.modules.iter_mut().find(|m| m.id == module).map(|m| &mut m.body) {
                    video.open(path);
                }
            }
            None => {}
        }
    }
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

/// Which rows of an `n`-row dropdown are showing.
fn dropdown_window(model: &Model, n: usize) -> std::ops::Range<usize> {
    let max_first = n.saturating_sub(DROPDOWN_MAX_ROWS);
    let first = (model.dropdown_scroll.max(0.0) as usize).min(max_first);
    first..(first + DROPDOWN_MAX_ROWS).min(n)
}

fn dropdown_option_rect(selector: Rect, row: usize) -> Rect {
    let y = selector.bottom() - 6.0 - DROPDOWN_ROW_H / 2.0 - row as f32 * DROPDOWN_ROW_H;
    Rect::from_x_y_w_h(selector.x(), y, selector.w(), DROPDOWN_ROW_H)
}

fn slider_value_at(rect: Rect, mouse_x: f32) -> f32 {
    ((mouse_x - rect.left()) / rect.w()).clamp(0.0, 1.0)
}

fn title_button_rect(body: Rect, slot: usize) -> Rect {
    Rect::from_w_h(20.0, 20.0)
        .top_right_of(body)
        .shift(vec2(-8.0 - slot as f32 * 24.0, -7.0))
}

fn module_output_port(body: Rect) -> Point2 {
    if is_minimized_body(body) {
        return pt2(body.right(), body.top() - MINIMIZED_H / 2.0);
    }
    pt2(body.right(), body.y())
}

/// The output ports a module shows, by output number: one for most, one
/// per picked element on a Select that splits (each at its row).
fn output_ports(module: &PlacedModule) -> Vec<(usize, Point2)> {
    if !has_output(module.kind) {
        return Vec::new();
    }
    let body = module_body_rect(module);
    match &module.body {
        ModuleBody::Select(select) if !select.combine => {
            let minimized = is_minimized_body(body);
            select
                .output_elements()
                .into_iter()
                .map(|e| (e, if minimized { module_output_port(body) } else { pt2(body.right(), select_row_rect(body, e).y()) }))
                .collect()
        }
        _ => vec![(0, module_output_port(body))],
    }
}

/// Where output `output` leaves the module (a cable from an output that
/// isn't showing, like an element no longer there, leaves from the middle).
fn output_port_pos(module: &PlacedModule, output: usize) -> Point2 {
    output_ports(module)
        .into_iter()
        .find(|(o, _)| *o == output)
        .map_or_else(|| module_output_port(module_body_rect(module)), |(_, p)| p)
}

/// The output port under `p`, if any.
fn output_at(module: &PlacedModule, p: Point2) -> Option<usize> {
    output_ports(module).into_iter().find(|(_, pos)| p.distance(*pos) <= PORT_HIT_RADIUS).map(|(o, _)| o)
}

/// What output `output` of `id` carries right now (see `modules::signal_type`).
fn port_signal(model: &Model, id: u64, output: usize) -> Option<PortType> {
    signal_type(&model.modules, &model.connections, id, output, &mut Vec::new())
}

/// Whether a body rect is a minimized module's (with or without preview).
fn is_minimized_body(body: Rect) -> bool {
    body.h() <= MINIMIZED_H + 0.5 || (body.h() - MINIMIZED_PREVIEW_H).abs() < 0.5
}

/// Input ports are spaced evenly down the left edge; a single input sits
/// dead center, matching every module that used to have exactly one.
fn module_input_port(module: &PlacedModule, body: Rect, index: usize, total: usize) -> Point2 {
    let kind = module.kind;
    // A Stack's layers plug into one pill; its mix input hangs below it.
    if kind == ModuleKind::Stack {
        if is_minimized_body(body) {
            return pt2(body.left(), body.top() - MINIMIZED_H / 2.0);
        }
        return if index == 0 { pill_rect(body).xy() } else { pt2(body.left(), body.top() - 104.0) };
    }
    // Shader ports hang below the file selector, one labelled row each.
    if kind.is_shader() {
        if is_minimized_body(body) {
            return pt2(body.left(), body.top() - MINIMIZED_H / 2.0);
        }
        return pt2(body.left(), body.top() - SHADER_PORT_TOP + module.port_shift() - index as f32 * SHADER_PORT_STEP);
    }
    if total <= 1 {
        if is_minimized_body(body) {
            return pt2(body.left(), body.top() - MINIMIZED_H / 2.0);
        }
        return pt2(body.left(), body.y());
    }
    let step = body.h() / (total as f32 + 1.0);
    pt2(body.left(), body.top() - step * (index as f32 + 1.0))
}

// --- Multi-input pill ----------------------------------------------------------

const PILL_W: f32 = 22.0;
const PILL_H: f32 = 56.0;

/// The pill a multi-input port is drawn as, centered on the left edge.
fn pill_rect(body: Rect) -> Rect {
    if is_minimized_body(body) {
        return Rect::from_x_y_w_h(body.left(), body.top() - MINIMIZED_H / 2.0, PILL_W, 18.0);
    }
    Rect::from_x_y_w_h(body.left(), body.top() - 50.0, PILL_W, PILL_H)
}

/// Where the `k`-th of `n` cables ends inside the pill, first on top.
fn pill_slot(body: Rect, k: usize, n: usize) -> Point2 {
    let rect = pill_rect(body);
    if n <= 1 {
        return rect.xy();
    }
    let spacing = ((rect.h() - 16.0) / (n as f32 - 1.0)).min(12.0);
    pt2(rect.x(), rect.y() + spacing * (n as f32 - 1.0) / 2.0 - k as f32 * spacing)
}

/// Whether `p` is on input `index` (a multi-input's pill is a bigger target).
fn port_hit(module: &PlacedModule, body: Rect, index: usize, total: usize, p: Point2) -> bool {
    if multi_input::is_multi(module.kind, index) {
        return pill_rect(body).pad(-4.0).contains(p);
    }
    p.distance(module_input_port(module, body, index, total)) <= PORT_HIT_RADIUS
}

fn draw_pill(draw: &Draw, t: &Theme, body: Rect, port_type: PortType, layers: usize, hovered: bool) {
    let rect = pill_rect(body);
    let color = t.port(port_type);
    ui::rounded(draw, rect, rect.w() / 2.0, t.port_ring);
    ui::rounded_outline(draw, rect, rect.w() / 2.0, color, if hovered { 4.0 } else { 2.5 });
    if layers == 0 {
        draw.ellipse().xy(rect.xy()).radius(3.0).no_fill().stroke(color).stroke_weight(1.5);
    }
    for k in 0..layers {
        draw.ellipse().xy(pill_slot(body, k, layers)).radius(3.5).color(color);
    }
    // A number pill (Select's) has its mode pills beside it instead.
    if port_type == PortType::Video {
        let label = if layers == 0 { "layers".to_string() } else { format!("\u{00D7}{layers}") };
        let text = Rect::from_corners(pt2(rect.right() + 6.0, rect.y() - 9.0), pt2(rect.right() + 70.0, rect.y() + 9.0));
        ui::label_left(draw, text, &label, t.title_text, 11);
    }
}

// --- Stack order menu ----------------------------------------------------------

struct StackMenu {
    module: u64,
    at: Point2,
}

struct StackRow {
    up: Rect,
    down: Rect,
    remove: Rect,
    label: Rect,
}

const STACK_ROW_H: f32 = 28.0;

/// The menu's panel and one row per layer, for a menu opened at `at`.
fn stack_menu_layout(win: Rect, at: Point2, n: usize) -> (Rect, Vec<StackRow>) {
    let (w, h) = (300.0, 40.0 + n as f32 * STACK_ROW_H);
    let left = at.x.min(win.right() - w - 8.0).max(win.left() + 8.0);
    let top = at.y.max(win.bottom() + h + 8.0).min(win.top() - 8.0);
    let panel = Rect::from_x_y_w_h(left + w / 2.0, top - h / 2.0, w, h);
    let rows = (0..n)
        .map(|i| {
            let y = top - 40.0 - STACK_ROW_H / 2.0 - i as f32 * STACK_ROW_H + 4.0;
            let button = |from_right: f32| Rect::from_x_y_w_h(panel.right() - 18.0 - from_right, y, 24.0, 22.0);
            StackRow {
                remove: button(0.0),
                down: button(28.0),
                up: button(56.0),
                label: Rect::from_corners(pt2(panel.left() + 12.0, y - 11.0), pt2(panel.right() - 100.0, y + 11.0)),
            }
        })
        .collect();
    (panel, rows)
}

/// What a layer is called in the menu.
fn layer_name(model: &Model, id: u64) -> String {
    match model.modules.iter().find(|m| m.id == id) {
        Some(PlacedModule { body: ModuleBody::Shader(s), kind, .. }) => {
            s.file.as_deref().map(shader_display_name).unwrap_or_else(|| kind_label(*kind).to_string())
        }
        Some(m) => kind_label(m.kind).to_string(),
        None => "?".to_string(),
    }
}

/// Moves layer `k` of a Stack `delta` places (cable order is layer order).
fn move_layer(model: &mut Model, stack: u64, k: usize, delta: isize) {
    let indices: Vec<usize> =
        model.connections.iter().enumerate().filter(|(_, c)| c.to == stack && c.to_input == 0).map(|(i, _)| i).collect();
    let Some(other) = k.checked_add_signed(delta).filter(|o| *o < indices.len()) else { return };
    if k < indices.len() {
        model.connections.swap(indices[k], indices[other]);
    }
}

fn remove_layer(model: &mut Model, stack: u64, k: usize) {
    let found = model.connections.iter().enumerate().filter(|(_, c)| c.to == stack && c.to_input == 0).nth(k).map(|(i, _)| i);
    if let Some(i) = found {
        model.connections.remove(i);
    }
}

fn draw_stack_menu(draw: &Draw, t: &Theme, win: Rect, model: &Model, mouse: Point2) {
    let Some(menu) = &model.stack_menu else { return };
    let layers = multi_input::sources(&model.connections, menu.module, 0);
    let (panel, rows) = stack_menu_layout(win, menu.at, layers.len());
    ui::panel(draw, t, panel, "Stack order \u{00B7} first is the bottom");
    for (k, (row, src)) in rows.iter().zip(&layers).enumerate() {
        ui::label_left(draw, row.label, &format!("{}   {}", k + 1, layer_name(model, *src)), t.text, 12);
        ui::pill_sized(draw, t, row.up, "\u{2191}", false, k > 0 && row.up.contains(mouse), 12);
        ui::pill_sized(draw, t, row.down, "\u{2193}", false, k + 1 < layers.len() && row.down.contains(mouse), 12);
        ui::pill_sized(draw, t, row.remove, "\u{00D7}", false, row.remove.contains(mouse), 12);
    }
}

fn start_button_rect(win: Rect) -> Rect {
    Rect::from_w_h(220.0, 46.0).mid_bottom_of(canvas_rect(win)).shift_y(36.0)
}

fn exit_button_rect(win: Rect) -> Rect {
    Rect::from_w_h(180.0, 40.0).top_right_of(win).shift(vec2(-40.0, -40.0))
}

// --- Drawing -----------------------------------------------------------

fn view(app: &App, model: &Model, render_frame: nannou::Frame) {
    let started = Instant::now();
    let draw = app.draw();
    let win = app.window_rect();
    let t = model.settings.palette.theme();
    let mouse = pt2(app.mouse.x, app.mouse.y);

    match model.view {
        View::Config => {
            draw.background().color(t.canvas);
            // Nothing under a popup reacts to hover.
            let hover = if popup_open(model) || model.dragging.is_some() { None } else { Some(mouse) };

            let world = draw.translate(model.pan.extend(0.0)).scale(model.zoom);
            let world_hover = hover.map(|m| to_world(model, m));
            draw_groups(&world, t, model, app.time, world_hover);
            draw_cables(&world, t, model, to_world(model, pt2(app.mouse.x, app.mouse.y)));
            for module in model.modules.iter().filter(|m| !model.hidden(m.id)) {
                draw_module(&world, t, model, module, app.time, world_hover);
            }
            draw_dragged_cable(&world, t, app, model);
            if let Some(rect) = marquee_rect(model, mouse) {
                let mut fill = t.accent;
                fill.alpha = 40;
                world.rect().xy(rect.xy()).wh(rect.wh()).color(fill);
                world.polyline()
                    .weight(1.0 / model.zoom)
                    .color(t.accent)
                    .points_closed([rect.top_left(), rect.top_right(), rect.bottom_right(), rect.bottom_left()]);
            }

            ui::accent_button(&draw, t, start_button_rect(win), "Fullscreen  (F)", hover.is_some_and(|m| start_button_rect(win).contains(m)));
            project::draw(&draw, t, win, model, hover, app.time);
            draw_sidebar(&draw, t, win, model, hover, app.time);
            if model.settings_open {
                draw_settings_panel(&draw, t, win, model, hover);
            }
            if model.help_open {
                draw_help_panel(&draw, t, win);
            }
            if model.wled_open {
                draw_wled_panel(&draw, t, win, model, hover.unwrap_or(pt2(f32::MAX, f32::MAX)));
            }
            if model.settings.debug_log {
                draw_debug_log(&draw, t, win);
            }

            // Popups render last so they sit on top of everything.
            if let Some(i) = open_dropdown(model) {
                let module = &model.modules[i];
                let selector = dropdown_anchor(module);
                let all_labels = dropdown_labels(model, module);
                let visible = dropdown_window(model, all_labels.len());
                let first = visible.start;
                let labels = &all_labels[visible.clone()];
                let rows: Vec<Rect> = (0..labels.len()).map(|r| dropdown_option_rect(selector, r)).collect();
                let world_mouse = to_world(model, mouse);
                let hovered = rows.iter().position(|r| r.contains(world_mouse));
                let stars: Vec<Option<bool>> = Vec::new();
                let selected = dropdown_selected(model, module).checked_sub(first).filter(|r| *r < labels.len());
                ui::dropdown(&world, t, &rows, labels, selected, hovered, &stars);
                if all_labels.len() > labels.len() {
                    // A scrollbar along the list's right edge.
                    let (top, bottom) = (rows[0].top(), rows[rows.len() - 1].bottom());
                    let h = top - bottom;
                    let thumb_h = h * labels.len() as f32 / all_labels.len() as f32;
                    let thumb_top = top - h * first as f32 / all_labels.len() as f32;
                    let thumb = Rect::from_x_y_w_h(rows[0].right() + 1.0, thumb_top - thumb_h / 2.0, 4.0, thumb_h);
                    ui::rounded(&world, thumb, 2.0, t.text_dim);
                }
            }
            if model.palette_open {
                let labels: Vec<String> = Palette::ALL.iter().map(|p| p.name().to_string()).collect();
                let rows: Vec<Rect> = (0..labels.len()).map(|i| palette_option_rect(win, model.debug_open, i)).collect();
                let hovered = rows.iter().position(|r| r.contains(mouse));
                let selected = Palette::ALL.iter().position(|p| *p == model.settings.palette);
                ui::dropdown(&draw, t, &rows, &labels, selected, hovered, &[]);
            }
            draw_param_menu(&draw, t, win, model, mouse, app.time);
            if let Some(menu) = &model.item_menu {
                let items = item_menu_items(model, menu.target);
                let rows: Vec<Rect> = (0..items.len()).map(|i| canvas_menu_row(win, menu.at, items.len(), i)).collect();
                let labels: Vec<String> = items.iter().map(|(label, _)| label.to_string()).collect();
                let hovered = rows.iter().position(|r| r.contains(mouse));
                ui::dropdown(&draw, t, &rows, &labels, None, hovered, &[]);
            }
            draw_stack_menu(&draw, t, win, model, mouse);
            if let Some(at) = model.canvas_menu {
                let items = canvas_menu_items(model);
                let rows: Vec<Rect> = (0..items.len()).map(|i| canvas_menu_row(win, at, items.len(), i)).collect();
                let labels: Vec<String> = items.iter().map(|c| c.label().to_string()).collect();
                let hovered = rows.iter().position(|r| r.contains(mouse));
                ui::dropdown(&draw, t, &rows, &labels, None, hovered, &[]);
            }

            if model.zoom_changed.is_some_and(|at| at.elapsed() < ZOOM_INDICATOR_TIMEOUT) {
                let rect = Rect::from_w_h(64.0, 24.0).bottom_right_of(canvas_rect(win)).shift(vec2(-12.0, 12.0));
                ui::rounded(&draw, rect, ui::RADIUS, t.shadow);
                ui::label(&draw, rect, &format!("{:.0}%", model.zoom * 100.0), t.text, 12);
            }
        }
        View::Fullscreen | View::FullscreenOverlay => {
            draw.background().color(BLACK);
            draw_output_signal(&draw, t, win, model);

            if model.view == View::FullscreenOverlay {
                let rect = exit_button_rect(win);
                ui::button(&draw, t, rect, "Exit Fullscreen", rect.contains(mouse));
            }
        }
    }

    if model.settings.show_fps {
        let area = if model.view == View::Config { canvas_rect(win).pad_top(project::BAR_H) } else { win };
        draw_fps(&draw, t, area, &model.fps.label);
    }

    draw.to_frame(app, &render_frame).unwrap();
    model.view_time.set(model.view_time.get() + started.elapsed());
}

fn draw_fps(draw: &Draw, t: &Theme, area: Rect, label: &str) {
    let rect = Rect::from_w_h(150.0, 22.0).top_left_of(area).shift(vec2(10.0, -10.0));
    ui::rounded(draw, rect, ui::RADIUS, t.shadow);
    ui::label_left(draw, rect, label, t.ok, 12);
}

const DEBUG_LOG_LINES: usize = 14;
const DEBUG_LOG_LINE_H: f32 = 14.0;

/// The newest debug-log lines, in a translucent strip above the Start button.
fn draw_debug_log(draw: &Draw, t: &Theme, win: Rect) {
    let canvas = canvas_rect(win);
    let h = DEBUG_LOG_LINES as f32 * DEBUG_LOG_LINE_H + 16.0;
    let panel = Rect::from_w_h(canvas.w() - 32.0, h).mid_bottom_of(canvas).shift_y(84.0);
    ui::rounded(draw, panel, ui::RADIUS + 2.0, rgba8(0, 0, 0, 190));
    for (i, line) in debug_log::recent(DEBUG_LOG_LINES).iter().enumerate() {
        let y = panel.top() - 8.0 - DEBUG_LOG_LINE_H / 2.0 - i as f32 * DEBUG_LOG_LINE_H;
        draw.text(line)
            .xy(pt2(panel.x(), y))
            .wh(vec2(panel.w() - 16.0, DEBUG_LOG_LINE_H))
            .left_justify()
            .no_line_wrap()
            .color(t.text_dim)
            .font_size(11);
    }
}

fn draw_sidebar(draw: &Draw, t: &Theme, win: Rect, model: &Model, hover: Option<Point2>, now: f32) {
    let sidebar = sidebar_rect(win);
    draw.rect().xy(sidebar.xy()).wh(sidebar.wh()).color(t.sidebar);
    let hovered = |r: Rect| hover.is_some_and(|m| r.contains(m));
    let list = sidebar_list_rect(win);

    // The scrolling list first; the fixed header and footer cover whatever
    // scrolls under them.
    for row in sidebar_items(model, win) {
        match row {
            SidebarRow::Header { rect, label, sub: false } => {
                ui::label_left(draw, rect.pad_left(-6.0), &label.to_uppercase(), t.text_dim, 11);
            }
            SidebarRow::Header { rect, label, sub: true } => {
                ui::label_left(draw, rect.pad_left(4.0), &label, t.accent, 11);
            }
            SidebarRow::Hint { rect, text } => {
                draw.text(text)
                    .xy(rect.xy())
                    .wh(rect.pad_left(10.0).wh())
                    .left_justify()
                    .color(t.text_dim)
                    .font_size(11);
            }
            SidebarRow::Module { row_rect, plus_rect, label, action } => {
                let over = hovered(row_rect) && list.contains(hover.unwrap_or_default());
                if over {
                    ui::rounded(draw, row_rect, ui::RADIUS, t.button_hover);
                }
                match action {
                    SidebarAction::Spawn(_) => ui::label_left(draw, row_rect, &label, t.text, 14),
                    SidebarAction::ShaderCategory { .. } | SidebarAction::Example(_) => ui::label_left(draw, row_rect.pad_left(10.0).pad_right(28.0), &label, t.text, 13),
                    SidebarAction::Preset(i) => {
                        let icon = Rect::from_x_y_w_h(row_rect.left() + 18.0, row_rect.y(), 14.0, 11.0);
                        ui::rounded_outline(draw, icon, 3.0, t.accent, 1.5);
                        let text_rect = row_rect.pad_left(18.0).pad_right(28.0);
                        match editing_preset(model, i) {
                            Some(view) => {
                                let caret = (now * 2.0) as i32 % 2 == 0;
                                ui::rounded(draw, text_rect.pad(2.0), ui::RADIUS, t.field);
                                ui::rounded_outline(draw, text_rect.pad(2.0), ui::RADIUS, t.accent, 1.5);
                                ui::edit_label_left(draw, t, text_rect.pad_left(4.0), view, t.field_text, 13, caret);
                            }
                            None => ui::label_left(draw, text_rect, &label, t.text, 13),
                        }
                    }
                }
                ui::rounded(draw, plus_rect, ui::RADIUS - 1.0, if over { t.accent_hover } else { t.accent });
                ui::label(draw, plus_rect, "+", t.accent_text, 16);
            }
        }
    }

    let header = Rect::from_corners(pt2(sidebar.left(), list.top()), sidebar.top_right());
    draw.rect().xy(header.xy()).wh(header.wh()).color(t.sidebar);
    for (i, name) in ["Modules", "Examples"].into_iter().enumerate() {
        let rect = sidebar_tab_rect(win, i);
        ui::pill_sized(draw, t, rect, name, model.examples_tab == (i == 1), hovered(rect), 12);
    }
    let footer = Rect::from_corners(sidebar.bottom_left(), pt2(sidebar.right(), list.bottom()));
    draw.rect().xy(footer.xy()).wh(footer.wh()).color(t.sidebar);
    draw.line()
        .start(pt2(sidebar.right(), sidebar.bottom()))
        .end(pt2(sidebar.right(), sidebar.top()))
        .weight(1.0)
        .color(t.border);

    let save_rect = save_button_rect(win);
    ui::button(draw, t, save_rect, "Save", hovered(save_rect));
    let load_rect = load_button_rect(win);
    ui::button(draw, t, load_rect, "Load", hovered(load_rect));
    let settings_rect = settings_button_rect(win);
    ui::button(draw, t, settings_rect, "Settings", hovered(settings_rect) || model.settings_open);
    let help_rect = help_button_rect(win);
    ui::button(draw, t, help_rect, "Help", hovered(help_rect) || model.help_open);
    let chevron_at = pt2(settings_rect.right() - 16.0, settings_rect.y());
    ui::chevron(draw, chevron_at, 8.0, t.button_text, model.settings_open);

    // WLED, with one light summing up the controllers: green when all are
    // online, amber while connecting/stale, red if any is failing.
    let wled_rect = wled_button_rect(win);
    ui::button(draw, t, wled_rect, "WLED", hovered(wled_rect) || model.wled_open);
    let poll = model.wled.poll_interval();
    let links: Vec<Link> = model.wled.devices().into_iter().filter_map(|a| model.wled.reading(a)).map(|r| Link::of(&r, poll)).collect();
    if !links.is_empty() {
        let worst = links
            .iter()
            .find(|l| matches!(l, Link::Error(_)))
            .or_else(|| links.iter().find(|l| !matches!(l, Link::Online { .. })))
            .unwrap_or(&links[0]);
        draw.ellipse().x_y(wled_rect.right() - 16.0, wled_rect.y()).radius(5.0).color(link_color(t, Some(worst)));
    }

    if let Some((message, is_error, _)) = &model.status {
        // Above the bottom buttons, where it can be long without hitting anything.
        let rect = Rect::from_x_y_w_h(sidebar.x(), wled_rect.top() + 2.0, sidebar.w() - 12.0, 0.0);
        let text = Rect::from_x_y_w_h(rect.x(), rect.y() + 22.0, rect.w(), 40.0);
        ui::rounded(draw, text.pad(-2.0), ui::RADIUS, t.panel);
        draw.text(message)
            .xy(text.xy())
            .wh(text.pad_left(8.0).pad_right(8.0).wh())
            .color(if *is_error { t.error } else { t.ok })
            .font_size(11);
    }
}

fn draw_title_buttons(draw: &Draw, t: &Theme, body: Rect, buttons: &[TitleButton], chrome: &ModuleChrome, hover: Option<Point2>) {
    for (slot, button) in buttons.iter().enumerate() {
        let rect = title_button_rect(body, slot);
        let over = hover.is_some_and(|m| rect.contains(m));
        let (fill, glyph) = match button {
            TitleButton::Delete => (if over { t.danger } else { rgba8(0, 0, 0, 60) }, "\u{00D7}"),
            TitleButton::Minimize => (
                if over { t.button_hover } else { rgba8(0, 0, 0, 60) },
                if chrome.minimized { "+" } else { "\u{2013}" },
            ),
            TitleButton::Preview => (
                if chrome.show_preview { t.accent } else if over { t.button_hover } else { rgba8(0, 0, 0, 60) },
                if chrome.show_preview { "\u{25A0}" } else { "\u{25A1}" },
            ),
        };
        ui::rounded(draw, rect, 5.0, fill);
        ui::label(draw, rect, glyph, t.title_text, 13);
    }
}

fn draw_module(draw: &Draw, t: &Theme, model: &Model, module: &PlacedModule, now: f32, hover: Option<Point2>) {
    let chrome = module.body.chrome();
    let no_signal = source_has_no_signal(module);
    let body = module_body_rect(module);
    let hovered = |r: Rect| hover.is_some_and(|m| r.contains(m));

    // Dragged over a cable it would be spliced into: the box takes the accent color.
    let will_connect = matches!(model.dragging, Some(Drag::Module { id, start, .. }) if id == module.id && insert_candidate(model, id, start).is_some());
    ui::shadow(draw, t, body, MODULE_RADIUS);
    ui::rounded(draw, body, MODULE_RADIUS, if will_connect { t.accent } else { t.module(module.kind) });
    // A slightly darker title band.
    let band_h = if chrome.minimized { MINIMIZED_H } else { 34.0 };
    let band = Rect::from_x_y_w_h(body.x(), body.top() - band_h / 2.0, body.w(), band_h);
    ui::rounded(draw, band, MODULE_RADIUS, rgba8(0, 0, 0, 45));
    let selected = model.selected.contains(&module.id);
    if selected {
        ui::rounded_outline(draw, body.pad(-3.0), MODULE_RADIUS + 3.0, t.accent, 2.5);
    } else {
        ui::rounded_outline(draw, body, MODULE_RADIUS, t.border, 1.0);
    }

    let buttons = title_buttons_for(module.kind);
    let title_rect = Rect::from_x_y_w_h(body.x(), band.y(), body.w(), 24.0).pad_right(buttons.len() as f32 * 24.0 + 8.0);
    let title = match &module.body {
        ModuleBody::Shader(s) => match (&s.file, &s.category) {
            (Some(file), _) => shader_display_name(file),
            (None, Some(category)) => category.clone(),
            (None, None) => kind_label(module.kind).to_string(),
        },
        _ => kind_label(module.kind).to_string(),
    };
    ui::label_left(draw, title_rect, &title, t.title_text, 15);
    draw_title_buttons(draw, t, body, buttons, chrome, hover);

    if !chrome.minimized {
        if has_config_row(module.kind) {
            let config = config_rect(body);
            match &module.body {
                ModuleBody::Camera(cam) => {
                    ui::selector(draw, t, config, &cam.label(&model.available_cameras), hovered(config) || cam.dropdown_open);
                }
                ModuleBody::Video(video) => draw_video_controls(draw, t, model, module.id, video, body, &hovered),
                ModuleBody::Wled(w) => draw_wled_controls(draw, t, model, w, body, &hovered),
                ModuleBody::Osc(osc) => {
                    let (text, color) = match (&osc.error, &osc.address, osc.raw()) {
                        (Some(e), _, _) => (e.clone(), t.error),
                        (None, Some(a), Some(v)) => (format!("{a} {}", fmt_num(v)), t.title_text),
                        _ => ("waiting\u{2026}".to_string(), t.text_dim),
                    };
                    ui::label_left(draw, math_readout_rect(module, body), &text, color, 10);
                }
                ModuleBody::Map(map) => {
                    for (i, label) in MAP_BUTTONS.iter().enumerate() {
                        let rect = map_button_rect(body, i);
                        let on = match i {
                            1 => map.clamp,
                            2 => map.auto,
                            3 => map.center,
                            _ => false,
                        };
                        ui::pill_sized(draw, t, rect, label, on, hovered(rect), 11);
                    }
                    let input = input_signal(&model.modules, &model.connections, module.id, 0, now, &mut vec![module.id]);
                    let output = module_signal(&model.modules, &model.connections, module.id, 0, now, &mut Vec::new());
                    let text = match (&input, &output) {
                        (Some(i), Some(o)) if o.is_many() => format!("{} \u{2192} {}", i.values().len(), fmt_signal(o)),
                        (Some(i), Some(o)) => format!("{} \u{2192} {}", fmt_signal(i), fmt_signal(o)),
                        _ => "no input".to_string(),
                    };
                    let first = output.as_ref().and_then(|o| o.values().first().copied());
                    let v = ValueBox { norm: first.unwrap_or(0.0).clamp(0.0, 1.0), text: &text, driven: true, hovered: false, editing: None, caret: false };
                    ui::value_box(draw, t, math_readout_rect(module, body), v, 12);
                }
                ModuleBody::Shader(shader) => {
                    let label = match (&shader.file, &shader.category) {
                        (Some(file), _) => shader_display_name(file),
                        (None, Some(category)) => format!("Select {category}"),
                        (None, None) => "Select a shader".to_string(),
                    };
                    if shader.file.is_none() || !shader.single_choice {
                        ui::selector(draw, t, config, &label, hovered(config) || shader.dropdown_open);
                    }
                }
                ModuleBody::Mult(_) => {
                    let value = module_signal(&model.modules, &model.connections, module.id, 0, now, &mut Vec::new());
                    let text = match &value {
                        Some(v) => format!("= {}", fmt_signal(v)),
                        None => "= \u{2014}".to_string(),
                    };
                    let first = value.as_ref().and_then(|v| v.values().first().copied());
                    let v = ValueBox { norm: first.unwrap_or(0.0), text: &text, driven: true, hovered: false, editing: None, caret: false };
                    ui::value_box(draw, t, config, v, 13);
                }
                ModuleBody::Select(select) => draw_select_controls(draw, t, select, body, &hovered),
                _ => {}
            }
        }

        if has_mode_row(module.kind) {
            let rect = mode_rect(module.kind, body);
            let (name, open) = match &module.body {
                ModuleBody::Lfo(lfo) => (lfo.wave.name(), lfo.dropdown_open),
                ModuleBody::Blend(blend) => (blend.mode.name(), blend.dropdown_open),
                ModuleBody::Lag(lag) => (lag.mode.name(), lag.dropdown_open),
                _ => ("", false),
            };
            ui::selector(draw, t, rect, name, hovered(rect) || open);
        }
        if let ModuleBody::Lfo(lfo) = &module.body {
            let center = lfo_center_rect(body);
            ui::pill(draw, t, center, "\u{00B1}", lfo.centered, hovered(center));
        }

        for slot in param_slots(module) {
            let Some(pv) = param_view(model, module, slot, now) else { continue };
            let rect = param_rect(module, slot);
            let target = ParamTarget { module: module.id, slot };
            // Enum properties are dropdowns (unless a cable drives them).
            if !pv.driven && is_enum_param(model, target) {
                let open = matches!(&module.body, ModuleBody::Shader(s) if s.prop_dropdown == Some(match slot { Slot::Prop(i) => i, _ => usize::MAX }));
                ui::selector_sized(draw, t, rect, &pv.text, hovered(rect) || open, 11);
                continue;
            }
            let editing = editing_text(model, target, EditField::Value);
            let v = ValueBox {
                norm: pv.param.norm(pv.shown),
                text: &pv.text,
                driven: pv.driven,
                hovered: hovered(rect) || model.param_menu == Some(target),
                editing,
                caret: (now * 2.0) as i32 % 2 == 0,
            };
            ui::value_box(draw, t, rect, v, if slot == Slot::Main { 13 } else { 11 });
        }

        if chrome.show_preview {
            let preview = preview_rect(body, module.base_height(), has_config_row(module.kind));
            draw_preview(draw, t, preview, chrome.preview_texture.as_ref(), no_signal);
        }
    } else if chrome.show_preview {
        let preview = Rect::from_x_y_w_h(body.x(), body.top() - MINIMIZED_H - 4.0 - MINI_PREVIEW_H / 2.0, body.w() - 24.0, MINI_PREVIEW_H);
        draw_preview(draw, t, preview, chrome.preview_texture.as_ref(), no_signal);
    }

    for (output, pos) in output_ports(module) {
        let Some(port_type) = port_signal(model, module.id, output) else { continue };
        let connected = model.connections.iter().any(|c| c.from == module.id && c.from_output == output);
        let mark = if port_type == PortType::Numbers { ArrayMark::Carries } else { ArrayMark::None };
        draw_port(draw, t, pos, port_type, mark, connected, hover.is_some_and(|m| m.distance(pos) <= PORT_HIT_RADIUS));
    }

    let types = module.input_types();
    for (i, port_type) in types.iter().enumerate() {
        let pos = module_input_port(module, body, i, types.len());
        let source = input_port(&model.connections, module.id, i);
        if multi_input::is_multi(module.kind, i) {
            let layers = multi_input::sources(&model.connections, module.id, i).len();
            draw_pill(draw, t, body, *port_type, if chrome.minimized { 0 } else { layers }, hover.is_some_and(|m| pill_rect(body).contains(m)));
            continue;
        }
        // An input that takes arrays wears a faint ring; a solid one while an array is plugged in.
        let mark = match (port_type, source) {
            (PortType::Numbers, Some((from, out))) if port_signal(model, from, out) == Some(PortType::Numbers) => ArrayMark::Carries,
            (PortType::Numbers, _) => ArrayMark::Accepts,
            _ => ArrayMark::None,
        };
        draw_port(draw, t, pos, *port_type, mark, source.is_some(), hover.is_some_and(|m| m.distance(pos) <= PORT_HIT_RADIUS));
    }

    if let (ModuleBody::Shader(shader), false) = (&module.body, chrome.minimized) {
        for n in 0..shader.channels() {
            let port = module_input_port(module, body, n, types.len());
            let rect = Rect::from_corners(pt2(body.left() + 20.0, port.y - 9.0), pt2(body.right() - 12.0, port.y + 9.0));
            ui::label_left(draw, rect.pad_left(-10.0), &if shader.channels() == 1 { "Imagen In".to_string() } else { format!("Imagen In {}", n + 1) }, t.title_text, 11);
        }
        if let Some(error) = &shader.error {
            // Sits under the ports; the full text is also readable by
            // fixing the file, since the shader reloads on save.
            let ports_bottom = body.top() - SHADER_PORT_TOP + module.port_shift() - types.len() as f32 * SHADER_PORT_STEP;
            draw.text(error)
                .xy(pt2(body.x(), ports_bottom - 30.0))
                .wh(vec2(body.w() - 16.0, 56.0))
                .left_justify()
                .color(t.error)
                .font_size(10);
        }
    }
}

fn draw_wled_controls(draw: &Draw, t: &Theme, model: &Model, w: &WledState, body: Rect, hovered: &dyn Fn(Rect) -> bool) {
    let config = config_rect(body);
    let devices = wled_devices(model);
    let label = match w.device {
        Some(addr) => devices.iter().find(|(a, _)| *a == addr).map_or_else(|| format!("{addr} (removed)"), |(_, n)| n.clone()),
        None if devices.is_empty() => "Add a controller\u{2026}".into(),
        None => "Select a controller".into(),
    };
    ui::selector(draw, t, config, &label, hovered(config) || w.dropdown_open);

    // The status sits on a dark box, wrapped inside it, so a long error
    // stays readable and never spills out of the module.
    let status = wled_status_rect(body, w.status.len());
    ui::rounded(draw, status, ui::RADIUS, rgba8(20, 22, 25, 225));
    let first_line = status.top() - 3.0 - WLED_STATUS_LINE_H / 2.0;
    draw.ellipse().xy(pt2(status.left() + 9.0, first_line)).radius(4.0).color(link_color(t, w.link.as_ref()));
    let color = if w.is_error() { t.error } else { rgba8(235, 236, 238, 255) };
    for (k, line) in w.status.iter().enumerate() {
        let y = first_line - k as f32 * WLED_STATUS_LINE_H;
        let rect = Rect::from_corners(pt2(status.left() + 18.0, y - 7.0), pt2(status.right() - 4.0, y + 7.0));
        draw.text(line).xy(rect.xy()).wh(rect.wh()).left_justify().no_line_wrap().color(color).font_size(modules::sources::wled::STATUS_FONT);
    }

    let top = wled_rows_top(w);
    if w.inputs.is_empty() {
        let row = wled_input_rect(body, top, 0);
        let hint = if w.link.is_some() { "no inputs configured on the device" } else { "" };
        ui::label_left(draw, row, hint, t.title_text, 10);
    }
    for (i, input) in w.inputs.iter().enumerate() {
        let row = wled_input_rect(body, top, i);
        ui::rounded(draw, row, ui::RADIUS, rgba8(0, 0, 0, 35));
        // Activity light: lit while the input moves or is pressed.
        let light = pt2(row.left() + 10.0, row.y());
        if input.active {
            draw.ellipse().xy(light).radius(7.0).color(rgba8(255, 255, 255, 40));
        }
        draw.ellipse().xy(light).radius(4.0).color(if input.active { t.ok } else { rgba8(0, 0, 0, 120) });
        let name = Rect::from_corners(pt2(row.left() + 18.0, row.bottom()), pt2(row.left() + 108.0, row.top()));
        ui::label_left(draw, name, &modules::sources::wled::input_label(input.gpio, input.kind), t.title_text, 10);
        let bar = Rect::from_corners(pt2(row.left() + 112.0, row.y() - 3.0), pt2(row.right() - 40.0, row.y() + 3.0));
        ui::rounded(draw, bar, 3.0, rgba8(0, 0, 0, 90));
        let fill_w = bar.w() * input.value.clamp(0.0, 1.0);
        if fill_w >= 1.0 {
            let fill = Rect::from_x_y_w_h(bar.left() + fill_w / 2.0, bar.y(), fill_w, bar.h());
            ui::rounded(draw, fill, 3.0, t.accent);
        }
        let raw = Rect::from_corners(pt2(row.right() - 38.0, row.bottom()), row.top_right());
        draw.text(&input.raw.to_string()).xy(raw.xy()).wh(raw.wh()).right_justify().no_line_wrap().color(t.title_text).font_size(10);
    }
}

fn draw_select_controls(draw: &Draw, t: &Theme, select: &modules::math::select::SelectState, body: Rect, hovered: &dyn Fn(Rect) -> bool) {
    for (k, label) in ["Split", "Combine"].iter().enumerate() {
        let rect = select_mode_rect(body, k);
        ui::pill_sized(draw, t, rect, label, select.combine == (k == 1), hovered(rect), 12);
    }
    if select.values.is_empty() {
        let hint = if select.pending_gpio.is_some() { "waiting for the device\u{2026}" } else { "plug numbers or an array in" };
        ui::label_left(draw, select_row_rect(body, 0), hint, t.title_text, 11);
    }
    let packing = select.combine && select.selected.iter().all(|x| !x);
    for (e, value) in select.values.iter().enumerate() {
        let row = select_row_rect(body, e);
        let picked = select.is_selected(e);
        // Picked: tinted with the accent, outlined, a filled mark and bright
        // text. Not picked: plain, a hollow mark and dimmer text.
        let fill = if picked {
            let mut tint = t.accent;
            tint.alpha = 75;
            tint
        } else if hovered(row) {
            rgba8(0, 0, 0, 70)
        } else {
            rgba8(0, 0, 0, 35)
        };
        ui::rounded(draw, row, ui::RADIUS, fill);
        if picked {
            ui::rounded_outline(draw, row, ui::RADIUS, t.accent, 2.5);
        }
        let mark = pt2(row.left() + 11.0, row.y());
        if picked {
            draw.ellipse().xy(mark).radius(5.0).color(t.accent).stroke(rgba8(255, 255, 255, 230)).stroke_weight(1.5);
        } else {
            draw.ellipse().xy(mark).radius(4.5).no_fill().stroke(rgba8(255, 255, 255, 120)).stroke_weight(1.5);
        }
        let text_color = if picked { rgba8(255, 255, 255, 255) } else { rgba8(255, 255, 255, 165) };
        let label = select.labels.get(e).cloned().unwrap_or_else(|| (e + 1).to_string());
        let name = Rect::from_corners(pt2(row.left() + 20.0, row.bottom()), pt2(row.left() + 108.0, row.top()));
        draw.text(&label).xy(name.xy()).wh(name.wh()).left_justify().no_line_wrap().color(text_color).font_size(12);
        let bar = Rect::from_corners(pt2(row.left() + 110.0, row.y() - 3.0), pt2(row.right() - 36.0, row.y() + 3.0));
        ui::rounded(draw, bar, 3.0, rgba8(0, 0, 0, 90));
        let fill_w = bar.w() * value.clamp(0.0, 1.0);
        if fill_w >= 1.0 {
            let fill = Rect::from_x_y_w_h(bar.left() + fill_w / 2.0, bar.y(), fill_w, bar.h());
            ui::rounded(draw, fill, 3.0, if picked || packing { t.accent } else { t.fill_driven });
        }
        let text = Rect::from_corners(pt2(row.right() - 34.0, row.bottom()), pt2(row.right() - 4.0, row.top()));
        draw.text(&fmt_num(*value)).xy(text.xy()).wh(text.wh()).right_justify().no_line_wrap().color(text_color).font_size(11);
    }
}

fn draw_video_controls(draw: &Draw, t: &Theme, model: &Model, id: u64, video: &VideoState, body: Rect, hovered: &dyn Fn(Rect) -> bool) {
    let config = config_rect(body);
    let label = video.file_name().unwrap_or_else(|| "Open video\u{2026}".into());
    ui::selector(draw, t, config, &label, hovered(config));

    let play = video_play_rect(body);
    ui::pill(draw, t, play, "", video.playing, hovered(play));
    let color = if video.playing { t.accent_text } else { t.field_text };
    if video.playing {
        for dx in [-3.5, 3.5] {
            draw.rect().x_y(play.x() + dx, play.y()).w_h(3.5, 12.0).color(color);
        }
    } else {
        let c = play.xy();
        draw.polygon().color(color).points([c + vec2(-4.0, 7.0), c + vec2(-4.0, -7.0), c + vec2(7.0, 0.0)]);
    }

    let loop_rect = video_loop_rect(body);
    if !video.is_still() {
        ui::selector_sized(draw, t, loop_rect, video.looping.name(), hovered(loop_rect) || video.dropdown_open, 11);
    }

    let time = video_time_rect(body);
    let error = video.worker.as_ref().and_then(|w| w.error());
    match (&error, video.path.is_some(), video.info) {
        (Some(err), _, _) => ui::label_left(draw, time, err, t.error, 10),
        (None, true, None) => ui::label_left(draw, time, "loading\u{2026}", t.text_dim, 11),
        (None, _, Some(info)) => {
            ui::label_left(draw, time, &format!("{} / {}", fmt_time(video.position), fmt_time(info.duration)), t.title_text, 11)
        }
        (None, false, None) => {}
    }

    // Position bar: dimmer while a cable drives the scrub input.
    let bar = video_scrub_rect(body);
    let driven = input_source(&model.connections, id, 1).is_some();
    ui::rounded(draw, bar, 5.0, rgba8(0, 0, 0, 70));
    let fraction = if video.duration() > 0.0 { (video.position / video.duration()).clamp(0.0, 1.0) as f32 } else { 0.0 };
    let fill_w = bar.w() * fraction;
    if fill_w >= 1.0 {
        let fill = Rect::from_x_y_w_h(bar.left() + fill_w / 2.0, bar.y(), fill_w, bar.h());
        ui::rounded(draw, fill, 5.0, if driven { t.fill_driven } else { t.fill });
    }
    let knob = hovered(bar.pad(-4.0)) || matches!(model.dragging, Some(Drag::Scrub { id: d }) if d == id);
    draw.ellipse().x_y(bar.left() + fill_w, bar.y()).radius(if knob { 7.0 } else { 5.5 }).color(t.field_hover);
}

/// Ports are always drawn in their signal-type color, so the type is
/// visible regardless of connection state: connected ports are solid,
/// unconnected ones are rings. Hovered ports grow.
/// How a number port shows arrays: a ring around it (solid while one
/// flows through, faint on an input that could take one).
#[derive(Clone, Copy, PartialEq)]
enum ArrayMark {
    None,
    Accepts,
    Carries,
}

fn draw_port(draw: &Draw, t: &Theme, pos: Point2, port_type: PortType, mark: ArrayMark, connected: bool, hovered: bool) {
    let color = t.port(port_type);
    let r = if hovered { PORT_RADIUS + 3.0 } else { PORT_RADIUS };
    match mark {
        ArrayMark::None => {}
        ArrayMark::Accepts => {
            let mut faint = color;
            faint.alpha = 110;
            draw.ellipse().xy(pos).radius(r + 4.0).no_fill().stroke(faint).stroke_weight(1.0);
        }
        ArrayMark::Carries => {
            draw.ellipse().xy(pos).radius(r + 4.5).no_fill().stroke(t.port_ring).stroke_weight(3.5);
            draw.ellipse().xy(pos).radius(r + 4.5).no_fill().stroke(color).stroke_weight(2.0);
        }
    }
    if connected {
        draw.ellipse().xy(pos).radius(r).color(color).stroke(t.port_ring).stroke_weight(2.0);
    } else {
        draw.ellipse().xy(pos).radius(r).color(t.port_ring).stroke(color).stroke_weight(3.0);
    }
}

// --- Help ----------------------------------------------------------------------

enum HelpRow {
    Section(&'static str),
    /// A shortcut or gesture, and what it does.
    Item(&'static str, &'static str),
}

use HelpRow::{Item, Section};

const HELP_LEFT: &[HelpRow] = &[
    Section("Canvas"),
    Item("Drag a module", "Move it (selection follows)"),
    Item("Shift+click", "Add / remove from selection"),
    Item("Drag on empty", "Marquee select"),
    Item("Middle / Shift+drag", "Pan"),
    Item("Wheel / Cmd+scroll", "Zoom at the cursor"),
    Item("Trackpad scroll", "Pan"),
    Item("+  -  0", "Zoom in / out / 100%"),
    Item("H", "Center the view"),
    Item("L", "Auto layout"),
    Item("Right-click", "Canvas menu"),
    Section("Modules"),
    Item("Cmd+C / X / V", "Copy / cut / paste"),
    Item("D or Cmd+D", "Duplicate"),
    Item("Del", "Delete (cables rejoin)"),
    Item("Cmd+G", "Group (+Shift: ungroup)"),
    Item("Cmd+A", "Select all"),
    Item("P  /  M", "Previews / minimize all"),
    Item("F", "Fullscreen"),
    Item("Esc", "Deselect, close popups"),
    Section("Cables"),
    Item("Drag from output", "Connect to an input"),
    Item("Drop on a cable", "Replace that cable"),
    Item("Move node on cable", "Insert it in the path"),
    Item("Click the Stack pill", "Order its layers"),
    Item("Ringed port / double cable", "Carries an array"),
    Item("Select rows", "Pick: one output each"),
];

const HELP_RIGHT: &[HelpRow] = &[
    Section("Number boxes"),
    Item("Drag", "Change the value"),
    Item("Shift+drag", "Fine adjustment"),
    Item("Click", "Type a value"),
    Item("Right-click", "Range, clamp, reset"),
    Item("Cable in", "Drives the box (0..1)"),
    Section("Text fields"),
    Item("Click / drag", "Place caret / select"),
    Item("Double-click", "Select all"),
    Item("Shift+arrows", "Extend the selection"),
    Item("Home / End", "Jump to start / end"),
    Item("Cmd+A C X V", "Select all, copy, cut, paste"),
    Item("Enter / Esc", "Confirm / cancel"),
    Section("Projects"),
    Item("Tabs", "Each project is independent"),
    Item("+  /  x", "New / close project"),
    Item("Double-click a tab", "Rename (or right-click)"),
    Item("Examples tab", "Open one as a new project"),
    Item("Load", "Opens in a new tab"),
    Item("Save", "Saves the active project"),
    Section("Help"),
    Item("F1", "Show / hide this panel"),
];

fn help_panel_rect(win: Rect) -> Rect {
    Rect::from_w_h(640.0, 560.0).top_right_of(canvas_rect(win)).shift(vec2(-16.0, -16.0 - project::BAR_H))
}

fn draw_help_panel(draw: &Draw, t: &Theme, win: Rect) {
    let panel = help_panel_rect(win);
    ui::panel(draw, t, panel, "Help \u{00B7} shortcuts and gestures");
    let col_w = (panel.w() - 36.0) / 2.0;
    for (c, rows) in [HELP_LEFT, HELP_RIGHT].into_iter().enumerate() {
        let left = panel.left() + 12.0 + c as f32 * (col_w + 12.0);
        let mut y = panel.top() - 48.0;
        for row in rows {
            match row {
                Section(title) => {
                    y -= 8.0;
                    let r = Rect::from_corners(pt2(left, y - 18.0), pt2(left + col_w, y));
                    ui::label_left(draw, r.pad_left(-10.0), &title.to_uppercase(), t.accent, 11);
                    y -= 20.0;
                }
                Item(keys, what) => {
                    let r = Rect::from_corners(pt2(left, y - 16.0), pt2(left + col_w, y));
                    let keys_r = Rect::from_corners(r.bottom_left(), pt2(left + 128.0, r.top()));
                    let what_r = Rect::from_corners(pt2(left + 128.0, r.bottom()), r.top_right());
                    ui::label_left(draw, keys_r.pad_left(-10.0), keys, t.text, 11);
                    ui::label_left(draw, what_r.pad_left(-10.0), what, t.text_dim, 11);
                    y -= 17.0;
                }
            }
        }
    }
}

fn draw_settings_panel(draw: &Draw, t: &Theme, win: Rect, model: &Model, hover: Option<Point2>) {
    let settings = &model.settings;
    let panel = settings_panel_rect(win, model.debug_open);
    ui::panel(draw, t, panel, "Settings");
    let hovered = |r: Rect| hover.is_some_and(|m| r.contains(m));

    let range = Settings::SPEED_MAX - Settings::SPEED_MIN;
    let sliders = [("Scroll speed", settings.scroll_speed), ("Drag speed", settings.drag_speed)];
    for (index, (label, value)) in sliders.iter().enumerate() {
        let rect = settings_slider_rect(win, index);
        let text = format!("{label} {value:.2}\u{00D7}");
        let v = ValueBox { norm: (value - Settings::SPEED_MIN) / range, text: &text, driven: false, hovered: hovered(rect), editing: None, caret: false };
        ui::value_box(draw, t, rect, v, 12);
    }

    for (row, kind) in settings_rows(model.debug_open).iter().enumerate() {
        let rect = settings_row_rect(win, row);
        let over = hovered(rect);
        match kind {
            SettingsRow::ShowFps => ui::toggle(draw, t, rect, "Show FPS", settings.show_fps, over),
            SettingsRow::ExitButton => ui::toggle(draw, t, rect, "Exit button in fullscreen", settings.show_exit_button, over),
            SettingsRow::RestoreSession => ui::toggle(draw, t, rect, "Open last session on start", settings.restore_session, over),
            SettingsRow::StartFullscreen => ui::toggle(draw, t, rect, "Start in fullscreen", settings.start_fullscreen, over),
            SettingsRow::ProjectMode => ui::toggle(draw, t, rect, "Project mode (off: sample)", settings.project_mode, over),
            SettingsRow::Palette => {
                ui::label_left(draw, rect, "Palette", t.text, 12);
                let selector = rect.pad_left(110.0);
                ui::selector(draw, t, selector, settings.palette.name(), hovered(selector) || model.palette_open);
            }
            SettingsRow::DebugSection => {
                let color = if over { t.text } else { t.text_dim };
                ui::chevron(draw, pt2(rect.left() + 14.0, rect.y()), 8.0, color, model.debug_open);
                ui::label_left(draw, rect.pad_left(14.0), "Debug", color, 12);
            }
            SettingsRow::DebugLog => ui::toggle(draw, t, rect, "Debug log", settings.debug_log, over),
            SettingsRow::SerializeGrabs => {
                ui::toggle(draw, t, rect, "Serialize camera grabs", settings.serialize_camera_grabs, over)
            }
        }
    }
}

/// A number box's options: its value, its range, whether it clamps.
fn draw_param_menu(draw: &Draw, t: &Theme, win: Rect, model: &Model, mouse: Point2, now: f32) {
    let Some((target, layout)) = param_menu_layout(model, win) else { return };
    let Some(module) = model.modules.iter().find(|m| m.id == target.module) else { return };
    let Some(param) = param_of(module, target.slot) else { return };
    ui::panel(draw, t, layout.panel, &param_title(module, target.slot));

    let caret = (now * 2.0) as i32 % 2 == 0;
    let fields = [
        (layout.value, "Value", EditField::Value, param.value),
        (layout.min, "Min", EditField::Min, param.min),
        (layout.max, "Max", EditField::Max, param.max),
    ];
    for (rect, label, field, value) in fields {
        let label_rect = Rect::from_corners(pt2(layout.panel.left() + 6.0, rect.bottom()), pt2(rect.left(), rect.top()));
        ui::label_left(draw, label_rect, label, t.text_dim, 12);
        let text = fmt_num(value);
        let norm = if field == EditField::Value { param.norm(param.value) } else { 0.0 };
        let v = ValueBox {
            norm,
            text: &text,
            driven: false,
            hovered: rect.contains(mouse),
            editing: editing_text(model, target, field),
            caret,
        };
        ui::value_box(draw, t, rect, v, 12);
    }
    ui::toggle(draw, t, layout.clamp, "Clamp to range", param.clamp, layout.clamp.contains(mouse));
    ui::button(draw, t, layout.reset, "Reset", layout.reset.contains(mouse));
}

/// One row of a shader node's file dropdown.
#[derive(Clone)]
enum ShaderRow {
    None,
    Header(String),
    File(String),
}

/// "None", then each category heading followed by the files whose role the
/// node lists (the library is sorted by category).
fn shader_rows(model: &Model, role: ShaderRole, only_category: Option<&str>) -> Vec<ShaderRow> {
    let mut rows = vec![ShaderRow::None];
    let mut category: Option<&str> = None;
    for info in model.shader_library.iter().filter(|i| role.lists(i.role) && only_category.map_or(true, |c| i.category == c)) {
        if category != Some(info.category.as_str()) {
            category = Some(&info.category);
            rows.push(ShaderRow::Header(info.category.clone()));
        }
        rows.push(ShaderRow::File(info.file.clone()));
    }
    rows
}

/// Row labels for a module's open dropdown: "None" first, then each choice.
fn dropdown_labels(model: &Model, module: &PlacedModule) -> Vec<String> {
    match module.body {
        ModuleBody::Lfo(_) => return Wave::ALL.iter().map(|w| w.name().to_string()).collect(),
        ModuleBody::Blend(_) => return BlendMode::ALL.iter().map(|m| m.name().to_string()).collect(),
        ModuleBody::Lag(_) => return LagMode::ALL.iter().map(|m| m.name().to_string()).collect(),
        ModuleBody::Video(_) => return LoopMode::ALL.iter().map(|m| m.name().to_string()).collect(),
        ModuleBody::Shader(ref s) => {
            if let Some(prop) = s.prop_dropdown.and_then(|i| s.props().get(i)) {
                return prop.options.clone();
            }
        }
        _ => {}
    }
    let choices: Vec<String> = match &module.body {
        ModuleBody::Camera(_) => model.available_cameras.iter().map(|c| c.human_name().to_string()).collect(),
        ModuleBody::Wled(_) => wled_devices(model).into_iter().map(|(_, name)| name).collect(),
        _ => Vec::new(),
    };
    if let (ModuleBody::Shader(sh), Some(role)) = (&module.body, module.kind.shader_role()) {
        return shader_rows(model, role, sh.category.as_deref())
            .into_iter()
            .map(|r| match r {
                ShaderRow::None => "None".to_string(),
                ShaderRow::Header(c) => format!("{}{c}", ui::HEADING),
                ShaderRow::File(f) => shader_display_name(&f),
            })
            .collect();
    }
    std::iter::once("None".to_string()).chain(choices).collect()
}

/// The row of the current choice in `dropdown_labels`.
fn dropdown_selected(model: &Model, module: &PlacedModule) -> usize {
    match &module.body {
        ModuleBody::Camera(cam) => cam.selected.map_or(0, |i| i + 1),
        ModuleBody::Lfo(lfo) => Wave::ALL.iter().position(|w| *w == lfo.wave).unwrap_or(0),
        ModuleBody::Blend(blend) => BlendMode::ALL.iter().position(|m| *m == blend.mode).unwrap_or(0),
        ModuleBody::Lag(lag) => LagMode::ALL.iter().position(|m| *m == lag.mode).unwrap_or(0),
        ModuleBody::Video(v) => LoopMode::ALL.iter().position(|m| *m == v.looping).unwrap_or(0),
        ModuleBody::Wled(w) => w.device.and_then(|d| wled_devices(model).iter().position(|(a, _)| *a == d)).map_or(0, |i| i + 1),
        ModuleBody::Shader(s) if s.prop_dropdown.is_some() => {
            let i = s.prop_dropdown.unwrap_or(0);
            s.prop_value(i, None) as usize
        }
        ModuleBody::Shader(s) => s
            .file
            .as_ref()
            .and_then(|f| shader_rows(model, module.kind.shader_role()?, s.category.as_deref()).iter().position(|r| matches!(r, ShaderRow::File(l) if l == f)))
            .unwrap_or(0),
        _ => 0,
    }
}

/// A camera or video that stopped delivering frames.
fn source_has_no_signal(module: &PlacedModule) -> bool {
    match &module.body {
        ModuleBody::Camera(cam) => cam.no_signal(),
        ModuleBody::Video(video) => video.no_signal(),
        _ => false,
    }
}

/// `no_signal` dims the (stale) frame under a "No Signal" caption.
fn draw_preview(draw: &Draw, t: &Theme, rect: Rect, texture: Option<&gpu::DrawTex>, no_signal: bool) {
    ui::rounded(draw, rect, ui::RADIUS, BLACK.into_format().into());
    match texture {
        Some(texture) => {
            let [img_w, img_h] = texture.view.size();
            let scale = (rect.w() / img_w as f32).min(rect.h() / img_h as f32);
            draw.texture(&texture.view)
                .xy(rect.xy())
                .w_h(img_w as f32 * scale, img_h as f32 * scale);
        }
        None => ui::label(draw, rect, "No Signal", t.text_dim, 13),
    }
    if no_signal && texture.is_some() {
        ui::rounded(draw, rect, ui::RADIUS, rgba8(0, 0, 0, 170));
        ui::label(draw, rect, "No Signal", t.text, 13);
    }
    ui::rounded_outline(draw, rect, ui::RADIUS, t.border, 1.0);
}

/// The world-space ends of a connection's cable, or `None` when it isn't
/// drawn. Cables to modules inside a collapsed group attach to the group's
/// box; ones between two hidden members aren't drawn.
fn cable_ends(model: &Model, c: &Connection) -> Option<(Point2, Point2)> {
    let from = model.modules.iter().find(|m| m.id == c.from)?;
    let to = model.modules.iter().find(|m| m.id == c.to)?;
    let to_inputs = to.input_types().len();
    if c.to_input >= to_inputs {
        return None;
    }
    let collapsed_frame = |id: u64| {
        model.group_of(id).filter(|g| g.collapsed).and_then(|g| model.group_frame(g).map(|f| (g.id, f)))
    };
    let (from_group, to_group) = (collapsed_frame(from.id), collapsed_frame(to.id));
    if let (Some((a, _)), Some((b, _))) = (from_group, to_group) {
        if a == b {
            return None;
        }
    }
    let start = from_group.map_or_else(|| output_port_pos(from, c.from_output), |(_, f)| group::collapsed_port(f, true));
    let end = to_group.map_or_else(
        || {
            let body = module_body_rect(to);
            if multi_input::is_multi(to.kind, c.to_input) && !is_minimized_body(body) {
                let layers = multi_input::source_ports(&model.connections, to.id, c.to_input);
                let k = layers.iter().position(|s| *s == (c.from, c.from_output)).unwrap_or(0);
                return pill_slot(body, k, layers.len());
            }
            module_input_port(to, body, c.to_input, to_inputs)
        },
        |(_, f)| group::collapsed_port(f, false),
    );
    Some((start, end))
}

fn draw_cables(draw: &Draw, t: &Theme, model: &Model, mouse: Point2) {
    // While a module is dragged over a cable it could be inserted into, that
    // cable lights up.
    let candidate = match model.dragging {
        Some(Drag::Module { id, start, .. }) => insert_candidate(model, id, start).map(|(index, _)| index),
        // A cable being dragged lights up the cable it would replace.
        Some(Drag::Cable { from, output }) if !over_any_input_port(model, from, output, mouse) => cable_drop_target(model, from, output, mouse),
        _ => None,
    };
    for (index, c) in model.connections.iter().enumerate() {
        let Some((start, end)) = cable_ends(model, c) else { continue };
        let signal = port_signal(model, c.from, c.from_output);
        let color = if candidate == Some(index) { t.accent } else { signal.map_or(t.text_dim, |p| t.port(p)) };
        draw_cable(draw, t, start, end, color, signal == Some(PortType::Numbers));
    }
}

/// How far (world units) a dragged module must travel before releasing it
/// over a cable inserts it, so a plain click never rewires anything.
const INSERT_MIN_DRAG: f32 = 6.0;

/// The cable a dragged module would be inserted into, and which of its
/// inputs takes the signal: the module must be the only one moving, sit on
/// the cable, and have an unplugged input and an output of the cable's type.
/// How close (world units) a dragged cable's end must be to another cable to replace it.
const CABLE_DROP_RADIUS: f32 = 14.0;

fn over_any_input_port(model: &Model, from: u64, output: usize, world: Point2) -> bool {
    let Some(from_type) = port_signal(model, from, output) else { return false };
    model.modules.iter().filter(|m| !model.hidden(m.id)).any(|m| {
        let types = m.input_types();
        let body = module_body_rect(m);
        types.iter().enumerate().any(|(i, ty)| ty.accepts(from_type) && port_hit(m, body, i, types.len(), world))
    })
}

/// The existing cable (index into `connections`) that a cable dragged from
/// `from`'s output `output` and dropped at `world` would replace: the nearest
/// one under the pointer whose input takes the same signal, if the swap
/// wouldn't loop back.
fn cable_drop_target(model: &Model, from: u64, output: usize, world: Point2) -> Option<usize> {
    let from_type = port_signal(model, from, output)?;
    let mut best: Option<(usize, f32)> = None;
    for (index, c) in model.connections.iter().enumerate() {
        if c.from == from || c.to == from {
            continue;
        }
        let Some((start, end)) = cable_ends(model, c) else { continue };
        let input = model.modules.iter().find(|m| m.id == c.to).and_then(|m| m.input_types().get(c.to_input).copied());
        if !input.is_some_and(|i| i.accepts(from_type)) {
            continue;
        }
        if reaches(&model.connections, c.to, from, &mut Vec::new()) {
            continue;
        }
        let points = cable_points(start, end);
        let distance = points
            .windows(2)
            .map(|w| {
                let (a, b) = (w[0], w[1]);
                let ab = b - a;
                let t = if ab.length_squared() > 0.0 { ((world - a).dot(ab) / ab.length_squared()).clamp(0.0, 1.0) } else { 0.0 };
                world.distance(a + ab * t)
            })
            .fold(f32::MAX, f32::min);
        if distance <= CABLE_DROP_RADIUS && best.map_or(true, |(_, d)| distance < d) {
            best = Some((index, distance));
        }
    }
    best.map(|(index, _)| index)
}

fn insert_candidate(model: &Model, id: u64, start: Point2) -> Option<(usize, usize)> {
    if model.selected.iter().any(|s| *s != id) || model.hidden(id) {
        return None;
    }
    let module = model.modules.iter().find(|m| m.id == id)?;
    // A Select has no output until something in it is picked.
    if module.kind == ModuleKind::Select {
        return None;
    }
    if module.pos.distance(start) < INSERT_MIN_DRAG {
        return None;
    }
    let body = module_body_rect(module);
    let inputs = module.input_types();
    // (connection, input of the module, distance from the module's center)
    let mut best: Option<(usize, usize, f32)> = None;
    for (index, c) in model.connections.iter().enumerate() {
        if c.from == id || c.to == id {
            continue;
        }
        let Some((cable_start, cable_end)) = cable_ends(model, c) else { continue };
        let Some(signal) = port_signal(model, c.from, c.from_output) else { continue };
        let Some(input) = (0..inputs.len()).find(|&n| inputs[n].accepts(signal) && input_source(&model.connections, id, n).is_none())
        else {
            continue;
        };
        // What the module would send on: a math module passes an array on as one.
        let sends = match output_type(module.kind) {
            Some(PortType::Number) if inputs[input] == PortType::Numbers => signal,
            other => match other {
                Some(o) => o,
                None => continue,
            },
        };
        let takes = model.modules.iter().find(|m| m.id == c.to).and_then(|m| m.input_types().get(c.to_input).copied());
        if !takes.is_some_and(|t| t.accepts(sends)) {
            continue;
        }
        // Wiring `from -> module` must not close a loop through the module.
        if reaches(&model.connections, id, c.from, &mut Vec::new()) {
            continue;
        }
        let nearest = cable_points(cable_start, cable_end)
            .into_iter()
            .filter(|p| body.contains(*p))
            .map(|p| p.distance(body.xy()))
            .fold(f32::MAX, f32::min);
        if nearest < f32::MAX && best.map_or(true, |(_, _, d)| nearest < d) {
            best = Some((index, input, nearest));
        }
    }
    best.map(|(index, input, _)| (index, input))
}

/// Splits connection `index` (`a -> b`) into `a -> module` (at `input`) and
/// `module -> b`.
fn insert_into_cable(model: &mut Model, id: u64, index: usize, input: usize) {
    let Connection { from, from_output, to, to_input } = model.connections.remove(index);
    model.connections.push(Connection { from, from_output, to: id, to_input: input });
    model.connections.push(Connection { from: id, from_output: 0, to, to_input });
    drop_mismatched(&model.modules, &mut model.connections);
}

/// Modules dropped with their center inside an open group's frame join it
/// (the frame grows to hold them). Groups that move as a whole keep theirs.
fn join_group_under(model: &mut Model, moved: &[u64]) {
    let mut joins: Vec<(u64, Vec<u64>)> = Vec::new();
    for id in moved {
        let Some(center) = model.modules.iter().find(|m| m.id == *id).map(|m| module_body_rect(m).xy()) else { continue };
        if model.group_of(*id).is_some_and(|g| g.members.iter().all(|m| moved.contains(m))) {
            continue;
        }
        let target = model.groups.iter().rev().find(|g| {
            !g.collapsed && !g.members.contains(id) && model.group_frame(g).is_some_and(|f| f.contains(center))
        });
        if let Some(g) = target {
            match joins.iter_mut().find(|(gid, _)| *gid == g.id) {
                Some((_, ids)) => ids.push(*id),
                None => joins.push((g.id, vec![*id])),
            }
        }
    }
    for (gid, ids) in joins {
        for g in &mut model.groups {
            g.members.retain(|m| !ids.contains(m));
        }
        if let Some(g) = model.groups.iter_mut().find(|g| g.id == gid) {
            g.members.extend(ids);
        }
    }
    model.groups.retain(|g| !g.members.is_empty());
}

/// After dropping modules, shoves any module they overlap sideways (away
/// from the dropped one), chaining on to whatever those then overlap.
fn push_overlapping(model: &mut Model, moved: &[u64]) {
    const GAP: f32 = 16.0;
    let mut settled: Vec<u64> = moved.to_vec();
    let mut queue: Vec<u64> = moved.to_vec();
    let mut steps = 0;
    while let Some(anchor) = queue.pop() {
        steps += 1;
        if steps > 64 {
            break;
        }
        let Some(a) = model.modules.iter().find(|m| m.id == anchor).map(module_body_rect) else { continue };
        let keep_clear = Rect::from_xy_wh(a.xy(), a.wh() + vec2(2.0 * GAP, 2.0 * GAP));
        let hit: Vec<u64> = model
            .modules
            .iter()
            .filter(|m| !settled.contains(&m.id) && !model.hidden(m.id) && module_body_rect(m).overlap(keep_clear).is_some())
            .map(|m| m.id)
            .collect();
        for id in hit {
            let Some(m) = model.modules.iter_mut().find(|m| m.id == id) else { continue };
            let r = module_body_rect(m);
            let dx = if r.x() >= a.x() { a.right() + GAP - r.left() } else { a.left() - GAP - r.right() };
            m.pos.x += dx;
            settled.push(id);
            queue.push(id);
        }
    }
}

/// Group frames sit behind their modules: a tinted panel with a title bar
/// (expanded), or a compact box standing in for its members (collapsed).
fn draw_groups(draw: &Draw, t: &Theme, model: &Model, now: f32, hover: Option<Point2>) {
    let caret = (now * 2.0) as i32 % 2 == 0;
    for g in &model.groups {
        let Some(frame) = model.group_frame(g) else { continue };
        let all_selected = g.members.iter().all(|id| model.selected.contains(id));
        let title_bar = group::title_rect(frame);
        if g.collapsed {
            ui::shadow(draw, t, frame, MODULE_RADIUS);
            ui::boxed(draw, frame, MODULE_RADIUS, t.panel, if all_selected { t.accent } else { t.border });
            let n = g.members.len();
            let info = Rect::from_x_y_w_h(frame.x(), frame.bottom() + 12.0, frame.w(), 16.0);
            ui::label_left(draw, info, &format!("{n} module{}", if n == 1 { "" } else { "s" }), t.text_dim, 11);
            // Ports only where cables actually cross the box.
            let crosses = |into: bool| {
                model.connections.iter().any(|c| {
                    let (inside, outside) = if into { (c.to, c.from) } else { (c.from, c.to) };
                    g.members.contains(&inside) && !g.members.contains(&outside)
                })
            };
            for (output, kind) in [(false, crosses(true)), (true, crosses(false))] {
                if kind {
                    let pos = group::collapsed_port(frame, output);
                    draw.ellipse().xy(pos).radius(PORT_RADIUS).color(t.accent).stroke(t.port_ring).stroke_weight(2.0);
                }
            }
        } else {
            let mut fill = t.accent;
            fill.alpha = 22;
            ui::rounded(draw, frame, MODULE_RADIUS + 4.0, fill);
            let mut border = t.accent;
            border.alpha = if all_selected { 255 } else { 110 };
            ui::rounded_outline(draw, frame, MODULE_RADIUS + 4.0, border, if all_selected { 2.0 } else { 1.0 });
            let mut band = t.accent;
            band.alpha = 60;
            ui::rounded(draw, title_bar, MODULE_RADIUS + 4.0, band);
        }

        let label_rect = title_bar.pad_right(group::BUTTONS.len() as f32 * 24.0 + 8.0);
        match editing_title(model, g.id) {
            Some(view) => {
                ui::rounded(draw, label_rect.pad(3.0), ui::RADIUS, t.field);
                ui::rounded_outline(draw, label_rect.pad(3.0), ui::RADIUS, t.accent, 1.5);
                ui::edit_label_left(draw, t, label_rect.pad_left(4.0), view, t.field_text, 13, caret);
            }
            None => {
                // Hovering the title hints that it's editable (double-click or right-click → Rename).
                if hover.is_some_and(|m| label_rect.contains(m)) && !g.collapsed {
                    ui::rounded_outline(draw, label_rect.pad(3.0), ui::RADIUS, t.accent, 1.0);
                }
                ui::label_left(draw, label_rect, &g.title, t.text, 13);
            }
        }

        for (slot, button) in group::BUTTONS.iter().enumerate() {
            let rect = group::button_rect(frame, slot);
            let over = hover.is_some_and(|m| rect.contains(m));
            ui::rounded(draw, rect, 5.0, if over { t.button_hover } else { rgba8(0, 0, 0, 50) });
            match button {
                GroupButton::Ungroup => ui::label(draw, rect, "\u{00D7}", t.text, 13),
                GroupButton::Collapse => ui::label(draw, rect, if g.collapsed { "+" } else { "\u{2013}" }, t.text, 13),
                GroupButton::SaveToPalette => ui::star(draw, rect.xy(), 6.5, t.video_port, over),
            }
        }
    }
}

fn draw_dragged_cable(draw: &Draw, t: &Theme, app: &App, model: &Model) {
    if let Some(Drag::Cable { from, output }) = model.dragging {
        if let Some(module) = model.modules.iter().find(|m| m.id == from) {
            let signal = port_signal(model, from, output);
            let color = signal.map_or(t.text_dim, |p| t.port(p));
            let end = to_world(model, pt2(app.mouse.x, app.mouse.y));
            draw_cable(draw, t, output_port_pos(module, output), end, color, signal == Some(PortType::Numbers));
        }
    }
}

/// The sampled curve of a cable from `start` to `end`.
fn cable_points(start: Point2, end: Point2) -> Vec<Point2> {
    let pull = ((end.x - start.x).abs() * 0.5).max(40.0);
    let (c1, c2) = (start + vec2(pull, 0.0), end - vec2(pull, 0.0));
    (0..=24)
        .map(|i| {
            let s = i as f32 / 24.0;
            let u = 1.0 - s;
            start * (u * u * u) + c1 * (3.0 * u * u * s) + c2 * (3.0 * u * s * s) + end * (s * s * s)
        })
        .collect()
}

/// A cable as a horizontal S-curve, so it leaves and enters ports sideways.
/// An array's cable is drawn doubled: two strands side by side.
fn draw_cable(draw: &Draw, t: &Theme, start: Point2, end: Point2, color: ui::Color, array: bool) {
    let points = cable_points(start, end);
    if !array {
        draw.polyline().weight(6.0).color(t.shadow).points(points.clone());
        draw.polyline().weight(3.5).color(color).points(points);
        return;
    }
    draw.polyline().weight(9.0).color(t.shadow).points(points.clone());
    let normal = |i: usize| {
        let (a, b) = (points[i.saturating_sub(1)], points[(i + 1).min(points.len() - 1)]);
        let d = (b - a).normalize_or_zero();
        vec2(-d.y, d.x)
    };
    for side in [-2.2, 2.2] {
        let strand: Vec<Point2> = (0..points.len()).map(|i| points[i] + normal(i) * side).collect();
        draw.polyline().weight(2.0).color(color).points(strand);
    }
}

/// Draws whatever Output currently carries: its cached preview texture
/// (kept fresh unconditionally — see `refresh_one_texture`), or a "no
/// signal" placeholder if nothing's reached it yet.
fn draw_output_signal(draw: &Draw, t: &Theme, win: Rect, model: &Model) {
    let texture = model
        .modules
        .iter()
        .find(|m| m.id == model.output_id)
        .and_then(|m| m.body.chrome().preview_texture.as_ref());

    match texture {
        Some(texture) => {
            let [img_w, img_h] = texture.view.size();
            let scale = (win.w() / img_w as f32).min(win.h() / img_h as f32);
            draw.texture(&texture.view).w_h(img_w as f32 * scale, img_h as f32 * scale);
        }
        None => {
            draw.text("No Signal").color(t.text_dim).font_size(32);
        }
    }
}
