//! Group frames: a titled box around some modules. Members move together,
//! and a collapsed group hides its members behind one compact box (they
//! keep running — cables from outside attach to the box's edges). A group
//! can be saved to the sidebar's personal palette as a preset.

use nannou::prelude::*;

pub const TITLE_H: f32 = 30.0;
const PAD: f32 = 16.0;
pub const COLLAPSED_W: f32 = 200.0;
pub const COLLAPSED_H: f32 = 54.0;

pub struct Group {
    pub id: u64,
    pub title: String,
    pub members: Vec<u64>,
    pub collapsed: bool,
}

/// Which title-bar button of a group frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GroupButton {
    /// Removes the frame, keeping the modules.
    Ungroup,
    Collapse,
    /// Saves the group to the sidebar's personal palette.
    SaveToPalette,
}

pub const BUTTONS: [GroupButton; 3] = [GroupButton::Ungroup, GroupButton::Collapse, GroupButton::SaveToPalette];

/// The frame around the members' bodies (with room for the title bar), or
/// the compact box when collapsed — anchored at the members' top-left so
/// collapsing and expanding keeps the group in place.
pub fn frame_rect(collapsed: bool, member_bodies: &[Rect]) -> Option<Rect> {
    let first = member_bodies.first()?;
    let bounds = member_bodies.iter().skip(1).fold(*first, |b, r| {
        Rect::from_corners(pt2(b.left().min(r.left()), b.bottom().min(r.bottom())), pt2(b.right().max(r.right()), b.top().max(r.top())))
    });
    let top_left = pt2(bounds.left() - PAD, bounds.top() + PAD + TITLE_H);
    Some(if collapsed {
        Rect::from_corners(top_left, top_left + vec2(COLLAPSED_W, -COLLAPSED_H))
    } else {
        Rect::from_corners(top_left, pt2(bounds.right() + PAD, bounds.bottom() - PAD))
    })
}

pub fn title_rect(frame: Rect) -> Rect {
    Rect::from_x_y_w_h(frame.x(), frame.top() - TITLE_H / 2.0, frame.w(), TITLE_H)
}

pub fn button_rect(frame: Rect, slot: usize) -> Rect {
    Rect::from_w_h(20.0, 20.0).top_right_of(frame).shift(vec2(-8.0 - slot as f32 * 24.0, -5.0))
}

/// Where a cable to/from a hidden member attaches on a collapsed box.
pub fn collapsed_port(frame: Rect, output: bool) -> Point2 {
    let y = frame.y() - TITLE_H / 4.0;
    if output {
        pt2(frame.right(), y)
    } else {
        pt2(frame.left(), y)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frame_wraps_members_with_title_room() {
        let a = Rect::from_x_y_w_h(0.0, 0.0, 100.0, 50.0);
        let b = Rect::from_x_y_w_h(200.0, -100.0, 100.0, 50.0);
        let f = frame_rect(false, &[a, b]).unwrap();
        assert!(f.left() < a.left() && f.right() > b.right());
        assert!(f.bottom() < b.bottom() && f.top() >= a.top() + TITLE_H);
    }

    #[test]
    fn collapsed_box_keeps_top_left() {
        let a = Rect::from_x_y_w_h(0.0, 0.0, 100.0, 50.0);
        let open = frame_rect(false, &[a]).unwrap();
        let closed = frame_rect(true, &[a]).unwrap();
        assert_eq!((open.left(), open.top()), (closed.left(), closed.top()));
        assert_eq!(closed.w(), COLLAPSED_W);
        assert!(frame_rect(false, &[]).is_none());
    }
}
