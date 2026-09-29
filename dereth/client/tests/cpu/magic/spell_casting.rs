//! A player casts a spell: a click on the spell bar's Cast button raises the request, the
//! production dispatch validates the spell, picks the customized or scarab-only formula, applies
//! the component check and the five target refusals in the client's own words, stops the player's
//! body and puts `0x0048 Magic_CastUntargetedSpell` or `0x004A Magic_CastTargetedSpell` into the
//! outbox. No recording holds a cast (the corpus scan proves it can see, against a positive
//! control), so the spell table, its formulas and the component WCIDs are synthetic and written
//! here; the route is real: `SpellcastingPanel::cast` produces the request and
//! `Interaction::run_ui_requests` consumes it. No socket is opened; every request is compared as
//! bytes. Fixture: the recorded message corpus for the scan; no dats.

#![allow(clippy::pedantic)]

use std::collections::BTreeMap;
use std::sync::Arc;

use crate::common::recorded_sessions;
use dereth_client::interaction::Interaction;
use dereth_client_model::{RecordingRequests, RecordingSink, Request, World};
use dereth_client_net::client_session::testing::{Corpus, Direction};
use dereth_primitives::{ObjectId, ServerTime};
use dereth_protocol::Opcode;
use dereth_ui_screens::view::UiRequest;

// ------------------------------------------------------------------------------------------
// 0. The literals, pinned as the client's own numbers and never read through the symbol.
//
// A test that reads a constant through the same symbol it writes cannot detect a wrong constant.
// Every number below was independently read from the original client's implementation or symbols.
// ------------------------------------------------------------------------------------------

/// `0x0048 Magic_CastUntargetedSpell`, carrying only the spell id.
const MAGIC_CAST_UNTARGETED: u32 = 0x0048;
/// `0x004A Magic_CastTargetedSpell` carries `(target, spell)`,
/// **target first**.
const MAGIC_CAST_TARGETED: u32 = 0x004A;
/// Bit 3 of the spell definition selects the self-targeted casting branch.
const SPELL_SELF_TARGETED: u32 = 0x0008;
/// `0x8107` marks a redirectable item-enchantment target and appears twice with different meanings.
const TYPE_GEAR: u32 = 0x8107;
/// `0x8000` marks a caster; the endowment test checks whether the high byte is negative as `i8`.
const TYPE_CASTER: u32 = 0x8000;
/// Item type `0x0010`, the creature target type carried by a war spell.
const TYPE_CREATURE: u32 = 0x0010;
/// Bit `0x10` of the public object description is the inferred attackable bit used by target checks.
const BF_ATTACKABLE: u32 = 0x0010;
/// Component id `0xBC`, used to fill the scarab-only formula after its power component.
const SCARAB_ONLY_FILLER: u32 = 0xBC;
/// The five school-specific augmentation qualities selected by the formula branch.
const AUG_INFUSED_WAR: u32 = 0x129;
const AUG_INFUSED_LIFE: u32 = 0x128;
const AUG_INFUSED_ITEM: u32 = 0x127;
const AUG_INFUSED_CREATURE: u32 = 0x126;
const AUG_INFUSED_VOID: u32 = 0x148;
/// Bool quality `0x44`; absence leaves the caller's false default unchanged.
const SPELL_COMPONENTS_REQUIRED: u32 = 0x44;
/// Display channel `0x1A`, used for every local casting refusal.
const REFUSAL_CHANNEL: u32 = 0x1A;
/// Inventory location `0x0100_0000`, where the casting panel looks for an endowment item.
const ENDOWMENT_LOCATION: u32 = 0x0100_0000;

/// The six client messages used by the casting and target-compatibility paths.
const S_NO_COMPONENTS: &str = "You do not have all of this spell's components";
const S_NEED_TARGET: &str = "You must select a suitable target before casting this spell";
const S_NO_TARGET_WANTED: &str = "This spell would require no target";
const S_TARGET_WANTED: &str = "This spell would require a target";
const S_NOT_ON_SELF: &str = "You cannot cast this spell upon yourself";
const S_STACK: &str = "Cannot cast spell on a stack of items.";
/// The target-compatibility path's formatted refusal.
const S_CANNOT_CAST_ON: &str = "This spell cannot be cast on Drudge Slave";
/// The target-compatibility path's **success** line.
const S_CASTING: &str = "Casting Flame Bolt";

