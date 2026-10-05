//! Every examine route -- the examine cursor's pick, the examine target mode, the identify button
//! and the selection-examine key -- sends one `0x00C8 Item_Appraise` and arms the awaited
//! appraisal id and the examine serial, so the reply that answers it opens the examination panel,
//! and a repeat reply for the object already shown refreshes the panel without reopening it.
//! Fixture: the retail dats and the recorded captures. No recording holds a 3D pick, so the routes
//! are driven directly on a headless bench seeded from a recording, and the recorded
//! `0x00C9 Item_SetAppraiseInfo` replies are replayed into a live gameplay screen, each paired with
//! a direct examine raised through the shared world operation.

use crate::common::captures_dir;
use crate::common::client_dir;
use crate::common::recorded_sessions;

use std::net::SocketAddr;

use dereth_client::app::App;
use dereth_client_net::client_session::SessionEvent;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_shell::ui::UiMouseEvent;
use dereth_primitives::{LocalTime, ObjectId, ServerTime};
use dereth_protocol::{Message, Opcode};
use dereth_ui_screens::panels::examination::{self, ExamineSubUi};
use dereth_ui_screens::view::{TargetMode, UiRequest};

// ---------------------------------------------------------------------------------------------
// Harness: the recorded-capture reader and a headless gameplay application.
// ---------------------------------------------------------------------------------------------

fn have_dats() {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are this file's oracle: none at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
}

use dereth_client_net::client_session::testing::capture::{self, Datagram as Record};

fn corpus_sessions() -> Vec<String> {
    let dir = captures_dir();
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
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
    out.sort();
    // The number is the corpus index's own, so a new recording re-measures this scan instead of
    // failing it.
    assert_eq!(
        out.len(),
        recorded_sessions(),
        "the recorded captures; found {out:?}"
    );
    out
}

fn load(session: &str) -> Vec<Record> {
    capture::shared_session(session).to_vec()
}

fn connection_sequence_number(records: &[Record]) -> u32 {
    dereth_client_net::recording::connection_sequence_number(records).unwrap_or(0)
}

fn addr(pair: u16) -> SocketAddr {
    capture::peer(pair)
}

fn app_in_gameplay(frames: u32) -> App {
    crate::common::app::app_in_gameplay(frames, None)
}

use crate::common::app::gameplay_screen;

/// Read this window node's own visible flag. This is not a raster check and does not inspect
/// ancestor visibility.
fn window_visible(app: &mut App) -> bool {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let h = ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("<EXAM> is in the layout");
    ui.node(h).expect("a live node").region.flags.visible
}

fn close_window(app: &mut App) {
    let (ui, screen) = gameplay_screen(app);
    let root = screen.root().expect("the gameplay root");
    let h = ui
        .get_child_recursive(root, examination::WINDOW)
        .expect("<EXAM>");
    ui.set_visible(h, false);
}

// ---------------------------------------------------------------------------------------------
// Half one: the four routes
// ---------------------------------------------------------------------------------------------

/// The four examine routes and how this headless fixture drives them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Direct wrapper mouse press/release followed by an injected object-found answer for the
    /// examine search reason. No rendered 3D pick runs here.
    Cursor,
    /// A directly queued examine target mode, direct wrapper click, and injected object-found
    /// answer. No toolbar button hit-test runs here.
    TargetMode,
    /// A directly queued `UiRequest::Examine` for the selected target.
    Button,
    /// Input action `0x1000002B SelectionExamine`.
    Key,
}

const ROUTES: [Route; 4] = [Route::Cursor, Route::TargetMode, Route::Button, Route::Key];

/// Build the direct input event used by the key route. The bench has no OS-key producer, 3D world
/// or network owner: `use_time` receives `world: None` and `net: None`, and the request remains in
/// `Interaction::last_sent` or the pending queue where the test reads its enum value.
fn key(action: u32) -> dereth_client_runtime::actions::Action {
    dereth_client_runtime::actions::Action {
        id: dereth_input::ActionId(action),
        phase: dereth_client_runtime::actions::ActionPhase::Repeat,
        extent: 1.0,
        repeats: 1,
    }
}

struct Bench {
    inter: dereth_client_runtime::interaction::Interaction,
    objects: ObjectStream,
    store: dereth_dat::RetailDatStore,
}

