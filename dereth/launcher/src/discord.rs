//! Rich presence: while a game the launcher started is running, the Discord app on this computer
//! shows it, with no Discord sign-in and no server between them.
//!
//! [`Presence`] keeps Discord told. Once a second it is handed what should be shown
//! ([`crate::backend::Backend::presence`]): the activity of the last game started and still
//! running, or nothing once it exits or the player turns the setting off. It connects to the
//! Discord app's socket ([`dereth_launch::discord::endpoints`]) only while there is something to
//! show, says the activity whenever it changes and again every [`REFRESH`], and clears it and
//! disconnects when there is nothing to show. A Discord that is not running, refuses the
//! application or goes away is never an error the player sees: the launcher looks for it again
//! every [`RETRY`] for as long as a game runs, so a Discord started later, or restarted, shows the
//! game again. The repeat every [`REFRESH`] is how a Discord that went away is noticed while
//! nothing changes.

use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use dereth_launch::discord::{self, Activity, Application, Opcode};
use dereth_launch::folders::System;

use crate::backend::{lock, Shared};

/// How often Discord is looked for while a game runs and it has not been found.
pub const RETRY: Duration = Duration::from_secs(10);

/// How often the activity is said again while it is unchanged, well inside how often Discord lets
/// an activity change.
pub const REFRESH: Duration = Duration::from_secs(30);

/// How long an answer is waited for, where the system lets a wait be bounded (a Unix socket).
#[cfg(unix)]
const ANSWER_WITHIN: Duration = Duration::from_secs(5);

/// Frames read past while waiting for an answer, before the connection is given up on.
const FRAMES_PAST: usize = 16;

/// A connection to the Discord app.
trait Pipe: Read + Write + Send {}
impl<T: Read + Write + Send> Pipe for T {}

/// Connect to the socket at `path`: a named pipe, opened as a file, on Windows; a Unix socket
/// elsewhere.
fn open(path: &Path) -> io::Result<Box<dyn Pipe>> {
    #[cfg(windows)]
    {
        let pipe = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(path)?;
        Ok(Box::new(pipe))
    }
    #[cfg(unix)]
    {
        let socket = std::os::unix::net::UnixStream::connect(path)?;
        socket.set_read_timeout(Some(ANSWER_WITHIN))?;
        socket.set_write_timeout(Some(ANSWER_WITHIN))?;
        Ok(Box::new(socket))
    }
    #[cfg(not(any(windows, unix)))]
    {
        let _ = path;
        Err(io::ErrorKind::Unsupported.into())
    }
}

/// Send `frame`, then read until a frame that `wanted` takes, answering the app's pings on the way.
/// The app closing the connection, or a frame that cannot be read, is an error.
fn exchange(
    pipe: &mut dyn Pipe,
    frame: &[u8],
    wanted: &dyn Fn(&serde_json::Value) -> bool,
) -> io::Result<serde_json::Value> {
    pipe.write_all(frame)?;
    pipe.flush()?;
    for _ in 0..FRAMES_PAST {
        let f = discord::read_frame(pipe)?;
        match Opcode::from_u32(f.opcode) {
            Some(Opcode::Frame) if wanted(&f.payload) => return Ok(f.payload),
            Some(Opcode::Ping) => {
                pipe.write_all(&discord::encode(Opcode::Pong, &f.payload))?;
                pipe.flush()?;
            }
            Some(Opcode::Close) => {
                return Err(io::Error::new(
                    io::ErrorKind::ConnectionAborted,
                    format!("Discord closed the connection: {}", f.payload),
                ))
            }
            _ => {}
        }
    }
    Err(io::Error::new(
        io::ErrorKind::InvalidData,
        "Discord did not answer",
    ))
}