#[test]
fn the_pinned_literals_are_the_symbols_they_stand_for() {
    use dereth_client_model::magic::{messages, spell_index, SCARAB_ONLY_FILLER_SCID};
    use dereth_client_model::weenie::item_type;

    assert_eq!(Opcode::MAGIC_CAST_UNTARGETED_SPELL.0, MAGIC_CAST_UNTARGETED);
    assert_eq!(Opcode::MAGIC_CAST_TARGETED_SPELL.0, MAGIC_CAST_TARGETED);
    assert_eq!(spell_index::SELF_TARGETED, SPELL_SELF_TARGETED);
    assert_eq!(item_type::REDIRECTABLE_ITEM_ENCHANTMENT_TARGET, TYPE_GEAR);
    assert_eq!(item_type::CASTER, TYPE_CASTER);
    assert_eq!(item_type::CREATURE, TYPE_CREATURE);
    assert_eq!(SCARAB_ONLY_FILLER_SCID, SCARAB_ONLY_FILLER);
    assert_eq!(
        dereth_client_model::magic::SPELL_COMPONENTS_REQUIRED,
        SPELL_COMPONENTS_REQUIRED
    );
    assert_eq!(dereth_client_model::chat::REFUSAL_CHANNEL, REFUSAL_CHANNEL);
    assert_eq!(
        dereth_ui_screens::panels::spellcasting::ENDOWMENT_LOCATION,
        ENDOWMENT_LOCATION
    );
    let aug = dereth_client_model::magic::infused_augmentation_for_school;
    assert_eq!(aug(1), Some(AUG_INFUSED_WAR));
    assert_eq!(aug(2), Some(AUG_INFUSED_LIFE));
    assert_eq!(aug(3), Some(AUG_INFUSED_ITEM));
    assert_eq!(aug(4), Some(AUG_INFUSED_CREATURE));
    assert_eq!(aug(5), Some(AUG_INFUSED_VOID));
    assert_eq!(
        aug(0),
        None,
        "and the default arm goes straight to the pack test"
    );
    assert_eq!(aug(6), None);

    assert_eq!(messages::MISSING_COMPONENTS, S_NO_COMPONENTS);
    assert_eq!(messages::NEED_TARGET, S_NEED_TARGET);
    assert_eq!(messages::WOULD_REQUIRE_NO_TARGET, S_NO_TARGET_WANTED);
    assert_eq!(messages::WOULD_REQUIRE_A_TARGET, S_TARGET_WANTED);
    assert_eq!(messages::CANNOT_CAST_ON_SELF, S_NOT_ON_SELF);
    assert_eq!(messages::STACK_OF_ITEMS, S_STACK);
    assert_eq!(
        messages::cannot_be_cast_on("Drudge Slave"),
        S_CANNOT_CAST_ON
    );
    assert_eq!(messages::casting("Flame Bolt"), S_CASTING);

    // `0x0048` and `0x004A` are `NetQueue::Weenie`, which is what makes `outbound_kind` classify
    // them as ordered game actions rather than control blobs.
    for op in [MAGIC_CAST_UNTARGETED, MAGIC_CAST_TARGETED] {
        assert_eq!(
            Opcode(op).info().and_then(|i| i.send_queue),
            Some(dereth_primitives::NetQueue::Weenie),
            "{op:#06x}"
        );
        assert_eq!(
            Opcode(op).info().map(|i| i.direction),
            Some(dereth_protocol::Direction::C2S)
        );
    }
}

// ------------------------------------------------------------------------------------------
// 1. The corpus, with its calibration
// ------------------------------------------------------------------------------------------

struct Blob {
    session: String,
    dir: char,
    opcode: u32,
    sub: u32,
}

/// Every blob of every recorded session, read through the shared corpus cache, with the game
/// action or event number of each `0xF7B1`/`0xF7B0`.
fn corpus() -> Vec<Blob> {
    let mut out = Vec::new();
    for c in Corpus::shared_all() {
        for b in &c.blobs {
            let dir = match b.dir {
                Direction::ClientToServer => 'c',
                Direction::ServerToClient => 's',
            };
            let sub = if (b.opcode == 0xF7B0 || b.opcode == 0xF7B1) && b.payload.len() >= 16 {
                u32::from_le_bytes([b.payload[12], b.payload[13], b.payload[14], b.payload[15]])
            } else {
                0
            };
            out.push(Blob {
                session: c.name.clone(),
                dir,
                opcode: b.opcode,
                sub,
            });
        }
    }
    assert!(
        !out.is_empty(),
        "the corpus is empty; the scan below would report a false zero"
    );
    out
}

/// **The calibrated zero.** Everything below is transcribe-and-drive rather than replay, and the
/// justification is that no recording holds a cast. A scan that was simply broken would say the
/// same thing, so the same pass counts something the corpus *does* hold.
///
/// A negative from an instrument that could not observe the space proves nothing, so this test
/// asserts both directions.
#[test]
fn the_corpus_has_never_seen_a_cast_and_the_scan_can_see() {
    let blobs = corpus();
    let count = |dir: char, sub: u32| {
        blobs
            .iter()
            .filter(|b| b.dir == dir && b.sub == sub && b.sub != 0)
            .count()
    };
    let sessions: std::collections::BTreeSet<&str> =
        blobs.iter().map(|b| b.session.as_str()).collect();
    // Counted from the corpus index rather than written here, so a new recording re-measures the
    // denominator instead of failing it.
    assert_eq!(
        sessions.len(),
        recorded_sessions(),
        "every recorded capture is in the scan; a smaller number means it read less than the corpus"
    );

    for sub in [MAGIC_CAST_UNTARGETED, MAGIC_CAST_TARGETED] {
        assert_eq!(count('c', sub), 0, "no client {sub:#06x} in any capture");
        assert_eq!(count('s', sub), 0, "no server {sub:#06x} either");
    }
    // The positive control, on the **same** scan: enchantment updates and dispels, which the
    // recordings do hold.
    assert!(
        count('s', 0x02C2) > 0,
        "no 0x02C2 Magic_UpdateEnchantment in the corpus: the scan cannot see game events"
    );
    assert!(
        count('s', 0x02C7) > 0,
        "no 0x02C7 Magic_DispelEnchantment in the corpus: the scan cannot see game events"
    );
    // Denominators, so "0 of 0" cannot pass as "0 of many".
    let actions = blobs
        .iter()
        .filter(|b| b.opcode == 0xF7B1 && b.dir == 'c')
        .count();
    let events = blobs
        .iter()
        .filter(|b| b.opcode == 0xF7B0 && b.dir == 's')
        .count();
    assert!(actions > 2000, "{actions} client game actions scanned");
    assert!(events > 900, "{events} server game events scanned");
}

// ------------------------------------------------------------------------------------------
// 2. A world with a spell table. Synthetic, and said to be.
// ------------------------------------------------------------------------------------------

