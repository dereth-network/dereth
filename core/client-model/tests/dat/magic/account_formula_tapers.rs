//! The shipped table has no holed formula; the account hash is the client string hash; versions
//! 1/2/3 randomise taper slots and unknown versions change nothing; other accounts get other
//! tapers; the world and the cast path use the account formula; the ACE port agrees over the whole
//! table.
//! Fixture: shipped formula tables, spell data and taboo patterns from the retail DATs.

#![allow(clippy::pedantic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use dereth_assets::tables::{SpellBase, SpellTable};
use dereth_assets::Decode;
use dereth_client_model::{RecordingRequests, RecordingSink, Request, World};
use dereth_dat::RetailDatStore;
use dereth_primitives::{AssetSource, DataId, ObjectId};
use {
    dereth_client_model::magic::account_name_hash, dereth_client_model::magic::randomize_for_name,
    dereth_client_model::magic::ComponentBase, dereth_client_model::magic::ComponentCatalogue,
    dereth_client_model::magic::LOWEST_TAPER_ID, dereth_client_model::magic::NUM_TAPERS,
    dereth_rules::magic::num_spell_components,
};

// =================================================================================================
// 0. The literals — every one re-read from the shipped table by station 0, never through the
//    symbol the production code reads it through.
// =================================================================================================

/// The synthetic account this file derives tapers for. **Not a credential**: it is a string.
const ACCOUNT: &str = "dereunit1118";
/// A second synthetic account, to show the tapers actually move with the name.
const OTHER_ACCOUNT: &str = "P1118Tester";

/// `compute_hash("dereunit1118")`, computed by hand from the hash loop.
const ACCOUNT_HASH: u32 = 61_589_640;
const OTHER_ACCOUNT_HASH: u32 = 234_876_546;

/// The spell table, `0x0E00000E`.
const SPELL_TABLE: u32 = 0x0E00_000E;

/// `Acid Stream V` — formula version 1 with all **eight** slots, so all three taper arms fire.
const ACID_STREAM_V: u32 = 62;
/// `Primary Portal Tie` — version 1 with **seven** slots: tapers at 1 and 3, none at 6.
const PRIMARY_PORTAL_TIE: u32 = 47;
/// `Acid Stream II` — version 1 with **six** slots: the taper at 1 only, and that arm never reads
/// the seed, so this formula is the same for every account.
const ACID_STREAM_II: u32 = 59;
/// `Strength Self I` — version 1 with **five** slots and no taper at all.
const STRENGTH_SELF_I: u32 = 2;
/// `Regeneration Self V` — version 1, eight slots, and `_bitfield & 8` self-targeted, which is
/// what lets the cast station reach the wire without a selection.
const REGENERATION_SELF_V: u32 = 169;
/// `Searing Disc` — formula version **2**.
const SEARING_DISC: u32 = 1783;
/// `Evaporate All Magic Other` — formula version **3**, eight slots.
const EVAPORATE_ALL_MAGIC: u32 = 1847;
/// `Aerfalle's Touch` — formula version 3 with **seven** slots, so slot 7 is 0. This is ACE's
/// explicit `comps.Count < 8` arm (`SpellTable.cs:162`).
const AERFALLES_TOUCH: u32 = 2697;

/// The decrypted formulas the shipped dat stores, slot by slot. Station 0 re-derives all of them.
const DAT_ACID_STREAM_V: [u32; 8] = [5, 68, 15, 70, 34, 37, 66, 55];
const DAT_PRIMARY_PORTAL_TIE: [u32; 8] = [3, 73, 21, 66, 32, 42, 59, 0];
const DAT_ACID_STREAM_II: [u32; 8] = [2, 65, 15, 34, 37, 55, 0, 0];
const DAT_STRENGTH_SELF_I: [u32; 8] = [1, 7, 33, 44, 60, 0, 0, 0];
const DAT_REGENERATION_SELF_V: [u32; 8] = [5, 74, 11, 64, 26, 41, 70, 61];
const DAT_SEARING_DISC: [u32; 8] = [110, 110, 19, 67, 34, 37, 63, 58];
const DAT_EVAPORATE_ALL_MAGIC: [u32; 8] = [1, 110, 8, 73, 29, 111, 66, 50];
const DAT_AERFALLES_TOUCH: [u32; 8] = [4, 63, 65, 66, 67, 69, 50, 0];

