//! The census of every data file of every install: per file (header, structure, iterations), per
//! type (counts, id range, sizes, dates), per id (size, date, a payload hash), and the differences
//! between installs. Writes TSV only; no payload byte leaves the files.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};

use auberean_dat::container::{guid_string, sound, Entry, CELL_SET, HIRES_SUBSET};
use auberean_dat::locate::{all_dat_files, installs_under};
use auberean_dat::{cell_type, DatFile, RecordType};

pub fn fnv64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= u64::from(*b);
        h = h.wrapping_mul(0x0000_0100_0000_01B3);
    }
    h
}

/// The type of an id in a given file, as a number (0: none).
pub fn type_of(dat: &DatFile, id: u32) -> u32 {
    auberean_assets::verify::type_in(dat, id).map_or(0, |t| t.0)
}

/// A type's short name, or `unknown`.
pub fn type_name(ty: u32) -> String {
    if ty == 0 {
        "unknown".into()
    } else {
        RecordType(ty).short_name().into()
    }
}

/// A row key for the type census: the id's top byte for portal and local dats, the record kind
/// for the cell dat.
pub fn group_of(dat: &DatFile, id: u32) -> String {
    if dat.header().data_set == CELL_SET {
        type_name(type_of(dat, id))
    } else {
        format!("{:02X}", id >> 24)
    }
}

pub fn date(t: u32) -> String {
    // Days since 1970-01-01 to a civil date (Howard Hinnant's algorithm).
    let z = i64::from(t / 86_400) + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02}")
}

/// The negative iterations of a list as runs, `-2005..-2000,-1014..-1000`, or `none`.
pub fn negative_runs(v: &[i32]) -> String {
    let mut neg: Vec<i32> = v.iter().copied().filter(|x| *x < 0).collect();
    neg.sort_unstable();
    let mut runs: Vec<(i32, i32)> = Vec::new();
    for x in neg {
        match runs.last_mut() {
            Some(r) if r.1 + 1 == x => r.1 = x,
            _ => runs.push((x, x)),
        }
    }
    if runs.is_empty() {
        return "none".into();
    }
    runs.iter()
        .map(|(a, b)| format!("{a}..{b}"))
        .collect::<Vec<_>>()
        .join(",")
}

/// Every `.dat` in an install folder, sorted, with what opening it as a container gave.
pub fn dats(install: &Path) -> Vec<(String, PathBuf, Result<DatFile, String>)> {
    all_dat_files(install)
        .unwrap_or_default()
        .into_iter()
        .map(|(n, p)| {
            let r = DatFile::open(&p).map_err(|e| e.to_string());
            (n, p, r)
        })
        .collect()
}

pub struct IdRow {
    pub id: u32,
    pub size: u32,
    pub date: u32,
    pub hash: u64,
}

