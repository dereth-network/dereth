//! The backpack grid: every filled cell's icon composite carries the opaque item-type background
//! tile (row lowest-set-bit(type)+1, or `0x21` for a zero mask, of the `UIIconBackgrounds` group
//! `0x10000004`; the local player uses the container row), empty cells carry none and show frame
//! surface `0x06004D20` through state `0x1000001C`, and the tiles reach the pixels inside the grid
//! only. The grid's length is the open container's capacity (`FixedListSize`), not its item count:
//! a 102-slot pack shows 18 cells (six by three 32x32 cells inside 192x96) and scrolls the rest,
//! and opening a side pack resizes the grid and clears every former composite.
//! Fixture: every recording on disk replayed into `Hud` and a GPU-less gameplay screen built from
//! the retail dats; the pixel test uses two headless `App`s run the same number of frames.

#![cfg(gpu)]

use crate::common::client_dir;
use crate::common::{recorded_sessions, recorded_world_sessions};

use std::collections::BTreeMap;
use std::rc::Rc;

use dereth_client::app::App;
use dereth_client_net::client_session::testing::{session_names, shared_session};
use dereth_client_net::client_session::SessionEvent;
use dereth_client_net::recording::connection_sequence_number;
use dereth_client_runtime::config::Config;
use dereth_client_runtime::net::ClientNetwork;
use dereth_client_runtime::objects::ObjectStream;
use dereth_client_shell::hud::Hud;
use dereth_primitives::{AssetSource, DataId, LocalTime, ObjectId};
use dereth_ui::framework::Screen;
use dereth_ui::{Box2D, ElementId, UiSystem};
use dereth_ui_screens::items::widget::icon_background;
use dereth_ui_screens::panels::inventory::ITEM_LIST;
use dereth_ui_screens::screens::gameplay::{window::INVENTORY_PAGE, GamePlayScreen};

// =================================================================================================
// The capture reader.
// =================================================================================================

/// Every recording the corpus index names, in name order.
fn corpus_sessions() -> Vec<String> {
    let mut out: Vec<String> = session_names().iter().map(|s| (*s).to_owned()).collect();
    out.sort();
    out
}