const FLAME_BOLT: u32 = 100; // war, targeted, five components
const STRENGTH_SELF: u32 = 200; // creature, self-targeted

/// Component SCIDs and the weenie class IDs (WCIDs) the component table maps them to.
/// **Synthetic**: the shipped table and its mapper are host-side dat reads.
const SCID_TO_WCID: [(u32, u32); 7] = [
    (1, 1001),                  // Lead Scarab, power level 1
    (6, 1006),                  // Pyreal Scarab, power level 6
    (70, 1070),                 // a herb
    (71, 1071),                 // a powder
    (72, 1072),                 // a potion
    (0x31, 1073),               // a talisman, the target component: it aims at creatures
    (SCARAB_ONLY_FILLER, 1188), // the filler used to pad the scarab-only formula
];

fn spell_base(
    name: &str,
    school: u32,
    bitfield: u32,
    comps: [u32; 8],
    target_type: u32,
) -> dereth_assets::tables::SpellBase {
    // Formula decryption subtracts the key from every non-zero slot, so the raw
    // slots are the plain ones **plus** the key. A non-zero key is used deliberately: a zero one
    // would make `decrypt_formula` unfalsifiable here.
    const KEY: u32 = 0x0DEF_ACED;
    let mut raw = [0u32; 8];
    for (r, c) in raw.iter_mut().zip(comps.iter()) {
        *r = if *c == 0 { 0 } else { c.wrapping_add(KEY) };
    }
    dereth_assets::tables::SpellBase {
        name: name.into(),
        description: String::new(),
        school,
        icon: 0,
        category: 0,
        bitfield,
        base_mana: 0,
        base_range_constant: 0.0,
        base_range_mod: 0.0,
        power: 0,
        spell_economy_mod: 0.0,
        formula_version: 1,
        component_loss: 0.0,
        meta_spell_type: 0,
        meta_spell_id: 0,
        duration: None,
        portal_lifetime: None,
        raw_comps: raw,
        comp_key: KEY,
        comps: comps.iter().copied().filter(|c| *c != 0).collect(),
        caster_effect: 0,
        target_effect: 0,
        fizzle_effect: 0,
        recovery_interval: 0.0,
        recovery_amount: 0.0,
        display_order: 0,
        non_component_target_type: target_type,
        mana_mod: 0,
    }
}

fn spell_table() -> Arc<dereth_assets::tables::SpellTable> {
    let mut spells = BTreeMap::new();
    // A five-component war spell with a Pyreal Scarab at slot 0: the reduced formula keeps the
    // scarab and appends **four** fillers (power level 6 -> 4).
    spells.insert(
        FLAME_BOLT,
        spell_base(
            "Flame Bolt",
            1,
            0,
            [6, 70, 71, 72, 0x31, 0, 0, 0],
            TYPE_CREATURE,
        ),
    );
    // Self-targeted, so casting never reaches the target test at all.
    spells.insert(
        STRENGTH_SELF,
        spell_base(
            "Strength Self",
            4,
            SPELL_SELF_TARGETED,
            [1, 70, 71, 72, 0x31, 0, 0, 0],
            TYPE_CREATURE,
        ),
    );
    Arc::new(dereth_assets::tables::SpellTable {
        id: dereth_primitives::DataId(0x0E00_000E),
        spell_buckets: 0,
        spells,
        spellset_bucket_index: 1,
        spellsets: BTreeMap::new(),
    })
}

const PLAYER: ObjectId = ObjectId(0x5000_000A);
const TARGET: ObjectId = ObjectId(0x5000_00B0);

fn put(w: &mut World, id: ObjectId, pwd: dereth_protocol::types::PublicWeenieDesc) {
    let mut wn = dereth_client_model::weenie::Weenie::new(id);
    wn.pwd = pwd;
    w.tables.weenies.insert(id, wn);
}

/// A world in which the player is in-world, the spell table is loaded and a legal target exists.
///
/// The `ComponentTracker` is filled through the game world's `update_spell_component` method, its
/// production writer, **not** by hand.
fn cast_world(owned: &[u32]) -> World {
    let mut w = World::new();
    w.player = Some(PLAYER);
    put(
        &mut w,
        PLAYER,
        dereth_protocol::types::PublicWeenieDesc {
            name: "Lark".into(),
            obj_type: dereth_client_model::weenie::item_type::CREATURE,
            ..Default::default()
        },
    );
    if let Some(x) = w.tables.weenies.get_mut(PLAYER) {
        x.qualities = Some(dereth_client_model::qualities::Qualities::default());
    }
    put(
        &mut w,
        TARGET,
        dereth_protocol::types::PublicWeenieDesc {
            name: "Drudge Slave".into(),
            obj_type: TYPE_CREATURE,
            bitfield: BF_ATTACKABLE,
            ..Default::default()
        },
    );
    w.magic.spell_table = Some(spell_table());
    w.magic.catalogue = dereth_client_model::magic::ComponentCatalogue::new(
        SCID_TO_WCID,
        SCID_TO_WCID.iter().map(|(scid, _)| {
            (
                *scid,
                dereth_client_model::magic::ComponentBase {
                    name: format!("component {scid}"),
                    category: 0,
                    icon: 0,
                },
            )
        }),
    );
    // Give the player the components, through the production writer.
    let mut inv = dereth_client_model::objects::ObjectInventory::default();
    for (i, wcid) in owned.iter().enumerate() {
        let id = ObjectId(0x6000_0000 + i as u32);
        put(
            &mut w,
            id,
            dereth_protocol::types::PublicWeenieDesc {
                name: format!("component {wcid}"),
                wcid: *wcid,
                obj_type: dereth_client_model::weenie::item_type::SPELL_COMPONENTS,
                stack_size: Some(10),
                // Ownership follows container and wielder links back to the player, which is what
                // `update_spell_component`'s `owned` argument represents.
                container_id: Some(PLAYER),
                ..Default::default()
            },
        );
        inv.items.push(id);
    }
    w.tables.inventories.insert(PLAYER, inv);
    let (offered, changed) = w.initialize_spell_components();
    assert_eq!(
        offered,
        owned.len(),
        "the writer was offered every component"
    );
    assert_eq!(changed, owned.len(), "and took every one");
    w
}

