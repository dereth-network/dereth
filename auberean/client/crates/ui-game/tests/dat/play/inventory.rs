//! The inventory on the retail layouts: the inventory window's pack, money bag and paper doll, an
//! opened container's window, the stack splitter, dragging items between them.

use auberean_ui::types::button_id;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::gameplay::{GamePlay, Notice, Request, View};
use auberean_ui_game::inventory::container::change;
use auberean_ui_game::inventory::external::ExternalContainer;
use auberean_ui_game::inventory::stack::StackSplitter;
use auberean_ui_game::inventory::{
    self, notice, said, Container, Contents, Facts, Inventory, ItemFacts, Request as Asked,
};
use auberean_ui_game::toolbar::{action as tb, Toolbar};

use crate::common::records;

const PLAYER: u64 = 0x5000_0001;
const SWORD: u64 = 0x7000_0001;
const SHIELD: u64 = 0x7000_0002;
const ARROWS: u64 = 0x7000_0003;
const HELM: u64 = 0x7000_0004;
const CORPSE: u64 = 0x6000_0001;
const BONE: u64 = 0x7000_0010;

/// Pictures the items show (the retail slots' own pictures, so they exist).
const SWORD_ICON: u32 = 0x4100_0ADE;
const HELM_ICON: u32 = 0x4100_44DC;
/// An icon made of two layers (an item's look's), untinted.
const SHIELD_LAYERS: [(u32, u32); 2] = [(0x4100_5FF5, 0xFFFF_FFFF), (0x4100_76CA, 0xFFFF_FFFF)];

fn item(name: &str, icon: u32, quantity: u32, stackable: bool) -> ItemFacts {
    ItemFacts {
        quantity,
        icon,
        icon_layers: Vec::new(),
        stackable,
        stack_size: if stackable { 100 } else { 1 },
        name: name.to_owned(),
        wear: None,
        preferred_location: 0,
        had: true,
        worn: false,
        melts_at_once: false,
        offered_name: None,
        data_id: 0,
    }
}

/// The player with a sword, a shield and ten arrows in the main pack, a helm worn, 1234 coins.
fn facts() -> Facts {
    let pack = vec![SWORD, SHIELD, ARROWS];
    let slots = vec![(0, 3), (3, 0), (3, 0), (3, 0)];
    let segments = slots
        .iter()
        .map(|(first, count)| Contents {
            items: pack.clone(),
            first: *first,
            count: *count,
        })
        .collect();
    let mut f = Facts {
        version: 1,
        player: PLAYER,
        segments: slots.clone(),
        worn: vec![(1, HELM)],
        money: 1234,
        coin_stack: 1000,
        ..Facts::default()
    };
    f.containers.insert(
        PLAYER,
        Container {
            size: 78,
            segments,
            slots,
        },
    );
    f.items.insert(SWORD, item("Sword", SWORD_ICON, 1, false));
    let mut shield = item("Shield", 0, 1, false);
    shield.icon_layers = SHIELD_LAYERS.to_vec();
    f.items.insert(SHIELD, shield);
    f.items.insert(ARROWS, item("Arrows", SWORD_ICON, 10, true));
    let mut helm = item("Helm", HELM_ICON, 1, false);
    helm.wear = Some((30, 120));
    helm.worn = true;
    f.items.insert(HELM, helm);
    f
}

/// The play screen made and the player in the world.
fn play(ui: &mut Ui) -> GamePlay {
    let mut out = Vec::new();
    let mut p = GamePlay::new(ui, &mut out);
    p.login_finished(ui, &mut out);
    p
}

/// One frame with what the scripts answer and their notices this frame.
fn tick(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, f: &Facts, notices: &[Notice]) -> Vec<Request> {
    *t += 0.05;
    ui.use_time(*t);
    let view = View {
        inventory: Some(f),
        everyone: notices,
        ..View::default()
    };
    let player: Vec<Notice> = notices
        .iter()
        .filter(|n| n.about.is_none() || n.about == Some(f.player))
        .cloned()
        .collect();
    let mut out = Vec::new();
    p.tick(ui, &view, &player, &mut out);
    out
}

/// A 64-bit id as a notice's two words, high first.
fn split(v: u64) -> [u32; 2] {
    let w = |x: u64| u32::try_from(x & 0xFFFF_FFFF).unwrap_or(0);
    [w(v >> 32), w(v)]
}

fn el(ui: &Ui, name: &str) -> Handle {
    ui.element(Token::of(name))
        .unwrap_or_else(|| panic!("{name} made"))
}

fn middle(ui: &Ui, h: Handle) -> (i32, i32) {
    let r = ui.screen_rect(h).expect("placed");
    ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2)
}

