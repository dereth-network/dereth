//! Auberean's own host operations: what a plugin reads of the game, and what it asks of it, each
//! request becoming the action the game's own interface gives the client for the same gesture.
//!
//! The operations every game shares are the interface's ([`dereth_plugin_api::Game::ops`]); these
//! are the game's, reached from Lua through Auberean's bindings (`crate::bindings`). A read answers
//! from the game's view ([`crate::reads`]). A request answers nothing: the client carries the
//! action out as the frame ends, and what follows arrives as events.

use auberean_client_contract::control::{Control, ControlId};
use auberean_client_contract::orbit::{MovementMode, OrbitSettings};
use auberean_client_contract::view::friend_mode;
use auberean_client_contract::{
    pet, shortcut_kind, Action, ChatWindowAction, CraftAction, PetAction, SelectKind, SocialAction,
    ToolbarShortcut, TradeAction,
};
use auberean_client_runtime::questions;
use dereth_plugin_api::schema::{Op, Param, Ty};
use dereth_plugin_api::Value;

use crate::events::parse_id;

const fn p(name: &'static str, ty: Ty) -> Param {
    Param { name, ty }
}

const OBJECT: Param = p("object", Ty::Str);

/// Auberean's own operations, in the order the spec lists them (`docs/plugins/auberean.md`).
#[rustfmt::skip]
pub const OPS: &[Op] = &[
    Op { name: "game.player", lua: Some("game.player"), params: &[], result: Some(Ty::Value),
         doc: "The player: its id, name, world, vitals and place; nil before its object is here." },
    Op { name: "game.objects", lua: Some("game.objects"), params: &[], result: Some(Ty::Value),
         doc: "The objects around the player, each as `game.object` gives it." },
    Op { name: "game.object", lua: Some("game.object"), params: &[OBJECT], result: Some(Ty::Value),
         doc: "One object around the player: its id, name, kind, place and what holds it; nil when it is not there." },
    Op { name: "game.selection", lua: Some("game.selection"), params: &[], result: Some(Ty::Value),
         doc: "What is selected (`current`), what was before (`last`) and `auto_target`." },
    Op { name: "game.inventory", lua: Some("game.inventory"), params: &[], result: Some(Ty::Value),
         doc: "The player's `money` and `items`." },
    Op { name: "game.effects", lua: Some("game.effects"), params: &[], result: Some(Ty::Value),
         doc: "The effects on the player, each with its seconds `remaining`." },
    Op { name: "game.skills", lua: Some("game.skills"), params: &[], result: Some(Ty::Value),
         doc: "The player's skills: trained or not, and their levels." },
    Op { name: "game.clock", lua: Some("game.clock"), params: &[], result: Some(Ty::Value),
         doc: "The game's `world_time` and `game_time`, seconds." },
    Op { name: "game.questions", lua: Some("game.questions"), params: &[], result: Some(Ty::Value),
         doc: "The game's questions open: each one's `id`, `kind`, `text`, whether it has `one_answer` and goes in front (`at_once`), and who it names." },
    Op { name: "game.place", lua: Some("game.place"), params: &[], result: Some(Ty::Value),
         doc: "Where the player stands, as the radar's place line gives it: `inside`, the map `coordinates` outdoors, `portal_space` and the `radar_range`." },
    Op { name: "game.social", lua: Some("game.social"), params: &[], result: Some(Ty::Value),
         doc: "The player's `fellowship`, `allegiance`, `friends`, own chat `state`, `kingdom` and the `conflict` the place allows." },
    Op { name: "game.journal", lua: Some("game.journal"), params: &[], result: Some(Ty::Value),
         doc: "The player's quests as the journal lists them, newest given first; nil before the player is here." },
    Op { name: "game.markers", lua: Some("game.markers"), params: &[], result: Some(Ty::Value),
         doc: "The markers on the map: each one's `kind`, `object`, `cell`, `picture` and `label`." },
    Op { name: "game.pets", lua: Some("game.pets"), params: &[], result: Some(Ty::Value),
         doc: "The player's pets in the order they came: each one's `id`, `name`, `icon` and `mode`." },
    Op { name: "game.toolbar", lua: Some("game.toolbar"), params: &[], result: Some(Ty::Value),
         doc: "The shortcut bar: the `set` shown, whether it is `locked`, its `numbers` and `rows`, and the `slots` that hold something." },
    Op { name: "game.craft", lua: Some("game.craft"), params: &[], result: Some(Ty::Value),
         doc: "The crafting page: the craft `experience`, the `skills` and their recipes, the `categories`, and the recipe `opened`." },
    Op { name: "game.select", lua: Some("act.select"), params: &[OBJECT], result: None,
         doc: "Select an object (nil: nothing), as a click on it in the world does." },
    Op { name: "game.select_next", lua: Some("act.select_next"), params: &[p("kind", Ty::Str), p("previous", Ty::Bool)], result: None,
         doc: "Select the next object of a kind around the player (`previous`: the one before), as the selection keys do." },
    Op { name: "game.select_nearest", lua: Some("act.select_nearest"), params: &[p("kind", Ty::Str)], result: None,
         doc: "Select the nearest object of a kind around the player." },
    Op { name: "game.select_self", lua: Some("act.select_self"), params: &[], result: None,
         doc: "Select the player (when it is selected already, the selection before)." },
    Op { name: "game.select_last", lua: Some("act.select_last"), params: &[], result: None,
         doc: "Select the selection before this one." },
    Op { name: "game.select_last_attacker", lua: Some("act.select_last_attacker"), params: &[], result: None,
         doc: "Select the last to attack the player." },
    Op { name: "game.look_at", lua: Some("act.look_at"), params: &[OBJECT], result: None,
         doc: "Turn the player's head toward an object (nil: straight ahead)." },
    Op { name: "game.use_selected", lua: Some("act.use_selected"), params: &[], result: None,
         doc: "Use what is selected, as a double click on it or the use key does." },
    Op { name: "game.use_slot", lua: Some("act.use_slot"), params: &[p("slot", Ty::Int)], result: None,
         doc: "Use what a shortcut slot holds, as a click on the slot does." },
    Op { name: "game.attack", lua: Some("act.attack"), params: &[p("start", Ty::Bool)], result: None,
         doc: "Start attacking what is selected, or stop." },
    Op { name: "game.cease_attack", lua: Some("act.cease_attack"), params: &[OBJECT], result: None,
         doc: "Stop attacking an object." },
    Op { name: "game.control", lua: Some("act.control"), params: &[p("control", Ty::Int), p("extent", Ty::Value)], result: None,
         doc: "Press a control of the game's action map (by its id) as a key bound to it would, at `extent` (true or 1: full), or let it go (false or 0)." },
    Op { name: "game.release_controls", lua: Some("act.release_controls"), params: &[p("typing", Ty::Bool)], result: None,
         doc: "Let go of every control in force (with `typing`, the modifier and interface controls stay)." },
    Op { name: "game.camera", lua: Some("act.camera"), params: &[p("settings", Ty::Opt(&Ty::Value))], result: None,
         doc: "Choose the orbit camera with these settings (`movement`: `camera` or `character`, `reverse_x`, `reverse_y`, `mouse_turn`, `key_turn`), or the game's own (nil)." },
    Op { name: "game.show_names", lua: Some("act.show_names"), params: &[p("on", Ty::Bool)], result: None,
         doc: "Show or hide the names over heads, kept with the player's options." },
    Op { name: "game.chat_window", lua: Some("chat.window"), params: &[p("window", Ty::Int), p("settings", Ty::Value)], result: None,
         doc: "Set a chat window's `opacity` (0 to 1), `filter` (the kinds of text it shows, a mask) or `devote` (the kind it is given over to)." },
    Op { name: "game.chat_kind", lua: Some("chat.kind"), params: &[p("kind", Ty::Int), p("settings", Ty::Value)], result: None,
         doc: "Set a kind of text's `size`, `color` or `popup` (whether it pops the chat up)." },
    Op { name: "game.chat_keep", lua: Some("chat.keep"), params: &[], result: None,
         doc: "Send the chat windows' settings to the server to keep." },
    Op { name: "game.fellowship_answer", lua: Some("social.answer_fellowship"), params: &[p("accept", Ty::Bool)], result: None,
         doc: "Take the place offered in a fellowship, or turn it down." },
    Op { name: "game.oath_answer", lua: Some("social.answer_oath"), params: &[p("accept", Ty::Bool)], result: None,
         doc: "Take the oath offered, or refuse it." },
    Op { name: "game.create_fellowship", lua: Some("social.create_fellowship"), params: &[p("name", Ty::Str), p("share_experience", Ty::Bool), p("looting", Ty::Int)], result: None,
         doc: "Make a fellowship with this name, sharing experience or not, sharing loot as `looting` says." },
    Op { name: "game.disband_fellowship", lua: Some("social.disband_fellowship"), params: &[], result: None,
         doc: "Break the player's fellowship up (its leader only)." },
    Op { name: "game.leave_fellowship", lua: Some("social.leave_fellowship"), params: &[], result: None,
         doc: "Leave the player's fellowship." },
    Op { name: "game.recruit", lua: Some("social.recruit"), params: &[OBJECT], result: None,
         doc: "Ask a player into the fellowship." },
    Op { name: "game.dismiss", lua: Some("social.dismiss"), params: &[OBJECT], result: None,
         doc: "Send a fellow out of the fellowship." },
    Op { name: "game.ask_allegiance", lua: Some("social.ask_allegiance"), params: &[OBJECT], result: None,
         doc: "Ask for the allegiance's profile about a member (nil: no longer)." },
    Op { name: "game.swear", lua: Some("social.swear"), params: &[], result: None,
         doc: "Swear an oath to the one selected." },
    Op { name: "game.break_oath", lua: Some("social.break_with"), params: &[OBJECT], result: None,
         doc: "Break with a patron or a vassal." },
    Op { name: "game.add_friend", lua: Some("social.add_friend"), params: &[p("name", Ty::Str)], result: None,
         doc: "Add a name to the friends." },
    Op { name: "game.remove_friend", lua: Some("social.remove_friend"), params: &[p("name", Ty::Str)], result: None,
         doc: "Take a name off the friends." },
    Op { name: "game.ask_friends", lua: Some("social.ask_friends"), params: &[], result: None,
         doc: "Ask the chat service for the friends again." },
    Op { name: "game.chat_state", lua: Some("social.set_state"), params: &[p("state", Ty::Value), p("location", Ty::Opt(&Ty::Str))], result: None,
         doc: "Set the player's own state on the chat service (`available`, `away` or `busy`) and where it is." },
    Op { name: "game.pet_attack", lua: Some("pets.attack"), params: &[OBJECT], result: None,
         doc: "Send the player's pets to attack an object." },
    Op { name: "game.pet_return", lua: Some("pets.come_back"), params: &[], result: None,
         doc: "Call the player's pets back." },
    Op { name: "game.pet_buff", lua: Some("pets.buff"), params: &[], result: None,
         doc: "Have the player's pets strengthen the player." },
    Op { name: "game.pet_heal", lua: Some("pets.heal"), params: &[], result: None,
         doc: "Have the player's pets heal the player." },
    Op { name: "game.pet_mode", lua: Some("pets.mode"), params: &[OBJECT, p("mode", Ty::Value)], result: None,
         doc: "Set a pet's mode, by its word (`aggressive`, `idle` and the rest) or its number." },
    Op { name: "game.pet_name", lua: Some("pets.rename"), params: &[OBJECT, p("name", Ty::Str)], result: None,
         doc: "Give a pet a name." },
    Op { name: "game.toolbar_set", lua: Some("toolbar.show_set"), params: &[p("set", Ty::Int)], result: None,
         doc: "Show another set of ten slots (from 1)." },
    Op { name: "game.toolbar_slot", lua: Some("toolbar.put"), params: &[p("slot", Ty::Int), p("shortcut", Ty::Opt(&Ty::Value))], result: None,
         doc: "Put a shortcut in a slot (as `game.toolbar` gives one: `kind` and what it names), or empty it (nil)." },
    Op { name: "game.craft_open", lua: Some("craft.open"), params: &[p("recipe", Ty::Int)], result: None,
         doc: "Open a recipe in the making window." },
    Op { name: "game.craft_read", lua: Some("craft.read"), params: &[p("recipe", Ty::Int), p("spinner", Ty::Int)], result: None,
         doc: "Read a recipe at a spinner value, as the making window does (recipe 0: the window is closed)." },
    Op { name: "game.craft_raise", lua: Some("craft.raise"), params: &[p("skill", Ty::Int)], result: None,
         doc: "Raise a craft skill." },
    Op { name: "game.craft_add", lua: Some("craft.add"), params: &[p("recipe", Ty::Int), p("slot", Ty::Int), OBJECT, p("spinner", Ty::Int)], result: None,
         doc: "Put an item in a recipe's ingredient slot, at a spinner value." },
    Op { name: "game.craft_make", lua: Some("craft.make"), params: &[p("recipe", Ty::Int), p("spinner", Ty::Int), p("ingredients", Ty::Value)], result: None,
         doc: "Make a recipe at a spinner value from the items in its slots (a list of `{ slot, item }`)." },
    Op { name: "game.answer", lua: Some("act.answer"), params: &[p("question", Ty::Int), p("yes", Ty::Bool)], result: None,
         doc: "Answer one of the game's open questions, yes (true) or no; one with one answer takes either." },
    Op { name: "game.trade", lua: Some("trade.ask"), params: &[p("what", Ty::Str), p("object", Ty::Opt(&Ty::Str))], result: None,
         doc: "Ask something of a trade: `open` (with the partner who asked), `accept`, `decline` or `close`." },
];