/// Bool quality `0x44` on the player's qualities controls whether components are required.
fn require_components(w: &mut World, on: bool) {
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};
    w.player_qualities_mut()
        .expect("the player has qualities")
        .set(
            StatKey::new(StatType::Bool, SPELL_COMPONENTS_REQUIRED),
            StatValue::Bool(on),
        );
    assert_eq!(w.are_spell_components_required(), on);
}

/// The full customized formula's WCIDs, and the scarab-only one's.
const FULL: [u32; 5] = [1006, 1070, 1071, 1072, 1073];
const SCARAB_ONLY: [u32; 2] = [1006, 1188];

// ------------------------------------------------------------------------------------------
// 3. The casting cluster is reachable
// ------------------------------------------------------------------------------------------

/// **The whole casting cluster is reached from the production request arm.** These functions
/// reference one another, so each must be shown reachable from outside `magic.rs`:
///
/// | symbol | observable role |
/// |---|---|
/// | game-world `cast_spell` | validates the spell and chooses its target branch |
/// | game-world `get_appropriate_spell_formula` | selects customized or scarab-only components |
/// | game-world `are_spell_components_required` | reads the opt-in component gate |
/// | game-world `object_compatible_with_spell_target_type` | applies the five target refusals |
/// | game-world `free_hands_and_cast` | stops movement, emits the request and increments busy state |
/// | `magic::spell_target_type` | reads the target type from the formula's target component |
/// | `magic::formula_is_complete` | checks every required component |
/// | `magic::infused_augmentation_for_school` | maps five schools to their augmentation qualities |
/// | `magic::SpellFormulaKind` + the six `messages` constants | distinguish the two formulas and their notices |
///
/// This test asserts the whole cluster is reached from the production request arm, which is the
/// only thing that makes any of it live.
///
/// **Falsified by** deleting the `UiRequest::CastSpell` arm from `Interaction::run_ui_requests`:
/// the request falls into `unowned` and the assertion on it fires.
#[test]
fn the_cast_cluster_is_reachable_from_the_production_arm() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    let unowned = inter.run_ui_requests(&mut w, false, ServerTime(0.0));
    assert!(unowned.is_empty(), "the production arm exists: {unowned:?}");
    assert_eq!(inter.stats.spells_cast, 1, "and it sent");

    // Every member of the cluster ran on that one call:
    //   cast_spell -> are_spell_components_required (the 0x44 read gated the loop)
    //             -> get_appropriate_spell_formula -> infused_augmentation_for_school
    //             -> spell_target_type -> formula_is_complete
    //             -> object_compatible_with_spell_target_type
    //             -> free_hands_and_cast -> Request::CastTargetedSpell
    let [Request::CastTargetedSpell(m)] = inter.pending_requests() else {
        panic!("expected one 0x004A, got {:?}", inter.pending_requests())
    };
    assert_eq!(m.target, TARGET);
    assert_eq!(m.spell_id, FLAME_BOLT);

    // Incrementing the busy count is the cast-send tail and the whole of the client's casting state.
    assert_eq!(w.magic.busy_count, 1);
    // `Item_UseDone (0x01C7)` takes it back down, which is the other half of the same counter.
    assert_eq!(w.use_done(0), 0);
    assert_eq!(w.magic.busy_count, 0);
}

// ------------------------------------------------------------------------------------------
// 4. The wire
// ------------------------------------------------------------------------------------------

/// The targeted `(target, spell)` and untargeted `(spell)` message bodies, as literal bytes.
///
/// `0x004A` is **target first, spell second**, which is the ordering the message name does not
/// give away; `0x0048` carries the spell alone. Both spell ids are packed as `u32`.
#[test]
fn the_two_cast_messages_encode_as_the_client_packs_them() {
    // Targeted, from a real cast.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    w.cast_spell(&mut req, &mut out, FLAME_BOLT)
        .expect("a legal target");
    let [Request::CastTargetedSpell(m)] = req.0.as_slice() else {
        panic!("{:?}", req.0)
    };
    assert_eq!(
        dereth_protocol::write_body(m).expect("it encodes"),
        vec![
            0xB0, 0x00, 0x00, 0x50, // target = 0x500000B0
            0x64, 0x00, 0x00, 0x00, // spell  = 100
        ]
    );

    // Untargeted: the same spell with a closing component that names no target (the potion in
    // place of the talisman), which is the arm the client inlines rather than calling the shared
    // targeted-send path.
    let mut w2 = cast_world(&FULL);
    require_components(&mut w2, false);
    if let Some(t) = Arc::get_mut(w2.magic.spell_table.as_mut().expect("a table")) {
        let b = t.spells.get_mut(&FLAME_BOLT).expect("the spell");
        b.raw_comps[4] = 72u32.wrapping_add(b.comp_key);
    }
    let mut req2 = RecordingRequests::default();
    w2.cast_spell(&mut req2, &mut RecordingSink::default(), FLAME_BOLT)
        .expect("no target needed");
    let [Request::CastUntargetedSpell(m2)] = req2.0.as_slice() else {
        panic!("{:?}", req2.0)
    };
    assert_eq!(
        dereth_protocol::write_body(m2).expect("it encodes"),
        vec![0x64, 0x00, 0x00, 0x00]
    );

    // Self-targeted: bit 3 short-circuits to a targeted send aimed at the player,
    // so it is a **targeted** message aimed at the caster and never touches the selection.
    let mut w3 = cast_world(&FULL);
    require_components(&mut w3, false);
    let mut req3 = RecordingRequests::default();
    w3.cast_spell(&mut req3, &mut RecordingSink::default(), STRENGTH_SELF)
        .expect("self-targeted needs no selection");
    let [Request::CastTargetedSpell(m3)] = req3.0.as_slice() else {
        panic!("{:?}", req3.0)
    };
    assert_eq!(m3.target, PLAYER, "the player, not the selection");
    assert_eq!(m3.spell_id, STRENGTH_SELF);
}

