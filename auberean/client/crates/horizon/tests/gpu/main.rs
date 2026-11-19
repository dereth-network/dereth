//! Horizon on a graphics device: each of the views behind character select the settings offer,
//! drawn from the frame the shell builds for it; the map window over the world, its picture of
//! the whole world drawn from the landblocks; the log window, closed, typing and with its menus
//! open; a selected creature's marks; the focus bar, in both interfaces; and the HUD at peace
//! and attacking.
//!
//! Fixture: a hardware adapter (a software rasteriser is refused), the install under
//! `$AUBEREAN_TEST_DAT_DIR` and the server double of the `dat` tests. These tests run with
//! `--features retail-dats`. `AUBEREAN_VISTA_SHOT_DIR`, when set, names a folder the pictures
//! drawn are written to, to look at.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

#[allow(dead_code)]
#[path = "../dat/session.rs"]
mod session;

mod focus;
mod hud;

use auberean_horizon::ui::pregame::Screen;
use auberean_horizon::vista;

use session::Session;

const W: u32 = 640;
const H: u32 = 360;

/// A picture's texels, red, green, blue, alpha, row by row from the top.
type Texels = Vec<u8>;

/// The view behind character select in the next frame, drawn on the device.
fn drawn(s: &mut Session, renderer: &mut auberean_gpu::Renderer) -> Texels {
    let records = &session::data().0.records;
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let pic = s.frame_with(|out| {
        let frame = out.world.expect("the view's frame");
        renderer
            .render(frame, W, H, &mut load)
            .unwrap_or_else(|e| panic!("the view would not draw: {e}"))
    });
    assert_eq!((pic.width, pic.height), (W, H));
    pic.rgba
}

/// The mean brightness of rows `rows` of `pic`, 0 to 255.
fn brightness(pic: &Texels, rows: std::ops::Range<u32>) -> f64 {
    let (mut sum, mut n) = (0.0, 0.0);
    for y in rows {
        for x in 0..W {
            let i = usize::try_from((y * W + x) * 4).expect("a texel");
            sum += f64::from(pic[i]) * 0.3
                + f64::from(pic[i + 1]) * 0.59
                + f64::from(pic[i + 2]) * 0.11;
            n += 1.0;
        }
    }
    sum / n
}

fn keep(name: &str, pic: &Texels) {
    let Some(dir) = std::env::var_os("AUBEREAN_VISTA_SHOT_DIR").map(std::path::PathBuf::from)
    else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the shot folder");
    let path = dir.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), W, H);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(pic))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The view behind character select with `own` as Horizon's own file, drawn twice a quarter of a
/// turn apart, kept under `name`: a picture of many colors, the sky at its top brighter than the
/// land at its foot, and the second another picture.
fn draws_and_turns(name: &str, own: &str) {
    let mut s = Session::choosing(Some("[UI]\r\nInterface=Horizon\r\n"));
    let folder = session::Folder::default();
    folder.0.lock().expect("the folder").insert(
        auberean_horizon::own::FILE.to_owned(),
        own.as_bytes().to_vec(),
    );
    s.shell.host.folder = Some(folder);
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    let mut renderer =
        auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    eprintln!("adapter: {}", renderer.adapter);
    let middle = drawn(&mut s, &mut renderer);
    // A quarter of a turn on: the camera's heading at its right end.
    let quarter = s.shell.host.time + vista::PERIOD / 4.0;
    s.until(vista::PERIOD, |s| s.shell.host.time + 1.0 / 60.0 >= quarter);
    let right = drawn(&mut s, &mut renderer);
    keep(&format!("{name}-middle"), &middle);
    keep(&format!("{name}-right"), &right);
    let colors: std::collections::HashSet<&[u8]> = middle.chunks(4).collect();
    assert!(colors.len() > 2_000, "{name}: {} colors", colors.len());
    let (sky, land) = (
        brightness(&middle, 0..H / 10),
        brightness(&middle, H - H / 10..H),
    );
    assert!(
        sky > land,
        "{name}: the sky {sky:.0} over the land {land:.0}"
    );
    let changed = middle
        .chunks(4)
        .zip(right.chunks(4))
        .filter(|(a, b)| a != b)
        .count();
    #[allow(clippy::cast_precision_loss)]
    let share = changed as f64 / f64::from(W * H);
    assert!(share > 0.25, "{name}: {share:.2} of the picture changed");
}

