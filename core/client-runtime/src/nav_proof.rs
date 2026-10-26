//! `--nav-proof <cell>[,<cell>][,options]`: travel the player to a place, in a dungeon, outdoors,
//! and through portals and recalls, then log off.
//!
//! A developer run, not a player feature. Once the character is in the world it sends the chat
//! lines `--nav-prep` names (setting a test character up), teleports to the first cell
//! (`@teleloc`) and, unless `nosmite` is given, every two seconds of wall time from then on clears
//! the monsters in radar range and restores its vitals (`@smite all`, `@heal`): a dungeon's
//! residents would otherwise kill a new character on the way. The account must be allowed those
//! commands.
//!
//! It then asks the client's travel ([`crate::travel`]) for a journey to the second cell (in a
//! dungeon, without one, to the place farthest from the arrival by walking). With `tryrecalls`, the
//! character first uses each recall whose destination it has not learned, to learn it. The travel
//! keeps its portal knowledge in `--nav-store`'s folder. With `nocreatures` the route is not planned
//! round creatures, to compare. It logs the outcome, then ends the run.

use std::sync::Arc;

use dereth_dat::RetailDatStore;
use dereth_navigation::global_distance;
use dereth_primitives::{CellId, Position};
use dereth_world_data::land_source::DatLandSource;

use crate::travel::{Travel, TravelAllow, TravelEvent, TravelRequest, TravelTarget};

/// What the driver asks the frame to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProofCommand {
    /// Send this chat line, as typed.
    Chat(String),
    /// The run is over: log off.
    Finish,
}

/// What `--nav-proof` was asked to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProofSpec {
    /// Where to teleport to and start.
    pub start: CellId,
    /// Where to go; in a dungeon, the farthest place when absent.
    pub goal: Option<CellId>,
    /// Clear the monsters in range and heal, every two seconds.
    pub smite: bool,
    /// Plan round creatures standing or ambling near the route.
    pub creatures: bool,
    /// First use every recall whose destination is not known, to learn it.
    pub try_recalls: bool,
}

/// What the frame tells the driver.
#[derive(Debug)]
pub struct ProofInput<'a> {
    /// The wall clock.
    pub wall: f64,
    /// The player's position, when there is a body.
    pub at: Option<Position>,
    /// Whether no teleport is in progress.
    pub idle: bool,
    /// The land source, when the world is built.
    pub land: Option<&'a Arc<DatLandSource>>,
    /// The dat store.
    pub store: &'a Arc<RetailDatStore>,
    /// The world's identity, once known.
    pub world: Option<&'a str>,
    /// What the travel told since the last frame.
    pub events: &'a [TravelEvent],
}

#[derive(Debug, Clone, Copy, PartialEq)]
enum Phase {
    /// Waiting for the character to be in the world.
    WaitWorld,
    /// The teleport was asked for at this time.
    Teleport(f64),
    /// Arrived; waiting for the body to settle since this time.
    Settle(f64),
    /// Learning recalls.
    Learning,
    /// Travelling to the goal.
    Travelling,
    /// Finished.
    Done,
}

/// The journey that learns recalls, and the one to the goal.
const LEARN: u64 = 1;
const GO: u64 = 2;

/// The `--nav-proof` driver.
#[derive(Debug)]
pub struct NavProof {
    spec: ProofSpec,
    prep: Vec<String>,
    phase: Phase,
    arrival: Option<Position>,
    goal: Option<Position>,
    /// When the monsters were last cleared.
    last_clear: f64,
}