/// A spell absent from the spell table — and a client with **no** table at all — return
/// silently: `Ok`, nothing sent, nothing shown.
///
/// The two are the same lookup-failure arm in the client, and both are asserted because a "no
/// table" case that refused loudly would be a
/// player-visible invention.
#[test]
fn an_unknown_spell_and_a_missing_table_are_both_silent() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    let mut req = RecordingRequests::default();
    let mut out = RecordingSink::default();
    assert_eq!(w.cast_spell(&mut req, &mut out, 999), Ok(()));
    assert!(req.0.is_empty(), "nothing sent");
    assert!(out.0.is_empty(), "and nothing said");

    w.magic.spell_table = None;
    assert_eq!(w.cast_spell(&mut req, &mut out, FLAME_BOLT), Ok(()));
    assert!(req.0.is_empty());
    assert!(out.0.is_empty());

    // The control: with the table back, the same call does send. Without this the two zeros above
    // are indistinguishable from a `cast_spell` that never works.
    w.magic.spell_table = Some(spell_table());
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    assert_eq!(w.cast_spell(&mut req, &mut out, FLAME_BOLT), Ok(()));
    assert_eq!(req.0.len(), 1);
}

// ------------------------------------------------------------------------------------------
// 5. The component gate, and both formula-selection arms
// ------------------------------------------------------------------------------------------

/// Behaviour: magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent
/// The refusal a character who is short of a component gets, **byte for byte** against the
/// original client's own literal, and delivered on channel `0x1A` the way the client delivers it.
///
/// The refusal path builds `"You do not have all of this spell's components"` and sends it on
/// channel `0x1A`. The apostrophe is a plain `'`.
#[test]
fn a_character_short_of_a_component_is_refused_with_the_clients_own_line() {
    // Everything but the talisman.
    let mut w = cast_world(&[1006, 1070, 1071, 1072]);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(inter.stats.spells_cast, 0, "nothing was cast");
    assert!(
        inter.pending_requests().is_empty(),
        "and nothing went on the wire"
    );
    assert_eq!(inter.last_refusal.as_deref(), Some(S_NO_COMPONENTS));
    // Byte for byte, against the literal rather than against the symbol.
    assert_eq!(
        S_NO_COMPONENTS.as_bytes(),
        b"You do not have all of this spell's components".as_slice()
    );
    // And it reached the scroll on the client's own channel.
    let lines: Vec<(u32, String)> = w
        .scroll
        .pending()
        .iter()
        .map(|f| (f.chat_type, f.body.clone()))
        .collect();
    assert!(
        lines
            .iter()
            .any(|(c, t)| *c == REFUSAL_CHANNEL && t == S_NO_COMPONENTS),
        "{lines:?}"
    );

    // The control: with the talisman, the identical call sends. A refusal test that never sees a
    // success is a test of nothing.
    let mut w2 = cast_world(&FULL);
    require_components(&mut w2, true);
    w2.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut ok = Interaction::default();
    ok.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    assert!(ok
        .run_ui_requests(&mut w2, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(ok.stats.spells_cast, 1);
}

/// **Both formula-selection arms are reachable, and they check different
/// component sets.** This is the question the row asks.
///
/// * *Customized* — the full formula. Reached with no augmentation and no spell pack.
/// * *Scarab-only* — the power components plus one to four
///   copies of SCID `0xBC`. Reached **two** ways, and both are exercised: the school's
///   `AugmentationInfused*Magic` int quality and ownership of the school's spell pack.
///
/// The discriminator is a character holding **only** the scarab and the filler: customized
/// refuses, scarab-only casts. Without that asymmetry the two arms would be indistinguishable.
#[test]
fn both_formula_arms_are_reachable_and_ask_for_different_components() {
    use dereth_client_model::magic::SpellFormulaKind;
    use dereth_client_model::qualities::{StatKey, StatType, StatValue};

    // Customized: the scarab and the filler are not the whole formula, so it refuses.
    let mut w = cast_world(&SCARAB_ONLY);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    assert_eq!(
        w.get_appropriate_spell_formula(1, false),
        SpellFormulaKind::Customized
    );
    let mut req = RecordingRequests::default();
    assert_eq!(
        w.cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT),
        Err(S_NO_COMPONENTS.to_string())
    );
    assert!(req.0.is_empty());

    // Route 1 — the augmentation. Flame Bolt is school 1 (War) -> `0x129`.
    let mut aug = cast_world(&SCARAB_ONLY);
    require_components(&mut aug, true);
    aug.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    aug.player_qualities_mut().expect("qualities").set(
        StatKey::new(StatType::Int, AUG_INFUSED_WAR),
        StatValue::Int(1),
    );
    assert_eq!(
        aug.get_appropriate_spell_formula(1, false),
        SpellFormulaKind::ScarabOnly
    );
    let mut req = RecordingRequests::default();
    aug.cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT)
        .expect("the scarab-only formula is satisfied");
    assert_eq!(
        req.0.len(),
        1,
        "the same character, the same components, now casts"
    );

    // ...and the augmentation is **per school**: a creature spell still wants the full formula.
    assert_eq!(
        aug.get_appropriate_spell_formula(4, false),
        SpellFormulaKind::Customized
    );

    // Route 2 — the spell pack. The five school-to-WCID values have not been extracted, so the map
    // is **synthetic here and empty in production**; what is real is the ownership walk and that an
    // empty map refuses.
    const PACK_WCID: u32 = 0x4321;
    let mut pack = cast_world(&SCARAB_ONLY);
    require_components(&mut pack, true);
    pack.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let bag = ObjectId(0x7000_0001);
    put(
        &mut pack,
        bag,
        dereth_protocol::types::PublicWeenieDesc {
            name: "War Magic Spell Pack".into(),
            wcid: PACK_WCID,
            obj_type: dereth_client_model::weenie::item_type::CONTAINER,
            container_id: Some(PLAYER),
            ..Default::default()
        },
    );
    let mut inv = pack
        .inventory(PLAYER)
        .cloned()
        .expect("the player has an inventory");
    inv.containers.push(bag);
    pack.tables.inventories.insert(PLAYER, inv);
    // Before the map is filled the pack is invisible — the production state.
    assert_eq!(pack.school_of_magic_to_wcid(1), 0);
    assert!(!pack.magic_pack_is_owned(pack.school_of_magic_to_wcid(1)));
    let mut req = RecordingRequests::default();
    assert!(pack
        .cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT)
        .is_err());
    // With the map, the same side pack selects the scarab-only formula.
    pack.magic.school_pack_wcid.insert(1, PACK_WCID);
    assert!(pack.magic_pack_is_owned(pack.school_of_magic_to_wcid(1)));
    let mut req = RecordingRequests::default();
    pack.cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT)
        .expect("the pack reduces the requirement");
    assert_eq!(req.0.len(), 1);
}

