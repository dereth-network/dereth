//! The attribute `+10` footer button (`0x100005EB`), driven by a real pointer press on the live
//! tree: with enough unassigned experience it is enabled (state 1, attribute `0x0D` false), a
//! press lands on the copy under the footer container the sub-panel's state selects, and it
//! sends one `0x0045` raise with the ten-level cost; states 1 and `0x0D` draw different pixels;
//! and a runtime `AvailableExperience` update re-runs the footer and relights the button. The
//! footer enables with `if (cost == 0 || available < cost) state 0x0D else state 1`, where the
//! cost is the table entry ten levels up (or to the cap) minus the points already spent.
//! Fixture: the retail dats and `short-play-with-training` replayed to its `0x0013` on a
//! socket-free link, with Strength, level and experience overwritten to one retail screenshot's
//! figures; every action is read back from the link's outgoing queue.

#![cfg(gpu)]

use crate::common::client_dir;

use std::net::SocketAddr;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::shared_session;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_primitives::LocalTime;
use dereth_transport::wire::ParsedPacket;
use dereth_ui::{ElemHandle, ElementId, UiSystem};
use dereth_ui_screens::panels::{attributes, skills, statmgmt};
use dereth_ui_screens::screens::gameplay::GamePlayScreen;

// ---------------------------------------------------------------------------------------------
// Harness — a recorded session replayed to its player description, on a socket-free link.
// ---------------------------------------------------------------------------------------------

const CAPTURE_PLAYER: dereth_primitives::ObjectId = dereth_primitives::ObjectId(0x5000_0001);
const SESSION: &str = "short-play-with-training";

fn dat_store() -> dereth_dat::RetailDatStore {
    crate::common::dat_store()
}

fn app_in_gameplay(frames: u32) -> App {
    let cfg = Config {
        ui: true,
        headless: true,
        sound: false,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the headless app starts");
    app.start_shell().expect("the shell comes up");
    {
        let w = &mut app.probe_mut().objects_mut().world;
        w.tables.weenies.insert(
            CAPTURE_PLAYER,
            dereth_client_model::weenie::Weenie::new(CAPTURE_PLAYER),
        );
        assert!(w.set_player(CAPTURE_PLAYER), "the identity is adopted once");
    }
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the shell exists");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("the gameplay screen is current");
    (ui, screen)
}

/// One `[F7B1][stamp][opcode][body]`.
#[derive(Debug, Clone, PartialEq, Eq)]
struct SentAction {
    queue: u16,
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
                opcode: u32::from_le_bytes(f.payload[8..12].try_into().expect("4")),
                body: f.payload[12..].to_vec(),
            });
        }
    }
    out
}

/// Replay the capture's server half through a socket-free link until `0x0013` lands.
fn replay(session: &str) -> (ClientNetwork, Vec<SessionEvent>) {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("the loopback host binds");
    let mut objects = ObjectStream::new();
    let mut events = Vec::new();
    let mut entered = false;
    for r in records {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let mut seen_desc = false;
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
            seen_desc |= matches!(e, SessionEvent::PlayerDescription(_));
            events.push(e);
        }
        if seen_desc {
            break;
        }
    }
    assert!(
        events
            .iter()
            .any(|e| matches!(e, SessionEvent::PlayerDescription(_))),
        "{session} never reached 0x0013, so it cannot be this test's oracle"
    );
    let _ = net.take_outgoing();
    (net, events)
}

fn player_description(events: &[SessionEvent]) -> &dereth_protocol::login::LoginPlayerDescription {
    events
        .iter()
        .find_map(|e| match e {
            SessionEvent::PlayerDescription(d) => Some(&**d),
            _ => None,
        })
        .expect("the capture's 0x0013")
}

/// Re-point the world at the identity the capture carries.
fn embody(app: &mut App) {
    let player = app.hud().player.unwrap_or(CAPTURE_PLAYER);
    let w = &mut app.probe_mut().objects_mut().world;
    if w.player == Some(player) {
        return;
    }
    w.player = None;
    w.tables
        .weenies
        .insert(player, dereth_client_model::weenie::Weenie::new(player));
    assert!(w.set_player(player), "the identity is adopted once");
}

