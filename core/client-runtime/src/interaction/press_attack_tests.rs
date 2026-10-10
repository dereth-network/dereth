//! The attack keys and the attack height buttons where a press is the whole attack (the Horizon
//! interface), beside the game's own controls, which are held to charge.
//!
//! Each frame runs as the client runs it: the frame's actions or button requests, then the power
//! bar. The shard is played by hand: its commence and its attack-done answers, as it sends them
//! while it repeats an attack for a player whose Repeat Attacks option is on, and, when the option
//! is off and it repeats nothing, the action-cancelled attack-done that ends each attack.
//!
//! Behaviour: none (experimental Horizon interface); the game's own controls' tests name their rows.

use super::*;
use dereth_client_contract::actions::{Action, ActionId};
use dereth_client_model::combat::{
    CombatMode, ATTACK_SEQUENCE_ENDED, COMBAT_TABLE_DID, MISSILE_READY_STYLES, MOTION_READY,
};
use dereth_client_model::player::options::option;

const PLAYER: ObjectId = ObjectId(0x5000_0001);
const MONSTER: ObjectId = ObjectId(0x8000_0042);
/// The combat table every character is born carrying, which melee's readiness reads.
const COMBAT_TABLE: u32 = 0x3000_0021;
/// The game's walk-forward command: a body walking is not ready to shoot.
const WALK_FORWARD: u32 = 0x4500_0005;
/// One frame at thirty frames a second.
const FRAME: f64 = 1.0 / 30.0;

/// What one attack request carried.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Sent {
    height: u32,
    power: f32,
}

/// A player in `mode`, with a bow drawn or a sword ready, an attackable creature selected, and
/// Repeat Attacks on; the interaction attacking on a press when `press`, as Horizon has it.
struct Bench {
    inter: Interaction,
    game: dereth_client_model::World,
    now: f64,
    sent: Vec<Request>,
}

impl Bench {
    fn new(mode: CombatMode, press: bool) -> Self {
        let mut game = dereth_client_model::World::new();
        game.player = Some(PLAYER);
        let mut me = dereth_client_model::Weenie::new(PLAYER);
        me.pwd.name = "Aldis".into();
        me.qualities
            .get_or_insert_with(dereth_client_model::Qualities::new)
            .set(
                dereth_client_model::StatKey::new(
                    dereth_client_model::StatType::Did,
                    COMBAT_TABLE_DID,
                ),
                dereth_client_model::StatValue::Did(dereth_primitives::DataId(COMBAT_TABLE)),
            );
        game.tables.weenies.insert(PLAYER, me);
        let mut monster = dereth_client_model::Weenie::new(MONSTER);
        monster.pwd.name = "Drudge".into();
        monster.pwd.obj_type = dereth_rules::weenie::item_type::CREATURE;
        monster.pwd.bitfield |= dereth_rules::weenie::bitfield::ATTACKABLE;
        game.tables.weenies.insert(MONSTER, monster);
        game.set_selected_object(Some(MONSTER), false, &mut dereth_client_model::NullSink);
        game.player_system
            .options
            .set(option::AUTO_REPEAT_ATTACK, true);
        game.combat.combat_mode = mode;
        if mode == CombatMode::Missile {
            game.combat.current_style = MISSILE_READY_STYLES[0];
        }
        game.combat.forward_command = MOTION_READY;
        let mut inter = Interaction::new();
        inter.note_press_attacks(press);
        game.refuse_advanced_combat(press);
        game.always_repeat_attacks(press);
        Self {
            inter,
            game,
            now: 10.0,
            sent: Vec::new(),
        }
    }

    /// The player's advanced combat interface option, and a mode change, which is when the game
    /// reads it.
    fn advanced_option(&mut self, on: bool) {
        self.game
            .player_system
            .options
            .set(option::ADVANCED_COMBAT_UI, on);
        let mode = self.game.combat.combat_mode;
        let mut req = RecordingRequests::default();
        let _ = self.game.set_combat_mode(
            &mut req,
            &mut dereth_client_model::NullSink,
            CombatMode::NonCombat,
            false,
            true,
            false,
        );
        let _ = self.game.set_combat_mode(
            &mut req,
            &mut dereth_client_model::NullSink,
            mode,
            false,
            true,
            false,
        );
    }

    fn ready(&self) -> bool {
        self.inter.ready_for_attack(&self.game)
    }

