use dereth_primitives::{DataId, ObjectId};
use dereth_testkit::HeadlessClient;
use dereth_ui::{ElemHandle, ElementId};
use dereth_ui_screens::panels::spellcomponent;

use super::examine;

/// A client standing in the world with the spell page up and the components page bound, and
/// the five kinds one shipped formula's slots name: `(slot, kind, name, where it sorts)`.
pub fn a_page_and_five_kinds() -> (HeadlessClient, Vec<(u32, u32, String, u32)>) {
    let mut c = examine::a_client();
    examine::describe(&mut c);
    let _ = examine::stand_in_the_world(&mut c);
    let formula = examine::formula_of(&c, super::FLAME_BOLT);
    let kinds: Vec<(u32, u32, String, u32)> = {
        let cat = &c.view().world().magic.catalogue;
        formula
            .iter()
            .map(|scid| {
                let b = cat
                    .inq_spell_component_base(*scid)
                    .expect("the shipped table has a row for every slot");
                (*scid, cat.scid_to_wcid(*scid), b.name.clone(), b.category)
            })
            .collect()
    };
    assert_eq!(kinds.len(), 5, "the five slots of the bolt's formula");
    assert!(kinds.iter().all(|k| k.1 != 0), "each names a real kind");
    examine::open_the_spellbook(&mut c);
    c.tick(1);
    assert!(
        panel(&c).bound(),
        "the components page bound off the shipped tree"
    );
    assert_eq!(
        panel(&c).templates(),
        2,
        "with its header template and its row template"
    );
    (c, kinds)
}

pub fn panel(c: &HeadlessClient) -> &spellcomponent::SpellComponentPanel {
    &c.view().expect_app().hud().panels.spell_components
}

/// `(kind, element)` of every drawn row, in list order.
pub fn rows(c: &HeadlessClient) -> Vec<(u32, ElemHandle)> {
    panel(c).rows.iter().map(|r| (r.wcid, r.element)).collect()
}

/// The kinds the drawn headings stand for, in list order.
pub fn headers(c: &HeadlessClient) -> Vec<u32> {
    panel(c).headers.iter().map(|(k, _)| *k).collect()
}

pub fn rebuilds(c: &HeadlessClient) -> u32 {
    panel(c).rebuilds
}

/// The text of one column of a drawn row.
pub fn child_text(c: &mut HeadlessClient, row: ElemHandle, child: u32) -> Option<String> {
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell").ui;
    let h = ui.get_child_recursive(row, ElementId(child))?;
    ui.text_element_mut(h).map(|t| t.glyphs.inq_text(false))
}

/// The picture one column of a drawn row is showing.
pub fn row_image(c: &HeadlessClient, row: ElemHandle, child: u32) -> Option<DataId> {
    let ui = &c.view().expect_app().ui().expect("the UI shell").ui;
    let h = ui.get_child_recursive(row, ElementId(child))?;
    ui.node(h)?.region.image.as_ref().map(|g| g.did)
}

/// The held-count column of the row for `wcid`.
pub fn owned_text(c: &mut HeadlessClient, wcid: u32) -> Option<String> {
    let row = rows(c).into_iter().find(|(w, _)| *w == wcid)?.1;
    child_text(c, row, spellcomponent::row::OWNED)
}

/// A stack whose **own** picture is one no shipped component has, so the drawn picture has to
/// have come from the table.
pub fn carry_with_a_wrong_picture(c: &mut HeadlessClient, id: ObjectId, scid: u32, stack: u16) {
    examine::carry(c, id, scid, stack);
    let w = c.world_mut();
    if let Some(x) = w.tables.weenies.get_mut(id) {
        x.pwd.icon_id = super::A_WRONG_PICTURE;
    }
}
