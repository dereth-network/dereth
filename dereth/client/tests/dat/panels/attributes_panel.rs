//! The attributes page selects, costs and spends: selecting an attribute row fills the footer with
//! that row's own numbers, the raise buttons send `0x0045 TrainAttribute` or
//! `0x0044 TrainAttribute2nd` (a vital row names the odd, maximum id), a vital raise's recorded
//! answer clears the awaiting-raise latch on its own, and the header shows the heritage line and
//! the PK status. The two stat pages do not answer for each other's footer buttons.
//! Fixture: the retail dats and the recordings that carry attribute training, replayed into a
//! headless gameplay `App` with no socket; every capture-backed cost, id, deduction and level
//! comes from that traffic and the page's own link, read from `ClientNetwork`'s outgoing queue.
//! The formatter and PK truth-table cases are synthetic and labelled where they appear.

use crate::common::captures_dir;
use crate::common::client_dir;

use std::collections::BTreeMap;
use std::net::SocketAddr;

use dereth_client::app::App;
use dereth_client::net::ClientNetwork;
use dereth_client::objects::ObjectStream;
use dereth_client_model::qualities::update as qupdate;
use dereth_client_net::client_session::SessionEvent;
use dereth_primitives::{LocalTime, ObjectId};
use dereth_transport::wire::ParsedPacket;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::panels::{attributes, statmgmt};

// ---------------------------------------------------------------------------------------------
// Harness. The same shape as `skill_advancement.rs`.
// ---------------------------------------------------------------------------------------------

fn dat_store() -> dereth_dat::RetailDatStore {
    dereth_dat::RetailDatStore::open_dir(&client_dir()).unwrap_or_else(|e| {
        panic!(
            "the retail dats at {} are this test's oracle: {e}",
            client_dir().display()
        )
    })
}

fn app_in_gameplay(frames: u32) -> App {
    crate::common::app::app_in_gameplay(frames, Some(ObjectId(0x5000_0001)))
}

use crate::common::app::gameplay_screen;

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records)
        .expect("the capture has no LoginRequest")
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

/// One `[F7B1][stamp][opcode][body]`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SentAction {
    queue: u16,
    stamp: u32,
    opcode: u32,
    body: Vec<u8>,
}

fn actions_in(datagrams: &[(Vec<u8>, SocketAddr)]) -> Vec<SentAction> {
    let mut out = Vec::new();
    for (raw, _) in datagrams {
        let p = ParsedPacket::parse(raw).expect("this process's own datagram parses");
        for f in &p.fragments {
            if f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4 {
                continue;
            }
            if u32::from_le_bytes(f.payload[0..4].try_into().expect("4"))
                != dereth_protocol::OrderedActionHeader::MAGIC
            {
                continue;
            }
            out.push(SentAction {
                queue: f.header.queue_id,
                stamp: u32::from_le_bytes(f.payload[4..8].try_into().expect("4")),
                opcode: u32::from_le_bytes(f.payload[8..12].try_into().expect("4")),
                body: f.payload[12..].to_vec(),
            });
        }
    }
    out
}

/// The retail client's own game actions, from fragment 0 of each blob.
fn captured_actions(session: &str) -> Vec<(f64, SentAction)> {
    let mut out = Vec::new();
    for r in load(session) {
        if !r.c2s {
            continue;
        }
        let Ok(p) = ParsedPacket::parse(&r.raw) else {
            continue;
        };
        for f in &p.fragments {
            if f.header.blob_num != 0
                || f.payload.len() < dereth_protocol::OrderedActionHeader::PACK_SIZE + 4
            {
                continue;
            }
            if u32::from_le_bytes(f.payload[0..4].try_into().expect("4"))
                != dereth_protocol::OrderedActionHeader::MAGIC
            {
                continue;
            }
            out.push((
                r.t,
                SentAction {
                    queue: f.header.queue_id,
                    stamp: u32::from_le_bytes(f.payload[4..8].try_into().expect("4")),
                    opcode: u32::from_le_bytes(f.payload[8..12].try_into().expect("4")),
                    body: f.payload[12..].to_vec(),
                },
            ));
        }
    }
    out
}

/// The two opcodes the attributes panel emits.
const ATTRIBUTE_OPCODES: [u32; 2] = [0x0044, 0x0045];

fn corpus_sessions() -> Vec<String> {
    let mut sessions: Vec<String> = std::fs::read_dir(captures_dir())
        .expect("captures dir")
        .filter_map(Result::ok)
        .filter(|e| !crate::common::is_unclean_logout_recording(&e.path()))
        .filter_map(|e| {
            let p = e.path();
            if p.extension().and_then(|x| x.to_str()) != Some("jsonl") {
                return None;
            }
            p.file_stem().and_then(|x| x.to_str()).map(str::to_owned)
        })
        .collect();
    sessions.sort();
    assert!(
        !sessions.is_empty(),
        "no captures in {}",
        captures_dir().display()
    );
    sessions
}

fn sessions_with_attribute_traffic() -> Vec<String> {
    corpus_sessions()
        .into_iter()
        .filter(|s| {
            captured_actions(s)
                .iter()
                .any(|(_, a)| ATTRIBUTE_OPCODES.contains(&a.opcode))
        })
        .collect()
}

/// `[id][amount]`, the body both attribute train messages share.
fn train_body(a: &SentAction) -> (u32, u32) {
    assert_eq!(a.body.len(), 8, "a train body is [id][amount]: {a:?}");
    (
        u32::from_le_bytes(a.body[0..4].try_into().expect("4")),
        u32::from_le_bytes(a.body[4..8].try_into().expect("4")),
    )
}

fn replay_timed(session: &str) -> Vec<(f64, SessionEvent)> {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            events.push((r.t, e));
        }
    }
    assert!(
        events
            .iter()
            .any(|(_, e)| matches!(e, SessionEvent::PlayerDescription(_))),
        "{session} never reached 0x0013, so it cannot be this test's oracle"
    );
    events
}

/// Replay a capture's server half into an [`ObjectStream`], stopping once the **player's own
/// weenie** exists.
///
/// `App::apply_hud_events` feeds the HUD out of a list of `SessionEvent`s and never touches the
/// object table, so an app built that way has qualities but no weenies. The original PK-header
/// update reads both predicates from the player object. This
/// is the object half, installed into the app with `App::objects_mut` so the header has the same
/// world the running client would give it.
fn replay_objects(session: &str) -> ObjectStream {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if objects
            .world
            .player
            .is_some_and(|p| objects.world.weenie(p).is_some())
        {
            return objects;
        }
    }
    panic!("{session} never created the player's own weenie, so it cannot be the PK oracle");
}

/// A live, in-world link this process can `send_action` on. Nothing is sent to anything: the
/// socket is a loopback the test owns and the datagrams are read back out of its own queue.
fn in_world_link(session: &str) -> ClientNetwork {
    let records = load(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(&records),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    for r in &records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, addr(r.pair), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let mut seen = false;
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
            seen |= matches!(e, SessionEvent::PlayerDescription(_));
        }
        if seen {
            break;
        }
    }
    let _ = net.take_outgoing();
    net
}

const ANSWER_WINDOW_S: f64 = 0.5;

struct RoundTrip {
    session: String,
    index: usize,
    when: f64,
    request: SentAction,
    answers: Vec<SessionEvent>,
}

