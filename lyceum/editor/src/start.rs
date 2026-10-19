//! How the editor starts: what `--version` says, the start screen a run with no world opens, the
//! worlds it remembers, and the error box a run started from the desktop shows.
//!
//! A run with a world (`lyceum-editor <world.toml>`) opens it, as it always has, and the world joins
//! the recent ones ([`remember`]). A run with none, as a desktop starts the editor (Explorer, Finder,
//! the application menu), opens the start screen: the recent worlds, "Open a world…", "New world
//! from the sample", and the client folder the worlds are drawn over, each chosen in a folder
//! picker of its own. Choosing a world starts the editor again on it, with that client folder, and
//! the start screen ends: one run opens one window.
//!
//! `--list-recent` prints what the start screen would show and opens no window, which is how a
//! build without a display checks it.

use std::path::{Path, PathBuf};

use dereth_desktop::Product as _;
use lyceum_editor::text::{Table, Text};
use lyceum_editor::Lyceum;

/// What `lyceum-editor --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
pub fn version_text() -> String {
    format!(
        "{}\ncommit: {}\ntarget: {}\n",
        Lyceum::BUILD_ID,
        option_env!("LYCEUM_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("LYCEUM_BUILD_TARGET").unwrap_or("unknown"),
    )
}

/// What a command line that names no world asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NoWorld {
    /// The start screen, over the client folder `--dats` named, if it named one.
    Screen { dats: Option<PathBuf> },
    /// What the start screen would show, printed (`--list-recent`).
    List { dats: Option<PathBuf> },
}

/// The start screen's command lines: none at all, `--dats <dir>`, and either with
/// `--list-recent`. `None` for any other, which is a world's (or a usage error, as before).
pub fn no_world(argv: &[String]) -> Option<NoWorld> {
    let mut dats = None;
    let mut list = false;
    let mut it = argv.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--dats" => dats = Some(PathBuf::from(it.next()?)),
            "--list-recent" => list = true,
            _ => return None,
        }
    }
    Some(if list {
        NoWorld::List { dats }
    } else {
        NoWorld::Screen { dats }
    })
}

/// The editor's settings folder, where the recent worlds are kept beside its text.
pub fn settings_dir() -> Option<PathBuf> {
    dereth_desktop::folders::default_settings_dir(Lyceum::SETTINGS_DIR_NAME)
}

/// The recent worlds' file, in the editor's settings folder.
pub const RECENT_FILE: &str = "recent-worlds.toml";

/// How many recent worlds are kept.
const RECENT_KEPT: usize = 10;

/// The worlds opened last, newest first, and the client folder they were drawn over.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Recent {
    pub worlds: Vec<PathBuf>,
    pub dats: Option<PathBuf>,
}

impl Recent {
    /// The recent worlds `text` holds; nothing when it does not read.
    pub fn read(text: &str) -> Self {
        let table: toml::Table = text.parse().unwrap_or_default();
        Self {
            worlds: table
                .get("worlds")
                .and_then(toml::Value::as_array)
                .map(|a| {
                    a.iter()
                        .filter_map(toml::Value::as_str)
                        .map(PathBuf::from)
                        .collect()
                })
                .unwrap_or_default(),
            dats: table
                .get("dats")
                .and_then(toml::Value::as_str)
                .filter(|d| !d.is_empty())
                .map(PathBuf::from),
        }
    }

    /// The file's text.
    pub fn text(&self) -> String {
        let mut t = toml::Table::new();
        if let Some(d) = &self.dats {
            t.insert("dats".into(), d.display().to_string().into());
        }
        t.insert(
            "worlds".into(),
            toml::Value::Array(
                self.worlds
                    .iter()
                    .map(|w| w.display().to_string().into())
                    .collect(),
            ),
        );
        format!(
            "# The worlds Lyceum opened last, newest first, and the client folder it drew them over.\n{t}"
        )
    }

    /// The recent worlds in `file`; none when there is no file.
    pub fn load(file: &Path) -> Self {
        std::fs::read_to_string(file)
            .map(|t| Self::read(&t))
            .unwrap_or_default()
    }

    /// `world`, opened over `dats`, as the newest, once.
    pub fn remember(&mut self, world: &Path, dats: &Path) {
        let world = std::path::absolute(world).unwrap_or_else(|_| world.to_path_buf());
        self.worlds.retain(|w| *w != world);
        self.worlds.insert(0, world);
        self.worlds.truncate(RECENT_KEPT);
        self.dats = Some(std::path::absolute(dats).unwrap_or_else(|_| dats.to_path_buf()));
    }

    /// The recent worlds that are still there.
    pub fn present(&self) -> Vec<PathBuf> {
        self.worlds
            .iter()
            .filter(|w| w.is_file())
            .cloned()
            .collect()
    }
}

