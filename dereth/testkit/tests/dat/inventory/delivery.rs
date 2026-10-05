use dereth_client_model::Request;
use dereth_client_runtime::interaction::TargetMode;
use dereth_primitives::ObjectId;
use dereth_testkit::{ClientSpec, HeadlessClient, Inbound, Player, Target};
use dereth_ui::ElemHandle;

/// The recording's own player, and the two things this pair of clicks is about: the oil, and
/// the bow it is used on.
const PLAYER: ObjectId = ObjectId(0x5000_000A);
const OIL: ObjectId = ObjectId(0x8000_0A6F);
const BOW: ObjectId = ObjectId(0x8000_09A4);
/// The second pair the recording carries.
const SECOND_OIL: ObjectId = ObjectId(0x8000_0A73);
const WAND: ObjectId = ObjectId(0x8000_099D);
/// The blob the recorded client's own first targeted use is, and its second.
const FIRST_USE_AT: usize = 4315;
const SECOND_USE_AT: usize = 4363;

/// The client, with the recording replayed up to the moment its own player used the oil on
/// the bow, and the pack open.
fn a_client_in_the_academy() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // A corpus replay carries no login, so nothing else says who the player is.
    c.world_mut().player = Some(PLAYER);
    c.when(Inbound::corpus_until("long-solo-play", FIRST_USE_AT));
    super::open_pack(&mut c);
    c.tick(3);
    c
}

/// The tile the pack, the strip or the figure is showing `id` in.
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

/// A real click at a tile's own centre.
fn click_tile(c: &mut HeadlessClient, id: ObjectId) {
    let h = slot(c, id);
    let target = super::point_of(c, h);
    c.when(Player::Click(target)).tick(1);
}

/// The shipped toolbar's Use button.
fn use_button() -> Target {
    Target::Element(dereth_ui_screens::toolbar::target_mode::USE_BUTTON)
}

/// The pointer moved to a tile's own centre and left there, with a frame run.
fn hover_tile(c: &mut HeadlessClient, h: ElemHandle) {
    let target = super::point_of(c, h);
    let Target::Point(p) = target else {
        unreachable!("point_of answers a point")
    };
    {
        let (ui, _screen) = super::gameplay_screen(c.app_mut());
        ui.mouse_move(dereth_primitives::LocalTime(1.0), p.x, p.y);
    }
    c.tick(1);
}

/// Every targeted use the client has asked for, as `(thing, target)`.
fn targeted(c: &HeadlessClient) -> Vec<(ObjectId, ObjectId)> {
    c.outbound()
        .iter()
        .filter_map(|r| match r {
            Request::UseWithTargetEvent(m) => Some((m.object, m.target)),
            _ => None,
        })
        .collect()
}

/// Whether the client's last targeted use is, byte for byte, the one the recording carries at
/// `idx` -- everything but the order stamp the wire puts in front of it.
///
/// This is the point of the whole module: a client that used the right thing on the wrong
/// one, or used it with the wrong amount, is invisible on screen.
fn is_the_recorded_use(c: &HeadlessClient, idx: usize) -> bool {
    use dereth_protocol::Message;
    let Some(Request::UseWithTargetEvent(m)) = c
        .outbound()
        .iter()
        .rev()
        .find(|r| matches!(r, Request::UseWithTargetEvent(_)))
    else {
        return false;
    };
    let mut bytes = dereth_protocol::items::InventoryUseWithTargetEvent::OPCODE
        .0
        .to_le_bytes()
        .to_vec();
    bytes.extend(dereth_protocol::write_body(m).expect("the request encodes"));
    let corpus = dereth_client_net::client_session::testing::Corpus::load("long-solo-play")
        .expect("the recording parses")
        .expect("the corpus carries it");
    corpus
        .blobs
        .iter()
        .find(|b| b.idx == idx)
        .is_some_and(|b| bytes == b.payload[8..])
}

/// The cursor the client is showing, as the shipped cursor list numbers them.
fn cursor_is(c: &mut HeadlessClient, key: u32) -> bool {
    let store = std::sync::Arc::clone(c.dat_store().expect("the retail data is open"));
    let want = dereth_client_runtime::assets::enum_did(
        &*store,
        dereth_client_shell::cursor::UICURSOR_GROUP,
        key,
    )
    .expect("the shipped cursor list");
    let app = c.app_mut();
    app.current_cursor_did() == Some(want)
        && app.ui().expect("the shell").ui.last_cursor == Some((want, 14, 14))
        && app.cursor_stats().failures == 0
}

// -----------------------------------------------------------------------------------------

