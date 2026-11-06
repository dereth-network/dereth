//! Every server-to-client script call recorded in 2013 and 2017, run against the model the
//! recorded messages build, with no transport: one client per capture (one run of the retail
//! client), fed the capture's server messages in their order and its entries into the world.

use std::net::SocketAddr;
use std::sync::Arc;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::{
    link_scripts, link_scripts_noting, CallResult, CallTally, Client, ClientData, ClientSettings,
};
use auberean_dat::locate::{install_dats, DatName};
use auberean_dat::testing::{open_wrong_once, require_target_install};
use auberean_dat::DatFile;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

/// The client's entering the world, which the event queue's holding rule turns on.
pub(crate) const PLAY_CHARACTER: u32 = 0x1002B;
const CLIENT_POSITION: u32 = 0x1008E;

/// The server's script-call carriers: to the player, to an object in a cell, to an object by its
/// visual, and the alternate form to the player.
const CALLS: [u32; 4] = [0x10066, 0x10067, 0x100C4, 0x100CA];

pub(crate) fn data() -> Option<ClientData> {
    let dir = require_target_install();
    let found = install_dats(&dir).ok()?;
    let (_, p) = found.iter().find(|(n, _)| *n == DatName::Portal)?;
    let portal = Arc::new(DatFile::open(p).ok()?);
    let (_, l) = found
        .iter()
        .find(|(n, _)| *n == DatName::Local("English".into()))?;
    let english = Arc::new(DatFile::open(l).ok()?);
    let program = link_scripts(&portal).expect("the library links");
    Some(ClientData {
        files: DataFiles {
            files: vec![Arc::clone(&portal), english],
            language: ENGLISH,
        },
        scripts: Some((portal, Arc::new(program))),
    })
}