fn key(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, f: &Facts, k: u32) {
    ui.key(k, true);
    tick(ui, p, t, f, &[]);
    ui.key(k, false);
    tick(ui, p, t, f, &[]);
}

fn click(ui: &mut Ui, p: &mut GamePlay, t: &mut f64, f: &Facts, name: &str) -> Vec<Request> {
    let (x, y) = middle(ui, el(ui, name));
    ui.pointer_move(x, y);
    let mut out = tick(ui, p, t, f, &[]);
    for down in [true, false] {
        ui.pointer_button(button_id::LEFT, down);
        out.extend(tick(ui, p, t, f, &[]));
    }
    out
}

/// Drag `from` and drop it on `to`, the clock moving between.
fn drag(
    ui: &mut Ui,
    p: &mut GamePlay,
    t: &mut f64,
    f: &Facts,
    from: Handle,
    to: Handle,
) -> Vec<Request> {
    let (x, y) = middle(ui, from);
    ui.pointer_move(x, y);
    let mut out = tick(ui, p, t, f, &[]);
    ui.pointer_button(button_id::LEFT, true);
    out.extend(tick(ui, p, t, f, &[]));
    ui.pointer_move(x + 8, y + 8);
    out.extend(tick(ui, p, t, f, &[]));
    assert!(ui.dragged().is_some(), "dragging");
    let (tx, ty) = middle(ui, to);
    ui.pointer_move(tx, ty);
    out.extend(tick(ui, p, t, f, &[]));
    ui.pointer_button(button_id::LEFT, false);
    out.extend(tick(ui, p, t, f, &[]));
    out
}

fn window(p: &GamePlay) -> &Inventory {
    p.child::<Toolbar>()
        .and_then(|t| t.child::<Inventory>())
        .expect("the inventory window")
}

fn asked(out: &[Request]) -> Vec<Asked> {
    out.iter()
        .filter_map(|r| match r {
            Request::Inventory(a) => Some(a.clone()),
            _ => None,
        })
        .collect()
}

/// The pack's first cells hold the items the scripts list for it, each showing its icon (its
/// look's layers laid together, when it has them) and its name as the tooltip; the inventory window shows by its key, the main pack first, a tab showing
/// its own pack; an item's icon carries the item, the pack it is in and its slot.
///
/// Behaviour: ui.inventory.the-pack-shows-its-items-in-their-slots
#[test]
fn the_pack_shows_its_items_in_their_slots() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert!(!ui.shown(el(&ui, "InventoryField")), "hidden at first");
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    assert!(ui.shown(el(&ui, "InventoryField")), "the inventory key");
    let pack = window(&p).backpack().expect("the pack");
    let main = &pack.packs()[0];
    assert_eq!(main.items(), vec![SWORD, SHIELD, ARROWS]);
    assert_eq!(main.max_size(), 42, "the main pack's cells");
    assert_eq!(
        pack.packs()
            .iter()
            .map(|d| d.first_slot())
            .collect::<Vec<_>>(),
        vec![0, 42, 54, 66],
        "each pack starts after the one before"
    );
    let list = main.list().expect("its cells");
    assert_eq!(ui.list_items(list).len(), 42);
    let sword = main.item_display(SWORD).and_then(|d| d.icon()).unwrap();
    let shield = main.item_display(SHIELD).and_then(|d| d.icon()).unwrap();
    assert_eq!(ui.element_tooltip(sword), Some("Sword"));
    assert_eq!(
        ui.node(sword).unwrap().picture.map(|p| p.id),
        Some(SWORD_ICON)
    );
    let made = ui.node(shield).unwrap().picture.map(|p| p.id).unwrap();
    let laid = ui.picture(made).expect("the icon laid together");
    let bottom = ui.assets().image(SHIELD_LAYERS[0].0).unwrap();
    assert_eq!((laid.width, laid.height), (bottom.width, bottom.height));
    assert_ne!(laid.bgra, bottom.bgra, "the top layer over the bottom one");
    let n = ui.node(shield).unwrap();
    assert_eq!(
        n.instance_id_attr(inventory::attr::SHORTCUT_DATA),
        Some(SHIELD)
    );
    assert_eq!(n.instance_id_attr(inventory::attr::FROM_ID), Some(PLAYER));
    assert_eq!(n.int_attr(inventory::attr::OLD_INDEX), Some(1));
    assert!(ui.shown(el(&ui, "PackMainContainer")));
    assert!(!ui.shown(el(&ui, "PackOneContainer")));
    click(&mut ui, &mut p, &mut t, &f, "PackOneButton");
    assert!(
        ui.shown(el(&ui, "PackOneContainer")),
        "its tab shows pack one"
    );
    assert!(!ui.shown(el(&ui, "PackMainContainer")));
    click(&mut ui, &mut p, &mut t, &f, "InventoryCloseButton");
    assert!(!ui.shown(el(&ui, "InventoryField")), "the close button");
}

