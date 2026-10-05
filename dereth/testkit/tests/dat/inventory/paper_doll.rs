use super::*;

pub(super) const DOLL_PLAYER: ObjectId = ObjectId(0x5000_0001);
/// Worn on the chest from the start: the thing that is in the way of everything else.
const ROBE: ObjectId = ObjectId(0x5000_0020);
/// Carried, and wants the same place on the body as the robe.
const CHEST_PIECE: ObjectId = ObjectId(0x5000_0021);
/// Carried, and wants a place nothing is using.
pub(super) const BOOTS: ObjectId = ObjectId(0x5000_0022);
/// Carried, and can be worn nowhere at all.
const DOLL_ROCK: ObjectId = ObjectId(0x5000_0023);
const CARRIED: [ObjectId; 3] = [CHEST_PIECE, BOOTS, DOLL_ROCK];
/// The place on the body the robe holds, which is what makes the chest piece a clash.
const CLASH: u32 = 0x0000_0020;
/// One nothing holds.
const FREE: u32 = 0x0000_0080;
/// The slot of the figure the robe is drawn in, and one the figure keeps visible.
const DOLL_CHEST_SLOT: ElementId = ElementId(0x1000_01E2);
/// The foot slot, which only appears when the player asks for the grid of places instead of the
/// picture.
const DOLL_FOOT_SLOT: ElementId = ElementId(0x1000_05B3);

/// A player wearing a robe and carrying three things: one that clashes with the robe, one that
/// does not, and one that can be worn nowhere.
fn seed_a_dressed_player(w: &mut dereth_client_model::World) {
    use dereth_rules::slots::loc;

    w.player = Some(DOLL_PLAYER);
    w.tables.inventories.insert(
        DOLL_PLAYER,
        dereth_client_model::objects::ObjectInventory::new(DOLL_PLAYER),
    );
    for id in CARRIED.iter().copied().chain([DOLL_PLAYER, ROBE]) {
        let mut wn = dereth_client_model::Weenie::new(id);
        wn.valid = true;
        wn.pwd.name = format!("obj{:X}", id.0 & 0xFF);
        w.tables.weenies.insert(id, wn);
    }
    {
        let p = w.tables.weenies.get_mut(DOLL_PLAYER).expect("seeded");
        p.pwd.items_capacity = Some(102);
        p.pwd.containers_capacity = Some(7);
        p.pwd.bitfield |= dereth_rules::weenie::bitfield::OPENABLE;
    }
    for (id, valid, priority) in [
        (ROBE, loc::CHEST_WEAR, CLASH),
        (CHEST_PIECE, loc::CHEST_WEAR, CLASH),
        (BOOTS, loc::FOOT_WEAR, FREE),
    ] {
        let wn = w.tables.weenies.get_mut(id).expect("seeded");
        wn.pwd.valid_locations = Some(valid);
        wn.pwd.priority = Some(priority);
    }
    for (i, id) in CARRIED.iter().enumerate() {
        w.tables
            .weenies
            .get_mut(*id)
            .expect("seeded")
            .pwd
            .container_id = Some(DOLL_PLAYER);
        w.tables
            .inventories
            .get_mut(DOLL_PLAYER)
            .expect("seeded")
            .add_content(*id, false, i);
    }
    let robe = w.tables.weenies.get_mut(ROBE).expect("seeded");
    robe.pwd.wielder_id = Some(DOLL_PLAYER);
    robe.pwd.location = Some(loc::CHEST_WEAR);
    w.tables
        .inventories
        .get_mut(DOLL_PLAYER)
        .expect("seeded")
        .set_placement(ROBE, loc::CHEST_WEAR, CLASH);
    w.remake_character_inventory();
    assert_eq!(
        w.clothing_priority_mask & CLASH,
        CLASH,
        "the premise: the robe is on"
    );
    assert_eq!(
        w.clothing_priority_mask & FREE,
        0,
        "and nothing holds the boots' place"
    );
}

fn a_dressed_client() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    seed_a_dressed_player(&mut c.app_mut().probe_mut().objects_mut().world);
    open_pack(&mut c);
    c.tick(2);
    assert_eq!(
        doll_item(&mut c, DOLL_CHEST_SLOT),
        Some(ROBE),
        "the premise: the robe is drawn on the figure"
    );
    c
}

/// The figure's own drop area -- the picture, not one of its places.
fn figure(c: &mut HeadlessClient) -> ElemHandle {
    shipped(c, PAPER_DOLL_DRAG_MASK)
}

/// What the figure is saying about the drag being carried over it.
fn figure_answer(c: &mut HeadlessClient) -> StateId {
    let h = {
        let (_ui, screen) = gameplay_screen(c.app_mut());
        screen
            .inventory
            .doll_drag_overlay
            .expect("the figure's own answer element is bound")
    };
    let (ui, _root) = gameplay_root(c.app_mut());
    ui.node(h).expect("alive").state
}