/// What [`ACCOUNT`] turns each of those into. Computed by hand from the retail rule and
/// cross-checked against an independent transcription of ACE's C#; station 6 re-derives every one
/// of them from that second transcription over the whole table.
const ACCT_ACID_STREAM_V: [u32; 8] = [5, 68, 15, 72, 34, 37, 63, 55];
const ACCT_PRIMARY_PORTAL_TIE: [u32; 8] = [3, 73, 21, 63, 32, 42, 59, 0];
const ACCT_REGENERATION_SELF_V: [u32; 8] = [5, 74, 11, 70, 26, 41, 68, 61];
const ACCT_SEARING_DISC: [u32; 8] = [110, 110, 19, 72, 34, 37, 71, 58];
const ACCT_EVAPORATE_ALL_MAGIC: [u32; 8] = [1, 110, 8, 71, 29, 111, 69, 50];
const ACCT_AERFALLES_TOUCH: [u32; 8] = [4, 63, 65, 69, 67, 69, 67, 0];

/// And what [`OTHER_ACCOUNT`] turns three of them into — different tapers, same reagents.
const OTHER_ACID_STREAM_V: [u32; 8] = [5, 68, 15, 66, 34, 37, 63, 55];
const OTHER_REGENERATION_SELF_V: [u32; 8] = [5, 74, 11, 69, 26, 41, 63, 61];
const OTHER_AERFALLES_TOUCH: [u32; 8] = [4, 63, 65, 73, 67, 69, 64, 0];

/// Before `0xF658` arrives, the account name is the empty string,
/// whose hash is 0. Not a placeholder — the seed is genuinely zero, so every `seed / x` is 0 and
/// slots 3 and 6 come out as the bare `LOWEST_TAPER_ID` on the two versions that divide.
const EMPTY_ACID_STREAM_V: [u32; 8] = [5, 68, 15, 63, 34, 37, 63, 55];

/// The version histogram of the shipped spell table. There is no fourth value, so
/// The per-name randomizer's `default: return 0` arm is unreachable from the dat.
const SHIPPED_VERSIONS: [(u32, usize); 3] = [(1, 5888), (2, 163), (3, 215)];

fn store() -> RetailDatStore {
    dereth_dat::testing::open_store_or_fail()
}

fn shipped_spell_table() -> SpellTable {
    let s = store();
    let id = DataId(SPELL_TABLE);
    SpellTable::decode_payload(id, &s.read(id).expect("the spell table 0x0E00000E"))
        .expect("the spell table decodes")
}

/// The spell's own stored formula — the eight stored slots with the component key taken
/// off the non-zero ones, **positions preserved**.
fn dat_slots(b: &SpellBase) -> [u32; 8] {
    dereth_client_model::magic::decrypt_formula(&b.raw_comps, b.comp_key)
}

// =================================================================================================
// 1. The dat oracle
// =================================================================================================

