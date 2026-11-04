//! Playing: creatures, fights and deaths, missiles, quests, skills, trade, what people say and the
//! rolls; heals, buffs and effects; crafting and butchery; the basic attacks of each weapon.

mod allegiance;
mod areas;
mod attack_speed;
mod auras;
mod barrels_and_turrets;
mod binding;
mod books;
mod butchery;
mod chests;
mod combinations;
mod conservation;
mod crafting;
mod creature_effects;
mod creature_tactics;
mod creature_weapons;
mod creatures;
mod doors;
mod drops;
mod dye_plants;
mod dying;
mod effects;
mod experience;
mod fellowships;
mod fights;
mod focus;
mod follow;
mod gives;
mod holds;
mod integrity;
mod interactions;
mod items;
mod kill_quests;
mod kingdoms;
mod lying_client;
mod map_trips;
mod maps;
mod market;
mod mining;
mod missiles;
mod mounts;
mod own_requests;
mod pack;
mod paper_doll;
mod patrols;
mod people_routes;
mod pet_kinds;
mod pets;
mod player_fights;
mod quest_uses;
mod quests;
mod recalls;
mod regeneration;
mod resting;
mod rules;
mod skills;
mod specials;
mod stack_numbers;
mod storms;
mod story;
mod summoned_portals;
mod talk;
mod trade;
mod trading;
mod two_blows;
mod untraining;
mod vaults;
mod watchers;
mod weapons;
mod wearing;
mod workshops;
mod worn_effects;

/// What a client was told of its experience: a chat-window text (its string id, with the
/// experience it names, if any), or a new total.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Told {
    Text(u32, Option<u64>),
    Total(u64),
}

/// What a client was told of its experience since `from`, in order.
pub(crate) fn experience(h: &yalain_testkit::Harness, b: usize, from: usize) -> Vec<Told> {
    use auberean_protocol::ServerMessage;
    use auberean_script::native_object::VariableValue;
    use yalain_world::calls::Node;
    use yalain_world::progress::{q, var};
    h.world_messages(b, from)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c)
                if c.function.0 == yalain_world::session::function::SHOW_TEXT =>
            {
                let v = c
                    .args
                    .as_ref()
                    .and_then(|a| yalain_world::calls::values(&a.0).ok())
                    .unwrap_or_default();
                v.iter().find_map(|x| match x.root() {
                    Some(Node::TextInfo(t)) => Some(Told::Text(
                        t.string_id,
                        t.variables.iter().find_map(|v| match v.value {
                            VariableValue::Long(n) if v.id == var::XP => Some(n),
                            _ => None,
                        }),
                    )),
                    _ => None,
                })
            }
            ServerMessage::UpdateLongInt(x) if x.quality == q::TOTAL_XP => {
                Some(Told::Total(x.value))
            }
            _ => None,
        })
        .collect()
}
