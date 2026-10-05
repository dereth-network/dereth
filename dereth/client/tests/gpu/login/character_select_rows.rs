//! Character select and creation against the real screens: the character list is drawn
//! alphabetically at the computed row pitch, the names rasterise only inside the glyph rectangles
//! the shipped `Font` metrics predict, double-clicking a row sends `Login_SendEnterWorld 0xF657`
//! with the recorded client's own bytes, the char-gen wizard builds a `0xF656` that passes every
//! check ACE's `PlayerFactory.Create` makes, and rebuilding the list leaks no descriptor slot.
//! Clicks are button messages broadcast on the application's own element tree.
//! Fixture: the retail dats, a headless App with no link on the character-management screen fed a
//! synthetic `0xF658` character set, and the recorded `0xF657`/`0xF658` pairs of every capture.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_runtime::config::Config;
use dereth_primitives::{DataId, ObjectId};
use dereth_ui::framework::mode;
use dereth_ui::{ElemHandle, ElementId, MessageId};
use dereth_ui_screens::screens::charmgmt::{self, CharacterAction, CharacterManagementScreen};

/// Fails when the retail dats are not where `$DERETH_TEST_DAT_DIR` says.
fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

/// The retail store, or **fail**.
fn store() -> std::sync::Arc<dereth_dat::RetailDatStore> {
    crate::common::dats()
}

fn base_config() -> Config {
    Config {
        headless: true,
        sound: false,
        // The character screen covers the whole back buffer; the world would only slow the
        // comparison down.
        world: false,
        character: false,
        dat_dir: client_dir(),
        ..Config::default()
    }
}

/// Synthetic character names with the recorded names' byte widths and ordering relationships. Their
/// alphabetical order differs from the server-order input, so the display sort and fallback
/// selection remain independent checks.
const LIVE_NAMES: [&str; 3] = ["+Aldis", "+Aldwyne", "+Alba"];

fn character_set(names: &[&str]) -> dereth_ui::persist::CharacterSet {
    dereth_ui::persist::CharacterSet {
        set: names
            .iter()
            .enumerate()
            .map(|(i, n)| dereth_ui::persist::CharacterIdentity {
                id: ObjectId(0x5000_0001 + u32::try_from(i).unwrap_or(0)),
                name: (*n).to_string(),
                seconds_grace_period: 0,
            })
            .collect(),
        // ACE's `max_chars_per_account` default.
        num_allowed_characters: 11,
        account: "ac01".into(),
        ..dereth_ui::persist::CharacterSet::default()
    }
}

/// Bring an offline `App` up on the character-management screen with `names` in the list.
///
/// With no account/server connection there is no transport, so the data-patch screen's
/// condition is satisfied at once and the flow runs DataPatch → Intro → CharacterManagement in
/// three frames. The character set is written where `0xF658` would have written it. A missing
/// device or shell fails the test here; every caller also `expect`s the result.
fn app_on_character_screen(names: &[&str]) -> Option<App> {
    let mut app = App::new(base_config())
        .unwrap_or_else(|e| panic!("a headless App on the software device: {e}"));
    app.start_shell()
        .unwrap_or_else(|e| panic!("the shell starts over the retail dats: {e}"));
    app.load_first_pixel_scene()
        .unwrap_or_else(|e| panic!("the first-pixel scene loads: {e}"));
    let host = app.probe_mut().host_state_mut();
    host.character_set = Some(character_set(names));
    host.received_set = true;
    host.world_name = Some("ACEmulator".into());
    // The data-patch screen, then the intro, then the character screen — with the intro **skipped**,
    // because it really plays: the Turbine logo movie and two splash images, about 15.8 simulated
    // seconds. Skipping it is the player's own route, element message `0x10000001`
    // on the intro's root, broadcast on the real element tree just like every other click here.
    let mut after = 0;
    for _ in 0..12 {
        app.frame();
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::CHARACTER_MANAGEMENT) {
            // Two more frames once it is up: list rebuilding runs on the *next* frame's
            // `push_host_state_into_screen` edge, and a third draws what it built.
            after += 1;
            if after == 2 {
                break;
            }
            continue;
        }
        if app.ui().and_then(|u| u.flow.current_mode()) == Some(mode::INTRO) {
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
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHARACTER_MANAGEMENT),
        "the flow did not reach the character screen"
    );
    Some(app)
}