impl Bench {
    /// Fill the game tables from recorded server events rather than inventing an object id. The
    /// selection scans each recording until it finds the first inventory item with a live object,
    /// falling back to the first non-player live object, and stops as soon as a target is found.
    /// This supplies the object-found handler's `game.weenie(id).is_some()` precondition; it is not
    /// a complete corpus inventory census.
    ///
    /// Returns the bench and the object it will examine, which is one the recording created.
    fn new() -> (Self, ObjectId) {
        have_dats();
        let mut b = Self {
            inter: dereth_client_runtime::interaction::Interaction::new(),
            objects: ObjectStream::new(),
            store: dereth_dat::RetailDatStore::open_dir(&client_dir()).expect("the retail dats"),
        };
        let mut target = None;
        let mut tried = Vec::new();
        for session in corpus_sessions() {
            let records = load(&session);
            let csn = connection_sequence_number(&records);
            let mut net =
                ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("net");
            let mut entered = false;
            let mut peak = 0usize;
            let mut saw_player = false;
            for r in &records {
                let now = LocalTime(r.t);
                if !r.c2s {
                    net.feed(&r.raw, addr(r.pair), now);
                }
                net.tick(now);
                let _ = net.take_outgoing();
                let events = b.objects.pump(&mut net, now);
                for e in &events {
                    if let SessionEvent::CharacterSet(set) = e {
                        if !entered {
                            if let Some(c) = set.characters.first() {
                                let account = set.account.clone();
                                net.enter_world(c.gid, &account);
                                entered = true;
                            }
                        }
                    }
                }
                peak = peak.max(b.objects.world.tables.weenies.len());
                saw_player |= b.objects.world.player.is_some();
                // Any object the recording created that is not the player itself -- the pick's
                // own precondition is that the id names a known object, and nothing more.
                if let Some(p) = b.objects.world.player {
                    if let Some(id) = b
                        .objects
                        .world
                        .inventory(p)
                        .and_then(|inv| {
                            inv.items
                                .iter()
                                .copied()
                                .find(|i| b.objects.world.weenie(*i).is_some())
                        })
                        .or_else(|| {
                            b.objects
                                .world
                                .tables
                                .weenies
                                .iter()
                                .map(|(id, _)| id)
                                .find(|id| *id != p)
                        })
                    {
                        target = Some(id);
                        break;
                    }
                }
            }
            // The denominator distinguishes a replay that found no target from one that never
            // ran. Peak is used because each log-off resets the world tables.
            tried.push(format!(
                "{session}: saw_player={saw_player} peak_weenies={peak}"
            ));
            if target.is_some() {
                break;
            }
        }
        let target = target.unwrap_or_else(|| {
            panic!("no capture puts an object in the tables to examine -- {tried:?}")
        });
        // The replay may have left requests of its own in the outbox; this bench measures what the
        // *gesture* sends, so start from a clean one.
        b.inter.take_pending_requests();
        b.inter.last_sent.clear();
        (b, target)
    }

    fn use_time(&mut self, actions: Vec<dereth_client_runtime::actions::Action>, now: f64) {
        dereth_client_runtime::interaction::use_time(
            &mut self.inter,
            &self.store,
            None,
            &mut self.objects,
            None,
            actions,
            false,
            (800, 600),
            LocalTime(now),
        );
    }

    /// Every pending or last-sent `Request::Appraise` this bench has produced, as its target.
    ///
    /// Both halves of the outbox: `use_time` step 4 drains `Interaction::outbox` into `last_sent`,
    /// so a request made *during* a `use_time` is in the second and one made after it (the two
    /// routes that finish in `on_world_object_found`) is still in the first.
    fn appraises(&self) -> Vec<ObjectId> {
        self.inter
            .last_sent
            .iter()
            .chain(self.inter.pending_requests())
            .filter_map(|r| match r {
                dereth_client_model::Request::Appraise(a) => Some(a.target),
                _ => None,
            })
            .collect()
    }

    fn awaiting(&self) -> Option<ObjectId> {
        self.objects.world.appraisal.examining
    }

    fn serial(&self) -> u64 {
        self.objects.world.appraisal.examine_serial
    }

