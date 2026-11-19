//! What the player's scripts tell the panels ([`Notice`]), as plugin events: one event for each
//! part of the game (`notice.player`, `notice.fellowship` and the rest, [`EVENTS`]), so a plugin
//! hears what the game's panels hear.
//!
//! Each event's table says what the notice is about (`about`, the object's id, nil when the notice
//! is for every panel), what it says (`what`, a word) and that word's own fields. A notice the
//! client does not read is not told.

use auberean_client_contract::notice::{
    change, vital, AllegianceNotice, BookNotice, ChatNotice, CombatNotice, CraftNotice,
    DialogNotice, ExaminationLine, ExaminationNotice, ExaminationRequest, FellowshipNotice,
    FriendState, FriendsNotice, InterfaceNotice, InventoryNotice, MapNotice, Notice, PetNotice,
    PlayerNotice, QuestNotice, Said, Scenes, SelectionNotice, ShortcutsNotice, SkillsNotice, Store,
    StoreNotice, TradeNotice, WorldNotice,
};
use dereth_plugin_api::{Event, Value};

use crate::events::id_text;
use crate::reads::{
    allegiance, big, conflict_word, fellow, fellowship, maybe_text, pet_mode_word, quest, some_id,
    text_ref,
};

/// The notices' events, one for each part of the game, with the fields each one's table carries,
/// in the order the spec lists them (`docs/plugins/auberean.md`).
pub const EVENTS: &[(&str, &str)] = &[
    ("notice.player", "about, what, and the word's fields"),
    ("notice.selection", "about, what, and the word's fields"),
    ("notice.examination", "about, what, and the word's fields"),
    ("notice.chat", "about, what, and the word's fields"),
    ("notice.dialog", "about, what, and the word's fields"),
    ("notice.world", "about, what, and the word's fields"),
    ("notice.combat", "about, what, and the word's fields"),
    ("notice.skills", "about, what, and the word's fields"),
    ("notice.shortcuts", "about, what, and the word's fields"),
    ("notice.inventory", "about, what, and the word's fields"),
    ("notice.fellowship", "about, what, and the word's fields"),
    ("notice.allegiance", "about, what, and the word's fields"),
    ("notice.friends", "about, what, and the word's fields"),
    ("notice.trade", "about, what, and the word's fields"),
    ("notice.pet", "about, what, and the word's fields"),
    ("notice.map", "about, what, and the word's fields"),
    ("notice.store", "about, what, and the word's fields"),
    ("notice.craft", "about, what, and the word's fields"),
    ("notice.quest", "about, what, and the word's fields"),
    ("notice.book", "about, what, and the word's fields"),
    ("notice.interface", "about, what, and the word's fields"),
];

/// The plugin event a notice makes; none for one the client does not read.
#[must_use]
pub fn translate(n: &Notice) -> Option<Event> {
    let (area, what, fields) = said(&n.said)?;
    let table = fields
        .with("about", n.about.map(id_text))
        .with("what", what);
    Some(Event::new(&format!("notice.{area}"), table))
}

fn id(v: u64) -> Value {
    Value::from(id_text(v))
}

fn m() -> Value {
    Value::map()
}