/// Why an operation's arguments are wrong.
fn bad(s: impl Into<String>) -> String {
    s.into()
}

fn object(args: &[Value], i: usize) -> Result<u64, String> {
    match args.get(i) {
        None | Some(Value::Nil) => Ok(0),
        Some(v) => parse_id(v).ok_or_else(|| bad("an object is named by its id (\"0x…\")")),
    }
}

fn whole(args: &[Value], i: usize, what: &str) -> Result<u32, String> {
    args.get(i)
        .and_then(Value::as_i64)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| bad(format!("{what} must be a whole number")))
}

fn flag(args: &[Value], i: usize) -> bool {
    args.get(i).and_then(Value::as_bool).unwrap_or(true)
}

/// The actions operation `op` asks for with `args`; `None` when `op` is none of [`OPS`].
///
/// # Errors
/// The arguments are wrong: the text says why.
pub fn actions(op: &str, args: &[Value]) -> Option<Result<Vec<Action>, String>> {
    let one = |a: Action| Ok(vec![a]);
    Some(match op {
        "game.select" => object(args, 0).and_then(|target| one(Action::Select { target })),
        "game.look_at" => object(args, 0).and_then(|target| one(Action::LookAt { target })),
        "game.use_selected" => one(Action::UseSelected),
        "game.use_slot" => {
            whole(args, 0, "a slot").and_then(|slot| one(Action::UseToolbarSlot { slot }))
        }
        "game.attack" => one(Action::Attack {
            start: flag(args, 0),
        }),
        "game.cease_attack" => {
            object(args, 0).and_then(|target| one(Action::CeaseAttack { target }))
        }
        "game.show_names" => one(Action::ShowObjectNames { on: flag(args, 0) }),
        "game.control" => control(args),
        "game.release_controls" => one(Action::ReleaseControls {
            typing: args.first().and_then(Value::as_bool).unwrap_or(false),
        }),
        "game.chat_window" => chat_window(args),
        "game.chat_kind" => chat_kind(args),
        "game.chat_keep" => one(Action::ChatWindows(ChatWindowAction::Keep)),
        // As the game's own interface answers them: turned down with its reasons.
        "game.fellowship_answer" => {
            one(Action::Social(questions::fellowship_answer(flag(args, 0))))
        }
        "game.oath_answer" => one(Action::Social(questions::oath_answer(flag(args, 0)))),
        "game.trade" => trade(args),
        "game.select_next" => select_kind(args, 0).and_then(|kind| {
            one(Action::SelectCycle {
                kind,
                previous: args.get(1).and_then(Value::as_bool).unwrap_or(false),
            })
        }),
        "game.select_nearest" => {
            select_kind(args, 0).and_then(|kind| one(Action::SelectNearest { kind }))
        }
        "game.select_self" => one(Action::SelectSelf),
        "game.select_last" => one(Action::SelectLast),
        "game.select_last_attacker" => one(Action::SelectLastAttacker),
        "game.create_fellowship" => text(args, 0, "a fellowship's name").and_then(|name| {
            let looting = match args.get(2) {
                None | Some(Value::Nil) => 0,
                _ => whole(args, 2, "looting")?,
            };
            one(Action::Social(SocialAction::CreateFellowship {
                name: name.encode_utf16().collect(),
                social: args.get(1).and_then(Value::as_bool).unwrap_or(true),
                looting,
            }))
        }),
        "game.disband_fellowship" => one(Action::Social(SocialAction::DisbandFellowship)),
        "game.leave_fellowship" => one(Action::Social(SocialAction::LeaveFellowship)),
        "game.recruit" => {
            someone(args, 0).and_then(|id| one(Action::Social(SocialAction::Recruit(id))))
        }
        "game.dismiss" => {
            someone(args, 0).and_then(|id| one(Action::Social(SocialAction::Dismiss(id))))
        }
        "game.ask_allegiance" => {
            object(args, 0).and_then(|id| one(Action::Social(SocialAction::QueryAllegiance(id))))
        }
        "game.swear" => one(Action::Social(SocialAction::Swear)),
        "game.break_oath" => {
            someone(args, 0).and_then(|id| one(Action::Social(SocialAction::Break(id))))
        }
        "game.add_friend" => text(args, 0, "a friend's name").and_then(|n| {
            one(Action::Social(SocialAction::AddFriend(
                n.encode_utf16().collect(),
            )))
        }),
        "game.remove_friend" => text(args, 0, "a friend's name").and_then(|n| {
            one(Action::Social(SocialAction::RemoveFriend(
                n.encode_utf16().collect(),
            )))
        }),
        "game.ask_friends" => one(Action::Social(SocialAction::GetFriends)),
        "game.chat_state" => chat_state(args),
        "game.pet_attack" => {
            someone(args, 0).and_then(|id| one(Action::Pet(PetAction::Attack(id))))
        }
        "game.pet_return" => one(Action::Pet(PetAction::Return)),
        "game.pet_buff" => one(Action::Pet(PetAction::Buff)),
        "game.pet_heal" => one(Action::Pet(PetAction::Heal)),
        "game.pet_mode" => someone(args, 0).and_then(|pet| {
            let mode = pet_mode(args.get(1))?;
            one(Action::Pet(PetAction::Mode { pet, mode }))
        }),
        "game.pet_name" => someone(args, 0).and_then(|pet| {
            let name = text(args, 1, "a pet's name")?.to_owned();
            one(Action::Pet(PetAction::Name { pet, name }))
        }),
        "game.toolbar_set" => whole(args, 0, "a set").and_then(|set| {
            if (1..=SETS).contains(&set) {
                one(Action::ToolbarSet { set })
            } else {
                Err(bad(format!("a set is 1 to {SETS}")))
            }
        }),
        "game.toolbar_slot" => toolbar_slot(args),
        "game.craft_open" => {
            whole(args, 0, "a recipe").and_then(|r| one(Action::Craft(CraftAction::Open(r))))
        }
        "game.craft_read" => whole(args, 0, "a recipe").and_then(|recipe| {
            let spinner = signed(args, 1, "a spinner value")?;
            one(Action::Craft(CraftAction::Inquire { recipe, spinner }))
        }),
        "game.craft_raise" => whole(args, 0, "a craft skill")
            .and_then(|s| one(Action::Craft(CraftAction::RaiseCraftSkill(s)))),
        "game.craft_add" => whole(args, 0, "a recipe").and_then(|recipe| {
            let ordinal = whole(args, 1, "a slot")?;
            let item = someone(args, 2)?;
            let spinner = signed(args, 3, "a spinner value")?;
            one(Action::Craft(CraftAction::AddIngredient {
                recipe,
                ordinal,
                item,
                spinner,
            }))
        }),
        "game.craft_make" => craft_make(args),
        "game.answer" => whole(args, 0, "a question").and_then(|question| {
            one(Action::AnswerQuestion {
                question,
                yes: flag(args, 1),
            })
        }),
        _ => return None,
    })
}

