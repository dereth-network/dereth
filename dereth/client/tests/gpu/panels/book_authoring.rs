//! Book authoring through the shipped `<BOOK>` window: the page character limit is the text
//! element's own, an unowned book closes beyond its use radius while a carried one never does,
//! Next adds a page (`0x00AC`) and the authoritative reply makes it editable, blank owned pages are
//! deleted or refused on page turns, the page menu builds real rows and author labels (with
//! accounts for a PSR viewer), failed add replies re-request the book, and hiding the window
//! commits the current page.
//! Fixture: the retail dats and a headless 800x600 App in gameplay mode; the client's outbound
//! datagrams are read from a socket-free replay endpoint that also feeds the ordered responses.

#![cfg(any(feature = "vulkan", feature = "wgpu", all(windows, feature = "d3d12")))]

use crate::common::client_dir;
use crate::common::gpu_lock;

use dereth_client::{app::App, config::Config, net::ClientNetwork};
use dereth_primitives::{LocalTime, ObjectId};
use dereth_protocol::objects::{ItemCreateObject, ObjectCreatePayload};
use dereth_protocol::Message;
use dereth_ui::ElementId;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const BOOK: ObjectId = ObjectId(0x8000_0B00);
const USE_RADIUS: f32 = 3.0;

struct Peer {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    blob: u32,
    stamp: u32,
}

impl Peer {
    fn new() -> (Self, ClientNetwork) {
        let mut net = ClientNetwork::new("127.0.0.1:19100", 7304, "book-authoring", "unused", 0)
            .expect("a socket-free replay net");
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

    fn event<M: Message>(&mut self, app: &mut App, message: &M) {
        self.stamp += 1;
        let blob = dereth_protocol::events::pack_event(PLAYER, self.stamp, message).expect("event");
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
            .expect("datagram");
        app.replay_network_mut()
            .expect("replay endpoint")
            .session
            .transport
            .feed(&raw, None, LocalTime(0.0))
            .expect("fed");
    }
}

fn setup() -> (App, Peer) {
    assert!(dereth_dat::testing::have_dats(), "retail dats are required");
    let mut app = App::new(Config {
        ui: true,
        headless: true,
        sound: false,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        preferences_file: std::env::temp_dir().join("dereth-book-authoring-not-created/prefs.ini"),
        ..Config::default()
    })
    .expect("app");
    app.start_shell().expect("shell");
    app.load_static_scene(dereth_client::world::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..Default::default()
    })
    .expect("scene");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..4 {
        app.frame();
    }
    let (mut peer, net) = Peer::new();
    app.attach_replay_network(net).expect("replay endpoint");
    let mut player = ObjectCreatePayload {
        id: PLAYER,
        ..Default::default()
    };
    player.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
    player.physicsdesc.setup_id = Some(0x0200_0001);
    player.physicsdesc.timestamps.instance = 1;
    peer.send(
        &mut app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(player)).expect("player"),
    );
    app.frame();
    app.objects_mut().world.player = Some(PLAYER);
    app.objects_mut()
        .world
        .weenie_mut(PLAYER)
        .expect("created player")
        .pwd
        .name = "Larktest".into();
    settle(&mut app);
    let _ = actions_sent(&mut app);
    (app, peer)
}

fn settle(app: &mut App) {
    for _ in 0..6 {
        app.frame();
    }
}

fn gameplay(app: &mut App) -> &mut GamePlayScreen {
    let shell = app.ui_mut().expect("shell");
    let any: &mut dyn std::any::Any = &mut **shell.flow.current_mut().expect("gameplay");
    any.downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen")
}

fn click(app: &mut App, id: ElementId) {
    let h = app
        .ui()
        .expect("shell")
        .ui
        .get_element(id)
        .expect("element");
    click_handle(app, h);
}

