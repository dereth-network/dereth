use super::*;
use crate::region::Box2D;

/// The meter's initialisation finds its child image, element id 2, recursively.
///
/// The lookup is recursive, but the child draw only ever compares it against a **direct**
/// child, so a meter whose id-2 element were a grandchild would clip nothing. Every shipped
/// meter has it as a direct child.
pub const CHILD_IMAGE: crate::ElementId = crate::ElementId(2);

/// The fill direction, attribute `0x6F` — which edge the fill grows from.
pub mod direction {
    /// Fill from the left: the child keeps `x0 ..= x0 + w*position - 1`.
    pub const LEFT: u32 = 1;
    /// Fill from the top.
    pub const TOP: u32 = 2;
    /// Fill from the right: `x0` moves in by `w*(1 - position)`.
    pub const RIGHT: u32 = 3;
    /// Fill from the bottom.
    pub const BOTTOM: u32 = 4;
}

/// The four meter attributes this widget reads, as named in `MasterProperty`.
pub mod attr {
    /// `0x67 UICore_Meter_frame_meter`.
    pub const FRAME_METER: u32 = 0x67;
    /// `0x68 UICore_Meter_move_fill`.
    pub const MOVE_FILL: u32 = 0x68;
    /// `0x69 UICore_Meter_position`.
    pub const POSITION: u32 = 0x69;
    /// `0x6F UICore_Meter_child_direction`.
    pub const CHILD_DIRECTION: u32 = 0x6F;
}

#[derive(Debug)]
pub struct Meter {
    /// The displayed position, 0..1. `position` is the attribute itself (`0x69`); the client
    /// re-reads it as a float attribute on every draw, and this mirror is written by
    /// [`Meter::on_set_attribute`] so the draw path needs no property lookup.
    pub position: f32,
    pub anim_start_pos: f32,
    pub anim_end_pos: f32,
    pub anim_start_time: f64,
    pub anim_end_time: f64,
    pub animating: bool,
    /// Frame-meter mode — index a strip instead of revealing a child.
    pub frame_meter: bool,
    /// Attribute `0x68` `UICore_Meter_move_fill`.
    pub move_fill: bool,
    pub current_frame: i32,
    pub frame_count: i32,
    /// The fill direction. The constructor sets **1**, and attribute `0x6F` has no
    /// default in the retail `MasterProperty`, so a meter that declares none fills leftwards —
    /// which is what the three vitals bars do.
    pub direction: u32,
}

impl Default for Meter {
    /// Behavior: the members it sets by hand.
    fn default() -> Self {
        Self {
            position: 0.0,
            anim_start_pos: 0.0,
            anim_end_pos: 0.0,
            anim_start_time: -1.0,
            anim_end_time: -1.0,
            animating: false,
            frame_meter: false,
            move_fill: false,
            current_frame: -1,
            frame_count: 0,
            direction: direction::LEFT,
        }
    }
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Meter::default())
}

impl Meter {
    /// The eased position at `now`, using the UI animation's 0..1024 curve.
    #[must_use]
    pub fn eased(&self, table: &[i16; 100], now: f64) -> f32 {
        let span = self.anim_end_time - self.anim_start_time;
        #[allow(clippy::cast_possible_truncation)] // the original's curve input is a float
        let t = if span <= 0.0 {
            1.0_f32
        } else {
            ((now - self.anim_start_time) / span) as f32
        };
        let level = f32::from(crate::media::anim_level(table, t)) / 1024.0;
        self.anim_start_pos + (self.anim_end_pos - self.anim_start_pos) * level
    }
}

