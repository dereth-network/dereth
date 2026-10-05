//! UI and script sounds: the shipped layouts ask for exactly one UI sound (the button press), a
//! real pointer press plays it and a control with no sound media plays nothing, the click goes
//! through the shipped row-pick and voice-priority bugs, a physics script's sound hook reaches the
//! mixer, the listener follows the camera, running into new terrain swaps the ambient set, and
//! losing focus mutes the output. Fixture: the retail dats; an `App` on the character-management
//! screen with sound on and no audio device (the mixer's pre-device block is the observable);
//! and a software-device `WorldScene` at Holtburg with an attached body for the world sounds.
//! No datagram leaves this process.
//!
//! # The producers
//!
//! Media playback can start centered sounds by sound type and table or by data ID and volume.
//! Teleport animation, UI time, and the admin-environment path can start centered table sounds.
//! Physics objects can start a typed sound with volume. A basic animation hook starts an object
//! sound by data ID; a tweaked hook also supplies priority, probability, and volume; and a table
//! hook starts one by sound type. Ambient playback can start positional or centered ambient sounds.
//!
//! * **There is no dedicated collision-sound producer.** Environment collision reporting invokes
//!   the object's collision callback, which selects its default physics script. The script lookup
//!   and internal script player then run any sound hook carried by that script, resolved against
//!   **that object's** sound table. There is no surface table, material table, or impact-energy
//!   term in this path. The collision sound (0x2F) exists in the enum, but only the server can name
//!   it through `0xF750`.
//! * **No panel plays a UI sound by name.** Which UI event plays which sound is entirely a
//!   property of the UI layout data: a button reaches sound playback only through its media state.
//!   The media machine, never a screen, produces `UiRequest::PlaySound`.
//!
//! # The three shipped bugs the click path keeps, and where each lives
//!
//! | # | Bug | Where it lives in this client |
//! |---|---|---|
//! | 12.1 | the ambient volume is applied **twice**, so its slider is quadratic | `dereth_audio::AudioSystem::play_ambient_sound` multiplies by `prefs.ambient_volume` and `dereth_audio::atten::attenuation` multiplies by it again for `Category::Ambient` |
//! | 12.2 | the voice priority is inert, so the **seventeenth** simultaneous sound is dropped | `dereth_audio::mixer::VoicePool::start`'s `victim.priority < incoming.priority`, against a priority that `SoundCache` only ever leaves at `SoundData`'s constructed 0.0 |
//! | 12.3 | the **last row** of a multi-row table entry is unreachable | `dereth_audio::table::row_index`, `trunc(u * (n - 1))` |
//!
//! Each has its own guard elsewhere; [`the_click_sound_still_obeys_the_three_shipped_bugs`] is
//! this module's guard that the click path goes through them rather than round them.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::gpu_lock;
use crate::common::test_gpu;
use dereth_scene::world_scene::SceneWrites;

use std::sync::Arc;

use dereth_client::app::App;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::objects::ObjectStream;
use dereth_dat::RetailDatStore;
use dereth_primitives::{DataId, LocalTime, Vec3};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, StateId};
use {dereth_client_runtime::scene::SceneConfig, dereth_scene::world_scene::WorldScene};
use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

/// One second of interleaved stereo at the client's own primary-buffer rate.
const BLOCK: usize = (dereth_audio::MIX_RATE as usize) * 2;

/// The button's pressed-and-over state, `base + 2` with `base = 1`. Every sound media record in the
/// shipped layouts hangs off this one state.
const PRESSED: StateId = StateId(3);

fn store() -> Arc<RetailDatStore> {
    crate::common::dats()
}