fn replay_to(
    session: &str,
    limit: usize,
    mut hud: Option<&mut Hud>,
) -> (ObjectStream, Vec<SessionEvent>) {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut events = Vec::new();
    for r in records.iter().take(limit) {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        let batch = objects.pump(&mut net, now);
        for e in &batch {
            if let SessionEvent::CharacterSet(set) = e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        if let Some(hud) = hud.as_deref_mut() {
            hud.now = now;
            let _ = hud.apply_events(&batch, &mut objects.world);
        }
        events.extend(batch);
    }
    (objects, events)
}

/// Return the replay limit immediately after the first maximum object population. Sampling
/// the end can miss the player after logoff and destruction; not every recording is assumed
/// to end that way. This measures a populated instant rather than an empty final scene.
fn busiest_instant(session: &str) -> usize {
    let records = shared_session(session);
    let mut net = ClientNetwork::new(
        "127.0.0.1:19000",
        7304,
        "ac01",
        "pass",
        connection_sequence_number(records).expect("the capture has no LoginRequest"),
    )
    .expect("host");
    let mut objects = ObjectStream::new();
    let mut entered = false;
    let mut best = (0usize, 1usize);
    for (i, r) in records.iter().enumerate() {
        let now = LocalTime(r.t);
        if !r.c2s {
            net.feed(&r.raw, r.peer(), now);
        }
        net.tick(now);
        let _ = net.take_outgoing();
        for e in objects.pump(&mut net, now) {
            if let SessionEvent::CharacterSet(set) = &e {
                if !entered {
                    if let Some(c) = set.characters.first() {
                        let account = set.account.clone();
                        net.enter_world(c.gid, &account);
                        entered = true;
                    }
                }
            }
        }
        let n = objects.world.tables.weenies.len();
        if n > best.0 {
            best = (n, i + 1);
        }
    }
    best.1
}

/// Replay to the populated instant, applying HUD events as each datagram arrives. Login pack
/// contents must precede later item moves, rather than overwrite them after replay completes.
fn scene(session: &str) -> (ObjectStream, Hud) {
    let mut hud = Hud::new();
    let (objects, _) = replay_to(session, busiest_instant(session), Some(&mut hud));
    hud.sync(&objects, None);
    (objects, hud)
}

// =================================================================================================
// Gameplay elements created from the shipped layout, without a GPU.
// =================================================================================================

fn open_store() -> dereth_dat::RetailDatStore {
    crate::common::dat_store()
}

fn shipped_gameplay() -> (UiSystem, Box<dyn Screen>) {
    use dereth_ui::framework::DidMapperResolver;
    let store = open_store();
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("decode MasterProperty");
    let mut ui = UiSystem::new((800, 600));
    ui.property_types = master.property_types();
    let mut flow = dereth_ui::UiFlow::new();
    dereth_ui_screens::register_all(&mut ui, &mut flow);
    let store = Rc::new(store);
    let resolver =
        Rc::new(DidMapperResolver::load_via_master(store.as_ref()).expect("the DidMapper"));
    dereth_ui_screens::env::install(&mut ui, store, resolver);
    let mut screen = GamePlayScreen::create_screen();
    screen
        .create(&mut dereth_ui::framework::ScreenCx::new(&mut ui))
        .expect("the gameplay screen is created from its real layout");
    (ui, screen)
}

fn as_gameplay(screen: &mut Box<dyn Screen>) -> &mut GamePlayScreen {
    let any: &mut dyn std::any::Any = &mut **screen;
    any.downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen")
}

/// The whole seam for one session: capture → `ObjectStream` → `Hud` → the live element tree.
struct Bench {
    ui: UiSystem,
    screen: Box<dyn Screen>,
    objects: ObjectStream,
    hud: Hud,
}

impl Bench {
    fn open(session: &str) -> Self {
        let (mut ui, screen) = shipped_gameplay();
        let (objects, hud) = scene(session);
        ui.requests.clear();
        let mut b = Self {
            ui,
            screen,
            objects,
            hud,
        };
        b.hud
            .drive(&mut b.ui, as_gameplay(&mut b.screen), 1, &b.objects);
        b
    }

    fn grid(&mut self) -> &dereth_ui_screens::items::widget::ItemListWidget {
        as_gameplay(&mut self.screen)
            .inventory
            .item_list
            .as_ref()
            .expect("the item-grid list is in the shipped layout")
    }

    /// Every live slot of every one of the inventory page's lists, as `(item, background did)`.
    ///
    /// The tile is the background field of the icon child's composite recipe. The checks use the
    /// discovered corpus and explicit lower bounds rather than a fixed census.
    fn cells(&mut self) -> Vec<(Option<ObjectId>, Option<DataId>)> {
        let g = as_gameplay(&mut self.screen);
        let p = &g.inventory;
        let mut out = Vec::new();
        let push = |w: &dereth_ui_screens::items::widget::ItemListWidget,
                    ui: &UiSystem,
                    out: &mut Vec<(Option<ObjectId>, Option<DataId>)>| {
            for s in &w.slots {
                let bg = match s.icon_recipe(ui) {
                    Some(dereth_ui::region::IconRecipe::Object { background, .. }) => background,
                    _ => None,
                };
                out.push((s.item, bg));
            }
        };
        for w in p
            .top_container
            .iter()
            .chain(p.container_list.iter())
            .chain(p.item_list.iter())
        {
            push(w, &self.ui, &mut out);
        }
        for (_, w) in &p.doll {
            push(w, &self.ui, &mut out);
        }
        out
    }
}

// =================================================================================================
// 1. The arithmetic
// =================================================================================================

/// The mapper index is lowest-set-bit(type)+1, with zero-bit-search returning -1.
/// Consequently only a zero mask takes fallback 0x21; every nonzero mask has a bit position.
/// Container bit 9 must use row 10, not the fallback. Removing the +1 maps melee bit 0 to the
/// mapper's zero row and fails the first expected index.
#[test]
fn the_type_index_is_lowest_set_bit_plus_one_with_the_clients_zero_fallback() {
    assert_eq!(
        icon_background::enum_index(0x0000_0001),
        1,
        "MELEE_WEAPON is bit 0"
    );
    assert_eq!(
        icon_background::enum_index(0x0000_0002),
        2,
        "ARMOR is bit 1"
    );
    assert_eq!(
        icon_background::enum_index(0x0000_0040),
        7,
        "MONEY is bit 6"
    );
    assert_eq!(
        icon_background::enum_index(0x0000_0200),
        10,
        "CONTAINER is bit 9"
    );
    assert_eq!(
        icon_background::enum_index(0x8000_0000),
        32,
        "the top bit is row 32"
    );
    assert_eq!(
        icon_background::enum_index(0),
        icon_background::DEFAULT_INDEX
    );
    assert_eq!(icon_background::DEFAULT_INDEX, 0x21);
    // A mask with several bits takes the *lowest*, which is the whole point of the function's
    // name: a two-handed weapon is `MELEE_WEAPON | ...` and draws on the melee tile.
    assert_eq!(icon_background::enum_index(0x0000_0101), 1);
}

/// Resolve the public UIIconBackgrounds group via the master DidMapper and read its rows.
/// No expected surface id is hardcoded: require 34 rows (zero, 32 bits, fallback), RenderSurface
/// type 0x06 for every nonzero bit and the fallback, and at least 10 distinct surface ids.
/// An unrelated group can fail the row-count check; no assertion here proves globally that
/// no other mapper has 34 rows.
#[test]
fn the_uiiconbackgrounds_group_answers_a_render_surface_for_every_item_type_bit() {
    let store = open_store();
    let master = <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
        dereth_assets::MASTER_DID_MAPPER,
        &store
            .read(dereth_assets::MASTER_DID_MAPPER)
            .expect("the master DidMapper"),
    )
    .expect("decode");
    let second = master
        .enum_to_id
        .iter()
        .find(|(k, _)| *k == icon_background::ITEM_TYPE_GROUP)
        .map(|(_, v)| DataId(*v))
        .expect("the master mapper names UIIconBackgrounds");
    let rows = <dereth_assets::DidMapper as dereth_assets::Decode>::decode_payload(
        second,
        &store.read(second).expect("the UIIconBackgrounds mapper"),
    )
    .expect("decode");
    assert_eq!(
        rows.enum_to_id.len(),
        34,
        "32 bit rows, the 0x21 fallback and the zero row"
    );

    let mut surfaces = std::collections::BTreeSet::new();
    for bit in 0..32u32 {
        let idx = icon_background::enum_index(1 << bit);
        let did = dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, idx)
            .unwrap_or_else(|| panic!("bit {bit} -> row {idx:#X} resolves nothing"));
        assert_eq!(
            did.0 >> 24,
            0x06,
            "row {idx:#X} is not a RenderSurface: {did:?}"
        );
        surfaces.insert(did.0);
    }
    let fallback = dereth_assets::did_by_enum(
        &store,
        icon_background::ITEM_TYPE_GROUP,
        icon_background::DEFAULT_INDEX,
    )
    .expect("the 0x21 fallback resolves");
    assert_eq!(fallback.0 >> 24, 0x06);
    surfaces.insert(fallback.0);
    println!(
        "UIIconBackgrounds answers {} distinct surfaces across 32 item-type bits + the fallback",
        surfaces.len()
    );
    assert!(
        surfaces.len() >= 10,
        "the tiles are supposed to differ by item type; {} distinct is not a palette",
        surfaces.len()
    );
}

