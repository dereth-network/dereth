//! The house purchase and maintenance window: a `House_HouseProfile 0x021D` (the answer to an
//! ordinary `Inventory_UseEvent 0x0036` on a slumlord) raises it with the profile's buy and rent
//! rows, the tabs decide which drops are payments, only carried items are accepted, Buy needs the
//! price paid in full, rows can be dragged back out, partial stacks go through a split, Buy and
//! proxy Rent ask for confirmation (apartments pay on the click), the panel closes beyond nine
//! units, and a failed transaction re-queries the lord (`House_QueryLord 0x0258`) only with a
//! profile. `0x0258` is the retry after a refused transaction, never the opener.
//! Fixture: a recorded 260-byte `0x021D` body and the recorded `0x021C` payment and `0x02EB`
//! refusal bytes, the retail dats, and a headless App on a socket-free replay endpoint (nothing
//! leaves the process; nothing is bought, rented or abandoned).

#![cfg(gpu)]

use crate::common::client_dir_required as client_dir;
use crate::common::gpu_lock;

use dereth_client::app::App;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::Opcode;
use dereth_transport::wire::ParsedPacket;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::slumlord;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const PLAYER: ObjectId = ObjectId(0x5000_0001);

/// The slumlord statue of the recorded villa — `0x9DAF0029`'s landblock-static
/// object, `0x7` + landblock `0x9DAF` + index `0x03A`. It is the first dword of the recorded
/// `0x0036` **and** of the recorded `0x021D`.
const SLUMLORD: ObjectId = ObjectId(0x79DA_F03A);

/// The three items the recorded retail client put on the wire at `t = 144.458`, in that order.
const RECORDED_ITEMS: [ObjectId; 3] = [
    ObjectId(0x8000_15C2),
    ObjectId(0x8000_15C1),
    ObjectId(0x8000_1592),
];

/// **The recorded `0x021D House_HouseProfile`, verbatim** — 260 bytes at `t = 29.090`, the
/// body of an `0xF7B0` event blob after its 16-byte `OrderedEventHeader`.
///
/// It decodes to: covenant crystal `0x79DAF03A`, dwelling `0x0592`, owner **0** (unowned),
/// bitmask 1 (`Active`), min level **35**, `max_level` / `min_alleg_rank` / `max_alleg_rank` all
/// `-1`, `maintenance_free` 0, type **2 (Villa)**, an **empty** owner name, three buy lines and
/// two rent lines. The twelve fields appear in the order written by ACE and decoded by the
/// retail client.
const RECORDED_PROFILE: &str = "\
    3af0da7992050000000000000100000023000000ffffffffffffffffffffffff000000000200000000000000\
    0300000080841e000000000011010000060050797265616c070050797265616c730000000500000000000000\
    be2d00000e0057726974206f66205265667567650f005772697473206f662052656675676500000001000000\
    00000000ff0100000e004372756465204c6f636b7069636b0f004372756465204c6f636b7069636b73000000\
    02000000a08601000000000011010000060050797265616c070050797265616c730000000200000000000000\
    be2d00000e0057726974206f66205265667567650f005772697473206f6620526566756765000000";

/// The WCID of Pyreal, used by the paid-items coin row.
const PYREAL: u32 = 273;
const SPLIT_RESULT: ObjectId = ObjectId(0x5000_0044);
const FIXTURE_SLUM_LORD: ObjectId = ObjectId(0x8000_9001);
const OP_GAME_ACTION: u32 = 0xF7B1;
const AC_SPLIT_TO_CONTAINER: u32 = 0x0055;
/// `Writ of Refuge`, the second buy line's weenie class (`be2d0000` = 11710).
const WRIT: u32 = 11_710;

fn hex(s: &str) -> Vec<u8> {
    let s: String = s.chars().filter(|c| !c.is_whitespace()).collect();
    (0..s.len() / 2)
        .map(|i| u8::from_str_radix(&s[i * 2..i * 2 + 2], 16).expect("hex"))
        .collect()
}

/// A deliberately small price, carried through the real `0x021D` decoder. The recorded profile
/// remains the structural oracle; the owner, owner name, house type, payment-list lengths and
/// first payment counts are changed so a pointer test does not need thirty-one maximum-size Pyreal
/// stacks to enable Buy.
fn confirmation_profile(owner: ObjectId, house_type: u32) -> Vec<u8> {
    let mut message: dereth_protocol::trade::HouseProfileMessage =
        dereth_protocol::read_body(&hex(RECORDED_PROFILE)).expect("the recorded profile decodes");
    message.profile.owner = owner;
    message.profile.name = if owner.0 == 0 {
        String::new()
    } else {
        "Other Owner".to_owned()
    };
    message.profile.house_type = house_type;
    message.profile.buy.truncate(1);
    message.profile.buy[0].num = 1;
    message.profile.rent.truncate(1);
    message.profile.rent[0].num = 1;
    dereth_protocol::write_body(&message).expect("the confirmation fixture encodes")
}

fn split_profile(amount: i32) -> Vec<u8> {
    let mut message: dereth_protocol::trade::HouseProfileMessage =
        dereth_protocol::read_body(&hex(RECORDED_PROFILE)).expect("the recorded profile decodes");
    message.profile.buy.truncate(1);
    message.profile.buy[0].num = amount;
    message.profile.rent.truncate(1);
    dereth_protocol::write_body(&message).expect("the split fixture encodes")
}

fn owner_payment_profile() -> Vec<u8> {
    let mut message: dereth_protocol::trade::HouseProfileMessage =
        dereth_protocol::read_body(&hex(RECORDED_PROFILE)).expect("the recorded profile decodes");
    message.profile.buy.truncate(2);
    message.profile.buy[0].num = 100_003;
    message.profile.buy[1].num = 1;
    message.profile.rent.truncate(1);
    dereth_protocol::write_body(&message).expect("the owner-payment fixture encodes")
}

// ---------------------------------------------------------------------------------------------
// Harness — a replay `Peer` that feeds ordered events and object blobs
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
            "127.0.0.1:19100",
            7304,
            "house-purchase-window-station",
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
            Some("127.0.0.1:19100".parse().expect("addr")),
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

    fn event(&mut self, app: &mut App, opcode: Opcode, body: &[u8]) {
        self.stamp += 1;
        let mut blob = 0xF7B0_u32.to_le_bytes().to_vec();
        blob.extend_from_slice(&PLAYER.0.to_le_bytes());
        blob.extend_from_slice(&self.stamp.to_le_bytes());
        blob.extend_from_slice(&opcode.0.to_le_bytes());
        blob.extend_from_slice(body);
        self.send(app, 9, blob);
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
}

/// The trade-note check asks for enum slot 10 in dual-DID mapper `0x10000001`: the shared two-hop
/// resolver returns a payload that the requested type can decode, carrying the 100 and 100,000
/// trade-note values.
#[test]
fn the_native_trade_note_enum_resolves_the_typed_mapper() {
    use dereth_assets::{Decode, DualDidMapper};
    use dereth_dat::{DbType, RetailDatStore};

    let store = RetailDatStore::open_dir(&client_dir()).expect("retail dats");
    let id =
        dereth_assets::did_by_enum(&store, 0x1000_0001, 10).expect("the TradeNotes enum entry");
    let bytes = store
        .read_typed(DbType::DualDidMapper, id)
        .expect("the typed TradeNotes mapper payload");
    let mapper = DualDidMapper::decode_payload(id, &bytes).expect("DualDidMapper");
    assert!(mapper.0.enum_to_id.iter().any(|(value, _)| *value == 100));
    assert!(mapper
        .0
        .enum_to_id
        .iter()
        .any(|(value, _)| *value == 100_000));
    eprintln!("TradeNotes mapper resolved to {id:?}");
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

fn settle_house_requests(app: &mut App, wire: &mut Vec<dereth_client_model::Request>) {
    use dereth_client_model::Request as R;
    for _ in 0..6 {
        app.frame();
        wire.extend(
            app.interaction()
                .last_sent
                .iter()
                .filter(|request| matches!(request, R::BuyHouse(_) | R::RentHouse(_)))
                .cloned(),
        );
    }
}

fn setup(tag: &str) -> (App, Peer) {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir().join(format!(
            "dereth-house-purchase-window-{tag}-not-created/prefs.ini"
        )),
        ..Config::default()
    })
    .expect("an application");
    app.start_shell().expect("the shell starts");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("a static scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net)
        .expect("the replay endpoint attaches");

    let mut p = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(p)).expect("blob"),
    );
    app.frame();
    app.probe_mut().objects_mut().world.player = Some(PLAYER);
    app.probe_mut()
        .objects_mut()
        .world
        .weenie_mut(PLAYER)
        .expect("the player was created")
        .pwd
        .name = "Larktest".to_string();
    peer.event(
        &mut app,
        Opcode::LOGIN_PLAYER_DESCRIPTION,
        &dereth_protocol::write_body(&dereth_protocol::login::LoginPlayerDescription::default())
            .expect("an empty description encodes"),
    );
    settle(&mut app);
    (app, peer)
}

