//! What a plugin reads of the game: the parts of the client's view ([`View`]) as plain values.
//!
//! The view is the one the game's interfaces read, as the host last refreshed it, so a plugin
//! reads what the player's screen would show. Ids are text ([`id_text`]); a text the view gives
//! only as a string reference is written out when the reference is a literal, and otherwise left
//! out (its `name_ref` says which string it is, for a plugin that waits for the string tables).

use auberean_client_contract::notice::{Allegiance, Fellow, Fellowship, Member, Quest};
use auberean_client_contract::question::Asked;
use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::{conflict, kingdom, Thing, View};
use auberean_client_contract::{pet, shortcut_kind};
use dereth_plugin_api::Value;

use crate::events::id_text;

/// A string reference's text when it is a literal; its string and table otherwise.
pub(crate) fn text_ref(map: Value, key: &str, r: &TextRef) -> Value {
    match &r.literal {
        Some(units) => map.with(key, String::from_utf16_lossy(units)),
        None if r.string != 0 => map.with(
            &format!("{key}_ref"),
            Value::map().with("string", r.string).with("table", r.table),
        ),
        None => map,
    }
}

/// [`text_ref`] for a reference the game may not have given.
pub(crate) fn maybe_text(map: Value, key: &str, r: Option<&TextRef>) -> Value {
    match r {
        Some(r) => text_ref(map, key, r),
        None => map,
    }
}

/// A count too large for a plugin's whole number held at the largest it holds.
pub(crate) fn big(n: u64) -> Value {
    Value::Int(i64::try_from(n).unwrap_or(i64::MAX))
}

/// An id that may be 0 for none: its text, or nil.
pub(crate) fn some_id(id: u64) -> Value {
    Value::from((id != 0).then(|| id_text(id)))
}

pub(crate) fn place(cell: u32, at: [f32; 3]) -> Value {
    Value::map()
        .with("cell", cell)
        .with("x", at[0])
        .with("y", at[1])
        .with("z", at[2])
}

/// The player: `id`, `name`, `world`, `race`, the vitals (`health`, `vigor`, `max_health`,
/// `max_vigor`), its `place` (with its `heading`), whether it is `moving` and whether it is
/// `mounted`; nil before its object is here.
#[must_use]
pub fn player(view: &View) -> Value {
    let Some(p) = &view.player else {
        return Value::Nil;
    };
    let mut v = Value::map()
        .with("id", id_text(p.id))
        .with("name", p.name.clone())
        .with("race", p.race)
        .with("moving", p.body.moving)
        .with("mounted", p.mounted)
        .with(
            "place",
            place(p.body.cell, p.body.origin).with("heading", p.body.heading),
        );
    if !p.world_name.is_empty() {
        v = v.with("world", p.world_name.as_str());
    }
    if let Some([health, max_health, vigor, max_vigor]) = p.vitals {
        v = v
            .with("health", health)
            .with("vigor", vigor)
            .with("max_health", max_health)
            .with("max_vigor", max_vigor);
    }
    v
}

/// One object around the player.
fn thing(t: &Thing) -> Value {
    let mut v = Value::map()
        .with("id", id_text(t.id))
        .with("kind", t.kind)
        .with("data_id", t.data_id)
        .with("selectable", t.selectable)
        .with("dead", t.dead)
        .with("fellow", t.fellow)
        .with("attackable", t.attackable)
        .with("parent", t.parent.map(id_text))
        .with("owner", t.owner.map(id_text))
        .with("container", t.container.map(id_text))
        .with("place", t.place.map(|(cell, at)| place(cell, at)));
    v = match &t.name {
        Some(n) => v.with("name", n.as_str()),
        None => text_ref(v, "name", &t.own_name),
    };
    v
}

/// The objects around the player, in the view's order.
#[must_use]
pub fn objects(view: &View) -> Value {
    view.world.as_ref().map_or(Value::List(Vec::new()), |w| {
        Value::List(w.things.iter().map(thing).collect())
    })
}

/// One object around the player, by id; nil when it is not there.
#[must_use]
pub fn object(view: &View, id: u64) -> Value {
    view.world
        .as_ref()
        .and_then(|w| w.things.iter().find(|t| t.id == id))
        .map_or(Value::Nil, thing)
}