// =================================================================================================
// 2. The grid's length
// =================================================================================================

/// Behaviour: inventory.pack.the-grid-draws-as-many-slots-as-the-container-can-hold
///
/// Capacity, not item count, determines total slots. Fixed-list resizing copies the container's
/// capacity into the fixed-size property; GameView::items_capacity supplies the value to
/// ItemListWidget::set_contents.
///
/// A 102-slot player pack displays 18 cells inside 192x96 using 32x32 cells, as retail does. Below, every replay reaching a player must match its own
/// decoded capacity and have more slots than items. A separate test checks visible geometry.
/// Dropping the capacity argument leaves the layout's default or dynamic empty-slot behavior;
/// the side-pack test below distinguishes a real update from the player's matching 102 default.
#[test]
fn the_grid_runs_to_the_containers_own_capacity_and_not_to_the_last_item() {
    let mut with_player = 0;
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        let Some(p) = b.objects.world.player else {
            println!("{session} never reached 0x0013 and carries no player");
            continue;
        };
        let capacity = i32::from(
            b.objects
                .world
                .weenie(p)
                .expect("the player has a weenie")
                .pwd
                .items_capacity
                .expect(
                    "the capture's 0x0013 carries the player's own items capacity, which is this \
                 test's oracle",
                ),
        );
        let items = b.objects.world.inventory(p).map_or(0, |i| i.items.len());
        let g = b.grid();
        let (cells, fixed, cell) = (g.slots.len(), g.fixed_list_size, g.cell);
        assert_eq!(
            fixed, capacity,
            "{session}: FixedListSize is the container's items capacity"
        );
        assert_eq!(
            cells,
            usize::try_from(capacity).expect("a positive capacity"),
            "{session}: the grid creates one cell per unit of capacity"
        );
        assert!(
            cells > items,
            "{session}: {cells} cells for {items} items is not 'stops at the last item'"
        );
        assert_eq!(cell, (32, 32), "{session}: the ItemSlot root is 32x32");
        println!(
            "{session} -- items capacity {capacity}, {cells} cells created, {items} of them \
             filled; the grid does NOT stop at the last item"
        );
        with_player += 1;
    }
    // The denominator is the corpus's own count of the recordings carrying a `0x0013`,
    // read by `dereth_client_net::client_session::testing`; the observed side above is the client reaching one.
    assert_eq!(
        with_player,
        recorded_world_sessions(),
        "every capture that reaches 0x0013 was walked; the count is the denominator"
    );
}

