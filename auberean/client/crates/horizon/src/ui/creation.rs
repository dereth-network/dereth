//! Character creation: the race and sex, the appearance (a slider for each kind of choice the race
//! has, Randomize for all of them), the name, and Create, which makes the character and enters the
//! world with it once the game says it is made. The figure stands in the middle as it will look;
//! a drag across it turns it. Back returns to character select, and the avatars are thrown away.

use auberean_client_sdk::Action;
use auberean_world::chargen;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx};

use crate::creation::{Creation, NAME_LIMIT};
use crate::draw::Rect;
use crate::ui::pregame::PreOut;

/// How far a drag across the figure turns it, in degrees a pixel.
pub const TURN_PER_PIXEL: f32 = 0.5;

/// The name box's focus id.
const NAME_BOX: u32 = 1;

/// Where the window's parts are.
#[derive(Clone, Debug, PartialEq)]
pub struct CreationLayout {
    /// The panel of choices down the left.
    pub panel: Rect,
    /// The races' buttons, in [`Creation::races`]'s order, and the sexes'.
    pub races: Vec<Rect>,
    pub sexes: [Rect; 2],
    /// The first appearance row; the rest follow at [`ROW`] layout units apart.
    pub first_row: Rect,
    pub randomize: Rect,
    /// The administrator's box, where the client offers it.
    pub admin: Rect,
    pub name: Rect,
    pub create: Rect,
    pub back: Rect,
    /// Where the figure stands.
    pub figure: Rect,
}

/// How far a race's name stands clear of each end of its button at the least, in layout units:
/// past the button's rim, with room to breathe.
pub const LABEL_PAD: f32 = 14.0;

/// An appearance row's height, in layout units.
pub const ROW: f32 = 30.0;

impl CreationLayout {
    /// The parts on a screen of `screen` pixels at `k` pixels a layout unit, `labels` how wide
    /// each race's button's label is drawn (screen pixels): each button wide enough for its own
    /// label, the room left over shared out evenly.
    #[must_use]
    pub fn new(screen: (f32, f32), k: f32, labels: &[f32]) -> Self {
        let (sw, sh) = screen;
        // Wide enough for all five races' buttons in a row, each name clear of its rim.
        let panel = Rect::new(48.0 * k, 150.0 * k, 540.0 * k, sh - 270.0 * k);
        let inner = panel.x + 20.0 * k;
        let w = panel.w - 40.0 * k;
        let gap = 6.0 * k;
        #[allow(clippy::cast_precision_loss)]
        let n = labels.len().max(1) as f32;
        let room = w - gap * (n - 1.0);
        let needed: Vec<f32> = labels.iter().map(|l| l + 2.0 * LABEL_PAD * k).collect();
        let sum: f32 = needed.iter().sum();
        let widths: Vec<f32> = if sum <= room {
            let extra = (room - sum) / n;
            needed.iter().map(|w| w + extra).collect()
        } else {
            needed.iter().map(|w| w * room / sum).collect()
        };
        let mut x = inner;
        let race_rects = widths
            .iter()
            .map(|rw| {
                let r = Rect::new(x.round(), panel.y + 50.0 * k, rw.round(), 28.0 * k);
                x += rw + gap;
                r
            })
            .collect();
        let sw2 = (w - 6.0 * k) / 2.0;
        let sexes = [
            Rect::new(inner, panel.y + 86.0 * k, sw2, 28.0 * k),
            Rect::new(inner + sw2 + 6.0 * k, panel.y + 86.0 * k, sw2, 28.0 * k),
        ];
        let first_row = Rect::new(inner, panel.y + 160.0 * k, w, 24.0 * k);
        let randomize = Rect::new(inner, panel.bottom() - 46.0 * k, 140.0 * k, 30.0 * k);
        let admin = Rect::new(
            randomize.right() + 16.0 * k,
            randomize.y,
            200.0 * k,
            30.0 * k,
        );
        let foot = sh - 100.0 * k;
        let name = Rect::new(sw - 48.0 * k - 360.0 * k, foot, 360.0 * k, 30.0 * k);
        let create = Rect::new(name.x + 200.0 * k, foot + 40.0 * k, 160.0 * k, 34.0 * k);
        let back = Rect::new(name.x, foot + 40.0 * k, 160.0 * k, 34.0 * k);
        let figure = Rect::new(
            panel.right() + 40.0 * k,
            140.0 * k,
            (name.x - panel.right() - 80.0 * k).max(200.0 * k),
            sh - 180.0 * k,
        );
        Self {
            panel,
            races: race_rects,
            sexes,
            first_row,
            randomize,
            admin,
            name,
            create,
            back,
            figure,
        }
    }

    /// The `i`th appearance row.
    #[must_use]
    pub fn row(&self, i: usize, k: f32) -> Rect {
        #[allow(clippy::cast_precision_loss)]
        let y = self.first_row.y + ROW * k * i as f32;
        Rect::new(self.first_row.x, y, self.first_row.w, self.first_row.h)
    }

    /// The slider of the `i`th appearance row.
    #[must_use]
    pub fn slider(&self, i: usize, k: f32) -> Rect {
        let r = self.row(i, k);
        Rect::new(
            r.x + 140.0 * k,
            r.y + 4.0 * k,
            r.w - 210.0 * k,
            r.h - 8.0 * k,
        )
    }
}

/// The creation window's own state.
#[derive(Debug, Default)]
pub struct CreationWindow {
    focus: u32,
    /// Create was pressed: the game is asked, and its answer awaited.
    pub waiting: bool,
    /// Why the last character was not made.
    pub error: Option<String>,
    /// The client offers the administrator's character.
    pub admin_offered: bool,
    /// Where the figure stands this frame, and the mark in the draw list it stands over.
    pub figure: Option<(Rect, usize)>,
    /// A drag turning the figure: the pointer's last place across.
    turning: Option<f32>,
    /// The races' buttons drawn last frame.
    pub races: Vec<Rect>,
}

