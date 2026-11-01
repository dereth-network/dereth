//! The story's acts as the quest window's story page lists them, asked of the scripts of every
//! recorded player when the player arrives and each time the scripts said the story changed.

use std::collections::BTreeMap;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::acts::UPDATE_STORY_QUESTS;
use auberean_client_runtime::CallResult;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// How the scripts say the whole story was given (no one act changed).
const WHOLE_STORY: u32 = 0x4000_0002;

/// Every recorded player's act list, asked as the story page asks for it, builds to its end: the
/// acts in order of their numbers, each act's scenes in order of theirs and each scene of that
/// act; and every act the scripts said changed is among them.
///
/// Behaviour: script.model.every-recorded-players-story-acts-build
#[test]
fn every_recorded_players_story_acts_build() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let (mut asked, mut players, mut acts, mut visible, mut scenes, mut playable) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let (mut changed, mut changed_found) = (0u64, 0u64);
    let mut results: BTreeMap<String, u64> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    let mut version = 0;
    let mut pending: Vec<u32> = Vec::new();
    let mut check = |c: &mut auberean_client_runtime::Client,
                     capture: i64,
                     version: &mut u64,
                     pending: &mut Vec<u32>| {
        for n in std::mem::take(&mut c.notices) {
            if n.native == UPDATE_STORY_QUESTS && n.words.first() != Some(&WHOLE_STORY) {
                pending.extend(n.words.get(1).copied().filter(|a| *a != 0));
            }
        }
        c.refresh_acts();
        let list = c.acts();
        let Some(r) = list.result.as_ref() else {
            return;
        };
        if list.version == *version && pending.is_empty() {
            return;
        }
        if *version == 0 || list.version != *version {
            asked += 1;
        }
        *version = list.version;
        *results.entry(format!("{r:?}")).or_default() += 1;
        if *r != CallResult::Completed {
            return;
        }
        for act in std::mem::take(pending) {
            changed += 1;
            if list.acts.iter().any(|a| a.id == act) {
                changed_found += 1;
            } else {
                problems.push(format!(
                    "capture {capture}: act {act:#x} changed, not listed"
                ));
            }
        }
        if !list.acts.is_sorted_by_key(|a| a.number) {
            problems.push(format!("capture {capture}: acts out of order"));
        }
        for a in &list.acts {
            if !a.scenes.is_sorted_by_key(|s| s.number) {
                problems.push(format!(
                    "capture {capture}: act {:#x}'s scenes out of order",
                    a.id
                ));
            }
            if a.scenes.iter().any(|s| s.act != a.id) {
                problems.push(format!(
                    "capture {capture}: act {:#x} holds another's scene",
                    a.id
                ));
            }
        }
    };
    let mut count = |c: &auberean_client_runtime::Client| {
        let list = c.acts();
        if list.result != Some(CallResult::Completed) {
            return;
        }
        players += 1;
        acts += list.acts.len() as u64;
        visible += list.acts.iter().filter(|a| a.visible != 0).count() as u64;
        for a in &list.acts {
            scenes += a.scenes.len() as u64;
            playable += a.scenes.iter().filter(|s| s.playable != 0).count() as u64;
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            check(&mut c, capture, &mut version, &mut pending);
            count(&c);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
            version = 0;
            pending.clear();
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
            check(&mut c, capture, &mut version, &mut pending);
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
    check(&mut c, capture, &mut version, &mut pending);
    count(&c);
    eprintln!(
        "{asked} act lists asked; at the sessions' ends {players} players, {acts} acts ({visible} \
         visible), {scenes} scenes ({playable} playable); {changed_found} of {changed} acts said \
         changed listed; results {results:?}"
    );
    problems.truncate(40);
    assert!(players >= 50, "only {players} players' act lists built");
    assert!(acts >= 100, "only {acts} acts listed");
    assert!(changed >= 10, "only {changed} acts said changed");
    assert_eq!(results.len(), 1, "{results:?}");
    assert!(problems.is_empty(), "{problems:#?}");
}