/// Behind character select the device draws each view chosen in the settings: a picture of many
/// colors, the sky at its top brighter than the land at its foot, and a quarter of a turn later
/// another picture.
#[test]
fn every_view_behind_character_select_draws_and_turns() {
    for v in vista::Vista::ALL {
        draws_and_turns(
            &format!("vista-{}", v.word()),
            &format!("scale auto\nvista {}\n", v.word()),
        );
    }
}

/// Writes `rgba`, `w` by `h`, as `name` in the shot folder, when one is named.
fn keep_sized(name: &str, w: u32, h: u32, rgba: &[u8]) {
    let Some(dir) = std::env::var_os("AUBEREAN_VISTA_SHOT_DIR").map(std::path::PathBuf::from)
    else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the shot folder");
    let path = dir.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(rgba))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The colors of `rgba` (`w` texels a row) inside `r`.
fn colors_in(rgba: &[u8], w: u32, r: auberean_horizon::draw::Rect) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, x1, y1) = (r.x as u32, r.y as u32, r.right() as u32, r.bottom() as u32);
    let mut colors = std::collections::HashSet::new();
    for y in y0..y1 {
        for x in x0..x1.min(w) {
            let i = ((y * w + x) * 4) as usize;
            colors.insert([rgba[i], rgba[i + 1], rgba[i + 2]]);
        }
    }
    colors.len()
}

/// Character select over the world with three characters: the client's mark across the top and
/// its picture in each card's well, drawn in many colors; written as `charselect-logo-in-card` to
/// look at.
#[test]
fn character_select_draws_the_client_s_mark_and_its_picture_on_each_card() {
    let mut s = Session::choosing_among(
        Some("[UI]\r\nInterface=Horizon\r\n"),
        &[
            (session::PLAYER + 1, "Aerlinthe Rowan"),
            (session::PLAYER + 2, "Elm"),
        ],
    );
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    // A second on, past the screen's fade.
    let at = s.shell.host.time + 1.0;
    s.until(2.0, |s| s.shell.host.time >= at);
    let mut renderer =
        auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    eprintln!("adapter: {}", renderer.adapter);
    let (w, h, shot) = composed(&mut s, &mut renderer);
    keep_sized("charselect-logo-in-card", w, h, &shot);
    let shown = s.horizon().borrow().ui().pregame.shown.clone();
    assert_eq!(shown.wells.len(), 3, "a card each");
    let mark = shown.brand.expect("the mark drawn");
    assert!(colors_in(&shot, w, mark) > 1_000, "the mark in many colors");
    for well in shown.wells {
        assert!(colors_in(&shot, w, well) > 200, "a picture in {well:?}");
    }
}

/// Creation over the world with every race offered: its race buttons along the panel's top, each
/// name clear of its button's rim; written as `creation-races` to look at.
#[test]
fn creation_draws_a_button_for_every_race() {
    use auberean_world::chargen;
    let mut s = Session::choosing(Some("[UI]\r\nInterface=Horizon\r\n"));
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    let records = &session::data().0.records;
    s.horizon().borrow_mut().ui_mut().pregame.creation =
        Some(auberean_horizon::creation::Creation::new(
            records,
            chargen::EMPYREAN | chargen::DRUDGE,
            auberean_primitives::random::Random::new(),
        ));
    let at = s.shell.host.time + 1.0;
    s.until(2.0, |s| s.shell.host.time >= at);
    assert_eq!(s.horizon().borrow().screen(), Some(Screen::Creation));
    let mut renderer =
        auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
    eprintln!("adapter: {}", renderer.adapter);
    let (w, h, shot) = composed(&mut s, &mut renderer);
    keep_sized("creation-races", w, h, &shot);
    let buttons = s
        .horizon()
        .borrow()
        .ui()
        .pregame
        .creation_window
        .races
        .clone();
    assert_eq!(buttons.len(), 5, "a button each");
    for b in buttons {
        assert!(colors_in(&shot, w, b) > 50, "a button drawn over {b:?}");
    }
}

