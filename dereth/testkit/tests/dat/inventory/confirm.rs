use dereth_client_model::Request;
use dereth_primitives::ObjectId;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Player, ScreenPoint, Target};
use dereth_ui::{ElemHandle, ElementId};

const PLAYER: ObjectId = ObjectId(0x5000_000A);
/// The thing used, and the thing it is used on.
const SOURCE: ObjectId = ObjectId(0x8000_0A6F);
const TARGET: ObjectId = ObjectId(0x8000_09A4);
/// The second pair the recording carries.
const SECOND_SOURCE: ObjectId = ObjectId(0x8000_0A73);
const SECOND_TARGET: ObjectId = ObjectId(0x8000_099D);
/// Where the recording is replayed to: the moment its own player made the first such use.
const FIRST_USE_AT: usize = 4315;
/// The blob the shard's own release of the busy flag is.
const RELEASE_AT: usize = 4324;

/// The queue a confirmation goes on.
const CONFIRMATIONS: u64 = 2;
/// The two buttons, and the line of text between them.
const YES: ElementId = ElementId(0x17);
const NO: ElementId = ElementId(0x19);

/// What makes a thing a stone that drains: it is one, and it holds nothing yet.
const A_DRAINING_STONE: u32 = 0x0008_0000;
/// What makes a thing a lump of salvage.
const SALVAGE: u32 = 0x4000_0000;
/// The setting that skips the box.
const SKIP_THE_BOX: usize = 26;
/// The bit that says a thing is kept and cannot be destroyed.
const RETAINED: u32 = 0x0100_0000;

/// The client with the recording replayed into it, the pack open, and the tutorial prompts
/// dismissed.
fn a_client_in_the_academy() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(PLAYER);
    c.when(Inbound::corpus_until("long-solo-play", FIRST_USE_AT));
    super::open_pack(&mut c);
    c.tick(3);
    dismiss_the_tutorial(&mut c);
    c.tick(1);
    c
}

/// **The player this replay does not have.** The recording carries the Academy's tutorial
/// prompts, and a real player dismisses each one as it appears; replayed in a burst with
/// nobody at the keyboard they stack over the middle of the screen, where every assertion
/// below about "no box is open" would see them. Dismissing them is that missing player and
/// not a suppression: that they were there at all is asserted first, so a client that stopped
/// raising them reddens this rather than passing more easily.
fn dismiss_the_tutorial(c: &mut HeadlessClient) {
    let contexts: Vec<u64> = {
        let ui = &c.app_mut().ui().expect("the shell").ui;
        ui.dialogs.non_queued().iter().map(|i| i.context).collect()
    };
    assert!(
        !contexts.is_empty(),
        "the recording's own tutorial prompts should be on screen by now; an empty list means \
             nothing is receiving them any more, which is another subject's claim but is worth \
             knowing here"
    );
    let ui = &mut c.app_mut().ui_mut().expect("the shell").ui;
    for context in contexts {
        if let Some(root) = ui.dialogs.close_dialog(context, 0.0) {
            ui.remove_and_delete_root(root);
        }
    }
    assert!(
        ui.dialogs.non_queued().is_empty(),
        "every prompt is dismissed"
    );
}

/// The tile a thing is in.
fn slot(c: &mut HeadlessClient, id: ObjectId) -> ElemHandle {
    let handles: Vec<ElemHandle> = c
        .app_mut()
        .ui()
        .expect("the shell")
        .ui
        .element_list()
        .to_vec();
    let (_ui, screen) = super::gameplay_screen(c.app_mut());
    handles
        .into_iter()
        .find(|h| {
            screen
                .inventory
                .locate(*h)
                .is_some_and(|(_, _, item)| item == Some(id))
        })
        .unwrap_or_else(|| panic!("{id:?} is in no live inventory tile"))
}

fn click_tile(c: &mut HeadlessClient, id: ObjectId) {
    let h = slot(c, id);
    let target = super::point_of(c, h);
    c.when(Player::Click(target)).tick(1);
}

fn use_button() -> Target {
    Target::Element(dereth_ui_screens::toolbar::target_mode::USE_BUTTON)
}

/// Pick a thing, press Use, and click what it is to be used on -- the whole two-click
/// gesture, through the pointer.
fn use_one_on_the_other(c: &mut HeadlessClient, source: ObjectId, target: ObjectId) {
    click_tile(c, source);
    c.when(Player::Click(use_button())).tick(1);
    // What is picked is deliberately moved between the two clicks: the gesture must follow
    // what it armed and not what is picked.
    c.world_mut().selected = Some(PLAYER);
    click_tile(c, target);
}