/// The area, the word and its fields of what a notice says.
fn said(s: &Said) -> Option<(&'static str, &'static str, Value)> {
    Some(match s {
        Said::Player(p) => {
            let (w, v) = player(p)?;
            ("player", w, v)
        }
        Said::Selection(n) => {
            let (w, v) = selection(n)?;
            ("selection", w, v)
        }
        Said::Examination(n) => {
            let (w, v) = examination(n)?;
            ("examination", w, v)
        }
        Said::Chat(n) => {
            let (w, v) = match n {
                ChatNotice::StartTell { to } => ("start_tell", maybe_text(m(), "to", to.as_ref())),
                ChatNotice::SettingsRead => ("settings_read", m()),
                _ => return None,
            };
            ("chat", w, v)
        }
        Said::Dialog(n) => {
            let (w, v) = dialog(n)?;
            ("dialog", w, v)
        }
        Said::World(n) => {
            let (w, v) = match n {
                WorldNotice::LookChanged => ("look_changed", m()),
                WorldNotice::ItemChanged => ("item_changed", m()),
                WorldNotice::Damage(h) => (
                    "damage",
                    h.as_ref().map_or_else(m, |h| {
                        m().with("text", h.text.as_str())
                            .with(
                                "color",
                                h.color.iter().map(|c| Value::from(*c)).collect::<Vec<_>>(),
                            )
                            .with("seconds", h.seconds)
                            .with("scale", h.scale)
                    }),
                ),
                _ => return None,
            };
            ("world", w, v)
        }
        Said::Combat(n) => {
            let (w, v) = match n {
                CombatNotice::Attacking { attacking } => {
                    ("attacking", m().with("attacking", *attacking))
                }
                CombatNotice::SpecialAsked { skill, special } => (
                    "special_asked",
                    m().with("skill", *skill).with("special", *special),
                ),
                CombatNotice::SpecialStarted { special } => {
                    ("special_started", m().with("special", *special))
                }
                CombatNotice::SpecialEnded { special } => {
                    ("special_ended", m().with("special", *special))
                }
                CombatNotice::SpecialToggled { skill, on } => {
                    ("special_toggled", m().with("skill", *skill).with("on", *on))
                }
                _ => return None,
            };
            ("combat", w, v)
        }
        Said::Skills(n) => {
            let (w, v) = match n {
                SkillsNotice::Changed { skill } => ("changed", m().with("skill", *skill)),
                SkillsNotice::Points {
                    experience,
                    credits,
                    hero_credits,
                    whole,
                } => (
                    "points",
                    m().with("experience", big(*experience))
                        .with("credits", *credits)
                        .with("hero_credits", *hero_credits)
                        .with("whole", *whole),
                ),
                SkillsNotice::ResetTimes => ("reset_times", m()),
                _ => return None,
            };
            ("skills", w, v)
        }
        Said::Shortcuts(n) => {
            let (w, v) = match n {
                ShortcutsNotice::Reloaded => ("reloaded", m()),
                ShortcutsNotice::RecipeGone { recipe } => {
                    ("recipe_gone", m().with("recipe", *recipe))
                }
                _ => return None,
            };
            ("shortcuts", w, v)
        }
        Said::Inventory(n) => {
            let (w, v) = inventory(n)?;
            ("inventory", w, v)
        }
        Said::Fellowship(n) => {
            let (w, v) = fellowship_notice(n)?;
            ("fellowship", w, v)
        }
        Said::Allegiance(n) => {
            let (w, v) = match n {
                AllegianceNotice::Changed(a) => (
                    "changed",
                    m().with("allegiance", a.as_deref().map(allegiance)),
                ),
                AllegianceNotice::OathOffered { vassal, name } => (
                    "oath_offered",
                    maybe_text(m().with("vassal", id(*vassal)), "name", name.as_ref()),
                ),
                AllegianceNotice::OathAnswered { patron, answer } => (
                    "oath_answered",
                    m().with("patron", id(*patron)).with("answer", *answer),
                ),
                _ => return None,
            };
            ("allegiance", w, v)
        }
        Said::Friends(n) => {
            let (w, v) = match n {
                FriendsNotice::List(list) => (
                    "list",
                    m().with("friends", list.iter().map(friend).collect::<Vec<_>>()),
                ),
                FriendsNotice::Added { name } => ("added", maybe_text(m(), "name", name.as_ref())),
                FriendsNotice::Removed { name } => {
                    ("removed", maybe_text(m(), "name", name.as_ref()))
                }
                FriendsNotice::State(f) => ("state", m().with("friend", f.as_ref().map(friend))),
                FriendsNotice::OwnState { mode } => ("own_state", m().with("mode", *mode)),
                _ => return None,
            };
            ("friends", w, v)
        }
        Said::Trade(n) => {
            let (w, v) = trade(n)?;
            ("trade", w, v)
        }
        Said::Pet(n) => {
            let (w, v) = match n {
                PetNotice::Added {
                    pet,
                    name,
                    icon,
                    mode,
                } => (
                    "added",
                    maybe_text(m().with("pet", id(*pet)), "name", name.as_ref())
                        .with("icon", *icon)
                        .with("mode", pet_mode_word(*mode)),
                ),
                PetNotice::Removed { pet } => ("removed", m().with("pet", id(*pet))),
                PetNotice::Named { pet, name } => (
                    "named",
                    maybe_text(m().with("pet", id(*pet)), "name", name.as_ref()),
                ),
                PetNotice::Mode { pet, mode } => (
                    "mode",
                    m().with("pet", id(*pet)).with("mode", pet_mode_word(*mode)),
                ),
                PetNotice::Attacked { pet } => ("attacked", m().with("pet", id(*pet))),
                _ => return None,
            };
            ("pet", w, v)
        }
        Said::Map(n) => {
            let (w, v) = match n {
                MapNotice::MarkerAdded {
                    kind,
                    object,
                    cell,
                    picture,
                    label,
                } => (
                    "marker_added",
                    maybe_text(
                        m().with("kind", *kind)
                            .with("object", some_id(*object))
                            .with("cell", *cell)
                            .with("picture", *picture),
                        "label",
                        label.as_ref(),
                    ),
                ),
                MapNotice::MarkerRemoved { kind, object } => (
                    "marker_removed",
                    m().with("kind", *kind).with("object", some_id(*object)),
                ),
                MapNotice::MarkersCleared { kind } => ("markers_cleared", m().with("kind", *kind)),
                _ => return None,
            };
            ("map", w, v)
        }
        Said::Store(n) => {
            let (w, v) = store_notice(n)?;
            ("store", w, v)
        }
        Said::Craft(n) => {
            let (w, v) = match n {
                CraftNotice::PanelChanged => ("panel_changed", m()),
                CraftNotice::IngredientAdded {
                    recipe,
                    slot,
                    item,
                    amount,
                    error,
                } => (
                    "ingredient_added",
                    m().with("recipe", *recipe)
                        .with("slot", *slot)
                        .with("item", some_id(*item))
                        .with("amount", *amount)
                        .with("error", *error),
                ),
                CraftNotice::Made { recipe, error } => {
                    ("made", m().with("recipe", *recipe).with("error", *error))
                }
                CraftNotice::Open { recipe } => ("open", m().with("recipe", *recipe)),
                CraftNotice::SkillChanged { skill } => ("skill_changed", m().with("skill", *skill)),
                _ => return None,
            };
            ("craft", w, v)
        }
        Said::Quest(n) => {
            let (w, v) = match n {
                QuestNotice::Journal {
                    change,
                    quest: q,
                    all,
                } => (
                    "journal",
                    m().with("change", *change)
                        .with("quest", q.as_ref().map(quest))
                        .with(
                            "all",
                            all.as_ref()
                                .map(|a| a.iter().map(quest).collect::<Vec<_>>()),
                        ),
                ),
                QuestNotice::Acts { change, act } => {
                    ("acts", m().with("change", *change).with("act", *act))
                }
                QuestNotice::Scenes(s) => ("scenes", m().with("scenes", s.as_ref().map(scenes))),
                _ => return None,
            };
            ("quest", w, v)
        }
        Said::Book(n) => {
            let (w, v) = match n {
                BookNotice::Open {
                    controls,
                    look,
                    title,
                    text,
                    picture,
                } => (
                    "open",
                    maybe_text(
                        maybe_text(
                            m().with("controls", *controls)
                                .with("look", *look)
                                .with("picture", *picture),
                            "title",
                            title.as_ref(),
                        ),
                        "text",
                        text.as_ref(),
                    ),
                ),
                _ => return None,
            };
            ("book", w, v)
        }
        Said::Interface(n) => {
            let (w, v) = match n {
                InterfaceNotice::ThroughPortal { inside } => {
                    ("through_portal", m().with("inside", *inside))
                }
                InterfaceNotice::PortalScene { playing, scenes: s } => (
                    "portal_scene",
                    m().with("playing", *playing)
                        .with("scenes", s.as_ref().map(scenes)),
                ),
                InterfaceNotice::SaveLayout { name } => {
                    ("save_layout", maybe_text(m(), "name", name.as_ref()))
                }
                InterfaceNotice::LoadLayout { name } => {
                    ("load_layout", maybe_text(m(), "name", name.as_ref()))
                }
                InterfaceNotice::ChatServiceLost => ("chat_service_lost", m()),
                InterfaceNotice::ChatServiceTicket { taken } => {
                    ("chat_service_ticket", m().with("taken", *taken))
                }
                _ => return None,
            };
            ("interface", w, v)
        }
        _ => return None,
    })
}