/// Run the census of every install under `root` (or only the named ones) into `out`.
pub fn run(root: &Path, out: &Path, only: &[String]) -> std::io::Result<()> {
    std::fs::create_dir_all(out.join("ids"))?;
    let mut dats_tsv = String::from(
        "install\tfile\tbytes\tblock\tkind\tsubset\tengine_version\tgame_version\tid_map\tstamp_word\tstamp_guid\tstamp_minor\tentries\tnodes\tleaf_depth\tfree_blocks\tused_blocks\ttotal_blocks\tsound\ttransaction_zero\titerations\titeration_max\tdate_min\tdate_max\n",
    );
    let mut types_tsv = String::from(
        "install\tfile\tgroup\ttype\ttype_name\tcount\tid_min\tid_max\tbytes\tsize_min\tsize_median\tsize_max\tdate_min\tdate_max\n",
    );
    let mut all: BTreeMap<(String, String), Vec<IdRow>> = BTreeMap::new();
    for install in installs_under(root) {
        let iname = install.file_name().unwrap().to_string_lossy().to_string();
        if !only.is_empty() && !only.contains(&iname) {
            continue;
        }
        for (file, path, opened) in dats(&install) {
            let bytes = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
            let dat = match opened {
                Ok(d) => d,
                Err(e) => {
                    eprintln!("{iname}/{file}: not a container ({e})");
                    let _ = writeln!(dats_tsv, "{iname}\t{file}\t{bytes}\tnot a container: {e}");
                    continue;
                }
            };
            eprintln!("{iname}/{file}: {} entries", dat.len());
            let h = *dat.header();
            let s = dat.verify().map_err(std::io::Error::other)?;
            let iters = dat.iterations().ok();
            let (dmin, dmax) = dat
                .entries()
                .fold((u32::MAX, 0), |(a, b), e| (a.min(e.date), b.max(e.date)));
            let subset = if h.data_subset == HIRES_SUBSET {
                "HiFi".to_string()
            } else {
                format!("{:#010x}", h.data_subset)
            };
            let stamp_word = dat.stamp_word().map_err(std::io::Error::other)?;
            let transaction_zero = dat
                .transaction_slot()
                .map_err(std::io::Error::other)?
                .iter()
                .all(|b| *b == 0);
            let _ = writeln!(
                dats_tsv,
                "{iname}\t{file}\t{bytes}\t{:#x}\t{}\t{subset}\t{}\t{}\t{:#010x}\t{:#010x}\t{}\t{:#x}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}",
                h.block_size,
                h.data_set,
                h.eng_pack_vnum,
                h.game_pack_vnum,
                h.master_map_id,
                stamp_word,
                guid_string(&h.version_major),
                h.version_minor,
                s.entries,
                s.nodes,
                s.leaf_depths.keys().map(|d| d.to_string()).collect::<Vec<_>>().join(","),
                s.free_chain_len,
                s.blocks_used,
                s.total_data_blocks,
                sound(&s),
                transaction_zero,
                iters.as_ref().map_or("-".into(), |v| v.len().to_string()),
                iters.as_ref().and_then(|v| v.iter().max()).map_or("-".into(), |m| m.to_string()),
                iters.as_ref().map_or("-".into(), |v| negative_runs(v)),
                date(dmin),
                date(dmax),
            );
            let mut groups: BTreeMap<String, Vec<&Entry>> = BTreeMap::new();
            for e in dat.entries() {
                groups.entry(group_of(&dat, e.id)).or_default().push(e);
            }
            for (g, es) in &groups {
                let mut sizes: Vec<u32> = es.iter().map(|e| e.size).collect();
                sizes.sort_unstable();
                let mut tys: Vec<u32> = es.iter().map(|e| type_of(&dat, e.id)).collect();
                tys.dedup();
                tys.sort_unstable();
                tys.dedup();
                let ty = tys
                    .iter()
                    .map(|t| format!("{t:08X}"))
                    .collect::<Vec<_>>()
                    .join(",");
                let tn = tys
                    .iter()
                    .map(|t| type_name(*t))
                    .collect::<Vec<_>>()
                    .join(",");
                let (dmin, dmax) = es
                    .iter()
                    .fold((u32::MAX, 0), |(a, b), e| (a.min(e.date), b.max(e.date)));
                let _ = writeln!(
                    types_tsv,
                    "{iname}\t{file}\t{g}\t{ty}\t{tn}\t{}\t{:08X}\t{:08X}\t{}\t{}\t{}\t{}\t{}\t{}",
                    es.len(),
                    es.first().unwrap().id,
                    es.last().unwrap().id,
                    sizes.iter().map(|s| u64::from(*s)).sum::<u64>(),
                    sizes[0],
                    sizes[sizes.len() / 2],
                    sizes[sizes.len() - 1],
                    date(dmin),
                    date(dmax),
                );
            }
            let mut rows = Vec::with_capacity(dat.len());
            for e in dat.entries() {
                let payload = dat.read_entry(e).map_err(std::io::Error::other)?;
                rows.push(IdRow {
                    id: e.id,
                    size: e.size,
                    date: e.date,
                    hash: fnv64(&payload),
                });
            }
            let mut f = std::io::BufWriter::new(std::fs::File::create(
                out.join("ids").join(format!("{iname}__{file}.tsv")),
            )?);
            writeln!(f, "id\tsize\tdate\tfnv64")?;
            for r in &rows {
                writeln!(f, "{:08X}\t{}\t{}\t{:016x}", r.id, r.size, r.date, r.hash)?;
            }
            all.insert((iname.clone(), file.to_ascii_lowercase()), rows);
        }
    }
    std::fs::write(out.join("dats.tsv"), dats_tsv)?;
    std::fs::write(out.join("types.tsv"), types_tsv)?;
    std::fs::write(out.join("diff.tsv"), diff(&all))?;
    Ok(())
}

