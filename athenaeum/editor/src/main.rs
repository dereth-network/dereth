//! Athenaeum, the Asheron's Call 2 world builder: a world's rows drawn where they stand in the
//! world as the client draws it, selected, placed, moved, turned, changed and removed, saved into
//! the world's pack, and played on Yalain.
//!
//! **Depends on** the world machinery (`athenaeum-foundry`: the world over the world database, its
//! edits, undo and save), the world database (`yalain-world-db`, `yalain-content`: rows, and a
//! template described as the server describes it), the pack format (`dereth-pack`), the world as
//! the client draws it (`auberean-world`, `-assets`, `-dat`, `-animation`, `-physics`,
//! `-primitives`, `auberean-draw`, `auberean-gpu`), the free camera (`dereth-builder`), the panels
//! (`egui`), and the window system (`winit`) and device (`wgpu`). **Used by** nothing: it is the
//! `athenaeum-editor` program. Yalain and the clients it plays the world with are programs it
//! starts, never crates it uses.
//!
//! ```text
//! athenaeum-editor <world> --dats <install> [--base <folder>] [--pack <name>]
//!                  [--at XXYY[:X,Y]] [--size 1600x900] [--scale 1.25]
//!                  [--yalain <exe>] [--server-dats <install>] [--port 9300]
//!                  [--client <exe>] [--client-dats <install>] [--retail <client.exe>]
//!                  [--client-arg ARG]...
//!                  [--script FILE] [--shots DIR] [--verbose]
//! ```
//!
//! `<world>` is a world's folder (`athenaeum new` makes one). Its base is the release of the world
//! database it names, from the per-user cache, or `--base`; edits go into its last pack, or
//! `--pack`. The install is the one the world is drawn from and built against. Yalain and our
//! client are found beside this program unless given; the server reads `--server-dats` (by
//! default the same install: give a copy no client runs from when the retail client plays). The
//! retail client is run from its own folder; `--client-arg` passes more to whichever starts (our
//! client's `--dev-script`, say). Screenshots (F12, and a script's) go to `--shots`, by
//! default the world's `build/shots`. A script (`script.rs`) plays editor steps as frames pass.
//!
//! A world whose base is not in the per-user cache yet has it fetched first, by the Yalain the
//! editor plays with (`yalain world fetch`), as the server would need it too.
//!
//! `athenaeum-editor --version` prints the version, and the commit and target a release build was
//! made from, and `--help` every option ([`USAGE`]). A release build is a windowed program on
//! Windows, with no console of its own.
//!
//! **Must never** write a base database or an install: it writes only the world's pack, its
//! recipes, its build folder, its playtest folder and its screenshots.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::Arc;
use std::time::Instant;

