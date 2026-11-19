//! The toolbar as the shortcut bar shows it, asked of the scripts each time the recorded sessions'
//! options came again (the moment the bar fills its slots).

use auberean_client_contract::{Action, ToolbarShortcut};
use auberean_client_net::LocalTime;
use auberean_protocol::{ClientMessage, ServerMessage};
use auberean_script::options::kind;
use auberean_testkit::captures;

use crate::calls::{client, data, PLAY_CHARACTER};

/// The scripts' notice that the player's options came again.
const RELOAD_OPTIONS: u32 = 0x0085 << 16 | 0x0009;

/// Captures checked: enough sessions to see many players' shortcuts.
const CAPTURES: usize = 60;

/// Each time a recorded session's options came again, the toolbar asked of the scripts holds the
/// player's shortcuts slot for slot; every slot below the hundredth holding a skill gets the
/// skill's name and an icon, every empty one the same empty slot's tooltip and no icon; every
/// skill a slot holds is one the player has trained; the bar's height is 1 to 5 rows and the set
/// at most 10 (0 when the options never chose one: the bar then shows the first).
///
/// Behaviour: ui.shortcuts.the-toolbar-from-the-scripts-names-and-shows-each-slot
#[test]
fn the_toolbar_from_the_scripts_names_and_shows_each_slot() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut seen = 0usize;
    let mut checked = 0usize;
    let mut skill_slots = 0usize;
    let mut problems: Vec<String> = Vec::new();
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            seen += 1;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if seen > CAPTURES {
            return;
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
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        c.take_message(m.blob_id, msg);
        let reloaded = c.notices.iter().any(|n| n.native == RELOAD_OPTIONS);
        c.notices.clear();
        if !reloaded {
            return;
        }
        c.refresh_toolbar();
        let Some(shortcuts) = c.shortcuts() else {
            return;
        };
        let t = c.toolbar().clone();
        checked += 1;
        if t.shortcuts != shortcuts {
            problems.push(format!("capture {capture}: the toolbar's shortcuts differ"));
        }
        let empty: Vec<&str> = t
            .shortcuts
            .iter()
            .zip(&t.slots)
            .filter(|(s, _)| s.kind == 0)
            .map(|(_, i)| i.tooltip.as_str())
            .collect();
        if empty
            .first()
            .is_some_and(|f| f.is_empty() || empty.iter().any(|e| e != f))
        {
            problems.push(format!(
                "capture {capture}: empty slots' tooltips {empty:?}"
            ));
        }
        for (i, (s, info)) in t.shortcuts.iter().zip(&t.slots).enumerate() {
            match s.kind {
                kind::SKILL => {
                    skill_slots += 1;
                    if info.tooltip.is_empty() || info.icon == 0 {
                        problems.push(format!(
                            "capture {capture}: slot {i} (skill {:#x}) {info:?}",
                            s.data_enum
                        ));
                    }
                }
                0 if info.icon != 0 => {
                    problems.push(format!("capture {capture}: empty slot {i} has an icon"));
                }
                _ => {}
            }
        }
        if t.skills.iter().any(|k| !k.trained) {
            problems.push(format!("capture {capture}: a slot's skill is not trained"));
        }
        if !(1..=5).contains(&t.height) || t.set > 10 {
            problems.push(format!(
                "capture {capture}: height {} set {}",
                t.height, t.set
            ));
        }
    })
    .expect("the index reads");
    eprintln!("{checked} reloads checked, {skill_slots} skill slots");
    problems.truncate(40);
    assert!(checked >= 20, "only {checked} reloads of the options");
    assert!(skill_slots >= 100, "only {skill_slots} skill slots");
    assert!(problems.is_empty(), "{problems:#?}");
}

/// The client's log-off: the pack is looked at before it goes.
const LOG_OFF: u32 = 0x10027;

/// Items of the pack checked at each look.
const ITEMS_PER_LOOK: usize = 3;

/// What putting the pack's items on the bar found.
#[derive(Default, Debug)]
struct Shown {
    looks: u64,
    items: u64,
    /// Items the bar names or shows otherwise than the pack does, and a few of them.
    differ: u64,
    differs: Vec<String>,
}

