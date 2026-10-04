use dereth_primitives::{DataId, LocalTime, ObjectId};
use dereth_ui::{ElementId, UiSystem};
use dereth_ui_screens::items::widget::child;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, SlotDecoration};

const ME: ObjectId = ObjectId(0x5300_0001);
const SACK: ObjectId = ObjectId(0x5300_0002);
/// The thing that is recharging.
const WAND: ObjectId = ObjectId(0x5300_0010);
/// The control: nothing is ever waiting on it.
const ROCK: ObjectId = ObjectId(0x5300_0011);
/// A second recharging thing, put in the pack part-way through a second.
const GEM: ObjectId = ObjectId(0x5300_0012);
const ICON: DataId = DataId(0x0600_1F19);
const RECHARGE: u32 = 42;
const PACKS: [ObjectId; 1] = [SACK];

/// A pack whose only moving parts are what is in it and how much of a recharge is left.
#[derive(Debug, Clone)]
struct Cool {
    items: Vec<ObjectId>,
    open: Option<ObjectId>,
    /// How long the whole recharge is, so a scenario can make the ratio the number it wants.
    duration: f64,
    /// What is left of it, or nothing running at all.
    left: Option<f64>,
}

impl Default for Cool {
    fn default() -> Self {
        Self {
            items: vec![WAND, ROCK],
            open: Some(ME),
            duration: 1.0,
            left: None,
        }
    }
}

impl GameView for Cool {
    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.open
    }
    fn player(&self) -> Option<ObjectId> {
        Some(ME)
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        match id {
            ME => Some("Alba"),
            SACK => Some("Belt Pouch"),
            WAND => Some("Training Wand"),
            ROCK => Some("Rock"),
            GEM => Some("Portal Gem"),
            _ => None,
        }
    }
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        if id == ME {
            &self.items
        } else {
            &[]
        }
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        if id == ME {
            &PACKS
        } else {
            &[]
        }
    }
    fn equipment(&self, _id: ObjectId) -> &[(ObjectId, u32)] {
        &[]
    }
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(24)
    }
    fn containers_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(7)
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        Some(ICON)
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        let recharging = id == WAND || id == GEM;
        Some(SlotDecoration {
            stack_size: 1,
            is_container: id == ME || id == SACK,
            items_capacity: 24,
            contained_items: 0,
            icon_id: ICON.0,
            openable: id == SACK,
            is_player: id == ME,
            cooldown_id: if recharging { RECHARGE } else { 0 },
            cooldown_duration: if recharging { self.duration } else { 0.0 },
            ..SlotDecoration::default()
        })
    }
    fn cooldown_remaining(&self, cooldown_id: u32, _now: f64) -> Option<f64> {
        if cooldown_id == RECHARGE {
            self.left.filter(|left| *left > 0.0)
        } else {
            None
        }
    }
}

