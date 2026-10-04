use super::*;

// ---------------------------------------------------------------------------------------------
// o524 and o566: the pack.
// ---------------------------------------------------------------------------------------------

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const PACK: ObjectId = ObjectId(0x5000_0003);
/// The subject: a stackable loose item whose name, plural name and count all move on their own.
pub(super) const OIL: ObjectId = ObjectId(0x8000_0A6E);
/// The control: a second loose item nothing touches, so a pass that re-decorated everything still
/// has to leave this one where it was.
const WAND: ObjectId = ObjectId(0x8000_0A6F);
/// A third object that exists and is not in the pack, so a station can move the id list without
/// inventing an object at the same time.
pub(super) const GEM: ObjectId = ObjectId(0x8000_0A70);
const PACK_ICON: DataId = DataId(0x0600_103F);
/// The raw cooldown key; the client adds its own offset before it asks the registry.
const COOLDOWN_ID: u32 = 42;
const COOLDOWN_DURATION: f64 = 30.0;

/// A pack that answers every accessor the inventory panels read, and nothing else.
///
/// Each input a station moves is its **own** field, so one station can move one of them with the
/// id lists, the capacities, the waiting set and every other decoration frozen.
#[derive(Debug, Clone)]
struct Pack {
    items: Vec<ObjectId>,
    containers: Vec<ObjectId>,
    equipment: Vec<(ObjectId, u32)>,
    items_capacity: i32,
    containers_capacity: i32,
    /// What the decoration reports, kept apart from the accessor above so that a station can move
    /// one without the other -- which is what tells "in the gate" from "in something the gate
    /// happens to carry".
    decoration_capacity: i32,
    waiting: Vec<ObjectId>,
    name: String,
    plural_name: String,
    stack_size: u32,
    /// A live cooldown, or none at all -- the registry holding no such entry.
    cooldown: Option<(f64, f64)>,
}

impl Default for Pack {
    fn default() -> Self {
        Self {
            items: vec![OIL, WAND],
            containers: vec![PACK],
            equipment: Vec::new(),
            items_capacity: 24,
            containers_capacity: 7,
            decoration_capacity: 24,
            waiting: Vec::new(),
            name: "Oil of Rendering".to_owned(),
            plural_name: "Oils of Rendering".to_owned(),
            stack_size: 1,
            cooldown: None,
        }
    }
}

impl GameView for Pack {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        match id {
            PLAYER => Some("Alba"),
            PACK => Some("Backpack"),
            OIL => Some(&self.name),
            WAND => Some("Training Wand"),
            GEM => Some("Facility Hub Portal Gem"),
            _ => None,
        }
    }
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        match id {
            OIL => Some(&self.plural_name),
            WAND => Some("Training Wands"),
            _ => None,
        }
    }
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        if id == PLAYER {
            &self.items
        } else {
            &[]
        }
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        if id == PLAYER {
            &self.containers
        } else {
            &[]
        }
    }
    fn equipment(&self, id: ObjectId) -> &[(ObjectId, u32)] {
        if id == PLAYER {
            &self.equipment
        } else {
            &[]
        }
    }
    fn items_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(self.items_capacity)
    }
    fn containers_capacity(&self, _id: ObjectId) -> Option<i32> {
        Some(self.containers_capacity)
    }
    fn item_waiting(&self, id: ObjectId) -> bool {
        self.waiting.contains(&id)
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        Some(PACK_ICON)
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        Some(SlotDecoration {
            stack_size: if id == OIL { self.stack_size } else { 1 },
            is_container: id == PLAYER || id == PACK,
            items_capacity: self.decoration_capacity,
            contained_items: 2,
            icon_id: PACK_ICON.0,
            // Only the wand carries a cooldown, so the oil is the control slot in the countdown
            // station: it must never light a ring.
            cooldown_id: if id == WAND { COOLDOWN_ID } else { 0 },
            cooldown_duration: if id == WAND { COOLDOWN_DURATION } else { 0.0 },
            waiting: self.waiting.contains(&id),
            ..SlotDecoration::default()
        })
    }
    fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
        let (start, duration) = self.cooldown?;
        if cooldown_id != COOLDOWN_ID {
            return None;
        }
        let left = start + duration - now;
        (left > 0.0).then_some(left)
    }
}

