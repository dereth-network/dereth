//! The retail interface's heritage page shows a button for each heritage the world offers and
//! none for the others; a world that offers every heritage its files define shows all thirteen.
//! Fixture: shipped layouts and character-generation tables loaded from the retail DATs.

use std::rc::Rc;

use crate::common::layout::RegistrationOrder;

use dereth_assets::Decode as _;
use dereth_primitives::AssetSource as _;
use dereth_ui::framework::ScreenCx;
use dereth_ui::framework::{DidMapperResolver, LayoutEnum, LayoutEnumResolver as _, Screen};
use dereth_ui_screens::screens::chargen::{
    CharGenScreen, CharGenTables, ColorWheelArt, HERITAGE_BUTTONS,
};

/// The world's creation tables, through the master map's `UNIQUEDB` group as the host loads
/// them.
fn tables(store: &dereth_dat::RetailDatStore) -> Rc<CharGenTables> {
    let r = DidMapperResolver::load_group(store, 2).expect("the UNIQUEDB group");
    let read = |e: u32| {
        let id = r.resolve(LayoutEnum(e)).expect("a table the map names");
        (id, store.read(id).expect("the table's payload"))
    };
    let (cg_id, cg) = read(0x0E);
    let (skill_id, skills) = read(4);
    let chargen = dereth_assets::tables::CharGen::decode_payload_in(
        store.container_era_of(cg_id),
        cg_id,
        &cg,
    )
    .expect("the character-generation table");
    let skills = dereth_assets::tables::SkillTable::decode_payload_in(
        store.container_era_of(skill_id),
        skill_id,
        &skills,
    )
    .expect("the skill table");
    Rc::new(CharGenTables {
        world: Rc::new(dereth_chargen::CreationTables {
            chargen,
            skills,
            clothing: Rc::new(std::collections::BTreeMap::new()),
        }),
        color_wheel_art: ColorWheelArt::default(),
        colors: None,
        texts: dereth_chargen::CreationTexts::default(),
    })
}

/// Which heritages have a button showing, once the wizard is given `heritages`' tables.
fn shown(heritages: Option<&[u32]>) -> Vec<u32> {
    let (mut ui, _flow, store) =
        crate::common::layout::load((800, 600), RegistrationOrder::BeforeResolver);
    let mut s = CharGenScreen::default();
    s.create(&mut ScreenCx::new(&mut ui))
        .expect("the char-gen screen builds");
    s.set_tables(CharGenTables::offering(&tables(&store), heritages));
    s.heritage_page_update(&mut ui);
    let root = s.roots()[0];
    HERITAGE_BUTTONS
        .iter()
        .filter(|(id, _)| {
            let h = ui
                .get_child_recursive(root, *id)
                .expect("a heritage button");
            ui.node(h).expect("alive").region.flags.visible
        })
        .map(|(_, heritage)| *heritage)
        .collect()
}

/// Behaviour: presentation.era.retail-creation-shows-only-the-offered-heritages
#[test]
fn the_heritage_page_shows_only_the_heritages_the_world_offers() {
    assert_eq!(shown(Some(&[1, 2, 3, 4, 9])), [1, 2, 3, 4, 9]);
    assert_eq!(shown(Some(&[1, 2, 3])), [1, 2, 3]);
    assert_eq!(shown(None), (1..=13).collect::<Vec<u32>>());
}