/// Every literal above is the shipped table's, and no shipped formula has an interior hole.
///
/// The hole question is not decorative: ACE indexes a **dense** `List<uint>` (`SpellBase.cs:99`
/// drops every zero slot) while the client indexes the eight-slot array. The two agree only while
/// the non-zero slots are a prefix — so that is asserted over all 6266 spells rather than assumed.
#[test]
fn the_shipped_table_carries_these_formulas_and_has_no_holed_one() {
    let t = shipped_spell_table();
    assert!(
        t.spells.len() > 6000,
        "a real table, not an empty decode: {}",
        t.spells.len()
    );

    let mut versions: BTreeMap<u32, usize> = BTreeMap::new();
    let mut holed = Vec::new();
    for (id, b) in &t.spells {
        *versions.entry(b.formula_version).or_default() += 1;
        let d = dat_slots(b);
        let n = num_spell_components(&d);
        if d[..n].iter().any(|c| *c == 0) {
            holed.push(*id);
        }
        // ACE's `SpellBase.cs:136` clamps a decrypted component above 198 with `& 0xFF`; the
        // client does no such thing. It never matters for this dat, and that is asserted rather
        // than assumed, because if it did the two ports would disagree on the reagents too.
        assert!(d.iter().all(|c| *c <= 198), "spell {id} decrypts to {d:?}");
    }
    assert!(
        holed.is_empty(),
        "formulas with an interior zero: {holed:?}"
    );
    assert_eq!(
        versions.into_iter().collect::<Vec<_>>(),
        SHIPPED_VERSIONS.to_vec(),
        "the formula-version histogram"
    );

    let pin = |id: u32, name: &str, version: u32, slots: [u32; 8]| {
        let b = t.spells.get(&id).unwrap_or_else(|| panic!("spell {id}"));
        assert_eq!(b.name, name, "spell {id}");
        assert_eq!(b.formula_version, version, "spell {id} formula version");
        assert_eq!(dat_slots(b), slots, "spell {id} decrypted slots");
    };
    pin(ACID_STREAM_V, "Acid Stream V", 1, DAT_ACID_STREAM_V);
    pin(
        PRIMARY_PORTAL_TIE,
        "Primary Portal Tie",
        1,
        DAT_PRIMARY_PORTAL_TIE,
    );
    pin(ACID_STREAM_II, "Acid Stream II", 1, DAT_ACID_STREAM_II);
    pin(STRENGTH_SELF_I, "Strength Self I", 1, DAT_STRENGTH_SELF_I);
    pin(
        REGENERATION_SELF_V,
        "Regeneration Self V",
        1,
        DAT_REGENERATION_SELF_V,
    );
    pin(SEARING_DISC, "Searing Disc", 2, DAT_SEARING_DISC);
    pin(
        EVAPORATE_ALL_MAGIC,
        "Evaporate All Magic Other",
        3,
        DAT_EVAPORATE_ALL_MAGIC,
    );
    pin(AERFALLES_TOUCH, "Aerfalle's Touch", 3, DAT_AERFALLES_TOUCH);

    // `spell_index::SELF_TARGETED` — the cast station below relies on this bit, so it is pinned
    // here off the dat rather than trusted.
    assert_eq!(
        t.spells[&REGENERATION_SELF_V].bitfield
            & dereth_client_model::magic::spell_index::SELF_TARGETED,
        dereth_client_model::magic::spell_index::SELF_TARGETED,
    );
}

// =================================================================================================
// 2. The hash and the taper base
// =================================================================================================

/// The string hash, on account names.
///
/// It walks the string's cp1252 bytes as **signed** chars, shifting the accumulator left by four
/// and adding each byte, folding the top nibble back in whenever it is set.
///
/// ACE's `SpellTable.ComputeHash` (`SpellTable.cs:32`) is the same loop over
/// `Encoding.GetEncoding(1252)` bytes read as `sbyte`. The empty string answers **0** — the `if`
/// guarding the loop is never entered — which is what the account name hashes to before
/// `0xF658` arrives.
#[test]
fn the_account_hash_is_the_clients_string_hash() {
    assert_eq!(account_name_hash(ACCOUNT), ACCOUNT_HASH);
    assert_eq!(account_name_hash(OTHER_ACCOUNT), OTHER_ACCOUNT_HASH);
    assert_eq!(account_name_hash(""), 0, "the empty name hashes to 0");

    // It is a *hash of the bytes*: case and trailing space are part of the name, because the
    // client neither folds nor trims what `0xF658` gave it.
    assert_ne!(
        account_name_hash("dereunit1118"),
        account_name_hash("DereUnit1118")
    );
    assert_ne!(
        account_name_hash("dereunit1118"),
        account_name_hash("dereunit1118 ")
    );

    // The same function `dereth_assets` uses for the spell-table component key — one hash, two
    // callers, exactly as in the client.
    assert_eq!(
        account_name_hash("Flame Bolt I"),
        dereth_assets::tables::spell_hash(b"Flame Bolt I"),
    );

    // A byte above 0x7F is signed, so it subtracts. cp1252 0xE9 is `é`.
    assert_eq!(
        account_name_hash("\u{e9}"),
        dereth_assets::tables::spell_hash(&[0xE9])
    );

    // The lowest taper id is 0x3F, and there are twelve tapers.
    assert_eq!(LOWEST_TAPER_ID, 63);
    assert_eq!(NUM_TAPERS, 12);
}

// =================================================================================================
// 3. The three versions
// =================================================================================================

fn randomized(slots: [u32; 8], account: &str, version: u32) -> [u32; 8] {
    let mut f = slots;
    randomize_for_name(&mut f, account, version);
    f
}