/// Which of the ten ring children of the slot holding `item` is up, if any. Read off the element
/// tree, because the panel's mirror and the screen are two different claims.
fn ring(ui: &UiSystem, w: &ItemListWidget, item: ObjectId) -> Option<usize> {
    let s = w.slots.iter().find(|s| s.item == Some(item))?;
    child::COOLDOWN
        .iter()
        .enumerate()
        .find(|(_, id)| {
            ui.get_child_recursive(s.handle, ElementId(**id))
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
        .map(|(i, _)| i)
}

/// How many rings are up on that slot -- a denominator, because "exactly one" is the property.
fn rings_up(ui: &UiSystem, w: &ItemListWidget, item: ObjectId) -> usize {
    let Some(s) = w.slots.iter().find(|s| s.item == Some(item)) else {
        return 0;
    };
    child::COOLDOWN
        .iter()
        .filter(|id| {
            ui.get_child_recursive(s.handle, ElementId(**id))
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
        .count()
}

/// Is the grey overlay up on the slot holding `item`? `None` when no slot holds it.
fn ghosted(ui: &UiSystem, w: &ItemListWidget, item: ObjectId) -> Option<bool> {
    let s = w.slots.iter().find(|s| s.item == Some(item))?;
    let h = s.ghosted?;
    Some(ui.node(h).is_some_and(|n| n.region.flags.visible))
}

fn grid(screen: &GamePlayScreen) -> &ItemListWidget {
    screen.inventory.item_list.as_ref().expect("the pack grid")
}

/// What the slot holding `item` says about itself on hover, off the element and not the mirror.
fn slot_tooltip(ui: &UiSystem, w: &ItemListWidget, item: ObjectId) -> Option<String> {
    let s = w.slots.iter().find(|s| s.item == Some(item))?;
    ui.node(s.handle)?.tooltip_text.clone()
}

/// The ten samples every countdown station takes, and what they must be.
///
/// The step is three seconds of simulated time per frame, which is stated because the answer
/// depends on it: the beat is one second, so every frame carries one and the sequence is the
/// ratio curve rather than the beat gate.
fn countdown_expectation(now: f64) -> Option<usize> {
    if now >= COOLDOWN_DURATION {
        return None;
    }
    let r = (COOLDOWN_DURATION - now) / COOLDOWN_DURATION;
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    Some(((r * 10.0).floor() as usize).min(9))
}

/// The player asking for a move greys that item alone, at once.
pub(super) fn a_move_request_ghosts_its_own_item_at_once() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let quiet = Pack::default();
    assert!(
        screen.update_inventory(ui, &quiet),
        "the first drive is always a redraw"
    );
    assert!(
        grid(screen).slots.iter().any(|s| s.item == Some(OIL)),
        "the premise: the item reached a slot of the shipped grid"
    );
    let starts_clear = ghosted(ui, grid(screen), OIL) == Some(false);
    let gate_live = !screen.update_inventory(ui, &quiet);

    // The request goes out. One field moves.
    let waiting = Pack {
        waiting: vec![OIL],
        ..Pack::default()
    };
    assert_eq!(
        waiting.items, quiet.items,
        "the premise: the id list is identical"
    );
    assert_eq!(waiting.equipment, quiet.equipment, "and so is what is worn");
    let ghosted_now = screen.update_inventory(ui, &waiting)
        && ghosted(ui, grid(screen), OIL) == Some(true)
        // …and only that one: a pass that greyed the list would satisfy the line above.
        && ghosted(ui, grid(screen), WAND) == Some(false)
        && !screen.update_inventory(ui, &waiting);

    // The shard answers and the flag clears.
    let cleared =
        screen.update_inventory(ui, &quiet) && ghosted(ui, grid(screen), OIL) == Some(false);

    c.assert_behaviour(
        "inventory.pack.a-request-the-player-made-ghosts-its-own-item-at-once",
        { move |_| starts_clear && gate_live && ghosted_now && cleared },
    );
    c.shutdown();
}

/// The grid draws as many slots as the open container can hold, read for itself.
///
/// The decoration is held **byte-identical** across the two frames, which is a state the
/// production host cannot produce -- and which is exactly the configuration a later change to one
/// accessor would create. It is what tells "in the gate" from "in something the gate carries".
pub(super) fn the_grid_follows_the_containers_own_capacity() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let small = Pack {
        items_capacity: 12,
        decoration_capacity: 24,
        ..Pack::default()
    };
    assert!(
        screen.update_inventory(ui, &small),
        "the first drive is always a redraw"
    );
    let before = grid(screen).slots.len();
    assert!(before > 0, "the premise: the grid drew slots");
    let gate_live = !screen.update_inventory(ui, &small);

    let large = Pack {
        items_capacity: 20,
        decoration_capacity: 24,
        ..Pack::default()
    };
    assert_eq!(
        large.items, small.items,
        "the premise: the id list is identical"
    );
    assert_eq!(
        large.slot_decoration(OIL),
        small.slot_decoration(OIL),
        "the premise: and so is every decoration"
    );
    let widened = screen.update_inventory(ui, &large)
        && grid(screen).slots.len() == 20
        && grid(screen).slots.len() != before
        && !screen.update_inventory(ui, &large);

    c.assert_behaviour(
        "inventory.pack.the-grid-draws-as-many-slots-as-the-container-can-hold",
        { move |_| gate_live && widened },
    );
    c.shutdown();
}

/// The side strip draws as many slots as the player can carry packs, which is a different number
/// about a different thing.
pub(super) fn the_side_strip_follows_the_players_own_capacity() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let small = Pack {
        containers_capacity: 3,
        ..Pack::default()
    };
    assert!(
        screen.update_inventory(ui, &small),
        "the first drive is always a redraw"
    );
    let narrow = screen
        .inventory
        .container_list
        .as_ref()
        .expect("the side strip")
        .slots
        .len();
    assert_eq!(
        narrow, 3,
        "the premise: the strip is as wide as the capacity says"
    );
    let gate_live = !screen.update_inventory(ui, &small);

    let large = Pack {
        containers_capacity: 6,
        ..Pack::default()
    };
    assert_eq!(
        large.containers, small.containers,
        "the premise: the id list is identical"
    );
    assert_eq!(
        large.slot_decoration(PLAYER),
        small.slot_decoration(PLAYER),
        "the premise: and the player's own decoration does not move either"
    );
    let widened = screen.update_inventory(ui, &large)
        && screen
            .inventory
            .container_list
            .as_ref()
            .expect("the side strip")
            .slots
            .len()
            == 6
        && !screen.update_inventory(ui, &large);

    c.assert_behaviour(
        "inventory.pack.the-side-strip-draws-as-many-slots-as-the-player-can-carry-packs",
        move |_| gate_live && widened,
    );
    c.shutdown();
}

/// The ring counts down while the pack is otherwise still, and without the separate pass it
/// would not.
pub(super) fn the_pack_ring_counts_down_while_the_gate_is_shut() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let quiet = Pack::default();
    ui.now = LocalTime(0.0);
    assert!(
        screen.update_inventory(ui, &quiet),
        "the first drive is always a redraw"
    );
    screen.do_item_heartbeat(ui, &quiet);
    assert!(
        grid(screen).slots.iter().any(|s| s.item == Some(WAND)),
        "the premise: the recharging thing is in a live slot"
    );
    let starts_dark = rings_up(ui, grid(screen), WAND) == 0;

    // One starts. **The only difference is not a field of the pack at all** -- it is an entry in
    // the player's own registry, which is why no snapshot of the pack could carry it in
    // principle.
    let running = Pack {
        cooldown: Some((0.0, COOLDOWN_DURATION)),
        ..Pack::default()
    };
    assert_eq!(
        running.items, quiet.items,
        "the premise: nothing in the pack moved"
    );
    assert_eq!(running.slot_decoration(WAND), quiet.slot_decoration(WAND));
    assert_eq!(running.items_capacity, quiet.items_capacity);
    assert_eq!(running.waiting, quiet.waiting);

    let mut seen: Vec<Option<usize>> = Vec::new();
    let mut redrawn = 0_u32;
    let mut control_lit = 0_usize;
    let mut one_at_a_time = true;
    for step in 1..=11 {
        let now = f64::from(step) * 3.0;
        ui.now = LocalTime(now);
        // Both passes, in the frame's own order: the gated one, then the exemption.
        if screen.update_inventory(ui, &running) {
            redrawn += 1;
        }
        screen.do_item_heartbeat(ui, &running);
        one_at_a_time &= rings_up(ui, grid(screen), WAND) <= 1;
        control_lit += rings_up(ui, grid(screen), OIL);
        assert_eq!(
            ring(ui, grid(screen), WAND),
            countdown_expectation(now),
            "t={now}: the ring the remaining fraction asks for"
        );
        seen.push(countdown_expectation(now));
    }

    let counted_down = redrawn == 0
        && control_lit == 0
        && seen.len() == 11
        && seen.iter().filter(|w| w.is_some()).count() == 9
        && one_at_a_time
        && seen
            .iter()
            .filter_map(|w| *w)
            .collect::<Vec<_>>()
            .windows(2)
            .all(|p| p[0] > p[1])
        && seen.last().copied().flatten().is_none();

    // **The negative half.** The same eleven frames with only the gated pass, which is what the
    // panel alone can do about a cooldown -- and the answer is nothing.
    let mut frozen = a_gameplay_client();
    let (ui, screen) = parts(frozen.app_mut());
    ui.now = LocalTime(3.0);
    assert!(
        screen.update_inventory(ui, &running),
        "the first drive is always a redraw"
    );
    let first = ring(ui, grid(screen), WAND);
    let mut shut = true;
    for step in 2..=11 {
        ui.now = LocalTime(f64::from(step) * 3.0);
        shut &= !screen.update_inventory(ui, &running);
    }
    let stayed_put = shut && first == Some(9) && ring(ui, grid(screen), WAND) == first;
    frozen.shutdown();

    c.assert_behaviour(
        "inventory.cooldown.a-running-ring-counts-down-while-the-pack-is-otherwise-still",
        move |_| starts_dark && counted_down && stayed_put,
    );
    c.shutdown();
}