/// The box that is up, if one is.
fn open_box(c: &mut HeadlessClient) -> Option<(u64, ElemHandle)> {
    let ui = &c.app_mut().ui().expect("the shell").ui;
    ui.dialogs
        .open_on(CONFIRMATIONS)
        .and_then(|i| i.element.map(|e| (i.context, e)))
}

/// Whether any box at all is on screen.
fn any_box(c: &mut HeadlessClient) -> bool {
    c.app_mut()
        .ui()
        .expect("the shell")
        .ui
        .dialogs
        .is_dialog_open(0)
}

/// The text the box is showing.
fn prompt(c: &mut HeadlessClient) -> String {
    let (_ctx, root) = open_box(c).expect("a box is up");
    let ui = &mut c.app_mut().ui_mut().expect("the shell").ui;
    let text = ui
        .get_child_recursive(root, dereth_ui::dialog::base::child::TEXT)
        .expect("the box has its line of text");
    ui.text_element_mut(text)
        .expect("it is text")
        .glyphs
        .inq_text(false)
}

/// Press one of the box's two buttons, through the pointer, at the button's own centre.
///
/// The point is computed rather than the button named, because a box's tree is not under the
/// screen's own roots and the element-naming step resolves only what is.
fn answer(c: &mut HeadlessClient, yes: bool) {
    let (_ctx, root) = open_box(c).expect("a box is up to be answered");
    let at = {
        let ui = &c.app_mut().ui().expect("the shell").ui;
        let h = ui
            .get_child_recursive(root, if yes { YES } else { NO })
            .expect("the box has both buttons");
        let b = ui.screen_clip_box(h);
        assert!(
            b.is_valid(),
            "the button is drawn where the player could press it"
        );
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at))).tick(1);
}

/// Make the thing a stone that drains, which is the branch that asks first.
fn make_it_a_draining_stone(c: &mut HeadlessClient, id: ObjectId) {
    let w = c
        .world_mut()
        .weenie_mut(id)
        .expect("the recording described it");
    w.pwd.obj_type = A_DRAINING_STONE;
    w.pwd.effects = Some(0);
}

/// The newest bubble in the strip over the world, and the assertion that it is drawn.
fn newest_bubble(c: &mut HeadlessClient) -> Option<String> {
    let h = *c.view().hud().panels.spew.list.as_ref()?.items.first()?;
    let ui = &mut c.app_mut().ui_mut().expect("the shell").ui;
    assert!(
        ui.screen_clip_box(h).is_valid(),
        "the bubble is drawn where it can be read"
    );
    Some(ui.text_element_mut(h)?.glyphs.inq_text(false))
}

/// Every targeted use the client has asked for.
fn targeted(c: &HeadlessClient) -> Vec<(ObjectId, ObjectId)> {
    c.outbound()
        .iter()
        .filter_map(|r| match r {
            Request::UseWithTargetEvent(m) => Some((m.object, m.target)),
            _ => None,
        })
        .collect()
}

// -----------------------------------------------------------------------------------------

/// **A Yes uses the pair the box was made with.** The box names the thing it is about, in the
/// words the player reads; the Yes sends the recorded client's own request; and the box is
/// gone afterwards, leaving nothing on screen.
pub fn a_yes_uses_the_pair_the_box_was_made_with() {
    use dereth_protocol::Message;

    let mut c = a_client_in_the_academy();
    make_it_a_draining_stone(&mut c, SOURCE);
    use_one_on_the_other(&mut c, SOURCE, TARGET);

    let (context, root) = open_box(&mut c).expect("the use asks first");
    let is_a_confirmation = {
        let ui = &c.app_mut().ui().expect("the shell").ui;
        ui.dialogs.open_on(CONFIRMATIONS).map(|i| i.kind)
            == Some(dereth_ui::dialog::DialogKind::Confirmation)
            && dereth_ui::dialog::types::dialog_element(ui, root).map(|d| d.context)
                == Some(context)
    };
    let target_name = c
        .view()
        .world()
        .weenie(TARGET)
        .expect("the recording described it")
        .pwd
        .name
        .clone();
    let says = prompt(&mut c)
        == format!(
            "\nAre you sure you want to attempt to destroy your {target_name} and drain its \
                 mana into this stone?"
        );

    // What is picked is moved again before the Yes: the box carries its own pair.
    c.world_mut().selected = Some(TARGET);
    answer(&mut c, true);

    let used = targeted(&c) == [(SOURCE, TARGET)];
    let is_the_recorded_request = {
        let Some(Request::UseWithTargetEvent(m)) = c
            .outbound()
            .iter()
            .rev()
            .find(|r| matches!(r, Request::UseWithTargetEvent(_)))
        else {
            panic!("the Yes sent one")
        };
        let mut bytes = dereth_protocol::items::InventoryUseWithTargetEvent::OPCODE
            .0
            .to_le_bytes()
            .to_vec();
        bytes.extend(dereth_protocol::write_body(m).expect("it encodes"));
        dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
            .expect("the recording parses")
            .expect("the corpus carries it")
            .blobs
            .iter()
            .find(|b| b.idx == FIRST_USE_AT)
            .is_some_and(|b| bytes == b.payload[8..])
    };
    let busy = c.view().world().magic.busy_count == 1;
    let gone = c.app_mut().ui().expect("the shell").ui.node(root).is_none() && !any_box(&mut c);

    c.assert_behaviour(
        "use.confirmation.a-yes-uses-the-pair-the-box-was-made-with",
        move |_| is_a_confirmation && says && used && is_the_recorded_request && busy && gone,
    );
    c.shutdown();
}