/// Split one capture into (everything before the first attribute request, one trip per request).
///
/// The prelude matters: `AvailableExperience` is moved by messages the server sends before any
/// raise, and a footer read that has not seen them is reading `0x0013` rather than the state the
/// retail client was looking at when it pressed the button.
///
/// Pairing is deliberately bounded: a trip takes answers from its request until the earlier of
/// the next train request or `ANSWER_WINDOW_S` (0.5 seconds). A non-attribute train trip carries
/// its in-window updates into the next attribute trip; events after that window and before the next
/// request are not assigned to either trip. The prelude is the accumulated state before the first
/// attribute request, not every later update in the capture.
fn round_trips(session: &str) -> (Vec<SessionEvent>, Vec<RoundTrip>) {
    let timed = replay_timed(session);
    let first_desc = timed
        .iter()
        .position(|(_, e)| matches!(e, SessionEvent::PlayerDescription(_)))
        .expect("the capture reached 0x0013");
    let last = timed[first_desc..]
        .iter()
        .position(|(_, e)| {
            matches!(
                e,
                SessionEvent::LoggedOff
                    | SessionEvent::StateChanged(
                        dereth_client_net::client_session::SessionState::CharacterSelect
                            | dereth_client_net::client_session::SessionState::Disconnected(_)
                    )
            )
        })
        .map_or(timed.len(), |i| first_desc + i);
    let timed = &timed[..last];

    // Every train request, not just the attribute ones: a skill raise between two attribute
    // raises still moves `AvailableExperience`, and its answer belongs to the prelude of the next
    // attribute trip rather than being dropped.
    let reqs: Vec<(f64, SentAction)> = captured_actions(session)
        .into_iter()
        .filter(|(_, a)| (0x0044..=0x0047).contains(&a.opcode))
        .collect();

    let mut trips = Vec::new();
    let mut carry: Vec<SessionEvent> = Vec::new();
    let mut prelude: Option<Vec<SessionEvent>> = None;
    let mut cursor = 0usize;
    for (i, (t, a)) in reqs.iter().enumerate() {
        while cursor < timed.len() && timed[cursor].0 < *t {
            carry.push(timed[cursor].1.clone());
            cursor += 1;
        }
        let next = reqs.get(i + 1).map_or(f64::INFINITY, |(t, _)| *t);
        let close = next.min(t + ANSWER_WINDOW_S);
        let mut answers = Vec::new();
        while cursor < timed.len() && timed[cursor].0 < close {
            answers.push(timed[cursor].1.clone());
            cursor += 1;
        }
        if ATTRIBUTE_OPCODES.contains(&a.opcode) {
            if prelude.is_none() {
                prelude = Some(std::mem::take(&mut carry));
            } else {
                // A non-attribute trip's events are handed to the next attribute trip, so the
                // state each one is read against is still the capture's.
                answers.splice(0..0, std::mem::take(&mut carry));
            }
            trips.push(RoundTrip {
                session: session.to_owned(),
                index: i,
                when: *t,
                request: a.clone(),
                answers,
            });
        } else {
            carry.extend(answers);
        }
        while cursor < timed.len() && timed[cursor].0 < next {
            cursor += 1;
        }
    }
    (prelude.unwrap_or_default(), trips)
}

fn decoded_answers(
    answers: &[SessionEvent],
) -> Vec<(dereth_protocol::Opcode, qupdate::QualityUpdate)> {
    answers
        .iter()
        .filter_map(|e| match e {
            SessionEvent::UiEvent { opcode, .. } if qupdate::is_update_opcode(*opcode) => {
                let u = qupdate::decode(*opcode, e.ui_body().expect("UI event").1)
                    .unwrap_or_else(|| panic!("the capture's own {opcode:?} decodes"));
                (u.subject.is_none()).then_some((*opcode, u))
            }
            _ => None,
        })
        .collect()
}

fn answer_on(
    answers: &[SessionEvent],
    t: dereth_client_model::StatType,
    property: u32,
) -> Option<(dereth_protocol::Opcode, qupdate::QualityUpdate)> {
    let key = dereth_client_model::StatKey::new(t, property);
    decoded_answers(answers)
        .into_iter()
        .find(|(_, u)| u.key == key)
}

/// One `ID_StatManagement_*` row of `StringTable 0x23000001`, read straight off the dats.
///
/// The panel resolves the same row through `UiSystem`'s string service; this goes to the file, so
/// a footer that drew the *other* subclass's title -- the mistake this file is checking for --
/// cannot be masked by both sides sharing a lookup.
fn string_row(token: &str) -> String {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(0x2300_0001);
    let b = dat_store()
        .read(id)
        .expect("the StatManagement StringTable is in the dats");
    let t = dereth_assets::ui::StringTable::decode_payload(id, &b).expect("StringTable");
    let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
    t.strings
        .iter()
        .find(|(k, _)| *k == hash)
        .and_then(|(_, s)| s.strings.first().cloned())
        .unwrap_or_else(|| panic!("{token} is not in StringTable 0x23000001"))
}

/// The experience table `0x0E000018`, loaded independently from the dats. The test and panel
/// share the decoder and cost helpers; this independent load checks the footer's routing and
/// captured data, not an independent arithmetic implementation.
fn xp_table() -> dereth_assets::tables::XpTable {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(0x0E00_0018);
    let b = dat_store()
        .read(id)
        .expect("the experience table is in the dats");
    dereth_assets::tables::XpTable::decode_payload(id, &b).expect("the experience table decodes")
}

// ---------------------------------------------------------------------------------------------
// Reading the live page back.
// ---------------------------------------------------------------------------------------------

fn attribute_panel(app: &mut App) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    ui.get_child_recursive(page, attributes::PANEL)
        .expect("the attributes panel")
}

/// The sub-panel state that selects one of the three stacked footer containers.
fn panel_state(app: &mut App) -> u32 {
    let panel = attribute_panel(app);
    let (ui, _) = gameplay_screen(app);
    ui.node(panel).map_or(0, |n| n.state.0)
}

fn footer_child(app: &mut App, child: u32) -> ElemHandle {
    let panel = attribute_panel(app);
    let (ui, _) = gameplay_screen(app);
    let state = ui.node(panel).map_or(0, |n| n.state.0);
    let c = ui
        .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container the panel's own state names");
    ui.get_child_recursive(c, ElementId(child))
        .expect("the footer child")
}

fn footer_text(app: &mut App, child: u32) -> String {
    let h = footer_child(app, child);
    let (ui, _) = gameplay_screen(app);
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

fn footer_state(app: &mut App, child: u32) -> u32 {
    let h = footer_child(app, child);
    let (ui, _) = gameplay_screen(app);
    ui.node(h)
        .map(|n| n.state.0)
        .expect("the button has a state")
}

/// One header child's live text, resolved off the sub-panel itself (the header hangs off it, not
/// off a footer container).
fn header_text(app: &mut App, child: u32) -> String {
    let panel = attribute_panel(app);
    let (ui, _) = gameplay_screen(app);
    let h = ui
        .get_child_recursive(panel, ElementId(child))
        .expect("the header child");
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Make the character page visible and select the sub-panel `h` belongs to, so a real pointer
/// press can reach it.
///
/// The original list-box hit test first refuses a list that the pointer is not over, so a row on a
/// hidden page cannot be selected by a player and must not be selectable here either. Setup uses
/// the current UI machinery directly: it invokes the panel-visibility receiver, then broadcasts
/// `MOUSE_CLICK` to the tab named by the live panel's `tab_to_page` map. The row press itself uses
/// the pointer producer below.
fn show_page_for(app: &mut App, h: ElemHandle) {
    let page = dereth_ui_screens::panels::remaining::CHARACTER_PAGE;
    {
        let (ui, screen) = gameplay_screen(app);
        if let Some(panel_id) = screen
            .panels
            .pages
            .iter()
            .find(|p| p.element == page)
            .map(|p| p.panel_id)
        {
            screen.recv_set_panel_visibility(ui, panel_id, true);
        }
    }
    app.frame();
    // Which sub-panel is this row in? Walk up to whichever of the two the row sits under.
    let sub = {
        let (ui, _) = gameplay_screen(app);
        let mut cur = Some(h);
        let mut found = None;
        while let Some(c) = cur {
            let id = ui.node(c).map(dereth_ui::ElementNode::element_id);
            if id == Some(dereth_ui_screens::panels::skills::PANEL)
                || id == Some(dereth_ui_screens::panels::attributes::PANEL)
            {
                found = id;
                break;
            }
            cur = ui.parent(c);
        }
        found
    };
    if let Some(sub) = sub {
        let tab = {
            let (ui, screen) = gameplay_screen(app);
            let root = screen.root().expect("the gameplay root");
            ui.get_child_recursive(root, page)
                .and_then(|ph| {
                    let n = ui.node(ph)?;
                    let b = n.behaviour.as_ref()?;
                    let p = (**b)
                        .as_any()?
                        .downcast_ref::<dereth_ui::widgets::panel::Panel>()?;
                    p.tab_to_page
                        .iter()
                        .find(|(_, pg)| **pg == sub)
                        .map(|(t, _)| *t)
                })
                .and_then(|t| ui.get_child_recursive(root, t))
        };
        if let Some(th) = tab {
            let (ui, _) = gameplay_screen(app);
            ui.broadcast_element_message(th, dereth_ui::msg::element::id::MOUSE_CLICK, 0, 0);
        }
    }
    for _ in 0..3 {
        app.frame();
    }
}
/// Press a list row **through the real producer**: `UiSystem::mouse_down` at the row's centre,
/// which broadcasts the `0x1C` on the element the pointer actually hits.
///
/// A real press never lands on a row (a field with no tooltip is not mouse-visible); it lands on
/// the **list box**, and the panel finds the row under the pointer from there. Broadcasting the
/// `0x1C` on the row element itself would bypass that producer and select a row no player could.
fn press_row(app: &mut App, h: ElemHandle) {
    show_page_for(app, h);
    // A row below the list box's own rectangle cannot be pressed: the original point-to-index
    // path refuses a y coordinate at or below the list height, for a player as for a test. Scroll
    // it into view first, which is what the list box's scrollbar does: `scroll_to_view` drives
    // the element's own scroll state, which is what the bar drives too.
    {
        let mut panels = std::mem::take(&mut app.hud_mut().panels);
        {
            let (ui, _) = gameplay_screen(app);
            for w in [panels.skills.list.as_mut(), panels.attributes.list.as_mut()]
                .into_iter()
                .flatten()
            {
                if let Some(i) = w.index_of(h) {
                    w.scroll_to_view(ui, i);
                }
            }
        }
        app.hud_mut().panels = panels;
    }
    {
        let (ui, _) = gameplay_screen(app);
        let b = ui.screen_box(h);
        ui.mouse_down(
            dereth_ui::focus::action::PRIMARY_CLICK,
            (b.x0 + b.x1) / 2,
            (b.y0 + b.y1) / 2,
        );
    }
    app.frame();
}

/// Deliver the chosen footer button's `BUTTON_CLICKED` message directly. Row selection above uses
/// pointer hit testing; this helper deliberately starts at the button-handler boundary and covers
/// request ownership and wire output, not footer-button pointer hit testing.
fn click_footer_button(app: &mut App, child: u32) {
    let h = footer_child(app, child);
    {
        let (ui, _) = gameplay_screen(app);
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 7, 0);
    }
    // Two frames, and the second is not slack. `App::frame` runs the packet controller
    // (`link.packet_controller_use_time`) near its top and the command slot
    // (`interaction_use_time`) near its bottom — the original client's packet-controller-before-UI
    // order — so this click's action becomes a datagram on the *next* frame's controller pass.
    app.frame();
    app.frame();
}

fn row_of(app: &App, stat: u32, secondary: bool) -> ElemHandle {
    app.hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.stat == stat && r.secondary == secondary)
        .unwrap_or_else(|| panic!("no row for stat {stat} secondary={secondary}"))
        .element
}