/// The concrete screen, through the same downcast the shell uses.
fn screen(app: &mut App) -> &mut CharacterManagementScreen {
    let shell = app.ui_mut().expect("the shell is up");
    let s = shell.flow.current_mut().expect("a screen");
    let any: &mut dyn std::any::Any = &mut **s;
    any.downcast_mut::<CharacterManagementScreen>()
        .expect("the character screen")
}

// ---------------------------------------------------------------------------------------------
// 1. The list is built, in the documented order, at the documented pitch
// ---------------------------------------------------------------------------------------------

/// Behaviour: character-select.rows.names-are-real-elements-at-the-shipped-pitch-in-alphabetical-order
///
/// Oracle: the wide-string bubble sort, the greyed-to-the-end
/// partition, and `row_height`'s two divisions — plus the shipped `charactermanagement` layout,
/// whose list box `0x1000039D` is 160 × 320 at (5, 7) inside `0x1000039C` at (37, 205).
///
/// The three synthetic characters model most-recently-played server order:
/// the input is (`+Aldis, +Aldwyne, +Alba`) and the screen must show them
/// alphabetically (`+Alba, +Aldis, +Aldwyne`).
#[test]
fn the_rows_are_real_elements_at_the_documented_pitch_and_in_alphabetical_order() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_character_screen(&LIVE_NAMES).expect("an app on the character screen");

    // The model.
    {
        let s = screen(&mut app);
        assert!(s.rows_built, "no row element was created");
        let names: Vec<&str> = s.rows.iter().map(|r| r.name.as_str()).collect();
        assert_eq!(
            names,
            vec!["+Alba", "+Aldis", "+Aldwyne"],
            "displayed alphabetically"
        );
        // The fallback selection is taken in *server* order, before the sort: the first character
        // ACE listed is the last one played.
        assert_eq!(s.selected_id, ObjectId(0x5000_0001));
        assert_eq!(s.selected_row().map(|r| r.name.as_str()), Some("+Aldis"));
    }

    // The elements, and where they are. 320 / max(11, 3) = 29, which beats 320 / 20 = 16.
    let pitch = charmgmt::row_height(320, 11, 3);
    assert_eq!(pitch, 29);
    let boxes: Vec<(i32, i32, i32, i32)> = {
        let s = screen(&mut app);
        let handles: Vec<ElemHandle> = s.rows.iter().filter_map(|r| r.element).collect();
        assert_eq!(handles.len(), 3, "every row has an element");
        let ui = &app.ui().expect("shell").ui;
        handles
            .iter()
            .map(|h| {
                let b = ui.screen_box(*h);
                (b.x0, b.y0, b.width(), b.height())
            })
            .collect()
    };
    // The list box's own screen origin is (37 + 5, 205 + 7) = (42, 212), and the rows stack from
    // there at the computed pitch.
    for (i, b) in boxes.iter().enumerate() {
        let i = i32::try_from(i).unwrap();
        assert_eq!(b.0, 42, "row {i} x");
        assert_eq!(b.1, 212 + i * pitch, "row {i} y");
        assert_eq!(b.2, 160, "row {i} width -- the template's own");
        assert_eq!(b.3, pitch, "row {i} height");
    }
}

// ---------------------------------------------------------------------------------------------
// 2. The names land on the right pixels
// ---------------------------------------------------------------------------------------------

