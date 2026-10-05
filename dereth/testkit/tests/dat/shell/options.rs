//! Shell fixtures and scenarios for options.

use super::*;
// ---------------------------------------------------------------------------------------------
// options.character-page.*
//
// The option ordinals and their bit masks, written out as literals beside the symbols that carry
// them, are a transcription and carry no row. That every row of the page edits the setting of the
// same name is the premise of `each-row-shows-the-bit-the-shard-sent-for-it` and is asserted inside
// it.
//
// **The counts here are read from the corpus index and never pinned.** Rather than a per-recording
// table of how many settings records each one sent and which header word each carried,
// `Outbound::count` answers the first from the index, and the second is a property of the bytes
// rather than of the corpus, so what is asserted is the fixed point over every recorded record
// rather than a table that a promoted recording moves.
// ---------------------------------------------------------------------------------------------

/// The ordered game action the settings record travels in, and the record's own message.
const CHARACTER_OPTIONS_EVENT: u32 = 0x01A1;

/// One recorded settings record: which recording sent it, the stamp of its envelope, and its body.
struct RecordedOptions {
    session: String,
    stamp: u32,
    body: Vec<u8>,
}

/// Every settings record the recorded clients sent, read out of the committed recordings.
///
/// **Why the payload is read here rather than through `Outbound::recorded`.** That reader answers
/// *which* message each client-to-server blob is and when it went, which is what a claim about
/// traffic needs; it does not carry the bytes, and the claim below is that the bytes come back
/// unchanged. The count it does answer is used as the cross-check, so the two readers have to agree
/// about how many there are. The missing bytes are a gap in the harness.
///
/// # Panics
/// Panics when a recording named by the index does not parse, which is a broken checkout.
fn recorded_option_records() -> Vec<RecordedOptions> {
    use dereth_client_net::client_session::testing::{Corpus, Direction};

    let mut out: Vec<RecordedOptions> = Vec::new();
    for name in dereth_client_net::client_session::testing::session_names() {
        let corpus = Corpus::load(name)
            .unwrap_or_else(|e| panic!("the recording {name} does not parse: {e}"))
            .unwrap_or_else(|| panic!("the decoded corpus has no recording {name}"));
        for b in &corpus.blobs {
            if b.dir != Direction::ClientToServer
                || dereth_testkit::outbound::message_of(&b.payload) != Some(CHARACTER_OPTIONS_EVENT)
            {
                continue;
            }
            out.push(RecordedOptions {
                session: (*name).to_owned(),
                stamp: u32::from_le_bytes(b.payload[4..8].try_into().expect("a stamp")),
                body: b.payload[12..].to_vec(),
            });
        }
    }
    assert!(
        !out.is_empty(),
        "no recording carries a settings record; a scan that read nothing would pass over nothing"
    );
    // The cross-check, against the index rather than a table written here: for every recording
    // that sent one, the crate's own client-to-server reader has to agree about how many.
    //
    // It is asked only about those recordings because `Outbound::all` refuses a recording with no
    // client-to-server blob at all, and the corpus has one. That is a gap in the harness.
    let mut per_session: std::collections::BTreeMap<&str, usize> =
        std::collections::BTreeMap::new();
    for r in &out {
        *per_session.entry(r.session.as_str()).or_default() += 1;
    }
    for (name, n) in per_session {
        assert_eq!(
            dereth_testkit::Outbound::count(name, CHARACTER_OPTIONS_EVENT),
            n,
            "{name}: the two readers disagree about how many settings records it carries"
        );
    }
    out
}

/// The one settings record a named recording sent.
fn the_record_of(session: &str) -> RecordedOptions {
    let mut it = recorded_option_records()
        .into_iter()
        .filter(|b| b.session == session);
    let first = it
        .next()
        .unwrap_or_else(|| panic!("{session} sent no settings record"));
    assert!(it.next().is_none(), "{session} sent more than one");
    first
}

/// Decode a recorded settings body into the record the client packed.
fn decode_module(body: &[u8]) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::Reader;
    let mut r = Reader::with_origin(body, 12);
    let m = dereth_protocol::login::PlayerModule::read(&mut r)
        .expect("a recorded settings record decodes");
    r.expect_exhausted()
        .expect("the cursor lands on the end of the record");
    m
}

/// Re-encode one the way the client's own sender does, so the alignment origin is the blob's.
fn encode_module(m: &dereth_protocol::login::PlayerModule) -> Vec<u8> {
    let mut w = dereth_protocol::actions::action_body_writer();
    m.write(&mut w).expect("it encodes");
    w.into_inner()
}

/// A world whose settings are a recorded record, the way the shard's description gives it one.
fn world_with(module: &dereth_protocol::login::PlayerModule) -> dereth_client_model::World {
    let mut w = dereth_client_model::World::new();
    w.player_system.apply_player_module(module);
    w
}

/// Every row of the page shows the setting the shard sent for it.
pub(super) fn every_row_shows_the_setting_the_shard_sent() {
    use dereth_client_contract::options::interface::Interface;
    use dereth_client_contract::options::sheet::{rows_for, PageId, Value};
    use dereth_client_model::player::options::PLAYER_OPTIONS;
    use dereth_ui_screens::options::character::option_name;
    use dereth_ui_screens::view::{GameView, PlayerOption};
    use {
        dereth_client_runtime::hud::character_option, dereth_client_runtime::hud::option_ordinal,
        dereth_client_shell::hud::Hud,
    };

    let record = the_record_of("first-login-walk-jump");
    let module = decode_module(&record.body);

    // The premise: every row edits the setting of the same name, and no two rows edit the same one.
    // Two tables meet here and neither is derived from the other.
    let mut seen = std::collections::BTreeSet::new();
    let mut names_agree = true;
    for row in rows_for(PageId::Character, Interface::Modern) {
        if let Value::Option(o) = row.value {
            let n = option_ordinal(o);
            names_agree &= PLAYER_OPTIONS[n].0 == option_name(o) && seen.insert(n);
        }
    }

    let mut objects = dereth_client_runtime::objects::ObjectStream::default();
    objects.world.player_system.apply_player_module(&module);
    let hud = Hud::default();
    let view = hud.view(&objects);

    let mut asked = 0usize;
    let mut ticked = 0usize;
    let mut each_row_agrees = true;
    for row in rows_for(PageId::Character, Interface::Modern) {
        if let Value::Option(o) = row.value {
            asked += 1;
            let n = option_ordinal(o);
            let (_, word, mask) = PLAYER_OPTIONS[n];
            let expect = match word {
                dereth_client_model::player::OptionWord::One => module.options & mask != 0,
                dereth_client_model::player::OptionWord::Two => module.options2 & mask != 0,
            };
            each_row_agrees &= view.player_option(o) == expect;
            ticked += usize::from(expect);
        }
    }

    // The third answer, which a yes-or-no cannot carry: a client that has been told nothing says
    // it does not know. Without it "off" and "never asked" are the same reading, and the page
    // that opened blank read exactly like a page of things turned off.
    let empty = dereth_client_runtime::objects::ObjectStream::default();
    let unknown = character_option(&empty.world, PlayerOption::AutoTarget).is_none();
    let known = character_option(&objects.world, PlayerOption::AutoTarget).is_some();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.each-row-shows-the-bit-the-shard-sent-for-it",
        move |_| {
            names_agree
            && seen.len() == asked
            && asked > 40
            && each_row_agrees
            // Some of this character's rows are on and some are off, or the comparison above
            // would hold on a reader that answered one thing for everything.
            && ticked > 0
            && ticked < asked
            && unknown
            && known
        },
    );
}

