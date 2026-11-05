//! Packs merged in load order, by the shared pack format's conflict rule. Every row sets one item,
//! named by its item key; two packs that set one item alike agree, and where they set it
//! differently a later pack's row takes it only where the world file says that pack wins
//! (`[[override]]`, naming the item, its landblock, or everything); otherwise the build stops with
//! every place that sets it. One pack setting an item twice is the pack's to put right. `[[remove]]`
//! rows take items away last.

use std::collections::BTreeMap;
use std::path::Path;

use auberean_pack::Item;
use dereth_pack::conflict::{self, settle, Claim, Settled};
use dereth_pack::Loc;

use crate::rows::{Database, Located, Row};
use crate::Problem;

/// The item a row sets, as the build's report, an override and a removal name it.
#[must_use]
pub fn item(l: &Located) -> Option<Item> {
    let landblock = l.landblock.unwrap_or(0);
    Some(match &l.row {
        Row::Object(r) => Item::Object {
            key: r.key.clone(),
            landblock,
        },
        Row::Camp(r) => Item::Camp {
            key: r.key.clone(),
            landblock,
        },
        Row::Place(r) => Item::Place { key: r.key.clone() },
        Row::Loot(r) => Item::Loot { profile: r.profile },
        Row::Keeper(r) => Item::Keeper {
            template: r.template,
        },
        Row::Sale(r) => Item::Sale {
            store: r.store,
            ordinal: r.ordinal,
        },
        Row::Quest(r) => Item::Quest { id: r.id },
        Row::Step(r) => Item::Step {
            quest: r.quest,
            key: r.key.clone(),
        },
        Row::Give(r) => Item::Give { key: r.key.clone() },
        Row::Talk(r) => Item::Talk { key: r.key.clone() },
        Row::Craft(r) => Item::Craft { id: r.id() },
        Row::Butchery(r) => Item::Butchery { profile: r.profile },
        Row::Scene(r) => Item::Scene { id: r.id },
        Row::Shard(r) => Item::Shard { object: r.object },
        Row::Weapon(r) => Item::Weapon {
            name: r.name.clone(),
        },
        Row::Route(r) => Item::Route { key: r.key.clone() },
        Row::Area(r) => Item::Area {
            landblock: r.landblock,
        },
        Row::Remove(_) => return None,
    })
}

/// The item key a row sets: `object KEY@XXYY`, `camp KEY@XXYY`, `place KEY`, `loot 0xVALUE`,
/// `keeper 0xTEMPLATE`, `sale 0xSTORE ORDINAL`, `quest 0xID`, `step 0xQUEST KEY`, `give KEY`,
/// `talk KEY`.
#[must_use]
pub fn item_key(l: &Located) -> Option<String> {
    item(l).map(|i| i.to_string())
}

/// What a row sets, to tell two rows that agree from two that differ: everything but where it
/// came from.
fn value(row: &Row) -> String {
    let mut r = row.clone();
    match &mut r {
        Row::Object(x) => x.prov = crate::rows::Prov::default(),
        Row::Camp(x) => x.prov = crate::rows::Prov::default(),
        Row::Loot(x) => x.prov = crate::rows::Prov::default(),
        Row::Keeper(x) => x.prov = crate::rows::Prov::default(),
        Row::Sale(x) => x.prov = crate::rows::Prov::default(),
        Row::Quest(x) => x.prov = crate::rows::Prov::default(),
        Row::Step(x) => x.prov = crate::rows::Prov::default(),
        Row::Give(x) => x.prov = crate::rows::Prov::default(),
        Row::Place(x) => x.prov = crate::rows::Prov::default(),
        Row::Talk(x) => x.prov = crate::rows::Prov::default(),
        Row::Craft(x) => x.prov = crate::rows::Prov::default(),
        Row::Butchery(x) => x.prov = crate::rows::Prov::default(),
        Row::Scene(x) => x.prov = crate::rows::Prov::default(),
        Row::Shard(x) => x.prov = crate::rows::Prov::default(),
        Row::Weapon(x) => x.prov = crate::rows::Prov::default(),
        Row::Route(x) => x.prov = crate::rows::Prov::default(),
        Row::Area(x) => x.prov = crate::rows::Prov::default(),
        Row::Remove(_) => {}
    }
    format!("{r:?}")
}