/// The scripts' notices about the pack: an item added at a slot goes into that cell; one removed
/// leaves its cell empty at the end; a request about one under way shows it pending.
///
/// Behaviour: ui.inventory.the-scripts-notices-keep-the-pack-current
#[test]
fn the_scripts_notices_keep_the_pack_current() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    let container = |kind: u32, item: u64, slot: u32| {
        let mut words = split(PLAYER).to_vec();
        words.push(kind);
        words.extend(split(item));
        words.push(slot);
        Notice {
            about: Some(PLAYER),
            ..said(notice::UPDATE_CONTAINER, &words)
        }
    };
    f.version += 1;
    f.items.insert(BONE, item("Bone", SWORD_ICON, 1, false));
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &f,
        &[container(change::ADDED, BONE, 1)],
    );
    let main = |p: &GamePlay| window(p).backpack().unwrap().packs()[0].items();
    assert_eq!(
        main(&p),
        vec![SWORD, BONE, SHIELD, ARROWS],
        "added at its slot"
    );
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &f,
        &[container(change::UNDER_WAY, SHIELD, 0)],
    );
    let pending = |p: &GamePlay| {
        window(p).backpack().unwrap().packs()[0]
            .item_display(SHIELD)
            .is_some_and(|d| d.pending())
    };
    assert!(pending(&p), "a request under way");
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &f,
        &[container(change::FAILED, SHIELD, 0)],
    );
    assert!(!pending(&p), "the request failed");
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &f,
        &[container(change::REMOVED, SWORD, 0)],
    );
    assert_eq!(main(&p), vec![BONE, SHIELD, ARROWS], "removed");
    let list = window(&p).backpack().unwrap().packs()[0].list().unwrap();
    assert_eq!(ui.list_items(list).len(), 42, "the cells stay");
}

/// The scripts' notice about the player's pack as a frame's notices carry it.
fn pack_notice(kind: u32, item: u64, slot: u32) -> Notice {
    let mut words = split(PLAYER).to_vec();
    words.push(kind);
    words.extend(split(item));
    words.push(slot);
    Notice {
        about: Some(PLAYER),
        ..said(notice::UPDATE_CONTAINER, &words)
    }
}

/// An item given as the pack is sent again (a new character's first entry: the pack's contents,
/// then the gift added at its slot) shows once: the frame's notices say everything again and then
/// the item added, and the scripts' answers after both already hold it.
///
/// Behaviour: ui.inventory.an-item-given-as-the-pack-is-sent-shows-once
#[test]
fn an_item_given_as_the_pack_is_sent_shows_once() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    // The answers after the frame: the gift at the pack's first slot.
    f.version += 1;
    f.items.insert(BONE, item("Bone", SWORD_ICON, 1, false));
    let pack = vec![BONE, SWORD, SHIELD, ARROWS];
    let slots = vec![(0, 4), (4, 0), (4, 0), (4, 0)];
    let held = f.containers.get_mut(&PLAYER).expect("the pack");
    for (c, (first, count)) in held.segments.iter_mut().zip(&slots) {
        c.items.clone_from(&pack);
        c.first = *first;
        c.count = *count;
    }
    held.slots.clone_from(&slots);
    f.segments = slots;
    let out = [
        pack_notice(change::ALL, 0, 0),
        pack_notice(change::ADDED, BONE, 0),
    ];
    tick(&mut ui, &mut p, &mut t, &f, &out);
    let main = window(&p).backpack().unwrap().packs()[0].items();
    assert_eq!(main, vec![BONE, SWORD, SHIELD, ARROWS], "the gift once");
    // A frame later the next answers come with the next notice: it takes effect.
    f.version += 1;
    let pack = vec![BONE, SHIELD, ARROWS];
    let held = f.containers.get_mut(&PLAYER).expect("the pack");
    for c in &mut held.segments {
        c.items.clone_from(&pack);
    }
    tick(
        &mut ui,
        &mut p,
        &mut t,
        &f,
        &[pack_notice(change::REMOVED, SWORD, 1)],
    );
    let main = window(&p).backpack().unwrap().packs()[0].items();
    assert_eq!(main, vec![BONE, SHIELD, ARROWS], "removed");
}