/// A tick changes one setting of what the shard sent and nothing else.
pub(super) fn a_tick_changes_only_that_setting() {
    use dereth_client_runtime::interaction::Interaction;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let record = the_record_of("first-login-walk-jump");
    let module = decode_module(&record.body);
    let before = (module.options, module.options2);
    // A setting this client does not model at all, carried in what the shard sent: it must come
    // through every write below untouched, and that is the regression this claim exists for.
    let unmodelled = module.options2 & 0x0200_0000;
    let mut world = world_with(&module);

    // A setting the client holds back until the record is saved.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    let arm_exists = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty();
    let held_back = inter.pending_requests().is_empty()
        && inter.stats.option_changes_deferred == 1
        && inter.stats.option_changes_unsendable == 0;
    let after = {
        let m = world
            .player_system
            .module
            .as_ref()
            .expect("the record is kept");
        (m.options, m.options2)
    };
    let dirty = world.player_system.is_dirty();

    // Setting it again is not a change at all.
    let mut again = Interaction::default();
    again.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    let no_second_change = again
        .run_ui_requests(&mut world, false, ServerTime(1.0))
        .is_empty()
        && again.stats.option_changes_deferred == 0;

    // And a setting the client sends the moment it moves goes out at once, on its own.
    let mut at_once = Interaction::default();
    at_once.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::HearGeneralChat,
            false,
        )],
    );
    let sent_at_once = at_once
        .run_ui_requests(&mut world, false, ServerTime(2.0))
        .is_empty()
        && at_once.stats.option_changes_unsendable == 0
        && at_once.stats.option_changes_sent == 1;
    let one_option_per_message = at_once
        .take_pending_requests()
        .iter()
        .filter(|r| matches!(r, dereth_client_model::Request::PlayerOptionChanged(_)))
        .count()
        == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.a-tick-changes-only-that-bit-of-what-the-shard-sent",
        move |_| {
            arm_exists
                && held_back
                && after.0 == before.0
                && after.1 == before.1 | 0x0000_0040
                && after.1 & 0x0200_0000 == unmodelled
                && dirty
                && no_second_change
                && sent_at_once
                && one_option_per_message
        },
    );
}

/// Every recorded settings record, taken in and sent back out, is the same bytes.
pub(super) fn the_settings_it_sends_back_are_byte_for_byte_the_recorded_ones() {
    use dereth_client_model::Request;

    let records = recorded_option_records();
    let mut all_identical = true;
    let mut all_framed = true;
    let mut cleared = true;
    for record in &records {
        let module = decode_module(&record.body);
        let mut world = world_with(&module);
        let packed = world
            .player_system
            .client_packed_module()
            .expect("the record is kept");
        all_identical &= encode_module(&packed) == record.body;

        // ...and through the sender, framed: the whole blob including its envelope.
        let mut req = dereth_client_model::RecordingRequests::default();
        assert!(
            world.player_system.save_to_server(&mut req, true),
            "a forced save always sends"
        );
        let [Request::CharacterOptionsEvent(m)] = req.0.as_slice() else {
            panic!(
                "{}: expected one settings record, got {:?}",
                record.session, req.0
            )
        };
        let framed = dereth_protocol::actions::pack_action(record.stamp, m).expect("it frames");
        all_framed &= framed.len() > 12 && framed[12..] == record.body[..];
        cleared &= !world.player_system.is_dirty();
    }
    let how_many = records.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour("options.character-page.the-settings-it-sends-back-are-byte-for-byte-the-ones-a-real-client-sent", move |_| {
        how_many > 0 && all_identical && all_framed && cleared
    });
}

/// One tick and one visit is one message with one setting moved.
pub(super) fn one_tick_and_one_visit_sends_one_message() {
    use dereth_client_model::Request;
    use dereth_client_runtime::interaction::Interaction;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let record = the_record_of("first-login-walk-jump");
    let original = decode_module(&record.body);
    let mut world = world_with(&original);

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![
            UiRequest::SetPlayerOption(PlayerOption::DisplayTimeStamps, true),
            UiRequest::SavePlayerOptions,
        ],
    );
    let ran = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty();
    let exactly_one = inter.stats.player_modules_sent == 1;

    let [Request::CharacterOptionsEvent(sent)] = inter.pending_requests() else {
        panic!(
            "expected one settings record, got {:?}",
            inter.pending_requests()
        )
    };
    let bytes = encode_module(&sent.module);
    // **Exactly one byte of the whole record differs**, and the narrowness is the assertion: the
    // spell bars, the window sizes and every other setting came through because the record was
    // changed in place rather than rebuilt.
    let differing: Vec<usize> = (0..bytes.len().min(record.body.len()))
        .filter(|i| bytes[*i] != record.body[*i])
        .collect();
    let same_length = bytes.len() == record.body.len();
    let carried_through = sent.module.options == original.options
        && sent.module.options2 == original.options2 | 0x0000_0040
        && sent.module.gameplay_options == original.gameplay_options
        && sent.module.spell_bars == original.spell_bars;

    // A second visit with nothing changed sends nothing.
    let mut nothing = Interaction::default();
    nothing.queue(Vec::new(), vec![UiRequest::SavePlayerOptions]);
    let quiet = nothing
        .run_ui_requests(&mut world, false, ServerTime(1.0))
        .is_empty()
        && nothing.stats.player_modules_sent == 0
        && nothing.pending_requests().is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.one-tick-and-one-visit-sends-one-message-with-one-bit-moved",
        move |_| {
            ran && exactly_one && same_length && differing.len() == 1 && carried_through && quiet
        },
    );
}

/// A deferred change reaches the shard on the frame's own timer, once.
pub(super) fn a_deferred_change_is_flushed_by_the_frame() {
    use dereth_client_model::Request;
    use dereth_client_runtime::interaction::Interaction;
    use dereth_primitives::{LocalTime, ServerTime};
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    let store = dereth_dat::testing::open_store().expect("the retail data files");
    let record = the_record_of("first-login-walk-jump");
    let mut objects = dereth_client_runtime::objects::ObjectStream::default();
    objects
        .world
        .player_system
        .apply_player_module(&decode_module(&record.body));
    let mut inter = Interaction::default();

    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(
            PlayerOption::DisplayTimeStamps,
            true,
        )],
    );
    assert!(inter
        .run_ui_requests(&mut objects.world, false, ServerTime(0.0))
        .is_empty());
    let deferred = objects.world.player_system.is_dirty() && inter.stats.player_modules_sent == 0;
    let _ = inter.take_pending_requests();

    // **Driven from the frame's own entry point and not from the method.** A scenario that called
    // the timer directly would survive deleting its call site in the frame: it structurally could
    // not see the change.
    let mut frame = |inter: &mut Interaction,
                     objects: &mut dereth_client_runtime::objects::ObjectStream,
                     t: f64| {
        dereth_client_runtime::interaction::use_time(
            inter,
            &store,
            None,
            objects,
            None,
            Vec::new(),
            false,
            (800, 600),
            LocalTime(t),
        );
    };

    frame(&mut inter, &mut objects, 479.0);
    let not_yet = inter.stats.player_modules_sent == 0;
    frame(&mut inter, &mut objects, 481.0);
    let flushed = inter.stats.player_modules_sent == 1;
    let one_message = matches!(
        inter.last_sent.as_slice(),
        [Request::CharacterOptionsEvent(_)]
    );
    frame(&mut inter, &mut objects, 9_999.0);
    let only_once = inter.stats.player_modules_sent == 1;

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.a-deferred-change-reaches-the-shard-eight-minutes-later-and-once",
        move |_| deferred && not_yet && flushed && one_message && only_once,
    );
}

