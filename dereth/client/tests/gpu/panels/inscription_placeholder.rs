//! The examine panel's inscription box on a target change, retail's unwanted request included:
//! with the caret in the box, changing to an unsigned, inscribable target the viewer does not
//! own shows `<Inscribe here>` (the stored inscription stays empty), turns editability `0x16`
//! and selectability `0x27` off, and the resulting `0x2F` focus loss sends
//! `0xBF Writing_SetInscription(new_target, "<Inscribe here>")`. A signed target, a
//! non-inscribable target and another owned target send nothing; Escape commits typed text; an
//! empty-world press keeps focus; the locked box's background explains refusals in chat; and the
//! three privilege qualities unlock only a live inscribable object. Client emission only.
//! Fixture: the retail dats and a headless `App` on the gameplay screen, fed synthetic objects
//! and appraisal replies by a socket-free peer; `0xBF` is decoded from the outgoing datagrams.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;

use dereth_client::app::App;
use dereth_client::config::Config;
use dereth_client::net::ClientNetwork;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::types::AppraisalProfile;
use dereth_protocol::{Message, Opcode};
use dereth_ui::ElementId;
use winit::event::MouseButton;

/// Inscription text element in the shipped examination layout.
const INSCRIPTION_TEXT: ElementId = ElementId(0x1000_013E);
/// Refusal background exposed to mouse hits while the inscription text is not editable.
const INSCRIPTION_BACKGROUND: ElementId = ElementId(0x1000_0137);
const PLAYER: ObjectId = ObjectId(0x5000_0001);
/// The player's own parchment: inscribable, unsigned, in his pack.
const MINE: ObjectId = ObjectId(0x8000_0001);
/// The target-change case: unsigned, inscribable, and without a container or wielder.
/// The ownership walk does not reach the player.
const NOT_MINE: ObjectId = ObjectId(0x8000_0002);
/// Inscribable, not the player's, and already **signed** — the calibration.
const SIGNED_NOT_MINE: ObjectId = ObjectId(0x8000_0003);
/// Signed by this player but outside his inventory: the own-scribe comparison's refusal leg.
const SIGNED_BY_ME_NOT_MINE: ObjectId = ObjectId(0x8000_0006);
/// Unsigned and inscribable, with a nonzero equipment location: clicking gives no refusal.
const EQUIPPED_NOT_MINE: ObjectId = ObjectId(0x8000_0007);
/// Not inscribable at all, and not the player's — the calibration.
const PLAIN_NOT_MINE: ObjectId = ObjectId(0x8000_0004);
/// A second parchment of the player's — the "still editable, keeps the caret" calibration.
const ALSO_MINE: ObjectId = ObjectId(0x8000_0005);

/// Public `ObjectDescriptionFlag::Inscribable`: mask `0x2` in the object-description bitfield.
const INSCRIBABLE: u32 = 0x0000_0002;

/// Invitation text, spelled independently so a changed production constant is detected.
const PLACEHOLDER: &str = "<Inscribe here>";

// ---------------------------------------------------------------------------------------------
// Harness — a socket-free peer and the synthetic objects the tests identify
// ---------------------------------------------------------------------------------------------

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "inscription-placeholder-station",
            "unused",
            0,
        )
        .expect("a net");
        net.session.transport.add_connection(
            0xB,
            0,
            1,
            0xDEAD_BEEF,
            0x1234_5678,
            Some("127.0.0.1:19000".parse().expect("addr")),
        );
        (
            Self {
                crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
                sequence: 1,
                blob: 0,
                stamp: 0,
            },
            net,
        )
    }

    fn send(&mut self, app: &mut App, queue: u16, bytes: Vec<u8>) {
        self.sequence += 1;
        self.blob += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_fragment(dereth_transport::Fragment::new(
                dereth_transport::FragmentHeader {
                    blob_id_low: self.blob,
                    blob_id_high: 0x8000_0000,
                    num_frags: 1,
                    blob_frag_size: 0,
                    blob_num: 0,
                    queue_id: queue,
                },
                bytes,
            ))
            .expect("one fragment");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a datagram");
        app.replay_network_mut()
            .expect("the replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("fed");
    }

    /// `[0xF7B0][iid][stamp][0x00C9][object][profile]` on the UI queue (9).
    fn appraisal_reply(&mut self, app: &mut App, id: ObjectId, p: &AppraisalProfile) {
        self.stamp += 1;
        let blob = dereth_protocol::events::pack_event(
            PLAYER,
            self.stamp,
            &dereth_protocol::objects::ItemSetAppraiseInfo {
                object: id,
                profile: p.clone(),
            },
        )
        .expect("the reply frames");
        self.send(app, 9, blob);
    }
}

