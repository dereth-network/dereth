//! The editor's own world, from the editor's side: started and stopped (the Live button), each
//! edit pushed to it once the hand that made it lets go, its answers shown, and the Dereth client
//! started in it at the camera (the Play button). And the world's content written for GDLE (the
//! Export for GDLE button), by the server's import tool, off the frame loop.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::live::{program, Live, LiveSetup, Starting};

use super::EditorApp;

/// Where the editor's world's parts are, as the command line names them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveArgs {
    pub empyrean: Option<PathBuf>,
    pub dir: Option<PathBuf>,
    pub client: Option<PathBuf>,
    /// The world's playtest folder ([`playtest_dir`]): the world's server keeps its files there
    /// unless `dir` names a folder, and the client playing it its settings.
    pub playtest: PathBuf,
}

/// The folder beside world file `world` that its playtests keep their files in: the world's
/// server and its data overlay (`world`), and the client's own settings and crash logs
/// (`client`). Never the player's settings, and nothing under the user's local data.
#[must_use]
pub fn playtest_dir(world: &Path) -> PathBuf {
    world
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."))
        .join(".playtest")
}

/// The interface the playtest client shows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PlayInterface {
    #[default]
    Horizon,
    Modern,
    Classic,
}

impl PlayInterface {
    pub const ALL: [Self; 3] = [Self::Horizon, Self::Modern, Self::Classic];

    /// The word the client's preferences file names it by.
    #[must_use]
    pub const fn word(self) -> &'static str {
        match self {
            Self::Horizon => "horizon",
            Self::Modern => "modern",
            Self::Classic => "classic",
        }
    }

    /// Its name, as the Play options show it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Horizon => "Horizon",
            Self::Modern => "Modern",
            Self::Classic => "Classic",
        }
    }
}

/// How the playtest client starts: its interface, and full screen or in a window.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PlayOptions {
    pub interface: PlayInterface,
    pub full_screen: bool,
}

/// The playtest window's size when the client first starts there: 1280 by 720, as the client's
/// preferences file packs it.
const PLAYTEST_RESOLUTION: u32 = (1280 << 16) | 720;

/// The client's preferences file's name.
const PREFERENCES: &str = "UserPreferences.ini";

/// The value of `key` in `[section]` of a preferences file's `text`.
fn ini_get<'a>(text: &'a str, section: &str, key: &str) -> Option<&'a str> {
    let mut here = false;
    for line in text.lines() {
        let line = line.trim();
        if let Some(name) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            here = name.eq_ignore_ascii_case(section);
        } else if here {
            if let Some((k, v)) = line.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    return Some(v.trim());
                }
            }
        }
    }
    None
}

/// `text` with `key` in `[section]` set to `value`: the line replaced where it is, else added
/// at the end of the section, else the section added at the end.
fn ini_set(text: &str, section: &str, key: &str, value: &str) -> String {
    let mut out: Vec<String> = Vec::new();
    let (mut here, mut done, mut seen) = (false, false, false);
    for line in text.lines() {
        let t = line.trim();
        if let Some(name) = t.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            if here && !done {
                out.push(format!("{key}={value}"));
                done = true;
            }
            here = name.eq_ignore_ascii_case(section);
            seen |= here;
        } else if here && !done {
            if let Some((k, _)) = t.split_once('=') {
                if k.trim().eq_ignore_ascii_case(key) {
                    out.push(format!("{key}={value}"));
                    done = true;
                    continue;
                }
            }
        }
        out.push(line.to_owned());
    }
    if !done {
        if !seen {
            out.push(format!("[{section}]"));
        }
        out.push(format!("{key}={value}"));
    }
    let mut s = out.join("\n");
    s.push('\n');
    s
}

