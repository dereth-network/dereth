//! The survey's output: `models.tsv` (one row per model), `uses.tsv` (one row per model and
//! use), `summary.md` (the headline numbers and the rankings) and `report.html` (a page with
//! every model, its picture and where it is used, sortable and filterable, that needs nothing
//! but the files beside it).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;

use crate::model::Facts;
use crate::survey::Survey;
use crate::tally::{ids_covering, total, Example, ModelId, Ranked, Tally, Use, UseStats};

/// The coverage thresholds the summary reports, in percent.
pub const THRESHOLDS: [u64; 3] = [50, 80, 95];

/// How many models each ranking in the summary lists.
pub const TOP: usize = 20;

/// Writes every output file into `out`.
///
/// # Errors
/// A file will not write.
pub fn write(survey: &Survey, out: &Path) -> Result<(), String> {
    std::fs::create_dir_all(out).map_err(|e| format!("{}: {e}", out.display()))?;
    let put = |name: &str, text: String| {
        let p = out.join(name);
        std::fs::write(&p, text).map_err(|e| format!("{}: {e}", p.display()))
    };
    put("models.tsv", models_tsv(survey))?;
    put("uses.tsv", uses_tsv(survey))?;
    put("summary.md", summary_md(survey))?;
    put("report.html", report_html(survey))?;
    Ok(())
}

/// The environment pieces' two uses together: every cell built from a piece.
pub const PIECES: [Use; 2] = [Use::DungeonPiece, Use::InteriorPiece];
/// The statics inside cells, dungeons and interiors together.
pub const CELL_STATICS: [Use; 2] = [Use::DungeonStatic, Use::InteriorStatic];

/// Each model's rank by its total over every use, 1 first.
fn total_ranks(t: &Tally) -> BTreeMap<ModelId, (usize, u64)> {
    t.ranked_total()
        .into_iter()
        .enumerate()
        .map(|(i, r)| (r.id, (i + 1, r.count)))
        .collect()
}

/// Each model's rank in one use, 1 first, with its share and cumulative share.
fn use_ranks(t: &Tally, u: Use) -> BTreeMap<ModelId, (usize, Ranked)> {
    t.ranked(u)
        .into_iter()
        .enumerate()
        .map(|(i, r)| (r.id, (i + 1, r)))
        .collect()
}

fn tsv_clean(s: &str) -> String {
    s.replace(['\t', '\n', '\r'], " ")
}

fn size_text(f: Option<&Facts>) -> [String; 3] {
    match f.and_then(Facts::size) {
        Some(s) => s.map(|x| format!("{x:.2}")),
        None => [String::new(), String::new(), String::new()],
    }
}

fn texture_list(f: &Facts) -> String {
    f.textures
        .iter()
        .map(|t| format!("0x{:08X}:{}x{}", t.id.0, t.width, t.height))
        .collect::<Vec<_>>()
        .join(" ")
}

fn uses_text(uses: &BTreeMap<Use, UseStats>) -> String {
    uses.iter()
        .map(|(u, s)| format!("{}:{}", u.name(), s.instances))
        .collect::<Vec<_>>()
        .join(" ")
}

/// `models.tsv`.
#[must_use]
pub fn models_tsv(s: &Survey) -> String {
    let ranks = total_ranks(&s.tally);
    let mut out = String::from(
        "id\tkind\trank\ttotal\tuses\tsize_x\tsize_y\tsize_z\ttriangles\tparts\ttextures\ttexture_list\tsolid_surfaces\tcollision\tcollision_detail\tthumbnail\ttags\n",
    );
    for (id, uses) in &s.tally.uses {
        let f = s.facts.get(id);
        let (rank, totalc) = ranks.get(id).copied().unwrap_or((0, 0));
        let [sx, sy, sz] = size_text(f);
        let _ = writeln!(
            out,
            "{}\t{}\t{rank}\t{totalc}\t{}\t{sx}\t{sy}\t{sz}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
            id.text(),
            id.kind().name(),
            uses_text(uses),
            f.map_or(0, |f| f.triangles),
            f.map_or(0, |f| f.parts.len()),
            f.map_or(0, |f| f.textures.len()),
            f.map(texture_list).unwrap_or_default(),
            f.map_or(0, |f| f.solid_surfaces),
            f.map_or("missing", |f| f.collision.kind()),
            f.map(|f| f.collision.text()).unwrap_or_default(),
            s.thumbnails.get(id).map_or("", String::as_str),
            tsv_clean(s.tags.get(id).map_or("", String::as_str)),
        );
    }
    out
}

