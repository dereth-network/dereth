//! `empyrean-import gdle`: content written for GDLE, for the worlds that run it.
//!
//! ```text
//! empyrean-import gdle --sql <dump.sql> --patches <dir|file>... [--quests <quests.json>]
//!                      [--now <YYYY-MM-DD HH:MM:SS>] --out <dir>
//! ```
//!
//! The patches (ACE-style SQL content: a world builder's filled SQL, say) are applied over the dump
//! as a build applies them. Everything they reached and that is still there is then written under
//! `--out`, by the converters ACE's `sql2json` commands use:
//! - `weenies/<class> - <class name>.json`: each weenie, as Lifestoned's JSON;
//! - `landblocks/<XXYY>.json`: each landblock whose spawns they wrote, with every spawn it has,
//!   as a GDLE landblock (with the weenies' names, as `sql2json` writes them);
//! - `quests/<name>.json`: each quest, as a GDLE quest;
//! - `quests.json`: the quests the patches add or change, in the list GDLE reads its quests from;
//!   with `--quests` (a server's own `quests.json`), that list with them merged in. A quest the
//!   list has keeps its entry unless the patches change it, when theirs takes its place, or take
//!   it away, when its entry goes; the rest follow the list's, by name. A quest's name is the same
//!   name in any case, as GDLE reads it;
//! - `quests-removed.json`: the names of the quests the patches take away (the dump has them and
//!   the world does not), when there are any: the entries a server's own list is to lose.
//!
//! A weenie, a landblock's spawns or a quest the patches took away is not written. `--now` stands
//! in for the time Lifestoned's JSON is stamped with (2000-01-01 00:00:00 by default), so the
//! output is the same for the same inputs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};
use empyrean_content::export::json as export_json;
use empyrean_content::export::json::models::QuestValue;
use empyrean_content::export::json::GdleQuest;
use empyrean_content::import::patch::{self, Input, InputKind};
use empyrean_content::import::sql::store::Store;
use empyrean_content::import::world_rows::WorldRows;
use empyrean_content::import::{self, load_store};
use empyrean_content::models::world::{LandblockInstance, Quest};
use empyrean_content::pack::TableId;
use empyrean_content::WorldContent;

/// `PropertyString.Name`.
const NAME: u16 = 1;

/// `gdle`'s options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub sql: PathBuf,
    pub patches: Vec<PathBuf>,
    /// A server's own `quests.json`, which the patches' quests are merged into.
    pub quests: Option<PathBuf>,
    pub now: DotNetDateTime,
    pub out: PathBuf,
}

/// Reads `gdle`'s options (the arguments after `gdle`); `parse_now` reads `--now`.
pub fn parse_args(
    args: &[String],
    parse_now: impl Fn(&str) -> Option<DotNetDateTime>,
) -> Result<Options, String> {
    let (mut sql, mut out, mut quests, mut now) = (None, None, None, import::default_now());
    let mut patches = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} takes a value"));
        match a.as_str() {
            "--sql" => sql = Some(PathBuf::from(value()?)),
            "--patches" => patches.push(PathBuf::from(value()?)),
            "--quests" => quests = Some(PathBuf::from(value()?)),
            "--out" => out = Some(PathBuf::from(value()?)),
            "--now" => {
                let v = value()?;
                now = parse_now(v)
                    .ok_or_else(|| format!("--now wants YYYY-MM-DD HH:MM:SS, got {v:?}"))?;
            }
            other => return Err(format!("{other} is not one of gdle's switches")),
        }
    }
    if patches.is_empty() {
        return Err("--patches names the content to write".into());
    }
    Ok(Options {
        sql: sql.ok_or("--sql names the dump the content is applied over")?,
        patches,
        quests,
        now,
        out: out.ok_or("--out names the folder to write")?,
    })
}