fn place_slumlord_at_player(
    app: &mut App,
    peer: &mut Peer,
    slumlord: ObjectId,
) -> dereth_primitives::Position {
    let here = app
        .world_state()
        .and_then(|scene| scene.character.as_ref())
        .expect("the local physics body")
        .position();
    let mut p = ObjectCreatePayload {
        id: slumlord,
        ..Default::default()
    };
    p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP
        | dereth_protocol::types::physicsdesc::flags::POSITION;
    p.physicsdesc.setup_id = Some(0x0200_0001);
    p.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: here.frame.origin.into(),
            orientation: here.frame.rotation.into(),
        },
    });
    p.physicsdesc.timestamps.instance = 2;
    p.physicsdesc.timestamps.position = 1;
    p.wdesc.name = "Villa Covenant Crystal".to_owned();
    peer.send(
        app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(p)).expect("blob"),
    );
    settle(app);
    assert!(
        app.world_scene()
            .and_then(|scene| scene.server_object_position(slumlord))
            .is_some(),
        "the range fixture has a real slumlord physics body"
    );
    here
}

/// Functional housing journeys need the profile's watched covenant crystal to have a real body.
/// Otherwise the missing-body rule closes the panel at its first one-second range poll, which
/// turns a slow pointer journey into a fixture race. Codec and captured-wire tests use
/// [`SLUMLORD`] and [`RECORDED_PROFILE`] unchanged.
fn deliver_profile_at_player(app: &mut App, peer: &mut Peer, body: &[u8]) -> ObjectId {
    place_slumlord_at_player(app, peer, FIXTURE_SLUM_LORD);
    let mut profile: dereth_protocol::trade::HouseProfileMessage =
        dereth_protocol::read_body(body).expect("the functional house profile decodes");
    profile.covenant_crystal = FIXTURE_SLUM_LORD;
    peer.event(
        app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &dereth_protocol::write_body(&profile).expect("the functional house profile encodes"),
    );
    FIXTURE_SLUM_LORD
}

/// One item, with the container link that the ownership check walks toward the player.
///
/// `mine == false` leaves the item with no container link. That represents an item outside the
/// player's pack for this ownership arm; it does not construct a chest, corpse or ground holder.
fn give_item(app: &mut App, id: ObjectId, wcid: u32, stack: u16, mine: bool) {
    let mut w = dereth_client_model::Weenie::new(id);
    w.valid = true;
    w.pwd.wcid = wcid;
    w.pwd.name = format!("thing {:X}", id.0);
    w.pwd.icon_id = 0x0600_1234;
    w.pwd.stack_size = Some(stack);
    w.pwd.container_id = mine.then_some(PLAYER);
    app.probe_mut()
        .objects_mut()
        .world
        .tables
        .weenies
        .insert(id, w);
}

fn give_carried_stack(app: &mut App, id: ObjectId, stack: u16) {
    give_carried_item(app, id, PYREAL, "Pyreal", "Pyreals", stack);
}

fn give_carried_item(
    app: &mut App,
    id: ObjectId,
    wcid: u32,
    name: &str,
    plural_name: &str,
    stack: u16,
) {
    let world = &mut app.probe_mut().objects_mut().world;
    if world.tables.inventories.get(PLAYER).is_none() {
        world.tables.inventories.insert(
            PLAYER,
            dereth_client_model::objects::ObjectInventory::new(PLAYER),
        );
    }
    world
        .tables
        .inventories
        .get_mut(PLAYER)
        .expect("the player inventory")
        .add_content(id, false, 0);
    let player = world.weenie_mut(PLAYER).expect("the player exists");
    player.pwd.bitfield |= dereth_client_model::weenie::bitfield::OPENABLE;
    player.pwd.items_capacity = Some(102);
    player.pwd.containers_capacity = Some(7);
    give_item(app, id, wcid, stack, true);
    let item = app
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(id)
        .expect("the item exists");
    item.pwd.name = name.to_owned();
    item.pwd.plural_name = Some(plural_name.to_owned());
    item.pwd.max_stack_size = Some(stack);
}

fn gameplay(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell is up");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen");
    (ui, screen)
}

fn open_inventory(app: &mut App) {
    let panel_id = {
        let (_, screen) = gameplay(app);
        screen
            .panels
            .pages
            .iter()
            .find(|q| q.element == ElementId(0x1000_018B))
            .map(|q| q.panel_id)
            .expect("the inventory page is in the panel stack")
    };
    let (ui, screen) = gameplay(app);
    screen.recv_set_panel_visibility(ui, panel_id, true);
    settle(app);
}

fn centre(ui: &UiSystem, h: ElemHandle) -> (i32, i32) {
    let (ox, oy) = ui.screen_origin(h);
    let b = ui.node(h).expect("alive").region.box_;
    (ox + b.width() / 2, oy + b.height() / 2)
}

fn grid_slot_of(app: &mut App, item: ObjectId) -> ElemHandle {
    let (_, screen) = gameplay(app);
    screen
        .inventory
        .item_list
        .as_ref()
        .expect("the grid")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} is in the shipped grid"))
        .handle
}

fn house_slot(app: &App) -> ElemHandle {
    let list = panel(app).buy_list.as_ref().expect("the buy list is bound");
    let used = list.slots.iter().filter(|s| s.item.is_some()).count();
    list.slots.get(used).expect("an empty buy slot").handle
}

fn house_row(app: &App, item: ObjectId) -> ElemHandle {
    panel(app)
        .buy_list
        .as_ref()
        .expect("the buy list is bound")
        .slots
        .iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("{item:?} has a visible housing-payment row"))
        .handle
}