/// A swap in one frame (one wielded thing back in the pack as the next leaves it): the item coming
/// back is added at the slot past the last, though by the scripts' answers after the frame the
/// pack holds no more items than before and that slot lies past them.
///
/// Behaviour: ui.inventory.an-item-coming-back-as-another-leaves-shows
#[test]
fn an_item_coming_back_as_another_leaves_shows() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    // The answers after the frame: as many items, the shield gone and the bone last.
    f.version += 1;
    f.items.insert(BONE, item("Bone", SWORD_ICON, 1, false));
    let pack = vec![SWORD, ARROWS, BONE];
    let held = f.containers.get_mut(&PLAYER).expect("the pack");
    for c in &mut held.segments {
        c.items.clone_from(&pack);
    }
    let out = [
        pack_notice(change::ADDED, BONE, 3),
        pack_notice(change::REMOVED, SHIELD, 1),
    ];
    tick(&mut ui, &mut p, &mut t, &f, &out);
    let main = window(&p).backpack().unwrap().packs()[0].items();
    assert_eq!(main, vec![SWORD, ARROWS, BONE], "the bone in the pack");
    for other in &window(&p).backpack().unwrap().packs()[1..] {
        assert!(other.items().is_empty(), "only in the main pack");
    }
}

/// An item pressed is selected; dragged onto another cell of its pack it is moved to that slot (no further than just
/// past the last item, which is a slot sooner once it has left its own); a stack is split first:
/// the splitter shows its name and the whole stack, OK with more than that says so, OK with a
/// number moves that many.
///
/// Behaviour: ui.inventory.an-item-dropped-on-a-cell-moves-there
#[test]
fn an_item_dropped_on_a_cell_moves_there() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let main = &window(&p).backpack().unwrap().packs()[0];
    let list = main.list().unwrap();
    let sword = main.item_display(SWORD).and_then(|d| d.icon()).unwrap();
    let arrows = main.item_display(ARROWS).and_then(|d| d.icon()).unwrap();
    let far_cell = ui.list_item(list, 5).unwrap();
    let out = drag(&mut ui, &mut p, &mut t, &f, sword, far_cell);
    assert_eq!(
        asked(&out),
        vec![
            Asked::Select(SWORD),
            Asked::Put {
                item: SWORD,
                from: PLAYER,
                to: PLAYER,
                old_slot: 0,
                new_slot: 2,
                quantity: 1,
            },
        ],
        "past the last item, a slot sooner"
    );
    let second = ui.list_item(list, 1).unwrap();
    let out = drag(&mut ui, &mut p, &mut t, &f, arrows, second);
    assert_eq!(
        asked(&out),
        vec![Asked::Select(ARROWS)],
        "a stack waits on the splitter"
    );
    let splitter = p.child::<StackSplitter>().expect("the splitter");
    assert_ne!(splitter.context(), 0);
    assert!(ui.shown(el(&ui, "StackableField")));
    assert_eq!(ui.text(el(&ui, "Label")).as_deref(), Some("Arrows"));
    assert_eq!(ui.text(el(&ui, "AmountEntry")).as_deref(), Some("10"));
    let entry = el(&ui, "AmountEntry");
    ui.set_text(entry, "20");
    click(&mut ui, &mut p, &mut t, &f, "StackOkButton");
    assert!(ui.shown(el(&ui, "StackableField")), "more than the stack");
    assert!(
        ui.text(el(&ui, "ErrorText"))
            .is_some_and(|s| s.contains("10")),
        "says the most: {:?}",
        ui.text(el(&ui, "ErrorText"))
    );
    ui.set_text(entry, "4");
    let out = click(&mut ui, &mut p, &mut t, &f, "StackOkButton");
    assert!(!ui.shown(el(&ui, "StackableField")), "closed");
    assert_eq!(
        asked(&out),
        vec![Asked::Put {
            item: ARROWS,
            from: PLAYER,
            to: PLAYER,
            old_slot: 2,
            new_slot: 1,
            quantity: 4,
        }]
    );
}

