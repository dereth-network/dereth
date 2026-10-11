//! The pack column, the character's slots, the selected tile's mark, cooldowns on tiles, the
//! stack splitter and the salvage window.
//!
//! Behaviour: none (experimental Horizon interface)

use dereth_client_contract::panels::salvage::SalvageAction;
use dereth_client_contract::view::DropTarget;
use dereth_client_contract::UiRequest;
use dereth_horizon::draw::Rect;
use dereth_horizon::ui::game::{GameState, Item, Target};
use dereth_horizon::ui::kit::{Drop, SELECTED_RIM};
use dereth_horizon::ui::panels::WindowId;
use dereth_primitives::ObjectId;

use crate::Harness;

const ME: ObjectId = ObjectId(0x5000_0001);
const POTION: ObjectId = ObjectId(0x8000_0010);
const SACK: ObjectId = ObjectId(0x8000_0012);
const CORPSE: ObjectId = ObjectId(0x8000_0020);
const GEM: ObjectId = ObjectId(0x8000_0021);
const PILE: ObjectId = ObjectId(0x8000_0022);
const HELM: ObjectId = ObjectId(0x8000_0023);

fn item(id: ObjectId, name: &str) -> Item {
    Item {
        id,
        name: name.into(),
        stack: Some(1),
        ..Item::default()
    }
}

fn carrying(side_packs: usize) -> GameState {
    GameState {
        in_world: true,
        name: "Tester".into(),
        player_id: Some(ME),
        main_pack: Some(item(ME, "Tester")),
        pack: vec![item(POTION, "Potion")],
        side_packs: (0..side_packs)
            .map(|n| {
                let id = if n == 0 {
                    SACK
                } else {
                    ObjectId(0x8000_0100 + u32::try_from(n).unwrap())
                };
                (
                    Item {
                        container: true,
                        ..item(id, "Sack")
                    },
                    Vec::new(),
                )
            })
            .collect(),
        ..GameState::default()
    }
}

fn selecting(state: &mut GameState, id: ObjectId, look: Option<Item>) {
    state.target = Some(Target {
        id,
        name: "It".into(),
        look,
        ..Target::default()
    });
}

fn centre(r: Rect) -> (f32, f32) {
    (r.x + r.w / 2.0, r.y + r.h / 2.0)
}

fn zones(h: &Harness, target: impl Fn(&DropTarget) -> bool) -> Vec<Rect> {
    h.ui.drops
        .iter()
        .filter(|(_, t)| matches!(t, Some(Drop::Target(d)) if target(d)))
        .map(|(r, _)| *r)
        .collect()
}

fn window(h: &Harness, id: WindowId) -> Rect {
    h.ui.windows
        .rects(1.0)
        .into_iter()
        .find(|(w, _)| *w == id)
        .map(|(_, r)| r)
        .expect("the window is open")
}

fn open(state: &GameState, windows: &[WindowId]) -> Harness {
    let mut h = Harness::new(Default::default());
    for w in windows {
        h.ui.windows.open(*w, -1.0);
    }
    h.frame(state);
    h.frame(state);
    h
}

fn click(h: &mut Harness, state: &GameState, at: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(at.0, at.1);
    h.press();
    let mut requests = h.frame(state).requests;
    h.release();
    requests.extend(h.frame(state).requests);
    requests
}

/// Whether a quad of `colour` lies within `r`.
fn drawn_in(h: &Harness, r: Rect, colour: u32) -> bool {
    h.list.quads.iter().any(|q| {
        q.colour == colour
            && q.dst.x >= r.x - 0.5
            && q.dst.y >= r.y - 0.5
            && q.dst.right() <= r.right() + 0.5
            && q.dst.bottom() <= r.bottom() + 0.5
    })
}