/// A scenery placement's terrain, as `Name/scene:count`, most first.
fn terrain_text(stats: &UseStats, names: &[String]) -> String {
    let mut v: Vec<(&(u8, u8), &u64)> = stats.terrain.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    v.iter()
        .map(|((t, sc), n)| {
            let name = names
                .get(usize::from(*t))
                .map_or_else(|| format!("terrain{t}"), |n| n.replace(' ', ""));
            format!("{name}/{sc}:{n}")
        })
        .collect::<Vec<_>>()
        .join(" ")
}

/// `A9B4 x12 at 101.20,33.00,12.50 (cell 0xA9B40105)`.
fn example_text(e: &Example) -> String {
    let mut t = format!("{:04X} x{}", e.landblock, e.count);
    if let Some(p) = e.position {
        let _ = write!(t, " at {:.2},{:.2},{:.2}", p.x, p.y, p.z);
    }
    if let Some(c) = e.cell {
        let _ = write!(t, " (cell 0x{c:08X})");
    }
    t
}

fn notes_text(stats: &UseStats, limit: usize) -> String {
    let mut v: Vec<(&String, &u64)> = stats.notes.iter().collect();
    v.sort_by(|a, b| b.1.cmp(a.1).then(a.0.cmp(b.0)));
    let more = v.len().saturating_sub(limit);
    let mut parts: Vec<String> = v
        .iter()
        .take(limit)
        .map(|(k, n)| format!("{k}: {n}"))
        .collect();
    if more > 0 {
        parts.push(format!("and {more} more"));
    }
    parts.join("; ")
}

/// `uses.tsv`.
#[must_use]
pub fn uses_tsv(s: &Survey) -> String {
    let mut out = String::from(
        "id\tkind\tuse\trank\tinstances\tshare\tcumulative\tlandblocks\tinterior_cells\tterrain\tscale_min\tscale_max\texamples\tnotes\n",
    );
    let ranks: BTreeMap<Use, BTreeMap<ModelId, (usize, Ranked)>> = Use::ALL
        .iter()
        .map(|&u| (u, use_ranks(&s.tally, u)))
        .collect();
    for (id, uses) in &s.tally.uses {
        for (u, st) in uses {
            let (rank, share, cumulative) = ranks
                .get(u)
                .and_then(|m| m.get(id))
                .map_or((0, 0.0, 0.0), |(i, r)| (*i, r.share, r.cumulative));
            let (lo, hi) = st.scale.map_or((String::new(), String::new()), |(a, b)| {
                (format!("{a:.3}"), format!("{b:.3}"))
            });
            let _ = writeln!(
                out,
                "{}\t{}\t{}\t{rank}\t{}\t{share:.6}\t{cumulative:.6}\t{}\t{}\t{}\t{lo}\t{hi}\t{}\t{}",
                id.text(),
                id.kind().name(),
                u.name(),
                st.instances,
                st.landblocks,
                st.interior_cells,
                terrain_text(st, &s.terrain_names),
                st.examples
                    .iter()
                    .map(example_text)
                    .collect::<Vec<_>>()
                    .join("; "),
                tsv_clean(&notes_text(st, 40)),
            );
        }
    }
    out
}

fn pct(x: f64) -> String {
    format!("{:.1}%", x * 100.0)
}

/// One headline row: distinct models, instances, landblocks and the coverage thresholds.
fn headline_row(out: &mut String, label: &str, ranked: &[Ranked], landblocks: Option<u64>) {
    let _ = write!(
        out,
        "| {label} | {} | {} | {} |",
        ranked.iter().filter(|r| r.count > 0).count(),
        total(ranked),
        landblocks.map_or_else(|| "-".to_owned(), |n| n.to_string()),
    );
    for t in THRESHOLDS {
        let _ = write!(out, " {} |", ids_covering(ranked, t));
    }
    out.push('\n');
}

fn top_table(out: &mut String, s: &Survey, title: &str, ranked: &[Ranked], stats_of: &[Use]) {
    let _ = writeln!(out, "\n## {title}\n");
    if ranked.is_empty() {
        out.push_str("None.\n");
        return;
    }
    out.push_str(
        "| rank | id | kind | instances | share | cumulative | landblocks | triangles | size (m) | collision | tags |\n|---|---|---|---|---|---|---|---|---|---|---|\n",
    );
    for (i, r) in ranked.iter().take(TOP).enumerate() {
        let landblocks: u64 = s.tally.uses.get(&r.id).map_or(0, |u| {
            stats_of
                .iter()
                .filter_map(|x| u.get(x))
                .map(|st| st.landblocks)
                .sum()
        });
        let f = s.facts.get(&r.id);
        let size = f.and_then(Facts::size).map_or_else(String::new, |v| {
            format!("{:.1} x {:.1} x {:.1}", v[0], v[1], v[2])
        });
        let _ = writeln!(
            out,
            "| {} | `{}` | {} | {} | {} | {} | {landblocks} | {} | {size} | {} | {} |",
            i + 1,
            r.id.text(),
            r.id.kind().name(),
            r.count,
            pct(r.share),
            pct(r.cumulative),
            f.map_or(0, |f| f.triangles),
            f.map_or_else(|| "missing".to_owned(), |f| f.collision.text()),
            s.tags.get(&r.id).map_or("", String::as_str),
        );
    }
}