/// Parse `<cell>[,<cell>][,nosmite][,nocreatures][,tryrecalls]`, each cell a hexadecimal cell id
/// with or without `0x`: an interior cell, or a land cell (`0x0001..=0x0040`) for its middle.
///
/// # Errors
/// A description of what is wrong.
pub fn parse_spec(v: &str) -> Result<ProofSpec, String> {
    let cell = |s: &str| -> Result<CellId, String> {
        let t = s.trim().trim_start_matches("0x").trim_start_matches("0X");
        let id = u32::from_str_radix(t, 16).map_err(|_| format!("{s:?} is not a cell id"))?;
        let index = id & 0xFFFF;
        if index == 0 || (index > 0x40 && index < 0x0100) || index == 0xFFFF {
            return Err(format!("{s:?} is neither a land cell nor an interior cell"));
        }
        Ok(CellId(id))
    };
    let mut spec = ProofSpec {
        start: CellId(0),
        goal: None,
        smite: true,
        creatures: true,
        try_recalls: false,
    };
    let mut cells = Vec::new();
    for part in v.split(',') {
        match part.trim() {
            "nosmite" => spec.smite = false,
            "nocreatures" => spec.creatures = false,
            "tryrecalls" => spec.try_recalls = true,
            other => cells.push(cell(other)?),
        }
    }
    let mut cells = cells.into_iter();
    spec.start = cells.next().ok_or("no cell to start in")?;
    spec.goal = cells.next();
    if cells.next().is_some() {
        return Err(String::from("at most two cells"));
    }
    if spec.goal.is_none() && spec.start.0 & 0xFFFF < 0x0100 {
        return Err(String::from("an outdoor run needs a cell to walk to"));
    }
    Ok(spec)
}

impl NavProof {
    /// A driver for `--nav-proof`, sending the `prep` chat lines first.
    #[must_use]
    pub fn new(spec: ProofSpec, prep: Vec<String>) -> Self {
        Self {
            spec,
            prep,
            phase: Phase::WaitWorld,
            arrival: None,
            goal: None,
            last_clear: f64::MIN,
        }
    }

    /// Whether the run is over.
    #[must_use]
    pub fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    fn finish(&mut self, out: &mut Vec<ProofCommand>, why: &str) {
        tracing::info!(target: "nav_proof", "{why}");
        out.push(ProofCommand::Finish);
        self.phase = Phase::Done;
    }

    /// Find where to stand at the start, and the goal.
    fn prepare(
        &mut self,
        land: &Arc<DatLandSource>,
        store: &Arc<RetailDatStore>,
    ) -> Result<(), String> {
        let start = self.spec.start;
        let end = self.spec.goal.unwrap_or(start);
        crate::travel::preload(land, start, start);
        crate::travel::preload(land, end, end);
        // Where to stand is the run's own question, answered by a planner of its own.
        let mut own = crate::travel::planner(land, store)?;
        let nav = &mut own;
        let stand = |nav: &mut crate::travel::Planner, cell: CellId| -> Result<Position, String> {
            let m = crate::travel::middle(land, cell)
                .ok_or_else(|| format!("no cell {:08X}", cell.0))?;
            let region = nav.region_between(&m, &m).map_err(|e| e.to_string())?;
            let s = region
                .nearest_place(&m, 12.0)
                .ok_or_else(|| format!("nowhere to stand near {:08X}", cell.0))?;
            Ok(region.position(s))
        };
        let arrival = stand(nav, start)?;
        let goal = match self.spec.goal {
            Some(c) => stand(nav, c)?,
            None => {
                let region = nav
                    .region_between(&arrival, &arrival)
                    .map_err(|e| e.to_string())?;
                let s = region
                    .nearest_place(&arrival, 2.0)
                    .ok_or("nowhere to stand at the arrival")?;
                region.position(crate::travel::farthest_by_walking(region, s))
            }
        };
        tracing::info!(
            target: "nav_proof",
            "arrival {:08X} {:?}, goal {:08X} {:?}",
            arrival.cell.0,
            arrival.frame.origin,
            goal.cell.0,
            goal.frame.origin
        );
        self.arrival = Some(arrival);
        self.goal = Some(goal);
        Ok(())
    }

    fn go(&mut self, travel: &mut Travel) {
        let Some(goal) = self.goal else { return };
        travel.request(TravelRequest::Go {
            id: GO,
            target: TravelTarget::Place(goal),
            allow: TravelAllow::default(),
        });
        self.phase = Phase::Travelling;
    }