use athenaeum_foundry::edit::NewKind;
use athenaeum_foundry::{build, world, Session};
use dereth_builder::camera::FlyCamera;
use dereth_primitives::Vec3;
use winit::application::ApplicationHandler;
use winit::event::{DeviceEvent, ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{KeyCode, PhysicalKey};
use winit::window::{Window, WindowId};

use athenaeum_editor::app::{self, Editor, ViewInput};
use athenaeum_editor::characters::Recipe;
use athenaeum_editor::script::{Script, Step};
use athenaeum_editor::{gui, icon, live, paint, scene, view};

#[derive(Debug, Default)]
struct Options {
    world: PathBuf,
    dats: PathBuf,
    base: Option<PathBuf>,
    pack: Option<String>,
    at: Option<(u16, Option<(f32, f32)>)>,
    size: (u32, u32),
    scale: f32,
    scale_chosen: bool,
    programs: live::Programs,
    script: Option<PathBuf>,
    shots: Option<PathBuf>,
    verbose: bool,
}

/// What `athenaeum-editor --help` prints: every option.
const USAGE: &str = "\
athenaeum-editor <world> --dats <install> [options]

  <world>               a world's folder, or its world.toml (`athenaeum new <folder>` makes one)
  --dats DIR            the Asheron's Call 2 install the world is drawn from and built against
  --base DIR            the world database the world is over (default: the release the world
                        names, from the per-user cache)
  --pack NAME           the pack edits go into (default: the world's last)
  --at XXYY[:X,Y]       start over landblock XXYY (hex), at X,Y in it (default 80,80)
  --size WxH            the window's size (default 1600x900)
  --scale S             the panels' scale (default: the screen's)
  --yalain EXE          the Yalain that plays the world (default: the one beside this program)
  --server-dats DIR     the install Yalain reads (default: --dats; give a copy no client runs
                        from when the retail client plays)
  --port P              where the look for Yalain's login port begins, moved on by a step of
                        this editor's own; the first free one is taken, and the world's is two
                        above (default 9300)
  --client EXE          Auberean, to play the world with (default: the one beside this program)
  --client-dats DIR     the install Auberean reads (default: --server-dats)
  --retail EXE          the retail client, run from its own folder, to play the world with
  --client-arg ARG      one more argument for whichever client starts (again for more)
  --script FILE         editor steps, played as frames pass
  --shots DIR           where screenshots go, F12's and a script's (default: the world's
                        build/shots)
  --verbose             log at the debug level
  --version             the version, and the commit and target a release build was made from
  --help                this help (also -h)
";

fn options() -> Result<Options, String> {
    options_from(std::env::args().skip(1))
}

/// The options these arguments give; `Err("")` for `--help`.
fn options_from(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut o = Options {
        size: (1600, 900),
        scale: 1.0,
        ..Options::default()
    };
    o.programs.port = 9300;
    let mut args = args.into_iter();
    while let Some(a) = args.next() {
        let mut value = || args.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--dats" => o.dats = PathBuf::from(value()?),
            "--base" => o.base = Some(PathBuf::from(value()?)),
            "--pack" => o.pack = Some(value()?),
            "--at" => {
                let v = value()?;
                let (b, xy) = v
                    .split_once(':')
                    .map_or((v.as_str(), None), |(b, r)| (b, Some(r)));
                let lb = u16::from_str_radix(b, 16).map_err(|e| format!("--at: {e}"))?;
                let xy = match xy {
                    Some(r) => {
                        let (x, y) = r.split_once(',').ok_or("--at: want XXYY:X,Y")?;
                        Some((
                            x.parse().map_err(|e| format!("--at: {e}"))?,
                            y.parse().map_err(|e| format!("--at: {e}"))?,
                        ))
                    }
                    None => None,
                };
                o.at = Some((lb, xy));
            }
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size: want WxH")?;
                o.size = (
                    w.parse().map_err(|e| format!("--size: {e}"))?,
                    h.parse().map_err(|e| format!("--size: {e}"))?,
                );
            }
            "--scale" => {
                o.scale = value()?.parse().map_err(|e| format!("--scale: {e}"))?;
                o.scale_chosen = true;
            }
            "--yalain" => o.programs.yalain = Some(PathBuf::from(value()?)),
            "--server-dats" => o.programs.server_dats = Some(PathBuf::from(value()?)),
            "--client" => o.programs.client = Some(PathBuf::from(value()?)),
            "--client-dats" => o.programs.client_dats = Some(PathBuf::from(value()?)),
            "--retail" => o.programs.retail = Some(PathBuf::from(value()?)),
            "--client-arg" => o.programs.client_args.push(value()?),
            "--port" => {
                o.programs.port = value()?.parse().map_err(|e| format!("--port: {e}"))?;
            }
            "--script" => o.script = Some(PathBuf::from(value()?)),
            "--shots" => o.shots = Some(PathBuf::from(value()?)),
            "--verbose" => o.verbose = true,
            "-h" | "--help" => return Err(String::new()),
            other if !other.starts_with("--") && o.world.as_os_str().is_empty() => {
                o.world = PathBuf::from(other);
            }
            other => return Err(format!("unknown option {other}")),
        }
    }
    if o.world.as_os_str().is_empty() {
        return Err("give the world's folder (athenaeum new <folder> makes one)".into());
    }
    if o.dats.as_os_str().is_empty() {
        return Err("--dats <install folder> is needed".into());
    }
    if o.programs.yalain.is_none() {
        o.programs.yalain = live::beside_me("yalain");
    }
    if o.programs.client.is_none() {
        o.programs.client = live::beside_me("auberean");
    }
    if o.programs.server_dats.is_none() {
        o.programs.server_dats = Some(o.dats.clone());
    }
    Ok(o)
}

