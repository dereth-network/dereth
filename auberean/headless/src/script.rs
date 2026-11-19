//! The headless client's script: one command per line, the smallest text form that drives the
//! client with no window.
//!
//! A line is a verb and its words, separated by runs of whitespace; `#` starts a comment that runs
//! to the end of the line, and a blank line is nothing. `say` keeps everything after its verb as
//! its text, a `#` included. A number is decimal, or hexadecimal after `0x`; an object is named by
//! its id.
//!
//! | Line | What it does |
//! |---|---|
//! | `login` | waits for the account's characters to be listed |
//! | `create <name>` | makes a character of that name, unless the list has one; waits for the list with it |
//! | `enter [<name> \| <id>]` | enters the world as the character (the first listed when none is named), asking again while the server refuses for the character still being in the world; waits until in the world |
//! | `tick <n>` | runs `n` frames |
//! | `run <secs>` | runs frames for `secs` seconds of the clock |
//! | `at <secs>` | runs frames until `secs` seconds after entering the world (after starting, before) |
//! | `act <action> <words>...` | one of the contract's actions, in its text form ([`crate::act`]) |
//! | `begin <control>` / `end <control>` | presses a control, or lets it go, as a key would |
//! | `press <control>` | presses a control for one frame and lets it go: a key tapped |
//! | `hold <control> <secs>` | presses a control, runs `secs`, lets it go |
//! | `walk <movement> <secs>` | `hold` for a movement: `forward`, `backward`, `turn-left`, `turn-right`, `strafe-left`, `strafe-right` |
//! | `say <text>` | types a line into the chat box (a plugin's command first, when one claims it) |
//! | `select <kind>` | the nearest of a kind: `creature`, `item`, `player`, `foe`, `fellow`, `pet` |
//! | `select next [<kind>]` / `select previous [<kind>]` | the next or the one before of a kind, as the selection keys step (creatures when none is given) |
//! | `select self` / `select last` / `select attacker` / `select none` / `select <id>` | the player, the selection before, the last to attack the player, nothing, an object |
//! | `use [<id>]` | uses what is selected (the object first selected, when one is named) |
//! | `craft <recipe> <spinner>` | makes a recipe: reads it as the ingredient panel does, fills each slot with the first item the scripts say fits, and makes it |
//! | `slots` | what the last making's slots hold now |
//! | `agree on\|off` | takes, or no longer takes, every place in a fellowship, every oath and every trade offered, and accepts a trade once the other side has |
//! | `offer on\|off` | offers, or no longer offers, the first item of the pack when a trade opens |
//! | `snapshot` | the view, as text |
//! | `dump events` / `dump notices` | the events, or the scripts' notices, since the last dump |
//! | `position` | where the player's body stands, or `none` |
//! | `plugin ...` / `ui ...` | the plugin lines ([`crate::plugins::line`]) |
//! | `logoff` | leaves the world; waits until back at the character list |
//! | `quit` | stops: nothing after it runs |
//!
//! A control is named by a movement above, or `jump`, `run-lock` or `stop`, or by its id.

use std::fmt;

use auberean_client_sdk::contract::control::{movement, ControlId};
use auberean_client_sdk::contract::SelectKind;
use auberean_client_sdk::Action;

/// A character `enter` names: by its id, or by its name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Character {
    Id(u64),
    Name(String),
}

/// What `select` selects.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Selecting {
    Nearest(SelectKind),
    Next(SelectKind),
    Previous(SelectKind),
    Me,
    Last,
    Attacker,
    Nothing,
    Object(u64),
}

impl Selecting {
    /// The action that selects so.
    #[must_use]
    pub const fn action(self) -> Action {
        match self {
            Self::Nearest(kind) => Action::SelectNearest { kind },
            Self::Next(kind) => Action::SelectCycle {
                kind,
                previous: false,
            },
            Self::Previous(kind) => Action::SelectCycle {
                kind,
                previous: true,
            },
            Self::Me => Action::SelectSelf,
            Self::Last => Action::SelectLast,
            Self::Attacker => Action::SelectLastAttacker,
            Self::Nothing => Action::Select { target: 0 },
            Self::Object(target) => Action::Select { target },
        }
    }
}

/// What `dump` prints.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Dump {
    Events,
    Notices,
}

/// One line of a script.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Login,
    Create(String),
    Enter(Option<Character>),
    Tick(u64),
    Run(f64),
    At(f64),
    Act(Action),
    Begin(ControlId),
    End(ControlId),
    Press(ControlId),
    Hold(ControlId, f64),
    Say(String),
    Select(Selecting),
    Use(Option<u64>),
    Craft {
        recipe: u32,
        spinner: i32,
    },
    Slots,
    Agree(bool),
    Offer(bool),
    Snapshot,
    Dump(Dump),
    Position,
    /// A plugin line, as written.
    Plugin(String),
    Logoff,
    Quit,
}

