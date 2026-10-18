//! Several projects open at once, each one independent, shown as tabs over
//! the canvas (the simple view). The active project's data lives in the
//! `Model` as always; the others are parked in their tab with their
//! modules, cables, groups, selection and view, and swapped in on click.
//! Parked projects don't run: their cameras and decoders are released.
//!
//! Only the active project is remembered between runs (see the session
//! file), so reopening the program brings back just the last one used.

use crate::group::Group;
use crate::modules::{push_module, Connection, ModuleBody, ModuleKind, PlacedModule};
use crate::ui::{self, Theme};
use crate::{canvas_rect, close_popups, commit_edit, EditTarget, Model, TextEdit};
use nannou::prelude::*;
use std::path::PathBuf;

/// A project that isn't on screen right now.
pub struct Parked {
    modules: Vec<PlacedModule>,
    connections: Vec<Connection>,
    groups: Vec<Group>,
    selected: Vec<u64>,
    output_id: u64,
    next_id: u64,
    pan: Vec2,
    zoom: f32,
    path: Option<PathBuf>,
}

pub struct ProjectTab {
    pub name: String,
    /// `None` for the active tab, whose data is in the model.
    pub parked: Option<Parked>,
}

impl ProjectTab {
    pub fn new(name: impl Into<String>) -> Self {
        ProjectTab { name: name.into(), parked: None }
    }
}

fn park(model: &mut Model) -> Parked {
    for m in &mut model.modules {
        match &mut m.body {
            ModuleBody::Camera(cam) => {
                cam.worker = None;
                cam.chrome.preview_texture = None;
            }
            ModuleBody::Video(video) => {
                video.worker = None;
                video.chrome.preview_texture = None;
            }
            _ => {}
        }
    }
    Parked {
        modules: std::mem::take(&mut model.modules),
        connections: std::mem::take(&mut model.connections),
        groups: std::mem::take(&mut model.groups),
        selected: std::mem::take(&mut model.selected),
        output_id: model.output_id,
        next_id: model.next_id,
        pan: model.pan,
        zoom: model.zoom,
        path: model.project.take(),
    }
}

fn unpark(model: &mut Model, p: Parked) {
    model.modules = p.modules;
    model.connections = p.connections;
    model.groups = p.groups;
    model.selected = p.selected;
    model.output_id = p.output_id;
    model.next_id = p.next_id;
    model.pan = p.pan;
    model.zoom = p.zoom;
    model.project = p.path;
}

/// Empties the canvas down to a lone Output.
fn blank(model: &mut Model, win: Rect) {
    model.modules.clear();
    model.connections.clear();
    model.groups.clear();
    model.selected.clear();
    model.next_id = 0;
    model.pan = Vec2::ZERO;
    model.zoom = 1.0;
    model.project = None;
    model.output_id = push_module(&mut model.modules, &mut model.next_id, ModuleKind::Output, canvas_rect(win).xy());
}

fn settle(model: &mut Model) {
    commit_edit(model);
    close_popups(model);
    model.dragging = None;
}

pub fn switch_to(model: &mut Model, index: usize) {
    if index == model.active_tab || index >= model.tabs.len() {
        return;
    }
    settle(model);
    let current = park(model);
    let target = model.tabs[index].parked.take();
    let active = model.active_tab;
    model.tabs[active].parked = Some(current);
    if let Some(p) = target {
        unpark(model, p);
    }
    model.active_tab = index;
}

fn fresh_name(model: &Model) -> String {
    let taken = |n: &str| model.tabs.iter().any(|t| t.name == n);
    if !taken("Untitled") {
        return "Untitled".to_string();
    }
    (2..).map(|i| format!("Untitled {i}")).find(|n| !taken(n)).unwrap_or_default()
}

/// A new, empty project in its own tab, now active.
pub fn new_tab(model: &mut Model, win: Rect, name: String) {
    settle(model);
    let current = park(model);
    let active = model.active_tab;
    model.tabs[active].parked = Some(current);
    model.tabs.push(ProjectTab::new(name));
    model.active_tab = model.tabs.len() - 1;
    blank(model, win);
}

/// Opens a project in a new tab, filled by `load`. If that fails the tab
/// is dropped again and the previous project comes back.
pub fn open(model: &mut Model, win: Rect, name: String, load: impl FnOnce(&mut Model) -> Result<(), String>) -> Result<(), String> {
    new_tab(model, win, name.clone());
    if let Err(err) = load(model) {
        let index = model.active_tab;
        close_tab(model, win, index);
        return Err(err);
    }
    let index = model.active_tab;
    model.tabs[index].name = name;
    Ok(())
}

pub fn close_tab(model: &mut Model, win: Rect, index: usize) {
    if index >= model.tabs.len() {
        return;
    }
    settle(model);
    if model.tabs.len() == 1 {
        blank(model, win);
        model.tabs[0].name = "Untitled".to_string();
        return;
    }
    if index == model.active_tab {
        let target = if index > 0 { index - 1 } else { 1 };
        if let Some(p) = model.tabs[target].parked.take() {
            unpark(model, p);
        }
        model.tabs.remove(index);
        model.active_tab = if target > index { target - 1 } else { target };
    } else {
        model.tabs.remove(index);
        if index < model.active_tab {
            model.active_tab -= 1;
        }
    }
}