fn app_with(events: &[SessionEvent]) -> App {
    let mut app = app_in_gameplay(4);
    let _ = app.apply_hud_events(events);
    for _ in 0..4 {
        app.frame();
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    app
}

/// [`app_with`], plus the capture's own socket-free endpoint so that what the panels send can be
/// observed at the wire. See [`wire_actions`].
fn app_with_link(events: &[SessionEvent], session: &str) -> App {
    let mut app = app_in_gameplay(4);
    app.attach_replay_network(in_world_link(session))
        .expect("a headless app with no link takes the replay endpoint");
    let _ = app.apply_hud_events(events);
    for _ in 0..4 {
        app.frame();
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let _ = wire_actions(&mut app);
    app
}

/// Every game action `app` has put on the wire since this was last called, in send order.
///
/// The original attributes page's raise arms send the primary or secondary attribute request
/// synchronously from inside the element callback, and the request owners here run synchronously
/// too, so the click-driven assertions read the wire rather than an outbox. Order and
/// multiplicity are preserved (this returns a `Vec`), and the opcode and body are checked.
fn wire_actions(app: &mut App) -> Vec<SentAction> {
    let out = app
        .replay_network_mut()
        .expect("this app was built by app_with_link")
        .take_outgoing();
    actions_in(&out)
}

// =============================================================================================
// 1. The wire ids the nine rows report.
// =============================================================================================

/// Behaviour: character-page.raise.both-buttons-on-both-pages-put-the-request-on-the-wire
/// **A vital raise names the *odd*, maximum id on the wire, and the capture says so.**
///
/// The original panel constructs the three secondary-attribute rows with the **even**, current ids
/// `2, 4, 6`. Each row reports the maximum-attribute id used by a raise request, set to one below
/// that current-id constructor argument. So the Health row asks the server to raise `1 MaxHealth`.
///
/// Getting this wrong is invisible on screen and wrong on the wire, which is why it is asserted
/// against the traffic and not against the transcription: **every `0x0044` in the corpus carries
/// id 1**, and there is no `0x0044` anywhere carrying an even id.
///
/// **Falsified by** making `AttributeRow::wire_stat` return `self.stat` for a secondary: the
/// corpus assertion fails on the first `0x0044`.
#[test]
fn a_vital_row_puts_the_maximum_id_on_the_wire_and_an_attribute_row_its_own() {
    let mut app = app_in_gameplay(4);
    let _ = app.apply_hud_events(&[]);
    app.frame();
    let rows = &app.hud().panels.attributes.rows;
    assert_eq!(
        rows.len(),
        9,
        "initial panel construction builds six attributes and three vitals"
    );

    let primaries: Vec<(u32, u32)> = rows
        .iter()
        .filter(|r| !r.secondary)
        .map(|r| (r.stat, r.wire_stat()))
        .collect();
    assert_eq!(
        primaries,
        vec![(1, 1), (2, 2), (4, 4), (3, 3), (5, 5), (6, 6)],
        "the primary-attribute row reports its stored attribute id unchanged"
    );
    let secondaries: Vec<(u32, u32)> = rows
        .iter()
        .filter(|r| r.secondary)
        .map(|r| (r.stat, r.wire_stat()))
        .collect();
    assert_eq!(
        secondaries,
        vec![(2, 1), (4, 3), (6, 5)],
        "the secondary-attribute row reports the maximum id, one below the current-id constructor argument"
    );

    // And the corpus agrees. Every recorded vital raise names an odd id; every recorded attribute
    // raise names one of the six primaries.
    let mut seen: BTreeMap<u32, Vec<u32>> = BTreeMap::new();
    for session in sessions_with_attribute_traffic() {
        for (_, a) in captured_actions(&session) {
            if !ATTRIBUTE_OPCODES.contains(&a.opcode) {
                continue;
            }
            let (id, _) = train_body(&a);
            seen.entry(a.opcode).or_default().push(id);
        }
    }
    let vitals = seen.get(&0x0044).expect("the corpus carries a 0x0044");
    let attrs = seen.get(&0x0045).expect("the corpus carries a 0x0045");
    assert!(
        vitals.iter().all(|id| [1, 3, 5].contains(id)),
        "every captured Train_TrainAttribute2nd names a maximum id: {vitals:?}"
    );
    assert!(
        attrs.iter().all(|id| (1..=6).contains(id)),
        "every captured Train_TrainAttribute names a primary: {attrs:?}"
    );
    eprintln!("captured attribute ids {seen:#X?}");
}

// =============================================================================================
// 2. Selection and the two footers.
// =============================================================================================

/// Behaviour: attributes.footer.selecting-a-row-fills-the-footer-with-its-own-numbers
/// **Pressing a row selects it, fills its footer with that row's own numbers, and pressing it
/// again puts the default footer back.**
///
/// Every number checked here is either the capture's (`AvailableExperience`,
/// `AvailableSkillCredits`, the attribute's own displayed value) or `dereth_client_model::advancement`'s
/// answer recomputed in this test from a `XpTable` read straight off the dats -- so a footer that
/// looks plausible and is wrong fails.
///
/// The **state** is the other half of the claim: the original selection update writes state
/// `0x10000012` unconditionally before it branches, so the attributes page never reaches the meter
/// container `0x10000013`, unlike the skills page. That is checked for all nine rows.
///
/// **Falsified by** deleting the `0x1C MOUSE_PRESS` arm from
/// `AttributesPanel::on_element_message` (no row selects and the footer stays default), and,
/// separately, by making the selection update write `state::SELECTION_METER` (the container the
/// getters read has no `0x100005EB` and `footer_child` panics).
#[test]
fn selecting_a_row_fills_the_footer_with_that_rows_own_numbers() {
    const SESSION: &str = "short-play-with-training";
    let (prelude, _) = round_trips(SESSION);
    let xp = xp_table();
    let mut app = app_with(&prelude);

    let q = app
        .objects()
        .world
        .player_qualities()
        .expect("the capture's 0x0013")
        .clone();
    let credits = i64::from(q.inq_int(dereth_client::hud::AVAILABLE_SKILL_CREDITS));
    let available = match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        2,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => n,
        other => panic!("the capture has an AvailableExperience, got {other:?}"),
    };

    // ---- the default footer, before anything is selected ------------------------------------
    assert_eq!(
        app.hud().panels.attributes.selected_index,
        -1,
        "attributes-panel construction leaves no row selected"
    );
    assert_eq!(panel_state(&mut app), statmgmt::state::DEFAULT);
    let default_title = string_row(statmgmt::string::DEFAULT_ATTRIBUTE_TITLE);
    assert_ne!(
        default_title,
        string_row(statmgmt::string::DEFAULT_SKILL_TITLE),
        "the two subclasses' default titles are different rows, so the test can tell them apart"
    );
    assert_eq!(
        footer_text(&mut app, statmgmt::child::TITLE),
        default_title,
        "the default footer uses the attribute title id, not the skill title id"
    );
    eprintln!("the attributes page's default footer title is {default_title:?}");
    // The original string substitution path formats each integer with the active language's
    // thousands separator, so every one of these values is grouped.
    assert_eq!(
        footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
        statmgmt::num(credits),
        "the attributes page's default footer really does show skill credits"
    );
    assert_eq!(
        footer_text(&mut app, statmgmt::child::LINE_TWO_VALUE),
        statmgmt::num(available)
    );

    // ---- every one of the nine rows ---------------------------------------------------------
    let rows: Vec<(u32, bool, String, ElemHandle)> = app
        .hud()
        .panels
        .attributes
        .rows
        .iter()
        .map(|r| (r.stat, r.secondary, r.name.clone(), r.element))
        .collect();
    for (i, (stat, secondary, name, h)) in rows.iter().enumerate() {
        let what = format!("{name} (stat {stat}, secondary {secondary})");
        press_row(&mut app, *h);

        assert_eq!(
            app.hud().panels.attributes.selected_index,
            i32::try_from(i).expect("nine rows"),
            "{what}: selecting a row stores that row's own index"
        );
        assert_eq!(
            panel_state(&mut app),
            statmgmt::state::SELECTION,
            "{what}: the attributes page never reaches the meter container"
        );

        // The row states: exactly one 6, eight 1s.
        let states: Vec<u32> = {
            let elems: Vec<ElemHandle> = app
                .hud()
                .panels
                .attributes
                .rows
                .iter()
                .map(|r| r.element)
                .collect();
            let (ui, _) = gameplay_screen(&mut app);
            elems
                .iter()
                .map(|e| ui.node(*e).map_or(0, |n| n.state.0))
                .collect()
        };
        let want: Vec<u32> = (0..9)
            .map(|j| {
                if j == i {
                    statmgmt::row_state::SELECTED
                } else {
                    statmgmt::row_state::UNSELECTED
                }
            })
            .collect();
        assert_eq!(
            states, want,
            "{what}: selection update leaves exactly one selected row"
        );

        // The cost, recomputed here from the capture's own record and the retail XP table.
        let (level_from_cp, cp_spent) = if *secondary {
            let v = q.attribute_2nd(stat - 1).expect("the capture's vital");
            (v.attribute.level_from_cp, v.attribute.cp_spent)
        } else {
            let a = q.attribute(*stat).expect("the capture's attribute");
            (a.level_from_cp, a.cp_spent)
        };
        let one = dereth_client_model::advancement::attribute_cost_to_raise(
            &xp,
            level_from_cp,
            cp_spent,
            *secondary,
        );
        let ten = dereth_client_model::advancement::attribute_cost_to_raise_10(
            &xp,
            level_from_cp,
            cp_spent,
            *secondary,
        );
        assert!(
            one > 0,
            "{what}: a level-1 character is nowhere near the attribute cap"
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
            statmgmt::num(one),
            "{what}: line one is the one-rank cost computed from this row's captured state"
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_TWO_VALUE),
            statmgmt::num(available),
            "{what}: line two is the server's own AvailableExperience"
        );
        assert_eq!(
            footer_text(&mut app, statmgmt::child::LINE_ONE_LABEL),
            string_row(statmgmt::string::XP_TO_RAISE_LABEL),
            "{what}: an attribute costs experience, not credits"
        );

        // The title carries the row's name and the number the page shows beside it.
        let title = footer_text(&mut app, statmgmt::child::TITLE);
        assert!(
            title.starts_with(&format!("{name}: ")),
            "{what}: title was {title:?}"
        );
        let shown = app
            .hud()
            .panels
            .attributes
            .rows
            .iter()
            .find(|r| r.element == *h)
            .expect("the row")
            .value
            .clone();
        if *secondary {
            // `"%s: %d/%d"` -- the row shows `current/maximum` and so does the title.
            assert!(
                title.contains(&shown),
                "{what}: the vital's title carries the row's own current/max pair \
                 ({title:?} vs {shown:?})"
            );
        } else {
            assert_eq!(title, format!("{name}: {shown}"), "{what}: `%s: %d`");
        }

        // Both buttons, against unassigned experience.
        assert_eq!(
            footer_state(&mut app, statmgmt::child::BUTTON),
            statmgmt::Footer::enable_for(u64::from(one), u64::try_from(available).expect("+ve")),
            "{what}: raise-1 is enabled iff the cost is non-zero and affordable"
        );
        assert_eq!(
            footer_state(&mut app, statmgmt::child::BUTTON_10),
            statmgmt::Footer::enable_for(u64::from(ten), u64::try_from(available).expect("+ve")),
            "{what}: and raise-10 against its own cost"
        );

        // Pressing the selected row again toggles the selected index back to -1.
        press_row(&mut app, *h);
        assert_eq!(
            app.hud().panels.attributes.selected_index,
            -1,
            "{what}: a second press deselects"
        );
        assert_eq!(panel_state(&mut app), statmgmt::state::DEFAULT);
        assert_eq!(
            footer_text(&mut app, statmgmt::child::TITLE),
            default_title,
            "{what}: and puts the default footer back"
        );
    }
    eprintln!("nine rows selected, costed and deselected against {SESSION}");
}