/// The playtest client's options as its preferences file in `settings` holds them; the
/// defaults (Horizon, in a window) where it holds none.
#[must_use]
pub fn read_play_options(settings: &Path) -> PlayOptions {
    let text = std::fs::read_to_string(settings.join(PREFERENCES)).unwrap_or_default();
    let interface = ini_get(&text, "UI", "Interface")
        .and_then(|w| {
            PlayInterface::ALL
                .into_iter()
                .find(|i| i.word().eq_ignore_ascii_case(w))
        })
        .unwrap_or_default();
    let full_screen = ini_get(&text, "Display", "FullScreen")
        .is_some_and(|v| v.eq_ignore_ascii_case("true") || v == "1");
    PlayOptions {
        interface,
        full_screen,
    }
}

/// Put options `o` into the playtest client's preferences file in `settings`. On first use the
/// file is made with the playtest's own start, in a window 1280 by 720; whatever else the client
/// keeps there is left as it is.
///
/// # Errors
/// The folder or the file would not write.
pub fn write_play_options(settings: &Path, o: PlayOptions) -> Result<(), String> {
    std::fs::create_dir_all(settings).map_err(|e| format!("{}: {e}", settings.display()))?;
    let path = settings.join(PREFERENCES);
    let mut text = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        ini_set(
            "",
            "Display",
            "Resolution",
            &PLAYTEST_RESOLUTION.to_string(),
        )
    });
    let full = if o.full_screen { "True" } else { "False" };
    text = ini_set(&text, "Display", "FullScreen", full);
    text = ini_set(&text, "UI", "Interface", o.interface.word());
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))
}

/// The playtest client started with `args`, its settings in `settings` rather than the player's.
#[must_use]
pub fn play_command(client: &Path, args: &[String], settings: &Path) -> std::process::Command {
    let mut c = std::process::Command::new(client);
    c.args(args)
        .env(dereth_desktop::folders::SETTINGS_DIR_ENV, settings)
        .stdin(std::process::Stdio::null());
    c
}

impl LiveArgs {
    /// What the world is made of: over the data files `dats`, its world database made from `sql`
    /// (the server's SQL).
    ///
    /// # Errors
    /// A part that is not there.
    pub fn setup(
        &self,
        dats: &std::path::Path,
        sql: Option<&std::path::Path>,
    ) -> Result<LiveSetup, String> {
        let at = self.empyrean.as_deref();
        let server = program(at, "empyrean-server")
            .ok_or("no empyrean-server: name the server's folder with --empyrean")?;
        let import = program(at, "empyrean-import")
            .ok_or("no empyrean-import: name the server's folder with --empyrean")?;
        let sql = sql.ok_or("no server SQL to make the world of: name it with --weenies")?;
        let (dir, keep) = match &self.dir {
            Some(d) => (d.clone(), true),
            None => (self.playtest.join("world"), false),
        };
        Ok(LiveSetup {
            server,
            import,
            sql: sql.to_path_buf(),
            dats: dats.to_path_buf(),
            dir,
            keep,
        })
    }
}

/// Whether a line of the server's log answers what the editor told it: an import, the overlay
/// read again, or an error.
fn answers(line: &str) -> bool {
    line.contains("Imported ")
        || line.contains("The data overlay")
        || line.contains("is not a landblock")
        || line.contains(" ERROR ")
}

/// A log line without its time and level and thread: what it says.
fn said(line: &str) -> &str {
    line.split_once("] ").map_or(line, |(_, s)| s).trim()
}

impl EditorApp {
    /// Start the editor's own world (it comes up over the frames that follow), or stop it and
    /// the client playing it.
    ///
    /// # Errors
    /// A part of it that is not there, or an overlay that will not write.
    pub fn set_live(&mut self, on: bool) -> Result<(), String> {
        if !on {
            self.starting = None;
            if let Some(mut c) = self.playing.take() {
                let _ = c.kill();
                let _ = c.wait();
            }
            if let Some(mut l) = self.live.take() {
                l.stop();
                self.panels.message = self.say("messages.live_stopped");
            }
            return Ok(());
        }
        if self.live.is_some() || self.starting.is_some() {
            return Ok(());
        }
        let setup = self
            .live_args
            .setup(&self.core.cfg.dat_dir, self.server_sql.as_deref())?;
        tracing::info!("starting the editor's world in {}", setup.dir.display());
        self.starting = Some(Live::launch(&setup, &self.session)?);
        self.panels.message = self.say("messages.live_starting");
        Ok(())
    }