/// The sets of ten slots the shortcut bar shows.
const SETS: u32 = auberean_client_contract::TOOLBAR_SLOTS / 10;

/// An object that must be named: its id.
fn someone(args: &[Value], i: usize) -> Result<u64, String> {
    match object(args, i)? {
        0 => Err(bad("an object is named by its id (\"0x…\")")),
        id => Ok(id),
    }
}

fn text<'a>(args: &'a [Value], i: usize, what: &str) -> Result<&'a str, String> {
    args.get(i)
        .and_then(Value::as_str)
        .ok_or_else(|| bad(format!("{what} is text")))
}

fn signed(args: &[Value], i: usize, what: &str) -> Result<i32, String> {
    match args.get(i) {
        None | Some(Value::Nil) => Ok(0),
        Some(v) => v
            .as_i64()
            .and_then(|n| i32::try_from(n).ok())
            .ok_or_else(|| bad(format!("{what} must be a whole number"))),
    }
}

/// The kinds of object the selection keys select among, by the words a plugin gives them.
pub const SELECT_KINDS: [(&str, SelectKind); 6] = [
    ("creature", SelectKind::Creature),
    ("item", SelectKind::Item),
    ("player", SelectKind::Player),
    ("foe", SelectKind::Foe),
    ("fellow", SelectKind::Fellow),
    ("pet", SelectKind::Pet),
];