/// Which wedges of the ring on the slot holding `item` are lit, anywhere on the pack page.
fn wedges(ui: &UiSystem, screen: &GamePlayScreen, item: ObjectId) -> Vec<usize> {
    let p = &screen.inventory;
    let Some(s) = p
        .top_container
        .iter()
        .chain(p.container_list.iter())
        .chain(p.item_list.iter())
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))
    else {
        return Vec::new();
    };
    child::COOLDOWN
        .iter()
        .enumerate()
        .filter(|(_, id)| {
            ui.get_child_recursive(s.handle, ElementId(**id))
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
        .map(|(i, _)| i)
        .collect()
}

/// The wedge the client's own arithmetic asks for, written out here rather than simplified:
/// the fraction left is multiplied by a hundred and then by a tenth, one is added and the
/// result is truncated, and everything at or past the tenth wedge is the tenth wedge.
fn wanted(r: f64) -> usize {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let n = (r * 100.0 * 0.1 + 1.0).trunc() as usize;
    n.saturating_sub(1).min(9)
}

/// Exactly one wedge is lit, and it is the one the fraction left names.
///
/// The last five samples are the doubles at which the client's own two multiplications and
/// the obvious single one part company; they are **searched for** rather than written down,
/// and the scenario requires that all five were found and asserted, because a sweep that
/// never lands on a boundary cannot tell the two apart.
pub fn exactly_one_wedge_of_the_ring_is_lit() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());

    let mut ordinary_ok = true;
    let mut samples = 0_usize;
    let mut at = 0.0_f64;
    for step in 1..=19 {
        let r = f64::from(step) * 0.05;
        at += 2.0;
        ui.now = LocalTime(at);
        let v = Cool {
            left: Some(r),
            ..Cool::default()
        };
        screen.update_inventory(ui, &v);
        screen.do_item_heartbeat(ui, &v);
        ordinary_ok &= wedges(&*ui, &*screen, WAND) == vec![wanted(r)];
        ordinary_ok &= wedges(&*ui, &*screen, ROCK).is_empty();
        samples += 1;
    }

    let mut discriminating = 0_usize;
    let mut boundary_ok = true;
    for tenth in [0.1_f64, 0.2, 0.4, 0.5, 0.8] {
        let Some(r) = (1..=8)
            .map(|k| f64::from_bits(tenth.to_bits() - k))
            .find(|r| (r * 100.0 * 0.1 + 1.0).trunc() != (r * 10.0 + 1.0).trunc())
        else {
            continue;
        };
        discriminating += 1;
        at += 2.0;
        ui.now = LocalTime(at);
        let v = Cool {
            left: Some(r),
            ..Cool::default()
        };
        screen.update_inventory(ui, &v);
        screen.do_item_heartbeat(ui, &v);
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let plainer = ((r * 10.0 + 1.0).trunc() as usize).saturating_sub(1).min(9);
        boundary_ok &= plainer != wanted(r) && wedges(&*ui, &*screen, WAND) == vec![wanted(r)];
    }

    c.assert_behaviour(
            "inventory.cooldown.exactly-one-wedge-of-the-ring-is-lit-and-it-is-the-one-the-time-left-names",
            move |_| samples == 19 && ordinary_ok && discriminating == 5 && boundary_ok,
        );
    c.shutdown();
}

/// A recharge the shard put on the character lights the ring on the thing it belongs to, the
/// ring shrinks as it runs down, and it goes out when the time is up.
pub fn a_recharging_thing_lights_its_ring_until_the_time_is_up() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());

    let quiet = Cool {
        duration: 30.0,
        ..Cool::default()
    };
    ui.now = LocalTime(0.0);
    assert!(
        screen.update_inventory(ui, &quiet),
        "the first drive is always a redraw"
    );
    screen.do_item_heartbeat(ui, &quiet);
    let starts_dark = wedges(&*ui, &*screen, WAND).is_empty();

    let mut seen: Vec<Vec<usize>> = Vec::new();
    let mut control = 0_usize;
    for step in 1..=11 {
        let now = f64::from(step) * 3.0;
        ui.now = LocalTime(now);
        let running = Cool {
            duration: 30.0,
            left: Some(30.0 - now),
            ..Cool::default()
        };
        screen.update_inventory(ui, &running);
        screen.do_item_heartbeat(ui, &running);
        control += wedges(&*ui, &*screen, ROCK).len();
        seen.push(wedges(&*ui, &*screen, WAND));
    }

    let lit: Vec<usize> = seen.iter().filter(|w| w.len() == 1).map(|w| w[0]).collect();
    let counted_down = control == 0
        && seen.iter().all(|w| w.len() <= 1)
        && lit.len() == 9
        && lit.windows(2).all(|p| p[0] > p[1])
        && seen.last().is_some_and(Vec::is_empty);

    c.assert_behaviour(
            "inventory.cooldown.a-thing-the-shard-says-is-recharging-lights-its-ring-and-it-goes-out-when-the-time-is-up",
            move |_| starts_dark && counted_down,
        );
    c.shutdown();
}