fn app_with_capture(session: &str) -> (App, Vec<SessionEvent>) {
    let (net, events) = replay(session);
    let mut app = app_in_gameplay(4);
    app.attach_replay_network(net)
        .expect("a headless app with no link takes the replay endpoint");
    let _ = app.apply_hud_events(&events);
    embody(&mut app);
    for _ in 0..4 {
        app.frame();
    }
    app.ui_mut()
        .expect("the UI shell is up")
        .ui
        .requests
        .clear();
    let _ = wire_actions(&mut app);
    (app, events)
}

/// Every game action `app` has put on its link since this was last called, in send order.
fn wire_actions(app: &mut App) -> Vec<SentAction> {
    let out = app
        .replay_network_mut()
        .expect("app_with_capture attaches the capture's own replay endpoint")
        .take_outgoing();
    actions_in(&out)
}

fn xp_table() -> dereth_assets::tables::XpTable {
    use dereth_assets::Decode;
    use dereth_primitives::AssetSource as _;
    let id = dereth_primitives::DataId(0x0E00_0018);
    let b = dat_store()
        .read(id)
        .expect("the experience progression table is in the dats");
    dereth_assets::tables::XpTable::decode_payload(id, &b).expect("experience progression table")
}

/// The signed 64-bit quality at key 2 as the panels read it, and the synthetic write behind it.
fn set_available_xp(app: &mut App, n: i64) {
    let q = app
        .probe_mut()
        .objects_mut()
        .world
        .player_qualities_mut()
        .expect("the capture's 0x0013 was installed on the player's own weenie");
    assert!(
        q.set(
            dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                dereth_client_runtime::hud::AVAILABLE_EXPERIENCE
            ),
            dereth_client_model::StatValue::Int64(n),
        ),
        "an Int64 is always storable"
    );
}