/// With no description nothing is sent and nothing is invented.
pub(super) fn with_no_description_nothing_is_sent() {
    use dereth_client_runtime::interaction::Interaction;
    use dereth_primitives::ServerTime;
    use dereth_ui_screens::view::{PlayerOption, UiRequest};

    // Nothing to send, and nothing composed out of the defaults.
    let mut world = dereth_client_model::World::new();
    let nothing_kept = world.player_system.module.is_none();
    let mut req = dereth_client_model::RecordingRequests::default();
    let sends_nothing = !world.player_system.save_to_server(&mut req, true) && req.0.is_empty();
    let flag_cleared = !world.player_system.is_dirty();

    // ...and the setting still moves locally, so the state is right the moment a description
    // arrives.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::SetPlayerOption(PlayerOption::AutoTarget, false)],
    );
    let moved_locally = inter
        .run_ui_requests(&mut world, false, ServerTime(0.0))
        .is_empty()
        && !world.player_system.options.auto_target()
        && world.player_system.module.is_none();

    // And a record the shard shaped is narrowed to the shape this client sends: what a
    // description may carry and a settings record may not is dropped, with its gate.
    let record = the_record_of("first-login-walk-jump");
    let mut module = decode_module(&record.body);
    module.timestamp_format = Some("%H:%M".to_owned());
    module.spell_bars.truncate(5);
    module.option_flags = (module.option_flags & !0x0400) | 0x0080 | 0x0004;
    let mut shaped = world_with(&module);
    let packed = shaped
        .player_system
        .client_packed_module()
        .expect("the record is kept");
    let narrowed = packed.option_flags & 0x0080 == 0
        && packed.option_flags & 0x0004 == 0
        && packed.timestamp_format.is_none()
        && packed.spell_bars.len() == 8
        && packed.spell_bars[..5] == module.spell_bars[..]
        && !encode_module(&packed).is_empty();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "options.character-page.with-no-description-nothing-is-sent-and-nothing-is-invented",
        move |_| nothing_kept && sends_nothing && flag_cleared && moved_locally && narrowed,
    );
}

// ---------------------------------------------------------------------------------------------
// The character options page, live
// ---------------------------------------------------------------------------------------------

/// `UICore_Button_toggled` -- the attribute a tick box's drawn value lives in.
const ATTR_CHECKED: u32 = 0x0E;

/// A client in the world with the shipped settings the shard would have sent.
pub(super) fn a_client_with_settings() -> HeadlessClient {
    use dereth_protocol::login::{LoginPlayerDescription, PlayerModule};

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The shard's own description, in memory: no socket and no link. Its settings are the shipped
    // defaults, which is what every character starts from.
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: PlayerModule {
                    options: dereth_client_model::player::options::DEFAULT_OPTIONS,
                    options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                    ..PlayerModule::default()
                },
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
    c
}

/// Do something with the live gameplay screen.
pub(super) fn with_gameplay<R>(
    c: &mut HeadlessClient,
    f: impl FnOnce(
        &mut dereth_ui::UiSystem,
        &mut dereth_ui_screens::screens::gameplay::GamePlayScreen,
    ) -> R,
) -> R {
    let shell = c.app_mut().ui_mut().expect("the UI shell is up");
    let screen = shell.flow.current_mut().expect("a screen is current");
    let any: &mut dyn std::any::Any = &mut **screen;
    let gameplay = any
        .downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
        .expect("the gameplay screen is current");
    f(&mut shell.ui, gameplay)
}

/// Open the character options page through the toolbar and the tab.
pub(super) fn open_the_character_options_page(
    c: &mut HeadlessClient,
    hands: &mut Hands,
) -> dereth_ui::ElemHandle {
    let page = with_gameplay(c, |_, s| {
        s.character_options
            .page
            .expect("the character options page is bound")
    });
    open_options_page(c, hands, page)
}

/// What the player sees on a tick box.
pub(super) fn drawn_tick(c: &HeadlessClient, h: dereth_ui::ElemHandle) -> bool {
    dereth_ui_screens::bind::attr_bool(
        &c.view().expect_app().ui().expect("the UI shell is up").ui,
        h,
        ATTR_CHECKED,
    )
    .unwrap_or(false)
}