fn setup() -> (App, Peer) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir()
            .join("dereth-inscription-placeholder-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    let s = dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client::world::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");

    // The ownership walk follows container/wielder links to the player. These synthetic
    // objects have no wielder; only the two owned parchments receive a player container.
    for (id, parent) in [
        (PLAYER, None),
        (MINE, Some(PLAYER)),
        (ALSO_MINE, Some(PLAYER)),
        (NOT_MINE, None),
        (SIGNED_NOT_MINE, None),
        (SIGNED_BY_ME_NOT_MINE, None),
        (EQUIPPED_NOT_MINE, None),
        (PLAIN_NOT_MINE, None),
    ] {
        let mut p = ObjectCreatePayload {
            id,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        if let Some(parent) = parent {
            p.wdesc.header |= dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
            p.wdesc.container_id = Some(parent);
        }
        peer.send(
            &mut app,
            10,
            dereth_protocol::write_blob(&ItemCreateObject(p)).expect("blob"),
        );
    }
    app.frame();

    app.objects_mut().world.player = Some(PLAYER);
    {
        let w = app
            .objects_mut()
            .world
            .weenie_mut(PLAYER)
            .expect("the player was created");
        w.pwd.name = "Larktest".to_string();
    }
    for (id, name, inscribable) in [
        (MINE, "Parchment", true),
        (ALSO_MINE, "Vellum", true),
        (NOT_MINE, "Somebody's scroll", true),
        (SIGNED_NOT_MINE, "A signed note", true),
        (SIGNED_BY_ME_NOT_MINE, "My dropped note", true),
        (EQUIPPED_NOT_MINE, "Displayed equipped note", true),
        (PLAIN_NOT_MINE, "Pebble", false),
    ] {
        let w = app.objects_mut().world.weenie_mut(id).expect("created");
        w.pwd.name = name.to_string();
        if inscribable {
            w.pwd.bitfield |= INSCRIBABLE;
        }
    }
    app.objects_mut()
        .world
        .weenie_mut(EQUIPPED_NOT_MINE)
        .expect("created")
        .pwd
        .location = Some(1);
    // The ownership premise every test rests on, measured rather than assumed.
    assert!(
        app.objects_mut().world.is_owned_by_player(MINE),
        "MINE is in his pack"
    );
    assert!(
        !app.objects_mut().world.is_owned_by_player(NOT_MINE),
        "NOT_MINE is nobody's"
    );
    (app, peer)
}

fn profile(scribe: Option<&str>, inscription: Option<&str>) -> AppraisalProfile {
    let mut p = AppraisalProfile {
        success_flag: 1,
        ..AppraisalProfile::default()
    };
    let mut entries = Vec::new();
    if let Some(t) = inscription {
        entries.push((7u32, t.to_string()));
    }
    if let Some(n) = scribe {
        entries.push((8u32, n.to_string()));
    }
    if !entries.is_empty() {
        p.flags |= dereth_protocol::types::appraisal::flags::STRING;
        p.tables.strings = Some(dereth_protocol::archive::PackedHash {
            table_size: 8,
            entries,
        });
    }
    p
}

/// Identify an object and let the reply and any resulting outgoing request settle.
///
/// Six frames after the reply allow the full chain to run: appraisal delivery updates the
/// panel; changed selectability relinquishes the caret; the `0x2F` notice leaves the UI outbox;
/// the losing-focus handler emits a request; interaction routing drains it; and packet
/// processing serializes the outgoing datagram. These are successive processing stages, not a
/// claim that each takes exactly one frame; two frames are enough for the UI state but not for
/// the request to be sent. The endpoint remains socket-free throughout.
fn identify(app: &mut App, peer: &mut Peer, id: ObjectId, p: &AppraisalProfile) {
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(dereth_ui_screens::view::UiRequest::Examine(id));
    app.frame();
    app.frame();
    peer.appraisal_reply(app, id, p);
    for _ in 0..6 {
        app.frame();
    }
}