/// Keep `world`, opened over the client folder `dats`, as the newest recent world. A file that
/// will not write is said in the log, and the world opens all the same.
pub fn remember(world: &Path, dats: &Path) {
    let Some(dir) = settings_dir() else {
        return;
    };
    let file = dir.join(RECENT_FILE);
    let mut recent = Recent::load(&file);
    recent.remember(world, dats);
    let _ = std::fs::create_dir_all(&dir);
    if let Err(e) = std::fs::write(&file, recent.text()) {
        tracing::warn!("{}: {e}", file.display());
    }
}

/// Whether `dir` holds the client's files.
fn holds_client(dir: &Path) -> bool {
    dereth_dat::holds_modern_dats(dir)
}

/// The world file in `dir`, when it holds one.
fn world_in(dir: &Path) -> Option<PathBuf> {
    Some(dir.join("world.toml")).filter(|p| p.is_file())
}

/// The sample world's files, as the editor carries them, by their place in a world's folder. Its
/// `world.toml` is written for each new world ([`world_toml`]).
const SAMPLE: &[(&str, &[u8])] = &[
    ("world.lock", include_bytes!("../../sample/world.lock")),
    (
        "holtburg/pack.toml",
        include_bytes!("../../sample/holtburg/pack.toml"),
    ),
    (
        "holtburg/art/stripes.png",
        include_bytes!("../../sample/holtburg/art/stripes.png"),
    ),
    (
        "holtburg/records/stripes.toml",
        include_bytes!("../../sample/holtburg/records/stripes.toml"),
    ),
    (
        "holtburg/edits/landblocks/A8B4.toml",
        include_bytes!("../../sample/holtburg/edits/landblocks/A8B4.toml"),
    ),
    (
        "holtburg/edits/landblocks/A9B4.toml",
        include_bytes!("../../sample/holtburg/edits/landblocks/A9B4.toml"),
    ),
    (
        "holtburg/server/sql/weenies/guide.sql",
        include_bytes!("../../sample/holtburg/server/sql/weenies/guide.sql"),
    ),
    (
        "holtburg/server/sql/landblocks/A9B4 guide.sql",
        include_bytes!("../../sample/holtburg/server/sql/landblocks/A9B4 guide.sql"),
    ),
];

/// A new world's `world.toml`: the sample's pack, built into the world's own folder.
fn world_toml(key: &str) -> String {
    format!(
        "# A world made from Lyceum's sample: statics west of Holtburg, a hill, a new texture and an\n\
         # NPC wearing it.\nkey = \"{key}\"\npacks = [\"holtburg\"]\nout = \"build\"\n"
    )
}

/// Whether `name` may name a new world: lowercase letters, digits and hyphens.
pub fn good_name(name: &str) -> bool {
    !name.is_empty()
        && !name.starts_with('-')
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

/// Make a new world named `name` in `parent` from the sample: its folder, the sample's files and
/// its own `world.toml`. Answers its `world.toml`.
///
/// # Errors
/// What stops it: a name that will not do, a folder already there, or a file that will not write.
pub fn make_world(parent: &Path, name: &str) -> Result<PathBuf, Note> {
    if !good_name(name) {
        return Err(Note::BadName);
    }
    let dir = parent.join(name);
    if dir.exists() {
        return Err(Note::Taken(name.to_owned()));
    }
    let write = |rel: &str, bytes: &[u8]| -> Result<(), Note> {
        let path = dir.join(rel);
        if let Some(p) = path.parent() {
            std::fs::create_dir_all(p)
                .map_err(|e| Note::Failed(format!("{}: {e}", p.display())))?;
        }
        std::fs::write(&path, bytes).map_err(|e| Note::Failed(format!("{}: {e}", path.display())))
    };
    for (rel, bytes) in SAMPLE {
        write(rel, bytes)?;
    }
    write("world.toml", world_toml(name).as_bytes())?;
    Ok(dir.join("world.toml"))
}

/// The folders in `dir`, by name, sorted as a person reads them; hidden ones left out.
pub fn subfolders(dir: &Path) -> Vec<String> {
    let mut out: Vec<String> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .filter(|n| !n.starts_with('.'))
        .collect();
    out.sort_by_key(|n| n.to_lowercase());
    out
}

/// Where a folder picker may start over: each drive on Windows, the root elsewhere.
pub fn roots() -> Vec<PathBuf> {
    if cfg!(windows) {
        (b'A'..=b'Z')
            .map(|d| PathBuf::from(format!("{}:\\", char::from(d))))
            .filter(|p| p.is_dir())
            .collect()
    } else {
        vec![PathBuf::from("/")]
    }
}

/// The person's home folder, where a picker starts when nothing better is known.
fn home() -> PathBuf {
    std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map_or_else(|| PathBuf::from("/"), PathBuf::from)
}

/// What a folder picker is choosing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Purpose {
    /// A world to open: a folder with a `world.toml`.
    Open,
    /// The folder a new world goes in.
    New,
    /// The client folder the worlds are drawn over.
    Client,
}

