//! The journal's Page List buttons, read off the shipped layout `0x21000067`. Its "Reset" control
//! (`0x10000588`) is the **search** box's reset: its arm clears the search box, rebuilds the page
//! list and sorts it, and never touches the selection or deletes a page. The seven buttons (four
//! sort headers, Delete, "Search:" and "Reset") derive from one base element, so "Search:" is a
//! button despite reading as a label. The Page List's selected-row look is in `list_row_highlight`.
//!
//! Fixture: the shipped layouts and string table in the retail dats.
//!
//! Behaviour: none (a census of the shipped Page List layout)

use std::collections::BTreeMap;

use dereth_primitives::{AssetSource, DataId};

use dereth_ui::desc::{ElementDesc, LayoutDesc};
use dereth_ui::ElementId;

/// The page-list panel's layout.
const PAGE_LIST_LAYOUT: u32 = 0x2100_0067;

fn store() -> Option<dereth_dat::RetailDatStore> {
    let dir = dereth_dat::testing::dat_dir();
    if !dereth_dat::testing::have_dats() {
        eprintln!("no client_local_English.dat under {}", dir.display());
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

/// `client_local_English.dat`'s `0x23000001`, the table every caption in `0x21000067` names.
fn string_table(store: &dereth_dat::RetailDatStore, did: DataId) -> BTreeMap<u32, String> {
    let b = store.read(did).expect("string table");
    let t = <dereth_assets::ui::StringTable as dereth_assets::Decode>::decode_payload(did, &b)
        .expect("string table decodes");
    t.strings
        .into_iter()
        .map(|(k, v)| (k, v.strings.first().cloned().unwrap_or_default()))
        .collect()
}

fn layout(store: &dereth_dat::RetailDatStore, did: DataId) -> LayoutDesc {
    let b = store.read(did).expect("layout payload");
    LayoutDesc::read(did, &b, &types(store)).expect("layout decodes")
}

/// Every element of a layout, flattened, keyed by element id.
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

/// **The captions, which settle what each control is.**
///
/// `0x17` is a text element's string attribute. The four sort headers, Delete, "Search:"
/// and "Reset" all derive from the same base element (`0x1000057E` of layout `0x21000066`), so
/// all seven are the same kind of button and "Search:" is a *button* despite reading as a label.
#[test]
fn the_page_lists_seven_buttons_carry_their_shipped_captions() {
    let Some(store) = store() else {
        panic!("the retail dats are this file's oracle")
    };
    let l = layout(&store, DataId(PAGE_LIST_LAYOUT));
    let tbl = string_table(&store, DataId(0x2300_0001));
    let all = flatten(&l);

    let caption = |id: u32| -> String {
        let d = all
            .get(&id)
            .unwrap_or_else(|| panic!("{id:#010x} is in {PAGE_LIST_LAYOUT:#010x}"));
        let si = d
            .base
            .properties
            .get_string_info(0x17)
            .unwrap_or_else(|| panic!("{id:#010x} carries no 0x17 StringInfo"));
        si.literal
            .clone()
            .or_else(|| si.string_id.and_then(|s| tbl.get(&s).cloned()))
            .unwrap_or_default()
    };

    assert_eq!(caption(0x1000_057F), "#", "sort by page number");
    assert_eq!(caption(0x1000_0580), "Title");
    assert_eq!(caption(0x1000_0581), "Timer");
    assert_eq!(caption(0x1000_0582), "Label");
    assert_eq!(
        caption(0x1000_0585),
        "Delete",
        "the button that deletes the selected page"
    );
    assert_eq!(
        caption(0x1000_0586),
        "Search:",
        "its arm rebuilds the page list with the filter on. It reads as a label and it is a \
         button: there is no live filter in this panel and the edit box 0x10000587 maps to an arm \
         which does nothing, so pressing Enter in the box cannot search either"
    );
    assert_eq!(
        caption(0x1000_0588),
        "Reset",
        "its arm clears the search box, rebuilds the list and sorts it. This is the \
         SEARCH box's reset and has nothing to do with the selected row"
    );

    // All seven are the same base element, so "Search:" is not a differently-typed label.
    for id in [
        0x1000_057F,
        0x1000_0580,
        0x1000_0581,
        0x1000_0582,
        0x1000_0585,
        0x1000_0586,
        0x1000_0588,
    ] {
        let d = all[&id];
        assert_eq!(d.base_element.0, 0x1000_057E, "{id:#010x}'s base element");
        assert_eq!(
            d.base_layout,
            DataId(0x2100_0066),
            "{id:#010x}'s base layout"
        );
    }
}
