//! The hotbar: the shortcut bar's set shown, ten slots a row over the parameter bar (as many rows
//! as the player's options give, up to four), the arrows that show the next and the previous set
//! with the set's number between them, and the attack button past them, on the bar's right.
//!
//! - **A slot** shows what it holds (its icon, or a plain tile), its key in its corner on the first
//!   row while the player numbers the slots (the set's number by the arrows too), a skill's wait as a shade drawing back, a skill or item the player cannot use now
//!   shaded whole, and a skill's special attack as a rim: amber while asked for, gold once started,
//!   blue while turned on. Over it, its name. A slot past the hundredth is never used.
//! - **A click** on a slot uses it; a right click examines what it holds and opens the examine
//!   window.
//! - **Dragging** a slot onto another moves its shortcut there; off the bar, it empties its slot.
//!   While the slots are locked a slot is dragged only with Shift held, as the game's own bar's
//!   are: moved off without it, the press neither drags nor uses the slot. A skill dragged from
//!   the skill window onto a slot puts it there.
//! - **The attack button** says whether the player is attacking, as the game's own lit axe and
//!   dove do: the art's dove while it is not, and its axe, glowing, while it is; no words and no
//!   frame, the icon on a soft round ground with room round it. A click asks the game to start
//!   attacking (the game picks the nearest foe first for a player who targets on its own) or to
//!   stop. There is no other way to fight to choose: the game puts the player into combat and back
//!   to peace itself.

use auberean_client_sdk::contract::notice::examination::kind as examine;
use auberean_client_sdk::contract::{shortcut_kind, Action, ToolbarShortcut, TOOLBAR_SLOTS};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::{Rect, WHITE};
use crate::state::{Bar, GameState, Light, Slot};
use crate::ui::Tip;

/// A slot's side and the step between slots, in layout units.
pub const SLOT: f32 = 44.0;
pub const STEP: f32 = 46.0;

/// The slots in a row.
pub const PER_ROW: u32 = 10;

/// How far the pointer must move with a slot held for the slot to be dragged, in pixels.
pub const DRAG_SLOP: f32 = 4.0;

/// The rims of a slot's special attack: asked for, started, turned on.
pub const ASKED: u32 = 0xFFE0_9A30;
pub const STARTED: u32 = 0xFFFF_E070;
pub const TOGGLED: u32 = 0xFF60_C8F0;

/// The shade over a slot waiting or not usable now, and the tile of an empty slot's or a slot
/// never used.
pub const SHADE: u32 = 0xA000_0000;
pub const TILE: u32 = 0xA018_1C22;
pub const UNUSED: u32 = 0x6010_1014;

/// The keys written in the first row's corners.
const KEYS: [&str; 10] = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "0"];

/// Where the slot at `row` (0 the set shown, the rows over it after) and `column` is, on a screen
/// drawn at `k` pixels a layout unit.
#[must_use]
pub fn slot_rect(screen: (f32, f32), k: f32, row: u32, column: u32) -> Rect {
    #[allow(clippy::cast_precision_loss)]
    let width = PER_ROW as f32 * STEP - (STEP - SLOT);
    let x0 = (screen.0 / 2.0 - width * k / 2.0).round();
    #[allow(clippy::cast_precision_loss)]
    let (row, column) = (row as f32, column as f32);
    Rect::new(
        x0 + column * STEP * k,
        screen.1 - (104.0 + row * STEP) * k,
        SLOT * k,
        SLOT * k,
    )
}

/// The attack button's icon side and the room round it on its ground, in layout units.
pub const ATTACK_ICON: f32 = 32.0;
pub const ATTACK_PAD: f32 = 6.0;

/// The attack button's soft round ground behind its icon.
pub const ATTACK_GROUND: u32 = 0x7010_0C08;

/// Where the attack button is: right of the set's number, clear of the log window on the bar's
/// left, its icon and the room round it, centered on the slots' row.
#[must_use]
pub fn attack_rect(screen: (f32, f32), k: f32) -> Rect {
    let last = slot_rect(screen, k, 0, PER_ROW - 1);
    let side = (ATTACK_ICON + 2.0 * ATTACK_PAD) * k;
    Rect::new(
        last.right() + 56.0 * k,
        last.y + (last.h - side) / 2.0,
        side,
        side,
    )
}

