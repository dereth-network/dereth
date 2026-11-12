//! Which of a pack body's clip slots ([`Slot`]) an object's motion fills each frame: the one table
//! from the motion commands creatures' motion tables play to the slots, and what the remaster is
//! told of the motion its parts play ([`rm::Motion`]).

use dereth_animation::command::MotionCommand as C;
use dereth_remaster::assets::skin::Slot;
use dereth_remaster::scene as rm;

/// Every motion command a slot is for, beyond a stance's standing still. A cycle no row names is
/// stood in by its stance's idle; an action no row names leaves what plays going on.
const SLOTS: &[(C, Slot)] = &[
    (C::WALK_FORWARD, Slot::Walk),
    (C::WALK_BACKWARDS, Slot::WalkBack),
    (C::RUN_FORWARD, Slot::Run),
    (C::SIDE_STEP_RIGHT, Slot::SideStep),
    (C::SIDE_STEP_LEFT, Slot::SideStep),
    (C::TURN_RIGHT, Slot::Turn),
    (C::TURN_LEFT, Slot::Turn),
    // Swings by the height they land at.
    (C::ATTACK_HIGH1, Slot::MeleeHigh),
    (C::ATTACK_HIGH2, Slot::MeleeHigh),
    (C::ATTACK_HIGH3, Slot::MeleeHigh),
    (C::ATTACK_HIGH4, Slot::MeleeHigh),
    (C::ATTACK_HIGH5, Slot::MeleeHigh),
    (C::ATTACK_HIGH6, Slot::MeleeHigh),
    (C::THRUST_HIGH, Slot::MeleeHigh),
    (C::SLASH_HIGH, Slot::MeleeHigh),
    (C::BACKHAND_HIGH, Slot::MeleeHigh),
    (C::DOUBLE_SLASH_HIGH, Slot::MeleeHigh),
    (C::TRIPLE_SLASH_HIGH, Slot::MeleeHigh),
    (C::DOUBLE_THRUST_HIGH, Slot::MeleeHigh),
    (C::TRIPLE_THRUST_HIGH, Slot::MeleeHigh),
    (C::PUNCH_FAST_HIGH, Slot::MeleeHigh),
    (C::PUNCH_SLOW_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_SLASH_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_THRUST_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_DOUBLE_SLASH_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_TRIPLE_SLASH_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_DOUBLE_THRUST_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_TRIPLE_THRUST_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_PUNCH_FAST_HIGH, Slot::MeleeHigh),
    (C::OFFHAND_PUNCH_SLOW_HIGH, Slot::MeleeHigh),
    (C::ATTACK_MED1, Slot::MeleeMed),
    (C::ATTACK_MED2, Slot::MeleeMed),
    (C::ATTACK_MED3, Slot::MeleeMed),
    (C::ATTACK_MED4, Slot::MeleeMed),
    (C::ATTACK_MED5, Slot::MeleeMed),
    (C::ATTACK_MED6, Slot::MeleeMed),
    (C::THRUST_MED, Slot::MeleeMed),
    (C::SLASH_MED, Slot::MeleeMed),
    (C::BACKHAND_MED, Slot::MeleeMed),
    (C::DOUBLE_SLASH_MED, Slot::MeleeMed),
    (C::TRIPLE_SLASH_MED, Slot::MeleeMed),
    (C::DOUBLE_THRUST_MED, Slot::MeleeMed),
    (C::TRIPLE_THRUST_MED, Slot::MeleeMed),
    (C::PUNCH_FAST_MED, Slot::MeleeMed),
    (C::PUNCH_SLOW_MED, Slot::MeleeMed),
    (C::OFFHAND_SLASH_MED, Slot::MeleeMed),
    (C::OFFHAND_THRUST_MED, Slot::MeleeMed),
    (C::OFFHAND_DOUBLE_SLASH_MED, Slot::MeleeMed),
    (C::OFFHAND_TRIPLE_SLASH_MED, Slot::MeleeMed),
    (C::OFFHAND_DOUBLE_THRUST_MED, Slot::MeleeMed),
    (C::OFFHAND_TRIPLE_THRUST_MED, Slot::MeleeMed),
    (C::OFFHAND_PUNCH_FAST_MED, Slot::MeleeMed),
    (C::OFFHAND_PUNCH_SLOW_MED, Slot::MeleeMed),
    (C::OFFHAND_KICK, Slot::MeleeMed),
    (C::BITE, Slot::MeleeMed),
    (C::ATTACK_LOW1, Slot::MeleeLow),
    (C::ATTACK_LOW2, Slot::MeleeLow),
    (C::ATTACK_LOW3, Slot::MeleeLow),
    (C::ATTACK_LOW4, Slot::MeleeLow),
    (C::ATTACK_LOW5, Slot::MeleeLow),
    (C::ATTACK_LOW6, Slot::MeleeLow),
    (C::THRUST_LOW, Slot::MeleeLow),
    (C::SLASH_LOW, Slot::MeleeLow),
    (C::BACKHAND_LOW, Slot::MeleeLow),
    (C::DOUBLE_SLASH_LOW, Slot::MeleeLow),
    (C::TRIPLE_SLASH_LOW, Slot::MeleeLow),
    (C::DOUBLE_THRUST_LOW, Slot::MeleeLow),
    (C::TRIPLE_THRUST_LOW, Slot::MeleeLow),
    (C::PUNCH_FAST_LOW, Slot::MeleeLow),
    (C::PUNCH_SLOW_LOW, Slot::MeleeLow),
    (C::OFFHAND_SLASH_LOW, Slot::MeleeLow),
    (C::OFFHAND_THRUST_LOW, Slot::MeleeLow),
    (C::OFFHAND_DOUBLE_SLASH_LOW, Slot::MeleeLow),
    (C::OFFHAND_TRIPLE_SLASH_LOW, Slot::MeleeLow),
    (C::OFFHAND_DOUBLE_THRUST_LOW, Slot::MeleeLow),
    (C::OFFHAND_TRIPLE_THRUST_LOW, Slot::MeleeLow),
    (C::OFFHAND_PUNCH_FAST_LOW, Slot::MeleeLow),
    (C::OFFHAND_PUNCH_SLOW_LOW, Slot::MeleeLow),
    // Creatures' own attacks.
    (C::SPECIAL_ATTACK1, Slot::Special),
    (C::SPECIAL_ATTACK2, Slot::Special),
    (C::SPECIAL_ATTACK3, Slot::Special),
    (C::HEAD_THROW, Slot::Special),
    (C::FIST_SLAM, Slot::Special),
    (C::BREATHE_FLAME_, Slot::Special),
    (C::SPIN_ATTACK, Slot::Special),
    // Aiming, loading and shooting.
    (C::MISSILE_ATTACK1, Slot::Missile),
    (C::MISSILE_ATTACK2, Slot::Missile),
    (C::MISSILE_ATTACK3, Slot::Missile),
    (C::SHOOT, Slot::Missile),
    (C::RELOAD, Slot::Missile),
    (C::UNLOAD, Slot::Missile),
    (C::BOW_NO_AMMO, Slot::Missile),
    (C::CROSS_BOW_NO_AMMO, Slot::Missile),
    (C::AIM_LEVEL, Slot::Missile),
    (C::AIM_HIGH15, Slot::Missile),
    (C::AIM_HIGH30, Slot::Missile),
    (C::AIM_HIGH45, Slot::Missile),
    (C::AIM_HIGH60, Slot::Missile),
    (C::AIM_HIGH75, Slot::Missile),
    (C::AIM_HIGH90, Slot::Missile),
    (C::AIM_LOW15, Slot::Missile),
    (C::AIM_LOW30, Slot::Missile),
    (C::AIM_LOW45, Slot::Missile),
    (C::AIM_LOW60, Slot::Missile),
    (C::AIM_LOW75, Slot::Missile),
    (C::AIM_LOW90, Slot::Missile),
    // Casting and its gestures.
    (C::CAST_SPELL, Slot::Cast),
    (C::USE_MAGIC_STAFF, Slot::Cast),
    (C::USE_MAGIC_WAND, Slot::Cast),
    (C::MAGIC_BLAST, Slot::Cast),
    (C::MAGIC_SELF_HEAD, Slot::Cast),
    (C::MAGIC_SELF_HEART, Slot::Cast),
    (C::MAGIC_BONUS, Slot::Cast),
    (C::MAGIC_CLAP, Slot::Cast),
    (C::MAGIC_HARM, Slot::Cast),
    (C::MAGIC_HEAL, Slot::Cast),
    (C::MAGIC_THROW_MISSILE, Slot::Cast),
    (C::MAGIC_RECOIL_MISSILE, Slot::Cast),
    (C::MAGIC_PENALTY, Slot::Cast),
    (C::MAGIC_TRANSFER, Slot::Cast),
    (C::MAGIC_VISION, Slot::Cast),
    (C::MAGIC_ENCHANT_ITEM, Slot::Cast),
    (C::MAGIC_PORTAL, Slot::Cast),
    (C::MAGIC_PRAY, Slot::Cast),
    (C::MAGIC_POWER_UP01, Slot::Cast),
    (C::MAGIC_POWER_UP02, Slot::Cast),
    (C::MAGIC_POWER_UP03, Slot::Cast),
    (C::MAGIC_POWER_UP04, Slot::Cast),
    (C::MAGIC_POWER_UP05, Slot::Cast),
    (C::MAGIC_POWER_UP06, Slot::Cast),
    (C::MAGIC_POWER_UP07, Slot::Cast),
    (C::MAGIC_POWER_UP08, Slot::Cast),
    (C::MAGIC_POWER_UP09, Slot::Cast),
    (C::MAGIC_POWER_UP10, Slot::Cast),
    (C::MAGIC_POWER_UP01_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP02_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP03_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP04_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP05_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP06_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP07_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP08_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP09_PURPLE, Slot::Cast),
    (C::MAGIC_POWER_UP10_PURPLE, Slot::Cast),
    (C::AI_TELEGRAPH_CAST, Slot::Telegraph),
    // Struck.
    (C::TWITCH1, Slot::Hit),
    (C::TWITCH2, Slot::Hit),
    (C::TWITCH3, Slot::Hit),
    (C::TWITCH4, Slot::Hit),
    (C::STAGGER_BACKWARD, Slot::HitHeavy),
    (C::STAGGER_FORWARD, Slot::HitHeavy),
    (C::FALL_DOWN, Slot::HitHeavy),
    (C::TIPPED_LEFT, Slot::HitHeavy),
    (C::TIPPED_RIGHT, Slot::HitHeavy),
    // Dead, asleep, airborne.
    (C::DEAD, Slot::Dead),
    (C::FALLEN, Slot::Dead),
    (C::SLEEPING, Slot::Sleep),
    (C::FALLING, Slot::Fall),
    (C::JUMP, Slot::Jump),
    (C::JUMPUP, Slot::Jump),
    (C::HOP, Slot::Jump),
    // Gestures.
    (C::CHEER, Slot::Cheer),
    (C::CHEST_BEAT, Slot::Emote),
    (C::SCAN_HORIZON, Slot::Emote),
    (C::WARM_HANDS, Slot::Emote),
    (C::SHAKE_FIST, Slot::Emote),
    (C::BECKON, Slot::Emote),
    (C::BE_SEEING_YOU, Slot::Emote),
    (C::BLOW_KISS, Slot::Emote),
    (C::BOW_DEEP, Slot::Emote),
    (C::CLAP_HANDS, Slot::Emote),
    (C::CRY, Slot::Emote),
    (C::LAUGH, Slot::Emote),
    (C::MIME_EAT, Slot::Emote),
    (C::MIME_DRINK, Slot::Emote),
    (C::NOD, Slot::Emote),
    (C::POINT, Slot::Emote),
    (C::SHAKE_HEAD, Slot::Emote),
    (C::SHRUG, Slot::Emote),
    (C::WAVE, Slot::Emote),
    (C::AKIMBO, Slot::Emote),
    (C::HEARTY_LAUGH, Slot::Emote),
    (C::SALUTE, Slot::Emote),
    (C::SCRATCH_HEAD, Slot::Emote),
    (C::SMACK_HEAD, Slot::Emote),
    (C::TAP_FOOT, Slot::Emote),
    (C::WAVE_HIGH, Slot::Emote),
    (C::WAVE_LOW, Slot::Emote),
    (C::YAWN_STRETCH, Slot::Emote),
    (C::CRINGE, Slot::Emote),
    (C::KNEEL, Slot::Emote),
    (C::PLEAD, Slot::Emote),
    (C::SHIVER, Slot::Emote),
    (C::SHOO, Slot::Emote),
    (C::SLOUCH, Slot::Emote),
    (C::SPIT, Slot::Emote),
    (C::SURRENDER, Slot::Emote),
    (C::WOAH, Slot::Emote),
    (C::WINDED, Slot::Emote),
    (C::YMCA, Slot::Emote),
    (C::PRAY, Slot::Emote),
    (C::MOCK, Slot::Emote),
    (C::TEAPOT, Slot::Emote),
    (C::FLATULENCE, Slot::Emote),
    (C::DRUDGE_DANCE, Slot::Emote),
    (C::HAVE_A_SEAT, Slot::Emote),
    (C::POINT_LEFT, Slot::Emote),
    (C::POINT_RIGHT, Slot::Emote),
    (C::POINT_DOWN, Slot::Emote),
    (C::NUDGE_LEFT, Slot::Emote),
    (C::NUDGE_RIGHT, Slot::Emote),
    (C::KNOCK, Slot::Emote),
];

