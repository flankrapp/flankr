//! *At startup* (plan 20261024_02, the 2026-10-30 15:20 EDT amendment,
//! item 1): its label and ⓘ on the left, and on the right libcontrols'
//! `SelectableLabel` over [`STARTUP_OPTIONS`], "‹ Overview ›", whose
//! arrows step between *Overview* and *Last session*. The first settings
//! row built on `SelectableLabel` rather than the app's own
//! [`OptionRow`](super::option_row::OptionRow): the label holds the
//! selection (its shared state), steps it on an arrow's tap and says
//! which way (`StepLeft`, `StepRight`); an arrow at its end is drawn dim
//! and takes no tap. The screen reads the selection into the settings.
//!
//! The row is generic over its values ([`Choice`]): *Drawer sessions*
//! (plan 20261103_02, decision 2), "‹ Grouped by workspace ›" or
//! "‹ Not grouped ›", is the same row over [`DrawerSessions`].

use libcontrols::{
    ControlAction, ControlPart, DrawContext, DrawControl, EventContext, HostedControl,
    KeyboardEvent, MouseButton, MouseEvent, Rect, SelectableLabel,
};
use libmsdf::drawlist::DrawList;

use super::info::{self, INFO_PART};
use super::option_row::OptionRow;
use super::screen::{LABEL_PT, label_y};
use super::{Choice, DrawerSessions, HomeView, Startup};
use crate::agent::PAD;
use crate::theme::theme;

/// *At startup*'s row.
pub type StartupRow = SelectRow<Startup>;

/// *Drawer sessions*' row.
pub type DrawerSessionsRow = SelectRow<DrawerSessions>;
/// *Home view* (plan 20261112_03, the 21:58 EST amendment, item 1).
pub type HomeViewRow = SelectRow<HomeView>;

/// A label and ⓘ, and a `SelectableLabel` over `T`'s values.
#[derive(Clone, Debug)]
pub struct SelectRow<T: Choice> {
    pub label: String,
    pub rect: Rect,
    pub select: SelectableLabel<T>,
    pub scale: f32,
    /// The ⓘ after the label, which is `label_w` wide.
    pub info: bool,
    pub label_w: f32,
}

impl<T: Choice> SelectRow<T> {
    pub fn new(label: &str, value: T, scale: f32) -> Self {
        let select = SelectableLabel::new(T::options(), Rect::new(0.0, 0.0, 1.0, 1.0))
            .with_selected_index(value.index())
            .with_arrow_width(OptionRow::ARROW_W * scale)
            .with_font_size(LABEL_PT * scale)
            .with_text_color(theme().text)
            .with_arrow_color(theme().dim);
        SelectRow {
            label: label.to_string(),
            rect: Rect::new(0.0, 0.0, 1.0, 1.0),
            select,
            scale,
            info: true,
            label_w: 0.0,
        }
    }

    /// The ⓘ's target, after the label.
    pub fn info_rect(&self) -> Rect {
        let s = self.scale;
        info::target(
            self.rect.x + PAD * s + self.label_w,
            self.rect.y + self.rect.h * 0.5,
            s,
        )
    }

    fn on_info(&self, pos: [f32; 2]) -> bool {
        self.info && self.info_rect().contains(pos)
    }

    /// Moves the row to `rect`, the label's arrows at its right, as wide as
    /// an option row's.
    pub fn set_rect(&mut self, rect: Rect) {
        let s = self.scale;
        self.rect = rect;
        let w = OptionRow::ARROWS_W * s;
        self.select.rect = Rect::new(rect.x + rect.w - PAD * s - w, rect.y, w, rect.h);
        self.select.chevron_thickness = 1.8 * s;
    }

    /// The value selected.
    pub fn value(&self) -> T {
        self.select.selected_value().copied().unwrap_or_default()
    }

    pub fn set_value(&self, value: T) {
        self.select.set_selected_index(value.index());
    }

    /// The left and right arrows' rects.
    pub fn arrows(&self) -> (Rect, Rect) {
        let (left, _, right) = self.select.sub_rects();
        (left, right)
    }
}

impl<T: Choice> DrawControl for SelectRow<T> {
    fn draw(&self, list: &mut DrawList, ctx: &DrawContext) {
        let px = LABEL_PT * self.scale;
        ctx.draw_text(
            list,
            &self.label,
            [self.rect.x + PAD * self.scale, label_y(self.rect, px)],
            px,
            theme().text,
        );
        self.select.draw(list, ctx);
        if self.info {
            info::draw(list, ctx, self.info_rect(), self.scale);
        }
    }
}

