//! Look and feel: the color palettes and the small set of widgets every
//! part of the interface is drawn with (rounded boxes, buttons, number
//! boxes, toggles, dropdown lists, popovers).

use crate::modules::{ModuleKind, PortType};
use nannou::prelude::*;
use serde::{Deserialize, Serialize};

pub type Color = Srgba<u8>;

const fn c(hex: u32) -> Color {
    Srgba {
        color: Srgb { red: (hex >> 16) as u8, green: (hex >> 8) as u8, blue: hex as u8, standard: std::marker::PhantomData },
        alpha: 255,
    }
}

const fn ca(hex: u32, alpha: u8) -> Color {
    let mut color = c(hex);
    color.alpha = alpha;
    color
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Palette {
    Dark,
    Light,
    #[default]
    Midnight,
    Warm,
}

impl Palette {
    pub const ALL: [Palette; 4] = [Palette::Dark, Palette::Light, Palette::Midnight, Palette::Warm];

    pub fn name(self) -> &'static str {
        match self {
            Palette::Dark => "Dark",
            Palette::Light => "Light",
            Palette::Midnight => "Midnight",
            Palette::Warm => "Warm",
        }
    }

    pub fn theme(self) -> &'static Theme {
        match self {
            Palette::Dark => &DARK,
            Palette::Light => &LIGHT,
            Palette::Midnight => &MIDNIGHT,
            Palette::Warm => &WARM,
        }
    }
}

pub struct Theme {
    pub canvas: Color,
    pub sidebar: Color,
    pub panel: Color,
    pub border: Color,
    pub shadow: Color,
    pub button: Color,
    pub button_hover: Color,
    pub button_text: Color,
    pub accent: Color,
    pub accent_hover: Color,
    pub accent_text: Color,
    pub field: Color,
    pub field_hover: Color,
    pub field_text: Color,
    pub fill: Color,
    pub fill_driven: Color,
    pub text: Color,
    pub text_dim: Color,
    pub ok: Color,
    pub error: Color,
    pub danger: Color,
    pub video_port: Color,
    pub number_port: Color,
    pub port_ring: Color,
    pub camera: Color,
    pub blend: Color,
    pub shader: Color,
    pub output: Color,
    pub math: Color,
    pub title_text: Color,
}

impl Theme {
    pub fn module(&self, kind: ModuleKind) -> Color {
        match kind {
            ModuleKind::Camera | ModuleKind::Video => self.camera,
            ModuleKind::Blend | ModuleKind::Stack => self.blend,
            ModuleKind::Shader | ModuleKind::Generator | ModuleKind::Effect | ModuleKind::Feedback | ModuleKind::ShaderMix => {
                self.shader
            }
            ModuleKind::Output => self.output,
            ModuleKind::Number
            | ModuleKind::Mult
            | ModuleKind::Lfo
            | ModuleKind::Lag
            | ModuleKind::Map
            | ModuleKind::Osc
            | ModuleKind::Select
            | ModuleKind::Wled => self.math,
        }
    }

    pub fn port(&self, port: PortType) -> Color {
        match port {
            PortType::Video => self.video_port,
            PortType::Number | PortType::Numbers => self.number_port,
        }
    }
}

static DARK: Theme = Theme {
    canvas: c(0x3a3d42),
    sidebar: c(0x1e2023),
    panel: c(0x25282c),
    border: c(0x111214),
    shadow: ca(0x000000, 90),
    button: c(0x3b3f45),
    button_hover: c(0x4a4f57),
    button_text: c(0xeceff3),
    accent: c(0x3d8bd9),
    accent_hover: c(0x56a0ea),
    accent_text: c(0xffffff),
    field: c(0xf2f3f5),
    field_hover: c(0xffffff),
    field_text: c(0x1b1d20),
    fill: c(0x3d8bd9),
    fill_driven: c(0x9aa1ab),
    text: c(0xeceff3),
    text_dim: c(0x9aa1ab),
    ok: c(0x5fd38d),
    error: c(0xff6b6b),
    danger: c(0xd9534f),
    video_port: c(0xf2c94c),
    number_port: c(0x1b1d20),
    port_ring: c(0xffffff),
    camera: c(0x3f6f9e),
    blend: c(0x7c5a9e),
    shader: c(0x3f8a66),
    output: c(0x2f4f4f),
    math: c(0xa8793c),
    title_text: c(0xffffff),
};