/// Version 1 — the slot layout is positional, and which tapers exist depends
/// on how many components the formula has.
///
/// Five slots: `scarab, herb, powder, potion, talisman`, no taper. Six: a taper at 1. Seven: one
/// at 3 as well. Eight: one at 6 as well. The three count comparisons are against 5, 6 and 7.
#[test]
fn version_1_inserts_a_taper_per_extra_slot() {
    assert_eq!(
        randomized(DAT_ACID_STREAM_V, ACCOUNT, 1),
        ACCT_ACID_STREAM_V
    );
    assert_eq!(
        randomized(DAT_PRIMARY_PORTAL_TIE, ACCOUNT, 1),
        ACCT_PRIMARY_PORTAL_TIE
    );
    assert_eq!(
        randomized(DAT_REGENERATION_SELF_V, ACCOUNT, 1),
        ACCT_REGENERATION_SELF_V
    );

    // Six slots: the only taper is at 1, and that arm never touches the seed — so
    // the six-slot formula is the same for every account, and is already what the dat stores.
    assert_eq!(
        randomized(DAT_ACID_STREAM_II, ACCOUNT, 1),
        DAT_ACID_STREAM_II
    );
    assert_eq!(
        randomized(DAT_ACID_STREAM_II, OTHER_ACCOUNT, 1),
        DAT_ACID_STREAM_II
    );
    assert_eq!(randomized(DAT_ACID_STREAM_II, "", 1), DAT_ACID_STREAM_II);

    // Five slots: none of the three arms runs, whoever is logged in.
    assert_eq!(
        randomized(DAT_STRENGTH_SELF_I, ACCOUNT, 1),
        DAT_STRENGTH_SELF_I
    );
    assert_eq!(
        randomized(DAT_STRENGTH_SELF_I, OTHER_ACCOUNT, 1),
        DAT_STRENGTH_SELF_I
    );

    // Seven slots leave slot 6 — the talisman's neighbour — alone.
    assert_eq!(
        randomized(DAT_PRIMARY_PORTAL_TIE, OTHER_ACCOUNT, 1)[6],
        DAT_PRIMARY_PORTAL_TIE[6],
    );
}

/// Version 2 — no slot count and no guards, always slots 3 and 6.
#[test]
fn version_2_always_writes_slots_3_and_6() {
    assert_eq!(randomized(DAT_SEARING_DISC, ACCOUNT, 2), ACCT_SEARING_DISC);
    let a = randomized(DAT_SEARING_DISC, ACCOUNT, 2);
    for i in [0usize, 1, 2, 4, 5, 7] {
        assert_eq!(
            a[i], DAT_SEARING_DISC[i],
            "slot {i} is a reagent and must not move"
        );
    }
}

/// Version 3 — six residues mod 12 folded into slots 3 and 6.
///
/// The seven hash calls are seven reads of one cached hash, not seven hashes: every one of them
/// passes the same string.
#[test]
fn version_3_folds_six_residues_into_slots_3_and_6() {
    assert_eq!(
        randomized(DAT_EVAPORATE_ALL_MAGIC, ACCOUNT, 3),
        ACCT_EVAPORATE_ALL_MAGIC
    );
    // Seven slots: slot 7 is 0, which is ACE's `comps.Count < 8` arm (`SpellTable.cs:163`).
    assert_eq!(
        randomized(DAT_AERFALLES_TOUCH, ACCOUNT, 3),
        ACCT_AERFALLES_TOUCH
    );
    assert_eq!(
        randomized(DAT_AERFALLES_TOUCH, ACCOUNT, 3)[7],
        0,
        "and the empty slot stays empty",
    );
}

/// A formula version outside `1..=3` is returned untouched.
#[test]
fn an_unknown_formula_version_changes_nothing() {
    for v in [0u32, 4, 99, u32::MAX] {
        assert_eq!(
            randomized(DAT_ACID_STREAM_V, ACCOUNT, v),
            DAT_ACID_STREAM_V,
            "version {v}"
        );
    }
}