fn click_handle(app: &mut App, h: dereth_ui::ElemHandle) {
    let at = {
        let ui = &app.ui().expect("shell").ui;
        let b = ui.screen_box(h);
        ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    let ui = &mut app.ui_mut().expect("shell").ui;
    ui.mouse_move(LocalTime(1.0), at.0, at.1);
    ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1);
    ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, at.0, at.1, false);
    app.frame();
}

fn type_text(app: &mut App, text: &str) {
    let ui = &mut app.ui_mut().expect("shell").ui;
    for c in text.encode_utf16() {
        ui.character(c);
    }
    app.frame();
}

fn actions_sent(app: &mut App) -> Vec<(u32, Vec<u8>)> {
    let mut out = Vec::new();
    let mut reassembly = dereth_transport::indicator::Indicator::default();
    for (bytes, _) in app.replay_network_mut().expect("endpoint").take_outgoing() {
        let Ok(packet) = dereth_transport::wire::ParsedPacket::parse(&bytes) else {
            continue;
        };
        for blob in
            reassembly.check_in_packet(&packet.fragments, packet.header.rec_id, LocalTime(0.0))
        {
            let body = blob.payload;
            if blob.queue_id == 3
                && body.len() >= 12
                && u32::from_le_bytes(body[0..4].try_into().expect("four bytes")) == 0xF7B1
            {
                out.push((
                    u32::from_le_bytes(body[8..12].try_into().expect("four bytes")),
                    body[12..].to_vec(),
                ));
            }
        }
    }
    out
}

fn spew(app: &App) -> Vec<String> {
    let mut out = app.hud().panels.spew.model.pending.clone();
    out.extend(app.hud().panels.spew.model.items.iter().cloned());
    out
}

fn page(author: ObjectId, text: Option<&str>) -> dereth_protocol::trade::PageData {
    dereth_protocol::trade::PageData {
        author_id: author,
        author_name: "Somebody".into(),
        author_account: String::new(),
        version: Some(dereth_protocol::trade::PageData::VERSION_WITH_FLAGS),
        text_included: i32::from(text.is_some()),
        ignore_author: 0,
        page_text: text.map(str::to_owned),
    }
}

fn set_psr_quality(app: &mut App, sequence: u8, property_id: u32, value: bool) {
    let update = dereth_protocol::qualities::QualitiesPrivateUpdateBool(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id,
            value: i32::from(value),
        },
    );
    let mut blob = dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL
        .0
        .to_le_bytes()
        .to_vec();
    blob.extend(dereth_protocol::write_body(&update).expect("private Boolean quality update"));
    app.apply_hud_events(&[dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_BOOL,
        blob,
    }]);
    assert_eq!(
        app.objects()
            .world
            .player_qualities()
            .expect("the accepted player descriptor")
            .inq_bool(property_id),
        value,
        "the encoded private update reached the authoritative player quality store"
    );
}

fn open(app: &mut App, peer: &mut Peer, pages: Vec<dereth_protocol::trade::PageData>) {
    open_with_max(app, peer, 2, pages);
}

fn open_with_max(
    app: &mut App,
    peer: &mut Peer,
    max_num_pages: u32,
    pages: Vec<dereth_protocol::trade::PageData>,
) {
    open_with_limits(app, peer, max_num_pages, 1000, pages);
}

fn open_with_limits(
    app: &mut App,
    peer: &mut Peer,
    max_num_pages: u32,
    max_num_chars_per_page: i32,
    pages: Vec<dereth_protocol::trade::PageData>,
) {
    peer.event(
        app,
        &dereth_protocol::trade::WritingBookOpen {
            book_id: BOOK,
            max_num_pages,
            pages: dereth_protocol::trade::PageDataList {
                max_num_pages: i32::try_from(max_num_pages).expect("small fixture page count"),
                max_num_chars_per_page,
                pages,
            },
            inscription: "Draft".into(),
            scribe_id: PLAYER,
            scribe_name: "Larktest".into(),
        },
    );
    settle(app);
}