    /// Drive one route at `target`. The callers inspect request enums, awaiting id and serial.
    fn drive(&mut self, route: Route, target: ObjectId, now: f64) {
        use dereth_ui::focus::action;
        match route {
            Route::Cursor => {
                // Drive a direct right press/release at the same coordinates. In the original
                // frame order, mouse release and the later object-found notice share a frame. The
                // notice is raised outside the player-present guard, so inserting `use_time`
                // between gesture and injected answer supplies object id zero and clears the search
                // reason. That older fixture shape could observe only a reason that survived a
                // frame. This bench instead reads the armed reason before it directly supplies the
                // found id; it does not run the 3D pick or claim an OS mouse producer.
                for start in [true, false] {
                    self.inter.wrapper_mouse(
                        UiMouseEvent {
                            action: action::SECONDARY_CLICK,
                            start,
                            x: 400,
                            y: 300,
                            over: None,
                        },
                        (800, 600),
                        true,
                    );
                }
                // The gesture armed the pick. Assert that, rather than assume it: the whole route
                // dies quietly if the release is read as a camera drag.
                assert_eq!(
                    format!("{:?}", self.inter.search_reason()),
                    "Examine",
                    "the direct right-button release must arm the examine search reason"
                );
                assert!(
                    self.inter.pick.looking_for_object(),
                    "and the world-object lookup remains armed for the injected answer"
                );
                // Supply the object-found answer that the omitted rendered pick would have raised.
                self.inter
                    .on_world_object_found(target, &mut self.objects.world, ServerTime(now));
            }
            Route::TargetMode => {
                // Queue examine target mode directly, then send a direct left press which arms
                // targeted use. The object-found answer below completes the route.
                self.inter
                    .queue(vec![], vec![UiRequest::SetTargetMode(TargetMode::Examine)]);
                self.use_time(vec![], now);
                // Drive only the direct wrapper input before supplying the answer.
                self.inter.wrapper_mouse(
                    UiMouseEvent {
                        action: action::PRIMARY_CLICK,
                        start: true,
                        x: 400,
                        y: 300,
                        over: None,
                    },
                    (800, 600),
                    true,
                );
                assert_eq!(
                    format!("{:?}", self.inter.search_reason()),
                    "TargetedUse",
                    "a direct left click with examine target mode arms targeted-use search"
                );
                assert!(self.inter.pick.looking_for_object(), "and armed the pick");
                self.inter
                    .on_world_object_found(target, &mut self.objects.world, ServerTime(now));
            }
            Route::Button => {
                self.inter.queue(vec![], vec![UiRequest::Examine(target)]);
                self.use_time(vec![], now);
            }
            Route::Key => {
                // Selection-examine reads the selected object, so select this recorded target
                // before supplying the direct input event.
                self.inter.queue(vec![], vec![UiRequest::Select(target)]);
                self.use_time(vec![], now);
                assert_eq!(self.objects.world.selected, Some(target));
                self.use_time(
                    vec![key(
                        dereth_client_contract::actions::mapped::SELECTION_EXAMINE.0,
                    )],
                    now + 0.1,
                );
            }
        }
    }
}

/// Behaviour: examine.route.every-examine-route-sends-the-same-request-and-arms-the-wait
/// Both halves, asserted separately for each route: the `Item_Appraise` request, and the awaited
/// appraisal id that lets the reply open the panel. A test of the request alone would pass for a
/// route that never arms the panel. The routes are counted, so "the route never ran" and "the route
/// ran and did not arm" are different answers.
#[test]
fn every_examine_route_reaches_examine_object_and_sends_the_same_request() {
    let mut armed = 0;
    for (i, route) in ROUTES.into_iter().enumerate() {
        let (mut b, target) = Bench::new();
        assert!(
            b.objects.world.weenie(target).is_some(),
            "{route:?}: the object the gesture names is one the recording created"
        );
        assert_eq!(
            b.awaiting(),
            None,
            "{route:?}: nothing is awaited before the gesture"
        );
        assert_eq!(b.serial(), 0);

        b.drive(route, target, 1.0 + i as f64);

        // Half one: exactly one pending/last-sent Item_Appraise request enum naming the target.
        // This station does not serialize or inspect wire bytes.
        assert_eq!(
            b.appraises(),
            vec![target],
            "{route:?}: exactly one Item_Appraise, for the object the gesture named"
        );
        // Half two: the awaited appraisal id and the examine serial.
        assert_eq!(
            b.awaiting(),
            Some(target),
            "{route:?}: the examine notice must set the awaiting appraisal id; otherwise the \
             reply is not accepted as new and the panel's visible flag is never set"
        );
        assert_eq!(b.serial(), 1, "{route:?}: one notice, not two");
        armed += 1;
    }
    assert_eq!(
        armed, 4,
        "all four examine routes arm the awaited appraisal id"
    );
}

