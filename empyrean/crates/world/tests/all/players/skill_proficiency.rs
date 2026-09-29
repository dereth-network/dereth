//! ACE: Source/ACE.Server/Entity/Proficiency.cs::OnSuccessUse
//! Proficiency.OnSuccessUse scales PP by elapsed time and grants 110% as XP on the player's
//! queue; max-level/untrained/negative cases; player action queue waits for its tick;
//! augmentation tables are ACE's.
//! Fixture: isolated world state.

use empyrean_entity::enums::{AugmentationType, PropertyInt, PropertyInt64, Skill};
use empyrean_entity::ObjectGuid;
use empyrean_world::entity::actions::action_queue::run_actions;
use empyrean_world::entity::actions::i_actor::Actor;
use empyrean_world::entity::proficiency;
use empyrean_world::world_objects::augmentation_device;
use empyrean_world::World;

use super::stats::{player_world, PLAYER};

fn guid() -> ObjectGuid {
    ObjectGuid::new(PLAYER)
}

/// The stats player at level 2 with `total` experience and 1,000 unassigned, at `now`.
fn world(total: i64) -> World {
    let mut w = player_world();
    let o = w.objects.get_mut(guid()).unwrap();
    o.set_property(PropertyInt::Level, 2);
    o.set_property(PropertyInt64::TotalExperience, total);
    o.set_property(PropertyInt64::AvailableExperience, 1000);
    w
}

/// One successful use of Run against `difficulty` at `now`, then the player's queue (the granted
/// XP) runs.
fn use_run(w: &mut World, now: f64, difficulty: u32) {
    w.now.unix_time = now;
    let skill = proficiency::get_creature_skill(w, guid(), Skill::Run);
    proficiency::on_success_use(w, guid(), skill, difficulty);
    run_actions(w, Actor::Object(guid()));
}

/// (ResistanceAtLastCheck, LastUsedTime, ExperienceSpent) of Run.
fn run_record(w: &World) -> (u32, f64, u32) {
    let o = w.objects.get(guid()).unwrap();
    let r = o.biota.get_skill(Skill::Run).unwrap();
    (r.resistance_at_last_check, r.last_used_time, r.pp)
}

/// (TotalExperience, AvailableExperience).
fn xp(w: &World) -> (Option<i64>, Option<i64>) {
    let o = w.objects.get(guid()).unwrap();
    (o.total_experience(), o.available_experience())
}

#[test]
#[allow(clippy::float_cmp)]
fn a_harder_use_grants_scaled_proficiency_experience() {
    let mut w = world(1000);
    use_run(&mut w, 600.0, 300);
    // 200 PP raise Run from the 1,000 already there; 220 XP arrive from the queue
    assert_eq!(run_record(&w), (300, 600.0, 200));
    assert_eq!(xp(&w), (Some(1220), Some(1000 - 200 + 220)));
    assert!(
        w.objects
            .get(guid())
            .unwrap()
            .wo
            .world_object_database
            .changes_detected
    );

    // neither harder nor 15 minutes later: nothing
    use_run(&mut w, 700.0, 250);
    assert_eq!(run_record(&w), (300, 600.0, 200));
    assert_eq!(xp(&w), (Some(1220), Some(1020)));

    // 15 minutes after the last use: recorded at full scale although easier
    use_run(&mut w, 1500.0, 100);
    assert_eq!(run_record(&w), (100, 1500.0, 300));
    assert_eq!(xp(&w), (Some(1330), Some(1020 - 100 + 110)));
}

#[test]
#[allow(clippy::float_cmp)]
fn a_rewound_clock_only_moves_the_last_use() {
    let mut w = world(1000);
    use_run(&mut w, 600.0, 300);
    use_run(&mut w, 100.0, 5000);
    assert_eq!(
        run_record(&w),
        (300, 100.0, 200),
        "LastUsedTime = now, nothing else"
    );
    assert_eq!(xp(&w), (Some(1220), Some(1020)));
}

#[test]
#[allow(clippy::float_cmp)]
fn at_max_level_the_use_is_recorded_without_experience() {
    let mut w = world(2500);
    w.objects
        .get_mut(guid())
        .unwrap()
        .set_property(PropertyInt::Level, 3);
    use_run(&mut w, 900.0, 300);
    assert_eq!(run_record(&w), (300, 900.0, 0));
    assert_eq!(xp(&w), (Some(2500), Some(1000)));
}

