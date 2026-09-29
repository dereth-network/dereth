//! Behaviour: none (texture-chain resolution preserves record selection and palette bytes).

use dereth_assets::texture_lookup::{LookupError, TextureLookup};
use dereth_assets::{Decode, SurfaceTexture};
use dereth_dat::DbType;
use dereth_primitives::DataId;

#[test]
fn direct_surfaces_resolve_to_their_own_record_and_non_textures_are_rejected() {
    let store = dereth_dat::testing::open_store_or_fail();
    let lookup = TextureLookup::new(&store, 2);
    let id = DataId(0x0600_7576);
    let (resolved, surface, bytes) = lookup.resolve(id).expect("direct surface");
    assert_eq!(resolved, id);
    assert_eq!(surface.format, 500);
    assert_eq!(bytes, store.read_typed(DbType::RenderSurface, id).unwrap());
    assert!(matches!(
        lookup.resolve(DataId(0)),
        Err(LookupError::NotATexture(DataId(0)))
    ));
}

#[test]
fn two_level_chains_choose_high_detail_only_when_granted_and_requested() {
    let store = dereth_dat::testing::open_store_or_fail();
    let (id, levels) = store
        .ids_of(DbType::SurfaceTexture)
        .into_iter()
        .find_map(|id| {
            let bytes = store.read_typed(DbType::SurfaceTexture, id).ok()?;
            let texture = SurfaceTexture::decode_payload(id, &bytes).ok()?;
            (texture.source_levels.len() == 2).then_some((id, texture.source_levels))
        })
        .expect("a two-level retail texture");
    let before = TextureLookup::new(&store, 0);
    assert!(!before.keeps_high_detail());
    assert_eq!(before.resolve(id).unwrap().0, levels[1]);
    assert!(store.grant_highres().unwrap());
    let highest = TextureLookup::new(&store, 0);
    let medium = TextureLookup::new(&store, 2);
    assert!(highest.keeps_high_detail());
    assert!(!medium.keeps_high_detail());
    assert_eq!(highest.resolve(id).unwrap().0, levels[0]);
    assert_eq!(medium.resolve(id).unwrap().0, levels[1]);
}

#[test]
fn palette_lookup_preserves_every_stored_colour() {
    let store = dereth_dat::testing::open_store_or_fail();
    let lookup = TextureLookup::new(&store, 2);
    let id = store.ids_of(DbType::Palette)[0];
    let bytes = store.read_typed(DbType::Palette, id).unwrap();
    let expected = dereth_assets::Palette::decode_payload(id, &bytes).unwrap();
    assert_eq!(
        lookup.palette(id).unwrap().colors_argb,
        expected.colors_argb
    );
}