/// `summary.md`.
#[must_use]
pub fn summary_md(s: &Survey) -> String {
    let t = &s.tally;
    let w = &t.world;
    let mut out = String::from("# Art survey\n\n");
    let _ = writeln!(
        out,
        "Every model the world is built from, where it is used and how often. The survey walked \
         {} landblock ids ({} in the cell file) in {:.0} s.\n",
        s.landblocks_walked, w.landblocks, s.seconds
    );
    out.push_str("## Headline\n\n");
    out.push_str("| use | distinct ids | instances | landblocks |");
    for th in THRESHOLDS {
        let _ = write!(out, " ids for {th}% |");
    }
    out.push_str("\n|---|---|---|---|---|---|---|\n");
    for u in Use::ALL {
        headline_row(&mut out, u.label(), &t.ranked(u), landblocks_with(s, u));
    }
    headline_row(
        &mut out,
        "Environment pieces, every cell",
        &t.ranked_over(&PIECES),
        None,
    );
    headline_row(
        &mut out,
        "Statics inside cells, all",
        &t.ranked_over(&CELL_STATICS),
        None,
    );
    headline_row(
        &mut out,
        "Every use in the world (a model's total)",
        &t.ranked_total(),
        None,
    );
    out.push_str(
        "\n`ids for N%` is how many of the most used ids it takes to reach N% of that use's \
         instances. A setup's parts are counted inside the setup, not as ids of their own. The \
         character uses count creation choices and clothing tables rather than objects in the \
         world, so a model's total leaves them out. `landblocks` counts the landblocks holding \
         at least one instance (for dungeon cells, the dungeons).\n",
    );

    out.push_str("\n## The world\n\n");
    let _ = writeln!(
        out,
        "- Landblocks in the cell file: {}; landblocks the client grows scenery on: {}.",
        w.landblocks, w.scenery_landblocks
    );
    let _ = writeln!(
        out,
        "- Buildings: {}; cells inside them (reached through their portals): {}.",
        w.buildings, w.interior_cells
    );
    let _ = writeln!(
        out,
        "- Dungeons: {} (landblocks with cells no building reaches), holding {} cells.",
        w.dungeons, w.dungeon_cells
    );
    let _ = writeln!(
        out,
        "- Landblock statics whose origin is outside their own block (drawn, with no collision \
         record): {}.",
        w.statics_outside_block
    );
    let _ = writeln!(
        out,
        "- Interior cells a landblock promises that would not load: {}.",
        w.cells_unloaded
    );
    let _ = writeln!(
        out,
        "- Character bodies (heritage and sex): {}; clothing tables: {} ({} would not read).",
        s.characters.bodies, s.characters.clothing_tables, s.characters.clothing_unread
    );
    let _ = writeln!(
        out,
        "- Server spawn table rows: {}{}.",
        s.spawn_rows,
        if s.spawn_rows == 0 {
            " (no table given: the spawn use is empty)"
        } else {
            ""
        }
    );
    out.push_str(
        "\nScenery is placed exactly as the client places it at full detail, over every \
         landblock. A cell is a building's interior when it is reached from one of the block's \
         buildings through the building's portals and then the cells' own portals; every other \
         cell is a dungeon's, and one landblock's dungeon cells count as one dungeon. Spawn \
         counts are the objects present when every generator is full, as the server's table \
         gives them.\n",
    );

    top_table(
        &mut out,
        s,
        "Top procedural scenery",
        &t.ranked(Use::Scenery),
        &[Use::Scenery],
    );
    top_table(
        &mut out,
        s,
        "Top landblock statics",
        &t.ranked(Use::Static),
        &[Use::Static],
    );
    top_table(
        &mut out,
        s,
        "Top buildings",
        &t.ranked(Use::Building),
        &[Use::Building],
    );
    top_table(
        &mut out,
        s,
        "Top environment pieces (every cell)",
        &t.ranked_over(&PIECES),
        &PIECES,
    );
    top_table(
        &mut out,
        s,
        "Top statics inside cells",
        &t.ranked_over(&CELL_STATICS),
        &CELL_STATICS,
    );
    top_table(
        &mut out,
        s,
        "Top server spawns",
        &t.ranked(Use::Spawn),
        &[Use::Spawn],
    );
    top_table(
        &mut out,
        s,
        "Top clothing part swaps",
        &t.ranked(Use::Clothing),
        &[],
    );
    top_table(
        &mut out,
        s,
        "Every model by its total in the world",
        &t.ranked_total(),
        &Use::WORLD,
    );
    out
}

