//! Playing the world: Yalain started on this machine's loopback address in a folder of its own,
//! serving the world over its base, and a client started into it. After a save the server is
//! started again, so it serves what was saved (it compiles the world as it starts), and a client
//! the editor started is started again with it, standing where the camera is.
//!
//! The server keeps nothing (`--memory`); one character is made for the session, put where the
//! camera stands as it enters. A client that is Auberean logs in by itself as that character; any
//! other (the retail client) is told the server, account and ticket as the launcher told it.

use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};

/// The account and character a play session uses.
pub const ACCOUNT: &str = "builder";
pub const TICKET: &str = "athenaeum";
pub const CHARACTER: &str = "Builder";
/// The character's race and sex, as the server takes them: a human man.
pub const RACE: u32 = 1;
pub const SEX: u32 = 4096;

/// What starting the server and a client needs.
#[derive(Debug, Clone, Default)]
pub struct Programs {
    /// The server.
    pub yalain: Option<PathBuf>,
    /// The install the server reads (a copy no client runs from).
    pub server_dats: Option<PathBuf>,
    /// Our client, and the install it reads.
    pub client: Option<PathBuf>,
    pub client_dats: Option<PathBuf>,
    /// The retail client, run from its own folder.
    pub retail: Option<PathBuf>,
    /// More arguments for whichever client starts.
    pub client_args: Vec<String>,
    /// The login port; the world's is two above.
    pub port: u16,
}

/// A program beside this one, when there is one.
#[must_use]
pub fn beside_me(name: &str) -> Option<PathBuf> {
    let me = std::env::current_exe().ok()?;
    let p = me.with_file_name(format!("{name}{}", std::env::consts::EXE_SUFFIX));
    p.is_file().then_some(p)
}

/// Where the player enters: a cell, a point in its landblock, a heading.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Entry {
    pub cell: u32,
    pub at: [f32; 3],
    pub heading: f32,
}

/// The server's arguments for a world.
#[must_use]
pub fn server_args(
    programs: &Programs,
    world: &Path,
    base: Option<&Path>,
    dats: &Path,
    entry: Entry,
) -> Vec<String> {
    let mut a = vec![
        "--dats".to_string(),
        dats.display().to_string(),
        "--world-db".into(),
        world.display().to_string(),
    ];
    if let Some(b) = base {
        a.extend(["--world-base".into(), b.display().to_string()]);
    }
    a.extend([
        "--memory".into(),
        "--bind".into(),
        "127.0.0.1".into(),
        "--login-port".into(),
        programs.port.to_string(),
        "--world-port".into(),
        (programs.port + 2).to_string(),
        "--world-name".into(),
        "Athenaeum".into(),
        "--add-character".into(),
        format!("{ACCOUNT}:{CHARACTER}:{RACE}:{SEX}"),
        "--place".into(),
        format!(
            "{ACCOUNT}:{CHARACTER}:{:08X}:{}:{}:{}:{}",
            entry.cell, entry.at[0], entry.at[1], entry.at[2], entry.heading
        ),
    ]);
    a
}

/// What is running.
#[derive(Debug, Default)]
pub struct Live {
    server: Option<Child>,
    client: Option<Child>,
    /// The client's kind when one runs: ours or the retail one.
    retail: bool,
    /// A client to start once the server listens: the retail one or ours.
    pending: Option<bool>,
    /// The server's folder, where its log is.
    run: Option<PathBuf>,
    /// What the last start said, for the status line.
    pub said: String,
}

fn stop(c: &mut Option<Child>) {
    if let Some(mut c) = c.take() {
        let _ = c.kill();
        let _ = c.wait();
    }
}

fn running(c: &mut Option<Child>) -> bool {
    match c {
        Some(ch) => match ch.try_wait() {
            Ok(None) => true,
            _ => {
                *c = None;
                false
            }
        },
        None => false,
    }
}

impl Live {
    /// Whether the server runs.
    pub fn serving(&mut self) -> bool {
        running(&mut self.server)
    }

    /// Whether a client the editor started runs.
    pub fn playing(&mut self) -> bool {
        running(&mut self.client)
    }

    /// Start the server for `world` in the folder `run`, stopping one already running.
    ///
    /// # Errors
    /// No server program or install, or it does not start.
    pub fn serve(
        &mut self,
        programs: &Programs,
        world: &Path,
        base: Option<&Path>,
        run: &Path,
        entry: Entry,
    ) -> Result<(), String> {
        stop(&mut self.server);
        let yalain = programs
            .yalain
            .as_deref()
            .ok_or("no server program: give --yalain <yalain.exe>")?;
        let dats = programs
            .server_dats
            .as_deref()
            .ok_or("no install for the server: give --server-dats <install>")?;
        std::fs::create_dir_all(run).map_err(|e| format!("{}: {e}", run.display()))?;
        let log = std::fs::File::create(run.join("yalain.log"))
            .map_err(|e| format!("{}: {e}", run.display()))?;
        let err = log.try_clone().map_err(|e| e.to_string())?;
        let child = Command::new(yalain)
            .args(server_args(programs, world, base, dats, entry))
            .current_dir(run)
            .stdin(Stdio::null())
            .stdout(Stdio::from(log))
            .stderr(Stdio::from(err))
            .spawn()
            .map_err(|e| format!("{}: {e}", yalain.display()))?;
        self.said = format!(
            "Yalain on 127.0.0.1:{} (log {})",
            programs.port,
            run.join("yalain.log").display()
        );
        self.server = Some(child);
        self.run = Some(run.to_path_buf());
        Ok(())
    }