fn pick_up(app: &mut App, from: ElemHandle, t: f64) {
    let (ui, _) = gameplay(app);
    let (x, y) = centre(ui, from);
    ui.mouse_move(LocalTime(t), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_move(LocalTime(t + 0.05), x + 8, y + 8);
    app.frame();
    assert!(
        gameplay(app).0.drag_state().element.is_some(),
        "a real drag proxy exists"
    );
}

fn drop_on_buy_list(app: &mut App, item: ObjectId, t: f64) {
    let from = grid_slot_of(app, item);
    let to = house_slot(app);
    pick_up(app, from, t);
    let (x, y) = {
        let (ui, _) = gameplay(app);
        let at = centre(ui, to);
        ui.mouse_move(LocalTime(t + 0.1), at.0, at.1);
        let hit = ui
            .hit_test_screen(at.0, at.1)
            .expect("the buy list is under the pointer");
        assert!(
            ui.get_child_recursive(ui.root(), slumlord::BUY_LIST)
                .is_some_and(|list| hit == list || ui.is_ancestor_of(list, hit)),
            "the physical target is inside the housing panel's buy list"
        );
        at
    };
    app.frame();
    gameplay(app)
        .0
        .mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    settle(app);
}

/// Select the source with the pointer, then call the splitter's text handler directly and emit its
/// request. This exercises split parsing and routing, but does not claim a typed-text journey.
fn select_split_quantity(app: &mut App, item: ObjectId, amount: u32) {
    let h = grid_slot_of(app, item);
    let (ui, _) = gameplay(app);
    let (x, y) = centre(ui, h);
    ui.mouse_move(LocalTime(8.0), x, y);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    settle(app);
    let request = gameplay(app).1.splitter.on_text(&amount.to_string()).0;
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .emit(request);
    settle(app);
    assert_eq!(app.objects().world.split.split_size, amount);
}

fn u32_at(bytes: &[u8], offset: usize) -> u32 {
    u32::from_le_bytes(bytes[offset..offset + 4].try_into().expect("a dword"))
}

fn wire(app: &mut App) -> Vec<(u32, Vec<u8>)> {
    let net = app.replay_network_mut().expect("the replay endpoint");
    let mut out = Vec::new();
    for (raw, _) in net.take_outgoing() {
        let Ok(packet) = ParsedPacket::parse(&raw) else {
            continue;
        };
        for fragment in packet.fragments {
            if fragment.payload.len() < 12 || u32_at(&fragment.payload, 0) != OP_GAME_ACTION {
                continue;
            }
            let opcode = u32_at(&fragment.payload, 8);
            if ![0x01BF, 0x0263].contains(&opcode) {
                out.push((opcode, fragment.payload[12..].to_vec()));
            }
        }
    }
    out
}

/// The panel, read back off the live tree.
fn panel(app: &App) -> &slumlord::SlumlordPanel {
    &app.hud().panels.slumlord
}

/// Select the shared operation through the actual authored tab.
fn choose_tab(app: &mut App, rent: bool) {
    choose_physical_tab(app, rent);
}

/// Press the shipped tab which owns one of the two pages. The tab id comes from the live panel
/// table built from the layout's `0x2E` array; no fixture visibility is
/// imposed on either page. An already-selected tab needs no gesture: the housing panel heard the
/// default page's visibility edge while the base panel initialized its `0x2E` table, so its
/// operation already agrees with the selected page.
fn choose_physical_tab(app: &mut App, rent: bool) {
    let page = if rent {
        slumlord::RENT_PAGE
    } else {
        slumlord::BUY_PAGE
    };
    let other = if rent {
        slumlord::BUY_PAGE
    } else {
        slumlord::RENT_PAGE
    };
    let (current, tab) = {
        let ui = &app.ui().expect("the ui shell").ui;
        let root = ui.root();
        let panel = ui
            .get_child_recursive(root, slumlord::PANEL)
            .expect("the housing panel");
        let widget = ui
            .node(panel)
            .and_then(|n| n.behaviour.as_ref())
            .and_then(|b| (**b).as_any())
            .and_then(|a| a.downcast_ref::<dereth_ui::widgets::panel::Panel>())
            .expect("the housing panel has panel behavior");
        let tab = widget
            .tab_to_page
            .iter()
            .find(|(_, p)| **p == page)
            .map(|(t, _)| *t)
            .expect("the shipped 0x2E table names this page's tab");
        (
            widget.open_page,
            ui.get_child_recursive(panel, tab)
                .expect("the shipped tab element"),
        )
    };
    let mut wire = Vec::new();
    if current != Some(page) {
        click_handle(app, tab, &mut wire);
    }
    assert!(wire.is_empty(), "a local tab click sends no game request");
    let ui = &app.ui().expect("the ui shell").ui;
    let selected = ui
        .get_child_recursive(ui.root(), page)
        .expect("the selected page");
    let covered = ui
        .get_child_recursive(ui.root(), other)
        .expect("the other page");
    assert!(ui.node(selected).expect("alive").region.flags.visible);
    assert!(!ui.node(covered).expect("alive").region.flags.visible);
}

/// Enter the panel's drop decision with the real `GameView`.
///
/// This narrow helper bypasses pointer delivery and calls the panel's drop decision directly.
/// Separate physical journeys in this file create a live drag proxy and drop through the Buy-list
/// control. Everything this helper *decides* — player ownership, payment eligibility, remaining
/// balance, row insertion, refusal strings and button states — is production.
fn drop_on_the_window(app: &mut App, id: ObjectId) -> bool {
    let out = app
        .with_panels(|ui, panels, view| panels.slumlord.accept_drag_object(ui, id, view))
        .expect("a ui shell");
    out
}

/// …and the frames that carry whatever it emitted into the game.
fn drop_and_settle(app: &mut App, id: ObjectId) -> bool {
    let out = drop_on_the_window(app, id);
    settle(app);
    out
}

/// Click a shipped control through screen hit-testing and the actual mouse press/release path.
fn click_handle(
    app: &mut App,
    h: dereth_ui::ElemHandle,
    wire: &mut Vec<dereth_client_model::Request>,
) {
    let shell = app.ui_mut().expect("the ui shell");
    let r = shell.ui.screen_clip_box(h);
    assert!(r.is_valid(), "the control has a visible clip box");
    let (x, y) = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
    let hit = shell.ui.hit_test_screen(x, y);
    let mut at = Some(h);
    let mut chain = Vec::new();
    while let Some(node) = at.and_then(|handle| shell.ui.node(handle)) {
        chain.push((node.element_id(), node.region.flags.visible));
        at = node.region.parent;
    }
    assert!(
        hit.is_some_and(|hit| hit == h || shell.ui.is_ancestor_of(h, hit)),
        "the control at {r:?} is reachable through its visible ancestors; hit={:?}, chain={chain:?}",
        hit.and_then(|handle| shell.ui.node(handle).map(|node| node.element_id()))
    );
    shell.ui.mouse_move(LocalTime(2.0), x, y);
    shell.ui.mouse_down(7, x, y);
    shell.ui.mouse_up(7, x, y, false);
    settle_house_requests(app, wire);
}

fn button(app: &App, id: dereth_ui::ElementId) -> dereth_ui::ElemHandle {
    let ui = &app.ui().expect("the ui shell").ui;
    ui.get_child_recursive(ui.root(), id)
        .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"))
}

fn open_house_dialog(app: &App) -> (u64, dereth_ui::ElemHandle) {
    let ui = &app.ui().expect("the ui shell").ui;
    let info = ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
        .expect("the house question is open on the default dialog queue");
    assert_eq!(info.kind, dereth_ui::dialog::DialogKind::Confirmation);
    (
        info.context,
        info.element.expect("the shipped confirmation-dialog tree"),
    )
}

/// The lines this window has emitted, taken **out of the outbox before a frame carries them away**.
///
/// A real frame runs the HUD driver and moves every scroll line into the chat window on the tick
/// it arrives. The outbox is where the panel first puts them, while the
/// `Interaction::stats::panel_notice_strings` counter is the far-side observation; both are
/// asserted.
fn emitted(app: &mut App) -> Vec<(u32, String)> {
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .take()
        .into_iter()
        .filter_map(|r| match r {
            dereth_ui_screens::view::UiRequest::DisplayChatText { channel, text, .. } => {
                Some((channel, text))
            }
            _ => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------------------------
// Station 1 — the recorded payload, decoded
// ---------------------------------------------------------------------------------------------

/// **260 recorded bytes decode field by field and re-encode byte for byte.**
///
/// A self-consistent round trip cannot catch a wrong field order; this oracle is bytes a shard
/// actually sent, decoded field by field against what the capture means.
#[test]
fn the_recorded_villa_profile_decodes_field_by_field_and_re_encodes_byte_for_byte() {
    let bytes = hex(RECORDED_PROFILE);
    assert_eq!(
        bytes.len(),
        260,
        "the recorded blob body, after the 16-byte wire-order header"
    );
    let m: dereth_protocol::trade::HouseProfileMessage =
        dereth_protocol::read_body(&bytes).expect("the recorded 0x021D decodes");
    let p = &m.profile;
    assert_eq!(
        m.covenant_crystal, SLUMLORD,
        "GameEventHouseProfile's first dword is the slumlord"
    );
    assert_eq!(p.id.0, 0x0592, "DwellingID 1426");
    assert_eq!(
        p.owner.0, 0,
        "nobody owned it, which is why the Buy tab is the live one"
    );
    assert_eq!(p.bitmask, 1, "recorded house bitfield has the Active flag");
    assert_eq!(p.min_level, 35);
    assert_eq!(
        (p.max_level, p.min_alleg_rank, p.max_alleg_rank),
        (-1, -1, -1),
        "ACE's HouseProfile() constructor sets all four to -1 and only MinLevel was overridden; \
         a u32 reading of these is 4294967295 rather than `none`"
    );
    assert_eq!(p.maintenance_free, 0);
    assert_eq!(p.house_type, 2, "recorded house type is Villa");
    assert_eq!(
        p.name, "",
        "WriteString16L(null) is a zero length and two bytes of pad"
    );
    assert_eq!(p.buy.len(), 3);
    assert_eq!(p.rent.len(), 2);

    let line = |p: &dereth_protocol::trade::HousePayment| (p.num, p.paid, p.wcid, p.name.clone());
    assert_eq!(line(&p.buy[0]), (2_000_000, 0, PYREAL, "Pyreal".into()));
    assert_eq!(p.buy[0].plural_name, "Pyreals");
    assert_eq!(line(&p.buy[1]), (5, 0, WRIT, "Writ of Refuge".into()));
    assert_eq!(line(&p.buy[2]), (1, 0, 511, "Crude Lockpick".into()));
    assert_eq!(line(&p.rent[0]), (100_000, 0, PYREAL, "Pyreal".into()));
    assert_eq!(line(&p.rent[1]), (2, 0, WRIT, "Writ of Refuge".into()));

    // Re-encoding must reproduce the shard's own bytes, which is the half a round trip over our
    // own writer cannot prove.
    assert_eq!(
        dereth_protocol::write_body(&m).expect("re-encodes"),
        bytes,
        "byte for byte"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 2 — the window opens
// ---------------------------------------------------------------------------------------------

/// **A `0x021D` raises the window.**
///
/// The profile receiver updates the house state and then makes the window visible. The three
/// assertions separate the three ways that can fail: the receiver never counted it, the panel
/// never bound, or the panel bound and did not show.
#[test]
fn a_house_profile_raises_the_purchase_window() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("raise");

    assert!(
        panel(&app).bound(),
        "housing root 0x10000060 and both item lists are bound"
    );
    assert!(
        !panel(&app).visible,
        "environment-panel child setup hides all five pages"
    );
    assert_eq!(panel(&app).opens, 0);

    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);

    assert_eq!(
        app.hud().stats.house_profile_notices,
        1,
        "the 0x021D receiver ran"
    );
    assert_eq!(
        panel(&app).opens,
        1,
        "the house-profile receiver opened the window once"
    );
    assert!(
        panel(&app).visible,
        "profile delivery made the panel visible"
    );
    assert_eq!(
        panel(&app)
            .profile
            .as_ref()
            .map(|p| (p.slumlord, p.owner, p.house_type)),
        Some((SLUMLORD, ObjectId(0), 2)),
        "the decoded profile kept its owner and house type"
    );
}

/// Profile handling registers the covenant crystal at a nine-unit radius. The production
/// frame's range driver must leave the
/// window up while the two real physics bodies remain together, close it after the local body
/// moves beyond the radius, and allow a later profile to arm and raise it again.
#[test]
fn a_house_profile_closes_after_real_movement_beyond_nine_units_and_reopens() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("range-close");
    // Build the exact table split that physics synchronization preserves when the
    // physics world already owns an id: a pre-existing static body under the recorded
    // crystal id, deliberately
    // absent from the parallel server-object handle map. The raw 0x9DAF DAT scene does not create
    // 0x79DAF03A, so this is a focused object-lookup seam control rather than a claim that it does.
    let crystal = {
        let mut scene = app.world_scene_mut().expect("the scene");
        let character = scene.character.as_mut().expect("the local physics body");
        let player = character
            .world
            .get(character.handle)
            .expect("the player body");
        let geometry = std::sync::Arc::clone(&player.geometry);
        let position = player.position;
        let handle = character.world.create(SLUMLORD, geometry, false);
        character.world.force_into_cell(handle, &position);
        *character
            .world
            .get(handle)
            .expect("the static crystal is alive")
            .position()
    };
    assert!(
        app.objects().physics.handle(SLUMLORD).is_none(),
        "the pre-existing crystal is outside ObjectPhysics's server-object handle table"
    );
    let mut here = crystal;
    here.frame.origin.x += 2.0;
    app.probe_mut()
        .world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(here);
    settle(&mut app);
    let profile = hex(RECORDED_PROFILE);

    peer.event(&mut app, Opcode::HOUSE_HOUSE_PROFILE, &profile);
    settle(&mut app);
    let armed = app
        .objects()
        .world
        .object_range_checks
        .live()
        .find(|e| e.handler == dereth_client_model::range::RangeHandler::Slumlord)
        .copied()
        .expect("the profile receiver armed the housing panel's range handler");
    assert_eq!(armed.object, SLUMLORD);
    assert!((armed.range - 9.0).abs() < f64::EPSILON);
    assert!(armed.use_radii && !armed.ignore_z_delta);

    for _ in 0..40 {
        app.frame();
    }
    assert!(
        panel(&app).visible,
        "staying beside the slumlord does not close the window"
    );

    let mut away = here;
    away.frame.origin.x += 12.0;
    app.probe_mut()
        .world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(away);
    for _ in 0..40 {
        app.frame();
    }
    assert!(
        !panel(&app).visible,
        "the actual range-exit notice hid the housing panel"
    );
    assert!(
        !app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Slumlord, SLUMLORD),
        "the one-shot edge retired the watch"
    );

    app.probe_mut()
        .world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(here);
    peer.event(&mut app, Opcode::HOUSE_HOUSE_PROFILE, &profile);
    settle(&mut app);
    assert!(
        panel(&app).visible,
        "a later authoritative profile reopens the window"
    );
    assert!(
        app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Slumlord, SLUMLORD),
        "and re-arms the same watch"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 3 — the rows, off the tree
// ---------------------------------------------------------------------------------------------

/// **The refresh path's four writes, read back out of the elements.**
///
/// The asymmetry is retail's: the buy tab uses the quantity formatter
/// (`"<num> <name>"`) and the rent tab uses the paid-total formatter
/// (`"<paid>/<num> <name>"`). Both owner lines receive the same formatted string.
#[test]
fn the_two_tabs_show_the_price_the_rent_and_the_owner() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("rows");
    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);

    let handles = [
        panel(&app).buy_requirements,
        panel(&app).rent_requirements,
        panel(&app).buy_owner,
        panel(&app).rent_owner,
    ];
    let shell = app.ui_mut().expect("the ui shell");
    let text: Vec<String> = handles
        .into_iter()
        .map(|h| {
            h.and_then(|h| shell.ui.text_element_mut(h))
                .map_or_else(String::new, |t| t.glyphs.inq_text(false))
        })
        .collect();

    assert_eq!(
        text[0], "2000000 Pyreals, 5 Writs of Refuge, 1 Crude Lockpick",
        "the buy formatter joins `<num> <name>` rows with `, `, uses the singular at one, and otherwise uses the shard's plural"
    );
    assert_eq!(
        text[1], "0/100000 Pyreals, 0/2 Writs of Refuge",
        "the rent formatter uses `<paid>/<num> <name>`, unlike the buy formatter"
    );
    assert_eq!(
        text[2], "Owner: None",
        "the empty owner name selects the owner-none text"
    );
    assert_eq!(
        text[3], text[2],
        "one formatted owner string is written to both owner elements"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 4 — the mode, and the refusals that turn on it
// ---------------------------------------------------------------------------------------------

/// **The shipped `0x2E` table opens Buy by default.** The base panel raises that page's
/// visibility edge during initialization and again when the hidden panel is shown; the derived
/// housing listener turns it into the Buy operation and its item list before the first drop.
///
/// On an **unowned** dwelling, the Rent tab is the one that refuses:
/// the payment-allowance test answers `owner == 0` for Buy and `owner != 0` for Rent, so on
/// this villa exactly one of the two tabs can take a drop.
#[test]
fn the_tab_decides_which_drops_are_possible_at_all() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("mode");
    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);
    give_item(&mut app, ObjectId(0x8000_0001), PYREAL, 5_000, true);
    settle(&mut app);

    assert_eq!(
        panel(&app).op,
        slumlord::HouseOp::Buy,
        "the authored default page is usable without first toggling through Rent"
    );
    assert!(
        panel(&app).is_payment_allowed(),
        "Buy is allowed on an unowned dwelling"
    );

    choose_tab(&mut app, true);
    assert_eq!(panel(&app).op, slumlord::HouseOp::Rent);
    assert!(
        !panel(&app).is_payment_allowed(),
        "Rent is refused on an unowned dwelling"
    );
    assert!(
        !drop_on_the_window(&mut app, ObjectId(0x8000_0001)),
        "refused"
    );
    assert_eq!(
        emitted(&mut app).len(),
        0,
        "silently again -- the gate is below the string"
    );

    choose_tab(&mut app, false);
    assert_eq!(panel(&app).op, slumlord::HouseOp::Buy);
    assert!(
        panel(&app).is_payment_allowed(),
        "returning to Buy restores its payment list"
    );
    assert!(drop_and_settle(&mut app, ObjectId(0x8000_0001)), "taken");
    assert_eq!(panel(&app).buy_items.len(), 1);
}

/// **An item the player is not carrying is refused with retail's own sentence.**
///
/// The client carries the 41-character sentence as a wide string and emits it on channel
/// `0x1A` only on the **drop**: the drop asks for a visible refusal while hover requests quiet
/// validation.
#[test]
fn an_item_the_player_is_not_carrying_is_refused_with_retails_own_sentence() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("notyours");
    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);
    choose_tab(&mut app, false);

    // Outside the pack: this focused helper leaves the item with no container link.
    let stranger = ObjectId(0x8000_0002);
    give_item(&mut app, stranger, PYREAL, 5_000, false);
    settle(&mut app);

    // The far side of the seam first: let the frame carry the emitted line into the game, which
    // is where a player reads it.
    let before = panel(&app).drops_refused;
    let said_before = app.interaction().stats.panel_notice_strings;
    assert!(!drop_and_settle(&mut app, stranger));
    assert_eq!(panel(&app).drops_refused, before + 1);
    assert_eq!(
        app.interaction().stats.panel_notice_strings,
        said_before + 1,
        "one line reached the interaction chat-display path"
    );

    // …and the near side, which is the only place the exact bytes are still visible: the same
    // drop again, read out of the outbox before a frame takes it.
    assert!(!drop_on_the_window(&mut app, stranger));
    assert_eq!(
        emitted(&mut app),
        vec![(slumlord::NOTICE_CHANNEL, slumlord::NOT_CARRYING.to_owned())],
        "the refusal text appears on channel 0x1A exactly once and nothing else"
    );

    // The hover says nothing at all, which is the `quiet` half of the same function.
    let quiet = app
        .with_panels(|ui, panels, view| {
            panels
                .slumlord
                .drag_item_acceptable(&mut ui.requests, stranger, view, true)
        })
        .expect("a ui shell");
    assert!(!quiet, "still refused");
    assert_eq!(emitted(&mut app).len(), 0, "hover validation is silent");
}

