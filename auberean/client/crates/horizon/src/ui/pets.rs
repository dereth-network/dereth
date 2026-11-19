//! The pets beside the screen, under the minimap, while the player has pets and its options show
//! them; and the Dismount button beside the attack button while the player rides a mount.
//!
//! - **A pet's row** shows its icon, its name (red for a few seconds after something attacks
//!   it) and its mode between the arrows that turn it. Press a row to select the pet; press it
//!   with the right button to give the pet another name (Enter keeps it, Escape drops it).
//! - **The mode arrows** turn the pet's mode through the game's nine, from the mode last asked:
//!   the row shows the asked mode at once, and it is asked again every few seconds until the pet
//!   takes it.
//! - **Attack** sends the pets at the selection; **Return**, **Buff** and **Heal** command them.

use std::collections::BTreeMap;

use auberean_client_sdk::contract::pet::{self, mode};
use auberean_client_sdk::contract::{Action, PetAction};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::draw::{Argb, Rect, WHITE};
use crate::state::GameState;
use crate::ui::Tip;

/// How long a pet's name shows red after an attack, and how long between asking again for a
/// mode a pet has not taken, in seconds.
pub const ATTACKED_FOR: f64 = 5.0;
pub const ASK_AGAIN: f64 = 5.0;

/// A pet's name while something attacks it.
pub const ATTACKED: Argb = 0xFFE8_6050;

/// The panel's width, a row's height and the buttons' row's, in layout units.
const WIDTH: f32 = 240.0;
const ROW: f32 = 34.0;
const BUTTONS: f32 = 30.0;

/// The name box's own number, for the kit's focus.
const NAME_BOX: u32 = 0x4142_0602;

/// A mode's name.
#[must_use]
pub const fn mode_name(m: u32) -> &'static str {
    match m {
        mode::AGGRESSIVE => "Aggressive",
        mode::OPPORTUNISTIC => "Opportunistic",
        mode::COURAGEOUS => "Courageous",
        mode::PROTECTIVE => "Protective",
        mode::ASSISTING => "Assisting",
        mode::DEFENSIVE => "Defensive",
        mode::SUPPORTING => "Supporting",
        mode::COOPERATIVE => "Cooperative",
        mode::IDLE => "Idle",
        _ => "Other",
    }
}

/// Where the panel's parts are.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Layout {
    pub panel: Rect,
    /// The commands: attack, return, buff, heal.
    pub commands: [Rect; 4],
}

impl Layout {
    /// The parts for `n` pets on a screen of `screen` pixels, at `k` pixels a layout unit: under
    /// the minimap's place line, at the screen's right.
    #[must_use]
    pub fn of(screen: (f32, f32), k: f32, n: usize) -> Self {
        let under = crate::ui::minimap::parts(screen, k).place.bottom() + 16.0 * k;
        let x = screen.0 - (WIDTH + 26.0) * k;
        #[allow(clippy::cast_precision_loss)]
        let h = (n as f32 * ROW + BUTTONS + 8.0) * k;
        let panel = Rect::new(x, under, WIDTH * k, h);
        let w = (WIDTH - 12.0) / 4.0;
        let y = panel.bottom() - (BUTTONS + 2.0) * k;
        let command = |i: f32| Rect::new(x + (2.0 + i * (w + 2.0)) * k, y, w * k, BUTTONS * k);
        Self {
            panel,
            commands: [command(0.0), command(1.0), command(2.0), command(3.0)],
        }
    }

    /// The `n`th pet's row.
    #[must_use]
    pub fn row(&self, n: usize, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = self.panel.y + (2.0 + n as f32 * ROW) * k;
        Rect::new(
            self.panel.x + 2.0 * k,
            y,
            self.panel.w - 4.0 * k,
            (ROW - 2.0) * k,
        )
    }