/// A folder picker: what it chooses, the folder it shows and the folders in it, and a new world's
/// name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Picker {
    pub purpose: Purpose,
    pub dir: PathBuf,
    /// The folders in `dir`, read when the picker moves there rather than every frame.
    pub folders: Vec<String>,
    pub name: String,
}

impl Picker {
    /// A picker for `purpose` showing `dir`.
    fn new(purpose: Purpose, dir: PathBuf) -> Self {
        let name = if purpose == Purpose::New {
            "my-world".to_owned()
        } else {
            String::new()
        };
        let mut p = Self {
            purpose,
            dir: PathBuf::new(),
            folders: Vec::new(),
            name,
        };
        p.go(dir);
        p
    }

    /// Show `dir`.
    fn go(&mut self, dir: PathBuf) {
        self.folders = subfolders(&dir);
        self.dir = dir;
    }
}

/// A line the start screen says under its choices.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Note {
    NoWorldHere,
    BadName,
    Taken(String),
    NoClientHere,
    ChooseClient,
    Failed(String),
}

impl Note {
    /// The line, in the editor's text.
    pub fn say(&self, t: &Table) -> String {
        match self {
            Self::NoWorldHere => t.get("start.no_world_here").to_owned(),
            Self::BadName => t.get("start.name_rule").to_owned(),
            Self::Taken(n) => t.fill("start.taken", &[("name", n)]),
            Self::NoClientHere => t.get("start.no_client_here").to_owned(),
            Self::ChooseClient => t.get("start.choose_client").to_owned(),
            Self::Failed(why) => t.fill("start.failed", &[("why", why)]),
        }
    }
}

/// Something done on the start screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Open a recent world.
    Recent(PathBuf),
    /// Pick a world to open, a place for a new one, or the client folder.
    Pick(Purpose),
    /// The picker: up a folder, into one, to a drive or the root, its new world's name.
    Up,
    Into(String),
    Root(PathBuf),
    Name(String),
    /// The picker's folder is the one.
    Here,
    Cancel,
}

/// A world chosen, and the client folder it is drawn over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Choice {
    pub world: PathBuf,
    pub dats: PathBuf,
}

/// The start screen: the recent worlds, the client folder, an open picker, and what it says.
#[derive(Debug, Clone)]
pub struct Screen {
    pub recent: Recent,
    pub dats: Option<PathBuf>,
    pub picker: Option<Picker>,
    pub note: Option<Note>,
    /// A world chosen before there was a client folder, opened once there is one.
    waiting: Option<PathBuf>,
    /// The world chosen, for the editor to open.
    pub chosen: Option<Choice>,
    home: PathBuf,
    /// Where a picker may start over, looked for once: a drive that does not answer is not asked
    /// again every frame.
    roots: Vec<PathBuf>,
}

impl Screen {
    /// The start screen over `recent`, its client folder `dats` (the command line's), else the
    /// recent worlds' one, else the one the client would find, when either holds the client's
    /// files; its pickers start from `home` when nothing better is known.
    pub fn new(recent: Recent, dats: Option<PathBuf>, home: PathBuf) -> Self {
        let dats = dats
            .into_iter()
            .chain(recent.dats.clone())
            .chain(std::iter::once(
                dereth_client_runtime::config::default_dat_dir(),
            ))
            .find(|d| holds_client(d));
        Self {
            recent,
            dats,
            picker: None,
            note: None,
            waiting: None,
            chosen: None,
            home,
            roots: roots(),
        }
    }

    /// Where a picker for `purpose` starts: the client folder, or the folder the newest world is
    /// in, else home.
    fn start_dir(&self, purpose: Purpose) -> PathBuf {
        let known = match purpose {
            Purpose::Client => self.dats.clone(),
            Purpose::Open | Purpose::New => self
                .recent
                .present()
                .first()
                .and_then(|w| w.parent()?.parent().map(Path::to_path_buf)),
        };
        known
            .filter(|d| d.is_dir())
            .unwrap_or_else(|| self.home.clone())
    }

    /// `world` chosen: opened over the client folder, or held until there is one.
    fn choose(&mut self, world: PathBuf) {
        match &self.dats {
            Some(dats) => {
                self.chosen = Some(Choice {
                    world,
                    dats: dats.clone(),
                });
                self.picker = None;
                self.note = None;
            }
            None => {
                self.waiting = Some(world);
                self.picker = Some(Picker::new(
                    Purpose::Client,
                    self.start_dir(Purpose::Client),
                ));
                self.note = Some(Note::ChooseClient);
            }
        }
    }