fn select_kind(args: &[Value], i: usize) -> Result<SelectKind, String> {
    let word = args.get(i).and_then(Value::as_str).unwrap_or_default();
    SELECT_KINDS
        .iter()
        .find(|(w, _)| *w == word)
        .map(|(_, k)| *k)
        .ok_or_else(|| {
            let words: Vec<&str> = SELECT_KINDS.iter().map(|(w, _)| *w).collect();
            bad(format!("a kind to select is one of {}", words.join(", ")))
        })
}

/// A pet's mode, by its word or its number.
fn pet_mode(v: Option<&Value>) -> Result<u32, String> {
    let named = v.and_then(Value::as_str).and_then(|w| {
        pet::MODES
            .iter()
            .copied()
            .find(|m| crate::reads::pet_mode_word(*m) == Some(w))
    });
    let numbered = v
        .and_then(Value::as_i64)
        .and_then(|n| u32::try_from(n).ok())
        .filter(|m| pet::MODES.contains(m));
    named
        .or(numbered)
        .ok_or_else(|| bad("a pet's mode is one of its words (aggressive, idle and the rest)"))
}

fn chat_state(args: &[Value]) -> Result<Vec<Action>, String> {
    let mode = match args.first() {
        Some(Value::Str(w)) => match w.as_str() {
            "available" => friend_mode::AVAILABLE,
            "away" => friend_mode::AWAY,
            "busy" => friend_mode::BUSY,
            _ => return Err(bad("a state is available, away or busy")),
        },
        _ => whole(args, 0, "a state")?,
    };
    let location = args.get(1).and_then(Value::as_str).unwrap_or_default();
    Ok(vec![Action::Social(SocialAction::SetState {
        mode,
        location: location.encode_utf16().collect(),
    })])
}