    /// One frame with `actions`, then the power bar.
    fn frame(&mut self, actions: Vec<Action>) {
        self.now += FRAME;
        let now = dereth_primitives::LocalTime(self.now);
        let ready = self.ready();
        let phys = crate::selection_geometry::SceneSelectionPhysics::default();
        let left = self
            .inter
            .on_actions(actions, &mut self.game, &phys, 0.0, ready, now, None);
        assert!(left.is_empty(), "every action here is the combat system's");
        let ready = self.ready();
        self.inter.run_power_bar(&mut self.game, ready, now);
        self.sent.extend(self.inter.take_pending_requests());
    }

    /// `seconds` of frames with nothing pressed.
    fn wait(&mut self, seconds: f64) {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let n = (seconds / FRAME).round() as usize;
        for _ in 0..n {
            self.frame(Vec::new());
        }
    }

    /// One frame in which an attack height button asks for `height`.
    fn button(&mut self, height: AttackHeight) {
        self.inter.queue(
            Vec::new(),
            vec![UiRequest::CombatSetAttackHeight {
                height: height as u32,
            }],
        );
        self.now += FRAME;
        let ready = self.ready();
        assert!(self
            .inter
            .run_ui_requests(&mut self.game, false, ServerTime(self.now))
            .is_empty());
        self.inter.run_power_bar(
            &mut self.game,
            ready,
            dereth_primitives::LocalTime(self.now),
        );
        self.sent.extend(self.inter.take_pending_requests());
    }

    /// The shard swings: its commence.
    fn shard_swings(&mut self) {
        self.game.handle_commence_attack();
    }

    /// The shard's attack-done answer, `result` 0 for a clean finish.
    fn shard_done(&mut self, result: u32) {
        let mut req = RecordingRequests::default();
        let ready = self.ready();
        self.game.handle_attack_done(
            &mut req,
            result,
            ready,
            dereth_primitives::LocalTime(self.now),
        );
        self.sent.extend(req.0);
    }

    /// The attacks sent since the last look, and the cancels.
    fn take(&mut self) -> (Vec<Sent>, usize) {
        let mut attacks = Vec::new();
        let mut cancels = 0;
        for r in self.sent.drain(..) {
            match r {
                Request::TargetedMissileAttack(a) => {
                    assert_eq!(a.target, MONSTER);
                    attacks.push(Sent {
                        height: a.attack_height,
                        power: a.power_level,
                    });
                }
                Request::TargetedMeleeAttack(a) => {
                    assert_eq!(a.target, MONSTER);
                    attacks.push(Sent {
                        height: a.attack_height,
                        power: a.power_level,
                    });
                }
                Request::CancelAttack(_) => cancels += 1,
                _ => {}
            }
        }
        (attacks, cancels)
    }

    /// The player's Repeat Attacks option, which the shard reads to decide whether it repeats an
    /// attack.
    fn repeat_option(&mut self, on: bool) {
        self.game
            .player_system
            .options
            .set(option::AUTO_REPEAT_ATTACK, on);
    }

    /// Under Horizon with Repeat Attacks off: one press, the attack it sends, and the shard's
    /// answer ending it, after which the client's own next attack is charging.
    fn repeating_itself(mode: CombatMode) -> Self {
        let mut b = Self::new(mode, true);
        b.repeat_option(false);
        b.frame(vec![Action::begin(medium_key(mode))]);
        b.wait(0.6);
        b.shard_done(ATTACK_SEQUENCE_ENDED);
        let (attacks, cancels) = b.take();
        assert_eq!((attacks.len(), cancels), (1, 0), "the loop is under way");
        assert!(b.game.combat.repeat_attacking);
        b
    }

    /// One press, the attack it sends, and the shard taking it up and repeating it once: the loop
    /// under way, the bar refilling by itself.
    fn repeating(mode: CombatMode) -> Self {
        let mut b = Self::new(mode, true);
        b.frame(vec![Action::begin(action::COMBAT_AIM_MEDIUM)]);
        b.wait(0.6);
        b.shard_swings();
        b.shard_done(0);
        let (attacks, cancels) = b.take();
        assert_eq!((attacks.len(), cancels), (1, 0), "the loop is under way");
        assert!(b.game.combat.repeat_attacking);
        b
    }
}

fn medium(power: f32) -> Sent {
    Sent {
        height: AttackHeight::Medium as u32,
        power,
    }
}

/// The game's key for an attack at medium height in `mode`.
fn medium_key(mode: CombatMode) -> ActionId {
    if mode == CombatMode::Missile {
        action::COMBAT_AIM_MEDIUM
    } else {
        action::COMBAT_MEDIUM_ATTACK
    }
}

