//! The treasure numbers a world plays by when it gives none, written out as data: the starting
//! point of a world's own. Set `YALAIN_TREASURE_EXPORT` to a file to have the test write them
//! there as JSON.

use serde_json::{json, Value};
use yalain_content::treasure_effects::{Tier, When};
use yalain_content::treasure_tables::{
    CountBand, Delays, Family, LookRow, LookRule, LoreRule, Rules, Sort, SortTables,
    TreasureTables, VigorBand, Word,
};

/// A number as it is written in code: its shortest form, which reads back the same.
fn n(x: f32) -> Value {
    json!(x.to_string().parse::<f64>().expect("a number"))
}

fn ns(xs: &[f32]) -> Value {
    Value::Array(xs.iter().map(|x| n(*x)).collect())
}

const fn when_name(w: When) -> &'static str {
    match w {
        When::Permanent => "permanent",
        When::Equipped => "equipped",
        When::Hit => "hit",
    }
}

/// The tables as data, a list for each kind a world's rows would hold, each weighted list in the
/// order it is drawn in.
fn export(t: &TreasureTables) -> Value {
    let r = &t.rules;
    let sorts: Vec<Value> = Sort::ALL
        .iter()
        .map(|&sort| {
            let s = t.sort(sort);
            let mut row = json!({
                "sort": sort.name(),
                "counts": s.counts.iter().map(|b| json!({"from": b.from, "weights": ns(&b.weights)})).collect::<Vec<_>>(),
            });
            let o = row.as_object_mut().expect("an object");
            if let Some((lo, hi)) = s.damage_share {
                o.insert("damage_share".into(), json!([n(lo), n(hi)]));
            }
            if !s.vigor.is_empty() {
                let bands = s.vigor.iter().map(|b| {
                    let mut v = json!({"from": b.from, "least": b.least, "most": b.most});
                    if let Some(every) = b.every {
                        v["every"] = json!(every);
                    }
                    v
                });
                o.insert("vigor".into(), Value::Array(bands.collect()));
            }
            if let Some((lo, hi)) = s.span {
                o.insert("span".into(), json!([n(lo), n(hi)]));
            }
            if let Some(d) = &s.delays {
                o.insert(
                    "delays".into(),
                    json!({"from": d.from, "weights": ns(&d.weights)}),
                );
            }
            if !s.rating_share.is_empty() {
                let points = s.rating_share.iter().map(|(l, x)| json!([l, n(*x)]));
                o.insert("rating_share".into(), Value::Array(points.collect()));
                o.insert("rating_spread".into(), n(s.rating_spread));
                o.insert("slot_chance".into(), n(s.slot_chance));
            }
            row
        })
        .collect();
    let words: Vec<Value> = Sort::ALL
        .iter()
        .flat_map(|&sort| {
            t.sort(sort)
                .words
                .iter()
                .enumerate()
                .map(move |(order, w)| {
                    json!({
                        "sort": sort.name(),
                        "order": order,
                        "word": w.word,
                        "delays": [w.delays.0, w.delays.1],
                        "weights": ns(&w.weights),
                    })
                })
        })
        .collect();
    let families: Vec<Value> = Sort::ALL
        .iter()
        .flat_map(|&sort| {
            t.sort(sort)
                .families
                .iter()
                .enumerate()
                .map(move |(order, f)| {
                    let tiers: Vec<Value> = f
                        .tiers
                        .iter()
                        .map(|x| {
                            json!({
                                "least": x.least,
                                "state": x.state,
                                "puts_on": x.puts_on,
                                "chance": n(x.chance),
                                "glow": n(x.glow),
                            })
                        })
                        .collect();
                    json!({
                        "sort": sort.name(),
                        "order": order,
                        "when": when_name(f.when),
                        "weight": n(f.weight),
                        "by_level": f.by_level,
                        "tiers": tiers,
                    })
                })
        })
        .collect();
    json!({
        "rules": {
            "above_base": ns(&r.above_base),
            "most_level": r.most_level,
            "lore": {"above": r.lore.above, "from": r.lore.from, "to": r.lore.to},
            "potion_tiers": r.potion_tiers,
            "potion_spellcraft": [n(r.potion_spellcraft.0), n(r.potion_spellcraft.1)],
            "look": {
                "glow_most": n(r.look.glow_most),
                "round_chance": n(r.look.round_chance),
                "round_from": n(r.look.round_from),
                "round_tenths": r.look.round_tenths,
            },
        },
        "sorts": sorts,
        "words": words,
        "looks": t.looks.iter().map(|l| json!({
            "template": l.template,
            "table": l.table,
            "keys": l.keys.iter().map(|(k, m)| json!({"key": k, "most": n(*m)})).collect::<Vec<_>>(),
        })).collect::<Vec<_>>(),
        "potions": t.potions.iter().enumerate().map(|(order, states)| json!({
            "kind": yalain_content::treasure_tables::POTION_KINDS.get(order).copied().unwrap_or("potion"),
            "order": order,
            "states": states,
        })).collect::<Vec<_>>(),
        "families": families,
        "pairs": t.pairs.iter().map(|(a, b)| json!([a, b])).collect::<Vec<_>>(),
        "unnaming": t.unnaming,
    })
}