/// The selection: `current` and `last` (nil for nothing), and `auto_target`.
#[must_use]
pub fn selection(view: &View) -> Value {
    let Some(w) = &view.world else {
        return Value::Nil;
    };
    let some = |id: u64| (id != 0).then(|| id_text(id));
    Value::map()
        .with("current", some(w.selection.current))
        .with("last", some(w.selection.last))
        .with("auto_target", w.selection.auto_target)
}

/// What the player carries and wears: `money`, and `items`, each with its `id`, `name`,
/// `quantity`, whether it is `worn` and whether it is `stackable`.
#[must_use]
pub fn inventory(view: &View) -> Value {
    let Some(inv) = &view.inventory else {
        return Value::Nil;
    };
    let items: Vec<Value> = inv
        .items
        .iter()
        .map(|(id, i)| {
            Value::map()
                .with("id", id_text(*id))
                .with("name", i.name.as_str())
                .with("quantity", i.quantity)
                .with("worn", i.worn)
                .with("stackable", i.stackable)
        })
        .collect();
    Value::map().with("money", inv.money).with("items", items)
}

/// The effects on the player: each one's `id`, `name` and `remaining` seconds (nil when it does
/// not run out).
#[must_use]
pub fn effects(view: &View) -> Value {
    let Some(e) = &view.effects else {
        return Value::List(Vec::new());
    };
    Value::List(
        e.effects
            .iter()
            .map(|f| {
                let remaining =
                    (f.duration > 0.0).then(|| (f.time_cast + f.duration - e.now).max(0.0));
                Value::map()
                    .with("id", f.id)
                    .with("name", f.name.as_str())
                    .with("remaining", remaining)
            })
            .collect(),
    )
}

/// The player's skills: each one's `id`, whether it is `trained`, its `level` and `base_level`,
/// and its `name` where the view writes it.
#[must_use]
pub fn skills(view: &View) -> Value {
    let Some(s) = &view.skills else {
        return Value::List(Vec::new());
    };
    Value::List(
        s.states
            .iter()
            .map(|(id, st)| {
                let v = Value::map()
                    .with("id", *id)
                    .with("trained", st.trained)
                    .with("level", st.level)
                    .with("base_level", st.base_level);
                match s.nodes.iter().find(|n| n.skill == *id) {
                    Some(n) => text_ref(v, "name", &n.name),
                    None => v,
                }
            })
            .collect(),
    )
}

/// The game's clocks: `world_time` and `game_time`, seconds.
#[must_use]
pub fn clock(view: &View) -> Value {
    Value::map()
        .with("world_time", view.clock.world_time)
        .with("game_time", view.clock.game_time)
}

/// The game's questions open, in the order asked: each one's `id`, its `kind` (`died`,
/// `resurrection`, `effect`, `message`, `fellowship`, `oath`, `trade`), its `text`, whether it has
/// `one_answer` and goes in front (`at_once`), and the one it names (`by`, `leader`, `vassal`,
/// `partner`, with their names).
#[must_use]
pub fn questions(view: &View) -> Value {
    let name = |v: Value, key: &str, r: &Option<TextRef>| match r {
        Some(r) => text_ref(v, key, r),
        None => v,
    };
    Value::List(
        view.questions
            .iter()
            .map(|q| {
                let v = Value::map()
                    .with("id", q.id)
                    .with("one_answer", q.one_answer())
                    .with("at_once", q.at_once);
                let v = text_ref(v, "text", &q.text);
                match &q.asked {
                    Asked::Died => v.with("kind", "died"),
                    Asked::Resurrection { by } => name(v.with("kind", "resurrection"), "by", by),
                    Asked::Effect { effect, .. } => {
                        v.with("kind", "effect").with("effect", *effect)
                    }
                    Asked::Message { .. } => v.with("kind", "message"),
                    Asked::Fellowship {
                        leader,
                        leader_name,
                        fellowship,
                    } => {
                        let v = v
                            .with("kind", "fellowship")
                            .with("leader", id_text(*leader));
                        name(
                            name(v, "leader_name", leader_name),
                            "fellowship",
                            fellowship,
                        )
                    }
                    Asked::Oath { vassal, name: who } => name(
                        v.with("kind", "oath").with("vassal", id_text(*vassal)),
                        "name",
                        who,
                    ),
                    Asked::Trade { partner, name: who } => name(
                        v.with("kind", "trade").with("partner", id_text(*partner)),
                        "name",
                        who,
                    ),
                    _ => v,
                }
            })
            .collect(),
    )
}