#[test]
fn the_selected_item_s_tile_is_marked_in_a_corpse_and_in_a_pack() {
    let mut state = carrying(1);
    state.loot = Some((CORPSE, "Corpse of a Drudge".into(), vec![item(GEM, "Gem")]));
    let h = open(&state, &[WindowId::Loot, WindowId::Inventory]);
    let in_corpse = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == CORPSE),
    )[0];
    let in_pack = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
    )[0];
    assert!(!drawn_in(&h, in_corpse, SELECTED_RIM));
    for (id, tile) in [(GEM, in_corpse), (POTION, in_pack)] {
        selecting(&mut state, id, None);
        let mut h = open(&state, &[WindowId::Loot, WindowId::Inventory]);
        h.frame(&state);
        assert!(drawn_in(&h, tile, SELECTED_RIM), "{id:?}");
    }
}

#[test]
fn a_pack_is_dragged_from_the_pack_column_to_the_shortcut_bar() {
    let state = carrying(1);
    let mut h = open(&state, &[WindowId::Inventory]);
    let from = centre(zones(&h, |t| *t == DropTarget::Container(SACK))[0]);
    let to =
        h.ui.hud
            .drop_slots
            .iter()
            .find(|(_, s)| *s == 2)
            .map(|(r, _)| centre(*r))
            .expect("shortcut slot 2 is on screen");
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(to.0, to.1);
    h.frame(&state);
    h.release();
    let requests = h.frame(&state).requests;
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: SACK,
            target: DropTarget::ShortcutSlot(2)
        }),
        "{requests:?}"
    );
}

#[test]
fn the_main_pack_is_dragged_from_the_pack_column_to_the_shortcut_bar() {
    let state = carrying(1);
    let mut h = open(&state, &[WindowId::Inventory]);
    let from = centre(zones(&h, |t| *t == DropTarget::Container(ME))[0]);
    let to =
        h.ui.hud
            .drop_slots
            .iter()
            .find(|(_, s)| *s == 2)
            .map(|(r, _)| centre(*r))
            .expect("shortcut slot 2 is on screen");
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(to.0, to.1);
    h.frame(&state);
    h.release();
    let requests = h.frame(&state).requests;
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: ME,
            target: DropTarget::ShortcutSlot(2)
        }),
        "{requests:?}"
    );
}

/// What a drag from `from` to `to` asks for when it is let go.
fn drag(h: &mut Harness, state: &GameState, from: (f32, f32), to: (f32, f32)) -> Vec<UiRequest> {
    h.move_to(from.0, from.1);
    h.press();
    h.frame(state);
    h.move_to(to.0, to.1);
    h.frame(state);
    h.release();
    h.frame(state).requests
}

/// The middle of pack `id`'s place in the pack column.
fn pack_tile(h: &Harness, id: ObjectId) -> (f32, f32) {
    centre(zones(h, |t| *t == DropTarget::Container(id))[0])
}

/// The drop on the row of packs that puts a pack at `index` among them, on top of `under`.
fn on_the_row_of_packs(under: ObjectId, index: u32) -> DropTarget {
    DropTarget::ItemListSlot {
        container: ME,
        under: Some(under),
        index,
        num_ui_items: 3,
        dragged_is_container: true,
        container_list: true,
    }
}

#[test]
fn a_pack_dragged_along_the_pack_column_takes_the_place_it_is_let_go_on() {
    let state = carrying(3);
    let third = ObjectId(0x8000_0102);
    let mut h = open(&state, &[WindowId::Inventory]);
    let (main_tile, first_tile, third_tile) =
        (pack_tile(&h, ME), pack_tile(&h, SACK), pack_tile(&h, third));
    // Up: the third pack to the first's place, in front of it; on the main pack's place too.
    let up = drag(&mut h, &state, third_tile, first_tile);
    assert!(
        up.contains(&UiRequest::DragDrop {
            item: third,
            target: on_the_row_of_packs(SACK, 0),
        }),
        "{up:?}"
    );
    let to_main = drag(&mut h, &state, third_tile, main_tile);
    assert!(
        to_main.contains(&UiRequest::DragDrop {
            item: third,
            target: on_the_row_of_packs(SACK, 0),
        }),
        "{to_main:?}"
    );
    // Down: the first pack to the third's place, after it once it has left its own.
    let down = drag(&mut h, &state, first_tile, third_tile);
    assert!(
        down.contains(&UiRequest::DragDrop {
            item: SACK,
            target: on_the_row_of_packs(third, 3),
        }),
        "{down:?}"
    );
    // Neither is put in the pack it was let go on.
    for (requests, pack) in [(&up, SACK), (&down, third)] {
        assert!(
            !requests.iter().any(|r| matches!(
                r,
                UiRequest::DragDrop { target: DropTarget::Container(c), .. } if *c == pack
            )),
            "{requests:?}"
        );
    }
}