/// **A second box waits behind the first**, and answering the first with a No brings the
/// second up -- with its own pair, which the Yes then uses.
pub fn a_second_box_waits_behind_the_first_and_keeps_its_own_pair() {
    let mut c = a_client_in_the_academy();
    make_it_a_draining_stone(&mut c, SOURCE);
    make_it_a_draining_stone(&mut c, SECOND_SOURCE);

    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let first = open_box(&mut c).expect("the first box").0;
    c.tick(16);

    use_one_on_the_other(&mut c, SECOND_SOURCE, SECOND_TARGET);
    let waiting = open_box(&mut c).map(|(ctx, _)| ctx) == Some(first)
        && c.app_mut()
            .ui()
            .expect("the shell")
            .ui
            .dialogs
            .waiting_on(CONFIRMATIONS)
            == 1;

    answer(&mut c, false);
    let nothing_sent = targeted(&c).is_empty();
    let second_is_up = open_box(&mut c).map(|(ctx, _)| ctx) != Some(first)
        && c.app_mut()
            .ui()
            .expect("the shell")
            .ui
            .dialogs
            .waiting_on(CONFIRMATIONS)
            == 0;

    c.world_mut().selected = Some(SOURCE);
    answer(&mut c, true);
    let its_own_pair = targeted(&c) == [(SECOND_SOURCE, SECOND_TARGET)] && !any_box(&mut c);

    c.assert_behaviour(
        "use.confirmation.a-second-one-waits-behind-the-first-and-keeps-its-own-pair",
        move |_| waiting && nothing_sent && second_is_up && its_own_pair,
    );
    c.shutdown();
}

/// **The salvage box names what is being applied and what it may destroy**; a No cancels it
/// and asks the shard nothing; and a change of the skip setting made after the box is up does
/// not rewrite the answer it was created with. With the setting on from the start there is no
/// box at all and the use goes straight out.
pub fn the_salvage_box_says_what_it_may_destroy_and_a_no_cancels_it() {
    let mut c = a_client_in_the_academy();
    {
        let w = c
            .world_mut()
            .weenie_mut(SOURCE)
            .expect("the recording described it");
        w.pwd.obj_type = SALVAGE;
        w.pwd.name = "Salvaged Copper (100)".into();
    }
    c.world_mut().player_system.options.set(SKIP_THE_BOX, false);

    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let (_ctx, root) = open_box(&mut c).expect("the use asks first");
    let target_name = c
        .view()
        .world()
        .weenie(TARGET)
        .expect("the recording described it")
        .pwd
        .name
        .clone();
    let says = prompt(&mut c)
        == format!(
            "\nAre you sure you want to apply the Salvaged Copper to the {target_name}? The \
                 {target_name} may be destroyed."
        );

    answer(&mut c, false);
    let cancelled = c.app_mut().ui().expect("the shell").ui.node(root).is_none()
        && targeted(&c).is_empty()
        && c.view().world().magic.busy_count == 0;

    // The same use again; the setting is turned on **after** the box is up, which must not
    // rewrite what it was made to do.
    c.tick(16);
    use_one_on_the_other(&mut c, SOURCE, TARGET);
    c.world_mut().selected = Some(TARGET);
    c.world_mut().player_system.options.set(SKIP_THE_BOX, true);
    answer(&mut c, true);
    let still_its_own =
        targeted(&c) == [(SOURCE, TARGET)] && c.view().world().magic.busy_count == 1;

    // The shard's own release, and then the same gesture with the setting on from the start:
    // no box at all.
    c.when(Inbound::blobs("long-solo-play", &[RELEASE_AT]))
        .tick(16);
    let released = c.view().world().magic.busy_count == 0;
    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let skipped = targeted(&c) == [(SOURCE, TARGET), (SOURCE, TARGET)] && !any_box(&mut c);

    c.assert_behaviour(
        "use.confirmation.the-salvage-box-says-what-it-may-destroy-and-a-no-cancels-it",
        move |_| says && cancelled && still_its_own && released && skipped,
    );
    c.shutdown();
}