/// The other half of the same measurement: how many of those cells are inside the list's own box.
///
/// This is the number the retail frame shows: 18, so a grid of any capacity is the same length on
/// screen.
#[test]
fn eighteen_of_the_grids_cells_fall_inside_the_box_retail_draws_eighteen_in() {
    let mut b = Bench::open("first-login-walk-jump");
    let root = *Screen::roots(&*b.screen).first().expect("root");
    let h =
        b.ui.get_child_recursive(root, ITEM_LIST)
            .expect("the inventory grid is in the layout");
    let box_ =
        b.ui.node(h)
            .map(|n| n.region.box_)
            .expect("the inventory grid has a box");
    let (w, hgt) = (box_.width(), box_.height());
    let g = b.grid();
    let (cw, ch) = g.cell;
    let inside = (w / cw) * (hgt / ch);
    println!(
        "the inventory grid is {w}x{hgt} at a {cw}x{ch} cell -- {inside} cells inside the box"
    );
    assert_eq!(
        (w, hgt),
        (192, 96),
        "the shipped box, which retail draws at the same size"
    );
    assert_eq!(
        inside, 18,
        "6 columns x 3 rows, which is what retail's paired frame shows"
    );
    assert!(
        g.slots.len() > usize::try_from(inside).expect("positive"),
        "the rest of the capacity is behind the scrollbar, as retail's is"
    );
}

