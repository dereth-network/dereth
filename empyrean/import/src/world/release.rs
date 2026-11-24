//! `empyrean-import world release`: a world's release, its packs and its manifest.
//!
//! ```text
//! empyrean-import world release --world <world.toml> --version <x.y.z> --out <folder> [--previous <manifest.json>] [--dumps <folder>] [--dats <folder>]...
//! ```
//!
//! The world is folded once, every layer checked as `world check` checks it (an error refuses the
//! release), and each layer's world known by its dataset id: a layer that changes nothing has the
//! world below it. A layer whose rows are written (the base, a checkpoint, a patch whose
//! `patch.toml` says `status = "written"`) has a pack, `<layer>.pack`, beside its world SQL,
//! `<layer>.world.sql` and `<layer>.empyrean.sql`; but a world already packed, by an earlier layer
//! of this release or in the release `--previous` describes, isn't packed again, and its layers
//! name that pack. A world no written layer has is named by its dataset id alone.
//!
//! `manifest.json` names every layer, in order:
//!
//! ```json
//! {
//!   "format": 2,
//!   "world": "empyrean-world",
//!   "release": "v0.1.0",
//!   "layers": [
//!     { "id": "base", "dataset_id": "…", "era": "launch", "written": true,
//!       "pack": { "release": "v0.1.0", "asset": "base.pack", "sha256": "…", "size": 116000000,
//!                 "sql": ["base.world.sql", "base.empyrean.sql"] } },
//!     { "id": "1999-12-sudden-season", "dataset_id": "…", "era": "launch", "written": false,
//!       "pack": { "release": "v0.1.0", "asset": "base.pack", … } },
//!     { "id": "…", "dataset_id": "…", "era": "launch", "written": false, "pack": null }
//!   ]
//! }
//! ```
//!
//! so one manifest, the newest, says where every layer's world is, and fetching a layer is
//! downloading one pack. A layer's `era` is the era its world's pack records. `SHA256SUMS` lists every file written.
//!
//! Memory is kept to a check's: the one fold holds one layer's world at a time and packs nothing;
//! each pack is made afterwards, its layer's world folded again on its own, and its dataset id
//! must be the one the fold found.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use empyrean_world_db::compare::dataset_of;
use empyrean_world_db::files::default_stamp;

/// The manifest's format: 2 gives each layer its era.
pub const FORMAT: u64 = 2;

/// Where a world's pack is: the release holding it, its asset, its SHA-256 and size, and its SQL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackRef {
    pub release: String,
    pub asset: String,
    pub sha256: String,
    pub size: u64,
    pub sql: Vec<String>,
}

/// One layer of the manifest: its id, its world's dataset id and era, whether its rows are written,
/// and the pack holding its world, if any does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Layer {
    pub id: String,
    pub dataset_id: String,
    /// The era its world's pack records, by name (`launch`, `eor`, …).
    pub era: String,
    pub written: bool,
    pub pack: Option<PackRef>,
}

/// A release's manifest: the world it is of, the release, and every layer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Manifest {
    pub world: String,
    pub release: String,
    pub layers: Vec<Layer>,
}

impl Manifest {
    /// The manifest as JSON, two spaces deep.
    #[must_use]
    pub fn to_json(&self) -> String {
        let layers: Vec<Value> = self
            .layers
            .iter()
            .map(|l| {
                json!({
                    "id": l.id,
                    "dataset_id": l.dataset_id,
                    "era": l.era,
                    "written": l.written,
                    "pack": l.pack.as_ref().map(|p| json!({
                        "release": p.release,
                        "asset": p.asset,
                        "sha256": p.sha256,
                        "size": p.size,
                        "sql": p.sql,
                    })),
                })
            })
            .collect();
        let v = json!({
            "format": FORMAT,
            "world": self.world,
            "release": self.release,
            "layers": layers,
        });
        let mut text = serde_json::to_string_pretty(&v).unwrap_or_default();
        text.push('\n');
        text
    }

    /// A manifest read back from its JSON.
    ///
    /// # Errors
    /// JSON that isn't a manifest of this format.
    pub fn parse(text: &str) -> Result<Self, String> {
        let v: Value = serde_json::from_str(text).map_err(|e| format!("not JSON: {e}"))?;
        if v["format"].as_u64() != Some(FORMAT) {
            return Err(format!("not a manifest of format {FORMAT}"));
        }
        let s = |v: &Value, k: &str| {
            v[k].as_str()
                .map(str::to_owned)
                .ok_or_else(|| format!("no {k}"))
        };
        let mut layers = Vec::new();
        for l in v["layers"].as_array().ok_or("no layers")? {
            let pack = match &l["pack"] {
                Value::Null => None,
                p => Some(PackRef {
                    release: s(p, "release")?,
                    asset: s(p, "asset")?,
                    sha256: s(p, "sha256")?,
                    size: p["size"].as_u64().ok_or("no size")?,
                    sql: p["sql"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(Value::as_str)
                                .map(str::to_owned)
                                .collect()
                        })
                        .unwrap_or_default(),
                }),
            };
            layers.push(Layer {
                id: s(l, "id")?,
                dataset_id: s(l, "dataset_id")?,
                era: s(l, "era")?,
                written: l["written"].as_bool().unwrap_or(false),
                pack,
            });
        }
        Ok(Self {
            world: s(&v, "world")?,
            release: s(&v, "release")?,
            layers,
        })
    }
}