/// The tapers move with the **name**, and nothing else in the formula does.
#[test]
fn a_different_account_gets_different_tapers_and_the_same_reagents() {
    assert_eq!(
        randomized(DAT_ACID_STREAM_V, OTHER_ACCOUNT, 1),
        OTHER_ACID_STREAM_V
    );
    assert_eq!(
        randomized(DAT_REGENERATION_SELF_V, OTHER_ACCOUNT, 1),
        OTHER_REGENERATION_SELF_V
    );
    assert_eq!(
        randomized(DAT_AERFALLES_TOUCH, OTHER_ACCOUNT, 3),
        OTHER_AERFALLES_TOUCH
    );

    // The two accounts differ, which is the whole point.
    assert_ne!(ACCT_ACID_STREAM_V, OTHER_ACID_STREAM_V);
    assert_ne!(ACCT_REGENERATION_SELF_V, OTHER_REGENERATION_SELF_V);

    // Slots 0, 2, 4, 5 and 7 are never written, on any version. Slot 1 belongs to version 1
    // alone: versions 2 and 3 write **only** 3 and 6, which is why `Searing Disc` keeps a scarab
    // (110, Diamond) at slot 1 while its slots 3 and 6 are tapers.
    for (slots, version, writes_slot_1) in [
        (DAT_ACID_STREAM_V, 1u32, true),
        (DAT_SEARING_DISC, 2, false),
        (DAT_EVAPORATE_ALL_MAGIC, 3, false),
    ] {
        for account in [ACCOUNT, OTHER_ACCOUNT, ""] {
            let out = randomized(slots, account, version);
            for i in [0usize, 2, 4, 5, 7] {
                assert_eq!(
                    out[i], slots[i],
                    "slot {i}, version {version}, account {account:?}"
                );
            }
            let taper = |i: usize| {
                assert!(
                    (LOWEST_TAPER_ID..LOWEST_TAPER_ID + NUM_TAPERS).contains(&out[i]),
                    "slot {i} must be a taper, got {}",
                    out[i],
                );
            };
            taper(3);
            taper(6);
            if writes_slot_1 {
                taper(1);
            } else {
                assert_eq!(out[1], slots[1], "version {version} leaves slot 1 alone");
            }
        }
    }

    // The empty name is the pre-`0xF658` client and is a real answer, not a skip.
    assert_eq!(randomized(DAT_ACID_STREAM_V, "", 1), EMPTY_ACID_STREAM_V);
}

// =================================================================================================
// 4. World formula resolution — the rejecting station
// =================================================================================================

const PLAYER: ObjectId = ObjectId(0x5000_000A);

/// A world holding the **shipped** spell table, with the player in world and an account name.
fn world_with(account: &str) -> World {
    let mut w = World::new();
    w.player = Some(PLAYER);
    let mut me = dereth_client_model::weenie::Weenie::new(PLAYER);
    me.pwd = dereth_protocol::types::PublicWeenieDesc {
        name: "Lark".into(),
        obj_type: dereth_rules::weenie::item_type::CREATURE,
        ..Default::default()
    };
    me.qualities = Some(dereth_client_model::qualities::Qualities::default());
    w.tables.weenies.insert(PLAYER, me);
    w.magic.spell_table = Some(Arc::new(shipped_spell_table()));
    // The account name as the character-set message stores it.
    w.player_system.account = account.to_string();
    w
}

/// Behaviour: magic.formula.a-spells-tapers-are-the-accounts-and-not-the-dats
/// The world formula is the accounts and not the dats.
#[test]
fn the_world_formula_is_the_accounts_and_not_the_dats() {
    let w = shipped_spell_table();
    let f = |world: &World, id: u32| world.spell_formula(&w.spells[&id]);

    let mine = world_with(ACCOUNT);
    assert_eq!(f(&mine, ACID_STREAM_V), ACCT_ACID_STREAM_V);
    assert_eq!(f(&mine, REGENERATION_SELF_V), ACCT_REGENERATION_SELF_V);
    assert_eq!(f(&mine, SEARING_DISC), ACCT_SEARING_DISC);
    assert_eq!(f(&mine, EVAPORATE_ALL_MAGIC), ACCT_EVAPORATE_ALL_MAGIC);
    assert_eq!(f(&mine, AERFALLES_TOUCH), ACCT_AERFALLES_TOUCH);

    // The three taper slots genuinely differ from the dat's — otherwise the assertions above
    // would pass against `plain` as well and this station would prove nothing.
    assert_ne!(f(&mine, ACID_STREAM_V), DAT_ACID_STREAM_V);
    assert_ne!(f(&mine, REGENERATION_SELF_V), DAT_REGENERATION_SELF_V);
    assert_ne!(f(&mine, SEARING_DISC), DAT_SEARING_DISC);
    assert_ne!(f(&mine, EVAPORATE_ALL_MAGIC), DAT_EVAPORATE_ALL_MAGIC);
    assert_ne!(f(&mine, AERFALLES_TOUCH), DAT_AERFALLES_TOUCH);

    // Another account, same world shape, different tapers.
    let theirs = world_with(OTHER_ACCOUNT);
    assert_eq!(f(&theirs, ACID_STREAM_V), OTHER_ACID_STREAM_V);
    assert_eq!(f(&theirs, REGENERATION_SELF_V), OTHER_REGENERATION_SELF_V);

    assert_eq!(f(&mine, STRENGTH_SELF_I), DAT_STRENGTH_SELF_I);
    assert_eq!(f(&theirs, STRENGTH_SELF_I), DAT_STRENGTH_SELF_I);
}

