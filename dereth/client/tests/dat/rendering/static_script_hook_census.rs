//! What a landblock static's default script can raise: every hook type reached from the default
//! script of each retail setup that carries one, followed through `CALL_PES`. A static's hooks act
//! on the static itself, like any physics object's; the census says which arms an emitter host has
//! to honour (the direct sound hook) and which the shipped data never raises.
//! Fixture: the retail dats (a missing install fails). A census of shipped data, not a client
//! behaviour.
//!
//! Behaviour: none (a census of the shipped default scripts, not a client behaviour).

use std::sync::Arc;

use dereth_assets::{Decode, HookData, PhysicsScript, Setup};
use dereth_dat::{DbType, RetailDatStore};
use dereth_primitives::DataId;

/// The shipped hook-type numbers decoded from physics-script payloads.
const SOUND: usize = 1;
const SOUND_TABLE: usize = 2;
const ATTACK: usize = 3;
const ETHEREAL: usize = 6;
const SCALE: usize = 12;
const CREATE_PARTICLE: usize = 13;
const NODRAW: usize = 16;
const DEFAULT_SCRIPT_PART: usize = 18;
const CALL_PES: usize = 19;
const SOUND_TWEAKED: usize = 21;
const SET_OMEGA: usize = 22;
const TEXTURE_VELOCITY: usize = 23;
const SET_LIGHT: usize = 25;

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store_or_fail())
}

/// The whole raisable set, measured over the population the hosts come from.
///
/// This is the number the routing decision rests on: it says that the one arm worth wiring is the
/// direct-wave sound, that the setup sound-table arm is worth 49 hooks of which at most **four**
/// could ever resolve, and that five of the six arms a host cannot honour are raised by
/// **nothing** in the shipped data.
#[test]
fn the_scripted_statics_hook_population_says_what_a_host_can_raise() {
    let store = store();
    let mut setups = 0u32;
    let mut with_sound_table = 0u32;
    let mut instances = [0u32; 27];
    let mut touching = [0u32; 27];

    for id in &store.ids_of(DbType::Setup) {
        let Ok(bytes) = store.read_typed(DbType::Setup, *id) else {
            continue;
        };
        let Ok(s) = Setup::decode_payload(*id, &bytes) else {
            continue;
        };
        if s.default_script_id == DataId(0) {
            continue;
        }
        setups += 1;
        if s.default_stable_id != DataId(0) {
            with_sound_table += 1;
        }
        // `CallPES` chains, so the reachable set is a closure and not one script.
        let mut seen = [false; 27];
        let (mut queue, mut done) = (vec![s.default_script_id], Vec::new());
        while let Some(sid) = queue.pop() {
            if done.contains(&sid) {
                continue;
            }
            done.push(sid);
            let Ok(sb) = store.read_typed(DbType::PhysicsScript, sid) else {
                continue;
            };
            let Ok(sc) = PhysicsScript::decode_payload(sid, &sb) else {
                continue;
            };
            for step in &sc.script_data {
                let t = step.hook.hook_type as usize;
                if t < 27 {
                    instances[t] += 1;
                    seen[t] = true;
                }
                if let HookData::CallPes(dereth_primitives::records::HookCallPes { pes, .. }) =
                    step.hook.data
                {
                    queue.push(pes);
                }
            }
        }
        for (t, v) in seen.iter().enumerate() {
            if *v {
                touching[t] += 1;
            }
        }
    }

    let nonzero: Vec<(usize, u32, u32)> = (0..27)
        .filter(|t| instances[*t] > 0)
        .map(|t| (t, touching[t], instances[t]))
        .collect();
    eprintln!(
        "static default-script hook closure over {setups} setup(s) \
         ({with_sound_table} of which name a sound table): {nonzero:?}"
    );

    assert_eq!(setups, 2161, "the scripted-setup population moved");
    // The routable arm, and the reason it is worth a line of code.
    assert_eq!(
        (touching[SOUND_TWEAKED], instances[SOUND_TWEAKED]),
        (302, 524)
    );
    assert_eq!(
        instances[SOUND], 0,
        "the plain SoundHook form is unused in default scripts"
    );
    // Dropped, and the bound on what dropping costs.
    assert_eq!((touching[SOUND_TABLE], instances[SOUND_TABLE]), (49, 49));
    assert_eq!(
        with_sound_table, 4,
        "only four scripted setups name a sound table at all"
    );
    assert_eq!((touching[SCALE], instances[SCALE]), (43, 43));
    assert_eq!(
        (touching[TEXTURE_VELOCITY], instances[TEXTURE_VELOCITY]),
        (11, 11)
    );
    // Resolved inside the driver before the drain ever sees them.
    assert_eq!(
        (touching[CREATE_PARTICLE], instances[CREATE_PARTICLE]),
        (2046, 7757)
    );
    assert_eq!((touching[CALL_PES], instances[CALL_PES]), (317, 536));
    // The arms a host cannot honour and that nothing shipped asks it to.
    for (name, t) in [
        ("ETHEREAL", ETHEREAL),
        ("NODRAW", NODRAW),
        ("SET_OMEGA", SET_OMEGA),
        ("ATTACK", ATTACK),
        ("DEFAULT_SCRIPT_PART", DEFAULT_SCRIPT_PART),
        ("SET_LIGHT", SET_LIGHT),
    ] {
        assert_eq!(
            instances[t], 0,
            "{name} is now raised by a static's default script"
        );
    }
}