/// How many landblocks hold any instance of `u`, where that is known.
fn landblocks_with(s: &Survey, u: Use) -> Option<u64> {
    let w = &s.tally.world;
    match u {
        Use::Scenery => Some(w.scenery_landblocks),
        Use::Static => Some(w.static_landblocks),
        Use::Building => Some(w.building_landblocks),
        Use::DungeonPiece => Some(w.dungeons),
        Use::InteriorPiece => Some(w.interior_landblocks),
        Use::Spawn => Some(s.spawn_landblocks),
        _ => None,
    }
}

fn json_escape_for_script(s: &str) -> String {
    // Inside a script element, only a closing tag can end the data early.
    s.replace("</", "<\\/")
}

/// `report.html`.
#[must_use]
pub fn report_html(s: &Survey) -> String {
    let t = &s.tally;
    let totals = total_ranks(t);
    let ranks: BTreeMap<Use, BTreeMap<ModelId, (usize, Ranked)>> =
        Use::ALL.iter().map(|&u| (u, use_ranks(t, u))).collect();
    let models: Vec<serde_json::Value> = t
        .uses
        .iter()
        .map(|(id, uses)| {
            let f = s.facts.get(id);
            let (rank, tot) = totals.get(id).copied().unwrap_or((0, 0));
            let per_use: serde_json::Map<String, serde_json::Value> = uses
                .iter()
                .map(|(u, st)| {
                    let (r, share, cum) = ranks
                        .get(u)
                        .and_then(|m| m.get(id))
                        .map_or((0, 0.0, 0.0), |(i, r)| (*i, r.share, r.cumulative));
                    let mut where_: Vec<String> = st.examples.iter().map(example_text).collect();
                    if !st.terrain.is_empty() {
                        where_.push(format!("terrain: {}", terrain_text(st, &s.terrain_names)));
                    }
                    if let Some((a, b)) = st.scale {
                        where_.push(format!("scale {a:.3} to {b:.3}"));
                    }
                    if st.interior_cells > 0 {
                        where_.push(format!(
                            "{} interior cells behind its portals",
                            st.interior_cells
                        ));
                    }
                    if !st.notes.is_empty() {
                        where_.push(notes_text(st, 12));
                    }
                    (
                        u.name().to_owned(),
                        serde_json::json!({
                            "n": st.instances,
                            "lb": st.landblocks,
                            "rank": r,
                            "share": share,
                            "cum": cum,
                            "where": where_,
                        }),
                    )
                })
                .collect();
            serde_json::json!({
                "id": id.text(),
                "kind": id.kind().name(),
                "rank": rank,
                "total": tot,
                "uses": per_use,
                "size": f.and_then(Facts::size).map(|v| v.to_vec()),
                "tris": f.map_or(0, |f| f.triangles),
                "parts": f.map_or(0, |f| f.parts.len()),
                "tex": f.map_or(0, |f| f.textures.len()),
                "texList": f.map(texture_list).unwrap_or_default(),
                "coll": f.map_or_else(|| "missing".to_owned(), |f| f.collision.text()),
                "collKind": f.map_or("missing", |f| f.collision.kind()),
                "thumb": s.thumbnails.get(id),
                "tags": s.tags.get(id).cloned().unwrap_or_default(),
            })
        })
        .collect();
    let uses: Vec<serde_json::Value> = Use::ALL
        .iter()
        .map(|u| {
            let r = t.ranked(*u);
            serde_json::json!({
                "name": u.name(),
                "label": u.label(),
                "ids": r.iter().filter(|x| x.count > 0).count(),
                "instances": total(&r),
                "cover": THRESHOLDS.iter().map(|&p| ids_covering(&r, p)).collect::<Vec<_>>(),
            })
        })
        .collect();
    let data = serde_json::json!({
        "models": models,
        "uses": uses,
        "thresholds": THRESHOLDS,
        "world": {
            "landblocks": t.world.landblocks,
            "sceneryLandblocks": t.world.scenery_landblocks,
            "buildings": t.world.buildings,
            "dungeons": t.world.dungeons,
            "dungeonCells": t.world.dungeon_cells,
            "interiorCells": t.world.interior_cells,
        },
        "seconds": s.seconds,
    });
    let json = json_escape_for_script(&data.to_string());
    REPORT_TEMPLATE.replace("/*DATA*/null", &json)
}

const REPORT_TEMPLATE: &str = include_str!("report.html");
