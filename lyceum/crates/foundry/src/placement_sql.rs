//! A world repository's placement (`[[placement]]`) viewed as the server's SQL: one
//! `landblock_instance` row, with a `landblock_instance_link` row for each instance it links, the
//! SQL the spawn tool writes and moves; and that SQL read back as the placement's row.
//!
//! The row's fields are the placement's: `guid`, `class`, `cell`, `at` (three meters), `rotation`
//! (`w, x, y, z`), `link_child` (left out when false) and `links` (the guids it links). A time is
//! written but never read back: the editor's stamp is no change of the placement. Numbers are
//! written as they read back, so a row viewed and read back unchanged is the same row.

use toml::Value as T;

use crate::sql_ids;

fn int(row: &toml::Table, field: &str) -> Result<u32, String> {
    row.get(field)
        .and_then(T::as_integer)
        .and_then(|n| u32::try_from(n).ok())
        .ok_or_else(|| format!("a placement with no {field}"))
}

fn floats(row: &toml::Table, field: &str, n: usize, default: &[f64]) -> Vec<f64> {
    let got: Vec<f64> = row
        .get(field)
        .and_then(T::as_array)
        .map(|a| {
            a.iter()
                .filter_map(|v| v.as_float().or_else(|| v.as_integer().map(|i| i as f64)))
                .collect()
        })
        .unwrap_or_default();
    if got.len() == n {
        got
    } else {
        default.to_vec()
    }
}

/// A float as SQL writes it, read back the same.
fn real(v: f64) -> String {
    let t = format!("{v}");
    if t.contains('.') || t.contains('e') || t.contains("inf") || t.contains("NaN") {
        t
    } else {
        format!("{t}.0")
    }
}

/// The guid a link names: a guid, or a table naming its `child`.
fn link_child(v: &T) -> Option<u32> {
    match v {
        T::Integer(n) => u32::try_from(*n).ok(),
        T::Table(t) => t
            .get("child")
            .and_then(T::as_integer)
            .and_then(|n| u32::try_from(n).ok()),
        _ => None,
    }
}

/// Placement `row` as the server's SQL, its time `stamp` (`YYYY-MM-DD hh:mm:ss`).
///
/// # Errors
/// A row with no guid, class or cell.
pub fn to_sql(row: &toml::Table, stamp: &str) -> Result<String, String> {
    let guid = int(row, "guid")?;
    let class = int(row, "class")?;
    let cell = int(row, "cell")?;
    let at = floats(row, "at", 3, &[0.0; 3]);
    let r = floats(row, "rotation", 4, &[1.0, 0.0, 0.0, 0.0]);
    let child = row.get("link_child").and_then(T::as_bool).unwrap_or(false);
    let mut out = format!(
        "DELETE FROM `landblock_instance` WHERE `guid` = 0x{guid:08X};\n\n\
         INSERT INTO `landblock_instance` (`guid`, `weenie_Class_Id`, `obj_Cell_Id`, \
         `origin_X`, `origin_Y`, `origin_Z`, `angles_W`, `angles_X`, `angles_Y`, `angles_Z`, \
         `is_Link_Child`, `last_Modified`)\n\
         VALUES (0x{guid:08X}, {class}, 0x{cell:08X}, {}, {}, {}, {}, {}, {}, {}, {}, '{stamp}');\n",
        real(at[0]),
        real(at[1]),
        real(at[2]),
        real(r[0]),
        real(r[1]),
        real(r[2]),
        real(r[3]),
        if child { "True" } else { "False" },
    );
    let links: Vec<u32> = row
        .get("links")
        .and_then(T::as_array)
        .into_iter()
        .flatten()
        .filter_map(link_child)
        .collect();
    if !links.is_empty() {
        out.push_str(
            "\nINSERT INTO `landblock_instance_link` (`parent_GUID`, `child_GUID`, `last_Modified`)\nVALUES ",
        );
        let values: Vec<String> = links
            .iter()
            .map(|c| format!("(0x{guid:08X}, 0x{c:08X}, '{stamp}')"))
            .collect();
        out.push_str(&values.join("\n     , "));
        out.push_str(";\n");
    }
    Ok(out)
}