static LIGHT: Theme = Theme {
    canvas: c(0xdfe3e8),
    sidebar: c(0xf7f8fa),
    panel: c(0xffffff),
    border: c(0xb9c0c9),
    shadow: ca(0x1b2430, 45),
    button: c(0xe8ebef),
    button_hover: c(0xd8dde3),
    button_text: c(0x1b1d20),
    accent: c(0x2f74c0),
    accent_hover: c(0x3d86d6),
    accent_text: c(0xffffff),
    field: c(0xffffff),
    field_hover: c(0xf3f6fa),
    field_text: c(0x1b1d20),
    fill: c(0x6ea8e6),
    fill_driven: c(0xb7bec7),
    text: c(0x1b1d20),
    text_dim: c(0x6b7480),
    ok: c(0x2e9d5b),
    error: c(0xc0392b),
    danger: c(0xd9534f),
    video_port: c(0xe0a800),
    number_port: c(0x2b2f36),
    port_ring: c(0xffffff),
    camera: c(0x5b8fc4),
    blend: c(0x9a7cc0),
    shader: c(0x55a882),
    output: c(0x5e7a7a),
    math: c(0xc8955a),
    title_text: c(0xffffff),
};

static MIDNIGHT: Theme = Theme {
    canvas: c(0x141a2a),
    sidebar: c(0x0c111d),
    panel: c(0x1a2236),
    border: c(0x070a12),
    shadow: ca(0x000000, 120),
    button: c(0x26304a),
    button_hover: c(0x323e5e),
    button_text: c(0xdfe6f5),
    accent: c(0x7b6cf6),
    accent_hover: c(0x9184ff),
    accent_text: c(0xffffff),
    field: c(0x222b42),
    field_hover: c(0x2b3653),
    field_text: c(0xdfe6f5),
    fill: c(0x7b6cf6),
    fill_driven: c(0x56607a),
    text: c(0xdfe6f5),
    text_dim: c(0x8590ab),
    ok: c(0x4fd1a5),
    error: c(0xff7a90),
    danger: c(0xe05470),
    video_port: c(0x5ce1e6),
    number_port: c(0xf5a3ff),
    port_ring: c(0x0c111d),
    camera: c(0x2f4d8a),
    blend: c(0x5a3f8f),
    shader: c(0x23705f),
    output: c(0x2a3550),
    math: c(0x8a4f7a),
    title_text: c(0xffffff),
};

static WARM: Theme = Theme {
    canvas: c(0x3b322c),
    sidebar: c(0x221c18),
    panel: c(0x2c2420),
    border: c(0x140f0c),
    shadow: ca(0x000000, 100),
    button: c(0x4a3c33),
    button_hover: c(0x5c4b40),
    button_text: c(0xf6eadf),
    accent: c(0xe0803a),
    accent_hover: c(0xf0944f),
    accent_text: c(0x1e1410),
    field: c(0xf6eadf),
    field_hover: c(0xfff6ee),
    field_text: c(0x2a1f19),
    fill: c(0xe0803a),
    fill_driven: c(0xa89282),
    text: c(0xf6eadf),
    text_dim: c(0xb3a194),
    ok: c(0x9ccc65),
    error: c(0xff7f66),
    danger: c(0xd0583c),
    video_port: c(0xffd166),
    number_port: c(0x2a1f19),
    port_ring: c(0xf6eadf),
    camera: c(0x7a5a3a),
    blend: c(0x8a4b5c),
    shader: c(0x5d7a45),
    output: c(0x4a3f38),
    math: c(0xb06a30),
    title_text: c(0xffffff),
};

// --- Shapes ------------------------------------------------------------------

pub const RADIUS: f32 = 6.0;
const CORNER_SEGMENTS: usize = 6;

