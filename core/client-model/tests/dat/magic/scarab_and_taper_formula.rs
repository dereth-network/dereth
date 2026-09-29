//! The scarab-only filler is the prismatic taper; the school-to-foci map is in the dat; every
//! shipped spell has a long and a scarab+tapers formula; carrying the foci or the Infused
//! augmentation switches the formula.
//! Fixture: shipped formula tables, spell data and taboo patterns from the retail DATs.

#![allow(clippy::pedantic)]

use std::collections::BTreeMap;

use dereth_assets::tables::{DualDidMapper, SpellComponentTable, SpellTable};
use dereth_assets::Decode;
use dereth_client_model::magic::{
    decrypt_formula, scarab_only_filler_count, scarab_only_formula, ComponentBase,
    ComponentCatalogue, SpellFormulaKind, SCARAB_ONLY_FILLER_SCID,
};
use dereth_client_model::World;
use dereth_primitives::{AssetSource, DataId, ObjectId};
use dereth_protocol::types::PublicWeenieDesc;

// =================================================================================================
// 0. The literals this file pins, all re-derived from the shipped tables below.
// =================================================================================================

/// `SpellComponentTable 0x0E00000F` row 188 — the component `InqScarabOnlyFormula` pads with.
const PRISMATIC_TAPER_SCID: u32 = 188;
const PRISMATIC_TAPER_NAME: &str = "Prismatic Taper";

/// The dual-enum id map `0x27000003` (`SchoolOfMagic` / `ComponentPacks`) — the five Foci, school-keyed.
/// The same five values as `ACE.Server/WorldObjects/Player_Spells.cs`'s `FociWCIDs`.
const FOCI_WCIDS: [(u32, u32); 5] = [
    (1, 15271), // War                 -- ACE: Foci of Strife
    (2, 15270), // Life                -- ACE: Foci of Verdancy
    (3, 15269), // ItemEnchantment     -- ACE: Foci of Artifice
    (4, 15268), // CreatureEnchantment -- ACE: Foci of Enchantment
    (5, 43173), // Void                -- ACE: Foci of Shadow
];

const REGENERATION_SELF_V: u32 = 169;
/// `Flame Bolt I` — War Magic, five slots.
const FLAME_BOLT: u32 = 27;
/// Its decrypted dat formula: a **Gold Scarab** (power 5) and seven reagents.
const REGEN_DAT_FORMULA: [u32; 8] = [5, 74, 11, 64, 26, 41, 70, 61];
/// And its foci formula: the scarab, then four prismatic tapers. Power 5 -> 4 fillers.
const REGEN_FOCI_FORMULA: [u32; 8] = [5, 188, 188, 188, 188, 0, 0, 0];

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const FOCI: ObjectId = ObjectId(0x5000_0002);
/// `AugmentationInfusedLifeMagic` — the int quality `get_appropriate_spell_formula` reads for school 2.
const AUG_INFUSED_LIFE: u32 = 0x128;

fn store() -> dereth_dat::RetailDatStore {
    let dir = dereth_dat::testing::dat_dir();
    assert!(
        dereth_dat::testing::have_dats(),
        "the spell and component tables live in client_portal.dat: none at {} -- set \
         $DERETH_TEST_DAT_DIR",
        dir.display()
    );
    dereth_dat::RetailDatStore::open_dir(&dir).expect("the retail dat store opens")
}

fn component_table(store: &dereth_dat::RetailDatStore) -> SpellComponentTable {
    let cid = DataId(0x0E00_000F);
    SpellComponentTable::decode_payload(cid, &store.read(cid).expect("SpellComponentTable"))
        .expect("the component table decodes")
}

fn spell_table(store: &dereth_dat::RetailDatStore) -> SpellTable {
    let sid = DataId(0x0E00_000E);
    SpellTable::decode_payload(sid, &store.read(sid).expect("the spell table"))
        .expect("the spell table decodes")
}

/// The component names a formula's non-zero slots spell out, in slot order — the lines
/// the examine pane writes under ` COMPONENTS:`.
fn names_of(t: &SpellComponentTable, formula: [u32; 8]) -> Vec<String> {
    formula
        .iter()
        .filter(|c| **c != 0)
        .map(|c| {
            t.components
                .get(c)
                .expect("a shipped component")
                .name
                .clone()
        })
        .collect()
}

fn tapers(n: usize) -> Vec<String> {
    std::iter::repeat_n(PRISMATIC_TAPER_NAME.to_owned(), n).collect()
}

// =================================================================================================
// 1. The dat, so the constants above are the shipped tables' and not this file's
// =================================================================================================