/// The art's icon the attack button shows: the axe while the player attacks, else the dove.
#[must_use]
pub const fn attack_icon(attacking: bool) -> &'static str {
    if attacking {
        "attack.on"
    } else {
        "attack.off"
    }
}

/// Where the attack button's icon is: the button `r` less the room round it.
#[must_use]
pub fn attack_icon_rect(r: Rect, k: f32) -> Rect {
    r.inset(ATTACK_PAD * k)
}

/// The attack button over `r`: the dove while the player is not attacking and the axe while it
/// is, on its soft round ground, the icon glowing while the player attacks or the pointer is over
/// it; where the art has no icon, a plain button with its word. Whether it was clicked.
fn attack_button(p: &mut Painter<'_>, ctx: &mut Ctx<'_>, r: Rect, attacking: bool) -> bool {
    let icon = attack_icon(attacking);
    let Some(picture) = p.piece(icon) else {
        let label = if attacking { "Attacking" } else { "Attack" };
        return kit::button_lit(p, ctx, r, label, true, attacking);
    };
    let k = p.scale;
    let over = ctx.over(&r);
    let radius = r.w / 2.0;
    dereth_ui_kit::hud::ring(
        p,
        (r.x + radius, r.y + r.h / 2.0),
        radius,
        radius,
        ATTACK_GROUND,
    );
    let at = attack_icon_rect(r, k);
    if attacking || over {
        crate::ui::hud::glow(p, icon, at);
    }
    let pressed = if over && ctx.input.down[0] { k } else { 0.0 };
    p.sprite(&picture, at.offset(0.0, pressed), WHITE);
    over && ctx.input.clicked(&r)
}

/// Where the arrows to the set before and the set after are, and the set's number between them.
#[must_use]
pub fn set_rects(screen: (f32, f32), k: f32) -> (Rect, Rect, Rect) {
    let last = slot_rect(screen, k, 0, PER_ROW - 1);
    let x = last.right() + 8.0 * k;
    (
        Rect::new(x, last.y, 16.0 * k, 14.0 * k),
        Rect::new(x, last.bottom() - 14.0 * k, 16.0 * k, 14.0 * k),
        Rect::new(x + 18.0 * k, last.y, 20.0 * k, last.h),
    )
}

/// A shortcut held by the pointer: what it is, the slot it came from (none: the skill window's),
/// its icon, where the press was, and whether it moved far enough to be dragged.
#[derive(Clone, Debug, PartialEq)]
pub struct Drag {
    pub shortcut: ToolbarShortcut,
    pub from: Option<u32>,
    pub icon: u32,
    pub origin: (f32, f32),
    pub active: bool,
}

impl Drag {
    /// Whether the held shortcut may be dragged once the pointer moves far enough: from the skill
    /// window always; from a slot of a locked bar only with Shift held.
    #[must_use]
    pub const fn may_drag(&self, locked: bool, shift: bool) -> bool {
        self.from.is_none() || !locked || shift
    }
}

/// An examination a right click on a slot asks for: the request's kind and its datum.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Examine {
    pub kind: u32,
    pub data_enum: u32,
    pub item: u64,
    pub data_id: u32,
}

impl Examine {
    /// The examination's request for what a slot holds; none for what has none.
    #[must_use]
    pub fn of(slot: &Slot) -> Option<Self> {
        let s = &slot.shortcut;
        let (kind, data_enum, item, data_id) = match s.kind {
            shortcut_kind::SKILL => (examine::SKILL, s.data_enum, 0, 0),
            shortcut_kind::ITEM => (examine::ITEM, 0, s.item, 0),
            shortcut_kind::RECIPE => (examine::RECIPE, 0, 0, s.data_id),
            _ => return None,
        };
        Some(Self {
            kind,
            data_enum,
            item,
            data_id,
        })
    }