    /// Do `action`.
    pub fn act(&mut self, action: Action) {
        match action {
            Action::Recent(world) => self.choose(world),
            Action::Pick(purpose) => {
                self.picker = Some(Picker::new(purpose, self.start_dir(purpose)));
                self.note = None;
            }
            Action::Cancel => {
                self.picker = None;
                self.waiting = None;
                self.note = None;
            }
            Action::Up => {
                if let Some(p) = &mut self.picker {
                    if let Some(up) = p.dir.parent().map(Path::to_path_buf) {
                        p.go(up);
                    }
                }
            }
            Action::Into(name) => {
                if let Some(p) = &mut self.picker {
                    let into = p.dir.join(name);
                    p.go(into);
                }
            }
            Action::Root(root) => {
                if let Some(p) = &mut self.picker {
                    p.go(root);
                }
            }
            Action::Name(name) => {
                if let Some(p) = &mut self.picker {
                    p.name = name;
                }
            }
            Action::Here => self.here(),
        }
    }

    /// The picker's folder chosen, for what the picker is choosing.
    fn here(&mut self) {
        let Some(p) = self.picker.clone() else {
            return;
        };
        match p.purpose {
            Purpose::Open => match world_in(&p.dir) {
                Some(world) => self.choose(world),
                None => self.note = Some(Note::NoWorldHere),
            },
            Purpose::New => match make_world(&p.dir, p.name.trim()) {
                Ok(world) => self.choose(world),
                Err(note) => self.note = Some(note),
            },
            Purpose::Client => {
                if holds_client(&p.dir) {
                    self.dats = Some(p.dir.clone());
                    self.picker = None;
                    self.note = None;
                    if let Some(world) = self.waiting.take() {
                        self.choose(world);
                    }
                } else {
                    self.note = Some(Note::NoClientHere);
                }
            }
        }
    }

    /// What the start screen shows, a line each, as `--list-recent` prints it.
    pub fn lines(&self, t: &Table) -> Vec<String> {
        let mut out = vec![t.get("start.recent").to_owned()];
        let worlds = self.recent.present();
        if worlds.is_empty() {
            out.push(format!("  {}", t.get("start.none_yet")));
        }
        out.extend(worlds.iter().map(|w| format!("  {}", w.display())));
        out.push(t.get("start.open").to_owned());
        out.push(t.get("start.new").to_owned());
        out.push(format!(
            "{}: {}",
            t.get("start.client"),
            self.dats.as_ref().map_or_else(
                || t.get("start.not_chosen").to_owned(),
                |d| d.display().to_string()
            )
        ));
        out
    }

    /// The screen, in `ui`, its words from `t`; what is done on it is done.
    pub fn ui(&mut self, ui: &mut egui::Ui, t: &Table) {
        let mut actions = Vec::new();
        egui::Frame::central_panel(ui.style()).show(ui, |ui| {
            ui.set_min_size(ui.available_size());
            ui.heading(t.get("start.title"));
            ui.add_space(10.0);
            match &self.picker {
                None => self.choices_ui(ui, t, &mut actions),
                Some(p) => picker_ui(p, &self.roots, ui, t, &mut actions),
            }
            if let Some(n) = &self.note {
                ui.add_space(10.0);
                ui.colored_label(egui::Color32::from_rgb(0xF0, 0xC0, 0x60), n.say(t));
            }
        });
        for a in actions {
            self.act(a);
        }
    }

    /// The recent worlds, the two ways to another, and the client folder.
    fn choices_ui(&self, ui: &mut egui::Ui, t: &Table, actions: &mut Vec<Action>) {
        ui.label(egui::RichText::new(t.get("start.recent")).strong());
        let worlds = self.recent.present();
        if worlds.is_empty() {
            ui.label(t.get("start.none_yet"));
        }
        for w in worlds {
            let name = w.parent().and_then(Path::file_name).map_or_else(
                || w.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            );
            if ui
                .button(name)
                .on_hover_text(w.display().to_string())
                .clicked()
            {
                actions.push(Action::Recent(w.clone()));
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            if ui.button(t.get("start.open")).clicked() {
                actions.push(Action::Pick(Purpose::Open));
            }
            if ui.button(t.get("start.new")).clicked() {
                actions.push(Action::Pick(Purpose::New));
            }
        });
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.label(format!("{}:", t.get("start.client")));
            ui.label(self.dats.as_ref().map_or_else(
                || t.get("start.not_chosen").to_owned(),
                |d| d.display().to_string(),
            ));
            if ui.button(t.get("start.change")).clicked() {
                actions.push(Action::Pick(Purpose::Client));
            }
        });
    }
}