/// The window, the device and the editor.
struct App<'w> {
    editor: Editor<'w>,
    world: &'w scene::WorldData,
    gui: gui::Gui,
    window: Option<Arc<Window>>,
    present: Option<paint::Presenter>,
    input: ViewInput,
    script: Option<Script>,
    shots: PathBuf,
    shot_next: Option<PathBuf>,
    shot_count: u32,
    start: Instant,
    last: f64,
    failed: Option<String>,
    quit: bool,
    /// The command line chose the panels' scale.
    scale_chosen: bool,
}

impl App<'_> {
    /// The panels at the screen's scale, unless the command line chose one.
    fn follow_scale(&mut self, scale: f64) {
        if !self.scale_chosen {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.gui.scale = scale as f32;
            }
        }
    }

    /// Run the steps a script has due.
    fn script(&mut self, now: f64) {
        let Some(s) = &mut self.script else {
            return;
        };
        for step in s.due(now) {
            let e = &mut self.editor;
            match step {
                Step::Wait(_) => {}
                Step::Camera(lb, at, yaw, pitch) => {
                    let g = view::global(lb, at);
                    e.camera.position = Vec3::new(g[0], g[1], g[2]);
                    e.camera.yaw = yaw.to_radians();
                    e.camera.pitch = pitch.to_radians();
                    e.enter_block(lb, at);
                }
                Step::Fly(item) => e.fly_to(&item),
                Step::Select(item) => e.select(item),
                Step::Place(kind, template, lb, x, y, h) => {
                    e.new_kind = match kind.as_str() {
                        "static" => NewKind::Static,
                        "camp" => NewKind::Camp,
                        "place" => NewKind::Place,
                        "clearing" => NewKind::Clearing,
                        _ => NewKind::Person,
                    };
                    e.template = template;
                    let g = e.standing_at(lb, x, y);
                    e.place_at(g, h);
                }
                Step::Move(item, lb, x, y) => {
                    let g = e.standing_at(lb, x, y);
                    e.move_row(&item, g);
                }
                Step::Turn(item, h) => {
                    if let Err(err) = e.session.turn(&item, h) {
                        e.status = err;
                    }
                }
                Step::Set(item, field, value) => {
                    if let Err(err) = e.session.set(&item, &field, &value) {
                        e.status = err;
                    }
                }
                Step::Delete(item) => {
                    e.select(Some(item));
                    e.delete_selected();
                }
                Step::PutBack(item) => {
                    if let Err(err) = e.session.put_back(&item) {
                        e.status = err;
                    }
                }
                Step::Undo => {
                    e.session.undo();
                }
                Step::Redo => {
                    e.session.redo();
                }
                Step::Save => e.save(),
                Step::Check => e.build_check(),
                Step::Find(text) => e.find(&text),
                Step::Serve => e.serve(),
                Step::Play(retail) => e.play(retail),
                Step::Stop => e.stop_playing(),
                Step::Restart => e.restart(),
                Step::Character(r) => {
                    if let Err(err) = e.keep_character(&r) {
                        e.status = err;
                    }
                }
                Step::Recipe(name, line) => {
                    let r = e
                        .recipes
                        .iter()
                        .find(|r| r.name == name)
                        .cloned()
                        .unwrap_or_else(|| Recipe::new(&name));
                    if let Err(err) = r.with_line(&line).and_then(|r| e.keep_character(&r)) {
                        e.status = err;
                    }
                }
                Step::As(name) => e.character = name,
                Step::Reset(name) => e.reset_character(&name),
                Step::Export(name) => e.export_character(&name),
                Step::Import(file, name) => e.import_character(&file, &name),
                Step::Teleport => e.teleport_to_camera(),
                Step::Characters => e.characters_open = !e.characters_open,
                Step::Point(x, y) => self.gui.point([x, y]),
                Step::Button(left, down) => {
                    if left {
                        self.gui.button(egui::PointerButton::Primary, down);
                        self.input.left_down = down;
                        self.input.left_pressed |= down;
                        self.input.left_released |= !down;
                    } else {
                        self.gui.button(egui::PointerButton::Secondary, down);
                        self.input.right_down = down;
                    }
                }
                Step::Wheel(n) => {
                    self.gui.wheel(n);
                    self.input.wheel += n;
                }
                Step::Key(name, ctrl, shift) => match gui::key_named(&name) {
                    Some(code) => {
                        self.gui.key(
                            code,
                            egui::Modifiers {
                                ctrl,
                                shift,
                                command: ctrl,
                                ..egui::Modifiers::default()
                            },
                        );
                        self.input.ctrl = ctrl;
                        self.input.shift = shift;
                        self.input.pressed.push(code);
                    }
                    None => e.status = format!("{name}: no such key"),
                },
                Step::Type(text) => self.gui.type_text(&text),
                Step::Shot(p) => self.shot_next = Some(p),
                Step::Close => e.ask_close(),
                Step::Quit => self.quit = true,
            }
            tracing::info!("script: {}", self.editor.status);
        }
    }

    fn redraw(&mut self, el: &ActiveEventLoop) {
        let began = std::time::Instant::now();
        let now = self.start.elapsed().as_secs_f64();
        #[allow(clippy::cast_possible_truncation)]
        let dt = ((now - self.last) as f32).clamp(0.0, 0.25);
        self.last = now;
        self.script(now);
        let Some(present) = &mut self.present else {
            return;
        };
        let size = present.size();
        self.editor.size = size;
        self.editor.scale = self.gui.scale;
        let raw = self.gui.input(size, now);
        let editor = &mut self.editor;
        let mut viewport = egui::Rect::NOTHING;
        let out = self.gui.ctx.run_ui(raw, |ui| {
            editor.panels(ui);
            egui::CentralPanel::no_frame().show(ui, |ui| {
                viewport = ui.max_rect();
                editor.markers(ui.painter());
            });
        });
        let p = self.gui.pointer_pixels();
        let inside = viewport.contains(egui::pos2(p[0] / self.gui.scale, p[1] / self.gui.scale));
        // A window floating over the world keeps its clicks.
        let pointer = egui::pos2(p[0] / self.gui.scale, p[1] / self.gui.scale);
        let over_window = self
            .gui
            .ctx
            .layer_id_at(pointer)
            .is_some_and(|l| l.order != egui::Order::Background);
        let over = !inside || over_window || self.gui.ctx.egui_is_using_pointer();
        let typing = self.gui.ctx.egui_wants_keyboard_input();
        self.input.pointer = self.gui.pointer_pixels();
        self.editor.world_input(&self.input, over, typing, dt);
        self.input.left_pressed = false;
        self.input.left_released = false;
        self.input.motion = (0.0, 0.0);
        self.input.wheel = 0.0;
        self.input.pressed.clear();
        let shot = self.shot_next.take();
        let world = self.world;
        let frame = self.editor.frame();
        let mut load = |id: u32| auberean_draw::picture::load(&world.records, id);
        present.present(Some(frame), out, &self.gui.ctx, &mut load, shot.as_deref());
        if present.hidden() {
            std::thread::sleep(hidden_rest(began.elapsed()));
        }
        self.editor
            .draw_picture(&self.gui.ctx, &mut present.renderer, &mut load);
        if let Some(w) = &self.window {
            let dirty = if self.editor.session.unsaved() {
                "*"
            } else {
                ""
            };
            w.set_title(&format!(
                "Athenaeum - {}{dirty}",
                self.editor
                    .session
                    .dir()
                    .file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().to_string())
            ));
        }
        if self.quit || self.editor.close_now {
            self.editor.keep_camera();
            self.editor.live.stop_all();
            el.exit();
        }
    }
}

