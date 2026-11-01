//! The crafting page's craft skills and recipes, asked of the scripts of every recorded player as
//! the page asks them.

use std::collections::BTreeMap;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// Every recorded player's crafting page, asked at the end of its session as the page asks: the
/// craft skills, each one's own adaptor (the same craft skill as the list's), level and whether it
/// may be raised, its recipes (each of that craft skill) and their categories' names; every call
/// runs to its end.
///
/// Behaviour: script.model.the-crafting-page-reads-every-recorded-players-craft-skills
#[test]
fn the_crafting_page_reads_every_recorded_players_craft_skills() {
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
    let (mut players, mut crafters, mut skills, mut recipes, mut shown, mut categories) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut incomplete: BTreeMap<String, u64> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    let mut ask = |c: &mut auberean_client_runtime::Client, entered: bool, capture: i64| {
        if !entered || c.model.player_object().is_none() {
            return;
        }
        c.refresh_craft();
        let k = c.craft();
        players += 1;
        if !k.skills.is_empty() {
            crafters += 1;
        }
        categories += k.categories.len() as u64;
        for s in &k.skills {
            skills += 1;
            if s.own.as_ref().map(|o| o.did) != Some(s.listed.did) {
                problems.push(format!("capture {capture}: {:#x} on its own", s.listed.did));
            }
            for r in &s.recipes {
                recipes += 1;
                shown += u64::from(r.shown);
                if r.craft_skill != s.listed.did {
                    problems.push(format!(
                        "capture {capture}: recipe {:#x} of {:#x} under {:#x}",
                        r.did, r.craft_skill, s.listed.did
                    ));
                }
            }
        }
        for (f, why) in &k.incomplete {
            *incomplete.entry(format!("{f:#x} {why:?}")).or_default() += 1;
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            ask(&mut c, entered, capture);
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
    ask(&mut c, entered, capture);
    eprintln!(
        "{players} players, {crafters} with craft skills; {skills} craft skills, {recipes} recipes \
         ({shown} shown), {categories} categories named; incomplete: {incomplete:?}"
    );
    problems.truncate(40);
    assert!(players > 0);
    assert!(crafters > 0, "no recorded player has a craft skill");
    assert!(recipes > 0);
    assert!(incomplete.is_empty(), "{incomplete:?}");
    assert!(problems.is_empty(), "{problems:#?}");
}