/// Lowercase hex.
pub(super) fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// A world's release: every layer folded and checked once, the packs of the written layers whose
/// worlds aren't packed already written into `out`, and the manifest. `open` reads the world's
/// layers afresh (once for the fold, then once for each pack); `key` is the world's own name (its
/// `world.toml`'s `key`), `release` the release's tag, and `previous` the manifest of the release
/// before.
///
/// # Errors
/// A world that doesn't read, any error a check finds, a manifest of another world, a layer whose
/// world folds otherwise the second time, and files that can't be written.
pub fn release(
    open: &dyn Fn() -> Result<empyrean_world_db::layers::Folding, String>,
    key: &str,
    release: &str,
    previous: Option<&Manifest>,
    out: &Path,
) -> Result<Manifest, String> {
    if let Some(p) = previous {
        if p.world != key {
            return Err(format!(
                "the previous manifest is of {}, not {key}",
                p.world
            ));
        }
        if p.release == release {
            return Err(format!("the previous manifest is of {release} itself"));
        }
    }
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let folding = open()?;
    let ids = folding.ids.clone();
    let written = folding.written().to_vec();
    // Every world packed already, by its dataset id: the previous release's, then this one's.
    let mut packed: BTreeMap<String, PackRef> = BTreeMap::new();
    if let Some(p) = previous {
        for l in &p.layers {
            if let Some(pack) = &l.pack {
                packed
                    .entry(l.dataset_id.clone())
                    .or_insert_with(|| pack.clone());
            }
        }
    }
    // The one fold: each layer's dataset id, and the written layers whose worlds need a pack.
    let mut datasets: Vec<Option<(String, &'static str)>> = vec![None; ids.len()];
    let mut to_pack: BTreeMap<String, usize> = BTreeMap::new();
    let mut failure: Option<String> = None;
    let checks = folding.check_each(&mut |n, c| {
        if failure.is_some() {
            return;
        }
        let era = c.era.name();
        match dataset_of_world(c) {
            Ok((dataset, _)) => {
                let id = hex(&dataset);
                if written.get(n).copied().unwrap_or(false) && !packed.contains_key(&id) {
                    to_pack.entry(id.clone()).or_insert(n);
                }
                datasets[n] = Some((id, era));
            }
            Err(e) => failure = Some(format!("{}: {e}", ids[n])),
        }
    });
    if let Some(e) = failure {
        return Err(e);
    }
    let errors: Vec<String> = checks
        .iter()
        .flat_map(|l| {
            l.errors()
                .into_iter()
                .map(move |e| format!("{}: {e}", l.id))
        })
        .collect();
    if !errors.is_empty() {
        return Err(format!(
            "the world has errors; nothing released:\n{}",
            errors.join("\n")
        ));
    }
    // Each pack, its layer's world folded again on its own.
    for (id, &n) in &to_pack {
        let c = open()?.checked_content_at(n)?;
        let (dataset, c) = dataset_of_world(c).map_err(|e| format!("{}: {e}", ids[n]))?;
        if hex(&dataset) != *id {
            return Err(format!(
                "{}: its world folded on its own is dataset {}, not the fold's {id}",
                ids[n],
                hex(&dataset)
            ));
        }
        let p = pack_layer(&c, dataset, &ids[n], release, out)?;
        packed.insert(id.clone(), p);
    }
    let mut layers = Vec::with_capacity(ids.len());
    let mut below: Option<(String, &'static str)> = None;
    for (n, id) in ids.iter().enumerate() {
        // A layer that changes nothing has the world below it.
        let (dataset, era) = datasets[n]
            .clone()
            .or_else(|| below.clone())
            .ok_or_else(|| format!("{id}: no world"))?;
        layers.push(Layer {
            id: id.clone(),
            dataset_id: dataset.clone(),
            era: era.to_owned(),
            written: written.get(n).copied().unwrap_or(false),
            pack: packed.get(&dataset).cloned(),
        });
        below = Some((dataset, era));
    }
    let manifest = Manifest {
        world: key.to_owned(),
        release: release.to_owned(),
        layers,
    };
    write(out, "manifest.json", manifest.to_json().as_bytes())?;
    write_sums(out)?;
    Ok(manifest)
}

/// A layer's world numbered by rule, and its dataset id: the hash of its rows as written.
fn dataset_of_world(
    mut c: empyrean_content::WorldContent,
) -> Result<([u8; 16], empyrean_content::WorldContent), String> {
    empyrean_world_db::number(&mut c);
    let files = empyrean_world_db::write(&c, default_stamp())?;
    Ok((dataset_of(&files), c))
}

/// Layer `layer`'s pack and its world SQL written into `out`, for release `release`.
fn pack_layer(
    c: &empyrean_content::WorldContent,
    dataset: [u8; 16],
    layer: &str,
    release: &str,
    out: &Path,
) -> Result<PackRef, String> {
    let (bytes, _) = c.to_pack(dataset).map_err(|e| format!("{layer}: {e}"))?;
    let asset = format!("{layer}.pack");
    write(out, &asset, &bytes)?;
    let sha256 = hex(&Sha256::digest(&bytes));
    let size = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
    drop(bytes);
    let s = crate::world_sql::write(c)?;
    let mut sql = Vec::new();
    for (suffix, text) in [("world.sql", &s.world), ("empyrean.sql", &s.empyrean)] {
        let name = format!("{layer}.{suffix}");
        write(out, &name, text.as_bytes())?;
        sql.push(name);
    }
    Ok(PackRef {
        release: release.to_owned(),
        asset,
        sha256,
        size,
        sql,
    })
}

fn write(dir: &Path, name: &str, bytes: &[u8]) -> Result<(), String> {
    let path = dir.join(name);
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// `SHA256SUMS` over every file in `dir` but itself, by name.
fn write_sums(dir: &Path) -> Result<(), String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map_err(|e| format!("{}: {e}", dir.display()))?
        .filter_map(Result::ok)
        .filter(|e| e.path().is_file())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n != "SHA256SUMS")
        .collect();
    names.sort();
    let mut text = String::new();
    for n in names {
        let path = dir.join(&n);
        let mut h = Sha256::new();
        let mut f = std::fs::File::open(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        std::io::copy(&mut f, &mut h).map_err(|e| format!("{}: {e}", path.display()))?;
        text.push_str(&format!("{}  {n}\n", hex(&h.finalize())));
    }
    write(dir, "SHA256SUMS", text.as_bytes())
}

/// The world's own name: its `world.toml`'s `key`.
///
/// # Errors
/// A world file that doesn't read, or has no key.
pub fn key_of(world: &Path) -> Result<String, String> {
    let text = std::fs::read_to_string(world).map_err(|e| format!("{}: {e}", world.display()))?;
    let t: toml::Table = text
        .parse()
        .map_err(|e| format!("{}: {e}", world.display()))?;
    t.get("key")
        .and_then(toml::Value::as_str)
        .map(str::to_owned)
        .ok_or_else(|| format!("{}: no key", world.display()))
}

pub(super) fn run(args: &[String]) -> Result<bool, String> {
    let mut world: Option<PathBuf> = None;
    let mut version: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut previous: Option<PathBuf> = None;
    let mut dumps: Option<PathBuf> = None;
    let mut dats: Vec<PathBuf> = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        let mut value = || {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{a} needs a value"))
        };
        match a.as_str() {
            "--world" => world = Some(value()?.into()),
            "--version" => version = Some(value()?),
            "--out" => out = Some(value()?.into()),
            "--previous" => previous = Some(value()?.into()),
            "--dumps" => dumps = Some(value()?.into()),
            "--dats" => dats.push(value()?.into()),
            _ => return Err(super::USAGE.into()),
        }
    }
    let (Some(world), Some(version), Some(out)) = (world, version, out) else {
        return Err(super::USAGE.into());
    };
    let previous = previous
        .map(|p| {
            std::fs::read_to_string(&p)
                .map_err(|e| format!("{}: {e}", p.display()))
                .and_then(|t| Manifest::parse(&t).map_err(|e| format!("{}: {e}", p.display())))
        })
        .transpose()?;
    let key = key_of(&world)?;
    let open = || super::open(&world, dumps.as_deref(), &dats);
    let m = release(&open, &key, &format!("v{version}"), previous.as_ref(), &out)?;
    let new = m
        .layers
        .iter()
        .filter_map(|l| l.pack.as_ref())
        .filter(|p| p.release == m.release)
        .map(|p| p.asset.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    println!(
        "{}: {} layers, {} packed in it ({}), {} in earlier releases, {} with none",
        m.release,
        m.layers.len(),
        new.len(),
        new.into_iter().collect::<Vec<_>>().join(", "),
        m.layers
            .iter()
            .filter(|l| l.pack.as_ref().is_some_and(|p| p.release != m.release))
            .count(),
        m.layers.iter().filter(|l| l.pack.is_none()).count(),
    );
    Ok(true)
}