/// A folder picker: its folder, up, the drives, the folders in it, and its choice.
fn picker_ui(
    p: &Picker,
    roots: &[PathBuf],
    ui: &mut egui::Ui,
    t: &Table,
    actions: &mut Vec<Action>,
) {
    let (title, choose) = match p.purpose {
        Purpose::Open => ("start.open_title", "start.open_here"),
        Purpose::New => ("start.new_title", "start.make_here"),
        Purpose::Client => ("start.client_title", "start.use_here"),
    };
    ui.label(egui::RichText::new(t.get(title)).strong());
    ui.monospace(p.dir.display().to_string());
    ui.horizontal(|ui| {
        if ui.button(t.get("start.up")).clicked() {
            actions.push(Action::Up);
        }
        for r in roots {
            if ui.button(r.display().to_string()).clicked() {
                actions.push(Action::Root(r.clone()));
            }
        }
    });
    egui::ScrollArea::vertical()
        .max_height((ui.available_height() - 90.0).max(80.0))
        .auto_shrink([false, true])
        .show(ui, |ui| {
            for name in &p.folders {
                if ui.selectable_label(false, name.as_str()).clicked() {
                    actions.push(Action::Into(name.clone()));
                }
            }
        });
    if p.purpose == Purpose::New {
        ui.horizontal(|ui| {
            ui.label(t.get("start.name"));
            let mut name = p.name.clone();
            if ui.text_edit_singleline(&mut name).changed() {
                actions.push(Action::Name(name));
            }
        });
    }
    ui.horizontal(|ui| {
        if ui.button(t.get(choose)).clicked() {
            actions.push(Action::Here);
        }
        if ui.button(t.get("start.cancel")).clicked() {
            actions.push(Action::Cancel);
        }
    });
}

/// The editor's text: the file beside its settings when there is one, else its own.
fn text() -> Text {
    match settings_dir().map(|d| d.join(lyceum_editor::text::FILE_NAME)) {
        Some(f) if f.is_file() => Text::open(&f),
        _ => Text::built_in(),
    }
}

/// `--list-recent`: what the start screen would show, over the client folder `dats`.
pub fn list(dats: Option<PathBuf>) -> String {
    let recent = settings_dir()
        .map(|d| Recent::load(&d.join(RECENT_FILE)))
        .unwrap_or_default();
    let screen = Screen::new(recent, dats, home());
    let mut out = screen.lines(&text().table()).join("\n");
    out.push('\n');
    out
}

/// What tells the editor it was started by the start screen, whose errors it then shows in a box
/// of the system's, since nothing else would show them.
pub const STARTED_BY_SCREEN: &str = "LYCEUM_STARTED_BY_SCREEN";

/// The editor's command line for `choice`.
pub fn editor_argv(choice: &Choice) -> Vec<String> {
    vec![
        choice.world.display().to_string(),
        "--dats".to_owned(),
        choice.dats.display().to_string(),
    ]
}