fn toolbar_slot(args: &[Value]) -> Result<Vec<Action>, String> {
    let slot = whole(args, 0, "a slot")?;
    if slot >= auberean_client_contract::TOOLBAR_SLOTS {
        return Err(bad(format!(
            "a slot is 0 to {}",
            auberean_client_contract::TOOLBAR_SLOTS - 1
        )));
    }
    let shortcut = match args.get(1) {
        None | Some(Value::Nil) => ToolbarShortcut::default(),
        Some(v) => shortcut(v)?,
    };
    Ok(vec![Action::SetToolbarSlot { slot, shortcut }])
}

/// A shortcut as a plugin gives it: its `kind` and what the kind names.
fn shortcut(v: &Value) -> Result<ToolbarShortcut, String> {
    let kind = v.get("kind").as_str().unwrap_or_default();
    let field = |key: &str, what: &str| whole(std::slice::from_ref(v.get(key)), 0, what);
    Ok(match kind {
        "skill" => ToolbarShortcut {
            kind: shortcut_kind::SKILL,
            data_enum: field("skill", "a skill")?,
            ..ToolbarShortcut::default()
        },
        "item" => ToolbarShortcut {
            kind: shortcut_kind::ITEM,
            item: someone(std::slice::from_ref(v.get("item")), 0)?,
            ..ToolbarShortcut::default()
        },
        "text" => ToolbarShortcut {
            kind: shortcut_kind::TEXT,
            text: text(std::slice::from_ref(v.get("text")), 0, "a shortcut's text")?
                .encode_utf16()
                .collect(),
            ..ToolbarShortcut::default()
        },
        "emote" => ToolbarShortcut {
            kind: shortcut_kind::EMOTE,
            data_enum: field("emote", "an emote")?,
            ..ToolbarShortcut::default()
        },
        "recipe" => ToolbarShortcut {
            kind: shortcut_kind::RECIPE,
            data_id: field("recipe", "a recipe")?,
            ..ToolbarShortcut::default()
        },
        _ => {
            return Err(bad(
                "a shortcut's kind is skill, item, text, emote or recipe",
            ))
        }
    })
}