/// What Discord is told, and the connection it is told over.
pub struct Presence {
    app: Application,
    /// Where the Discord app may be listening, in the order tried.
    endpoints: Vec<PathBuf>,
    /// The launcher's process, which the activity belongs to.
    pid: u32,
    pipe: Option<Box<dyn Pipe>>,
    /// What Discord was last told over this connection, and when.
    shown: Option<(Activity, Instant)>,
    /// When Discord was last looked for.
    tried: Option<Instant>,
    nonce: u64,
}

impl std::fmt::Debug for Presence {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Presence")
            .field("app", &self.app)
            .field("connected", &self.pipe.is_some())
            .field("shown", &self.shown)
            .finish_non_exhaustive()
    }
}

impl Presence {
    /// Speak for `app` to a Discord app listening at one of `endpoints`.
    pub fn new(app: Application, endpoints: Vec<PathBuf>) -> Self {
        Self {
            app,
            endpoints,
            pid: std::process::id(),
            pipe: None,
            shown: None,
            tried: None,
            nonce: 0,
        }
    }

    /// The Dereth Network's application (or the one the environment names), and wherever this
    /// system's Discord app listens.
    pub fn from_env() -> Self {
        let var = |name: &str| std::env::var_os(name);
        Self::new(
            Application::from_env(&var),
            discord::endpoints(System::HOST, &var),
        )
    }

    /// Whether the Discord app is connected.
    #[cfg(test)]
    fn connected(&self) -> bool {
        self.pipe.is_some()
    }

    /// Bring Discord to `wanted` at `now`: show it, or clear what is shown and disconnect.
    pub fn update(&mut self, wanted: Option<&Activity>, now: Instant) {
        let Some(activity) = wanted else {
            if self.pipe.is_some() {
                // The connection closing clears it too; saying so first is the polite way.
                let _ = self.say(None);
            }
            self.disconnect();
            return;
        };
        let due = self.shown.as_ref().is_none_or(|(shown, at)| {
            shown != activity || now.saturating_duration_since(*at) >= REFRESH
        });
        if self.pipe.is_some() && due && self.show(activity, now).is_err() {
            // Discord quit or restarted: looked for again below.
            self.disconnect();
        }
        let retry = self
            .tried
            .is_none_or(|t| now.saturating_duration_since(t) >= RETRY);
        if self.pipe.is_none() && retry {
            self.tried = Some(now);
            if self.connect().is_ok() && self.show(activity, now).is_err() {
                self.disconnect();
            }
        }
    }

    fn show(&mut self, activity: &Activity, now: Instant) -> io::Result<()> {
        self.say(Some(activity))?;
        self.shown = Some((activity.clone(), now));
        Ok(())
    }

    /// `SET_ACTIVITY`, waiting for its answer. An answer refusing it leaves the connection up: it
    /// is said again when the activity changes or the refresh is due.
    fn say(&mut self, activity: Option<&Activity>) -> io::Result<()> {
        let pipe = self.pipe.as_mut().ok_or(io::ErrorKind::NotConnected)?;
        self.nonce += 1;
        let nonce = self.nonce;
        let frame = discord::set_activity(&self.app, self.pid, nonce, activity);
        let answer = exchange(pipe.as_mut(), &frame, &|p| discord::answers(p, nonce))?;
        if discord::is_error(&answer) {
            eprintln!("dereth-launcher: Discord refused the activity: {answer}");
        }
        Ok(())
    }

    /// Find the Discord app: the first socket that takes the handshake and greets back.
    fn connect(&mut self) -> io::Result<()> {
        let handshake = discord::handshake(&self.app);
        let mut last = io::Error::from(io::ErrorKind::NotFound);
        for path in &self.endpoints {
            let attempt = open(path).and_then(|mut pipe| {
                exchange(pipe.as_mut(), &handshake, &discord::is_ready).map(|_| pipe)
            });
            match attempt {
                Ok(pipe) => {
                    self.pipe = Some(pipe);
                    return Ok(());
                }
                Err(e) => last = e,
            }
        }
        Err(last)
    }

    fn disconnect(&mut self) {
        self.pipe = None;
        self.shown = None;
    }
}

