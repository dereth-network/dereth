//! A dungeon pulled from one world into a world repository: the things the source world places in
//! the dungeon's landblock (its spawns, their generators' links), written as the repository's
//! `[[placement]]` rows in the pack being edited, with a portal into it where the author stands.
//!
//! The dungeon's cells are not copied: the AC2-in-AC1 land keeps AC1's dungeons under it, so the
//! repository's world holds the same cells at the same numbers. They are checked instead, byte for
//! byte ([`cells_differ`]), and a dungeon whose cells differ is refused.
//!
//! What it can't settle it reports:
//! - **the exits:** the portals in the dungeon whose destinations lie outside it. One onto the land
//!   leads where AC1's land was, so it is listed for its author to re-aim; one into another
//!   dungeon leads there still, if the repository's world has that dungeon's things too;
//! - **the classes the repository lacks:** each class a placement names that no row of the
//!   repository's world defines, to be copied into its library.
//!
//! The source world's things are its server database's, with its packs' SQL over them: a pack's
//! spawn of a guid the database places too stands for it.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use dereth_dat::ModernDat;
use toml::Value as T;

use crate::edit::Session;
use crate::layer_rows;
use crate::repo::RowSchema;
use crate::sql_ids::{self, Instance, ServerContent};

/// The kind of row a thing placed is.
pub const KIND: &str = "placement";

/// A portal in the dungeon that leads out of it.
#[derive(Clone, Debug, PartialEq)]
pub struct Exit {
    pub guid: u32,
    pub class: u32,
    pub name: Option<String>,
    /// The cell it sends who uses it to, and where in that cell's landblock.
    pub to: u32,
    pub at: Option<[f32; 3]>,
}

impl Exit {
    /// Whether it leads onto the land (an outdoor cell), not into another dungeon.
    #[must_use]
    pub fn outdoors(&self) -> bool {
        self.to & 0xFFFF < 0x100
    }
}

/// What a pull writes and reports, before anything is written.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Plan {
    pub landblock: u16,
    /// The placement rows, by guid.
    pub rows: BTreeMap<u32, toml::Table>,
    /// The classes they place.
    pub classes: BTreeSet<u32>,
    /// The portal classes whose destination is in the dungeon, with how many of them the source
    /// world places outside it and their names, the most first: the dungeon's ways in.
    pub portals: Vec<(u32, usize, Option<String>)>,
    pub exits: Vec<Exit>,
    /// Things placed that don't read as numbers (a pack's token the build didn't fill).
    pub skipped: Vec<String>,
}

/// A float as its shortest text reads it, so a value stored as `f32` is written as it was typed.
fn float(v: f32) -> T {
    T::Float(v.to_string().parse::<f64>().unwrap_or(f64::from(v)))
}

/// A thing to place: its guid, its class, its cell, where in the cell's landblock, turned how
/// (`w, x, y, z`), whether a generator links it, and the things it links.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placing<'a> {
    pub guid: u32,
    pub class: u32,
    pub cell: u32,
    pub at: [f32; 3],
    pub rotation: [f32; 4],
    pub link_child: bool,
    pub links: &'a [u32],
}

/// The placement row of `p`, said to come from `prov`.
#[must_use]
pub fn placement_row(p: &Placing<'_>, prov: &toml::Table) -> toml::Table {
    let Placing {
        guid,
        class,
        cell,
        at,
        rotation,
        link_child,
        links,
    } = *p;
    let mut t = toml::Table::new();
    t.insert("guid".into(), T::Integer(i64::from(guid)));
    t.insert("class".into(), T::Integer(i64::from(class)));
    t.insert("cell".into(), T::Integer(i64::from(cell)));
    t.insert(
        "at".into(),
        T::Array(at.iter().map(|v| float(*v)).collect()),
    );
    t.insert(
        "rotation".into(),
        T::Array(rotation.iter().map(|v| float(*v)).collect()),
    );
    if link_child {
        t.insert("link_child".into(), T::Boolean(true));
    }
    if !links.is_empty() {
        t.insert(
            "links".into(),
            T::Array(links.iter().map(|g| T::Integer(i64::from(*g))).collect()),
        );
    }
    t.insert("prov".into(), T::Table(prov.clone()));
    t
}