/// **SCID 188 is the Prismatic Taper**, and the name is the dat's, not the client's.
///
/// The retail client's own strings hold exactly two `Prismatic`s, both the damage type. The component name can
/// only come from `SpellComponentTable 0x0E00000F`, nibble-swapped like every other one.
#[test]
fn the_filler_the_scarab_only_formula_pads_with_is_the_prismatic_taper() {
    let t = component_table(&store());
    assert_eq!(SCARAB_ONLY_FILLER_SCID, PRISMATIC_TAPER_SCID);
    let c = t
        .components
        .get(&PRISMATIC_TAPER_SCID)
        .expect("row 188 ships");
    assert_eq!(c.name, PRISMATIC_TAPER_NAME);
    assert_eq!(c.component_type, 6, "the taper component type");
    assert_eq!(c.category, 5, "the taper category");
    assert_ne!(c.icon, 0, "the pane has an icon to blit for it");

    assert!(!(63..=74).contains(&PRISMATIC_TAPER_SCID));
    assert_eq!(t.components.get(&63).expect("SCID 63").name, "Red Taper");
    assert_eq!(t.components.get(&74).expect("SCID 74").name, "Grey Taper");
}

/// **Resolve the five school-to-class-id mappings from the shipped DAT.**
///
/// The school-to-class-id lookup uses group `0x10000001` and property `0x28`: the root
/// DID mapper `0x25000000` maps group `0x10000001` to `WEENIE_CATEGORIES` `0x25000005`, which maps
/// value `4` (`SchoolOfMagic`) to dual enum-to-id map `0x27000003`. Resolved by that walk here, so a
/// dat patch that moves the object still passes.
#[test]
fn the_school_to_foci_wcid_mapper_is_in_the_dat_and_holds_the_five_foci() {
    let store = store();
    let id = dereth_assets::did_by_enum(&store, 0x1000_0001, 4)
        .expect("group 0x10000001 value 4 resolves");
    assert_eq!(
        id,
        DataId(0x2700_0003),
        "this dat build's SchoolOfMagic enum-to-data-id map"
    );
    let m = DualDidMapper::decode_payload(id, &store.read(id).expect("bytes")).expect("it decodes");

    let rows: BTreeMap<u32, u32> = m.0.enum_to_id.iter().copied().collect();
    let names: BTreeMap<u32, String> = m.0.enum_to_name.iter().cloned().collect();
    assert_eq!(
        rows.len(),
        6,
        "five schools plus the mapper's own Undef row"
    );
    assert_eq!(rows.get(&0), Some(&0), "row 0 is Undef -> 0");
    for (school, wcid) in FOCI_WCIDS {
        assert_eq!(rows.get(&school), Some(&wcid), "school {school}");
    }
    for (school, name) in [
        (1u32, "War"),
        (2, "Life"),
        (3, "ItemEnchantment"),
        (4, "CreatureEnchantment"),
        (5, "Void"),
    ] {
        assert_eq!(names.get(&school).map(String::as_str), Some(name));
    }

    // The sibling walk the component tracker already uses, asserted beside it so that "value 4"
    // cannot quietly be the SCID map. The WCID-to-SCID map is value **3**.
    assert_eq!(
        dereth_assets::did_by_enum(&store, 0x1000_0001, 3),
        Some(DataId(0x2700_0002)),
        "SpellComponents, a different mapper",
    );
}

/// The two formulas for one real spell, both out of the shipped table.
#[test]
fn a_shipped_spell_has_a_long_formula_and_a_scarab_plus_four_tapers_one() {
    let store = store();
    let t = component_table(&store);
    let b = spell_table(&store)
        .spells
        .get(&REGENERATION_SELF_V)
        .expect("spell 169")
        .clone();
    assert_eq!(b.name, "Regeneration Self V");
    assert_eq!(b.school, 2, "Life Magic");

    let plain = decrypt_formula(&b.raw_comps, b.comp_key);
    assert_eq!(plain, REGEN_DAT_FORMULA);
    assert_eq!(scarab_only_formula(&plain), REGEN_FOCI_FORMULA);
    assert_eq!(scarab_only_filler_count(5), 4, "a Gold Scarab is power 5");

    let mut want = vec!["Gold Scarab".to_owned()];
    want.extend(tapers(4));
    assert_eq!(names_of(&t, REGEN_FOCI_FORMULA), want);
    assert_eq!(
        names_of(&t, REGEN_DAT_FORMULA).len(),
        8,
        "the other mode is eight lines"
    );
}