/// The first row whose value is `want`, skipping `skip`, whose box the pointer would really land
/// on -- the list clips everything past the visible dozen, and a scenario that aimed at a clipped
/// row would be measuring the viewport rather than the tick box.
fn a_clickable_row(
    c: &mut HeadlessClient,
    want: bool,
    skip: usize,
) -> (usize, dereth_ui::ElemHandle) {
    let rows: Vec<(usize, bool, dereth_ui::ElemHandle)> = with_gameplay(c, |_, s| {
        s.character_options
            .rows
            .iter()
            .enumerate()
            .map(|(i, r)| (i, r.current, r.element))
            .collect()
    });
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the UI shell is up").ui;
    rows.into_iter()
        .find(|(i, cur, h)| {
            *cur == want && *i != skip && {
                let b = ui.screen_box(*h);
                ui.hit_test_screen((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2) == Some(*h)
            }
        })
        .map(|(i, _, h)| (i, h))
        .unwrap_or_else(|| panic!("no reachable row is currently {want}"))
}

/// Ticking a row sticks, and unticking still works.
pub(super) fn a_ticked_row_stays_ticked_and_unticking_still_works() {
    let mut c = a_client_with_settings();
    let mut hands = Hands::new();
    open_the_character_options_page(&mut c, &mut hands);

    // Off to on, which is the direction a page that loses the tick fails in.
    let (i, h) = a_clickable_row(&mut c, false, usize::MAX);
    let started_off = !drawn_tick(&c, h);
    hands.click_handle(&mut c, h);
    let drawn = drawn_tick(&c, h);
    let remembered = with_gameplay(&mut c, |_, s| s.character_options.rows[i].current);
    let in_the_settings = {
        let o = with_gameplay(&mut c, |_, s| s.character_options.rows[i].option);
        c.view()
            .objects()
            .world
            .player_system
            .options
            .get(dereth_client_runtime::hud::option_ordinal(o))
    };
    // ...and still all three a good while later: the page re-reads every row from the settings on
    // every frame, so a row that lost the race would come back off on some later frame.
    c.tick(32);
    let still_drawn = drawn_tick(&c, h);
    let still_set = {
        let o = with_gameplay(&mut c, |_, s| s.character_options.rows[i].option);
        c.view()
            .objects()
            .world
            .player_system
            .options
            .get(dereth_client_runtime::hud::option_ordinal(o))
    };

    // On to off, on a different row so this is not the one just ticked.
    let (j, g) = a_clickable_row(&mut c, true, i);
    hands.click_handle(&mut c, g);
    let unticked =
        !drawn_tick(&c, g) && !with_gameplay(&mut c, |_, s| s.character_options.rows[j].current);
    c.tick(32);
    let stays_unticked = !drawn_tick(&c, g);

    c.assert_behaviour(
        "options.character-page.a-row-ticked-on-stays-ticked-and-unticking-still-works",
        move |_| {
            started_off
                && drawn
                && remembered
                && in_the_settings
                && still_drawn
                && still_set
                && unticked
                && stays_unticked
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// The two fellowship settings, and the shard that holds them
//
// The Auto-Accept box must not show off while auto-accept is on. Whether a fellowship request is
// ignored or accepted is **the shard's** decision, read off its own copy of the character's
// settings -- so a box can only be said to match the setting if the shard's copy is the client's.
// The shard is therefore modelled here, by folding what the client sends through the reference
// server's own rule: one message sets one setting, and it applies no exclusion of its own.
//
// Nothing leaves this process: the "shard" is that fold, and the description it sends back is
// handed to the client in memory.
// ---------------------------------------------------------------------------------------------

/// The two settings, as the client and the reference server both number them.
const IGNORE_REQUESTS: u32 = 0x02;
const AUTO_ACCEPT_REQUESTS: u32 = 0x12;
/// The two tick boxes of the fellowship tab, by the ids the panel builds them with.
const IGNORE_BOX: ElementId = ElementId(0x1000_0270);
const AUTO_ACCEPT_BOX: ElementId = ElementId(0x1000_0271);
/// The social panel, and the fellowship page inside it.
const SOCIAL_PANEL: u32 = 0x0C;
const SOCIAL_PAGE: ElementId = ElementId(0x1000_018F);
const FELLOWSHIP_PAGE: ElementId = ElementId(0x1000_0292);

/// The shard's own copy of the character's settings, written only by what the client sends.
///
/// One message names one setting and a value, and the reference server sets exactly that bit.
/// **It applies no exclusion of its own**, which is the whole reason the client has to send both
/// halves of one.
struct ShardSettings {
    word: u32,
}

impl ShardSettings {
    fn mask(option: u32) -> u32 {
        match option {
            IGNORE_REQUESTS => 0x0000_0008,
            AUTO_ACCEPT_REQUESTS => 0x2000_0000,
            other => {
                panic!("this scenario models only the two fellowship settings, not {other:#x}")
            }
        }
    }

    fn apply(&mut self, option: u32, value: bool) {
        let m = Self::mask(option);
        if value {
            self.word |= m;
        } else {
            self.word &= !m;
        }
    }

    fn holds(&self, option: u32) -> bool {
        self.word & Self::mask(option) != 0
    }
}

/// A client in the world, with the shard's settings word, and the shard beside it.
fn a_client_and_a_shard() -> (HeadlessClient, ShardSettings, usize) {
    let c = a_client_with_settings();
    let seen = c.outbound().len();
    (
        c,
        ShardSettings {
            word: dereth_client_model::player::options::DEFAULT_OPTIONS,
        },
        seen,
    )
}

/// Hand the client the shard's own description of the character -- the login, and the relog.
fn the_shard_describes_the_character(c: &mut HeadlessClient, shard: &ShardSettings) {
    use dereth_protocol::login::{LoginPlayerDescription, PlayerModule};

    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: PlayerModule {
                    options: shard.word,
                    options2: dereth_client_model::player::options::DEFAULT_OPTIONS2,
                    ..PlayerModule::default()
                },
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
}

/// Every setting change the client has put out since `seen`, in order, folded into the shard.
fn to_the_shard(
    c: &HeadlessClient,
    shard: &mut ShardSettings,
    seen: &mut usize,
) -> Vec<(u32, bool)> {
    let out: Vec<(u32, bool)> = c.outbound()[*seen..]
        .iter()
        .filter_map(|r| match r {
            dereth_client_model::Request::PlayerOptionChanged(m) => Some((m.option, m.value != 0)),
            _ => None,
        })
        .collect();
    *seen = c.outbound().len();
    for (o, v) in &out {
        shard.apply(*o, *v);
    }
    out
}

/// Open the social panel and then the fellowship tab inside it, by clicking what a player clicks.
fn open_the_fellowship_tab(c: &mut HeadlessClient, hands: &mut Hands) {
    let button = with_gameplay(c, |_, s| {
        s.toolbar
            .buttons
            .iter()
            .find(|b| b.panel_id == SOCIAL_PANEL)
            .expect("the toolbar has a social button")
            .handle
    });
    hands.click_handle(c, button);
    let tab = with_gameplay(c, |ui, s| {
        let root = s.root().expect("the gameplay root");
        let page = ui
            .get_child_recursive(root, SOCIAL_PAGE)
            .expect("the social page");
        let id = ui
            .node(page)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<dereth_ui::widgets::panel::Panel>()
            })
            .and_then(|p| p.page_to_tab.get(&FELLOWSHIP_PAGE).copied())
            .expect("the social page's tab table names the fellowship page");
        ui.get_child_recursive(page, id)
            .expect("the tab caption element")
    });
    hands.click_handle(c, tab);
    c.tick(2);
    let page = element(c, FELLOWSHIP_PAGE);
    assert!(
        c.view()
            .expect_app()
            .ui()
            .expect("the UI shell is up")
            .ui
            .is_visible(page),
        "the fellowship tab must be up"
    );
}

/// Whether the two boxes agree with the shard and with the client's own settings.
fn the_boxes_match(c: &mut HeadlessClient, shard: &ShardSettings) -> bool {
    let auto = drawn_tick(c, element(c, AUTO_ACCEPT_BOX));
    let ignore = drawn_tick(c, element(c, IGNORE_BOX));
    let model = |ordinal: usize| c.view().objects().world.player_system.options.get(ordinal);
    auto == shard.holds(AUTO_ACCEPT_REQUESTS)
        && auto == model(18)
        && ignore == shard.holds(IGNORE_REQUESTS)
        && ignore == model(2)
}

/// Turning one of the two on tells the shard the other is off, first.
pub(super) fn one_fellowship_setting_turns_the_other_off_at_the_shard() {
    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_fellowship_tab(&mut c, &mut hands);

    // The shipped default: requests are ignored and nothing is auto-accepted.
    let at_login = drawn_tick(&c, element(&c, IGNORE_BOX))
        && !drawn_tick(&c, element(&c, AUTO_ACCEPT_BOX))
        && the_boxes_match(&mut c, &shard)
        && to_the_shard(&c, &mut shard, &mut seen).is_empty();

    // 1. Stop ignoring: one message, nothing else moves.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step1 = to_the_shard(&c, &mut shard, &mut seen) == vec![(IGNORE_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard);

    // 2. Auto-accept on, with ignoring already off: no exclusion fires and one message goes.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let step2 = to_the_shard(&c, &mut shard, &mut seen) == vec![(AUTO_ACCEPT_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && shard.holds(AUTO_ACCEPT_REQUESTS);

    // 3. Ignore on. This one *does* exclude, and the shard has to be told the cleared setting
    //    first -- a client that cleared it silently would leave the shard's copy stale.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step3 = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false), (IGNORE_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && shard.holds(IGNORE_REQUESTS)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    // 4. Ignore off again: one message, and the shard now auto-accepts nothing.
    let h = element(&c, IGNORE_BOX);
    hands.click_handle(&mut c, h);
    let step4 = to_the_shard(&c, &mut shard, &mut seen) == vec![(IGNORE_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    // 5. Log in again: the shard sends its own word back and the boxes follow it.
    the_shard_describes_the_character(&mut c, &shard);
    let after_relog =
        the_boxes_match(&mut c, &shard) && !drawn_tick(&c, element(&c, AUTO_ACCEPT_BOX));

    c.assert_behaviour(
        "options.fellowship.turning-one-of-the-two-on-tells-the-shard-the-other-is-off-first",
        move |_| at_login && step1 && step2 && step3 && step4 && after_relog,
    );
    c.shutdown();
}

/// The same from the shipped default, the other way round -- and turning it off is one message.
pub(super) fn auto_accept_from_the_default_clears_ignoring_at_the_shard() {
    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_fellowship_tab(&mut c, &mut hands);
    let starts_ignoring = the_boxes_match(&mut c, &shard) && shard.holds(IGNORE_REQUESTS);
    let _ = to_the_shard(&c, &mut shard, &mut seen);

    // From the shipped default, auto-accept on has to clear ignoring, and the shard is told.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let both_told = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(IGNORE_REQUESTS, false), (AUTO_ACCEPT_REQUESTS, true)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(IGNORE_REQUESTS)
        && shard.holds(AUTO_ACCEPT_REQUESTS);

    // Turning it off excludes nothing -- the rule is about the setting being turned *on* -- so
    // one message.
    let h = element(&c, AUTO_ACCEPT_BOX);
    hands.click_handle(&mut c, h);
    let one_message = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false)]
        && the_boxes_match(&mut c, &shard)
        && !shard.holds(IGNORE_REQUESTS)
        && !shard.holds(AUTO_ACCEPT_REQUESTS);

    the_shard_describes_the_character(&mut c, &shard);
    let after_relog = the_boxes_match(&mut c, &shard);

    c.assert_behaviour("options.fellowship.the-same-holds-from-the-shipped-default-and-turning-it-off-again-is-one-message", move |_| {
        starts_ignoring && both_told && one_message && after_relog
    });
    c.shutdown();
}

/// On the options page the excluded row goes out at once, and cancelling restores both.
pub(super) fn the_excluded_row_goes_out_at_once_and_cancel_restores_both() {
    use dereth_ui_screens::view::PlayerOption as P;

    let (mut c, mut shard, mut seen) = a_client_and_a_shard();
    let mut hands = Hands::new();
    the_shard_describes_the_character(&mut c, &shard);
    open_the_character_options_page(&mut c, &mut hands);
    let opened_quietly = to_the_shard(&c, &mut shard, &mut seen).is_empty();

    let ignore_row = with_gameplay(&mut c, |_, s| {
        s.character_options
            .row_of(P::IgnoreFellowshipRequests)
            .expect("the row")
    });
    let auto_row = with_gameplay(&mut c, |_, s| {
        s.character_options
            .row_of(P::FellowshipAutoAcceptRequests)
            .expect("the row")
    });
    let row_drawn = |c: &mut HeadlessClient, i: usize| {
        let h = with_gameplay(c, |_, s| s.character_options.rows[i].element);
        drawn_tick(c, h)
    };
    let at_the_default = row_drawn(&mut c, ignore_row) && !row_drawn(&mut c, auto_row);

    // Tick auto-accept on the page. The row four above it has to go out on the notice, without
    // waiting for the page to be opened again.
    with_gameplay(&mut c, |ui, s| {
        let row = s.character_options.rows[auto_row].row;
        let list = s
            .character_options
            .option_box
            .as_mut()
            .expect("the option list");
        let idx = list
            .items
            .iter()
            .position(|h| *h == row)
            .expect("the row is a list item");
        list.scroll_to_view(ui, idx);
    });
    c.tick(1);
    let h = with_gameplay(&mut c, |_, s| s.character_options.rows[auto_row].element);
    hands.click_handle(&mut c, h);
    let both_told = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(IGNORE_REQUESTS, false), (AUTO_ACCEPT_REQUESTS, true)];
    let rows_followed = row_drawn(&mut c, auto_row) && !row_drawn(&mut c, ignore_row);
    let shard_followed = !shard.holds(IGNORE_REQUESTS) && shard.holds(AUTO_ACCEPT_REQUESTS);
    let boxes_followed = the_boxes_match(&mut c, &shard);

    // Cancel restores what the page opened with -- **both** rows, because both moved -- and the
    // shard is told about both, or it would keep a setting the player has just cancelled.
    let cancel = with_gameplay(&mut c, |ui, s| {
        let page = s.character_options.page.expect("the page");
        ui.get_child_recursive(page, dereth_ui_screens::options::config::button::CANCEL)
            .expect("the page's cancel button")
    });
    hands.click_handle(&mut c, cancel);
    let cancelled = to_the_shard(&c, &mut shard, &mut seen)
        == vec![(AUTO_ACCEPT_REQUESTS, false), (IGNORE_REQUESTS, true)];
    let back_at_the_default = row_drawn(&mut c, ignore_row) && !row_drawn(&mut c, auto_row);
    let shard_back = shard.holds(IGNORE_REQUESTS) && !shard.holds(AUTO_ACCEPT_REQUESTS);
    let boxes_back = the_boxes_match(&mut c, &shard);

    c.assert_behaviour(
        "options.character-page.the-excluded-row-goes-out-at-once-and-cancel-restores-both",
        move |_| {
            opened_quietly
                && at_the_default
                && both_told
                && rows_followed
                && shard_followed
                && boxes_followed
                && cancelled
                && back_at_the_default
                && shard_back
                && boxes_back
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// options.chat-page.*
//
// The page is five per-window filter controls carrying sixty-four tick boxes between them and two
// opacity sliders, and **nothing a player does on it goes on the wire**: a change writes the
// retained settings record, raises a notice the chat windows answer, and waits for the eight-minute
// flush that `options.character-page.a-deferred-change-...` is about.
// ---------------------------------------------------------------------------------------------

/// The five windows the page edits, in the order it builds them.
const CHAT_WINDOWS: [u32; 5] = [8, 2, 3, 4, 5];
/// The main window's first group -- the one the scenarios click.
const MASK_COMBAT: u64 = 0x0060_0040;
/// The filter property, the two opacity properties, and the per-window array they live in.
const PROP_FILTER: u32 = 0x1000_007F;
const PROP_IDLE_OPACITY: u32 = 0x1000_0080;
const PROP_ACTIVE_OPACITY: u32 = 0x1000_0081;
const PROP_WINDOW_ARRAY: u32 = 0x1000_008C;
/// The settings refresh writes this slider-position attribute.
const ATTR_POSITION: u32 = 0x86;

/// A client in the world whose settings record is `module`, as the shard's description gives it
/// one. Without a record there is nothing for a chat option to be written into, which is the
/// state a client is in before it logs in.
fn a_client_with_module(module: dereth_protocol::login::PlayerModule) -> HeadlessClient {
    use dereth_protocol::login::LoginPlayerDescription;

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.app_mut().apply_hud_events(&[
        dereth_client_net::client_session::SessionEvent::PlayerDescription(Box::new(
            LoginPlayerDescription {
                player_module: module,
                ..LoginPlayerDescription::default()
            },
        )),
    ]);
    c.tick(3);
    c
}

/// A settings record whose main chat window's filter is exactly `mask`.
fn a_module_whose_main_filter_is(mask: u64) -> dereth_protocol::login::PlayerModule {
    use dereth_protocol::property::{BaseProperty, PackObjPropertyCollection, PropertyCollection};

    let property = |name: u32, value: BasePropertyValue| {
        (
            name,
            BaseProperty {
                name,
                value: Some(value),
            },
        )
    };
    let mut rows = vec![
        BaseProperty {
            name: 0x1000_008B,
            value: Some(BasePropertyValue::Struct(PropertyCollection::default())),
        };
        8
    ];
    rows[7].value = Some(BasePropertyValue::Struct(PropertyCollection {
        bucket_index: 2,
        entries: vec![property(PROP_FILTER, BasePropertyValue::Bitfield64(mask))],
    }));
    dereth_protocol::login::PlayerModule {
        gameplay_options: Some(PackObjPropertyCollection {
            version: 2,
            properties: PropertyCollection {
                bucket_index: 3,
                entries: vec![property(PROP_WINDOW_ARRAY, BasePropertyValue::Array(rows))],
            },
        }),
        ..dereth_protocol::login::PlayerModule::default()
    }
}

/// Open the chat options page through the toolbar and its tab.
fn open_the_chat_options_page(c: &mut HeadlessClient, hands: &mut Hands) -> dereth_ui::ElemHandle {
    let page = with_gameplay(c, |_, s| {
        s.chat_options.page.expect("the chat options page is bound")
    });
    open_options_page(c, hands, page)
}

/// The index of one window's filter control.
fn filter_control(c: &mut HeadlessClient, window: u32) -> usize {
    with_gameplay(c, |_, s| {
        s.chat_options
            .filter_of(window)
            .expect("the page has a control for that window")
    })
}

/// The page's own word for one window's filter.
fn filter_current(c: &mut HeadlessClient, window: u32) -> u64 {
    let i = filter_control(c, window);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => f.current,
        ChatOption::Opacity(_) => unreachable!("that control is a filter"),
    })
}

/// Whether one group's tick box of one window's control is drawn ticked.
fn filter_box_drawn(c: &mut HeadlessClient, window: u32, group: usize) -> bool {
    let (h, _) = filter_box(c, window, group);
    drawn_tick(c, h)
}

/// The tick box of one group of one window's control, and the group's own mask.
fn filter_box(c: &mut HeadlessClient, window: u32, group: usize) -> (dereth_ui::ElemHandle, u64) {
    let i = filter_control(c, window);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => (
            f.children[group]
                .element
                .expect("the control built its boxes"),
            f.children[group].mask,
        ),
        ChatOption::Opacity(_) => unreachable!("that control is a filter"),
    })
}