/// When the scripts say a container opened, its window shows its contents; take-all asks to take
/// everything; the close button closes it for the player and hides it; when the scripts say it
/// closed, it hides.
///
/// Behaviour: ui.inventory.an-opened-container-shows-its-contents
#[test]
fn an_opened_container_shows_its_contents() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert!(
        !ui.shown(el(&ui, "ExternalContainerField")),
        "hidden at first"
    );
    f.version += 1;
    f.containers.insert(
        CORPSE,
        Container {
            size: 6,
            segments: vec![Contents {
                items: vec![BONE],
                first: 0,
                count: 1,
            }],
            slots: vec![(0, 1)],
        },
    );
    f.items.insert(BONE, item("Bone", SWORD_ICON, 1, false));
    let state = |open: u32| Notice {
        about: Some(CORPSE),
        ..said(
            notice::CONTAINER_STATE_UPDATE,
            &[split(CORPSE)[0], split(CORPSE)[1], open],
        )
    };
    f.screen.insert(CORPSE, (300, 500));
    tick(&mut ui, &mut p, &mut t, &f, &[state(1)]);
    assert!(ui.shown(el(&ui, "ExternalContainerField")), "opened");
    let r = ui.node(el(&ui, "ExternalContainerField")).unwrap().rect;
    assert_eq!(
        (r.x0, r.y0),
        (
            dereth_primitives::num::to_i32(300.0 - r.width() as f32 * 0.5),
            dereth_primitives::num::to_i32(500.0 - r.height() as f32 * 0.5),
        ),
        "over the container where it stands on the screen"
    );
    f.screen.clear();
    let ext = p.child::<ExternalContainer>().unwrap();
    assert_eq!(ext.container(), Some(CORPSE));
    assert_eq!(ext.display().unwrap().items(), vec![BONE]);
    let out = click(&mut ui, &mut p, &mut t, &f, "LootAllButton");
    assert_eq!(asked(&out), vec![Asked::TakeAll { container: CORPSE }]);
    let out = click(&mut ui, &mut p, &mut t, &f, "ExternalContainerCloseButton");
    assert_eq!(asked(&out), vec![Asked::Close { container: CORPSE }]);
    assert!(
        !ui.shown(el(&ui, "ExternalContainerField")),
        "closed by its button"
    );
    tick(&mut ui, &mut p, &mut t, &f, &[state(1)]);
    assert!(ui.shown(el(&ui, "ExternalContainerField")), "opened again");
    tick(&mut ui, &mut p, &mut t, &f, &[state(0)]);
    assert!(
        !ui.shown(el(&ui, "ExternalContainerField")),
        "the scripts closed it"
    );
}

/// An item pulled from the world comes from no container: let go over an open chest's cell it
/// stays where it is; only the player's own pack takes it.
///
/// Behaviour: ui.inventory.an-item-from-the-world-goes-only-into-the-pack
#[test]
fn an_item_from_the_world_goes_only_into_the_pack() {
    const STONE: u64 = 0x8000_0000_0000_0077;
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    f.items.insert(STONE, item("Stone", SWORD_ICON, 1, false));
    f.selection = STONE;
    f.containers.insert(
        CORPSE,
        Container {
            size: 6,
            segments: vec![Contents {
                items: vec![BONE],
                first: 0,
                count: 1,
            }],
            slots: vec![(0, 1)],
        },
    );
    f.items.insert(BONE, item("Bone", SWORD_ICON, 1, false));
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    let state = Notice {
        about: Some(CORPSE),
        ..said(
            notice::CONTAINER_STATE_UPDATE,
            &[split(CORPSE)[0], split(CORPSE)[1], 1],
        )
    };
    tick(&mut ui, &mut p, &mut t, &f, &[state]);
    let ext = p.child::<ExternalContainer>().unwrap();
    let list = ext.display().unwrap().list().unwrap();
    let cell = ui.list_item(list, 3).unwrap();
    let world = (150, 420);
    ui.pointer_move(world.0, world.1);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert_eq!(ui.top(), ui.element(Token::of("ItemSelectionField")));
    ui.pointer_button(button_id::LEFT, true);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_move(world.0 + 8, world.1 + 8);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    let copy = ui.dragged().expect("the selection's icon dragged");
    assert_eq!(
        ui.node(copy)
            .unwrap()
            .instance_id_attr(inventory::attr::FROM_ID),
        None
    );
    let (x, y) = middle(&ui, cell);
    ui.pointer_move(x, y);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_button(button_id::LEFT, false);
    let out = tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert!(
        !asked(&out).iter().any(|a| matches!(a, Asked::Put { .. })),
        "{:?}",
        asked(&out)
    );
}

/// The paper doll shows what is worn in its place's slot, its wear on the meter beside it; an
/// item from the pack dropped on a slot (selected as it is pressed) is worn there; the money bag shows the player's money.
///
/// Behaviour: ui.inventory.the-paper-doll-shows-and-wears-items
#[test]
fn the_paper_doll_shows_and_wears_items() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let doll = window(&p).paper_doll().unwrap();
    assert_eq!(doll.worn_at(1), Some(HELM), "the helm on the head");
    let head = el(&ui, "Head_Slot");
    let icon = ui.child(head, Token::of("ItemIcon")).unwrap();
    assert_eq!(
        ui.node(icon).unwrap().picture.map(|p| p.id),
        Some(HELM_ICON)
    );
    assert!(ui.node(icon).unwrap().bool_attr(inventory::attr::EQUIPPED));
    let wear = el(&ui, "Head_Durability");
    assert!(ui.shown(wear), "its wear meter");
    assert!(!ui.shown(el(&ui, "Torso_Durability")), "nothing worn there");
    let main = &window(&p).backpack().unwrap().packs()[0];
    let shield = main.item_display(SHIELD).and_then(|d| d.icon()).unwrap();
    let hand = el(&ui, "SecondaryHand_Slot");
    let out = drag(&mut ui, &mut p, &mut t, &f, shield, hand);
    assert_eq!(
        asked(&out),
        vec![
            Asked::Select(SHIELD),
            Asked::Wear {
                item: SHIELD,
                location: 0x80
            },
        ]
    );
    let money = Notice {
        about: Some(PLAYER),
        ..said(notice::UPDATE_MONEY, &[1234])
    };
    tick(&mut ui, &mut p, &mut t, &f, &[money]);
    assert_eq!(window(&p).money_bag().unwrap().money(), 1234);
    let shown = ui.text(el(&ui, "AmountLabel")).unwrap_or_default();
    assert!(shown.contains("1"), "the amount shows: {shown:?}");
}