#[test]
#[allow(clippy::float_cmp)]
fn the_grant_stops_at_the_last_level() {
    // 100 XP left to level 3 (the max): 1,000 PP at full scale would be 1,100 XP, so the grant is
    // 100 and the PP `Math.Round(100 / 1.1f)` = 91
    let mut w = world(2400);
    use_run(&mut w, 900.0, 1000);
    assert_eq!(run_record(&w), (1000, 900.0, 91));
    let (total, available) = xp(&w);
    assert_eq!(total, Some(2500));
    assert_eq!(available, Some(1000 - 91 + 100));
}

#[test]
fn untrained_olthoi_and_negative_uses_grant_nothing() {
    let mut w = world(1000);
    w.now.unix_time = 600.0;
    // an untrained skill
    let jump = proficiency::get_creature_skill(&mut w, guid(), Skill::Jump);
    proficiency::on_success_use(&mut w, guid(), jump, 300);
    assert_eq!(
        w.objects
            .get(guid())
            .unwrap()
            .biota
            .get_skill(Skill::Jump)
            .unwrap()
            .resistance_at_last_check,
        0
    );
    // the int overload refuses a negative difficulty
    let run = proficiency::get_creature_skill(&mut w, guid(), Skill::Run);
    proficiency::on_success_use_int(&mut w, guid(), run, -1);
    assert_eq!(run_record(&w).0, 0);
    // an olthoi player
    w.objects
        .get_mut(guid())
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player_properties
        .is_olthoi_player = true;
    proficiency::on_success_use(&mut w, guid(), run, 300);
    assert_eq!(run_record(&w).0, 0);
    run_actions(&mut w, Actor::Object(guid()));
    assert_eq!(xp(&w), (Some(1000), Some(1000)));
}

#[test]
fn a_players_actions_wait_for_its_own_tick() {
    let mut w = world(1000);
    let hits = std::sync::Arc::new(std::sync::atomic::AtomicU32::new(0));
    let h = hits.clone();
    empyrean_world::entity::actions::i_actor::enqueue(
        &mut w,
        Actor::Object(guid()),
        empyrean_world::entity::actions::i_action::Action::delegate(move |_| {
            h.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        }),
    );
    assert_eq!(
        hits.load(std::sync::atomic::Ordering::SeqCst),
        0,
        "queued, not run"
    );
    assert_eq!(
        empyrean_world::world_objects::player_tick::action_queue_mut(&mut w, guid())
            .map(|q| q.len()),
        Some(1)
    );
    run_actions(&mut w, Actor::Object(guid()));
    assert_eq!(hits.load(std::sync::atomic::Ordering::SeqCst), 1);
}

#[test]
fn augmentation_tables_are_aces() {
    use empyrean_entity::enums::ext::aug_type_helper;
    // every type but None has both entries; the families share the caps ACE lists
    assert_eq!(augmentation_device::MAX_AUGS.len(), 41);
    assert_eq!(augmentation_device::AUG_PROPS.len(), 41);
    assert_eq!(
        augmentation_device::max_augs(AugmentationType::Strength),
        10
    );
    assert_eq!(
        augmentation_device::max_augs(AugmentationType::BurdenLimit),
        5
    );
    assert_eq!(
        augmentation_device::max_augs(AugmentationType::BonusSalvage),
        4
    );
    assert_eq!(
        augmentation_device::max_augs(AugmentationType::DeathItemLoss),
        3
    );
    assert_eq!(
        augmentation_device::max_augs(AugmentationType::ResistFire),
        2
    );
    assert_eq!(
        augmentation_device::aug_props(AugmentationType::ResistBludgeon),
        PropertyInt::AugmentationResistanceBlunt
    );
    assert_eq!(
        augmentation_device::aug_props(AugmentationType::ResistCold),
        PropertyInt::AugmentationResistanceFrost
    );
    assert_eq!(
        augmentation_device::aug_props(AugmentationType::AllStats),
        PropertyInt::AugmentationJackOfAllTrades
    );
    for &(t, _) in augmentation_device::MAX_AUGS {
        let _ = augmentation_device::aug_props(t);
        if aug_type_helper::is_attribute(t) {
            assert_eq!(augmentation_device::max_augs(t), 10);
        }
        if aug_type_helper::is_resist(t) {
            assert_eq!(augmentation_device::max_augs(t), 2);
        }
    }
    assert!(
        std::panic::catch_unwind(|| augmentation_device::aug_props(AugmentationType::None))
            .is_err(),
        "KeyNotFoundException"
    );
}