impl Meter {
    /// The clip box the meter's child draw gives the child image.
    ///
    /// The client's own switch, with `w = box.width()` and `h = box.height()` (inclusive, so
    /// `x1 - x0 + 1`), `l` = float attribute `0x69`, and `trunc` truncating toward zero:
    ///
    /// | direction | what it writes |
    /// |---|---|
    /// | 1 | `x1 = x0 + trunc(w·l) - 1` |
    /// | 2 | `y1 = y0 + trunc(h·l) - 1` |
    /// | 3 | `x0 += trunc(w·(1-l))`, then `x1 = x0 + trunc(w·l) - 1` |
    /// | 4 | `y0 += trunc(h·(1-l))`, then `y1 = y0 + trunc(h·l) - 1` |
    /// | other | the box is left alone |
    ///
    /// and the result is then intersected with the clip the child already had — so
    /// a level above 1.0 cannot make the fill bigger than its own graphic. That matters: a
    /// server can hand out `cur > max` (a recorded session reaches mana 111 against a computed
    /// maximum of 100), and the client does not clamp `cur/max` either.
    ///
    /// Returns `None` — no narrowing — for a frame meter (which has no child image at all),
    /// for any child that is not id 2, and for a `move_fill` meter, whose child-update
    /// arm moves the child instead. `move_fill` remains unsupported; no shipped layout
    /// sets the attribute.
    #[must_use]
    pub fn child_clip(&self, child: crate::ElementId, box_: Box2D) -> Option<Box2D> {
        if self.frame_meter || self.move_fill || child != CHILD_IMAGE || !box_.is_valid() {
            return None;
        }
        #[allow(clippy::cast_precision_loss)]
        // the client's own operands are `long`s in an FMUL
        let (w, h) = (box_.width() as f32, box_.height() as f32);
        let l = self.position;
        let mut b = box_;
        match self.direction {
            direction::LEFT => b.x1 = b.x0 + dereth_primitives::num::to_i32(w * l) - 1,
            direction::TOP => b.y1 = b.y0 + dereth_primitives::num::to_i32(h * l) - 1,
            direction::RIGHT => {
                b.x0 += dereth_primitives::num::to_i32(w * (1.0 - l));
                b.x1 = b.x0 + dereth_primitives::num::to_i32(w * l) - 1;
            }
            direction::BOTTOM => {
                b.y0 += dereth_primitives::num::to_i32(h * (1.0 - l));
                b.y1 = b.y0 + dereth_primitives::num::to_i32(h * l) - 1;
            }
            _ => return None,
        }
        Some(b.intersect(&box_))
    }
}

impl Element for Meter {
    fn listen_to_global_message(&mut self, ctx: &mut ElemCtx<'_>, id: MessageId, _p: u32) {
        if id != crate::msg::global::TICK || !self.animating {
            return;
        }
        // The tick carries no time; the host advances the meter through `advance`.
        if self.anim_end_time <= self.anim_start_time {
            self.position = self.anim_end_pos;
            self.animating = false;
            ctx.ui.want_tick(ctx.me, false);
            ctx.ui
                .broadcast_element_message(ctx.me, msgid::METER_ANIM_END, 0, 0);
        }
    }

    /// The meter's attribute setter for `0x69`, plus the three members
    /// its initialisation reads once.
    ///
    /// Initialisation reads bool attribute `0x67` and enum attribute `0x6F` **once**, and
    /// `UiSystem::initialize` step 4 replays every merged property through here, which is the
    /// same read at the same moment. A property that later *disappears* (`v == None`) is
    /// ignored for those three, because the client caches them and never looks again.
    fn on_set_attribute(
        &mut self,
        _c: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::PropertyValue>,
    ) {
        match (id, v) {
            (attr::POSITION, Some(crate::PropertyValue::Float(f))) => self.position = *f,
            (attr::FRAME_METER, Some(crate::PropertyValue::Bool(b))) => self.frame_meter = *b,
            (attr::MOVE_FILL, Some(crate::PropertyValue::Bool(b))) => self.move_fill = *b,
            (attr::CHILD_DIRECTION, Some(crate::PropertyValue::Enum(e))) => self.direction = *e,
            _ => {}
        }
    }

    fn child_clip(&self, child: crate::ElementId, child_screen: Box2D) -> Option<Box2D> {
        Self::child_clip(self, child, child_screen)
    }
}

impl Meter {
    /// Advance to `now`; ends the animation and unregisters when it is done.
    pub fn advance(&mut self, ui: &mut UiSystem, me: ElemHandle, now: f64) {
        if !self.animating {
            return;
        }
        let table = ui.easing;
        self.position = self.eased(&table, now);
        if now >= self.anim_end_time {
            self.position = self.anim_end_pos;
            self.animating = false;
            ui.want_tick(me, false);
            ui.broadcast_element_message(me, msgid::METER_ANIM_END, 0, 0);
        }
    }
}