fn f(v: &Value) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let x = v.as_f64().expect("a number") as f32;
    x
}

fn fs(v: &Value) -> Vec<f32> {
    v.as_array().expect("a list").iter().map(f).collect()
}

fn i(v: &Value) -> i32 {
    i32::try_from(v.as_i64().expect("a whole number")).expect("in range")
}

fn u(v: &Value) -> u32 {
    u32::try_from(v.as_u64().expect("a whole number")).expect("in range")
}

fn pair(v: &Value) -> (f32, f32) {
    (f(&v[0]), f(&v[1]))
}

/// The tables read back from their data, as a world's rows would give them.
fn import(v: &Value) -> TreasureTables {
    let r = &v["rules"];
    let mut sorts: Vec<SortTables> = Vec::new();
    for (k, &sort) in Sort::ALL.iter().enumerate() {
        let s = &v["sorts"][k];
        assert_eq!(s["sort"], sort.name());
        let of_sort = |list: &str| -> Vec<Value> {
            let mut rows: Vec<Value> = v[list]
                .as_array()
                .expect("a list")
                .iter()
                .filter(|x| x["sort"] == sort.name())
                .cloned()
                .collect();
            rows.sort_by_key(|x| x["order"].as_u64());
            rows
        };
        sorts.push(SortTables {
            counts: s["counts"]
                .as_array()
                .expect("bands")
                .iter()
                .map(|b| CountBand {
                    from: i(&b["from"]),
                    weights: fs(&b["weights"]),
                })
                .collect(),
            damage_share: s.get("damage_share").map(pair),
            vigor: s.get("vigor").map_or_else(Vec::new, |bands| {
                bands
                    .as_array()
                    .expect("bands")
                    .iter()
                    .map(|b| VigorBand {
                        from: i(&b["from"]),
                        least: i(&b["least"]),
                        most: i(&b["most"]),
                        every: b.get("every").map(i),
                    })
                    .collect()
            }),
            span: s.get("span").map(pair),
            delays: s.get("delays").map(|d| Delays {
                from: i(&d["from"]),
                weights: fs(&d["weights"]),
            }),
            rating_share: s.get("rating_share").map_or_else(Vec::new, |p| {
                p.as_array()
                    .expect("points")
                    .iter()
                    .map(|x| (i(&x[0]), f(&x[1])))
                    .collect()
            }),
            rating_spread: s.get("rating_spread").map_or(0.0, f),
            slot_chance: s.get("slot_chance").map_or(0.0, f),
            families: of_sort("families")
                .iter()
                .map(|x| Family {
                    when: match x["when"].as_str() {
                        Some("permanent") => When::Permanent,
                        Some("equipped") => When::Equipped,
                        _ => When::Hit,
                    },
                    weight: f(&x["weight"]),
                    by_level: x["by_level"].as_bool().expect("a flag"),
                    tiers: x["tiers"]
                        .as_array()
                        .expect("tiers")
                        .iter()
                        .map(|t| Tier {
                            least: i(&t["least"]),
                            state: u(&t["state"]),
                            puts_on: u(&t["puts_on"]),
                            chance: f(&t["chance"]),
                            glow: f(&t["glow"]),
                        })
                        .collect(),
                })
                .collect(),
            words: of_sort("words")
                .iter()
                .map(|x| Word {
                    word: u(&x["word"]),
                    delays: (i(&x["delays"][0]), i(&x["delays"][1])),
                    weights: fs(&x["weights"]),
                })
                .collect(),
        });
    }
    let [weapon, armor, shield, jewelry]: [SortTables; 4] = sorts.try_into().expect("four sorts");
    TreasureTables {
        rules: Rules {
            above_base: fs(&r["above_base"]),
            most_level: i(&r["most_level"]),
            lore: LoreRule {
                above: i(&r["lore"]["above"]),
                from: i(&r["lore"]["from"]),
                to: i(&r["lore"]["to"]),
            },
            potion_tiers: r["potion_tiers"]
                .as_array()
                .expect("edges")
                .iter()
                .map(i)
                .collect(),
            potion_spellcraft: pair(&r["potion_spellcraft"]),
            look: LookRule {
                glow_most: f(&r["look"]["glow_most"]),
                round_chance: f(&r["look"]["round_chance"]),
                round_from: f(&r["look"]["round_from"]),
                round_tenths: u(&r["look"]["round_tenths"]),
            },
        },
        weapon,
        armor,
        shield,
        jewelry,
        looks: v["looks"]
            .as_array()
            .expect("looks")
            .iter()
            .map(|l| LookRow {
                template: u(&l["template"]),
                table: u(&l["table"]),
                keys: l["keys"]
                    .as_array()
                    .expect("keys")
                    .iter()
                    .map(|k| (u(&k["key"]), f(&k["most"])))
                    .collect(),
            })
            .collect(),
        potions: v["potions"]
            .as_array()
            .expect("potions")
            .iter()
            .map(|p| {
                p["states"]
                    .as_array()
                    .expect("states")
                    .iter()
                    .map(u)
                    .collect()
            })
            .collect(),
        pairs: v["pairs"]
            .as_array()
            .expect("pairs")
            .iter()
            .map(|p| (u(&p[0]), u(&p[1])))
            .collect(),
        unnaming: v["unnaming"]
            .as_array()
            .expect("states")
            .iter()
            .map(u)
            .collect(),
    }
}

/// The 2017 treasure numbers written out as data read back exactly: every rule, every sort's bands,
/// shares and delays, every word, look, potion and family in its order, the pairs and the effects
/// nothing is named for.
/// Behaviour: server.loot.treasure-tables-written-as-data
#[test]
fn the_2017_treasure_numbers_written_as_data_read_back_the_same() {
    let tables = TreasureTables::retail_2017();
    let data = export(&tables);
    let text = serde_json::to_string_pretty(&data).expect("written");
    let read: Value = serde_json::from_str(&text).expect("read");
    assert_eq!(import(&read), tables);
    assert_eq!(read["words"].as_array().map(Vec::len), Some(33));
    assert_eq!(read["families"].as_array().map(Vec::len), Some(41));
    assert_eq!(read["looks"].as_array().map(Vec::len), Some(232));
    assert_eq!(read["potions"].as_array().map(Vec::len), Some(9));
    if let Some(path) = std::env::var_os("YALAIN_TREASURE_EXPORT") {
        std::fs::write(&path, text + "\n").expect("the export is written");
    }
}