/// Opening a side pack must change capacity and clear former icon composites. The player's
/// capacity 102 matches layout 0x100001C6's fixed size, so only a pack of another capacity
/// distinguishes a real resize from the default.
///
/// The loop finds, in every recording, any positive capacity other than 102 (a Sack is 24) and
/// requires at least one applicable recording, checking each pack's actual capacity. Those
/// unopened packs have no recorded contents; shrinking must leave every slot without an item or
/// icon recipe.
#[test]
fn opening_a_side_pack_resizes_the_grid_to_that_packs_capacity_and_takes_every_tile_down() {
    let mut checked = 0usize;
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        let Some(player) = b.objects.world.player else {
            continue;
        };
        // The pack the capture's own character is carrying, found rather than named.
        let Some((pack, capacity)) = b
            .objects
            .world
            .inventory(player)
            .map(|i| i.containers.clone())
            .unwrap_or_default()
            .into_iter()
            .find_map(|id| {
                let c = i32::from(b.objects.world.weenie(id)?.pwd.items_capacity?);
                (c > 0 && c != 102).then_some((id, c))
            })
        else {
            println!("{session} carries no side pack whose capacity differs from 102");
            continue;
        };

        let backed_before = b
            .cells()
            .iter()
            .filter(|(i, bg)| i.is_some() && bg.is_some())
            .count();
        assert!(
            backed_before > 0,
            "{session}: nothing was backed before the pack was opened"
        );
        let before = b.grid().slots.len();
        assert_eq!(before, 102, "{session}: the player's own pack is 102 cells");

        let request_start = b.ui.requests.len();
        {
            let g = as_gameplay(&mut b.screen);
            assert!(
                g.inventory.open_container(&mut b.ui.requests, pack),
                "{session}: {pack:?} is on the side-pack strip, so opening the container succeeds"
            );
        }
        let mut interaction = dereth_client_runtime::interaction::Interaction::default();
        interaction.queue(Vec::new(), b.ui.requests.take_since(request_start));
        assert!(interaction
            .run_ui_requests(
                &mut b.objects.world,
                false,
                dereth_primitives::ServerTime(2.0),
            )
            .is_empty());
        assert!(interaction.pending_requests().is_empty());
        assert_eq!(
            b.objects.world.open_container,
            Some(pack),
            "{session}: the opened pack is owned"
        );
        b.hud
            .drive(&mut b.ui, as_gameplay(&mut b.screen), 2, &b.objects);

        let g = b.grid();
        let (fixed, cells) = (g.fixed_list_size, g.slots.len());
        println!(
            "{session} -- opening {:#010X} (items capacity {capacity}) took the grid from \
             {before} cells to {cells}",
            pack.0
        );
        assert_eq!(
            fixed, capacity,
            "{session}: FixedListSize follows the OPEN container"
        );
        assert_eq!(
            cells,
            usize::try_from(capacity).expect("positive"),
            "{session}: the grid is the open pack's capacity, not the layout's 102"
        );
        // The selected unopened pack has no contents list populated by a 0x0196 message.
        // Every cell must lose both item and composite. The icon child owns the recipe;
        // clearing selects empty state 0x1000001C, whose media step replaces the image.
        // This checks that actual state transition, not a separate root-background clear.
        let grid: Vec<_> = g.slots.iter().map(|s| (s.item, s.icon)).collect();
        for (item, icon) in grid {
            let recipe = icon
                .and_then(|h| b.ui.node(h))
                .and_then(|n| n.region.image.as_ref())
                .and_then(|g| g.op)
                .and_then(dereth_ui::region::SurfaceOp::icon_recipe);
            assert_eq!(
                item, None,
                "{session}: the unopened pack's grid holds an object"
            );
            assert_eq!(
                recipe, None,
                "{session}: an emptied cell kept its icon composite"
            );
        }
        checked += 1;
    }
    println!("{checked} captures carry a side pack whose capacity differs from the layout's");
    assert!(
        checked > 0,
        "no recording carries a side pack with a distinct positive capacity"
    );
}