fn available_xp(app: &App) -> i64 {
    let q = app
        .objects()
        .world
        .player_qualities()
        .expect("the player descriptor");
    match q.get(dereth_client_model::StatKey::new(
        dereth_client_model::StatType::Int64,
        dereth_client_runtime::hud::AVAILABLE_EXPERIENCE,
    )) {
        Some(dereth_client_model::StatValue::Int64(n)) => n,
        other => panic!("AvailableExperience is an Int64, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------------------------
// Reading the live page back.
// ---------------------------------------------------------------------------------------------

fn sub_panel(app: &mut App, panel: ElementId) -> ElemHandle {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let page = ui
        .get_child_recursive(root, dereth_ui_screens::panels::remaining::CHARACTER_PAGE)
        .expect("the character page");
    ui.get_child_recursive(page, panel)
        .expect("the sub-panel is in the shipped tree")
}

/// Return `0x100005EB` under the footer container selected by the sub-panel's own state, rather
/// than the first `0x100005EB` in the tree.
fn footer_child(app: &mut App, panel: ElementId, child: u32) -> ElemHandle {
    let p = sub_panel(app, panel);
    let (ui, _) = gameplay_screen(app);
    let state = ui.node(p).map_or(0, |n| n.state.0);
    let c = ui
        .get_child_recursive(p, ElementId(statmgmt::Footer::container_for(state)))
        .expect("the footer container the panel's own state names");
    ui.get_child_recursive(c, ElementId(child))
        .expect("the footer child")
}

/// Attribute `0x0D` is the disabled flag written by button-state changes and checked before a
/// mouse-up action. `None` means the layout never declared it, which the two footer buttons do
/// (they ship `0x0D = true`).
fn disabled_attr(app: &mut App, h: ElemHandle) -> Option<bool> {
    let (ui, _) = gameplay_screen(app);
    dereth_ui_screens::bind::attr_bool(ui, h, statmgmt::ATTR_DISABLED)
}

fn node_state(app: &mut App, h: ElemHandle) -> u32 {
    let (ui, _) = gameplay_screen(app);
    ui.node(h).map(|n| n.state.0).expect("the element is alive")
}

fn centre(app: &mut App, h: ElemHandle) -> (i32, i32) {
    let (ui, _) = gameplay_screen(app);
    let b = ui.screen_box(h);
    ((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
}

/// The element a real press at `(x, y)` would land on — the mouse-over hit test.
fn element_at(app: &mut App, x: i32, y: i32) -> Option<(ElemHandle, u32)> {
    let (ui, _) = gameplay_screen(app);
    let h = ui.hit_test_screen(x, y)?;
    Some((h, ui.node(h).map(|n| n.element_id().0).unwrap_or(0)))
}

/// Make the character page visible and select the sub-panel `h` belongs to by clicking its tab.
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
    let sub = {
        let (ui, _) = gameplay_screen(app);
        let mut cur = Some(h);
        let mut found = None;
        while let Some(c) = cur {
            let id = ui.node(c).map(dereth_ui::ElementNode::element_id);
            if id == Some(skills::PANEL) || id == Some(attributes::PANEL) {
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

/// Press a list row through the real pointer-down producer.
fn press_row(app: &mut App, h: ElemHandle) {
    show_page_for(app, h);
    {
        let mut panels = std::mem::take(&mut app.probe_mut().hud_mut().panels);
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
        app.probe_mut().hud_mut().panels = panels;
    }
    let (x, y) = centre(app, h);
    // A real click is always preceded by a `WM_MOUSEMOVE` onto the row: without it the element
    // the pointer last rested on keeps the mouse-over state, which is a test artefact and not a
    // player's gesture.
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_move(LocalTime(1.0), x, y);
    }
    app.frame();
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    }
    app.frame();
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }
    app.frame();
}

/// **A real pointer press on an element**: move onto it, press, release — the three
/// UI element manager entry points a player's mouse reaches, with a frame between each so the
/// element outbox drains the way it does at runtime. Returns the element the press landed on.
fn pointer_press(app: &mut App, h: ElemHandle) -> Option<(ElemHandle, u32)> {
    let (x, y) = centre(app, h);
    let hit = element_at(app, x, y);
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_move(LocalTime(1.0), x, y);
    }
    app.frame();
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
    }
    app.frame();
    {
        let (ui, _) = gameplay_screen(app);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }
    // Two frames: the click's action becomes a datagram on the *next* frame's controller pass.
    app.frame();
    app.frame();
    hit
}

fn attribute_row(app: &App, stat: u32, secondary: bool) -> ElemHandle {
    app.hud()
        .panels
        .attributes
        .rows
        .iter()
        .find(|r| r.stat == stat && r.secondary == secondary)
        .unwrap_or_else(|| panic!("no attribute row for stat {stat} secondary={secondary}"))
        .element
}

fn skill_row(app: &App, skill: u32) -> ElemHandle {
    app.hud()
        .panels
        .skills
        .rows
        .iter()
        .find(|r| r.skill == skill)
        .unwrap_or_else(|| panic!("no skill row for skill {skill}"))
        .element
}

/// One station: the row selected, `AvailableExperience` at `xp`, and what the `+10` button
/// under the resolved footer container reads and does under a real press.
struct Station {
    disabled_attr: Option<bool>,
    state: u32,
    hit: Option<(ElemHandle, u32)>,
    sent: Vec<SentAction>,
}

/// What the live tree says about one footer button, for the transcript.
fn dump_button(app: &mut App, label: &str, h: ElemHandle) {
    let (ui, _) = gameplay_screen(app);
    let n = ui.node(h).expect("alive");
    eprintln!(
        "footer {label}: id={:#x} state={} inst0x0D={:?} merged0x0D={:?} has_state1={} has_state13={} \
         visible={} mouse_visible={} box={:?}",
        n.element_id().0,
        n.state.0,
        n.instance_properties.get_bool(statmgmt::ATTR_DISABLED),
        n.merged_properties().get_bool(statmgmt::ATTR_DISABLED),
        n.desc.access_state(dereth_ui::StateId(1)).is_some(),
        n.desc.access_state(dereth_ui::StateId(0x0D)).is_some(),
        n.region.flags.visible,
        n.is_mouse_visible,
        ui.screen_box(h),
    );
    eprintln!(
        "footer {label}: behaviour_in_slot={}",
        n.behaviour.is_some()
    );
}

fn run_station(app: &mut App, panel: ElementId, row: ElemHandle, xp: i64) -> Station {
    set_available_xp(app, xp);
    assert_eq!(
        available_xp(app),
        xp,
        "the synthetic AvailableExperience took"
    );
    press_row(app, row);
    let button = footer_child(app, panel, statmgmt::child::BUTTON_10);
    let one = footer_child(app, panel, statmgmt::child::BUTTON);
    dump_button(app, "+1 ", one);
    dump_button(app, "+10", button);
    let disabled_attr = disabled_attr(app, button);
    let state = node_state(app, button);
    let _ = wire_actions(app);
    let hit = pointer_press(app, button);
    let sent = wire_actions(app);
    Station {
        disabled_attr,
        state,
        hit,
        sent,
    }
}

fn deselect(app: &mut App, row: ElemHandle) {
    press_row(app, row);
}

// =============================================================================================
// Retail figures, verbatim from paired screenshots at the Aluvian Pathwarden Chest.
// =============================================================================================

/// Level 40, Strength 145 (+15 enchanted), Total XP 21,137,904, Unassigned 20,887,465, and the
/// footer's "Experience To Raise: 2,456" -- which the attribute table answers only for
/// `level_from_cp = 30`, `cp_spent = attr[30] = 32,676` (`attr[31] - attr[30] = 2,456`), so the
/// raw Strength is `init 100 + 30`.
const OWNER_LEVEL: i32 = 40;
const OWNER_TOTAL_XP: i64 = 21_137_904;
const OWNER_AVAILABLE_XP: i64 = 20_887_465;
const OWNER_STR_INIT: u32 = 100;
const OWNER_STR_RAISES: u32 = 30;
const OWNER_STR_CP_SPENT: u32 = 32_676;
const OWNER_STR_BUFF: f32 = 15.0;
/// `attr[40] - attr[30]`.
const OWNER_STR_COST_10: u32 = 63_878 - 32_676;

fn install_owner_figures(app: &mut App) {
    use {dereth_rules::enchant::ench_type, dereth_rules::enchant::Enchantment};
    let q = app
        .probe_mut()
        .objects_mut()
        .world
        .player_qualities_mut()
        .expect("the capture's 0x0013 was installed on the player's own weenie");
    let mut a = q.attribute(1).expect("the capture carries Strength");
    a.init_level = OWNER_STR_INIT;
    a.level_from_cp = OWNER_STR_RAISES;
    a.cp_spent = OWNER_STR_CP_SPENT;
    assert!(q.set_attribute(1, a), "Strength is storable");
    let int = |k| dereth_client_model::StatKey::new(dereth_client_model::StatType::Int, k);
    let int64 = |k| dereth_client_model::StatKey::new(dereth_client_model::StatType::Int64, k);
    assert!(q.set(
        int(dereth_client_runtime::hud::LEVEL),
        dereth_client_model::StatValue::Int(OWNER_LEVEL)
    ));
    assert!(q.set(
        int64(dereth_client_runtime::hud::TOTAL_EXPERIENCE),
        dereth_client_model::StatValue::Int64(OWNER_TOTAL_XP)
    ));
    assert!(q.set(
        int64(dereth_client_runtime::hud::AVAILABLE_EXPERIENCE),
        dereth_client_model::StatValue::Int64(OWNER_AVAILABLE_XP)
    ));
    // Strength Self VI-shaped: a single-stat additive attribute enchantment on key 1.
    assert!(
        q.enchantments.update_enchantment(Enchantment {
            id: 0x0001_0000 | 0x0000_0A48,
            spell_category: 1,
            power_level: 200,
            start_time: 0.0,
            duration: 1800.0,
            caster: dereth_primitives::ObjectId(0x5000_0001),
            degrade_modifier: 0.0,
            degrade_limit: 0.0,
            last_time_degraded: 0.0,
            smod: dereth_protocol::types::qualities::StatMod {
                kind: ench_type::ATTRIBUTE
                    | ench_type::SINGLE_STAT
                    | ench_type::ADDITIVE
                    | ench_type::BENEFICIAL,
                key: 1,
                value: OWNER_STR_BUFF,
            },
            spell_set_id: None,
        }),
        "the +15 goes into the additive list"
    );
}

/// Behaviour: advancement.raise.the-ten-point-button-lights-up-with-the-experience-and-a-press-sends-the-raise
///
/// **The screenshot state, headless: select Strength by a real press, read the `+10`, press it.**
/// The footer reproduces the screenshot's title and figures, the `+10` is enabled, and one real
/// press sends one `0x0045` with Strength and the ten-level cost.
#[test]
fn the_strength_plus_ten_lights_with_enough_experience_and_a_real_press_sends_the_ten_point_raise()
{
    let (mut app, _events) = app_with_capture(SESSION);
    install_owner_figures(&mut app);
    let row = attribute_row(&app, 1, false);
    press_row(&mut app, row);
    let fc = app.hud().panels.attributes.footer_content.clone();
    eprintln!("+10 footer: {fc:?}");
    assert_eq!(
        fc.title, "Strength: 145 (+15)",
        "the title matches the screenshot"
    );
    assert_eq!(
        fc.line_one_value, "2,456",
        "the +1 cost matches the screenshot"
    );
    assert_eq!(
        fc.line_two_value, "20,887,465",
        "the unassigned experience matches the screenshot"
    );
    let button = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON_10);
    let one = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON);
    dump_button(&mut app, "+1 ", one);
    dump_button(&mut app, "+10", button);
    assert_eq!(
        disabled_attr(&mut app, button),
        Some(false),
        "attribute 0x0D of 0x100005EB reads enabled"
    );
    assert_eq!(
        node_state(&mut app, button),
        statmgmt::button_state::ENABLED,
        "and the button is in state 1"
    );
    let _ = wire_actions(&mut app);
    let hit = pointer_press(&mut app, button);
    assert_eq!(
        hit.map(|(h, _)| h),
        Some(button),
        "the press lands on the resolved +10 footer button's element (hit {hit:?})"
    );
    let sent = wire_actions(&mut app);
    let mut want = Vec::new();
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(&OWNER_STR_COST_10.to_le_bytes());
    assert_eq!(
        sent,
        vec![SentAction {
            queue: u16::from(dereth_client_net::queues::Queue::Weenie as u8),
            opcode: 0x0045,
            body: want
        }],
        "one real press, one 0x0045 carrying Strength and the ten-point amount {OWNER_STR_COST_10}"
    );
}

/// The pixels of `b` in a BGRA frame, row-major.
fn crop(w: u32, bgra: &[u8], b: dereth_ui::Box2D) -> Vec<[u8; 4]> {
    let mut out = Vec::new();
    for y in b.y0.max(0)..b.y1 {
        for x in b.x0.max(0)..b.x1 {
            let i = ((y as u32 * w + x as u32) * 4) as usize;
            out.push([bgra[i], bgra[i + 1], bgra[i + 2], bgra[i + 3]]);
        }
    }
    out
}

/// **What the two states of the `+10` button actually draw**: state 1 and state `0x0D` differ in
/// pixels, not only in attribute `0x0D`.
#[test]
fn the_plus_ten_in_state_1_draws_differently_from_state_0xd() {
    let (mut app, _events) = app_with_capture(SESSION);
    install_owner_figures(&mut app);
    let row = attribute_row(&app, 1, false);
    press_row(&mut app, row);
    let button = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON_10);
    let one = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON);
    assert_eq!(
        node_state(&mut app, button),
        statmgmt::button_state::ENABLED
    );
    let (bx, b1x) = {
        let (ui, _) = gameplay_screen(&mut app);
        (ui.screen_box(button), ui.screen_box(one))
    };
    for _ in 0..3 {
        app.frame();
    }
    let (w, _h, a) = app
        .renderer_mut()
        .capture_bgra()
        .expect("an offscreen capture");
    // Now force the disabled state that follows a ten-level raise.
    {
        let (ui, _) = gameplay_screen(&mut app);
        statmgmt::set_button_state_at(ui, button, statmgmt::button_state::DISABLED);
    }
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        node_state(&mut app, button),
        statmgmt::button_state::DISABLED
    );
    let (w2, _h2, b) = app
        .renderer_mut()
        .capture_bgra()
        .expect("an offscreen capture");
    assert_eq!(w, w2);
    let pa = crop(w, &a, bx);
    let pb = crop(w, &b, bx);
    let changed = pa.iter().zip(&pb).filter(|(x, y)| x != y).count();
    let p1 = crop(w, &a, b1x);
    let nonblack = |p: &[[u8; 4]]| {
        p.iter()
            .filter(|c| c[0] > 16 || c[1] > 16 || c[2] > 16)
            .count()
    };
    eprintln!(
        "+10 shots: +10 box {bx:?} pixels={} changed_between_states={} nonblack_state1={} nonblack_state0xd={}; +1 box {b1x:?} nonblack={}",
        pa.len(), changed, nonblack(&pa), nonblack(&pb), nonblack(&p1)
    );
    // Print a few sample colours from the centre row of each.
    let mid = |p: &[[u8; 4]]| {
        let n = (bx.x1 - bx.x0) as usize;
        let r = (bx.y1 - bx.y0) as usize / 2;
        p[r * n..r * n + n].to_vec()
    };
    eprintln!("+10 state1  centre row: {:?}", mid(&pa));
    eprintln!("+10 state0xd centre row: {:?}", mid(&pb));
    assert!(
        changed > 0,
        "state 1 and state 0xD of the +10 button draw identical pixels"
    );
}