/// A slot is redrawn when anything it draws about its item changes.
///
/// The decorations are held **byte-identical** across the two name stations, which is the whole
/// point of them: a gate that carried only the decoration could not see those frames at all.
pub(super) fn a_pack_slot_follows_every_word_it_draws() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    // The premise, and the gate's liveness.
    let base = Pack::default();
    assert!(
        screen.update_inventory(ui, &base),
        "the first drive is always a redraw"
    );
    let filled = grid(screen)
        .slots
        .iter()
        .filter(|s| s.item.is_some())
        .count()
        == 2
        && screen.inventory.slots_decorated >= 2;
    let gate_live = !screen.update_inventory(ui, &base);
    // …and the gate is shut because nothing moved, not because it is welded.
    let mut moved = base.clone();
    moved.items.push(GEM);
    let not_welded = screen.update_inventory(ui, &moved) && !screen.update_inventory(ui, &moved);

    let decorations = |p: &Pack| -> Vec<Option<SlotDecoration>> {
        [PLAYER, PACK, OIL, WAND, GEM]
            .iter()
            .map(|id| p.slot_decoration(*id))
            .collect()
    };

    // 1. The name alone, at a stack of one.
    assert!(
        screen.update_inventory(ui, &base),
        "back to the frame the stations start from"
    );
    let singular = slot_tooltip(ui, grid(screen), OIL) == Some("Oil of Rendering".to_owned());
    let renamed = Pack {
        name: "Oil of Bafflement".to_owned(),
        ..Pack::default()
    };
    assert_eq!(
        decorations(&base),
        decorations(&renamed),
        "the premise: every decoration holds"
    );
    assert_eq!(renamed.items, base.items, "the premise: the id list holds");
    let name_moved = screen.update_inventory(ui, &renamed)
        && slot_tooltip(ui, grid(screen), OIL) == Some("Oil of Bafflement".to_owned())
        // …while the control slot, which nothing touched, kept its own.
        && slot_tooltip(ui, grid(screen), WAND) == Some("Training Wand".to_owned())
        && !screen.update_inventory(ui, &renamed)
        // …and back, because a gate is a claim about an edge and an edge has two of them.
        && screen.update_inventory(ui, &base)
        && slot_tooltip(ui, grid(screen), OIL) == Some("Oil of Rendering".to_owned());

    // 2. The plural name alone, which only a stack above one can show.
    let five = Pack {
        stack_size: 5,
        ..Pack::default()
    };
    assert!(screen.update_inventory(ui, &five));
    let plural_shown =
        slot_tooltip(ui, grid(screen), OIL) == Some("5 Oils of Rendering".to_owned());
    let repluralised = Pack {
        stack_size: 5,
        plural_name: "Flasks of Oil".to_owned(),
        ..Pack::default()
    };
    assert_eq!(
        decorations(&five),
        decorations(&repluralised),
        "the premise: the count holds too"
    );
    assert_eq!(
        five.name, repluralised.name,
        "the premise: the singular name holds"
    );
    let plural_moved = screen.update_inventory(ui, &repluralised)
        && slot_tooltip(ui, grid(screen), OIL) == Some("5 Flasks of Oil".to_owned())
        && !screen.update_inventory(ui, &repluralised);

    // …and an item with no plural of its own falls back to its name, which is a different value
    // rather than a different path, so it redraws.
    let no_plural = Pack {
        stack_size: 5,
        plural_name: String::new(),
        ..Pack::default()
    };
    let fallback = screen.update_inventory(ui, &no_plural)
        && slot_tooltip(ui, grid(screen), OIL) == Some("5 Oil of Rendering".to_owned());

    // 3. The count alone, which was inside the gate before any of this and is the control: a
    // failure above must read as an omission rather than as a dead panel.
    assert!(screen.update_inventory(ui, &base));
    let seven = Pack {
        stack_size: 7,
        ..Pack::default()
    };
    assert_eq!(
        (base.name.as_str(), base.plural_name.as_str()),
        (seven.name.as_str(), seven.plural_name.as_str()),
        "the premise: both names are held still"
    );
    let count_moved = screen.update_inventory(ui, &seven)
        && slot_tooltip(ui, grid(screen), OIL) == Some("7 Oils of Rendering".to_owned())
        && !screen.update_inventory(ui, &seven);

    c.assert_behaviour(
        "inventory.pack.a-slot-is-redrawn-when-anything-it-draws-about-its-item-changes",
        move |_| {
            filled
                && gate_live
                && not_welded
                && singular
                && name_moved
                && plural_shown
                && plural_moved
                && fallback
                && count_moved
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// o549: the shortcut bar.
// ---------------------------------------------------------------------------------------------

const BAR_OVERLAY: DataId = DataId(0x0600_3355);

/// A shortcut bar that answers every accessor its redraw reads, and nothing else.
#[derive(Debug, Clone)]
pub(super) struct Bar {
    pub(super) slots: Vec<Option<ObjectId>>,
    pub(super) combat_mode: u32,
    pub(super) stack_size: u32,
    pub(super) overlay: Option<DataId>,
    pub(super) name: String,
    /// Which objects the client has heard of at all. An object outside this set has no decoration,
    /// which is the arrival edge -- held identical across every station but its own.
    pub(super) known: Vec<ObjectId>,
    pub(super) cooldown: Option<(f64, f64)>,
}

impl Default for Bar {
    fn default() -> Self {
        let mut slots = vec![None; SLOT_COUNT as usize];
        slots[0] = Some(WAND);
        slots[1] = Some(OIL);
        Self {
            slots,
            combat_mode: 0,
            stack_size: 1,
            overlay: None,
            name: "Training Wand".to_owned(),
            known: vec![WAND, OIL, GEM],
            cooldown: None,
        }
    }
}

impl GameView for Bar {
    fn shortcut(&self, slot: u32) -> Option<ObjectId> {
        self.slots.get(slot as usize).copied().flatten()
    }
    fn combat_mode(&self) -> u32 {
        self.combat_mode
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        match id {
            WAND => Some(&self.name),
            OIL => Some("Arrow"),
            GEM => Some("Facility Hub Portal Gem"),
            _ => None,
        }
    }
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        match id {
            WAND => Some("Training Wands"),
            OIL => Some("Arrows"),
            _ => None,
        }
    }
    fn icon(&self, _id: ObjectId) -> Option<DataId> {
        Some(PACK_ICON)
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        if !self.known.contains(&id) {
            return None;
        }
        Some(SlotDecoration {
            stack_size: if id == WAND { self.stack_size } else { 1 },
            icon_id: PACK_ICON.0,
            icon_overlay_id: if id == WAND { self.overlay } else { None },
            shortcut_num: self
                .slots
                .iter()
                .position(|slot| *slot == Some(id))
                .and_then(|index| u32::try_from(index).ok()),
            cooldown_id: if id == WAND { COOLDOWN_ID } else { 0 },
            cooldown_duration: if id == WAND { COOLDOWN_DURATION } else { 0.0 },
            ..SlotDecoration::default()
        })
    }
    fn cooldown_remaining(&self, cooldown_id: u32, now: f64) -> Option<f64> {
        let (start, duration) = self.cooldown?;
        if cooldown_id != COOLDOWN_ID {
            return None;
        }
        let left = start + duration - now;
        (left > 0.0).then_some(left)
    }
}

fn tiles(screen: &GamePlayScreen) -> &[ItemListWidget] {
    &screen.shortcuts.slots
}

fn tile_tooltip(ui: &UiSystem, lists: &[ItemListWidget], item: ObjectId) -> Option<String> {
    let s = lists
        .iter()
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))?;
    ui.node(s.handle)?.tooltip_text.clone()
}