impl Command {
    /// Whether it is carried out at once, taking no frame.
    #[must_use]
    pub const fn at_once(&self) -> bool {
        !matches!(
            self,
            Self::Login
                | Self::Create(_)
                | Self::Enter(_)
                | Self::Tick(_)
                | Self::Run(_)
                | Self::At(_)
                | Self::Press(_)
                | Self::Hold(..)
                | Self::Craft { .. }
                | Self::Logoff
        )
    }
}

/// The movements `walk` holds, by the names a script gives them.
pub const MOVEMENTS: [(&str, ControlId); 6] = [
    ("forward", movement::FORWARD),
    ("backward", movement::BACKWARD),
    ("turn-left", movement::TURN_LEFT),
    ("turn-right", movement::TURN_RIGHT),
    ("strafe-left", movement::STRAFE_LEFT),
    ("strafe-right", movement::STRAFE_RIGHT),
];

/// The other controls a script names.
const CONTROLS: [(&str, ControlId); 3] = [
    ("jump", movement::JUMP),
    ("run-lock", movement::RUN_LOCK),
    ("stop", movement::STOP),
];

/// The kinds `select` names.
const KINDS: [(&str, SelectKind); 6] = [
    ("creature", SelectKind::Creature),
    ("item", SelectKind::Item),
    ("player", SelectKind::Player),
    ("foe", SelectKind::Foe),
    ("fellow", SelectKind::Fellow),
    ("pet", SelectKind::Pet),
];

/// A control by the name a script gives it, or by its id.
#[must_use]
pub fn control(word: &str) -> Option<ControlId> {
    MOVEMENTS
        .iter()
        .chain(&CONTROLS)
        .find(|(n, _)| *n == word)
        .map(|(_, id)| *id)
        .or_else(|| {
            number(word)
                .and_then(|n| u32::try_from(n).ok())
                .map(ControlId)
        })
}

/// The name a script gives a control, or its id in hexadecimal.
#[must_use]
pub fn control_name(id: ControlId) -> String {
    MOVEMENTS
        .iter()
        .chain(&CONTROLS)
        .find(|(_, c)| *c == id)
        .map_or_else(|| format!("{:#010x}", id.0), |(n, _)| (*n).to_owned())
}

/// A kind of object by the name a script gives it.
#[must_use]
pub fn kind(word: &str) -> Option<SelectKind> {
    KINDS.iter().find(|(n, _)| *n == word).map(|(_, k)| *k)
}

/// A whole number: decimal, or hexadecimal after `0x`.
#[must_use]
pub fn number(word: &str) -> Option<u64> {
    match word.strip_prefix("0x").or_else(|| word.strip_prefix("0X")) {
        Some(hex) => u64::from_str_radix(&hex.replace('_', ""), 16).ok(),
        None => word.parse().ok(),
    }
}

/// A line that is not a command: its line number (from 1) and what is wrong with it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScriptError {
    pub line: usize,
    pub what: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.what)
    }
}

impl std::error::Error for ScriptError {}

/// Read a whole script.
///
/// # Errors
/// The first line that is neither blank, a comment, nor a command with the words it takes.
pub fn parse(text: &str) -> Result<Vec<Command>, ScriptError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        if let Some(c) = parse_line(raw).map_err(|what| ScriptError { line: i + 1, what })? {
            out.push(c);
        }
    }
    Ok(out)
}

/// Read one line: none for a blank line or a comment.
///
/// # Errors
/// What is wrong with it.
pub fn parse_line(raw: &str) -> Result<Option<Command>, String> {
    let head = raw.trim_start();
    let end = head.find(char::is_whitespace).unwrap_or(head.len());
    let (verb, rest) = head.split_at(end);
    if verb.is_empty() || verb.starts_with('#') {
        return Ok(None);
    }
    if verb == "say" {
        let text = rest.strip_prefix(' ').unwrap_or(rest).trim_end();
        if text.is_empty() {
            return Err("say needs its text".into());
        }
        return Ok(Some(Command::Say(text.to_owned())));
    }
    let rest = rest.find('#').map_or(rest, |at| &rest[..at]);
    if verb == "plugin" || verb == "ui" {
        return Ok(Some(Command::Plugin(format!("{verb}{}", rest.trim_end()))));
    }
    let words: Vec<&str> = rest.split_whitespace().collect();
    command(verb, &words).map(Some)
}

fn seconds(verb: &str, word: &str) -> Result<f64, String> {
    word.parse::<f64>()
        .ok()
        .filter(|s| s.is_finite() && *s >= 0.0)
        .ok_or_else(|| format!("{verb} needs seconds, not {word:?}"))
}

fn named_control(verb: &str, word: &str) -> Result<ControlId, String> {
    control(word).ok_or_else(|| format!("{verb}: no control {word:?}"))
}