/// Decode matching `0xBF Writing_SetInscription` bodies from queued outgoing datagrams.
/// Queue 3 is the public Weenie message queue. Invalid packets and nonmatching fragments are
/// skipped; this helper neither reassembles fragments nor counts unrelated outgoing traffic.
fn inscriptions_sent(app: &mut App) -> Vec<dereth_protocol::trade::WritingSetInscription> {
    let mut out = Vec::new();
    for (bytes, _) in app
        .replay_network_mut()
        .expect("the endpoint")
        .take_outgoing()
    {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for fragment in packet.fragments {
            if fragment.header.queue_id != 3 {
                continue;
            }
            let body = &fragment.payload;
            if body.len() < 12 || u32::from_le_bytes([body[0], body[1], body[2], body[3]]) != 0xF7B1
            {
                continue;
            }
            if u32::from_le_bytes([body[8], body[9], body[10], body[11]])
                != Opcode::WRITING_SET_INSCRIPTION.0
            {
                continue;
            }
            let mut r = dereth_protocol::archive::Reader::new(&body[12..]);
            out.push(
                dereth_protocol::trade::WritingSetInscription::read(&mut r)
                    .expect("the client's own 0xBF body decodes"),
            );
        }
    }
    out
}

fn box_text(app: &mut App) -> String {
    let ui = &mut app.ui_mut().expect("the shell").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("<EXAM> ships 0x1000013E");
    ui.text_element_mut(h)
        .expect("a text element")
        .glyphs
        .inq_text(false)
}

fn box_has_the_caret(app: &mut App) -> bool {
    let ui = &mut app.ui_mut().expect("the shell").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("<EXAM> ships 0x1000013E");
    ui.focus_element() == Some(h)
}

fn box_is_editable(app: &App) -> bool {
    let ui = &app.ui().expect("the shell").ui;
    let h = ui
        .get_element(INSCRIPTION_TEXT)
        .expect("<EXAM> ships 0x1000013E");
    ui.node(h)
        .and_then(|n| {
            n.merged_properties()
                .get_bool(dereth_ui::props::attr::TEXT_EDITABLE)
        })
        .unwrap_or(false)
}

fn set_psr_quality(app: &mut App, sequence: u8, property_id: u32, value: bool) {
    let update = dereth_protocol::qualities::QualitiesPrivateUpdateBool(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id,
            value: i32::from(value),
        },
    );
    let mut blob = Opcode::QUALITIES_PRIVATE_UPDATE_BOOL
        .0
        .to_le_bytes()
        .to_vec();
    blob.extend(dereth_protocol::write_body(&update).expect("private Boolean quality update"));
    app.apply_hud_events(&[SessionEvent::UiEvent {
        opcode: Opcode::QUALITIES_PRIVATE_UPDATE_BOOL,
        blob,
    }]);
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("the accepted player description")
            .inq_bool(property_id),
        value,
        "the encoded private update reached the authoritative player quality store"
    );
}

fn click_the_box(app: &mut App) {
    let at = {
        let ui = &mut app.ui_mut().expect("the shell").ui;
        let h = ui.get_element(INSCRIPTION_TEXT).expect("bound");
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let shell = app.ui_mut().expect("the shell");
    shell.ui.mouse_move(LocalTime(1.0), at.0, at.1);
    shell
        .ui
        .mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    shell
        .ui
        .mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    app.frame();
}

fn click_inscription_background(app: &mut App) {
    let at = {
        let ui = &mut app.ui_mut().expect("the shell").ui;
        let h = ui
            .get_element(INSCRIPTION_BACKGROUND)
            .expect("<EXAM> ships 0x10000137");
        assert!(
            ui.node(h).is_some_and(|n| n.is_mouse_visible),
            "inscription editability gives the refusal background mouse visibility"
        );
        let b = ui.screen_box(h);
        (b.y0..b.y1)
            .flat_map(|y| (b.x0..b.x1).map(move |x| (x, y)))
            .find(|(x, y)| ui.hit_test_screen(*x, *y) == Some(h))
            .unwrap_or_else(|| {
                let at = ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2);
                panic!(
                    "the mouse-visible inscription background has no exposed pixel; centre hits {:?}",
                    ui.hit_test_screen(at.0, at.1)
                )
            })
    };
    let shell = app.ui_mut().expect("the shell");
    shell.ui.mouse_move(LocalTime(2.0), at.0, at.1);
    shell
        .ui
        .mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    shell
        .ui
        .mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    for _ in 0..4 {
        app.frame();
    }
}