/// On a screen larger than the layouts' (1600 by 1200), an item dragged out of the pack is shown
/// once, in its cell's size, its corner at the pointer: its icon is not repeated across the
/// screen beside it.
///
/// Behaviour: ui.inventory.a-dragged-item-shows-its-icon-once-on-a-larger-screen
#[test]
fn a_dragged_item_shows_its_icon_once_on_a_larger_screen() {
    let mut ui = Ui::new(records(), 1600, 1200);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let shield = window(&p)
        .backpack()
        .and_then(|b| b.packs()[0].item_display(SHIELD))
        .and_then(|d| d.icon())
        .expect("the shield in the pack");
    let cell = ui.screen_rect(shield).expect("placed");
    let (x, y) = middle(&ui, shield);
    ui.pointer_move(x, y);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_button(button_id::LEFT, true);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_move(x + 8, y + 8);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_move(300, 300);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert!(ui.dragged().is_some(), "dragging");
    assert_eq!(
        ui.drag_area(),
        Some(auberean_ui::Rect::sized(
            300,
            300,
            cell.width(),
            cell.height()
        )),
        "the cell's size, at the pointer"
    );
    let mut s = auberean_ui::draw::Surface::new(1600, 1200);
    auberean_ui::draw::draw(&ui, &mut s);
    let shown = |x: i32, y: i32| s.get(x, y)[3] != 0;
    assert!(
        shown(300 + cell.width() / 2, 300 + cell.height() / 2),
        "the icon"
    );
    for (x, y) in [
        (300 + cell.width() + 8, 310),
        (310, 300 + cell.height() + 8),
    ] {
        assert!(!shown(x, y), "nothing drawn at ({x}, {y})");
    }
    ui.pointer_button(button_id::LEFT, false);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
}

/// An item of the pack dragged out over the world and let go asks what lies under the pointer
/// there; once the world answers, the item is moved to what was found: to no object when the
/// ground (or the player itself) lay there, which puts it down, else into that object. Nothing
/// moves while the answer is awaited.
///
/// Behaviour: ui.inventory.an-item-dropped-in-the-world-goes-to-what-lies-there
#[test]
fn an_item_dropped_in_the_world_goes_to_what_lies_there() {
    const MAN: u64 = 0x8000_0000_0000_0042;
    for (found, to) in [(0, 0), (PLAYER, 0), (MAN, MAN)] {
        let mut ui = Ui::new(records(), 1024, 768);
        let mut p = play(&mut ui);
        let mut t = 0.0;
        let mut f = facts();
        tick(&mut ui, &mut p, &mut t, &f, &[]);
        key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
        let sword = window(&p)
            .backpack()
            .and_then(|b| b.packs()[0].item_display(SWORD))
            .and_then(|d| d.icon())
            .expect("the sword in the pack");
        let (x, y) = middle(&ui, sword);
        ui.pointer_move(x, y);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
        ui.pointer_button(button_id::LEFT, true);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
        ui.pointer_move(x + 8, y + 8);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
        assert!(ui.dragged().is_some(), "dragging");
        let ground = (150, 420);
        ui.pointer_move(ground.0, ground.1);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
        assert_eq!(
            ui.hit(ground.0, ground.1),
            ui.element(Token::of("ItemSelectionField")),
            "the field over the world lies there"
        );
        ui.pointer_button(button_id::LEFT, false);
        let out = tick(&mut ui, &mut p, &mut t, &f, &[]);
        assert!(
            asked(&out).contains(&Asked::PickWorld),
            "the world asked what lies under the pointer: {:?}",
            asked(&out)
        );
        let waiting = tick(&mut ui, &mut p, &mut t, &f, &[]);
        assert!(
            !asked(&waiting)
                .iter()
                .any(|a| matches!(a, Asked::Put { .. })),
            "nothing moves before the answer"
        );
        f.picked = Some(found);
        let mut out = tick(&mut ui, &mut p, &mut t, &f, &[]);
        out.extend(tick(&mut ui, &mut p, &mut t, &f, &[]));
        assert_eq!(
            asked(&out),
            vec![Asked::Put {
                item: SWORD,
                from: PLAYER,
                to,
                old_slot: 0,
                new_slot: 0,
                quantity: 1,
            }],
            "found {found:#x}"
        );
    }
}