/// The idle of `style`: a melee, missile or magic stance's own, the peace idle for the rest.
#[must_use]
pub(crate) fn idle_of(style: C) -> Slot {
    match style {
        C::HAND_COMBAT
        | C::SWORD_COMBAT
        | C::SWORD_SHIELD_COMBAT
        | C::TWO_HANDED_SWORD_COMBAT
        | C::TWO_HANDED_STAFF_COMBAT
        | C::DUAL_WIELD_COMBAT => Slot::IdleMelee,
        C::BOW_COMBAT
        | C::CROSSBOW_COMBAT
        | C::SLING_COMBAT
        | C::THROWN_WEAPON_COMBAT
        | C::THROWN_SHIELD_COMBAT
        | C::ATLATL_COMBAT => Slot::IdleMissile,
        C::MAGIC => Slot::IdleMagic,
        _ => Slot::Idle,
    }
}

/// The slot `motion` fills in `style`: a change of stance, a twitch at peace (a fidget, not a
/// flinch), a row of [`SLOTS`]; a cycle no row names is the stance's idle, and an action no row
/// names is `None`, leaving what plays going on.
#[must_use]
pub(crate) fn slot_of(style: C, motion: C) -> Option<Slot> {
    if motion.is_style() {
        return Some(Slot::Stance);
    }
    let twitch = matches!(motion, C::TWITCH1 | C::TWITCH2 | C::TWITCH3 | C::TWITCH4);
    if twitch && style == C::NON_COMBAT {
        return Some(Slot::Emote);
    }
    if let Some((_, s)) = SLOTS.iter().find(|(c, _)| *c == motion) {
        return Some(*s);
    }
    (!motion.is_action()).then(|| idle_of(style))
}