#[test]
fn under_horizon_one_press_fires_at_the_power_aimed_at_and_the_shard_repeats_it_with_a_bow_or_a_sword(
) {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, true);
        let key = medium_key(mode);
        b.frame(vec![Action::begin(key)]);
        // The key stays down: the bar charges to the power aimed at, a half, and no further.
        let mut first = Vec::new();
        for i in 0..90 {
            b.frame(vec![Action::repeat(key, 1)]);
            let (attacks, cancels) = b.take();
            assert_eq!(cancels, 0);
            if !attacks.is_empty() {
                first.push((i, attacks));
            }
        }
        assert_eq!(
            first.len(),
            1,
            "{mode:?}: one attack, held or not: {first:?}"
        );
        let (at, attacks) = &first[0];
        assert_eq!(
            attacks,
            &vec![medium(0.5)],
            "{mode:?}: at the power aimed at"
        );
        assert!(
            (13..=16).contains(at),
            "{mode:?}: when the bar gets there, half a second on: frame {at}"
        );
        // The shard repeats it, three times, and nothing more is asked of it, the key held or let go.
        for round in 0..3 {
            b.shard_swings();
            b.shard_done(0);
            assert!(
                b.game.combat.build_in_progress,
                "{mode:?}: the bar refills by itself"
            );
            for _ in 0..40 {
                b.frame(vec![Action::repeat(key, 1)]);
            }
            if round == 1 {
                b.frame(vec![Action::end(key)]);
            }
            assert!(b.game.combat.repeat_attacking, "{mode:?}: still repeating");
            assert_eq!(b.take(), (vec![], 0), "{mode:?}: round {round}");
        }
    }
}

#[test]
fn under_horizon_a_height_buttons_press_is_the_whole_attack_too() {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, true);
        b.button(AttackHeight::High);
        b.wait(0.6);
        assert_eq!(
            b.take(),
            (
                vec![Sent {
                    height: AttackHeight::High as u32,
                    power: 0.5
                }],
                0
            ),
            "{mode:?}"
        );
        assert!(!b.game.combat.attack_request_in_progress);
    }
}

#[test]
fn under_horizon_another_height_pressed_while_the_shard_repeats_goes_with_the_next_attack() {
    let mut b = Bench::repeating(CombatMode::Missile);
    // Between two of the shard's attacks: it goes at once, at the new height, for the shard to
    // take up on its next attack.
    b.frame(vec![Action::begin(action::COMBAT_AIM_HIGH)]);
    let (attacks, cancels) = b.take();
    assert_eq!(cancels, 0);
    assert_eq!(
        attacks.iter().map(|s| s.height).collect::<Vec<_>>(),
        vec![AttackHeight::High as u32; attacks.len()],
        "{attacks:?}"
    );
    assert!(!attacks.is_empty());
    // While an attack is out: it waits for the shard's answer, then goes at the new height.
    b.shard_swings();
    b.frame(vec![Action::begin(action::COMBAT_AIM_LOW)]);
    assert_eq!(b.take(), (vec![], 0), "nothing while the attack is out");
    b.shard_done(0);
    let (attacks, _) = b.take();
    assert_eq!(
        attacks.first().map(|s| s.height),
        Some(AttackHeight::Low as u32),
        "{attacks:?}"
    );
    assert_eq!(b.game.combat.requested_attack_height, AttackHeight::Low);
}

#[test]
fn under_horizon_the_repeat_stops_for_escape_a_lost_target_a_step_moving_and_a_refusal() {
    let stops = |what: &str, mode: CombatMode, interrupt: &dyn Fn(&mut Bench)| {
        let mut b = Bench::repeating(mode);
        interrupt(&mut b);
        b.wait(1.0);
        let (attacks, cancels) = b.take();
        assert_eq!(
            (attacks, cancels),
            (vec![], 1),
            "{what}: one cancel, no attack"
        );
        assert!(
            !b.game.combat.repeat_attacking,
            "{what}: the repeat is over"
        );
    };
    stops("Escape", CombatMode::Melee, &|b: &mut Bench| {
        b.frame(vec![Action::begin(action::ESCAPE_KEY)]);
    });
    stops("the target lost", CombatMode::Missile, &|b: &mut Bench| {
        let mut out = Notices::default();
        b.game.set_selected_object(None, false, &mut out);
        b.inter
            .absorb(&mut b.game, out, RecordingRequests::default());
        let phys = crate::selection_geometry::SceneSelectionPhysics::default();
        b.inter.run_selection_change_notices(
            &mut b.game,
            &phys,
            0.0,
            dereth_primitives::LocalTime(b.now),
        );
        b.sent.extend(b.inter.take_pending_requests());
    });
    stops("a step", CombatMode::Melee, &|b: &mut Bench| {
        // The movement handler's abort, on a new movement key.
        let mut req = RecordingRequests::default();
        b.game.abort_automatic_attack(&mut req);
        b.sent.extend(req.0);
    });
    stops(
        "walking with a bow drawn",
        CombatMode::Missile,
        &|b: &mut Bench| {
            b.game.combat.forward_command = WALK_FORWARD;
        },
    );
    stops(
        "the shard refusing",
        CombatMode::Missile,
        &|b: &mut Bench| {
            b.shard_swings();
            b.shard_done(0x0036);
        },
    );
}

