//! The view as text, from a view made by hand.

use auberean_client_sdk::contract::view::{
    Container, Contents, Inventory, Item, Surroundings, Thing, View,
};
use auberean_client_sdk::Phase;
use auberean_headless::snapshot::{held, text};

const PLAYER: u64 = 0x2130_0000_0000_0001;
const SWORD: u64 = 0x8131_0000_0000_0010;
const SHIRT: u64 = 0x8131_0000_0000_0011;
const DOOR: u64 = 0x8130_0000_0000_0993;
const OFFERED: u64 = 0x8131_0000_0000_0020;

fn item(name: &str, worn: bool) -> Item {
    Item {
        quantity: 1,
        name: name.to_owned(),
        worn,
        ..Item::default()
    }
}

/// A player holding a sword in its pack and wearing a shirt, with `selected` selected; the
/// inventory's items carry the selection's facts and a trade offer's as well, as the runtime's do.
fn view(selected: u64, door_around: bool) -> View {
    let pack = Container {
        size: 24,
        segments: vec![Contents {
            items: vec![SWORD],
            first: 0,
            count: 24,
        }],
        slots: vec![(0, 24)],
    };
    let mut items = std::collections::BTreeMap::new();
    items.insert(SWORD, item("Rusty Sword", false));
    items.insert(SHIRT, item("Human Starter Shirt", true));
    items.insert(DOOR, item("Curiosity Shop", false));
    items.insert(OFFERED, item("Sapling Bow", false));
    let mut things = vec![Thing {
        id: PLAYER,
        ..Thing::default()
    }];
    if door_around {
        things.push(Thing {
            id: DOOR,
            name: Some("Curiosity Shop".to_owned()),
            ..Thing::default()
        });
    }
    let mut world = Surroundings {
        player: PLAYER,
        things,
        ..Surroundings::default()
    };
    world.selection.current = selected;
    View {
        world: Some(world),
        inventory: Some(Inventory {
            player: PLAYER,
            containers: [(PLAYER, pack)].into_iter().collect(),
            worn: vec![(1, SHIRT)],
            items,
            selection: selected,
            ..Inventory::default()
        }),
        ..View::default()
    }
}

/// The inventory's lines between its head and the next fact.
fn inventory_lines(lines: &[String]) -> Vec<String> {
    let at = lines
        .iter()
        .position(|l| l.starts_with("inventory:"))
        .expect("an inventory line");
    lines[at..]
        .iter()
        .take_while(|l| l.starts_with("inventory:") || l.starts_with("  "))
        .cloned()
        .collect()
}

/// The inventory lists what the pack holds and what is worn; the selection, whose facts the
/// inventory carries for its icon to be dragged from the world, is not counted in it, nor is a
/// trade's offer. A selection around the player is shown there; one neither held nor around is
/// told by name on a line of its own.
///
/// Behaviour: session.headless.the-inventory-lists-what-the-player-holds
#[test]
fn the_inventory_lists_the_pack_and_what_is_worn_never_the_selection() {
    let v = view(DOOR, true);
    let inv = v.inventory.as_ref().expect("an inventory");
    assert_eq!(
        held(inv).into_iter().collect::<Vec<_>>(),
        [SWORD, SHIRT],
        "the pack's and the worn, not the selection's or the offer's"
    );
    let lines = text(&v, Phase::World);
    assert_eq!(
        inventory_lines(&lines),
        [
            "inventory: 2 items, 0 money",
            "  0x8131000000000010 Rusty Sword x1",
            "  0x8131000000000011 Human Starter Shirt x1 worn",
        ]
    );
    assert!(
        !lines.iter().any(|l| l.starts_with("selected:")),
        "a selection around is shown there: {lines:#?}"
    );

    let lines = text(&view(DOOR, false), Phase::World);
    assert_eq!(inventory_lines(&lines).len(), 3, "{lines:#?}");
    assert!(
        lines.contains(&"selected: 0x8130000000000993 Curiosity Shop".to_owned()),
        "{lines:#?}"
    );

    let lines = text(&view(SWORD, false), Phase::World);
    assert_eq!(inventory_lines(&lines).len(), 3, "{lines:#?}");
    assert!(
        !lines.iter().any(|l| l.starts_with("selected:")),
        "a held selection is in the inventory: {lines:#?}"
    );
}