/// What a run wrote, file by file.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Written {
    pub weenies: Vec<PathBuf>,
    pub landblocks: Vec<PathBuf>,
    pub quests: Vec<PathBuf>,
    /// `quests.json`, and how many quests it lists.
    pub quest_list: Option<(PathBuf, usize)>,
    /// The quests the patches add or change, by name.
    pub own_quests: Vec<String>,
    /// The quests the patches take away, by name: the dump has them and the world does not.
    pub removed_quests: Vec<String>,
}

/// Runs `gdle`.
pub fn run(opts: &Options) -> Result<String, String> {
    let file =
        std::fs::File::open(&opts.sql).map_err(|e| format!("{}: {e}", opts.sql.display()))?;
    let inputs: Vec<Input> = opts
        .patches
        .iter()
        .map(|p| Input {
            kind: InputKind::Sql,
            path: p.clone(),
        })
        .collect();
    let list = opts.quests.as_deref().map(read_quest_list).transpose()?;
    // The dump's quests before the patches, which tell a quest the patches change from one they
    // leave as it was.
    let mut loaded = load_store(file, std::iter::empty(), opts.now).map_err(|e| e.to_string())?;
    let before = quests_of(&loaded.store)?;
    for src in patch::sources(&inputs) {
        let src = src.map_err(|e| e.to_string())?;
        let applied =
            patch::apply_source(&mut loaded.store, &src, opts.now).map_err(|e| e.to_string())?;
        loaded.applied.push(applied);
    }
    let mut rows = WorldRows::default();
    loaded.store.emit(&mut rows).map_err(|e| e.to_string())?;
    let (content, _) = rows.assemble();
    let touched: BTreeSet<(TableId, u64)> = loaded
        .applied
        .iter()
        .flat_map(|a| a.touched.iter().copied())
        .collect();
    let w = write(&content, &touched, &before, list, &opts.out, opts.now)?;
    Ok(format!(
        "{}: {} weenie(s), {} landblock(s), {} quest(s); quests.json lists {}, {} of them the          patches'; {} removed",
        opts.out.display(),
        w.weenies.len(),
        w.landblocks.len(),
        w.quests.len(),
        w.quest_list.as_ref().map_or(0, |(_, n)| *n),
        w.own_quests.len(),
        w.removed_quests.len()
    ))
}

/// Every quest `store` holds.
fn quests_of(store: &Store) -> Result<Vec<Quest>, String> {
    let Ok(table) = store.table_index("quest") else {
        return Ok(Vec::new());
    };
    let slots = store.slot_counts().get(table).copied().unwrap_or(0);
    let slots = u32::try_from(slots).map_err(|_| "the quest table is too long".to_owned())?;
    let selected = BTreeMap::from([(table, (0..slots).collect())]);
    let mut rows = WorldRows::default();
    store
        .emit_rows(&mut rows, &selected)
        .map_err(|e| e.to_string())?;
    Ok(rows.assemble().0.quests)
}

/// A server's own `quests.json`: GDLE's list of quests, each a `key` (its name) and a `value`
/// (`fullname`, `mindelta`, `maxsolves`).
pub fn read_quest_list(path: &Path) -> Result<Vec<GdleQuest>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let bad = || format!("{} is not GDLE's quests.json", path.display());
    let list: serde_json::Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).map_err(|_| bad())?;
    list.as_array()
        .ok_or_else(bad)?
        .iter()
        .map(|entry| {
            let key = entry.get("key")?.as_str()?.to_owned();
            let value = entry.get("value")?;
            let int = |name: &str| {
                value
                    .get(name)
                    .and_then(serde_json::Value::as_i64)
                    .and_then(|n| i32::try_from(n).ok())
            };
            Some(GdleQuest {
                key: Some(key),
                value: Some(QuestValue {
                    fullname: value
                        .get("fullname")
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_owned),
                    maxsolves: int("maxsolves")?,
                    mindelta: int("mindelta")?,
                }),
            })
        })
        .collect::<Option<Vec<_>>>()
        .ok_or_else(bad)
}