fn tile_overlay(ui: &UiSystem, lists: &[ItemListWidget], item: ObjectId) -> Option<DataId> {
    let s = lists
        .iter()
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))?;
    match s.icon_recipe(ui)? {
        dereth_ui::region::IconRecipe::Object { overlay, .. } => overlay,
        dereth_ui::region::IconRecipe::Spell { .. } => None,
    }
}

fn bar_rings_up(ui: &UiSystem, lists: &[ItemListWidget], item: ObjectId) -> usize {
    let Some(s) = lists
        .iter()
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))
    else {
        return 0;
    };
    child::COOLDOWN
        .iter()
        .filter(|id| {
            ui.get_child_recursive(s.handle, ElementId(**id))
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
        .count()
}

fn bar_ring(ui: &UiSystem, lists: &[ItemListWidget], item: ObjectId) -> Option<usize> {
    let s = lists
        .iter()
        .flat_map(|w| w.slots.iter())
        .find(|s| s.item == Some(item))?;
    child::COOLDOWN
        .iter()
        .enumerate()
        .find(|(_, id)| {
            ui.get_child_recursive(s.handle, ElementId(**id))
                .and_then(|h| ui.node(h))
                .is_some_and(|n| n.region.flags.visible)
        })
        .map(|(i, _)| i)
}

/// All eighteen tiles are there, they fill, and an unchanged frame redraws nothing.
pub(super) fn the_eighteen_tiles_fill_and_hold_still() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let found = tiles(screen).len() == SLOT_COUNT as usize;
    assert!(
        found,
        "the premise: all eighteen lists are in the shipped tree"
    );

    let v = Bar::default();
    assert!(
        screen.update_shortcuts(ui, &v),
        "the first drive is always a redraw"
    );
    let filled = tiles(screen)
        .iter()
        .flat_map(|w| w.slots.iter())
        .filter(|s| s.item.is_some())
        .count()
        == 2
        && screen.shortcuts.slots_decorated == 2;
    let gate_live = !screen.update_shortcuts(ui, &v);

    // The discriminator: the gate is shut because nothing moved, not because it is welded.
    let mut moved = v.clone();
    moved.slots[2] = Some(GEM);
    let not_welded = screen.update_shortcuts(ui, &moved) && !screen.update_shortcuts(ui, &moved);

    c.assert_behaviour(
        "shortcut.bar.the-eighteen-tiles-fill-and-an-unchanged-frame-redraws-nothing",
        { move |_| found && filled && gate_live && not_welded },
    );
    c.shutdown();
}

