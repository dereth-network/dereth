//! The command script: one command per line, the smallest text form that drives a headless client.
//!
//! The grammar is in the crate's `README.md` and this module is its only implementation. It is a
//! *script*, not a language: no expressions, no variables, no nesting. A line is a verb and its
//! arguments separated by runs of whitespace, `#` starts a comment that runs to the end of the
//! line, and a blank line is nothing. `say` is the one exception -- everything after the verb is
//! its text, verbatim, because a chat line may contain spaces and a `#`.
//!
//! Every error carries the one-based line number, because a script is read from a file and a
//! parser that cannot say *where* is a parser the writer of the file cannot use.

use std::fmt;

use dereth_client_sdk::actions::{names, ActionId};
use dereth_client_sdk::primitives::ObjectId;

/// One line of a script.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    /// `login <session> [<character>]` -- replay a recorded session's login. Without a character
    /// the replay stops at the character-select screen; with one it selects that character and
    /// keeps replaying.
    Login {
        session: String,
        character: Option<String>,
    },
    /// `tick <n>` -- run `n` whole frames.
    Tick(u64),
    /// `walk <direction> <secs>` -- hold a movement action for `secs` of simulated time. The
    /// movement shorthand for [`Command::Hold`].
    Walk { direction: Direction, secs: f64 },
    /// `hold <action> <secs>` -- begin an action, run `secs` of simulated time, end it.
    Hold { action: ActionId, secs: f64 },
    /// `press <action>` -- begin an action, run a frame, end it, run a frame: a key tapped.
    Press(ActionId),
    /// `begin <action>` -- the action's begin alone, and one frame.
    Begin(ActionId),
    /// `end <action>` -- the action's end alone, and one frame.
    End(ActionId),
    /// `use <object-id>` -- `UiRequest::Use`, the toolbar's Use button with a selection.
    Use(ObjectId),
    /// `say <text>` -- `UiRequest::ChatLine` on window 0.
    Say(String),
    /// `dump events` / `dump steps` -- the last frame's event log, or only its fourteen steps.
    Dump(Dump),
    /// `snapshot` -- `GameSnapshot::from_view` over the HUD's view, as text.
    Snapshot,
    /// `world <landblock>` -- load a world offline, with a body, at the middle of the landblock
    /// (a 16-bit id, `0x` hex or decimal). Needs `--world`.
    World(u16),
    /// `position` -- where the body stands: its cell and its block-local origin, or `none`.
    Position,
    /// `quit` -- stop reading the script and shut the client down.
    Quit,
}

/// What `dump` prints.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Dump {
    /// Every event of the frame just run, in push order, one per line.
    Events,
    /// Only the `Step` events -- the fourteen frame steps.
    Steps,
}

/// A direction `walk` can hold, and the client action each one is.
///
/// The client has no "walk to heading theta": turning is a rate applied while a key is held, not a
/// target to steer to, so a heading in degrees would have to be a controller this crate invented.
/// These six are `dereth_client_sdk::actions::movement::action`'s own movement ids, named as the script names them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Direction {
    /// `MOVE_FORWARD`.
    Forward,
    /// `MOVE_BACKWARD`.
    Back,
    /// `TURN_LEFT`.
    Left,
    /// `TURN_RIGHT`.
    Right,
    /// `STRAFE_LEFT`.
    StrafeLeft,
    /// `STRAFE_RIGHT`.
    StrafeRight,
}

impl Direction {
    /// The names the script accepts, in the order the README lists them.
    pub const NAMES: [&'static str; 6] = [
        "forward",
        "back",
        "left",
        "right",
        "strafe-left",
        "strafe-right",
    ];

    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "forward" => Self::Forward,
            "back" => Self::Back,
            "left" => Self::Left,
            "right" => Self::Right,
            "strafe-left" => Self::StrafeLeft,
            "strafe-right" => Self::StrafeRight,
            _ => return None,
        })
    }

    /// The action id this direction holds down.
    #[must_use]
    pub const fn action(self) -> dereth_client_sdk::actions::ActionId {
        use dereth_client_sdk::actions::movement::action as a;
        match self {
            Self::Forward => a::MOVE_FORWARD,
            Self::Back => a::MOVE_BACKWARD,
            Self::Left => a::TURN_LEFT,
            Self::Right => a::TURN_RIGHT,
            Self::StrafeLeft => a::STRAFE_LEFT,
            Self::StrafeRight => a::STRAFE_RIGHT,
        }
    }

    /// The word the script spells this direction with.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Forward => "forward",
            Self::Back => "back",
            Self::Left => "left",
            Self::Right => "right",
            Self::StrafeLeft => "strafe-left",
            Self::StrafeRight => "strafe-right",
        }
    }
}