/// The decoded book retains its declared maximum characters per page, but opening the book,
/// making it editable, and closing its current page never read that field. The shipped page text
/// element instead carries its own fixed maximum-character property (`0x1E`). A deliberately tiny
/// declared value therefore must not replace that authored cap, while real editor input and
/// ModifyPage must still honor it.
#[test]
fn shipped_page_character_limit_is_fixed_and_independent_of_the_declared_list_field() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    open_with_limits(
        &mut app,
        &mut peer,
        2,
        1,
        vec![
            page(PLAYER, Some("")),
            page(ObjectId(0x5000_0002), Some("next")),
        ],
    );
    let _ = actions_sent(&mut app);

    let page_text = dereth_ui_screens::panels::book::PAGE_TEXT;
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        let h = ui.get_element(page_text).expect("page text");
        assert_eq!(
            ui.node(h)
                .expect("page text node")
                .merged_properties()
                .get_int(dereth_ui::props::attr::TEXT_MAX_CHARACTERS),
            Some(999),
            "the shipped PAGE_TEXT authors the ordinary book editor's fixed 0x1E limit"
        );
        assert_eq!(
            ui.text_element_mut(h).expect("text").glyphs.max_characters,
            999,
            "opening the book must leave the authored limit intact rather than project the list field"
        );
    }

    click(&mut app, page_text);
    let attempted = "x".repeat(1005);
    type_text(&mut app, &attempted);
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        let h = ui.get_element(page_text).expect("page text");
        assert_eq!(
            ui.text_element_mut(h).expect("text").glyphs.inq_text(false),
            "x".repeat(999),
            "real character input is capped by the shipped text element"
        );
    }
    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);

    let sent = actions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "leaving the edited page commits exactly once: {sent:02x?}"
    );
    assert_eq!(
        sent[0].0, 0x00AB,
        "the normal page-turn path sends ModifyPage"
    );
    let mut r = dereth_protocol::archive::Reader::new(&sent[0].1);
    let modify = dereth_protocol::trade::WritingBookModifyPage::read(&mut r).expect("modify body");
    assert_eq!(modify.book_id, BOOK);
    assert_eq!(modify.page, 0);
    assert_eq!(
        modify.text,
        "x".repeat(999),
        "ModifyPage carries the editor's capped text, not the declared list limit"
    );
}

fn place_book_at_player(
    app: &mut App,
    peer: &mut Peer,
    owned: bool,
) -> dereth_primitives::Position {
    let here = app
        .world_state()
        .and_then(|scene| scene.character.as_ref())
        .expect("the local physics body")
        .position();
    let mut book = ObjectCreatePayload {
        id: BOOK,
        ..Default::default()
    };
    book.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP
        | dereth_protocol::types::physicsdesc::flags::POSITION;
    book.physicsdesc.setup_id = Some(0x0200_0001);
    book.physicsdesc.position = Some(dereth_protocol::types::PositionWire {
        objcell_id: here.cell.0,
        frame: dereth_protocol::types::Frame {
            origin: here.frame.origin.into(),
            orientation: here.frame.rotation.into(),
        },
    });
    book.physicsdesc.timestamps.instance = 2;
    book.physicsdesc.timestamps.position = 1;
    book.wdesc.name = "Range Book".to_owned();
    book.wdesc.header |= dereth_protocol::types::weeniedesc::header::USE_RADIUS;
    book.wdesc.use_radius = Some(USE_RADIUS);
    if owned {
        book.wdesc.header |= dereth_protocol::types::weeniedesc::header::CONTAINER_ID;
        book.wdesc.container_id = Some(PLAYER);
    }
    peer.send(
        app,
        10,
        dereth_protocol::write_blob(&ItemCreateObject(book)).expect("book object"),
    );
    settle(app);
    let placed = app
        .world_scene()
        .and_then(|scene| scene.server_object_position(BOOK))
        .expect("the book has a positioned physics body");
    assert_eq!(
        app.objects()
            .world
            .weenie(BOOK)
            .and_then(|w| w.pwd.use_radius),
        Some(USE_RADIUS),
        "the object-create path must preserve the book's use radius"
    );
    assert_eq!(
        app.objects().world.is_owned_by_player(BOOK),
        owned,
        "the fixture must reach the owned/unowned book-opening fork"
    );
    placed
}