/// The chat window's own filter -- what routing really reads, as opposed to the page's word.
fn window_filter(c: &mut HeadlessClient, window: u32) -> u64 {
    with_gameplay(c, |_, s| {
        s.chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window")
            .filter
    })
}

/// The idle and active opacity the chat window holds -- what the fade reads.
fn window_opacity(c: &mut HeadlessClient, window: u32) -> (f32, f32) {
    with_gameplay(c, |_, s| {
        let w = s
            .chat
            .iter()
            .find(|w| w.window_id == window)
            .expect("the chat window");
        (w.default_opacity, w.active_opacity)
    })
}

/// The slider control for one property, and what it holds.
fn slider_control(c: &mut HeadlessClient, property: u32) -> usize {
    with_gameplay(c, |_, s| {
        s.chat_options.slider_of(property).expect("the page has it")
    })
}

fn slider_element(c: &mut HeadlessClient, property: u32) -> dereth_ui::ElemHandle {
    let i = slider_control(c, property);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Opacity(o) => o.element,
        ChatOption::Filter(_) => unreachable!("that control is a slider"),
    })
}

fn slider_current(c: &mut HeadlessClient, property: u32) -> f32 {
    let i = slider_control(c, property);
    with_gameplay(c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Opacity(o) => o.current,
        ChatOption::Filter(_) => unreachable!("that control is a slider"),
    })
}

