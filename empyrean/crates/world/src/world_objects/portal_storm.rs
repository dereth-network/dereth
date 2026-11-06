//! Not ACE: portal storms, when `[portal_storms] threshold` is set.
//!
//! DIVERGE: V504. ACE has only the messages. The rule is ac1999's, not a known retail one: when a
//! landblock's surface holds more players than the threshold it brews, then is imminent, then moves
//! the player longest there every ten seconds. Those in buildings are warned and not moved; dungeons
//! take no part.

use std::collections::HashMap;

use empyrean_common::master_configuration::PortalStormConfiguration;
use empyrean_entity::{LandblockId, ObjectGuid};

use crate::network::game_event::events::{
    game_event_portal_storm, game_event_portal_storm_brewing, game_event_portal_storm_imminent,
    game_event_portal_storm_subsided,
};
use crate::network::game_event::game_event_message::session_data;
use crate::network::game_messages::game_message::enqueue_send;
use crate::World;

/// How often a landblock is looked at.
const CHECK_SECONDS: f64 = 1.0;
/// How often an imminent storm moves a player.
const MOVE_SECONDS: f64 = 10.0;
/// How often an imminent storm says so again.
const IMMINENT_RESEND_SECONDS: f64 = 60.0;
/// The level the warnings carry.
const WARNING_LEVEL: f32 = 1.0;

/// How far along a landblock's storm is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Stage {
    #[default]
    Calm,
    Brewing,
    Imminent,
}

/// What a landblock's storm does next.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StormStep {
    Nothing,
    Subsided,
    /// Warn who has not heard this stage.
    Warn,
    /// Warn, and move one player.
    Move,
}

/// One landblock's storm.
#[derive(Debug, Clone, Default)]
pub struct Storm {
    pub stage: Stage,
    stage_since: f64,
    next_check: f64,
    next_move: f64,
    arrived: HashMap<ObjectGuid, f64>,
    warned: HashMap<ObjectGuid, f64>,
}

impl Storm {
    /// The step for `count` players on the surface at `now`.
    pub fn advance(&mut self, count: usize, now: f64, c: PortalStormConfiguration) -> StormStep {
        let over = c.threshold != 0 && count > c.threshold as usize;
        if !over {
            if self.stage == Stage::Calm {
                return StormStep::Nothing;
            }
            self.set_stage(Stage::Calm, now);
            return StormStep::Subsided;
        }
        let elapsed = now - self.stage_since;
        match self.stage {
            Stage::Calm => self.set_stage(Stage::Brewing, now),
            Stage::Brewing if elapsed >= f64::from(c.brew_seconds) => {
                self.set_stage(Stage::Imminent, now);
            }
            Stage::Imminent
                if elapsed >= f64::from(c.imminent_seconds) && now >= self.next_move =>
            {
                self.next_move = now + MOVE_SECONDS;
                return StormStep::Move;
            }
            _ => {}
        }
        StormStep::Warn
    }

    fn set_stage(&mut self, stage: Stage, now: f64) {
        self.stage = stage;
        self.stage_since = now;
        self.next_move = 0.0;
        self.warned.clear();
    }
}

/// A landblock's tick: `players` are those in it.
pub fn tick(w: &mut World, id: LandblockId, players: &[ObjectGuid], now: f64) {
    let config = w.portal_storm_config;
    if config.threshold == 0 && !w.portal_storms.contains_key(&id) {
        return;
    }
    let surface: Vec<ObjectGuid> = players
        .iter()
        .copied()
        .filter(|p| on_the_surface(w, *p))
        .collect();
    let mut storm = w.portal_storms.remove(&id).unwrap_or_default();
    if now < storm.next_check {
        w.portal_storms.insert(id, storm);
        return;
    }
    storm.next_check = now + CHECK_SECONDS;
    storm.arrived.retain(|p, _| surface.contains(p));
    storm.warned.retain(|p, _| surface.contains(p));
    for p in &surface {
        storm.arrived.entry(*p).or_insert(now);
    }
    // Those warned hear it pass; a new stage forgets them.
    let warned: Vec<ObjectGuid> = storm.warned.keys().copied().collect();
    match storm.advance(surface.len(), now, config) {
        StormStep::Nothing => {}
        StormStep::Subsided => {
            for p in warned {
                send(w, p, |s| {
                    game_event_portal_storm_subsided::game_event_portal_storm_subsided(s)
                });
            }
        }
        step => {
            warn(w, &mut storm, &surface, now);
            if step == StormStep::Move {
                move_one(w, &mut storm);
            }
        }
    }
    w.portal_storms.insert(id, storm);
}

fn on_the_surface(w: &World, player: ObjectGuid) -> bool {
    use crate::physics::phys_ext;
    let Some(h) = w.objects.get(player).and_then(|o| o.phys) else {
        return false;
    };
    match phys_ext::cur_cell(w, h) {
        Some(c) if phys_ext::is_env_cell(c) => phys_ext::seen_outside(w, c),
        _ => true,
    }
}

fn send(
    w: &mut World,
    player: ObjectGuid,
    message: impl FnOnce(
        &mut crate::sessions::SessionData,
    ) -> crate::network::game_messages::game_message::GameMessage,
) {
    let Some(session) = crate::managers::player_manager::player_session(w, player) else {
        return;
    };
    let m = message(session_data(w, session));
    enqueue_send(w, session, m);
}

fn warn(w: &mut World, storm: &mut Storm, surface: &[ObjectGuid], now: f64) {
    for p in surface {
        let due = match storm.warned.get(p) {
            None => true,
            Some(at) => storm.stage == Stage::Imminent && now - at >= IMMINENT_RESEND_SECONDS,
        };
        if !due {
            continue;
        }
        if storm.stage == Stage::Brewing {
            send(w, *p, |s| {
                game_event_portal_storm_brewing::game_event_portal_storm_brewing(s, WARNING_LEVEL)
            });
        } else {
            send(w, *p, |s| {
                game_event_portal_storm_imminent::game_event_portal_storm_imminent(s, WARNING_LEVEL)
            });
        }
        storm.warned.insert(*p, now);
    }
}

/// The player longest there who is outdoors goes to the middle of the next landblock over.
fn move_one(w: &mut World, storm: &mut Storm) {
    let outdoors = storm
        .arrived
        .iter()
        .filter(|(p, _)| {
            w.objects
                .get(**p)
                .and_then(|o| o.location())
                .is_some_and(|l| !l.indoors())
        })
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(p, _)| *p);
    let Some(player) = outdoors else {
        return;
    };
    send(w, player, |s| {
        game_event_portal_storm::game_event_portal_storm(s)
    });
    storm.warned.remove(&player);
    let Some(mut to) = w.objects.get(player).and_then(|o| o.location()) else {
        return;
    };
    let x = to.landblock_x();
    let y = to.landblock_y();
    let (nx, ny) = match empyrean_common::thread_safe_random::ThreadSafeRandom::next(0, 3) {
        0 if x < 0xFE => (x + 1, y),
        1 if x > 0 => (x - 1, y),
        2 if y < 0xFE => (x, y + 1),
        _ if y > 0 => (x, y - 1),
        _ => (x + 1, y),
    };
    to.set_landblock_id(LandblockId::new((nx << 24) | (ny << 16) | 0x0001));
    to.position_x = 96.0;
    to.position_y = 96.0;
    if crate::entity::position_extensions::adjust_map_coords(w, &mut to).is_ok() {
        crate::world_objects::player_location::teleport(w, player, &to, false);
    }
}