#[test]
fn a_pack_the_pad_is_moving_is_put_down_on_the_place_pressed() {
    let state = carrying(3);
    let third = ObjectId(0x8000_0102);
    let mut h = open(&state, &[WindowId::Inventory]);
    let first_tile = pack_tile(&h, SACK);
    // Its menu's Move picks it up; the pad's confirm is the left button.
    h.ui.windows.moving_pack = Some(third);
    let requests = click(&mut h, &state, first_tile);
    assert!(
        requests.contains(&UiRequest::DragDrop {
            item: third,
            target: on_the_row_of_packs(SACK, 0),
        }),
        "{requests:?}"
    );
    assert_eq!(h.ui.windows.moving_pack, None, "put down");
}

/// The place among the player's packs the game asks the server to put `moving` at, when the
/// interface puts it down at place `index` of the pack column: the place sent with
/// `Inventory_PutItemInContainer` (`0x0019`), or `None` when nothing is sent.
fn sent_place(state: &GameState, moving: ObjectId, index: usize) -> Option<u32> {
    use dereth_client_model::inventory::{ItemListDrop, SplitState};
    use dereth_client_model::objects::ObjectInventory;
    use dereth_client_model::{RecordingRequests, RecordingSink, Request, Weenie, World};
    use dereth_protocol::types::PublicWeenieDesc;
    use dereth_rules::weenie::{bitfield, item_type};
    let mut w = World::new();
    let mut put = |id: ObjectId, pwd: PublicWeenieDesc| {
        let mut it = Weenie::new(id);
        it.pwd = pwd;
        it.valid = true;
        w.tables.weenies.insert(id, it);
        w.tables.inventories.insert(id, ObjectInventory::new(id));
    };
    put(
        ME,
        PublicWeenieDesc {
            bitfield: bitfield::PLAYER,
            obj_type: item_type::CREATURE,
            items_capacity: Some(102),
            containers_capacity: Some(7),
            ..PublicWeenieDesc::default()
        },
    );
    for (pack, _) in &state.side_packs {
        put(
            pack.id,
            PublicWeenieDesc {
                container_id: Some(ME),
                items_capacity: Some(24),
                bitfield: bitfield::OPENABLE,
                ..PublicWeenieDesc::default()
            },
        );
    }
    w.player = Some(ME);
    for (n, (pack, _)) in state.side_packs.iter().enumerate() {
        w.tables
            .inventories
            .get_mut(ME)
            .expect("the player's own list")
            .add_content(pack.id, true, n);
    }
    let DropTarget::ItemListSlot {
        container,
        under,
        index,
        num_ui_items,
        dragged_is_container,
        container_list,
    } = dereth_horizon::ui::panels::pack_place(state, moving, index)
    else {
        panic!("a drop on the row of packs");
    };
    let mut sent = RecordingRequests::default();
    w.item_list_accept_drag(
        &mut sent,
        &mut RecordingSink::default(),
        moving,
        ItemListDrop {
            parent_container: container,
            under,
            index,
            num_ui_items,
            dragged_is_container,
            container_list,
        },
        SplitState::whole_stack(1),
        dereth_primitives::ServerTime(1.0),
    );
    sent.0.iter().find_map(|r| match r {
        Request::PutItemInContainer(m) if m.item == moving && m.container == ME => Some(m.slot),
        _ => None,
    })
}