/// The provenance of things a server database places: its name and revision, from its file's name
/// (`ACE-World-Database-v0.9.296.sql`).
#[must_use]
pub fn database_prov(database: &Path) -> toml::Table {
    let stem = database
        .file_stem()
        .map_or_else(String::new, |s| s.to_string_lossy().into_owned());
    let (source, revision) = match stem.rsplit_once("-v") {
        Some((s, r)) if r.starts_with(|c: char| c.is_ascii_digit()) => {
            (s.to_owned(), Some(format!("v{r}")))
        }
        _ => (stem, None),
    };
    let mut t = toml::Table::new();
    t.insert("from".into(), T::String("source".into()));
    t.insert("source".into(), T::String(source));
    if let Some(r) = revision {
        t.insert("revision".into(), T::String(r));
    }
    t.insert(
        "note".into(),
        T::String("pulled with its dungeon by lyceum pull-dungeon".into()),
    );
    t
}

/// The provenance of things a source world's pack places.
#[must_use]
pub fn pack_prov(world: &str) -> toml::Table {
    let mut t = toml::Table::new();
    t.insert("from".into(), T::String("authored".into()));
    t.insert(
        "note".into(),
        T::String(format!(
            "pulled with its dungeon from {world} by lyceum pull-dungeon"
        )),
    );
    t
}

/// What pulling dungeon `lb` from a world whose server places `server` and whose packs place
/// `packs` over it would write: each thing placed in the landblock, with the links between them,
/// the portals into it and the exits out. `server_prov` and `packs_prov` say where each came from.
#[must_use]
pub fn plan(
    server: &ServerContent,
    packs: &ServerContent,
    lb: u16,
    server_prov: &toml::Table,
    packs_prov: &toml::Table,
) -> Plan {
    let mut out = Plan {
        landblock: lb,
        ..Plan::default()
    };
    let in_lb = |cell: u32| cell >> 16 == u32::from(lb);
    // Each thing placed in the landblock by its guid: the pack's over the server's.
    let mut placed: BTreeMap<u32, (&Instance, &toml::Table)> = BTreeMap::new();
    for (content, prov) in [(server, server_prov), (packs, packs_prov)] {
        for i in content.instances.iter().filter(|i| in_lb(i.cell)) {
            match (sql_ids::number(&i.guid), sql_ids::number(&i.weenie)) {
                (Some(g), Some(_)) if i.cell_symbol.is_none() => {
                    placed.insert(g, (i, prov));
                }
                _ => out.skipped.push(format!("{} ({})", i.guid, i.weenie)),
            }
        }
    }
    let links: BTreeSet<(u32, u32)> = server
        .links
        .iter()
        .chain(&packs.links)
        .copied()
        .filter(|(p, c)| placed.contains_key(p) && placed.contains_key(c))
        .collect();
    let facts = |class: u32| {
        packs
            .weenies
            .get(&class)
            .or_else(|| server.weenies.get(&class))
    };
    for (guid, (i, prov)) in &placed {
        let class = sql_ids::number(&i.weenie).unwrap_or(0);
        let children: Vec<u32> = links
            .iter()
            .filter(|(p, _)| p == guid)
            .map(|(_, c)| *c)
            .collect();
        out.rows.insert(
            *guid,
            placement_row(
                &Placing {
                    guid: *guid,
                    class,
                    cell: i.cell,
                    at: i.origin,
                    rotation: i.angles,
                    link_child: i.link_child,
                    links: &children,
                },
                prov,
            ),
        );
        out.classes.insert(class);
        if let Some(f) = facts(class) {
            if let Some(to) = f.destination.filter(|to| !in_lb(*to)) {
                out.exits.push(Exit {
                    guid: *guid,
                    class,
                    name: f.name.clone(),
                    to,
                    at: f.destination_at,
                });
            }
        }
    }
    // The ways in: the portals whose destination is in the dungeon, by how often the world places
    // them outside it.
    let mut ways: BTreeMap<u32, usize> = BTreeMap::new();
    for content in [server, packs] {
        for (class, f) in &content.weenies {
            if f.destination.is_some_and(in_lb) {
                ways.entry(*class).or_default();
            }
        }
    }
    for i in server.instances.iter().chain(&packs.instances) {
        if let Some(n) = sql_ids::number(&i.weenie).and_then(|c| ways.get_mut(&c)) {
            if !in_lb(i.cell) {
                *n += 1;
            }
        }
    }
    out.portals = ways
        .into_iter()
        .map(|(c, n)| (c, n, facts(c).and_then(|f| f.name.clone())))
        .collect();
    out.portals
        .sort_by_key(|(c, n, _)| (std::cmp::Reverse(*n), *c));
    out
}