fn visible_spew(app: &App) -> Vec<String> {
    let model = &app.hud().panels.spew.model;
    model
        .pending
        .iter()
        .chain(model.items.iter())
        .cloned()
        .collect()
}

fn type_into_box(app: &mut App, text: &str) {
    let shell = app.ui_mut().expect("the shell");
    for c in text.encode_utf16() {
        shell.ui.character(c);
    }
    app.frame();
}

/// Dispatch Escape to the focused element. Attribute `0xCB` enables losing focus on Escape,
/// the inscription's confirmation edge (Enter is not).
fn press_escape(app: &mut App) {
    {
        let shell = app.ui_mut().expect("the shell");
        let Some(h) = shell.ui.focus_element() else {
            return;
        };
        shell.ui.dispatch_action(
            h,
            &dereth_ui::focus::InputEvent {
                action: dereth_ui::focus::action::ESCAPE,
                start: true,
                x: 0,
                y: 0,
            },
        );
    }
    app.frame();
    app.frame();
}

/// Send a primary press and release at a hit-tested point of the shipped `<SBOX>` through
/// normalized pump messages and the input manager. This exercises routing beyond the direct
/// `UiSystem::mouse_down` helper; it does not inject operating-system input.
fn press_empty_world_view(app: &mut App) {
    let at = {
        let shell = app.ui_mut().expect("the shell");
        let root = *shell
            .flow
            .current()
            .expect("the game screen")
            .roots()
            .first()
            .expect("a root");
        let sbox = shell
            .ui
            .get_child_recursive(root, dereth_ui_screens::hud::world_view::SMART_BOX)
            .expect("the shipped <SBOX>");
        let b = shell.ui.screen_box(sbox);
        (b.y0..=b.y1)
            .step_by(4)
            .flat_map(|y| (b.x0..=b.x1).step_by(4).map(move |x| (x, y)))
            .find(|&(x, y)| shell.ui.hit_test_screen(x, y) == Some(sbox))
            .expect("the visible 3-D viewport has an exposed pointer pixel")
    };

    let missed_before = app.interaction().pick.stats.missed;
    let mut pump = dereth_client::pump::Pump::new();
    pump.state.is_ready = true;
    pump.state.is_active_app = true;
    for message in [
        pump.mouse_move_message(f64::from(at.0), f64::from(at.1), 10_000),
        pump.mouse_button_message(MouseButton::Left, true, 10_010)
            .expect("WM_LBUTTONDOWN"),
        pump.mouse_button_message(MouseButton::Left, false, 10_020)
            .expect("WM_LBUTTONUP"),
    ] {
        pump.dispatch(message);
        app.input_manager_mut()
            .expect("the real input maps")
            .on_message(message);
    }
    for _ in 0..4 {
        app.frame();
    }
    assert!(
        app.interaction().pick.stats.missed > missed_before,
        "the actual <SBOX> press completed as an empty-world pick: {:?}",
        app.interaction().pick.stats
    );
}

/// Identify the player's own unsigned parchment, click into the box and type. Leaves the caret in
/// the box, which is the placeholder request's precondition.
fn writing_on_his_own(app: &mut App, peer: &mut Peer, words: &str) {
    identify(app, peer, MINE, &profile(None, None));
    assert_eq!(
        box_text(app),
        PLACEHOLDER,
        "the unsigned owned parchment displays the invitation"
    );
    click_the_box(app);
    type_into_box(app, words);
    assert!(
        box_has_the_caret(app),
        "the premise: the box holds the caret"
    );
    assert_eq!(box_text(app), words, "and it holds what was typed");
    let _ = inscriptions_sent(app);
}

// ---------------------------------------------------------------------------------------------
// 1. The placeholder request
// ---------------------------------------------------------------------------------------------