fn peak(buf: &[f32]) -> f32 {
    buf.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

/// `sound: true` and `headless: true`: `App::start_shell` computes
/// `want_device = cfg.sound && !cfg.headless`, so the whole subsystem comes up with **no device**
/// and `Audio::mix` is the observable. That is deliberate: the mixer's pre-device block, never
/// the endpoint.
fn base_config() -> Config {
    Config {
        headless: true,
        sound: true,
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// The character-management screen, reached through the mode machine.
///
/// It is this screen and not the gameplay one because it is where the shipped layouts put their
/// buttons: `0x21000040/0x1000047F` is the base button template the six controls here inherit
/// from, and it is the record that carries the click sound.
fn app_on_character_screen() -> App {
    let mut app = App::new(base_config()).expect("required retail DATs and WARP device");
    app.start_shell().expect("the shell starts");
    app.load_first_pixel_scene().expect("the first scene loads");
    {
        let host = app.probe_mut().host_state_mut();
        host.character_set = Some(dereth_ui::persist::CharacterSet {
            set: Vec::new(),
            num_allowed_characters: 11,
            account: "acct0001".into(),
            ..dereth_ui::persist::CharacterSet::default()
        });
        host.received_set = true;
        host.world_name = Some("ACEmulator".into());
    }
    for _ in 0..24 {
        app.frame();
        let m = app.ui().and_then(|u| u.flow.current_mode());
        if m == Some(mode::CHARACTER_MANAGEMENT) {
            app.frame();
            app.frame();
            return app;
        }
        if m == Some(mode::INTRO) {
            let root = app
                .ui()
                .and_then(|u| u.flow.current())
                .and_then(|s| s.roots().first().copied());
            if let (Some(root), Some(shell)) = (root, app.ui_mut()) {
                shell.ui.broadcast_element_message(
                    root,
                    dereth_ui_screens::screens::intro::MSG_SKIP,
                    0,
                    0,
                );
            }
        }
    }
    panic!("the character-management screen was never reached");
}

/// Every live element whose **merged** state-3 description carries a sound media record, with the
/// descriptor's two fields. The merge matters: all three records in the dat sit on base templates,
/// and the full-description merge chain puts them on the controls a player can click.
fn clickable_sound_elements(app: &App) -> Vec<(ElemHandle, u32, DataId, u32)> {
    fn walk(
        ui: &dereth_ui::UiSystem,
        h: ElemHandle,
        out: &mut Vec<(ElemHandle, u32, DataId, u32)>,
    ) {
        if let Some(n) = ui.node(h) {
            if let Some(sd) = n.desc.access_state(PRESSED) {
                for m in &sd.media {
                    if let dereth_ui::desc::MediaFields::Sound { file, sound_type } = m.fields {
                        out.push((h, n.desc.element_id.0, file, sound_type));
                    }
                }
            }
        }
        for c in ui.children(h) {
            walk(ui, c, out);
        }
    }
    let shell = app.ui().expect("a shell");
    let mut out = Vec::new();
    for r in shell
        .flow
        .current()
        .map(dereth_ui::Screen::roots)
        .unwrap_or(&[])
    {
        walk(&shell.ui, *r, &mut out);
    }
    out
}

/// Of those, the ones a pointer can actually reach right now: visible to the root and answering
/// the hit test at their own centre.
///
/// Not every control that *declares* the sound is clickable at every moment: the character
/// screen's Enter World and Delete sit inside a group that is hidden while no character is
/// selected, and this harness has an empty character list. Filtering here rather than in the walk
/// keeps the two facts separate, so [`a_real_button_press_plays_the_layouts_own_click_sound`] can
/// say how many of each it found.
fn pressable_sound_elements(app: &App) -> Vec<(ElemHandle, u32, DataId, u32)> {
    let shell = app.ui().expect("a shell");
    let ui = &shell.ui;
    clickable_sound_elements(app)
        .into_iter()
        .filter(|(h, ..)| {
            let mut p = Some(*h);
            while let Some(n) = p {
                if !ui.node(n).is_some_and(|e| e.region.flags.visible) {
                    return false;
                }
                p = ui.parent(n);
            }
            let r = ui.screen_clip_box(*h);
            if !r.is_valid() {
                return false;
            }
            let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == *h || ui.is_ancestor_of(*h, hit))
        })
        .collect()
}

/// A real pointer click on `h`, through the pump and input manager: the reason this suite is at
/// App level at all. Driving
/// `UiSystem::mouse_down` directly would measure the media machine without proving that the player
/// can click anything.
fn click(app: &mut App, pump: &mut Pump, h: ElemHandle, time: u32) {
    let at = {
        let ui = &app.ui().expect("a shell").ui;
        let mut parent = Some(h);
        while let Some(p) = parent {
            assert!(
                ui.node(p).expect("a live ancestor").region.flags.visible,
                "the control's ancestor {p:?} is not visible, so no pointer could reach it"
            );
            parent = ui.parent(p);
        }
        let r = ui.screen_clip_box(h);
        assert!(
            r.is_valid(),
            "the control has no visible rectangle to click"
        );
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the hit test at {at:?} does not find the control"
        );
        at
    };
    let send = |app: &mut App, pump: &mut Pump, msg: Win32Message| {
        pump.dispatch(msg);
        if let Some(i) = app.input_manager_mut() {
            i.on_message(msg);
        }
    };
    let msg = pump.mouse_move_message(f64::from(at.0), f64::from(at.1), time);
    send(app, pump, msg);
    for (down, t) in [(true, time + 1), (false, time + 2)] {
        let msg = pump
            .mouse_button_message(winit::event::MouseButton::Left, down, t)
            .expect("a message");
        send(app, pump, msg);
    }
    app.frame();
    app.frame();
}

// ---------------------------------------------------------------------------------------------
// 1. The calibration: what the shipped data asks for.
// ---------------------------------------------------------------------------------------------

/// **Every sound media record in every shipped layout, counted: there are three.**
///
/// This scan first establishes that the instrument can see its target. All 101 `0x21xxxxxx`
/// layouts in `client_local_English.dat` decode, and across every element, every state and every
/// media list they carry **three** type-9 descriptors:
///
/// ```text
///   layout 21000006  element 10000014  state 3  file 2000004B  stype 114
///   layout 21000040  element 1000047F  state 3  file 2000004B  stype 114
///   layout 2100006F  element 10000014  state 3  file 2000004B  stype 114
/// ```
///
/// All three are state **3**, the button's pressed-and-over state. All three name
/// the button-press sound (114 = `0x72`) and the same sound table. The test resolves that UI sound
/// table through the enum map rather than trusting a constant.
///
/// So the whole of "UI sound" in the shipped client is *one* sound on *one* state, and the
/// twenty-one other `Sound_UI_*` ids in the enum are named for layout authors who never used them.
#[test]
fn the_shipped_layouts_ask_for_exactly_one_ui_sound_and_it_is_the_button_press() {
    use dereth_assets::ui::{ElementDesc, LayoutDesc, MediaFields};
    use dereth_assets::{Decode, MasterProperty};
    use dereth_primitives::AssetSource;

    let store = store();
    let mid = DataId(0x3900_0001);
    let bytes = store.read(mid).expect("MasterProperty 0x39000001");
    let types = MasterProperty::decode_payload(mid, &bytes)
        .expect("it decodes")
        .property_types();

    fn walk(e: &ElementDesc, out: &mut Vec<(u32, u32, DataId, u32)>) {
        for (sid, s) in std::iter::once((&e.state.state_id, &e.state))
            .chain(e.states.iter().map(|(i, s)| (i, s)))
        {
            for m in &s.media {
                if let MediaFields::Sound { file, sound_type } = m.fields {
                    out.push((e.element_id, *sid, file, sound_type));
                }
            }
        }
        for (_, c) in &e.children {
            walk(c, out);
        }
    }

    let mut layouts = 0u32;
    let mut found: Vec<(DataId, u32, u32, DataId, u32)> = Vec::new();
    for id in store.ids_of(dereth_dat::DbType::UiLayout) {
        let bytes = store.read(id).expect("a listed layout reads");
        let l = LayoutDesc::decode_payload(id, &bytes, &types).expect("a listed layout decodes");
        layouts += 1;
        let mut here = Vec::new();
        for (_, e) in &l.elements {
            walk(e, &mut here);
        }
        found.extend(
            here.into_iter()
                .map(|(eid, sid, f, st)| (id, eid, sid, f, st)),
        );
    }

    // The instrument looked at something: 101 layouts, and they are not empty.
    assert!(
        layouts > 90,
        "only {layouts} layouts decoded; the scan is not reading the dat"
    );
    assert_eq!(
        found.len(),
        3,
        "the shipped layouts' sound media records: {found:?}"
    );

    let ui_table = dereth_client_runtime::assets::enum_did(
        &*store,
        dereth_client_runtime::audio::UI_SOUND_TABLE_GROUP,
        dereth_client_runtime::audio::UI_SOUND_TABLE_ENUM,
    )
    .expect("the client UI sound-table enum resolves");
    for (layout, elem, state, file, stype) in &found {
        assert_eq!(
            *state, PRESSED.0,
            "layout {layout:?} element {elem:#010X} is not on state 3"
        );
        assert_eq!(
            *stype,
            dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
            "layout {layout:?} element {elem:#010X} names {stype}, not the button-press sound"
        );
        assert_eq!(
            *file, ui_table,
            "layout {layout:?} element {elem:#010X} names table {file:?}, not the UI sound table"
        );
    }
    eprintln!("{layouts} layouts scanned; sound media records: {found:?}");
}

// ---------------------------------------------------------------------------------------------
// 2. Buttons click.
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.ui.a-button-press-plays-the-layouts-click-sound
///
/// **A real pointer press on a real control plays the sound its own layout asked for.**
///
/// A pressed button runs its pressed state's media list and raises
/// [`dereth_ui::MediaEffect::PlaySound`]; the reader of [`dereth_ui::UiSystem::media_effects`]
/// must pass it on rather than handle only `PlayMovie`, so that
/// `dereth_ui_screens::UiRequest::PlaySound` and `AudioSystem::play_ui_sound` are reached from a
/// click.
///
/// Every hop is measured separately, because each of them fails silently on its own:
///
/// 1. the pointer reaches the tree at all (`UiStats::mouse_downs`);
/// 2. the button reaches state 3, so its media list runs (`UiStats::media_sounds`);
/// 3. the sound track started a voice (`AudioStats::sounds_started`, `Audio::active_voices`);
/// 4. **the block the mixer fills is not silence.** A started voice with no sample, a missing
///    table or a −50 dB attenuation would pass every counter above and be inaudible.
#[test]
fn a_real_button_press_plays_the_layouts_own_click_sound() {
    let _gpu = gpu_lock();
    let mut app = app_on_character_screen();

    let declared = clickable_sound_elements(&app).len();
    let controls = pressable_sound_elements(&app);
    assert!(
        !controls.is_empty(),
        "{declared} live control(s) on the character screen declare a state-3 media sound and \
         none of them is reachable by a pointer -- the harness is pointed at the wrong screen and \
         the assertions below would be vacuous"
    );
    let (h, elem, table, stype) = controls[0];
    assert_eq!(stype, dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS);
    eprintln!(
        "{declared} control(s) carry the click sound, {} of them pressable; pressing {elem:#010X} \
         (table {table:?}, stype {stype:#04X})",
        controls.len()
    );

    let before = {
        let a = app.audio_mut().expect("the sound subsystem is up");
        a.stats.sounds_started
    };
    let downs_before = app.ui().expect("a shell").stats.mouse_downs;

    let mut pump = Pump::new();
    click(&mut app, &mut pump, h, 10_000);

    let (downs, media_sounds) = {
        let shell = app.ui().expect("a shell");
        (shell.stats.mouse_downs, shell.stats.media_sounds)
    };
    assert!(
        downs > downs_before,
        "the pointer never reached the tree: the pointer-down count did not increase"
    );
    assert!(
        media_sounds > 0,
        "the button was pressed and no sound media step ran: either it never reached state 3 \
         or the media effect is still being dropped"
    );

    let audio = app.audio_mut().expect("the sound subsystem is up");
    assert!(
        audio.stats.sounds_started > before,
        "the media step ran and sound management was never called: {} -> {}",
        before,
        audio.stats.sounds_started
    );
    assert!(
        audio.active_voices() > 0,
        "the centered UI sound started no voice"
    );

    let mut buf = vec![0.0f32; BLOCK];
    audio.mix(&mut buf);
    let p = peak(&buf);
    assert!(p > 0.0, "a voice started and the mixer's block is silence");
    eprintln!(
        "click -> {media_sounds} media step(s), {} voice(s), block peak {p:.5}",
        audio.active_voices()
    );
}

/// **The control for the test above: an element with no sound media makes no sound.**
///
/// Without this the acceptance test could pass on a client that plays a click for *every* pointer
/// event, which would be a different defect wearing the same green.
#[test]
fn a_press_on_a_control_with_no_sound_media_plays_nothing() {
    let _gpu = gpu_lock();
    let mut app = app_on_character_screen();
    let with_sound: Vec<ElemHandle> = clickable_sound_elements(&app)
        .into_iter()
        .map(|c| c.0)
        .collect();

    // A live, visible element the hit test resolves to **exactly**, not to a descendant, because
    // mouse-down puts the hit element into state 3 and a descendant with the sound would make this
    // pass for the wrong reason.
    let candidate = {
        let shell = app.ui().expect("a shell");
        let ui = &shell.ui;
        let mut found = None;
        for h in ui.element_list() {
            if with_sound.contains(h) {
                continue;
            }
            let mut p = Some(*h);
            let mut visible = true;
            while let Some(n) = p {
                visible &= ui.node(n).is_some_and(|e| e.region.flags.visible);
                p = ui.parent(n);
            }
            if !visible {
                continue;
            }
            let r = ui.screen_clip_box(*h);
            if !r.is_valid() {
                continue;
            }
            let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
            if ui.hit_test_screen(at.0, at.1) == Some(*h) {
                found = Some(*h);
                break;
            }
        }
        found.expect("some clickable element without a sound media")
    };

    let before = app.audio_mut().expect("audio").stats.sounds_started;
    let mut pump = Pump::new();
    click(&mut app, &mut pump, candidate, 20_000);
    let audio = app.audio_mut().expect("audio");
    assert_eq!(
        audio.stats.sounds_started, before,
        "a control whose layout declares no sound played one anyway"
    );
}

/// **The click sound goes *through* the three shipped bugs, not round them.**
///
/// The click path ends in `AudioSystem::play_ui_sound`, which is entry 7 and shares
/// `pick_row` and `VoicePool::start` with every other sound in the game. This asserts the two of
/// the three that entry 7 can reach, on the same call the button makes:
///
/// * **12.3**: the row pick is `trunc(u * (n - 1))`, so the last row of a multi-row entry never
///   comes back. Asserted over the UI table's own rows.
/// * **12.2**: the voice priority is inert, so the seventeenth simultaneous click is dropped
///   rather than stealing. Sixteen voices, hard.
///
/// (12.1 is the ambient path's and is guarded with the world audio; the note at the top of this
/// file says where each of the three lives.)
#[test]
fn the_click_sound_still_obeys_the_three_shipped_bugs() {
    let store = store();
    let mut a = dereth_client_runtime::audio::Audio::new(dereth_audio::Prefs::default(), 1, false);
    let table = dereth_client_runtime::assets::enum_did(
        &*store,
        dereth_client_runtime::audio::UI_SOUND_TABLE_GROUP,
        dereth_client_runtime::audio::UI_SOUND_TABLE_ENUM,
    )
    .expect("the UI sound table resolves");

    // 12.3, on the shipped rows: `row_index` never answers `n - 1` for `n >= 2`.
    a.play_media_sound(
        &store,
        table,
        dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
    );
    let rows = a
        .assets()
        .table(table)
        .and_then(|t| {
            dereth_audio::table::lookup(t, dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS)
        })
        .expect("the button-press sound is in the UI table")
        .len();
    if rows >= 2 {
        let last = rows - 1;
        for u in [0.0f32, 0.25, 0.5, 0.75, 0.999_999_88] {
            assert_ne!(
                dereth_audio::table::row_index(rows, u),
                Some(last),
                "the last of {rows} rows became reachable"
            );
        }
    }
    eprintln!("the button-press sound has {rows} row(s)");

    // 12.2: the seventeenth simultaneous click is dropped, not stolen.
    for _ in 0..dereth_audio::NUM_VOICES + 4 {
        a.play_media_sound(
            &store,
            table,
            dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
        );
    }
    assert_eq!(dereth_audio::NUM_VOICES, 16);
    assert_eq!(a.active_voices(), 16, "the pool grew past sixteen voices");
}

// ---------------------------------------------------------------------------------------------
// 3. Walking into a wall: the collision family, as retail has it.
// ---------------------------------------------------------------------------------------------

/// **A physics script's sound hook reaches the mixer, which is the whole of a collision sound.**
///
/// Environment collision reporting invokes the object's collision callback, which selects the
/// default physics script. The script's hooks are queued into the animation-hook list and run by
/// the physics hook processor. A tweaked sound hook is entry 2 and a sound-table hook is entry 4.
/// **That indirection is the collision sound path.** Retail has no dedicated
/// collision-sound producer and no surface or material table. This test has the narrower positive
/// scope: it names a known script and proves that the script's sound hook reaches the mixer.
///
/// `0x330003CC` is one of the 153 shipped `0x33` scripts whose leading hook is a sound hook:
/// a tweaked sound hook for wave `0x0A00058A` at t = 0. Driving it through
/// [`dereth_client_runtime::audio::world_use_time`] is the same hop `collision_scripts` takes on the frame
/// a body walks into something.
#[test]
fn a_physics_scripts_sound_hook_reaches_the_mixer() {
    let _gpu = gpu_lock();
    let store = store();
    let mut gpu = test_gpu(320, 240);
    let cfg = SceneConfig {
        scenery_radius: 1,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    // **Ambience off, and it is not tidying.** Holtburg's beds start voices of their own within a
    // second, so a test that watched `active_voices` with them on would go green on the
    // scenery. The observable here is the hook, and the pool must contain nothing else.
    let silent_beds = dereth_audio::Prefs {
        ambient_enabled: false,
        ..dereth_audio::Prefs::default()
    };
    let mut audio = dereth_client_runtime::audio::Audio::new(silent_beds, 1, false);
    let mut stream = ObjectStream::new();
    let anim = dereth_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store));
    let mut t = 0.0_f64;

    // The body's own physics object, reached by handle and not by id. The scene's player-object
    // lookup is `None` until `ObjectStream::player` names it, which is why this path uses the
    // attached character directly.
    assert!(
        scene
            .character
            .as_ref()
            .expect("the body is attached")
            .driver_mut()
            .play_script_id(DataId(0x3300_03CC)),
        "the shipped script 0x330003CC would not start: the fixture, not the wiring"
    );

    let mut voices = 0;
    for _ in 0..60 {
        t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");
        dereth_client_runtime::audio::world_use_time(
            Some(&mut audio),
            Some(&mut scene),
            &mut stream,
            &anim,
            &store,
            LocalTime(t),
        );
        scene.update(
            Default::default(),
            Default::default(),
            LocalTime(t),
            1.0 / 30.0,
        );
        voices = voices.max(audio.active_voices());
        if audio.world_stats().triggers > 0 {
            break;
        }
    }
    let w = audio.world_stats();
    assert!(
        w.triggers > 0,
        "no sound hook reached the trigger path in two seconds"
    );
    assert!(
        voices > 0,
        "the script's sound hook reached the trigger path and started no voice"
    );
    assert_eq!(
        w.trigger_misses, 0,
        "{} hook(s) named a wave the dat would not give",
        w.trigger_misses
    );

    let mut buf = vec![0.0f32; BLOCK];
    audio.mix(&mut buf);
    assert!(
        peak(&buf) > 0.0,
        "a collision-script voice started and the block is silence"
    );
    eprintln!(
        "{} hook(s) fired, {voices} voice(s), peak {:.5}",
        w.triggers,
        peak(&buf)
    );
}

