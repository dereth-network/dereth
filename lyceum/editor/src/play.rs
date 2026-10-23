//! The editor's own world, from the editor's side: started and stopped (the Live button), each
//! edit pushed to it once the hand that made it lets go, its answers shown, and the Dereth client
//! started in it at the camera (the Play button). And the world written out for a server
//! (Empyrean, ACE or GDLE, as a script asks), off the frame loop.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use crate::live::{program, Live, LiveSetup, Starting};

use super::EditorApp;

/// Where the editor's world's parts are, as the command line names them.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LiveArgs {
    pub empyrean: Option<PathBuf>,
    pub dir: Option<PathBuf>,
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

impl LiveArgs {
    /// What the world is made of: over the data files of `era` in `dats`, its world database made
    /// from `sql` (the server's SQL); and the later files the client plays it with beside them,
    /// in `client_dats`.
    ///
    /// # Errors
    /// A part that is not there.
    pub fn setup(
        &self,
        (dats, era): (&std::path::Path, dereth_primitives::EraId),
        client_dats: &std::path::Path,
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
            None => (self.playtest.join("world"), true),
        };
        Ok(LiveSetup {
            server,
            import,
            sql: sql.to_path_buf(),
            dats: dats.to_path_buf(),
            era,
            client_dats: client_dats.to_path_buf(),
            dir,
            keep,
        })
    }
}

/// Whether a line of the server's log answers what the editor told it: an import, the overlay
/// read again, or an error.
fn answers(line: &str) -> bool {
    line.contains("Overlay revision ")
        || line.contains("Imported ")
        || line.contains("The data overlay")
        || line.contains("is not a landblock")
        || line.contains(" ERROR ")
}

/// A log line without its time and level and thread: what it says.
fn said(line: &str) -> &str {
    line.split_once("] ").map_or(line, |(_, s)| s).trim()
}

impl EditorApp {
    /// Download the server database on first use, keeping the frame loop responsive.
    ///
    /// # Errors
    /// Missing bundled tools or a cache folder that cannot be determined.
    pub fn fetch_database(&mut self) -> Result<(), String> {
        if self.fetching.is_some() || self.server_sql.is_some() {
            return Ok(());
        }
        let era = self.session.world().era;
        if let Some(path) = crate::catalog::server_database(era) {
            self.database_ready(path);
            return Ok(());
        }
        let text = self.text.table();
        crate::programs::ready(&mut self.live_args.empyrean, &text)?;
        let import = program(self.live_args.empyrean.as_deref(), "empyrean-import")
            .ok_or_else(|| self.say("missing.empyrean"))?;
        let cache = crate::catalog::database_cache()
            .ok_or_else(|| self.say("messages.database_no_cache"))?;
        self.fetching = Some(crate::database::Fetch::start(import, cache, era)?);
        self.panels.message = self.say("messages.database_fetching");
        Ok(())
    }

    fn database_ready(&mut self, path: PathBuf) {
        self.server_sql = Some(path.clone());
        self.catalog = Some(crate::catalog::Catalog::start(
            (self.session.base().dir.clone(), self.session.base().era),
            self.session.region().cloned(),
            Some(path),
            None,
            self.thumbs_root.clone(),
            self.terrain_colors.clone(),
        ));
        self.panels.message = self.say("messages.database_ready");
    }