#[test]
fn the_game_puts_a_pack_moved_along_the_column_in_the_place_it_was_put_down_on() {
    let state = carrying(3);
    let packs: Vec<ObjectId> = state.side_packs.iter().map(|(p, _)| p.id).collect();
    for from in 0..packs.len() {
        // Place 0 of the column is the main pack's, which stands for the first among the packs.
        for to in 0..=packs.len() {
            let wanted = to.saturating_sub(1);
            let place = sent_place(&state, packs[from], to);
            if wanted == from {
                assert_eq!(place, None, "pack {from} put back on its own place");
                continue;
            }
            // The server takes the pack out of its place and puts it in at the place sent.
            let mut order = packs.clone();
            let moved = order.remove(from);
            let place = usize::try_from(place.expect("a place is sent")).unwrap();
            order.insert(place.min(order.len()), moved);
            assert_eq!(
                order.iter().position(|p| *p == moved),
                Some(wanted),
                "pack {from} put down on column place {to}: {order:?}"
            );
        }
    }
}

#[test]
fn closing_a_corpse_or_the_inventory_lets_go_of_a_selection_inside_it() {
    let let_go = UiRequest::Select(ObjectId(0));
    // The game closes the corpse with its gem selected.
    let mut state = carrying(0);
    state.loot = Some((CORPSE, "Corpse of a Drudge".into(), vec![item(GEM, "Gem")]));
    selecting(&mut state, GEM, None);
    let mut h = open(&state, &[]);
    assert!(!h.frame(&state).requests.contains(&let_go));
    state.loot = None;
    let closed = h.frame(&state).requests;
    assert!(closed.contains(&let_go), "the corpse closed: {closed:?}");
    // The inventory closed with its potion selected.
    let mut state = carrying(0);
    selecting(&mut state, POTION, None);
    let mut h = open(&state, &[WindowId::Inventory]);
    assert!(!h.frame(&state).requests.contains(&let_go));
    h.ui.windows.close(WindowId::Inventory);
    assert!(
        h.frame(&state).requests.contains(&let_go),
        "the inventory closed"
    );
    // A selection elsewhere stays.
    selecting(&mut state, PILE, None);
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    h.ui.windows.close(WindowId::Inventory);
    assert!(
        !h.frame(&state).requests.contains(&let_go),
        "the pile is not in it"
    );
}

#[test]
fn a_focus_opened_in_the_inventory_draws_no_slots() {
    let mut state = carrying(1);
    state.side_packs[0].0.decoration = Some(dereth_client_contract::view::SlotDecoration {
        is_container: true,
        items_capacity: 0,
        ..Default::default()
    });
    let mut h = open(&state, &[WindowId::Inventory]);
    let tile = centre(zones(&h, |t| *t == DropTarget::Container(SACK))[0]);
    click(&mut h, &state, tile);
    h.frame(&state);
    assert!(
        zones(
            &h,
            |t| matches!(t, DropTarget::ItemListSlot { container, .. } if *container == SACK)
        )
        .is_empty(),
        "no slots in a focus"
    );
}

#[test]
fn a_thing_the_game_is_still_busy_with_is_dimmed_on_its_tile() {
    let mut state = carrying(0);
    let h = open(&state, &[WindowId::Inventory]);
    let tile = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
    )[0];
    assert!(!drawn_in(&h, tile, 0xA010_0C08));
    state.pack[0].decoration = Some(dereth_client_contract::view::SlotDecoration {
        waiting: true,
        ..Default::default()
    });
    let h = open(&state, &[WindowId::Inventory]);
    assert!(drawn_in(&h, tile, 0xA010_0C08));
}

#[test]
fn a_salvage_bag_s_tile_shows_how_full_it_is() {
    let mut state = carrying(0);
    state.pack[0].decoration = Some(dereth_client_contract::view::SlotDecoration {
        structure: 50,
        max_structure: 100,
        ..Default::default()
    });
    let h = open(&state, &[WindowId::Inventory]);
    let tile = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
    )[0];
    assert!(drawn_in(&h, tile, 0xFF7C_D8F8));
    state.pack[0].decoration = None;
    let h = open(&state, &[WindowId::Inventory]);
    assert!(!drawn_in(&h, tile, 0xFF7C_D8F8));
}