/// Outline of `rect` with rounded corners, counter-clockwise.
pub fn rounded_points(rect: Rect, radius: f32) -> Vec<Point2> {
    let r = radius.min(rect.w() / 2.0).min(rect.h() / 2.0).max(0.0);
    let corners = [
        (pt2(rect.right() - r, rect.top() - r), 0.0),
        (pt2(rect.left() + r, rect.top() - r), 0.5),
        (pt2(rect.left() + r, rect.bottom() + r), 1.0),
        (pt2(rect.right() - r, rect.bottom() + r), 1.5),
    ];
    let mut points = Vec::with_capacity(4 * (CORNER_SEGMENTS + 1));
    for (center, start) in corners {
        for i in 0..=CORNER_SEGMENTS {
            let a = (start + 0.5 * i as f32 / CORNER_SEGMENTS as f32) * PI;
            points.push(center + vec2(a.cos(), a.sin()) * r);
        }
    }
    points
}

pub fn rounded(draw: &Draw, rect: Rect, radius: f32, fill: Color) {
    if rect.w() <= 0.0 || rect.h() <= 0.0 {
        return;
    }
    draw.polygon().color(fill).points(rounded_points(rect, radius));
}

pub fn rounded_outline(draw: &Draw, rect: Rect, radius: f32, color: Color, weight: f32) {
    draw.polyline().weight(weight).color(color).points_closed(rounded_points(rect, radius));
}

/// A filled rounded box with a border.
pub fn boxed(draw: &Draw, rect: Rect, radius: f32, fill: Color, border: Color) {
    rounded(draw, rect, radius, fill);
    rounded_outline(draw, rect, radius, border, 1.0);
}

/// A soft drop shadow under floating things (panels, dropdowns, popovers).
pub fn shadow(draw: &Draw, t: &Theme, rect: Rect, radius: f32) {
    rounded(draw, rect.shift(vec2(3.0, -4.0)), radius + 2.0, t.shadow);
}

// --- Icons -------------------------------------------------------------------
//
// The embedded font (Noto Sans) has no arrows, checks or stars, so these
// are drawn as shapes.

/// A small triangle pointing down (`open`) or right.
pub fn chevron(draw: &Draw, center: Point2, size: f32, color: Color, open: bool) {
    let h = size / 2.0;
    let points = if open {
        [pt2(-h, h * 0.5), pt2(h, h * 0.5), pt2(0.0, -h * 0.6)]
    } else {
        [pt2(-h * 0.5, h), pt2(-h * 0.5, -h), pt2(h * 0.6, 0.0)]
    };
    draw.polygon().color(color).points(points.map(|p| p + center));
}

pub fn check(draw: &Draw, center: Point2, size: f32, color: Color) {
    let s = size / 2.0;
    let points = [pt2(-s, 0.0), pt2(-s * 0.3, -s * 0.7), pt2(s, s * 0.7)].map(|p| p + center);
    draw.polyline().weight(2.0).color(color).points(points);
}

/// A five-point star, filled or as an outline.
pub fn star(draw: &Draw, center: Point2, radius: f32, color: Color, filled: bool) {
    let points: Vec<Point2> = (0..10)
        .map(|i| {
            let r = if i % 2 == 0 { radius } else { radius * 0.45 };
            let a = PI / 2.0 + i as f32 * PI / 5.0;
            center + vec2(a.cos(), a.sin()) * r
        })
        .collect();
    if filled {
        draw.polygon().color(color).points(points);
    } else {
        draw.polyline().weight(1.5).color(color).points_closed(points);
    }
}

// --- Text --------------------------------------------------------------------

pub fn label(draw: &Draw, rect: Rect, text: &str, color: Color, size: u32) {
    draw.text(text).xy(rect.xy()).wh(rect.wh()).no_line_wrap().color(color).font_size(size);
}

pub fn label_left(draw: &Draw, rect: Rect, text: &str, color: Color, size: u32) {
    let inner = rect.pad_left(10.0).pad_right(6.0);
    draw.text(text)
        .xy(inner.xy())
        .wh(inner.wh())
        .left_justify()
        .no_line_wrap()
        .color(color)
        .font_size(size);
}