/// One window's filter as the retained settings record holds it.
fn module_filter(c: &HeadlessClient, window: u32) -> Option<u64> {
    let app = c.view().expect_app();
    let m = app.objects().world.player_system.module.as_ref()?;
    let BasePropertyValue::Array(rows) = m
        .gameplay_options
        .as_ref()?
        .properties
        .get(PROP_WINDOW_ARRAY)?
    else {
        return None;
    };
    let BasePropertyValue::Struct(fields) = rows
        .get((window as usize).checked_sub(1)?)?
        .value
        .as_ref()?
    else {
        return None;
    };
    match fields.get(PROP_FILTER)? {
        BasePropertyValue::Bitfield64(v) => Some(*v),
        _ => None,
    }
}

/// One of the two opacities as the retained record holds it.
fn module_opacity(c: &HeadlessClient, property: u32) -> Option<f32> {
    let app = c.view().expect_app();
    let m = app.objects().world.player_system.module.as_ref()?;
    match m.gameplay_options.as_ref()?.properties.get(property)? {
        BasePropertyValue::Float(v) => Some(*v),
        _ => None,
    }
}

/// Bring a control's row into the pane, as a player would have to before the pointer could reach
/// it.
fn scroll_chat_control_into_view(c: &mut HeadlessClient, i: usize) {
    with_gameplay(c, |ui, s| {
        let row = match &s.chat_options.options[i] {
            ChatOption::Filter(f) => f.element,
            ChatOption::Opacity(o) => o.row,
        };
        let list = s.chat_options.option_box.as_mut().expect("the option list");
        if let Some(idx) = list.items.iter().position(|h| *h == row) {
            list.scroll_to_view(ui, idx);
        }
    });
    c.tick(1);
}

/// One of the page's three buttons, scoped to this page -- all three option pages carry the same
/// three ids, so an unscoped lookup answers on the wrong one.
fn chat_page_button(c: &mut HeadlessClient, id: ElementId) -> dereth_ui::ElemHandle {
    with_gameplay(c, |ui, s| {
        let page = s.chat_options.page.expect("the page");
        ui.get_child_recursive(page, id).expect("the page's button")
    })
}