/// The merged rows, in load order (an overriding row takes the place of the row it replaces).
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Merged {
    pub rows: Vec<Located>,
    /// What a build should say but need not stop for: a removal of an item no pack sets.
    pub warnings: Vec<Problem>,
    /// The items packs set differently that the world settled, each as the report says it.
    pub overridden: Vec<String>,
}

/// Merge a database's packs.
///
/// # Errors
/// Every conflict (two packs setting one item differently without an override saying which wins)
/// and every item set twice within one pack.
pub fn merge(db: &Database) -> Result<Merged, Vec<Problem>> {
    let mut order: Vec<String> = Vec::new();
    let mut by_item: BTreeMap<String, (conflict::Item, Vec<&Located>)> = BTreeMap::new();
    let mut problems = Vec::new();
    let mut removes = Vec::new();
    for pack in &db.packs {
        for l in &pack.rows {
            if let Row::Remove(items) = &l.row {
                removes.extend(items.iter().map(|i| (i.clone(), l)));
                continue;
            }
            let Some(it) = item(l) else { continue };
            let key = it.to_string();
            let entry = by_item.entry(key.clone()).or_insert_with(|| {
                order.push(key.clone());
                let mut c = conflict::Item::new(key.clone());
                if let Some(lb) = it.landblock() {
                    c = c.in_landblock(lb);
                }
                (c, Vec::new())
            });
            if let Some(prev) = entry.1.iter().find(|p| p.pack == l.pack) {
                problems.push(Problem::new(
                    &l.file,
                    l.line,
                    format!(
                        "{key} is set twice in pack {} (also {}:{})",
                        l.pack, prev.file, prev.line
                    ),
                ));
                continue;
            }
            entry.1.push(l);
        }
    }
    let mut settled = Settled::default();
    let mut chosen: BTreeMap<String, Located> = BTreeMap::new();
    for key in &order {
        let (it, rows) = &by_item[key];
        let claims: Vec<Claim> = rows
            .iter()
            .map(|l| {
                Claim::new(
                    &l.pack,
                    &Loc::new(Path::new(&l.file), l.line),
                    format!("sets it ({})", l.row.kind()),
                    value(&l.row),
                )
            })
            .collect();
        let before = settled.conflicts.len();
        let w = settle(&db.world.overrides, it, &claims, &mut settled);
        if settled.conflicts.len() > before {
            continue;
        }
        chosen.insert(key.clone(), rows[w].clone());
    }
    for c in &settled.conflicts {
        let last = c.claims.last().map_or((String::new(), 0), |l| {
            (l.at.file.to_string_lossy().replace('\\', "/"), l.at.line)
        });
        let places: Vec<String> = c
            .claims
            .iter()
            .map(|cl| format!("pack {} ({})", cl.pack, cl.at))
            .collect();
        let (first, last_pack) = (
            c.claims.first().map_or("", |x| x.pack.as_str()),
            c.claims.last().map_or("", |x| x.pack.as_str()),
        );
        problems.push(Problem::new(
            &last.0,
            last.1,
            format!(
                "{} is set by {}: say which wins with [[override]] pack = \"{last_pack}\" over = \"{first}\" in world.toml",
                c.what,
                places.join(" and ")
            ),
        ));
    }
    let mut warnings = Vec::new();
    for (item, l) in removes {
        if chosen.remove(&item).is_none() && !by_item.contains_key(&item) {
            warnings.push(Problem::new(
                &l.file,
                l.line,
                format!("remove: no pack sets {item}"),
            ));
        }
    }
    if !problems.is_empty() {
        problems.sort();
        return Err(problems);
    }
    // In load order: each item where its first row stood, holding the row that stands.
    let rows = order.iter().filter_map(|k| chosen.remove(k)).collect();
    Ok(Merged {
        rows,
        warnings,
        overridden: settled.overridden.iter().map(ToString::to_string).collect(),
    })
}