// =============================================================================================
// 3. Links 3 and 4 — the request, and the bytes.
// =============================================================================================

/// **Every attribute request in the corpus is the one this panel would emit, byte for byte.**
///
/// For each recorded `0x0045`/`0x0044`, in order:
///
/// 1. the qualities are advanced to the state assigned by [`round_trips`] (`0x0013`, the prelude,
///    and the carried in-window updates);
/// 2. the row that names that id is **selected by a press**, and the footer's own cost is read
///    off the screen;
/// 3. whichever of the two costs matches the amount the retail client asked for decides which
///    button is clicked -- so the *panel* chooses the amount, not the test;
/// 4. the datagram the app's **own** link puts on the wire is compared with the capture's, by
///    opcode, queue and body.
///
/// The test drives the app alone and reads the bytes off the app's own endpoint, with no
/// hand-written bridge between emitter and sender: the original raise arms send the attribute
/// request synchronously from inside the element callback, over one link. A request nobody owns
/// never becomes a datagram, so a deleted arm fails step 4.
///
/// **Falsified by** changing `AttributesPanel::raise_10_selection` to send `cost_to_raise`
/// (short-play-with-training's first `0x0045` is a raise-10 and its body stops matching), by swapping the
/// two `UiRequest` variants in `attributes::request_for` (the opcode assertion fails on the first
/// `0x0044`), or by deleting either arm from `run_ui_requests` (no datagram is produced at all).
#[test]
fn every_captured_attribute_request_is_the_one_this_panel_would_emit() {
    let xp = xp_table();
    // Keyed by `(opcode, was it the raise-10 button)`, so the report line says which of the four
    // (row type x button) combinations the corpus actually covers and which it does not.
    let mut checked: BTreeMap<(u32, bool), u32> = BTreeMap::new();
    // Denominator, so "the arm was reached" cannot be read off a loop that never ran.
    let mut routed = 0_u32;

    for session in sessions_with_attribute_traffic() {
        let (prelude, trips) = round_trips(&session);
        // The real routing path, and now the whole of it: the app's own frame turns a panel's
        // request into a `dereth_client_model::Request` and its own link puts the bytes on the wire, so
        // this test drives one object rather than a panel here and a router there.
        let mut app = app_with_link(&prelude, &session);

        for trip in &trips {
            let (id, amount) = train_body(&trip.request);
            let what = format!(
                "{}[{}] {:#06X} id={id} amount={amount} at t={:.3}",
                trip.session, trip.index, trip.request.opcode, trip.when
            );
            let secondary = trip.request.opcode == 0x0044;
            // The page's row for that id: a primary keys on the id itself, a vital on `id + 1`
            // (the row is built with the even, current id).
            let row_stat = if secondary { id + 1 } else { id };
            let h = row_of(&app, row_stat, secondary);

            press_row(&mut app, h);
            assert_eq!(
                app.hud()
                    .panels
                    .attributes
                    .selected()
                    .map(|r| r.wire_stat()),
                Some(id),
                "{what}: the selected row is the one the retail client raised"
            );

            let q = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .clone();
            let (level_from_cp, cp_spent) = if secondary {
                let v = q
                    .attribute_2nd(id)
                    .unwrap_or_else(|| panic!("{what}: no such vital"));
                (v.attribute.level_from_cp, v.attribute.cp_spent)
            } else {
                let a = q
                    .attribute(id)
                    .unwrap_or_else(|| panic!("{what}: no such attribute"));
                (a.level_from_cp, a.cp_spent)
            };
            let one = dereth_client_model::advancement::attribute_cost_to_raise(
                &xp,
                level_from_cp,
                cp_spent,
                secondary,
            );
            let ten = dereth_client_model::advancement::attribute_cost_to_raise_10(
                &xp,
                level_from_cp,
                cp_spent,
                secondary,
            );
            assert_eq!(
                footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
                statmgmt::num(one),
                "{what}: the footer shows the one-rank cost, grouped as every footer number is"
            );
            assert!(
                amount == one || amount == ten,
                "{what}: the retail client asked for {amount}; the one-rank cost is {one} and \
                 the ten-rank cost is {ten}. level_from_cp={level_from_cp} cp_spent={cp_spent}"
            );
            let (button, raise10) = if amount == one {
                (statmgmt::child::BUTTON, false)
            } else {
                (statmgmt::child::BUTTON_10, true)
            };

            // ---- the click ------------------------------------------------------------------
            //
            // The original raise arms send synchronously from inside the element callback, so
            // the app's own link carries the message and the assertion reads it there: one
            // request is one message; the id and amount
            // are still the retail client's, now checked as `trip.request.body` byte for byte
            // rather than as a `UiRequest` this test would have had to name itself; and the
            // opcode and queue are checked as well. "Nobody owns the raise" no longer needs its
            // own assertion, because an unowned request never becomes a datagram at all.
            let _ = wire_actions(&mut app);
            click_footer_button(&mut app, button);
            let sent = wire_actions(&mut app);
            routed += 1;
            assert_eq!(
                sent.len(),
                1,
                "{what}: one click, one game action (raise10={raise10}), got {sent:?}"
            );
            assert_eq!(sent[0].opcode, trip.request.opcode, "{what}: same opcode");
            assert_eq!(
                sent[0].body, trip.request.body,
                "{what}: same body, byte for byte"
            );
            assert_eq!(sent[0].queue, trip.request.queue, "{what}: same net queue");
            *checked.entry((trip.request.opcode, raise10)).or_default() += 1;
            assert!(
                app.hud().panels.attributes.awaiting_raise,
                "{what}: the awaiting-raise latch is set after the first request"
            );
            assert_eq!(
                footer_state(&mut app, button),
                statmgmt::button_state::DISABLED,
                "{what}: and puts the button it was clicked on into 0x0D"
            );

            // A second click while the latch is up sends nothing -- the whole double-spend guard.
            // At the wire for the same reason: at the outbox this could no longer fail.
            click_footer_button(&mut app, button);
            assert!(
                wire_actions(&mut app).is_empty(),
                "{what}: the awaiting-raise latch refuses the second click"
            );

            // The server's own answer, so the next trip is costed against the state that follows.
            let _ = app.apply_hud_events(&trip.answers);
            for _ in 0..2 {
                app.frame();
            }
            assert!(
                !app.hud().panels.attributes.awaiting_raise,
                "{what}: the answer cleared the latch"
            );
            // Leave nothing selected for the next trip.
            press_row(&mut app, h);
            app.ui_mut()
                .expect("the UI shell is up")
                .ui
                .requests
                .clear();
        }
    }

    for op in ATTRIBUTE_OPCODES {
        assert!(
            checked.iter().any(|((o, _), n)| *o == op && *n > 0),
            "{op:#06X} was never exercised; census {checked:#X?}"
        );
    }
    // The denominator. Every one of these went in as a `UiRequest` and came out of
    // `interaction::use_time` as a datagram, so a loop that silently stopped running cannot read
    // as a pass. long-solo-play carries two `0x0045` and three `0x0044`,
    // short-play-with-training two `0x0045` and one `0x0044`, and post-relog-attribute-training,
    // a bulk training session, 114 `0x0045` and 60 `0x0044`: 182 in all.
    assert_eq!(
        routed, 182,
        "the corpus's 182 attribute round trips are this test's oracle; census {checked:#X?}"
    );
    assert_eq!(
        checked.values().sum::<u32>(),
        routed,
        "every routed trip is counted in the census {checked:#X?}"
    );
    // All four combinations (row type x button) have a recorded oracle; the vital raise-10s
    // come from post-relog-attribute-training.
    for combo in [
        (0x0045, false),
        (0x0045, true),
        (0x0044, false),
        (0x0044, true),
    ] {
        assert!(
            checked.contains_key(&combo),
            "the (opcode, raise10) combination {combo:#X?} is gone from the corpus; census \
             {checked:#X?}"
        );
    }
    eprintln!(
        "{routed} of 182 corpus attribute round trips driven from the page through \
         Interaction::run_ui_requests and byte-matched, by (opcode, raise10): {checked:#X?}"
    );
}

