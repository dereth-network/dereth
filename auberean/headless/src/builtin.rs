//! The program's flags as a script: what `--create`, `--walk`, `--say`, `--agree`, `--offer`,
//! `--trade-with` and `--craft` ask for, written out as the [`crate::script`] commands that do it.
//!
//! The client enters the world (making the character first with a name to make), opens the
//! examination panel's tabs, and from then on, timed from its entry:
//! - walking: forward for two seconds from the second second, then turning right for one, over and
//!   over, telling where it is at each change;
//! - saying its line at the third second and every ten after;
//! - asking a player for a trade at the third second;
//! - making each recipe in turn, the first at the third second and each next twelve after;
//!
//! until its time is up, when it tells what the last making's slots hold, logs off and stops. The
//! answers to offers are standing orders (`agree on`, `offer on`) set at the entry.

use crate::script::{Character, Command};
use auberean_client_sdk::contract::control::movement;
use auberean_client_sdk::contract::TradeAction;
use auberean_client_sdk::Action;

/// The examination panel's tabs, asked for on entering.
const EXAMINATION_TABS: u32 = 0x3FF;
/// How long a making is given to end before the next is made.
const BETWEEN_CRAFTS: f64 = 12.0;

/// What the flags ask of the client in the world.
#[derive(Clone, Debug, PartialEq)]
pub struct Flags {
    /// The character to enter: by id; or by name, made first when the list has none of it.
    pub character: Option<u64>,
    pub create: Option<String>,
    /// How long to stay in the world.
    pub seconds: f64,
    pub walk: bool,
    pub say: Option<String>,
    pub agree: bool,
    pub offer: bool,
    pub trade_with: Option<u64>,
    /// The recipes to make in turn, each at its spinner value.
    pub crafts: Vec<(u32, i32)>,
}

impl Default for Flags {
    fn default() -> Self {
        Self {
            character: None,
            create: None,
            seconds: 30.0,
            walk: false,
            say: None,
            agree: false,
            offer: false,
            trade_with: None,
            crafts: Vec::new(),
        }
    }
}

/// The script the flags ask for.
#[must_use]
pub fn script(f: &Flags) -> Vec<Command> {
    let mut out = vec![Command::Login];
    let who = match (&f.create, f.character) {
        (Some(name), _) => {
            out.push(Command::Create(name.clone()));
            Some(Character::Name(name.clone()))
        }
        (None, Some(id)) => Some(Character::Id(id)),
        (None, None) => None,
    };
    out.push(Command::Enter(who));
    if f.agree {
        out.push(Command::Agree(true));
    }
    if f.offer {
        out.push(Command::Offer(true));
    }
    out.push(Command::Act(Action::Examination {
        tabs: Some(EXAMINATION_TABS),
    }));
    // What is done when, from the entry; at the same second, in the order walking, trading,
    // making, saying.
    let mut timed: Vec<(f64, usize, Command)> = Vec::new();
    if f.walk {
        let mut t = 2.0;
        let mut leg = 0u32;
        while t < f.seconds {
            let (from, to, next) = if leg.is_multiple_of(2) {
                (
                    (leg > 0).then_some(movement::TURN_RIGHT),
                    movement::FORWARD,
                    2.0,
                )
            } else {
                (Some(movement::FORWARD), movement::TURN_RIGHT, 1.0)
            };
            if let Some(from) = from {
                timed.push((t, 0, Command::End(from)));
            }
            timed.push((t, 0, Command::Begin(to)));
            timed.push((t, 0, Command::Position));
            leg += 1;
            t += next;
        }
    }
    if let Some(partner) = f.trade_with {
        let open = Command::Act(Action::Trade(TradeAction::Open(partner)));
        timed.push((3.0, 1, open));
    }
    for (n, (recipe, spinner)) in f.crafts.iter().enumerate() {
        #[allow(clippy::cast_precision_loss)]
        let t = 3.0 + BETWEEN_CRAFTS * n as f64;
        let craft = Command::Craft {
            recipe: *recipe,
            spinner: *spinner,
        };
        timed.push((t, 2, craft));
    }
    if let Some(line) = &f.say {
        let mut t = 3.0;
        while t < f.seconds {
            timed.push((t, 3, Command::Say(line.clone())));
            t += 10.0;
        }
    }
    timed.retain(|(t, _, _)| *t < f.seconds);
    timed.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
    let mut at = None;
    for (t, _, c) in timed {
        if at != Some(t) {
            out.push(Command::At(t));
            at = Some(t);
        }
        out.push(c);
    }
    out.push(Command::At(f.seconds));
    if !f.crafts.is_empty() {
        out.push(Command::Slots);
    }
    out.push(Command::Logoff);
    out.push(Command::Quit);
    out
}