/// A vital's word.
const fn vital_word(v: u32) -> Option<&'static str> {
    match v {
        vital::HEALTH => Some("health"),
        vital::VIGOR => Some("vigor"),
        vital::EXPERIENCE => Some("experience"),
        vital::FOCUS => Some("focus"),
        vital::HEALTH_TAKEN_BY_PLAYERS => Some("health_taken"),
        vital::VIGOR_TAKEN_BY_PLAYERS => Some("vigor_taken"),
        _ => None,
    }
}

fn player(p: &PlayerNotice) -> Option<(&'static str, Value)> {
    Some(match p {
        PlayerNotice::Vital {
            vital: which,
            current,
            max,
        } => (
            "vital",
            m().with(
                "vital",
                vital_word(*which).map_or_else(|| Value::from(*which), Value::from),
            )
            .with("current", big(*current))
            .with("max", big(*max)),
        ),
        PlayerNotice::Hero {
            hero,
            focus,
            max_focus,
        } => (
            "hero",
            m().with("hero", *hero)
                .with("focus", big(*focus))
                .with("max_focus", big(*max_focus)),
        ),
        PlayerNotice::Vitae {
            kept,
            regained,
            experience,
        } => (
            "vitae",
            m().with("kept", *kept)
                .with("regained", *regained)
                .with("experience", *experience),
        ),
        PlayerNotice::Conflict { kind } => ("conflict", m().with("conflict", conflict_word(*kind))),
        PlayerNotice::Death { dead } => ("death", m().with("dead", *dead)),
        PlayerNotice::Mounted { mounted } => ("mounted", m().with("mounted", *mounted)),
        PlayerNotice::EffectsChanged => ("effects_changed", m()),
        PlayerNotice::PulseMissed { effect } => ("pulse_missed", m().with("effect", *effect)),
        PlayerNotice::OptionsRead { options } => ("options_read", m().with("options", *options)),
        PlayerNotice::OptionsChanged { options } => {
            ("options_changed", m().with("options", *options))
        }
        _ => return None,
    })
}