/// A mapper's zero-valued row resolves to None, not Some(DataId(0)). The ordinary index helper
/// never returns row 0, so the other tests cannot see that filter. Check the zero row,
/// an absent group and an absent row independently; each must resolve to nothing.
#[test]
fn the_enum_lookup_refuses_the_mappers_own_zero_row() {
    let store = open_store();
    assert_eq!(
        dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 0),
        None,
        "row 0 of UIIconBackgrounds is 0x00000000 and must read as 'nothing', not as DataId(0)"
    );
    assert_eq!(
        dereth_assets::did_by_enum(&store, 0xDEAD_BEEF, 1),
        None,
        "a group the master mapper does not name resolves nothing"
    );
    assert!(
        dereth_assets::did_by_enum(&store, icon_background::ITEM_TYPE_GROUP, 0xDEAD).is_none(),
        "a row the group does not name resolves nothing"
    );
}

// =================================================================================================
// 3. The tile
// =================================================================================================

/// Behaviour: inventory.pack.every-filled-cell-carries-its-item-types-tile
///
/// For each occupied slot whose object exists, resolve its type's expected background from
/// the DAT mapper and compare the composite recipe. This joins decoded type, bit-index rule,
/// mapper and UI data rather than comparing a manually copied surface table. The local player
/// deliberately uses the container row instead of its ordinary type.
///
/// Empty slots have no object recipe; state 0x1000001C displays frame 0x06004D20 instead.
/// Removing composite assignment from ItemListWidget::decorate leaves filled recipes absent.
/// Missing objects form a separately counted case, rather than silently passing as decorated.
#[test]
fn every_filled_cell_carries_its_item_types_tile_and_every_empty_cell_carries_none() {
    let store = open_store();
    let (mut filled, mut empty, mut sessions, mut no_weenie) = (0usize, 0usize, 0usize, 0usize);
    let mut by_type: BTreeMap<u32, u32> = BTreeMap::new();
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        let types: BTreeMap<ObjectId, u32> = b
            .objects
            .world
            .tables
            .weenies
            .iter()
            .map(|(id, w)| (id, w.pwd.obj_type))
            .collect();
        let cells = b.cells();
        let mut here = 0usize;
        for (item, bg) in &cells {
            match item {
                Some(id) => {
                    // An item whose object is absent is not decorated: GameView::slot_decoration
                    // answers None for it. These slots must have no background; count them
                    // separately from filled successes and empty slots.
                    let Some(ty) = types.get(id).copied() else {
                        assert_eq!(
                            *bg, None,
                            "{session}: {id:?} has no weenie, so nothing may have decorated it"
                        );
                        no_weenie += 1;
                        continue;
                    };
                    let want = dereth_assets::did_by_enum(
                        &store,
                        icon_background::ITEM_TYPE_GROUP,
                        if Some(*id) == b.objects.world.player.or(b.hud.player) {
                            10 // The local-player special case forces the container row.
                        } else {
                            icon_background::enum_index(ty)
                        },
                    )
                    .unwrap_or_else(|| {
                        panic!("{session}: type {ty:#X} resolves no UIIconBackgrounds row")
                    });
                    assert_eq!(
                        *bg,
                        Some(want),
                        "{session}: the cell holding {id:?} (type {ty:#X}) draws the wrong tile"
                    );
                    *by_type.entry(ty).or_default() += 1;
                    filled += 1;
                    here += 1;
                }
                None => {
                    assert_eq!(
                        *bg, None,
                        "{session}: an empty cell has no icon descriptor and must carry no tile"
                    );
                    empty += 1;
                }
            }
        }
        println!(
            "{session} -- {here} filled cells, all backed, of {} cells",
            cells.len()
        );
        sessions += 1;
    }
    println!(
        "{filled} filled cells backed, {empty} empty cells left bare and {no_weenie} \
         cells naming an object with no weenie left alone, across {sessions} captures; \
         {} distinct ITEM_TYPEs exercised",
        by_type.len()
    );
    assert_eq!(
        sessions,
        recorded_sessions(),
        "the whole corpus was walked, not a subset"
    );
    assert!(filled > 0, "no recording fills a cell");
    assert!(
        by_type.len() > 1,
        "only {} item types exercised; the tile is supposed to vary by type",
        by_type.len()
    );
    // Recordings that never reach player creation contribute empty slots rather than filled
    // successes. Keep the empty/filled denominator separate from total-session coverage.
    assert!(
        empty > filled,
        "most of a 102-cell grid is empty and must stay bare: {empty}/{filled}"
    );
}