#[test]
fn under_horizon_the_advanced_option_is_refused_and_elsewhere_it_holds_the_attack_until_let_go() {
    // Horizon: the option on, a press still charges to the power aimed at and goes by itself.
    let mut b = Bench::new(CombatMode::Missile, true);
    b.advanced_option(true);
    assert!(!b.game.combat.advanced_combat_mode);
    b.frame(vec![Action::begin(action::COMBAT_AIM_MEDIUM)]);
    b.wait(0.6);
    assert_eq!(b.take(), (vec![medium(0.5)], 0));

    // The game's own controls with the option on: held, the bar charges and nothing goes; let
    // go, the attack goes at the level reached.
    let mut b = Bench::new(CombatMode::Missile, false);
    b.advanced_option(true);
    assert!(b.game.combat.advanced_combat_mode);
    b.frame(vec![Action::begin(action::COMBAT_AIM_MEDIUM)]);
    for _ in 0..20 {
        b.frame(vec![Action::repeat(action::COMBAT_AIM_MEDIUM, 1)]);
    }
    assert_eq!(b.take(), (vec![], 0), "nothing while it is held");
    b.frame(vec![Action::end(action::COMBAT_AIM_MEDIUM)]);
    let (attacks, _) = b.take();
    assert_eq!(attacks.len(), 1);
    assert!(
        (attacks[0].power - 0.7).abs() < 0.05,
        "at the level reached: {attacks:?}"
    );

    // Leaving Horizon gives the option back.
    let mut b = Bench::new(CombatMode::Missile, true);
    b.game
        .player_system
        .options
        .set(option::ADVANCED_COMBAT_UI, true);
    b.game.refuse_advanced_combat(false);
    assert!(b.game.combat.advanced_combat_mode);
}

/// Behaviour: combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills
#[test]
fn in_the_games_own_controls_a_click_goes_by_itself_when_the_bar_reaches_the_power_aimed_at() {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, false);
        let key = medium_key(mode);
        b.frame(vec![Action::begin(key), Action::end(key)]);
        b.wait(0.4);
        assert_eq!(b.take(), (vec![], 0), "{mode:?}: not before it gets there");
        b.wait(0.2);
        assert_eq!(b.take(), (vec![medium(0.5)], 0), "{mode:?}");
    }
}

/// Behaviour: combat.attack.a-held-control-charges-to-full-and-swings-only-on-release
#[test]
fn in_the_games_own_controls_a_held_key_charges_to_full_and_goes_only_when_let_go() {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, false);
        let key = medium_key(mode);
        b.frame(vec![Action::begin(key)]);
        for _ in 0..60 {
            b.frame(vec![Action::repeat(key, 1)]);
        }
        assert_eq!(b.take(), (vec![], 0), "{mode:?}: nothing while it is held");
        b.frame(vec![Action::end(key)]);
        assert_eq!(
            b.take(),
            (vec![medium(1.0), medium(0.5)], 0),
            "{mode:?}: at full, then again at the power aimed at"
        );
    }
}

#[test]
fn under_horizon_with_repeat_attacks_off_the_client_sends_each_next_attack_itself_until_escape() {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, true);
        b.repeat_option(false);
        b.frame(vec![Action::begin(medium_key(mode))]);
        b.wait(0.6);
        assert_eq!(b.take(), (vec![medium(0.5)], 0), "{mode:?}: the press");
        // Three rounds: the shard ends each attack and repeats none of them; the bar charges
        // again and the next attack goes when it reaches the power aimed at.
        for round in 0..3 {
            b.shard_done(ATTACK_SEQUENCE_ENDED);
            b.wait(0.4);
            assert_eq!(b.take(), (vec![], 0), "{mode:?}, round {round}: charging");
            b.wait(0.2);
            assert_eq!(
                b.take(),
                (vec![medium(0.5)], 0),
                "{mode:?}, round {round}: the next attack"
            );
        }
        // Escape stops it: the cancel goes, and the shard's answer to the attack that was out
        // starts no other.
        b.frame(vec![Action::begin(action::ESCAPE_KEY)]);
        b.shard_done(ATTACK_SEQUENCE_ENDED);
        b.wait(2.0);
        assert_eq!(b.take(), (vec![], 1), "{mode:?}: stopped");
        assert!(!b.game.combat.repeat_attacking);
        assert!(
            !b.game.player_system.options.auto_repeat_attack(),
            "{mode:?}: the player's option is left as it was"
        );
    }
}