/// The examine operation returns immediately for id zero, and
/// `ObjectId(0)` is exactly what a pick that found nothing carries. Without the guard the panel
/// would arm on empty air and the next unrelated reply would be refused for the wrong reason.
#[test]
fn examining_the_null_object_arms_nothing_and_sends_nothing() {
    let (mut b, _) = Bench::new();
    b.inter.queue(vec![], vec![UiRequest::Examine(ObjectId(0))]);
    b.use_time(vec![], 1.0);
    assert_eq!(b.appraises(), Vec::<ObjectId>::new());
    assert_eq!(b.awaiting(), None);
    assert_eq!(b.serial(), 0);
}

// ---------------------------------------------------------------------------------------------
// Half two: the corpus
// ---------------------------------------------------------------------------------------------

struct Outcome {
    session: String,
    object: ObjectId,
    name: String,
    was_down_before: bool,
    is_up_after: bool,
    pane: Option<ExamineSubUi>,
    title_text: Option<String>,
    /// How many serial changes the panel's own pull has turned into examine notices by then.
    examines_pulled: u32,
}

/// Raise an examine through the shared world operation used by all four routes. The recorded
/// server event has already been pumped before this helper runs. The helper then promotes the
/// serial floor, raises the request directly, and frames once so the panel can pull
/// `GameView::examine_request`; it does not replay the original gesture timing.
fn raise_examine(app: &mut App, id: ObjectId, floor: &mut u64) {
    // The serial is carried across the recording boundary by hand, and it has to be.
    //
    // `GameView::examine_request` hands the panel `(id, AppraisalCache::examine_serial)` and the
    // panel pulls only when that serial differs from the last one it pulled -- which is what
    // makes a repeat examine of the *same* object a new notice. The cache belongs to the
    // `World` and is re-created on every login, so its serial restarts at 0 for each recording;
    // the panel belongs to the screen, which this test keeps for the whole sweep. In the client
    // those two are created and destroyed together and the serial a panel sees is monotonic.
    // Without the floor, one recording's first examine can carry the serial the panel already
    // pulled from the previous recording and would not count as a new notice. The floor makes the
    // serial monotonic over the whole sweep, which is the client's guarantee, while still leaving
    // the shared world operation as producer.
    {
        let cache = &mut app.probe_mut().objects_mut().world.appraisal;
        cache.examine_serial = cache.examine_serial.max(*floor);
    }
    let mut req = dereth_client_model::RecordingRequests::default();
    app.probe_mut()
        .objects_mut()
        .world
        .examine_object(&mut req, id);
    *floor = app.probe_mut().objects_mut().world.appraisal.examine_serial;
    assert!(
        req.0
            .iter()
            .any(|r| matches!(r, dereth_client_model::Request::Appraise(a) if a.target == id)),
        "examine_object must also send Item_Appraise -- it is attempt_appraise plus one line"
    );
    assert_eq!(
        app.probe_mut().objects_mut().world.appraisal.examining,
        Some(id)
    );
    // The application frame runs the examination-panel update and pulls the new serial.
    app.frame();
}

fn deliver_reply(
    app: &mut App,
    object: ObjectId,
    profile: &dereth_protocol::types::AppraisalProfile,
) {
    let mut sink = dereth_client_model::RecordingSink::default();
    app.probe_mut()
        .objects_mut()
        .world
        .set_appraise_info(object, profile.clone(), &mut sink);
    app.frame();
}