impl<T: Choice> HostedControl for SelectRow<T> {
    fn bounds(&self) -> Rect {
        self.rect
    }

    /// The label's parts on it, the ⓘ's on it, the body elsewhere.
    fn hit_test(&self, pos: [f32; 2]) -> Option<(ControlPart, f32)> {
        if self.on_info(pos) {
            return Some((INFO_PART, self.rect.normalized_x(pos[0])));
        }
        if !self.rect.contains(pos) {
            return None;
        }
        self.select
            .hit_test(pos)
            .or_else(|| Some((ControlPart::Body, self.rect.normalized_x(pos[0]))))
    }

    /// A tap on an arrow steps the label; the press, which may become a
    /// scroll, does not. A tap on the ⓘ is its explanation.
    fn handle_mouse(&self, event: &MouseEvent, ctx: &EventContext) -> Option<ControlAction> {
        match event {
            MouseEvent::Click {
                pos,
                button: MouseButton::Left,
            } if self.on_info(*pos) => Some(ControlAction::Activated { part: INFO_PART }),
            MouseEvent::Click {
                button: MouseButton::Left,
                ..
            } => match ctx.part {
                ControlPart::LeftArrow | ControlPart::RightArrow => {
                    self.select.handle_mouse(event, ctx)
                }
                _ => None,
            },
            _ => None,
        }
    }

    fn handle_keyboard(&self, event: &KeyboardEvent, ctx: &EventContext) -> Option<ControlAction> {
        self.select.handle_keyboard(event, ctx)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(value: Startup) -> StartupRow {
        let mut r = StartupRow::new("At startup", value, 1.0);
        r.label_w = 90.0;
        r.set_rect(Rect::new(0.0, 0.0, 400.0, 52.0));
        r
    }

    fn click(r: &StartupRow, pos: [f32; 2]) -> Option<ControlAction> {
        let (part, _) = r.hit_test(pos)?;
        let mut ctx = EventContext::default();
        ctx.part = part;
        r.handle_mouse(
            &MouseEvent::Click {
                pos,
                button: MouseButton::Left,
            },
            &ctx,
        )
    }

    #[test]
    fn the_arrows_step_between_overview_and_last_session() {
        let r = row(Startup::Overview);
        assert_eq!(r.select.display_string(), "Overview");
        let (left, right) = r.arrows();
        // At the first value the left arrow takes no tap.
        assert_eq!(click(&r, left.center()), None);
        assert_eq!(r.value(), Startup::Overview);
        assert_eq!(click(&r, right.center()), Some(ControlAction::StepRight));
        assert_eq!(r.value(), Startup::LastSession);
        assert_eq!(r.select.display_string(), "Last session");
        assert_eq!(click(&r, right.center()), None, "the last value");
        assert_eq!(click(&r, left.center()), Some(ControlAction::StepLeft));
        assert_eq!(r.value(), Startup::Overview);
        // The ⓘ is its explanation; the label itself does nothing.
        assert_eq!(
            click(&r, r.info_rect().center()),
            Some(ControlAction::Activated { part: INFO_PART })
        );
        assert_eq!(click(&r, [PAD + 2.0, 26.0]), None);
        // Set from the settings.
        r.set_value(Startup::LastSession);
        assert_eq!(r.value(), Startup::LastSession);
    }

    #[test]
    fn drawer_sessions_steps_between_grouped_and_not_grouped() {
        let mut r = DrawerSessionsRow::new("Drawer sessions", DrawerSessions::Grouped, 1.0);
        r.label_w = 110.0;
        r.set_rect(Rect::new(0.0, 0.0, 400.0, 52.0));
        assert_eq!(r.select.display_string(), "Grouped by workspace");
        let (left, right) = r.arrows();
        let click = |pos: [f32; 2]| {
            let (part, _) = r.hit_test(pos)?;
            let mut ctx = EventContext::default();
            ctx.part = part;
            r.handle_mouse(
                &MouseEvent::Click {
                    pos,
                    button: MouseButton::Left,
                },
                &ctx,
            )
        };
        assert_eq!(click(left.center()), None, "the first value");
        assert_eq!(click(right.center()), Some(ControlAction::StepRight));
        assert_eq!(r.value(), DrawerSessions::Flat);
        assert_eq!(r.select.display_string(), "Not grouped");
        assert_eq!(click(right.center()), None, "the last value");
    }
}