// =============================================================================================
// 4. Link 5 and 6 — the answer, and the screen.
// =============================================================================================

/// **A vital raise's answer lands, clears the latch and moves the number.**
///
/// `Train_TrainAttribute2nd (0x0044)` is answered with `Qualities_PrivateUpdateAttribute2nd`
/// (`0x02E7`), the whole `SecondaryAttribute` record, and that opcode shares `PropertySequenceGate` tag 9
/// with `Qualities_PrivateUpdateAttribute2ndLevel` (`0x02E9`), the regeneration tick. A predicate
/// keyed on the tag alone would refuse both, so the awaiting-raise latch would never clear and the
/// raise button would stay in `0x0D` for the rest of the session; `answers_a_raise_update`
/// separates them by the value variant. This test presses the button to reach it.
///
/// Every number is the capture's: the cost the retail client put on the wire, the server's own
/// `0x02E7` body and its own `0x02CF` `AvailableExperience`.
///
/// **Falsified by** replacing the `Hud::apply_quality_update` decision with
/// `answers_a_raise(u.key)`.
///
/// The server does not send the changed record on its own. Every raise's answer carries a
/// `Qualities_PrivateUpdateInt64` on `AvailableExperience` beside it, `answers_a_raise` accepts
/// `Int64/2`, and a test that fed the batch therefore cleared the latch under either predicate.
/// Step 3 below splits the batch and delivers the `0x02E7` **alone**, which is the message the
/// vital latch actually depends on; the mutation fails against that.
#[test]
fn a_captured_vital_raise_answer_lands_and_clears_the_attribute_latch() {
    use dereth_client_model::{StatKey, StatType, StatValue};

    let xp = xp_table();
    let mut proved = 0u32;
    // Driven trips by levels bought (1 or 10), for the report line.
    let mut arms: BTreeMap<u32, u32> = BTreeMap::new();
    for session in sessions_with_attribute_traffic() {
        let (prelude, trips) = round_trips(&session);
        // With the capture's own link, so the click below is observed at the wire.
        let mut app = app_with_link(&prelude, &session);
        for trip in &trips {
            let (id, cost) = train_body(&trip.request);
            if trip.request.opcode != 0x0044 {
                // Keep the state moving, but only vitals are under test here.
                let _ = app.apply_hud_events(&trip.answers);
                app.frame();
                continue;
            }
            let what = format!("{session}[{}] vital {id} for {cost}", trip.index);

            // The capture must actually contain the record-shaped answer.
            let (opcode, answer) = answer_on(&trip.answers, StatType::Attribute2nd, id)
                .unwrap_or_else(|| panic!("{what}: the capture's own answer"));
            assert_eq!(
                opcode,
                dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND,
                "{what}: a vital raise is answered with the whole record, not a level tick"
            );
            let StatValue::Attribute2nd(after) = answer.value else {
                panic!("{what}: not a SecondaryAttribute")
            };
            let (_, xp_answer) = answer_on(&trip.answers, StatType::Int64, 2)
                .unwrap_or_else(|| panic!("{what}: the capture's own 0x02CF"));
            let StatValue::Int64(available_after) = xp_answer.value else {
                panic!("{what}: not Int64")
            };

            let q0 = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .clone();
            let before = q0
                .attribute_2nd(id)
                .unwrap_or_else(|| panic!("{what}: no vital"));
            let Some(StatValue::Int64(available_before)) = q0.get(StatKey::new(StatType::Int64, 2))
            else {
                panic!("{what}: the capture has an AvailableExperience")
            };

            // ---- 1. select and read the footer's own cost --------------------------------
            //
            // The footer's line one is always the **one-level** cost; the raise-10 button
            // charges ten levels and its cost is never drawn. So the wire amount equals the
            // footer only on the raise-1 arm. Both costs are derived here straight from the
            // vital column of the XP table (cumulative experience per level), independently of
            // the panel's cost helpers: one level is the next level's total less what is already
            // spent, the raise-10 span is that plus the per-level steps after it. The span is
            // ten levels, or the levels left below the cap when fewer remain (the table is
            // `max + 1` entries long).
            let level = before.attribute.level_from_cp as usize;
            let spent = u64::from(before.attribute.cp_spent);
            let vital_xp = &xp.vital_xp;
            let cap = vital_xp.len() - 1;
            assert!(
                level < cap,
                "{what}: a vital at the cap has no raise to buy"
            );
            let span = (cap - level).min(10);
            let one = u64::from(vital_xp[level + 1]) - spent;
            let ten = one
                + (level + 1..level + span)
                    .map(|k| u64::from(vital_xp[k + 1] - vital_xp[k]))
                    .sum::<u64>();
            let h = row_of(&app, id + 1, true);
            press_row(&mut app, h);
            assert_eq!(
                footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
                statmgmt::num(one),
                "{what}: the footer shows the one-level cost"
            );
            let (button, levels) = if u64::from(cost) == one {
                (statmgmt::child::BUTTON, 1)
            } else if u64::from(cost) == ten {
                (
                    statmgmt::child::BUTTON_10,
                    u32::try_from(span).expect("at most ten"),
                )
            } else {
                panic!(
                    "{what}: the retail client asked for {cost}; one level costs {one} and the \
                     {span}-level span costs {ten} (level_from_cp={level} cp_spent={spent})"
                )
            };
            *arms.entry(levels).or_default() += 1;
            let value_before = app
                .hud()
                .panels
                .attributes
                .rows
                .iter()
                .find(|r| r.element == h)
                .expect("the row")
                .value
                .clone();

            // ---- 2. the click ------------------------------------------------------------
            //
            // At the wire rather than the outbox — see [`wire_actions`] for why the outbox
            // reading was our own artefact. Still exactly one message; the vital id and the
            // footer's cost are now checked as the bytes that actually left.
            let _ = wire_actions(&mut app);
            click_footer_button(&mut app, button);
            let sent = wire_actions(&mut app);
            assert_eq!(sent.len(), 1, "{what}: one click, one message: {sent:?}");
            assert_eq!(sent[0].opcode, 0x0044, "{what}: Train_TrainAttribute2nd");
            let mut want_body = Vec::new();
            want_body.extend_from_slice(&id.to_le_bytes());
            want_body.extend_from_slice(&cost.to_le_bytes());
            assert_eq!(
                sent[0].body, want_body,
                "{what}: the message carries the {levels}-level cost the button charges"
            );
            assert!(
                app.hud().panels.attributes.awaiting_raise,
                "{what}: the latch is set"
            );
            assert_eq!(
                footer_state(&mut app, button),
                statmgmt::button_state::DISABLED
            );
            let answered_before = app.hud().stats.raises_answered;

            // ---- 3. the server's own answer, out of the capture --------------------------
            //
            // **In two halves, and the split is the point.** The server answers a raise with the
            // changed record *and* a `Qualities_PrivateUpdateInt64` on `AvailableExperience`, and
            // `answers_a_raise` accepts `Int64/2` on its own -- so a batch containing both clears
            // the latch even under a key-only predicate, and a test that fed the batch could
            // not tell the two predicates apart. The `0x02E7` is therefore delivered **alone**
            // first, which is the message the vital raise's latch actually depends on, and the
            // rest of the batch follows.
            let (record_only, rest): (Vec<SessionEvent>, Vec<SessionEvent>) =
                trip.answers.iter().cloned().partition(|e| match e {
                    SessionEvent::UiEvent { opcode, .. } => {
                        *opcode == dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_ATTRIBUTE2ND
                    }
                    _ => false,
                });
            assert_eq!(
                record_only.len(),
                1,
                "{what}: one 0x02E7 answers one 0x0044"
            );
            let _ = app.apply_hud_events(&record_only);
            for _ in 0..2 {
                app.frame();
            }
            assert!(
                !app.hud().panels.attributes.awaiting_raise,
                "{what}: the record ON ITS OWN clears the latch -- the value-aware predicate, \
                 exercised from the button"
            );
            assert_eq!(
                app.hud().stats.raises_answered,
                answered_before + 1,
                "{what}: and the 0x10000004 arm ran on the record, not on the experience update"
            );
            let _ = app.apply_hud_events(&rest);
            for _ in 0..2 {
                app.frame();
            }

            // ---- 4. the numbers ----------------------------------------------------------
            let q1 = app
                .objects()
                .world
                .player_qualities()
                .expect("the player description")
                .clone();
            // The maximum id and the current id (here `id` and `id + 1`) name one record, so what
            // the window leaves behind is the last writer on either id: a regeneration tick
            // (`0x02E9`, current level only) on `id + 1` can land after the `0x02E7` inside the
            // same answer window. The server's record decides everything but the current level.
            let mut want = before;
            for (_, w) in decoded_answers(&trip.answers) {
                if w.key != StatKey::new(StatType::Attribute2nd, id)
                    && w.key != StatKey::new(StatType::Attribute2nd, id + 1)
                {
                    continue;
                }
                match w.value {
                    StatValue::Attribute2nd(r) => want = r,
                    StatValue::Attribute2ndLevel(n) => want.current_level = n,
                    ref other => panic!("{what}: {other:?} on a vital key"),
                }
            }
            let stored = q1.attribute_2nd(id).expect("the vital");
            assert_eq!(
                stored, want,
                "{what}: the stored record is the last writer's on either id"
            );
            assert_eq!(
                stored.attribute, after.attribute,
                "{what}: and all but the current level is the server's record, exactly"
            );
            assert_eq!(
                after.attribute.cp_spent - before.attribute.cp_spent,
                cost,
                "{what}: the server added exactly the experience asked for to _cp_spent"
            );
            assert_eq!(
                after.attribute.level_from_cp,
                before.attribute.level_from_cp + levels,
                "{what}: each bought rank is one level"
            );
            assert_eq!(
                available_before - available_after,
                i64::from(cost),
                "{what}: AvailableExperience fell by exactly the cost the footer showed"
            );

            assert!(
                !app.hud().panels.attributes.awaiting_raise,
                "{what}: the latch is still clear after the rest of the batch"
            );
            assert!(
                app.hud().stats.raises_answered > answered_before,
                "{what}: the stat-management panel's 0x10000004 arm ran"
            );
            // The latch is gone, so the raise-1 button's state is the footer's own decision
            // again: enabled while the next level has a price and the experience covers it.
            // Derived from the table as above. Only the raise-1 button is asserted: whether the
            // vital footer's raise-10 button is decided on its own ten-level price or on the
            // one-level price is an open question about retail, and
            // a raise-10 near the cap is exactly where the two readings differ.
            let after_level = after.attribute.level_from_cp as usize;
            let one_after = if after_level >= cap {
                0
            } else {
                u64::from(vital_xp[after_level + 1]) - u64::from(after.attribute.cp_spent)
            };
            let affordable =
                one_after > 0 && u64::try_from(available_after).is_ok_and(|a| one_after <= a);
            assert_eq!(
                footer_state(&mut app, statmgmt::child::BUTTON),
                if affordable {
                    statmgmt::button_state::ENABLED
                } else {
                    statmgmt::button_state::DISABLED
                },
                "{what}: the raise button's state is the next level's price against the \
                 experience left ({one_after} vs {available_after})"
            );

            // The row and the footer were both re-run against the new record.
            let value_after = app
                .hud()
                .panels
                .attributes
                .rows
                .iter()
                .find(|r| r.element == h)
                .expect("the row survived")
                .value
                .clone();
            assert_ne!(
                value_after, value_before,
                "{what}: the row's own current/maximum pair moved: {value_before} -> {value_after}"
            );
            let cost_after = dereth_client_model::advancement::attribute_cost_to_raise(
                &xp,
                after.attribute.level_from_cp,
                after.attribute.cp_spent,
                true,
            );
            assert_eq!(
                u64::from(cost_after),
                one_after,
                "{what}: the next level's price, from the panel's helper and from the table"
            );
            // At the cap there is no next level, and the footer draws the infinity string in
            // place of a number (a raise-10 can end exactly there).
            assert_eq!(
                footer_text(&mut app, statmgmt::child::LINE_ONE_VALUE),
                if cost_after == 0 {
                    string_row(statmgmt::string::INFINITY)
                } else {
                    statmgmt::num(cost_after)
                },
                "{what}: the next raise's cost is recomputed from the new _cp_spent"
            );
            assert_ne!(
                cost_after, cost,
                "{what}: the next rank costs a different amount"
            );
            assert_eq!(
                footer_text(&mut app, statmgmt::child::LINE_TWO_VALUE),
                statmgmt::num(available_after),
                "{what}: and line two is the server's own new AvailableExperience"
            );

            press_row(&mut app, h);
            proved += 1;
        }
    }
    assert!(
        arms.get(&1).copied().unwrap_or(0) >= 4,
        "the corpus carries at least four one-level vital raises; census {arms:?}"
    );
    assert!(
        arms.keys().any(|&n| n > 1),
        "post-relog-attribute-training's vital raise-10s were not driven; census {arms:?}"
    );
    eprintln!("{proved} captured vital raises driven through the page, by levels: {arms:?}");
}