/// Every pixel a glyph run claims, from the shipped `Font` metrics.
///
/// Place each glyph at the pen plus the font's horizontal and vertical
/// offsets, using the glyph width and the font's maximum character height.
fn glyph_pixels(
    store: &dereth_dat::RetailDatStore,
    list: &[dereth_ui::UiDrawCmd],
    fb: (u32, u32),
) -> (Vec<bool>, usize) {
    let mut fonts: std::collections::BTreeMap<DataId, dereth_render::font::Font> =
        std::collections::BTreeMap::new();
    let mut mask = vec![false; (fb.0 * fb.1) as usize];
    let mut inked = 0usize;
    for cmd in list {
        let Some(clip) = dereth_client_shell::ui_draw::visible_box(cmd, fb) else {
            continue;
        };
        for g in &cmd.glyphs {
            let font = fonts.entry(g.font).or_insert_with(|| {
                dereth_client_shell::ui_draw::load_font(store, g.font).expect("a font")
            });
            let Some(d) = font.get_char_desc(g.ch) else {
                continue;
            };
            if d.width == 0 {
                continue;
            }
            inked += 1;
            let x0 = g.x + i32::from(d.horizontal_offset_before);
            let y0 = g.y + i32::from(d.vertical_offset_before);
            let x1 = x0 + i32::from(d.width) - 1;
            let y1 = y0 + i32::try_from(font.max_char_height).unwrap_or(0) - 1;
            for y in y0.max(clip.1)..=y1.min(clip.3) {
                for x in x0.max(clip.0)..=x1.min(clip.2) {
                    if x >= 0 && y >= 0 && x < fb.0 as i32 && y < fb.1 as i32 {
                        mask[(y as u32 * fb.0 + x as u32) as usize] = true;
                    }
                }
            }
        }
    }
    (mask, inked)
}