/// Opening an unowned book registers a range watch at the object's use radius; opening an owned
/// book does not.
/// The existing production range poll must keep the real window open nearby, then deliver the
/// matching leave edge after local-body movement, close it, and retire the one-shot watch.
#[test]
fn an_unowned_book_closes_after_real_movement_beyond_its_use_radius() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let book_position = place_book_at_player(&mut app, &mut peer, false);
    open(
        &mut app,
        &mut peer,
        vec![page(ObjectId(0x5000_0002), Some("nearby"))],
    );

    let armed = app
        .objects()
        .world
        .object_range_checks
        .live()
        .find(|e| e.handler == dereth_client_model::range::RangeHandler::Book)
        .copied()
        .expect("opening the book armed the unowned book's range handler");
    assert_eq!(armed.object, BOOK);
    assert!((armed.range - f64::from(USE_RADIUS)).abs() < 1e-6);
    assert!(armed.use_radii && !armed.ignore_z_delta);
    for _ in 0..40 {
        app.frame();
    }
    assert_eq!(
        gameplay(&mut app).book.book_id,
        Some(BOOK),
        "staying nearby keeps the book open"
    );

    let mut away = book_position;
    away.frame.origin.x += 8.0;
    app.world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(away);
    for _ in 0..40 {
        app.frame();
    }
    assert_eq!(
        gameplay(&mut app).book.book_id,
        None,
        "the matching range exit closes the panel"
    );
    assert!(
        !app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Book, BOOK),
        "the range exit and hidden edge retire the book watch"
    );
}

/// A carried book takes the nonzero owned-by-player arm: no proximity watch exists, so walking
/// away cannot close a book being read from the player's own inventory.
#[test]
fn an_owned_book_never_arms_the_range_handler_or_closes_on_movement() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let book_position = place_book_at_player(&mut app, &mut peer, true);
    open(&mut app, &mut peer, vec![page(PLAYER, Some("carried"))]);
    assert!(
        !app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Book, BOOK),
        "retail does not range-watch a book owned by the player"
    );

    let mut away = book_position;
    away.frame.origin.x += 8.0;
    app.world_state_mut()
        .expect("scene")
        .character
        .as_mut()
        .expect("local body")
        .teleport(away);
    for _ in 0..40 {
        app.frame();
    }
    assert_eq!(
        gameplay(&mut app).book.book_id,
        Some(BOOK),
        "the carried book remains open"
    );
}