/// Each install against the one before it (by folder name order, which is build order for the
/// held installs), file by file and group by group: ids only in the older, only in the newer,
/// in both with the same payload, in both with a different one.
fn diff(all: &BTreeMap<(String, String), Vec<IdRow>>) -> String {
    let mut installs: Vec<&String> = all.keys().map(|(i, _)| i).collect();
    installs.dedup();
    // Build order: the 2005 builds are 02.00.20.xxxx, the 2013 relaunch restarted at 00.00.00.xxxx.
    installs.sort_by_key(|i| (!i.starts_with("02."), (*i).clone()));
    let mut out =
        String::from("older\tnewer\tfile\tgroup\tonly_older\tonly_newer\tsame\tchanged\n");
    for pair in installs.windows(2) {
        let (a, b) = (pair[0], pair[1]);
        let files: std::collections::BTreeSet<&String> = all
            .keys()
            .filter(|(i, _)| i == a || i == b)
            .map(|(_, f)| f)
            .collect();
        for f in files {
            let empty = Vec::new();
            let ra = all.get(&(a.clone(), f.clone())).unwrap_or(&empty);
            let rb = all.get(&(b.clone(), f.clone())).unwrap_or(&empty);
            let cell = f.starts_with("cell");
            let group = |id: u32| {
                if cell {
                    type_name(cell_type(id).map_or(0, |t| t.0))
                } else {
                    format!("{:02X}", id >> 24)
                }
            };
            let mb: BTreeMap<u32, u64> = rb.iter().map(|r| (r.id, r.hash)).collect();
            let ma: BTreeMap<u32, u64> = ra.iter().map(|r| (r.id, r.hash)).collect();
            let mut g: BTreeMap<String, [usize; 4]> = BTreeMap::new();
            for r in ra {
                let c = g.entry(group(r.id)).or_default();
                match mb.get(&r.id) {
                    None => c[0] += 1,
                    Some(h) if *h == r.hash => c[2] += 1,
                    Some(_) => c[3] += 1,
                }
            }
            for r in rb {
                if !ma.contains_key(&r.id) {
                    g.entry(group(r.id)).or_default()[1] += 1;
                }
            }
            for (k, c) in g {
                let _ = writeln!(
                    out,
                    "{a}\t{b}\t{f}\t{k}\t{}\t{}\t{}\t{}",
                    c[0], c[1], c[2], c[3]
                );
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tool helpers)
    use super::*;

    #[test]
    fn dates_are_civil_utc() {
        assert_eq!(date(0), "1970-01-01");
        assert_eq!(date(1_434_144_313), "2015-06-12");
        assert_eq!(date(0x3D3E_8D4A), "2002-07-24");
    }

    #[test]
    fn fnv64_matches_the_reference_vectors() {
        assert_eq!(fnv64(b""), 0xcbf2_9ce4_8422_2325);
        assert_eq!(fnv64(b"a"), 0xaf63_dc4c_8601_ec8c);
    }
}