impl ApplicationHandler for App<'_> {
    fn resumed(&mut self, el: &ActiveEventLoop) {
        if self.present.is_some() {
            return;
        }
        let (w, h) = self.editor.size;
        let attrs = icon::with_icon(Window::default_attributes())
            .with_title("Athenaeum")
            .with_inner_size(window_size_for((w, h)));
        let window = match el.create_window(attrs) {
            Ok(w) => Arc::new(w),
            Err(e) => {
                self.failed = Some(format!("no window: {e}"));
                el.exit();
                return;
            }
        };
        match paint::Presenter::new(Arc::clone(&window)) {
            Ok(p) => {
                tracing::info!(adapter = %p.renderer.adapter, "drawing");
                self.present = Some(p);
            }
            Err(e) => {
                self.failed = Some(e);
                el.exit();
                return;
            }
        }
        let size = window.inner_size();
        if let Some(p) = &mut self.present {
            p.resize(size.width, size.height);
        }
        self.follow_scale(window.scale_factor());
        self.window = Some(window);
    }

    fn window_event(&mut self, el: &ActiveEventLoop, _id: WindowId, event: WindowEvent) {
        self.gui.event(&event);
        match &event {
            WindowEvent::CloseRequested => self.editor.ask_close(),
            WindowEvent::ScaleFactorChanged { scale_factor, .. } => {
                self.follow_scale(*scale_factor);
            }
            WindowEvent::Resized(s) => {
                if let Some(p) = &mut self.present {
                    p.resize(s.width, s.height);
                }
            }
            WindowEvent::KeyboardInput { event, .. } => {
                if let PhysicalKey::Code(code) = event.physical_key {
                    if event.state == ElementState::Pressed {
                        if code == KeyCode::F12 {
                            self.shot_count += 1;
                            self.shot_next =
                                Some(self.shots.join(format!("shot-{:03}.png", self.shot_count)));
                        }
                        if !event.repeat {
                            self.input.pressed.push(code);
                        }
                        self.input.down.insert(code);
                    } else {
                        self.input.down.remove(&code);
                    }
                }
            }
            WindowEvent::ModifiersChanged(m) => {
                self.input.ctrl = gui::command_key(m.state());
                self.input.shift = m.state().shift_key();
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let down = *state == ElementState::Pressed;
                match button {
                    MouseButton::Left => {
                        self.input.left_down = down;
                        if down {
                            self.input.left_pressed = true;
                        } else {
                            self.input.left_released = true;
                        }
                    }
                    MouseButton::Right => self.input.right_down = down,
                    _ => {}
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                #[allow(clippy::cast_possible_truncation)]
                let notches = match delta {
                    MouseScrollDelta::LineDelta(_, y) => *y,
                    MouseScrollDelta::PixelDelta(p) => (p.y / 120.0) as f32,
                };
                self.input.wheel += notches;
            }
            WindowEvent::Focused(false) => {
                self.input.down.clear();
                self.input.left_down = false;
                self.input.right_down = false;
            }
            WindowEvent::RedrawRequested => self.redraw(el),
            _ => {}
        }
    }

    fn device_event(&mut self, _el: &ActiveEventLoop, _id: winit::event::DeviceId, e: DeviceEvent) {
        if let DeviceEvent::MouseMotion { delta } = e {
            #[allow(clippy::cast_possible_truncation)]
            {
                self.input.motion.0 += delta.0 as f32;
                self.input.motion.1 += delta.1 as f32;
            }
        }
    }

    fn about_to_wait(&mut self, _el: &ActiveEventLoop) {
        if let Some(w) = &self.window {
            w.request_redraw();
        }
    }
}

