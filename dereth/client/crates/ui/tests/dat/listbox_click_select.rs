//! Which shipped list boxes select on a press. The list box's message listener and its press
//! handler both gate their selection on flag bit `0x2`, which is clear by default (the constructor
//! leaves `0x290`) and set only by attribute `0x59`, so a list that does not declare `0x59` cannot
//! be clicked, in retail or here. Of the 35 list boxes in the 101 shipped layouts, 20 declare it
//! and 15 do not; the attribute, title and page lists all do; and only the list-box element type
//! declares it at all, so the press arm never reaches an item list.
//!
//! Fixture: the shipped layouts in the retail dats; the census is printed and asserted.
//!
//! Behaviour: none (a census of the shipped list boxes' click-select attribute)

use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId};

use dereth_ui::desc::{ElementDesc, LayoutDesc};
use dereth_ui::{ElementId, ElementType};

const FIRST: u32 = 0x2100_0000;
const LAST: u32 = 0x2100_0075;

/// The list-box element's four layout-driven bits and four arms.
const ATTRS: [(u32, &str); 4] = [
    (0x59, "ClickSelect"),
    (0x5A, "DragRollover"),
    (0x5B, "DragSelect"),
    (0x61, "SelectedItemStateChange"),
];

fn store() -> Option<dereth_dat::RetailDatStore> {
    let dir = dereth_dat::testing::dat_dir();
    if !dereth_dat::testing::have_dats() {
        eprintln!(
            "skipping: no client_local_English.dat under {}",
            dir.display()
        );
        return None;
    }
    dereth_dat::RetailDatStore::open_dir(&dir).ok()
}

fn types(store: &dereth_dat::RetailDatStore) -> dereth_assets::ui::PropertyTypes {
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    let master =
        <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
            .expect("the master property table decodes");
    master.property_types()
}

fn walk<'a>(
    layout: DataId,
    id: ElementId,
    d: &'a ElementDesc,
    out: &mut Vec<(DataId, ElementId, &'a ElementDesc)>,
) {
    if d.ty == dereth_ui::factory::ty::LISTBOX {
        out.push((layout, id, d));
    }
    for (cid, c) in &d.children {
        walk(layout, *cid, c, out);
    }
}

/// **The shipped census of the list-box element's four bits, measured 2026-09-11.**
///
/// 35 list boxes in the 101 shipped layouts. **20 declare `0x59` true, 15 declare it not at all,
/// and not one declares it false.** Five — `0x21000043`'s, the vendor panes — declare `0x5B`
/// *false* explicitly; nothing anywhere declares `0x5A`.
///
/// Both halves matter: the bit is armed for the lists that select on a press, and it is genuinely
/// absent elsewhere, so a list with no `0x59` really is a list retail will not let you click.
#[test]
fn twenty_shipped_list_boxes_declare_click_select_and_fifteen_declare_nothing() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = types(&store);

    let mut layouts: BTreeMap<DataId, LayoutDesc> = BTreeMap::new();
    for did in FIRST..=LAST {
        let did = DataId(did);
        if !store.exists(did) {
            continue;
        }
        let b = store.read(did).expect("layout payload");
        layouts.insert(
            did,
            LayoutDesc::read(did, &b, &types).expect("layout decodes"),
        );
    }
    let mut lists: Vec<(DataId, ElementId, &ElementDesc)> = Vec::new();
    for (did, l) in &layouts {
        for (id, d) in &l.elements {
            walk(*did, *id, d, &mut lists);
        }
    }

    let mut on = 0_usize;
    let mut absent = 0_usize;
    let mut off: Vec<String> = Vec::new();
    let mut drag_select_false = 0_usize;
    let mut drag_rollover_declared = 0_usize;
    for (layout, id, d) in &lists {
        let cells: Vec<String> = ATTRS
            .iter()
            .map(|(a, _)| match d.base.properties.get_bool(*a) {
                Some(v) => format!("{v}"),
                None => "-".to_owned(),
            })
            .collect();
        eprintln!("{layout} {:#010x}  {}", id.0, cells.join(" "));
        match d.base.properties.get_bool(0x59) {
            None => absent += 1,
            Some(false) => off.push(format!("{layout} {:#010x}", id.0)),
            Some(true) => on += 1,
        }
        if d.base.properties.get_bool(0x5B) == Some(false) {
            drag_select_false += 1;
        }
        if d.base.properties.get_bool(0x5A).is_some() {
            drag_rollover_declared += 1;
        }
    }
    eprintln!(
        "list boxes: {}  0x59 true: {on}  0x59 absent: {absent}",
        lists.len()
    );
    assert_eq!(lists.len(), 35, "shipped list-box element count");
    assert_eq!(on, 20, "list boxes declaring 0x59 true");
    assert_eq!(absent, 15, "list boxes declaring no 0x59 at all");
    assert!(
        off.is_empty(),
        "nothing in the shipped data declares 0x59 false: {off:?}"
    );
    assert_eq!(
        drag_select_false, 5,
        "0x21000043's five vendor lists declare 0x5B false"
    );
    assert_eq!(drag_rollover_declared, 0, "nothing declares 0x5A");
}