/// Every wield this client has asked for, in order.
///
/// Read off the scenario's own record of what the client sent rather than off the frame's
/// outbox: that outbox is **replaced** every pass, so a reader that looked at it after the
/// frames a drop needs would find whatever the last frame put there, which is nothing.
fn wields(c: &HeadlessClient) -> Vec<(ObjectId, u32)> {
    c.outbound()
        .iter()
        .filter_map(|r| match r {
            Request::GetAndWieldItem(m) => Some((m.item, m.slot)),
            _ => None,
        })
        .collect()
}

/// Drop `from` on the figure and report the lines that joined the notice strip and every wield
/// the frames after the release asked for.
pub(super) fn drop_on_the_figure(
    c: &mut HeadlessClient,
    from: ElemHandle,
) -> (Vec<String>, Vec<(ObjectId, u32)>) {
    let mask = figure(c);
    let start = point_of(c, from);
    c.when(Grab(start));
    let at = carry_over(c, mask);
    let before = strip_lines(c);
    c.when(LetGo(at));
    c.tick(3);
    clear_requests(c.ui_outbox());
    let sent = wields(c);
    let after = strip_lines(c);
    (after.into_iter().skip(before.len()).collect(), sent)
}

// ---------------------------------------------------------------------------------------------
// inventory.body.the-figure-says-whether-it-could-take-what-is-carried
// ---------------------------------------------------------------------------------------------

