//! ACE: Source/ACE.Server/WorldObjects/Creature_Vitals.cs::VitalHeartBeat
//! Vitals through game messages.
//! Fixture: shared virtual-time server state and recorded content where enabled.

#[cfg(feature = "real-content")]
mod regeneration_real {
    //! ACE: Source/ACE.Server/WorldObjects/Creature_Vitals.cs::VitalHeartBeat
    use crate::support::real_content_bot::real::*;

    /// Vitals regenerate: 60 s after losing half its health, a resting character has more.
    #[test]
    fn vitals_regenerate_over_time() {
        let mut l = create_and_enter();
        let g = l.g;
        let vital = l.ts.world.objects.get(g).expect("in the world").health();
        let max = vital.max_value(&mut StatCtx::in_world(&mut l.ts.world, g));
        let half = i32::try_from(max / 2).expect("small");
        empyrean_world::dispatch::update_vital::update_vital(&mut l.ts.world, g, vital, half);
        l.advance(60.0);
        assert!(
            l.health(g).unwrap_or(0) > max / 2,
            "health regenerates at rest ({:?} of {max})",
            l.health(g)
        );
    }
}