// =============================================================================================
// 5. The header's three remaining fields.
// =============================================================================================

/// Behaviour: attributes.header.shows-the-heritage-line-and-pk-status
/// **The heritage line and the PK status come up, and they are the capture's own.**
///
/// The oracle is read independently here: the gender and heritage-group `EnumMapper`s and the
/// character-title `EnumMapper` + `StringTable` are decoded straight off the retail dats in this
/// test, and the ids come out of the capture's `PropertyInt 0x71`/`0xBC` and its own
/// `0x0029 Social_CharacterTitleTable`. Nothing here writes a heritage name down -- except the
/// **three literals the original client hard-codes**, which do not come from the mapper at all.
/// Its heritage-name lookup answers `2` with `"Gharu'ndim"` while the mapper's row 2 is
/// `"Gharundim"`, and `short-play-with-training`'s character happens to be one of those hard-coded cases.
///
/// **Falsified by** dropping the `2 => "Gharu'ndim"` arm from
/// `HudView::gender_heritage_display`
/// (short-play-with-training's line becomes "Female Gharundim"), and by swapping `GENDER` and `HERITAGE_GROUP`
/// (both sessions' lines invert).
#[test]
fn the_header_shows_the_captures_own_heritage_line_and_pk_status() {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource as _;

    let store = dat_store();
    let mapper = |id: u32| -> dereth_assets::tables::EnumMapper {
        let id = dereth_primitives::DataId(id);
        let b = store
            .read(id)
            .unwrap_or_else(|e| panic!("{id:?} is this test's oracle: {e}"));
        dereth_assets::tables::EnumMapper::decode_payload(id, &b).expect("EnumMapper")
    };
    let genders = mapper(0x2200_000A);
    let heritages = mapper(0x2200_000B);
    let title_tokens = mapper(0x2200_0041);
    let title_strings = {
        let id = dereth_primitives::DataId(0x2300_000E);
        let b = store.read(id).expect("the title StringTable");
        dereth_assets::ui::StringTable::decode_payload(id, &b).expect("StringTable")
    };
    let name = |m: &dereth_assets::tables::EnumMapper, k: u32| -> String {
        m.id_to_string
            .iter()
            .find(|(id, _)| *id == k)
            .map(|(_, s)| s.clone())
            .unwrap_or_else(|| panic!("no row {k} in the mapper"))
    };

    // The three literals the original heritage-name lookup answers before consulting the mapper.
    let heritage_name = |k: u32| -> String {
        match k {
            2 => "Gharu'ndim".to_owned(),
            5 => "Umbraen".to_owned(),
            0xD => "Olthoi".to_owned(),
            other => name(&heritages, other),
        }
    };

    let mut seen = 0u32;
    for session in ["long-solo-play", "short-play-with-training"] {
        let (prelude, _) = round_trips(session);
        let mut app = app_in_gameplay(4);
        // The object half. Without it there is no player object and the original PK-header update
        // takes its missing-player arm, so the PK assertion below would pass for the wrong reason.
        *app.objects_mut() = replay_objects(session);
        let _ = app.apply_hud_events(&prelude);
        for _ in 0..4 {
            app.frame();
        }
        app.ui_mut()
            .expect("the UI shell is up")
            .ui
            .requests
            .clear();
        let q = app
            .objects()
            .world
            .player_qualities()
            .expect("the capture's 0x0013")
            .clone();
        let gender = u32::try_from(q.inq_int(dereth_client::hud::GENDER)).expect("a gender");
        let heritage =
            u32::try_from(q.inq_int(dereth_client::hud::HERITAGE_GROUP)).expect("a heritage");
        assert!(
            gender != 0,
            "{session}: the capture carries PropertyInt 0x71"
        );
        assert!(
            heritage != 0,
            "{session}: the capture carries PropertyInt 0xBC"
        );

        let mut want = format!("{} {}", name(&genders, gender), heritage_name(heritage));
        // The appended display title, from the capture's own `0x0029`.
        let title_id = app.hud().display_title;
        assert!(
            title_id != 0,
            "{session}: the capture carries a Social_CharacterTitleTable"
        );
        let token = name(&title_tokens, title_id);
        let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
        let title = title_strings
            .strings
            .iter()
            .find(|(k, _)| *k == hash)
            .and_then(|(_, s)| s.strings.first().cloned())
            .unwrap_or_else(|| panic!("{session}: {token} is not in StringTable 0x2300000E"));
        want.push(' ');
        want.push_str(&title);

        assert_eq!(
            header_text(&mut app, statmgmt::header::HERITAGE),
            want,
            "{session}: the heritage header is `<Gender> <Heritage> <Title>`"
        );
        assert_eq!(
            app.hud().panels.attributes.header_content.heritage,
            want,
            "{session}: and the panel recorded what it wrote"
        );

        // The PK status comes from the player object's own public-description flags.
        let player = app.objects().world.player.expect("0xF746 named a player");
        let w = app
            .objects()
            .world
            .weenie(player)
            .expect("the player's own weenie");
        let want_pk = dereth_ui_screens::view::PkStatus::of(w.is_pk(), w.is_pk_lite());
        assert_eq!(
            header_text(&mut app, statmgmt::header::PK_STATUS),
            string_row(want_pk.token()),
            "{session}: the PK header chooses among three states from the PK and PK-lite flags"
        );
        // The three-way choice itself, which the live sessions here cannot exercise: long-solo-play
        // and short-play-with-training carry neither PK bit, so this live half can only reach the
        // NPK arm. Stated rather than claimed.
        use dereth_ui_screens::view::PkStatus;
        assert_eq!(PkStatus::of(false, false), PkStatus::Npk);
        assert_eq!(PkStatus::of(true, false), PkStatus::Pk);
        assert_eq!(PkStatus::of(false, true), PkStatus::PkLite);
        assert_eq!(
            PkStatus::of(true, true),
            PkStatus::Pk,
            "slot 8 is tested first and wins"
        );
        assert_eq!(
            want_pk,
            PkStatus::Npk,
            "{session}'s character is not flagged for PK"
        );
        // And the fourth field, the name, is still there, so the header is not being
        // reported complete while something else went blank.
        assert!(
            !header_text(&mut app, statmgmt::header::NAME).is_empty(),
            "{session}: the name"
        );
        eprintln!(
            "{session} header -> heritage {want:?}, pk {:?}",
            header_text(&mut app, statmgmt::header::PK_STATUS)
        );
        seen += 1;
    }
    assert_eq!(seen, 2);
}