fn selection(n: &SelectionNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        SelectionNotice::Changed { selected } => {
            ("changed", m().with("selected", some_id(*selected)))
        }
        SelectionNotice::Selectable { selectable } => {
            ("selectable", m().with("selectable", *selectable))
        }
        SelectionNotice::Vitals(v) => (
            "vitals",
            v.map_or_else(m, |v| {
                m().with("kind", v.kind)
                    .with("health", v.health)
                    .with("max_health", v.max_health)
                    .with("health_taken", v.health_taken_by_players)
                    .with("vigor", v.vigor)
                    .with("max_vigor", v.max_vigor)
                    .with("vigor_taken", v.vigor_taken_by_players)
            }),
        ),
        SelectionNotice::Targeting { waiting } => ("targeting", m().with("waiting", *waiting)),
        _ => return None,
    })
}

fn request(r: &ExaminationRequest) -> Value {
    m().with("kind", r.kind)
        .with("object", some_id(r.object))
        .with("tabs", r.tabs)
        .with("data_enum", r.data_enum)
        .with("data_id", r.data_id)
}

fn line(l: &ExaminationLine) -> Value {
    let v = m()
        .with("kind", l.kind)
        .with("tabs", l.tabs)
        .with("appearance", l.appearance)
        .with("value", big(l.value))
        .with("time", l.time)
        .with(
            "icon",
            l.icon
                .iter()
                .map(|(picture, color)| m().with("picture", *picture).with("color", *color))
                .collect::<Vec<_>>(),
        )
        .with("effect", l.effect.iter().map(line).collect::<Vec<_>>());
    maybe_text(v, "text", l.text.as_ref())
}

fn examination(n: &ExaminationNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        ExaminationNotice::Profile(p) => (
            "profile",
            p.as_ref().map_or_else(m, |p| {
                m().with("request", request(&p.request))
                    .with("tabs", p.tabs)
                    .with("lines", p.lines.iter().map(line).collect::<Vec<_>>())
            }),
        ),
        ExaminationNotice::RequestChanged(r) => (
            "request_changed",
            m().with("request", r.as_ref().map(request)),
        ),
        ExaminationNotice::Open => ("open", m()),
        _ => return None,
    })
}

fn dialog(n: &DialogNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        DialogNotice::Message { text, at_once } => (
            "message",
            maybe_text(m().with("at_once", *at_once), "text", text.as_ref()),
        ),
        DialogNotice::ResurrectionOffered { by } => {
            ("resurrection_offered", maybe_text(m(), "by", by.as_ref()))
        }
        DialogNotice::EffectQuestion {
            target,
            source,
            effect,
            question,
        } => (
            "effect_question",
            maybe_text(
                m().with("target", some_id(*target))
                    .with("source", some_id(*source))
                    .with("effect", *effect),
                "question",
                question.as_ref(),
            ),
        ),
        _ => return None,
    })
}