/// The placements SQL `text` writes, as rows: each `landblock_instance` row, with the links its
/// `landblock_instance_link` rows give it, in the order written.
///
/// # Errors
/// An instance whose guid, class or cell isn't a number.
pub fn rows(text: &str) -> Result<Vec<toml::Table>, String> {
    let mut out: Vec<toml::Table> = Vec::new();
    let mut links: Vec<(u32, u32)> = Vec::new();
    let mut bad = None;
    sql_ids::rows(text, |row| {
        let num = |c: &str| row.get(c).and_then(sql_ids::number);
        let real = |c: &str| {
            row.get(c)
                .and_then(|v| v.trim().parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        match row.table {
            "landblock_instance" => {
                let (Some(guid), Some(class), Some(cell)) =
                    (num("guid"), num("weenie_Class_Id"), num("obj_Cell_Id"))
                else {
                    bad = Some(format!(
                        "a spawn whose guid, weenie and cell aren't numbers ({})",
                        row.get("guid").unwrap_or("no guid")
                    ));
                    return;
                };
                let mut t = toml::Table::new();
                t.insert("guid".into(), T::Integer(guid.into()));
                t.insert("class".into(), T::Integer(class.into()));
                t.insert("cell".into(), T::Integer(cell.into()));
                t.insert(
                    "at".into(),
                    T::Array(
                        ["origin_X", "origin_Y", "origin_Z"]
                            .iter()
                            .map(|c| T::Float(real(c)))
                            .collect(),
                    ),
                );
                t.insert(
                    "rotation".into(),
                    T::Array(
                        ["angles_W", "angles_X", "angles_Y", "angles_Z"]
                            .iter()
                            .map(|c| T::Float(real(c)))
                            .collect(),
                    ),
                );
                if row.get("is_Link_Child").is_some_and(|v| {
                    let v = v.trim();
                    v.eq_ignore_ascii_case("true") || v == "1"
                }) {
                    t.insert("link_child".into(), T::Boolean(true));
                }
                out.push(t);
            }
            "landblock_instance_link" => {
                if let (Some(p), Some(c)) = (num("parent_GUID"), num("child_GUID")) {
                    links.push((p, c));
                }
            }
            _ => {}
        }
    });
    if let Some(why) = bad {
        return Err(why);
    }
    for (p, c) in links {
        if let Some(t) = out
            .iter_mut()
            .find(|t| t.get("guid").and_then(T::as_integer) == Some(p.into()))
        {
            if let T::Array(a) = t.entry("links").or_insert_with(|| T::Array(Vec::new())) {
                a.push(T::Integer(c.into()));
            }
        }
    }
    Ok(out)
}

/// Read-back row `row` with its links written as `below` writes the same ones: a link `below`
/// gives as a table (its child and its time) is kept so, so naming the same child is no change.
#[must_use]
pub fn links_as_below(row: &toml::Table, below: Option<&toml::Table>) -> toml::Table {
    let was: Vec<&T> = below
        .and_then(|b| b.get("links"))
        .and_then(T::as_array)
        .map(|a| a.iter().collect())
        .unwrap_or_default();
    let mut out = row.clone();
    if let Some(T::Array(a)) = out.get_mut("links") {
        for v in a.iter_mut() {
            let c = link_child(v);
            if let Some(w) = was.iter().find(|w| link_child(w) == c) {
                *v = (*w).clone();
            }
        }
    }
    out
}

/// The guid of the instance index `index` of landblock `lb`, as the server numbers them.
#[must_use]
pub fn guid_of(lb: u16, index: u16) -> u32 {
    0x7000_0000 | (u32::from(lb) << 12) | u32::from(index & 0xFFF)
}