#[test]
fn a_click_on_a_pack_opens_its_grid_and_every_one_of_eight_is_in_the_window() {
    let state = carrying(7);
    let mut h = open(&state, &[WindowId::Inventory]);
    let inventory = window(&h, WindowId::Inventory);
    let mut packs = vec![SACK];
    packs.extend((1..7).map(|n| ObjectId(0x8000_0100 + n)));
    for pack in &packs {
        let tile = zones(&h, |t| *t == DropTarget::Container(*pack));
        assert_eq!(tile.len(), 1, "{pack:?} has one place in the column");
        assert!(
            tile[0].x >= inventory.x
                && tile[0].right() <= inventory.right()
                && tile[0].bottom() <= inventory.bottom(),
            "{pack:?} at {:?} is inside the window {inventory:?}",
            tile[0]
        );
    }
    let last = *packs.last().unwrap();
    let tile = zones(&h, |t| *t == DropTarget::Container(last))[0];
    click(&mut h, &state, centre(tile));
    assert!(
        !zones(
            &h,
            |t| matches!(t, DropTarget::ItemListSlot { container, .. } if *container == last)
        )
        .is_empty(),
        "the last pack's grid is open"
    );
}

#[test]
fn a_filled_slot_names_the_slot_and_what_is_in_it() {
    let mut state = carrying(0);
    state.equipped = vec![Item {
        worn: dereth_rules::slots::loc::HEAD_WEAR,
        ..item(HELM, "Iron Helm")
    }];
    let mut h = open(&state, &[WindowId::Character]);
    let character = window(&h, WindowId::Character);
    // The head slot is over the figure, in the middle of the figure's column: found going down
    // that column from the top of the window's body.
    let body = Rect::new(
        character.x + 12.0,
        character.y + 46.0,
        character.w - 24.0,
        character.h - 56.0,
    );
    let head = Some(("Head".to_owned(), vec!["Iron Helm".to_owned()]));
    let found = (0..80).any(|step| {
        #[allow(clippy::cast_precision_loss)]
        h.move_to(body.x + 192.0, body.y + 4.0 * step as f32);
        h.frame(&state);
        h.ui.windows.tip() == head.as_ref()
    });
    assert!(found, "the head slot names the helm");
    state.equipped.clear();
    h.frame(&state);
    assert_eq!(
        h.ui.windows.tip(),
        Some(&("Head".to_owned(), vec!["Empty".to_owned()]))
    );
}

#[test]
fn the_paper_doll_stands_in_the_character_window_where_a_drop_is_worn() {
    let state = carrying(0);
    let h = open(&state, &[WindowId::Character]);
    let character = window(&h, WindowId::Character);
    let (doll, _) = h.ui.windows.doll.expect("the doll is placed");
    let canvas = zones(&h, |t| *t == DropTarget::EquipCanvas)[0];
    for r in [character, canvas] {
        assert!(
            doll.x >= r.x
                && doll.right() <= r.right()
                && doll.y >= r.y
                && doll.bottom() <= r.bottom(),
            "{doll:?} in {r:?}"
        );
    }
    assert_eq!(WindowId::from_name("armoury"), None);
}

#[test]
fn an_item_cooling_down_shows_the_sweep_on_its_tile() {
    let mut state = carrying(0);
    state.pack[0].cooldown = Some((5.0, 10.0));
    let h = open(&state, &[WindowId::Inventory]);
    let tile = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
    )[0];
    // With no art, the sweep is a dark wash over the tile.
    assert!(drawn_in(&h, tile, 0x9000_0000));
    state.pack[0].cooldown = None;
    let h = open(&state, &[WindowId::Inventory]);
    assert!(!drawn_in(&h, tile, 0x9000_0000));
}

#[test]
fn an_item_whose_cooldown_the_game_still_holds_after_its_time_keeps_the_sweep() {
    let mut state = carrying(0);
    state.pack[0].cooldown = Some((0.0, 10.0));
    let h = open(&state, &[WindowId::Inventory]);
    let tile = zones(
        &h,
        |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
    )[0];
    assert!(drawn_in(&h, tile, 0x9000_0000));
}