/// Every word and every mark a tile draws is inside its gate, each on its own.
pub(super) fn a_tile_follows_every_thing_it_draws() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let one = Bar {
        stack_size: 1,
        ..Bar::default()
    };
    assert!(
        screen.update_shortcuts(ui, &one),
        "the first drive is always a redraw"
    );
    let gate_live = !screen.update_shortcuts(ui, &one);
    let singular = tile_tooltip(ui, tiles(screen), WAND) == Some("Training Wand".to_owned());

    // 1. How many there are.
    let seven = Bar {
        stack_size: 7,
        ..Bar::default()
    };
    assert_eq!(
        seven.slots, one.slots,
        "the premise: the eighteen slots hold"
    );
    assert_eq!(seven.combat_mode, one.combat_mode, "and the stance");
    assert_eq!(seven.known, one.known, "and what the client has heard of");
    assert_eq!(
        one.slot_decoration(OIL),
        seven.slot_decoration(OIL),
        "and the other tile's decoration does not move either"
    );
    let count_moved = screen.update_shortcuts(ui, &seven)
        && tile_tooltip(ui, tiles(screen), WAND) == Some("7 Training Wands".to_owned())
        && !screen.update_shortcuts(ui, &seven);

    // 2. The mark over the picture, which changes no text at all.
    assert!(
        screen.update_shortcuts(ui, &one),
        "back to the frame the next station starts from"
    );
    let plain = tile_overlay(ui, tiles(screen), WAND).is_none();
    let marked = Bar {
        overlay: Some(BAR_OVERLAY),
        ..Bar::default()
    };
    assert_eq!(
        marked.slots, one.slots,
        "the premise: the eighteen slots hold"
    );
    assert_eq!(
        one.slot_decoration(WAND).map(|d| d.stack_size),
        marked.slot_decoration(WAND).map(|d| d.stack_size),
        "the premise: and the count, so this station is entered on one field"
    );
    let overlay_moved = screen.update_shortcuts(ui, &marked)
        && tile_overlay(ui, tiles(screen), WAND) == Some(BAR_OVERLAY)
        && !screen.update_shortcuts(ui, &marked);

    // 3. The name, which a gate carrying only the picture's decoration would still have missed.
    assert!(screen.update_shortcuts(ui, &one));
    let renamed = Bar {
        name: "Sword of Lost Hope".to_owned(),
        ..Bar::default()
    };
    assert_eq!(
        renamed.slots, one.slots,
        "the premise: the eighteen slots hold"
    );
    assert_eq!(
        one.slot_decoration(WAND),
        renamed.slot_decoration(WAND),
        "the premise: and every decoration -- which is what makes this the discriminating one"
    );
    let name_moved = screen.update_shortcuts(ui, &renamed)
        && tile_tooltip(ui, tiles(screen), WAND) == Some("Sword of Lost Hope".to_owned())
        && !screen.update_shortcuts(ui, &renamed);

    c.assert_behaviour(
        "shortcut.bar.a-tile-is-redrawn-when-anything-it-draws-about-its-item-changes",
        { move |_| gate_live && singular && count_moved && plain && overlay_moved && name_moved },
    );
    c.shutdown();
}