/// A box nobody answers dies with the screen it was raised over, taking what it was going to
/// do with it -- and the same use made again afterwards works, on the rebuilt screen's own
/// new tiles rather than on remembered old ones.
pub fn one_nobody_answers_dies_with_the_screen_and_the_next_use_still_works() {
    let mut c = a_client_in_the_academy();
    make_it_a_draining_stone(&mut c, SOURCE);
    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let (old_context, root) = open_box(&mut c).expect("the use asks first");
    let switches = c.app_mut().ui().expect("the shell").flow.switches;

    // The screen is thrown away and built again, which is what a mode change does.
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    // Two frames: the rebuilt screen's allegiance panel asks on the first and the answer is
    // taken on the second.
    c.tick(2);
    let died = {
        let shell = c.app_mut().ui().expect("the shell");
        shell.flow.switches > switches
            && shell.ui.node(root).is_none()
            && !shell.ui.dialogs.has_callback(old_context)
    } && !any_box(&mut c)
        && c.view().world().magic.busy_count == 0
        && targeted(&c).is_empty();

    // The replacement screen has new tiles, so the gesture is made afresh on them.
    super::open_pack(&mut c);
    c.tick(16);
    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let a_new_one = open_box(&mut c).map(|(ctx, _)| ctx) != Some(old_context);
    c.world_mut().selected = Some(TARGET);
    answer(&mut c, true);
    let works = a_new_one && targeted(&c) == [(SOURCE, TARGET)];

    c.assert_behaviour(
        "use.confirmation.one-nobody-answers-dies-with-the-screen-and-the-next-use-works",
        move |_| died && works,
    );
    c.shutdown();
}

/// Something the player has marked as kept is refused before any box is raised, in the
/// client's own words; unmark it and the same gesture says what is being done to what, in a
/// bubble the player can see.
pub fn something_kept_is_refused_before_any_box_and_a_fresh_one_is_told_about() {
    let mut c = a_client_in_the_academy();
    make_it_a_draining_stone(&mut c, SOURCE);
    c.world_mut()
        .weenie_mut(TARGET)
        .expect("described")
        .pwd
        .bitfield |= RETAINED;

    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let refused = !any_box(&mut c)
        && c.view().interaction().last_refusal.as_deref()
            == Some("You cannot drain the mana of this item because it is \"Retained\".\n");

    c.world_mut()
        .weenie_mut(TARGET)
        .expect("described")
        .pwd
        .bitfield &= !RETAINED;
    c.tick(16);
    use_one_on_the_other(&mut c, SOURCE, TARGET);
    let want = {
        let w = c.view().world();
        format!(
            "Using the {} with the {}",
            w.weenie(SOURCE)
                .expect("described")
                .object_name(dereth_client_model::weenie::NameType::Appropriate),
            w.weenie(TARGET)
                .expect("described")
                .object_name(dereth_client_model::weenie::NameType::Appropriate)
        )
    };
    // The newest bubble, which is what the player has just been told, drawn where he can
    // read it.
    let told = newest_bubble(&mut c) == Some(want) && open_box(&mut c).is_some();
    answer(&mut c, false);

    c.assert_behaviour(
        "use.confirmation.something-kept-is-refused-before-any-box-is-raised",
        move |_| refused && told,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_a_yes_uses_the_pair_the_box_was_made_with => a_yes_uses_the_pair_the_box_was_made_with ["use.confirmation.a-yes-uses-the-pair-the-box-was-made-with"],
    scenario_a_second_box_waits_behind_the_first_and_keeps_its_own_pair => a_second_box_waits_behind_the_first_and_keeps_its_own_pair ["use.confirmation.a-second-one-waits-behind-the-first-and-keeps-its-own-pair"],
    scenario_the_salvage_box_says_what_it_may_destroy_and_a_no_cancels_it => the_salvage_box_says_what_it_may_destroy_and_a_no_cancels_it ["use.confirmation.the-salvage-box-says-what-it-may-destroy-and-a-no-cancels-it"],
    scenario_one_nobody_answers_dies_with_the_screen_and_the_next_use_still_works => one_nobody_answers_dies_with_the_screen_and_the_next_use_still_works ["use.confirmation.one-nobody-answers-dies-with-the-screen-and-the-next-use-works"],
    scenario_something_kept_is_refused_before_any_box_and_a_fresh_one_is_told_about => something_kept_is_refused_before_any_box_and_a_fresh_one_is_told_about ["use.confirmation.something-kept-is-refused-before-any-box-is-raised"],
}