/// `text` broken into lines no wider than `width` at font `size`: between
/// words where it can, mid-word for a word too long to fit on its own.
pub fn wrap(text: &str, width: f32, size: u32) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let candidate = if line.is_empty() { word.to_string() } else { format!("{line} {word}") };
        if text_width(&candidate, size) <= width {
            line = candidate;
            continue;
        }
        if !line.is_empty() {
            lines.push(std::mem::take(&mut line));
        }
        for ch in word.chars() {
            line.push(ch);
            if text_width(&line, size) > width && line.chars().count() > 1 {
                let last = line.pop().unwrap();
                lines.push(std::mem::take(&mut line));
                line.push(last);
            }
        }
    }
    if !line.is_empty() || lines.is_empty() {
        lines.push(line);
    }
    lines
}

/// Width in pixels of `text` at font `size`, as `label_left` lays it out.
pub fn text_width(text: &str, size: u32) -> f32 {
    if text.is_empty() {
        return 0.0;
    }
    nannou::text::text(text)
        .font_size(size)
        .no_line_wrap()
        .left_justify()
        .build(Rect::from_w_h(100_000.0, 100.0))
        .width()
}

/// Which gap between characters of `text` is closest to `x`, measured from
/// where `label_left` starts drawing: the caret position for a click there.
pub fn caret_at(text: &str, size: u32, x: f32) -> usize {
    let n = text.chars().count();
    (0..=n)
        .min_by(|a, b| {
            let da = (text_width(&text.chars().take(*a).collect::<String>(), size) - x).abs();
            let db = (text_width(&text.chars().take(*b).collect::<String>(), size) - x).abs();
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        })
        .unwrap_or(n)
}

/// Numbers shown in boxes: two decimals, one once they get big.
pub fn fmt_num(v: f32) -> String {
    if v.abs() >= 100.0 {
        format!("{v:.1}")
    } else {
        format!("{v:.2}")
    }
}

// --- Widgets -----------------------------------------------------------------

pub fn button(draw: &Draw, t: &Theme, rect: Rect, text: &str, hovered: bool) {
    let fill = if hovered { t.button_hover } else { t.button };
    boxed(draw, rect, RADIUS, fill, t.border);
    label(draw, rect, text, t.button_text, 14);
}

/// The main call to action (Start fullscreen).
pub fn accent_button(draw: &Draw, t: &Theme, rect: Rect, text: &str, hovered: bool) {
    shadow(draw, t, rect, RADIUS);
    rounded(draw, rect, RADIUS, if hovered { t.accent_hover } else { t.accent });
    label(draw, rect, text, t.accent_text, 16);
}

/// A box that opens a dropdown (camera, shader file, palette).
pub fn selector(draw: &Draw, t: &Theme, rect: Rect, text: &str, hovered: bool) {
    selector_sized(draw, t, rect, text, hovered, 12);
}

pub fn selector_sized(draw: &Draw, t: &Theme, rect: Rect, text: &str, hovered: bool, font: u32) {
    boxed(draw, rect, RADIUS, if hovered { t.field_hover } else { t.field }, t.border);
    label_left(draw, rect.pad_right(18.0), text, t.field_text, font);
    chevron(draw, pt2(rect.right() - 13.0, rect.y()), 8.0, t.field_text, true);
}

/// A number box: a bar showing where the value sits in its range (always
/// inside the box), its text, and a caret while it's being typed into.
/// `driven` boxes are fed by a cable and drawn as a dimmer readout.
/// Text being typed: where the caret is and what part is selected.
#[derive(Clone, Copy)]
pub struct EditView<'a> {
    pub text: &'a str,
    pub cursor: usize,
    pub selection: Option<(usize, usize)>,
}

/// Highlight behind the selected characters of `text`, which starts at `left`.
pub fn selection_highlight(draw: &Draw, t: &Theme, view: EditView, left: f32, y: f32, h: f32, size: u32) {
    let Some((a, b)) = view.selection else { return };
    let x = |n: usize| left + text_width(&view.text.chars().take(n).collect::<String>(), size);
    let (x0, x1) = (x(a), x(b));
    let mut fill = t.accent;
    fill.alpha = 110;
    draw.rect().x_y((x0 + x1) / 2.0, y).w_h(x1 - x0, h).color(fill);
}

