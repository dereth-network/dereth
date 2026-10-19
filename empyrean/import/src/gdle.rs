//! `empyrean-import gdle`: content written for GDLE, for the worlds that run it.
//!
//! ```text
//! empyrean-import gdle --sql <dump.sql> --patches <dir|file>... [--now <YYYY-MM-DD HH:MM:SS>] --out <dir>
//! ```
//!
//! The patches (ACE-style SQL content: a world builder's filled SQL, say) are applied over the dump
//! as a build applies them. Everything they reached and that is still there is then written under
//! `--out`, by the converters ACE's `sql2json` commands use:
//! - `weenies/<class> - <class name>.json`: each weenie, as Lifestoned's JSON;
//! - `landblocks/<XXYY>.json`: each landblock whose spawns they wrote, with every spawn it has,
//!   as a GDLE landblock (with the weenies' names, as `sql2json` writes them);
//! - `quests/<name>.json`: each quest, as a GDLE quest.
//!
//! A weenie, a landblock's spawns or a quest the patches took away is not written. `--now` stands
//! in for the time Lifestoned's JSON is stamped with (2000-01-01 00:00:00 by default), so the
//! output is the same for the same inputs.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use empyrean_common::dotnet::{DotNetDateTime, DotNetDict};
use empyrean_content::export::json as export_json;
use empyrean_content::import::patch::{self, Input, InputKind};
use empyrean_content::import::world_rows::WorldRows;
use empyrean_content::import::{self, load_store};
use empyrean_content::models::world::LandblockInstance;
use empyrean_content::pack::TableId;
use empyrean_content::WorldContent;

/// `PropertyString.Name`.
const NAME: u16 = 1;

/// `gdle`'s options.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Options {
    pub sql: PathBuf,
    pub patches: Vec<PathBuf>,
    pub now: DotNetDateTime,
    pub out: PathBuf,
}

/// Reads `gdle`'s options (the arguments after `gdle`); `parse_now` reads `--now`.
pub fn parse_args(
    args: &[String],
    parse_now: impl Fn(&str) -> Option<DotNetDateTime>,
) -> Result<Options, String> {
    let (mut sql, mut out, mut now) = (None, None, import::default_now());
    let mut patches = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or_else(|| format!("{a} takes a value"));
        match a.as_str() {
            "--sql" => sql = Some(PathBuf::from(value()?)),
            "--patches" => patches.push(PathBuf::from(value()?)),
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
    let loaded = load_store(file, patch::sources(&inputs), opts.now).map_err(|e| e.to_string())?;
    let mut rows = WorldRows::default();
    loaded.store.emit(&mut rows).map_err(|e| e.to_string())?;
    let (content, _) = rows.assemble();
    let touched: BTreeSet<(TableId, u64)> = loaded
        .applied
        .iter()
        .flat_map(|a| a.touched.iter().copied())
        .collect();
    let w = write(&content, &touched, &opts.out, opts.now)?;
    Ok(format!(
        "{}: {} weenie(s), {} landblock(s), {} quest(s)",
        opts.out.display(),
        w.weenies.len(),
        w.landblocks.len(),
        w.quests.len()
    ))
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
/// Lifestoned JSON.
pub fn write(
    content: &WorldContent,
    touched: &BTreeSet<(TableId, u64)>,
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
            quests: vec![Quest {
                id: 3,
                name: "BessieMilked".into(),
                min_delta: 600,
                max_solves: -1,
                message: Some("Milked".into()),
                ..Quest::default()
            }],
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
        let w = write(&content(), &touched, &out, import::default_now()).unwrap();
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
                "--now",
                "x",
                "--out",
                "o",
            ]),
            now,
        )
        .unwrap();
        assert_eq!(o.patches, [PathBuf::from("a"), PathBuf::from("b.sql")]);
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