// ---------------------------------------------------------------------------------------------
// Station 5 — the Buy button needs the price paid IN FULL
// ---------------------------------------------------------------------------------------------

/// **The button update's second gate**, which is the whole of retail's protection against
/// a partial purchase: the button is grey until the Buy payment is complete, not merely until the
/// list is non-empty.
///
/// The Rent button is the opposite rule: one row in the rent-item list is enough because
/// maintenance is cumulative.
#[test]
fn the_buy_button_waits_for_the_whole_price_and_the_rent_button_does_not() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("buttons");
    // The housing panel watches the profile's covenant crystal, and a watched object with no
    // physics body closes the window at the first one-second poll. This test pays thirty-four
    // rows and therefore runs well
    // past that poll, so the recorded lord needs a real body at the player — the same thing
    // [`deliver_profile_at_player`] does, done here with the **recorded** id so the profile keeps
    // its own `covenant_crystal`.
    place_slumlord_at_player(&mut app, &mut peer, SLUMLORD);
    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);
    choose_tab(&mut app, false);
    assert!(
        panel(&app).visible,
        "the window is open and stays open for the whole payment"
    );
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled
    );

    // Two of the three lines, in full.
    give_item(&mut app, ObjectId(0x8000_0010), PYREAL, 2_000, true);
    give_item(&mut app, ObjectId(0x8000_0011), WRIT, 5, true);
    give_item(&mut app, ObjectId(0x8000_0012), 511, 1, true);
    settle(&mut app);
    assert!(
        drop_and_settle(&mut app, ObjectId(0x8000_0011)),
        "5 Writs, the whole line"
    );
    assert!(
        drop_and_settle(&mut app, ObjectId(0x8000_0012)),
        "1 Crude Lockpick, the whole line"
    );
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled,
        "two of three lines paid leaves the Buy payment incomplete and the button disabled"
    );

    // A stack of 2,000 Pyreals against a 2,000,000 price is a *partial* payment, and the payment
    // accumulator credits it — so the row goes in and the button stays grey.
    assert!(drop_and_settle(&mut app, ObjectId(0x8000_0010)));
    assert_eq!(panel(&app).buy_items.len(), 3);
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled,
        "2000 of 2000000 Pyreals"
    );

    // The rest of the coin. The remaining-balance predicate makes the second stack acceptable
    // precisely because the line is not yet full.
    for i in 0..31u32 {
        // 31 * 65535 + 2000 > 2,000,000; each is its own object because a row may appear once.
        let id = ObjectId(0x8000_0100 + i);
        give_item(&mut app, id, PYREAL, u16::MAX, true);
        settle(&mut app);
        drop_and_settle(&mut app, id);
    }
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Enabled,
        "full payment enables the Buy button"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 5b — whole-stack payments use the physical pointer path