/// Behaviour: panels.book.next-adds-a-page-and-the-reply-makes-it-editable
///
/// Retail's add-page action writes the literal `0x00AC` and one dword (not `0x00AA`).
#[test]
fn next_adds_a_page_and_the_reply_makes_it_a_real_editable_saved_page() {
    let _gpu = gpu_lock();
    let mut app;
    let mut peer;
    (app, peer) = setup();
    open(
        &mut app,
        &mut peer,
        vec![page(ObjectId(0x5000_0002), Some("published"))],
    );
    assert_eq!(gameplay(&mut app).book.cur_page, 0);
    let _ = actions_sent(&mut app);

    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);
    assert_eq!(
        actions_sent(&mut app),
        vec![(0x00AC, BOOK.0.to_le_bytes().to_vec())],
        "the shipped Next button must reach retail's AddPage literal 0x00AC, not 0x00AA"
    );

    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: BOOK,
            page_number: 1,
            success: 1,
        },
    );
    settle(&mut app);
    let page_text = dereth_ui_screens::panels::book::PAGE_TEXT;
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        let menu = ui
            .get_element(dereth_ui_screens::panels::book::PAGE_MENU)
            .expect("page menu");
        assert_eq!(
            dereth_ui::widgets::menu::selected_index(ui, menu),
            1,
            "the authoritative AddPage response keeps the acknowledged blank row selected"
        );
        let h = ui.get_element(page_text).expect("page text");
        assert!(
            ui.text_element_mut(h).expect("text").bits.editable(),
            "the reply's new page is writable"
        );
    }

    click(&mut app, page_text);
    type_text(&mut app, "hello from the real editor");
    click(&mut app, dereth_ui_screens::panels::book::PREV_BUTTON);
    settle(&mut app);
    let sent = actions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "leaving the edited page commits once: {sent:02x?}"
    );
    assert_eq!(
        sent[0].0, 0x00AB,
        "ModifyPage must use the retail literal rather than a self-referential enum check"
    );
    let mut r = dereth_protocol::archive::Reader::new(&sent[0].1);
    let modify = dereth_protocol::trade::WritingBookModifyPage::read(&mut r).expect("modify body");
    assert_eq!(modify.book_id, BOOK);
    assert_eq!(modify.page, 1);
    assert_eq!(modify.text, "hello from the real editor");

    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);
    assert!(
        actions_sent(&mut app).is_empty(),
        "revisiting the acknowledged page must not request another AddPage"
    );
    let ui = &mut app.ui_mut().expect("shell").ui;
    let h = ui.get_element(page_text).expect("page text");
    assert_eq!(
        ui.text_element_mut(h).expect("text").glyphs.inq_text(false),
        "hello from the real editor",
        "the committed local page remains visible on revisit"
    );
}

#[test]
fn a_withheld_page_is_requested_and_the_authoritative_reply_is_displayed_read_only() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    open(
        &mut app,
        &mut peer,
        vec![
            page(ObjectId(0x5000_0002), Some("one")),
            page(ObjectId(0x5000_0002), None),
        ],
    );
    assert_eq!(gameplay(&mut app).book.cur_page, 0);
    let _ = actions_sent(&mut app);
    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);
    assert_eq!(
        actions_sent(&mut app),
        vec![(0x00AE, [BOOK.0.to_le_bytes(), 1u32.to_le_bytes()].concat())]
    );

    peer.event(
        &mut app,
        &dereth_protocol::trade::BookPageDataResponse {
            object_id: BOOK,
            page: 1,
            data: page(ObjectId(0x5000_0002), Some("the shard supplied this page")),
        },
    );
    settle(&mut app);
    let ui = &mut app.ui_mut().expect("shell").ui;
    let h = ui
        .get_element(dereth_ui_screens::panels::book::PAGE_TEXT)
        .expect("page text");
    let t = ui.text_element_mut(h).expect("text");
    assert_eq!(t.glyphs.inq_text(false), "the shard supplied this page");
    assert!(
        !t.bits.editable(),
        "somebody else's ordinary page stays read-only"
    );
}

/// Closing the current page deletes an owned blank page locally before navigation, then a forward
/// destination is decremented because the later page shifted left.
#[test]
fn physical_next_deletes_an_owned_blank_page_and_follows_the_shifted_page() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    open(
        &mut app,
        &mut peer,
        vec![
            page(PLAYER, Some("")),
            page(ObjectId(0x5000_0002), Some("shifted into page zero")),
        ],
    );
    let _ = actions_sent(&mut app);

    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);
    assert_eq!(
        actions_sent(&mut app),
        vec![(0x00AD, [BOOK.0.to_le_bytes(), 0u32.to_le_bytes()].concat())],
        "DeletePage is an independent retail literal with book/page body"
    );
    let book = &gameplay(&mut app).book;
    assert_eq!(
        book.cur_page, 0,
        "the forward destination follows the shifted page index"
    );
    assert_eq!(
        book.pages.len(),
        1,
        "the blank page is removed locally before 0x00B7"
    );
    assert_eq!(book.page_body, "shifted into page zero");
}