pub(crate) fn client(data: &ClientData) -> Client {
    let settings = ClientSettings {
        login_server: SocketAddr::from(([127, 0, 0, 1], 9000)),
        account: Vec::new(),
        ticket: Vec::new(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    Client::new(settings, data.clone())
}

/// Every server-to-client script call recorded in 2013 and 2017 runs against the model once the
/// event queue lets it through: to its end, or stopped cleanly at a native not written yet
/// (counted by native), or, in a capture that began after its player was created, not run (no
/// instance to run it on). The machine itself never fails.
///
/// Behaviour: script.model.every-recorded-server-call-runs-against-the-model
#[test]
fn every_recorded_server_call_runs_against_the_model_or_stops_at_a_named_native() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut tally = CallTally::default();
    let mut capture = -1;
    let mut c = client(&data);
    let mut fed = 0u64;
    let mut frame = f64::NEG_INFINITY;
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            tally_into(&mut tally, &c.calls);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        // Each recorded instant is a frame: the world's upkeep runs before its messages.
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            fed += u64::from(CALLS.contains(&m.message_type));
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    tally_into(&mut tally, &c.calls);
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    eprintln!(
        "{fed} calls recorded; {} not taken out of the event queue (waiting for an object or a stamp that never came); {} calls: {} to the end, {} stopped at a native not written yet, {} not run (no player), {} null reference, {} refused, {} ended otherwise; stopped otherwise: {:?}",
        fed - tally.calls,
        tally.calls,
        tally.completed,
        tally.not_ported.values().sum::<u64>(),
        tally.no_receiver,
        tally.null_reference,
        tally.refused,
        tally.ended,
        tally.stopped
    );
    let mut stops: Vec<_> = tally.not_ported.iter().collect();
    stops.sort_by_key(|(_, n)| std::cmp::Reverse(**n));
    for (native, n) in stops.iter().take(30) {
        eprintln!("  {n:>7}  {}", lib.native_name(**native));
    }
    assert_eq!(fed, 757_726);
    assert_eq!(tally.null_reference, 0);
    // Every call with a player to run on runs to its end: none stops at a native not written.
    assert!(tally.completed >= 741_475, "{} to the end", tally.completed);
    let not_ported: u64 = tally.not_ported.values().sum();
    assert_eq!(not_ported, 0, "{not_ported} stopped at a native");
    assert!(
        tally.stopped.is_empty(),
        "the machine itself failed: {:?}",
        tally.stopped
    );
}

/// Every quality the 2013 and 2017 servers set on an object here is told to the object's scripts
/// once it is set (its kind and number), and the scripts take it to its end: at least 251,000
/// changes. The objects' scripts keep their bodies' collision state in step with the quality words
/// that hold its masks: at least 4,600 bodies' collision states set.
///
/// Behaviour: script.model.every-recorded-quality-change-is-told-to-the-objects-scripts
#[test]
fn every_recorded_quality_change_is_told_to_the_objects_scripts() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut tally = CallTally::default();
    let (mut capture, mut bodies) = (-1, 0u64);
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let done = |c: &Client, tally: &mut CallTally, bodies: &mut u64| {
        tally_into(tally, &c.stat_changes);
        *bodies += c.scripts.as_ref().map_or(0, |s| {
            s.host
                .bodies
                .values()
                .filter(|b| b.collision.bits != 0)
                .count() as u64
        });
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            done(&c, &mut tally, &mut bodies);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    done(&c, &mut tally, &mut bodies);
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    eprintln!(
        "{} quality changes told: {} to the end, {} stopped at a native not written yet, {} null reference, {} refused, {} ended otherwise; stopped otherwise: {:?}; {bodies} bodies' collision states set",
        tally.calls,
        tally.completed,
        tally.not_ported.values().sum::<u64>(),
        tally.null_reference,
        tally.refused,
        tally.ended,
        tally.stopped
    );
    for (native, n) in &tally.not_ported {
        eprintln!("  {n:>7}  {}", lib.native_name(*native));
    }
    assert!(tally.completed >= 251_000, "{tally:?}");
    assert!(tally.not_ported.is_empty(), "{tally:?}");
    assert!(tally.stopped.is_empty(), "{tally:?}");
    assert_eq!(tally.null_reference, 0, "{tally:?}");
    assert!(bodies >= 4_600, "{bodies}");
}

/// Every object the 2013 and 2017 servers made, placed, took out of the world or destroyed has
/// its scripts told, and they take it to its end: each object made (its instance made then), each
/// one placed in the world with no parent, each one in the world contained, leaving or carried
/// (then told it is out), each one the server destroyed. A capture that began after its player was
/// created has no player for the scripts to find (a corpse placed asks whether the player had its
/// creature selected): its lives are counted apart. At least 125,000 objects made, 81,000 placed,
/// 30,000 taken out of the world and 34,000 destroyed.
///
/// Behaviour: script.model.every-recorded-objects-life-is-told-to-its-scripts
#[test]
fn every_recorded_objects_life_is_told_to_its_scripts() {
    use auberean_client_runtime::lifecycle::function as f;
    use std::collections::BTreeMap;
    /// The player's creation.
    const PLAYER_OBJECT: u32 = 0x10060;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let (mut tally, mut without) = (CallTally::default(), CallTally::default());
    let mut told = BTreeMap::<u32, u64>::new();
    let (mut capture, mut with_player) = (-1, false);
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let done = |c: &Client,
                with_player: bool,
                tally: &mut CallTally,
                without: &mut CallTally,
                told: &mut BTreeMap<u32, u64>| {
        if with_player {
            tally_into(tally, &c.lives);
            for (k, v) in &c.life.told {
                *told.entry(*k).or_default() += v;
            }
        } else {
            tally_into(without, &c.lives);
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            done(&c, with_player, &mut tally, &mut without, &mut told);
            capture = m.capture;
            with_player = false;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        with_player |= m.message_type == PLAYER_OBJECT;
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    done(&c, with_player, &mut tally, &mut without, &mut told);
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let n = |k: u32| told.get(&k).copied().unwrap_or(0);
    eprintln!(
        "{} lives told in captures with their player ({} made, {} into the world, {} out of it, {} after it, {} destroyed): {} to the end, {} stopped at a native not written yet, {} null reference, {} refused, {} ended otherwise; stopped otherwise: {:?}; without a player: {} told, {} to the end, {} null reference",
        tally.calls,
        n(f::HANDLE_CREATION),
        n(f::HANDLE_ENTER_WORLD),
        n(f::HANDLE_LEAVE_WORLD),
        n(f::HANDLE_POST_LEAVE_WORLD),
        n(f::HANDLE_DESTRUCTION),
        tally.completed,
        tally.not_ported.values().sum::<u64>(),
        tally.null_reference,
        tally.refused,
        tally.ended,
        tally.stopped,
        without.calls,
        without.completed,
        without.null_reference,
    );
    for (native, k) in &tally.not_ported {
        eprintln!("  {k:>7}  {}", lib.native_name(*native));
    }
    assert!(tally.not_ported.is_empty(), "{tally:?}");
    assert!(without.not_ported.is_empty(), "{without:?}");
    assert!(tally.stopped.is_empty(), "{tally:?}");
    assert_eq!(tally.null_reference, 0, "{tally:?}");
    assert_eq!(tally.completed, tally.calls, "{tally:?}");
    assert_eq!(n(f::HANDLE_LEAVE_WORLD), n(f::HANDLE_POST_LEAVE_WORLD));
    assert!(n(f::HANDLE_CREATION) >= 125_000, "{told:?}");
    assert!(n(f::HANDLE_ENTER_WORLD) >= 81_000, "{told:?}");
    assert!(n(f::HANDLE_LEAVE_WORLD) >= 30_000, "{told:?}");
    assert!(n(f::HANDLE_DESTRUCTION) >= 34_000, "{told:?}");
}

/// Every rename the 2017 servers sent (each a visual update of the name quality) reaches the
/// object's name as its scripts read it, through the event queue as the client runs it. One stamped
/// past the object's last visual stamp takes effect, and the name is the new one at once. One
/// stamped at or before it is dropped (the servers send an object's earlier visual messages again
/// after its creation, which already carries their result): the name the creation brought is
/// already the same. One about an object the client does not have yet waits, and the name is the
/// new one once the object's creation comes. Of 16,315 renames, at least 16,313 reach the name
/// (the rest are renamed again before theirs comes), every one taken at once.
///
/// Behaviour: script.model.every-recorded-rename-reaches-the-objects-name
#[test]
fn every_recorded_rename_reaches_the_objects_name() {
    use auberean_client_runtime::repository;
    use auberean_script::native_object::TextInfo;
    use std::collections::BTreeMap;
    /// A rename's message type.
    const RENAME: u32 = 0x100BD;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    /// How the renames went.
    #[derive(Debug, Default)]
    struct Renames {
        fed: u64,
        taken: u64,
        taken_reached: u64,
        dropped: u64,
        dropped_already_named: u64,
        held: u64,
        held_already_named: u64,
        reached_later: u64,
        superseded: u64,
        never_reached: u64,
    }
    let mut r = Renames::default();
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    // Renames whose name is not yet the object's: by object, the newest.
    let mut pending = BTreeMap::<u64, TextInfo>::new();
    let name_is = |c: &mut Client, id: u64, v: &TextInfo| {
        c.with_host(|h| repository::answer(h, &repository::STRING_INFO, id, repository::NAME))
            .flatten()
            .as_ref()
            == Some(v)
    };
    let settle = |c: &mut Client, pending: &mut BTreeMap<u64, TextInfo>, r: &mut Renames| {
        pending.retain(|id, v| {
            let reached = name_is(c, *id, v);
            r.reached_later += u64::from(reached);
            !reached
        });
    };
    let close = |pending: &mut BTreeMap<u64, TextInfo>, r: &mut Renames| {
        r.never_reached += std::mem::take(pending).len() as u64;
    };
    let mut stats = auberean_client_model::events::QueueStats::default();
    captures::messages_in_time_order(&index, &types, &[2017], |m| {
        if m.capture != capture {
            settle(&mut c, &mut pending, &mut r);
            close(&mut pending, &mut r);
            add_stats(&mut stats, c.queue.stats());
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
            settle(&mut c, &mut pending, &mut r);
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let rename = match &msg {
            ServerMessage::UpdateStringInfoVisual(q) if m.message_type == RENAME => {
                Some((q.object.id, repository::text_info(&q.value)))
            }
            _ => None,
        };
        let before = c.queue.stats();
        c.take_message(m.blob_id, msg);
        let Some((id, v)) = rename else {
            return;
        };
        r.fed += 1;
        let reached = name_is(&mut c, id, &v);
        let after = c.queue.stats();
        if after.dropped > before.dropped {
            r.dropped += 1;
            r.dropped_already_named += u64::from(reached);
        } else if after.waited == before.waited && after.held_visual == before.held_visual {
            r.taken += 1;
            r.taken_reached += u64::from(reached);
        } else {
            r.held += 1;
            r.held_already_named += u64::from(reached);
        }
        if !reached && pending.insert(id, v).is_some() {
            r.superseded += 1;
        }
    })
    .expect("the index reads");
    settle(&mut c, &mut pending, &mut r);
    close(&mut pending, &mut r);
    add_stats(&mut stats, c.queue.stats());
    eprintln!("{r:?}; the event queue: {stats:?}");
    assert_eq!(r.fed, 16_315);
    assert_eq!(r.taken_reached, r.taken, "{r:?}");
    assert!(r.taken >= 24, "{r:?}");
    assert_eq!(r.dropped_already_named, r.dropped, "{r:?}");
    assert_eq!(r.never_reached, 0, "{r:?}");
    assert!(
        r.taken_reached + r.dropped_already_named + r.held_already_named + r.reached_later
            >= 16_313,
        "{r:?}"
    );
}

fn add_stats(
    into: &mut auberean_client_model::events::QueueStats,
    s: auberean_client_model::events::QueueStats,
) {
    into.held_entering += s.held_entering;
    into.waited += s.waited;
    into.held_visual += s.held_visual;
    into.dropped += s.dropped;
    into.force_creates += s.force_creates;
    into.queues_given_up += s.queues_given_up;
}

fn tally_into(into: &mut CallTally, from: &CallTally) {
    into.calls += from.calls;
    into.completed += from.completed;
    into.ended += from.ended;
    into.refused += from.refused;
    into.null_reference += from.null_reference;
    into.no_receiver += from.no_receiver;
    for (k, v) in &from.not_ported {
        *into.not_ported.entry(*k).or_default() += v;
    }
    for (k, v) in &from.stopped {
        *into.stopped.entry(k.clone()).or_default() += v;
    }
}

/// How a story hook's calls went, by call.
#[derive(Default, Debug)]
struct HookTally {
    asked: u64,
    completed: u64,
    not_ported: std::collections::BTreeMap<u32, u64>,
    other: std::collections::BTreeMap<String, u64>,
}

impl HookTally {
    fn add(&mut self, r: &CallResult) {
        self.asked += 1;
        match r {
            CallResult::Completed => self.completed += 1,
            CallResult::NotPorted(n) => *self.not_ported.entry(*n).or_default() += 1,
            r => *self.other.entry(format!("{r:?}")).or_default() += 1,
        }
    }
}

/// A story hook's kinds: a hit, the missiles made, one fired.
const HIT: u32 = 1;
const CREATE_MISSILES: u32 = 2;
const FIRE_MISSILE: u32 = 3;
/// A story's bit for a basic attack (its skill not shown).
const BASIC_ATTACK: u32 = 0x20;

/// Every story the 2013 and 2017 servers told, its hooks run as the attacker's body plays them,
/// in order: each hook asks the attacker's scripts for the effects and behaviors it shows; a
/// hit's damage is shown by the target's scripts (the combat lines into the chat, the floating
/// text to the panels); a hit on the player by a skill asks the attacker's scripts whether the
/// skill harms; a hook that makes missiles asks the attacker's body's scripts for the missile of
/// every hook that fires one, and a hook that fires one asks for its own. Every call runs to its
/// end: at least 1,558,000 hooks' effects, 795,000 hits' damage (183,000 combat lines, 703,000
/// floating texts, another's hit only within range of where the player walked), 59,000 questions of harm (43,000 harmful) and 327,000 missiles, each made from
/// a record, aimed at an object and passing through physics types the property list names.
///
/// Behaviour: script.model.every-recorded-story-hook-runs-against-the-scripts
#[test]
fn every_recorded_story_hook_runs_against_the_attackers_and_targets_scripts() {
    use auberean_client_runtime::combat::{Damage, DO_3D_DAMAGE};
    use auberean_script::native_object::NativeObject;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    // Where the player walked: another's hit shows its floating text only within range of them.
    types.push(CLIENT_POSITION);
    let (mut effects, mut damage, mut harm, mut missile) = (
        HookTally::default(),
        HookTally::default(),
        HookTally::default(),
        HookTally::default(),
    );
    // What the missiles were: made from a record, held before firing, thrown, a copy of the
    // weapon, aimed at an object, passing through something, of physics types the property list
    // names (their own, and those they pass through when placed).
    let (mut made_from, mut held, mut thrown, mut cloned, mut aimed, mut passing) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut typed = 0u64;
    let (mut stories, mut hooks, mut missiles, mut no_attacker) = (0u64, 0u64, 0u64, 0u64);
    let (mut shown_fx, mut lines, mut floating, mut harmful) = (0u64, 0u64, 0u64, 0u64);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            match ClientMessage::decode(&m.raw) {
                Ok(ClientMessage::PlayCharacter(p)) => c.act(Action::PlayCharacter {
                    character: p.character,
                }),
                Ok(ClientMessage::Position(p)) => {
                    let heading = c.body.heading;
                    c.body.place(p.cell, p.origin, heading);
                }
                _ => {}
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let story = match &msg {
            ServerMessage::DoStory(e) => Some((e.sender.id, e.action.clone())),
            _ => None,
        };
        c.take_message(m.blob_id, msg);
        let Some((body, story)) = story else {
            return;
        };
        if c.model.player_object().is_none() {
            return;
        }
        stories += 1;
        if c.model.object(body).is_none() {
            no_attacker += 1;
            return;
        }
        let (lines_before, notices_before) = (c.shown.len(), c.notices.len());
        for h in &story.hooks {
            hooks += 1;
            let kind = h.bits & 0xF;
            let [result, _, attack_hook, target_health, attacker_health, target_pk, attacker_pk] =
                h.numbers;
            #[allow(clippy::cast_sign_loss)]
            let (result, attack_hook) = (result as u32, attack_hook as u32);
            if let Some((r, fx)) = c.hit_effects(body, story.skill, attack_hook, result) {
                effects.add(&r);
                shown_fx += u64::from(fx.attacker_fx | fx.target_fx | fx.weapon_fx != 0);
            }
            if kind != HIT {
                missiles += 1;
                let fired: Vec<_> = match kind {
                    CREATE_MISSILES => story
                        .hooks
                        .iter()
                        .filter(|f| f.bits & 0xF == FIRE_MISSILE)
                        .collect(),
                    FIRE_MISSILE => vec![h],
                    _ => Vec::new(),
                };
                for f in fired {
                    #[allow(clippy::cast_sign_loss)]
                    let hook = f.numbers[2] as u32;
                    if let Some((r, m)) =
                        c.missile_info(body, story.skill, hook, story.attacker, f.target)
                    {
                        missile.add(&r);
                        made_from += u64::from(m.entity != 0);
                        held += u64::from(m.holding_location != 0);
                        thrown += u64::from(m.thrown);
                        cloned += u64::from(m.clone);
                        aimed += u64::from(m.targeting.bits & 1 != 0);
                        passing += u64::from(m.collision.bits != 0);
                        typed += u64::from(
                            m.collision.physics_type.mask != 0
                                && m.collision.placement_physics.mask != 0,
                        );
                    }
                }
                continue;
            }
            let shown = Damage {
                attacker: story.attacker,
                target: h.target,
                skill: if story.bits & BASIC_ATTACK != 0 {
                    0
                } else {
                    story.skill
                },
                result,
                target_health,
                attacker_health,
                target_pk,
                attacker_pk,
            };
            if let Some(r) = c.show_damage(&shown) {
                damage.add(&r);
            }
            if Some(h.target) == c.model.player && story.skill != 0 {
                if let Some((r, yes)) = c.is_skill_harmful(body, story.skill) {
                    harm.add(&r);
                    harmful += u64::from(yes);
                }
            }
        }
        lines += (c.shown.len() - lines_before) as u64;
        floating += c.notices[notices_before..]
            .iter()
            .filter(|n| {
                n.native == DO_3D_DAMAGE
                    && n.objects
                        .iter()
                        .flatten()
                        .any(|o| matches!(o, NativeObject::DamageText(_)))
            })
            .count() as u64;
        c.shown.clear();
        c.notices.clear();
    })
    .expect("the index reads");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    eprintln!(
        "{stories} stories with a player here ({no_attacker} without their body), {hooks} hooks ({missiles} of missiles); {shown_fx} hits with an effect; {lines} combat lines, {floating} floating texts; {harmful} harmful"
    );
    eprintln!(
        "missiles: {made_from} made from a record, {held} held, {thrown} thrown, {cloned} copies of the weapon, {aimed} aimed at an object, {passing} passing through something, {typed} with physics types"
    );
    for (what, t) in [
        ("effects", &effects),
        ("damage", &damage),
        ("harm", &harm),
        ("missiles", &missile),
    ] {
        eprintln!(
            "{what}: {} asked, {} to the end, other {:?}",
            t.asked, t.completed, t.other
        );
        for (n, k) in &t.not_ported {
            eprintln!("  {k:>7}  {}", lib.native_name(*n));
        }
    }
    for t in [&effects, &damage, &harm, &missile] {
        assert!(t.not_ported.is_empty(), "{t:?}");
        assert!(t.other.is_empty(), "{t:?}");
    }
    assert!(stories >= 852_000, "{stories} stories");
    assert!(effects.completed >= 1_558_000, "{effects:?}");
    assert!(damage.completed >= 795_000, "{damage:?}");
    assert!(harm.completed >= 59_000, "{harm:?}");
    assert!(missile.completed >= 327_000, "{missile:?}");
    // What the hits showed: effects, the combat lines, the floating texts, the skills that harm.
    assert!(shown_fx >= 512_000, "{shown_fx}");
    assert!(lines >= 183_000, "{lines}");
    assert!(floating >= 703_000, "{floating}");
    assert!(harmful >= 43_000, "{harmful}");
    // What the missiles were.
    for (what, n, floor) in [
        ("made from a record", made_from, 327_000),
        ("held before firing", held, 266_000),
        ("thrown", thrown, 37_000),
        ("copies of the weapon", cloned, 36_000),
        ("aimed at an object", aimed, 327_000),
        ("passing through something", passing, 327_000),
        ("with physics types", typed, 327_000),
    ] {
        assert!(n >= floor, "{n} missiles {what}");
    }
}

/// Every player's creation the 2005 and 2017 servers sent (449) puts the world in the region it
/// names, region 1, whose record the install's region mapping names (the region record the scene
/// reads) and which says it is region 1: the scene is asked to take the region up at the first
/// creation after entering the world (183), and a creation again in the same world (a hand-off to
/// another server, 266) changes nothing.
///
/// Behaviour: model.world.every-recorded-player-creation-puts-the-world-in-its-region
#[test]
fn every_recorded_player_creation_puts_the_world_in_its_region() {
    const PLAYER_OBJECT: u32 = 0x10060;
    const CHAR_EXIT_GAME: u32 = 0x10027;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let types = [PLAYER_OBJECT, CHAR_EXIT_GAME, PLAY_CHARACTER];
    let (mut creations, mut changes, mut again, mut elsewhere) = (0u64, 0u64, 0u64, 0u64);
    let mut records = std::collections::BTreeSet::new();
    let mut capture = -1;
    let mut c = client(&data);
    captures::messages_in_time_order(&index, &types, &[2005, 2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        let creation = matches!(msg, ServerMessage::PlayerObject(_));
        c.take_message(m.blob_id, msg);
        if !creation {
            return;
        }
        creations += 1;
        elsewhere += u64::from(c.world.region() != 1);
        match c.world.take_region_change() {
            Some(r) => {
                changes += 1;
                records.insert(r);
            }
            None => again += 1,
        }
    })
    .expect("the index reads");
    eprintln!(
        "{creations} player creations: {changes} took a region up ({records:x?}), {again} named the region the world was in, {elsewhere} left the world elsewhere"
    );
    assert_eq!(creations, 449);
    assert_eq!(elsewhere, 0);
    assert_eq!((changes, again), (183, 266));
    assert_eq!(records.into_iter().collect::<Vec<_>>(), vec![REGION_RECORD]);
}

/// The region record every world of the install starts from.
const REGION_RECORD: u32 = 0x0F00_0000;

/// The data names one region: its mapping has the one row and the portal file the one region
/// record. A player's creation naming any other number changes nothing: the world stays in no
/// region, or in the one it has, and the scene is not asked to take anything up.
///
/// Behaviour: model.world.a-region-the-data-does-not-name-is-not-taken-up
#[test]
fn a_region_the_data_does_not_name_is_not_taken_up() {
    use auberean_client_runtime::world::REGION_MAPPING;
    use auberean_protocol::login::PlayerObject;
    let data = data().expect("the September 2013 install opens");
    let mut c = client(&data);
    {
        let db = c.data_download().cache().database();
        assert_eq!(db.data_id_by_enum(1, REGION_MAPPING), REGION_RECORD);
        assert_eq!(db.data_id_by_enum(2, REGION_MAPPING), 0);
    }
    let portal = &data.files.files[0];
    assert_eq!(portal.entries_in(0x0F00_0000, 0x0FFF_FFFF).count(), 1);
    // Out of the world: taken at once however often it comes.
    let enters = |c: &mut Client, region| {
        c.take_message(
            (3 << 56) | 0x2000_0000_0000_0000,
            ServerMessage::PlayerObject(PlayerObject {
                id: 0x2130_0000_0000_0001,
                region,
            }),
        );
        (c.world.region(), c.world.take_region_change())
    };
    assert_eq!(enters(&mut c, 2), (0, None));
    assert_eq!(enters(&mut c, 1), (1, Some(REGION_RECORD)));
    assert_eq!(enters(&mut c, 2), (1, None));
}

/// Records asked for, by type and id, each with when it was first asked.
type Asks = std::collections::BTreeMap<(u32, u32), f64>;

/// **A created object's saved state names its records as the recorded clients' did**: over the
/// 2013 and 2017 sessions, every saved state the created objects' records bring, or name in turn,
/// is asked by the scripts which records it needs (about 42,600 times, a state asked again only once it
/// has left memory), each answer running to its end, naming about 15,300 ids. Given the gaps the recorded
/// clients' files had (every record a recorded client asked the server for, removed from ours),
/// our client asks the server for no picture, sound or saved state: the 12 pictures and the music
/// a state names that the recorded clients asked for came with a story's subtitle tables, when the
/// story played, not at a creation, and no state a creation brought named them before.
///
/// Behaviour: data.cache.a-created-objects-state-names-its-records
#[test]
fn every_created_objects_saved_state_names_its_records_and_asks_for_none_the_recorded_clients_did_not(
) {
    use auberean_dat::{QualifiedId, RecordType};
    use std::collections::BTreeMap;
    const DATA_REQUEST: u32 = 0x10034;
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let years = [2013, 2017];
    // What the recorded clients asked for, by capture.
    let mut retail = BTreeMap::<i64, Asks>::new();
    captures::messages_in_time_order(&index, &[DATA_REQUEST], &years, |m| {
        if let Ok(ClientMessage::DataRequest(r)) = ClientMessage::decode(&m.raw) {
            retail
                .entry(m.capture)
                .or_default()
                .entry((r.kind, r.id))
                .or_insert(m.time);
        }
    })
    .expect("the index reads");
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut ours = BTreeMap::<i64, Asks>::new();
    let mut named = auberean_glue::data_ids::Named::default();
    let mut take = |c: &Client, capture: i64, ours: &mut BTreeMap<i64, Asks>| {
        named.add(&c.states_named);
        for s in c.sent() {
            if let ClientMessage::DataRequest(r) = &s.message {
                ours.entry(capture)
                    .or_default()
                    .entry((r.kind, r.id))
                    .or_insert(s.at);
            }
        }
    };
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    captures::messages_in_time_order(&index, &types, &years, |m| {
        if m.capture != capture {
            take(&c, capture, &mut ours);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
            // Our client's files lack what the recorded client's lacked.
            let stores = c.data_download().cache().stores();
            for &(kind, id) in retail.get(&capture).map(|a| a.keys()).into_iter().flatten() {
                let _ = stores.purge_data(QualifiedId { id, kind });
            }
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    take(&c, capture, &mut ours);
    let count = |asks: &BTreeMap<i64, Asks>, kind: RecordType| -> usize {
        asks.values()
            .map(|a| a.keys().filter(|(k, _)| *k == kind.0).count())
            .sum()
    };
    let named_kinds = [
        RecordType::PICTURE,
        RecordType::MUSIC_INFO,
        RecordType::ENCODED_WAVE,
        RecordType::SCRIPT_STATE,
    ];
    for kind in named_kinds {
        eprintln!(
            "{:#x}: the recorded clients asked for {}, ours {}",
            kind.0,
            count(&retail, kind),
            count(&ours, kind)
        );
    }
    eprintln!("{named:?}");
    assert!(named.stopped.is_empty(), "{named:?}");
    assert_eq!(named.completed, named.asked, "{named:?}");
    assert!(named.asked >= 42_000, "{named:?}");
    assert!(named.records >= 15_000, "{named:?}");
    assert_eq!(
        (
            count(&retail, RecordType::PICTURE),
            count(&retail, RecordType::MUSIC_INFO)
        ),
        (12, 1)
    );
    for kind in named_kinds {
        assert_eq!(count(&ours, kind), 0, "{:#x}: {ours:x?}", kind.0);
    }
}

/// The world's region runs by its sky's calendar: the end-of-retail region's days and years; a
/// number with no region keeps the clock it had, or one with no calendar.
///
/// Behaviour: world.calendar.a-region-runs-by-its-skys-calendar
#[test]
fn a_region_runs_by_its_skys_calendar() {
    use auberean_client_runtime::game_time::region_clock;
    let data = data().expect("the September 2013 install opens");
    let c = client(&data);
    let cache = c.data_download().cache();
    let clock = region_clock(cache, 1, None);
    assert_eq!(
        (clock.day_length_seconds(), clock.days_per_year()),
        (7347.0, 360)
    );
    let none = region_clock(cache, 0x7FFF_FFFF, None);
    assert_eq!(none.days_per_year(), 1, "no calendar: one long day");
    let kept = region_clock(cache, 0x7FFF_FFFF, Some(clock));
    assert_eq!(kept.days_per_year(), 360, "the clock it had");
}

/// The enumerations the qualities are named by are there in the end-of-retail data and hold no
/// names, so a dump lists each value under an empty name, as the client does on that data.
///
/// Behaviour: script.model.the-end-of-retail-data-names-no-quality
#[test]
fn the_end_of_retail_data_names_no_quality() {
    let data = data().expect("the September 2013 install opens");
    let c = client(&data);
    let db = c.data_download().cache().database();
    for e in [
        0x4000_001F,
        0x4000_0020,
        0x4000_0021,
        0x4000_0022,
        0x4000_0023,
        0x4000_0024,
        0x4000_0025,
        0x4000_0026,
        0x4000_0046,
        0x4000_0050,
    ] {
        let did = db.data_id_by_enum(e, auberean_assets::database::ENUM_MAP_MAPPING);
        assert_ne!(did, 0, "{e:#x} has its record");
        assert_eq!(db.enum_map_len(did), 0, "{e:#x} holds no names");
    }
}

/// A script library read wrong once, over a file that did not change, is read again and links.
/// Behaviour: data.read.script-library-reads-are-checked
#[test]
fn a_script_library_read_wrong_once_is_read_again_and_links() {
    let dir = require_target_install();
    let found = install_dats(&dir).expect("the install");
    let (_, p) = found
        .iter()
        .find(|(n, _)| *n == DatName::Portal)
        .expect("a portal file");
    let portal = open_wrong_once(p, auberean_script::library::RECORD);
    let mut told = false;
    link_scripts_noting(&portal, |_| told = true).expect("the second read links");
    assert!(told, "the host is told it took a second read");
}

/// A collision handed to the scripts of what is in a recorded world runs to its end: in the
/// script library the interface's collision function is answered by a function that only says
/// it was handled, so nothing reads a collision's environment properties, and nothing stops.
///
/// Behaviour: script.model.a-collision-ends-in-the-scripts-without-reading-the-profile
#[test]
fn a_collision_handed_to_recorded_objects_scripts_runs_to_its_end() {
    use auberean_physics::collision::CollisionReport;
    use auberean_physics::pack::{Pack, Writer};

    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut c = client(&data);
    let mut ids = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    let mut ran = 0u32;
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            seen.clear();
        }
        if ran >= 400 {
            return;
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                c.act(Action::PlayCharacter {
                    character: p.character,
                });
            }
            return;
        }
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        c.take_message(m.blob_id, msg);
        c.model
            .player
            .filter(|_| ran < 400)
            .into_iter()
            .for_each(|_| {
                ids.clear();
                ids.extend(c.model.objects().map(|o| o.id).filter(|i| seen.insert(*i)));
            });
        for id in ids.drain(..) {
            let report = CollisionReport {
                impact_speed: 3.0,
                ..CollisionReport::default()
            };
            let mut w = Writer::new();
            report.pack(&mut w);
            let r = c.handle_collision(id, w.finish());
            assert!(
                matches!(r, Some(CallResult::Completed | CallResult::NoReceiver)),
                "object {id:#x}: {r:?}"
            );
            ran += u32::from(matches!(r, Some(CallResult::Completed)));
        }
    })
    .expect("the index reads");
    assert!(ran >= 100, "only {ran} collisions ran to their end");
}