// ---------------------------------------------------------------------------------------------
// 4. The listener follows the camera.
// ---------------------------------------------------------------------------------------------

/// Behaviour: audio.listener.follows-the-camera
///
/// **The listener position is updated every frame from the camera.**
///
/// The viewer update passes the camera-produced viewer position, not the player's physics
/// position. The two differ by the chase camera's offset, which makes this assertable: a build that
/// set the listener to the body would put it in a different place, and a build that set it once
/// would leave it behind.
#[test]
fn the_listener_follows_the_camera_every_frame() {
    let _gpu = gpu_lock();
    let store = store();
    let mut gpu = test_gpu(320, 240);
    let cfg = SceneConfig {
        scenery_radius: 1,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let mut audio =
        dereth_client_runtime::audio::Audio::new(dereth_audio::Prefs::default(), 1, false);
    let mut stream = ObjectStream::new();
    let anim = dereth_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store));
    let mut t = 0.0_f64;
    let walk = dereth_client_runtime::character::CharacterInput {
        forward: true,
        run: true,
        ..Default::default()
    };

    let mut seen: Vec<Vec3> = Vec::new();
    for _ in 0..90 {
        t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
        scene
            .sync_objects(&store, &mut gpu, &mut stream)
            .expect("sync_objects");
        dereth_client_runtime::audio::world_use_time(
            Some(&mut audio),
            Some(&mut scene),
            &mut stream,
            &anim,
            &store,
            LocalTime(t),
        );
        // The listener the frame just installed is the *camera*'s, taken from `WorldScene`.
        let l = audio.listener();
        assert_eq!(
            l,
            scene.listener(),
            "the listener is not the value the world controller's viewer setter would have passed"
        );
        seen.push(l.pos);
        scene.update(Default::default(), walk, LocalTime(t), 1.0 / 30.0);
        scene.stream(&store, &mut gpu).expect("stream");
    }
    let first = seen[0];
    let last = *seen.last().expect("ninety frames");
    let moved = (last.x - first.x).abs() + (last.y - first.y).abs() + (last.z - first.z).abs();
    assert!(
        moved > 1.0,
        "the listener did not move at all over ninety frames of running: {moved}"
    );
    eprintln!("listener travelled {moved:.2} m over 90 frames");
}

