//! The game's questions to the player in Horizon's box: the player died, someone offers to bring
//! the player back, an effect asks whether it may go on, a message to read, a fellowship's place,
//! an oath or a trade offered.
//!
//! One box is up at a time, over the middle of the screen: the first question that goes in front,
//! else the oldest. Each kind of question has its own title (Resurrection, Fellowship, Allegiance,
//! Trade; a message is a Notice, an effect's question a Confirm), but death's, which has none, as
//! the game's has none. A question with one
//! answer has one button (Resurrect for death's, OK for a message), the others Yes and No; the
//! answer goes to the game as the client's action, which does what it asks. What each answer does
//! is the client's, so the box only says which was pressed; once answered, a question is not shown
//! again while the game still lists it.
//!
//! **Log Out** asks its own question in the same box before the player leaves the world, in the
//! game's words ("Are you sure you want to log out?"), titled Log Out, Yes and No; it stands over
//! everything, the settings too, and Escape takes it down unanswered ([`LogOutQuestion`]).

use auberean_client_sdk::contract::question::Asked;
use auberean_client_sdk::Action;
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Painter, TextStyle};
use dereth_ui_kit::widgets::{self as kit, Ctx, WindowState};

use crate::draw::Rect;
use crate::state::{Asking, GameState};
use crate::ui::Outcome;

/// The box's width, and the room under its words for its buttons, in layout units.
const WIDTH: f32 = 440.0;
const BUTTONS: f32 = 52.0;

/// The questions answered that the game may still list for a frame, and where the box was last
/// drawn.
#[derive(Debug, Default)]
pub struct Questions {
    answered: Vec<u32>,
    drawn: Option<Parts>,
}

/// Where the box's parts are.
#[derive(Clone, Debug, PartialEq)]
pub struct Parts {
    /// The question it asks.
    pub id: u32,
    /// Its title.
    pub title: &'static str,
    /// The box.
    pub rect: Rect,
    /// Its buttons, first to last: the one answer, or Yes then No.
    pub buttons: Vec<Rect>,
}

/// The buttons of a question: its one answer's, or Yes and No.
#[must_use]
pub fn labels(q: &Asking) -> &'static [&'static str] {
    match q.asked {
        Asked::Died => &["Resurrect"],
        _ if q.one_answer => &["OK"],
        _ => &["Yes", "No"],
    }
}

/// The box's title for a question: its kind's own, else Notice for a question with one answer and
/// Confirm for the others.
#[must_use]
pub fn title(q: &Asking) -> &'static str {
    kind_title(&q.asked).unwrap_or(if q.one_answer { "Notice" } else { "Confirm" })
}

/// The title a kind of question has of its own (death's box has none, as the game's own has
/// none): `None` for a kind the box has no title for.
#[must_use]
pub const fn kind_title(asked: &Asked) -> Option<&'static str> {
    Some(match asked {
        Asked::Died => "",
        Asked::Resurrection { .. } => "Resurrection",
        Asked::Effect { .. } => "Confirm",
        Asked::Message { .. } => "Notice",
        Asked::Fellowship { .. } => "Fellowship",
        Asked::Oath { .. } => "Allegiance",
        Asked::Trade { .. } => "Trade",
        _ => return None,
    })
}

impl Questions {
    /// The question up: the first of those not yet answered that goes in front, else the oldest.
    #[must_use]
    pub fn up<'a>(&self, state: &'a GameState) -> Option<&'a Asking> {
        let waiting = || {
            state
                .questions
                .iter()
                .filter(|q| !self.answered.contains(&q.id))
        };
        waiting().find(|q| q.at_once).or_else(|| waiting().next())
    }

    /// Where the box for `q` stands on a screen drawn by `p`: over the middle, as tall as its
    /// words.
    #[must_use]
    pub fn rect(p: &Painter<'_>, q: &Asking) -> Rect {
        box_rect(p, &q.text)
    }

    /// Where the box was last drawn, while one is up.
    #[must_use]
    pub const fn drawn(&self) -> Option<&Parts> {
        self.drawn.as_ref()
    }

    /// Whether a box is up.
    #[must_use]
    pub fn is_up(&self, state: &GameState) -> bool {
        self.up(state).is_some()
    }

    /// Draw the box up, if one is; where it is. A button pressed answers its question.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        state: &GameState,
        out: &mut Outcome,
    ) -> Option<Rect> {
        // Those the game no longer lists are forgotten.
        self.answered
            .retain(|id| state.questions.iter().any(|q| q.id == *id));
        self.drawn = None;
        let q = self.up(state)?.clone();
        let title = title(&q);
        let (rect, buttons, pressed) = ask_box(p, ctx, title, &q.text, labels(&q));
        if let Some(i) = pressed {
            out.acts.push(Action::AnswerQuestion {
                question: q.id,
                yes: i == 0,
            });
            self.answered.push(q.id);
        }
        self.drawn = Some(Parts {
            id: q.id,
            title,
            rect,
            buttons,
        });
        Some(rect)
    }
}