/// The ring steps once a second; a slot filled in between shows its wedge straight away.
pub fn the_ring_steps_once_a_second_and_a_new_slot_shows_its_wedge_at_once() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());

    let at_ninety = Cool {
        left: Some(0.95),
        ..Cool::default()
    };
    ui.now = LocalTime(1.0);
    assert!(
        screen.update_inventory(ui, &at_ninety),
        "the first drive is always a redraw"
    );
    screen.do_item_heartbeat(ui, &at_ninety);
    let first = wedges(&*ui, &*screen, WAND);

    // Half a second later the fraction has moved a long way and the ring has not: the step is
    // once a second, so what the player sees is the last step's wedge.
    let moved_on = Cool {
        left: Some(0.25),
        ..Cool::default()
    };
    ui.now = LocalTime(1.5);
    screen.do_item_heartbeat(ui, &moved_on);
    let held_still = wedges(&*ui, &*screen, WAND) == first;

    // …and a whole second after the last step it moves.
    ui.now = LocalTime(2.0);
    screen.do_item_heartbeat(ui, &moved_on);
    let stepped = wedges(&*ui, &*screen, WAND) == vec![wanted(0.25)] && first != vec![wanted(0.25)];

    // A slot filled between two steps draws its wedge at once rather than waiting for the
    // next one -- the clock is left where the last step put it and no step is run.
    ui.now = LocalTime(2.5);
    let arrived = Cool {
        items: vec![WAND, ROCK, GEM],
        left: Some(0.65),
        ..Cool::default()
    };
    let redrew = screen.update_inventory(ui, &arrived);
    let at_once = redrew && wedges(&*ui, &*screen, GEM) == vec![wanted(0.65)];

    c.assert_behaviour(
            "inventory.cooldown.the-ring-steps-once-a-second-and-a-slot-just-filled-shows-its-wedge-at-once",
            move |_| first.len() == 1 && held_still && stepped && at_once,
        );
    c.shutdown();
}

/// The frame is on the pack that is open, and on nothing else.
pub fn the_open_pack_is_the_one_wearing_the_frame() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    let mut v = Cool::default();
    assert!(
        screen.update_inventory(ui, &v),
        "the first drive is always a redraw"
    );

    let framed = |ui: &UiSystem, screen: &GamePlayScreen| -> Vec<ObjectId> {
        let p = &screen.inventory;
        p.top_container
            .iter()
            .chain(p.container_list.iter())
            .flat_map(|w| w.slots.iter())
            .filter(|s| {
                s.item.is_some()
                    && ui
                        .get_child_recursive(s.handle, ElementId(child::OPEN_CONTAINER))
                        .and_then(|h| ui.node(h))
                        .is_some_and(|n| n.region.flags.visible)
            })
            .filter_map(|s| s.item)
            .collect()
    };

    // Before any pack is opened the frame is on the character's own picture, which is what
    // the main pack is.
    let at_rest = framed(&*ui, &*screen) == vec![ME];

    assert!(
        screen.inventory.open_container(&mut ui.requests, SACK),
        "the premise: the side pack opens"
    );
    for request in ui.requests.take() {
        if let dereth_ui_screens::UiRequest::NewParentContainer(id) = request {
            v.open = Some(id);
        }
    }
    screen.update_inventory(ui, &v);
    let moved = framed(&*ui, &*screen) == vec![SACK];

    c.assert_behaviour(
        "inventory.pack.the-pack-that-is-open-is-the-one-wearing-the-frame",
        { move |_| at_rest && moved },
    );
    c.shutdown();
}

/// Picking a thing rings it wherever its picture is, and unrings what was picked before.
pub fn picking_a_thing_rings_it_everywhere_and_unrings_the_last() {
    let mut c = super::a_client_with_shortcuts();

    let first = super::rings::ringed(&mut c, super::NUM_ROCK);
    let nothing_yet = !first.is_empty() && first.iter().all(|up| !*up);

    c.world_mut().set_selected_object(
        Some(super::NUM_ROCK),
        false,
        &mut dereth_client_model::NullSink,
    );
    c.tick(2);
    let rock = super::rings::ringed(&mut c, super::NUM_ROCK);
    let shirt_quiet = super::rings::ringed(&mut c, super::WORN_SHIRT);
    let rang = rock.len() >= 2 && rock.iter().all(|up| *up) && shirt_quiet.iter().all(|up| !*up);

    c.world_mut().set_selected_object(
        Some(super::WORN_SHIRT),
        false,
        &mut dereth_client_model::NullSink,
    );
    c.tick(2);
    let rock_now = super::rings::ringed(&mut c, super::NUM_ROCK);
    let shirt_now = super::rings::ringed(&mut c, super::WORN_SHIRT);
    let moved =
        !shirt_now.is_empty() && shirt_now.iter().all(|up| *up) && rock_now.iter().all(|up| !*up);

    c.assert_behaviour(
            "inventory.selection.picking-a-thing-rings-it-everywhere-its-picture-is-and-unrings-the-last-one",
            move |_| nothing_yet && rang && moved,
        );
    c.shutdown();
}

