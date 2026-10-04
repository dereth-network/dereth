use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElemHandle, UiSystem};
use dereth_ui_screens::items::widget::ItemSlot;
use dereth_ui_screens::screens::gameplay::GamePlayScreen;
use dereth_ui_screens::view::{GameView, SlotDecoration};

const ME: ObjectId = ObjectId(0x5100_0001);
/// A pack with things in it.
const SACK: ObjectId = ObjectId(0x5100_0002);
/// A pack with nothing in it.
const EMPTY_SACK: ObjectId = ObjectId(0x5100_0003);
/// A stack.
const ARROWS: ObjectId = ObjectId(0x5100_0004);
/// One of something.
const COAT: ObjectId = ObjectId(0x5100_0005);
/// Something that wears down.
const LOCKPICK: ObjectId = ObjectId(0x5100_0006);
const ICON: DataId = DataId(0x0600_1F19);
const PACKS: [ObjectId; 2] = [SACK, EMPTY_SACK];

/// A pack that answers every accessor a slot's decoration reads, with each thing this group
/// moves kept in its own field.
#[derive(Debug, Clone)]
struct Deco {
    items: Vec<ObjectId>,
    stack: u32,
    plural: Option<String>,
    worn_down: (u32, u32),
    in_the_sack: i32,
}

impl Default for Deco {
    fn default() -> Self {
        Self {
            items: vec![ARROWS, COAT, LOCKPICK],
            stack: 250,
            plural: Some("Arrows".to_owned()),
            worn_down: (5, 20),
            in_the_sack: 6,
        }
    }
}

impl GameView for Deco {
    fn player(&self) -> Option<ObjectId> {
        Some(ME)
    }
    fn name(&self, id: ObjectId) -> Option<&str> {
        match id {
            ME => Some("Alba"),
            SACK => Some("Belt Pouch"),
            EMPTY_SACK => Some("Sack"),
            ARROWS => Some("Arrow"),
            COAT => Some("Academy Coat"),
            LOCKPICK => Some("Lockpick"),
            _ => None,
        }
    }
    fn plural_name(&self, id: ObjectId) -> Option<&str> {
        if id == ARROWS {
            self.plural.as_deref()
        } else {
            None
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
        #[allow(clippy::cast_possible_truncation, clippy::cast_possible_wrap)]
        let mine = self.items.len() as i32;
        Some(SlotDecoration {
            stack_size: if id == ARROWS { self.stack } else { 1 },
            is_container: id == ME || id == SACK || id == EMPTY_SACK,
            items_capacity: 24,
            contained_items: match id {
                ME => mine,
                SACK => self.in_the_sack,
                _ => 0,
            },
            structure: if id == LOCKPICK { self.worn_down.0 } else { 0 },
            max_structure: if id == LOCKPICK { self.worn_down.1 } else { 0 },
            icon_id: ICON.0,
            openable: id == SACK || id == EMPTY_SACK,
            is_player: id == ME,
            containers_capacity: if id == ME { 7 } else { 0 },
            ..SlotDecoration::default()
        })
    }
}

fn text_of(ui: &mut UiSystem, h: ElemHandle) -> String {
    ui.text_element_mut(h)
        .map_or_else(String::new, |t| t.glyphs.inq_text(false))
}

/// Every live slot of the pack page: the grid, both strips of packs and the figure.
fn all_slots(screen: &GamePlayScreen) -> Vec<ItemSlot> {
    let p = &screen.inventory;
    p.top_container
        .iter()
        .chain(p.container_list.iter())
        .chain(p.item_list.iter())
        .chain(p.doll.iter().map(|(_, w)| w))
        .flat_map(|w| w.slots.iter())
        .cloned()
        .collect()
}

fn slot_of(screen: &GamePlayScreen, item: ObjectId) -> ItemSlot {
    all_slots(screen)
        .into_iter()
        .find(|s| s.item == Some(item))
        .unwrap_or_else(|| panic!("nothing on the pack page is drawing {item:?}"))
}

/// What the slot holding `item` says on hover: the panel's own record, the words on the
/// element itself, and whether the element will show them at all.
fn hover(
    ui: &UiSystem,
    screen: &GamePlayScreen,
    item: ObjectId,
) -> (Option<String>, Option<String>, bool) {
    let s = slot_of(screen, item);
    let n = ui.node(s.handle).expect("the slot is alive");
    (
        s.tooltip.clone(),
        n.tooltip_text.clone(),
        n.region.flags.tooltip,
    )
}

/// Whether a bar on the slot holding `item` is up, and what it reads.
fn bar(ui: &UiSystem, h: Option<ElemHandle>) -> (bool, f32) {
    let h = h.expect("the shipped slot carries this bar");
    let n = ui.node(h).expect("the bar is alive");
    (
        n.region.flags.visible,
        n.merged_properties()
            .get_float(dereth_ui_screens::bind::attr::METER_LEVEL)
            .unwrap_or(-1.0),
    )
}

/// A stack says how many there are; one of something says only its name.
pub fn a_slot_says_how_many_there_are_on_hover() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());

    let several = Deco::default();
    assert!(
        screen.update_inventory(ui, &several),
        "the first drive is always a redraw"
    );
    let counted = hover(&*ui, &*screen, ARROWS)
        == (
            Some("250 Arrows".to_owned()),
            Some("250 Arrows".to_owned()),
            true,
        );
    let alone = hover(&*ui, &*screen, COAT)
        == (
            Some("Academy Coat".to_owned()),
            Some("Academy Coat".to_owned()),
            true,
        );
    let nothing_to_say = all_slots(&*screen).iter().any(|s| {
        s.item.is_none()
            && s.tooltip.is_none()
            && !ui.node(s.handle).expect("alive").region.flags.tooltip
    });

    // A stack the shard gave no plural for is counted with the name it did give.
    let bare = Deco {
        plural: None,
        ..Deco::default()
    };
    assert!(
        screen.update_inventory(ui, &bare),
        "a plural arriving is a redraw"
    );
    let singular = hover(&*ui, &*screen, ARROWS).0 == Some("250 Arrow".to_owned());

    c.assert_behaviour(
        "inventory.pack.a-slot-says-on-hover-how-many-there-are-and-only-when-there-are-several",
        move |_| counted && alone && nothing_to_say && singular,
    );
    c.shutdown();
}