/// Where a box asking `text` stands on a screen drawn by `p`: over the middle, as tall as its
/// words.
#[must_use]
pub fn box_rect(p: &Painter<'_>, text: &str) -> Rect {
    let k = p.scale;
    let body = TextStyle::new(Family::Body, 14.0, 0);
    let lines = p.wrap(&body, text.trim(), (WIDTH - 40.0) * k).len().max(1);
    #[allow(clippy::cast_precision_loss)]
    let words = p.line_height(&body) / k * lines as f32;
    let size = kit::window_size_for(p, (WIDTH, words + 16.0 + BUTTONS));
    let (sw, sh) = p.screen;
    Rect::new(
        ((sw - size.0 * k) / 2.0).round(),
        ((sh - size.1 * k) / 2.0 - sh * 0.12).round(),
        size.0 * k,
        size.1 * k,
    )
}

/// A box over the middle of the screen with `title`, asking `text`, its buttons `labels` along its
/// foot: where the box and its buttons are, and the button pressed.
pub fn ask_box(
    p: &mut Painter<'_>,
    ctx: &mut Ctx<'_>,
    title: &str,
    text: &str,
    labels: &[&str],
) -> (Rect, Vec<Rect>, Option<usize>) {
    let k = p.scale;
    let rect = box_rect(p, text);
    let mut window = WindowState {
        open: true,
        pos: Some((rect.x, rect.y)),
        opened_at: ctx.time - 1.0,
        no_close: true,
        ..WindowState::default()
    };
    let w = kit::window(
        p,
        ctx,
        &mut window,
        title,
        (rect.w / k, rect.h / k),
        (rect.x, rect.y),
    );
    let body = TextStyle::new(Family::Body, 14.0, ctx.colours.text()).edge(ctx.colours.edge());
    let mut y = w.body.y + 8.0 * k;
    for line in p.wrap(&body, text.trim(), (WIDTH - 40.0) * k) {
        let lw = p.measure(&body, &line);
        p.text(&body, w.body.x + w.body.w / 2.0 - lw / 2.0, y, &line);
        y += p.line_height(&body);
    }
    // The buttons along the foot of the body, centred.
    let bw = 110.0 * k;
    #[allow(clippy::cast_precision_loss)]
    let n = labels.len() as f32;
    let x0 = w.body.x + w.body.w / 2.0 - (bw * n + 10.0 * k * (n - 1.0)) / 2.0;
    let by = w.body.bottom() - (BUTTONS - 8.0) * k;
    #[allow(clippy::cast_precision_loss)]
    let buttons: Vec<Rect> = (0..labels.len())
        .map(|i| Rect::new(x0 + i as f32 * (bw + 10.0 * k), by, bw, 30.0 * k))
        .collect();
    let mut pressed = None;
    for (i, (b, label)) in buttons.iter().zip(labels).enumerate() {
        if kit::button(p, ctx, *b, label, true) {
            pressed = Some(i);
        }
    }
    (w.rect, buttons, pressed)
}

/// The question Log Out asks before the player leaves the world, in the game's own words: the
/// menu bar's Log Out, the settings' and the key all ask it, and only Yes logs out.
#[derive(Debug, Default)]
pub struct LogOutQuestion {
    up: bool,
    /// The question, from the game's messages; Horizon's own words while they are not at hand.
    pub text: String,
    drawn: Option<(Rect, Vec<Rect>)>,
}

/// The log-out question's words where the game's messages are not at hand.
pub const LOG_OUT_TEXT: &str = "Are you sure you want to log out?";

impl LogOutQuestion {
    /// Ask it.
    pub fn ask(&mut self) {
        self.up = true;
    }

    /// Take it down unanswered.
    pub fn dismiss(&mut self) {
        self.up = false;
        self.drawn = None;
    }

    /// Whether it is up.
    #[must_use]
    pub const fn is_up(&self) -> bool {
        self.up
    }

    /// Its words.
    #[must_use]
    pub fn words(&self) -> &str {
        if self.text.is_empty() {
            LOG_OUT_TEXT
        } else {
            &self.text
        }
    }

    /// Where its box stands on a screen drawn by `p`, while it is up.
    #[must_use]
    pub fn rect(&self, p: &Painter<'_>) -> Option<Rect> {
        self.up.then(|| box_rect(p, self.words()))
    }

    /// Where its box and its buttons (Yes, then No) were last drawn, while it is up.
    #[must_use]
    pub const fn drawn(&self) -> Option<&(Rect, Vec<Rect>)> {
        self.drawn.as_ref()
    }

    /// Draw it, while it is up; where it is. Yes logs out; either answer takes it down.
    pub fn frame(
        &mut self,
        p: &mut Painter<'_>,
        ctx: &mut Ctx<'_>,
        out: &mut Outcome,
    ) -> Option<Rect> {
        self.drawn = None;
        if !self.up {
            return None;
        }
        let text = self.words().to_owned();
        let (rect, buttons, pressed) = ask_box(p, ctx, "Log Out", &text, &["Yes", "No"]);
        match pressed {
            Some(0) => {
                out.log_out = true;
                self.up = false;
            }
            Some(_) => self.up = false,
            None => self.drawn = Some((rect, buttons)),
        }
        Some(rect)
    }
}