/// Where the player stands, as the radar's place line gives it: whether it is `inside`, its map
/// `coordinates` outdoors (`east` and `north`, nil inside), whether it is in `portal_space`, and
/// how far the radar reaches there (`radar_range`, meters); nil before the player is in the world.
#[must_use]
pub fn place_line(view: &View) -> Value {
    let Some(w) = &view.world else {
        return Value::Nil;
    };
    Value::map()
        .with("inside", w.inside)
        .with(
            "coordinates",
            w.map_coordinates
                .map(|(east, north)| Value::map().with("east", east).with("north", north)),
        )
        .with("portal_space", w.portal_space)
        .with("radar_range", w.radar_range())
}

/// A fellow: its `name`, `level`, `health` and `vigor` with their greatest (`max_health`,
/// `max_vigor`) and what players took of them (`health_taken`, `vigor_taken`), and its `cell`.
pub(crate) fn fellow(map: Value, f: &Fellow) -> Value {
    text_ref(map, "name", &f.name)
        .with("level", f.level)
        .with("health", f.health)
        .with("max_health", f.max_health)
        .with("vigor", f.vigor)
        .with("max_vigor", f.max_vigor)
        .with("health_taken", f.health_taken_by_players)
        .with("vigor_taken", f.vigor_taken_by_players)
        .with("cell", f.cell)
}

/// A fellowship: its `name`, its `leader`, whether it shares experience (`share_experience`), how
/// it shares loot (`looting`) and its `fellows` in the order they joined, each with its `id`.
pub(crate) fn fellowship(f: &Fellowship) -> Value {
    let fellows: Vec<Value> = f
        .fellows
        .iter()
        .map(|(id, fe)| fellow(Value::map().with("id", id_text(*id)), fe))
        .collect();
    Value::map()
        .with("name", f.name.as_str())
        .with("leader", some_id(f.leader))
        .with("share_experience", f.social != 0)
        .with("looting", f.looting)
        .with("fellows", fellows)
}

fn member(m: &Member, online: bool) -> Value {
    let v = Value::map()
        .with("id", some_id(m.id))
        .with("online", online)
        .with("level", m.level)
        .with("rank", m.rank)
        .with("officer", m.officer)
        .with("faction", m.faction)
        .with("species", m.species)
        .with("gender", m.gender)
        .with("experience_passed", big(m.experience_passed));
    text_ref(text_ref(v, "name", &m.name), "title", &m.title)
}

/// An allegiance: its `name`, `message` (of the day), `faction`, how many it has (`total`), its
/// `officers`, and its `monarch`, the player's `patron`, the player (`member`) and the player's
/// `vassals`, each with its `id`, `name`, `title`, whether it is `online`, its `level`, `rank`,
/// whether it is an `officer`, its `faction`, `species`, `gender` and `experience_passed`.
pub(crate) fn allegiance(a: &Allegiance) -> Value {
    let v = Value::map()
        .with("faction", a.faction)
        .with("total", a.total)
        .with(
            "officers",
            a.officers
                .iter()
                .map(|o| Value::from(id_text(*o)))
                .collect::<Vec<_>>(),
        )
        .with("monarch", member(&a.monarch.0, a.monarch.1))
        .with("patron", member(&a.patron.0, a.patron.1))
        .with("member", member(&a.member.0, a.member.1))
        .with(
            "vassals",
            a.vassals
                .iter()
                .map(|(m, on)| member(m, *on))
                .collect::<Vec<_>>(),
        );
    text_ref(text_ref(v, "name", &a.name), "message", &a.message)
}

/// A kingdom's word.
#[must_use]
pub const fn kingdom_word(k: u32) -> Option<&'static str> {
    match k {
        kingdom::SHADOW => Some("shadow"),
        kingdom::ORDER => Some("order"),
        kingdom::DOMINION => Some("dominion"),
        kingdom::NEUTRAL => Some("neutral"),
        _ => None,
    }
}

/// The word for a kind of conflict between players a place allows.
#[must_use]
pub const fn conflict_word(c: u32) -> Option<&'static str> {
    match c {
        conflict::PEACE => Some("peace"),
        conflict::CONTROLLED => Some("controlled"),
        conflict::KINGDOM_AGAINST_KINGDOM => Some("kingdom_against_kingdom"),
        conflict::FREE_FOR_ALL => Some("free_for_all"),
        conflict::KINGDOM_AGAINST_KINGDOM_ANYWHERE => Some("kingdom_against_kingdom_anywhere"),
        _ => None,
    }
}