/// Oracle: the frame itself — two runs of the **same screen** from the
/// same dats, differing only in the thing under test.
///
/// Here that is the *names*: run A has the three synthetic characters, run B has three characters
/// with the **same ids, the same count and empty names**. Every element, every box, every image,
/// the row pitch, the selection and the six captions are therefore identical between them, and the
/// only thing that can have changed a pixel is a character-name glyph. Every changed pixel must lie
/// inside a rectangle the shipped `Font` metrics predict, and there must be no changed pixel
/// outside one.
#[test]
fn the_character_names_rasterise_only_inside_the_glyph_rectangles_the_font_metrics_predict() {
    let _gpu = gpu_lock();
    let store = store();

    type Shot = (u32, u32, Vec<u8>, Vec<dereth_ui::UiDrawCmd>);
    let run = |names: &[&str]| -> Option<Shot> {
        let mut app = app_on_character_screen(names)?;
        let list = app.ui_draw_list().to_vec();
        let (w, h, bgra) = app.renderer_mut().capture_bgra().ok()?;
        Some((w, h, bgra, list))
    };

    // The **same** three characters in both runs, in a server order that is already alphabetical
    // so the wide-string sort is a no-op: the row order, the row boxes and — because the fallback
    // selection is the first row either way — the selected row's *state* are then identical, and
    // the names are the only difference left. (Test 1 is where the sort itself is asserted.)
    const SORTED: [&str; 3] = ["+Alba", "+Aldis", "+Aldwyne"];
    let (w, h, blank, plain) =
        run(&["", "", ""]).expect("a rendered frame: retail dats and a WARP device");
    let (w2, h2, named, list) =
        run(&SORTED).expect("a rendered frame: retail dats and a WARP device");
    assert_eq!((w, h), (w2, h2));

    // The two runs really are the same screen.
    assert_eq!(
        plain.len(),
        list.len(),
        "the two runs drew different element counts"
    );
    for (a, b) in plain.iter().zip(&list) {
        assert_eq!((a.screen, a.clip, a.image), (b.screen, b.clip, b.image));
    }

    // The rows' own glyph runs, and only theirs, differ.
    let row_glyphs = |l: &[dereth_ui::UiDrawCmd]| -> Vec<String> {
        l.iter()
            .filter(|c| c.screen.x0 == 42 && !c.glyphs.is_empty())
            .map(|c| {
                c.glyphs
                    .iter()
                    .filter_map(|g| char::from_u32(u32::from(g.ch)))
                    .collect()
            })
            .collect()
    };
    assert_eq!(
        row_glyphs(&plain),
        Vec::<String>::new(),
        "an empty name draws no glyph"
    );
    assert_eq!(row_glyphs(&list), vec!["+Alba", "+Aldis", "+Aldwyne"]);

    let (mask, inked) = glyph_pixels(&store, &list, (w, h));
    // The three names are 19 characters between them; the rest of the count is the screen's own
    // six captions, which are identical in both runs.
    assert!(inked > 40, "only {inked} glyphs claimed any pixels");

    let mut inside = 0usize;
    let mut outside = 0usize;
    for (i, claimed) in mask.iter().enumerate() {
        if blank[i * 4..i * 4 + 4] != named[i * 4..i * 4 + 4] {
            if *claimed {
                inside += 1;
            } else {
                outside += 1;
            }
        }
    }
    assert_eq!(
        outside, 0,
        "{outside} pixels changed outside every glyph rectangle"
    );
    assert!(
        inside > 200,
        "only {inside} pixels changed inside a glyph rectangle"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. Choosing a character sends what the retail client sent
// ---------------------------------------------------------------------------------------------

/// Behaviour: character-select.enter-world.a-double-click-sends-the-retail-bytes
///
/// Oracle: every recording that carries one — the recorded client's `0xF657 Login_SendEnterWorld`
/// fragment against the message this build builds for the character the *screen* chose, so the
/// bytes are the ones the retail client put on the wire.
#[test]
fn double_clicking_a_row_asks_to_enter_the_world_with_the_retail_clients_own_bytes() {
    let _gpu = gpu_lock();
    have_dats();
    // Every capture's own `0xF657` is compared, not just the first one found: the encoder is fed each recording's character and account and the bytes must be the recording's.
    // This half needs no `App`, so it costs nothing to run it over the whole corpus.
    let all = retail_enter_worlds();
    for r in &all {
        let ours = write_message(&dereth_protocol::login::LoginSendEnterWorld {
            character: r.character,
            account: r.account.clone(),
        });
        assert_eq!(
            ours, r.payload,
            "0xF657 differs from the retail client's: ours {ours:02X?}, retail {:02X?}",
            r.payload
        );
    }
    // The byte comparison covers every recording with an enter-world message; the row drive
    // excludes the recordings whose `0xF658` never listed the character they entered with. In the
    // three `fellowship-*` recordings the player CREATED the character in-session (`0xF656`,
    // answered by `0xF643`, then `0xF7C8`/`0xF657` with the new id), two of them on an account
    // whose `0xF658` lists no character at all, so the screen has no row to double-click; that
    // entry is the char-gen wizard's path. The two `house-purchase-*` recordings are excluded for
    // the same reason, and `requested-death-vitae-salvage` records a different account; the byte
    // comparison above still covers all of them.
    let driven: Vec<_> = all
        .into_iter()
        .filter(|r| {
            !(r.session.starts_with("fellowship")
                || r.session.starts_with("house")
                || r.session == "requested-death-vitae-salvage")
        })
        .collect();
    // The row drive's subject is named (`first-login-walk-jump`) rather than taken as the first
    // driven recording, so adding recordings cannot move it.
    let mut recorded = driven
        .into_iter()
        .find(|r| r.session == "first-login-walk-jump")
        .expect("first-login-walk-jump is the row drive's subject and carries a 0xF657");
    // The capture's `0xF657` names the character the retail player picked and the account it was
    // on; drive the screen with exactly that list so the choice is comparable.
    let names: Vec<&str> = recorded.names.iter().map(String::as_str).collect();
    let mut app = app_on_character_screen(&names).expect("an app on the character screen");

    // Select the row the recorded player entered on, then double-click it. Both messages use
    // the same element-tree broadcast reached by mouse input.
    let row = {
        let s = screen(&mut app);
        let want = recorded.character;
        s.rows
            .iter()
            .find(|r| r.id == want)
            .and_then(|r| r.element)
            .expect("the chosen character has a row")
    };
    // The shell's own frame rather than `App::frame`, for one reason: `App::ui_use_time` drains
    // the actions and hands them to the session, and with no link they are logged and dropped.
    // This is the same `UiShell::frame` that call makes, one level up.
    let host = app.host_state().clone();
    let actions = {
        let shell = app.ui_mut().expect("shell");
        shell
            .ui
            .broadcast_element_message(row, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
        shell
            .ui
            .broadcast_element_message(row, MessageId(0x1A), 7, 0);
        shell.frame(
            dereth_primitives::LocalTime(1.0),
            &host,
            &mut dereth_ui::NullInputPump,
        );
        shell.take_character_actions()
    };
    assert_eq!(
        actions,
        vec![CharacterAction::LogOn(recorded.character)],
        "the double-click is `EnterGame` -> `player login`"
    );
    assert_eq!(
        screen(&mut app).open_dialog,
        Some(charmgmt::DialogContext::EnteringWorld),
        "the enter-world action opens the entering-world dialog first"
    );

    // And the bytes: the enter-world request writes the opcode, the object ID and
    // the packed account name; ours must be the recording's, byte for byte.
    let ours = write_message(&dereth_protocol::login::LoginSendEnterWorld {
        character: recorded.character,
        account: std::mem::take(&mut recorded.account),
    });
    assert_eq!(
        ours, recorded.payload,
        "0xF657 differs from the retail client's: ours {ours:02X?}, retail {:02X?}",
        recorded.payload
    );
}

/// The retail client's `0xF657` out of the packet corpus, plus the `0xF658` that preceded it.
struct RecordedEnterWorld {
    /// Which recording it came out of.
    session: String,
    /// The whole fragment payload, opcode included.
    payload: Vec<u8>,
    character: ObjectId,
    account: String,
    names: Vec<String>,
}

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

/// **Every** `0xF657` the retail client sent, one per capture that reached character select.
fn retail_enter_worlds() -> Vec<RecordedEnterWorld> {
    let mut found = Vec::new();
    for session in corpus_sessions() {
        let mut set: Option<dereth_protocol::login::LoginCharacterSet> = None;
        'datagrams: for d in shared_session(&session) {
            let Ok(p) = dereth_transport::wire::ParsedPacket::parse(&d.raw) else {
                continue;
            };
            for f in &p.fragments {
                if f.header.blob_num != 0 || f.payload.len() < 4 {
                    continue;
                }
                let op =
                    u32::from_le_bytes([f.payload[0], f.payload[1], f.payload[2], f.payload[3]]);
                // The server's `0xF658` is fragmented in some sessions; only the single-fragment
                // case is read, which every recording has at least one of.
                if !d.c2s && op == 0xF658 && f.header.num_frags == 1 {
                    if let Ok(m) = dereth_protocol::read_body::<
                        dereth_protocol::login::LoginCharacterSet,
                    >(&f.payload[4..])
                    {
                        set = Some(m);
                    }
                }
                if d.c2s && op == 0xF657 {
                    let m =
                        dereth_protocol::read_body::<dereth_protocol::login::LoginSendEnterWorld>(
                            &f.payload[4..],
                        )
                        .expect("the recorded 0xF657 decodes");
                    let set = set.as_ref().expect("0xF658 preceded 0xF657");
                    found.push(RecordedEnterWorld {
                        session: session.clone(),
                        payload: f.payload.clone(),
                        character: m.character,
                        account: m.account.clone(),
                        names: set.characters.iter().map(|c| c.name.clone()).collect(),
                    });
                    break 'datagrams;
                }
            }
        }
    }
    // The `break 'datagrams` above keeps this one entry per capture: a recording that sends
    // several `0xF657` over one connection contributes its first. Login-only recordings (the
    // account authenticates, `0xF658` arrives, the recording disconnects without a double-click)
    // contribute none.
    assert!(
        !found.is_empty(),
        "the corpus carries a 0xF657 Login_SendEnterWorld"
    );
    found
}

/// The opcode and body a message puts on the wire, which is what a fragment payload holds.
fn write_message<M: dereth_protocol::Message>(m: &M) -> Vec<u8> {
    let mut out = M::OPCODE.0.to_le_bytes().to_vec();
    out.extend(dereth_protocol::write_body(m).expect("the message encodes"));
    out
}

// ---------------------------------------------------------------------------------------------
// 4. A new character is created end to end
// ---------------------------------------------------------------------------------------------

/// Oracle: the character-generation field order and checksum, encoded through
/// `dereth_protocol`, and **every check ACE's
/// `PlayerFactory.Create` makes**: 55 skill entries, each named skill in `SkillBaseHash`, its cost
/// within `AvailableSkillCredits`, every attribute in 10…100 and their total no more than the
/// heritage's `AttributeCredits`.
///
/// The wizard is driven the way a player drives it — a heritage button, a town button, a name, and
/// Finish — through the same element-tree broadcast the mouse reaches, and the message that
/// comes out is the one the client would send.
#[test]
fn the_wizard_builds_a_character_ace_will_accept() {
    let _gpu = gpu_lock();
    have_dats();
    let store = store();
    let mut app = app_on_character_screen(&LIVE_NAMES).expect("an app on the character screen");

    // Create Character (`0x100003A0`) queues mode `0x1000000B`.
    let create_button = {
        let root = {
            let shell = app.ui().expect("shell");
            shell.flow.current().expect("a screen").roots()[0]
        };
        app.ui()
            .expect("shell")
            .ui
            .get_child_recursive(root, ElementId(0x1000_03A0))
    };
    let Some(create_button) = create_button else {
        panic!("the Create Character button is not in the layout");
    };
    app.ui_mut().expect("shell").ui.broadcast_element_message(
        create_button,
        dereth_ui::msg::element::id::BUTTON_CLICKED,
        7,
        0,
    );
    app.frame();
    assert_eq!(
        app.ui().and_then(|u| u.flow.current_mode()),
        Some(mode::CHAR_GEN),
        "Create Character queues the wizard"
    );

    // Aluvian (`0x100003BF`), Holtburg (`0x1000040D`), a name, Finish (`0x100003C8`).
    let click = |app: &mut App, id: u32| {
        let shell = app.ui_mut().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        let h = shell
            .ui
            .get_child_recursive(root, ElementId(id))
            .unwrap_or_else(|| panic!("element {id:#010X} is not in the wizard's tree"));
        shell
            .ui
            .broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    };
    click(&mut app, 0x1000_03BF);
    click(&mut app, 0x1000_040D);
    app.frame();
    // The shipped layout shows FINISH (`0x100003C8`) only on the summary page, and the button
    // handler requires the summary progress state before accepting that click, so go there.
    click(&mut app, 0x1000_03F4); // the Summary tab
    app.frame();

    {
        let shell = app.ui_mut().expect("shell");
        let root = shell.flow.current().expect("a screen").roots()[0];
        {
            let s = shell.flow.current_mut().expect("a screen");
            let any: &mut dyn std::any::Any = &mut **s;
            let w = any
                .downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
                .expect("the wizard");
            assert!(w.tables.is_some(), "the char-gen tables loaded");
            assert_eq!(w.state.heritage_group, 1, "Aluvian");
            assert_eq!(w.state.start_area, 0, "Holtburg");
            assert!(
                w.state.gender != 0,
                "a gender was taken from the heritage's own table"
            );
        }
        // Type a name into the summary page's box the way the element does.
        let name_box = shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::screens::chargen::NAME_FIELD)
            .expect("the name box");
        if let Some(t) = shell.ui.text_element_mut(name_box) {
            t.set_text("Tarinell");
        }
        shell
            .ui
            .broadcast_element_message(name_box, MessageId(0x44), 0, 0);
    }
    app.frame();
    click(&mut app, 0x1000_03C8);

    // Finishing warns about unspent credits first when any remain; answering yes sends. As above, the
    // shell's own frame rather than `App::frame`, because with no link the app logs the request
    // and drops it.
    // No `app.frame()` between the click and this block: with a profession applied, finishing
    // sends on the first press, and `App::frame` would drain the request into an application with
    // no link, which drops it.
    let host = app.host_state().clone();
    fn wizard(app: &mut App) -> &mut dereth_ui_screens::screens::chargen::CharGenScreen {
        let shell = app.ui_mut().expect("shell");
        let s = shell.flow.current_mut().expect("a screen");
        let any: &mut dyn std::any::Any = &mut **s;
        any.downcast_mut::<dereth_ui_screens::screens::chargen::CharGenScreen>()
            .expect("the wizard")
    }
    // The shell's own frame dispatches the queued click.
    {
        let shell = app.ui_mut().expect("shell");
        shell.frame(
            dereth_primitives::LocalTime(2.0),
            &host,
            &mut dereth_ui::NullInputPump,
        );
    }
    // Finish warns only when attribute credits remain. Character randomization selects and applies
    // a profession, spending the credits, so finishing usually sends on the first press.
    let unspent = wizard(&mut app).state.remaining_atrb_credits;
    let want = if unspent > 0 {
        dereth_ui_screens::screens::chargen::CharGenDialog::CreditWarning
    } else {
        dereth_ui_screens::screens::chargen::CharGenDialog::PleaseWait
    };
    assert_eq!(
        wizard(&mut app).open_dialog,
        Some(want),
        "Finish warns only about unspent attribute credits, and there are {unspent}"
    );
    let actions = {
        if unspent > 0 {
            wizard(&mut app).close_dialog(true);
        }
        let shell = app.ui_mut().expect("shell");
        shell.frame(
            dereth_primitives::LocalTime(3.0),
            &host,
            &mut dereth_ui::NullInputPump,
        );
        shell.take_chargen_actions()
    };
    let [dereth_ui_screens::screens::chargen::CharGenAction::SendCharGenResult(result)] =
        actions.as_slice()
    else {
        panic!("the wizard did not ask to create anything: {actions:?}");
    };

    // Everything ACE checks, checked here.
    let mut msg = dereth_client_runtime::app::chargen_result_to_wire(result);
    msg.checksum_value = msg.checksum();
    assert_eq!(msg.name, "Tarinell");
    assert_eq!(msg.version, 1);
    assert_eq!(
        msg.skill_advancement_classes.len(),
        55,
        "ClientServerSkillsMismatch otherwise"
    );

    use dereth_assets::Decode;
    let cg_bytes =
        dereth_primitives::AssetSource::read(store.as_ref(), DataId(0x0E00_0002)).expect("dat");
    let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &cg_bytes)
        .expect("the char-gen table decodes");
    let sk_bytes =
        dereth_primitives::AssetSource::read(store.as_ref(), DataId(0x0E00_0004)).expect("dat");
    let skills = dereth_assets::tables::SkillTable::decode_payload(DataId(0x0E00_0004), &sk_bytes)
        .expect("the skill table decodes");
    let hg = cg
        .heritage_groups
        .get(&msg.heritage_group)
        .expect("a real heritage");
    assert!(
        hg.sexes.contains_key(&msg.gender),
        "gender {} is not in {}'s table",
        msg.gender,
        hg.name
    );
    assert!(
        (msg.start_area as usize) < cg.starter_areas.len(),
        "start area {} is past the {} the table holds",
        msg.start_area,
        cg.starter_areas.len()
    );
    let attrs = [
        msg.strength,
        msg.endurance,
        msg.coordination,
        msg.quickness,
        msg.focus,
        msg.self_,
    ];
    for a in attrs {
        assert!(
            (10..=100).contains(&a),
            "attribute {a} is outside ACE's 10..=100"
        );
    }
    let total: i32 = attrs.iter().sum();
    assert!(
        total <= i32::try_from(hg.attribute_credits).unwrap(),
        "{total} attribute points spent of {}",
        hg.attribute_credits
    );
    let mut used = 0i32;
    for (i, sac) in msg.skill_advancement_classes.iter().enumerate() {
        if *sac == 0 || *sac == 1 {
            continue;
        }
        let id = u32::try_from(i).unwrap();
        assert!(
            skills.skills.contains_key(&id),
            "skill {id} is trained and is not in SkillBaseHash -- InvalidSkillRequested"
        );
        let (trained, specialized) =
            dereth_chargen::CharGenState::skill_costs(&cg, &skills, msg.heritage_group, id);
        used += if *sac == 3 { specialized } else { trained };
    }
    assert!(
        used <= i32::try_from(hg.skill_credits).unwrap(),
        "{used} skill credits spent of {}",
        hg.skill_credits
    );

    // And it round-trips: what we would put on the wire decodes back to the same thing, with the
    // checksum the client computes.
    let bytes = write_message(&dereth_protocol::login::CharacterSendCharGenResult {
        account: "ac01".into(),
        result: msg.clone(),
    });
    let back = dereth_protocol::read_body::<dereth_protocol::login::CharacterSendCharGenResult>(
        &bytes[4..],
    )
    .expect("0xF656 round-trips");
    assert_eq!(back.result, msg);
    assert_eq!(back.result.checksum_value, back.result.checksum());
}