fn craft_make(args: &[Value]) -> Result<Vec<Action>, String> {
    let recipe = whole(args, 0, "a recipe")?;
    let spinner = signed(args, 1, "a spinner value")?;
    let list = match args.get(2) {
        None | Some(Value::Nil) => &[][..],
        Some(v) => v
            .as_list()
            .ok_or_else(|| bad("the ingredients are a list of { slot, item }"))?,
    };
    let mut ingredients = Vec::new();
    for i in list {
        let slot = whole(
            std::slice::from_ref(i.get("slot")),
            0,
            "an ingredient's slot",
        )?;
        let item = someone(std::slice::from_ref(i.get("item")), 0)?;
        ingredients.push((slot, item));
    }
    Ok(vec![Action::Craft(CraftAction::ExecuteCraft {
        recipe,
        spinner,
        ingredients,
    })])
}

/// The camera a plugin chose: the orbit camera with its settings (each left out as the default,
/// the turning rates held to their ranges), or the game's own (nil).
///
/// # Errors
/// The settings are not a table, or a setting is wrong: the text says why.
pub fn camera(args: &[Value]) -> Result<Option<OrbitSettings>, String> {
    let v = match args.first() {
        None | Some(Value::Nil) => return Ok(None),
        Some(v) => v,
    };
    if v.as_map().is_none() {
        return Err(bad(
            "the camera's settings are a table, or nil for the game's own",
        ));
    }
    let mut s = OrbitSettings::default();
    match v.get("movement") {
        Value::Nil => {}
        m => {
            s.movement = match m.as_str() {
                Some("camera") => MovementMode::Camera,
                Some("character") => MovementMode::Character,
                _ => return Err(bad("the movement is camera or character")),
            }
        }
    }
    s.reverse_x = v.get("reverse_x").as_bool().unwrap_or(false);
    s.reverse_y = v.get("reverse_y").as_bool().unwrap_or(false);
    #[allow(clippy::cast_possible_truncation)]
    {
        if let Some(t) = v.get("mouse_turn").as_f64() {
            s.mouse_turn = t as f32;
        }
        if let Some(t) = v.get("key_turn").as_f64() {
            s.key_turn = t as f32;
        }
    }
    Ok(Some(s.held_to_ranges()))
}

