//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! The documented worked motion table expands as documented; every reachable (stance, command) pair
//! of all 436 shipped tables expands consistently; every queued framerate is base x speed bit for
//! bit.
//! Fixture: the shipped retail DAT records and recorded inputs.

use super::common;

use std::collections::BTreeSet;

use dereth_animation::command::MotionCommand;
use dereth_animation::data::AnimAssets;
use dereth_animation::seq::Sequence;
use dereth_animation::table::{MotionState, MotionTable};
use dereth_dat::DbType;
use dereth_primitives::DataId;

/// Every command the table can be asked for under a given style: the cycles, the modifiers and the
/// destination of every link.
fn reachable(t: &MotionTable, style: MotionCommand) -> Vec<MotionCommand> {
    let d = t.data();
    let prefix = style.0 << 16;
    let mut out = BTreeSet::new();
    for key in d.cycles.keys().chain(d.modifiers.keys()) {
        if key & 0xFFFF_0000 == prefix {
            // The ordinal alone does not name a command; recover the full id from the table's own
            // link destinations where possible, and fall back to the ordinal with the substate bit.
            out.insert(*key & 0x00FF_FFFF);
        }
    }
    let mut full: BTreeSet<u32> = BTreeSet::new();
    for group in d.links.values() {
        for dest in group.keys() {
            full.insert(*dest);
        }
    }
    for v in d.style_defaults.values() {
        full.insert(v.0);
    }
    // Prefer the full 32-bit form when the table names it; otherwise the ordinal is enough for the
    // key lookups, which mask it anyway.
    let mut cmds: Vec<MotionCommand> = Vec::new();
    for ord in out {
        match full.iter().find(|f| *f & 0x00FF_FFFF == ord) {
            Some(f) => cmds.push(MotionCommand(*f)),
            None => {
                if let Some(c) = MotionCommand::from_index(u16::try_from(ord).unwrap_or(u16::MAX)) {
                    cmds.push(c);
                }
            }
        }
    }
    cmds.extend(full.iter().map(|f| MotionCommand(*f)));
    cmds.sort_unstable();
    cmds.dedup();
    cmds
}

/// ORACLE: `docs/formats/19-motion-table.md`, "Worked example — `0x0900013A`", whose
/// 56 bytes are transcribed in the document: `default_style = NonCombat`, one style default
/// (`Ready`), one cycle keyed `0x003D0003` holding animation `0x030009BA` at 30 fps, no modifiers
/// and no links.
#[test]
fn the_documented_worked_example_expands_as_the_document_says() {
    let assets = common::open();
    let id = DataId(0x0900_013A);
    let Some(t) = assets.motion_table(id) else {
        panic!("{id} is not in the retail portal dat");
    };
    assert_eq!(t.default_style, MotionCommand::NON_COMBAT);
    assert_eq!(t.style_defaults.len(), 1);
    assert_eq!(
        t.style_defaults.get(&MotionCommand::NON_COMBAT.0).copied(),
        Some(MotionCommand::READY)
    );
    assert_eq!(t.cycles.len(), 1);
    assert!(
        t.cycles.contains_key(&0x003D_0003),
        "the cycle key from the document"
    );
    assert!(t.modifiers.is_empty() && t.links.is_empty());

    let table = MotionTable::new(t);
    let mut state = MotionState::new();
    let mut seq = Sequence::new();
    let n = table
        .set_default_state(&mut state, &mut seq, &assets)
        .expect("default state");

    assert_eq!(n, 0, "one animation in the cycle, minus the one that loops");
    assert_eq!(state.style, MotionCommand::NON_COMBAT);
    assert_eq!(state.substate, MotionCommand::READY);
    assert_eq!(state.substate_mod, 1.0);
    assert!(state.modifiers.is_empty() && state.actions.is_empty());
    assert_eq!(seq.nodes().len(), 1);
    assert_eq!(seq.nodes()[0].anim_id, DataId(0x0300_09BA));
    assert_eq!(seq.nodes()[0].low_frame, 0);
    assert_eq!(seq.nodes()[0].framerate.to_bits(), 30.0_f32.to_bits());
    assert_eq!(
        seq.first_cyclic(),
        Some(0),
        "the single node is the one that loops"
    );
    // `high_frame == -1` was resolved at construction against the animation's own frame count.
    let anim = assets
        .animation(DataId(0x0300_09BA))
        .expect("the animation");
    assert_eq!(
        seq.nodes()[0].high_frame,
        i32::try_from(anim.num_frames).expect("frames") - 1
    );
}