/// The ring on the bar counts down while the bar redraws nothing.
pub(super) fn the_bar_ring_counts_down_while_the_gate_is_shut() {
    let mut c = a_gameplay_client();
    let (ui, screen) = parts(c.app_mut());

    let quiet = Bar::default();
    ui.now = LocalTime(0.0);
    assert!(
        screen.update_shortcuts(ui, &quiet),
        "the first drive is always a redraw"
    );
    screen.do_item_heartbeat(ui, &quiet);
    assert!(
        tiles(screen)
            .iter()
            .flat_map(|w| w.slots.iter())
            .any(|s| s.item == Some(WAND)),
        "the premise: the recharging thing is in a live tile"
    );
    let starts_dark = bar_rings_up(ui, tiles(screen), WAND) == 0;

    let running = Bar {
        cooldown: Some((0.0, COOLDOWN_DURATION)),
        ..Bar::default()
    };
    assert_eq!(
        running.slots, quiet.slots,
        "the premise: nothing on the bar moved"
    );
    assert_eq!(running.slot_decoration(WAND), quiet.slot_decoration(WAND));
    assert_eq!(running.name, quiet.name);

    let mut seen: Vec<Option<usize>> = Vec::new();
    let mut redrawn = 0_u32;
    let mut control_lit = 0_usize;
    let mut one_at_a_time = true;
    for step in 1..=11 {
        let now = f64::from(step) * 3.0;
        ui.now = LocalTime(now);
        if screen.update_shortcuts(ui, &running) {
            redrawn += 1;
        }
        screen.do_item_heartbeat(ui, &running);
        one_at_a_time &= bar_rings_up(ui, tiles(screen), WAND) <= 1;
        control_lit += bar_rings_up(ui, tiles(screen), OIL);
        assert_eq!(
            bar_ring(ui, tiles(screen), WAND),
            countdown_expectation(now),
            "t={now}: the ring the remaining fraction asks for"
        );
        seen.push(countdown_expectation(now));
    }

    c.assert_behaviour(
        "shortcut.cooldown.a-running-ring-counts-down-while-the-bar-is-otherwise-still",
        move |_| {
            starts_dark
                && redrawn == 0
                && control_lit == 0
                && one_at_a_time
                && seen.len() == 11
                && seen.iter().filter(|w| w.is_some()).count() == 9
                && seen.first().copied().flatten() == Some(9)
                && seen
                    .iter()
                    .filter_map(|w| *w)
                    .collect::<Vec<_>>()
                    .windows(2)
                    .all(|p| p[0] > p[1])
                && seen.last().copied().flatten().is_none()
        },
    );
    c.shutdown();
}