/// A list rebuilt while the choice has not changed puts the ring back.
pub fn a_list_rebuilt_under_an_unchanged_choice_puts_the_ring_back() {
    let mut c = super::a_client_with_shortcuts();
    c.world_mut().set_selected_object(
        Some(super::NUM_PACK),
        false,
        &mut dereth_client_model::NullSink,
    );
    c.tick(2);
    let ringed = super::rings::ringed(&mut c, super::NUM_PACK);
    let started = !ringed.is_empty() && ringed.iter().all(|up| *up);

    // Rebuild the side-pack strip with the choice untouched. The decoration is the client's
    // own answer from the live world, and the list's own flush is observed before it is filled
    // again, so this cannot pass on a list that was never rebuilt.
    let decoration = {
        use dereth_ui_screens::view::GameView as _;
        let app = c.view().expect_app();
        app.hud()
            .view(app.objects())
            .slot_decoration(super::NUM_PACK)
            .expect("the selected side pack has a decoration")
    };
    let icon = dereth_primitives::DataId(decoration.icon_id);
    let info = dereth_ui_screens::items::widget::SlotInfo {
        decoration,
        name: "side pack".to_owned(),
        plural_name: None,
        cooldown_remaining: None,
    };
    let flushed = {
        let (ui, screen) = super::parts(c.app_mut());
        let strip = screen
            .inventory
            .container_list
            .as_mut()
            .expect("the side-pack strip");
        strip.set_contents(ui, Some(super::NUM_PLAYER), Some(7), &[], &|_| None);
        let flushed = strip.slots.iter().all(|s| s.item.is_none() && !s.selected);
        strip.set_contents(
            ui,
            Some(super::NUM_PLAYER),
            Some(7),
            &[super::NUM_PACK],
            &|id| (id == super::NUM_PACK).then_some(icon),
        );
        strip.decorate(ui, &|id| (id == super::NUM_PACK).then(|| info.clone()));
        flushed
    };
    let back = super::rings::ringed(&mut c, super::NUM_PACK);
    let put_back = !back.is_empty() && back.iter().all(|up| *up);

    c.assert_behaviour(
        "inventory.selection.a-list-rebuilt-under-an-unchanged-choice-puts-the-ring-back",
        move |_| started && flushed && put_back,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_exactly_one_wedge_of_the_ring_is_lit => exactly_one_wedge_of_the_ring_is_lit ["inventory.cooldown.exactly-one-wedge-of-the-ring-is-lit-and-it-is-the-one-the-time-left-names"],
    scenario_a_recharging_thing_lights_its_ring_until_the_time_is_up => a_recharging_thing_lights_its_ring_until_the_time_is_up ["inventory.cooldown.a-thing-the-shard-says-is-recharging-lights-its-ring-and-it-goes-out-when-the-time-is-up"],
    scenario_the_ring_steps_once_a_second_and_a_new_slot_shows_its_wedge_at_once => the_ring_steps_once_a_second_and_a_new_slot_shows_its_wedge_at_once ["inventory.cooldown.the-ring-steps-once-a-second-and-a-slot-just-filled-shows-its-wedge-at-once"],
    scenario_the_open_pack_is_the_one_wearing_the_frame => the_open_pack_is_the_one_wearing_the_frame ["inventory.pack.the-pack-that-is-open-is-the-one-wearing-the-frame"],
    scenario_picking_a_thing_rings_it_everywhere_and_unrings_the_last => picking_a_thing_rings_it_everywhere_and_unrings_the_last ["inventory.selection.picking-a-thing-rings-it-everywhere-its-picture-is-and-unrings-the-last-one"],
    scenario_a_list_rebuilt_under_an_unchanged_choice_puts_the_ring_back => a_list_rebuilt_under_an_unchanged_choice_puts_the_ring_back ["inventory.selection.a-list-rebuilt-under-an-unchanged-choice-puts-the-ring-back"],
}
