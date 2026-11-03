//! A herdr workspace's header in the grouped drawer (plan 20261103_02,
//! decisions 3.1 and 3.4): 28 pt high, the sheet's inner width.
//!
//! - its text, 11 pt, dim: the workspace's label, else its first
//!   session's project, else *Workspace &lt;n&gt;*, cut with "…";
//! - at its right, in the session rows' indicator column, dim, the count
//!   of its rows;
//! - while it is collapsed and a session in it has a question or an
//!   approval waiting, a filled dot in the accent colour, as a remote
//!   card's, in that column, with the count before it.
//!
//! A tap on it is its body ([`ControlAction::Activated`]): the drawer
//! collapses or expands its group.

use libcontrols::{
    ControlAction, ControlPart, DrawContext, DrawControl, EventContext, HostedControl,
    KeyboardEvent, MouseButton, MouseEvent, Rect,
};
use libmsdf::drawlist::DrawList;

use super::cut_right;
use super::session_row::COLUMN_W;
use crate::agent::{fill, line_h};
use crate::theme::theme;

/// The header's height, its text's size, the gap before the count, and
/// the dot's diameter (a remote card's), in points.
pub const HEADER_H: f32 = 28.0;
pub const HEADER_PT: f32 = 11.0;
const INSET: f32 = 4.0;
const BEFORE_COUNT: f32 = 8.0;
const DOT: f32 = 7.0;

/// One workspace's header.
#[derive(Clone, Debug)]
pub struct GroupHeader {
    pub rect: Rect,
    pub workspace_id: String,
    pub text: String,
    /// Its rows, shown or not.
    pub count: usize,
    pub collapsed: bool,
    /// A session in it has a question or an approval waiting.
    pub waiting: bool,
    pub scale: f32,
}

impl GroupHeader {
    /// The indicator column, as a session row's.
    pub fn column(&self) -> Rect {
        let s = self.scale;
        Rect::new(
            self.rect.x + self.rect.w - COLUMN_W * s,
            self.rect.y,
            COLUMN_W * s,
            self.rect.h,
        )
    }

    /// Whether it shows the waiting dot: collapsed, with a session waiting.
    pub fn dot(&self) -> bool {
        self.collapsed && self.waiting
    }
}

impl DrawControl for GroupHeader {
    fn draw(&self, list: &mut DrawList, ctx: &DrawContext) {
        let s = self.scale;
        let px = HEADER_PT * s;
        let y = self.rect.y + (self.rect.h - line_h(px)) * 0.5;
        let col = self.column();
        let count = self.count.to_string();
        let count_w = ctx.text_advance(&count, px);
        // The count right-aligned in the column, or before the dot.
        let count_x = if self.dot() {
            col.x - count_w
        } else {
            col.x + col.w - count_w
        };
        ctx.draw_text(list, &count, [count_x, y], px, theme().dim);
        if self.dot() {
            let d = DOT * s;
            let [cx, cy] = col.center();
            fill(
                list,
                Rect::new(cx - d * 0.5, cy - d * 0.5, d, d),
                theme().accent,
                d * 0.5,
            );
        }
        let x = self.rect.x + INSET * s;
        let room = (count_x - BEFORE_COUNT * s - x).max(0.0);
        let text = cut_right(ctx, &self.text, px, room);
        ctx.draw_text(list, &text, [x, y], px, theme().dim);
    }
}

impl HostedControl for GroupHeader {
    fn bounds(&self) -> Rect {
        self.rect
    }

    fn hit_test(&self, pos: [f32; 2]) -> Option<(ControlPart, f32)> {
        self.rect
            .contains(pos)
            .then(|| (ControlPart::Body, self.rect.normalized_x(pos[0])))
    }

    fn handle_mouse(&self, event: &MouseEvent, _ctx: &EventContext) -> Option<ControlAction> {
        match event {
            MouseEvent::Click {
                pos,
                button: MouseButton::Left,
            } if self.rect.contains(*pos) => Some(ControlAction::Activated {
                part: ControlPart::Body,
            }),
            _ => None,
        }
    }

    fn handle_keyboard(
        &self,
        _event: &KeyboardEvent,
        _ctx: &EventContext,
    ) -> Option<ControlAction> {
        None
    }
}