    /// Wait for the editor's world to come up, as a script does.
    ///
    /// # Errors
    /// Why it did not.
    pub fn wait_live(&mut self) -> Result<(), String> {
        if let Some(s) = self.starting.take() {
            let live = s.wait()?;
            self.went_live(live)?;
        }
        Ok(())
    }

    /// Whether the editor's world is up, or starting.
    #[must_use]
    pub fn live_state(&self) -> (bool, bool) {
        (self.live.is_some(), self.starting.is_some())
    }

    fn went_live(&mut self, live: Live) -> Result<(), String> {
        tracing::info!(
            "the editor's world is up on 127.0.0.1:{} in {}",
            live.port,
            live.dir.display()
        );
        self.panels.message = self.say_with("messages.live_up", &[("port", &live.port)]);
        self.live = Some(live);
        self.live_read = 0;
        self.push_live()
    }

    /// The landblocks within one of the camera's.
    fn near_camera(&self) -> BTreeSet<u16> {
        let Some((lb, ..)) = self.work_block() else {
            return BTreeSet::new();
        };
        let (bx, by) = (i32::from(lb >> 8), i32::from(lb & 0xFF));
        (-1..=1)
            .flat_map(|dx| (-1..=1).map(move |dy| (bx + dx, by + dy)))
            .filter(|(x, y)| (0..0xFF).contains(x) && (0..0xFF).contains(y))
            .filter_map(|(x, y)| u16::try_from((x << 8) | y).ok())
            .collect()
    }

    /// Give the editor's world what the edits since the last push changed.
    ///
    /// # Errors
    /// A file that would not write, or a server whose console is closed.
    pub fn push_live(&mut self) -> Result<(), String> {
        self.live_dirty = false;
        let near = self.near_camera();
        let Some(live) = self.live.as_mut() else {
            return Ok(());
        };
        for line in live.push(&self.session, &near)? {
            tracing::info!("the editor's world: {line}");
        }
        Ok(())
    }