/// The window to ask for: on macOS a window goes by points, of which a Retina screen has two pixels
/// to one; elsewhere by the screen's pixels.
fn window_size_for(size: (u32, u32)) -> winit::dpi::Size {
    if cfg!(target_os = "macos") {
        winit::dpi::LogicalSize::new(size.0, size.1).into()
    } else {
        winit::dpi::PhysicalSize::new(size.0, size.1).into()
    }
}

/// The skills' names enumeration in the portal file.
const SKILL_NAMES: u32 = 0x2300_000F;

/// The skills' names, by number, for the playtest characters' recipes (none when the install's
/// enumeration does not read).
fn skill_names(files: &build::InstallFiles) -> Vec<(u32, String)> {
    files
        .portal
        .read(SKILL_NAMES)
        .ok()
        .and_then(|b| {
            auberean_assets::decode::<auberean_assets::strings::EnumMap>(
                SKILL_NAMES,
                &b,
                &files.ctx,
            )
            .ok()
        })
        .map(|m| m.names)
        .unwrap_or_default()
}

/// Where a world with nothing of its own yet opens: open ground on the edge of a town.
const START: (u16, f32, f32) = (0x5766, 123.0, 50.0);

/// Where the camera starts: `--at`, else where it was when the world was last closed, else over
/// the target pack's first row, else over the
/// outdoor landblock where the world places the most people and objects, looking north at them.
fn start_camera(o: &Options, session: &Session, scene: &mut scene::Scene<'_>) -> FlyCamera {
    let outdoors = |e: &athenaeum_foundry::Entry| match &e.row.row {
        yalain_world_db::rows::Row::Object(r) => r.cell.is_none_or(|c| c & 0xFFFF < 0x100),
        _ => false,
    };
    if o.at.is_none() {
        if let Some(c) = app::kept_camera(session.dir()) {
            return c;
        }
    }
    let (lb, x, y) = if let Some((lb, xy)) = o.at {
        let (x, y) = xy.unwrap_or((80.0, 80.0));
        (lb, x, y)
    } else if let Some(e) = session
        .entries()
        .iter()
        .find(|e| e.layer == athenaeum_foundry::Layer::Target && outdoors(e))
    {
        let at = e.at();
        (e.landblock(), at.x, at.y)
    } else {
        START
    };
    let g = view::global(lb, [x, y, 0.0]);
    let ground = scene.ground(g[0], g[1]).unwrap_or(0.0);
    FlyCamera::new(Vec3::new(g[0], g[1] - 30.0, ground + 16.0), 0.0, -0.42)
}

