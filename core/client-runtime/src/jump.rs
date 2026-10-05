//! Combat input jump callbacks. The input owner supplies the actual Character and sends
//! commands at this boundary; this module neither advances a frame nor infers physics success.

use crate::character::Character;
use dereth_client_model::combat::{CombatState, PowerBarMode};
use dereth_primitives::LocalTime;

/// The PlayerDesc owner and DAT tables supply live inquiries, never a second qualities copy.
/// Jump eligibility and velocity inquiries reject a missing player description.
pub fn refresh_qualities(
    character: &Character,
    qualities: Option<&dereth_client_model::qualities::Qualities>,
    skills: Option<&dereth_assets::tables::SkillTable>,
    filter: Option<&dereth_assets::tables::QualityFilter>,
    rules: &dereth_primitives::WorldRules,
) {
    character.set_jump_scale(rules.jump_scale);
    let Some(qualities) = qualities else {
        character.set_jump_qualities(0.0, false, None);
        return;
    };
    let load = dereth_rules::burden::inq_load_in(qualities, rules);
    let jump =
        skills.and_then(|table| dereth_rules::skills::inq_jump_skill(qualities, table, filter));
    character.set_jump_qualities(load, load < 2.0, jump);
}

/// Finishing a jump stores the body value unconditionally before clearing the pending flag.
pub fn finish(combat: &mut CombatState, character: Option<&Character>) {
    combat.finish_jump_with_body(|| {
        if let Some(character) = character {
            character.finish_jump();
        }
    });
}

/// Begin a jump. True means its final movement event must be sent now.
/// CancelAttack is sent before the ordered old-mode zero / Jump Begin / Start notices.
pub fn commence(
    character: &mut Character,
    game: &mut dereth_client_model::World,
    now: LocalTime,
    mut cancel_attack: impl FnMut(),
) -> bool {
    if game.combat.jump_pending {
        return false;
    }
    let status = character.charge_jump();
    if status != 0 {
        feedback(game, status, true);
        return false;
    }
    if game.player_system.options.auto_repeat_attack() {
        cancel_attack();
        game.combat.repeat_attacking = false;
    }
    game.combat.set_power_bar_level(0.0);
    game.combat.jump_pending = true;
    game.combat.begin_power_bar(PowerBarMode::Jump, false, 0);
    game.combat.start_power_bar_build(now);
    true
}

/// The explicit result read by DoJump before any physics tick. Velocity is local, not global.
#[derive(Debug, Clone, Copy)]
pub struct JumpResult {
    pub status: u32,
    pub extent: f32,
    pub local_velocity: dereth_protocol::types::Vec3,
}

/// Attempt an autonomous jump. No pending charge means no attempt and no packet.
pub fn release(
    character: &mut Character,
    game: &mut dereth_client_model::World,
    now: LocalTime,
) -> Option<JumpResult> {
    if !game.combat.jump_pending {
        return None;
    }
    game.combat.current_style = character
        .driver()
        .movement
        .interp
        .interpreted_state
        .current_style
        .0;
    let extent = game.combat.jump_power_level(now);
    finish(&mut game.combat, Some(character));
    let status = character.jump_with_extent(extent);
    // get_local_physics_velocity reads the integrated velocity, not get_velocity's cached achieved
    // displacement. The latter can still be zero immediately after SetLocalVelocity.
    let global_velocity = character
        .world
        .get(character.handle)
        .map_or(dereth_primitives::Vec3::ZERO, |body| body.velocity_vector);
    let velocity = dereth_physics::math::globaltolocalvec(
        dereth_physics::math::l2g(character.position().frame.rotation),
        global_velocity,
    );
    feedback(game, status, false);
    Some(JumpResult {
        status,
        extent,
        local_velocity: dereth_protocol::types::Vec3 {
            x: velocity.x,
            y: velocity.y,
            z: velocity.z,
        },
    })
}

/// Static jump-result strings; source channel `0x1A`.
fn feedback(game: &mut dereth_client_model::World, status: u32, charge: bool) {
    let text = match status {
        0 => return,
        0x48 => "You can't jump from this position",
        0x49 => "You're too loaded down to jump",
        0x24 => "You can't jump while in the air",
        _ if charge => "You can't jump while in the air",
        _ => return,
    };
    game.scroll
        .on_display_string_info(dereth_client_model::scroll::LOCAL_ERROR_TYPE, text);
}