/// Behaviour: examine.inscription.a-target-change-sends-the-literal-placeholder
///
/// Change the target to an inscribable, unsigned item that is not the player's while the box has
/// the caret, and the client sends `0xBF` carrying the literal `<Inscribe here>` for the **new**
/// object.
///
/// The sequence: the target change clears the displayed text, stored inscription and stored
/// scribe; the unsigned target displays the invitation while the stored inscription stays empty;
/// for a viewer who does not own it, editability and selectability go false and the caret is
/// relinquished; the `0x2F` notice then reads nonempty text different from the stored empty
/// inscription, and the current object id already names the new target.
///
/// Both request fields are checked. The test does not establish that a server will write that
/// text: ACE's `Source/ACE.Server/WorldObjects/Player_Inventory.cs` `HandleActionSetInscription`
/// searches inventory and equipped items first, and only Sentinel/Admin callers extend the search
/// to other objects.
#[test]
fn the_invitation_leg_sends_the_literal_placeholder_for_an_item_that_is_not_yours() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "Ink");

    // The target changes to somebody else's unsigned, inscribable scroll.
    identify(&mut app, &mut peer, NOT_MINE, &profile(None, None));

    assert_eq!(
        box_text(&mut app),
        PLACEHOLDER,
        "the invitation is in the box"
    );
    assert!(
        !box_has_the_caret(&mut app),
        "disabling selection relinquishes the caret on the unowned target"
    );

    let sent = inscriptions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "exactly one 0xBF, from the relinquish that the redraw caused"
    );
    assert_eq!(
        sent[0].object_id, NOT_MINE,
        "the request names the new target"
    );
    assert_eq!(
        sent[0].text, PLACEHOLDER,
        "the request carries the invitation displayed in the box"
    );
    assert_ne!(
        sent[0].text, "Ink",
        "the typed text was cleared before the new invitation was installed"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. The typed text otherwise — the same edge on the player's own item
// ---------------------------------------------------------------------------------------------

/// The ordinary path, so the test above is a statement about the target-change case. Escape on
/// the player's own parchment commits what was typed, to that object, and not the placeholder.
#[test]
fn the_ordinary_commit_carries_the_typed_text_and_never_the_placeholder() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "For Asheron");
    press_escape(&mut app);

    let sent = inscriptions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "losing focus sends exactly one inscription request"
    );
    assert_eq!(sent[0].object_id, MINE);
    assert_eq!(
        sent[0].text, "For Asheron",
        "the typed text, not the invitation"
    );
}

/// **A 3-D press does not commit.** Pressing empty world does not commit the inscription:
/// `<SBOX>` itself is the mouse hit, so the manager's no-hit focus-drop arm is not taken. Base
/// mouse delivery precedes action 7; unless mouse-look consumes the press, primary selection is
/// armed. Neither path emits an inscription or focus notice.
///
/// The test checks an increased missed-pick counter, unchanged selection, retained
/// caret and text, and no inscription request. Escape on the same box remains the positive
/// control: it drops focus and sends exactly the typed text, excluding a broken sender as the
/// explanation for the world-press result.
#[test]
fn an_empty_world_press_keeps_the_inscription_focused_and_escape_commits_it() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "Still wet");
    assert_eq!(
        app.objects().world.selected,
        None,
        "the empty-world control starts unselected"
    );

    press_empty_world_view(&mut app);

    assert_eq!(
        app.objects().world.selected,
        None,
        "the completed empty pick selects nothing"
    );
    assert!(
        box_has_the_caret(&mut app),
        "a hit on <SBOX> is not the no-hit focus-drop arm"
    );
    assert_eq!(
        box_text(&mut app),
        "Still wet",
        "the world press preserves the typed text"
    );
    assert!(
        inscriptions_sent(&mut app).is_empty(),
        "a 3-D press emits no 0xBF by itself"
    );

    press_escape(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    let sent = inscriptions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "the ordinary losing-focus edge still emits exactly one 0xBF"
    );
    assert_eq!(
        (sent[0].object_id, sent[0].text.as_str()),
        (MINE, "Still wet")
    );
    assert_ne!(
        sent[0].text, PLACEHOLDER,
        "the positive control commits no invitation text"
    );
}

// ---------------------------------------------------------------------------------------------
// 3. The three target changes that send nothing
// ---------------------------------------------------------------------------------------------

/// A target signed by another scribe fills the box and stored inscription from string
/// property 7. Focus is still relinquished, but the equality check suppresses the request.
#[test]
fn a_signed_item_that_is_not_yours_relinquishes_and_still_sends_nothing() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "Ink");
    identify(
        &mut app,
        &mut peer,
        SIGNED_NOT_MINE,
        &profile(Some("Aldwyne"), Some("Bob's")),
    );

    assert_eq!(
        box_text(&mut app),
        "Bob's",
        "the signed inscription is in the box"
    );
    assert!(!box_has_the_caret(&mut app), "and the caret still goes");
    assert!(
        inscriptions_sent(&mut app).is_empty(),
        "the box matches the stored inscription, so nothing is sent"
    );
}