// ---------------------------------------------------------------------------------------------

/// Three concrete items, through the shipped inventory grid and Buy list. Loose
/// Pyreals and a Writ are ordinary payments; a Trade Note (100,000) is normalized by retail's
/// `TradeNotes` dual enum-id map and credits the Pyreal requirement by its face value.
#[test]
fn whole_pyreal_writ_and_trade_note_drops_stick_in_the_buy_window() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("whole-payments");
    deliver_profile_at_player(&mut app, &mut peer, &owner_payment_profile());
    settle(&mut app);

    let note_wcid = app
        .hud()
        .trade_note_values
        .iter()
        .find(|(value, _)| *value == 100_000)
        .map(|(_, wcid)| *wcid)
        .expect("the retail TradeNotes mapper carries a 100,000 Pyreal note");
    let pyreals = ObjectId(0x5000_0050);
    let writ = ObjectId(0x5000_0051);
    let note = ObjectId(0x5000_0052);
    give_carried_item(&mut app, pyreals, PYREAL, "Pyreal", "Pyreals", 3);
    give_carried_item(&mut app, writ, WRIT, "Writ of Refuge", "Writs of Refuge", 1);
    give_carried_item(
        &mut app,
        note,
        note_wcid,
        "Trade Note (100,000)",
        "Trade Notes (100,000)",
        1,
    );
    open_inventory(&mut app);
    settle(&mut app);
    assert_eq!(panel(&app).op, slumlord::HouseOp::Buy);
    let _ = wire(&mut app);

    drop_on_buy_list(&mut app, pyreals, 30.0);
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![pyreals]
    );
    drop_on_buy_list(&mut app, writ, 31.0);
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![pyreals, writ]
    );
    drop_on_buy_list(&mut app, note, 32.0);
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![pyreals, writ, note],
        "all three physical drops remain visible rows"
    );
    assert_eq!(panel(&app).buy_items[2].trade_note_value, Some(100_000));
    assert_eq!(panel(&app).buy_button_state, slumlord::ButtonState::Enabled);
    let sent = wire(&mut app);
    assert!(
        sent.is_empty(),
        "after the explicit startup drain, whole payment drops are local until Buy is confirmed; \
         got {sent:?}"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 5c — a payment row can be taken back before purchase
// ---------------------------------------------------------------------------------------------

/// The begin-drag receiver removes the picked row immediately
/// and recomputes the payment. This is what lets a player take loose Pyreals back out and replace
/// them with an equal-value Trade Note; the note must not be rejected as an overpayment against a
/// stale Pyreal row.
#[test]
fn a_pyreal_payment_row_can_be_dragged_out_and_replaced_with_a_trade_note() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("payment-removal");
    deliver_profile_at_player(&mut app, &mut peer, &split_profile(100_000));
    settle(&mut app);

    let note_wcid = app
        .hud()
        .trade_note_values
        .iter()
        .find(|(value, _)| *value == 100_000)
        .map(|(_, wcid)| *wcid)
        .expect("the retail TradeNotes mapper carries a 100,000 Pyreal note");
    let pyreals = ObjectId(0x5000_0053);
    let note = ObjectId(0x5000_0054);
    give_carried_item(&mut app, pyreals, PYREAL, "Pyreal", "Pyreals", 3);
    give_carried_item(
        &mut app,
        note,
        note_wcid,
        "Trade Note (100,000)",
        "Trade Notes (100,000)",
        1,
    );
    open_inventory(&mut app);
    settle(&mut app);
    let _ = wire(&mut app);

    drop_on_buy_list(&mut app, pyreals, 40.0);
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        vec![pyreals]
    );
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled
    );

    let row = house_row(&app, pyreals);
    pick_up(&mut app, row, 41.0);
    assert!(
        panel(&app).buy_items.is_empty(),
        "the drag-begin notice removes the payment at pick-up"
    );
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled
    );
    {
        let (ui, _) = gameplay(&mut app);
        ui.mouse_move(LocalTime(41.1), -10, -10);
    }
    app.frame();
    gameplay(&mut app)
        .0
        .mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, -10, -10, false);
    settle(&mut app);
    assert!(
        gameplay(&mut app).0.drag_state().element.is_none(),
        "the outside release ends drag"
    );
    assert_eq!(
        app.with_panels(|_, _, view| view.item_waiting(pyreals)),
        Some(false),
        "the ordinary failed-drop lifecycle clears the picked payment object's waiting flag"
    );

    drop_on_buy_list(&mut app, note, 42.0);
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| (d.id, d.trade_note_value))
            .collect::<Vec<_>>(),
        vec![(note, Some(100_000))],
        "the replacement note now owns the Pyreal payment row"
    );
    assert_eq!(panel(&app).buy_button_state, slumlord::ButtonState::Enabled);
    assert!(
        wire(&mut app).is_empty(),
        "row removal and substitution are local until Buy"
    );
}

