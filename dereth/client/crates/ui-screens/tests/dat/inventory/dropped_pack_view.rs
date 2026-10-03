//! The main pack slot owns the player as its open item from the first fill and gives it up when a
//! side pack is opened; a pack dropped while viewed hands the grid back to the player's own
//! things; a pack dropped while the main pack is open leaves the grid alone.
//! Fixture: shipped layouts, strings and keymaps loaded from the retail DATs.

use crate::common::layout::RegistrationOrder;

use dereth_primitives::{DataId, ObjectId};
use dereth_ui::{ElementId, Screen, UiSystem};
use dereth_ui_screens::panels::inventory::InventoryPanels;
use dereth_ui_screens::view::{GameView, SlotDecoration};

const PLAYER: ObjectId = ObjectId(0x5000_1000);
/// The side pack the player opens and then drops.
const PACK: ObjectId = ObjectId(0x5000_6000);
/// Where a dropped object goes: out of the player and into nothing.
const NOWHERE: ObjectId = ObjectId(0);

/// Contained-item and contained-container queries over a world small
/// enough to state: the player's loose items, his side packs, and the pack's own contents.
#[derive(Debug)]
struct World {
    open: Option<ObjectId>,
    loose: Vec<ObjectId>,
    packs: Vec<ObjectId>,
    in_pack: Vec<ObjectId>,
    /// The pack still exists as an object after the drop; it is simply no longer the player's.
    pack_is_the_players: bool,
}

impl World {
    fn new() -> Self {
        Self {
            open: Some(PLAYER),
            loose: (0..4).map(|i| ObjectId(0x5000_7000 + i)).collect(),
            packs: vec![PACK],
            in_pack: (0..3).map(|i| ObjectId(0x5000_8000 + i)).collect(),
            pack_is_the_players: true,
        }
    }

    /// The one move this file makes: the pack leaves the player and lands in nothing, which is
    /// what the `0x0024 Item_ServerSaysRemove` removal applies when a container is dropped.
    fn drop_the_pack(&mut self) {
        self.packs.retain(|p| *p != PACK);
        self.pack_is_the_players = false;
        if self.open == Some(PACK) {
            self.open = Some(PLAYER);
        }
    }
}

impl GameView for World {
    fn player(&self) -> Option<ObjectId> {
        Some(PLAYER)
    }
    fn open_inventory_container(&self) -> Option<ObjectId> {
        self.open
    }
    fn container_contents(&self, id: ObjectId) -> &[ObjectId] {
        if id == PACK {
            &self.in_pack
        } else if id == PLAYER {
            &self.loose
        } else {
            &[]
        }
    }
    fn contained_containers(&self, id: ObjectId) -> &[ObjectId] {
        if id == PLAYER {
            &self.packs
        } else {
            &[]
        }
    }
    fn items_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(102)
    }
    fn containers_capacity(&self, _: ObjectId) -> Option<i32> {
        Some(7)
    }
    fn icon(&self, _: ObjectId) -> Option<DataId> {
        Some(DataId(0x0600_13A5))
    }
    fn slot_decoration(&self, id: ObjectId) -> Option<SlotDecoration> {
        Some(SlotDecoration {
            is_player: id == PLAYER,
            is_container: id == PACK,
            obj_type: if id == PACK { 0x200 } else { 0x10 },
            icon_id: 0x0600_13A5,
            items_capacity: 102,
            ..SlotDecoration::default()
        })
    }
}