    /// The arrows that turn the `n`th pet's mode back and on.
    #[must_use]
    pub fn arrows(&self, n: usize, k: f32) -> (Rect, Rect) {
        let r = self.row(n, k);
        (
            Rect::new(r.right() - 112.0 * k, r.y + 16.0 * k, 14.0 * k, 14.0 * k),
            Rect::new(r.right() - 16.0 * k, r.y + 16.0 * k, 14.0 * k, 14.0 * k),
        )
    }
}

/// A mode asked of a pet that it has not taken yet: the mode, and when it was last asked.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Asked {
    mode: u32,
    at: f64,
}

/// The pets beside the screen.
#[derive(Debug, Default)]
pub struct PetsPanel {
    /// The modes asked of the pets, by pet.
    asked: BTreeMap<u64, Asked>,
    /// When each pet was last attacked, on the interface's clock.
    attacked: BTreeMap<u64, f64>,
    /// The pet being given another name, and the name typed.
    naming: Option<(u64, String)>,
    focus: u32,
}

impl PetsPanel {
    /// The mode a pet shows: the one last asked of it, else the one it has taken.
    #[must_use]
    pub fn mode_shown(&self, pet: u64, taken: u32) -> u32 {
        self.asked.get(&pet).map_or(taken, |a| a.mode)
    }

    /// Whether a pet's name shows it attacked at `now`.
    #[must_use]
    pub fn attacked(&self, pet: u64, now: f64) -> bool {
        self.attacked
            .get(&pet)
            .is_some_and(|at| now - at <= ATTACKED_FOR)
    }

