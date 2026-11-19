//! What the player examines, as Horizon's examine window shows it: the examination's request, the
//! profile the scripts gave for it, and the profile's lines on the tabs asked for, written out.
//! Followed once a frame from the notices.
//!
//! A new request empties the window until its profile comes. A profile's lines show on the tabs
//! its request asked for (the first tab's too when none of those has a line); an effect's line
//! shows the effect's own lines in its place, but for the effects among them. The player's options
//! say whether the window is the classic one (every tab's lines on one page) or the tabbed one;
//! until they are read it is the classic one, as the game's own starts.

use auberean_client_sdk::contract::notice::examination::{line, tab};
use auberean_client_sdk::contract::notice::{
    ExaminationLine, ExaminationNotice, ExaminationProfile, ExaminationRequest, Said,
};
use auberean_client_sdk::contract::PlayerOption;

use crate::state::{Frame, GameState};

/// A piece of a profile's lines, laid out left to right.
#[derive(Clone, Debug, PartialEq)]
pub enum Piece {
    /// Words or a number; none while its strings are not here.
    Text(Option<String>),
    /// A picture of the game's records.
    Picture(u32),
    /// An icon's layers, bottom first: each a picture and the color it is tinted by.
    Icon(Vec<(u32, u32)>),
    /// What follows starts a new line.
    Break,
    /// What follows moves on by this many indents.
    Indent(u32),
    /// The time left until the game's time given.
    Countdown(f64),
}

/// What is examined.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Examined {
    /// The request examined now, and the profile given for it.
    pub request: Option<ExaminationRequest>,
    pub profile: Option<ExaminationProfile>,
    /// The scripts asked for the window to open, this frame.
    pub open_asked: bool,
    /// The profile's lines on the tabs asked for.
    pub pieces: Vec<Piece>,
    /// The tabs asked for, and those the profile has lines for.
    pub asked: u32,
    pub made: u32,
    /// The player's option of the classic window, once the options are read.
    pub classic_option: Option<bool>,
}

impl Examined {
    /// Whether the window is the classic one: as the player's options say, else (before they are
    /// read) yes.
    #[must_use]
    pub fn classic(&self) -> bool {
        self.classic_option.unwrap_or(true)
    }
}

/// A line's own words: a number, a float with two decimals, yes or no, its text, a 64-bit number.
fn words(l: &ExaminationLine, f: &Frame<'_>) -> Option<Option<String>> {
    #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
    let low = l.value as u32;
    Some(Some(match l.kind {
        #[allow(clippy::cast_possible_wrap)]
        line::INT => (low as i32).to_string(),
        line::FLOAT => format!("{:.2}", f32::from_bits(low)),
        line::BOOL => if low == 0 { "No" } else { "Yes" }.to_owned(),
        line::STRING => return Some(l.text.as_ref().and_then(f.write)),
        #[allow(clippy::cast_possible_wrap)]
        line::LONG => (l.value as i64).to_string(),
        _ => return None,
    }))
}

/// One line's pieces, onto `out`.
fn lay_out(l: &ExaminationLine, f: &Frame<'_>, out: &mut Vec<Piece>) {
    #[allow(clippy::cast_possible_truncation)]
    let low = l.value as u32;
    match l.kind {
        line::INT | line::FLOAT | line::BOOL | line::STRING | line::LONG => {
            if let Some(w) = words(l, f) {
                out.push(Piece::Text(w));
            }
        }
        line::IMAGE => out.push(Piece::Picture(low)),
        line::ICON => out.push(Piece::Icon(l.icon.clone())),
        line::BREAK => out.push(Piece::Break),
        line::INDENT => out.push(Piece::Indent(low.max(1))),
        line::COUNTDOWN => out.push(Piece::Countdown(l.time)),
        line::EFFECT => {
            for sub in l.effect.iter().filter(|s| s.kind != line::EFFECT) {
                lay_out(sub, f, out);
            }
        }
        _ => {}
    }
}

impl Examined {
    /// The profile's lines laid out again: those on the tabs its request asked for (the first
    /// tab's added when none of those has a line).
    fn lay_out(&mut self, f: &Frame<'_>) {
        self.pieces.clear();
        let Some(p) = &self.profile else {
            self.asked = self.request.map_or(tab::GENERAL, |r| r.tabs);
            self.made = 0;
            return;
        };
        let mut asked = p.request.tabs;
        if asked & p.tabs == 0 {
            asked |= tab::GENERAL;
        }
        self.asked = asked;
        self.made = p.tabs;
        for l in p.lines.iter().filter(|l| l.tabs & asked != 0) {
            lay_out(l, f, &mut self.pieces);
        }
    }
}

/// What is examined followed for one frame.
pub fn follow(state: &mut GameState, f: &Frame<'_>) {
    let e = &mut state.examined;
    e.open_asked = false;
    if let Some(on) = f
        .view
        .and_then(|v| v.options.as_ref())
        .and_then(|o| o.flag(PlayerOption::ClassicExamination))
    {
        e.classic_option = Some(on);
    }
    for n in f.notices {
        let Said::Examination(x) = &n.said else {
            continue;
        };
        match x {
            ExaminationNotice::Profile(Some(p)) => e.profile = Some(p.clone()),
            ExaminationNotice::RequestChanged(Some(r)) => {
                e.request = Some(*r);
                e.profile = None;
            }
            ExaminationNotice::Open => e.open_asked = true,
            _ => {}
        }
    }
    e.lay_out(f);
}