/// The cells of dungeon `lb` that the two worlds hold differently, by number: one that only one
/// holds, or whose record differs.
#[must_use]
pub fn cells_differ(source: &Session, target: &Session, lb: u16) -> Vec<u16> {
    let (a, b) = (source.cells(lb), target.cells(lb));
    let read = |s: &Session, n: u16| {
        crate::compile::read_compiled(
            s.compiled(),
            s.base(),
            ModernDat::Cell,
            (u32::from(lb) << 16) | u32::from(n),
        )
    };
    a.keys()
        .chain(b.keys())
        .copied()
        .collect::<BTreeSet<u16>>()
        .into_iter()
        .filter(|n| read(source, *n) != read(target, *n))
        .collect()
}

/// The lowest guid of landblock `lb` from its authored range up (`0x7LLLL800`) that `taken` lacks.
#[must_use]
pub fn free_guid(lb: u16, taken: &BTreeSet<u32>) -> Option<u32> {
    let base = 0x7000_0000 | (u32::from(lb) << 12);
    (0x800..0x1000)
        .map(|n| base | n)
        .find(|g| !taken.contains(g))
}

/// Pack folder `pack`'s files with `rows` (placement rows) written in, each in the file its kind's
/// rule names: the files' new texts, by path. `read` gives a file's text now, if any.
///
/// # Errors
/// A row the schema can't name, or a file that isn't TOML.
pub fn write_rows(
    schema: &RowSchema,
    pack: &Path,
    rows: &[toml::Table],
    read: &mut dyn FnMut(&Path) -> Option<String>,
) -> Result<BTreeMap<PathBuf, String>, String> {
    let mut out: BTreeMap<PathBuf, String> = BTreeMap::new();
    for row in rows {
        let file = pack.join(layer_rows::file_of(schema, KIND, row)?);
        let item = crate::repo::item_of(schema, KIND, row)
            .ok_or_else(|| format!("a placement row with no guid: {row}"))?;
        let now = out.get(&file).cloned().or_else(|| read(&file));
        if let Some(text) = layer_rows::write_row(schema, KIND, now.as_deref(), &item, Some(row))? {
            out.insert(file, text);
        }
    }
    Ok(out)
}

/// Where the way into a pulled dungeon stands: a cell, landblock meters and a heading in degrees.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Entrance {
    pub cell: u32,
    pub at: [f32; 3],
    pub heading: f32,
}

/// A dungeon to pull, and where to.
#[derive(Clone, Debug)]
pub struct Request<'a> {
    /// The source world's file, and the server database its world runs on.
    pub source: &'a Path,
    pub database: &'a Path,
    pub landblock: u16,
    /// The world repository's file, its row vocabulary (`empyrean-import schema`), and its land.
    pub target: &'a Path,
    pub schema: &'a RowSchema,
    pub land: Option<&'a Path>,
    /// The pack of the repository's newest layer to write in: its last, when `None`.
    pub pack: Option<&'a str>,
    /// Where to place a portal into the dungeon, and which class (the source's commonest way in,
    /// when `None`).
    pub entrance: Option<Entrance>,
    pub portal: Option<u32>,
    /// Whether the source's things go in beside what the repository places there already.
    pub alongside: bool,
    /// Whether to report what would be written, and write nothing.
    pub dry_run: bool,
    /// Whether the classes the repository lacks are added to its library list
    /// ([`library_list`]), for its library to copy them.
    pub library: bool,
}