/// The tab comes up with every control the page declares.
pub(super) fn the_chat_options_tab_draws_its_controls() {
    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    let seen = c.outbound().len();
    open_the_chat_options_page(&mut c, &mut hands);

    let ordered_and_linked = with_gameplay(&mut c, |_, s| {
        let keys = s
            .chat_options
            .options
            .iter()
            .map(|option| match option {
                ChatOption::Opacity(o) => (o.property, 0),
                ChatOption::Filter(f) => (f.property, f.window_id),
            })
            .collect::<Vec<_>>();
        keys == [
            (0x1000_0080, 0),
            (0x1000_0081, 0),
            (0x1000_007F, 8),
            (0x1000_007F, 2),
            (0x1000_007F, 3),
            (0x1000_007F, 4),
            (0x1000_007F, 5),
        ] && s.chat_options.slider_links == [(0, 1)]
    });
    let (controls, boxes, headers, separators, rows, failures) = with_gameplay(&mut c, |_, s| {
        (
            s.chat_options.options.len(),
            s.chat_options.filter_child_count(),
            s.chat_options.headers,
            s.chat_options.separators,
            s.chat_options.row_count(),
            s.chat_options.failures,
        )
    });
    let (header_captions, slider_captions, child_captions) = with_gameplay(&mut c, |_, s| {
        (
            s.chat_options.header_captions,
            s.chat_options.slider_end_captions,
            s.chat_options.child_captions,
        )
    });
    // The chat font's face and size: two drop-downs in the page's box, just under the two
    // opacity sliders.
    let fonts_under_the_opacity = with_gameplay(&mut c, |_, s| {
        let Some(list) = s.chat_options.option_box.as_ref() else {
            return false;
        };
        let at = s.chat_options.after_opacity();
        let names: Vec<&str> = s
            .chat_font_rows
            .iter()
            .map(|&i| s.config_page.options[i].preference)
            .collect();
        let places: Vec<Option<usize>> = s
            .chat_font_rows
            .iter()
            .map(|&i| list.index_of(s.config_page.options[i].row))
            .collect();
        names == ["UI.ChatFontFace", "UI.ChatFontSize"]
            && at.is_some()
            && places == [at, at.map(|a| a + 1)]
    });

    // Every control opens at its window's own default, and every box is drawn from that default.
    let mut opens_at_the_defaults = true;
    for w in CHAT_WINDOWS {
        let want = dereth_ui_screens::chat::interface::default_filter(w);
        opens_at_the_defaults &= filter_current(&mut c, w) == want;
        let groups = dereth_ui_screens::chat::interface::filter_groups_for(w).len();
        for k in 0..groups {
            let (h, mask) = filter_box(&mut c, w, k);
            opens_at_the_defaults &= drawn_tick(&c, h) == (want & mask != 0);
        }
    }
    let sliders_open_at_their_defaults = (slider_current(&mut c, PROP_IDLE_OPACITY) - 0.5).abs()
        < 1e-6
        && (slider_current(&mut c, PROP_ACTIVE_OPACITY) - 1.0).abs() < 1e-6;
    let quiet = c.outbound().len() == seen;

    c.assert_behaviour(
        "options.chat-page.the-tab-comes-up-with-every-control-the-page-declares",
        move |_| {
            // Seven controls, sixty-four boxes, six headings and six rules between them; every
            // caption resolved out of the shipped text, and nothing failed to build.
            controls == 7
                && ordered_and_linked
                && boxes == 64
                && headers == 6
                && separators == 6
                && rows == headers + separators + controls + 2
                && failures == 0
                && header_captions == 6
                && slider_captions == 2
                && child_captions == 64
                && opens_at_the_defaults
                && sliders_open_at_their_defaults
                && quiet
        },
    );
    c.assert_behaviour(
        "options.chat-page.the-chat-fonts-face-and-size-sit-under-the-windows-opacity",
        move |_| fonts_under_the_opacity,
    );
    c.shutdown();
}

/// A partly chosen group is drawn differently from a wholly chosen one.
pub(super) fn a_partly_chosen_group_is_drawn_differently() {
    // One bit of the combat group, which is what makes it partly chosen.
    let mut c = a_client_with_module(a_module_whose_main_filter_is(0x40));
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);

    let (button, mask) = filter_box(&mut c, 8, 0);
    let control = with_gameplay(&mut c, |_, s| match &s.chat_options.options[i] {
        ChatOption::Filter(f) => f.element,
        ChatOption::Opacity(_) => unreachable!(),
    });
    let (some, all) = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        (
            dereth_ui_screens::bind::attr_data_id(ui, control, ATTR_IMAGE_SOME)
                .expect("the control carries its partly-chosen picture"),
            dereth_ui_screens::bind::attr_data_id(ui, control, ATTR_IMAGE_ALL)
                .expect("the control carries its wholly-chosen picture"),
        )
    };
    let two_pictures = some != all;

    // The picture has to reach the live region **and** the frame the client drew: recording which
    // one was wanted is not a drawn result.
    let box_media =
        |c: &mut HeadlessClient| -> (dereth_ui::ElemHandle, Option<dereth_primitives::DataId>) {
            let (b, _) = filter_box(c, 8, 0);
            let app = c.view().expect_app();
            let ui = &app.ui().expect("the UI shell is up").ui;
            let image = ui
                .children(b)
                .into_iter()
                .next()
                .expect("the box has a picture child");
            let did = ui
                .node(image)
                .and_then(|n| n.region.image.as_ref().map(|g| g.did));
            (image, did)
        };
    let submitted =
        |c: &HeadlessClient, image: dereth_ui::ElemHandle, did: dereth_primitives::DataId| {
            c.view()
                .expect_app()
                .ui_draw_list()
                .iter()
                .any(|cmd| cmd.who == image && cmd.image == Some(did))
        };

    let (image, drawn) = box_media(&mut c);
    let partly = drawn == Some(some) && submitted(&c, image, some);

    // Any bit makes the box ticked, so one press turns the partly chosen group off.
    hands.click_handle(&mut c, button);
    let (_, off) = box_media(&mut c);
    let turned_off = !filter_box_drawn(&mut c, 8, 0) && off != Some(some);

    // The next press chooses the whole group, and the other picture is what is drawn.
    let (button, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, button);
    let (image, drawn) = box_media(&mut c);
    let wholly =
        filter_current(&mut c, 8) & mask == mask && drawn == Some(all) && submitted(&c, image, all);

    c.assert_behaviour(
        "options.chat-page.a-partly-chosen-group-and-a-wholly-chosen-one-are-drawn-differently",
        move |_| mask == MASK_COMBAT && two_pictures && partly && turned_off && wholly,
    );
    c.shutdown();
}

/// Ticking a filter reaches the window that routes by it, and sends nothing.
pub(super) fn ticking_a_filter_reaches_the_window_and_sends_nothing() {
    use dereth_primitives::ServerTime;

    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    let (_, mask) = filter_box(&mut c, 8, 0);
    let before = window_filter(&mut c, 8);
    let starts_clean = mask == MASK_COMBAT
        && filter_box_drawn(&mut c, 8, 0)
        && !c
            .view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);

    let want = before & !MASK_COMBAT;
    let reached = !filter_box_drawn(&mut c, 8, 0)
        && filter_current(&mut c, 8) == want
        // The chat window's own filter is what routing reads; the page's word is not.
        && window_filter(&mut c, 8) == want
        && module_filter(&c, 8) == Some(want)
        && c.view().expect_app().objects().world.player_system.is_dirty();

    // The other four windows did not move: the notice names the window it is about.
    let others_untouched = [2, 3, 4, 5]
        .into_iter()
        .all(|w| window_filter(&mut c, w) == dereth_ui_screens::chat::interface::default_filter(w));

    // Nothing went out: a chat option raises a notice and a flag, and no message at all.
    let quiet = c.outbound().len() == seen;

    // ...and the record that does eventually go carries the new filter.
    let packed = {
        let ps = &mut c.objects_mut().world.player_system;
        assert!(
            ps.use_time(ServerTime(10_000.0)),
            "the deferred flush fires"
        );
        ps.client_packed_module().expect("the packed record")
    };
    let carried = matches!(
        packed
            .gameplay_options
            .as_ref()
            .and_then(|g| g.properties.get(PROP_WINDOW_ARRAY)),
        Some(BasePropertyValue::Array(rows))
            if matches!(
                rows[7].value.as_ref(),
                Some(BasePropertyValue::Struct(fields))
                    if fields.get(PROP_FILTER) == Some(&BasePropertyValue::Bitfield64(want))
            )
    );

    // Turning it back on restores the group's bits rather than flipping the whole word.
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let restored = filter_box_drawn(&mut c, 8, 0) && window_filter(&mut c, 8) == before;

    c.assert_behaviour(
        "options.chat-page.ticking-a-filter-reaches-the-window-that-routes-by-it-and-sends-nothing",
        move |_| starts_clean && reached && others_untouched && quiet && carried && restored,
    );
    c.shutdown();
}