/// One `Qualities_PrivateUpdateInt64` as a `SessionEvent`: the runtime arrival path for
/// `AvailableExperience`, registered as a signed 64-bit player-quality update.
fn int64_answer(
    sequence: u8,
    property_id: u32,
    value: i64,
) -> dereth_client_net::client_session::SessionEvent {
    let m = dereth_protocol::qualities::QualitiesPrivateUpdateInt64(
        dereth_protocol::qualities::PrivateUpdate {
            sequence,
            property_id,
            value,
        },
    );
    let mut w = dereth_protocol::archive::Writer::new();
    w.u32(dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT64.0);
    dereth_protocol::Message::write(&m, &mut w).expect("encode");
    dereth_client_net::client_session::SessionEvent::UiEvent {
        opcode: dereth_protocol::Opcode::QUALITIES_PRIVATE_UPDATE_INT64,
        blob: w.into_inner(),
    }
}

fn install_owner_attribute_and_level(app: &mut App) {
    use {dereth_rules::enchant::ench_type, dereth_rules::enchant::Enchantment};
    let q = app
        .probe_mut()
        .objects_mut()
        .world
        .player_qualities_mut()
        .expect("the 0x0013");
    let mut a = q.attribute(1).expect("Strength");
    a.init_level = OWNER_STR_INIT;
    a.level_from_cp = OWNER_STR_RAISES;
    a.cp_spent = OWNER_STR_CP_SPENT;
    assert!(q.set_attribute(1, a));
    assert!(q.set(
        dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int,
            dereth_client_runtime::hud::LEVEL
        ),
        dereth_client_model::StatValue::Int(OWNER_LEVEL)
    ));
    assert!(q.set(
        dereth_client_model::StatKey::new(
            dereth_client_model::StatType::Int64,
            dereth_client_runtime::hud::TOTAL_EXPERIENCE
        ),
        dereth_client_model::StatValue::Int64(OWNER_TOTAL_XP)
    ));
    assert!(q.enchantments.update_enchantment(Enchantment {
        id: 0x0001_0000 | 0x0000_0A48,
        spell_category: 1,
        power_level: 200,
        start_time: 0.0,
        duration: 1800.0,
        caster: dereth_primitives::ObjectId(0x5000_0001),
        degrade_modifier: 0.0,
        degrade_limit: 0.0,
        last_time_degraded: 0.0,
        smod: dereth_protocol::types::qualities::StatMod {
            kind: ench_type::ATTRIBUTE
                | ench_type::SINGLE_STAT
                | ench_type::ADDITIVE
                | ench_type::BENEFICIAL,
            key: 1,
            value: OWNER_STR_BUFF
        },
        spell_set_id: None,
    }));
}