/// A left-aligned text field's contents while being edited, as `label_left`
/// lays them out: selection highlight, text and caret.
pub fn edit_label_left(draw: &Draw, t: &Theme, rect: Rect, view: EditView, color: Color, size: u32, caret: bool) {
    let left = rect.left() + 10.0;
    selection_highlight(draw, t, view, left, rect.y(), size as f32 * 1.4, size);
    label_left(draw, rect, view.text, color, size);
    if caret {
        let before: String = view.text.chars().take(view.cursor).collect();
        let x = left + text_width(&before, size);
        draw.line().start(pt2(x, rect.y() - size as f32 * 0.6)).end(pt2(x, rect.y() + size as f32 * 0.6)).weight(1.5).color(color);
    }
}

pub struct ValueBox<'a> {
    pub norm: f32,
    pub text: &'a str,
    pub driven: bool,
    pub hovered: bool,
    /// What's typed so far and where the caret sits (in characters), while editing.
    pub editing: Option<EditView<'a>>,
    pub caret: bool,
}

pub fn value_box(draw: &Draw, t: &Theme, rect: Rect, v: ValueBox, font: u32) {
    let editing = v.editing.is_some();
    let fill = if v.hovered && !v.driven { t.field_hover } else { t.field };
    rounded(draw, rect, RADIUS, fill);
    if !editing {
        let w = rect.w() * v.norm.clamp(0.0, 1.0);
        if w >= 1.0 {
            let bar = Rect::from_x_y_w_h(rect.left() + w / 2.0, rect.y(), w, rect.h());
            rounded(draw, bar, RADIUS.min(w / 2.0), if v.driven { t.fill_driven } else { t.fill });
        }
    }
    let border = if editing { t.accent } else { t.border };
    rounded_outline(draw, rect, RADIUS, border, if editing { 2.0 } else { 1.0 });
    match v.editing {
        Some(view) => {
            let left = rect.x() - text_width(view.text, font) / 2.0;
            selection_highlight(draw, t, view, left, rect.y(), rect.h() * 0.6, font);
            label(draw, rect, view.text, t.field_text, font);
            if v.caret {
                let before: String = view.text.chars().take(view.cursor).collect();
                let x = left + text_width(&before, font);
                draw.line().start(pt2(x, rect.y() - rect.h() * 0.28)).end(pt2(x, rect.y() + rect.h() * 0.28)).weight(1.5).color(t.field_text);
            }
        }
        None => label(draw, rect, v.text, t.field_text, font),
    }
}

pub fn toggle(draw: &Draw, t: &Theme, rect: Rect, text: &str, on: bool, hovered: bool) {
    boxed(draw, rect, RADIUS, if hovered { t.button_hover } else { t.button }, t.border);
    label_left(draw, rect.pad_right(40.0), text, t.button_text, 12);
    // A small switch: a track with a knob on the left (off) or right (on).
    let track = Rect::from_w_h(30.0, 16.0).mid_right_of(rect).shift_x(-8.0);
    rounded(draw, track, 8.0, if on { t.accent } else { t.fill_driven });
    let knob_x = if on { track.right() - 8.0 } else { track.left() + 8.0 };
    draw.ellipse().x_y(knob_x, track.y()).radius(6.0).color(t.field_hover);
}

/// A compact on/off button (e.g. the LFO's center toggle): accent when on.
pub fn pill(draw: &Draw, t: &Theme, rect: Rect, text: &str, on: bool, hovered: bool) {
    pill_sized(draw, t, rect, text, on, hovered, 14);
}

pub fn pill_sized(draw: &Draw, t: &Theme, rect: Rect, text: &str, on: bool, hovered: bool, font: u32) {
    let (fill, color) = match (on, hovered) {
        (true, false) => (t.accent, t.accent_text),
        (true, true) => (t.accent_hover, t.accent_text),
        (false, true) => (t.field_hover, t.field_text),
        (false, false) => (t.field, t.field_text),
    };
    boxed(draw, rect, RADIUS, fill, t.border);
    label(draw, rect, text, color, font);
}