/// Fetch the release of the world database a world lies over into the per-user cache when it is
/// not there, with the Yalain the editor plays with, as `yalain world fetch` does.
fn fetch_base(world_dir: &Path, yalain: Option<&Path>) -> Result<(), String> {
    let Ok(text) = std::fs::read_to_string(world_dir.join("world.toml")) else {
        return Ok(());
    };
    let mut files = yalain_world_db::Files::new();
    files.insert("world.toml".into(), text);
    let Some(base) = yalain_world_db::rows::read_partly(&files)
        .0
        .and_then(|db| db.world.base)
    else {
        return Ok(());
    };
    let tag = base
        .version
        .unwrap_or_else(|| yalain_world_db::release::PINNED_TAG.to_string());
    if world::release_dir(&tag).is_some_and(|d| d.join("world.toml").is_file()) {
        return Ok(());
    }
    let Some(yalain) = yalain else {
        return Ok(());
    };
    tracing::info!(tag, "fetching the world database the world lies over");
    let mut fetch = std::process::Command::new(yalain);
    fetch
        .args(["world", "fetch", "--version", &tag])
        .stdin(std::process::Stdio::null());
    athenaeum_editor::stop::windowless(&mut fetch);
    let out = fetch
        .output()
        .map_err(|e| format!("{}: {e}", yalain.display()))?;
    if out.status.success() {
        Ok(())
    } else {
        Err(format!(
            "the world database {tag} could not be fetched: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ))
    }
}

fn run() -> Result<(), String> {
    let o = options()?;
    tracing_subscriber::fmt()
        .with_max_level(if o.verbose {
            tracing::Level::DEBUG
        } else {
            tracing::Level::INFO
        })
        .with_writer(std::io::stderr)
        .init();
    let world_dir = if o.world.file_name().is_some_and(|n| n == "world.toml") {
        o.world
            .parent()
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    } else {
        o.world.clone()
    };
    // Yalain runs in a folder of its own, so it is told the world's folder whole.
    let world_dir =
        std::path::absolute(&world_dir).map_err(|e| format!("{}: {e}", world_dir.display()))?;
    let base = o
        .base
        .as_deref()
        .map(std::path::absolute)
        .transpose()
        .map_err(|e| e.to_string())?;
    if base.is_none() {
        fetch_base(&world_dir, o.programs.yalain.as_deref())?;
    }
    let opened = world::open(&world_dir, base.as_deref())?;
    let base_dir = opened.base_dir.clone();
    tracing::info!(
        world = %world_dir.display(),
        base = ?base_dir,
        rows = opened.base.merged.rows.len(),
        "opened"
    );
    let session = Session::open(opened, o.pack.as_deref())?;
    let files = build::InstallFiles::open(&o.dats)?;
    let install = files.install()?;
    let data = scene::WorldData::open(&o.dats)?;
    let mut scene = scene::Scene::new(&data);
    let camera = start_camera(&o, &session, &mut scene);
    let mut editor = Editor::new(session, &install, scene, camera, o.programs.clone());
    editor.set_templates(
        files
            .portal
            .entries_in(0x4700_0000, 0x47FF_FFFF)
            .map(|e| e.id)
            .collect(),
    );
    editor.skill_names = skill_names(&files);
    editor.base = base_dir;
    editor.size = o.size;
    let shots = o
        .shots
        .clone()
        .unwrap_or_else(|| world_dir.join("build").join("shots"));
    let script = match &o.script {
        Some(f) => Some(Script::load(f, &shots)?),
        None => None,
    };
    let mut app = App {
        editor,
        world: &data,
        gui: gui::Gui::new(o.scale),
        window: None,
        present: None,
        input: ViewInput::default(),
        script,
        shots,
        shot_next: None,
        shot_count: 0,
        start: Instant::now(),
        last: 0.0,
        failed: None,
        quit: false,
        scale_chosen: o.scale_chosen,
    };
    let el = EventLoop::new().map_err(|e| e.to_string())?;
    el.set_control_flow(ControlFlow::Poll);
    el.run_app(&mut app).map_err(|e| e.to_string())?;
    app.editor.live.stop_all();
    if let Some(e) = app.failed {
        return Err(e);
    }
    Ok(())
}

fn main() -> ExitCode {
    // `--version` alone: what this build is, and nothing opened.
    if std::env::args().skip(1).eq(["--version"]) {
        print!("{}", version_text());
        return ExitCode::SUCCESS;
    }
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) if e.is_empty() => {
            print!("{USAGE}");
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("athenaeum-editor: {e}");
            ExitCode::FAILURE
        }
    }
}