// Station 5d — partial-stack payment is a two-gesture journey.
/// A pointer drop of part of a stack asks for `0x0055`, then the authoritative F745 selects the
/// new object. Retail has no slumlord-specific pending result and does not auto-add it: a second
/// real drag of that now-whole stack is what creates the visible row and credits the payment.
#[test]
fn a_partial_stack_is_split_then_the_authoritative_result_can_be_paid() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("partial-payment");
    deliver_profile_at_player(&mut app, &mut peer, &split_profile(3));
    settle(&mut app);
    give_carried_stack(&mut app, ObjectId(0x5000_0043), 10);
    open_inventory(&mut app);
    settle(&mut app);
    assert_eq!(
        panel(&app).op,
        slumlord::HouseOp::Buy,
        "the default Buy tab is live"
    );

    let source = ObjectId(0x5000_0043);
    select_split_quantity(&mut app, source, 3);
    let _ = wire(&mut app);
    drop_on_buy_list(&mut app, source, 10.0);
    let sent = wire(&mut app);
    assert_eq!(
        sent.iter().map(|(op, _)| *op).collect::<Vec<_>>(),
        vec![AC_SPLIT_TO_CONTAINER]
    );
    assert_eq!(u32_at(&sent[0].1, 0), source.0);
    assert_eq!(
        u32_at(&sent[0].1, 4),
        PLAYER.0,
        "split beside the source container"
    );
    assert_eq!(u32_at(&sent[0].1, 8), 0, "the split passes place zero");
    assert_eq!(
        u32_at(&sent[0].1, 12),
        3,
        "the pointer gesture's splitter amount"
    );
    assert!(
        panel(&app).buy_items.is_empty(),
        "the source is not an optimistic payment row"
    );
    assert_eq!(app.interaction().stats.house_splits_sent, 1);
    assert_eq!(
        app.probe_mut()
            .objects_mut()
            .world
            .pending_split
            .map(|p| (p.wcid, p.stack_size)),
        Some((PYREAL, 3)),
        "the generic inventory holder parked the split qualities"
    );
    assert_eq!(
        app.interaction().last_refusal.as_deref(),
        Some("Splitting the Pyreals before adding to housing panel")
    );

    let update = dereth_protocol::items::ItemUpdateStackSize {
        sequence: 1,
        item: source,
        amount: 7,
        new_value: 0,
    };
    app.probe_mut()
        .apply_interaction_events(&[SessionEvent::UiEvent {
            opcode: Opcode(0x0197),
            blob: dereth_protocol::write_blob(&update).expect("source stack update"),
        }]);
    settle(&mut app);

    let mut result = ObjectCreatePayload {
        id: SPLIT_RESULT,
        ..Default::default()
    };
    result.physicsdesc.timestamps.instance = 2;
    result.wdesc.header = dereth_protocol::types::weeniedesc::header::PLURAL_NAME
        | dereth_protocol::types::weeniedesc::header::STACK_SIZE
        | dereth_protocol::types::weeniedesc::header::MAX_STACK_SIZE
        | dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
    result.wdesc.name = "Pyreal".to_owned();
    result.wdesc.plural_name = Some("Pyreals".to_owned());
    result.wdesc.wcid = PYREAL;
    result.wdesc.icon_id = 0x0600_1234;
    result.wdesc.stack_size = Some(3);
    result.wdesc.max_stack_size = Some(10);
    result.wdesc.container_id = Some(PLAYER);
    let result_blob = dereth_protocol::write_blob(&ItemCreateObject(result)).expect("result F745");
    let decoded: ItemCreateObject =
        dereth_protocol::read_body(&result_blob[4..]).expect("the encoded F745 round-trips");
    assert_eq!(decoded.0.wdesc.stack_size, Some(3));
    assert_eq!(decoded.0.wdesc.max_stack_size, Some(10));
    assert_eq!(decoded.0.wdesc.container_id, Some(PLAYER));
    peer.send(&mut app, 10, result_blob);
    settle(&mut app);
    assert_eq!(
        app.probe_mut()
            .objects_mut()
            .world
            .weenie(SPLIT_RESULT)
            .map(|w| (w.pwd.wcid, w.pwd.stack_size, w.pwd.max_stack_size, w.valid)),
        Some((PYREAL, Some(3), Some(10), true))
    );
    assert!(
        app.probe_mut().objects_mut().world.pending_split.is_none(),
        "matching F745 consumed the generic pending split"
    );
    assert_eq!(
        app.probe_mut().objects_mut().world.selected,
        Some(SPLIT_RESULT)
    );
    assert!(
        panel(&app).buy_items.is_empty(),
        "F745 selects but does not invent a slumlord row"
    );

    let _ = wire(&mut app);
    drop_on_buy_list(&mut app, SPLIT_RESULT, 20.0);
    assert!(
        wire(&mut app).is_empty(),
        "the whole result is a local payment row, not another split"
    );
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| (d.id, d.amount))
            .collect::<Vec<_>>(),
        vec![(SPLIT_RESULT, 3)]
    );
    assert_eq!(panel(&app).buy_button_state, slumlord::ButtonState::Enabled);

    // The only slumlord-specific authoritative item callback removes an existing row once the
    // shard says it left the player's inventory; it is not the split-result insertion mechanism.
    let moved = dereth_protocol::objects::ItemServerSaysMoveItem { item: SPLIT_RESULT };
    peer.event(
        &mut app,
        Opcode(0x019A),
        &dereth_protocol::write_body(&moved).expect("0x019A move result"),
    );
    settle(&mut app);
    assert!(panel(&app).buy_items.is_empty());
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled
    );
}

// ---------------------------------------------------------------------------------------------
// Station 6 — the request, against the bytes the recorded retail client sent
// ---------------------------------------------------------------------------------------------