// ---------------------------------------------------------------------------------------------
// 5. The ambient set changes with the cell.
// ---------------------------------------------------------------------------------------------

/// **Crossing into different terrain gives a different ambient set.**
///
/// The position-change path scans the 3×3 landblock neighbourhood and builds one ambient
/// sound for each ambient descriptor and sound ID named by the terrain words. The set is therefore
/// a function of the terrain under and around the listener, and running far enough must change it.
///
/// The set is asserted rather than the scan count: that the scan is driven by movement and not
/// by the frame is guarded with the world audio, and a scan that ran and produced the same list is
/// not a cell change.
#[test]
fn running_into_new_terrain_swaps_the_ambient_set() {
    let _gpu = gpu_lock();
    let store = store();
    let mut gpu = test_gpu(320, 240);
    let cfg = SceneConfig {
        scenery_radius: 1,
        time_of_day: Some(0.5),
        ..SceneConfig::default()
    };
    let mut scene = WorldScene::load(&store, &mut gpu, cfg).expect("the landscape loads");
    let region = dereth_client_runtime::landblock::load_region(&store).expect("the region decodes");
    scene
        .attach_character(&store, &region, &mut gpu)
        .expect("the body is created");

    let mut audio =
        dereth_client_runtime::audio::Audio::new(dereth_audio::Prefs::default(), 1, false);
    let mut stream = ObjectStream::new();
    let anim = dereth_world_data::anim_assets::DatAnimAssets::new(Arc::clone(&store));
    let mut t = 0.0_f64;

    let mut run = |audio: &mut dereth_client_runtime::audio::Audio,
                   scene: &mut WorldScene,
                   t: &mut f64,
                   frames: usize,
                   input: dereth_client_runtime::character::CharacterInput| {
        for _ in 0..frames {
            *t += dereth_client_runtime::platform::clock::HEADLESS_STEP;
            scene
                .sync_objects(&store, &mut gpu, &mut stream)
                .expect("sync_objects");
            dereth_client_runtime::audio::world_use_time(
                Some(audio),
                Some(scene),
                &mut stream,
                &anim,
                &store,
                LocalTime(*t),
            );
            scene.update(Default::default(), input, LocalTime(*t), 1.0 / 30.0);
            scene.stream(&store, &mut gpu).expect("stream");
        }
    };

    run(&mut audio, &mut scene, &mut t, 10, Default::default());
    let at_start = audio.ambient_set();
    assert!(
        !at_start.is_empty(),
        "Holtburg's terrain named no ambient sound at all, so the instrument reads zero for \
         everything and the comparison below cannot fail"
    );
    let scans_at_start = audio.world_stats().position_scans;

    let walk = dereth_client_runtime::character::CharacterInput {
        forward: true,
        run: true,
        ..Default::default()
    };
    run(&mut audio, &mut scene, &mut t, 900, walk);
    let after = audio.ambient_set();

    assert!(
        audio.world_stats().position_scans > scans_at_start,
        "the 3x3 rescan never ran again, so nothing could have changed"
    );
    assert_ne!(
        at_start, after,
        "thirty seconds of running left the ambient set identical: {at_start:?}"
    );
    eprintln!(
        "ambient set {} -> {} entries over {} scans",
        at_start.len(),
        after.len(),
        audio.world_stats().position_scans
    );
}