/// **The tiles actually differ**, which the test above does not establish on its own: if every type
/// resolved the same surface the join would still hold and the grid would still read as loose
/// icons on one flat colour.
#[test]
fn the_corpus_draws_more_than_one_tile() {
    let store = open_store();
    let mut seen = std::collections::BTreeSet::new();
    for session in corpus_sessions() {
        let mut b = Bench::open(&session);
        for (item, bg) in b.cells() {
            // A slot whose object is absent is not decorated and contributes no tile.
            if item.is_some() {
                if let Some(d) = bg {
                    seen.insert(d.0);
                }
            }
        }
    }
    let _ = &store;
    println!(
        "the corpus's items draw {} distinct background tiles: {seen:?}",
        seen.len()
    );
    assert!(
        seen.len() > 1,
        "only {} distinct tiles across the whole corpus",
        seen.len()
    );
}

// =================================================================================================
// 4. The pixels
// =================================================================================================

/// Remove only the background field from an object-icon recipe, retaining effects, base icon,
/// overlay and underlay, all of which live in the icon child's recipe. The compositor has a deliberate unbased fallback that preserves icon visibility; removing
/// the background can select that blend path too. The control holds the other recipe inputs,
/// not the final composited icon bytes.
fn drop_background(ui: &mut UiSystem, s: &mut dereth_ui_screens::items::widget::ItemSlot) -> bool {
    let Some(dereth_ui::region::IconRecipe::Object {
        background: _,
        effects,
        icon,
        overlay,
        underlay,
    }) = s.icon_recipe(ui)
    else {
        return false;
    };
    s.set_icon_composite(
        ui,
        Some(dereth_ui::region::IconRecipe::Object {
            background: None,
            effects,
            icon,
            overlay,
            underlay,
        }),
    )
}

fn app_in_gameplay(frames: u32) -> App {
    assert!(
        dereth_dat::testing::have_dats(),
        "the retail dats are required at {} -- set DERETH_TEST_DAT_DIR",
        client_dir().display()
    );
    let cfg = Config {
        headless: true,
        sound: false,
        ui: true,
        width: 800,
        height: 600,
        dat_dir: client_dir(),
        ..Config::default()
    };
    let mut app = App::new(cfg).expect("the GPU device and the shipped UI");
    app.start_shell().expect("the shell starts");
    let s = dereth_client_runtime::scene::SceneConfig {
        landblock: app.config().landblock,
        land_radius: app.config().land_radius,
        scenery_radius: app.config().scenery_radius,
        ..dereth_client_runtime::scene::SceneConfig::default()
    };
    app.load_static_scene(s).expect("the static scene loads");
    app.queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    for _ in 0..frames {
        app.frame();
    }
    app
}

fn app_gameplay_screen(app: &mut App) -> (&mut UiSystem, &mut GamePlayScreen) {
    let shell = app.ui_mut().expect("the UI shell");
    let ui = &mut shell.ui;
    let screen = shell.flow.current_mut().expect("a current screen");
    let any: &mut dyn std::any::Any = &mut **screen;
    let screen = any
        .downcast_mut::<GamePlayScreen>()
        .expect("gameplay screen");
    (ui, screen)
}

fn open_the_backpack(app: &mut App) {
    let (ui, screen) = app_gameplay_screen(app);
    let page = screen
        .panels
        .pages
        .iter()
        .find(|p| p.element == INVENTORY_PAGE)
        .copied()
        .expect("the inventory page is in the shipped panel stack");
    screen.recv_set_panel_visibility(ui, page.panel_id, true);
}

fn element_box(app: &mut App, id: ElementId) -> Box2D {
    let (ui, screen) = app_gameplay_screen(app);
    let root = *Screen::roots(&*screen).first().expect("root");
    let h = ui
        .get_child_recursive(root, id)
        .expect("in the shipped layout");
    ui.screen_clip_box(h)
}

