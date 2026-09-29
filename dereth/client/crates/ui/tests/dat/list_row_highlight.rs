//! Where a list row's selected highlight comes from, in the dat. Two data facts close the chain
//! with nothing per-panel: `MasterProperty 0x39000001` gives the list box's unselected and selected
//! state attributes (`0x5D`, `0x5E`) the defaults 1 and 6, which an element with no authored value
//! reads; and a list's entry template authors a state 6 whose only content is a lighter image. The
//! Friends list (`0x10000517`, template `0x10000519`, plate `0x06001AAF`) is one such list; of the
//! sixteen lists that enable the selected-item state change, the ones whose template authors a
//! state 6 highlight through the default alone. The Page List (`0x10000583`, template
//! `0x10000589`) is another: it enables the state change and names neither `0x5D` nor `0x5E`;
//! only the five vendor panes of `0x21000043` name the pair.
//!
//! The selection routine passes the state straight to the row's state setter; the drawn band is
//! asserted by the testkit scenario `pressing_a_row_draws_the_band_across_that_row`. Fixture: the
//! master property and shipped layouts in the retail dats.

use std::collections::BTreeMap;

use dereth_assets::ui::PropertyValue;
use dereth_primitives::{AssetSource, DataId};
use dereth_ui::desc::{ElementDesc, LayoutDesc, MediaFields};
use dereth_ui::{ElementId, StateId};

/// The Friends panel's layout.
const FRIENDS_LAYOUT: DataId = DataId(0x2100_005D);
/// The Friends list box.
const FRIENDS_LIST: u32 = 0x1000_0517;
/// The row root the list's own `0x64` array names.
const FRIENDS_ROW_TEMPLATE: u32 = 0x1000_0519;
/// The lighter plate state 6 blits.
const SELECTED_PLATE: DataId = DataId(0x0600_1AAF);
/// The Page List panel's layout.
const PAGE_LIST_LAYOUT: u32 = 0x2100_0067;
/// The row template the Page List builds its rows from.
const ROW_TEMPLATE: u32 = 0x1000_0589;
/// The Page List box.
const LIST: u32 = 0x1000_0583;

fn store() -> Option<dereth_dat::RetailDatStore> {
    let dir = dereth_dat::testing::dat_dir();
    if !dereth_dat::testing::have_dats() {
        eprintln!("no client_local_English.dat under {}", dir.display());
        return None;
    }
    dereth_dat::RetailDatStore::open_dir(&dir).ok()
}

fn master(store: &dereth_dat::RetailDatStore) -> dereth_assets::MasterProperty {
    let master_id = DataId(0x3900_0001);
    let bytes = store.read(master_id).expect("MasterProperty 0x39000001");
    <dereth_assets::MasterProperty as dereth_assets::Decode>::decode_payload(master_id, &bytes)
        .expect("the master property table decodes")
}

fn types(store: &dereth_dat::RetailDatStore) -> dereth_assets::ui::PropertyTypes {
    master(store).property_types()
}

fn layout(store: &dereth_dat::RetailDatStore, did: DataId) -> LayoutDesc {
    let b = store.read(did).expect("layout payload");
    LayoutDesc::read(did, &b, &types(store)).expect("layout decodes")
}

fn flatten(l: &LayoutDesc) -> BTreeMap<u32, &ElementDesc> {
    fn walk<'a>(id: ElementId, d: &'a ElementDesc, out: &mut BTreeMap<u32, &'a ElementDesc>) {
        out.insert(id.0, d);
        for (cid, c) in &d.children {
            walk(*cid, c, out);
        }
    }
    let mut out = BTreeMap::new();
    for (id, d) in &l.elements {
        walk(*id, d, &mut out);
    }
    out
}

/// **Fact 1.** The master rows for the two states carry defaults, and they are 1 and 6.
///
/// The `0x59`/`0x61` rows beside them carry none, which is the calibration: the decoder reads a
/// default where there is one and reports none where there is none.
#[test]
fn master_property_defaults_the_item_states_to_1_and_6() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let m = master(&store);
    let row = |id: u32| {
        m.properties
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, d)| d)
            .unwrap()
    };
    let name = |d: &dereth_assets::ui::PropertyDesc| {
        m.enum_names
            .iter()
            .find(|n| n.0 == d.name)
            .map(|n| n.1.clone())
            .unwrap_or_default()
    };

    let normal = row(0x5D);
    assert_eq!(name(normal), "UICore_ListBox_item_normal_state");
    assert_eq!(
        normal.default,
        Some(PropertyValue::Enum(1)),
        "0x5D defaults to state 1"
    );

    let selected = row(0x5E);
    assert_eq!(name(selected), "UICore_ListBox_item_selected_state");
    assert_eq!(
        selected.default,
        Some(PropertyValue::Enum(6)),
        "0x5E defaults to state 6"
    );

    assert_eq!(row(0x59).default, None, "0x59 click_select has no default");
    assert_eq!(
        row(0x61).default,
        None,
        "0x61 selected_item_state_change has no default"
    );
}