/// The component-required quality gates the whole check, and its default is
/// **false**: the quality lookup leaves the caller's `0` when the quality is absent.
#[test]
fn with_components_not_required_nothing_is_checked_at_all() {
    let mut w = cast_world(&[]); // no components whatsoever
    assert!(
        !w.are_spell_components_required(),
        "absent quality reads false"
    );
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut req = RecordingRequests::default();
    w.cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT)
        .expect("no components needed");
    assert_eq!(req.0.len(), 1);

    // Turned on, the same empty-handed character is refused. Both directions, so "always casts"
    // cannot pass as "the gate is off".
    require_components(&mut w, true);
    let mut req = RecordingRequests::default();
    assert!(w
        .cast_spell(&mut req, &mut RecordingSink::default(), FLAME_BOLT)
        .is_err());
    assert!(req.0.is_empty());
}

// ------------------------------------------------------------------------------------------
// 6. Target compatibility, and the success line
// ------------------------------------------------------------------------------------------

/// Target compatibility shows `"Casting %hs"` when its display argument is true,
/// and casting passes true — so a successful **targeted** cast prints a line, and the
/// self-targeted and untargeted arms do not, because neither goes through that function.
#[test]
fn a_targeted_cast_announces_itself_and_the_other_two_arms_do_not() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut out = RecordingSink::default();
    w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT)
        .expect("legal");
    let lines = display_strings(&out);
    assert_eq!(lines, vec![(REFUSAL_CHANNEL, S_CASTING.to_string())]);

    // Self-targeted: bit 3 returns before target compatibility is checked.
    let mut w2 = cast_world(&FULL);
    require_components(&mut w2, false);
    let mut out2 = RecordingSink::default();
    w2.cast_spell(&mut RecordingRequests::default(), &mut out2, STRENGTH_SELF)
        .expect("legal");
    assert!(
        display_strings(&out2).is_empty(),
        "no line on the self-targeted arm"
    );
}