/// The player's fellowship (nil while in none), its `allegiance` (nil until one is told), its
/// `friends` (each `name`, `location` and `mode`: 0 offline, 1 available, 2 away, 4 busy), its own
/// `state` on the chat service, its `kingdom` and the `conflict` the place allows.
#[must_use]
pub fn social(view: &View) -> Value {
    let s = &view.social;
    let friends: Vec<Value> = s
        .friends
        .iter()
        .map(|f| {
            Value::map()
                .with("name", f.name.as_str())
                .with("location", f.location.as_str())
                .with("mode", f.mode)
        })
        .collect();
    Value::map()
        .with("fellowship", s.fellowship.as_ref().map(fellowship))
        .with("allegiance", s.allegiance.as_ref().map(allegiance))
        .with("friends", friends)
        .with("state", s.own_state)
        .with("kingdom", s.kingdom.and_then(kingdom_word))
        .with("conflict", s.conflict.and_then(conflict_word))
}

/// A quest of the journal: its `id`, `name`, `description` and `journal`, its `status`, how hard it
/// is (`challenge`), when it was `given` (game seconds), whether it is `maxed_out`, its
/// `seconds_remaining`, and whether it is `cancellable`.
pub(crate) fn quest(q: &Quest) -> Value {
    let v = Value::map()
        .with("id", q.id)
        .with("status", q.status)
        .with("challenge", q.challenge)
        .with("given", q.given)
        .with("maxed_out", q.maxed_out)
        .with("seconds_remaining", q.seconds_remaining)
        .with("cancellable", q.cancellable);
    let v = text_ref(v, "name", &q.name);
    let v = text_ref(v, "description", &q.description);
    text_ref(v, "journal", &q.journal)
}

/// The player's quests as the journal lists them, newest given first; nil before the player is
/// here.
#[must_use]
pub fn journal(view: &View) -> Value {
    view.journal.as_ref().map_or(Value::Nil, |j| {
        Value::List(j.quests.iter().map(quest).collect())
    })
}

/// The markers on the map: each one's `kind`, the `object` it stands for (nil for none), the
/// `cell` it marks, its `picture` and its `label`.
#[must_use]
pub fn markers(view: &View) -> Value {
    Value::List(
        view.markers
            .markers
            .iter()
            .map(|m| {
                let v = Value::map()
                    .with("kind", m.kind)
                    .with("object", some_id(m.object))
                    .with("cell", m.cell)
                    .with("picture", m.picture);
                maybe_text(v, "label", m.label.as_ref())
            })
            .collect(),
    )
}

/// A pet's mode as a word.
#[must_use]
pub const fn pet_mode_word(mode: u32) -> Option<&'static str> {
    use pet::mode as m;
    match mode {
        m::AGGRESSIVE => Some("aggressive"),
        m::OPPORTUNISTIC => Some("opportunistic"),
        m::COURAGEOUS => Some("courageous"),
        m::PROTECTIVE => Some("protective"),
        m::ASSISTING => Some("assisting"),
        m::DEFENSIVE => Some("defensive"),
        m::SUPPORTING => Some("supporting"),
        m::COOPERATIVE => Some("cooperative"),
        m::IDLE => Some("idle"),
        _ => None,
    }
}

/// The player's pets in the order they came: each one's `id`, `name` (as the game shows it,
/// without its owner's part), `icon` and `mode` (a word).
#[must_use]
pub fn pets(view: &View) -> Value {
    Value::List(
        view.pets
            .iter()
            .map(|p| {
                let v = Value::map()
                    .with("id", id_text(p.id))
                    .with("icon", p.icon)
                    .with("mode", pet_mode_word(p.mode));
                match p.name.as_ref().and_then(|n| n.literal.as_ref()) {
                    Some(units) => {
                        v.with("name", pet::shown_name(&String::from_utf16_lossy(units)))
                    }
                    None => maybe_text(v, "name", p.name.as_ref()),
                }
            })
            .collect(),
    )
}

