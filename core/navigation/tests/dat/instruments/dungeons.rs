//! Behaviour: none (prints how routes from a dungeon's portal drop to its bottom are driven)
//!
//! For each of a set of dungeons of many floors: the route from where a portal into it lands the
//! character to the lowest place a body walks to from there, planned and then driven by keys
//! through the physics world at 60, 20, 10 and 5 frames a second. Run with
//! `--ignored --nocapture`.

use std::time::Instant;

use dereth_primitives::{CellId, Frame, Position, Quat, Vec3};

use crate::common::*;
use crate::drive::{drive, RUN_SPEED};

/// A dungeon of many floors and where a portal into it lands the character: its landblock, the
/// cell in it, and the place.
pub const DUNGEONS: &[(&str, u16, u16, f32, f32, f32)] = &[
    (
        "Sub-Terranean Cavern",
        0x0116,
        0x042F,
        71.28,
        -114.0,
        -5.995,
    ),
    ("Crystalline", 0x8704, 0x0402, 10.0, -50.0, 6.0),
    ("Desert Dwelling", 0x5863, 0x0252, 181.62, -80.047, 21.511),
    ("Fathomless Chasm", 0x5460, 0x0349, 250.0, -100.0, 42.0),
    ("Black Dominion", 0x010A, 0x0223, 0.0, -70.0, 0.0),
    ("Facility", 0x2D6B, 0x02C4, 141.394, 158.127, 38.405),
    ("Artifex Vault", 0x0137, 0x0341, 188.4, -10.0, 0.0),
    ("Abhorrent Vault", 0x02D4, 0x0260, 70.0, -20.0, 0.005),
    ("Oubliette", 0x6349, 0x022C, 50.0, -90.0, 0.005),
    ("Dericost Vault", 0x0071, 0x045A, 80.0, -230.0, 0.005),
    (
        "Temple of the Black Spear",
        0x004F,
        0x0343,
        79.92,
        -34.254,
        0.005,
    ),
    ("Hebian-to Sewers", 0x01CA, 0x027F, 40.0, -177.4, 0.0),
    ("Vile-Smelling Refuse", 0x0280, 0x041C, 60.0, -90.0, 0.005),
    (
        "Training Academy, Outer Courtyard",
        0x8602,
        0x02C3,
        119.0,
        -141.0,
        0.0,
    ),
];

#[test]
#[ignore = "instrument: prints how dungeon routes from the drop to the bottom are driven"]
fn routes_from_the_drop_to_the_bottom_driven_at_four_frame_rates() {
    println!(
        "| dungeon | down m | route m | legs | plan ms | fps | arrived | s | ideal s | stops | stuck |"
    );
    println!("|---|---|---|---|---|---|---|---|---|---|---|");
    for &(name, block, index, x, y, z) in DUNGEONS {
        let cell = (u32::from(block) << 16) | u32::from(index);
        let mut d = dungeon(block);
        let drop = Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY));
        let (from, to) = {
            let Ok(region) = d.nav.region(CellId(cell)) else {
                println!("| {name} | no region |");
                continue;
            };
            let Some(s) = region.nearest_place(&drop, 12.0) else {
                println!("| {name} | no place near the drop |");
                continue;
            };
            let b = bottom(region, s);
            (region.position(s), region.position(b))
        };
        let t = Instant::now();
        let planned = d.nav.plan_toward(&from, &to);
        let ms = t.elapsed().as_secs_f64() * 1e3;
        let Ok((path, _)) = planned else {
            println!("| {name} | no route: {planned:?} |");
            continue;
        };
        let down = from.frame.origin.z - to.frame.origin.z;
        for fps in [60.0, 20.0, 10.0, 5.0] {
            let player = std::sync::Arc::clone(&d.player);
            let Dungeon { world, nav, .. } = &mut d;
            // Each frame rate starts from what the dungeon's data says, not from what the last
            // one's walk taught.
            nav.unblock();
            // As the travel does: the places where the body stopped are taken out, and with no
            // route left they are put back.
            let mut replan = |p: &Position, toward: &Position| {
                nav.block_ahead(p, toward);
                let again = nav.plan_toward(p, &to);
                let again = match again {
                    Err(_) if nav.unblock() > 0 => nav.plan_toward(p, &to),
                    other => other,
                };
                again.ok().map(|(p, _)| p)
            };
            let r = drive(world, &player, path.clone(), fps, &mut replan);
            println!(
                "| {name} | {down:.0} | {:.0} | {} | {ms:.0} | {fps:.0} | {} | {:.1} | {:.1} | {} | {} |",
                path.length(),
                path.waypoints.len() - 1,
                if r.arrived { "yes" } else { "no" },
                r.seconds,
                r.ideal,
                r.stops,
                r.stuck,
            );
        }
    }
    println!("(run speed {RUN_SPEED} m/s)");
}

#[test]
#[ignore = "instrument: prints the drops the planner names where no walk reaches a dungeon's bottom"]
fn drops_named_where_no_walk_reaches_the_bottom() {
    println!("| dungeon | lowest walked to | lowest of all | answer | a body jumping off lands |");
    println!("|---|---|---|---|---|");
    for &(name, block, index, x, y, z) in DUNGEONS {
        let cell = (u32::from(block) << 16) | u32::from(index);
        let mut d = dungeon(block);
        let drop = Position::new(CellId(cell), Frame::new(Vec3::new(x, y, z), Quat::IDENTITY));
        let (from, walked, low) = {
            let Ok(region) = d.nav.region(CellId(cell)) else {
                continue;
            };
            let Some(s) = region.nearest_place(&drop, 12.0) else {
                continue;
            };
            let Some(l) = lowest(region) else { continue };
            (
                region.position(s),
                region.position(bottom(region, s)),
                region.position(l),
            )
        };
        let answer = d.nav.plan(&from, &low);
        let (said, lands) = match &answer {
            Ok(p) => (format!("a walk of {:.0} m", p.length()), String::from("-")),
            Err(dereth_navigation::NoPath::OnlyByDrop {
                stand,
                land,
                height,
            }) => {
                let fell = crate::drive::jump_off(&mut d.world, &d.player, stand, land);
                (
                    format!(
                        "a drop of {height:.1} m, off {:08X} {:?}",
                        stand.cell.0, stand.frame.origin
                    ),
                    fell.map_or_else(
                        || String::from("no"),
                        |(across, rise)| format!("{across:.1} m across, {rise:.1} m up or down"),
                    ),
                )
            }
            Err(e) => (format!("{e}"), String::from("-")),
        };
        println!(
            "| {name} | {:.1} | {:.1} | {said} | {lands} |",
            walked.frame.origin.z, low.frame.origin.z
        );
    }
}