    /// One frame.
    pub fn use_time(&mut self, input: &ProofInput<'_>, travel: &mut Travel) -> Vec<ProofCommand> {
        let mut out = Vec::new();
        let wall = input.wall;
        if self.spec.smite
            && matches!(
                self.phase,
                Phase::Settle(_) | Phase::Learning | Phase::Travelling
            )
            && wall - self.last_clear > 2.0
        {
            out.push(ProofCommand::Chat(String::from("@smite all")));
            out.push(ProofCommand::Chat(String::from("@heal")));
            self.last_clear = wall;
        }
        match self.phase {
            Phase::Done => {}
            Phase::WaitWorld => {
                let (Some(_), Some(land), true, Some(_)) =
                    (input.at, input.land, input.idle, input.world)
                else {
                    return out;
                };
                travel.creatures = self.spec.creatures;
                if let Err(e) = self.prepare(land, input.store) {
                    self.finish(&mut out, &format!("cannot prepare the route: {e}"));
                    return out;
                }
                for line in &self.prep {
                    out.push(ProofCommand::Chat(line.clone()));
                }
                let Some(a) = self.arrival else { return out };
                let o = a.frame.origin;
                out.push(ProofCommand::Chat(format!(
                    "@teleloc 0x{:08X} [{:.4} {:.4} {:.4}] 1 0 0 0",
                    a.cell.0,
                    o.x,
                    o.y,
                    o.z + 0.05
                )));
                tracing::info!(target: "nav_proof", "teleporting to {:08X}", a.cell.0);
                self.phase = Phase::Teleport(wall);
            }
            Phase::Teleport(since) => {
                let arrived = match (input.at, self.arrival) {
                    (Some(p), Some(a)) => global_distance(&p, &a) < 5.0,
                    _ => false,
                };
                if arrived && input.idle {
                    self.phase = Phase::Settle(wall);
                } else if wall - since > 60.0 {
                    self.finish(&mut out, "the teleport did not arrive within 60 s");
                }
            }
            Phase::Settle(since) => {
                if wall - since < 3.0 || input.at.is_none() {
                    return out;
                }
                if self.spec.try_recalls {
                    travel.request(TravelRequest::LearnRecalls { id: LEARN });
                    self.phase = Phase::Learning;
                } else {
                    self.go(travel);
                }
            }
            Phase::Learning | Phase::Travelling => {
                for e in input.events {
                    match e {
                        TravelEvent::Arrived { id: LEARN, .. }
                        | TravelEvent::Failed { id: LEARN, .. } => self.go(travel),
                        TravelEvent::Arrived {
                            id: GO,
                            distance,
                            stats,
                        } => {
                            let why = format!(
                                "arrived: {distance:.2} m from the goal at {:?}; {} journeys, {} \
                                 portal uses, {} refused, {} recalls; walked {:.1} m in {:.1} s \
                                 of game time ({:.1} s of wall time); {} stuck events, {} \
                                 replans, {} round new obstacles, {} round creatures, {} doors \
                                 opened",
                                input.at.map(|p| p.cell),
                                stats.journeys,
                                stats.portal_uses,
                                stats.refusals,
                                stats.recalls,
                                stats.walked,
                                stats.game_seconds,
                                stats.wall_seconds,
                                stats.stuck,
                                stats.replans,
                                stats.obstacle_replans,
                                stats.creature_replans,
                                stats.doors
                            );
                            self.finish(&mut out, &why);
                        }
                        TravelEvent::Failed { id: GO, reason, .. } => {
                            self.finish(&mut out, &format!("gave up: {reason}"));
                        }
                        _ => {}
                    }
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks the developer run's argument parsing).
    use super::*;

    #[test]
    fn a_nav_proof_spec_is_one_or_two_cells_and_options() {
        let s = parse_spec("0x01EE0105").expect("one interior cell");
        assert_eq!(
            (s.start, s.goal, s.smite, s.creatures, s.try_recalls),
            (CellId(0x01EE_0105), None, true, true, false)
        );
        let s = parse_spec("A9B40021,a9b30100,nosmite,nocreatures,tryrecalls").expect("two cells");
        assert_eq!(
            (s.start, s.goal, s.smite, s.creatures, s.try_recalls),
            (
                CellId(0xA9B4_0021),
                Some(CellId(0xA9B3_0100)),
                false,
                false,
                true
            )
        );
        assert!(parse_spec("0xA9B40021").is_err(), "outdoors needs a goal");
        assert!(parse_spec("0x01EE0080").is_err(), "neither kind of cell");
        assert!(parse_spec("dungeon").is_err());
    }
}
