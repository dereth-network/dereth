//! A player's progress: experience it is awarded and the levels it reaches.
//!
//! **An award** sends, in one tick: for a part of a kill, "You have gained %1 experience points!"
//! (to a hero whose part is worth focus, "... and %2 Focus points!", or "You have gained %2 Focus
//! points!" for a part worth no experience, then its new focus: see [`crate::focus`]; a fellow's
//! share of a part comes without a text); then the total and the unspent experience; the
//! level-up, when the total reaches a new level; for a quest, "For completing the %1 quest, you
//! have earned %2 experience points." An award of nothing sends nothing (2017: 436 of 441 kills
//! worth nothing sent no totals). A kill's parts are followed by "You have killed the %1." to its
//! killer (2017 played the kill's private effect in only 451 of 4,040 solo awards; this server
//! does not).
//!
//! **Levels** go by the total experience, to level 50 unless the character is a hero; past that a
//! character's experience goes on adding up without a level, and brings the levels it has earned
//! at its next award as a hero.
//!
//! **A level-up** sends the level, the skill repository with a credit more (and from level 45 the
//! level's hero skill credits), the greatest health
//! and vigor of the level, "Congratulations! You are now level %1..." (the unspent experience, the
//! health and vigor gained, the level, the credits), health and vigor made whole (2017 sent the
//! maxima before the text in 41 of 70 level-ups, after it in 29), and to every client that sees the
//! player its level-up effect and the end of its wounded effect.

use auberean_protocol::physics::{Fx, ObjectEvent, Word};
use auberean_protocol::quality::PrivateQuality;
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::{queue, ServerMessage};
use auberean_script::native_object::{TextInfo, Variable, VariableValue};
use yalain_net::{BlobKind, ConnId, ServerNet};
use yalain_store::Store;

use crate::player::quality;
use crate::{quals, Server};

/// The qualities of experience and level.
pub mod q {
    pub const TOTAL_XP: u32 = 300;
    pub const AVAILABLE_XP: u32 = 301;
    pub const LEVEL: u32 = 302;
}

/// Texts of progress: (string id, table).
pub mod text {
    pub const GAINED: (u32, u32) = (0xCBE5_99B5, 0x2500_000B);
    /// "You have gained %1 experience points and %2 Focus points!"
    pub const GAINED_FOCUS: (u32, u32) = (0x708C_43BB, 0x2500_000B);
    /// "You have gained %2 Focus points!" (a part worth focus and no experience)
    pub const FOCUS_ONLY: (u32, u32) = (0x721B_A709, 0x2500_000B);
    pub const KILLED: (u32, u32) = (0xACAD_8D2A, 0x2500_0015);
    pub const LEVEL_UP: (u32, u32) = (0xECCF_2C87, 0x2500_000B);
    pub const QUEST_XP: (u32, u32) = (0xDAA1_17F4, 0x2500_07FA);
    /// "You receive no experience, since that wasn't a fair fight. Your opponent couldn't even
    /// reach you!"
    pub const UNFAIR: (u32, u32) = (0x75D8_7E7A, 0x2500_000B);
}

/// Variables of those texts.
pub mod var {
    /// An amount of experience.
    pub const XP: u32 = 0xB140_0000;
    /// An amount of focus (the gained texts' second variable).
    pub const FOCUS: u32 = 0xED3E_1D5A;
    /// What was killed (a string reference).
    pub const VICTIM: u32 = 0x9E66_1D49;
    pub const HEALTH_GAINED: u32 = 0x0316_0CCA;
    pub const NEW_LEVEL: u32 = 0x6025_52CC;
    pub const VIGOR_GAINED: u32 = 0xED26_3CFA;
    pub const CREDITS: u32 = 0xB263_AC49;
    /// The quest's name (a string reference).
    pub const QUEST: u32 = 0x2356_2D3A;
}

/// The level-up and wounded effects.
pub const LEVEL_UP_FX: u32 = 0x4100_0001;
pub const WOUNDED_FX: u32 = 0x4100_0002;

/// The kind of chat text progress is shown as.
pub const TEXT_KIND: u32 = 8;

/// The skill credits each level from the second grants (the level table's), when the table is
/// not loaded.
pub const CREDITS_PER_LEVEL: i32 = 1;