/// How the items changed, as a word.
fn change_word(c: u32) -> Value {
    match c {
        change::ALL => Value::from("all"),
        change::ADDED => Value::from("added"),
        change::REMOVED => Value::from("removed"),
        change::FAILED => Value::from("failed"),
        change::UNDER_WAY => Value::from("under_way"),
        other => Value::from(other),
    }
}

fn inventory(n: &InventoryNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        InventoryNotice::Worn {
            change: c,
            item,
            location,
        } => (
            "worn",
            m().with("change", change_word(*c))
                .with("item", some_id(*item))
                .with("location", *location),
        ),
        InventoryNotice::Contents {
            container,
            change: c,
            item,
            slot,
        } => (
            "contents",
            m().with("container", some_id(*container))
                .with("change", change_word(*c))
                .with("item", some_id(*item))
                .with("slot", *slot),
        ),
        InventoryNotice::ContainerState { container, state } => (
            "container_state",
            m().with("container", some_id(*container))
                .with("open", *state != 0)
                .with("state", *state),
        ),
        InventoryNotice::PackChanged => ("pack_changed", m()),
        InventoryNotice::Money { amount } => ("money", m().with("amount", *amount)),
        _ => return None,
    })
}

fn fellowship_notice(n: &FellowshipNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        FellowshipNotice::Changed(f) => (
            "changed",
            m().with("fellowship", f.as_deref().map(fellowship)),
        ),
        FellowshipNotice::Cleared => ("cleared", m()),
        FellowshipNotice::Fellow {
            fellow: who,
            record,
        } => (
            "fellow",
            m().with("fellow", id(*who)).with(
                "record",
                record
                    .as_deref()
                    .map(|r| fellow(m().with("id", id(*who)), r)),
            ),
        ),
        FellowshipNotice::FellowGone { fellow: who } => {
            ("fellow_gone", m().with("fellow", id(*who)))
        }
        FellowshipNotice::Level { fellow: who, level } => {
            ("level", m().with("fellow", id(*who)).with("level", *level))
        }
        FellowshipNotice::Health {
            fellow: who,
            health,
            taken_by_players,
        } => (
            "health",
            m().with("fellow", id(*who))
                .with("health", *health)
                .with("taken", *taken_by_players),
        ),
        FellowshipNotice::MaxHealth { fellow: who, max } => {
            ("max_health", m().with("fellow", id(*who)).with("max", *max))
        }
        FellowshipNotice::Vigor {
            fellow: who,
            vigor,
            taken_by_players,
        } => (
            "vigor",
            m().with("fellow", id(*who))
                .with("vigor", *vigor)
                .with("taken", *taken_by_players),
        ),
        FellowshipNotice::MaxVigor { fellow: who, max } => {
            ("max_vigor", m().with("fellow", id(*who)).with("max", *max))
        }
        FellowshipNotice::Leader { leader } => ("leader", m().with("leader", some_id(*leader))),
        FellowshipNotice::Cell { fellow: who, cell } => {
            ("cell", m().with("fellow", id(*who)).with("cell", *cell))
        }
        FellowshipNotice::Offered {
            leader,
            leader_name,
            fellowship: name,
        } => (
            "offered",
            maybe_text(
                maybe_text(
                    m().with("leader", id(*leader)),
                    "leader_name",
                    leader_name.as_ref(),
                ),
                "fellowship",
                name.as_ref(),
            ),
        ),
        FellowshipNotice::OfferExpired { leader } => {
            ("offer_expired", m().with("leader", id(*leader)))
        }
        _ => return None,
    })
}

fn friend(f: &FriendState) -> Value {
    m().with("name", f.name.as_str())
        .with("location", f.location.as_str())
        .with("mode", f.mode)
}