    /// Whether the server has said it listens.
    pub fn listening(&self) -> bool {
        self.run
            .as_ref()
            .and_then(|r| std::fs::read_to_string(r.join("yalain.log")).ok())
            .is_some_and(|l| l.contains("listening on"))
    }

    /// Start a client once the server listens (now, when it does).
    pub fn play_when_ready(&mut self, retail: bool) {
        self.pending = Some(retail);
    }

    /// Each frame: a client waiting for the server starts when it listens; a server that stopped
    /// says why. Answers whether a client started.
    ///
    /// # Errors
    /// The client does not start, or the server stopped.
    pub fn tick(&mut self, programs: &Programs) -> Result<bool, String> {
        if self.server.is_some() && !self.serving() {
            self.pending = None;
            let tail = self
                .run
                .as_ref()
                .and_then(|r| std::fs::read_to_string(r.join("yalain.log")).ok())
                .and_then(|l| {
                    l.lines()
                        .rev()
                        .find(|x| !x.trim().is_empty())
                        .map(str::to_string)
                })
                .unwrap_or_default();
            return Err(format!("the server stopped: {tail}"));
        }
        if let (Some(retail), true) = (self.pending, self.listening()) {
            self.pending = None;
            let run = self.run.clone().unwrap_or_default();
            self.play(programs, &run, retail)?;
            self.said = format!(
                "Yalain on 127.0.0.1:{}; {} client playing as {CHARACTER}",
                programs.port,
                if retail { "the retail" } else { "our" }
            );
            return Ok(true);
        }
        Ok(false)
    }

    /// Start a client into the server: ours, or the retail one with `retail`.
    ///
    /// # Errors
    /// No such client, or it does not start.
    pub fn play(&mut self, programs: &Programs, run: &Path, retail: bool) -> Result<(), String> {
        stop(&mut self.client);
        let port = programs.port.to_string();
        let child = if retail {
            let exe = programs
                .retail
                .as_deref()
                .ok_or("no retail client: give --retail <client.exe>")?;
            let dir = exe.parent().unwrap_or(Path::new("."));
            Command::new(exe)
                .args([
                    "--host",
                    "127.0.0.1",
                    "--port",
                    &port,
                    "--account",
                    ACCOUNT,
                    "--glsticketdirect",
                    TICKET,
                    "--user",
                    CHARACTER,
                ])
                .args(&programs.client_args)
                .current_dir(dir)
                .spawn()
                .map_err(|e| format!("{}: {e}", exe.display()))?
        } else {
            let exe = programs
                .client
                .as_deref()
                .ok_or("no client: give --client <auberean.exe>")?;
            let dats = programs
                .client_dats
                .as_deref()
                .or(programs.server_dats.as_deref())
                .ok_or("no install for the client: give --client-dats <install>")?;
            let settings = run.join("client-settings");
            std::fs::create_dir_all(&settings).map_err(|e| e.to_string())?;
            let log = std::fs::File::create(run.join("client.log")).map_err(|e| e.to_string())?;
            let err = log.try_clone().map_err(|e| e.to_string())?;
            Command::new(exe)
                .args([
                    "--dats".to_string(),
                    dats.display().to_string(),
                    "--server".into(),
                    format!("127.0.0.1:{port}"),
                    "--dev-login".into(),
                    format!("{ACCOUNT}:{TICKET}"),
                    "--dev-character".into(),
                    CHARACTER.into(),
                    "--shots".into(),
                    run.join("shots").display().to_string(),
                ])
                .args(&programs.client_args)
                .env("AUBEREAN_SETTINGS_DIR", &settings)
                .current_dir(run)
                .stdout(Stdio::from(log))
                .stderr(Stdio::from(err))
                .spawn()
                .map_err(|e| format!("{}: {e}", exe.display()))?
        };
        self.retail = retail;
        self.client = Some(child);
        Ok(())
    }

    /// After a save: start the server again, and a client the editor started with it.
    ///
    /// # Errors
    /// As [`Live::serve`] and [`Live::play`].
    pub fn reload(
        &mut self,
        programs: &Programs,
        world: &Path,
        base: Option<&Path>,
        run: &Path,
        entry: Entry,
    ) -> Result<(), String> {
        let replay = self.playing() || self.pending.is_some();
        stop(&mut self.client);
        self.serve(programs, world, base, run, entry)?;
        if replay {
            self.pending = Some(self.retail);
        }
        Ok(())
    }

    /// Stop everything the editor started.
    pub fn stop_all(&mut self) {
        stop(&mut self.client);
        stop(&mut self.server);
    }
}

impl Drop for Live {
    fn drop(&mut self) {
        self.stop_all();
    }
}