pub fn untitled_name(model: &Model) -> String {
    fresh_name(model)
}

// --- Tab bar ---------------------------------------------------------------

pub const BAR_H: f32 = 30.0;
const TAB_MAX_W: f32 = 170.0;

pub fn bar_rect(win: Rect) -> Rect {
    let canvas = canvas_rect(win);
    Rect::from_x_y_w_h(canvas.x(), canvas.top() - BAR_H / 2.0, canvas.w(), BAR_H)
}

struct TabRects {
    tab: Rect,
    close: Rect,
}

fn layout(win: Rect, n: usize) -> (Vec<TabRects>, Rect) {
    let bar = bar_rect(win);
    let width = ((bar.w() - 48.0) / n.max(1) as f32).min(TAB_MAX_W);
    let tabs = (0..n)
        .map(|i| {
            let tab = Rect::from_x_y_w_h(bar.left() + 6.0 + width / 2.0 + i as f32 * (width + 2.0), bar.y(), width, BAR_H - 6.0);
            let close = Rect::from_x_y_w_h(tab.right() - 12.0, tab.y(), 18.0, 18.0);
            TabRects { tab, close }
        })
        .collect::<Vec<_>>();
    let after = tabs.last().map_or(bar.left() + 6.0, |t| t.tab.right() + 2.0);
    (tabs, Rect::from_x_y_w_h(after + 14.0, bar.y(), 28.0, BAR_H - 6.0))
}

/// A click on the tab bar. Returns whether the bar took it.
pub fn press(model: &mut Model, win: Rect, mouse: Point2, double_click: bool) -> bool {
    if !bar_rect(win).contains(mouse) {
        return false;
    }
    let (tabs, plus) = layout(win, model.tabs.len());
    if let Some(i) = tabs.iter().position(|t| t.close.contains(mouse)) {
        close_tab(model, win, i);
    } else if let Some(i) = tabs.iter().position(|t| t.tab.contains(mouse)) {
        switch_to(model, i);
        // Double-click renames.
        if double_click {
            start_rename(model, i);
        }
    } else if plus.contains(mouse) {
        let name = untitled_name(model);
        new_tab(model, win, name);
    }
    true
}

/// Opens the tab's name for editing, with the old name in the field.
pub fn start_rename(model: &mut Model, index: usize) {
    let Some(name) = model.tabs.get(index).map(|t| t.name.clone()) else { return };
    model.editing = Some(TextEdit::new(EditTarget::TabName(index), name, false));
}

pub fn tab_at(model: &Model, win: Rect, mouse: Point2) -> Option<usize> {
    layout(win, model.tabs.len()).0.iter().position(|t| t.tab.contains(mouse))
}

/// The rectangle a tab's name is drawn in.
fn name_rect(tab: Rect) -> Rect {
    tab.pad_left(8.0).pad_right(26.0)
}

/// The caret position under the pointer for the tab name being edited.
pub fn caret_for(model: &Model, win: Rect, index: usize, mouse: Point2, inside_only: bool) -> Option<usize> {
    let edit = model.editing.as_ref()?;
    let rects = layout(win, model.tabs.len()).0.into_iter().nth(index)?;
    let field = name_rect(rects.tab);
    if inside_only && !field.contains(mouse) {
        return None;
    }
    // `label_left` starts the text 10 px in; the field itself starts 2 px in.
    Some(ui::caret_at(&edit.buffer, 12, mouse.x - (field.left() + 2.0 + 10.0)))
}

pub fn draw(draw: &Draw, t: &Theme, win: Rect, model: &Model, hover: Option<Point2>, now: f32) {
    let bar = bar_rect(win);
    ui::rounded(draw, bar, 0.0, t.sidebar);
    let over = |r: Rect| hover.is_some_and(|m| r.contains(m));
    let (tabs, plus) = layout(win, model.tabs.len());
    for (i, (rects, tab)) in tabs.iter().zip(&model.tabs).enumerate() {
        ui::pill_sized(draw, t, rects.tab, "", i == model.active_tab, over(rects.tab), 12);
        let text = name_rect(rects.tab);
        match model.editing.as_ref().filter(|e| e.target == EditTarget::TabName(i)) {
            Some(edit) => {
                ui::rounded(draw, text.pad(-2.0), ui::RADIUS, t.field);
                ui::rounded_outline(draw, text.pad(-2.0), ui::RADIUS, t.accent, 1.5);
                let view = ui::EditView { text: &edit.buffer, cursor: edit.cursor, selection: edit.selection() };
                ui::edit_label_left(draw, t, text.pad_left(2.0), view, t.field_text, 12, (now * 2.0) as i32 % 2 == 0);
            }
            None => ui::label_left(draw, text, &tab.name, if i == model.active_tab { t.accent_text } else { t.field_text }, 12),
        }
        ui::label(draw, rects.close, "\u{00D7}", if over(rects.close) { t.danger } else { t.text_dim }, 14);
    }
    ui::pill_sized(draw, t, plus, "+", false, over(plus), 14);
}