/// Start the editor on `choice`, a process of its own, and leave it running.
fn start_editor(choice: &Choice) -> Result<(), String> {
    let me = std::env::current_exe().map_err(|e| e.to_string())?;
    std::process::Command::new(me)
        .args(editor_argv(choice))
        .env(STARTED_BY_SCREEN, "1")
        .spawn()
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Whether a run's error is shown in a box of the system's: a run with no terminal to say it in
/// that is the start screen (`screen`) or one it started.
pub fn error_box_wanted(console: bool, screen: bool) -> bool {
    !console && (screen || std::env::var_os(STARTED_BY_SCREEN).is_some())
}

/// Show `message` in the system's error box (on Windows; elsewhere it is in the log).
pub fn error_box(message: &str) {
    /// The box's style: an OK button and the error icon.
    const OK_AND_ERROR: u32 = 0x10;
    dereth_console::error_box("Lyceum", message, OK_AND_ERROR);
}

/// The start screen, in a window of its own, over the client folder `dats`, until a world is
/// chosen (the editor is then started on it) or the window is closed.
///
/// # Errors
/// The window or the device, or a settings folder there is none of.
#[cfg(gpu)]
pub fn screen(dats: Option<PathBuf>) -> Result<(), String> {
    use dereth_input::host::HostEvent;

    let settings = settings_dir().ok_or("there is no settings folder for Lyceum")?;
    let _ = std::fs::create_dir_all(&settings);
    let prefs = dereth_desktop::folders::default_preferences_file(Lyceum::SETTINGS_DIR_NAME)
        .unwrap_or_default();
    let mut cfg = dereth_client_runtime::config::Config::from_args_and_prefs_at(
        &["--log-file".to_owned()],
        &prefs,
    )
    .map_err(|e| e.detail)?;
    cfg.width = 900;
    cfg.height = 620;
    dereth_desktop::logging::install::<Lyceum>(&cfg);
    let text = text();
    let recent_file = settings.join(RECENT_FILE);
    let mut screen = Screen::new(Recent::load(&recent_file), dats, home());
    let events: dereth_client_shell::platform::window::WindowEvents = Default::default();
    let mut platform =
        dereth_desktop::launch::open_platform::<Lyceum>(&cfg, std::rc::Rc::clone(&events))
            .map_err(|e| e.to_string())?;
    let (mut w, mut h) = platform.window.client_size();
    let mut present = device(platform.window.as_ref(), w, h, &cfg)?;
    let mut gui = lyceum_editor::gui::Gui::new();
    let began = std::time::Instant::now();
    loop {
        let pumped = platform.window.pump_events((w, h), false);
        // The window puts its events in the shared queue as it drains them.
        let all = std::mem::take(&mut *events.borrow_mut());
        if pumped.exited
            || all
                .iter()
                .any(|e| matches!(e, HostEvent::CloseRequested | HostEvent::Destroyed))
        {
            return Ok(());
        }
        for e in &all {
            if let HostEvent::Resized { width, height } = *e {
                if width > 0 && height > 0 && (width, height) != (w, h) {
                    (w, h) = (width, height);
                    present.resize(w, h).map_err(|e| e.to_string())?;
                }
            }
        }
        frame(
            &mut gui,
            &mut screen,
            &text.table(),
            &all,
            (w, h),
            began.elapsed().as_secs_f64(),
            present.as_mut(),
        )?;
        if let Some(choice) = screen.chosen.take() {
            match start_editor(&choice) {
                Ok(()) => {
                    let mut recent = Recent::load(&recent_file);
                    recent.remember(&choice.world, &choice.dats);
                    if let Err(e) = std::fs::write(&recent_file, recent.text()) {
                        tracing::warn!("{}: {e}", recent_file.display());
                    }
                    return Ok(());
                }
                Err(e) => screen.note = Some(Note::Failed(e)),
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(15));
    }
}

/// One frame of the start screen: `events` read, the screen drawn over nothing, and presented.
#[cfg(gpu)]
fn frame(
    gui: &mut lyceum_editor::gui::Gui,
    screen: &mut Screen,
    t: &Table,
    events: &[dereth_input::host::HostEvent],
    size: (u32, u32),
    time: f64,
    present: &mut dyn dereth_client_runtime::present::Presentation,
) -> Result<(), String> {
    let raw = gui.input(events, size, time);
    let ctx = gui.ctx.clone();
    let out = ctx.run_ui(raw, |ui| screen.ui(ui, t));
    let items = gui.paint(out, size);
    for (texture, data) in std::mem::take(&mut gui.uploads) {
        present
            .overlay_upload(texture, &data)
            .map_err(|e| e.to_string())?;
    }
    present.prepare_graphics_device();
    present.start_frame().map_err(|e| e.to_string())?;
    present.set_game_viewport(None);
    present.draw_scene(None).map_err(|e| e.to_string())?;
    present.draw_overlay(&items).map_err(|e| e.to_string())?;
    present.end_frame().map_err(|e| e.to_string())
}

/// The device the start screen draws on, in `window` of `w` by `h` pixels.
#[cfg(gpu)]
fn device(
    window: &dyn dereth_client_runtime::platform::window::WindowHost,
    w: u32,
    h: u32,
    cfg: &dereth_client_runtime::config::Config,
) -> Result<Box<dyn dereth_client_runtime::present::Presentation>, String> {
    use dereth_render::device::{Backend, DeviceConfig, Gpu};
    let fallback = Backend::default_backend().ok_or("this build has no graphics backend")?;
    let backend = cfg.renderer.map_or(fallback, Backend::from);
    let device_cfg = DeviceConfig {
        width: w,
        height: h,
        force_software: false,
        ..DeviceConfig::default()
    };
    let gpu = Gpu::new_on(backend, window.render_handles(), &device_cfg)
        .map_err(|e| format!("graphics engine: {e}"))?;
    let scene = dereth_scene::gpu::SceneRenderer::on_device(gpu)
        .map_err(|e| format!("graphics engine: {e}"))?;
    Ok(Box::new(dereth_client_shell::gpu::Renderer::around(scene)))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's start screen and its recent worlds)
    use super::*;

    fn scratch(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("lyceum-start-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&d);
        std::fs::create_dir_all(&d).unwrap();
        d
    }

    fn argv(s: &[&str]) -> Vec<String> {
        s.iter().map(|a| (*a).to_owned()).collect()
    }

    #[test]
    fn a_run_with_no_world_opens_the_start_screen_and_one_with_a_world_does_not() {
        assert_eq!(no_world(&[]), Some(NoWorld::Screen { dats: None }));
        assert_eq!(
            no_world(&argv(&["--dats", "C:/AC"])),
            Some(NoWorld::Screen {
                dats: Some(PathBuf::from("C:/AC"))
            })
        );
        assert_eq!(
            no_world(&argv(&["--list-recent"])),
            Some(NoWorld::List { dats: None })
        );
        assert_eq!(no_world(&argv(&["world.toml"])), None);
        assert_eq!(
            no_world(&argv(&["--headless"])),
            None,
            "a usage error, as before"
        );
        assert_eq!(no_world(&argv(&["--dats"])), None);
    }

    #[test]
    fn the_version_names_the_editor_and_its_version_first() {
        let text = version_text();
        assert_eq!(
            text.lines().next(),
            Some(format!("lyceum-editor {}", env!("CARGO_PKG_VERSION")).as_str())
        );
        assert!(text.lines().any(|l| l.starts_with("target: ")), "{text}");
    }

    #[test]
    fn a_world_opened_is_the_newest_recent_one_once_and_the_list_is_kept_short() {
        let mut r = Recent::default();
        for i in 0..12 {
            r.remember(
                &PathBuf::from(format!("/w/{i}/world.toml")),
                Path::new("/ac"),
            );
        }
        r.remember(Path::new("/w/5/world.toml"), Path::new("/ac2"));
        assert_eq!(r.worlds.len(), RECENT_KEPT);
        assert!(r.worlds[0].ends_with("5/world.toml"), "{:?}", r.worlds);
        assert_eq!(
            r.worlds
                .iter()
                .filter(|w| w.ends_with("5/world.toml"))
                .count(),
            1
        );
        assert!(r.dats.as_deref().is_some_and(|d| d.ends_with("ac2")));
        let back = Recent::read(&r.text());
        assert_eq!(back, r, "the file reads back as written: {}", r.text());
        assert_eq!(Recent::read("not [ toml"), Recent::default());
    }

    /// A folder that holds the client's files, as the editor recognises one.
    fn client_folder(dir: &Path) -> PathBuf {
        let c = dir.join("client");
        std::fs::create_dir_all(&c).unwrap();
        std::fs::write(dereth_dat::ModernDat::Portal.in_dir(&c), b"").unwrap();
        c
    }

    #[test]
    fn a_new_world_from_the_sample_is_made_where_chosen_and_opened() {
        let d = scratch("new");
        let client = client_folder(&d);
        let mut s = Screen::new(Recent::default(), Some(client.clone()), d.clone());
        s.act(Action::Pick(Purpose::New));
        assert_eq!(s.picker.as_ref().map(|p| p.dir.clone()), Some(d.clone()));
        s.act(Action::Name("Bad Name".to_owned()));
        s.act(Action::Here);
        assert_eq!(s.note, Some(Note::BadName));
        s.act(Action::Name("vale".to_owned()));
        s.act(Action::Here);
        let world = d.join("vale").join("world.toml");
        assert_eq!(
            s.chosen,
            Some(Choice {
                world: world.clone(),
                dats: client
            })
        );
        let toml = std::fs::read_to_string(&world).unwrap();
        assert!(toml.contains("key = \"vale\""), "{toml}");
        assert!(toml.contains("out = \"build\""), "{toml}");
        for (rel, bytes) in SAMPLE {
            assert_eq!(
                std::fs::read(d.join("vale").join(rel)).unwrap(),
                *bytes,
                "{rel}"
            );
        }
        // A second world of the same name is refused, and nothing in the first is written over.
        let mut again = Screen::new(Recent::default(), None, d.clone());
        again.act(Action::Pick(Purpose::New));
        again.act(Action::Name("vale".to_owned()));
        again.act(Action::Here);
        assert_eq!(again.note, Some(Note::Taken("vale".to_owned())));
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_sample_the_editor_carries_is_the_sample_world_in_the_tree() {
        let sample = Path::new(env!("CARGO_MANIFEST_DIR")).join("../sample");
        let mut on_disk = Vec::new();
        let mut todo = vec![sample.clone()];
        while let Some(dir) = todo.pop() {
            for e in std::fs::read_dir(&dir).unwrap() {
                let p = e.unwrap().path();
                if p.is_dir() {
                    todo.push(p);
                } else {
                    let rel = p.strip_prefix(&sample).unwrap();
                    on_disk.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
        on_disk.retain(|r| r != "world.toml");
        on_disk.sort();
        let mut carried: Vec<String> = SAMPLE.iter().map(|(r, _)| (*r).to_owned()).collect();
        carried.sort();
        assert_eq!(carried, on_disk, "the start screen's sample is the tree's");
        let sample_toml = std::fs::read_to_string(sample.join("world.toml")).unwrap();
        assert!(
            sample_toml.contains("packs = [\"holtburg\"]"),
            "{sample_toml}"
        );
    }

    #[test]
    fn a_world_opened_with_no_client_folder_waits_for_one() {
        let d = scratch("open");
        let world_dir = d.join("mine");
        std::fs::create_dir_all(&world_dir).unwrap();
        std::fs::write(world_dir.join("world.toml"), "key = \"mine\"\n").unwrap();
        let mut s = Screen::new(Recent::default(), None, d.clone());
        if s.dats.is_some() {
            // This machine's working folder holds the client's files: the case is covered by the
            // test below.
            return;
        }
        s.act(Action::Pick(Purpose::Open));
        assert_eq!(
            s.picker.as_ref().map(|p| p.folders.clone()),
            Some(vec!["mine".to_owned()]),
            "the folders of the folder the picker shows"
        );
        s.act(Action::Here);
        assert_eq!(
            s.note,
            Some(Note::NoWorldHere),
            "no world.toml in the home folder"
        );
        s.act(Action::Into("mine".to_owned()));
        s.act(Action::Here);
        assert_eq!(s.chosen, None);
        assert_eq!(s.note, Some(Note::ChooseClient));
        assert_eq!(s.picker.as_ref().map(|p| p.purpose), Some(Purpose::Client));
        s.act(Action::Here);
        assert_eq!(s.note, Some(Note::NoClientHere));
        let client = client_folder(&d);
        s.act(Action::Root(client.clone()));
        s.act(Action::Here);
        assert_eq!(
            s.chosen,
            Some(Choice {
                world: world_dir.join("world.toml"),
                dats: client
            })
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn a_recent_world_opens_over_the_remembered_client_folder() {
        let d = scratch("recent");
        let client = client_folder(&d);
        let world = d.join("w").join("world.toml");
        std::fs::create_dir_all(world.parent().unwrap()).unwrap();
        std::fs::write(&world, "").unwrap();
        let mut recent = Recent::default();
        recent.remember(&d.join("gone").join("world.toml"), &client);
        recent.remember(&world, &client);
        let mut s = Screen::new(recent, None, d.clone());
        assert_eq!(
            s.recent.present(),
            std::slice::from_ref(&world),
            "a world no longer there is not shown"
        );
        let t = Text::built_in();
        let lines = s.lines(&t.table());
        assert_eq!(lines[0], "Recent worlds");
        assert!(lines[1].ends_with("world.toml"));
        assert!(lines.contains(&"Open a world…".to_owned()), "{lines:?}");
        assert!(
            lines.contains(&"New world from the sample".to_owned()),
            "{lines:?}"
        );
        s.act(Action::Recent(world.clone()));
        assert_eq!(
            s.chosen,
            Some(Choice {
                world,
                dats: client.clone()
            })
        );
        assert_eq!(
            editor_argv(s.chosen.as_ref().unwrap())[1..],
            ["--dats".to_owned(), client.display().to_string()]
        );
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn the_start_screen_draws_in_one_pass_with_every_word_from_the_text() {
        let d = scratch("draw");
        let t = Text::built_in();
        let mut gui = lyceum_editor::gui::Gui::new();
        let mut s = Screen::new(Recent::default(), None, d.clone());
        for picker in [
            None,
            Some(Purpose::Open),
            Some(Purpose::New),
            Some(Purpose::Client),
        ] {
            if let Some(p) = picker {
                s.act(Action::Pick(p));
            }
            let raw = gui.input(&[], (900, 620), 0.0);
            let ctx = gui.ctx.clone();
            let out = ctx.run_ui(raw, |ui| s.ui(ui, &t.table()));
            let items = gui.paint(out, (900, 620));
            assert!(!items.is_empty(), "{picker:?}: something drawn");
        }
        assert!(!gui.uploads.is_empty(), "the font's texture goes up");
        let _ = std::fs::remove_dir_all(&d);
    }

    #[test]
    fn only_a_run_from_the_desktop_shows_its_error_in_a_box() {
        assert!(!error_box_wanted(true, true), "a terminal says it");
        assert!(
            error_box_wanted(false, true),
            "the start screen from the desktop"
        );
        if std::env::var_os(STARTED_BY_SCREEN).is_none() {
            assert!(
                !error_box_wanted(false, false),
                "a world's run started some other way"
            );
        }
    }
}