/// **The luminance pair is cleared below level 200, and formatted with `xp_to_string` above it.**
///
/// The original gate performs these two tests:
///
/// ```text
/// compare the level slot with 0xC8; if below, clear
/// load the maximum-luminance value; if zero, clear
/// ```
///
/// **Both captures are level 1 and carry neither `PropertyInt64` 6 nor 7**, so the live half of
/// this test can only assert the *cleared* branch -- which it does, off the real page. The
/// formatted branch has no capture and is asserted against the original formatter's
/// `NUMBERFMTA` contract instead: `Grouping 3`, `lpThousandSep ","`, `NumDigits 0`.
/// That is stated rather than claimed to be captured.
///
/// **Falsified by** removing the `maximum == 0` half of the gate in
/// `HeaderInputs::luminance_line`: the synthetic level-200-with-no-luminance case draws `"0 / 0"`.
#[test]
fn the_luminance_pair_is_cleared_below_level_200_and_grouped_above_it() {
    use dereth_ui_screens::panels::statmgmt::{xp_to_string, HeaderInputs, XpHeader};

    // The current `xp_to_string` implementation of that format.
    assert_eq!(xp_to_string(0), "0");
    assert_eq!(xp_to_string(999), "999");
    assert_eq!(xp_to_string(1_000), "1,000");
    assert_eq!(xp_to_string(1_234_567), "1,234,567");
    assert_eq!(
        xp_to_string(-1_234),
        "-1,234",
        "NegativeOrder 1 is a leading minus"
    );

    let with = |level: i32, lum: (i64, i64)| {
        HeaderInputs {
            xp: Some(XpHeader {
                level,
                ..XpHeader::default()
            }),
            luminance: lum,
            ..HeaderInputs::default()
        }
        .luminance_line()
    };
    assert_eq!(
        with(199, (5_000, 1_500_000)),
        (String::new(), String::new()),
        "level < 200"
    );
    assert_eq!(
        with(200, (5_000, 1_500_000)),
        ("Luminance:".to_owned(), "5,000 / 1,500,000".to_owned()),
        "at exactly 200 the arm draws, available first"
    );
    assert_eq!(
        with(275, (0, 0)),
        (String::new(), String::new()),
        "a zero luminance maximum clears the pair"
    );
    assert_eq!(
        with(275, (0, 1_500_000)).1,
        "0 / 1,500,000",
        "a spent-out character still shows the pair"
    );

    // And the live page, on a capture that is below the gate.
    let (prelude, _) = round_trips("short-play-with-training");
    let mut app = app_with(&prelude);
    let level = app
        .objects()
        .world
        .player_qualities()
        .expect("0x0013")
        .inq_int(dereth_client::hud::LEVEL);
    assert!(
        level < 200,
        "short-play-with-training's character is level {level}, below the luminance gate"
    );
    assert_eq!(header_text(&mut app, statmgmt::header::LUMINANCE_LABEL), "");
    assert_eq!(header_text(&mut app, statmgmt::header::LUMINANCE), "");
    assert_eq!(
        app.hud().panels.attributes.header_content.luminance_label,
        ""
    );
}

