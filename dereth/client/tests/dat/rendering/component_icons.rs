//! A component's icon in the retail interface's magic window: read from the world's files as an
//! icon layer, so on a February 2005 world its black is transparent where the later files store
//! alpha, and with its opaque white outline turned opaque black. The interface's own art from the
//! same files draws its black.
//!
//! Fixture: the February 2005 dats (`DERETH_TEST_PRETOD_DAT_DIR`) with the retail dats
//! (`DERETH_TEST_DAT_DIR`) beside them. A missing input fails.

use dereth_assets::Decode;
use dereth_client::textures::TextureStore;
use dereth_client::ui_draw::derive_plain;
use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;
use dereth_ui::region::SurfaceOp;
use dereth_ui::ImageSource;

/// The February 2005 world with the end-of-retail files beside it.
fn older_world() -> RetailDatStore {
    if let Some(msg) = dereth_dat::testing::pre_tod_shortfall() {
        panic!("{msg}");
    }
    let world = dereth_dat::testing::pre_tod_dat_dir().unwrap_or_default();
    RetailDatStore::open_pre_tod_with_later(&world, &dereth_dat::testing::dat_dir())
        .unwrap_or_else(|e| panic!("the February 2005 world did not open: {e}"))
}

/// The component table's icon for the first component, Lead Scarab.
fn first_component_icon(store: &RetailDatStore) -> DataId {
    let id = DataId(0x0E00_000F);
    let bytes = store.read_portal(id).expect("the component table");
    let table = dereth_assets::tables::SpellComponentTable::decode_payload(id, &bytes)
        .expect("the component table decodes");
    DataId(table.components.values().next().expect("a component").icon)
}

fn texels(d: &dereth_primitives::TextureData) -> Vec<u32> {
    d.levels[0]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|t| u32::from_le_bytes(*t))
        .collect()
}

const OUTLINE: SurfaceOp = SurfaceOp::ReplaceColor {
    from: SurfaceOp::OPAQUE_WHITE,
    to: SurfaceOp::OPAQUE_BLACK,
};

/// Behaviour: spell-components.strip.a-components-icon-has-a-black-outline-and-a-clear-surround
#[test]
fn a_components_icon_draws_a_clear_surround_and_a_black_outline_on_either_world() {
    let older = std::sync::Arc::new(older_world());
    let interface = older.interface_files();
    let eor = dereth_dat::testing::open_store_or_fail();
    let worlds: [(&str, &RetailDatStore, &RetailDatStore); 2] = [
        ("February 2005", &older, &interface),
        ("end of retail", &eor, &eor),
    ];
    for (name, world, chrome) in worlds {
        let icon = first_component_icon(world);
        let content = TextureStore::new(world);
        let chrome = TextureStore::new(chrome);
        let raw = texels(&content.texture_data(icon).expect("the icon decodes"));
        let white = raw
            .iter()
            .filter(|&&t| t == SurfaceOp::OPAQUE_WHITE)
            .count();
        assert!(white > 0, "{name}: the art has an opaque white outline");

        let drawn = texels(
            &derive_plain(&chrome, &content, icon, Some(OUTLINE), ImageSource::World)
                .expect("the icon decodes"),
        );
        assert!(
            drawn.iter().all(|&t| t != SurfaceOp::OPAQUE_WHITE),
            "{name}: no opaque white is left"
        );
        assert!(
            drawn
                .iter()
                .filter(|&&t| t == SurfaceOp::OPAQUE_BLACK)
                .count()
                >= white,
            "{name}: the outline is black"
        );
        assert_eq!(drawn[0] >> 24, 0, "{name}: the corner is clear");
    }
    // The 2005 icon itself stores no alpha: its corner is opaque black until it is read as an
    // icon, which is what made a black box around it.
    let icon = first_component_icon(&older);
    let raw = texels(
        &TextureStore::new(&older)
            .texture_data(icon)
            .expect("decodes"),
    );
    assert_eq!(raw[0], SurfaceOp::OPAQUE_BLACK);
}
