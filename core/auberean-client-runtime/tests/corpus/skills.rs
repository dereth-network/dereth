//! The skill window's panels and skills, asked of the scripts of every recorded player as the
//! skill window asks them.

use std::collections::BTreeMap;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// Every recorded player's skill window, asked at the end of its session as the window asks: the
/// root panel of each kind of skill its race has, each root panel's name, icon and nodes, and
/// every skill of them, each answer through the player's scripts and the skill panel manager's;
/// every call runs to its end.
///
/// Behaviour: script.model.the-skill-window-reads-every-recorded-players-panels
#[test]
fn the_skill_window_reads_every_recorded_players_panels() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut entered = false;
    let (mut players, mut panels, mut nodes, mut skills) = (0u64, 0u64, 0u64, 0u64);
    let mut incomplete: BTreeMap<String, u64> = BTreeMap::new();
    let mut ask = |c: &mut auberean_client_runtime::Client, entered: bool| {
        if !entered || c.model.player_object().is_none() {
            return;
        }
        players += 1;
        c.ask_skills(None);
        let roots: Vec<u32> = c.skills.roots.iter().flatten().copied().collect();
        for r in roots {
            c.ask_skills(Some(r));
            panels += 1;
            let p = c.skills.panel.as_ref().expect("the panel asked");
            nodes += p.nodes.len() as u64;
            skills += c.skills.states.len() as u64;
            for (f, why) in &c.skills.incomplete {
                *incomplete.entry(format!("{f:#x} {why:?}")).or_default() += 1;
            }
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            ask(&mut c, entered);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
            entered = false;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
        }
        if !m.to_client {
            if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                entered = true;
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
    ask(&mut c, entered);
    eprintln!(
        "{players} players; {panels} root panels, {nodes} nodes, {skills} skills; incomplete: {incomplete:?}"
    );
    assert!(players > 0);
    assert!(nodes > 0);
    assert!(incomplete.is_empty(), "{incomplete:?}");
}

/// When a recorded player arrives in the world, the client asks its scripts what the skill window
/// shows, whatever the scripts said before (the race's root panels among it, as the window asks for
/// them when the play screen's panels are made), once for that player: every player whose root
/// panels the scripts know by the end of the run has them known as it arrives. The first forty
/// recorded runs that enter the world.
///
/// Behaviour: script.model.the-skill-windows-root-panels-are-known-when-the-player-arrives
#[test]
fn the_skill_windows_root_panels_are_known_when_the_player_arrives() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let runs: Vec<i64> = {
        let con = captures::open(&index).expect("the index opens");
        let mut q = con
            .prepare(&format!(
                "SELECT DISTINCT s.capture_id FROM message m JOIN session s USING (session_id) \
                 WHERE m.status = 'ac2' AND m.mtype = {PLAY_CHARACTER} ORDER BY s.capture_id LIMIT 40"
            ))
            .expect("the runs that enter");
        q.query_map([], |r| r.get(0))
            .expect("the runs read")
            .flatten()
            .collect()
    };
    let known = |c: &auberean_client_runtime::Client| c.skills.roots.iter().any(Option::is_some);
    let (mut players, mut at_arrival, mut at_end) = (0u64, 0u64, 0u64);
    for run in runs {
        let mut c = client(&data);
        let mut frame = f64::NEG_INFINITY;
        let (mut entered, mut arrived) = (false, None);
        captures::capture_by_id_in_time_order(&index, &types, run, |m| {
            if m.time > frame {
                frame = m.time;
                c.set_time(LocalTime(m.time));
                c.upkeep();
            }
            if !m.to_client {
                if let Ok(ClientMessage::PlayCharacter(p)) = ClientMessage::decode(&m.raw) {
                    entered = true;
                    c.act(Action::PlayCharacter {
                        character: p.character,
                    });
                }
                return;
            }
            if let Ok(msg) = ServerMessage::decode(&m.raw) {
                c.take_message(m.blob_id, msg);
            }
            if entered && arrived.is_none() && c.model.player_object().is_some() {
                let before = c.skills.version;
                c.refresh_skills();
                let asked = c.skills.version;
                assert_eq!(asked, before + 1, "run {run}: asked as the player arrives");
                c.refresh_skills();
                assert_eq!(c.skills.version, asked, "run {run}: once for that player");
                arrived = Some(known(&c));
            }
        })
        .expect("the run reads");
        let Some(first) = arrived else { continue };
        players += 1;
        c.ask_skills(None);
        let last = known(&c);
        assert!(
            first || !last,
            "run {run}: the root panels known at its end were known as it arrived"
        );
        at_arrival += u64::from(first);
        at_end += u64::from(last);
    }
    eprintln!("{players} players arrived; root panels known for {at_arrival} as they arrived, {at_end} at the end");
    assert!(at_arrival > 0);
}