/// The opacity slider writes what the fade reads, and the two stay in order.
pub(super) fn the_opacity_slider_writes_what_the_fade_reads() {
    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    // Both windows start at the shipped fallback, which is what makes the fade's travel nothing.
    let starts_flat =
        window_opacity(&mut c, 8) == (1.0, 1.0) && module_opacity(&c, PROP_IDLE_OPACITY).is_none();

    let i = slider_control(&mut c, PROP_IDLE_OPACITY);
    scroll_chat_control_into_view(&mut c, i);
    let bar = slider_element(&mut c, PROP_IDLE_OPACITY);
    // A press a quarter of the way along the bar: the widget puts the thumb where the pointer is.
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app.ui().expect("the UI shell is up").ui.screen_box(bar);
        (b.x0 + (b.x1 - b.x0) / 4, (b.y0 + b.y1) / 2)
    };
    hands.click_at(&mut c, x, y);

    let pos = {
        let app = c.view().expect_app();
        dereth_ui_screens::bind::attr_float(&app.ui().expect("shell").ui, bar, ATTR_POSITION)
            .unwrap_or(-1.0)
    };
    let v = slider_current(&mut c, PROP_IDLE_OPACITY);
    let moved = (0.0..=1.0).contains(&pos) && v < 0.5 && (v - pos).abs() < 1e-5;

    let stored = module_opacity(&c, PROP_IDLE_OPACITY).expect("the record took the new value");
    let written = (stored - v).abs() < 1e-6
        && c.view()
            .expect_app()
            .objects()
            .world
            .player_system
            .is_dirty();

    // ...and the fade's own source, on every window, because this setting is not per-window.
    let every_window = CHAT_WINDOWS.into_iter().all(|w| {
        let (idle, active) = window_opacity(&mut c, w);
        (idle - v).abs() < 1e-6 && (active - 1.0).abs() < 1e-6
    });
    let (idle, active) = window_opacity(&mut c, 8);
    let has_travel = active - idle > 0.1;
    let quiet = c.outbound().len() == seen;

    // The page keeps the two in order: dragging the active one below the idle one takes the idle
    // one down with it, and writes its setting too.
    let j = slider_control(&mut c, PROP_ACTIVE_OPACITY);
    scroll_chat_control_into_view(&mut c, j);
    let bar = slider_element(&mut c, PROP_ACTIVE_OPACITY);
    let (x, y) = {
        let app = c.view().expect_app();
        let b = app.ui().expect("the UI shell is up").ui.screen_box(bar);
        (b.x0 + 1, (b.y0 + b.y1) / 2)
    };
    hands.click_at(&mut c, x, y);
    let a = slider_current(&mut c, PROP_ACTIVE_OPACITY);
    let d = slider_current(&mut c, PROP_IDLE_OPACITY);
    let pulled_down = a <= v
        && (d - a).abs() < 1e-6
        && module_opacity(&c, PROP_IDLE_OPACITY).is_some_and(|s| (s - a).abs() < 1e-6);

    c.assert_behaviour("options.chat-page.the-opacity-slider-writes-what-the-fade-reads-and-keeps-the-two-in-order", move |_| {
        starts_flat && moved && written && every_window && has_travel && quiet && pulled_down
    });
    c.shutdown();
}

/// Apply, cancel and restore-defaults do what an option page does.
pub(super) fn apply_cancel_and_defaults_do_what_an_option_page_does() {
    use dereth_ui_screens::options::config::button;

    let mut c = a_client_with_module(dereth_protocol::login::PlayerModule::default());
    let mut hands = Hands::new();
    open_the_chat_options_page(&mut c, &mut hands);
    let seen = c.outbound().len();

    let snapshot = window_filter(&mut c, 8);
    let i = filter_control(&mut c, 8);
    scroll_chat_control_into_view(&mut c, i);

    // Cancel reverts an uncommitted change...
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let moved = window_filter(&mut c, 8) != snapshot
        && with_gameplay(&mut c, |_, s| s.chat_options.changed());
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let reverted = window_filter(&mut c, 8) == snapshot
        && module_filter(&c, 8) == Some(snapshot)
        && filter_box_drawn(&mut c, 8, 0)
        && !with_gameplay(&mut c, |_, s| s.chat_options.changed());

    // ...and a second cancel writes nothing, which is why leaving the page does not rewrite
    // every control on it.
    let before = c
        .view()
        .expect_app()
        .hud()
        .stats
        .chat_option_controls_reread;
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let nothing_to_revert = c
        .view()
        .expect_app()
        .hud()
        .stats
        .chat_option_controls_reread
        == before;

    // Apply takes a new snapshot, so the next cancel has nothing older to go back to.
    let (h, _) = filter_box(&mut c, 8, 0);
    hands.click_handle(&mut c, h);
    let after_click = window_filter(&mut c, 8);
    let apply = chat_page_button(&mut c, button::APPLY);
    hands.click_handle(&mut c, apply);
    let applied = window_filter(&mut c, 8) == after_click
        && !with_gameplay(&mut c, |_, s| s.chat_options.changed());
    let cancel = chat_page_button(&mut c, button::CANCEL);
    hands.click_handle(&mut c, cancel);
    let cancel_after_apply = window_filter(&mut c, 8) == after_click;

    // Restore-defaults is unconditional: a control nobody touched goes back too.
    let j = filter_control(&mut c, 2);
    scroll_chat_control_into_view(&mut c, j);
    let (h, _) = filter_box(&mut c, 2, 0);
    hands.click_handle(&mut c, h);
    let defaults = chat_page_button(&mut c, button::DEFAULTS);
    hands.click_handle(&mut c, defaults);
    let all_back = CHAT_WINDOWS.into_iter().all(|w| {
        let want = dereth_ui_screens::chat::interface::default_filter(w);
        filter_current(&mut c, w) == want
            && window_filter(&mut c, w) == want
            && module_filter(&c, w) == Some(want)
    }) && (slider_current(&mut c, PROP_IDLE_OPACITY) - 0.5).abs() < 1e-6
        && (slider_current(&mut c, PROP_ACTIVE_OPACITY) - 1.0).abs() < 1e-6
        && module_opacity(&c, PROP_IDLE_OPACITY) == Some(0.5)
        && module_opacity(&c, PROP_ACTIVE_OPACITY) == Some(1.0);

    // Nothing in the whole scenario put a byte on the wire.
    let quiet = c.outbound().len() == seen;

    c.assert_behaviour(
        "options.chat-page.apply-cancel-and-defaults-do-what-an-option-page-does",
        move |_| {
            moved
                && reverted
                && nothing_to_revert
                && applied
                && cancel_after_apply
                && all_back
                && quiet
        },
    );
    c.shutdown();
}