fn control(args: &[Value]) -> Result<Vec<Action>, String> {
    let id = ControlId(whole(args, 0, "a control")?);
    #[allow(clippy::cast_possible_truncation)]
    let extent = match args.get(1) {
        None | Some(Value::Nil) => 1.0,
        Some(Value::Bool(on)) => f32::from(u8::from(*on)),
        Some(v) => v
            .as_f64()
            .filter(|e| e.is_finite() && *e >= 0.0)
            .ok_or_else(|| bad("a control's extent is true, false or a number from 0"))?
            as f32,
    };
    Ok(vec![Action::Control(Control::at(id, extent))])
}

fn settings(args: &[Value]) -> Result<&std::collections::BTreeMap<String, Value>, String> {
    args.get(1)
        .and_then(Value::as_map)
        .ok_or_else(|| bad("the settings are a table"))
}

fn chat_window(args: &[Value]) -> Result<Vec<Action>, String> {
    let window = whole(args, 0, "a chat window")?;
    let mut out = Vec::new();
    for (key, v) in settings(args)? {
        let a = match key.as_str() {
            #[allow(clippy::cast_possible_truncation)]
            "opacity" => ChatWindowAction::Opacity {
                window,
                opacity: v
                    .as_f64()
                    .filter(|o| (0.0..=1.0).contains(o))
                    .ok_or_else(|| bad("a window's opacity is 0 to 1"))?
                    as f32,
            },
            "filter" => ChatWindowAction::Filter {
                window,
                filter: whole(std::slice::from_ref(v), 0, "a filter")?,
            },
            "devote" => ChatWindowAction::Devote {
                window,
                kind: whole(std::slice::from_ref(v), 0, "a kind of text")?,
            },
            other => return Err(bad(format!("a chat window has no setting {other:?}"))),
        };
        out.push(Action::ChatWindows(a));
    }
    Ok(out)
}

