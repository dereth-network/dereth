//! Every emote and stance key animates the human body, as a property of the data files.
//!
//! The emote input-action table (`dereth_client_runtime::actions::emote`) maps 91 input actions to
//! motion commands, which the client issues unchanged. The human motion table (`0x09000001`, which
//! every human uses) plays a looping state by the packed `(style << 16) | ordinal` key of its
//! `cycles` and a one-shot action by the full command among the inner keys of its `links`; every
//! entry must resolve to one or the other in the end-of-retail numbering (`/snowangel` is
//! `0x43000118`, not the earlier client's `0x43000115`).
//!
//! Fixture: the retail `client_portal.dat` under `$DERETH_TEST_DAT_DIR`; it never skips.

use dereth_assets::Decode as _;
use dereth_client_runtime::actions::emote::INPUT_ACTION_COMMANDS;
use dereth_dat::DbType;
use dereth_primitives::DataId;

const HUMAN_MOTION_TABLE: DataId = DataId(0x0900_0001);
const NON_COMBAT: u32 = 0x8000_003D;

/// Behaviour: movement.emote.every-emote-and-stance-key-animates-the-human-body
#[test]
fn every_emote_and_stance_key_animates_the_human_body() {
    let store = dereth_dat::testing::open_store().expect("the retail dats are this test's oracle");
    let bytes = store
        .read_typed(DbType::MTable, HUMAN_MOTION_TABLE)
        .expect("the human motion table reads");
    let mt = dereth_assets::MotionTable::decode_payload(HUMAN_MOTION_TABLE, &bytes)
        .expect("and decodes");
    let mt = dereth_world_data::anim_convert::motion_table(&mt);

    let style = (NON_COMBAT & 0xFFFF) << 16;
    let linked: std::collections::BTreeSet<u32> = mt
        .links
        .values()
        .flat_map(|inner| inner.keys().copied())
        .collect();
    let mut silent = Vec::new();
    for &(action, cmd) in INPUT_ACTION_COMMANDS {
        let c = dereth_animation::MotionCommand(cmd);
        assert!(
            c.to_index().is_some(),
            "{action:#010X} -> {cmd:#010X} is not in the command table"
        );
        let plays = if c.is_substate() {
            mt.cycles.contains_key(&(style | c.ordinal()))
        } else {
            linked.contains(&cmd)
        };
        if !plays {
            silent.push(format!("{action:#010X} -> {c:?}"));
        }
    }
    assert!(
        silent.is_empty(),
        "emote keys the human body cannot play:\n  {}",
        silent.join("\n  ")
    );
}