// =============================================================================================
// 6. The two sub-panels are not each other.
// =============================================================================================

/// **The skills and attributes panels do not answer for each other's footer buttons.**
///
/// Every id in `panels::statmgmt` appears **twice** in the live tree, once under each
/// stat-management panel -- the list box `0x1000023D`, the three containers and both buttons. In
/// the original client each panel is itself the UI element that receives its own subtree's
/// messages, so a `BUTTON_CLICKED` on `0x10000246` reaches exactly one of them; this crate's
/// panels are structs and the screen fans one message out to every panel in turn.
///
/// With two listeners, the guard (`Footer::owns`) is what keeps each click with its own panel.
///
/// **Falsified by** deleting the `owns` test from
/// `AttributesPanel::on_element_message` made the attributes panel consume the skills page's raise
/// click and emit a `TrainAttribute` for
/// whatever attribute row happens to be selected.
#[test]
fn the_two_stat_panels_do_not_answer_for_each_others_footer_buttons() {
    const SESSION: &str = "short-play-with-training";
    let (prelude, _) = round_trips(SESSION);
    // With the capture's own link: this test's claim is now counted in messages, not in queued
    // requests. See the note at the clicks below.
    let mut app = app_with_link(&prelude, SESSION);

    // Select a row on **each** page, so both panels have something they could raise.
    let attr_row = row_of(&app, 1, false);
    press_row(&mut app, attr_row);
    let skill = app
        .hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.group == dereth_ui_screens::panels::skills::SkillGroup::Trained)
        .expect("the capture's character has a trained skill")
        .clone();
    press_row(&mut app, skill.element);
    assert_eq!(app.hud().panels.skills.selected_skill, skill.skill);
    assert_eq!(
        app.hud().panels.attributes.selected().map(|r| r.stat),
        Some(1)
    );

    // Click the **skills** page's raise button. Only the skills panel may answer.
    let skills_button = {
        let (ui, screen) = gameplay_screen(&mut app);
        let root = screen.root().expect("root");
        let page = ui
            .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
            .expect("the character page");
        let panel = ui
            .get_child_recursive(page, dereth_ui_screens::panels::skills::PANEL)
            .expect("the skills panel");
        let state = ui.node(panel).map_or(0, |n| n.state.0);
        let c = ui
            .get_child_recursive(panel, ElementId(statmgmt::Footer::container_for(state)))
            .expect("the skills footer container");
        ui.get_child_recursive(c, ElementId(statmgmt::child::BUTTON))
            .expect("the button")
    };
    // **At the wire.** "Exactly one panel answered" is the whole claim of this test: one
    // *message*; see [`wire_actions`]. A skill answer and an attribute answer are different
    // opcodes, which makes the "which panel" half sharp.
    let _ = wire_actions(&mut app);
    {
        let (ui, _) = gameplay_screen(&mut app);
        ui.broadcast_element_message(
            skills_button,
            dereth_ui::msg::element::id::BUTTON_CLICKED,
            7,
            0,
        );
    }
    // Two frames: the controller pass that turns the queued action into a datagram runs near the
    // top of the *next* frame. Same reason as `click_footer_button`.
    app.frame();
    app.frame();
    let got = wire_actions(&mut app);
    assert_eq!(got.len(), 1, "exactly one panel answered: {got:?}");
    assert!(
        got[0].opcode == 0x0046 || got[0].opcode == 0x0047,
        "the skills page's own button sent a skill message (0x0046/0x0047), not {:#06X}",
        got[0].opcode
    );
    assert!(
        !app.hud().panels.attributes.awaiting_raise,
        "and the attributes panel's latch was not touched"
    );

    // The mirror: the attributes page's own button, with the skills page still selected.
    let _ = wire_actions(&mut app);
    click_footer_button(&mut app, statmgmt::child::BUTTON);
    let got = wire_actions(&mut app);
    assert_eq!(got.len(), 1, "exactly one panel answered: {got:?}");
    assert_eq!(
        got[0].opcode, 0x0045,
        "the attributes page's own button sent Train_TrainAttribute, not {:#06X}",
        got[0].opcode
    );
    assert_eq!(
        u32::from_le_bytes(got[0].body[0..4].try_into().expect("4")),
        1,
        "and it is Strength's raise: {:?}",
        got[0].body
    );
}

/// A compile-time reminder that this file's `ObjectId` import is the one the harness uses.
#[allow(dead_code)]
fn _unused(_: ObjectId) {}