#[test]
fn a_stack_on_the_ground_is_split_on_the_inventory_s_slider() {
    let mut state = carrying(0);
    selecting(
        &mut state,
        PILE,
        Some(Item {
            stack: Some(50),
            ..item(PILE, "Pyreal")
        }),
    );
    let mut h = Harness::new(Default::default());
    h.ui.windows.open(WindowId::Inventory, -1.0);
    let seeded = h.frame(&state).requests;
    assert!(
        seeded.contains(&UiRequest::StackSliderChanged { split: 50, max: 50 }),
        "{seeded:?}"
    );
    h.frame(&state);
    // The slider's left end, in the splitter at the inventory's foot.
    let inventory = window(&h, WindowId::Inventory);
    let requests = click(
        &mut h,
        &state,
        (inventory.x + 66.0, inventory.bottom() - 62.0),
    );
    assert!(
        requests.contains(&UiRequest::StackSliderChanged { split: 1, max: 50 }),
        "{requests:?}"
    );
}

#[test]
fn a_stack_on_the_ground_is_split_under_the_target_bar_while_the_inventory_is_closed() {
    let mut state = carrying(0);
    selecting(
        &mut state,
        PILE,
        Some(Item {
            stack: Some(50),
            ..item(PILE, "Pyreal")
        }),
    );
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.frame(&state);
    // The slider's left end, in the box right of the parameter bar.
    let r = dereth_horizon::ui::panels::Windows::ground_split_rect(1.0, (1920.0, 1080.0));
    let requests = click(&mut h, &state, (r.x + 54.0, r.y + 20.0));
    assert!(
        requests.contains(&UiRequest::StackSliderChanged { split: 1, max: 50 }),
        "{requests:?}"
    );
    // With the inventory open, the inventory's splitter takes it and the box is gone.
    h.ui.windows.open(WindowId::Inventory, -1.0);
    h.frame(&state);
    let requests = click(&mut h, &state, (r.x + 54.0, r.y + 20.0));
    assert!(
        !requests
            .iter()
            .any(|r| matches!(r, UiRequest::StackSliderChanged { .. })),
        "{requests:?}"
    );
}

#[test]
fn salvaging_hands_the_list_to_the_game_s_salvage_session() {
    let mut state = carrying(0);
    state.salvage = Some((ObjectId(0x8000_0099), vec![item(GEM, "Gem")]));
    let mut h = open(&state, &[]);
    let salvage = window(&h, WindowId::Salvage);
    let requests = click(
        &mut h,
        &state,
        (salvage.right() - 80.0, salvage.bottom() - 28.0),
    );
    assert!(
        requests.contains(&UiRequest::SalvageList(SalvageAction::Submit)),
        "{requests:?}"
    );
}

#[test]
fn an_item_dragged_over_the_vendor_turns_it_to_selling() {
    let mut state = carrying(0);
    state.shop = Some(dereth_client_contract::view::ShopView {
        open: true,
        ..Default::default()
    });
    let mut h = open(&state, &[WindowId::Inventory, WindowId::Vendor]);
    let sells = |h: &Harness| {
        h.ui.drops
            .iter()
            .any(|(_, t)| matches!(t, Some(Drop::Sell)))
    };
    assert!(!sells(&h), "the vendor opens on its stock");
    let from = centre(
        zones(
            &h,
            |t| matches!(t, DropTarget::ItemListSlot { container, index: 0, .. } if *container == ME),
        )[0],
    );
    // A point of the vendor's window clear of the inventory's.
    let vendor = window(&h, WindowId::Vendor);
    let inventory = window(&h, WindowId::Inventory);
    let over = [0.1_f32, 0.5, 0.9]
        .iter()
        .flat_map(|fx| {
            [0.3_f32, 0.6, 0.9].map(|fy| (vendor.x + vendor.w * fx, vendor.y + vendor.h * fy))
        })
        .find(|(x, y)| !inventory.contains(*x, *y))
        .expect("the vendor shows past the inventory");
    h.move_to(from.0, from.1);
    h.press();
    h.frame(&state);
    h.move_to(over.0, over.1);
    h.frame(&state);
    h.frame(&state);
    assert!(sells(&h), "the sell list takes the drop");
}