fn trade(n: &TradeNotice) -> Option<(&'static str, Value)> {
    Some(match n {
        TradeNotice::Registered {
            asker,
            asked,
            partner,
        } => (
            "registered",
            maybe_text(
                m().with("asker", id(*asker)).with("asked", id(*asked)),
                "partner",
                partner.as_ref(),
            ),
        ),
        TradeNotice::Opened => ("opened", m()),
        TradeNotice::Closed { by, reason } => (
            "closed",
            m().with("by", some_id(*by)).with("reason", *reason),
        ),
        TradeNotice::Accepted { by } => ("accepted", m().with("by", some_id(*by))),
        TradeNotice::Declined { by } => ("declined", m().with("by", some_id(*by))),
        TradeNotice::Emptied => ("emptied", m()),
        TradeNotice::AcceptanceCleared => ("acceptance_cleared", m()),
        TradeNotice::Offered { by, item, quantity } => (
            "offered",
            m().with("by", some_id(*by))
                .with("item", some_id(*item))
                .with("quantity", *quantity),
        ),
        TradeNotice::Withdrawn { by, item } => (
            "withdrawn",
            m().with("by", some_id(*by)).with("item", some_id(*item)),
        ),
        _ => return None,
    })
}

/// A shop: its `id`, `name`, `description`, `sorter`, `portrait`, the page shown (`position`,
/// `size`) and its `sales` in order.
fn store(s: &Store) -> Value {
    let sales: Vec<Value> = s
        .sales
        .iter()
        .map(|x| {
            let v = m()
                .with("item", some_id(x.item))
                .with("item_data", x.item_data)
                .with("icon", x.icon)
                .with("cost_icon", x.cost_icon)
                .with("cost_quantity", x.cost_quantity)
                .with("stock", x.stock)
                .with("ordinal", x.ordinal)
                .with("state", x.state);
            text_ref(
                text_ref(v, "description", &x.description),
                "cost_description",
                &x.cost_description,
            )
        })
        .collect();
    let v = m()
        .with("id", s.id)
        .with("portrait", s.portrait)
        .with("position", s.position)
        .with("size", s.size)
        .with("sales", sales);
    let v = text_ref(v, "name", &s.name);
    let v = text_ref(v, "description", &s.description);
    text_ref(v, "sorter", &s.sorter)
}

fn store_notice(n: &StoreNotice) -> Option<(&'static str, Value)> {
    let shop = |s: &Option<Box<Store>>| m().with("store", s.as_deref().map(store));
    Some(match n {
        StoreNotice::Catalog { header, shops } => (
            "catalog",
            maybe_text(
                m().with("shops", shops.iter().map(store).collect::<Vec<_>>()),
                "header",
                header.as_ref(),
            ),
        ),
        StoreNotice::CatalogLeft => ("catalog_left", m()),
        StoreNotice::Shown(s) => ("shown", shop(s)),
        StoreNotice::Changed(s) => ("changed", shop(s)),
        StoreNotice::Stock { item, stock } => (
            "stock",
            m().with("item", some_id(*item)).with("stock", *stock),
        ),
        StoreNotice::Left => ("left", m()),
        StoreNotice::Purchase {
            product,
            quantity,
            cost,
            cost_quantity,
        } => (
            "purchase",
            maybe_text(
                maybe_text(
                    m().with("quantity", *quantity)
                        .with("cost_quantity", *cost_quantity),
                    "product",
                    product.as_ref(),
                ),
                "cost",
                cost.as_ref(),
            ),
        ),
        StoreNotice::Consignments(s) => ("consignments", shop(s)),
        StoreNotice::ConsignmentsChanged(s) => ("consignments_changed", shop(s)),
        StoreNotice::ConsignmentsLeft => ("consignments_left", m()),
        StoreNotice::SaleWaiting { waiting } => ("sale_waiting", m().with("waiting", *waiting)),
        _ => return None,
    })
}

/// A story's scenes in order: each one's `id`, whether it is `playable`, and its `keyframes`
/// (each one's `picture`, `music`, `voice`, `subtitle` and `seconds`, where it has them).
fn scenes(s: &Scenes) -> Value {
    Value::List(
        s.scenes
            .iter()
            .map(|sc| {
                let keyframes: Vec<Value> = sc
                    .keyframes
                    .iter()
                    .map(|k| {
                        maybe_text(
                            m().with("picture", k.picture)
                                .with("music", k.music)
                                .with("voice", k.voice)
                                .with("seconds", k.seconds),
                            "subtitle",
                            k.subtitle.as_ref(),
                        )
                    })
                    .collect();
                m().with("id", sc.id)
                    .with("playable", sc.playable)
                    .with("keyframes", keyframes)
            })
            .collect(),
    )
}