/// `own` merged into `list`: each quest in place of the entries of its name (in any case), the
/// rest after the list's, in their order; and the entries of the quests named in `removed` (in
/// any case) taken out.
#[must_use]
pub fn merge_quests(
    mut list: Vec<GdleQuest>,
    own: Vec<GdleQuest>,
    removed: &[String],
) -> Vec<GdleQuest> {
    let name = |q: &GdleQuest| q.key.as_deref().unwrap_or_default().to_lowercase();
    let gone: BTreeSet<String> = removed.iter().map(|r| r.to_lowercase()).collect();
    list.retain(|e| !gone.contains(&name(e)));
    let mut rest = Vec::new();
    for quest in own {
        let mut placed = false;
        for entry in list.iter_mut().filter(|e| name(e) == name(&quest)) {
            entry.clone_from(&quest);
            placed = true;
        }
        if !placed {
            rest.push(quest);
        }
    }
    list.extend(rest);
    list
}

/// A name as a file takes it: the characters a file name cannot hold made `_`.
fn file_name(s: &str) -> String {
    s.chars()
        .map(|c| {
            if c.is_control() || matches!(c, '<' | '>' | ':' | '"' | '/' | '\\' | '|' | '?' | '*') {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn put(dir: &Path, name: &str, text: &str) -> Result<PathBuf, String> {
    std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let path = dir.join(name);
    std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(path)
}

/// Write what `touched` names of `content` under `out`, as GDLE reads it; `now` stamps the
/// Lifestoned JSON. `before` is every quest the world had before the patches; `list`, a
/// server's own quest list to merge the patches' quests into.
pub fn write(
    content: &WorldContent,
    touched: &BTreeSet<(TableId, u64)>,
    before: &[Quest],
    list: Option<Vec<GdleQuest>>,
    out: &Path,
    now: DotNetDateTime,
) -> Result<Written, String> {
    let mut w = Written::default();
    let reached = |t: TableId| -> BTreeSet<u64> {
        touched
            .iter()
            .filter(|(table, _)| *table == t)
            .map(|(_, k)| *k)
            .collect()
    };
    let weenies = reached(TableId::WEENIE);
    for weenie in content
        .weenies
        .iter()
        .filter(|x| weenies.contains(&u64::from(x.class_id)))
    {
        let (json, _) = export_json::try_convert_ace_weenie_to_lsd_json(weenie, now)
            .ok_or_else(|| format!("weenie {} does not convert", weenie.class_id))?;
        w.weenies.push(put(
            &out.join("weenies"),
            &format!(
                "{} - {}.json",
                weenie.class_id,
                file_name(&weenie.class_name)
            ),
            &json,
        )?);
    }
    let landblocks = reached(TableId::LANDBLOCK_INSTANCE);
    if !landblocks.is_empty() {
        // The weenies' names, as the server's `sql2json` gives them, in class-id order.
        let mut by_class: Vec<_> = content.weenies.iter().collect();
        by_class.sort_by_key(|x| x.class_id);
        let (mut names, mut class_names) = (DotNetDict::new(), DotNetDict::new());
        for x in by_class {
            let name = x
                .weenie_properties_string
                .iter()
                .find(|p| p.r#type == NAME)
                .map(|p| p.value.clone());
            names.add(x.class_id, name.unwrap_or_default());
            class_names.add(x.class_id, x.class_name.clone());
        }
        let mut by_block: BTreeMap<u64, Vec<LandblockInstance>> = BTreeMap::new();
        for i in &content.landblock_instances {
            let lb = u64::from(i.obj_cell_id >> 16);
            if landblocks.contains(&lb) {
                let mut i = i.clone();
                i.landblock = i32::try_from(lb).ok();
                by_block.entry(lb).or_default().push(i);
            }
        }
        for (lb, instances) in &by_block {
            let converted =
                export_json::try_convert_landblock(instances, Some(&names), Some(&class_names))
                    .ok_or_else(|| format!("landblock {lb:04X} does not convert"))?;
            let json =
                export_json::serialize(&converted).map_err(|e| format!("{lb:04X}: {e:?}"))?;
            w.landblocks.push(put(
                &out.join("landblocks"),
                &format!("{lb:04X}.json"),
                &json,
            )?);
        }
    }
    let quests = reached(TableId::QUEST);
    for q in content
        .quests
        .iter()
        .filter(|q| quests.contains(&u64::from(q.id)))
    {
        let converted = export_json::try_convert_quest(q)
            .ok_or_else(|| format!("quest {} does not convert", q.name))?;
        let json = export_json::serialize(&converted).map_err(|e| format!("{}: {e:?}", q.name))?;
        w.quests.push(put(
            &out.join("quests"),
            &format!("{}.json", file_name(&q.name)),
            &json,
        )?);
    }
    // The quests the patches add or change, by name: one they leave as the dump has it, as GDLE
    // reads a quest, is the world database's and not theirs.
    let convert = |q: &Quest| {
        export_json::try_convert_quest(q)
            .ok_or_else(|| format!("quest {} does not convert", q.name))
    };
    let was: BTreeMap<String, &Quest> = before.iter().map(|q| (q.name.to_lowercase(), q)).collect();
    let mut own = Vec::new();
    for q in content
        .quests
        .iter()
        .filter(|q| quests.contains(&u64::from(q.id)))
    {
        let quest = convert(q)?;
        let unchanged = match was.get(&q.name.to_lowercase()) {
            Some(b) => convert(b)?.value == quest.value,
            None => false,
        };
        if !unchanged {
            own.push(quest);
        }
    }
    own.sort_by(|a, b| a.key.cmp(&b.key));
    w.own_quests = own.iter().filter_map(|q| q.key.clone()).collect();
    // The quests the patches take away: the dump has them, and the world they make does not.
    let now: BTreeSet<String> = content
        .quests
        .iter()
        .map(|q| q.name.to_lowercase())
        .collect();
    let mut removed: Vec<String> = before
        .iter()
        .filter(|q| !now.contains(&q.name.to_lowercase()))
        .map(|q| q.name.clone())
        .collect();
    removed.sort_by_key(|n| n.to_lowercase());
    removed.dedup_by(|a, b| a.eq_ignore_ascii_case(b));
    let list = merge_quests(list.unwrap_or_default(), own, &removed);
    let json = export_json::serialize(&list).map_err(|e| format!("quests.json: {e:?}"))?;
    w.quest_list = Some((put(out, "quests.json", &json)?, list.len()));
    if !removed.is_empty() {
        let json = serde_json::to_string_pretty(&removed)
            .map_err(|e| format!("quests-removed.json: {e}"))?;
        put(out, "quests-removed.json", &json)?;
    }
    w.removed_quests = removed;
    Ok(w)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (checks repository tools: content written for GDLE)
    use super::*;
    use empyrean_content::models::world::{Quest, Weenie, WeeniePropertiesString};

    fn content() -> WorldContent {
        let weenie = |class_id: u32, class_name: &str, name: &str| Weenie {
            class_id,
            class_name: class_name.into(),
            r#type: 10,
            weenie_properties_string: vec![WeeniePropertiesString {
                object_id: class_id,
                r#type: NAME,
                value: name.into(),
                ..WeeniePropertiesString::default()
            }],
            ..Weenie::default()
        };
        let spawn = |guid: u32, class: u32, cell: u32| LandblockInstance {
            guid,
            weenie_class_id: class,
            obj_cell_id: cell,
            ..LandblockInstance::default()
        };
        WorldContent {
            weenies: vec![
                weenie(7, "drudgeskulker", "Drudge Skulker"),
                weenie(100_001, "bessie", "Bessie"),
            ],
            landblock_instances: vec![
                spawn(0x7A9B_4FFE, 100_001, 0xA9B4_0009),
                spawn(0x7A9B_4001, 7, 0xA9B4_0010),
                spawn(0x7A9B_5001, 7, 0xA9B5_0010),
            ],
            quests: vec![
                Quest {
                    id: 3,
                    name: "BessieMilked".into(),
                    min_delta: 600,
                    max_solves: -1,
                    message: Some("Milked".into()),
                    ..Quest::default()
                },
                Quest {
                    id: 4,
                    name: "AncientQuest".into(),
                    min_delta: 1,
                    max_solves: 1,
                    ..Quest::default()
                },
            ],
            ..WorldContent::default()
        }
    }

    #[test]
    fn what_the_patches_reached_is_written_and_nothing_else() {
        let out = std::env::temp_dir().join(format!("empyrean-gdle-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        let touched: BTreeSet<(TableId, u64)> = [
            (TableId::WEENIE, 100_001),
            (TableId::WEENIE, 90_000), // taken away: not there to write
            (TableId::LANDBLOCK_INSTANCE, 0xA9B4),
            (TableId::QUEST, 3),
        ]
        .into_iter()
        .collect();
        let w = write(&content(), &touched, &[], None, &out, import::default_now()).unwrap();
        assert_eq!(
            w.weenies,
            [out.join("weenies").join("100001 - bessie.json")]
        );
        assert_eq!(w.landblocks, [out.join("landblocks").join("A9B4.json")]);
        assert_eq!(w.quests, [out.join("quests").join("BessieMilked.json")]);
        let lsd = std::fs::read_to_string(&w.weenies[0]).unwrap();
        assert!(lsd.contains("100001") && lsd.contains("Bessie"), "{lsd}");
        // Every spawn of the landblock, with its weenie's name; none of the next.
        let lb = std::fs::read_to_string(&w.landblocks[0]).unwrap();
        assert!(
            lb.contains(&0x7A9B_4FFEu32.to_string()) && lb.contains(&0x7A9B_4001u32.to_string()),
            "{lb}"
        );
        assert!(!lb.contains(&0x7A9B_5001u32.to_string()), "{lb}");
        assert!(
            lb.contains("Bessie") && lb.contains("Drudge Skulker"),
            "{lb}"
        );
        let q = std::fs::read_to_string(&w.quests[0]).unwrap();
        assert!(q.contains("BessieMilked") && q.contains("600"), "{q}");
        // With no server's list, quests.json holds the quest the patches added, and not the
        // world database's own.
        let (path, n) = w.quest_list.clone().unwrap();
        assert_eq!((path.clone(), n), (out.join("quests.json"), 1));
        let list: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(keys(&list), ["BessieMilked"]);
        assert_eq!(list[0]["value"]["mindelta"], 600);
        assert_eq!(list[0]["value"]["fullname"], "Milked");
        assert_eq!(w.own_quests, ["BessieMilked"]);
        assert!(w.removed_quests.is_empty());
        assert!(
            !out.join("quests-removed.json").exists(),
            "nothing taken away, no list of it"
        );
        let _ = std::fs::remove_dir_all(&out);
    }

    fn keys(list: &serde_json::Value) -> Vec<&str> {
        list.as_array()
            .unwrap()
            .iter()
            .map(|e| e["key"].as_str().unwrap())
            .collect()
    }

    /// A server's own quest list keeps every entry the patches leave alone, and takes theirs in
    /// place of an entry they change (its name in any case); a quest they add follows the list's,
    /// and one they write as the world database has it is not theirs. The entry of a quest they
    /// take away goes, and `quests-removed.json` names it.
    #[test]
    fn the_patches_quests_are_merged_into_a_servers_own_list() {
        let out = std::env::temp_dir().join(format!("empyrean-gdle-merge-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&out);
        std::fs::create_dir_all(&out).unwrap();
        let host = out.join("host-quests.json");
        std::fs::write(
            &host,
            "\u{feff}[\n\
             {\"key\": \"HostOnly\", \"value\": {\"fullname\": \"Mine\", \"mindelta\": 5, \"maxsolves\": 2}},\n\
             {\"key\": \"bessiemilked\", \"value\": {\"fullname\": \"Host's\", \"mindelta\": 1, \"maxsolves\": 1}},\n\
             {\"key\": \"AncientQuest\", \"value\": {\"fullname\": null, \"mindelta\": 9, \"maxsolves\": 9}},\n\
             {\"key\": \"gonequest\", \"value\": {\"fullname\": null, \"mindelta\": 1, \"maxsolves\": 1}}\n\
             ]",
        )
        .unwrap();
        let list = read_quest_list(&host).unwrap();
        assert_eq!(list.len(), 4);
        // The dump had three quests; the patches change BessieMilked, write AncientQuest as it
        // was, take GoneQuest away and add a fourth.
        let mut world = content();
        world.quests.push(Quest {
            id: 5,
            name: "NewQuest".into(),
            min_delta: 60,
            max_solves: 3,
            ..Quest::default()
        });
        let mut before = content().quests;
        before[0].min_delta = 300;
        before.push(Quest {
            id: 6,
            name: "GoneQuest".into(),
            ..Quest::default()
        });
        let touched: BTreeSet<(TableId, u64)> = [
            (TableId::QUEST, 3),
            (TableId::QUEST, 4),
            (TableId::QUEST, 5),
            (TableId::QUEST, 6),
        ]
        .into_iter()
        .collect();
        let w = write(
            &world,
            &touched,
            &before,
            Some(list),
            &out,
            import::default_now(),
        )
        .unwrap();
        assert_eq!(w.own_quests, ["BessieMilked", "NewQuest"]);
        let (path, n) = w.quest_list.clone().unwrap();
        assert_eq!(n, 4);
        let merged: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            keys(&merged),
            ["HostOnly", "BessieMilked", "AncientQuest", "NewQuest"]
        );
        assert_eq!(merged[0]["value"]["fullname"], "Mine");
        assert_eq!(merged[1]["value"]["mindelta"], 600, "the patches' entry");
        assert_eq!(
            merged[2]["value"]["mindelta"], 9,
            "the server's entry is kept"
        );
        assert_eq!(merged[3]["value"]["maxsolves"], 3);
        assert_eq!(w.removed_quests, ["GoneQuest"]);
        let removed: serde_json::Value = serde_json::from_str(
            &std::fs::read_to_string(out.join("quests-removed.json")).unwrap(),
        )
        .unwrap();
        assert_eq!(removed, serde_json::json!(["GoneQuest"]));

        std::fs::write(&host, "{\"quests\": []}").unwrap();
        assert!(read_quest_list(&host)
            .unwrap_err()
            .contains("is not GDLE's quests.json"));
        let _ = std::fs::remove_dir_all(&out);
    }

    #[test]
    fn gdle_reads_its_dump_patches_clock_and_folder() {
        let args = |s: &[&str]| s.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>();
        let now = |_: &str| Some(DotNetDateTime::new_hms(2026, 10, 19, 0, 0, 0));
        let o = parse_args(
            &args(&[
                "--sql",
                "d.sql",
                "--patches",
                "a",
                "--patches",
                "b.sql",
                "--quests",
                "q.json",
                "--now",
                "x",
                "--out",
                "o",
            ]),
            now,
        )
        .unwrap();
        assert_eq!(o.patches, [PathBuf::from("a"), PathBuf::from("b.sql")]);
        assert_eq!(o.quests, Some(PathBuf::from("q.json")));
        assert_eq!(o.now, DotNetDateTime::new_hms(2026, 10, 19, 0, 0, 0));
        assert!(
            parse_args(&args(&["--sql", "d.sql", "--out", "o"]), now).is_err(),
            "no patches"
        );
        assert!(
            parse_args(&args(&["--patches", "a", "--out", "o"]), now).is_err(),
            "no dump"
        );
        assert!(parse_args(&args(&["--sql", "d", "--patches", "a", "--json", "j"]), now).is_err());
    }
}