/// What a pull wrote.
#[derive(Clone, Debug, PartialEq)]
pub struct Pulled {
    pub plan: Plan,
    /// The files written (with `dry_run`, those that would be).
    pub files: Vec<PathBuf>,
    /// The way in placed: its guid and class.
    pub entrance: Option<(u32, u32)>,
    /// The classes placed that the repository's world doesn't define, with the source's names.
    pub missing: Vec<(u32, Option<String>)>,
    /// How many things the repository placed in the landblock already.
    pub already: usize,
    /// The library list the missing classes were added to, and those added (none it named
    /// already); `None` when none was written, or the repository has no list.
    pub library: Option<(PathBuf, Vec<u32>)>,
}

/// What a pull would write: what it reports (no files yet), the rows, and the pack folder they go
/// in.
#[derive(Clone, Debug)]
pub struct Prepared {
    pub pulled: Pulled,
    pub rows: Vec<toml::Table>,
    pub pack: PathBuf,
    /// The library list's new text, when it gains classes.
    pub library_text: Option<String>,
}

/// Pull `r`'s dungeon from its source world into its repository, over the client's files in
/// `dats`: the rows written into the pack's files on the disk (with `dry_run`, only named).
///
/// # Errors
/// As [`prepare`], and a file that won't write.
pub fn pull(r: &Request<'_>, dats: &Path) -> Result<Pulled, crate::error::Error> {
    use crate::error::Error;
    let Prepared {
        mut pulled,
        rows,
        pack,
        library_text,
    } = prepare(r, dats, None)?;
    if let (Some((path, _)), Some(text), false) = (&pulled.library, &library_text, r.dry_run) {
        std::fs::write(path, text).map_err(|e| Error::io(path, e))?;
    }
    let texts = write_rows(r.schema, &pack, &rows, &mut |p| {
        std::fs::read_to_string(p).ok()
    })
    .map_err(Error::Refused)?;
    for (path, text) in texts {
        if !r.dry_run {
            if let Some(d) = path.parent() {
                std::fs::create_dir_all(d).map_err(|e| Error::io(d, e))?;
            }
            std::fs::write(&path, text).map_err(|e| Error::io(&path, e))?;
        }
        pulled.files.push(path);
    }
    Ok(pulled)
}

