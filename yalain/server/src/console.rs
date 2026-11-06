//! The operator's console: commands typed into the server's standard input, one per line,
//! answered in its log.
//!
//! ```text
//! reload   compile the world database again (the --world-db folder, or the pinned release) and
//!          take what can change while the world runs; a database that does not compile leaves
//!          the world as it was
//! shutdown [message]
//!          count down (two minutes unless set otherwise), telling every player in the world as
//!          the 2017 worlds did (at two minutes, then every thirty seconds), then stop; the
//!          message, if given, is shown to every player in the world first
//! cancel-shutdown
//!          stop the countdown, and tell every player in the world
//! set-shutdown-interval <0-99999>
//!          how many seconds the next shutdown counts down
//! stop-now [message], stop [message], exit
//!          stop the server at once, keeping every character in the world (`exit` as Empyrean's
//!          console has it)
//! save-world
//!          keep every character in the world now; the world goes on
//! dn       the world's link to the Dereth Network and its blocks; `dn block <world or
//!          Name@world>` and `dn unblock ...` block a world or a character for everyone on this
//!          world, or lift it (`dn admin block ...` is the same, as it is said in game); `dn verify
//!          <token>` verifies the world with the token the site gave
//!          (`yalain_federation`'s operator commands)
//! hero     `hero <name>` makes a character a hero (it earns focus, and has its greatest at
//!          once), `hero <name> off` no longer one; in the world or out of it
//! teleport <character> to <where>
//!          send a character in the world through portal space: to another character in the
//!          world, to `place <name>` (a place the world names), or to `<cell> <x> <y> <z>
//!          [heading]` (the cell in hex, the heading in degrees)
//! place <character> to <where>
//!          set where a character out of the world enters next: beside a character in the
//!          world, `place <name>`, or `<cell> <x> <y> <z> [heading]`
//! make-character <recipe file>
//!          make a character with a build before it first plays (the `characters` module)
//! apply-build <recipe file>
//!          bring a character out of the world, played or not, up to a build, keeping what the
//!          build does not name
//! deletecharacter <name> [now]
//!          delete a character as its player would, booting it if it is in the world; with `now`,
//!          at once, with no hour in its account's list, its name free again
//! forcelogoff <name>, forcelogout <name>
//!          take a character out of the world at once, kept where it stands, and end its login
//!          (Empyrean's words): one a killed client left in the world goes now
//! export-character <name> <file>
//!          write a character to a new file, for another world
//! import-character <file> <account> [new name]
//!          bring a character from a file onto an account, as a new character
//! accountcreate <name> <secret> [admin]
//!          make an account, a player's unless given the word `admin`
//! accountget <name>, accountlist
//!          an account, or every account: its id, whether an admin's, and any ban
//! set-accountpassword <name> <secret>
//!          give an account a new secret
//! set-accountadmin <name> [on|off]
//!          make an account an admin's, or a player's again
//! set-characteradmin <name> [on|off]
//!          make a character of an admin's account an admin character, or a player again
//! set-accountgroups <name> [group,group...]
//!          put an account in its named groups (none when none are given)
//! ban <account> <days> <hours> <minutes> [reason]
//!          keep an account out for that long, booting it if it is logged in
//! unban <account>, banlist
//!          lift a ban; list the accounts banned now
//! gag <character>, ungag <character>
//!          stop a character's chat for five minutes of its time in the world, or let it talk
//! boot {char|account|iid} <who>[, reason]
//!          end a login, by its character's name, its account's or its character's id
//! gamecast <message>
//!          show every player in the world `Broadcast from System> <message>`
//! help     list the commands
//! ```
//!
//! The moderation commands and `gamecast` are `yalain_world::admin`'s, as Empyrean's console has
//! them; each answers in Empyrean's words, in the fixed form below, never with a secret.
//!
//! A name with spaces is written as the rest of the line, or between double quotes (`hero "Kara
//! Off"`, `teleport "Hand Of Doom" to "Sir Kara"`, `export-character "Hand Of Doom" hand.json`);
//! a file's path likewise when it holds spaces.
//!
//! Each command's answer is one line in the fixed form of `yalain_world::answers` (its words,
//! then `[ok <command>]` or `[refused <command>]`), which programs driving the console read; the
//! command is named as [`Command::name`] names it.
//!
//! A server started with no standard input (a service, a test) has no console and runs as
//! before: the end of the console's input ends the console, not the server, unless the server
//! was asked to stop then (`--stop-at-console-end`, for a program that holds the console's other
//! end).