/// Behaviour: magic.cast.a-refusal-the-client-makes-itself-is-said-in-its-own-words-and-never-sent
/// The five target-compatibility refusals, each reached through
/// `cast_spell` and each carrying retail's own literal.
///
/// **The `"This spell cannot be cast on %s"` case is the one that was wrong.** This file's
/// transcription produced `"{name} is not a valid target for this spell"`, a sentence that does
/// not exist in the original client's string data. The mistake escaped the tests because every
/// test read the constant through the same symbol that defined it.
#[test]
fn each_target_refusal_is_the_binarys_own_literal() {
    // Nothing selected produces the casting path's own message, not a target-type refusal.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT),
        Err(S_NEED_TARGET.to_string())
    );
    assert_eq!(
        display_strings(&out),
        vec![(REFUSAL_CHANNEL, S_NEED_TARGET.to_string())]
    );

    // Self-selected, and the spell's target type does not carry `0x8107`.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    w.set_selected_object(Some(PLAYER), false, &mut RecordingSink::default());
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT),
        Err(S_NOT_ON_SELF.to_string())
    );

    // A stack of items, refused before the type test.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    let stack = ObjectId(0x5000_00C0);
    put(
        &mut w,
        stack,
        dereth_protocol::types::PublicWeenieDesc {
            name: "Pyreal".into(),
            obj_type: TYPE_CREATURE,
            bitfield: BF_ATTACKABLE,
            stack_size: Some(5),
            ..Default::default()
        },
    );
    w.set_selected_object(Some(stack), false, &mut RecordingSink::default());
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT),
        Err(S_STACK.to_string())
    );
    // A stack of **one** is not a stack: the client's test is stack size `>= 2`. Both sides of the
    // boundary, because a probe that only tries 5 cannot tell `>= 2` from `>= 1`.
    if let Some(x) = w.tables.weenies.get_mut(stack) {
        x.pwd.stack_size = Some(1);
    }
    let mut out = RecordingSink::default();
    assert!(w
        .cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT)
        .is_ok());
    if let Some(x) = w.tables.weenies.get_mut(stack) {
        x.pwd.stack_size = Some(2);
    }
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT),
        Err(S_STACK.to_string()),
        "two is a stack"
    );

    // A type that does not match and is not attackable -> the **formatted** refusal.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    if let Some(x) = w.tables.weenies.get_mut(TARGET) {
        x.pwd.bitfield = 0; // not attackable
        x.pwd.obj_type = dereth_client_model::weenie::item_type::MISC; // and the mask does not match
    }
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut out = RecordingSink::default();
    assert_eq!(
        w.cast_spell(&mut RecordingRequests::default(), &mut out, FLAME_BOLT),
        Err(S_CANNOT_CAST_ON.to_string())
    );
    assert_eq!(
        display_strings(&out),
        vec![(REFUSAL_CHANNEL, S_CANNOT_CAST_ON.to_string())]
    );
    assert!(
        !S_CANNOT_CAST_ON.contains("is not a valid target"),
        "that sentence is not one the original client produces"
    );

    // The zero-mask arm: a spell that wants no target, with one selected.
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);
    let mut out = RecordingSink::default();
    assert_eq!(
        w.object_compatible_with_spell_target_type(&mut out, Some(TARGET), 0, false),
        Err(S_NO_TARGET_WANTED.to_string())
    );
    // ...and the mirror, a non-zero mask with nothing selected.
    let mut out = RecordingSink::default();
    assert_eq!(
        w.object_compatible_with_spell_target_type(&mut out, None, TYPE_CREATURE, false),
        Err(S_TARGET_WANTED.to_string())
    );
}

fn display_strings(out: &RecordingSink) -> Vec<(u32, String)> {
    out.0
        .iter()
        .filter_map(|n| match n {
            dereth_client_model::Notice::DisplayString { channel, text } => {
                Some((*channel, text.clone()))
            }
            _ => None,
        })
        .collect()
}

// ------------------------------------------------------------------------------------------
// 7. The casting-panel producer
// ------------------------------------------------------------------------------------------

/// The casting panel's three arms, driven through the panel and consumed by the
/// production request arm.
///
/// The panel is exercised without a UI tree: unbound, the open-submenu index is 0 — the
/// client's own `default` — which is the tab the assertions use. What is under test here is the
/// **order of the three arms**, which is the part a reader gets wrong: the spell id is read
/// first, the endowment test wins over it, and the zero-spell arm sends nothing.
#[test]
fn the_spell_bars_cast_button_raises_the_request_and_the_endowment_wins() {
    use dereth_ui_screens::panels::spellcasting::SpellcastingPanel;
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    let view = EmptyView;

    let mut p = SpellcastingPanel::default();
    p.sub_menus = vec![Default::default(); 8];
    // Arm 3: nothing selected -> nothing at all. Not a request, not a refusal.
    assert_eq!(p.cast(&mut ui), None);
    assert_eq!(p.casts, 0);

    // Arm 3 -> arm 1: a selected spell casts.
    p.set_selected(&mut ui, 0, FLAME_BOLT);
    assert_eq!(
        p.cast(&mut ui),
        Some(UiRequest::CastSpell {
            spell_id: FLAME_BOLT
        })
    );
    assert_eq!(p.casts, 1);

    // Arm 2: with the endowment slot selected and a wand present, the same button **uses the
    // wand** even though a spell id is still sitting in the sub-menu. That ordering is the
    // client's and is the thing this assertion exists for.
    let wand = ObjectId(0x5000_0F00);
    p.endowment_item = Some(wand);
    p.sub_menus[0].endowment_selected = true;
    assert_eq!(p.cast(&mut ui), Some(UiRequest::Use(wand)));
    assert_eq!(p.casts, 1, "a use is not a cast");
    assert_eq!(
        p.sub_menus[0].selected_spell, FLAME_BOLT,
        "and the spell is still selected"
    );

    // An endowment selected with **no** endowment item falls through to the spell — the
    // client's guard is `&&`, not `||`.
    p.endowment_item = None;
    assert_eq!(
        p.cast(&mut ui),
        Some(UiRequest::CastSpell {
            spell_id: FLAME_BOLT
        })
    );

    // A current-spell notice invokes the same cast action and nothing else.
    let before = p.casts;
    assert!(p.cast_current_spell(&mut ui).is_some());
    assert_eq!(p.casts, before + 1);

    // A quickslot cast first requires an actual spell-item list.
    // The former model-only positive cast from an unbound submenu was not a retail path.
    // Populated positive and empty-slot cases run through the shipped dats in the gpu tier;
    // this unbound case proves the null-list negative.
    let mut q = SpellcastingPanel::default();
    q.sub_menus = vec![Default::default(); 8];
    let bars = BarView(vec![STRENGTH_SELF, FLAME_BOLT]);
    assert_eq!(q.cast_quickslot_spell(&mut ui, 1, &bars), None);
    assert_eq!(q.sub_menus[0].selected_spell, 0);
    // A slot past the end does nothing at all — not even a refusal.
    assert_eq!(q.cast_quickslot_spell(&mut ui, 9, &bars), None);
    assert_eq!(q.casts, 0);
    let _ = &view;
}