/// The shell's next frame composed on the device, the world under the interface, at the shell's
/// size: its texels, red, green, blue, alpha, row by row from the top.
fn composed(s: &mut Session, renderer: &mut auberean_gpu::Renderer) -> (u32, u32, Texels) {
    let records = &session::data().0.records;
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let (w, h) = s.shell.size;
    let mut composer = auberean_present::Composer::new(renderer);
    let picture = s.frame_with(|out| {
        let mut encoder = renderer
            .device()
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        let picture = composer.compose(
            renderer,
            (w, h),
            out.world,
            None,
            out.veil,
            out.overlay,
            &out.views,
            &mut load,
            &mut encoder,
        );
        renderer.queue().submit([encoder.finish()]);
        picture
    });
    let row = (w * 4).div_ceil(256) * 256;
    let buffer = renderer.device().create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: u64::from(row * h),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = renderer
        .device()
        .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            texture: &picture,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(row),
                rows_per_image: Some(h),
            },
        },
        wgpu::Extent3d {
            width: w,
            height: h,
            depth_or_array_layers: 1,
        },
    );
    renderer.queue().submit([encoder.finish()]);
    let slice = buffer.slice(..);
    slice.map_async(wgpu::MapMode::Read, |r| r.expect("the picture read back"));
    renderer
        .device()
        .poll(wgpu::PollType::wait_indefinitely())
        .expect("the device finishes");
    let data = slice.get_mapped_range().expect("mapped");
    let mut rgba = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        let at = (y * row) as usize;
        rgba.extend_from_slice(&data[at..at + (w * 4) as usize]);
    }
    let bgra = picture.format() == wgpu::TextureFormat::Bgra8Unorm
        || picture.format() == wgpu::TextureFormat::Bgra8UnormSrgb;
    if bgra {
        for px in rgba.chunks_mut(4) {
            px.swap(0, 2);
        }
    }
    (w, h, rgba)
}

/// The map window over the world, opened on the whole world: its picture drawn from every
/// landblock off the frame, the land in many colors amid the sea, written as `map-window` to look
/// at.
#[test]
fn the_map_window_shows_the_whole_world_drawn_from_its_landblocks() {
    use session::{doing, guarded};
    let (w, h, shot, body) = guarded(15, |note| {
        use auberean_horizon::keys::Keyed;
        doing(note, "entering the world");
        let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
        s.enter_the_world();
        let state = s.horizon().borrow().state().clone();
        s.horizon().borrow_mut().ui_mut().keyed(Keyed::Map, &state);
        // The window builds its picture off the frame: frames go on while it does.
        doing(note, "building the map");
        s.step();
        while s.horizon().borrow().ui().map.is_building() {
            s.step();
        }
        assert!(
            s.horizon().borrow().ui().map.is_ready(),
            "the map's picture was not built"
        );
        s.step();
        s.step();
        let body = s.horizon().borrow().ui().map.body().expect("the map open");
        assert!(
            (s.horizon().borrow().ui().map.view.zoom - 1.0).abs() < 1e-6,
            "the whole world"
        );
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(
            note,
            "composing the frame on the device and reading it back",
        );
        let (w, h, shot) = composed(&mut s, &mut renderer);
        (w, h, shot, body)
    });
    if let Some(dir) = std::env::var_os("AUBEREAN_VISTA_SHOT_DIR").map(std::path::PathBuf::from) {
        std::fs::create_dir_all(&dir).expect("the shot folder");
        let path = dir.join("map-window.png");
        let file =
            std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&shot))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    }
    // Inside the window's body: the land's many colors, and the open sea.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, x1, y1) = (
        body.x as u32,
        body.y as u32,
        (body.x + body.w) as u32,
        (body.y + body.h) as u32,
    );
    let [sr, sg, sb] = auberean_horizon::world_map::sea_color();
    let (mut colors, mut sea) = (std::collections::HashSet::new(), 0usize);
    for y in y0..y1.min(h) {
        for x in x0..x1.min(w) {
            let i = ((y * w + x) * 4) as usize;
            let px = [shot[i], shot[i + 1], shot[i + 2]];
            if px.iter().zip([sr, sg, sb]).all(|(a, b)| a.abs_diff(b) <= 2) {
                sea += 1;
            }
            colors.insert(px);
        }
    }
    let area = ((x1 - x0) * (y1 - y0)) as usize;
    assert!(colors.len() > 2_000, "{} colors", colors.len());
    assert!(sea * 4 > area, "{sea} of {area} texels the sea's");
}

