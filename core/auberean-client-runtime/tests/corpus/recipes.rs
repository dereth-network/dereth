//! The recipes the recorded players put ingredients in, opened and read as the crafting page and
//! the ingredient panel do, held to the ingredients the recorded clients put in.

use std::collections::BTreeMap;

use auberean_client_contract::{Action, CraftAction};
use auberean_client_net::LocalTime;
use auberean_client_runtime::craft::notice::OPEN_INGREDIENT_PANEL;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::args::{self, Arg};
use auberean_testkit::captures::{self, carrier};

use crate::calls::{client, data, PLAY_CHARACTER};

/// The server's function an ingredient put in a slot is sent as.
const ADD_INGREDIENT: u32 = 0x8206_0006;

/// Before each ingredient a recorded client put in a slot, the recipe is opened as the crafting
/// page opens one (the player's scripts then open the ingredient panel on it) and read at the
/// spinner value the ingredient was sent with, as the ingredient panel reads it, every answer to
/// its end (a few sessions' scripts know no such recipe): the slot is one the recipe takes there,
/// and nearly every ingredient is an item the scripts say fits that slot (the server's word being
/// final).
///
/// Behaviour: script.model.the-ingredient-panel-reads-every-recorded-recipe
#[test]
fn the_ingredient_panel_reads_every_recorded_recipe() {
    let data = data().expect("the September 2013 install opens");
    let lib = &data.scripts.as_ref().expect("the scripts").1.library;
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, carrier::TO_SERVER]);
    let mut capture = -1;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut open_recipe = 0;
    let (mut opened, mut adds, mut read, mut add_slot, mut add_fits, mut slots) =
        (0u64, 0u64, 0u64, 0u64, 0u64, 0u64);
    let mut unread: BTreeMap<i64, u64> = BTreeMap::new();
    let mut incomplete: BTreeMap<String, u64> = BTreeMap::new();
    let mut problems: Vec<String> = Vec::new();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            c = client(&data);
            frame = f64::NEG_INFINITY;
            open_recipe = 0;
        }
        if m.time > frame {
            frame = m.time;
            c.set_time(LocalTime(m.time));
            c.upkeep();
            c.notices.clear();
        }
        if !m.to_client {
            if m.message_type == carrier::TO_SERVER {
                let Some(call) = m.script_call().filter(|s| s.function == ADD_INGREDIENT) else {
                    return;
                };
                let values = args::decode(&call.args, lib).unwrap_or_default();
                let words: Vec<u64> = values
                    .iter()
                    .map(|v| match v {
                        Arg::Int(w) | Arg::Float(w) => u64::from(*w),
                        Arg::LongInt(l) => *l,
                        _ => 0,
                    })
                    .collect();
                // Sent as the spinner's value, the item, the slot, the recipe.
                let [spinner, item, ordinal, recipe] = words[..] else {
                    problems.push(format!("capture {capture}: an add of {values:?}"));
                    return;
                };
                adds += 1;
                #[allow(clippy::cast_possible_truncation)]
                let (recipe, ordinal, spinner) = (recipe as u32, ordinal as u32, spinner as i32);
                if recipe != open_recipe {
                    c.notices.clear();
                    c.act(Action::Craft(CraftAction::Open(recipe)));
                    if c.notices.iter().any(|n| n.native == OPEN_INGREDIENT_PANEL) {
                        opened += 1;
                    }
                    open_recipe = recipe;
                }
                c.act(Action::Craft(CraftAction::Inquire { recipe, spinner }));
                c.refresh_recipe();
                for (f, why) in &c.craft().incomplete {
                    *incomplete.entry(format!("{f:#x} {why:?}")).or_default() += 1;
                }
                let Some(at) = c.craft().opened.as_ref() else {
                    *unread.entry(capture).or_default() += 1;
                    return;
                };
                read += 1;
                slots += at.ingredients.len() as u64;
                if at.mask & (1 << ordinal.saturating_sub(1)) != 0 {
                    add_slot += 1;
                }
                if at.fits.get(&(ordinal, item)).copied().unwrap_or(false) {
                    add_fits += 1;
                }
                return;
            }
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
    eprintln!(
        "{adds} ingredients put in: their recipes opened {opened} times, {read} read with {slots} \
         slots ({unread:?} unread by capture); {add_slot} of a slot taken, {add_fits} fitting; \
         incomplete: {incomplete:?}"
    );
    problems.truncate(20);
    assert!(adds >= 300, "only {adds} ingredients put in");
    assert!(opened >= 100, "only {opened} recipes opened");
    assert!(incomplete.is_empty(), "{incomplete:?}");
    assert!(problems.is_empty(), "{problems:#?}");
    assert!(read >= 550, "only {read} read");
    // A few sessions' scripts know no recipe the client put an ingredient in (48 in six).
    assert!(unread.values().sum::<u64>() <= 60, "{unread:?}");
    assert_eq!(add_slot, read, "every one of a slot the recipe takes");
    assert!(add_fits * 100 >= read * 98, "{add_fits} of {read} fitting");
}