    /// The action that asks for it: an item examined is the selection too.
    #[must_use]
    pub const fn action(self) -> Action {
        Action::Examine {
            kind: self.kind,
            data_enum: self.data_enum,
            item: self.item,
            data_id: self.data_id,
            selection: self.item,
        }
    }
}

/// What the hotbar asked for in a frame.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Asked {
    pub actions: Vec<Action>,
    /// A right click asked to examine what a slot holds.
    pub examine: Option<Examine>,
}

/// The hotbar.
#[derive(Debug, Default)]
pub struct Hotbar {
    /// The slots drawn last frame, each where it is and its number.
    drawn: Vec<(Rect, u32)>,
}

impl Hotbar {
    /// The slot under the pointer at `(x, y)` as last drawn, by its number.
    #[must_use]
    pub fn slot_at(&self, x: f32, y: f32) -> Option<u32> {
        self.drawn
            .iter()
            .find(|(r, _)| r.contains(x, y))
            .map(|(_, n)| *n)
    }

    /// The slots as last drawn, moved to where `place` shows them (the HUD layout's placement).
    pub fn place(&mut self, place: impl Fn(Rect) -> Rect) {
        for (r, _) in &mut self.drawn {
            *r = place(*r);
        }
    }

    /// Whether `(x, y)` is over the bar as last drawn.
    #[must_use]
    pub fn over(&self, x: f32, y: f32) -> bool {
        self.slot_at(x, y).is_some()
    }