/// How many texels of `pic` (`w` wide) in `r` differ from `other`'s.
fn changed_in(pic: &Texels, other: &Texels, w: u32, r: auberean_horizon::draw::Rect) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let (x0, y0, x1, y1) = (
        r.x.max(0.0) as u32,
        r.y.max(0.0) as u32,
        (r.x + r.w) as u32,
        (r.y + r.h) as u32,
    );
    let mut n = 0;
    for y in y0..y1 {
        for x in x0..x1.min(w) {
            let i = ((y * w + x) * 4) as usize;
            if pic.get(i..i + 3) != other.get(i..i + 3) {
                n += 1;
            }
        }
    }
    n
}

/// The log window over the world, drawn closed, typing, with its channel's menu open and with a
/// tab's menu open, written as `chat-closed`, `chat-typing`, `chat-mode-menu` and `chat-tab-menu`
/// to look at: each menu drawn where the log window said it is.
#[test]
fn the_log_window_draws_closed_typing_and_with_each_of_its_menus() {
    use auberean_horizon::state::Line;
    use auberean_shell::input::{Button, Event};
    use session::{doing, guarded};
    let shots = guarded(15, |note| {
        doing(note, "entering the world");
        let mut s = Session::new(Some("[UI]\r\nInterface=Horizon\r\n"));
        s.enter_the_world();
        let lines = [
            (0x10, "You say, \"Well met, all.\""),
            (0x20, "Ayla tells you, \"Meet me by the portal.\""),
            (0x8_0000, "[General] Borin says, \"Anyone selling ore?\""),
            (0x40, "Ayla waves."),
            (0x2, "You hit the rat for 12 points of damage."),
            (0x1, "You cannot do that now."),
        ];
        s.horizon().borrow_mut().ui_mut().chat.take(
            false,
            lines
                .iter()
                .map(|(kind, text)| Line {
                    kind: *kind,
                    text: (*text).to_owned(),
                })
                .collect::<Vec<_>>(),
        );
        s.step();
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(note, "drawing the log window closed");
        let closed = composed(&mut s, &mut renderer);
        doing(note, "typing");
        s.horizon().borrow_mut().ui_mut().chat.open("");
        s.step();
        s.shell.event(&Event::Text("heading out now".into()));
        s.step();
        let typing = composed(&mut s, &mut renderer);
        let (entry, label) = s
            .horizon()
            .borrow()
            .ui()
            .chat
            .entry_drawn()
            .expect("the entry open");
        doing(note, "opening the channel's menu");
        s.click((label.x + label.w / 2.0, label.y + label.h / 2.0));
        assert!(s.horizon().borrow().ui().chat.focus_menu_open());
        let mode_menu = composed(&mut s, &mut renderer);
        let mode_at = s.horizon().borrow().ui().chat.menu_rect().expect("drawn");
        doing(note, "opening a tab's menu");
        // A press elsewhere first closes the channel's menu, which may stand over the tabs.
        s.click((900.0, 300.0));
        let tab = s
            .horizon()
            .borrow()
            .ui()
            .chat
            .control("General")
            .expect("the tab");
        s.shell.event(&Event::Pointer {
            x: tab.x + tab.w / 2.0,
            y: tab.y + tab.h / 2.0,
        });
        s.step();
        for down in [true, false] {
            s.shell.event(&Event::Button {
                button: Button::Right,
                down,
            });
            s.step();
        }
        assert!(s.horizon().borrow().ui().chat.filters_open());
        let tab_menu = composed(&mut s, &mut renderer);
        let tab_at = s.horizon().borrow().ui().chat.menu_rect().expect("drawn");
        (closed, typing, mode_menu, tab_menu, entry, mode_at, tab_at)
    });
    let (closed, typing, mode_menu, tab_menu, entry, mode_at, tab_at) = shots;
    let (w, h, closed) = closed;
    for (name, (_, _, pic)) in [
        ("chat-typing", &typing),
        ("chat-mode-menu", &mode_menu),
        ("chat-tab-menu", &tab_menu),
    ] {
        keep_sized(name, w, h, pic);
    }
    keep_sized("chat-closed", w, h, &closed);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let area = |r: auberean_horizon::draw::Rect| (r.w * r.h) as usize;
    assert!(
        changed_in(&typing.2, &closed, w, entry) * 2 > area(entry),
        "the entry drawn where it was said to be"
    );
    assert!(
        changed_in(&mode_menu.2, &typing.2, w, mode_at) * 2 > area(mode_at),
        "the channel's menu drawn where it was said to be"
    );
    assert!(
        changed_in(&tab_menu.2, &closed, w, tab_at) * 2 > area(tab_at),
        "the tab's menu drawn where it was said to be"
    );
}