    /// Start the editor's own world (it comes up over the frames that follow), or stop it and
    /// the client playing it.
    ///
    /// # Errors
    /// A part of it that is not there, or an overlay that will not write.
    pub fn set_live(&mut self, on: bool) -> Result<(), String> {
        if !on {
            self.starting = None;
            self.character_work = None;
            self.waiting_play = None;
            self.stop_embedded();
            if let Some(mut l) = self.live.take() {
                l.stop();
                self.panels.message = self.say("messages.live_stopped");
            }
            return Ok(());
        }
        if self.live.is_some() || self.starting.is_some() {
            return Ok(());
        }
        let text = self.text.table();
        let args = &mut self.live_args;
        crate::programs::ready(&mut args.empyrean, &text)?;
        if self.server_sql.is_none() {
            return Err(self.say("messages.database_needed"));
        }
        let base = self.session.base();
        let setup = self.live_args.setup(
            (&base.dir, base.era),
            &self.core.cfg.dat_dir,
            self.server_sql.as_deref(),
        )?;
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

    fn went_live(&mut self, mut live: Live) -> Result<(), String> {
        tracing::info!(
            "the editor's world is up on 127.0.0.1:{} in {}",
            live.port,
            live.dir.display()
        );
        self.panels.message = self.say_with("messages.live_up", &[("port", &live.port)]);
        live.command(&format!(
            "playtest-character schema {}",
            crate::live::console_path(&self.live_args.playtest.join("character-schema.json"))
        ))?;
        self.characters.schema = None;
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
        let mut near = self.near_camera();
        near.extend(self.play_resident_blocks());
        let Some(mut live) = self.live.take() else {
            return Ok(());
        };
        let result = (|| {
            if let Some(revision) = live.push(&self.session, &near)? {
                if let Err(error) = self.adopt_play_revision(&revision) {
                    return match live.reject_pending() {
                        Ok(()) => Err(error),
                        Err(recovery) => {
                            live.stop();
                            Err(format!("{error}; recovery failed: {recovery}"))
                        }
                    };
                }
                tracing::info!(revision=revision.number, blocks=?revision.blocks, "live revision adopted by server and playtest");
                self.panels.message =
                    self.say_with("play.updated", &[("revision", &revision.number)]);
                live.accept_pending();
            }
            Ok(())
        })();
        live.error = result.as_ref().err().cloned();
        self.live = Some(live);
        result
    }

    /// Write the world as the editor holds it out for `target`, into the build's
    /// `export/<target>` folder, off the frame loop (or waited for: `wait`, a script's).
    ///
    /// # Errors
    /// No import tool where the server needs one, an overlay that would not write, or (waited
    /// for) the export's refusal.
    pub fn export_world(
        &mut self,
        target: lyceum_foundry::export::Target,
        wait: bool,
    ) -> Result<(), String> {
        use lyceum_foundry::export::{self, Era, ImportProgram, Importer, Options};
        let import = program(self.live_args.empyrean.as_deref(), "empyrean-import");
        if target.needs_world_database() && import.is_none() {
            return Err("no empyrean-import: name the server's folder with --empyrean".into());
        }
        let world = self.session.world();
        let out = world.out.join("export").join(target.word());
        let key = world.key.clone();
        let staging = std::env::temp_dir().join(format!(
            "lyceum-export-{}-{}",
            std::process::id(),
            target.word()
        ));
        lyceum_foundry::compile::write_overlay(
            self.session.compiled(),
            self.session.base(),
            &key,
            &staging,
        )
        .map_err(|e| e.to_string())?;
        let sql = self.session.compiled().sql.clone();
        let dats = self.session.base().dir.clone();
        let world_sql = self.server_sql.clone();
        let gdle_quests = Some(self.gdle_quests.trim())
            .filter(|q| target == lyceum_foundry::export::Target::Gdle && !q.is_empty())
            .map(std::path::PathBuf::from);
        let era = Era::of(world.era);
        self.panels.message =
            self.say_with("messages.export_writing", &[("target", &target.name())]);
        let (tx, rx) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let importer = import.map(ImportProgram);
            let opts = Options {
                target,
                out: &out,
                era,
                world_sql: world_sql.as_deref(),
                importer: importer.as_ref().map(|i| i as &dyn Importer),
                gdle_quests: gdle_quests.as_deref(),
            };
            let done = export::export_compiled(&staging, &sql, &key, &dats, &opts)
                .map(|d| d.out.display().to_string())
                .map_err(|e| e.to_string());
            let _ = std::fs::remove_dir_all(&staging);
            let _ = tx.send(done);
        });
        self.export = Some((target, rx));
        if wait {
            let r = self
                .export
                .take()
                .map(|(_, rx)| rx.recv().map_err(|e| e.to_string()))
                .transpose()?;
            return self.export_done(target, r.unwrap_or_else(|| Err("no export".into())));
        }
        Ok(())
    }

    fn export_done(
        &mut self,
        target: lyceum_foundry::export::Target,
        r: Result<String, String>,
    ) -> Result<(), String> {
        let name = target.name();
        match r {
            Ok(out) => {
                tracing::info!("exported for {name} to {out}");
                self.panels.message = self.say_with(
                    "messages.export_written",
                    &[("target", &name), ("out", &out)],
                );
                Ok(())
            }
            Err(e) => {
                self.panels.message =
                    self.say_with("messages.export_failed", &[("target", &name), ("why", &e)]);
                Err(self.panels.message.clone())
            }
        }
    }

    /// Each frame: the start's end, the edits waiting (once no drag or stroke goes on), the
    /// server's answers, and a server that ended.
    pub(super) fn live_frame(&mut self) {
        if let Some(result) = self
            .fetching
            .as_ref()
            .and_then(crate::database::Fetch::poll)
        {
            self.fetching = None;
            match result {
                Ok(path) => self.database_ready(path),
                Err(error) => {
                    self.panels.message =
                        self.say_with("messages.database_failed", &[("why", &error)])
                }
            }
        }
        if let Some((target, r)) = self
            .export
            .as_ref()
            .and_then(|(target, rx)| rx.try_recv().ok().map(|r| (*target, r)))
        {
            self.export = None;
            let _ = self.export_done(target, r);
        }
        if let Some(r) = self.starting.as_ref().and_then(Starting::poll) {
            self.starting = None;
            if let Err(e) = r.and_then(|l| self.went_live(l)) {
                self.panels.message = e;
            }
        }
        if let Some(mut l) = self.live.take_if(|l| !l.running()) {
            self.waiting_play = None;
            self.stop_embedded();
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
        if self.characters.schema.is_none() {
            if let Ok(bytes) = std::fs::read(self.live_args.playtest.join("character-schema.json"))
            {
                if let Ok(schema) = serde_json::from_slice::<crate::characters::Schema>(&bytes) {
                    if schema.version == 1 {
                        self.characters.schema = Some(schema);
                    }
                }
            }
        }
        self.waiting_play_frame();
        self.character_frame();
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
        if let Some(error) = &live.error {
            self.panels.message.clone_from(error);
        }
    }

    /// Wait for the editor's world to hold no player (a playtest's character logs off as it
    /// stops, and is in the world until its logout ends). False when it did not empty, or no
    /// world runs.
    pub fn wait_world_empty(&mut self) -> bool {
        self.live.as_mut().is_some_and(Live::wait_empty)
    }

    /// The server's log from line `from` on, as a test reads it.
    #[must_use]
    pub fn live_log(&self, from: usize) -> Vec<String> {
        self.live
            .as_ref()
            .map(|l| l.log_from(from))
            .unwrap_or_default()
    }

    /// Start a playtest in the editor's world view, its character put on the floor of the cell
    /// the camera is in, or on the ground under it, with `extra` after the editor's own switches.
    /// It starts once the world is ready for it (a character that played last has logged out),
    /// over the frames that follow; `wait` waits for it to end (a script's run), answering how it
    /// ended. With a playtest running, the game is given the keyboard and the mouse again.
    ///
    /// # Errors
    /// No world running, or a client that would not start.
    pub fn play(&mut self, extra: &[String], wait: bool) -> Result<Option<i32>, String> {
        if self
            .playing
            .as_ref()
            .is_some_and(|p| p.name != self.characters.name)
        {
            self.stop_embedded();
        }
        if self.playing.is_some() {
            self.focus_game(true);
            return Ok(None);
        }
        if self.waiting_play.is_some() {
            return if wait { self.wait_play() } else { Ok(None) };
        }
        self.characters.save()?;
        if !self.characters.applied().is_file() {
            self.prepare_character(false)?;
            if let Some(work) = self.character_work.as_mut() {
                work.extra = extra.to_vec();
            }
            return if wait { self.wait_play() } else { Ok(None) };
        }
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
        if self.live.is_none() {
            return Err(self.say("messages.live_not_running"));
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
        self.waiting_play = Some(WaitingPlay {
            extra: extra.to_vec(),
            cell,
            at: [x, y, z],
            ask: crate::live::ReadyAsk::default(),
        });
        self.panels.message = self.say("play.waiting");
        if wait {
            return self.wait_play();
        }
        Ok(None)
    }

    /// Stop the playtest, or one waiting to start: the game logs off and shuts down, and the
    /// editor has its whole window again.
    pub fn stop_play(&mut self) {
        let was = self.playing.is_some() || self.waiting_play.is_some();
        self.waiting_play = None;
        if self.character_work.is_some() && self.playing.is_some() {
            self.character_work = None;
        }
        self.stop_embedded();
        if was {
            self.panels.message = self.say("play.stopped");
        }
    }

    /// Each frame: a playtest waiting for the world started once the world is ready for it.
    pub(super) fn waiting_play_frame(&mut self) {
        let Some(mut waiting) = self.waiting_play.take() else {
            return;
        };
        let Some(live) = self.live.as_mut() else {
            return;
        };
        match live.poll_ready(&mut waiting.ask) {
            Ok(false) => self.waiting_play = Some(waiting),
            Ok(true) => {
                if let Err(e) = self.start_waiting_play(&waiting) {
                    self.panels.message = e;
                }
            }
            Err(e) => self.panels.message = e,
        }
    }

    /// Start the client of a playtest whose world is ready for it.
    fn start_waiting_play(&mut self, waiting: &WaitingPlay) -> Result<(), String> {
        let dats = self.core.cfg.dat_dir.clone();
        let live = self
            .live
            .as_mut()
            .ok_or("the editor's world is not running")?;
        let mut args = live.play_args(&dats, waiting.cell, waiting.at);
        if let Some(at) = args.iter().position(|s| s == "-u" || s == "-r") {
            args[at] = "-r".into();
            args[at + 1] = self.characters.name.clone();
        }
        args.extend(waiting.extra.iter().cloned());
        live.made_character = true;
        // The client's own settings, beside the world: never the player's.
        let settings = self.live_args.playtest.join("client");
        write_play_options(&settings, self.play_options)?;
        self.start_embedded(&args, &settings)?;
        if let Some(play) = self.playing.as_mut() {
            play.destination = Some((waiting.cell, waiting.at));
        }
        Ok(())
    }

    fn wait_play(&mut self) -> Result<Option<i32>, String> {
        let started = std::time::Instant::now();
        let mut entered = false;
        while self.playing.is_some() || self.character_work.is_some() || self.waiting_play.is_some()
        {
            if self.character_work.is_none() {
                entered |= self.play_ready();
            }
            if !self.frame() {
                break;
            }
            if started.elapsed() > std::time::Duration::from_secs(120) {
                self.character_work = None;
                self.stop_play();
                return Err("the playtest did not finish within two minutes".into());
            }
        }
        self.read_live();
        if !entered {
            return Err(format!(
                "the playtest ended before reaching the editor camera: {}",
                self.characters.message
            ));
        }
        Ok(Some(0))
    }

    /// Start a playtest and wait until its character stands at the editor's camera, the game
    /// running on in the world view (a script's or a test's).
    ///
    /// # Errors
    /// No world running, a client that would not start, or one that did not arrive.
    pub fn play_until_entered(&mut self, extra: &[String]) -> Result<(), String> {
        self.play(extra, false)?;
        let started = std::time::Instant::now();
        while !self.play_ready() {
            if !self.frame() {
                return Err("the editor closed while the playtest started".into());
            }
            if self.playing.is_none()
                && self.character_work.is_none()
                && self.waiting_play.is_none()
            {
                return Err(format!(
                    "the playtest ended before reaching the editor camera: {}",
                    self.panels.message
                ));
            }
            if started.elapsed() > std::time::Duration::from_secs(120) {
                self.stop_play();
                return Err("the playtest did not arrive within two minutes".into());
            }
        }
        Ok(())
    }
}

/// A playtest asked for, waiting for the world to be ready for it: the switches after the
/// editor's, where its character is put, and the question the world is being asked.
#[derive(Debug)]
pub(super) struct WaitingPlay {
    extra: Vec<String>,
    cell: u32,
    at: [f32; 3],
    ask: crate::live::ReadyAsk,
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
            playtest: playtest_dir(Path::new("worlds/mine/world.toml")),
        };
        let (dats, sql) = (
            (std::path::Path::new("dats"), dereth_primitives::EraId::Eor),
            std::path::Path::new("d.sql"),
        );
        let client = std::path::Path::new("dats");
        // No programs there; then no import tool; then no SQL.
        assert!(a.setup(dats, client, Some(sql)).is_err());
        std::fs::write(exe("empyrean-server"), b"").unwrap();
        assert!(a.setup(dats, client, Some(sql)).is_err());
        std::fs::write(exe("empyrean-import"), b"").unwrap();
        assert!(a.setup(dats, client, None).is_err());
        let s = a.setup(dats, client, Some(sql)).unwrap();
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
        let s = b.setup(dats, client, Some(sql)).unwrap();
        assert!(s.keep, "playtest characters survive closing the editor");
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
