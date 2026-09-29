//! ACE: Source/ACE.Server/Network/GameEvent/Events/GameEventIdentifyObjectResponse.cs::GameEventIdentifyObjectResponse
//! (real-content) Every recorded PlayerDescription goes wire -> biota -> wire as the same sets
//! per table.
//! Fixture: recorded object-update blobs and server/client quality decoders.

use dereth_protocol::login::LoginPlayerDescription;
use dereth_protocol::types::qualities::AcQualities;
use empyrean_entity::adapter::quality_bridge::{
    ac_qualities_to_biota, biota_to_ac_qualities, in_key_order,
};

const GAME_EVENT: u32 = 0xF7B0;
const EV_PLAYER_DESCRIPTION: u32 = 0x0013;

fn u32_at(b: &[u8], at: usize) -> Option<u32> {
    Some(u32::from_le_bytes(b.get(at..at + 4)?.try_into().ok()?))
}

/// Which blocks of two records (already in key order) differ, by name only.
fn differing_blocks(a: &AcQualities, b: &AcQualities) -> Vec<&'static str> {
    let (ta, tb) = (&a.base.tables, &b.base.tables);
    let checks = [
        ("base flags", a.base.flags == b.base.flags),
        ("weenie type", a.base.weenie_type == b.base.weenie_type),
        ("ints", ta.ints == tb.ints),
        ("int64s", ta.int64s == tb.int64s),
        ("bools", ta.bools == tb.bools),
        ("floats", ta.floats == tb.floats),
        ("strings", ta.strings == tb.strings),
        ("dids", ta.dids == tb.dids),
        ("iids", ta.iids == tb.iids),
        ("positions", ta.positions == tb.positions),
        ("vector flags", a.flags == b.flags),
        ("has_health", a.has_health == b.has_health),
        ("attribute cache", a.attribute_cache == b.attribute_cache),
        ("skills", a.skills == b.skills),
        ("spell book", a.spell_book == b.spell_book),
        ("enchantments", a.enchantments == b.enchantments),
        ("event filter", a.event_filter == b.event_filter),
        (
            "creation profiles",
            a.creation_profiles == b.creation_profiles,
        ),
    ];
    checks
        .iter()
        .filter(|(_, same)| !same)
        .map(|(n, _)| *n)
        .collect()
}

#[test]
fn every_recorded_player_description_round_trips_through_a_biota() {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    let sessions = Corpus::shared_all();
    assert!(
        !sessions.is_empty(),
        "the corpus contains recorded sessions"
    );

    let (mut seen, mut bad) = (0usize, Vec::new());
    for s in &sessions {
        let name = &s.name;
        for blob in &s.blobs {
            if blob.dir != Direction::ServerToClient || blob.opcode != GAME_EVENT {
                continue;
            }
            let payload = &blob.payload;
            if u32_at(payload, 12) != Some(EV_PLAYER_DESCRIPTION) {
                continue;
            }
            let idx = blob.idx;
            let pd = dereth_protocol::read_body_padded::<LoginPlayerDescription>(&payload[16..])
                .unwrap_or_else(|e| panic!("{name} #{idx}: the PlayerDescription decodes: {e:?}"));
            seen += 1;
            let wire = in_key_order(&pd.qualities);
            let back = in_key_order(&biota_to_ac_qualities(&ac_qualities_to_biota(
                &pd.qualities,
            )));
            if back != wire {
                bad.push(format!(
                    "{name} #{idx}: {}",
                    differing_blocks(&wire, &back).join(", ")
                ));
            }
        }
    }
    println!(
        "{seen} PlayerDescriptions across {} sessions",
        sessions.len()
    );
    assert!(seen > 0, "the recordings contain player descriptions");
    assert!(
        bad.is_empty(),
        "{} of {seen} differ after wire -> biota -> wire:\n  {}",
        bad.len(),
        bad.join("\n  ")
    );
}
