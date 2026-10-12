//! What the launcher shows on Discord while a game it started is running, and the messages that
//! say it to the Discord app on the same computer.
//!
//! The Discord desktop app listens on a local socket, `discord-ipc-0` to `discord-ipc-9` (a named
//! pipe on Windows, a Unix socket elsewhere: [`endpoints`]). A program that names a Discord
//! application there may set the player's activity with no sign-in at all: a handshake naming the
//! application ([`handshake`]), then `SET_ACTIVITY` ([`set_activity`]), which the app answers. Each
//! message is a frame: its [`Opcode`] and its length, both little-endian 32-bit numbers, then that
//! many bytes of JSON ([`encode`], [`read_frame`]).
//!
//! Discord then shows the player as playing the application, "Dereth Network", with the
//! [`Activity`] under it: the world, then the character when the launcher knows it, the client and
//! the era, and how long the game has been running. The connection itself, and when to speak,
//! are the launcher app's; this module is what is said.

use std::ffi::OsString;
use std::io::{self, Read};
use std::path::PathBuf;

use serde_json::{json, Value};

use crate::folders::System;
use crate::install::ClientKind;
use crate::world::World;

/// The Dereth Network's Discord application, whose name Discord shows over the activity.
pub const APPLICATION_ID: &str = "1557643227513757726";

/// The variable that names another application, for trying the presence against a test one.
pub const APPLICATION_ID_VAR: &str = "DERETH_DISCORD_APPLICATION_ID";

/// The key of the picture shown beside the activity, among the application's uploaded art. `None`
/// while it has none: the activity is shown without a picture.
pub const LARGE_IMAGE: Option<&str> = None;

/// The variable that names another picture's key, for trying art before a release names it.
pub const LARGE_IMAGE_VAR: &str = "DERETH_DISCORD_LARGE_IMAGE";

/// How many sockets the Discord app may listen on: one per copy running (the stable app, a test
/// build), each taking the first number free.
pub const ENDPOINT_COUNT: u32 = 10;

/// The largest frame read. The app's answers are a few hundred bytes.
pub const MAX_FRAME: u32 = 64 * 1024;

/// The longest a line of the activity may be, in characters; Discord refuses a longer one.
const MAX_LINE: usize = 128;

/// The application the launcher speaks for, and its picture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Application {
    pub id: String,
    pub large_image: Option<String>,
}

impl Application {
    /// The Dereth Network's application, or the ones the environment `var` reads names instead.
    pub fn from_env(var: &dyn Fn(&str) -> Option<OsString>) -> Self {
        let named = |name: &str| {
            var(name)
                .and_then(|v| v.into_string().ok())
                .map(|v| v.trim().to_owned())
                .filter(|v| !v.is_empty())
        };
        Self {
            id: named(APPLICATION_ID_VAR).unwrap_or_else(|| APPLICATION_ID.to_owned()),
            large_image: named(LARGE_IMAGE_VAR).or_else(|| LARGE_IMAGE.map(str::to_owned)),
        }
    }
}

/// What Discord shows under the application's name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Activity {
    /// The first line: the world's name.
    pub details: String,
    /// The second line: the character when the launcher knows it, the client, and the era.
    pub state: String,
    /// When the game started, in seconds since the epoch; Discord counts the time played from it.
    pub start: u64,
}

impl Activity {
    /// The activity of a game of `client` started at `start` on `world`, played by `character`
    /// when the launcher knows who. Never the account: that is half of a login.
    pub fn new(world: &World, client: ClientKind, character: Option<&str>, start: u64) -> Self {
        let era = crate::eras::label(world.era.as_deref());
        let state = character
            .map(str::trim)
            .filter(|c| !c.is_empty())
            .into_iter()
            .chain([client_label(client), era.as_str()])
            .collect::<Vec<_>>()
            .join(" · ");
        Self {
            details: line(&world.name),
            state: line(&state),
            start,
        }
    }
}

/// The client, as the activity names it.
pub fn client_label(client: ClientKind) -> &'static str {
    match client {
        ClientKind::Dereth => "Dereth",
        ClientKind::Retail => "Retail client",
    }
}

/// A line Discord takes: two characters at the least and [`MAX_LINE`] at the most.
fn line(text: &str) -> String {
    let text = text.trim();
    let out: String = text.chars().take(MAX_LINE).collect();
    if out.chars().count() < 2 {
        return format!("World {out}").trim_end().to_owned();
    }
    out
}