use std::io::BufRead as _;
use std::path::PathBuf;
use std::sync::mpsc::{self, Receiver, TryRecvError};

use dereth_primitives::num::math;
use yalain_store::StoredPosition;
use yalain_world::admin::{name_of, words, Word};
use yalain_world::characters::Destination;

/// The commands, as `help` lists them.
pub const HELP: &str = "commands: reload (compile the world database again and take what can \
                        change while the world runs), shutdown [message] (tell the players and \
                        stop after the countdown), cancel-shutdown, set-shutdown-interval \
                        <seconds> (the countdown's length, 0 to 99999), stop-now [message] (or \
                        stop, or exit), save-world (keep every character in the world now), dn \
                        (the Dereth Network: dn, dn block <world or Name@world>, dn unblock \
                        <world or Name@world>, dn admin global [<room>], dn verify <token>), hero (hero <name>, hero \
                        <name> off: make a \
                        character a hero or no longer one), teleport <character> to <character \
                        | place <name> | cell x y z [heading]>, place <character> to <character \
                        | place <name> | cell x y z [heading]> (where one out of the world \
                        enters next), make-character <recipe file>, apply-build <recipe file>, \
                        deletecharacter <name> [now], forcelogoff <name> (take a character out \
                        of the world at once), export-character <name> <file>, \
                        import-character <file> <account> [new name] (names with spaces as the \
                        rest of the line or in double quotes), the moderation commands (accountcreate <name> \
                        <secret> [admin], accountget <name>, accountlist, set-accountpassword \
                        <name> <secret>, set-accountadmin <name> [on|off], set-characteradmin \
                        <name> [on|off], set-accountgroups <name> [group,group...], ban <account> \
                        <days> \
                        <hours> <minutes> [reason], unban <account>, banlist, gag <character>, \
                        ungag <character>, boot {char|account|iid} <who>[, reason]), gamecast \
                        <message> (show every player in the world a message), help";

/// A command typed at the console.
#[derive(Debug, Clone, PartialEq)]
pub enum Command {
    Reload,
    /// `shutdown [message]`: the message, if one was given.
    Shutdown(Option<String>),
    CancelShutdown,
    /// `set-shutdown-interval <seconds>`: the seconds, `None` for a line without a number from 0
    /// to 99999.
    SetShutdownInterval(Option<u32>),
    /// `stop-now [message]`.
    StopNow(Option<String>),
    SaveWorld,
    Help,
    /// `dn` and the words after it.
    Network(Vec<String>),
    /// `hero <name> [off]`: the character's name, and whether it is made a hero; `None` for a
    /// line without a name.
    Hero(Option<(String, bool)>),
    /// `teleport <character> to <where>`, or how it is written when the line does not read.
    Teleport(Result<(String, Destination), String>),
    /// `place <character> to <where>`, or how it is written.
    Place(Result<(String, Destination), String>),
    /// `make-character <recipe file>`, or how it is written.
    MakeCharacter(Result<PathBuf, String>),
    /// `apply-build <recipe file>`, or how it is written.
    ApplyBuild(Result<PathBuf, String>),
    /// `deletecharacter <name> [now]`: the name, and whether at once; or how it is written.
    DeleteCharacter(Result<(String, bool), String>),
    /// `forcelogoff <name>`: the name; or how it is written.
    ForceLogoff(Result<String, String>),
    /// `export-character <name> <file>`, or how it is written.
    ExportCharacter(Result<(String, PathBuf), String>),
    /// `import-character <file> <account> [new name]`, or how it is written.
    ImportCharacter(Result<(PathBuf, String, Option<String>), String>),
    /// A moderation command, or how it is written when the line does not read.
    Admin(Result<yalain_world::admin::Command, String>),
    /// A word that is no command.
    Unknown(String),
}