// ---------------------------------------------------------------------------------------------
// 5. The descriptor heap stays bounded
// ---------------------------------------------------------------------------------------------

/// Oracle: the renderer's descriptor allocator. Rebuilding the list deletes and recreates every row
/// element, which is the shape that leaks a slot per rebuild if the release is missed.
#[test]
fn rebuilding_the_character_list_does_not_leak_descriptor_slots() {
    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_character_screen(&LIVE_NAMES).expect("an app on the character screen");
    let after_first = app.renderer().descriptor_usage();

    // Ten more character sets, each different so the rebuild really happens.
    for i in 0..10 {
        let names: Vec<String> = LIVE_NAMES.iter().map(|n| format!("{n}{i}")).collect();
        let refs: Vec<&str> = names.iter().map(String::as_str).collect();
        app.probe_mut().host_state_mut().character_set = Some(character_set(&refs));
        app.frame();
    }
    let steady = app.renderer().descriptor_usage();
    assert_eq!(
        steady.live, after_first.live,
        "eleven character lists took {} slots where one took {}",
        steady.live, after_first.live
    );
    let stats = app.renderer().descriptor_stats();
    assert_eq!(stats.exhaustions, 0, "the descriptor heap was exhausted");
    let release = app.ui_release_report();
    assert_eq!(release.unknown, 0, "a texture was released twice");
    eprintln!(
        "descriptors: live {} / high water {} of {} (free {}), releases {release:?}",
        steady.live, steady.high_water, steady.capacity, steady.free
    );
    app.shutdown();
}