// =================================================================================================
// 5. The cast path
// =================================================================================================

/// The components-required test — the player's bool quality `0x44`.
fn require_components(w: &mut World, on: bool) {
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};
    w.player_qualities_mut()
        .expect("the player has qualities")
        .set(
            StatKey::new(
                StatType::Bool,
                dereth_client_model::magic::SPELL_COMPONENTS_REQUIRED,
            ),
            StatValue::Bool(on),
        );
    assert_eq!(w.are_spell_components_required(), on);
}

/// SCID -> WCID as `1000 + scid`. **Synthetic**, and said to be: the real map is a
/// dual data-id mapper DAT object. Nothing
/// here depends on the values, only on the mapping being injective — which is what makes owning
/// SCID 70 and not owning SCID 72 distinguishable.
fn catalogue() -> ComponentCatalogue {
    let scids: Vec<u32> = (0u32..=200).collect();
    ComponentCatalogue::new(
        scids.iter().map(|s| (*s, 1000 + *s)),
        scids.iter().map(|s| {
            (
                *s,
                ComponentBase {
                    name: format!("component {s}"),
                    category: 0,
                    icon: 0,
                },
            )
        }),
    )
}

/// Put exactly these SCIDs in the player's pack, through the production writer
/// (the player system's own spell-component update), not by hand.
fn give(w: &mut World, scids: &[u32]) {
    let mut inv = dereth_client_model::objects::ObjectInventory::default();
    for (i, scid) in scids.iter().enumerate() {
        let id = ObjectId(0x6000_0000 + u32::try_from(i).expect("small"));
        let mut wn = dereth_client_model::weenie::Weenie::new(id);
        wn.pwd = dereth_protocol::types::PublicWeenieDesc {
            name: format!("component {scid}"),
            wcid: 1000 + *scid,
            obj_type: dereth_rules::weenie::item_type::SPELL_COMPONENTS,
            stack_size: Some(10),
            container_id: Some(PLAYER),
            ..Default::default()
        };
        w.tables.weenies.insert(id, wn);
        inv.items.push(id);
    }
    w.tables.inventories.insert(PLAYER, inv);
    let (offered, changed) = w.initialize_spell_components();
    assert_eq!(
        offered,
        scids.len(),
        "the writer was offered every component"
    );
    assert_eq!(changed, scids.len(), "and took every one");
}

fn cast_world(account: &str, owned: &[u32]) -> World {
    let mut w = world_with(account);
    w.magic.catalogue = catalogue();
    give(&mut w, owned);
    require_components(&mut w, true);
    w
}