#[test]
fn under_horizon_with_repeat_attacks_off_peace_a_step_a_lost_target_and_a_refusal_stop_it() {
    let stops = |what: &str, mode: CombatMode, interrupt: &dyn Fn(&mut Bench)| {
        let mut b = Bench::repeating_itself(mode);
        interrupt(&mut b);
        b.wait(2.0);
        let (attacks, _) = b.take();
        assert_eq!(attacks, vec![], "{what}: no attack follows");
        assert!(
            !b.game.combat.repeat_attacking,
            "{what}: the repeat is over"
        );
    };
    stops("peace", CombatMode::Missile, &|b: &mut Bench| {
        // The body is ready to change stance.
        let mut req = RecordingRequests::default();
        b.game
            .set_combat_mode(
                &mut req,
                &mut dereth_client_model::NullSink,
                CombatMode::NonCombat,
                true,
                true,
                false,
            )
            .expect("peace");
        assert_eq!(b.game.combat.combat_mode, CombatMode::NonCombat);
        b.sent.extend(req.0);
    });
    stops("a step", CombatMode::Melee, &|b: &mut Bench| {
        let mut req = RecordingRequests::default();
        b.game.abort_automatic_attack(&mut req);
        b.sent.extend(req.0);
    });
    stops("the target lost", CombatMode::Missile, &|b: &mut Bench| {
        let mut out = Notices::default();
        b.game.set_selected_object(None, false, &mut out);
        b.inter
            .absorb(&mut b.game, out, RecordingRequests::default());
        let phys = crate::selection_geometry::SceneSelectionPhysics::default();
        b.inter.run_selection_change_notices(
            &mut b.game,
            &phys,
            0.0,
            dereth_primitives::LocalTime(b.now),
        );
        b.sent.extend(b.inter.take_pending_requests());
    });
    stops(
        "walking with a bow drawn",
        CombatMode::Missile,
        &|b: &mut Bench| {
            b.game.combat.forward_command = WALK_FORWARD;
        },
    );
    stops("the shard refusing", CombatMode::Melee, &|b: &mut Bench| {
        // The client's next attack goes, and the shard refuses it with an error of its own, then
        // ends it.
        b.wait(0.6);
        let error = dereth_protocol::comms::CommunicationWeenieError { error_type: 0x003E };
        let mut blob = dereth_protocol::Opcode::COMMUNICATION_WEENIE_ERROR
            .0
            .to_le_bytes()
            .to_vec();
        blob.extend(dereth_protocol::write_body(&error).expect("encode"));
        super::apply_events(
            &mut b.inter,
            &[dereth_client_net::client_session::SessionEvent::UiEvent {
                opcode: dereth_protocol::Opcode::COMMUNICATION_WEENIE_ERROR,
                blob,
            }],
            &mut b.game,
        );
        b.shard_done(ATTACK_SEQUENCE_ENDED);
        b.sent.clear();
    });
    stops(
        "another answer from the shard",
        CombatMode::Missile,
        &|b: &mut Bench| {
            b.wait(0.6);
            b.shard_done(0x001D);
            b.sent.clear();
        },
    );
}

/// Behaviour: combat.attack.a-click-charges-the-bar-and-the-swing-comes-when-it-fills
#[test]
fn in_the_games_own_controls_with_repeat_attacks_off_one_click_is_one_attack() {
    for mode in [CombatMode::Missile, CombatMode::Melee] {
        let mut b = Bench::new(mode, false);
        b.repeat_option(false);
        let key = medium_key(mode);
        b.frame(vec![Action::begin(key), Action::end(key)]);
        b.wait(0.6);
        b.shard_done(ATTACK_SEQUENCE_ENDED);
        b.wait(2.0);
        assert_eq!(b.take(), (vec![medium(0.5)], 0), "{mode:?}");
        assert!(!b.game.combat.repeat_attacking);
    }
}