/// A shortcut's kind as a word.
#[must_use]
pub const fn shortcut_word(kind: u32) -> Option<&'static str> {
    match kind {
        shortcut_kind::SKILL => Some("skill"),
        shortcut_kind::ITEM => Some("item"),
        shortcut_kind::TEXT => Some("text"),
        shortcut_kind::EMOTE => Some("emote"),
        shortcut_kind::RECIPE => Some("recipe"),
        _ => None,
    }
}

/// The shortcut bar: the `set` of ten shown (from 1), whether it is `locked` and shows its
/// numbers (`numbers`), its `rows`, and its `slots` that hold something, each with its `slot`
/// number, its `kind` (`skill`, `item`, `text`, `emote` or `recipe`) and what the kind names
/// (`skill`, `item`, `text`, `emote` or `recipe`), with its `tooltip` and `icon`; nil until the
/// scripts answer for it.
#[must_use]
pub fn toolbar(view: &View) -> Value {
    let Some(t) = &view.toolbar else {
        return Value::Nil;
    };
    let slots: Vec<Value> = t
        .shortcuts
        .iter()
        .zip(0u32..)
        .filter_map(|(s, n)| {
            let kind = shortcut_word(s.kind)?;
            let v = Value::map().with("slot", n).with("kind", kind);
            let v = match s.kind {
                shortcut_kind::SKILL => v.with("skill", s.data_enum),
                shortcut_kind::ITEM => v.with("item", id_text(s.item)),
                shortcut_kind::TEXT => v.with("text", s.text.as_str()),
                shortcut_kind::EMOTE => v.with("emote", s.data_enum),
                _ => v.with("recipe", s.data_id),
            };
            Some(match t.slots.get(n as usize) {
                Some(shown) => v
                    .with("tooltip", shown.tooltip.as_str())
                    .with("icon", shown.icon),
                None => v,
            })
        })
        .collect();
    Value::map()
        .with("set", t.set)
        .with("locked", t.locked)
        .with("numbers", t.show_numbers)
        .with("rows", t.height)
        .with("slots", slots)
}

/// The crafting page: the player's craft `experience`; the `skills`, each with its `id`, `name`,
/// `level`, whether it is `trained` (the player has it), `can_raise` and its `recipes` (each `id`,
/// `name`, `category`, whether it is `shown` and the game time it may be made again,
/// `use_again`); the recipes' `categories` (each `id` and `name`); and the recipe the making window has
/// `opened` (its `recipe`, `name`, `spinner` with `min_spinner` and `max_spinner`, `charges`,
/// `difficulty`, `cost`, its `ingredients` (each `slot` and `description`) and the `items` that
/// may go in), nil when none is.
#[must_use]
pub fn craft(view: &View) -> Value {
    let Some(c) = &view.craft else {
        return Value::Nil;
    };
    let skills: Vec<Value> = c
        .skills
        .iter()
        .map(|s| {
            let recipes: Vec<Value> = s
                .recipes
                .iter()
                .map(|r| {
                    text_ref(Value::map().with("id", r.did), "name", &r.name)
                        .with("category", r.category)
                        .with("shown", r.shown)
                        .with("use_again", r.time_use_again)
                })
                .collect();
            text_ref(
                Value::map().with("id", s.listed.did),
                "name",
                &s.listed.name,
            )
            .with("level", s.level)
            .with("trained", s.own.is_some())
            .with("can_raise", s.can_raise)
            .with("recipes", recipes)
        })
        .collect();
    let categories: Vec<Value> = c
        .categories
        .iter()
        .map(|(id, name)| text_ref(Value::map().with("id", *id), "name", name))
        .collect();
    let opened = c.opened.as_ref().map(|r| {
        let ingredients: Vec<Value> = r
            .ingredients
            .iter()
            .map(|i| {
                text_ref(
                    Value::map().with("slot", i.id),
                    "description",
                    &i.description,
                )
            })
            .collect();
        text_ref(Value::map().with("recipe", r.recipe), "name", &r.name)
            .with("spinner", r.spinner)
            .with("min_spinner", r.min_spinner)
            .with("max_spinner", r.max_spinner)
            .with("charges", r.charges)
            .with("difficulty", r.difficulty)
            .with("cost", r.cost)
            .with("ingredients", ingredients)
            .with(
                "items",
                r.items
                    .iter()
                    .map(|i| Value::from(id_text(*i)))
                    .collect::<Vec<_>>(),
            )
    });
    Value::map()
        .with("experience", big(c.experience))
        .with("skills", skills)
        .with("categories", categories)
        .with("opened", opened)
}