    /// Write the world's content for GDLE into the build's `gdle` folder, off the frame loop (or
    /// waited for: `wait`, a script's).
    ///
    /// # Errors
    /// No import tool, no server SQL to write over, or (waited for) the tool's refusal.
    pub fn export_gdle(&mut self, wait: bool) -> Result<(), String> {
        let import = program(self.live_args.empyrean.as_deref(), "empyrean-import")
            .ok_or("no empyrean-import: name the server's folder with --empyrean")?;
        let dump = self
            .server_sql
            .clone()
            .ok_or("no server SQL to write over: name it with --weenies")?;
        let sql = self.session.compiled().sql.clone();
        let out = self.session.world().out.join("gdle");
        let scratch = std::env::temp_dir().join(format!("lyceum-gdle-{}", std::process::id()));
        self.panels.message = self.say("messages.gdle_writing");
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let _ = tx.send(crate::live::export_gdle(
                &import, &dump, &sql, &scratch, &out,
            ));
        });
        self.gdle = Some(rx);
        if wait {
            let r = self
                .gdle
                .take()
                .map(|rx| rx.recv().map_err(|e| e.to_string()))
                .transpose()?;
            return self.gdle_done(r.unwrap_or_else(|| Err("no export".into())));
        }
        Ok(())
    }

    fn gdle_done(&mut self, r: Result<String, String>) -> Result<(), String> {
        match r {
            Ok(said) => {
                tracing::info!("written for GDLE: {said}");
                self.panels.message = self.say_with("messages.gdle_written", &[("said", &said)]);
                Ok(())
            }
            Err(e) => {
                self.panels.message = self.say_with("messages.gdle_failed", &[("why", &e)]);
                Err(self.panels.message.clone())
            }
        }
    }

    /// Each frame: the start's end, the edits waiting (once no drag or stroke goes on), the
    /// server's answers, and a server that ended.
    pub(super) fn live_frame(&mut self) {
        if let Some(r) = self.gdle.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.gdle = None;
            let _ = self.gdle_done(r);
        }
        if let Some(r) = self.starting.as_ref().and_then(Starting::poll) {
            self.starting = None;
            if let Err(e) = r.and_then(|l| self.went_live(l)) {
                self.panels.message = e;
            }
        }
        if let Some(mut l) = self.live.take_if(|l| !l.running()) {
            l.keep_folder();
            self.panels.message = format!(
                "The editor's world stopped on its own; its log is in {}.",
                l.dir.join("server.log").display()
            );
            return;
        }
        if self.live.is_some() && self.live_dirty && self.drag.is_none() && !self.stroke {
            if let Err(e) = self.push_live() {
                self.panels.message = e;
            }
        }
        self.read_live();
    }

    /// The server's answers since they were read last, logged, the last one shown.
    pub fn read_live(&mut self) {
        let Some(live) = self.live.as_ref() else {
            return;
        };
        let lines = live.log_from(self.live_read);
        self.live_read += lines.len();
        for l in lines.iter().filter(|l| answers(l)) {
            tracing::info!("the editor's world says: {}", said(l));
            said(l).clone_into(&mut self.panels.message);
        }
    }

    /// The server's log from line `from` on, as a test reads it.
    #[must_use]
    pub fn live_log(&self, from: usize) -> Vec<String> {
        self.live
            .as_ref()
            .map(|l| l.log_from(from))
            .unwrap_or_default()
    }

    /// Start the Dereth client in the editor's world, its character put on the floor of the cell
    /// the camera is in, or on the ground under it, with `extra` after the editor's own switches; `wait` waits for it to end (a
    /// script's run), answering how it ended.
    ///
    /// # Errors
    /// No world running, no client, or a client that would not start.
    pub fn play(&mut self, extra: &[String], wait: bool) -> Result<Option<i32>, String> {
        let client = self
            .live_args
            .client
            .clone()
            .or_else(|| program(None, "dereth-client"))
            .ok_or_else(|| self.say("messages.no_client"))?;
        let (lb, x, y) = self
            .work_block()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let local_z = self.camera.position.z - lyceum_foundry::pick::block_origin(lb).z;
        // In a cell when the camera is in one, on its floor; else on the ground under it.
        let (cell, z) = match self
            .session
            .cell_at(lb, dereth_primitives::Vec3::new(x, y, local_z))
        {
            Some((c, floor)) => ((u32::from(lb) << 16) | u32::from(c), floor + 0.5),
            None => (
                (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(x, y),
                self.session
                    .ground(lb, x, y)
                    .map_or(self.camera.position.z, |g| g + 0.5),
            ),
        };
        let dats = self.core.cfg.dat_dir.clone();
        if self.live.is_none() {
            return Err(self.say("messages.live_not_running"));
        }
        // A client playing already ends first.
        if let Some(mut c) = self.playing.take() {
            let _ = c.kill();
            let _ = c.wait();
        }
        // What waits to reach the world reaches it first, and the world has taken it.
        if self.live_dirty {
            self.push_live()?;
        }
        let live = self
            .live
            .as_mut()
            .ok_or("the editor's world is not running")?;
        if !live.settle() {
            return Err("the editor's world did not answer what it was told".into());
        }
        // The character a client played last logs in again once its logout has ended.
        if live.made_character && !live.wait_empty() {
            return Err("the character played last is still in the editor's world".into());
        }
        let mut args = live.play_args(&dats, cell, [x, y, z]);
        args.extend(extra.iter().cloned());
        live.made_character = true;
        // The client's own settings, beside the world: never the player's.
        let settings = self.live_args.playtest.join("client");
        write_play_options(&settings, self.play_options)?;
        tracing::info!(
            "playing: {} {} (its settings in {})",
            client.display(),
            args.join(" "),
            settings.display()
        );
        let mut child = play_command(&client, &args, &settings)
            .spawn()
            .map_err(|e| format!("{}: {e}", client.display()))?;
        if wait {
            let status = child.wait().map_err(|e| e.to_string())?;
            self.read_live();
            return Ok(Some(status.code().unwrap_or(-1)));
        }
        self.playing = Some(child);
        Ok(None)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: what the world editor's own world is made of)
    use super::*;

    #[test]
    fn the_world_needs_the_servers_programs_and_its_sql_and_keeps_a_folder_named() {
        let dir = std::env::temp_dir().join(format!("lyceum-play-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let exe = |n: &str| {
            dir.join(if cfg!(windows) {
                format!("{n}.exe")
            } else {
                n.to_owned()
            })
        };
        let a = LiveArgs {
            empyrean: Some(dir.clone()),
            dir: Some(PathBuf::from("live")),
            client: None,
            playtest: playtest_dir(Path::new("worlds/mine/world.toml")),
        };
        let (dats, sql) = (std::path::Path::new("dats"), std::path::Path::new("d.sql"));
        // No programs there; then no import tool; then no SQL.
        assert!(a.setup(dats, Some(sql)).is_err());
        std::fs::write(exe("empyrean-server"), b"").unwrap();
        assert!(a.setup(dats, Some(sql)).is_err());
        std::fs::write(exe("empyrean-import"), b"").unwrap();
        assert!(a.setup(dats, None).is_err());
        let s = a.setup(dats, Some(sql)).unwrap();
        assert_eq!(
            (s.server, s.import, s.sql),
            (
                exe("empyrean-server"),
                exe("empyrean-import"),
                sql.to_path_buf()
            )
        );
        assert_eq!(
            (s.dir.as_path(), s.keep),
            (std::path::Path::new("live"), true)
        );
        let b = LiveArgs { dir: None, ..a };
        let s = b.setup(dats, Some(sql)).unwrap();
        assert!(!s.keep, "a folder of the playtest's");
        assert_eq!(s.dir, Path::new("worlds/mine/.playtest/world"));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_playtest_client_keeps_its_own_settings_beside_the_world_starting_windowed_in_horizon() {
        let dir = std::env::temp_dir().join(format!("lyceum-playtest-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let settings = playtest_dir(&dir.join("world.toml")).join("client");
        assert_eq!(settings, dir.join(".playtest").join("client"));
        // Nothing there yet: Horizon, in a window.
        assert_eq!(read_play_options(&settings), PlayOptions::default());
        write_play_options(&settings, PlayOptions::default()).unwrap();
        let text = std::fs::read_to_string(settings.join(PREFERENCES)).unwrap();
        assert_eq!(ini_get(&text, "Display", "FullScreen"), Some("False"));
        assert_eq!(ini_get(&text, "Display", "Resolution"), Some("83886800"));
        assert_eq!(ini_get(&text, "UI", "Interface"), Some("horizon"));
        // What the client keeps there itself stays; the interface chosen in the editor is set.
        let kept = format!("{text}[Sound]\nVolume=7\n");
        std::fs::write(settings.join(PREFERENCES), kept).unwrap();
        let classic = PlayOptions {
            interface: PlayInterface::Classic,
            full_screen: true,
        };
        write_play_options(&settings, classic).unwrap();
        assert_eq!(read_play_options(&settings), classic);
        let text = std::fs::read_to_string(settings.join(PREFERENCES)).unwrap();
        assert_eq!(ini_get(&text, "Sound", "Volume"), Some("7"));
        assert_eq!(ini_get(&text, "Display", "Resolution"), Some("83886800"));
        // The client is told to keep its settings there.
        let c = play_command(Path::new("dereth-client"), &[], &settings);
        assert!(c
            .get_envs()
            .any(|(k, v)| k == dereth_desktop::folders::SETTINGS_DIR_ENV
                && v == Some(settings.as_os_str())));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn the_servers_answers_are_told_from_its_chatter() {
        let l = "2026-10-19 08:15:39.622 INFO  [World Manager] Imported 90001 guide.sql";
        assert!(answers(l));
        assert_eq!(said(l), "Imported 90001 guide.sql");
        assert!(answers(
            "2026-10-19 08:15:39.622 ERROR [World Manager] The data overlay was not read again: x"
        ));
        assert!(!answers(
            "2026-10-19 08:15:39.622 INFO  [World Manager] Waiting for world to stop..."
        ));
    }
}