/// What pulling `r`'s dungeon would write, nothing written: its rows and its report. `target` is
/// the repository's world when a session holds it already (the editor's), else it is opened.
///
/// # Errors
/// A world or a database that doesn't read; a source landblock with no dungeon; cells the two
/// worlds hold differently; things the repository places there already (unless `alongside`), or a
/// guid it uses; no way into the dungeon to place.
pub fn prepare(
    r: &Request<'_>,
    dats: &Path,
    target: Option<&Session>,
) -> Result<Prepared, crate::error::Error> {
    use crate::error::Error;
    let refused = Error::Refused;
    let lb = r.landblock;
    let repo = crate::repo::Repo::open(r.target).map_err(refused)?;
    let at = repo.shown_layers() - 1;
    let layer = &repo.layers[at];
    let pack = match r.pack {
        Some(name) => layer
            .packs
            .iter()
            .find(|p| p.name == name)
            .ok_or_else(|| refused(format!("layer {} has no pack named {name}", layer.id)))?,
        None => layer
            .packs
            .last()
            .ok_or_else(|| refused(format!("layer {} has no pack", layer.id)))?,
    };
    let world = repo.fold(r.schema, at).map_err(refused)?;
    let field = |item: &crate::repo::ShownItem, name: &str| {
        item.fields
            .iter()
            .find(|f| f.name == name)
            .and_then(|f| f.value.as_integer())
            .and_then(|v| u32::try_from(v).ok())
    };
    let mut taken: BTreeSet<u32> = BTreeSet::new();
    let mut already = 0;
    let mut classes: BTreeSet<u32> = BTreeSet::new();
    for item in world.items.values() {
        match item.kind.as_str() {
            KIND => {
                taken.extend(field(item, "guid"));
                already +=
                    usize::from(field(item, "cell").is_some_and(|c| c >> 16 == u32::from(lb)));
            }
            "weenie" => classes.extend(field(item, "class")),
            _ => {}
        }
    }

    // The two worlds' cells of the dungeon, the same.
    let source = Session::open(r.source, dats, None)?;
    let cells = source.cells(lb);
    if cells.is_empty() {
        return Err(refused(format!(
            "{}: landblock {lb:04X} holds no dungeon",
            r.source.display()
        )));
    }
    if cells.values().any(|c| c.flags & 1 != 0) {
        return Err(refused(format!(
            "{}: landblock {lb:04X} holds a building's rooms, not a dungeon",
            r.source.display()
        )));
    }
    let opened = match target {
        Some(_) => None,
        None => {
            let build = std::env::temp_dir().join(format!("lyceum-pull-{}", std::process::id()));
            let t = (|| {
                let w = crate::repo_world::world_of(&repo, at, r.land, &build).map_err(refused)?;
                Session::open_world(w, dats, None, None)
            })();
            let _ = std::fs::remove_dir_all(&build);
            Some(t?)
        }
    };
    let target = target.or(opened.as_ref()).expect("one or the other");
    let differ = cells_differ(&source, target, lb);
    if !differ.is_empty() {
        let list: Vec<String> = differ
            .iter()
            .take(8)
            .map(|n| format!("{lb:04X}{n:04X}"))
            .collect();
        return Err(refused(format!(
            "the repository's world holds {} of landblock {lb:04X}'s cells otherwise than {} ({}{}): \
             only a dungeon both hold alike is pulled",
            differ.len(),
            r.source.display(),
            list.join(", "),
            if differ.len() > list.len() { ", ..." } else { "" }
        )));
    }

    // The source's things: its database's, its packs' over them.
    let dump = std::fs::read(r.database).map_err(|e| Error::io(r.database, e))?;
    let server = sql_ids::server_content(&String::from_utf8_lossy(&dump));
    let pack_sql: String = source
        .compiled()
        .sql
        .values()
        .map(String::as_str)
        .collect::<Vec<_>>()
        .join("\n");
    let packs = sql_ids::server_content(&pack_sql);
    let key = source.world().key.clone();
    let plan = plan(
        &server,
        &packs,
        lb,
        &database_prov(r.database),
        &pack_prov(&key),
    );
    if already > 0 && !r.alongside {
        return Err(refused(format!(
            "the repository already places {already} thing(s) in {lb:04X}: pull with alongside to add the source's beside them"
        )));
    }
    let clash: Vec<String> = plan
        .rows
        .keys()
        .filter(|g| taken.contains(g))
        .map(|g| format!("{g:#010X}"))
        .collect();
    if !clash.is_empty() {
        return Err(refused(format!(
            "the repository already uses {} of the guids: {}{}; nothing pulled",
            clash.len(),
            clash[..clash.len().min(8)].join(", "),
            if clash.len() > 8 { ", ..." } else { "" }
        )));
    }
    let mut rows: Vec<toml::Table> = plan.rows.values().cloned().collect();

    // The way in, where the author stands.
    let mut entrance = None;
    if let Some(e) = r.entrance {
        let class = r
            .portal
            .or_else(|| plan.portals.first().map(|(c, _, _)| *c))
            .ok_or_else(|| {
                refused(format!(
                    "no portal of {} leads into {lb:04X}: name one",
                    key
                ))
            })?;
        taken.extend(plan.rows.keys());
        let elb = u16::try_from(e.cell >> 16).unwrap_or(0);
        let guid = free_guid(elb, &taken)
            .ok_or_else(|| refused(format!("landblock {elb:04X} has no free guid left")))?;
        let q = crate::compile::heading_quat(e.heading);
        let mut prov = toml::Table::new();
        prov.insert("from".into(), T::String("authored".into()));
        prov.insert(
            "note".into(),
            T::String(format!(
                "the way into {lb:04X}, placed by lyceum pull-dungeon"
            )),
        );
        rows.push(placement_row(
            &Placing {
                guid,
                class,
                cell: e.cell,
                at: e.at,
                rotation: [q.w, q.x, q.y, q.z],
                link_child: false,
                links: &[],
            },
            &prov,
        ));
        entrance = Some((guid, class));
    }
    let names = |c: u32| {
        packs
            .weenies
            .get(&c)
            .or_else(|| server.weenies.get(&c))
            .and_then(|f| f.name.clone())
    };
    let missing: Vec<(u32, Option<String>)> = plan
        .classes
        .iter()
        .copied()
        .chain(entrance.map(|(_, c)| c))
        .collect::<BTreeSet<u32>>()
        .into_iter()
        .filter(|c| !classes.contains(c))
        .map(|c| (c, names(c)))
        .collect();

    // The classes it lacks, added to the repository's library list.
    let mut library = None;
    let mut library_text = None;
    if r.library && !missing.is_empty() {
        if let Some(path) = library_list(&repo) {
            let text = std::fs::read_to_string(&path).map_err(|e| Error::io(&path, e))?;
            let (new, added) = add_to_library(&text, lb, &missing);
            if !added.is_empty() {
                library_text = Some(new);
                library = Some((path, added));
            }
        }
    }

    Ok(Prepared {
        pulled: Pulled {
            plan,
            files: Vec::new(),
            entrance,
            missing,
            already,
            library,
        },
        rows,
        pack: pack.dir.clone(),
        library_text,
    })
}