/// Compare equal-frame headless applications with and without grid recipe backgrounds (two
/// successive frames of one advancing world differ outside the grid). The paired runs must
/// have equal dimensions and grid boxes,
/// a positive/equal changed-recipe count, zero changed RGB pixels outside the grid, and more
/// than 200 changed pixels per backed slot.
///
/// Top/container lists and the paper doll lose backgrounds in both runs; only the item grid
/// differs between them. No quickbar mutation is made here. The check exercises the composed
/// icon's pixel result.
#[test]
fn the_background_tile_reaches_the_pixels_inside_the_grid_and_nowhere_else() {
    let (objects, events) = {
        let (o, e) = replay_to(
            "first-login-walk-jump",
            busiest_instant("first-login-walk-jump"),
            None,
        );
        (o, e)
    };

    let shot = |show: bool| -> (u32, u32, Vec<u8>, Box2D, usize) {
        let mut app = app_in_gameplay(4);
        *app.probe_mut().objects_mut() = replay_to(
            "first-login-walk-jump",
            busiest_instant("first-login-walk-jump"),
            None,
        )
        .0;
        let _ = app.apply_hud_events(&events);
        open_the_backpack(&mut app);
        for _ in 0..8 {
            app.frame();
        }
        let area = element_box(&mut app, ITEM_LIST);
        // Everything outside the grid loses its tile in **both** runs, so the only thing that can
        // differ is the grid.
        let mut n = 0usize;
        {
            let (ui, screen) = app_gameplay_screen(&mut app);
            let p = &mut screen.inventory;
            for w in p
                .top_container
                .iter_mut()
                .chain(p.container_list.iter_mut())
            {
                for s in &mut w.slots {
                    drop_background(ui, s);
                }
            }
            for (_, w) in &mut p.doll {
                for s in &mut w.slots {
                    drop_background(ui, s);
                }
            }
            if !show {
                for w in p.item_list.iter_mut() {
                    for s in &mut w.slots {
                        if drop_background(ui, s) {
                            n += 1;
                        }
                    }
                }
            } else {
                n = p
                    .item_list
                    .as_ref()
                    .map_or(0, |w| w.slots.iter().filter(|s| s.item.is_some()).count());
            }
        }
        app.frame();
        let (w, h, bgra) = app
            .renderer_mut()
            .capture_bgra()
            .expect("the frame captures");
        app.shutdown();
        (w, h, bgra, area, n)
    };

    let (w, h, with, area, backed) = shot(true);
    let (w2, h2, without, area2, taken_down) = shot(false);
    assert_eq!((w, h), (w2, h2));
    assert_eq!(area, area2, "the grid moved between the two runs");
    assert!(
        area.is_valid(),
        "the inventory grid has a screen box: {area:?}"
    );
    assert!(backed > 0, "the run under test had no backed cell at all");
    assert_eq!(
        backed, taken_down,
        "the two runs disagree about how many cells carry a tile"
    );
    let _ = &objects;

    let (mut changed, mut outside) = (0u32, 0u32);
    for y in 0..h {
        for x in 0..w {
            let i = ((y * w + x) * 4) as usize;
            if with[i..i + 3] == without[i..i + 3] {
                continue;
            }
            changed += 1;
            let (px, py) = (x as i32, y as i32);
            if px < area.x0 || px > area.x1 || py < area.y0 || py > area.y1 {
                outside += 1;
            }
        }
    }
    println!(
        "{backed} backed cells change {changed} pixels, {outside} of them outside the grid \
         {area:?}"
    );
    assert_eq!(
        outside, 0,
        "{outside} of {changed} changed pixels fell outside the inventory grid"
    );
    // A 32x32 tile is 1024 pixels and the icon covers part of it; even one cell must move
    // hundreds. This floor is deliberately far below `backed * 1024` so that it measures
    // "the tiles drew" and not "the icons are small".
    assert!(
        changed > u32::try_from(backed).expect("small") * 200,
        "{backed} tiles changed only {changed} pixels, which is not a tile under each icon"
    );
}