/// Over the **whole** shipped table: every spell's foci formula is its own power components in
/// order, then nothing but prismatic tapers, and the taper count is
/// [`scarab_only_filler_count`] of the strongest of them.
///
/// This is what makes "scarab + N prismatic tapers" a rule rather than one spell's accident.
#[test]
fn every_shipped_spells_foci_formula_is_scarabs_then_prismatic_tapers() {
    let store = store();
    let spells = spell_table(&store);
    let mut taper_counts: BTreeMap<u32, usize> = BTreeMap::new();
    let mut longer = 0usize;
    let mut capped = 0usize;
    let mut checked = 0usize;
    for b in spells.spells.values() {
        let plain = decrypt_formula(&b.raw_comps, b.comp_key);
        let foci = scarab_only_formula(&plain);
        let kept: Vec<u32> = foci.iter().copied().take_while(|c| *c != 0).collect();
        let n_tapers =
            u32::try_from(kept.iter().filter(|c| **c == PRISMATIC_TAPER_SCID).count()).unwrap();
        let (scarabs, rest) = kept.split_at(kept.len() - n_tapers as usize);
        assert!(
            rest.iter().all(|c| *c == PRISMATIC_TAPER_SCID),
            "{:?}: {kept:?}",
            b.name
        );

        // The prefix is the plain formula's power components, in the plain formula's order --
        // `InqScarabOnlyFormula`'s `switch` keep-list, breaking at the first zero slot.
        let want: Vec<u32> = plain
            .iter()
            .copied()
            .take_while(|c| *c != 0)
            .filter(|c| matches!(*c, 1..=6 | 0x6E | 0x6F | 0x70 | 0xC0 | 0xC1))
            .collect();
        assert_eq!(scarabs, want.as_slice(), "{:?}", b.name);

        // The most-powerful-power-component search over the kept list, then the `switch` --
        // **capped by the eight slots**, because the slot write opens
        // with `if (7 < index) return 0;` and the loop that appends tapers ignores the answer.
        // A seven-scarab spell (`Impulse`, `[3;7] + 60`) therefore gets **one** taper where the
        // switch asked for three, and the other two are written nowhere.
        let power = scarabs
            .iter()
            .copied()
            .map(dereth_client_model::magic::scarab_power_level)
            .max();
        let asked = scarab_only_filler_count(power.unwrap_or(0));
        let room = u32::try_from(8 - scarabs.len()).unwrap();
        assert_eq!(
            n_tapers,
            asked.min(room),
            "{:?} {plain:?} -> {foci:?}",
            b.name
        );
        if asked > room {
            capped += 1;
        }

        let n = |f: &[u32; 8]| f.iter().filter(|c| **c != 0).count();
        if n(&foci) > n(&plain) {
            longer += 1;
        }
        *taper_counts.entry(n_tapers).or_insert(0) += 1;
        checked += 1;
    }
    assert_eq!(checked, 6266, "the whole shipped table");
    // Counts 0..=4 only -- `scarab_only_filler_count`'s whole range, nothing else.
    assert!(taper_counts.keys().all(|k| *k <= 4), "{taper_counts:?}");
    assert!(taper_counts.contains_key(&4), "{taper_counts:?}");
    // **A shipped quirk, recorded rather than tidied.** For a handful of spells the foci formula
    // is *longer* than the dat's own (a one-slot formula whose single scarab is high power grows
    // four tapers). That is the client's arithmetic and the pane would draw it, so it is pinned
    // as a count rather than forbidden.
    assert!(longer > 0, "the quirk exists");
    assert!(
        longer < checked / 10,
        "and it is a handful: {longer} of {checked}"
    );

    // **And a measured divergence from ACE.** `ACE.Server/Entity/SpellFormula.cs` builds
    // its Foci formula by appending tapers to an uncapped list, so for every one of these
    // spells ACE's required-component check asks
    // for *more* prismatic tapers than this client shows in the pane and spends on the cast.
    // The client is the authority for what the pane draws; the shard is the authority for what
    // burns. Nothing here changes either -- this records the compatibility difference.
    assert!(capped > 0, "the cap fires on the shipped table");
    assert!(
        capped < checked / 10,
        "on a minority of spells: {capped} of {checked}"
    );
}

// =================================================================================================
// 2. The model: the selector, and both of its routes to the tapers
// =================================================================================================