/// **The second click uses what was armed, not what is picked.** The player picks the oil,
/// presses Use, and then -- with something else picked in between, which a client reading the
/// selection instead of the armed thing would follow -- clicks the bow. What goes out is the
/// oil on the bow, byte for byte the recording's own, and the second click does not change
/// what is picked.
///
/// The recording's second pair is driven after the shard's own answers have been replayed,
/// so the release of the busy flag is the shard's and not a wait.
pub fn the_second_click_uses_what_was_armed_and_not_what_is_picked() {
    let mut c = a_client_in_the_academy();

    click_tile(&mut c, OIL);
    let picked = c.view().world().selected == Some(OIL);
    c.when(Player::Click(use_button())).tick(1);
    let armed = c.view().interaction().target_mode() == TargetMode::UseTarget;

    // Something else picked, after the arming: the click below must not follow it.
    c.world_mut().selected = Some(PLAYER);
    click_tile(&mut c, BOW);
    let used = targeted(&c) == [(OIL, BOW)]
        && is_the_recorded_use(&c, FIRST_USE_AT)
        && c.view().world().selected == Some(PLAYER)
        && c.view().world().targeting_object == ObjectId(0)
        && c.view().world().magic.busy_count == 1;

    // The shard's own answers, which release the busy flag and create what follows.
    c.when(Inbound::from_corpus(
        "long-solo-play",
        FIRST_USE_AT + 1..SECOND_USE_AT,
    ))
    .tick(7);
    let released = c.view().world().magic.busy_count == 0;

    click_tile(&mut c, SECOND_OIL);
    c.when(Player::Click(use_button())).tick(1);
    c.world_mut().selected = Some(PLAYER);
    click_tile(&mut c, WAND);
    let second = is_the_recorded_use(&c, SECOND_USE_AT)
        && c.view().world().selected == Some(PLAYER)
        && c.view().world().magic.busy_count == 1;

    c.assert_behaviour(
        "use.target-mode.the-second-click-uses-what-was-armed-and-not-what-is-picked",
        move |_| picked && armed && used && released && second,
    );
    c.shutdown();
}

/// **The Use button with nothing picked arms the mode instead**, and the next click on a thing
/// becomes the thing to be used -- without picking it. The click after that uses it.
pub fn the_use_button_with_nothing_picked_arms_and_the_next_click_is_the_source() {
    let mut c = a_client_in_the_academy();
    c.world_mut().selected = None;

    c.when(Player::Click(use_button())).tick(1);
    let armed = c.view().interaction().target_mode() == TargetMode::Use;

    click_tile(&mut c, OIL);
    let became_the_source = c.view().interaction().target_mode() == TargetMode::UseTarget
            && c.view().world().targeting_object == OIL
            // A click that is answering an armed mode does not also pick the thing.
            && c.view().world().selected.is_none();

    click_tile(&mut c, BOW);
    let used = is_the_recorded_use(&c, FIRST_USE_AT)
        && c.view().interaction().target_mode() == TargetMode::None;

    c.assert_behaviour(
        "use.target-mode.the-button-with-nothing-picked-arms-and-the-next-click-is-the-source",
        move |_| armed && became_the_source && used,
    );
    c.shutdown();
}

/// **While the second click is waiting the cursor follows what is under it**, and it is read
/// again every frame rather than only when the pointer moves: making the thing under the
/// pointer untouchable changes the cursor with no movement at all, and making it touchable
/// again changes it back. An empty tile and a button are neither.
pub fn the_cursor_follows_what_is_under_it_while_the_second_click_waits() {
    let mut c = a_client_in_the_academy();
    click_tile(&mut c, OIL);
    c.when(Player::Click(use_button())).tick(1);

    let bow = slot(&mut c, BOW);
    hover_tile(&mut c, bow);
    let over_the_bow = c.view().interaction().pick.click_object() == (BOW, -1)
            // A hover never picks anything.
            && c.view().world().selected == Some(OIL)
            && cursor_is(&mut c, 40);

    c.world_mut()
        .weenie_mut(BOW)
        .expect("the recording described it")
        .trade_state = 1;
    c.tick(1);
    let no_longer = cursor_is(&mut c, 41);
    c.world_mut()
        .weenie_mut(BOW)
        .expect("the recording described it")
        .trade_state = 0;
    c.tick(1);
    let again = cursor_is(&mut c, 40);

    c.assert_behaviour(
        "use.target-mode.the-cursor-follows-what-is-under-it-while-the-second-click-waits",
        move |_| over_the_bow && no_longer && again,
    );
    c.shutdown();
}

/// Three clicks delivered in one batch are each read against the state the one before it left,
/// rather than all against the state at the start: the first picks, the second arms, and the
/// third uses -- and the third does not pick.
pub fn a_batch_of_clicks_reads_each_answer_as_it_is_made() {
    let mut c = a_client_in_the_academy();
    c.world_mut().selected = Some(PLAYER);

    let oil = slot(&mut c, OIL);
    let bow = slot(&mut c, BOW);
    let button = super::element_of(&c, dereth_ui_screens::toolbar::target_mode::USE_BUTTON);
    // No frame between them: all three go into the shell in one delivery.
    for h in [oil, button, bow] {
        let (x, y) = super::centre_of(&mut c, h);
        let (ui, _screen) = super::gameplay_screen(c.app_mut());
        ui.mouse_move(dereth_primitives::LocalTime(1.0), x, y);
        ui.mouse_down(dereth_ui::focus::action::PRIMARY_CLICK, x, y);
        ui.mouse_up(dereth_ui::focus::action::PRIMARY_CLICK, x, y, false);
    }
    c.tick(1);

    let each_in_turn = is_the_recorded_use(&c, FIRST_USE_AT)
        && c.view().world().selected == Some(OIL)
        && c.view().world().targeting_object == ObjectId(0);

    c.assert_behaviour(
        "use.target-mode.a-batch-of-clicks-reads-each-answer-as-it-is-made",
        move |_| each_in_turn,
    );
    c.shutdown();
}