/// A non-inscribable target clears and hides the box without refilling it. At focus loss,
/// both text and scribe are empty, so no request is sent.
///
/// The losing-focus tail then restores the invitation and placeholder state `0x10000050`
/// behind the hidden box, so the box is not blank: it keeps the unseen invitation. The test
/// checks that text as well as the absence of a request.
#[test]
fn a_thing_you_cannot_write_on_at_all_sends_nothing() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "Ink");
    identify(&mut app, &mut peer, PLAIN_NOT_MINE, &profile(None, None));

    assert!(
        inscriptions_sent(&mut app).is_empty(),
        "an empty box on an unsigned item produces no inscription request"
    );
    assert_eq!(
        box_text(&mut app),
        PLACEHOLDER,
        "the losing-focus tail restores the invitation behind the hidden box"
    );
}

/// Another owned, unsigned target keeps editability and selectability true. Both property
/// updates take their unchanged-value early exits, so the invitation replaces the displayed
/// text while the caret stays put. No focus-loss request is emitted: the placeholder request
/// requires the ordinary unowned-target transition, rather than every target change.
#[test]
fn another_item_of_your_own_keeps_the_caret_and_sends_nothing() {
    let (mut app, mut peer) = setup();
    writing_on_his_own(&mut app, &mut peer, "Ink");
    identify(&mut app, &mut peer, ALSO_MINE, &profile(None, None));

    assert_eq!(
        box_text(&mut app),
        PLACEHOLDER,
        "the new owned parchment displays the invitation"
    );
    assert!(
        box_has_the_caret(&mut app),
        "unchanged editability and selectability preserve the caret"
    );
    assert!(
        inscriptions_sent(&mut app).is_empty(),
        "no relinquish, no commit edge, no 0xBF"
    );
}

/// The refusal background is mouse-visible only while inscription text is locked. These
/// hits reach exact channel `0x1A` refusal strings in the pending/item spew model, not pixel
/// readback. While an appraisal is pending, refusal uses the displayed item's accepted facts.
/// A nonzero equipment location is silent; an editable item does not expose the background.
#[test]
fn locked_inscription_background_explains_why_the_item_cannot_be_edited() {
    let (mut app, mut peer) = setup();

    identify(
        &mut app,
        &mut peer,
        SIGNED_NOT_MINE,
        &profile(Some("Aldwyne"), Some("Bob's")),
    );
    click_inscription_background(&mut app);
    assert!(
        visible_spew(&app)
            .iter()
            .any(|s| s == "Only Aldwyne can change the inscription"),
        "the signed-item refusal names the scribe, not the object"
    );

    identify(&mut app, &mut peer, NOT_MINE, &profile(None, None));
    let notices_before_pending = app.interaction().stats.panel_notice_strings;
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(dereth_ui_screens::view::UiRequest::Examine(PLAIN_NOT_MINE));
    app.frame();
    app.frame();
    click_inscription_background(&mut app);
    assert_eq!(
        app.interaction().stats.panel_notice_strings,
        notices_before_pending + 1,
        "the displayed item's refusal crosses the panel-notice boundary"
    );
    assert!(
        visible_spew(&app)
            .iter()
            .any(|s| s == "Item must be in your inventory to inscribe."),
        "the unanswered non-inscribable target does not replace the displayed ground item's facts"
    );
    assert!(
        !visible_spew(&app)
            .iter()
            .any(|s| s == "This item is not inscribable."),
        "pending target facts must remain unused until its appraisal reply is accepted"
    );

    identify(
        &mut app,
        &mut peer,
        SIGNED_BY_ME_NOT_MINE,
        &profile(Some("Larktest"), Some("Mine")),
    );
    click_inscription_background(&mut app);
    assert!(
        visible_spew(&app)
            .iter()
            .any(|s| s == "Item must be in your inventory to inscribe."),
        "even the matching scribe must still carry the item"
    );

    identify(&mut app, &mut peer, PLAIN_NOT_MINE, &profile(None, None));
    click_inscription_background(&mut app);
    assert!(
        visible_spew(&app)
            .iter()
            .any(|s| s == "This item is not inscribable."),
        "the inscribable-flag refusal keeps its exact punctuation"
    );

    identify(&mut app, &mut peer, NOT_MINE, &profile(None, None));
    click_inscription_background(&mut app);
    assert!(
        visible_spew(&app)
            .iter()
            .any(|s| s == "Item must be in your inventory to inscribe."),
        "an unsigned inscribable ground item explains the inventory gate"
    );

    let before_equipped = visible_spew(&app);
    identify(&mut app, &mut peer, EQUIPPED_NOT_MINE, &profile(None, None));
    click_inscription_background(&mut app);
    assert_eq!(
        visible_spew(&app),
        before_equipped,
        "a live nonzero equipment location produces no additional refusal"
    );

    let before = visible_spew(&app);
    identify(&mut app, &mut peer, MINE, &profile(None, None));
    let background_mouse_visible = {
        let ui = &app.ui().expect("the shell").ui;
        ui.get_element(INSCRIPTION_BACKGROUND)
            .and_then(|h| ui.node(h))
            .is_some_and(|n| n.is_mouse_visible)
    };
    assert!(
        !background_mouse_visible,
        "the editable inscription does not expose the refusal background to the mouse"
    );
    assert_eq!(
        visible_spew(&app),
        before,
        "an editable inscription emits no refusal"
    );
}