/// The cast path checks the accounts components.
#[test]
fn the_cast_path_checks_the_accounts_components() {
    let dat: Vec<u32> = DAT_REGENERATION_SELF_V
        .iter()
        .copied()
        .filter(|c| *c != 0)
        .collect();
    let acct: Vec<u32> = ACCT_REGENERATION_SELF_V
        .iter()
        .copied()
        .filter(|c| *c != 0)
        .collect();
    assert_ne!(
        dat, acct,
        "the two packs must differ or this station proves nothing"
    );

    // Carrying the dat's canonical components: refused.
    let mut w = cast_world(ACCOUNT, &dat);
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut req, &mut out, REGENERATION_SELF_V),
        Err(dereth_client_model::magic::messages::MISSING_COMPONENTS.to_string()),
    );
    assert!(
        req.0.is_empty(),
        "and nothing went to the wire: {:?}",
        req.0
    );
    assert_eq!(w.magic.busy_count, 0);

    // Carrying this account's: cast.
    let mut w2 = cast_world(ACCOUNT, &acct);
    let mut req2 = RecordingRequests::default();
    w2.cast_spell(
        &mut req2,
        &mut RecordingSink::default(),
        REGENERATION_SELF_V,
    )
    .expect("every component of the account's formula is owned");
    let [Request::CastTargetedSpell(m)] = req2.0.as_slice() else {
        panic!("{:?}", req2.0)
    };
    assert_eq!(m.spell_id, REGENERATION_SELF_V);
    assert_eq!(m.target, PLAYER, "self-targeted casts at the caster");
    assert_eq!(w2.magic.busy_count, 1);

    // And the *other* account's pack does not work for this one, which is the per-account half.
    let other: Vec<u32> = OTHER_REGENERATION_SELF_V
        .iter()
        .copied()
        .filter(|c| *c != 0)
        .collect();
    let mut w3 = cast_world(ACCOUNT, &other);
    let mut req3 = RecordingRequests::default();
    assert!(w3
        .cast_spell(
            &mut req3,
            &mut RecordingSink::default(),
            REGENERATION_SELF_V
        )
        .is_err());
    assert!(req3.0.is_empty());

    // The quality `0x44` gate is still the client's: with components not required, the dat's pack
    // casts fine, because the loop never runs.
    let mut w4 = cast_world(ACCOUNT, &dat);
    require_components(&mut w4, false);
    let mut req4 = RecordingRequests::default();
    w4.cast_spell(
        &mut req4,
        &mut RecordingSink::default(),
        REGENERATION_SELF_V,
    )
    .expect("no component check at all");
    assert_eq!(req4.0.len(), 1);
}

// =================================================================================================
// 6. The whole table, against a second transcription of the ACE port
// =================================================================================================

/// ACE's `SpellTable.ComputeHash` (`ACE.DatLoader/FileTypes/SpellTable.cs:32`), re-transcribed.
fn ace_compute_hash(s: &str) -> u32 {
    let mut result: i64 = 0;
    for b in dereth_primitives::text::cp1252::encode(s).expect("ASCII fixture names") {
        let c = i64::from(b as i8);
        result = c + (result << 4);
        if result & 0xF000_0000 != 0 {
            result = (result ^ ((result & 0xF000_0000) >> 24)) & 0x0FFF_FFFF;
        }
    }
    u32::try_from(result & 0xFFFF_FFFF).expect("masked")
}

const ACE_LOWEST_TAPER_ID: u32 = 63;

/// ACE's `RandomizeVersion1` (`SpellTable.cs:75`), over a **dense** component list.
fn ace_v1(comps: &[u32], account: &str) -> Vec<u32> {
    let mut c = comps.to_vec();
    let (mut t1, mut t2, mut t3) = (false, false, false);
    let key = ace_compute_hash(account);
    let seed = key % 0x13D573;
    let scarab = c[0];
    let mut herb_index = 1;
    if c.len() > 5 {
        herb_index = 2;
        t1 = true;
    }
    let herb = c[herb_index];
    let mut powder_index = herb_index + 1;
    if c.len() > 6 {
        powder_index += 1;
        t2 = true;
    }
    let powder = c[powder_index];
    let potion_index = powder_index + 1;
    let potion = c[potion_index];
    let mut talisman_index = potion_index + 1;
    if c.len() > 7 {
        talisman_index += 1;
        t3 = true;
    }
    let talisman = c[talisman_index];
    if t1 {
        c[1] = (powder + 2 * herb + potion + talisman + scarab) % 0xC + ACE_LOWEST_TAPER_ID;
    }
    if t2 {
        c[3] = (scarab + herb + talisman + 2 * (powder + potion))
            .wrapping_mul(seed / (scarab + (powder + potion)))
            % 0xC
            + ACE_LOWEST_TAPER_ID;
    }
    if t3 {
        c[6] = (powder + 2 * talisman + potion + herb + scarab)
            .wrapping_mul(seed / (talisman + scarab))
            % 0xC
            + ACE_LOWEST_TAPER_ID;
    }
    c
}