/// Nothing in the pack is painted with a number, and the shop's own row still is.
///
/// The second half is driven on the very same shipped slot through the one writer the client
/// has for it, which is the shop's; a one-sided test would pass on a build that never wrote
/// the element at all.
pub fn no_number_is_painted_on_a_pack_icon() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    assert!(
        screen.update_inventory(ui, &Deco::default()),
        "the first drive is a redraw"
    );

    let occupied: Vec<ItemSlot> = all_slots(&*screen)
        .into_iter()
        .filter(|s| s.item.is_some())
        .collect();
    let enough = occupied.len() >= 4;
    let silent = occupied.iter().all(|s| {
        s.quantity_value == -1
            && match s.quantity {
                Some(q) => !ui.node(q).expect("alive").region.flags.visible,
                None => true,
            }
    });

    let mut slot = occupied[0].clone();
    let q = slot
        .quantity
        .expect("the shipped slot carries a place for a number");
    slot.set_quantity(7);
    let printed = slot.update_quantity_display(ui)
        && text_of(ui, q) == "7"
        && ui.node(q).expect("alive").region.flags.visible;
    slot.set_quantity(-1);
    let taken_off =
        slot.update_quantity_display(ui) && !ui.node(q).expect("alive").region.flags.visible;

    c.assert_behaviour(
        "inventory.pack.no-number-is-painted-on-a-pack-icon-and-a-shop-row-still-paints-one",
        move |_| enough && silent && printed && taken_off,
    );
    c.shutdown();
}

/// A pack with something in it says how full it is; an empty one says nothing at all.
pub fn a_pack_with_something_in_it_shows_how_full_it_is() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    assert!(
        screen.update_inventory(ui, &Deco::default()),
        "the first drive is a redraw"
    );

    let held = slot_of(&*screen, SACK);
    let filled = bar(&*ui, held.capacity_bar);
    let is_a_container = held.is_container;
    let own = bar(&*ui, slot_of(&*screen, ME).capacity_bar);
    let empty = bar(&*ui, slot_of(&*screen, EMPTY_SACK).capacity_bar);
    let reads_right = filled.0
        && (filled.1 - 6.0 / 24.0).abs() < 1e-6
        && own.0
        && (own.1 - 3.0 / 24.0).abs() < 1e-6
        && !empty.0;

    // …and it follows what is in the pack, up and back down to nothing.
    let fuller = Deco {
        in_the_sack: 12,
        ..Deco::default()
    };
    assert!(
        screen.update_inventory(ui, &fuller),
        "a pack filling up is a redraw"
    );
    let grown = bar(&*ui, slot_of(&*screen, SACK).capacity_bar);
    let emptied_out = Deco {
        in_the_sack: 0,
        ..Deco::default()
    };
    assert!(
        screen.update_inventory(ui, &emptied_out),
        "a pack emptying is a redraw"
    );
    let gone = bar(&*ui, slot_of(&*screen, SACK).capacity_bar);
    let follows = grown.0 && (grown.1 - 0.5).abs() < 1e-6 && !gone.0;

    c.assert_behaviour(
            "inventory.pack.a-pack-with-something-in-it-shows-how-full-it-is-and-an-empty-one-shows-nothing",
            move |_| is_a_container && reads_right && follows,
        );
    c.shutdown();
}