/// The picture of the character answers for the whole body: yes for something that can be worn
/// and has a free place, no for something whose place is taken, no again for something that can
/// be worn nowhere.
pub(super) fn the_figure_says_whether_it_could_take_what_is_carried() {
    let mut c = a_dressed_client();
    let mask = figure(&mut c);
    let silent_at_first = figure_answer(&mut c) != paper_doll_drag_overlay::ACCEPT;

    let mut over_the_figure = |c: &mut HeadlessClient, item: ObjectId| {
        let tile = pack_slot(c, item);
        let from = point_of(c, tile);
        c.when(Grab(from));
        let at = carry_over(c, mask);
        let said = figure_answer(c);
        c.when(LetGo(at));
        clear_requests(c.ui_outbox());
        said
    };
    let free_place = over_the_figure(&mut c, BOOTS);
    let taken_place = over_the_figure(&mut c, CHEST_PIECE);
    let no_place_at_all = over_the_figure(&mut c, DOLL_ROCK);

    c.assert_behaviour(
        "inventory.body.the-figure-says-whether-it-could-take-what-is-carried",
        {
            move |_| {
                silent_at_first
                    && free_place == paper_doll_drag_overlay::ACCEPT
                    && taken_place == paper_doll_drag_overlay::REFUSE
                    && no_place_at_all == paper_doll_drag_overlay::REFUSE
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.something-already-worn-leaves-the-figure-saying-nothing
// ---------------------------------------------------------------------------------------------

/// Carrying a piece you are **already wearing** back over the figure leaves it saying nothing at
/// all -- not even the refusal.
///
/// "Unchanged" is only a claim if it can fail, so the answer element is first put into a state
/// neither arm writes while a drag is in the air, and the scenario ends with the positive
/// control: the same pointer, the same figure, a legal piece, and the yes appears.
pub(super) fn something_already_worn_leaves_the_figure_saying_nothing() {
    let mut c = a_dressed_client();
    let mask = figure(&mut c);

    const ARMED: StateId = paper_doll_drag_overlay::DOWN;
    {
        let h = {
            let (_ui, screen) = gameplay_screen(c.app_mut());
            screen.inventory.doll_drag_overlay.expect("bound")
        };
        let (ui, _root) = gameplay_root(c.app_mut());
        ui.set_state(h, ARMED);
    }
    assert_eq!(
        figure_answer(&mut c),
        ARMED,
        "the instrument really is armed"
    );

    let robe_tile = doll_tile(&mut c, DOLL_CHEST_SLOT);
    let from = point_of(&mut c, robe_tile);
    c.when(Grab(from));
    let at = carry_over(&mut c, mask);
    let said = figure_answer(&mut c);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let boots = pack_slot(&mut c, BOOTS);
    let from = point_of(&mut c, boots);
    c.when(Grab(from));
    let at = carry_over(&mut c, mask);
    let control = figure_answer(&mut c);
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    c.assert_behaviour(
        "inventory.body.something-already-worn-leaves-the-figure-saying-nothing",
        {
            move |_| {
                said == ARMED
                    && said != paper_doll_drag_overlay::REFUSE
                    && control == paper_doll_drag_overlay::ACCEPT
            }
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.the-figures-answer-comes-down-on-leave-and-on-the-drop
// ---------------------------------------------------------------------------------------------

/// The figure's answer comes down when the pointer leaves it, and again when the drag is let go
/// on it -- both, because either alone would leave it standing in one of the two ways a drag can
/// end.
pub(super) fn the_figures_answer_comes_down_on_leave_and_on_the_drop() {
    let mut c = a_dressed_client();
    let mask = figure(&mut c);

    let boots = pack_slot(&mut c, BOOTS);
    let from = point_of(&mut c, boots);
    c.when(Grab(from));
    carry_over(&mut c, mask);
    let up_first = figure_answer(&mut c) == paper_doll_drag_overlay::ACCEPT;
    let rock = pack_slot(&mut c, DOLL_ROCK);
    let at = carry_over(&mut c, rock);
    let down_on_leave = figure_answer(&mut c) == paper_doll_drag_overlay::DOWN;
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());

    let boots = pack_slot(&mut c, BOOTS);
    let from = point_of(&mut c, boots);
    c.when(Grab(from));
    let at = carry_over(&mut c, mask);
    let up_again = figure_answer(&mut c) == paper_doll_drag_overlay::ACCEPT;
    c.when(LetGo(at));
    clear_requests(c.ui_outbox());
    let down_on_drop = figure_answer(&mut c) == paper_doll_drag_overlay::DOWN;

    c.assert_behaviour(
        "inventory.body.the-figures-answer-comes-down-on-leave-and-on-the-drop",
        move |_| up_first && down_on_leave && up_again && down_on_drop,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.a-wearable-dropped-on-the-figure-is-put-on-with-its-own-list-of-places
// ---------------------------------------------------------------------------------------------

/// Letting a wearable go on the figure asks the shard to put it on, and what it asks with is the
/// item's **own** list of places rather than a place the client chose -- the shard picks. The
/// icon greys and the one-at-a-time lock is taken while the shard is asked, and nothing is said
/// to the player, because a legal wear has nothing to say.
///
/// The second half is the control: the same piece dropped on its **place** rather than on the
/// figure sends the same thing, so that path is still there.
pub(super) fn a_wearable_dropped_on_the_figure_is_put_on_with_its_own_places() {
    let mut c = a_dressed_client();
    let nothing_yet = wields(&c).is_empty();

    let boots = pack_slot(&mut c, BOOTS);
    let (lines, sent) = drop_on_the_figure(&mut c, boots);

    let places = c
        .app_mut()
        .objects()
        .world
        .weenie(BOOTS)
        .expect("seeded")
        .pwd
        .valid_locations;
    let asked_with_its_own = sent == vec![(BOOTS, places.expect("the boots go somewhere"))];
    let one_wear = c.app_mut().interaction().stats.wears_requested == 1;
    let greyed = c
        .app_mut()
        .objects()
        .world
        .weenie(BOOTS)
        .expect("seeded")
        .waiting;
    let locked = c.app_mut().objects().world.request_lock.pending
        == dereth_client_model::inventory::requests::InventoryRequest::Wield;
    let said_nothing = lines.is_empty();
    c.shutdown();

    // The control: the same drop on the place itself, which the player reaches by asking for the
    // grid of places instead of the picture.
    let mut c = a_dressed_client();
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.inventory.set_slot_view(ui, true);
    }
    c.tick(1);
    let foot = doll_tile(&mut c, DOLL_FOOT_SLOT);
    let boots = pack_slot(&mut c, BOOTS);
    let from = point_of(&mut c, boots);
    c.when(Grab(from));
    let at = carry_over(&mut c, foot);
    c.when(LetGo(at));
    c.tick(3);
    clear_requests(c.ui_outbox());
    let the_place_agrees = wields(&c) == sent;

    c.assert_behaviour(
        "inventory.body.a-wearable-dropped-on-the-figure-is-put-on-with-its-own-list-of-places",
        move |_| {
            nothing_yet
                && asked_with_its_own
                && one_wear
                && greyed
                && locked
                && said_nothing
                && the_place_agrees
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.what-cannot-be-worn-at-all-is-refused-in-the-clients-own-words
// ---------------------------------------------------------------------------------------------

/// Something that can be worn nowhere, dropped on the figure, gets exactly one line saying so,
/// asks the shard for nothing, and is not left greyed.
pub(super) fn what_cannot_be_worn_at_all_is_refused_in_words() {
    let mut c = a_dressed_client();
    let nowhere = c
        .app_mut()
        .objects()
        .world
        .weenie(DOLL_ROCK)
        .expect("seeded")
        .pwd
        .valid_locations;
    assert_eq!(nowhere, None, "the premise: the rock can be worn nowhere");

    let rock = pack_slot(&mut c, DOLL_ROCK);
    let (lines, sent) = drop_on_the_figure(&mut c, rock);
    let one_line = lines.len() == 1 && lines.iter().any(|t| t == CANNOT_PUT_THAT_ITEM_THERE);
    let asked_nothing = sent.is_empty() && c.app_mut().interaction().stats.wears_requested == 0;
    let not_greyed = !c
        .app_mut()
        .objects()
        .world
        .weenie(DOLL_ROCK)
        .expect("seeded")
        .waiting;

    c.assert_behaviour(
        "inventory.body.what-cannot-be-worn-at-all-is-refused-in-the-clients-own-words",
        move |_| one_line && asked_nothing && not_greyed,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.an-occupied-place-names-what-is-in-the-way
// ---------------------------------------------------------------------------------------------

/// A piece whose place on the body is taken is refused by name: the line says which thing has to
/// come off first, rather than saying only that it cannot go on.
pub(super) fn an_occupied_place_names_what_is_in_the_way() {
    let mut c = a_dressed_client();
    let piece = pack_slot(&mut c, CHEST_PIECE);
    let (lines, sent) = drop_on_the_figure(&mut c, piece);

    let names_the_blocker = lines.len() == 1
        && lines
            .iter()
            .any(|t| t == "You must remove your obj20 to wear that");
    let asked_nothing = sent.is_empty();
    let not_greyed = !c
        .app_mut()
        .objects()
        .world
        .weenie(CHEST_PIECE)
        .expect("seeded")
        .waiting;

    c.assert_behaviour(
        "inventory.body.an-occupied-place-names-what-is-in-the-way",
        { move |_| names_the_blocker && asked_nothing && not_greyed },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.something-already-worn-is-told-so-when-it-is-dropped
// ---------------------------------------------------------------------------------------------

/// Dragging a piece off the figure and dropping it back on the figure says that it is already
/// being worn -- the same state the hover answers with silence.
///
/// That asymmetry is the whole of it and it is why this row and the silent one are two rows: a
/// client that spoke on the hover would be nagging the player about every piece they own, and
/// one that said nothing on the drop would leave the gesture looking as if it had worked.
pub(super) fn something_already_worn_is_told_so_when_it_is_dropped() {
    let mut c = a_dressed_client();
    let robe = doll_tile(&mut c, DOLL_CHEST_SLOT);
    let (lines, sent) = drop_on_the_figure(&mut c, robe);

    let says_so = lines.len() == 1 && lines.iter().any(|t| t == "The obj20 is already being worn");
    let asked_nothing = sent.is_empty();
    let not_greyed = !c
        .app_mut()
        .objects()
        .world
        .weenie(ROBE)
        .expect("seeded")
        .waiting;

    c.assert_behaviour(
        "inventory.body.something-already-worn-is-told-so-when-it-is-dropped",
        { move |_| says_so && asked_nothing && not_greyed },
    );
    c.shutdown();
}

// =============================================================================================
// The figure: what a drop on a place on the body is, and what it is not.
//
// The failure these guard against: a drop on the paper doll puts the item in a pack instead of
// wielding it, silently -- no refusal, no message, the thing just goes into a pack.
//
// The fixture is one recording's own character, worn clothes and all. Which place on the body
// each of his things belongs in is **his recording's** and is never chosen here, and the census
// over all nine of his placements is in the cpu tier, where it needs no retail data file.
// =============================================================================================

/// The player of the recording these scenarios dress.
const DRESSED_PLAYER: ObjectId = ObjectId(0x5000_0003);

/// The recorded character, in world, wearing what the recording says he wore, with his pack page
/// up.
///
/// **The identity is set before the recording is replayed**, and it has to be: his description
/// arrives before the message that names him, and a description that arrives for nobody drops
/// every placement in it -- which leaves the figure blank and every scenario below measuring an
/// empty screen.
fn a_client_wearing_the_recorded_characters_clothes() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    c.world_mut().player = Some(DRESSED_PLAYER);
    c.when(Inbound::from_corpus(
        "short-second-connection",
        0..usize::MAX,
    ))
    .tick(4);
    open_pack(&mut c);
    c.tick(2);
    assert!(
        {
            let (_ui, screen) = gameplay_screen(c.app_mut());
            screen
                .inventory
                .doll
                .iter()
                .filter(|(_, w)| w.item_at(0).is_some())
                .count()
                >= 9
        },
        "the premise: the recording dresses this character, and the figure is showing it"
    );
    c
}

/// Where the recording says each of this character's worn things belongs.
fn recorded_placements(c: &HeadlessClient) -> Vec<(ObjectId, u32)> {
    c.view()
        .world()
        .inventory(DRESSED_PLAYER)
        .expect("the recorded character's own list")
        .placements
        .iter()
        .map(|p| (p.iid, p.loc))
        .collect()
}

/// Show the places on the body that are hidden by default.
///
/// The figure is shipped with nine of its places switched off so that the drawing of the
/// character can be seen, and the window's own checkbox swaps them back -- which is what a
/// player does in retail too. A place that is not on screen is not under the pointer, so a drag
/// aimed at one needs the checkbox first. This restores a precondition; it relaxes nothing.
fn show_every_place_on_the_body(c: &mut HeadlessClient) {
    let (ui, screen) = gameplay_screen(c.app_mut());
    screen.inventory.set_slot_view(ui, true);
    c.tick(1);
}

// ---------------------------------------------------------------------------------------------
// inventory.body.every-place-the-figure-is-filled-from-answers-a-drop-with-its-own-place
// ---------------------------------------------------------------------------------------------

/// Every place on the figure that the recording filled answers a drop with **its own** place on
/// the body, and the place it answers with is one the recording's own placement lands in.
///
/// The assertion runs from the screen back to the recording rather than the other way: the thing
/// the figure is showing is looked up, and the place that is showing it is required to be one
/// the recording put that thing in.
pub(super) fn every_place_the_figure_is_filled_from_answers_a_drop_with_its_own_place() {
    let mut c = a_client_wearing_the_recorded_characters_clothes();
    let placements = recorded_placements(&c);
    let there_are_some = placements.len() >= 9;

    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;
    let mut checked = 0usize;
    let mut every_one_agrees = true;
    for (item, worn_at) in &placements {
        let showing: Vec<&ItemListWidget> = inv
            .doll
            .iter()
            .filter(|(_, w)| w.item_at(0) == Some(*item))
            .map(|(_, w)| w)
            .collect();
        if showing.is_empty() {
            every_one_agrees = false;
            continue;
        }
        for w in showing {
            let Some(mask) = InventoryPanels::location_of_slot(w.element) else {
                every_one_agrees = false;
                continue;
            };
            every_one_agrees &= mask & worn_at != 0;
            every_one_agrees &=
                inv.drop_target(w.handle) == Some(equipment_destination(w.element.0));
            checked += 1;
        }
    }
    let enough = checked >= 9;

    c.assert_behaviour(
        "inventory.body.every-place-the-figure-is-filled-from-answers-a-drop-with-its-own-place",
        move |_| there_are_some && enough && every_one_agrees,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.every-place-on-the-figure-is-a-place-to-wear-and-no-cell-of-a-pack-is
// ---------------------------------------------------------------------------------------------

/// All twenty-four places on the body take a drop as something to **wear**, whether the
/// character is wearing anything there or not; no cell of the pack grid or of either strip of
/// packs does; and an element that is neither is nobody's drop at all.
///
/// The other direction is half the claim: a client that answered every drop with a wear would be
/// the same defect mirrored, and a player would watch things they meant to pack end up on their
/// body.
pub(super) fn every_place_on_the_figure_is_a_place_to_wear_and_no_cell_of_a_pack_is() {
    let mut c = a_client_wearing_the_recorded_characters_clothes();

    let outside = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let page = screen
            .inventory
            .item_list
            .as_ref()
            .expect("the pack grid")
            .handle;
        ui.parent(page)
            .and_then(|p| ui.parent(p))
            .expect("the grid has an ancestor")
    };

    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;

    let mut places: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut every_place_wears = true;
    for (mask, w) in &inv.doll {
        every_place_wears &= inv.drop_target(w.handle) == Some(equipment_destination(w.element.0));
        every_place_wears &= InventoryPanels::location_of_slot(w.element) == Some(*mask);
        // The tile inside the place answers the same way, which is what matters: the handle a
        // real hit test returns is the tile and not the list around it.
        for s in &w.slots {
            every_place_wears &=
                inv.drop_target(s.handle) == Some(equipment_destination(w.element.0));
        }
        every_place_wears &= places.insert(w.element.0);
    }
    let twenty_four = places.len() == 24;
    let the_shipped_set = places
        == PAPER_DOLL_SLOTS
            .iter()
            .map(|(id, _)| *id)
            .collect::<std::collections::BTreeSet<u32>>();

    let mut no_pack_cell_wears = true;
    let grid = inv.item_list.as_ref().expect("the pack grid");
    for (i, s) in grid.slots.iter().enumerate() {
        no_pack_cell_wears &= inv.drop_target(s.handle)
            == Some(DropTarget::ItemList {
                list: grid.element,
                slot: u32::try_from(i).unwrap_or(u32::MAX),
            });
    }
    for w in inv.top_container.iter().chain(inv.container_list.iter()) {
        for (i, s) in w.slots.iter().enumerate() {
            no_pack_cell_wears &= inv.drop_target(s.handle)
                == Some(DropTarget::ItemList {
                    list: w.element,
                    slot: u32::try_from(i).unwrap_or(u32::MAX),
                });
        }
    }
    let nobodys_drop = inv.drop_target(outside).is_none();

    c.assert_behaviour(
        "inventory.body.every-place-on-the-figure-is-a-place-to-wear-and-no-cell-of-a-pack-is",
        move |_| {
            every_place_wears
                && twenty_four
                && the_shipped_set
                && no_pack_cell_wears
                && nobodys_drop
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.the-same-drag-is-a-wear-on-the-figure-and-a-move-into-a-pack
// ---------------------------------------------------------------------------------------------

/// **The fault this module is about, and its mirror, in one run.** The same thing carried by the
/// same gesture is asked to be *worn* when it is let go on a place on the figure and *packed*
/// when it is let go on a pack -- and what decides is only what it was let go on.
///
/// It went wrong silently: a drop on the figure became a move into a pack, with no refusal and
/// no message, and the player simply found the thing in their backpack. A client that answered
/// every drop with a wear would be the same fault turned around, which is why the pack half is
/// asserted in the same scenario rather than trusted.
pub(super) fn the_same_drag_is_a_wear_on_the_figure_and_a_move_into_a_pack() {
    use dereth_rules::slots::loc;

    let wearable = HINT_DRAG_ITEMS[0];

    // On the figure.
    let (worn, packed_instead) = {
        let mut c = a_client_with_three_things_and_a_pack();
        c.app_mut()
            .probe_mut()
            .objects_mut()
            .world
            .weenie_mut(wearable)
            .expect("seeded")
            .pwd
            .valid_locations = Some(loc::SHIELD);
        c.tick(2);
        let from = grid_cell(&mut c, 0);
        let place = doll_tile(&mut c, DOLL_SHIELD_SLOT);
        let from = point_of(&mut c, from);
        let mark = c.outbound().len();
        c.when(Grab(from));
        let at = carry_over(&mut c, place);
        c.when(LetGo(at));
        c.tick(3);
        clear_requests(c.ui_outbox());
        let sent: Vec<Request> = c.outbound()[mark..].to_vec();
        let worn = sent.iter().any(|r| {
            matches!(r, Request::GetAndWieldItem(m)
                if m.item == wearable && m.slot & loc::SHIELD != 0)
        });
        let packed = sent
            .iter()
            .any(|r| matches!(r, Request::PutItemInContainer(m) if m.item == wearable));
        c.shutdown();
        (worn, packed)
    };

    // On the pack, with everything else the same.
    let mut c = a_client_with_three_things_and_a_pack();
    c.app_mut()
        .probe_mut()
        .objects_mut()
        .world
        .weenie_mut(wearable)
        .expect("seeded")
        .pwd
        .valid_locations = Some(loc::SHIELD);
    c.tick(2);
    let from = grid_cell(&mut c, 0);
    let pack_cell = strip_cell(&mut c, 0);
    let from = point_of(&mut c, from);
    let onto = point_of(&mut c, pack_cell);
    let mark = c.outbound().len();
    c.when(Player::Drag {
        from,
        to: onto,
        hold_frames: 1,
    });
    clear_requests(c.ui_outbox());
    let sent: Vec<Request> = c.outbound()[mark..].to_vec();
    let packed = sent.iter().any(|r| {
        matches!(r, Request::PutItemInContainer(m)
            if m.item == wearable && m.container == HINT_DRAG_PACK)
    });
    let worn_instead = sent
        .iter()
        .any(|r| matches!(r, Request::GetAndWieldItem(_)));

    c.assert_behaviour(
        "inventory.body.the-same-drag-is-a-wear-on-the-figure-and-a-move-into-a-pack",
        move |_| worn && !packed_instead && packed && !worn_instead,
    );
    c.shutdown();
}

// =============================================================================================
// The figure's picture on the live tree.
//
// The figure's other claims need no retail data file and are in the cpu tier; these two are about
// the shipped tree: that the picture is a place to let something go beside the twenty-four places,
// and that asking for the grid of places takes the picture out of the hit test rather than out of
// the map.
// =============================================================================================

/// The figure's viewport, its own answer overlay and the checkbox: three more things the window
/// binds, and none of them a place to let something go.
const FIGURE_VIEWPORT: ElementId = ElementId(0x1000_01D5);
const FIGURE_ANSWER_OVERLAY: ElementId = ElementId(0x1000_046D);
const PLACES_CHECKBOX: ElementId = ElementId(0x1000_05BE);

// ---------------------------------------------------------------------------------------------
// inventory.body.the-picture-of-the-character-is-a-twenty-fifth-place-to-let-something-go
// ---------------------------------------------------------------------------------------------

/// The figure takes a drop on the picture of the character as well as on each of its
/// twenty-four places -- twenty-five in all -- and nothing else the window binds takes one.
///
/// The picture is the one that was missing: a drag released on the character's own body named no
/// target at all, so the request went nowhere and the icon was not even put back.
pub(super) fn the_picture_of_the_character_is_a_twenty_fifth_place_to_let_something_go() {
    let mut c = a_client_with_three_things_and_a_pack();

    let picture = shipped(&mut c, PAPER_DOLL_DRAG_MASK);
    let others: Vec<(ElementId, ElemHandle)> =
        [FIGURE_VIEWPORT, FIGURE_ANSWER_OVERLAY, PLACES_CHECKBOX]
            .into_iter()
            .map(|id| (id, shipped(&mut c, id)))
            .collect();

    let (_ui, screen) = gameplay_screen(c.app_mut());
    let inv = &screen.inventory;

    let mut targets: std::collections::BTreeSet<u32> = std::collections::BTreeSet::new();
    let mut every_place_takes_one = true;
    for (_, w) in &inv.doll {
        every_place_takes_one &=
            inv.drop_target(w.handle) == Some(equipment_destination(w.element.0));
        targets.insert(w.element.0);
    }
    let twenty_four_places = targets.len() == 24;

    let the_picture_takes_one = inv.drop_target(picture) == Some(DropTarget::EquipCanvas);
    targets.insert(PAPER_DOLL_DRAG_MASK.0);
    let twenty_five_in_all = targets.len() == 25;
    let nothing_else_does = others.iter().all(|(_, h)| inv.drop_target(*h).is_none());

    c.assert_behaviour(
        "inventory.body.the-picture-of-the-character-is-a-twenty-fifth-place-to-let-something-go",
        move |_| {
            every_place_takes_one
                && twenty_four_places
                && the_picture_takes_one
                && twenty_five_in_all
                && nothing_else_does
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.asking-for-the-grid-of-places-takes-the-picture-out-of-the-hit-test
// ---------------------------------------------------------------------------------------------

/// Asking for the grid of places switches the picture **off**, so a pointer over where it was
/// no longer finds it -- while the map of what takes a drop is unchanged. It is the hit test
/// that moves, not the producer.
///
/// The visibility itself is asserted and not only the hit test, and the reason is a measurement:
/// the grid of places covers the same rectangle, and the walk that finds what is under the
/// pointer answers the last child first, so one of the places answers whether the picture was
/// switched off or not. A scenario that only asked what is under the pointer would pass with the
/// picture still on.
pub(super) fn asking_for_the_grid_of_places_takes_the_picture_out_of_the_hit_test() {
    let mut c = a_client_with_three_things_and_a_pack();
    let picture = shipped(&mut c, PAPER_DOLL_DRAG_MASK);
    let (x, y) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        centre(ui, picture)
    };

    // The state the window is shipped in, which is what a player sees first.
    let (showing_the_picture, the_pointer_finds_it) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        (
            !screen.inventory.slots_view,
            ui.hit_test_screen(x, y) == Some(picture),
        )
    };

    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        screen.inventory.set_slot_view(ui, true);
    }
    c.tick(1);

    let (switched_off, the_pointer_no_longer_finds_it, the_map_is_unchanged) = {
        let (ui, screen) = gameplay_screen(c.app_mut());
        (
            !ui.node(picture).expect("alive").region.flags.visible,
            ui.hit_test_screen(x, y) != Some(picture),
            screen.inventory.drop_target(picture) == Some(DropTarget::EquipCanvas),
        )
    };

    c.assert_behaviour(
        "inventory.body.asking-for-the-grid-of-places-takes-the-picture-out-of-the-hit-test",
        move |_| {
            showing_the_picture
                && the_pointer_finds_it
                && switched_off
                && the_pointer_no_longer_finds_it
                && the_map_is_unchanged
        },
    );
    c.shutdown();
}

/// The player who is clicked on.
const CLICKED_PLAYER: ObjectId = ObjectId(0x5000_0001);
/// A breastplate: chest **armour**.
const CHEST_ARMOUR: ObjectId = ObjectId(0x8000_1001);
/// A shirt: chest **clothing**, which the painted picture cannot tell apart from the armour.
const CHEST_SHIRT: ObjectId = ObjectId(0x8000_1002);

/// A player wearing a breastplate over a shirt, with the pack page up.
///
/// The two are worn in the same region of the picture on purpose: that is the case where what is
/// selected is decided by which of them is on top, and a client that answered with whichever it
/// met first would pass every other measurement here.
fn a_client_with_a_dressed_chest() -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    {
        let world = &mut c.app_mut().probe_mut().objects_mut().world;
        world.player = Some(CLICKED_PLAYER);
        let mut inv = dereth_client_model::objects::ObjectInventory::new(CLICKED_PLAYER);
        inv.placements = vec![
            InventoryPlacement {
                iid: CHEST_ARMOUR,
                loc: 0x0000_0200,
                priority: 0,
            },
            InventoryPlacement {
                iid: CHEST_SHIRT,
                loc: 0x0000_0002,
                priority: 0,
            },
        ];
        world.tables.inventories.insert(CLICKED_PLAYER, inv);
    }
    open_pack(&mut c);
    c.tick(4);
    c
}

/// The picture the client reads the body region out of.
fn the_painted_picture(c: &mut HeadlessClient) -> ClickMap {
    let (_ui, screen) = gameplay_screen(c.app_mut());
    screen
        .inventory
        .click_map
        .clone()
        .expect("the shipped picture is loaded when the window comes up")
}

/// The first point of the picture painted for `region`, in the picture's own coordinates.
fn a_point_painted_for(map: &ClickMap, region: u32) -> (i32, i32) {
    for y in 0..map.height {
        for x in 0..map.width {
            if map.mask_at(x, y) == region {
                return (x, y);
            }
        }
    }
    panic!("the shipped picture paints no point for the region {region:#06X}")
}

/// A point of the picture, as a point on screen.
fn on_screen(c: &mut HeadlessClient, point: (i32, i32)) -> Target {
    let picture = shipped(c, PAPER_DOLL_DRAG_MASK);
    let (x0, y0) = {
        let (ui, _screen) = gameplay_screen(c.app_mut());
        ui.screen_origin(picture)
    };
    Target::Point(ScreenPoint::new(x0 + point.0, y0 + point.1))
}

// ---------------------------------------------------------------------------------------------
// inventory.body.the-shipped-picture-of-the-character-is-painted-for-every-region
// ---------------------------------------------------------------------------------------------

/// The picture the client reads a body region out of is painted for **all nine** regions, and a
/// point outside it belongs to no region at all.
///
/// The count is what matters. A picture with, say, no points painted for the lower leg would let
/// every other measurement pass while one part of the body stayed dead to the pointer -- which
/// is the shape of the whole complaint.
pub(super) fn the_shipped_picture_of_the_character_is_painted_for_every_region() {
    let mut c = a_client_with_a_dressed_chest();
    let map = the_painted_picture(&mut c);

    let mut painted = 0usize;
    let mut unpainted = 0usize;
    for (_colour, region) in HIT_TEST_COLOURS {
        let mut n = 0u32;
        for y in 0..map.height {
            for x in 0..map.width {
                if map.mask_at(x, y) == region {
                    n += 1;
                }
            }
        }
        if n == 0 {
            unpainted += 1;
        } else {
            painted += 1;
        }
    }
    let nine = HIT_TEST_COLOURS.len() == 9;
    let all_painted = unpainted == 0 && painted == 9;
    let off_the_picture_is_no_region = map.mask_at(-1, 0) == 0 && map.mask_at(0, map.height) == 0;

    c.assert_behaviour(
        "inventory.body.the-shipped-picture-of-the-character-is-painted-for-every-region",
        move |_| nine && all_painted && off_the_picture_is_no_region,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.a-click-on-the-picture-selects-what-is-worn-in-that-region
// ---------------------------------------------------------------------------------------------

/// A click on a part of the picture of the character selects what the player is wearing there --
/// and when two things are worn in the same part, the one that is **on top**.
///
/// The picture cannot tell the shirt from the breastplate: they share the same painted region,
/// so a client that answered with whichever it met first would answer the shirt half the time.
/// The premise is asserted first: the click really does land on the picture, and nothing is
/// selected before it.
pub(super) fn a_click_on_the_picture_selects_what_is_worn_in_that_region() {
    let mut c = a_client_with_a_dressed_chest();
    let still_the_picture = !gameplay_screen(c.app_mut()).1.inventory.slots_view;

    let map = the_painted_picture(&mut c);
    let chest = a_point_painted_for(&map, 0x0202);
    let at = on_screen(&mut c, chest);
    let picture = shipped(&mut c, PAPER_DOLL_DRAG_MASK);
    let lands_on_the_picture = {
        let Target::Point(p) = at else {
            unreachable!("built as a point")
        };
        let (ui, _screen) = gameplay_screen(c.app_mut());
        let hit = ui
            .hit_test_screen(p.x, p.y)
            .expect("the body is under the pointer");
        hit == picture || ui.is_ancestor_of(picture, hit)
    };
    let nothing_selected_yet = c.view().world().selected.is_none();

    c.when(Player::Click(at));
    clear_requests(c.ui_outbox());
    let selected = c.view().world().selected;

    c.assert_behaviour(
        "inventory.body.a-click-on-the-picture-selects-what-is-worn-in-that-region",
        move |_| {
            still_the_picture
                && lands_on_the_picture
                && nothing_selected_yet
                && selected == Some(CHEST_ARMOUR)
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// inventory.body.a-click-on-a-bare-region-of-the-picture-selects-the-player
// ---------------------------------------------------------------------------------------------

/// A click on a part of the picture where the player is wearing nothing selects **the player**.
///
/// That is what makes an undressed character clickable at all, and it is the easiest line in the
/// whole path to lose as a "nothing there, do nothing" answer.
pub(super) fn a_click_on_a_bare_region_of_the_picture_selects_the_player() {
    let mut c = a_client_with_a_dressed_chest();
    let map = the_painted_picture(&mut c);
    // The head, where this character wears nothing.
    let head = a_point_painted_for(&map, 0x0001);
    let at = on_screen(&mut c, head);
    let nothing_selected_yet = c.view().world().selected.is_none();

    c.when(Player::Click(at));
    clear_requests(c.ui_outbox());
    let selected = c.view().world().selected;

    c.assert_behaviour(
        "inventory.body.a-click-on-a-bare-region-of-the-picture-selects-the-player",
        move |_| nothing_selected_yet && selected == Some(CLICKED_PLAYER),
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// A wield the shard refuses, and the make-shortcut key
// ---------------------------------------------------------------------------------------------

/// Something the player can eat, carried in the pack; the direct-use kind of thing a shortcut
/// holds.
pub(super) const APPLE: ObjectId = ObjectId(0x5000_0030);
/// A second one, which no shortcut holds.
pub(super) const PEAR: ObjectId = ObjectId(0x5000_0031);

/// The dressed player, with the apple and the pear in the pack and the apple on shortcut slot 2.
pub(super) fn a_dressed_client_with_food() -> HeadlessClient {
    let mut c = a_dressed_client();
    {
        let w = &mut c.app_mut().probe_mut().objects_mut().world;
        for (i, (id, name)) in [(APPLE, "Apple"), (PEAR, "Pear")].into_iter().enumerate() {
            let mut wn = dereth_client_model::Weenie::new(id);
            wn.valid = true;
            wn.pwd.name = name.to_owned();
            // `USEABLE_REMOTE`-free direct use: the use request goes straight out.
            wn.pwd.useability = Some(0x8);
            wn.pwd.container_id = Some(DOLL_PLAYER);
            w.tables.weenies.insert(id, wn);
            w.tables
                .inventories
                .get_mut(DOLL_PLAYER)
                .expect("seeded")
                .add_content(id, false, CARRIED.len() + i);
        }
        w.player_system
            .add_shortcut(dereth_protocol::login::ShortCutData {
                index: 2,
                object_id: APPLE,
                spell_id: 0,
            });
    }
    c.tick(3);
    c
}

pub(super) fn equipment_destination(element: u32) -> dereth_client_contract::view::DropTarget {
    let (mask, side) =
        dereth_rules::slots::location_info_from_element_id(element).expect("equipment slot");
    dereth_client_contract::view::DropTarget::EquipLocation {
        mask,
        side: side as u32,
    }
}