/// After the live object's inscribable-mask gate, privilege can bypass failed ordinary
/// scribe/ownership checks. The predicate ORs private Boolean qualities `0x2C`, `0x2D`, `0x61`.
/// Each quality is tested independently, then the last is cleared to restore the lock.
#[test]
fn the_three_psr_qualities_unlock_only_a_live_inscribable_object() {
    let (mut app, mut peer) = setup();

    // Install a synthetic player description through the production 0x0013 consumer.
    // Typed private-update bodies are then dispatched directly to the HUD event entry point;
    // their accepted values are checked in the authoritative World quality store.
    app.apply_hud_events(&[SessionEvent::PlayerDescription(Box::new(
        dereth_protocol::login::LoginPlayerDescription::default(),
    ))]);

    let signed = profile(Some("Aldwyne"), Some("Bob's"));
    identify(&mut app, &mut peer, SIGNED_NOT_MINE, &signed);
    assert!(
        !box_is_editable(&app),
        "an ordinary player cannot edit another scribe's item"
    );

    for property_id in [0x2C, 0x2D, 0x61] {
        set_psr_quality(&mut app, 1, property_id, true);
        identify(&mut app, &mut peer, SIGNED_NOT_MINE, &signed);
        assert!(
            box_is_editable(&app),
            "privilege Boolean {property_id:#04X} reaches the shipped text editability property"
        );
        if property_id != 0x61 {
            set_psr_quality(&mut app, 2, property_id, false);
        }
    }

    click_the_box(&mut app);
    assert!(
        box_has_the_caret(&mut app),
        "the now-editable shipped text takes real pointer focus"
    );
    type_into_box(&mut app, " PSR edit");
    press_escape(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    let sent = inscriptions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "the privileged physical edit commits exactly once"
    );
    assert_eq!(
        (sent[0].object_id, sent[0].text.as_str()),
        (SIGNED_NOT_MINE, "Bob's PSR edit"),
        "an existing inscription is edited in place rather than cleared on focus"
    );

    set_psr_quality(&mut app, 2, 0x61, false);
    identify(&mut app, &mut peer, SIGNED_NOT_MINE, &signed);
    assert!(
        !box_is_editable(&app),
        "removing the last PSR bit restores the ordinary lock"
    );

    // Hook appraisal data determines displayed text, but editability first requires mask
    // 0x2 in the live object's description. Privilege cannot turn hook-only display data
    // into write permission when that live flag is absent.
    set_psr_quality(&mut app, 3, 0x61, true);
    let mut hook_only = profile(Some("Aldwyne"), Some("Hook text"));
    hook_only.flags |= dereth_protocol::types::appraisal::flags::HOOK_PROFILE;
    hook_only.hook_profile = Some(dereth_protocol::types::appraisal::HookAppraisalProfile {
        bitfield: dereth_client_model::appraisal::hook_appraisal::INSCRIBABLE,
        ..Default::default()
    });
    identify(&mut app, &mut peer, PLAIN_NOT_MINE, &hook_only);
    assert_eq!(
        box_text(&mut app),
        "Hook text",
        "hook appraisal data still controls display"
    );
    assert!(
        !box_is_editable(&app),
        "a missing live object-description inscribable bit remains locked even for a PSR"
    );
    assert!(
        inscriptions_sent(&mut app).is_empty(),
        "the hook-only control sends nothing"
    );
}