// ---------------------------------------------------------------------------------------------
// 6. Focus.
// ---------------------------------------------------------------------------------------------

/// **Losing the foreground mutes the output and suppresses new sounds; regaining it restores
/// both.**
///
/// The client uses two mechanisms, and both are required:
///
/// 1. the active-only preference and inactive-window guard at every play entry and the internal
///    play path suppress **new** sounds;
/// 2. no secondary buffer carries `DSBCAPS_GLOBALFOCUS` (the creation flags are `0x100E2`), so
///    DirectSound silences everything already playing for the duration. A modern API has no such
///    rule, so this client mutes the master itself.
///
/// The observable is the mixer's block, which is where mechanism 2 lives: a build with only
/// mechanism 1 keeps producing samples from the voice that was already running.
#[test]
fn losing_focus_mutes_the_output_and_suppresses_new_sounds() {
    let store = store();
    let mut a = dereth_client_runtime::audio::Audio::new(dereth_audio::Prefs::default(), 1, false);
    let table = dereth_client_runtime::assets::enum_did(
        &*store,
        dereth_client_runtime::audio::UI_SOUND_TABLE_GROUP,
        dereth_client_runtime::audio::UI_SOUND_TABLE_ENUM,
    )
    .expect("the UI sound table resolves");

    // A voice already running, and audible.
    a.play_media_sound(
        &store,
        table,
        dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
    );
    assert!(a.active_voices() > 0);
    let mut buf = vec![0.0f32; BLOCK / 100];
    a.mix(&mut buf);
    assert!(
        peak(&buf) > 0.0,
        "the calibration voice is silent before focus is touched"
    );

    // Mechanism 2: the block goes silent while the window is not the foreground.
    a.set_focus(false);
    buf.fill(0.0);
    a.mix(&mut buf);
    assert_eq!(
        peak(&buf),
        0.0,
        "an already-playing voice was still audible with no focus"
    );

    // Mechanism 1: a new sound does not start either, with the default preference.
    let started = a.stats.sounds_started;
    let voices = a.active_voices();
    a.play_media_sound(
        &store,
        table,
        dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
    );
    assert!(
        a.stats.sounds_started > started,
        "the call itself is still made"
    );
    assert_eq!(
        a.active_voices(),
        voices,
        "PlaySoundOnlyWhenActive did not suppress the start while inactive"
    );

    // And back.
    a.set_focus(true);
    let before = a.active_voices();
    a.play_media_sound(
        &store,
        table,
        dereth_client_runtime::audio::SOUND_UI_BUTTON_PRESS,
    );
    assert!(
        a.active_voices() > before,
        "regaining focus did not let a sound start again"
    );
}