/// Why experience is awarded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Award {
    /// A damager's part of a kill, paid to it (or to its pet's owner).
    Part,
    /// A quest completed, by its name (its text sent even when it pays nothing).
    Quest((u32, u32)),
    /// A step of a quest that pays on its way (no text: 2017 sent the totals alone).
    Step,
    /// A fellow's share of a part of a kill (no text).
    Share,
    /// What vassals earned for their patron (its text is the allegiance's).
    Allegiance,
}

/// A string reference.
#[must_use]
pub fn text(id: (u32, u32)) -> TextInfo {
    TextInfo {
        string_id: id.0,
        table: id.1,
        ..TextInfo::default()
    }
}

/// A string reference with variables set.
#[must_use]
pub fn text_with(id: (u32, u32), vars: Vec<Variable>) -> TextInfo {
    let mut t = text(id);
    for v in vars {
        t.set_variable(v);
    }
    t
}

/// A variable holding a word, of a kind.
#[must_use]
pub const fn word(id: u32, kind: u16, v: u32) -> Variable {
    Variable {
        id,
        kind,
        value: VariableValue::Word(v),
    }
}

/// A variable holding a 64-bit number, of a kind.
#[must_use]
pub const fn long(id: u32, kind: u16, v: u64) -> Variable {
    Variable {
        id,
        kind,
        value: VariableValue::Long(v),
    }
}

/// A variable holding another string reference.
#[must_use]
pub fn nested(id: u32, t: TextInfo) -> Variable {
    Variable {
        id,
        kind: 9,
        value: VariableValue::Nested(Box::new(t)),
    }
}

/// Send the player a private message.
pub fn private(net: &mut ServerNet, world: ConnId, m: &ServerMessage) {
    net.send(world, queue::EVENT, BlobKind::PRIVATE, m.encode());
}

/// Send the player a whole-number quality.
pub fn send_int(net: &mut ServerNet, world: ConnId, key: u32, value: i32) {
    private(
        net,
        world,
        &ServerMessage::UpdateInt(PrivateQuality {
            quality: key,
            value,
        }),
    );
}

/// Send the player a floating-point quality.
pub fn send_float(net: &mut ServerNet, world: ConnId, key: u32, value: f32) {
    private(
        net,
        world,
        &ServerMessage::UpdateFloat(PrivateQuality {
            quality: key,
            value,
        }),
    );
}

/// Send the player a long whole-number quality.
pub fn send_long(net: &mut ServerNet, world: ConnId, key: u32, value: u64) {
    private(
        net,
        world,
        &ServerMessage::UpdateLongInt(PrivateQuality {
            quality: key,
            value,
        }),
    );
}

/// Show the player a progress text.
pub fn say(net: &mut ServerNet, world: ConnId, t: TextInfo) {
    private(net, world, &crate::chat::show_text(TEXT_KIND, t));
}

/// A part of a kill that was no fair fight, told to the player it was for: it pays no experience.
pub fn unfair(net: &mut ServerNet, world: ConnId) {
    say(net, world, text(text::UNFAIR));
}

/// "You have killed the %1.", to a kill's killer after its parts.
pub fn killed(net: &mut ServerNet, world: ConnId, name: Option<(u32, u32)>) {
    let victim = name.map_or_else(TextInfo::default, text);
    say(
        net,
        world,
        text_with(text::KILLED, vec![nested(var::VICTIM, victim)]),
    );
}

/// Award a player experience.
pub fn award<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    amount: u64,
    why: Award,
) {
    award_with_focus(s, net, world, amount, 0, why);
}