    /// Draw the bar and the attack button; what answers the pointer, and what was asked.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        asked: &mut Asked,
    ) -> Vec<Rect> {
        let k = p.scale;
        let mut covers = Vec::new();
        self.drawn.clear();
        let attack = attack_rect(p.screen, k);
        let attacking = state.combat.attacking;
        if attack_button(p, ctx, attack, attacking) {
            asked.actions.push(Action::Attack { start: !attacking });
        }
        if ctx.over_quiet(&attack) {
            let lines = if attacking {
                ["You are attacking.", "Stop attacking the selected target."]
            } else {
                [
                    "You are not attacking.",
                    "Start attacking the selected target.",
                ]
            };
            *tip = Some(Tip {
                title: "Attack".into(),
                lines: lines.map(Into::into).to_vec(),
            });
        }
        covers.push(attack);
        let Some(bar) = &state.bar else {
            return covers;
        };
        let first = bar.first();
        let keys = TextStyle::new(Family::Numerals, 10.0, 0xFFEE_E1C5).edge(0xFF00_0000);
        let waits = TextStyle::new(Family::Numerals, 13.0, WHITE).edge(0xFF00_0000);
        for row in 0..bar.rows {
            for column in 0..PER_ROW {
                let n = first + row * PER_ROW + column;
                let r = slot_rect(p.screen, k, row, column);
                covers.push(r);
                if n >= TOOLBAR_SLOTS {
                    p.fill(r, UNUSED);
                    continue;
                }
                self.drawn.push((r, n));
                let slot = bar.slot(n).cloned().unwrap_or_default();
                self.slot(p, ctx, state, &slot, n, r, drag, tip, asked);
                if let Some((total, ready)) = slot.wait {
                    let left = (ready - state.game_time).max(0.0);
                    if left > 0.0 {
                        #[allow(clippy::cast_possible_truncation)]
                        let share = (left / total.max(0.001)).clamp(0.0, 1.0) as f32;
                        p.fill(Rect::new(r.x, r.y, r.w, r.h * share), SHADE);
                        p.text_in(&waits, r, Align::Centre, &crate::ui::hud::time_left(left));
                    }
                }
                if row == 0 && bar.numbers {
                    p.text(&keys, r.x + 3.0 * k, r.y + 1.0 * k, KEYS[column as usize]);
                }
            }
        }
        let (before, after, number) = set_rects(p.screen, k);
        if kit::step_button(p, ctx, before, true, "", true) {
            asked.actions.push(Action::ToolbarSet {
                set: bar.stepped(-1),
            });
        }
        if kit::step_button(p, ctx, after, false, "", true) {
            asked.actions.push(Action::ToolbarSet {
                set: bar.stepped(1),
            });
        }
        if bar.numbers {
            let set = TextStyle::new(Family::Numerals, 14.0, 0xFFEE_E1C5).edge(0xFF00_0000);
            p.text_in(&set, number, Align::Centre, &bar.set.to_string());
        }
        covers.extend([before, after, number]);
        covers
    }

    /// One slot drawn, and what the pointer does to it.
    #[allow(clippy::too_many_arguments)]
    fn slot(
        &self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        slot: &Slot,
        n: u32,
        r: Rect,
        drag: &mut Option<Drag>,
        tip: &mut Option<Tip>,
        asked: &mut Asked,
    ) {
        let k = p.scale;
        let icon = (!slot.is_empty()).then(|| p.art.icon(slot.icon)).flatten();
        if icon.is_none() {
            p.fill(r, TILE);
        }
        // Held for a drag, the slot shows as empty under the pointer's copy.
        let lifted = drag.as_ref().is_some_and(|d| d.active && d.from == Some(n));
        let tint = if slot.unusable || lifted {
            0x80FF_FFFF
        } else {
            WHITE
        };
        let pressed = kit::icon_slot(p, ctx, r, icon.as_ref(), tint);
        if slot.unusable && !slot.is_empty() {
            p.fill(r, SHADE);
        }
        if slot.shortcut.kind == shortcut_kind::SKILL {
            match state.combat.light(slot.shortcut.data_enum) {
                Light::Plain => {}
                Light::Asked => p.outline(r, 2.0 * k, ASKED),
                Light::Started => {
                    p.fill(r, 0x40FF_E070);
                    p.outline(r, 3.0 * k, STARTED);
                }
                Light::Toggled => p.outline(r, 2.0 * k, TOGGLED),
            }
        }
        if !slot.is_empty() && ctx.over_quiet(&r) && drag.is_none() {
            *tip = Some(Tip {
                title: slot.name.clone(),
                lines: Vec::new(),
            });
        }
        if pressed && !slot.is_empty() && drag.is_none() {
            *drag = Some(Drag {
                shortcut: slot.to_shortcut(),
                from: Some(n),
                icon: slot.icon,
                origin: ctx.input.mouse,
                active: false,
            });
        }
        if !slot.is_empty() && ctx.input.right_clicked(&r) {
            asked.examine = Examine::of(slot);
        }
    }

    /// A held shortcut let go of at `(x, y)` (`shift` held): dragged onto a slot, it goes there
    /// (out of the slot it came from); dragged off the bar, its slot empties (while the slots are
    /// locked, only with Shift); not dragged, its slot is used.
    #[must_use]
    pub fn drop(&self, d: &Drag, (x, y): (f32, f32), shift: bool, bar: &Bar) -> Vec<Action> {
        let empty = ToolbarShortcut::default();
        if !d.active {
            return d
                .from
                .map(|slot| Action::UseToolbarSlot { slot })
                .into_iter()
                .collect();
        }
        match (self.slot_at(x, y), d.from) {
            (Some(to), Some(from)) if to == from => Vec::new(),
            (Some(to), from) => {
                let mut out = Vec::new();
                if let Some(from) = from {
                    out.push(Action::SetToolbarSlot {
                        slot: from,
                        shortcut: empty,
                    });
                }
                out.push(Action::SetToolbarSlot {
                    slot: to,
                    shortcut: d.shortcut.clone(),
                });
                out
            }
            (None, Some(from)) if !bar.locked || shift => vec![Action::SetToolbarSlot {
                slot: from,
                shortcut: empty,
            }],
            (None, _) => Vec::new(),
        }
    }
}

/// The held shortcut's icon under the pointer, once it is dragged.
pub fn draw_drag(p: &mut Painter<'_>, d: &Drag, mouse: (f32, f32)) {
    if !d.active {
        return;
    }
    let k = p.scale;
    let r = Rect::new(
        mouse.0 - SLOT * k / 2.0,
        mouse.1 - SLOT * k / 2.0,
        SLOT * k,
        SLOT * k,
    );
    match p.art.icon(d.icon) {
        Some(i) => p.sprite(&i, r, 0xE0FF_FFFF),
        None => p.fill(r, TILE),
    }
}