fn world_with(t: &SpellComponentTable) -> World {
    let mut w = World::new();
    w.set_player(PLAYER);
    let mut me = dereth_client_model::weenie::Weenie::new(PLAYER);
    me.pwd = PublicWeenieDesc {
        name: "Tester".into(),
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(PLAYER, me);
    // The player-description interface reads the augmentation
    // ints off it, and a null pointer is the "not in world yet" arm rather than this one.
    if let Some(x) = w.tables.weenies.get_mut(PLAYER) {
        x.qualities = Some(dereth_client_model::qualities::Qualities::default());
    }
    w.tables.inventories.insert(
        PLAYER,
        dereth_client_model::objects::ObjectInventory::new(PLAYER),
    );
    w.magic.catalogue = ComponentCatalogue::new(
        Vec::new(),
        t.components.iter().map(|(scid, c)| {
            (
                *scid,
                ComponentBase {
                    name: c.name.clone(),
                    category: c.category,
                    icon: c.icon,
                },
            )
        }),
    );
    w
}

/// Put a Foci in the player's **side-pack list**, which is the only list
/// the magic-pack ownership test walks — the side-pack list, not the items list.
fn carry_the_foci(w: &mut World, wcid: u32) {
    let mut foci = dereth_client_model::weenie::Weenie::new(FOCI);
    foci.pwd = PublicWeenieDesc {
        name: "Foci of Verdancy".into(),
        wcid,
        ..PublicWeenieDesc::default()
    };
    w.tables.weenies.insert(FOCI, foci);
    let mut inv = w.inventory(PLAYER).cloned().expect("an inventory");
    inv.containers.push(FOCI);
    w.tables.inventories.insert(PLAYER, inv);
}

/// Behaviour: magic.formula.carrying-the-foci-or-the-augmentation-switches-to-scarab-plus-prismatic-tapers
/// **The reported question, as a model assertion.** With the dat's mapper in place, a character
/// carrying the Life Foci is shown and charged scarab + four prismatic tapers; the same character
/// without it gets the eight-slot per-account formula.
#[test]
fn carrying_the_foci_switches_the_formula_to_scarab_plus_prismatic_tapers() {
    let store = store();
    let t = component_table(&store);
    let spells = spell_table(&store);
    let b = spells
        .spells
        .get(&REGENERATION_SELF_V)
        .expect("spell 169")
        .clone();

    let mut w = world_with(&t);
    w.player_system.account = "dereunit116f".to_owned();

    assert_eq!(
        w.school_of_magic_to_wcid(2),
        0,
        "an empty map before the dat's rows arrive"
    );
    assert_eq!(
        w.get_appropriate_spell_formula(2, false),
        SpellFormulaKind::Customized
    );
    let full = w.spell_formula(&b);
    assert_eq!(full.iter().filter(|c| **c != 0).count(), 8);
    assert_eq!(full[0], 5, "the Gold Scarab is slot 0 either way");
    assert_ne!(full, REGEN_FOCI_FORMULA);
    assert!(!names_of(&t, full).contains(&PRISMATIC_TAPER_NAME.to_owned()));

    // The dat's mapper, then the Foci in a side pack.
    w.magic.school_pack_wcid = FOCI_WCIDS.into_iter().collect();
    assert_eq!(
        w.school_of_magic_to_wcid(2),
        15270,
        "Life -> Foci of Verdancy"
    );
    assert!(!w.magic_pack_is_owned(15270), "not carried yet");
    carry_the_foci(&mut w, 15270);
    assert!(w.magic_pack_is_owned(15270));

    assert_eq!(w.spell_formula(&b), REGEN_FOCI_FORMULA);
    let mut want = vec!["Gold Scarab".to_owned()];
    want.extend(tapers(4));
    assert_eq!(names_of(&t, w.spell_formula(&b)), want);

    // ...and it is **per school**. A War spell is untouched by the Life Foci.
    let war = spells
        .spells
        .get(&FLAME_BOLT)
        .expect("Flame Bolt I")
        .clone();
    assert_eq!(war.school, 1);
    assert!(
        !w.magic_pack_is_owned(w.school_of_magic_to_wcid(1)),
        "no War Foci is carried"
    );
    assert_eq!(w.spell_formula(&war).iter().filter(|c| **c != 0).count(), 5);
    assert!(!names_of(&t, w.spell_formula(&war)).contains(&PRISMATIC_TAPER_NAME.to_owned()));
}

/// The infused magic augmentation reaches the same taper formula.
#[test]
fn the_infused_magic_augmentation_reaches_the_same_taper_formula() {
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};
    let store = store();
    let t = component_table(&store);
    let b = spell_table(&store)
        .spells
        .get(&REGENERATION_SELF_V)
        .expect("spell 169")
        .clone();

    let mut w = world_with(&t);
    w.magic.school_pack_wcid = FOCI_WCIDS.into_iter().collect();
    assert!(
        !w.magic_pack_is_owned(w.school_of_magic_to_wcid(2)),
        "no foci is carried"
    );
    assert_eq!(w.spell_formula(&b).iter().filter(|c| **c != 0).count(), 8);

    w.player_qualities_mut().expect("qualities").set(
        StatKey::new(StatType::Int, AUG_INFUSED_LIFE),
        StatValue::Int(1),
    );
    assert_eq!(
        w.get_appropriate_spell_formula(2, false),
        SpellFormulaKind::ScarabOnly
    );
    assert_eq!(w.spell_formula(&b), REGEN_FOCI_FORMULA);
}