/// Behaviour: panels.house-purchase.paying-sends-buy-house-with-the-windows-items
///
/// **Payment sends `0x021C` with the items in the window's own order.**
///
/// The housing walk is *forwards*, unlike the salvage panel's reverse walk, and the order reaches
/// the wire: the recorded retail client sent
/// `3af0da79 03000000 c2150080 c1150080 92150080`, which is the drop order preserved.
///
/// The window is emptied as the request goes out. This test observes the empty list after the
/// request has traversed a frame and reached the replay endpoint; it does not independently time
/// those two operations within the handler.
#[test]
fn paying_sends_buy_house_with_the_items_in_the_windows_own_order() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("pay");
    // See the same line in
    // [`the_buy_button_waits_for_the_whole_price_and_the_rent_button_does_not`]. The recorded
    // lord gets a real body at the player so the window survives the one-second range poll; the
    // profile keeps its own `covenant_crystal`, which is what the recorded `0x021C` names.
    place_slumlord_at_player(&mut app, &mut peer, SLUMLORD);
    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);
    choose_tab(&mut app, false);
    assert!(
        panel(&app).visible,
        "the window is open before the first drop"
    );

    for (i, id) in RECORDED_ITEMS.iter().enumerate() {
        // The recorded three: a Crude Lockpick, a Writ of Refuge and a split trade-note stack.
        // This recorded-order test substitutes ordinary Pyreals; the whole-note and dragged-row
        // tests separately exercise the trade-note arm.
        let wcid = [511u32, WRIT, PYREAL][i];
        let stack = [1u16, 5, 2_000][i];
        give_item(&mut app, *id, wcid, stack, true);
    }
    settle(&mut app);
    for id in RECORDED_ITEMS {
        assert!(drop_and_settle(&mut app, id), "{id:?} is taken");
    }
    assert_eq!(
        panel(&app)
            .buy_items
            .iter()
            .map(|d| d.id)
            .collect::<Vec<_>>(),
        RECORDED_ITEMS.to_vec(),
        "the window holds them in drop order"
    );

    // Drain the startup traffic so what follows is this gesture's own.
    let _ = wire(&mut app);
    let sent = app
        .with_panels(|ui, panels, view| panels.slumlord.make_payment(ui, view))
        .expect("a ui shell");
    assert!(sent, "all three payment guards pass");
    settle(&mut app);

    assert_eq!(
        app.interaction().stats.house_payments_sent,
        1,
        "one 0x021C in the outbox"
    );
    // The encoder comparison below is a codec check that runs whatever the panel
    // did; this is the same bytes taken off the replay endpoint, so the window's own drop order
    // is proved to reach the wire rather than merely to round-trip through `write_body`.
    assert_eq!(
        wire(&mut app)
            .into_iter()
            .filter(|(opcode, _)| *opcode == 0x021C)
            .collect::<Vec<_>>(),
        vec![(0x021Cu32, hex("3af0da7903000000c2150080c115008092150080"))],
        "the recorded body reaches the wire exactly once"
    );
    assert!(
        panel(&app).buy_items.is_empty(),
        "the payment list is empty after the request reaches the endpoint"
    );
    assert_eq!(
        panel(&app).buy_button_state,
        slumlord::ButtonState::Disabled,
        "the Buy button is disabled with the empty payment list"
    );

    let want = dereth_protocol::trade::HouseBuyHouse {
        slumlord: SLUMLORD,
        items: RECORDED_ITEMS.to_vec(),
    };
    assert_eq!(
        dereth_protocol::write_body(&want).expect("encodes"),
        hex("3af0da7903000000c2150080c115008092150080"),
        "the body the recorded retail client put on the wire at t = 144.458"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 6b — the two physical confirmation paths
// ---------------------------------------------------------------------------------------------

/// The physical-button handler does not pay immediately for a landscape house or for
/// somebody else's maintenance. Both physical buttons open the shipped default-queue
/// confirmation dialog; No closes the slumlord window and sends nothing, while Yes alone sends
/// the current item list after the matching guard is checked again.
#[test]
fn the_physical_buy_and_proxy_rent_buttons_ask_before_they_pay() {
    let _gpu = gpu_lock();

    // Landscape purchase: actual button -> exact question -> No/no packet and the window closes.
    let (mut app, mut peer) = setup("buy-confirmation");
    let buy_lord =
        deliver_profile_at_player(&mut app, &mut peer, &confirmation_profile(ObjectId(0), 2));
    settle(&mut app);
    assert_eq!(
        panel(&app).op,
        slumlord::HouseOp::Buy,
        "the shipped default Buy page initializes the derived slumlord operation before any tab click"
    );
    let coin = ObjectId(0x8000_7001);
    give_item(&mut app, coin, PYREAL, 1, true);
    settle(&mut app);
    assert!(drop_and_settle(&mut app, coin));
    assert_eq!(panel(&app).buy_button_state, slumlord::ButtonState::Enabled);

    let mut wire = Vec::new();
    let buy_button = button(&app, slumlord::BUY_BUTTON);
    click_handle(&mut app, buy_button, &mut wire);
    assert!(wire.is_empty(), "the confirmation comes before 0x021C");
    let (first_context, first_root) = open_house_dialog(&app);
    let (prompt, _yes, no) = {
        let ui = &app.ui().expect("the ui shell").ui;
        (
            ui.get_child_recursive(first_root, dereth_ui::dialog::base::child::TEXT)
                .expect("the prompt text"),
            ui.get_child_recursive(first_root, dereth_ui::dialog::base::child::BUTTON1)
                .expect("the Yes button"),
            ui.get_child_recursive(first_root, dereth_ui::dialog::base::child::BUTTON2)
                .expect("the No button"),
        )
    };
    assert_eq!(
        app.ui_mut()
            .expect("the ui shell")
            .ui
            .text_element_mut(prompt)
            .expect("the prompt is text")
            .glyphs
            .inq_text(false),
        slumlord::BUY_CONFIRMATION
    );
    click_handle(&mut app, no, &mut wire);
    assert!(wire.is_empty(), "No never sends House_BuyHouse");
    assert!(
        !panel(&app).visible,
        "closing the buy-house confirmation with `false` hides the housing panel"
    );

    // A fresh profile reopens the panel. Removing the payment while the question is up proves
    // Confirmation close rechecks full payment instead of trusting button-time state.
    deliver_profile_at_player(&mut app, &mut peer, &confirmation_profile(ObjectId(0), 2));
    settle(&mut app);
    choose_physical_tab(&mut app, false);
    give_item(&mut app, coin, PYREAL, 1, true);
    settle(&mut app);
    assert!(drop_and_settle(&mut app, coin));
    let buy_button = button(&app, slumlord::BUY_BUTTON);
    click_handle(&mut app, buy_button, &mut wire);
    let (second_context, second_root) = open_house_dialog(&app);
    assert_ne!(
        second_context, first_context,
        "dismiss then click opens a fresh context"
    );
    app.with_panels(|ui, panels, _| panels.slumlord.clean_item_lists(ui))
        .expect("a ui shell");
    let yes = app
        .ui()
        .expect("the ui shell")
        .ui
        .get_child_recursive(second_root, dereth_ui::dialog::base::child::BUTTON1)
        .expect("the Yes button");
    click_handle(&mut app, yes, &mut wire);
    assert!(
        wire.is_empty(),
        "Yes with an unpaid Buy profile is refused at dialog close"
    );
    assert!(
        panel(&app).visible,
        "a failed Yes guard does not hide the panel"
    );

    // Refill after the failed close-time guard: a fresh click/context and Yes now send the exact
    // purchase request once. This keeps the two purchase outcomes distinct from proxy rent.
    assert!(drop_and_settle(&mut app, coin));
    let buy_button = button(&app, slumlord::BUY_BUTTON);
    click_handle(&mut app, buy_button, &mut wire);
    let (third_context, third_root) = open_house_dialog(&app);
    assert_ne!(
        third_context, second_context,
        "the guarded close released the Buy context"
    );
    let yes = app
        .ui()
        .expect("the ui shell")
        .ui
        .get_child_recursive(third_root, dereth_ui::dialog::base::child::BUTTON1)
        .expect("the Yes button");
    click_handle(&mut app, yes, &mut wire);
    assert_eq!(
        wire,
        vec![dereth_client_model::Request::BuyHouse(
            dereth_protocol::trade::HouseBuyHouse {
                slumlord: buy_lord,
                items: vec![coin],
            }
        )],
        "a paid-in-full landscape purchase sends exactly once, after Yes"
    );
    assert!(
        panel(&app).buy_items.is_empty(),
        "the Buy payment leaves the panel empty after 0x021C is sent"
    );

    // Proxy maintenance: the same physical route, the distinct exact prompt, and one 0x0221 on
    // Yes. The fixture owner differs from the local player, which is the whole of the house-owner check.
    let (mut app, mut peer) = setup("rent-confirmation");
    let rent_lord = deliver_profile_at_player(
        &mut app,
        &mut peer,
        &confirmation_profile(ObjectId(0x5000_00AA), 2),
    );
    settle(&mut app);
    choose_physical_tab(&mut app, true);
    let rent_coin = ObjectId(0x8000_7002);
    give_item(&mut app, rent_coin, PYREAL, 1, true);
    settle(&mut app);
    assert!(drop_and_settle(&mut app, rent_coin));
    assert_eq!(
        panel(&app).rent_button_state,
        slumlord::ButtonState::Enabled
    );
    let mut wire = Vec::new();
    let rent_button = button(&app, slumlord::RENT_BUTTON);
    click_handle(&mut app, rent_button, &mut wire);
    assert!(
        wire.is_empty(),
        "the proxy confirmation comes before 0x0221"
    );
    let (_, root) = open_house_dialog(&app);
    let (prompt, yes) = {
        let ui = &app.ui().expect("the ui shell").ui;
        (
            ui.get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
                .expect("the prompt text"),
            ui.get_child_recursive(root, dereth_ui::dialog::base::child::BUTTON1)
                .expect("the Yes button"),
        )
    };
    assert_eq!(
        app.ui_mut()
            .expect("the ui shell")
            .ui
            .text_element_mut(prompt)
            .expect("the prompt is text")
            .glyphs
            .inq_text(false),
        slumlord::RENT_BY_PROXY_CONFIRMATION
    );
    click_handle(&mut app, yes, &mut wire);
    assert_eq!(
        wire,
        vec![dereth_client_model::Request::RentHouse(
            dereth_protocol::trade::HouseRentHouse {
                slumlord: rent_lord,
                items: vec![rent_coin],
            }
        )],
        "accepting proxy-rent confirmation sends the current row exactly once"
    );
    assert!(
        panel(&app).rent_items.is_empty(),
        "the rent payment leaves the panel empty after 0x0221 is sent"
    );
}

// ---------------------------------------------------------------------------------------------
// Station 7 — the retry, which is the only sender of 0x0258 in retail
// ---------------------------------------------------------------------------------------------

/// **A refused transaction re-asks the lord.**
///
/// `0x0226 House_HouseStatus` and `0x0259 House_HouseTransaction` both raise the failed-transaction
/// notice, which has three receivers. The housing panel's receiver re-queries the lord only when a
/// house profile is present.
///
/// With **no** profile it must send nothing, which is the guard and the reason a houseless
/// character standing nowhere near a slumlord does not spray `0x0258` at the shard on every
/// login — `0x0226` arrives on every one of them.
#[test]
fn a_failed_transaction_re_queries_the_lord_and_only_with_a_profile() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("retry");

    // The login `0x0226`, before any slumlord has been used.
    peer.event(&mut app, Opcode::HOUSE_HOUSE_STATUS, &2_u32.to_le_bytes());
    settle(&mut app);
    let after_login = app.interaction().stats.house_lord_queries;
    assert_eq!(
        after_login, 0,
        "without a profile, the failed-transaction notice sends no retry"
    );

    peer.event(
        &mut app,
        Opcode::HOUSE_HOUSE_PROFILE,
        &hex(RECORDED_PROFILE),
    );
    settle(&mut app);
    let sent = app
        .with_panels(|ui, panels, _| {
            panels
                .slumlord
                .recv_failed_house_transaction(&mut ui.requests)
        })
        .expect("a ui shell");
    assert!(sent);
    settle(&mut app);
    assert_eq!(app.interaction().stats.house_lord_queries, 1, "one 0x0258");
    assert_eq!(panel(&app).requeries, 1);
}