/// Award a player experience and focus (a part of a kill, or a fellow's share of one). A part's
/// text names both: to a hero, "You have gained %1 experience points and %2 Focus points!" (or
/// "You have gained %2 Focus points!" for a part worth no experience), its new focus sent next; to
/// anyone else the plain text, the focus named in it and not gained.
pub fn award_with_focus<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    amount: u64,
    focus: i32,
    why: Award,
) {
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let gains_focus = focus > 0 && crate::focus::is_hero(&p.qualities);
    if why == Award::Part && (amount > 0 || gains_focus) {
        let t = match (gains_focus, amount > 0) {
            (false, _) => text::GAINED,
            (true, true) => text::GAINED_FOCUS,
            (true, false) => text::FOCUS_ONLY,
        };
        say(
            net,
            world,
            text_with(
                t,
                vec![
                    long(var::XP, 10, amount),
                    word(var::FOCUS, 2, focus.max(0).cast_unsigned()),
                ],
            ),
        );
    }
    if gains_focus {
        crate::focus::gain(s, net, world, focus);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let character = p.character;
    pay_vitae(s.vitae_costs.as_slice(), p, net, world, amount);
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let total = quals::long(&p.qualities, q::TOTAL_XP).unwrap_or(0) + amount;
    let available = quals::long(&p.qualities, q::AVAILABLE_XP).unwrap_or(0) + amount;
    quals::set_long(&mut p.qualities, q::TOTAL_XP, total);
    quals::set_long(&mut p.qualities, q::AVAILABLE_XP, available);
    // An untraining under way may be finished by this award, before the award's own totals.
    if p.untraining.is_some() {
        crate::skills::earned(s, net, world, total);
    }
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let available = quals::long(&p.qualities, q::AVAILABLE_XP).unwrap_or(available);
    // An award of nothing sends no totals.
    if amount > 0 {
        send_long(net, world, q::TOTAL_XP, total);
        send_long(net, world, q::AVAILABLE_XP, available);
    }
    let was = quals::int(&p.qualities, q::LEVEL).unwrap_or(1);
    let hero = crate::focus::is_hero(&p.qualities);
    let now = s.levels.reached(total, hero, was);
    if now > was {
        level_up(s, net, world, was, now);
    }
    if amount > 0 && matches!(why, Award::Part | Award::Share) {
        crate::allegiance::earned(s, net, character, amount);
    }
    if now > was {
        crate::allegiance::level_reached(s, net, character);
    }
    match why {
        Award::Part | Award::Step | Award::Share | Award::Allegiance => {}
        Award::Quest(name) => say(
            net,
            world,
            text_with(
                text::QUEST_XP,
                vec![long(var::XP, 12, amount), nested(var::QUEST, text(name))],
            ),
        ),
    }
}

/// The vitae report: "You are at %1 vitae." with what is still to earn (its text and table, and
/// the second variable's, as 2017 sent them).
pub const VITAE_REPORT: (u32, u32) = (0x39A4_E695, 0x2500_0015);
pub const VITAE_TO_EARN: (u32, u32) = (0xDBEC_DE15, 0x2500_0015);
pub mod vitae_var {
    pub const VITAE: u32 = 0x5B4C_DC80;
    pub const TO_EARN: u32 = 0xAB62_A528;
    pub const XP: u32 = 0x2B3E_2C48;
    pub const ONE: u32 = 0x4B7C_74C0;
}

/// Earned experience pays a player's vitae back (the library's rule, held to 1,827 of 1,922 2017
/// awards): while the award covers what is still to earn, that is paid and vitae rises by one
/// (sent), and what is to earn is worked out again for the new vitae (sent when it changed); the
/// rest is taken off what is to earn. A rise is reported. The award itself is not reduced.
fn pay_vitae(
    costs: &[u64],
    p: &mut crate::world::Player,
    net: &mut ServerNet,
    world: ConnId,
    amount: u64,
) {
    use crate::death::{vitae_q, xp_to_raise};
    let Some(mut vitae) = quals::float(&p.qualities, vitae_q::VITAE) else {
        return;
    };
    if vitae >= 100.0 || amount == 0 {
        return;
    }
    let level = crate::examine::level_of(&p.qualities);
    let mut debt = quals::long(&p.qualities, vitae_q::XP_TO_RAISE)
        .or_else(|| xp_to_raise(costs, level, vitae))
        .unwrap_or(1);
    let mut left = amount;
    let mut rose = false;
    while vitae < 100.0 && left >= debt {
        left -= debt;
        vitae = (vitae + 1.0).min(100.0);
        rose = true;
        quals::set_float(&mut p.qualities, vitae_q::VITAE, vitae);
        send_float(net, world, vitae_q::VITAE, vitae);
        match xp_to_raise(costs, level, vitae) {
            Some(next) => {
                if next != debt {
                    send_long(net, world, vitae_q::XP_TO_RAISE, next);
                }
                debt = next;
            }
            None => debt = 0,
        }
    }
    if vitae < 100.0 {
        debt -= left;
        send_long(net, world, vitae_q::XP_TO_RAISE, debt);
    }
    quals::set_long(&mut p.qualities, vitae_q::XP_TO_RAISE, debt);
    if rose {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let whole = vitae as u32;
        let to_earn = if vitae < 100.0 {
            text_with(
                VITAE_TO_EARN,
                vec![long(vitae_var::XP, 12, debt), word(vitae_var::ONE, 2, 1)],
            )
        } else {
            TextInfo {
                has_literal: 1,
                literal: Some(Vec::new()),
                ..TextInfo::default()
            }
        };
        say(
            net,
            world,
            text_with(
                VITAE_REPORT,
                vec![
                    word(vitae_var::VITAE, 2, whole),
                    nested(vitae_var::TO_EARN, to_earn),
                ],
            ),
        );
    }
}

/// A player reaches a new level.
pub fn level_up<S: Store>(
    s: &mut Server<S>,
    net: &mut ServerNet,
    world: ConnId,
    was: i32,
    now: i32,
) {
    let (old, new) = (s.levels.level(was).copied(), s.levels.level(now).copied());
    let gained_credits: i32 = (was + 1..=now)
        .map(|l| {
            s.levels
                .level(l)
                .map_or(CREDITS_PER_LEVEL, |x| x.skill_credits)
        })
        .sum();
    // From level 45 each level brings hero skill credits too, sent in the same repository (2017:
    // 3 of 3 rises of 100 came with a level reached, 48 to 50; every session start at level 45 or
    // above, 151 of 151, had some, none below).
    let gained_hero_credits: i32 = (was + 1..=now)
        .filter_map(|l| s.levels.level(l).map(|x| x.hero_credits))
        .sum();
    let Some(p) = s.players.get_mut(&world) else {
        return;
    };
    let old_health = quals::int(&p.qualities, quality::HEALTH_MAX).unwrap_or(100);
    let old_vigor = quals::int(&p.qualities, quality::VIGOR_MAX).unwrap_or(100);
    let health = new.map_or(old_health, |l| l.health);
    let vigor = new.map_or(old_vigor, |l| l.vigor);
    let _ = old;
    quals::set_int(&mut p.qualities, q::LEVEL, now);
    send_int(net, world, q::LEVEL, now);
    p.play.credits = p
        .play
        .credits
        .saturating_add(u32::try_from(gained_credits.max(0)).unwrap_or(0));
    p.play.hero_credits = p
        .play
        .hero_credits
        .saturating_add(u32::try_from(gained_hero_credits.max(0)).unwrap_or(0));
    private(
        net,
        world,
        &crate::skills::repository_update(p.play.credits, p.play.hero_credits),
    );
    send_int(net, world, quality::HEALTH_MAX, health);
    send_int(net, world, quality::VIGOR_MAX, vigor);
    let available = quals::long(&p.qualities, q::AVAILABLE_XP).unwrap_or(0);
    say(
        net,
        world,
        text_with(
            text::LEVEL_UP,
            vec![
                long(var::XP, 12, available),
                word(
                    var::HEALTH_GAINED,
                    2,
                    (health - old_health).max(0).cast_unsigned(),
                ),
                word(var::NEW_LEVEL, 2, now.cast_unsigned()),
                word(
                    var::VIGOR_GAINED,
                    2,
                    (vigor - old_vigor).max(0).cast_unsigned(),
                ),
                word(var::CREDITS, 6, gained_credits.max(0).cast_unsigned()),
            ],
        ),
    );
    quals::set_int(&mut p.qualities, quality::HEALTH_MAX, health);
    quals::set_int(&mut p.qualities, quality::VIGOR_MAX, vigor);
    quals::set_int(&mut p.qualities, quality::HEALTH, health);
    quals::set_int(&mut p.qualities, quality::VIGOR, vigor);
    send_int(net, world, quality::HEALTH, health);
    send_int(net, world, quality::VIGOR, vigor);
    p.visual = p.visual.wrapping_add(1);
    let fx = ServerMessage::DoFx(ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action: Fx {
            fx: LEVEL_UP_FX,
            scalar: 1.0,
        },
    });
    p.visual = p.visual.wrapping_add(1);
    let stop = ServerMessage::StopFx(ObjectEvent {
        sender: ObjectStamp {
            other_stamp: p.visual,
            ..p.stamp()
        },
        action: Word(WOUNDED_FX),
    });
    let id = p.character;
    crate::combat::to_all_seeing(s, net, id, &fx);
    crate::combat::to_all_seeing(s, net, id, &stop);
}
