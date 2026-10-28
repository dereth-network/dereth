//! Lyceum's dialect of the shared pack format (`dereth-pack`): what its symbols name
//! ([`crate::symbol::Kind`]), the eras a world is built on, and the items an override names
//! ([`crate::conflict`]).

use dereth_pack::Dialect;
use dereth_primitives::EraId;

use crate::conflict::{loot_item, quest_item, spawn_item, weenie_item};
use crate::loot::LootKind;
use crate::symbol::Kind;

/// The first game's words for the pack format, as Lyceum speaks them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Ac1;

impl Dialect for Ac1 {
    const GAME: &'static str = dereth_pack::DEFAULT_GAME;
    const PRODUCT: &'static str = "Lyceum";
    const BUILD_COMMAND: &'static str = "lyceum build";
    const ITEM_FORMS: &'static str = "record <id>, weenie <class>, spawn <guid>, quest <name>, \
         death-treasure <id>, wielded-treasure <id>, \
         landblock <XXYY>, vertex <x>,<y> <field>, static <XXYY> <index or symbol>, \
         building <XXYY> <index>, cell <XXYY> <cell>, cell-static <XXYY> <cell> <index>, \
         or a new record's kind and symbol";
    type Kind = Kind;
    type Era = EraId;

    fn era(name: Option<&str>) -> Result<EraId, String> {
        match name {
            None => Ok(EraId::Eor),
            Some(name) => EraId::parse(name).ok_or_else(|| {
                format!("{name:?} is not an era; a world is built on eor or infiltration")
            }),
        }
    }

    fn item(text: &str, words: &[&str]) -> Option<String> {
        let block = dereth_pack::conflict::landblock;
        match words {
            ["weenie", c] => crate::sql_ids::number(c).map(weenie_item),
            ["spawn", g] => crate::sql_ids::number(g).map(spawn_item),
            ["quest", _, ..] => Some(quest_item(text.trim().split_at(5).1)),
            ["death-treasure", id] => {
                crate::sql_ids::number(id).map(|n| loot_item(LootKind::Death, n))
            }
            ["wielded-treasure", id] => {
                crate::sql_ids::number(id).map(|n| loot_item(LootKind::Wielded, n))
            }
            ["vertex", xy, field] => {
                let field = field.to_ascii_lowercase();
                xy.split_once(',')
                    .and_then(|(x, y)| Some((x.parse::<u32>().ok()?, y.parse::<u32>().ok()?)))
                    .filter(|_| crate::compile::Field::parse(&field).is_some())
                    .map(|(x, y)| format!("vertex {x},{y} {field}"))
            }
            [what @ ("static" | "building"), l, which] => block(l).map(|lb| {
                let which = which
                    .parse::<usize>()
                    .map_or_else(|_| (*which).to_owned(), |i| i.to_string());
                format!("{what} {lb:04X} {which}")
            }),
            ["cell", l, c] => block(l)
                .zip(block(c))
                .map(|(lb, c)| format!("cell {lb:04X} {c:04X}")),
            ["cell-static", l, c, i] => block(l)
                .zip(block(c))
                .zip(i.parse::<usize>().ok())
                .map(|((lb, c), i)| format!("cell-static {lb:04X} {c:04X} {i}")),
            _ => None,
        }
    }
}