fn replay_and_examine(app: &mut App, session: &str, out: &mut Vec<Outcome>, floor: &mut u64) {
    let records = load(session);
    let csn = connection_sequence_number(&records);
    let mut net = ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn)
        .expect("a replay client network");
    let mut entered = false;
    for r in records.iter().filter(|r| !r.c2s) {
        let now = LocalTime(r.t);
        net.feed(&r.raw, addr(r.pair), now);
        net.tick(now);
        let _ = net.take_outgoing();
        let events = app.probe_mut().objects_mut().pump(&mut net, now);
        let _ = app.apply_hud_events(&events);
        for e in &events {
            match e {
                SessionEvent::CharacterSet(set) if !entered => {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
                SessionEvent::UiEvent { opcode, .. }
                    if *opcode == Opcode::ITEM_SET_APPRAISE_INFO =>
                {
                    let (_, body) = e.ui_body().expect("UI event");
                    let mut rd = dereth_protocol::archive::Reader::new(body);
                    let m = dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut rd)
                        .expect("a recorded 0x00C9 decodes");
                    let name = app
                        .probe_mut()
                        .objects_mut()
                        .world
                        .weenie(m.object)
                        .map_or_else(String::new, |w| w.pwd.name.clone());
                    let was_down_before = !window_visible(app);
                    raise_examine(app, m.object, floor);
                    assert!(
                        !window_visible(app),
                        "{}/{:?}: the examine notice alone must not set the panel visible; only an awaited reply does",
                        session,
                        m.object
                    );
                    deliver_reply(app, m.object, &m.profile);
                    let is_up_after = window_visible(app);
                    let (_ui, screen) = gameplay_screen(app);
                    out.push(Outcome {
                        session: session.to_string(),
                        object: m.object,
                        name,
                        was_down_before,
                        is_up_after,
                        pane: screen.examination.active,
                        title_text: screen.examination.title_text.clone(),
                        examines_pulled: screen.examination.examines_pulled,
                    });
                    close_window(app);
                    app.frame();
                }
                _ => {}
            }
        }
    }
}

/// Every recorded `0x00C9` reply, paired with a direct examine request before its decoded profile
/// is applied, sets the examination window visible and fills its pane and title.
#[test]
fn every_recorded_reply_opens_the_panel_when_the_route_asked_for_it() {
    have_dats();
    let mut app = app_in_gameplay(3);
    let mut out = Vec::new();
    let mut serial_floor = 0u64;
    for s in corpus_sessions() {
        replay_and_examine(&mut app, &s, &mut out, &mut serial_floor);
    }
    assert_eq!(
        out.len(),
        30,
        "the current eighteen-session corpus carries 30 recorded 0x00C9 replies; got {}",
        out.len()
    );
    // The per-recording split of the replies.
    let mut per_session: std::collections::BTreeMap<String, usize> =
        corpus_sessions().into_iter().map(|s| (s, 0)).collect();
    for o in &out {
        *per_session.entry(o.session.clone()).or_default() += 1;
    }
    let split: Vec<(&str, usize)> = per_session.iter().map(|(s, n)| (s.as_str(), *n)).collect();
    assert_eq!(
        split,
        [
            ("combat-mode-while-moving", 0),
            ("ddd-interrogation-only", 0),
            ("early-inventory-and-casting", 4),
            ("fellowship-one-vassal", 0),
            ("fellowship-three-vassal", 0),
            ("fellowship-two-monarch", 0),
            ("first-login-walk-jump", 0),
            ("house-purchase-and-trade", 3),
            ("house-purchase-refused", 0),
            ("login-account-booted", 0),
            ("long-movement-run", 2),
            ("long-solo-play", 20),
            ("melee-attack-run", 0),
            ("post-relog-attribute-training", 1),
            ("pre-relog-play", 0),
            ("requested-death-vitae-salvage", 0),
            ("short-play-with-training", 0),
            ("short-second-connection", 0),
        ]
    );
    let mut opened = 0;
    for o in &out {
        assert!(
            o.was_down_before,
            "{}/{:?}: the examination window node's own visible flag must start false",
            o.session, o.object
        );
        // The assertion requires only a nonempty title; it does not compare the title with the
        // recorded object's name.
        assert!(
            o.is_up_after,
            "{}/{:?} ({}): the reply for an awaited id must set <EXAM>'s own visible flag",
            o.session, o.object, o.name
        );
        assert!(
            o.pane.is_some(),
            "{}/{:?}: a pane must be active",
            o.session,
            o.object
        );
        assert!(
            o.title_text.as_ref().is_some_and(|t| !t.is_empty()),
            "{}/{:?}: the examination title is present and nonempty",
            o.session,
            o.object
        );
        opened += 1;
    }
    // Every recorded reply opens the panel.
    assert_eq!(opened, 30);
    // The denominator that says the *pull* ran, not just that the panel opened -- so "the seam
    // carried the notice" and "the panel was already open" are different answers.
    assert_eq!(
        out.last().expect("30 outcomes").examines_pulled,
        30,
        "every one of the 30 reached the panel through GameView::examine_request"
    );
}

