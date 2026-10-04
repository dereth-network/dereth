use super::*;

// =============================================================================================
// The use key on something that asks first.
//
// The failure these guard against: using an altar appears to do nothing. The whole chain is the
// claim and each link of it is the shipped one: the key the shipped keymap binds to *use*, the
// question the client raises for itself, the question's own buttons under a real pointer, and what
// does and does not leave for the shard.
//
// The thing being used is a **recorded** one, arriving as the shard sent it. What it carries is
// object description and nothing else -- a name, a kind, a radius and the switch that makes it
// ask -- and the recording never carried a use for it, because the client asks locally first and
// sends only after yes.
// =============================================================================================

/// The player of this scenario, and the recorded altar.
const ALTAR_PLAYER: ObjectId = ObjectId(0x5000_0001);
const ALTAR: ObjectId = ObjectId(0x701F_9020);

/// The question's own two buttons.
const ANSWER_YES: ElementId = ElementId(0x17);
const ANSWER_NO: ElementId = ElementId(0x19);

/// The recorded description of the altar, as the shard sent it.
const RECORDED_ALTAR: &str = concat!(
    "45f7000020901f701100000003880100100401000c00000000003d00030000003d00030000000000",
    "df01f9016abc4442000070c23cdf3f41f70435bf0000000000000000f70435bf2e00000934000020",
    "590300020000000000000000000000000000000000000000300020001400416c746172206f662042",
    "61656c275a6861726f6e000056034f138000000014040000200000000000a04032000000",
);

fn bytes_of(hex: &str) -> Vec<u8> {
    assert_eq!(hex.len() % 2, 0);
    hex.as_bytes()
        .chunks_exact(2)
        .map(|p| {
            let digit = |b: u8| match b {
                b'0'..=b'9' => b - b'0',
                b'a'..=b'f' => b - b'a' + 10,
                _ => panic!("a hexadecimal digit"),
            };
            (digit(p[0]) << 4) | digit(p[1])
        })
        .collect()
}

/// A client in the world with the recorded altar beside it and the altar picked.
///
/// The premises are asserted here rather than in each scenario: the thing really arrived, it is
/// the thing the recording described, and it is the one that would be used.
fn a_client_standing_at_the_altar() -> (HeadlessClient, dereth_testkit::Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    let mut peer = dereth_testkit::Peer::attach_creating(&mut c, ALTAR_PLAYER);
    c.app_mut().probe_mut().objects_mut().world.player = Some(ALTAR_PLAYER);
    peer.send(&mut c, 10, bytes_of(RECORDED_ALTAR));
    c.tick(3);

    {
        let w = c.view().world();
        let altar = w.weenie(ALTAR).expect("the recorded description arrives");
        assert_eq!(
            altar.pwd.name, "Altar of Bael'Zharon",
            "the premise: it is what was recorded"
        );
        assert_eq!(
            altar.pwd.useability,
            Some(0x20),
            "and it is a thing that can be used"
        );
    }
    c.app_mut().probe_mut().objects_mut().world.selected = Some(ALTAR);
    c.tick(1);
    (c, peer)
}

/// The question the client has put up, if it has put one up.
fn the_question(c: &mut HeadlessClient) -> Option<ElemHandle> {
    c.view()
        .expect_app()
        .ui()
        .expect("the UI shell")
        .ui
        .dialogs
        .open_on(dereth_ui::dialog::factory::DEFAULT_QUEUE)?
        .element
}

/// What the question says.
fn the_question_says(c: &mut HeadlessClient) -> String {
    let root = the_question(c).expect("the client has asked");
    let shell = c.app_mut().ui_mut().expect("the UI shell");
    let h = shell
        .ui
        .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
        .expect("a question binds its own text");
    shell
        .ui
        .text_element_mut(h)
        .map(|t| t.glyphs.inq_text(false))
        .unwrap_or_default()
}