/// The kinds of frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Opcode {
    /// The first frame, naming the application.
    Handshake = 0,
    /// A command, or the app's answer to one.
    Frame = 1,
    /// The app ends the connection, saying why.
    Close = 2,
    /// Either side asks the other to answer with a [`Opcode::Pong`] carrying the same payload.
    Ping = 3,
    Pong = 4,
}

impl Opcode {
    pub fn from_u32(op: u32) -> Option<Self> {
        Some(match op {
            0 => Self::Handshake,
            1 => Self::Frame,
            2 => Self::Close,
            3 => Self::Ping,
            4 => Self::Pong,
            _ => return None,
        })
    }
}

/// One frame read.
#[derive(Debug, Clone, PartialEq)]
pub struct Frame {
    /// The opcode as sent, which may be one this module does not name.
    pub opcode: u32,
    pub payload: Value,
}

/// A frame: the opcode, the payload's length, the payload.
pub fn encode(op: Opcode, payload: &Value) -> Vec<u8> {
    let body = payload.to_string().into_bytes();
    let len = u32::try_from(body.len()).expect("a frame's payload fits a 32-bit length");
    let mut out = Vec::with_capacity(8 + body.len());
    out.extend_from_slice(&(op as u32).to_le_bytes());
    out.extend_from_slice(&len.to_le_bytes());
    out.extend_from_slice(&body);
    out
}

/// Read one frame. A payload longer than [`MAX_FRAME`], or one that is not JSON, is an error,
/// as is the stream ending part-way.
pub fn read_frame(r: &mut dyn Read) -> io::Result<Frame> {
    let mut head = [0u8; 8];
    r.read_exact(&mut head)?;
    let opcode = u32::from_le_bytes([head[0], head[1], head[2], head[3]]);
    let len = u32::from_le_bytes([head[4], head[5], head[6], head[7]]);
    if len > MAX_FRAME {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("a frame of {len} bytes"),
        ));
    }
    let mut body = vec![0u8; len as usize];
    r.read_exact(&mut body)?;
    let payload =
        serde_json::from_slice(&body).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
    Ok(Frame { opcode, payload })
}

/// The handshake: protocol version 1, and the application.
pub fn handshake(app: &Application) -> Vec<u8> {
    encode(Opcode::Handshake, &json!({"v": 1, "client_id": app.id}))
}

/// `SET_ACTIVITY` for the launcher's process `pid`: the activity, or none to clear it. `nonce`
/// is carried back by the app's answer.
pub fn set_activity(
    app: &Application,
    pid: u32,
    nonce: u64,
    activity: Option<&Activity>,
) -> Vec<u8> {
    let mut args = json!({"pid": pid});
    if let Some(a) = activity {
        let mut shown = json!({
            "details": a.details,
            "state": a.state,
            "timestamps": {"start": a.start},
        });
        if let Some(key) = &app.large_image {
            shown["assets"] = json!({"large_image": key});
        }
        args["activity"] = shown;
    }
    encode(
        Opcode::Frame,
        &json!({"cmd": "SET_ACTIVITY", "args": args, "nonce": nonce.to_string()}),
    )
}

/// Whether a frame's payload is the app's greeting, which follows a handshake it accepts.
pub fn is_ready(payload: &Value) -> bool {
    payload["cmd"] == "DISPATCH" && payload["evt"] == "READY"
}

/// Whether a frame's payload answers the command sent with `nonce`.
pub fn answers(payload: &Value, nonce: u64) -> bool {
    payload["nonce"]
        .as_str()
        .and_then(|n| n.parse::<u64>().ok())
        == Some(nonce)
}

/// Whether an answer says the command was refused.
pub fn is_error(payload: &Value) -> bool {
    payload["evt"] == "ERROR"
}