/// A press on the paper doll's figure waits for its own search: what the last press found, still
/// there as the search is asked, is not selected, and what the search finds is.
///
/// Behaviour: ui.inventory.a-press-on-the-doll-waits-for-its-own-answer
#[test]
fn a_press_on_the_doll_waits_for_its_own_answer() {
    /// The kind of mesh on the head, where the helm is worn.
    const HEAD_MESH: u64 = 0x8100_0003;
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let figure = window(&p)
        .paper_doll()
        .and_then(|d| d.viewport())
        .expect("the figure's viewport");
    let (x, y) = middle(&ui, figure);
    ui.pointer_move(x, y);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    // Only the search's answer is under test: the press goes straight to the figure.
    while let Some(over) = ui.top().filter(|o| *o != figure) {
        ui.set_attr(
            over,
            auberean_ui::token::attr::MOUSE_INVISIBLE,
            auberean_ui::Value::Bool(true),
        );
        ui.pointer_move(x, y);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
    }
    assert_eq!(ui.top(), Some(figure));
    let press = |ui: &mut Ui, p: &mut GamePlay, t: &mut f64, f: &Facts| {
        ui.pointer_button(button_id::LEFT, true);
        tick(ui, p, t, f, &[])
    };

    // The first press finds nothing where it pressed.
    let out = press(&mut ui, &mut p, &mut t, &f);
    assert!(asked(&out).contains(&Asked::Pick), "{:?}", asked(&out));
    f.picked = Some(0);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_button(button_id::LEFT, false);
    tick(&mut ui, &mut p, &mut t, &f, &[]);

    // The second finds the head. The nothing from before stands as it is asked.
    let out = press(&mut ui, &mut p, &mut t, &f);
    assert!(asked(&out).contains(&Asked::Pick), "{:?}", asked(&out));
    f.picked = None;
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    f.picked = Some(HEAD_MESH);
    let mut out = tick(&mut ui, &mut p, &mut t, &f, &[]);
    out.extend(tick(&mut ui, &mut p, &mut t, &f, &[]));
    assert!(
        asked(&out).contains(&Asked::Select(HELM)),
        "what is worn there selected: {:?}",
        asked(&out)
    );
}

/// A second item dropped in the world goes to what the world answers for it, not to what the last
/// drop's search found, which is still there as the search is asked.
///
/// Behaviour: ui.inventory.a-drop-in-the-world-waits-for-its-own-answer
#[test]
fn a_drop_in_the_world_waits_for_its_own_answer() {
    const MAN: u64 = 0x8000_0000_0000_0042;
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let ground = (150, 420);
    let drop_sword = |ui: &mut Ui, p: &mut GamePlay, t: &mut f64, f: &Facts| {
        let sword = window(p)
            .backpack()
            .and_then(|b| b.packs()[0].item_display(SWORD))
            .and_then(|d| d.icon())
            .expect("the sword in the pack");
        let (x, y) = middle(ui, sword);
        ui.pointer_move(x, y);
        tick(ui, p, t, f, &[]);
        ui.pointer_button(button_id::LEFT, true);
        tick(ui, p, t, f, &[]);
        ui.pointer_move(x + 8, y + 8);
        tick(ui, p, t, f, &[]);
        ui.pointer_move(ground.0, ground.1);
        tick(ui, p, t, f, &[]);
        ui.pointer_button(button_id::LEFT, false);
        tick(ui, p, t, f, &[])
    };
    let out = drop_sword(&mut ui, &mut p, &mut t, &f);
    assert!(asked(&out).contains(&Asked::PickWorld));
    f.picked = Some(MAN);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    tick(&mut ui, &mut p, &mut t, &f, &[]);

    // The last answer stands as the second search is asked, then goes as the search starts.
    let out = drop_sword(&mut ui, &mut p, &mut t, &f);
    assert!(asked(&out).contains(&Asked::PickWorld));
    f.picked = None;
    let mut out = tick(&mut ui, &mut p, &mut t, &f, &[]);
    out.extend(tick(&mut ui, &mut p, &mut t, &f, &[]));
    assert!(
        !asked(&out).iter().any(|a| matches!(a, Asked::Put { .. })),
        "nothing moves before its own answer: {:?}",
        asked(&out)
    );
    f.picked = Some(0);
    let mut out = tick(&mut ui, &mut p, &mut t, &f, &[]);
    out.extend(tick(&mut ui, &mut p, &mut t, &f, &[]));
    assert_eq!(
        asked(&out),
        vec![Asked::Put {
            item: SWORD,
            from: PLAYER,
            to: 0,
            old_slot: 0,
            new_slot: 0,
            quantity: 1,
        }],
    );
}