/// The texels of `pic` (`w` wide) in `r` (left, top, right, bottom; held to the picture) that
/// differ from `other`'s.
fn changed_in_box(pic: &Texels, other: &Texels, (w, h): (u32, u32), r: [f32; 4]) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let [x0, y0, x1, y1] = r.map(|v| v.max(0.0) as u32);
    let mut n = 0;
    for y in y0..y1.min(h) {
        for x in x0..x1.min(w) {
            let i = ((y * w + x) * 4) as usize;
            if pic[i..i + 3] != other[i..i + 3] {
                n += 1;
            }
        }
    }
    n
}

/// A creature selected in the world: the ring laid on the floor round its feet and the one arrow
/// pointing down over its name, both gone once nothing is selected. Written as `selected-creature`
/// (and `unselected-creature`) to look at.
#[test]
fn a_selected_creature_is_ringed_on_the_floor_with_one_arrow_over_its_name() {
    use auberean_horizon::ui::marker::Where;
    use session::{creature_ahead, doing, guarded};
    const DRUDGE: u64 = 0x2130_0000_0000_0010;
    let (size, with, without, b) = guarded(20, |note| {
        doing(note, "entering the world");
        let mut s = Session::going_on(
            Some("[UI]\r\nInterface=Horizon\r\n"),
            creature_ahead(DRUDGE, 4.0, 8.1, 7),
        );
        s.enter_the_world();
        doing(note, "waiting for the creature");
        let told = s.until(10.0, |s| {
            s.shell.client.as_ref().is_some_and(|c| {
                c.model
                    .object(DRUDGE)
                    .is_some_and(|o| o.qualities.ints.contains_key(&20))
            })
        });
        assert!(told, "the creature and its kind");
        s.shell
            .client
            .as_mut()
            .expect("the session")
            .act(auberean_client_sdk::Action::SetSelection { target: DRUDGE });
        doing(note, "selecting it");
        let marked = s.until(5.0, |s| {
            let h = s.horizon();
            let h = h.borrow();
            let st = h.state();
            st.ringed
                && st.target.as_ref().is_some_and(|t| t.id == DRUDGE)
                && matches!(st.selection_at, Some(Where::Box(_)))
        });
        assert!(marked, "selected, ringed and on the screen");
        s.until(1.0, |_| false);
        let Some(Where::Box(b)) = s.horizon().borrow().state().selection_at else {
            panic!("on the screen");
        };
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(note, "composing the frame with the selection");
        let (w, h, with) = composed(&mut s, &mut renderer);
        s.shell
            .client
            .as_mut()
            .expect("the session")
            .act(auberean_client_sdk::Action::SetSelection { target: 0 });
        s.until(1.0, |_| false);
        doing(note, "composing the frame with nothing selected");
        let (_, _, without) = composed(&mut s, &mut renderer);
        ((w, h), with, without, b)
    });
    let keep_as = |name: &str, pic: &Texels| {
        let Some(dir) = std::env::var_os("AUBEREAN_VISTA_SHOT_DIR").map(std::path::PathBuf::from)
        else {
            return;
        };
        std::fs::create_dir_all(&dir).expect("the shot folder");
        let path = dir.join(format!("{name}.png"));
        let file =
            std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), size.0, size.1);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(pic))
            .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    };
    keep_as("selected-creature", &with);
    keep_as("unselected-creature", &without);
    // Under its feet, round them and below the box its shape fills: the ring on the floor.
    let (cx, foot) = (b.x + b.w / 2.0, b.y + b.h);
    let ring = changed_in_box(
        &with,
        &without,
        size,
        [
            cx - b.w * 1.5,
            foot - b.h * 0.05,
            cx + b.w * 1.5,
            foot + b.h * 0.6,
        ],
    );
    // Over the box: the name and the arrow over it.
    let over = changed_in_box(
        &with,
        &without,
        size,
        [cx - b.w, b.y - 120.0, cx + b.w, b.y],
    );
    assert!(
        ring > 200,
        "the ring: {ring} texels changed about the feet ({b:?})"
    );
    assert!(
        over > 100,
        "the name and arrow: {over} texels changed over it ({b:?})"
    );
}