/// Keep Discord told of the launcher's games, on a thread of its own, so a slow or silent Discord
/// never holds up the launcher.
pub fn watch(shared: Shared, mut presence: Presence) {
    std::thread::spawn(move || loop {
        let wanted = lock(&shared).presence();
        presence.update(wanted.as_ref(), Instant::now());
        std::thread::sleep(Duration::from_secs(1));
    });
}

#[cfg(test)]
pub(crate) mod tests {
    //! Behaviour: none (the launcher's Discord presence, against a stand-in for the Discord app).
    use super::*;
    use dereth_launch::install::ClientKind;
    use dereth_launch::world::World;
    use serde_json::{json, Value};
    use std::sync::mpsc::{self, Receiver};

    /// What the stand-in saw: a frame the launcher sent, or the connection ending.
    #[derive(Debug, Clone, PartialEq)]
    pub enum Seen {
        Frame(u32, Value),
        Closed,
    }

    /// How the stand-in behaves.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum Script {
        /// Greets the handshake and answers every command.
        Answer,
        /// Refuses the handshake, as Discord does an application it does not know.
        Refuse,
        /// Answers the first command, then goes away, as Discord quitting does.
        QuitAfterOne,
    }

    /// A unique socket name for one test, never one the Discord app uses.
    pub fn endpoint(name: &str) -> PathBuf {
        let unique = format!("dereth-launcher-test-{}-{name}", std::process::id());
        if cfg!(windows) {
            PathBuf::from(format!(r"\\.\pipe\{unique}"))
        } else {
            std::env::temp_dir().join(unique)
        }
    }

    /// A stand-in for the Discord app at `path`, taking one connection. Listening has begun when
    /// this returns.
    pub fn stand_in(path: &Path, script: Script) -> Receiver<Seen> {
        let accept = listen(path);
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let Ok(mut conn) = accept() else { return };
            let mut answered = 0;
            loop {
                let Ok(f) = discord::read_frame(&mut conn) else {
                    let _ = tx.send(Seen::Closed);
                    return;
                };
                let _ = tx.send(Seen::Frame(f.opcode, f.payload.clone()));
                let reply = match (f.opcode, script) {
                    (0, Script::Refuse) => discord::encode(
                        Opcode::Close,
                        &json!({"code": 4000, "message": "Invalid Client ID"}),
                    ),
                    (0, _) => discord::encode(
                        Opcode::Frame,
                        &json!({"cmd": "DISPATCH", "evt": "READY", "data": {"v": 1}}),
                    ),
                    _ => discord::encode(
                        Opcode::Frame,
                        &json!({"cmd": f.payload["cmd"], "evt": null, "data": {},
                                "nonce": f.payload["nonce"]}),
                    ),
                };
                if conn.write_all(&reply).is_err() {
                    return;
                }
                if f.opcode == 1 {
                    answered += 1;
                }
                if script == Script::Refuse || (script == Script::QuitAfterOne && answered == 1) {
                    hang_up(conn);
                    let _ = tx.send(Seen::Closed);
                    return;
                }
            }
        });
        rx
    }

    /// The server end of a connection, as the stand-in reads and writes it.
    #[cfg(unix)]
    type Conn = std::os::unix::net::UnixStream;
    #[cfg(windows)]
    type Conn = std::fs::File;

    /// Listen at `path`; the call returned waits for one connection.
    #[cfg(unix)]
    fn listen(path: &Path) -> impl FnOnce() -> io::Result<Conn> + Send {
        let _ = std::fs::remove_file(path);
        let listener = std::os::unix::net::UnixListener::bind(path).expect("the stand-in listens");
        move || listener.accept().map(|(conn, _)| conn)
    }

    /// Listen at `path`, a named pipe; the call returned waits for one connection.
    #[cfg(windows)]
    fn listen(path: &Path) -> impl FnOnce() -> io::Result<Conn> + Send {
        use std::os::windows::ffi::OsStrExt;
        use std::os::windows::io::{AsRawHandle, FromRawHandle, OwnedHandle};
        use windows_sys::Win32::Foundation::{
            GetLastError, ERROR_PIPE_CONNECTED, INVALID_HANDLE_VALUE,
        };
        use windows_sys::Win32::Storage::FileSystem::PIPE_ACCESS_DUPLEX;
        use windows_sys::Win32::System::Pipes::{
            ConnectNamedPipe, CreateNamedPipeW, PIPE_READMODE_BYTE, PIPE_TYPE_BYTE,
            PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
        };
        let name: Vec<u16> = path.as_os_str().encode_wide().chain([0]).collect();
        // SAFETY: `name` is a NUL-terminated wide string that outlives the call, and no security
        // attributes are passed.
        let handle = unsafe {
            CreateNamedPipeW(
                name.as_ptr(),
                PIPE_ACCESS_DUPLEX,
                PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT,
                PIPE_UNLIMITED_INSTANCES,
                4096,
                4096,
                0,
                std::ptr::null(),
            )
        };
        assert_ne!(handle, INVALID_HANDLE_VALUE, "the stand-in listens");
        // SAFETY: the handle was just created, is valid, and is owned by nothing else.
        let owned = unsafe { OwnedHandle::from_raw_handle(handle) };
        move || {
            // SAFETY: the handle is a pipe this closure owns; no overlapped structure is passed,
            // so the call blocks until a client connects.
            let ok = unsafe { ConnectNamedPipe(owned.as_raw_handle(), std::ptr::null_mut()) };
            // SAFETY: reads the calling thread's last error, nothing else.
            if ok == 0 && unsafe { GetLastError() } != ERROR_PIPE_CONNECTED {
                return Err(io::Error::last_os_error());
            }
            Ok(std::fs::File::from(owned))
        }
    }

    /// End a connection the way the Discord app quitting does, once the client has read what it
    /// was sent.
    fn hang_up(conn: Conn) {
        #[cfg(windows)]
        let _ = conn.sync_all();
        drop(conn);
    }

    /// The next thing the stand-in saw, waiting a while for it.
    pub fn next(rx: &Receiver<Seen>) -> Seen {
        rx.recv_timeout(Duration::from_secs(10))
            .expect("the stand-in saw something")
    }

    /// Whether the stand-in has seen nothing more, now. Every exchange with it is finished by
    /// the time `update` returns, so what it saw is already there.
    pub fn quiet(rx: &Receiver<Seen>) -> bool {
        matches!(rx.try_recv(), Err(mpsc::TryRecvError::Empty))
    }

    fn app() -> Application {
        Application {
            id: "1557643227513757726".into(),
            large_image: None,
        }
    }

    fn activity(world: &str) -> Activity {
        Activity::new(
            &World::new("w", world),
            ClientKind::Dereth,
            None,
            1_700_000_000,
        )
    }

    /// The stand-in's `SET_ACTIVITY` payload's arguments.
    fn set_activity_args(seen: Seen) -> Value {
        match seen {
            Seen::Frame(1, v) if v["cmd"] == "SET_ACTIVITY" => v["args"].clone(),
            other => panic!("not SET_ACTIVITY: {other:?}"),
        }
    }

    #[test]
    fn the_handshake_names_the_application_and_set_activity_carries_the_world_client_era_and_start()
    {
        let path = endpoint("shows");
        let rx = stand_in(&path, Script::Answer);
        let mut p = Presence::new(app(), vec![endpoint("shows-nobody-here"), path]);
        let t0 = Instant::now();
        p.update(Some(&activity("Coldeve")), t0);
        assert!(p.connected(), "the second socket is the one listening");
        assert_eq!(
            next(&rx),
            Seen::Frame(0, json!({"v": 1, "client_id": "1557643227513757726"}))
        );
        let args = set_activity_args(next(&rx));
        assert_eq!(args["pid"], std::process::id());
        assert_eq!(
            args["activity"],
            json!({"details": "Coldeve", "state": "Dereth · End of Retail",
                   "timestamps": {"start": 1_700_000_000u64}})
        );

        // Unchanged, it is not said again until the refresh is due; changed, it is said at once.
        p.update(Some(&activity("Coldeve")), t0 + Duration::from_secs(1));
        assert!(quiet(&rx));
        p.update(Some(&activity("Coldeve")), t0 + REFRESH);
        assert_eq!(
            set_activity_args(next(&rx))["activity"]["details"],
            "Coldeve"
        );
        p.update(
            Some(&activity("Leafcull")),
            t0 + REFRESH + Duration::from_secs(1),
        );
        assert_eq!(
            set_activity_args(next(&rx))["activity"]["details"],
            "Leafcull"
        );
        assert!(quiet(&rx));
    }

    #[test]
    fn the_presence_is_cleared_and_the_connection_closed_when_the_game_exits() {
        let path = endpoint("clears");
        let rx = stand_in(&path, Script::Answer);
        let mut p = Presence::new(app(), vec![path]);
        let t0 = Instant::now();
        p.update(Some(&activity("Coldeve")), t0);
        assert!(matches!(next(&rx), Seen::Frame(0, _)));
        assert!(set_activity_args(next(&rx)).get("activity").is_some());
        p.update(None, t0 + Duration::from_secs(1));
        assert!(!p.connected());
        assert_eq!(
            set_activity_args(next(&rx)),
            json!({"pid": std::process::id()}),
            "no activity: Discord shows none"
        );
        assert_eq!(next(&rx), Seen::Closed);
        // Nothing to show and nothing shown: nothing more is said, nor looked for.
        p.update(None, t0 + Duration::from_secs(60));
        assert!(!p.connected());
    }

    #[test]
    fn with_no_discord_running_or_one_refusing_the_application_nothing_fails() {
        let mut p = Presence::new(app(), vec![endpoint("absent")]);
        let t0 = Instant::now();
        p.update(Some(&activity("Coldeve")), t0);
        assert!(!p.connected());
        p.update(None, t0 + Duration::from_secs(1));

        let path = endpoint("refuses");
        let rx = stand_in(&path, Script::Refuse);
        let mut p = Presence::new(app(), vec![path]);
        p.update(Some(&activity("Coldeve")), t0);
        assert!(!p.connected(), "a refused handshake is no connection");
        assert!(matches!(next(&rx), Seen::Frame(0, _)));
        assert_eq!(next(&rx), Seen::Closed);
    }

    #[test]
    fn a_discord_started_later_or_restarted_is_found_again_and_shown_the_game() {
        let path = endpoint("restarts");
        let mut p = Presence::new(app(), vec![path.clone()]);
        let coldeve = activity("Coldeve");
        let t0 = Instant::now();
        p.update(Some(&coldeve), t0);
        assert!(!p.connected(), "no Discord yet");

        // Discord starts: it is found at the next look, not before.
        let first = stand_in(&path, Script::QuitAfterOne);
        p.update(Some(&coldeve), t0 + RETRY - Duration::from_secs(1));
        assert!(!p.connected() && quiet(&first));
        p.update(Some(&coldeve), t0 + RETRY);
        assert!(p.connected());
        assert!(matches!(next(&first), Seen::Frame(0, _)));
        assert_eq!(
            set_activity_args(next(&first))["activity"]["details"],
            "Coldeve"
        );
        assert_eq!(next(&first), Seen::Closed, "then Discord quits");

        // It restarts; the refresh finds the old connection gone, and the new one is shown the game.
        let second = stand_in(&path, Script::Answer);
        p.update(Some(&coldeve), t0 + RETRY + REFRESH);
        assert!(p.connected());
        assert!(matches!(next(&second), Seen::Frame(0, _)));
        assert_eq!(
            set_activity_args(next(&second))["activity"]["details"],
            "Coldeve"
        );
    }
}