/// An action as a script names it: the retail action name the shipped enum table gives it
/// (`MovementForward`, `MovementJump`, `CameraRotateLeft`, ...), or its id in decimal or `0x`-hex.
fn parse_action(tok: &str) -> Option<ActionId> {
    if tok.starts_with("0x") || tok.starts_with("0X") {
        return parse_u32(tok).map(ActionId);
    }
    names::action_for_enum_name(tok)
}

/// The name a script line prints an action with: the table's, or the decimal id.
#[must_use]
pub fn action_name(a: ActionId) -> String {
    names::enum_name_for_action(a)
}

/// A line that is not a command, with the line number that is not one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScriptError {
    /// One-based, as an editor counts.
    pub line: usize,
    /// What was wrong with it.
    pub what: String,
}

impl fmt::Display for ScriptError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.what)
    }
}

impl std::error::Error for ScriptError {}

fn err(line: usize, what: impl Into<String>) -> ScriptError {
    ScriptError {
        line,
        what: what.into(),
    }
}

/// Strip a trailing `#` comment. Not applied to `say`, whose text may contain one.
fn uncomment(line: &str) -> &str {
    match line.find('#') {
        Some(at) => &line[..at],
        None => line,
    }
}

/// An unsigned integer, decimal or `0x`-prefixed hex -- the two spellings an object id is written
/// in anywhere else in this tree.
fn parse_u32(tok: &str) -> Option<u32> {
    tok.strip_prefix("0x")
        .or_else(|| tok.strip_prefix("0X"))
        .map_or_else(
            || tok.parse::<u32>().ok(),
            |hex| u32::from_str_radix(hex, 16).ok(),
        )
}

/// Parse a whole script.
///
/// # Errors
/// [`ScriptError`] on the first line that is neither blank, a comment, nor a command this module
/// knows with the right number of arguments.
pub fn parse(text: &str) -> Result<Vec<Command>, ScriptError> {
    let mut out = Vec::new();
    for (i, raw) in text.lines().enumerate() {
        let n = i + 1;
        // `say` keeps everything after its verb, so the comment strip has to happen after the verb
        // is known rather than before it.
        let head = raw.trim_start();
        let verb_end = head.find(char::is_whitespace).unwrap_or(head.len());
        let (verb, rest) = head.split_at(verb_end);
        if verb.is_empty() || verb.starts_with('#') {
            continue;
        }
        let cmd = if verb == "say" {
            let text = rest
                .strip_prefix(' ')
                .unwrap_or(rest)
                .trim_end_matches(['\r', '\n']);
            if text.is_empty() {
                return Err(err(n, "say needs text"));
            }
            Command::Say(text.to_owned())
        } else {
            let args: Vec<&str> = uncomment(rest).split_whitespace().collect();
            parse_args(n, verb, &args)?
        };
        out.push(cmd);
    }
    Ok(out)
}