impl Shown {
    /// Put the first items of the player's pack the client knows, one after another, in the bar's
    /// first empty slot, and compare what the bar is told of each with what the pack shows; the
    /// slot emptied again after.
    fn look(&mut self, c: &mut auberean_client_runtime::Client, capture: i64) {
        if c.model.player_object().is_none() {
            return;
        }
        c.refresh_inventory();
        c.refresh_toolbar();
        let inv = c.inventory().clone();
        let Some(pack) = inv.containers.get(&inv.player) else {
            return;
        };
        let Some(slot) = c
            .toolbar()
            .shortcuts
            .iter()
            .take(100)
            .position(|s| s.kind == 0)
        else {
            return;
        };
        let slot = u32::try_from(slot).unwrap_or(0);
        let items: Vec<u64> = pack
            .segments
            .first()
            .map(|s| s.items.clone())
            .unwrap_or_default()
            .into_iter()
            .filter(|id| c.model.object(*id).is_some() && inv.items.contains_key(id))
            .take(ITEMS_PER_LOOK)
            .collect();
        if items.is_empty() {
            return;
        }
        self.looks += 1;
        for id in items {
            c.act(Action::SetToolbarSlot {
                slot,
                shortcut: ToolbarShortcut {
                    kind: kind::ITEM,
                    item: id,
                    ..ToolbarShortcut::default()
                },
            });
            c.refresh_toolbar();
            self.items += 1;
            let on_bar = c
                .toolbar()
                .items
                .iter()
                .find(|i| i.item == id)
                .and_then(|i| i.here.clone());
            let in_pack = inv.items.get(&id).map(|f| (f.name.clone(), f.icon));
            if on_bar != in_pack || on_bar.as_ref().is_some_and(|(n, _)| n.contains("error")) {
                self.differ += 1;
                if self.differs.len() < 10 {
                    self.differs.push(format!(
                        "capture {capture}, item {id:#x}: bar {on_bar:?}, pack {in_pack:?}"
                    ));
                }
            }
        }
        c.act(Action::SetToolbarSlot {
            slot,
            shortcut: ToolbarShortcut::default(),
        });
        c.refresh_toolbar();
    }
}

/// At each recorded log-off (in the first sessions), each of the first items of the player's pack
/// put on an empty slot of the bar is named and shown on the bar as the pack shows it: its name
/// for its quantity and its icon, as the item's scripts give them.
///
/// Behaviour: ui.shortcuts.an-item-on-the-bar-is-named-and-shown-as-the-pack-shows-it
#[test]
fn an_item_on_the_bar_is_named_and_shown_as_the_pack_shows_it() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.extend([PLAY_CHARACTER, LOG_OFF]);
    let mut capture = -1;
    let mut seen = 0usize;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut shown = Shown::default();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            seen += 1;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if seen > CAPTURES {
            return;
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
                Ok(ClientMessage::LogOff(_)) => shown.look(&mut c, capture),
                _ => {}
            }
            return;
        }
        if let Ok(msg) = ServerMessage::decode(&m.raw) {
            c.take_message(m.blob_id, msg);
        }
    })
    .expect("the index reads");
    eprintln!("{shown:?}");
    assert!(shown.items >= 30, "{shown:?}");
    assert_eq!(shown.differ, 0, "{shown:?}");
}

/// What putting things that do not belong on the bar in an empty slot found.
#[derive(Default, Debug)]
struct Cleaned {
    looks: u64,
    /// Emotes and items the player does not have put on the bar, and how many were emptied.
    put: u64,
    emptied: u64,
    /// Skills of other players' bars put on this one: emptied (the toolbar no longer holds them),
    /// kept trained, and kept though not trained.
    skills_emptied: u64,
    skills_kept: u64,
    kept_untrained: u64,
    /// The player's own slots changed by the clean-up.
    own_changed: u64,
    problems: Vec<String>,
}