/// A press on the world held and moved drags the selection's icon (the object selected in the
/// world, wherever it lies), not an item of the pack the pointer then passes over; let go over the
/// pack, the selection is asked to be put in it.
///
/// Behaviour: ui.inventory.the-world-pulled-on-drags-the-selection-not-a-pack-item
#[test]
fn the_world_pulled_on_drags_the_selection_not_a_pack_item() {
    use auberean_ui::token::attr;
    const STONE: u64 = 0x8000_0000_0000_0077;
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let mut f = facts();
    let mut stone = item("Stone", HELM_ICON, 1, false);
    stone.had = false;
    f.items.insert(STONE, stone);
    f.selection = STONE;
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    key(&mut ui, &mut p, &mut t, &f, tb::INVENTORY_PANEL);
    let sword = window(&p)
        .backpack()
        .and_then(|b| b.packs()[0].item_display(SWORD))
        .and_then(|d| d.icon())
        .expect("the sword in the pack");
    let world = (400, 200);
    ui.pointer_move(world.0, world.1);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    assert_eq!(
        ui.top(),
        ui.element(Token::of("ItemSelectionField")),
        "the pointer on the world"
    );
    ui.pointer_button(button_id::LEFT, true);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    // Pulled: moved on in steps of a few texels toward the pack, over the world, two moves within
    // one frame.
    let (sx, sy) = middle(&ui, sword);
    for i in 1..=4 {
        ui.pointer_move(world.0 + 4 * i, world.1 + 4 * i);
        if i > 1 {
            tick(&mut ui, &mut p, &mut t, &f, &[]);
        }
    }
    let carried = |ui: &Ui| {
        ui.dragged()
            .and_then(|d| ui.node(d))
            .and_then(|n| n.instance_id_attr(Token::of("ShortcutData")))
    };
    assert_eq!(carried(&ui), Some(STONE), "the selection's icon dragged");
    // On over the pack's sword: still the selection.
    for i in 1..=8 {
        let x = world.0 + 16 + (sx - world.0 - 16) * i / 8;
        let y = world.1 + 16 + (sy - world.1 - 16) * i / 8;
        ui.pointer_move(x, y);
        tick(&mut ui, &mut p, &mut t, &f, &[]);
    }
    assert_eq!(carried(&ui), Some(STONE), "the sword is not dragged");
    assert!(
        ui.node(sword).is_some_and(|n| n.bool_attr(attr::DRAGABLE)),
        "the sword stays in its cell"
    );
    // Let go on the pack's second cell.
    let shield = window(&p)
        .backpack()
        .and_then(|b| b.packs()[0].item_display(SHIELD))
        .and_then(|d| d.icon())
        .expect("the shield in the pack");
    let (x, y) = middle(&ui, shield);
    ui.pointer_move(x, y);
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    ui.pointer_button(button_id::LEFT, false);
    let mut out = tick(&mut ui, &mut p, &mut t, &f, &[]);
    out.extend(tick(&mut ui, &mut p, &mut t, &f, &[]));
    assert_eq!(
        asked(&out),
        vec![Asked::Put {
            item: STONE,
            from: inventory::NO_OBJECT,
            to: PLAYER,
            old_slot: 0,
            new_slot: 1,
            quantity: 1,
        }],
        "the selection asked into the pack"
    );
}

/// The stack splitter's entry holds the amount offered as the client writes it, a signed number
/// (of which the entry shows nine characters): an amount from 2^31 up shows negative.
///
/// Behaviour: ui.inventory.the-stack-amount-is-written-signed
#[test]
fn the_stack_amount_is_written_signed() {
    let mut ui = Ui::new(records(), 1024, 768);
    let mut p = play(&mut ui);
    let mut t = 0.0;
    let f = facts();
    tick(&mut ui, &mut p, &mut t, &f, &[]);
    let field = el(&ui, "StackableField");
    let entry = ui
        .child_recursive(field, Token::of("AmountEntry"))
        .expect("the entry");
    let mut shown = |amount: u32| {
        let ask = inventory::init_stack(1, amount, u32::MAX, 0, 0, 0);
        tick(&mut ui, &mut p, &mut t, &f, &[ask]);
        ui.text(entry).unwrap_or_default()
    };
    // The entry holds nine characters.
    assert_eq!(shown(0x7FFF_FFFF), "214748364");
    assert_eq!(shown(0x8000_0000), "-21474836");
    assert_eq!(shown(u32::MAX), "-1");
}