impl Command {
    /// The command's own name, as its answer names it (`stop` and `exit` are `stop-now`'s, `?`
    /// is `help`'s); a word that is no command is named as typed, in small letters.
    #[must_use]
    pub fn name(&self) -> String {
        match self {
            Self::Reload => "reload",
            Self::Shutdown(_) => "shutdown",
            Self::CancelShutdown => "cancel-shutdown",
            Self::SetShutdownInterval(_) => "set-shutdown-interval",
            Self::StopNow(_) => "stop-now",
            Self::SaveWorld => "save-world",
            Self::Help => "help",
            Self::Network(_) => "dn",
            Self::Hero(_) => "hero",
            Self::Teleport(_) => "teleport",
            Self::Place(_) => "place",
            Self::MakeCharacter(_) => "make-character",
            Self::ApplyBuild(_) => "apply-build",
            Self::DeleteCharacter(_) => "deletecharacter",
            Self::ForceLogoff(_) => "forcelogoff",
            Self::ExportCharacter(_) => "export-character",
            Self::ImportCharacter(_) => "import-character",
            // Answered in Empyrean's words, under the moderation command's own word.
            Self::Admin(Ok(c)) => c.word(),
            Self::Admin(Err(_)) => "admin",
            Self::Unknown(word) => return word.clone(),
        }
        .to_owned()
    }
}

/// Log a command's answer in the fixed form programs read: what was done, or why nothing was.
pub fn answer(command: &str, done: &Result<String, String>) {
    use yalain_world::answers;
    match done {
        Ok(text) => tracing::info!(target: answers::TARGET, "{}", answers::ok(command, text)),
        Err(why) => tracing::warn!(target: answers::TARGET, "{}", answers::refused(command, why)),
    }
}

/// The command a line holds: its first word, in any case; `None` for a blank line.
#[must_use]
pub fn parse(line: &str) -> Option<Command> {
    if let Some(c) = yalain_world::admin::parse(line) {
        return Some(Command::Admin(c));
    }
    let word = line.split_whitespace().next()?.to_ascii_lowercase();
    let all = words(line);
    let args = &all[1..];
    // What follows the command's word, as typed.
    let rest = || {
        let after = line.trim_start()[word.len()..].trim();
        (!after.is_empty()).then(|| after.to_owned())
    };
    Some(match word.as_str() {
        "reload" => Command::Reload,
        "shutdown" => Command::Shutdown(rest()),
        "cancel-shutdown" => Command::CancelShutdown,
        "set-shutdown-interval" => Command::SetShutdownInterval(
            rest()
                .and_then(|n| n.parse::<u32>().ok())
                .filter(|n| *n <= yalain_world::shutdown::MAX_SECONDS),
        ),
        "stop-now" | "stop" | "exit" => Command::StopNow(rest()),
        "save-world" => Command::SaveWorld,
        "help" | "?" => Command::Help,
        "dn" => Command::Network(line.split_whitespace().skip(1).map(str::to_owned).collect()),
        "hero" => Command::Hero(hero(args)),
        "teleport" => Command::Teleport(teleport(args).ok_or_else(|| {
            "teleport <character> to <character | place <name> | cell x y z [heading]>".to_owned()
        })),
        "place" => Command::Place(teleport(args).ok_or_else(|| {
            "place <character> to <character | place <name> | cell x y z [heading]>".to_owned()
        })),
        "make-character" => Command::MakeCharacter(match args {
            [file] => Ok(PathBuf::from(&file.text)),
            _ => Err("make-character <recipe file>".to_owned()),
        }),
        "apply-build" => Command::ApplyBuild(match args {
            [file] => Ok(PathBuf::from(&file.text)),
            _ => Err("apply-build <recipe file>".to_owned()),
        }),
        "deletecharacter" => Command::DeleteCharacter(
            delete(args).ok_or_else(|| "deletecharacter <name> [now]".to_owned()),
        ),
        "forcelogoff" | "forcelogout" => Command::ForceLogoff(if args.is_empty() {
            Err("forcelogoff <name>".to_owned())
        } else {
            Ok(name_of(args))
        }),
        "export-character" => Command::ExportCharacter(match args {
            [name @ .., file] if !name.is_empty() => Ok((name_of(name), PathBuf::from(&file.text))),
            _ => Err("export-character <name> <file>".to_owned()),
        }),
        "import-character" => Command::ImportCharacter(match args {
            [file, account, name @ ..] => Ok((
                PathBuf::from(&file.text),
                account.text.clone(),
                (!name.is_empty()).then(|| name_of(name)),
            )),
            _ => Err("import-character <file> <account> [new name]".to_owned()),
        }),
        _ => Command::Unknown(word),
    })
}

