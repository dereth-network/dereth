//! How many of the retail dat's animations and physics scripts carry each of the 27 hook types:
//! the population behind `dereth_animation::AnimEvent`, which says whether a hook the scene
//! handles fires never, rarely or constantly (ETHEREAL is on 47 of 2,066 animations, NODRAW on none),
//! and whether the shipped ETHEREAL hooks close every door they open (as many turn the bit off as on).
//!
//! Fixture: every animation and physics script in the retail `client_portal.dat`. Each census
//! asserts its own denominator (every enumerated record decodes), so a broken decoder cannot
//! report a small population as the real one.
//!
//! Behaviour: none (a census of the shipped animations and scripts, not a client behaviour).

use std::sync::Arc;

use dereth_assets::{Animation, Decode};
use dereth_dat::{DbType, RetailDatStore};

fn store() -> Arc<RetailDatStore> {
    Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the hook census needs the retail dats under {} (set DERETH_TEST_DAT_DIR)",
            dereth_dat::testing::dat_dir().display()
        )
    }))
}

/// The original hook decoder's type numbers in order; the array index is the encoded type.
const HOOK_NAMES: [&str; 27] = [
    "NOOP",
    "SOUND",
    "SOUND_TABLE",
    "ATTACK",
    "ANIMDONE",
    "REPLACE_OBJECT",
    "ETHEREAL",
    "TRANSPARENT_PART",
    "LUMINOUS",
    "LUMINOUS_PART",
    "DIFFUSE",
    "DIFFUSE_PART",
    "SCALE",
    "CREATE_PARTICLE",
    "DESTROY_PARTICLE",
    "STOP_PARTICLE",
    "NODRAW",
    "DEFAULT_SCRIPT",
    "DEFAULT_SCRIPT_PART",
    "CALL_PES",
    "TRANSPARENT",
    "SOUND_TWEAKED",
    "SET_OMEGA",
    "TEXTURE_VELOCITY",
    "TEXTURE_VELOCITY_PART",
    "SET_LIGHT",
    "CREATE_BLOCKING_PARTICLE",
];

/// Oracle: decode every `0x03xxxxxx` animation in `client_portal.dat` and count every hook.
#[test]
fn the_retail_dat_hook_type_census() {
    let store = store();
    let ids = store.ids_of(DbType::Anim);
    assert!(
        ids.len() > 2000,
        "only {} animation(s): not the retail portal.dat",
        ids.len()
    );

    // Per type: how many animations carry at least one, and how many instances in total.
    let mut anims = [0u32; 27];
    let mut instances = [0u32; 27];
    let (mut parsed, mut failed) = (0u32, 0u32);
    let mut unknown_types: Vec<u32> = Vec::new();
    // ETHEREAL hooks by direction: a non-zero `ethereal` turns the bit on, a zero turns it off.
    let (mut ethereal_on, mut ethereal_off) = (0u32, 0u32);

    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::Anim, *id) else {
            failed += 1;
            continue;
        };
        let Ok(anim) = Animation::decode_payload(*id, &bytes) else {
            failed += 1;
            continue;
        };
        parsed += 1;
        let mut seen = [false; 27];
        for frame in &anim.part_frames {
            for hook in &frame.hooks {
                if let dereth_assets::HookData::Ethereal { ethereal } = hook.data {
                    if ethereal != 0 {
                        ethereal_on += 1;
                    } else {
                        ethereal_off += 1;
                    }
                }
                let t = hook.hook_type as usize;
                if t < 27 {
                    seen[t] = true;
                    instances[t] += 1;
                } else if !unknown_types.contains(&hook.hook_type) {
                    unknown_types.push(hook.hook_type);
                }
            }
        }
        for (t, s) in seen.iter().enumerate() {
            if *s {
                anims[t] += 1;
            }
        }
    }

    eprintln!(
        "hook-type census over {parsed} animation(s) of {} enumerated:",
        ids.len()
    );
    for t in 0..27 {
        eprintln!(
            "  {t:>2} {:<26} {:>5} animation(s), {:>6} instance(s)",
            HOOK_NAMES[t], anims[t], instances[t]
        );
    }
    eprintln!("  unknown hook types seen: {unknown_types:?}");

    assert_eq!(
        failed, 0,
        "{failed} animation(s) would not decode; the census has a hole in it"
    );
    assert_eq!(usize::try_from(parsed).expect("fits"), ids.len());
    assert!(
        unknown_types.is_empty(),
        "hook types outside 0..=26: {unknown_types:?}"
    );

    // The two figures the door tests rely on, derived here by a different code path. If these do
    // not reproduce, nothing else in the table can be trusted.
    assert_eq!(anims[6], 47, "ETHEREAL: 47 of 2,066 animations carry one");
    assert_eq!(anims[16], 0, "NODRAW: no animation carries one");
    assert!(
        ethereal_on > 0 && ethereal_off > 0,
        "the hook must point both ways or a door is a trap"
    );
    assert_eq!(
        ethereal_on, ethereal_off,
        "every retail ethereal animation is balanced: as many hooks open as close. An imbalance \
         would mean a door somewhere can be left permanently ethereal by its own data"
    );
}

/// The **second** hook carrier, and the one that changes the answer.
///
/// `MotionDriver::update_scripts` routes a physics script's hooks through the *same* `events`
/// vector as an animation's (`core/animation/src/driver.rs`), so an `AnimEvent` variant that no
/// animation raises may still be raised constantly by physics scripts. NODRAW is the example: its
/// only retail uses are in physics scripts.
#[test]
fn the_retail_dat_physics_script_hook_type_census() {
    let store = store();
    let ids = store.ids_of(DbType::PhysicsScript);
    assert!(
        ids.len() > 4000,
        "only {} physics script(s): not the retail portal.dat",
        ids.len()
    );

    let mut scripts = [0u32; 27];
    let mut instances = [0u32; 27];
    let (mut parsed, mut failed) = (0u32, 0u32);

    for id in &ids {
        let Ok(bytes) = store.read_typed(DbType::PhysicsScript, *id) else {
            failed += 1;
            continue;
        };
        let Ok(s) = dereth_assets::PhysicsScript::decode_payload(*id, &bytes) else {
            failed += 1;
            continue;
        };
        parsed += 1;
        let mut seen = [false; 27];
        for step in &s.script_data {
            let t = step.hook.hook_type as usize;
            if t < 27 {
                seen[t] = true;
                instances[t] += 1;
            }
        }
        for (t, v) in seen.iter().enumerate() {
            if *v {
                scripts[t] += 1;
            }
        }
    }

    eprintln!(
        "physics-script hook census over {parsed} script(s) of {} enumerated:",
        ids.len()
    );
    for t in 0..27 {
        eprintln!(
            "  {t:>2} {:<26} {:>5} script(s), {:>6} instance(s)",
            HOOK_NAMES[t], scripts[t], instances[t]
        );
    }

    assert_eq!(failed, 0, "{failed} physics script(s) would not decode");
    assert_eq!(usize::try_from(parsed).expect("fits"), ids.len());
}