impl CreationWindow {
    /// Whether the name box has the keys.
    #[must_use]
    pub const fn typing(&self) -> bool {
        self.focus != 0
    }

    /// Back to how it opens.
    pub fn reset(&mut self) {
        let admin_offered = self.admin_offered;
        *self = Self {
            admin_offered,
            ..Self::default()
        };
    }

    /// The game said the character was not made, and why.
    pub fn refused(&mut self, line: String) {
        self.waiting = false;
        self.error = Some(line);
    }

    /// Draw the window for `c`; `Some(false)` when Back was pressed.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        c: &mut Creation,
        out: &mut PreOut,
    ) -> Option<bool> {
        let k = p.scale;
        let races = c.races();
        let labels: Vec<f32> = races
            .iter()
            .map(|(_, name)| kit::button_label_width(p, name))
            .collect();
        let l = CreationLayout::new(p.screen, k, &labels);
        self.figure = Some((l.figure, p.list.mark()));
        self.turn(ctx, c, l.figure);
        let head = TextStyle::new(Family::Heading, 22.0, ctx.colours.heading()).edge(0xFF00_0000);
        let label = TextStyle::new(Family::Body, 13.0, ctx.colours.text()).edge(ctx.colours.edge());
        let idle = !self.waiting;
        kit::panel(p, l.panel, 0.9);
        p.text(
            &head,
            l.panel.x + 20.0 * k,
            l.panel.y + 12.0 * k,
            "Character Creation",
        );
        self.races.clone_from(&l.races);
        for ((race, name), r) in races.iter().zip(&l.races) {
            let lit = c.chosen.0 == *race;
            if kit::button_lit(p, ctx, *r, name, idle, lit) {
                c.choose(*race, c.chosen.1);
            }
        }
        for ((sex, name), r) in crate::creation::SEXES.iter().zip(&l.sexes) {
            let lit = c.chosen.1 == *sex;
            if kit::button_lit(p, ctx, *r, name, idle, lit) {
                c.choose(c.chosen.0, *sex);
            }
        }
        p.text(
            &TextStyle::new(Family::Heading, 16.0, ctx.colours.heading()),
            l.panel.x + 20.0 * k,
            l.panel.y + 128.0 * k,
            "Appearance",
        );
        for (i, row) in c.rows().iter().enumerate() {
            let r = l.row(i, k);
            p.text_in(
                &label,
                Rect::new(r.x, r.y, 136.0 * k, r.h),
                Align::Left,
                row.name,
            );
            p.text_in(
                &label,
                Rect::new(r.right() - 64.0 * k, r.y, 64.0 * k, r.h),
                Align::Right,
                &format!("{} / {}", row.at + 1, row.stops),
            );
            #[allow(clippy::cast_precision_loss)]
            let t = row.at as f32 / (row.stops - 1).max(1) as f32;
            if let Some(t) = kit::track_slider(p, ctx, l.slider(i, k), t).filter(|_| idle) {
                // The stop nearest the pointer.
                #[allow(
                    clippy::cast_precision_loss,
                    clippy::cast_possible_truncation,
                    clippy::cast_sign_loss
                )]
                let stop =
                    ((t * (row.stops - 1) as f32).round().max(0.0) as usize).min(row.stops - 1);
                if stop != row.at {
                    c.set(row.kind, stop);
                }
            }
        }
        if kit::button(p, ctx, l.randomize, "Random", idle) {
            c.randomize(chargen::ALL);
        }
        if self.admin_offered {
            if let Some(on) = kit::check_row(p, ctx, l.admin, c.admin, &label, "Administrator") {
                c.admin = on;
            }
        }
        p.text(&label, l.name.x, l.name.y - 22.0 * k, "Name");
        let entered = kit::text_box_holding(
            p,
            ctx,
            l.name,
            &mut c.name,
            &mut self.focus,
            NAME_BOX,
            Align::Left,
            NAME_LIMIT,
        );
        if let Some(e) = &self.error {
            let style = TextStyle::new(Family::Body, 13.0, 0xFFFF_9A6A).edge(0xFF00_0000);
            p.text(&style, l.name.x, l.name.y - 46.0 * k, e);
        } else if self.waiting {
            p.text(
                &label,
                l.name.x,
                l.name.y - 46.0 * k,
                "Creating the character...",
            );
        }
        let can_create = idle && !c.trimmed_name().is_empty();
        if (kit::button(p, ctx, l.create, "Create", can_create) || entered && can_create)
            && self.create(c, out)
        {
            self.focus = 0;
        }
        if kit::button(p, ctx, l.back, "Back", idle)
            || idle && self.focus == 0 && ctx.input.take_key(crate::ui::vk::ESCAPE)
        {
            return Some(false);
        }
        None
    }

    /// The character asked for; whether it was.
    fn create(&mut self, c: &Creation, out: &mut PreOut) -> bool {
        let Some(made) = c.character() else {
            return false;
        };
        out.acts.push(Action::CreateCharacter(made));
        self.waiting = true;
        self.error = None;
        true
    }

    /// A drag across the figure turns it.
    fn turn(&mut self, ctx: &Ctx<'_>, c: &mut Creation, figure: Rect) {
        let x = ctx.input.mouse.0;
        if ctx.input.pressed[0] && ctx.input.hover(&figure) {
            self.turning = Some(x);
        }
        if !ctx.input.down[0] {
            self.turning = None;
        }
        if let Some(from) = self.turning {
            c.turn = (c.turn + (x - from) * TURN_PER_PIXEL).rem_euclid(360.0);
            self.turning = Some(x);
        }
    }
}