/// **The attribute, skill, title and page lists all declare `0x59` true.**
///
/// Separate from the census because it is the load-bearing half: if any of these were in the
/// fifteen, `ListBox`'s press arm would be gated off for exactly the panels that need it.
///
/// * `0x1000023D` — the list shared by the attribute and skill panels
///   (layout `0x21000045`). It does **not** declare `0x61`.
/// * `0x10000532` — the character-title list (layout `0x2100005E`).
/// * `0x10000583` — the book-page list (layout `0x21000067`).
#[test]
fn the_attribute_title_and_page_lists_declare_click_select() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = types(&store);

    for (layout, element, state_change) in [
        (0x2100_0045_u32, 0x1000_023D_u32, None),
        (0x2100_005E, 0x1000_0532, Some(true)),
        (0x2100_0067, 0x1000_0583, Some(true)),
    ] {
        let did = DataId(layout);
        let b = store.read(did).expect("layout payload");
        let l = LayoutDesc::read(did, &b, &types).expect("layout decodes");
        let mut lists = Vec::new();
        for (id, d) in &l.elements {
            walk(did, *id, d, &mut lists);
        }
        let d = lists
            .iter()
            .find(|(_, id, _)| id.0 == element)
            .map(|(_, _, d)| *d)
            .unwrap_or_else(|| panic!("{did} carries no list box {element:#010x}"));
        assert_eq!(
            d.base.properties.get_bool(0x59),
            Some(true),
            "{element:#010x} must declare 0x59 ClickSelect"
        );
        assert_eq!(
            d.base.properties.get_bool(0x61),
            state_change,
            "{element:#010x}'s 0x61 SelectedItemStateChange"
        );
    }
}

/// The type constant this file's walk keys on, so a renumbering cannot quietly make the census
/// empty. The list-box element is factory type **5**.
#[test]
fn the_list_box_factory_type_is_five() {
    assert_eq!(dereth_ui::factory::ty::LISTBOX, ElementType(5));
}

/// **Which element *types* declare `0x59` at all — the blast radius of the press arm.**
///
/// `dereth-ui-screens` binds `dereth_ui::widgets::listbox::create` to **two** types: `5`
/// (the list-box element) and the game type `0x10000031` (the item-list element, which derives from
/// it in the client and whose screen-side binder publishes its active slots into the same
/// item vector). So the press arm reaches an item list too, and "who else carries `0x59`" is a
/// question the census of type 5 alone cannot answer.
///
/// **Measured, and the answer is clean: `0x59` is declared on element type `5` and on nothing
/// else.** All twenty are plain list-box elements; **no `0x10000031` item list declares it**,
/// so the press arm cannot fire on an inventory, vendor, trade or spellbar slot list at all. It is
/// asserted here rather than inferred from the item-list suites staying green: a suite that does
/// not exercise the case cannot distinguish "unaffected" from "untested".
#[test]
fn only_the_list_box_type_declares_click_select() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = types(&store);

    fn walk_all(d: &ElementDesc, out: &mut BTreeMap<u32, usize>) {
        if d.base.properties.get_bool(0x59).is_some() {
            *out.entry(d.ty.0).or_insert(0) += 1;
        }
        for c in d.children.values() {
            walk_all(c, out);
        }
    }

    let mut by_type: BTreeMap<u32, usize> = BTreeMap::new();
    for did in FIRST..=LAST {
        let did = DataId(did);
        if !store.exists(did) {
            continue;
        }
        let b = store.read(did).expect("layout payload");
        let l = LayoutDesc::read(did, &b, &types).expect("layout decodes");
        for d in l.elements.values() {
            walk_all(d, &mut by_type);
        }
    }
    for (t, n) in &by_type {
        eprintln!("type {t:#010x}: {n} element(s) declare 0x59");
    }
    assert_eq!(
        by_type,
        [(5_u32, 20_usize)].into_iter().collect::<BTreeMap<_, _>>(),
        "0x59 is declared on type 5 and on nothing else -- in particular not on 0x10000031, the \
         item list, which shares the same behaviour"
    );
}