impl Cleaned {
    /// On the bar's first empty slot, one after another: an emote, an item the player does not
    /// have, and each skill another player's bar held that this one does not; each looked at
    /// after the toolbar is next asked, the slot emptied by hand after a skill that stayed.
    fn look(&mut self, c: &mut auberean_client_runtime::Client, capture: i64, others: &[u32]) {
        c.refresh_toolbar();
        let own = c.toolbar().shortcuts.clone();
        let Some(slot) = own.iter().take(100).position(|s| s.kind == 0) else {
            return;
        };
        let at = u32::try_from(slot).unwrap_or(0);
        self.looks += 1;
        let put = |c: &mut auberean_client_runtime::Client, shortcut: ToolbarShortcut| {
            c.act(Action::SetToolbarSlot { slot: at, shortcut });
            c.refresh_toolbar();
        };
        for (what, shortcut) in [
            (
                "an emote",
                ToolbarShortcut {
                    kind: kind::EMOTE,
                    data_enum: 0x4000_0001,
                    ..ToolbarShortcut::default()
                },
            ),
            (
                "an item the player does not have",
                ToolbarShortcut {
                    kind: kind::ITEM,
                    item: 0x7FFF_0000_0000_0001,
                    ..ToolbarShortcut::default()
                },
            ),
        ] {
            put(c, shortcut);
            self.put += 1;
            let held = c.shortcuts().and_then(|s| s.get(slot).map(|s| s.kind));
            if c.toolbar().shortcuts[slot].kind == 0 && held == Some(0) {
                self.emptied += 1;
            } else {
                self.problems
                    .push(format!("capture {capture}: {what} stayed in slot {slot}"));
            }
        }
        let mine: Vec<u32> = own
            .iter()
            .filter(|s| s.kind == kind::SKILL)
            .map(|s| s.data_enum)
            .collect();
        for skill in others.iter().filter(|k| !mine.contains(k)) {
            put(
                c,
                ToolbarShortcut {
                    kind: kind::SKILL,
                    data_enum: *skill,
                    ..ToolbarShortcut::default()
                },
            );
            let t = c.toolbar();
            if t.shortcuts[slot].kind == 0 {
                self.skills_emptied += 1;
                continue;
            }
            if t.skills.iter().any(|k| k.skill == *skill && !k.trained) {
                self.kept_untrained += 1;
            } else {
                self.skills_kept += 1;
            }
            put(c, ToolbarShortcut::default());
        }
        let now = &c.toolbar().shortcuts;
        let changed = own
            .iter()
            .zip(now)
            .enumerate()
            .filter(|(i, (a, b))| *i != slot && a != b)
            .count();
        self.own_changed += u64::try_from(changed).unwrap_or(0);
    }
}

/// At each recorded session's first options reload (the first sessions), the game takes off the
/// bar, as the toolbar is next asked, what does not belong on it: an emote, an item the player
/// does not have, a skill the player has not trained (each skill another player's bar held); a
/// skill the player has trained stays, and the player's own slots are left as they are.
///
/// Behaviour: ui.shortcuts.the-game-takes-off-the-bar-what-does-not-belong
#[test]
fn the_game_takes_off_the_bar_what_does_not_belong_on_it() {
    let data = data().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let mut types: Vec<u32> = (0x1_0000..0x1_0100)
        .filter(|t| ServerMessage::is_known_type(*t))
        .collect();
    types.push(PLAY_CHARACTER);
    let mut capture = -1;
    let mut seen = 0usize;
    let mut looked = false;
    let mut c = client(&data);
    let mut frame = f64::NEG_INFINITY;
    let mut others: Vec<u32> = Vec::new();
    let mut cleaned = Cleaned::default();
    captures::messages_in_time_order(&index, &types, &[2013, 2017], |m| {
        if m.capture != capture {
            capture = m.capture;
            seen += 1;
            looked = false;
            c = client(&data);
            frame = f64::NEG_INFINITY;
        }
        if seen > CAPTURES || looked {
            return;
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
        let Ok(msg) = ServerMessage::decode(&m.raw) else {
            return;
        };
        c.take_message(m.blob_id, msg);
        let reloaded = c.notices.iter().any(|n| n.native == RELOAD_OPTIONS);
        c.notices.clear();
        if !reloaded {
            return;
        }
        looked = true;
        cleaned.look(&mut c, capture, &others);
        for s in &c.toolbar().shortcuts {
            if s.kind == kind::SKILL && !others.contains(&s.data_enum) {
                others.push(s.data_enum);
            }
        }
    })
    .expect("the index reads");
    eprintln!("{cleaned:?}");
    assert!(cleaned.looks >= 20, "{cleaned:?}");
    assert_eq!(cleaned.emptied, cleaned.put, "{:#?}", cleaned.problems);
    assert!(
        cleaned.skills_emptied >= 10 && cleaned.skills_kept >= 1,
        "{cleaned:?}"
    );
    assert_eq!(cleaned.kept_untrained, 0, "{cleaned:?}");
    assert_eq!(cleaned.own_changed, 0, "{cleaned:?}");
}