/// **Fact 2.** The Friends list enables the state change, names no states itself, and its own
/// entry template authors a state 6 whose one media entry is the lighter plate.
#[test]
fn the_friends_row_template_authors_state_6_with_the_lighter_plate() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = master(&store).property_types();
    let b = store.read(FRIENDS_LAYOUT).expect("layout payload");
    let l = LayoutDesc::read(FRIENDS_LAYOUT, &b, &types).expect("layout decodes");
    let all = flatten(&l);

    let list = all
        .get(&FRIENDS_LIST)
        .expect("the Friends list box is in 0x2100005D");
    assert_eq!(list.ty, dereth_ui::factory::ty::LISTBOX);
    let p = &list.base.properties;
    assert_eq!(p.get_bool(0x59), Some(true), "click select");
    assert_eq!(p.get_bool(0x61), Some(true), "selected item state change");
    assert_eq!(
        p.get_enum(0x5D),
        None,
        "no normal state named on the list box"
    );
    assert_eq!(
        p.get_enum(0x5E),
        None,
        "no selected state named on the list box"
    );

    // The list's own `0x64` array names the template: layout 0x2100005D, element 0x10000519.
    let Some(PropertyValue::Array(items)) = p.get(0x64) else {
        panic!("0x64 entry templates")
    };
    assert_eq!(items.len(), 1, "one entry template");
    let PropertyValue::Struct(members) = &items[0].value else {
        panic!("0x65 struct")
    };
    let member = |id: u32| {
        members
            .iter()
            .find(|(k, _)| *k == id)
            .map(|(_, v)| v.value.clone())
    };
    assert_eq!(member(0x63), Some(PropertyValue::DataFile(FRIENDS_LAYOUT)));
    assert_eq!(
        member(0x62),
        Some(PropertyValue::Enum(FRIENDS_ROW_TEMPLATE))
    );

    let row = all
        .get(&FRIENDS_ROW_TEMPLATE)
        .expect("the row template is in 0x2100005D");
    assert_eq!(
        row.ty,
        dereth_ui::factory::ty::FIELD,
        "a field element row root"
    );
    assert_eq!(
        row.states.keys().copied().collect::<Vec<_>>(),
        vec![StateId(6)]
    );
    let six = &row.states[&StateId(6)];
    assert!(six.properties.is_empty(), "state 6 changes no property");
    assert_eq!(six.media.len(), 1, "state 6 has one media entry");
    assert!(
        matches!(six.media[0].fields, MediaFields::Image { file, draw_mode: 1 } if file == SELECTED_PLATE),
        "and it is image media 0x06001AAF: {:?}",
        six.media[0]
    );
}

/// Behaviour: ui.list.pressing-a-row-draws-the-band-across-that-row-and-no-other
///
/// **Of the shipped list boxes that enable `0x61`, how many
/// have an entry template that authors a state 6? Those are the lists that highlight in retail
/// with no `0x5D`/`0x5E` of their own, through the master default alone.**
#[test]
fn every_state_change_list_whose_template_authors_state_6_highlights_through_the_default() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = master(&store).property_types();
    let mut layouts: BTreeMap<DataId, LayoutDesc> = BTreeMap::new();
    for did in 0x2100_0000_u32..=0x2100_0075 {
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
    // Every state a template declares, following a partial description's base chain.
    let states_of = |layout: DataId, element: u32| -> Vec<StateId> {
        let mut out = Vec::new();
        let (mut layout, mut element) = (layout, element);
        for _ in 0..8 {
            let Some(l) = layouts.get(&layout) else { break };
            let all = flatten(l);
            let Some(d) = all.get(&element) else { break };
            out.extend(d.states.keys().copied());
            if d.ty.0 != 0 {
                break;
            }
            layout = d.base_layout;
            element = d.base_element.0;
        }
        out.sort_unstable();
        out.dedup();
        out
    };

    let mut with_61 = 0;
    let mut highlighting: Vec<String> = Vec::new();
    for (did, l) in &layouts {
        for (id, d) in flatten(l) {
            if d.ty != dereth_ui::factory::ty::LISTBOX
                || d.base.properties.get_bool(0x61) != Some(true)
            {
                continue;
            }
            with_61 += 1;
            let names_pair = d.base.properties.get_enum(0x5E).is_some();
            let Some(PropertyValue::Array(items)) = d.base.properties.get(0x64) else {
                eprintln!("  {did} list {id:#010x}: 0x61 but no 0x64 entry templates");
                continue;
            };
            for it in items {
                let PropertyValue::Struct(members) = &it.value else {
                    continue;
                };
                let get = |k: u32| {
                    members
                        .iter()
                        .find(|(m, _)| *m == k)
                        .map(|(_, v)| v.value.clone())
                };
                let (Some(PropertyValue::DataFile(tl)), Some(PropertyValue::Enum(te))) =
                    (get(0x63), get(0x62))
                else {
                    continue;
                };
                let states = states_of(tl, te);
                eprintln!("  {did} list {id:#010x} template {tl}/{te:#010x} states {states:?}");
                if states.contains(&StateId(6)) {
                    highlighting.push(format!(
                        "{did} list {id:#010x} template {te:#010x}{}",
                        if names_pair {
                            " (names the pair)"
                        } else {
                            " (master default)"
                        }
                    ));
                }
            }
        }
    }
    for h in &highlighting {
        eprintln!("  {h}");
    }
    assert_eq!(
        with_61, 16,
        "sixteen list boxes enable the state change (journal_page_list's count)"
    );
    assert!(
        highlighting
            .iter()
            .any(|h| h.contains("list 0x10000517 template 0x10000519 (master default)")),
        "the Friends list highlights through the master default: {highlighting:?}"
    );
    assert!(
        highlighting
            .iter()
            .any(|h| h.contains("list 0x10000583 template 0x10000589 (master default)")),
        "and so does the Page List: {highlighting:?}"
    );
}