// ---------------------------------------------------------------------------------------------
// 5. The world's message scrolls under the mouse wheel
// ---------------------------------------------------------------------------------------------

/// One Windows message, through the pump and into the input shell, as the window procedure
/// delivers it.
fn send(
    pump: &mut dereth_desktop::pump::Pump,
    app: &mut App,
    m: dereth_input::win32::Win32Message,
) {
    pump.dispatch(m);
    app.input_manager_mut()
        .expect("the input shell exists in a UI build")
        .on_message(m);
}

/// Behaviour: login.character-select.the-mouse-wheel-scrolls-the-worlds-message
///
/// Oracle: the message window's own text element and its scroll offset. The wheel is driven as
/// Windows drives it, `WM_MOUSEWHEEL` detents with the pointer over the text and nothing focused.
#[test]
fn the_mouse_wheel_scrolls_the_worlds_message_with_nothing_focused() {
    use dereth_ui_screens::screens::screen_message::TEXT;
    use {dereth_desktop::pump::Pump, dereth_input::win32::Win32Message};

    let _gpu = gpu_lock();
    have_dats();
    let mut app = app_on_character_screen(&LIVE_NAMES).expect("an app on the character screen");
    let long: String = (1..=60).map(|i| format!("Line {i}.\n")).collect();
    app.probe_mut().host_state_mut().character_screen_message = Some(long);
    app.frame();
    app.frame();
    let window = screen(&mut app)
        .message
        .window
        .expect("the message window is up");
    let text = app
        .ui()
        .and_then(|u| u.ui.get_child_recursive(window, TEXT))
        .expect("the window's text");
    let offset = |app: &mut App| {
        app.ui_mut()
            .and_then(|u| u.ui.text_element_mut(text))
            .map(|t| t.scroll.y)
            .expect("a text element")
    };
    assert!(
        app.ui().is_some_and(|u| u.ui.focus_element().is_none()),
        "nothing has the focus"
    );
    let before = offset(&mut app);

    let mut pump = Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    let b = app.ui().expect("shell").ui.screen_box(text);
    let mut t = 400_000;
    let at = pump.mouse_move_message(
        f64::from((b.x0 + b.x1) / 2),
        f64::from((b.y0 + b.y1) / 2),
        t,
    );
    send(&mut pump, &mut app, at);
    app.frame();
    for _ in 0..3 {
        t += 50;
        // One detent towards the player: WHEEL_DELTA 120, negative, in the high word.
        let down = u32::from((-120_i16).cast_unsigned()) << 16;
        let wheel = Win32Message::new(dereth_input::win32::msg::WM_MOUSEWHEEL, down as usize, 0, t);
        send(&mut pump, &mut app, wheel);
        app.frame();
    }
    let after = offset(&mut app);
    assert!(
        after > before,
        "three detents down scroll the message: offset {before} -> {after}"
    );
    app.shutdown();
}
