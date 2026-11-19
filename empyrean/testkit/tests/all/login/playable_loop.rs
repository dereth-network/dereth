//! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
//! Playable loop through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

#[cfg(feature = "real-content")]
mod journey_real {
    //! ACE: Source/ACE.Server/Managers/WorldManager.cs::DoPlayerEnterWorld
    use crate::support::real_content_bot::real::*;

    /// The whole loop.
    #[test]
    fn a_new_character_plays_from_chargen_to_relog() {
        let mut l = create_and_enter();
        // This bot walks in straight lines through the Academy's furniture and the drudges it
        // fights, which no client does, so the motion correction (on by default) would rightly
        // send it back: the journey runs without it. The correction has its own tests
        // (`movement::position_cadence::server_movement`).
        assert!(empyrean_world::managers::property_manager::modify_bool(
            &l.ts.world,
            "server_motion_correction",
            false
        ));
        empyrean_world::world_objects::player_tick::refresh_movement_options(&mut l.ts.world);
        walk_run_jump(&mut l);
        pick_up_and_wield(&mut l);
        let create_spot = l.location();
        let looted = kill_and_loot_a_drudge(&mut l);
        die_and_respawn(&mut l, &create_spot);
        relog(&mut l, looted);

        println!(
            "Playable loop: {} messages received, all decoded; not_ported sites reached:",
            l.ts.received_raw(l.id).len()
        );
        for (site, n) in &l.not_ported {
            println!("  {n:>7}  {site}");
        }
    }
}