/// What `athenaeum-editor --version` prints: the version, and the commit and target a release build
/// was made from (`unknown` in a build that did not say).
fn version_text() -> String {
    format!(
        "athenaeum-editor {}\ncommit: {}\ntarget: {}\n",
        env!("CARGO_PKG_VERSION"),
        option_env!("ATHENAEUM_BUILD_COMMIT").unwrap_or("unknown"),
        option_env!("ATHENAEUM_BUILD_TARGET").unwrap_or("unknown"),
    )
}

/// How long a frame that took `took` rests in a window that can't be seen. Nothing there waits for
/// the display, so without this it would draw as fast as it can.
fn hidden_rest(took: std::time::Duration) -> std::time::Duration {
    std::time::Duration::from_micros(16_667).saturating_sub(took)
}

#[cfg(test)]
mod hidden_tests {
    //! Behaviour: none (a window nobody can see draws no picture)
    use super::hidden_rest;
    use std::time::Duration;

    #[test]
    fn a_hidden_window_rests_out_a_sixtieth_of_a_second_and_a_slow_frame_not_at_all() {
        assert_eq!(hidden_rest(Duration::ZERO), Duration::from_micros(16_667));
        assert_eq!(
            hidden_rest(Duration::from_millis(10)),
            Duration::from_micros(6_667)
        );
        assert_eq!(hidden_rest(Duration::from_millis(40)), Duration::ZERO);
    }
}

#[cfg(test)]
mod version_tests {
    //! Behaviour: none (tooling: what `--version` prints, which the release workflow checks)
    use super::version_text;

    #[test]
    fn version_names_the_program_its_version_its_commit_and_its_target() {
        let text = version_text();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3, "{text}");
        assert_eq!(
            lines[0],
            concat!("athenaeum-editor ", env!("CARGO_PKG_VERSION"))
        );
        assert!(lines[1].starts_with("commit: "), "{text}");
        assert!(lines[2].starts_with("target: "), "{text}");
    }
}

#[cfg(test)]
mod help_tests {
    //! Behaviour: none (tooling: what `--help` prints, which a release carries in place of the source)
    use super::{options_from, USAGE};

    #[test]
    fn help_and_h_ask_for_the_help() {
        for a in ["--help", "-h"] {
            let asked = options_from([a.to_owned()]).err();
            assert_eq!(asked.as_deref(), Some(""), "{a}");
        }
        // A client's argument is the client's, even when it is `--help`.
        let passed = options_from(["--client-arg", "--help"].map(str::to_owned)).err();
        assert_ne!(passed.as_deref(), Some(""));
    }

    /// Each option the help lists is one the command line takes; `--version` and `--help` are
    /// answered before the options are read.
    #[test]
    fn every_option_the_help_lists_is_one_the_command_line_takes() {
        let listed: Vec<&str> = USAGE
            .lines()
            .filter_map(|l| l.split_whitespace().next())
            .filter(|w| w.starts_with("--") && !["--version", "--help"].contains(w))
            .collect();
        assert_eq!(listed.len(), 16, "{listed:?}");
        for o in listed {
            if let Err(e) = options_from([o.to_owned()]) {
                assert!(!e.starts_with("unknown option"), "{o}: {e}");
            }
        }
    }
}