/// **The selection highlight the Page List reaches through property defaults.** See the module
/// header.
#[test]
fn the_page_list_row_authors_a_selected_look_and_its_list_names_no_state_pair() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let l = layout(&store, DataId(PAGE_LIST_LAYOUT));
    let all = flatten(&l);

    let row = all[&ROW_TEMPLATE];
    let six = row
        .states
        .get(&dereth_ui::StateId(6))
        .expect("the row template authors a state 6 -- the highlight");
    assert!(
        !six.media.is_empty(),
        "state 6 is an image swap and nothing else: {:?}",
        six.media
    );
    assert_ne!(
        six.media, row.base.media,
        "and it is a DIFFERENT image from the row's own, or there would be nothing to see"
    );

    let list = all[&LIST];
    assert_eq!(
        list.base.properties.get_bool(0x61),
        Some(true),
        "0x61 SelectedItemStateChange -- set_selected_item's two state writes are enabled"
    );
    assert_eq!(
        list.base.properties.get(0x5D),
        None,
        "...and 0x5D UnselectedState is absent from the element, so the enum-attribute read \
         falls back to the master-property default (1 for 0x5D, 6 for 0x5E; see the module \
         header)"
    );
    assert_eq!(
        list.base.properties.get(0x5E),
        None,
        "...and so is 0x5E SelectedState"
    );
}

/// **The calibration for the claim above: the attribute pair is not simply unused in the
/// shipped data.** Five shipped list boxes declare it, and they are the only five.
///
/// An instrument that cannot look reports absence. This is the proof that the absence measured
/// on `0x10000583` is a real negative and not a decoder that never reads `0x5D`/`0x5E` at all.
#[test]
fn only_the_five_vendor_lists_declare_the_selected_state_pair() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let types = types(&store);

    let mut declared: Vec<String> = Vec::new();
    let mut with_61 = 0_usize;
    for did in 0x2100_0000_u32..=0x2100_0075 {
        let did = DataId(did);
        if !store.exists(did) {
            continue;
        }
        let b = store.read(did).expect("layout payload");
        let l = LayoutDesc::read(did, &b, &types).expect("layout decodes");
        for (id, d) in flatten(&l) {
            if d.ty != dereth_ui::factory::ty::LISTBOX {
                continue;
            }
            let p = &d.base.properties;
            if p.get_bool(0x61) == Some(true) {
                with_61 += 1;
            }
            if let (Some(u), Some(s)) = (p.get_enum(0x5D), p.get_enum(0x5E)) {
                declared.push(format!("{did} {id:#010x} unselected={u} selected={s}"));
            }
        }
    }
    eprintln!("list boxes declaring 0x61: {with_61}");
    for d in &declared {
        eprintln!("  {d}");
    }
    assert_eq!(
        declared.len(),
        5,
        "the five vendor panes of 0x21000043 are the only lists in the shipped tree that name \
         their selected and unselected states: {declared:?}"
    );
    assert!(
        declared
            .iter()
            .all(|d| d.ends_with("unselected=1 selected=6")),
        "and they all agree that the selected look is state 6: {declared:?}"
    );
    assert_eq!(
        with_61, 16,
        "sixteen list boxes enable the state change; eleven of them, the Page List included, \
         name no states for it to push"
    );
}