/// ACE's `RandomizeVersion2` (`SpellTable.cs:125`).
fn ace_v2(comps: &[u32], account: &str) -> Vec<u32> {
    let mut c = comps.to_vec();
    let key = ace_compute_hash(account);
    let seed = key % 0x13D573;
    let (p1, cc, x, a) = (c[0], c[4], c[5], c[7]);
    c[3] = (a + 2 * comps[0] + 2 * cc * x + comps[0] + comps[2] + comps[1]) % 0xC
        + ACE_LOWEST_TAPER_ID;
    c[6] = (a + 2 * p1 * comps[2] + 2 * x + p1 * comps[2] + cc)
        .wrapping_mul(seed / (comps[1] * a + 2 * cc))
        % 0xC
        + ACE_LOWEST_TAPER_ID;
    c
}

/// ACE's `RandomizeVersion3` (`SpellTable.cs:143`), including its explicit short-formula arm.
fn ace_v3(comps: &[u32], account: &str) -> Vec<u32> {
    let mut c = comps.to_vec();
    let key = ace_compute_hash(account);
    let h0 = (key % 0x13D573 + c[0]) % 0xC;
    let h1 = (key % 0x4AEFD + c[1]) % 0xC;
    let h2 = (key % 0x96A7F + c[2]) % 0xC;
    let h4 = (key % 0x100A03 + c[4]) % 0xC;
    let h5 = (key % 0xEB2EF + c[5]) % 0xC;
    let h7 = if c.len() < 8 {
        key % 0x121E7D % 0xC
    } else {
        (key % 0x121E7D + c[7]) % 0xC
    };
    c[3] = (h0 + h1 + h2 + h4 + h5 + h2 * h5 + h0 * h1 + h7 * (h4 + 1)) % 0xC + ACE_LOWEST_TAPER_ID;
    c[6] = (h0
        + h1
        + h2
        + h4
        + key % 0x65039 % 0xC
        + h7 * (h4 * (h0 * h1 * h2 * h5 + 7) + 1)
        + h5
        + 4 * h0 * h1
        + h0 * h1
        + 11 * h2 * h5)
        % 0xC
        + ACE_LOWEST_TAPER_ID;
    c
}

/// ACE's `GetSpellFormula` (`SpellTable.cs:58`).
fn ace_formula(b: &SpellBase, account: &str) -> Vec<u32> {
    let dense: Vec<u32> = dat_slots(b).into_iter().filter(|c| *c != 0).collect();
    match b.formula_version {
        1 => ace_v1(&dense, account),
        2 => ace_v2(&dense, account),
        3 => ace_v3(&dense, account),
        _ => dense,
    }
}

/// Every shipped spell, three account names, both ports: the client and the reference server ask
/// the player for the same components.
///
/// This is also the divisor sweep: `RandomizeVersion1`'s two `div`s and `RandomizeVersion2`'s one
/// are reached 6266 times each here without `dereth_client_model::magic::div_or_zero` ever having to answer,
/// which is what lets that guard be documented as unreachable rather than as a deviation. (A
/// zero divisor would show up as a disagreement, because ACE's `/` would panic in debug.)
#[test]
fn the_ace_port_and_the_native_arithmetic_agree_over_the_whole_shipped_table() {
    let t = shipped_spell_table();
    let mut checked = 0usize;
    let mut moved = 0usize;
    for account in [ACCOUNT, OTHER_ACCOUNT, ""] {
        for (id, b) in &t.spells {
            let dat = dat_slots(b);
            let mut mine = dat;
            randomize_for_name(&mut mine, account, b.formula_version);
            let theirs = ace_formula(b, account);
            assert_eq!(
                mine[..theirs.len()],
                theirs[..],
                "spell {id} {:?}, version {}, account {account:?}",
                b.name,
                b.formula_version,
            );
            assert!(
                mine[theirs.len()..].iter().all(|c| *c == 0),
                "spell {id}: ACE's dense list ends where the slots do",
            );
            if mine != dat {
                moved += 1;
            }
            checked += 1;
        }
    }
    assert_eq!(checked, t.spells.len() * 3, "every spell, every account");
    assert!(
        moved > 9000,
        "{moved} of {checked} formulas actually changed"
    );
}