/// A forward turn from the player's already-blank last page is refused before current-page close
/// can delete it. Retail names the object in a type-`0x1A` notice and leaves both the page and
/// current selection unchanged.
#[test]
fn physical_next_from_an_owned_blank_page_refuses_without_closing_or_mutating_it() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    place_book_at_player(&mut app, &mut peer, true);
    let named = app
        .objects_mut()
        .world
        .weenie_mut(BOOK)
        .expect("placed book");
    named.pwd.stack_size = Some(2);
    named.pwd.plural_name = Some("Range Books".to_owned());
    open_with_max(&mut app, &mut peer, 3, vec![page(PLAYER, Some(""))]);
    let _ = actions_sent(&mut app);
    let turns = gameplay(&mut app).book.page_turns;
    assert_eq!(
        gameplay(&mut app).book.title_text,
        "Draft",
        "the signed book displays its inscription, not its object name"
    );

    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);

    assert!(
        actions_sent(&mut app).is_empty(),
        "the refusal sends no DeletePage or AddPage"
    );
    let book = &gameplay(&mut app).book;
    assert_eq!(book.cur_page, 0, "the refusal keeps the current page");
    assert_eq!(book.page_turns, turns, "the refusal is not a page turn");
    assert_eq!(
        book.pages.len(),
        1,
        "closing the current page did not delete the owned blank page"
    );
    assert_eq!(book.pages[0].text.as_deref(), Some(""));
    assert_eq!(
        spew(&app),
        vec!["The Range Books is already open to a blank page".to_owned()],
        "the real Next gesture raises a type-0x1A display-string notice and reaches the spew box"
    );
}

/// Opening the writing menu builds one runtime row for each real page, followed by `"(blank)"`
/// rows through `max_num_pages`. A physical menu choice raises message 7 and uses the selected row
/// index as the requested current page.
#[test]
fn book_open_builds_real_page_menu_rows_and_physical_choices_turn_or_refuse() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    place_book_at_player(&mut app, &mut peer, true);
    let mut anonymous = page(ObjectId(0x5000_0002), Some("anonymous second page"));
    anonymous.ignore_author = 1;
    anonymous.author_name.clear();
    open_with_max(
        &mut app,
        &mut peer,
        4,
        vec![page(ObjectId(0x5000_0002), Some("first page")), anonymous],
    );
    let _ = actions_sent(&mut app);

    let menu = app
        .ui()
        .expect("shell")
        .ui
        .get_element(dereth_ui_screens::panels::book::PAGE_MENU)
        .expect("page menu");
    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        assert_eq!(
            dereth_ui::widgets::menu::num_items(ui, menu),
            4,
            "all max-page rows are live"
        );
        let labels: Vec<_> = (0..4)
            .map(|i| {
                let row = dereth_ui::widgets::menu::get_item(ui, menu, i).expect("runtime row");
                ui.text_element_mut(row)
                    .map(|t| t.glyphs.inq_text(false))
                    .expect("row text")
            })
            .collect();
        assert_eq!(labels, ["- Somebody", "", "(blank)", "(blank)"]);
        assert_eq!(
            dereth_ui::widgets::menu::selected_index(ui, menu),
            0,
            "open selects the current page"
        );
    }

    click(&mut app, dereth_ui_screens::panels::book::PAGE_MENU);
    let second = dereth_ui::widgets::menu::get_item(&app.ui().expect("shell").ui, menu, 1)
        .expect("second row");
    click_handle(&mut app, second);
    settle(&mut app);
    assert_eq!(
        (
            app.objects_mut().world.book_session_view().current_page,
            gameplay(&mut app).book.cur_page,
        ),
        (1, 1),
        "the physical row click turns the shared session and its projection to page two"
    );
    assert_eq!(gameplay(&mut app).book.page_body, "anonymous second page");
    assert!(
        actions_sent(&mut app).is_empty(),
        "a loaded page turn sends no writing request"
    );

    click(&mut app, dereth_ui_screens::panels::book::PAGE_MENU);
    let skipped = dereth_ui::widgets::menu::get_item(&app.ui().expect("shell").ui, menu, 3)
        .expect("fourth row");
    click_handle(&mut app, skipped);
    settle(&mut app);
    assert!(
        actions_sent(&mut app).is_empty(),
        "skipping past the page count sends no Add/Delete request"
    );
    assert_eq!(
        gameplay(&mut app).book.cur_page,
        1,
        "the skip refusal keeps the current page"
    );
    assert_eq!(
        dereth_ui::widgets::menu::selected_index(&app.ui().expect("shell").ui, menu),
        1,
        "the refused page change restores the menu selection to the current page"
    );
    assert_eq!(
        spew(&app),
        vec!["The Range Book is already open to a blank page".to_owned()],
        "the menu skip reaches the same exact type-0x1A refusal"
    );
}