/// Where the Discord app may be listening on `system`, whose environment variables `var` reads,
/// in the order to try: `\\.\pipe\discord-ipc-<n>` on Windows; elsewhere `discord-ipc-<n>` in
/// the first of `XDG_RUNTIME_DIR`, `TMPDIR`, `TMP` and `TEMP` that is set (else `/tmp`), and on
/// Linux also in the folders there that the Flatpak and Snap packages of Discord use.
pub fn endpoints(system: System, var: &dyn Fn(&str) -> Option<OsString>) -> Vec<PathBuf> {
    if system == System::Windows {
        return (0..ENDPOINT_COUNT)
            .map(|n| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{n}")))
            .collect();
    }
    let numbered = |dir: PathBuf| {
        (0..ENDPOINT_COUNT)
            .map(|n| dir.join(format!("discord-ipc-{n}")))
            .collect::<Vec<_>>()
    };
    let base = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"]
        .iter()
        .find_map(|name| var(name).filter(|v| !v.is_empty()))
        .map_or_else(|| PathBuf::from("/tmp"), PathBuf::from);
    let mut dirs = vec![base.clone()];
    if system == System::Linux {
        dirs.push(base.join("app").join("com.discordapp.Discord"));
        dirs.push(base.join("snap.discord"));
    }
    dirs.into_iter().flat_map(numbered).collect()
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the launcher's Discord presence: what it shows, and the frames it sends).
    use super::*;
    use crate::world::Told;

    fn world(name: &str, era: Option<&str>) -> World {
        let mut w = World::new("coldeve", name);
        w.era = era.map(str::to_owned);
        w.era_source = era.map(|_| Told::World);
        w
    }

    fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<OsString> {
        let pairs: Vec<(String, String)> = pairs
            .iter()
            .map(|(k, v)| ((*k).to_owned(), (*v).to_owned()))
            .collect();
        move |name: &str| {
            pairs
                .iter()
                .find(|(k, _)| k == name)
                .map(|(_, v)| OsString::from(v))
        }
    }

    #[test]
    fn a_world_naming_no_era_shows_its_name_the_client_and_the_end_of_retail() {
        let a = Activity::new(
            &world("Coldeve", None),
            ClientKind::Dereth,
            None,
            1_700_000_000,
        );
        assert_eq!(a.details, "Coldeve");
        assert_eq!(a.state, "Dereth · End of Retail");
        assert_eq!(a.start, 1_700_000_000);
    }

    #[test]
    fn a_world_naming_its_era_shows_that_era_and_the_retail_client_says_so() {
        let a = Activity::new(
            &world("Seedsow", Some("infiltration")),
            ClientKind::Retail,
            None,
            5,
        );
        assert_eq!(a.state, "Retail client · Infiltration (February 2005)");
        let a = Activity::new(
            &world("Frostfell", Some("dark-majesty")),
            ClientKind::Dereth,
            None,
            5,
        );
        assert_eq!(
            a.state, "Dereth · dark-majesty",
            "an era this build does not know, in the world's own word"
        );
    }

    #[test]
    fn the_character_leads_the_state_when_the_launcher_knows_it() {
        let w = world("Coldeve", None);
        let a = Activity::new(&w, ClientKind::Dereth, Some(" Asheron "), 5);
        assert_eq!(a.state, "Asheron · Dereth · End of Retail");
        let a = Activity::new(&w, ClientKind::Dereth, Some(""), 5);
        assert_eq!(
            a.state, "Dereth · End of Retail",
            "an empty name is no name"
        );
    }

    #[test]
    fn a_line_is_kept_to_what_discord_takes() {
        let long = "W".repeat(300);
        let a = Activity::new(&world(&long, None), ClientKind::Dereth, None, 5);
        assert_eq!(a.details.chars().count(), MAX_LINE);
        let a = Activity::new(&world("X", None), ClientKind::Dereth, None, 5);
        assert_eq!(a.details, "World X", "two characters at the least");
    }

    #[test]
    fn a_frame_is_its_opcode_and_length_little_endian_then_the_json_and_reads_back() {
        let bytes = encode(Opcode::Frame, &json!({"a": 1}));
        assert_eq!(&bytes[..8], &[1, 0, 0, 0, 7, 0, 0, 0]);
        assert_eq!(&bytes[8..], br#"{"a":1}"#);
        let mut two = bytes.clone();
        two.extend(encode(Opcode::Pong, &json!("x")));
        let mut r = two.as_slice();
        assert_eq!(
            read_frame(&mut r).unwrap(),
            Frame {
                opcode: 1,
                payload: json!({"a": 1})
            }
        );
        assert_eq!(read_frame(&mut r).unwrap().opcode, Opcode::Pong as u32);
        assert!(read_frame(&mut r).is_err(), "nothing more");
        assert_eq!(Opcode::from_u32(2), Some(Opcode::Close));
        assert_eq!(Opcode::from_u32(9), None);
    }

    #[test]
    fn a_frame_too_long_or_not_json_or_cut_short_is_refused() {
        let mut head = Vec::from(1u32.to_le_bytes());
        head.extend((MAX_FRAME + 1).to_le_bytes());
        assert!(read_frame(&mut head.as_slice()).is_err());
        let mut bad = Vec::from(1u32.to_le_bytes());
        bad.extend(3u32.to_le_bytes());
        bad.extend(b"{x}");
        assert!(read_frame(&mut bad.as_slice()).is_err());
        let cut = encode(Opcode::Frame, &json!({"a": 1}));
        assert!(read_frame(&mut &cut[..cut.len() - 1]).is_err());
    }

    fn payload(frame: &[u8]) -> (u32, Value) {
        let f = read_frame(&mut &frame[..]).unwrap();
        (f.opcode, f.payload)
    }

    #[test]
    fn the_handshake_names_the_application_and_set_activity_carries_the_activity() {
        let app = Application {
            id: APPLICATION_ID.into(),
            large_image: None,
        };
        assert_eq!(
            payload(&handshake(&app)),
            (0, json!({"v": 1, "client_id": "1557643227513757726"}))
        );
        let a = Activity::new(
            &world("Coldeve", None),
            ClientKind::Dereth,
            None,
            1_700_000_000,
        );
        let (op, v) = payload(&set_activity(&app, 4242, 7, Some(&a)));
        assert_eq!(op, Opcode::Frame as u32);
        assert_eq!(
            v,
            json!({"cmd": "SET_ACTIVITY", "nonce": "7", "args": {"pid": 4242, "activity": {
                "details": "Coldeve", "state": "Dereth · End of Retail",
                "timestamps": {"start": 1_700_000_000u64}}}})
        );
        assert!(answers(&json!({"nonce": "7"}), 7));
        assert!(!answers(&json!({"nonce": "8"}), 7));
    }

    #[test]
    fn the_picture_is_sent_only_when_the_application_has_one_and_none_clears() {
        let app = Application {
            id: "1".into(),
            large_image: Some("dereth".into()),
        };
        let a = Activity::new(&world("Coldeve", None), ClientKind::Dereth, None, 5);
        let (_, v) = payload(&set_activity(&app, 1, 1, Some(&a)));
        assert_eq!(
            v["args"]["activity"]["assets"],
            json!({"large_image": "dereth"})
        );
        let (_, v) = payload(&set_activity(&app, 1, 2, None));
        assert_eq!(
            v,
            json!({"cmd": "SET_ACTIVITY", "nonce": "2", "args": {"pid": 1}}),
            "no activity: Discord shows none"
        );
    }

    #[test]
    fn the_application_is_the_dereth_networks_unless_the_environment_names_another() {
        let app = Application::from_env(&env(&[]));
        assert_eq!(app.id, APPLICATION_ID);
        assert_eq!(app.large_image.as_deref(), LARGE_IMAGE);
        let app = Application::from_env(&env(&[
            (APPLICATION_ID_VAR, " 42 "),
            (LARGE_IMAGE_VAR, "emblem"),
        ]));
        assert_eq!(
            (app.id.as_str(), app.large_image.as_deref()),
            ("42", Some("emblem"))
        );
        let app = Application::from_env(&env(&[(APPLICATION_ID_VAR, "")]));
        assert_eq!(app.id, APPLICATION_ID, "an empty variable names nothing");
    }

    #[test]
    fn the_app_is_looked_for_on_each_systems_sockets_in_order() {
        let windows = endpoints(System::Windows, &env(&[("TMPDIR", "/x")]));
        assert_eq!(windows.len(), 10);
        assert_eq!(windows[0], PathBuf::from(r"\\.\pipe\discord-ipc-0"));
        assert_eq!(windows[9], PathBuf::from(r"\\.\pipe\discord-ipc-9"));

        let mac = endpoints(System::MacOs, &env(&[("TMPDIR", "/var/folders/t/T/")]));
        assert_eq!(mac.len(), 10);
        assert_eq!(mac[0], PathBuf::from("/var/folders/t/T/discord-ipc-0"));

        let linux = endpoints(
            System::Linux,
            &env(&[("XDG_RUNTIME_DIR", "/run/user/1000"), ("TMPDIR", "/x")]),
        );
        assert_eq!(linux.len(), 30);
        assert_eq!(linux[0], PathBuf::from("/run/user/1000/discord-ipc-0"));
        assert_eq!(
            linux[10],
            PathBuf::from("/run/user/1000/app/com.discordapp.Discord/discord-ipc-0")
        );
        assert_eq!(
            linux[20],
            PathBuf::from("/run/user/1000/snap.discord/discord-ipc-0")
        );
        assert_eq!(
            endpoints(System::Linux, &env(&[]))[0],
            PathBuf::from("/tmp/discord-ipc-0")
        );
    }
}