/// `hero <name> [off]`: the name (the rest of the line, or quoted), and whether it is made a
/// hero: a last word `off`, unquoted, makes it no longer one.
fn hero(args: &[Word]) -> Option<(String, bool)> {
    match args {
        [] => None,
        [name @ .., off] if !off.quoted && off.text.eq_ignore_ascii_case("off") => {
            (!name.is_empty()).then(|| (name_of(name), false))
        }
        name => Some((name_of(name), true)),
    }
}

/// `deletecharacter <name> [now]`: the name (the rest of the line, or quoted), and whether at
/// once: a last word `now`, unquoted, deletes at once.
fn delete(args: &[Word]) -> Option<(String, bool)> {
    match args {
        [] => None,
        [name @ .., now] if !now.quoted && now.text.eq_ignore_ascii_case("now") => {
            (!name.is_empty()).then(|| (name_of(name), true))
        }
        name => Some((name_of(name), false)),
    }
}

/// `teleport <character> to <where>` (and `place`'s): the character's name is the words before
/// the first `to` not in quotes; where is another character's name, `place <name>`, or a cell
/// (hex), x, y, z and a heading in degrees if given.
fn teleport(args: &[Word]) -> Option<(String, Destination)> {
    let i = args
        .iter()
        .position(|w| !w.quoted && w.text.eq_ignore_ascii_case("to"))?;
    let (who, to) = (&args[..i], &args[i + 1..]);
    if who.is_empty() || to.is_empty() {
        return None;
    }
    let to = match to {
        [p, place @ ..]
            if !p.quoted && p.text.eq_ignore_ascii_case("place") && !place.is_empty() =>
        {
            Destination::Place(name_of(place))
        }
        _ => match position(to) {
            Some(at) => Destination::At(at),
            None => Destination::Character(name_of(to)),
        },
    };
    Some((name_of(who), to))
}

/// A place written as a cell in hex, x, y and z, and a heading in degrees (north if not given).
fn position(words: &[Word]) -> Option<StoredPosition> {
    let f = |w: &Word| w.text.parse::<f32>().ok().filter(|v| v.is_finite());
    let (cell, x, y, z, heading) = match words {
        [c, x, y, z] => (c, x, y, z, 0.0),
        [c, x, y, z, h] => (c, x, y, z, f(h)?),
        _ => return None,
    };
    let hex = cell.text.trim_start_matches("0x").trim_start_matches("0X");
    Some(StoredPosition {
        cell: u32::from_str_radix(hex, 16).ok()?,
        origin: [f(x)?, f(y)?, f(z)?],
        rotation: rotation_of_degrees(heading),
    })
}

/// The rotation of a heading in degrees clockwise from north.
#[must_use]
pub fn rotation_of_degrees(heading: f32) -> [f32; 4] {
    let half = -heading.to_radians() / 2.0;
    [math::cosf(half), 0.0, 0.0, math::sinf(half)]
}

/// The lines typed into standard input, as they come. The channel ends with the input.
#[must_use]
pub fn lines() -> Receiver<String> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        for line in std::io::stdin().lock().lines() {
            let Ok(line) = line else { break };
            if tx.send(line).is_err() {
                break;
            }
        }
    });
    rx
}

/// Work running on a thread of its own, so the world goes on meanwhile.
pub struct Job<T>(Receiver<Result<T, String>>);

/// Start `work` on a thread of its own.
pub fn start<T: Send + 'static>(
    work: impl FnOnce() -> Result<T, String> + Send + 'static,
) -> Job<T> {
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let _ = tx.send(work());
    });
    Job(rx)
}