/// The repository's library list: the `library.txt` of the newest layer's folder that holds one
/// (New Dereth's base, `base/library.txt`), which names the classes its library copies.
#[must_use]
pub fn library_list(repo: &crate::repo::Repo) -> Option<PathBuf> {
    repo.layers
        .iter()
        .rev()
        .filter_map(|l| l.packs.first()?.dir.parent().map(|d| d.join("library.txt")))
        .find(|p| p.is_file())
}

/// Library list `text` with a `weenie <class>  # <name>` line for each of `classes` it doesn't
/// name, by class, under a line saying which dungeon's pull added them; and the classes added.
#[must_use]
pub fn add_to_library(
    text: &str,
    lb: u16,
    classes: &[(u32, Option<String>)],
) -> (String, Vec<u32>) {
    let named: BTreeSet<u32> = text
        .lines()
        .filter_map(|l| {
            let l = l.split('#').next()?.trim();
            let (head, rest) = l.split_once(char::is_whitespace)?;
            (head == "weenie")
                .then(|| sql_ids::number(rest.trim()))
                .flatten()
        })
        .collect();
    let new: BTreeMap<u32, &Option<String>> = classes
        .iter()
        .filter(|(c, _)| !named.contains(c))
        .map(|(c, n)| (*c, n))
        .collect();
    if new.is_empty() {
        return (text.to_owned(), Vec::new());
    }
    let mut out = text.trim_end_matches('\n').to_owned();
    out.push_str(&format!(
        "\n\n# The classes dungeon {lb:04X} places, added by lyceum pull-dungeon.\n"
    ));
    for (c, name) in &new {
        match name {
            Some(n) => out.push_str(&format!("weenie {c}  # {n}\n")),
            None => out.push_str(&format!("weenie {c}\n")),
        }
    }
    (out, new.into_keys().collect())
}