// ---------------------------------------------------------------------------------------------
// Station 8 — the shard's own refusal, as recorded
// ---------------------------------------------------------------------------------------------

/// **What the recorded shard answered, and the two messages it answered with.**
///
/// ACE's `Source/ACE.Server/WorldObjects/Player_House.cs:95`, the
/// `house_15day_account` gate:
///
/// ```csharp
/// var msg = "Your account must be at least 15 days old to purchase this dwelling. …";
/// Session.Network.EnqueueSend(new GameEventCommunicationTransientString(Session, msg),
///                             new GameMessageSystemChat(msg, ChatMessageType.Broadcast));
/// ```
///
/// and both are in the recording at `t = 144.465` — a `0x02EB` event **and** an `0xF7E0` system
/// chat line carrying the same 115 characters. This is the byte-level check of the refusal, and
/// it is the reason the window must stay usable after a refusal (hence station 7's retry).
#[test]
fn the_recorded_refusal_is_the_fifteen_day_rule_and_arrives_twice() {
    const REFUSAL: &str = "Your account must be at least 15 days old to purchase this dwelling. \
                           This applies to all housing except apartments.";
    let transient = hex(
        "7300596f7572206163636f756e74206d757374206265206174206c656173742031352064617973206f6c64\
         20746f2070757263686173652074686973206477656c6c696e672e2054686973206170706c69657320746f\
         20616c6c20686f7573696e67206578636570742061706172746d656e74732e000000",
    );
    assert_eq!(transient.len(), 120, "the recorded 0x02EB body");
    let len = u16::from_le_bytes([transient[0], transient[1]]);
    assert_eq!(usize::from(len), REFUSAL.len(), "115 characters");
    assert_eq!(
        std::str::from_utf8(&transient[2..2 + usize::from(len)]).expect("ascii"),
        REFUSAL
    );
}

// ---------------------------------------------------------------------------------------------
// Station 9 — the shipped tree, measured
// ---------------------------------------------------------------------------------------------

/// **Every element binds, along with the three element-message controls.**
///
/// `0x10000090` is not the close button: panel initialization never binds it — it is the
/// **Buy tab page**, and the close is `0x1000009E`.
#[test]
fn the_window_is_the_fourth_env_panel_page_and_binds_ten_children() {
    let _gpu = gpu_lock();
    let (mut app, _peer) = setup("tree");

    assert_eq!(
        dereth_ui_screens::panels::catalogue::ENV_PANEL_PAGES[3],
        slumlord::PANEL.0,
        "the environment panel's fourth page is the housing panel"
    );
    let p = panel(&app);
    assert!(p.root.is_some(), "0x10000060");
    for (what, bound) in [
        ("buy requirements 0x10000091", p.buy_requirements.is_some()),
        ("buy owner 0x10000093", p.buy_owner.is_some()),
        ("buy button 0x10000094", p.buy_button.is_some()),
        ("buy item list 0x10000095", p.buy_list.is_some()),
        (
            "rent requirements 0x10000098",
            p.rent_requirements.is_some(),
        ),
        ("rent owner 0x1000009A", p.rent_owner.is_some()),
        ("rent button 0x1000009B", p.rent_button.is_some()),
        ("rent item list 0x1000009C", p.rent_list.is_some()),
        ("the close X 0x1000009E", p.close_button.is_some()),
    ] {
        assert!(bound, "{what} is bound in the shipped tree");
    }

    let root = app.ui_mut().expect("shell").ui.root();
    for page in [slumlord::BUY_PAGE, slumlord::RENT_PAGE] {
        assert!(
            app.ui_mut()
                .expect("shell")
                .ui
                .get_child_recursive(root, page)
                .is_some(),
            "{page:?} is a tab page of the window, not its close button"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// Station 6c — the one house type the Buy button does not ask about
// ---------------------------------------------------------------------------------------------

/// Behaviour: panels.house-purchase.an-apartment-buy-pays-on-the-click
///
/// **Buying an apartment skips the thirty-day confirmation entirely.**
///
/// The physical-button branch is the only housing-panel path that branches on the
/// profile's house type:
///
/// ```text
///   if no profile is present or the house type is not apartment:
///       ask for purchase confirmation and stop
///   otherwise:
///       make the payment on the click
/// ```
///
/// and the question it skips is `BUY_CONFIRMATION`'s own sentence — *"you will be unable to
/// purchase a different dwelling for 30 days"* — which is exactly the restriction
/// the house-information row ends by exempting apartments from
/// (`". This restriction does not apply to apartments."`). The two literals
/// are the same rule stated twice, and this is the branch that honours it.
///
/// Both recorded sessions are a **villa**, so the recorded villa control never takes the apartment
/// arm. The synthetic confirmation journeys use [`confirmation_profile`]'s documented owner,
/// name, type, list-length and count mutations, and [`deliver_profile_at_player`] replaces the
/// covenant crystal. The shard cannot help here — ACE's `SlumLord.ActOnUse` answers with
/// `GameEventHouseProfile` and the whole decision is the client's — so the apartment case is
/// constructed through the real `0x021D` decoder from the recorded body.
#[test]
fn an_apartment_buy_pays_on_the_click_with_no_confirmation() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup("apartment-buy");
    let lord = deliver_profile_at_player(
        &mut app,
        &mut peer,
        &confirmation_profile(ObjectId(0), slumlord::APARTMENT),
    );
    settle(&mut app);
    assert_eq!(
        panel(&app).profile.as_ref().map(|p| p.house_type),
        Some(slumlord::APARTMENT),
        "the constructed profile arrived through the real decoder as an apartment"
    );
    assert_eq!(
        panel(&app).op,
        slumlord::HouseOp::Buy,
        "the shipped default page is Buy"
    );

    let coin = ObjectId(0x8000_7003);
    give_item(&mut app, coin, PYREAL, 1, true);
    settle(&mut app);
    assert!(drop_and_settle(&mut app, coin));
    assert_eq!(panel(&app).buy_button_state, slumlord::ButtonState::Enabled);

    let mut wire = Vec::new();
    let buy_button = button(&app, slumlord::BUY_BUTTON);
    click_handle(&mut app, buy_button, &mut wire);

    assert!(
        app.ui()
            .expect("the ui shell")
            .ui
            .dialogs
            .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)
            .is_none(),
        "the apartment branch pays directly and opens no buy confirmation"
    );
    assert_eq!(
        wire,
        vec![dereth_client_model::Request::BuyHouse(
            dereth_protocol::trade::HouseBuyHouse {
                slumlord: lord,
                items: vec![coin],
            }
        )],
        "one 0x021C House_BuyHouse, on the click itself"
    );
    assert!(
        panel(&app).buy_items.is_empty(),
        "the apartment payment leaves the list empty after the send"
    );
    assert!(
        panel(&app).visible,
        "nothing closed the window: there was no dialog to close"
    );

    // The other side of the same equality, from the same fixture: a cottage is a landscape house
    // and asks. The check compares with Apartment alone; it is not an ordinal
    // "smaller than a villa" test, and this test would notice a `<`/`!=` slip.
    let (mut app, mut peer) = setup("cottage-buy");
    deliver_profile_at_player(&mut app, &mut peer, &confirmation_profile(ObjectId(0), 1));
    settle(&mut app);
    let coin = ObjectId(0x8000_7004);
    give_item(&mut app, coin, PYREAL, 1, true);
    settle(&mut app);
    assert!(drop_and_settle(&mut app, coin));
    let mut wire = Vec::new();
    let buy_button = button(&app, slumlord::BUY_BUTTON);
    click_handle(&mut app, buy_button, &mut wire);
    assert!(
        wire.is_empty(),
        "a cottage is not an apartment, so confirmation comes first"
    );
    let (_, root) = open_house_dialog(&app);
    let prompt = app
        .ui()
        .expect("the ui shell")
        .ui
        .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
        .expect("the prompt text");
    assert_eq!(
        app.ui_mut()
            .expect("the ui shell")
            .ui
            .text_element_mut(prompt)
            .expect("the prompt is text")
            .glyphs
            .inq_text(false),
        slumlord::BUY_CONFIRMATION,
        "the thirty-day sentence an apartment never sees"
    );
}