/// Answer the question with a real pointer on its own button.
///
/// The point is taken from the button's own box rather than from its id, because a question is
/// put up outside the screen's own roots and naming an element there finds nothing.
fn answer_the_question(c: &mut HeadlessClient, yes: bool) {
    let root = the_question(c).expect("the client has asked");
    let at = {
        let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
        let button = ui
            .get_child_recursive(root, if yes { ANSWER_YES } else { ANSWER_NO })
            .expect("the question's own button");
        let r = ui.screen_clip_box(button);
        assert!(
            r.is_valid(),
            "the button is on screen, or nothing could be clicked"
        );
        ScreenPoint::new((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(4);
    clear_requests(c.ui_outbox());
}

/// Press the key the **shipped keymap** binds to *use*.
pub(super) fn press_the_use_key(c: &mut HeadlessClient) {
    let key = dereth_client::platform::window::key_from_key_code(winit::keyboard::KeyCode::KeyR)
        .expect("the host names this key");
    dereth_testkit::input_steps::press_use(c, key);
}

/// Every use of the altar this client has asked the shard for.
fn uses_of_the_altar(c: &HeadlessClient) -> usize {
    c.outbound()
        .iter()
        .filter(|r| matches!(r, Request::UseEvent(m) if m.object == ALTAR))
        .count()
}

// ---------------------------------------------------------------------------------------------
// use.confirmation.the-shipped-use-key-raises-the-question-and-the-two-answers-differ
// ---------------------------------------------------------------------------------------------

/// Pressing the shipped use key on something that asks first raises the question and sends
/// nothing; **yes** then uses it once and leaves the player busy until the shard says the use is
/// done; **no** closes the question and sends nothing at all.
///
/// Both answers are one scenario because they are the two answers to one question, and the failure
/// this guards against -- pressing the key and watching nothing happen -- is indistinguishable from
/// either of them measured alone.
pub(super) fn the_shipped_use_key_raises_the_question_and_the_two_answers_differ() {
    // Yes.
    let (mut c, mut peer) = a_client_standing_at_the_altar();
    let nothing_asked_yet = the_question(&mut c).is_none();
    press_the_use_key(&mut c);
    let the_question_is_up = the_question(&mut c).is_some();
    let in_its_own_words = the_question_says(&mut c)
        == dereth_client_model::inventory::use_object::UsageConfirmation::PkAltar.prompt();
    let nothing_sent_while_thinking = uses_of_the_altar(&c) == 0;

    // What the client acts on is what it put the question up about, and not whatever happens to
    // be picked when the answer comes: the selection is taken away before yes is pressed.
    c.app_mut().probe_mut().objects_mut().world.selected = None;
    answer_the_question(&mut c, true);
    let used_once = uses_of_the_altar(&c) == 1;
    let it_really_left = c.take_wire_count(0x0036) == 1;
    let busy = c.view().world().magic.busy_count == 1;
    let the_question_is_gone = the_question(&mut c).is_none();

    peer.event(
        &mut c,
        &dereth_protocol::objects::ItemUseDone { failure_type: 0 },
    );
    c.tick(6);
    let the_shard_releases_it = c.view().world().magic.busy_count == 0;
    c.shutdown();

    // No.
    let (mut c, _peer) = a_client_standing_at_the_altar();
    press_the_use_key(&mut c);
    let asked_again = the_question(&mut c).is_some();
    answer_the_question(&mut c, false);
    let nothing_sent = uses_of_the_altar(&c) == 0;
    let not_busy = c.view().world().magic.busy_count == 0;
    let closed = the_question(&mut c).is_none();

    c.assert_behaviour(
        "use.confirmation.the-shipped-use-key-raises-the-question-and-the-two-answers-differ",
        move |_| {
            nothing_asked_yet
                && the_question_is_up
                && in_its_own_words
                && nothing_sent_while_thinking
                && used_once
                && it_really_left
                && busy
                && the_question_is_gone
                && the_shard_releases_it
                && asked_again
                && nothing_sent
                && not_busy
                && closed
        },
    );
    c.shutdown();
}

/// The thing in the recorded corpse this pick-up is about, and a compatible stack to merge into.
const CORPSE_THING: ObjectId = ObjectId(0x8000_0A99);
const CARRIED_STACK: ObjectId = ObjectId(0x8000_F185);
const CORPSE_STACK_SIZE: u16 = 15;
/// The last of the messages that create the corpse's contents.
const CORPSE_FILLED_BY: usize = 6594;

/// What the shipped keymap calls "pick up what is picked", and the map it lives on.
const PICK_UP: dereth_input::ActionId = dereth_input::ActionId(0x1000_002C);
const SELECTION_COMMANDS: dereth_input::InputMapId = dereth_input::InputMapId(0x1000_0007);

/// Press the key the **shipped keymap** binds to picking up what is picked.
fn press_the_pick_up_key(c: &mut HeadlessClient) {
    let key = dereth_client::platform::window::key_from_key_code(winit::keyboard::KeyCode::KeyF)
        .expect("the host names this key");
    dereth_testkit::input_steps::press_bound(c, PICK_UP, SELECTION_COMMANDS, key);
}

/// A client that has replayed the recording as far as the corpse's contents, with the recorded
/// thing picked or nothing picked.
fn a_client_at_the_recorded_corpse(pick: Option<ObjectId>) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..CORPSE_FILLED_BY + 1,
    ))
    .tick(1);
    assert!(
        c.view().world().weenie(CORPSE_THING).is_some(),
        "the premise: the recording creates the thing this pick-up is about"
    );
    // What the client acts on is what is picked, so the picking is made through the client's own
    // entry point rather than assumed from the recording.
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            pick,
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
    c.tick(1);
    assert_eq!(
        c.view().world().selected,
        pick,
        "the premise: that is what is picked"
    );
    c
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack
// ---------------------------------------------------------------------------------------------

/// Pressing the key the shipped keymap binds to picking things up puts what is picked into the
/// player's own pack.
///
/// The key had resolved correctly all along and produced its action; what was missing was
/// anything that answered it, so the action was swept away and nothing went out for any object,
/// ever. The message it must produce is one the recording itself carries for the same thing.
pub(super) fn the_shipped_key_puts_what_is_picked_into_the_players_own_pack() {
    let mut c = a_client_at_the_recorded_corpse(Some(CORPSE_THING));
    let player = c
        .view()
        .world()
        .player
        .expect("the recording names its own player");
    let before = c.view().interaction().stats.pick_ups;

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();

    let the_action_was_answered = c.view().interaction().stats.pick_ups == before + 1;
    let it_went_into_the_pack = sent.iter().any(|r| {
        matches!(r, Request::PutItemInContainer(m)
            if m.item == CORPSE_THING && m.container == player)
    });

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-puts-what-is-picked-into-the-players-own-pack",
        move |_| the_action_was_answered && it_went_into_the_pack,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge
// ---------------------------------------------------------------------------------------------

/// When what is picked is a stack and the player is already carrying a compatible one, the same
/// key merges them, and it carries the count the picking itself worked out rather than a fresh
/// empty one.
///
/// The count is seeded by the picking, before the key is ever pressed, which is asserted as the
/// premise: a client that started the merge from nothing would send a zero and the player would
/// watch the stack not move.
pub(super) fn the_shipped_key_carries_the_picked_stacks_own_count_into_the_merge() {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.when(Inbound::from_corpus(
        "long-solo-play",
        0..CORPSE_FILLED_BY + 1,
    ))
    .tick(1);
    let player = c
        .view()
        .world()
        .player
        .expect("the recording names its own player");

    // The recording's own corpse thing, with its count and a compatible carried stack made
    // explicit: that is the one branch whose amount is the thing this scenario is about, and a
    // plain corpse thing would only exercise the plain move again.
    {
        let world = &mut c.app_mut().probe_mut().objects_mut().world;
        let source = world
            .weenie_mut(CORPSE_THING)
            .expect("the recorded corpse thing");
        source.pwd.wcid = 0xF185;
        source.pwd.stack_size = Some(CORPSE_STACK_SIZE);
        source.pwd.max_stack_size = Some(100);

        let mut target = dereth_client_model::Weenie::new(CARRIED_STACK);
        target.pwd = PublicWeenieDesc {
            name: "a compatible carried stack".into(),
            wcid: 0xF185,
            stack_size: Some(20),
            max_stack_size: Some(100),
            container_id: Some(player),
            ..PublicWeenieDesc::default()
        };
        target.valid = true;
        world.tables.weenies.insert(CARRIED_STACK, target);
        world.view_object_contents(
            player,
            &[dereth_protocol::types::ContentProfile {
                iid: CARRIED_STACK,
                container_properties: 0,
            }],
            &mut dereth_client_model::NullSink,
        );
    }

    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .set_selected_object(
            Some(CORPSE_THING),
            false,
            &mut dereth_client_model::RecordingSink::default(),
        );
    c.tick(1);
    let the_count_was_seeded_by_the_picking = c.view().world().split
        == dereth_client_model::inventory::SplitState::whole_stack(u32::from(CORPSE_STACK_SIZE));

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();
    let merged = matches!(
        sent.as_slice(),
        [Request::StackableMerge(m)]
            if m.merge_from == CORPSE_THING
                && m.merge_to == CARRIED_STACK
                && m.amount == i32::from(CORPSE_STACK_SIZE)
    );

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-carries-the-picked-stacks-own-count-into-the-merge",
        move |_| the_count_was_seeded_by_the_picking && merged,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing
// ---------------------------------------------------------------------------------------------

/// With nothing picked, the same key is **not** answered at all and nothing goes out.
///
/// Without this a client that picked up whatever it found, including nothing at all, would pass
/// the two scenarios above.
pub(super) fn the_shipped_key_with_nothing_picked_sends_nothing() {
    let mut c = a_client_at_the_recorded_corpse(None);
    let before = c.view().interaction().stats.pick_ups;

    let mark = c.outbound().len();
    press_the_pick_up_key(&mut c);
    c.tick(2);
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();

    let not_answered = c.view().interaction().stats.pick_ups == before;
    let nothing_went_out = !sent
        .iter()
        .any(|r| matches!(r, Request::PutItemInContainer(_)));

    c.assert_behaviour(
        "inventory.pickup.the-shipped-key-with-nothing-picked-sends-nothing",
        move |_| not_answered && nothing_went_out,
    );
    c.shutdown();
}