/// Author labels depend on names while nonzero editing flags allow a non-owner to write.
#[test]
fn raw_ignore_author_flags_keep_the_author_label_and_allow_nonzero_editing() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();

    for (raw, expected_label, expected_editable) in [
        (0, "- Raw Author", false),
        (1, "- Raw Author", true),
        (2, "- Raw Author", true),
    ] {
        let mut p = page(ObjectId(0x5000_0002), Some("raw flag page"));
        p.author_name = "Raw Author".into();
        p.ignore_author = raw;
        open_with_max(&mut app, &mut peer, 1, vec![p]);

        let ui = &mut app.ui_mut().expect("shell").ui;
        let menu = ui
            .get_element(dereth_ui_screens::panels::book::PAGE_MENU)
            .expect("page menu");
        let row = dereth_ui::widgets::menu::get_item(ui, menu, 0).expect("first runtime row");
        assert_eq!(
            ui.text_element_mut(row)
                .expect("row text")
                .glyphs
                .inq_text(false),
            expected_label,
            "raw ignore_author {raw} does not hide a named author"
        );
        let page_text = ui
            .get_element(dereth_ui_screens::panels::book::PAGE_TEXT)
            .expect("page text");
        assert_eq!(
            ui.text_element_mut(page_text)
                .expect("text")
                .bits
                .editable(),
            expected_editable,
            "raw ignore_author {raw} follows the page display's nonzero rule"
        );
    }
}

/// Menu construction decorates each non-anonymous page with its author name. A viewer whose
/// authoritative player description marks it as a PSR additionally sees the account name in angle
/// brackets; retail does not fall back when that account string is empty.
#[test]
fn book_menu_decorates_authors_with_accounts_for_an_authoritative_psr_viewer() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    let mut credited = page(ObjectId(0x5000_0002), Some("credited page"));
    credited.author_name = "Visible Character".into();
    credited.author_account = "Account Name".into();
    let mut empty_account = page(ObjectId(0x5000_0003), Some("empty account page"));
    empty_account.author_name = "No Account".into();
    let mut anonymous = page(ObjectId(0x5000_0004), Some("anonymous page"));
    anonymous.author_name.clear();
    anonymous.author_account = "Hidden Account".into();
    anonymous.ignore_author = 1;
    let pages = vec![credited, empty_account, anonymous];

    open_with_max(&mut app, &mut peer, 4, pages.clone());
    let menu = app
        .ui()
        .expect("shell")
        .ui
        .get_element(dereth_ui_screens::panels::book::PAGE_MENU)
        .expect("page menu");
    let read_labels = |app: &mut App| {
        let ui = &mut app.ui_mut().expect("shell").ui;
        (0..4)
            .map(|i| {
                let row = dereth_ui::widgets::menu::get_item(ui, menu, i).expect("runtime row");
                ui.text_element_mut(row)
                    .map(|t| t.glyphs.inq_text(false))
                    .expect("row text")
            })
            .collect::<Vec<_>>()
    };
    assert_eq!(
        read_labels(&mut app),
        ["- Visible Character", "- No Account", "", "(blank)"],
        "an ordinary viewer sees plain author labels, empty-name precedence, and blank padding"
    );

    // Establish the player-description lifetime through the production 0x0013 consumer before its
    // authoritative Boolean-quality update, matching the PSR fixture of the inscription tests.
    app.apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            dereth_protocol::login::LoginPlayerDescription::default(),
        )),
    ]);
    assert!(
        app.objects().world.player_qualities().is_some(),
        "the player descriptor is live"
    );
    set_psr_quality(&mut app, 1, 0x61, true);
    open_with_max(&mut app, &mut peer, 4, pages);
    assert_eq!(
        read_labels(&mut app),
        [
            "- Visible Character <Account Name>",
            "- No Account <>",
            "",
            "(blank)"
        ],
        "the same encoded pages must reach the real menu's PSR account-name branch"
    );
}

