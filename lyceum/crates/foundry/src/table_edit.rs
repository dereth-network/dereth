//! One row of the game's tables ([`crate::tables`]) changed in a records file, field by field: the
//! file's other rows, its comments and the row's own lines that stay the same are kept.
//!
//! A row is found by its key: the fields that name what it changes (a starter area's `name`, a
//! heritage skill's `heritage` and `skill`), names compared as the tables compare them, spaces and
//! case aside. A row left with only its key says nothing, and goes.

use toml::Value as T;

/// A name as the tables compare names: lowercase, with no spaces.
#[must_use]
pub fn key_of(name: &str) -> String {
    name.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// Whether `a` and `b` are the same key value: names alike spaces and case aside, else equal.
fn same(a: &T, b: &T) -> bool {
    match (a, b) {
        (T::String(a), T::String(b)) => key_of(a) == key_of(b),
        _ => a == b,
    }
}

/// A value as TOML writes it, cells (a `cell` key's number) in hexadecimal.
#[must_use]
pub fn render(v: &T, key: Option<&str>) -> String {
    match v {
        T::Integer(i) if key == Some("cell") => format!("0x{i:08X}"),
        T::Array(a) => format!(
            "[{}]",
            a.iter()
                .map(|x| render(x, key))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        T::Table(t) => format!(
            "{{ {} }}",
            t.iter()
                .map(|(k, x)| format!("{k} = {}", render(x, Some(k))))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        T::Float(f) if f.fract() == 0.0 && f.is_finite() => format!("{f:.1}"),
        v => v.to_string(),
    }
}

fn value(v: &T, key: &str) -> Result<toml_edit::Value, String> {
    render(v, Some(key))
        .parse::<toml_edit::Value>()
        .map_err(|e| format!("{key}: {e}"))
}

fn matches(row: &toml_edit::Table, key: &[(&str, T)]) -> bool {
    key.iter().all(|(k, want)| {
        row.get(k)
            .and_then(|i| i.as_value())
            .and_then(|v| {
                v.to_string().trim().parse::<T>().ok().or_else(|| {
                    toml::from_str::<toml::Table>(&format!("v = {}", v.to_string().trim()))
                        .ok()
                        .and_then(|mut t| t.remove("v"))
                })
            })
            .is_some_and(|have| same(&have, want))
    })
}

/// The `[[kind]]` row of key `key` in records file `text`, when it has one.
///
/// # Errors
/// Text that isn't TOML.
pub fn row(
    text: Option<&str>,
    kind: &str,
    key: &[(&str, T)],
) -> Result<Option<toml::Table>, String> {
    let Some(text) = text else { return Ok(None) };
    let all: toml::Table = toml::from_str(text).map_err(|e| e.to_string())?;
    Ok(all
        .get(kind)
        .and_then(T::as_array)
        .into_iter()
        .flatten()
        .filter_map(T::as_table)
        .find(|r| {
            key.iter()
                .all(|(k, want)| r.get(*k).is_some_and(|have| same(have, want)))
        })
        .cloned())
}

/// Records file `text` (`None`, no file yet) with the `[[kind]]` row of key `key` given the fields
/// `fields` (`None` takes a field away), made when there is none. `None` when the file is left
/// with nothing in it.
///
/// # Errors
/// Text that isn't TOML, or a value it can't hold.
pub fn set_fields(
    text: Option<&str>,
    kind: &str,
    key: &[(&str, T)],
    fields: &[(&str, Option<T>)],
) -> Result<Option<String>, String> {
    let mut doc: toml_edit::DocumentMut = text
        .unwrap_or_default()
        .parse()
        .map_err(|e| format!("{e}"))?;
    if !doc.contains_key(kind) {
        doc.insert(
            kind,
            toml_edit::Item::ArrayOfTables(toml_edit::ArrayOfTables::new()),
        );
    }
    let rows = doc[kind]
        .as_array_of_tables_mut()
        .ok_or_else(|| format!("{kind} is not a list of rows"))?;
    let at = rows.iter().position(|r| matches(r, key));
    let at = match at {
        Some(i) => i,
        None => {
            if fields.iter().all(|(_, v)| v.is_none()) {
                return Ok(text.map(str::to_owned));
            }
            let mut t = toml_edit::Table::new();
            for (k, v) in key {
                t.insert(k, toml_edit::Item::Value(value(v, k)?));
            }
            rows.push(t);
            rows.len() - 1
        }
    };
    let row = rows.get_mut(at).ok_or("no row")?;
    for (k, v) in fields {
        match v {
            Some(v) => {
                row.insert(k, toml_edit::Item::Value(value(v, k)?));
            }
            None => {
                row.remove(k);
            }
        }
    }
    if row.iter().all(|(k, _)| key.iter().any(|(kk, _)| *kk == k)) {
        rows.remove(at);
    }
    if rows.is_empty() {
        doc.remove(kind);
    }
    let out = doc.to_string();
    Ok((!out.trim().is_empty()).then_some(out))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: a row of the game's tables changed by the world editor)
    use super::*;

    /// A row is found by its key, spaces and case aside; given fields, it keeps the rest of the
    /// file; made when there is none; and gone when it says nothing past its key.
    #[test]
    fn a_tables_row_is_changed_field_by_field_and_goes_when_it_says_nothing() {
        let text = "# The areas.\n[[starter-area]]\nname = \"Arwic\"\nheritages = \"all\"\n\n[[heritage]]\nheritage = \"Aluvian\"\nskill_credits = 50\n";
        let key = [("name", T::String("arwic".into()))];
        let place: T = toml::from_str::<toml::Table>(
            "v = [{ cell = 0xC6A90009, at = [84.0, 141.0, \"ground\"], heading = 90.0 }]",
        )
        .unwrap()["v"]
            .clone();
        let one = set_fields(
            Some(text),
            "starter-area",
            &key,
            &[("at", Some(place.clone()))],
        )
        .unwrap()
        .unwrap();
        assert!(one.starts_with("# The areas.\n"), "{one}");
        assert!(one.contains("cell = 0xC6A90009"), "{one}");
        assert!(one.contains("skill_credits = 50"), "{one}");
        let r = row(Some(&one), "starter-area", &key).unwrap().unwrap();
        assert_eq!(r["at"], place);
        assert_eq!(r["heritages"], T::String("all".into()));

        // A new area is a new row, after the others.
        let new = [("name", T::String("Holtburg".into()))];
        let two = set_fields(
            Some(&one),
            "starter-area",
            &new,
            &[("heritages", Some(T::String("all".into())))],
        )
        .unwrap()
        .unwrap();
        assert!(row(Some(&two), "starter-area", &new).unwrap().is_some());

        // Its fields taken away, the row goes; the file's last row gone, the file is empty.
        let gone = set_fields(Some(&two), "starter-area", &new, &[("heritages", None)])
            .unwrap()
            .unwrap();
        assert!(row(Some(&gone), "starter-area", &new).unwrap().is_none());
        let only = "[[starter-area]]\nname = \"Arwic\"\nremove = true\n";
        assert_eq!(
            set_fields(Some(only), "starter-area", &key, &[("remove", None)]).unwrap(),
            None
        );
        // Nothing to take from a row that isn't there.
        assert_eq!(
            set_fields(None, "starter-area", &key, &[("remove", None)]).unwrap(),
            None
        );
    }
}