/// Behaviour: examine.window.an-answer-the-pane-did-not-ask-for-opens-nothing-and-a-repeat-does-not-reopen-it
/// A new awaited reply shows the panel; a reply for the current object only refreshes its data.
/// This test manually applies a second `0x00C9` for the same object with **no** intervening examine.
/// It does not drive the 0.75-second combat re-poll or its scheduler.
///
/// Asserting the negative is the point: without it, "the panel opens" and "the panel is always
/// open" are the same measurement.
#[test]
fn a_second_reply_for_the_same_object_does_not_reopen_a_panel_the_player_closed() {
    have_dats();
    let mut app = app_in_gameplay(3);
    let mut first: Option<(ObjectId, dereth_protocol::types::AppraisalProfile)> = None;
    'outer: for s in corpus_sessions() {
        let records = load(&s);
        let csn = connection_sequence_number(&records);
        let mut net =
            ClientNetwork::new("127.0.0.1:19000", 7304, "ac01", "pass", csn).expect("net");
        let mut entered = false;
        for r in records.iter().filter(|r| !r.c2s) {
            let now = LocalTime(r.t);
            net.feed(&r.raw, addr(r.pair), now);
            net.tick(now);
            let _ = net.take_outgoing();
            let events = app.probe_mut().objects_mut().pump(&mut net, now);
            let _ = app.apply_hud_events(&events);
            for e in &events {
                match e {
                    SessionEvent::CharacterSet(set) if !entered => {
                        if let Some(c) = set.characters.first() {
                            let account = set.account.clone();
                            net.enter_world(c.gid, &account);
                            entered = true;
                        }
                    }
                    SessionEvent::UiEvent { opcode, .. }
                        if *opcode == Opcode::ITEM_SET_APPRAISE_INFO =>
                    {
                        let (_, body) = e.ui_body().expect("UI event");
                        let mut rd = dereth_protocol::archive::Reader::new(body);
                        let m = dereth_protocol::objects::ItemSetAppraiseInfo::read(&mut rd)
                            .expect("decodes");
                        if app
                            .probe_mut()
                            .objects_mut()
                            .world
                            .weenie(m.object)
                            .is_some()
                        {
                            first = Some((m.object, m.profile.clone()));
                            break 'outer;
                        }
                    }
                    _ => {}
                }
            }
        }
    }
    let (object, profile) = first.expect("the corpus carries a 0x00C9 for a live object");

    // Station 1: the player examines, the reply lands, the panel opens.
    assert!(!window_visible(&mut app));
    raise_examine(&mut app, object, &mut 0);
    deliver_reply(&mut app, object, &profile);
    assert!(
        window_visible(&mut app),
        "the awaited reply sets the examination window node visible"
    );

    // Station 2: the player closes it. `awaiting` is now clear and `current` holds the object.
    close_window(&mut app);
    app.frame();
    assert!(!window_visible(&mut app));
    {
        let (_ui, screen) = gameplay_screen(&mut app);
        assert_eq!(
            screen.examination.awaiting, None,
            "applying the awaited reply cleared it"
        );
        assert_eq!(
            screen.examination.current,
            Some(object),
            "and moved it to current"
        );
    }

    // Station 3: manually apply the *same* object's reply with no new examine. It must refresh and
    // stay down. The `replies_applied >= 2` check below is an absolute counter observation, not an
    // exact one-reply delta.
    deliver_reply(&mut app, object, &profile);
    assert!(
        !window_visible(&mut app),
        "a reply matching the current appraisal object refreshes the panel and must not set \
         the examination window node visible again"
    );
    {
        let (_ui, screen) = gameplay_screen(&mut app);
        assert!(
            screen.examination.replies_applied >= 2,
            "it did take the second reply"
        );
    }

    // Station 4: a *fresh* examine of the same object re-opens it because a new examine notice
    // clears the current appraisal id. This is the case the serial in
    // `GameView::examine_request` exists for: the id did not change.
    raise_examine(&mut app, object, &mut 0);
    {
        let (_ui, screen) = gameplay_screen(&mut app);
        assert_eq!(
            screen.examination.awaiting,
            Some(object),
            "re-armed by the same id"
        );
        assert_eq!(screen.examination.current, None, "and current cleared");
    }
    deliver_reply(&mut app, object, &profile);
    assert!(
        window_visible(&mut app),
        "a new examine of the same object sets the examination window node visible again"
    );
}
