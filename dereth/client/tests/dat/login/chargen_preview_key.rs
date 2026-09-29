//! The two Olthoi heritages reuse ids across the columns the character-creation preview key reads
//! (their sexes share one setup, and the two share an environment setup), which is why the preview
//! key carries the two animation enums as well. Fixture: `CharGen_CharacterData 0x0E000002` from
//! the retail dats (missing dats fail).

use dereth_dat::RetailDatStore;
use dereth_primitives::DataId;

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

/// Behaviour: login.chargen.two-olthoi-heritages-reuse-preview-ids
/// The only heritages whose preview animation enums differ from the default are Olthoi (`0x0C`)
/// and OlthoiAcid (`0x0D`). Their setups differ, so a `(setup, animating, bg_setup, objdesc)` key
/// alone would change on every reachable transition, but only because of this dat build: those two
/// heritages share one `environment_setup`, and both sexes inside each of them share one setup —
/// so the heritage table demonstrably *does* reuse ids across rows, and the containment was a
/// property of this dat build rather than of the code. If a patch ever makes the two setups equal,
/// this test says so and the fix is already in place.
#[test]
fn the_two_olthoi_heritages_reuse_ids_across_the_columns_the_preview_key_reads() {
    use dereth_assets::Decode;
    use dereth_ui_screens::screens::chargen::{
        ANIM_ENUMS_DEFAULT, ANIM_ENUMS_OLTHOI, ANIM_ENUMS_OLTHOI_ACID,
    };

    const OLTHOI: u32 = 0x0C;
    const OLTHOI_ACID: u32 = 0x0D;

    let s = store();
    let bytes = dereth_primitives::AssetSource::read(&s, DataId(0x0E00_0002))
        .expect("CharGen_CharacterData 0x0E000002");
    let cg = dereth_assets::tables::CharGen::decode_payload(DataId(0x0E00_0002), &bytes)
        .expect("decode the char-gen table");

    // The three animation-enum groups selected by character generation are all
    // distinct, so a heritage change between any two of them moves the enums.
    assert_ne!(ANIM_ENUMS_DEFAULT.0, ANIM_ENUMS_OLTHOI.0);
    assert_ne!(ANIM_ENUMS_OLTHOI.0, ANIM_ENUMS_OLTHOI_ACID.0);
    assert_ne!(ANIM_ENUMS_DEFAULT.1, ANIM_ENUMS_OLTHOI.1);

    let h = |k: u32| {
        cg.heritage_groups
            .get(&k)
            .unwrap_or_else(|| panic!("heritage {k:#X}"))
    };
    let (a, b) = (h(OLTHOI), h(OLTHOI_ACID));
    eprintln!(
        "heritage {:#04X} {:?} setup {:#010X} env {:#010X}; {:#04X} {:?} setup {:#010X} env {:#010X}",
        OLTHOI, a.name, a.setup.0, a.environment_setup.0,
        OLTHOI_ACID, b.name, b.setup.0, b.environment_setup.0,
    );

    // The half that made the old key fire.
    assert_ne!(
        a.setup, b.setup,
        "the two Olthoi heritages now share a setup: the old preview key would have been silent \
         across this transition. The key already carries the animation enums, so nothing is \
         broken -- but the data no longer contains the omission on its own"
    );

    // The half that says the containment was data and not construction: this table reuses ids.
    assert_eq!(
        a.environment_setup, b.environment_setup,
        "the two Olthoi heritages no longer share an environment_setup -- re-measure this note"
    );
    for (k, g) in [(OLTHOI, a), (OLTHOI_ACID, b)] {
        let setups: Vec<u32> = g.sexes.values().map(|s| s.setup.0).collect();
        assert!(
            setups.len() > 1,
            "heritage {k:#X} has one sex, so it cannot demonstrate reuse"
        );
        assert!(
            setups.windows(2).all(|w| w[0] == w[1]),
            "heritage {k:#X}'s sexes no longer share one setup: {setups:#X?}"
        );
    }
}