fn chat_kind(args: &[Value]) -> Result<Vec<Action>, String> {
    let kind = whole(args, 0, "a kind of text")?;
    let mut out = Vec::new();
    for (key, v) in settings(args)? {
        let one = std::slice::from_ref(v);
        let a = match key.as_str() {
            "size" => ChatWindowAction::FontSize {
                kind,
                size: whole(one, 0, "a size")?,
            },
            "color" => ChatWindowAction::FontColor {
                kind,
                color: whole(one, 0, "a color")?,
            },
            "popup" => ChatWindowAction::Popup {
                kind,
                on: v.as_bool().ok_or_else(|| bad("popup is true or false"))?,
            },
            other => return Err(bad(format!("a kind of text has no setting {other:?}"))),
        };
        out.push(Action::ChatWindows(a));
    }
    Ok(out)
}

fn trade(args: &[Value]) -> Result<Vec<Action>, String> {
    let what = args
        .first()
        .and_then(Value::as_str)
        .ok_or_else(|| bad("trade.ask(what): open, accept, decline or close"))?;
    let a = match what {
        "open" => {
            let partner = object(args, 1)?;
            if partner == 0 {
                return Err(bad("a trade opens with a partner"));
            }
            TradeAction::Open(partner)
        }
        "accept" => TradeAction::Accept,
        "decline" => TradeAction::Decline,
        "close" => TradeAction::Close,
        other => return Err(bad(format!("no trade request {other:?}"))),
    };
    Ok(vec![Action::Trade(a)])
}