fn parse_args(n: usize, verb: &str, args: &[&str]) -> Result<Command, ScriptError> {
    match (verb, args) {
        ("login", [session]) => Ok(Command::Login {
            session: (*session).to_owned(),
            character: None,
        }),
        ("login", [session, character]) => Ok(Command::Login {
            session: (*session).to_owned(),
            character: Some((*character).to_owned()),
        }),
        ("login", _) => Err(err(n, "login takes <session> and an optional <character>")),
        ("tick", [count]) => count
            .parse::<u64>()
            .map(Command::Tick)
            .map_err(|_| err(n, format!("tick needs a frame count, not {count:?}"))),
        ("tick", _) => Err(err(n, "tick takes <n>")),
        ("walk", [dir, secs]) => {
            let direction = Direction::parse(dir).ok_or_else(|| {
                err(
                    n,
                    format!("{dir:?} is not one of {}", Direction::NAMES.join(", ")),
                )
            })?;
            let secs = secs
                .parse::<f64>()
                .ok()
                .filter(|s| s.is_finite() && *s >= 0.0)
                .ok_or_else(|| err(n, format!("walk needs seconds, not {secs:?}")))?;
            Ok(Command::Walk { direction, secs })
        }
        ("walk", _) => Err(err(n, "walk takes <direction> <secs>")),
        ("hold", [action, secs]) => {
            let action = parse_action(action)
                .ok_or_else(|| err(n, format!("{action:?} is not an action name or id")))?;
            let secs = secs
                .parse::<f64>()
                .ok()
                .filter(|s| s.is_finite() && *s >= 0.0)
                .ok_or_else(|| err(n, format!("hold needs seconds, not {secs:?}")))?;
            Ok(Command::Hold { action, secs })
        }
        ("hold", _) => Err(err(n, "hold takes <action> <secs>")),
        (verb @ ("press" | "begin" | "end"), [action]) => {
            let action = parse_action(action)
                .ok_or_else(|| err(n, format!("{action:?} is not an action name or id")))?;
            Ok(match verb {
                "press" => Command::Press(action),
                "begin" => Command::Begin(action),
                _ => Command::End(action),
            })
        }
        ("press" | "begin" | "end", _) => Err(err(n, format!("{verb} takes <action>"))),
        ("use", [id]) => parse_u32(id)
            .map(|v| Command::Use(ObjectId(v)))
            .ok_or_else(|| err(n, format!("use needs an object id, not {id:?}"))),
        ("use", _) => Err(err(n, "use takes <object-id>")),
        ("dump", ["events"]) => Ok(Command::Dump(Dump::Events)),
        ("dump", ["steps"]) => Ok(Command::Dump(Dump::Steps)),
        ("dump", _) => Err(err(n, "dump takes events or steps")),
        ("world", [block]) => parse_u32(block)
            .and_then(|v| u16::try_from(v).ok())
            .map(Command::World)
            .ok_or_else(|| err(n, format!("world needs a landblock id, not {block:?}"))),
        ("world", _) => Err(err(n, "world takes <landblock>")),
        ("position", []) => Ok(Command::Position),
        ("position", _) => Err(err(n, "position takes no arguments")),
        ("snapshot", []) => Ok(Command::Snapshot),
        ("snapshot", _) => Err(err(n, "snapshot takes no arguments")),
        ("quit", []) => Ok(Command::Quit),
        ("quit", _) => Err(err(n, "quit takes no arguments")),
        _ => Err(err(n, format!("unknown command {verb:?}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_and_position_parse() {
        assert_eq!(
            parse("world 0xA9B4\nposition\n").expect("parses"),
            vec![Command::World(0xA9B4), Command::Position]
        );
        assert!(parse("world 0x1A9B4").is_err(), "a landblock id is 16 bits");
        assert!(parse("position 1").is_err());
    }

    #[test]
    fn the_acceptance_script_parses_to_the_three_commands_it_reads_as() {
        let script = "\
# first-login-walk-jump, to the character-select screen.
login first-login-walk-jump
dump steps
quit
";
        assert_eq!(
            parse(script).expect("the acceptance script"),
            vec![
                Command::Login {
                    session: "first-login-walk-jump".to_owned(),
                    character: None
                },
                Command::Dump(Dump::Steps),
                Command::Quit,
            ]
        );
    }

    #[test]
    fn blank_lines_comments_and_indentation_are_not_commands() {
        let script = "\n   \n# just a comment\n\t\n  quit  # and a trailing one\n";
        assert_eq!(
            parse(script).expect("a script of nothing and a quit"),
            vec![Command::Quit]
        );
    }

    #[test]
    fn say_keeps_its_whole_line_including_spaces_and_hashes() {
        // The one command whose argument is not tokenised: a chat line is what the player typed.
        let cmds = parse("say hello from the capture proxy #2").expect("a chat line");
        assert_eq!(
            cmds,
            vec![Command::Say("hello from the capture proxy #2".to_owned())]
        );
    }

    #[test]
    fn an_object_id_is_decimal_or_hex_and_they_are_the_same_id() {
        assert_eq!(
            parse("use 0x50000001\nuse 1342177281").expect("both spellings"),
            vec![
                Command::Use(ObjectId(0x5000_0001)),
                Command::Use(ObjectId(0x5000_0001))
            ]
        );
    }

    #[test]
    fn every_direction_the_readme_lists_parses_to_its_own_action() {
        let script: String = Direction::NAMES
            .iter()
            .map(|d| format!("walk {d} 0.5\n"))
            .collect();
        let cmds = parse(&script).expect("six directions");
        assert_eq!(cmds.len(), Direction::NAMES.len());
        let mut actions = Vec::new();
        for (c, want) in cmds.iter().zip(Direction::NAMES) {
            let Command::Walk { direction, secs } = c else {
                panic!("{c:?} is not a walk")
            };
            assert_eq!(direction.name(), want);
            assert!((secs - 0.5).abs() < f64::EPSILON);
            actions.push(direction.action());
        }
        actions.sort_by_key(|a| a.0);
        actions.dedup();
        assert_eq!(
            actions.len(),
            Direction::NAMES.len(),
            "two directions share an action id"
        );
    }

    #[test]
    fn an_action_is_named_by_the_retail_table_or_by_its_id() {
        let cmds =
            parse("hold MovementForward 2\npress MovementJump\nbegin 0x35\nend 53\nhold 0x29 0.5")
                .expect("five action lines");
        assert_eq!(
            cmds,
            vec![
                Command::Hold {
                    action: ActionId(0x29),
                    secs: 2.0
                },
                Command::Press(ActionId(0x31)),
                Command::Begin(ActionId(0x35)),
                Command::End(ActionId(0x35)),
                Command::Hold {
                    action: ActionId(0x29),
                    secs: 0.5
                },
            ]
        );
        assert_eq!(action_name(ActionId(0x31)), "MovementJump");
    }

    #[test]
    fn walk_is_the_hold_of_its_directions_movement_action() {
        for d in Direction::NAMES {
            let Command::Walk { direction, .. } = &parse(&format!("walk {d} 1")).expect("walk")[0]
            else {
                panic!("not a walk")
            };
            let hold = parse(&format!("hold {} 1", action_name(direction.action())))
                .expect("the same action by name");
            assert_eq!(
                hold,
                vec![Command::Hold {
                    action: direction.action(),
                    secs: 1.0
                }]
            );
        }
    }

    #[test]
    fn login_takes_a_character_or_leaves_the_replay_at_character_select() {
        assert_eq!(
            parse("login first-login-walk-jump Frostfell").expect("a named character"),
            vec![Command::Login {
                session: "first-login-walk-jump".to_owned(),
                character: Some("Frostfell".to_owned()),
            }]
        );
    }

    #[test]
    fn a_bad_line_names_its_own_number_and_nothing_is_returned() {
        let e = parse("quit\ntick\nquit").expect_err("tick with no count");
        assert_eq!(e.line, 2);
        assert!(e.to_string().starts_with("line 2: "), "{e}");

        assert_eq!(parse("fly 3").expect_err("no such verb").line, 1);
        assert_eq!(
            parse("quit\n\nwalk sideways 1")
                .expect_err("no such direction")
                .line,
            3
        );
        assert_eq!(parse("dump everything").expect_err("no such dump").line, 1);
        assert_eq!(
            parse("snapshot now").expect_err("snapshot is nullary").line,
            1
        );
        assert_eq!(parse("use nine").expect_err("not an id").line, 1);
        assert_eq!(
            parse("walk forward -1").expect_err("negative seconds").line,
            1
        );
        assert_eq!(parse("press Flap").expect_err("no such action").line, 1);
        assert_eq!(
            parse("hold MovementForward").expect_err("no seconds").line,
            1
        );
        assert_eq!(parse("begin").expect_err("no action").line, 1);
    }
}