/// Behaviour: magic.cast.a-spell-bar-click-puts-the-cast-request-on-the-wire
/// The panel's request, carried through `Interaction::run_ui_requests` into a `0x004A`.
///
/// This is the join the row is about: a click on the spell bar becomes the game action the client
/// sends. **Falsified by** deleting the `UiRequest::CastSpell` arm — `unowned` then carries it.
#[test]
fn a_click_on_the_spell_bar_puts_a_cast_on_the_wire() {
    use dereth_ui_screens::panels::spellcasting::SpellcastingPanel;
    let mut ui = dereth_ui::UiSystem::new((800, 600));
    let mut p = SpellcastingPanel::default();
    p.sub_menus = vec![Default::default(); 8];
    p.set_selected(&mut ui, 0, FLAME_BOLT);
    let r = p.cast(&mut ui).expect("the panel raised it");

    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());
    let mut inter = Interaction::default();
    inter.queue(Vec::new(), vec![r]);
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    let [Request::CastTargetedSpell(m)] = inter.pending_requests() else {
        panic!("{:?}", inter.pending_requests())
    };
    assert_eq!((m.target, m.spell_id), (TARGET, FLAME_BOLT));
    assert_eq!(inter.stats.spells_cast, 1);
}

#[derive(Debug)]
struct EmptyView;
impl dereth_ui_screens::view::GameView for EmptyView {}

#[derive(Debug)]
struct BarView(Vec<u32>);
impl dereth_ui_screens::view::GameView for BarView {
    fn spell_tab(&self, tab: usize) -> &[u32] {
        if tab == 0 {
            &self.0
        } else {
            &[]
        }
    }
}

// ------------------------------------------------------------------------------------------
// 8. The movement stop that begins a sent cast.
// ------------------------------------------------------------------------------------------

/// Behaviour: magic.cast.a-sent-cast-stops-the-players-body
/// **A cast stops the player.** Before sending the message or incrementing the busy count, the
/// original client asks the controlled body to stop completely. If the server controls the body,
/// that request returns without stopping it; otherwise it dispatches a complete stop. The
/// untargeted branch inlines the same behavior, so **every** cast that is actually sent runs it.
///
/// Without the stop, a player running toward a distant target keeps running
/// through the whole windup, and ACE disrupts a cast whose caster moved more than
/// `Windup_MaxMove = 6.0` metres — mana and components gone, nothing cast. Retail's client never
/// gets there because it stopped him the moment he pressed the button.
///
/// **There is no client-side range check to be found**: the casting path's only local
/// refusals are the component loop, `"You must select a suitable target before casting this
/// spell"`, and the target path's three refusals (self / stack /
/// wrong type). Nothing in either function reads a position, and the client's whole magic-error
/// vocabulary — `You don't know that spell!`, `Incorrect target type`, `You don't have
/// all the components for this spell.`, `You don't have enough Mana to cast this spell.`, `Your
/// spell fizzled.`, `Your spell's target is missing!`, `Your projectile spell mislaunched!`,
/// `You've attempted an impossible spell path!`, `Your spell cannot be cast inside/outside` —
/// contains no distance message at all. Range is the server's to refuse.
#[test]
fn a_cast_that_is_sent_asks_the_body_to_stop() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(inter.stats.spells_cast, 1, "the cast was sent");
    assert!(
        inter.take_stop_completely(),
        "the targeted send requests a complete stop BEFORE emitting the message; the \
         interaction frame consumes that flag and turns it into \
         the body's stop-from-action request"
    );
    assert!(
        !inter.take_stop_completely(),
        "and it is drained, not sticky"
    );
}

/// Behaviour: magic.cast.a-sent-cast-stops-the-players-body
/// The self-targeted branch enters the shared targeted-send stop path with the player as target,
/// so it is asserted separately.
#[test]
fn a_self_targeted_cast_stops_the_body_too() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, false);

    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: STRENGTH_SELF,
        }],
    );
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(inter.stats.spells_cast, 1);
    assert!(
        inter.take_stop_completely(),
        "the self-targeted cast follows the same stop-before-send \
        path as the targeted cast"
    );
}

/// Behaviour: magic.cast.a-sent-cast-stops-the-players-body
/// **A refusal is not a cast.** Every local refusal returns before the send-and-stop path, so
/// nothing stops. In particular, the `"You must select a suitable target"` arm returns rather than
/// falling through.
#[test]
fn a_refused_cast_does_not_stop_the_body() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    // No selection, so casting takes the selected-id-is-zero refusal arm.
    let mut inter = Interaction::default();
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(inter.stats.spells_cast, 0, "nothing was sent");
    assert!(!inter.take_stop_completely(), "and nothing was stopped");
}

/// Behaviour: magic.cast.a-sent-cast-stops-the-players-body
/// While the server is driving the body, the stop request returns success without stopping it.
#[test]
fn a_server_controlled_body_is_not_stopped_by_a_cast() {
    let mut w = cast_world(&FULL);
    require_components(&mut w, true);
    w.set_selected_object(Some(TARGET), false, &mut RecordingSink::default());

    let mut inter = Interaction::default();
    inter.note_controlled_by_server(true);
    inter.queue(
        Vec::new(),
        vec![UiRequest::CastSpell {
            spell_id: FLAME_BOLT,
        }],
    );
    assert!(inter
        .run_ui_requests(&mut w, false, ServerTime(0.0))
        .is_empty());
    assert_eq!(inter.stats.spells_cast, 1, "the cast is still sent");
    assert!(
        !inter.take_stop_completely(),
        "a server-controlled body is not stopped even though the cast is \
         still sent"
    );
}