/// A move the player has already asked for keeps its hold, and the Use button pressed
/// afterwards does not overtake it: the move goes out, the hold names the thing it is about,
/// and nothing is armed.
pub fn a_move_already_asked_for_is_not_overtaken_by_the_use_button() {
    use dereth_client_contract::{DropTarget, UiRequest};

    let mut c = a_client_in_the_academy();
    click_tile(&mut c, OIL);

    c.when(Player::Ui(vec![UiRequest::DragDrop {
        item: BOW,
        target: DropTarget::Container(PLAYER),
    }]));
    c.when(Player::Click(use_button())).tick(1);

    let moved = c.outbound().iter().any(
        |r| matches!(r, Request::PutItemInContainer(m) if m.item == BOW && m.container == PLAYER),
    );
    let held = c.view().world().request_lock.object == Some(BOW)
        && !c.view().world().request_lock.is_idle();
    let not_overtaken = c.view().interaction().target_mode() == TargetMode::None
        && c.view().world().targeting_object == ObjectId(0)
        && !c
            .outbound()
            .iter()
            .any(|r| matches!(r, Request::UseWithTargetEvent(_) | Request::UseEvent(_)));

    c.assert_behaviour(
        "inventory.place.a-move-already-asked-for-is-not-overtaken-by-the-use-button",
        move |_| moved && held && not_overtaken,
    );
    c.shutdown();
}

/// Clicking a side pack fills the grid with that pack's contents **in the same frame**, and
/// clicking back to the player's own row fills it with the player's again.
pub fn clicking_a_side_pack_refills_the_grid_in_the_same_frame() {
    let mut c = a_client_in_the_academy();

    let (bag, handle) = {
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        screen
            .inventory
            .container_list
            .as_ref()
            .expect("the strip of packs")
            .slots
            .iter()
            .find_map(|s| s.item.map(|id| (id, s.handle)))
            .expect("the recording carries a side pack")
    };
    let target = super::point_of(&mut c, handle);
    c.when(Player::Click(target));
    let opened = {
        let world_says =
            c.view().world().selected == Some(bag) && c.view().world().open_container == Some(bag);
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        world_says
            && screen
                .inventory
                .item_list
                .as_ref()
                .expect("the grid")
                .parent_container
                == Some(bag)
    };

    let player_row = {
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        screen
            .inventory
            .top_container
            .as_ref()
            .expect("the row above the strip")
            .slots
            .iter()
            .find(|s| s.item == Some(PLAYER))
            .expect("the player's own row")
            .handle
    };
    let target = super::point_of(&mut c, player_row);
    c.when(Player::Click(target));
    let back = {
        let world_says = c.view().world().open_container == Some(PLAYER);
        let (_ui, screen) = super::gameplay_screen(c.app_mut());
        let grid = screen.inventory.item_list.as_ref().expect("the grid");
        world_says
            && grid.parent_container == Some(PLAYER)
            && grid.slots.iter().any(|s| s.item == Some(OIL))
    };

    c.assert_behaviour(
        "inventory.pack.clicking-a-side-pack-refills-the-grid-in-the-same-frame",
        move |_| opened && back,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_the_second_click_uses_what_was_armed_and_not_what_is_picked => the_second_click_uses_what_was_armed_and_not_what_is_picked ["use.target-mode.the-second-click-uses-what-was-armed-and-not-what-is-picked"],
    scenario_the_use_button_with_nothing_picked_arms_and_the_next_click_is_the_source => the_use_button_with_nothing_picked_arms_and_the_next_click_is_the_source ["use.target-mode.the-button-with-nothing-picked-arms-and-the-next-click-is-the-source"],
    scenario_the_cursor_follows_what_is_under_it_while_the_second_click_waits => the_cursor_follows_what_is_under_it_while_the_second_click_waits ["use.target-mode.the-cursor-follows-what-is-under-it-while-the-second-click-waits"],
    scenario_a_batch_of_clicks_reads_each_answer_as_it_is_made => a_batch_of_clicks_reads_each_answer_as_it_is_made ["use.target-mode.a-batch-of-clicks-reads-each-answer-as-it-is-made"],
    scenario_a_move_already_asked_for_is_not_overtaken_by_the_use_button => a_move_already_asked_for_is_not_overtaken_by_the_use_button ["inventory.place.a-move-already-asked-for-is-not-overtaken-by-the-use-button"],
    scenario_clicking_a_side_pack_refills_the_grid_in_the_same_frame => clicking_a_side_pack_refills_the_grid_in_the_same_frame ["inventory.pack.clicking-a-side-pack-refills-the-grid-in-the-same-frame"],
}