/// Both response arms that do not insert a page request the whole book again.
#[test]
fn failed_add_reply_requests_literal_book_data_without_an_optimistic_page() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    open(
        &mut app,
        &mut peer,
        vec![page(ObjectId(0x5000_0002), Some("published"))],
    );
    let _ = actions_sent(&mut app);
    click(&mut app, dereth_ui_screens::panels::book::NEXT_BUTTON);
    settle(&mut app);
    let _ = actions_sent(&mut app);

    peer.event(
        &mut app,
        &dereth_protocol::trade::WritingBookAddPageResponse {
            book_id: BOOK,
            page_number: 1,
            success: 0,
        },
    );
    settle(&mut app);
    assert_eq!(
        actions_sent(&mut app),
        vec![(0x00AA, BOOK.0.to_le_bytes().to_vec())],
        "failed AddPage refetches BookData using the independently pinned retail literal"
    );
    let book = &gameplay(&mut app).book;
    assert_eq!(
        book.pages.len(),
        1,
        "a failed response cannot create an optimistic page"
    );
    assert!(
        book.request_pending,
        "retail leaves the request pending until the refetch arrives"
    );
}

/// Hiding the book window closes the book on the visibility falling edge, which commits the
/// current editable page before clearing the panel's book state.
#[test]
fn hiding_the_real_book_window_commits_the_current_owned_page() {
    let _gpu = gpu_lock();
    let (mut app, mut peer) = setup();
    place_book_at_player(&mut app, &mut peer, false);
    open(&mut app, &mut peer, vec![page(PLAYER, Some("draft"))]);
    assert!(
        app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Book, BOOK),
        "the unowned book is watched before the manual close"
    );
    let _ = actions_sent(&mut app);
    click(&mut app, dereth_ui_screens::panels::book::PAGE_TEXT);
    type_text(&mut app, " revised");

    {
        let ui = &mut app.ui_mut().expect("shell").ui;
        let window = ui
            .get_element(dereth_ui_screens::panels::book::WINDOW)
            .expect("real book window");
        ui.set_visible(window, false);
    }
    settle(&mut app);
    let sent = actions_sent(&mut app);
    assert_eq!(
        sent.len(),
        1,
        "the visibility edge commits once: {sent:02x?}"
    );
    assert_eq!(
        sent[0].0, 0x00AB,
        "close uses the independently pinned ModifyPage literal"
    );
    let mut r = dereth_protocol::archive::Reader::new(&sent[0].1);
    let modify = dereth_protocol::trade::WritingBookModifyPage::read(&mut r).expect("modify body");
    assert_eq!(modify.book_id, BOOK);
    assert_eq!(modify.page, 0);
    assert_eq!(modify.text, "draft revised");
    assert_eq!(
        gameplay(&mut app).book.book_id,
        None,
        "closing the book clears the panel's book id"
    );
    assert!(
        !app.objects()
            .world
            .object_range_checks
            .is_watching(dereth_client_model::range::RangeHandler::Book, BOOK),
        "the same hidden edge unregisters every book range watch"
    );
}