    /// Whether the name box has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.naming.is_some()
    }

    /// Where it is, for the pets `state` has; none while it is not shown.
    #[must_use]
    pub fn layout(state: &GameState, screen: (f32, f32), k: f32) -> Option<Layout> {
        let pets = &state.pets;
        pets.shown().then(|| Layout::of(screen, k, pets.pets.len()))
    }

    /// Follow one frame at `now`: the attacks heard, the modes the pets took let go, and those
    /// they have not asked again when it is time.
    pub fn follow(&mut self, state: &GameState, now: f64, acts: &mut Vec<Action>) {
        let pets = &state.pets;
        for p in &pets.attacked {
            self.attacked.insert(*p, now);
        }
        self.attacked.retain(|p, _| pets.pet(*p).is_some());
        self.asked
            .retain(|p, a| pets.pet(*p).is_some_and(|pet| pet.mode != a.mode));
        for (p, a) in &mut self.asked {
            if now - a.at >= ASK_AGAIN {
                a.at = now;
                acts.push(Action::Pet(PetAction::Mode {
                    pet: *p,
                    mode: a.mode,
                }));
            }
        }
        if self
            .naming
            .as_ref()
            .is_some_and(|(p, _)| pets.pet(*p).is_none() || !pets.shown())
        {
            self.naming = None;
        }
    }

    /// A pet's mode turned on or back from the one shown, asked at once.
    fn turn(&mut self, pet: u64, taken: u32, on: bool, now: f64, acts: &mut Vec<Action>) {
        let from = self.mode_shown(pet, taken);
        let to = if on {
            pet::next_mode(from)
        } else {
            pet::previous_mode(from)
        };
        self.asked.insert(pet, Asked { mode: to, at: now });
        acts.push(Action::Pet(PetAction::Mode { pet, mode: to }));
    }

    /// Draw it while it is shown, at `now` on the interface's clock; where it is.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        now: f64,
        tip: &mut Option<Tip>,
        acts: &mut Vec<Action>,
    ) -> Option<Rect> {
        let k = p.scale;
        let l = Self::layout(state, p.screen, k)?;
        kit::panel(p, l.panel, 0.85);
        let selection = state.target.as_ref().map_or(0, |t| t.id);
        let small = TextStyle::new(Family::Body, 12.0, 0xFFC8_C0B0).edge(ctx.colours.edge());
        for (n, pet) in state.pets.pets.iter().enumerate() {
            let row = l.row(n, k);
            kit::list_highlight(p, row, pet.id == selection);
            let icon = Rect::new(row.x + 2.0 * k, row.y + 1.0 * k, 30.0 * k, 30.0 * k);
            if let Some(i) = p.art.icon(pet.icon) {
                p.sprite(&i, icon, WHITE);
            }
            let colour = if self.attacked(pet.id, now) {
                ATTACKED
            } else {
                ctx.colours.text()
            };
            let name = TextStyle::new(Family::Body, 13.0, colour).edge(ctx.colours.edge());
            let field = Rect::new(
                icon.right() + 6.0 * k,
                row.y,
                row.right() - icon.right() - 10.0 * k,
                16.0 * k,
            );
            match &mut self.naming {
                Some((id, typed)) if *id == pet.id => {
                    let entered = kit::text_box(p, ctx, field, typed, &mut self.focus, NAME_BOX);
                    if entered {
                        let typed = typed.trim().to_owned();
                        if !typed.is_empty() && typed.to_lowercase() != pet.name.to_lowercase() {
                            acts.push(Action::Pet(PetAction::Name {
                                pet: pet.id,
                                name: typed,
                            }));
                        }
                        self.naming = None;
                    } else if ctx.input.take_key(crate::ui::vk::ESCAPE) {
                        self.naming = None;
                    }
                }
                _ => {
                    let fitted = p.fit(&name, &pet.name, field.w);
                    p.text_in(&name, field, Align::Left, &fitted);
                }
            }
            let (back, on) = l.arrows(n, k);
            let shown = self.mode_shown(pet.id, pet.mode);
            p.text_in(
                &small,
                Rect::new(back.right(), back.y, on.x - back.right(), back.h),
                Align::Centre,
                mode_name(shown),
            );
            let mut turned = None;
            if kit::button(p, ctx, back, "<", true) {
                turned = Some(false);
            }
            if kit::button(p, ctx, on, ">", true) {
                turned = Some(true);
            }
            if let Some(up) = turned {
                self.turn(pet.id, pet.mode, up, now, acts);
                continue;
            }
            if ctx.input.right_clicked(&row) {
                self.naming = Some((pet.id, pet.name.clone()));
                self.focus = NAME_BOX;
                acts.push(Action::SetSelection { target: pet.id });
            } else if ctx.over(&row) && ctx.input.clicked(&row) {
                acts.push(Action::SetSelection { target: pet.id });
            }
            if ctx.input.hover(&row) {
                *tip = Some(Tip {
                    title: pet.name.clone(),
                    lines: vec![mode_name(pet.mode).to_owned()],
                });
            }
        }
        let commands = [
            ("Attack", Action::Pet(PetAction::Attack(selection))),
            ("Return", Action::Pet(PetAction::Return)),
            ("Buff", Action::Pet(PetAction::Buff)),
            ("Heal", Action::Pet(PetAction::Heal)),
        ];
        for (r, (label, act)) in l.commands.iter().zip(commands) {
            if kit::button(p, ctx, *r, label, true) {
                acts.push(act);
            }
        }
        Some(l.panel)
    }
}

/// Where the Dismount button is: right of the attack button, level with its middle.
#[must_use]
pub fn dismount_rect(screen: (f32, f32), k: f32) -> Rect {
    let attack = crate::ui::hotbar::attack_rect(screen, k);
    let h = 30.0 * k;
    Rect::new(
        attack.right() + 8.0 * k,
        attack.y + (attack.h - h) / 2.0,
        108.0 * k,
        h,
    )
}

/// The Dismount button, while the player rides a mount: pressed, the player gets down; where it
/// is.
pub fn dismount(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    state: &GameState,
    acts: &mut Vec<Action>,
) -> Option<Rect> {
    if !state.pets.mounted {
        return None;
    }
    let r = dismount_rect(p.screen, p.scale);
    if kit::button(p, ctx, r, "Dismount", true) {
        acts.push(Action::Dismount);
    }
    Some(r)
}