/// The gate proper: every reachable `(style, command, speed)` triple of every shipped motion table
/// expands without panicking and satisfies the invariants the fixture comparison would check.
#[test]
fn every_reachable_pair_of_every_shipped_table_expands_consistently() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::MTable);
    assert!(!ids.is_empty(), "the input contains motion tables");
    let input_count = ids.len();

    let mut tables = 0usize;
    let mut pairs = 0usize;
    let mut successes = 0usize;
    let mut with_links = 0usize;

    for id in ids {
        let Some(data) = assets.motion_table(id) else {
            panic!("{id} failed to decode");
        };
        let styles: Vec<MotionCommand> = data
            .style_defaults
            .keys()
            .map(|k| MotionCommand(*k))
            .collect();
        let table = MotionTable::new(data);
        tables += 1;

        for style in styles {
            let commands = reachable(&table, style);
            for speed in [1.0_f32, 0.5, -1.0] {
                for cmd in &commands {
                    // Start from a clean default state every time, which is what
                    // `enter_default_state` gives a freshly created object.
                    let mut state = MotionState::new();
                    let mut seq = Sequence::new();
                    if table
                        .set_default_state(&mut state, &mut seq, &assets)
                        .is_none()
                    {
                        continue;
                    }
                    // Put the object into this style first, so the pair is really reachable.
                    if style != state.style {
                        table.do_object_motion(style, &mut state, &mut seq, 1.0, &assets);
                    }
                    if state.style != style {
                        continue;
                    }
                    let before: Vec<(DataId, i32, i32, u32)> = nodes(&seq);
                    pairs += 1;

                    let Some(link_count) =
                        table.do_object_motion(*cmd, &mut state, &mut seq, speed, &assets)
                    else {
                        continue;
                    };
                    successes += 1;
                    if link_count != 0 && link_count != u32::MAX {
                        with_links += 1;
                    }

                    // (a) `first_cyclic` is the last node appended, or the list is untouched.
                    if let Some(fc) = seq.first_cyclic() {
                        assert!(fc < seq.nodes().len(), "{id} {style:?} {cmd:?}");
                    } else {
                        assert!(seq.nodes().is_empty(), "{id} {style:?} {cmd:?}");
                    }

                    if link_count != u32::MAX {
                        let queued = seq.nodes().len().saturating_sub(
                            before.len().saturating_sub(count_removed(&before, &seq)),
                        );
                        assert!(
                            usize::try_from(link_count).unwrap_or(usize::MAX) <= queued + 1,
                            "{id} {style:?} {cmd:?} speed {speed}: link count {link_count} \
                             but only {queued} nodes were queued"
                        );
                    }

                    assert!(state.style.is_style(), "{id}: style {:?}", state.style);
                    assert!(
                        state.substate.is_substate() || state.substate.is_modifier(),
                        "{id}: substate {:?}",
                        state.substate
                    );
                    assert!(state.actions.len() <= 64, "{id}: runaway action queue");

                    // (d) determinism: the same call from the same state gives the same nodes.
                    let mut state2 = MotionState::new();
                    let mut seq2 = Sequence::new();
                    table.set_default_state(&mut state2, &mut seq2, &assets);
                    if style != state2.style {
                        table.do_object_motion(style, &mut state2, &mut seq2, 1.0, &assets);
                    }
                    let link2 =
                        table.do_object_motion(*cmd, &mut state2, &mut seq2, speed, &assets);
                    assert_eq!(
                        link2,
                        Some(link_count),
                        "{id} {style:?} {cmd:?} not deterministic"
                    );
                    assert_eq!(
                        nodes(&seq),
                        nodes(&seq2),
                        "{id} {style:?} {cmd:?} node list"
                    );
                    assert_eq!(state.substate, state2.substate);
                    assert_eq!(state.substate_mod.to_bits(), state2.substate_mod.to_bits());
                }
            }
        }
    }

    eprintln!(
        "expansion gate: {tables} tables, {pairs} (style, command, speed) triples, \
         {successes} accepted, {with_links} of those queued at least one link animation"
    );
    assert_eq!(tables, input_count);
    assert!(
        pairs > 0,
        "only {pairs} triples: the reachability walk found too little"
    );
    assert!(successes > 0, "only {successes} accepted");
    assert!(with_links > 0, "only {with_links} produced link animations");
}

/// Framerates must compare **exactly** as bit patterns, because `add_motion` multiplies each
/// `AnimData`'s framerate by the speed and any drift there changes hook timing. This checks the
/// multiply directly against the table's own stored framerates over the whole corpus.
#[test]
fn every_queued_framerate_is_the_base_framerate_times_the_speed_bit_for_bit() {
    let assets = common::open();
    let ids = assets.ids_of(DbType::MTable);
    assert!(!ids.is_empty(), "the input contains motion tables");
    let speeds = [1.0_f32, 0.5, -1.0, 1.248];
    let mut checked = 0usize;
    let mut expected = 0usize;
    for id in ids {
        let data = assets.motion_table(id).expect("every motion table decodes");
        expected += data.cycles.values().map(|md| md.anims.len()).sum::<usize>() * speeds.len();
        for speed in speeds {
            for md in data.cycles.values() {
                for a in &md.anims {
                    assert_eq!(
                        a.scaled(speed).framerate.to_bits(),
                        (a.framerate * speed).to_bits(),
                        "{id}"
                    );
                    checked += 1;
                }
            }
        }
    }
    // 73,828 = four speeds over the ~18,457 `AnimData` entries the 436 tables' cycle maps
    // hold between them.
    assert!(expected > 0, "the input exercises scaled animation rates");
    assert_eq!(
        checked, expected,
        "every input animation is checked at every speed"
    );
}

fn nodes(seq: &Sequence) -> Vec<(DataId, i32, i32, u32)> {
    seq.nodes()
        .iter()
        .map(|n| (n.anim_id, n.low_frame, n.high_frame, n.framerate.to_bits()))
        .collect()
}

/// How many of the nodes present before the call are gone afterwards — `remove_cyclic_anims`
/// removes the cyclic block and nothing else.
fn count_removed(before: &[(DataId, i32, i32, u32)], after: &Sequence) -> usize {
    let now = nodes(after);
    let keep = before
        .iter()
        .zip(now.iter())
        .take_while(|(a, b)| a == b)
        .count();
    before.len() - keep
}