fn on(verb: &str, word: &str) -> Result<bool, String> {
    match word {
        "on" => Ok(true),
        "off" => Ok(false),
        _ => Err(format!("{verb} takes on or off, not {word:?}")),
    }
}

fn id(verb: &str, word: &str) -> Result<u64, String> {
    number(word).ok_or_else(|| format!("{verb} needs an object's id, not {word:?}"))
}

fn command(verb: &str, words: &[&str]) -> Result<Command, String> {
    Ok(match (verb, words) {
        ("login", []) => Command::Login,
        ("create", [name]) => Command::Create((*name).to_owned()),
        ("enter", []) => Command::Enter(None),
        ("enter", [who]) => Command::Enter(Some(if who.starts_with("0x") {
            Character::Id(id(verb, who)?)
        } else {
            Character::Name((*who).to_owned())
        })),
        ("tick", [n]) => Command::Tick(
            n.parse()
                .map_err(|_| format!("tick needs a count of frames, not {n:?}"))?,
        ),
        ("run", [s]) => Command::Run(seconds(verb, s)?),
        ("at", [s]) => Command::At(seconds(verb, s)?),
        ("act", words) => Command::Act(crate::act::parse(words)?),
        ("begin", [c]) => Command::Begin(named_control(verb, c)?),
        ("end", [c]) => Command::End(named_control(verb, c)?),
        ("press", [c]) => Command::Press(named_control(verb, c)?),
        ("hold", [c, s]) => Command::Hold(named_control(verb, c)?, seconds(verb, s)?),
        ("walk", [m, s]) => {
            let (_, id) = MOVEMENTS.iter().find(|(n, _)| n == m).ok_or_else(|| {
                let names: Vec<&str> = MOVEMENTS.iter().map(|(n, _)| *n).collect();
                format!("walk: {m:?} is none of {}", names.join(", "))
            })?;
            Command::Hold(*id, seconds(verb, s)?)
        }
        ("select", [w]) => Command::Select(match *w {
            "self" => Selecting::Me,
            "last" => Selecting::Last,
            "attacker" => Selecting::Attacker,
            "none" => Selecting::Nothing,
            "next" => Selecting::Next(SelectKind::Creature),
            "previous" => Selecting::Previous(SelectKind::Creature),
            w => match kind(w) {
                Some(k) => Selecting::Nearest(k),
                None => Selecting::Object(id(verb, w)?),
            },
        }),
        ("select", [step @ ("next" | "previous"), k]) => {
            let k = kind(k).ok_or_else(|| format!("select: no kind {k:?}"))?;
            Command::Select(if *step == "next" {
                Selecting::Next(k)
            } else {
                Selecting::Previous(k)
            })
        }
        ("use", []) => Command::Use(None),
        ("use", [w]) => Command::Use(Some(id(verb, w)?)),
        ("craft", [r, s]) => Command::Craft {
            recipe: number(r)
                .and_then(|n| u32::try_from(n).ok())
                .ok_or_else(|| format!("craft needs a recipe, not {r:?}"))?,
            spinner: s
                .parse()
                .map_err(|_| format!("craft needs a spinner value, not {s:?}"))?,
        },
        ("slots", []) => Command::Slots,
        ("agree", [w]) => Command::Agree(on(verb, w)?),
        ("offer", [w]) => Command::Offer(on(verb, w)?),
        ("snapshot", []) => Command::Snapshot,
        ("dump", ["events"]) => Command::Dump(Dump::Events),
        ("dump", ["notices"]) => Command::Dump(Dump::Notices),
        ("position", []) => Command::Position,
        ("logoff", []) => Command::Logoff,
        ("quit", []) => Command::Quit,
        (
            "login" | "create" | "enter" | "tick" | "run" | "at" | "begin" | "end" | "press"
            | "hold" | "walk" | "select" | "use" | "craft" | "slots" | "agree" | "offer"
            | "snapshot" | "dump" | "position" | "logoff" | "quit",
            _,
        ) => return Err(format!("{verb} takes {}", usage(verb))),
        _ => return Err(format!("no command {verb:?}")),
    })
}

/// The words a verb takes.
fn usage(verb: &str) -> &'static str {
    match verb {
        "create" => "<name>",
        "enter" => "an optional <name> or <id>",
        "tick" => "<frames>",
        "run" | "at" => "<seconds>",
        "begin" | "end" | "press" => "<control>",
        "hold" => "<control> <seconds>",
        "walk" => "<movement> <seconds>",
        "select" => "<kind>, next or previous [<kind>], self, last, attacker, none or <id>",
        "use" => "an optional <id>",
        "craft" => "<recipe> <spinner>",
        "agree" | "offer" => "on or off",
        "dump" => "events or notices",
        _ => "nothing more",
    }
}