fn env() -> (UiSystem, InventoryPanels, World) {
    let (mut ui, _flow, _store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut screen = dereth_ui_screens::screens::gameplay::GamePlayScreen::default();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the shipped gameplay tree");
    let page = ui
        .get_child_recursive(screen.root().expect("a root"), ElementId(0x1000_018B))
        .expect("the inventory page 0x1000018B");
    ui.set_visible(page, true);
    let mut panel = InventoryPanels::default();
    panel.post_init(&mut ui, page);
    let world = World::new();
    assert!(
        panel.update(&mut ui, &world),
        "the first fill changed something"
    );
    (ui, panel, world)
}

/// What the bottom grid is showing, in list order.
fn grid(panel: &InventoryPanels) -> Vec<ObjectId> {
    panel
        .item_list
        .as_ref()
        .map(|w| w.slots.iter().filter_map(|s| s.item).collect())
        .unwrap_or_default()
}

/// Which of the two container strips a click lands on.
#[derive(Clone, Copy)]
enum Strip {
    /// The one-slot main-pack list, holding the player.
    MainPack,
    /// The side-pack strip.
    SidePacks,
}

/// Open a container the way a click does — the item list's `0x1C`/action-7 message arm, through
/// the strip's own slot element.
fn click(
    ui: &mut UiSystem,
    panel: &mut InventoryPanels,
    world: &mut World,
    strip: Strip,
    id: ObjectId,
) {
    let list = match strip {
        Strip::MainPack => panel.top_container.as_ref(),
        Strip::SidePacks => panel.container_list.as_ref(),
    };
    let h = list
        .and_then(|w| {
            w.slots
                .iter()
                .find(|s| s.item == Some(id))
                .map(|s| s.handle)
        })
        .unwrap_or_else(|| panic!("no live strip slot holds {id:?}"));
    assert_eq!(
        panel.on_slot_clicked(&mut ui.requests, h),
        Some(id),
        "the click did not open {id:?}"
    );
    for request in ui.requests.take() {
        if let dereth_ui_screens::UiRequest::NewParentContainer(id) = request {
            world.open = Some(id);
        }
    }
}

/// The open item of each strip, main pack first.
fn open_items(panel: &InventoryPanels) -> (Option<ObjectId>, Option<ObjectId>) {
    (
        panel.top_container.as_ref().and_then(|w| w.open_item_id),
        panel.container_list.as_ref().and_then(|w| w.open_item_id),
    )
}

// ================================================================================================

/// **The display-inventory notice's tail — the top container's `open_item_id` is the player from
/// login complete, with no click.** The notice flushes the top container, adds the player to it and
/// then opens the top container on the player, and that open is what writes `open_item_id`.
#[test]
fn the_main_pack_slot_owns_the_player_as_its_open_item_from_the_first_fill() {
    let (_ui, panel, _w) = env();
    let top = panel
        .top_container
        .as_ref()
        .expect("top container 0x100001C9");
    assert_eq!(
        top.slots.iter().filter_map(|s| s.item).collect::<Vec<_>>(),
        vec![PLAYER],
        "the top container's one slot is the player"
    );
    assert_eq!(
        open_items(&panel),
        (Some(PLAYER), None),
        "the main pack opens on the player at the first fill, and the strip is untouched"
    );
}

/// Behaviour: inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things
///
/// A side pack opened from the strip, then dropped while you are looking inside it: the grid goes
/// back to the player's own loose items.
///
/// The click re-parents the grid under the strip, which clears the main pack's open item; the
/// move-item notice then asks the main pack to open its first item, and with its open item clear
/// that is not an early-out, so the player is opened again. Falsified by a click that leaves the
/// main pack's open item set, and by a fill that re-seeds it after the click.
#[test]
fn a_pack_dropped_while_you_are_looking_inside_it_hands_the_grid_back_to_the_player() {
    let (mut ui, mut panel, mut w) = env();
    assert_eq!(
        grid(&panel),
        w.loose,
        "the grid starts on the player's loose items"
    );

    click(&mut ui, &mut panel, &mut w, Strip::SidePacks, PACK);
    panel.update(&mut ui, &w);
    // A second fill with nothing changed: the display-inventory tail must not run again.
    panel.update(&mut ui, &w);
    assert_eq!(
        panel.open_container,
        Some(PACK),
        "the click opened the pack"
    );
    assert_eq!(grid(&panel), w.in_pack, "and the grid shows its contents");
    assert_eq!(
        open_items(&panel),
        (None, Some(PACK)),
        "the strip holds the open item, and the main pack gave its own up when the grid left it"
    );

    w.drop_the_pack();
    assert!(
        !w.contained_containers(PLAYER).contains(&PACK),
        "the pack left the inventory"
    );
    let _ = NOWHERE;
    panel.update(&mut ui, &w);

    assert_eq!(
        panel.open_container,
        Some(PLAYER),
        "the main pack reopened the player"
    );
    assert_eq!(
        grid(&panel),
        w.loose,
        "the grid shows the player's own loose items again"
    );
    assert_eq!(
        panel.item_list.as_ref().and_then(|x| x.parent_container),
        Some(PLAYER),
        "and it is parented to the player"
    );
    assert_eq!(
        open_items(&panel),
        (Some(PLAYER), None),
        "the main pack holds the open item again, and the strip gave its own up"
    );
}

/// A side pack dropped while the main pack is the one open leaves the grid where it was: the
/// moved object is not the grid's parent container, so the move-item notice does nothing.
/// Clicking the main pack after a side pack takes the open item back from the strip.
#[test]
fn a_pack_dropped_while_the_main_pack_is_open_leaves_the_grid_alone() {
    let (mut ui, mut panel, mut w) = env();
    click(&mut ui, &mut panel, &mut w, Strip::SidePacks, PACK);
    panel.update(&mut ui, &w);
    click(&mut ui, &mut panel, &mut w, Strip::MainPack, PLAYER);
    panel.update(&mut ui, &w);
    assert_eq!(panel.open_container, Some(PLAYER));
    assert_eq!(grid(&panel), w.loose);
    assert_eq!(
        open_items(&panel),
        (Some(PLAYER), None),
        "the main pack took the open item back from the strip"
    );

    w.drop_the_pack();
    panel.update(&mut ui, &w);
    assert_eq!(panel.open_container, Some(PLAYER), "nothing moved the grid");
    assert_eq!(grid(&panel), w.loose);
    assert_eq!(open_items(&panel), (Some(PLAYER), None));
}

/// Behaviour: inventory.pack.a-pack-that-leaves-the-player-hands-the-grid-back-to-the-players-own-things
#[test]
fn a_recreated_inventory_panel_resumes_the_shared_open_pack() {
    let (mut ui, _panel, mut world) = env();
    world.open = Some(PACK);
    let mut rebuilt = InventoryPanels::default();
    let page = ui
        .get_element(ElementId(0x1000_018B))
        .expect("the inventory page is bound");
    rebuilt.post_init(&mut ui, page);
    rebuilt.update(&mut ui, &world);
    assert_eq!(rebuilt.open_container, Some(PACK));
    assert_eq!(grid(&rebuilt), world.in_pack);
    assert_eq!(open_items(&rebuilt), (None, Some(PACK)));
}
