//! The examination window's profiles as the recorded sessions' scripts handed them to the panels,
//! each effect line's own lines asked of the scripts as the window asks for them.

use std::collections::BTreeMap;

use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::examination::{kind, Profile, UPDATE_EXAMINATION_PROFILE};
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// Every profile the recorded sessions' scripts handed the examination window that has effect
/// lines comes with each effect line's own lines, made by the player's scripts for the line's
/// tabs; most have some, and every one of them shows on the line's tabs or the first tab.
///
/// Behaviour: script.model.an-examined-effect-line-brings-the-effects-own-lines
#[test]
fn an_examined_effect_line_brings_the_effects_own_lines() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let (mut profiles, mut with_effects, mut effect_lines, mut expanded, mut own_lines) =
        (0u64, 0u64, 0u64, 0u64, 0u64);
    let mut problems: Vec<String> = Vec::new();
    let mut kinds: BTreeMap<u32, u64> = BTreeMap::new();
    let mut take = |c: &mut auberean_client_runtime::Client, capture: i64| {
        for n in std::mem::take(&mut c.notices) {
            if n.native != UPDATE_EXAMINATION_PROFILE {
                continue;
            }
            for (j, o) in n.objects.iter().enumerate() {
                let Some(p) = o.as_ref().and_then(Profile::of) else {
                    continue;
                };
                profiles += 1;
                let effects: Vec<usize> = p
                    .nodes
                    .iter()
                    .enumerate()
                    .filter(|(_, l)| l.kind == kind::EFFECT)
                    .map(|(k, _)| k)
                    .collect();
                if effects.is_empty() {
                    continue;
                }
                with_effects += 1;
                effect_lines += effects.len() as u64;
                let made = n.effect_lines.get(j).cloned().unwrap_or_default();
                for (k, bytes) in &made {
                    let Ok(sub) = Profile::unpack(bytes) else {
                        problems.push(format!("capture {capture}: line {k} does not read"));
                        continue;
                    };
                    let line = &p.nodes[*k];
                    if !sub.nodes.is_empty() {
                        expanded += 1;
                    }
                    for s in &sub.nodes {
                        own_lines += 1;
                        *kinds.entry(s.kind).or_default() += 1;
                        if s.tabs & (line.tabs | 0x203) == 0 {
                            problems.push(format!(
                                "capture {capture}: a line on tabs {:#x} under one on {:#x}",
                                s.tabs, line.tabs
                            ));
                        }
                    }
                }
                if made.len() != effects.len() {
                    problems.push(format!(
                        "capture {capture}: {} of {} effect lines made",
                        made.len(),
                        effects.len()
                    ));
                }
            }
        }
    };
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            take(&mut c, capture);
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
            take(&mut c, capture);
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
    take(&mut c, capture);
    eprintln!(
        "{profiles} profiles, {with_effects} with effect lines ({effect_lines} lines, {expanded} \
         with lines of their own, {own_lines} lines in all: {kinds:x?})"
    );
    problems.truncate(40);
    assert!(
        with_effects >= 100,
        "only {with_effects} profiles with effect lines"
    );
    assert!(
        expanded * 10 >= effect_lines * 9,
        "{expanded} of {effect_lines} effect lines made lines"
    );
    assert!(problems.is_empty(), "{problems:#?}");
}