/// **A live arrival order: the row is selected while unassigned experience is still small (a
/// legitimately dim `+10`), then the server sends a `Qualities_PrivateUpdateInt64` raising it well
/// past the ten-point cost.** The panel listens for exactly that quality, so the `0x10000004`
/// panel arm re-runs the footer and the `+10` lights, and a real press sends the raise.
#[test]
fn a_runtime_available_experience_update_relights_the_plus_ten() {
    let (mut app, _events) = app_with_capture(SESSION);
    install_owner_attribute_and_level(&mut app);
    // Start below the ten-point cost.
    {
        let q = app
            .probe_mut()
            .objects_mut()
            .world
            .player_qualities_mut()
            .expect("the 0x0013");
        assert!(q.set(
            dereth_client_model::StatKey::new(
                dereth_client_model::StatType::Int64,
                dereth_client_runtime::hud::AVAILABLE_EXPERIENCE
            ),
            dereth_client_model::StatValue::Int64(100)
        ));
    }
    let row = attribute_row(&app, 1, false);
    press_row(&mut app, row);
    let button = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON_10);
    assert_eq!(
        disabled_attr(&mut app, button),
        Some(true),
        "before: 100 XP < {OWNER_STR_COST_10}, the +10 is dim"
    );
    assert_eq!(
        node_state(&mut app, button),
        statmgmt::button_state::DISABLED
    );

    // The server grants the experience at runtime.
    let _ = app.apply_hud_events(&[int64_answer(
        1,
        dereth_client_runtime::hud::AVAILABLE_EXPERIENCE,
        OWNER_AVAILABLE_XP,
    )]);
    for _ in 0..3 {
        app.frame();
    }
    assert_eq!(
        available_xp(&app),
        OWNER_AVAILABLE_XP,
        "the runtime AvailableExperience update landed on the player descriptor"
    );
    let button = footer_child(&mut app, attributes::PANEL, statmgmt::child::BUTTON_10);
    assert_eq!(
        disabled_attr(&mut app, button),
        Some(false),
        "after the XP update the footer re-runs and the +10 reads enabled"
    );
    assert_eq!(
        node_state(&mut app, button),
        statmgmt::button_state::ENABLED,
        "and is drawn in state 1"
    );
    let _ = wire_actions(&mut app);
    let hit = pointer_press(&mut app, button);
    assert_eq!(
        hit.map(|(h, _)| h),
        Some(button),
        "the press lands on the +10 (hit {hit:?})"
    );
    let sent = wire_actions(&mut app);
    let mut want = Vec::new();
    want.extend_from_slice(&1u32.to_le_bytes());
    want.extend_from_slice(&OWNER_STR_COST_10.to_le_bytes());
    assert_eq!(
        sent,
        vec![SentAction {
            queue: u16::from(dereth_client_net::queues::Queue::Weenie as u8),
            opcode: 0x0045,
            body: want
        }],
        "and a real press now sends the ten-point raise"
    );
}