/// What `driver`'s parts play this frame, the object placed `lift` meters above where it stands;
/// `None` while they play no animation. The motion under way is the first queued one whose
/// animations are playing (an attack, a change of stance, a death), else the cycle it stands in.
#[must_use]
pub(crate) fn motion_of(
    driver: &dereth_animation::driver::MotionDriver,
    lift: f32,
) -> Option<rm::Motion> {
    let seq = &driver.sequence;
    let ahead = driver.placed_ahead();
    let (curr, frame) = super::remaster_bridge::drawn_at(seq, ahead)?;
    let node = seq.nodes().get(curr)?;
    let settled = seq.first_cyclic().is_some_and(|c| curr >= c);
    #[allow(clippy::cast_possible_truncation)] // a share of an animation, 0 to 1
    let phase = (frame / f64::from(node.anim.num_frames.max(1))).clamp(0.0, 1.0) as f32;
    // Seconds left of this node, the hooks still to fire in it, then each node's before the cycle.
    let rate = f64::from(node.framerate);
    let left = if rate > 0.0 {
        (f64::from(node.high_frame) + 1.0 - frame) / rate
    } else if rate < 0.0 {
        (frame - f64::from(node.low_frame)) / -rate
    } else {
        0.0
    };
    let mut until_hit = None;
    let attack = |n: &dereth_animation::seq::AnimSequenceNode, i: i32| {
        n.get_part_frame(i).is_some_and(|f| {
            f.hooks
                .iter()
                .any(|h| matches!(h.kind, dereth_animation::hooks::HookKind::Attack(_)))
        })
    };
    if !settled && rate != 0.0 {
        // A frame's hooks fire as the player leaves it.
        let first = dereth_primitives::num::to_i32_f64(frame.floor());
        for (i, _, _) in node.fired_frames() {
            let ahead_of = if rate > 0.0 { i >= first } else { i <= first };
            if ahead_of && attack(node, i) {
                let to = if rate > 0.0 {
                    (f64::from(i) + 1.0 - frame) / rate
                } else {
                    (frame - f64::from(i)) / -rate
                };
                until_hit = Some(to.max(0.0));
                break;
            }
        }
    }
    let mut between = 0.0;
    if let Some(c) = seq.first_cyclic().filter(|_| !settled) {
        for n in seq.nodes().get(curr + 1..c).unwrap_or(&[]) {
            if until_hit.is_none() {
                if let Some((_, t, _)) = n.fired_frames().into_iter().find(|f| attack(n, f.0)) {
                    until_hit = Some(left + between + t);
                }
            }
            between += n.play_time();
        }
    }
    let until_cycle = if settled {
        0.0
    } else {
        left.max(0.0) + between
    };
    let manager = &driver.motion_table;
    let state = &manager.state;
    let under_way = manager.pending_head().filter(|_| !settled);
    let motion = under_way.unwrap_or(state.substate);
    let reversed = under_way.map_or(state.substate_mod < 0.0, |m| {
        m == state.substate && state.substate_mod < 0.0
    }) || matches!(motion, C::TURN_LEFT | C::SIDE_STEP_LEFT | C::WALK_BACKWARDS);
    let slot = match slot_of(state.style, motion) {
        // Dying while the parts fall; dead once they lie.
        Some(Slot::Dead) if motion == C::DEAD && !settled => Some(Slot::Die),
        Some(Slot::Walk) if reversed => Some(Slot::WalkBack),
        s => s,
    };
    // On the way to a stand, what the link's step carries the object (a run's stopping stride);
    // else what the cycle asks.
    let velocity = if !settled && slot.is_some_and(Slot::is_idle) {
        super::remaster_bridge::node_velocity(seq, curr, Some(frame))
    } else {
        super::remaster_bridge::cycle_velocity(seq)
    };
    #[allow(clippy::cast_possible_truncation)] // seconds, a few at most
    Some(rm::Motion {
        slot,
        idle: idle_of(state.style),
        reversed,
        settled,
        phase,
        until_cycle: until_cycle as f32,
        until_hit: until_hit.map(|t| t as f32),
        queued: u8::try_from(manager.pending_len()).unwrap_or(u8::MAX),
        keyframed: ahead.is_none(),
        frame: glam::Mat4::from_translation(glam::Vec3::new(0.0, 0.0, lift))
            * super::remaster_bridge::mat(&driver.particle_parent_frame())
            * glam::Mat4::from_scale(super::remaster_bridge::v3(driver.part_array.scale)),
        velocity,
    })
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental hi-fi art packs)
    use super::*;

    /// Every slot is one a pack can name, and every name a pack can use is a slot.
    #[test]
    fn the_slots_are_the_ones_the_pack_format_names() {
        let names: Vec<&str> = Slot::ALL.iter().map(|s| s.name()).collect();
        assert_eq!(names, dereth_assets::art_pack::CLIP_SLOTS);
        for s in Slot::ALL {
            assert_eq!(Slot::named(s.name()), Some(s));
        }
    }

    /// Each kind of motion lands in its slot: a stance's idle by the stance, swings by their
    /// height, a creature's own attack its special, aiming the missile slot, a spell's gesture the
    /// cast, a twitch a hit in a fight and a fidget at peace, a stagger a heavy hit, a change of
    /// stance the stance slot; a cycle with no slot its stance's idle, an action with none nothing.
    #[test]
    fn each_motion_creatures_play_lands_in_its_slot() {
        let fight = C::HAND_COMBAT;
        assert_eq!(slot_of(C::NON_COMBAT, C::READY), Some(Slot::Idle));
        assert_eq!(slot_of(fight, C::READY), Some(Slot::IdleMelee));
        assert_eq!(slot_of(C::BOW_COMBAT, C::READY), Some(Slot::IdleMissile));
        assert_eq!(slot_of(C::MAGIC, C::READY), Some(Slot::IdleMagic));
        assert_eq!(slot_of(fight, C::RUN_FORWARD), Some(Slot::Run));
        assert_eq!(slot_of(fight, C::WALK_BACKWARDS), Some(Slot::WalkBack));
        assert_eq!(slot_of(fight, C::SIDE_STEP_LEFT), Some(Slot::SideStep));
        assert_eq!(slot_of(fight, C::TURN_LEFT), Some(Slot::Turn));
        assert_eq!(slot_of(fight, C::ATTACK_HIGH2), Some(Slot::MeleeHigh));
        assert_eq!(slot_of(fight, C::SLASH_MED), Some(Slot::MeleeMed));
        assert_eq!(
            slot_of(fight, C::OFFHAND_PUNCH_SLOW_LOW),
            Some(Slot::MeleeLow)
        );
        assert_eq!(slot_of(fight, C::SPECIAL_ATTACK1), Some(Slot::Special));
        assert_eq!(slot_of(C::BOW_COMBAT, C::AIM_HIGH45), Some(Slot::Missile));
        assert_eq!(slot_of(C::BOW_COMBAT, C::RELOAD), Some(Slot::Missile));
        assert_eq!(slot_of(C::MAGIC, C::CAST_SPELL), Some(Slot::Cast));
        assert_eq!(slot_of(C::MAGIC, C::MAGIC_HEAL), Some(Slot::Cast));
        assert_eq!(slot_of(fight, C::AI_TELEGRAPH_CAST), Some(Slot::Telegraph));
        assert_eq!(slot_of(fight, C::TWITCH3), Some(Slot::Hit));
        assert_eq!(slot_of(C::NON_COMBAT, C::TWITCH3), Some(Slot::Emote));
        assert_eq!(slot_of(fight, C::STAGGER_FORWARD), Some(Slot::HitHeavy));
        assert_eq!(slot_of(fight, C::DEAD), Some(Slot::Dead));
        assert_eq!(slot_of(fight, C::SLEEPING), Some(Slot::Sleep));
        assert_eq!(slot_of(fight, C::FALLING), Some(Slot::Fall));
        assert_eq!(slot_of(C::NON_COMBAT, C::CHEER), Some(Slot::Cheer));
        assert_eq!(slot_of(C::NON_COMBAT, C::WAVE), Some(Slot::Emote));
        assert_eq!(slot_of(C::NON_COMBAT, C::HAND_COMBAT), Some(Slot::Stance));
        assert_eq!(slot_of(fight, C::SITTING), Some(Slot::IdleMelee));
        assert_eq!(slot_of(fight, C::ENTER_PORTAL), None);
    }
}