/// An open dropdown's options: a solid floating list, one solid row per
/// option, the current one marked and the hovered one highlighted.
/// `stars`: per row, `Some(pinned)` draws a favorite star at the right end
/// (see `STAR_W`); empty for lists without favorites.
pub fn dropdown(
    draw: &Draw,
    t: &Theme,
    rows: &[Rect],
    labels: &[String],
    selected: Option<usize>,
    hovered: Option<usize>,
    stars: &[Option<bool>],
) {
    let (Some(first), Some(last)) = (rows.first(), rows.last()) else { return };
    let list = Rect::from_corners(pt2(first.left(), last.bottom()), pt2(first.right(), first.top())).pad(-4.0);
    shadow(draw, t, list, RADIUS);
    boxed(draw, list, RADIUS, t.panel, t.border);
    for (i, (rect, text)) in rows.iter().zip(labels).enumerate() {
        let row = rect.pad_top(1.0).pad_bottom(1.0);
        // A category heading: dim, flat, never highlighted.
        if let Some(heading) = text.strip_prefix(HEADING) {
            label_left(draw, row.pad_left(10.0), &heading.to_uppercase(), t.text_dim, 10);
            continue;
        }
        let (fill, color) = if hovered == Some(i) {
            (t.accent, t.accent_text)
        } else if selected == Some(i) {
            (t.button_hover, t.button_text)
        } else {
            (t.button, t.button_text)
        };
        rounded(draw, row, RADIUS - 2.0, fill);
        if selected == Some(i) {
            check(draw, pt2(row.left() + 12.0, row.y()), 10.0, color);
        }
        // "Label\tHint" draws the hint (e.g. a shortcut) right-aligned and dim.
        let (main, hint) = text.split_once('\t').unwrap_or((text.as_str(), ""));
        label_left(draw, row.pad_left(16.0).pad_right(STAR_W), main, color, 12);
        if !hint.is_empty() {
            let hint_rect = row.pad_right(10.0);
            let mut dim = color;
            dim.alpha = 150;
            draw.text(hint).xy(hint_rect.xy()).wh(hint_rect.wh()).right_justify().no_line_wrap().color(dim).font_size(11);
        }
        if let Some(Some(pinned)) = stars.get(i) {
            let star_color = if *pinned { t.video_port } else { color };
            star(draw, pt2(row.right() - STAR_W / 2.0, row.y()), 7.0, star_color, *pinned);
        }
    }
}

/// Marks a dropdown label as a non-selectable category heading.
pub const HEADING: char = '\u{1}';

/// Width of the favorite-star hit area at the right of a dropdown row.
pub const STAR_W: f32 = 28.0;

/// A floating panel (settings, a number box's options).
pub fn panel(draw: &Draw, t: &Theme, rect: Rect, title: &str) {
    shadow(draw, t, rect, RADIUS + 2.0);
    boxed(draw, rect, RADIUS + 2.0, t.panel, t.border);
    let title_rect = Rect::from_x_y_w_h(rect.x(), rect.top() - 18.0, rect.w() - 16.0, 20.0);
    label_left(draw, title_rect, title, t.text, 14);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rounded_outline_stays_inside_rect() {
        let rect = Rect::from_x_y_w_h(10.0, -5.0, 100.0, 30.0);
        for p in rounded_points(rect, 8.0) {
            assert!(p.x >= rect.left() - 1e-3 && p.x <= rect.right() + 1e-3, "{p:?}");
            assert!(p.y >= rect.bottom() - 1e-3 && p.y <= rect.top() + 1e-3, "{p:?}");
        }
    }

    #[test]
    fn radius_shrinks_to_fit_small_rects() {
        let rect = Rect::from_x_y_w_h(0.0, 0.0, 4.0, 4.0);
        assert!(rounded_points(rect, 8.0).iter().all(|p| p.x.abs() <= 2.0 + 1e-3 && p.y.abs() <= 2.0 + 1e-3));
    }
}