/// A worn-down thing shows how much of it is left; a whole one shows nothing.
pub fn a_damaged_thing_shows_how_worn_it_is() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    assert!(
        screen.update_inventory(ui, &Deco::default()),
        "the first drive is a redraw"
    );

    let worn = bar(&*ui, slot_of(&*screen, LOCKPICK).structure_bar);
    let whole_thing = bar(&*ui, slot_of(&*screen, COAT).structure_bar);
    let shown = worn.0 && (worn.1 - 0.25).abs() < 1e-6 && !whole_thing.0;

    let repaired = Deco {
        worn_down: (20, 20),
        ..Deco::default()
    };
    assert!(
        screen.update_inventory(ui, &repaired),
        "a repair is a redraw"
    );
    let mended = bar(&*ui, slot_of(&*screen, LOCKPICK).structure_bar);

    c.assert_behaviour(
        "inventory.pack.a-damaged-thing-shows-how-worn-it-is-and-a-whole-one-shows-nothing",
        move |_| shown && !mended.0,
    );
    c.shutdown();
}

/// Every slot answers the pointer, full or empty, and having something to say on hover is
/// not what makes it do so.
pub fn every_slot_stays_clickable_with_or_without_a_tooltip() {
    let mut c = super::a_gameplay_client();
    let (ui, screen) = super::parts(c.app_mut());
    assert!(
        screen.update_inventory(ui, &Deco::default()),
        "the first drive is a redraw"
    );

    let slots = all_slots(&*screen);
    let occupied: Vec<&ItemSlot> = slots.iter().filter(|s| s.item.is_some()).collect();
    let vacant: Vec<&ItemSlot> = slots.iter().filter(|s| s.item.is_none()).collect();
    let enough = occupied.len() >= 4 && vacant.len() >= 10;
    let full_ones = occupied
        .iter()
        .all(|s| s.tooltip.is_some() && ui.node(s.handle).expect("alive").is_mouse_visible);
    let empty_ones = vacant.iter().all(|s| {
        let n = ui.node(s.handle).expect("alive");
        s.tooltip.is_none() && !n.region.flags.tooltip && n.is_mouse_visible
    });
    // …and a click and a drop both still find every one of them.
    let handles: Vec<ElemHandle> = slots.iter().map(|s| s.handle).collect();
    let routed = handles.iter().all(|h| {
        screen.inventory.locate(*h).is_some() && screen.inventory.drop_target(*h).is_some()
    });

    // A slot losing what it had loses its words with it, and is still clickable.
    let emptied = Deco {
        items: vec![COAT, LOCKPICK],
        ..Deco::default()
    };
    assert!(
        screen.update_inventory(ui, &emptied),
        "a thing leaving the pack is a redraw"
    );
    let after: Vec<ItemSlot> = all_slots(&*screen)
        .into_iter()
        .filter(|s| s.item.is_none())
        .collect();
    let still_clickable = after.iter().all(|s| {
        let n = ui.node(s.handle).expect("alive");
        s.tooltip.is_none() && n.is_mouse_visible
    }) && after.len() > vacant.len();

    c.assert_behaviour(
        "inventory.pack.every-slot-stays-clickable-whether-or-not-it-has-anything-to-say",
        move |_| enough && full_ones && empty_ones && routed && still_clickable,
    );
    c.shutdown();
}

dereth_testkit::scenarios! {
    scenario_a_slot_says_how_many_there_are_on_hover => a_slot_says_how_many_there_are_on_hover ["inventory.pack.a-slot-says-on-hover-how-many-there-are-and-only-when-there-are-several"],
    scenario_no_number_is_painted_on_a_pack_icon => no_number_is_painted_on_a_pack_icon ["inventory.pack.no-number-is-painted-on-a-pack-icon-and-a-shop-row-still-paints-one"],
    scenario_a_pack_with_something_in_it_shows_how_full_it_is => a_pack_with_something_in_it_shows_how_full_it_is ["inventory.pack.a-pack-with-something-in-it-shows-how-full-it-is-and-an-empty-one-shows-nothing"],
    scenario_a_damaged_thing_shows_how_worn_it_is => a_damaged_thing_shows_how_worn_it_is ["inventory.pack.a-damaged-thing-shows-how-worn-it-is-and-a-whole-one-shows-nothing"],
    scenario_every_slot_stays_clickable_with_or_without_a_tooltip => every_slot_stays_clickable_with_or_without_a_tooltip ["inventory.pack.every-slot-stays-clickable-whether-or-not-it-has-anything-to-say"],
}