impl<T> Job<T> {
    /// Its result once it is done.
    pub fn poll(&self) -> Option<Result<T, String>> {
        match self.0.try_recv() {
            Ok(r) => Some(r),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("the work stopped before it was done".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{parse, Command};

    /// The console's shutdown commands, as Empyrean's: `shutdown` with an optional message (as
    /// typed, its spaces inside kept), `cancel-shutdown`, `set-shutdown-interval` with 0 to 99999
    /// seconds, `stop-now` with an optional message, and `save-world`, the command's word in any
    /// case.
    /// Behaviour: server.operator.shutdown-message-and-interval
    #[test]
    fn the_consoles_shutdown_commands_read_their_message_and_seconds() {
        let m = |s: &str| Some(s.to_owned());
        assert_eq!(parse("shutdown"), Some(Command::Shutdown(None)));
        assert_eq!(parse("  SHUTDOWN   "), Some(Command::Shutdown(None)));
        assert_eq!(
            parse("shutdown Back in  ten minutes. "),
            Some(Command::Shutdown(m("Back in  ten minutes.")))
        );
        assert_eq!(parse("cancel-shutdown"), Some(Command::CancelShutdown));
        assert_eq!(
            parse("set-shutdown-interval 600"),
            Some(Command::SetShutdownInterval(Some(600)))
        );
        assert_eq!(
            parse("set-shutdown-interval 0"),
            Some(Command::SetShutdownInterval(Some(0)))
        );
        for bad in [
            "set-shutdown-interval",
            "set-shutdown-interval 100000",
            "set-shutdown-interval -1",
            "set-shutdown-interval soon",
        ] {
            assert_eq!(
                parse(bad),
                Some(Command::SetShutdownInterval(None)),
                "{bad}"
            );
        }
        assert_eq!(parse("stop-now"), Some(Command::StopNow(None)));
        assert_eq!(
            parse("Stop-Now Restarting."),
            Some(Command::StopNow(m("Restarting.")))
        );
        assert_eq!(parse("save-world"), Some(Command::SaveWorld));
    }

    /// `stop` and `exit` (Empyrean's console's word, which Lyceum types) stop the server at once,
    /// as `stop-now` does, with a message if one is given.
    /// Behaviour: server.operator.stop-and-exit-at-the-console
    #[test]
    fn stop_and_exit_stop_the_server_at_once_as_stop_now_does() {
        assert_eq!(parse("stop"), Some(Command::StopNow(None)));
        assert_eq!(parse(" EXIT "), Some(Command::StopNow(None)));
        assert_eq!(
            parse("stop Back soon."),
            Some(Command::StopNow(Some("Back soon.".into())))
        );
    }

    /// A character's name with spaces is read whole by every console command that takes one: as
    /// the rest of the line, or between double quotes (which also keep a word like `off` or `to`
    /// inside a name).
    /// Behaviour: server.operator.names-with-spaces-at-the-console
    #[test]
    fn the_consoles_commands_read_names_with_spaces_whole() {
        use yalain_world::admin::Command as Admin;
        use yalain_world::characters::Destination;
        let name = |s: &str| s.to_owned();
        assert_eq!(
            parse("hero Hand Of Doom"),
            Some(Command::Hero(Some((name("Hand Of Doom"), true))))
        );
        assert_eq!(
            parse("hero Hand Of Doom off"),
            Some(Command::Hero(Some((name("Hand Of Doom"), false))))
        );
        assert_eq!(
            parse("hero \"Kara Off\""),
            Some(Command::Hero(Some((name("Kara Off"), true))))
        );
        assert_eq!(
            parse("hero \"Kara Off\" off"),
            Some(Command::Hero(Some((name("Kara Off"), false))))
        );
        assert_eq!(
            parse("hero \"Off\""),
            Some(Command::Hero(Some((name("Off"), true))))
        );
        assert_eq!(
            parse("teleport \"To\" to Kara"),
            Some(Command::Teleport(Ok((
                name("To"),
                Destination::Character(name("Kara"))
            ))))
        );
        assert_eq!(parse("hero"), Some(Command::Hero(None)));
        assert_eq!(parse("hero off"), Some(Command::Hero(None)));
        assert_eq!(
            parse("teleport Hand Of Doom to Sir Kara"),
            Some(Command::Teleport(Ok((
                name("Hand Of Doom"),
                Destination::Character(name("Sir Kara"))
            ))))
        );
        assert_eq!(
            parse("teleport \"Walks To Arwic\" to place Dng_BTD_CrystalOsteth"),
            Some(Command::Teleport(Ok((
                name("Walks To Arwic"),
                Destination::Place(name("Dng_BTD_CrystalOsteth"))
            ))))
        );
        assert_eq!(
            parse("teleport Kara to \"Place Holder\""),
            Some(Command::Teleport(Ok((
                name("Kara"),
                Destination::Character(name("Place Holder"))
            ))))
        );
        let Some(Command::Teleport(Ok((_, Destination::At(at))))) =
            parse("teleport Kara to 77B80006 4.4 100.8 182.5 90")
        else {
            panic!("a place by its cell and point");
        };
        assert_eq!(at.cell, 0x77B8_0006);
        assert_eq!(at.origin, [4.4, 100.8, 182.5]);
        assert!((at.rotation[0] - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-6);
        assert!(matches!(
            parse("teleport Kara"),
            Some(Command::Teleport(Err(_)))
        ));
        assert!(matches!(
            parse("teleport to Kara"),
            Some(Command::Teleport(Err(_)))
        ));
        assert_eq!(
            parse("deletecharacter  Hand  Of Doom "),
            Some(Command::DeleteCharacter(Ok((name("Hand Of Doom"), false))))
        );
        assert_eq!(
            parse("export-character Hand Of Doom \"/tmp/my worlds/hand.json\""),
            Some(Command::ExportCharacter(Ok((
                name("Hand Of Doom"),
                "/tmp/my worlds/hand.json".into()
            ))))
        );
        assert_eq!(
            parse("import-character hand.json builder Hand Of Glory"),
            Some(Command::ImportCharacter(Ok((
                "hand.json".into(),
                name("builder"),
                Some(name("Hand Of Glory"))
            ))))
        );
        assert_eq!(
            parse("import-character hand.json builder"),
            Some(Command::ImportCharacter(Ok((
                "hand.json".into(),
                name("builder"),
                None
            ))))
        );
        assert_eq!(
            parse("make-character \"characters/Hand Of Doom.toml\""),
            Some(Command::MakeCharacter(Ok(
                "characters/Hand Of Doom.toml".into()
            )))
        );
        assert_eq!(
            parse("gag \"Sir Kara\""),
            Some(Command::Admin(Ok(Admin::Gag {
                name: name("Sir Kara")
            })))
        );
        assert_eq!(
            parse("accountget \"my old account\""),
            Some(Command::Admin(Ok(Admin::AccountGet {
                name: name("my old account")
            })))
        );
        let Some(Command::Admin(Ok(Admin::Boot { who, .. }))) =
            parse("boot char \"Sir Kara\", spamming")
        else {
            panic!("a boot");
        };
        assert_eq!(who, "Sir Kara");
    }

    /// The console reads the moderation commands as Empyrean's console has them, their words in
    /// any case, and a line of one that does not read as how it is written; a secret typed in one
    /// never shows in what the command prints of itself.
    /// Behaviour: server.operator.accounts-at-the-console
    #[test]
    fn the_consoles_moderation_commands_read_and_never_show_a_secret() {
        use yalain_world::admin::{BootBy, Command as Admin};
        let Some(Command::Admin(Ok(c))) = parse("AccountCreate kara hunter2 admin") else {
            panic!("an account made");
        };
        assert_eq!(
            c,
            Admin::AccountCreate {
                name: "kara".into(),
                ticket: "hunter2".into(),
                admin: true,
            }
        );
        let Some(Command::Admin(Ok(p))) = parse("set-accountpassword kara hunter3") else {
            panic!("a secret set");
        };
        for shown in [
            format!("{c:?}"),
            format!("{p:?}"),
            format!("{:?}", parse("accountcreate a hunter4")),
        ] {
            assert!(!shown.contains("hunter"), "{shown}");
        }
        assert_eq!(
            parse("ban kara 1 2.5 0 gold  selling"),
            Some(Command::Admin(Ok(Admin::Ban {
                name: "kara".into(),
                days: 1.0,
                hours: 2.5,
                minutes: 0.0,
                reason: Some("gold selling".into()),
            })))
        );
        assert_eq!(
            parse("boot account kara , afk"),
            Some(Command::Admin(Ok(Admin::Boot {
                by: BootBy::Account,
                who: "kara".into(),
                reason: Some("afk".into()),
            })))
        );
        assert_eq!(
            parse("gag Sir Kara"),
            Some(Command::Admin(Ok(Admin::Gag {
                name: "Sir Kara".into()
            })))
        );
        assert!(matches!(
            parse("ban kara soon"),
            Some(Command::Admin(Err(_)))
        ));
        assert!(matches!(parse("unban"), Some(Command::Admin(Err(_)))));
        assert_eq!(parse("banlist"), Some(Command::Admin(Ok(Admin::BanList))));
    }

    /// `deletecharacter <name> now` deletes at once: a last unquoted `now`, the name before it
    /// read whole; a quoted `"Now"` is a name, and `now` alone is how the command is written.
    /// Behaviour: server.operator.characters-deleted-at-once
    #[test]
    fn deletecharacter_now_reads_the_name_before_it() {
        let name = |s: &str| s.to_owned();
        assert_eq!(
            parse("deletecharacter Hand Of Doom now"),
            Some(Command::DeleteCharacter(Ok((name("Hand Of Doom"), true))))
        );
        assert_eq!(
            parse("deletecharacter \"Hand Of Doom\" NOW"),
            Some(Command::DeleteCharacter(Ok((name("Hand Of Doom"), true))))
        );
        assert_eq!(
            parse("deletecharacter Hand Of Doom"),
            Some(Command::DeleteCharacter(Ok((name("Hand Of Doom"), false))))
        );
        assert_eq!(
            parse("deletecharacter \"Now\""),
            Some(Command::DeleteCharacter(Ok((name("Now"), false))))
        );
        for bad in ["deletecharacter", "deletecharacter now"] {
            assert!(
                matches!(parse(bad), Some(Command::DeleteCharacter(Err(_)))),
                "{bad}"
            );
        }
    }

    /// `place <character> to <where>` reads as `teleport` does: the name before the first
    /// unquoted `to`, then a character, `place <name>`, or a cell and point.
    /// Behaviour: server.operator.characters-placed-out-of-the-world
    #[test]
    fn place_reads_a_name_and_where_as_teleport_does() {
        use yalain_world::characters::Destination;
        assert_eq!(
            parse("place Hand Of Doom to place Dng_BTD_CrystalOsteth"),
            Some(Command::Place(Ok((
                "Hand Of Doom".into(),
                Destination::Place("Dng_BTD_CrystalOsteth".into())
            ))))
        );
        let Some(Command::Place(Ok((who, Destination::At(at))))) =
            parse("place \"Sir Kara\" to 77B80006 4.4 100.8 182.5")
        else {
            panic!("a cell and point");
        };
        assert_eq!(who, "Sir Kara");
        assert_eq!((at.cell, at.origin), (0x77B8_0006, [4.4, 100.8, 182.5]));
        assert!(matches!(parse("place Kara"), Some(Command::Place(Err(_)))));
    }

    /// `apply-build <recipe file>` names one file, quoted when it holds spaces.
    /// Behaviour: server.operator.builds-given-to-played-characters
    #[test]
    fn apply_build_reads_its_recipe_file() {
        assert_eq!(
            parse("apply-build \"characters/Hand Of Doom.toml\""),
            Some(Command::ApplyBuild(Ok(
                "characters/Hand Of Doom.toml".into()
            )))
        );
        assert!(matches!(
            parse("apply-build"),
            Some(Command::ApplyBuild(Err(_)))
        ));
    }

    /// Each command answers under its own name, whichever of its words was typed: the names
    /// programs read in `[ok <command>]` and `[refused <command>]` stay as they are.
    /// Behaviour: server.operator.console-answers-in-a-fixed-form
    #[test]
    fn each_command_answers_under_its_own_name() {
        for (line, name) in [
            ("reload", "reload"),
            ("shutdown soon", "shutdown"),
            ("cancel-shutdown", "cancel-shutdown"),
            ("set-shutdown-interval 5", "set-shutdown-interval"),
            ("stop-now", "stop-now"),
            ("stop", "stop-now"),
            ("EXIT", "stop-now"),
            ("save-world", "save-world"),
            ("?", "help"),
            ("hero Kara", "hero"),
            ("teleport Kara to Fen", "teleport"),
            ("place Kara to Fen", "place"),
            ("make-character a.toml", "make-character"),
            ("apply-build a.toml", "apply-build"),
            ("deletecharacter Kara now", "deletecharacter"),
            ("forcelogoff Kara", "forcelogoff"),
            ("ForceLogout Kara", "forcelogoff"),
            ("export-character Kara a.json", "export-character"),
            ("import-character a.json one", "import-character"),
            ("boot char Kara, afk", "boot"),
            ("BanList", "banlist"),
            ("GameCast Back soon.", "gamecast"),
            ("Frobnicate", "frobnicate"),
        ] {
            assert_eq!(
                parse(line).map(|c| c.name()),
                Some(name.to_owned()),
                "{line}"
            );
        }
    }
}
