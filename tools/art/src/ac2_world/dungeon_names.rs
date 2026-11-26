//! AC2's dungeons by name. AC2 keeps no name with a dungeon: its cells carry none, and the name
//! belonged to the portal that led in. So the converter carries a table of dungeon to name
//! ([`TABLE`]): each AC2 dungeon block and the string that names it, by string table and token,
//! never the words. The tokens are those of yalain-world's portals into the dungeon (one standing
//! outside first, else one inside it, which names a room), else of the client's own teleports
//! into it. A dungeon none of them leads into has no line.
//!
//! At conversion each token is looked up in the player's own AC2 install ([`resolve`]) and the
//! words written with the AC1 block each dungeon went under into the overlay folder ([`FILE`]),
//! which Lyceum names the world's dungeons by.

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::Path;
use std::sync::Arc;

use auberean_dat::locate::DatName;

/// AC2 dungeon block, its name's string table and token, and where the token came from: a line
/// each, tab-separated, the ids in hex, after a header.
pub const TABLE: &str = include_str!("dungeon_names.tsv");

/// The file in the overlay folder naming the world's dungeons: `ac1_block ac2_block name`, a line
/// per dungeon written, the name empty where the table has none.
pub const FILE: &str = "dungeon-names.tsv";

/// What a name's token came from: a portal into the dungeon standing outside it, one inside it,
/// or a teleport's.
const FROM: [&str; 3] = ["portal", "room", "teleport"];

/// The table: each AC2 dungeon block's name, as its string table and token.
///
/// # Errors
/// A line that is not a block, a table, a token and where it came from, or a block twice.
pub fn tokens() -> Result<BTreeMap<u16, (u32, u32)>, String> {
    let mut out = BTreeMap::new();
    for (k, line) in TABLE.lines().enumerate().skip(1) {
        let n = k + 1;
        let cols: Vec<&str> = line.split('\t').collect();
        let [block, table, token, from] = cols[..] else {
            return Err(format!("line {n}: not four columns"));
        };
        let block = u16::from_str_radix(block, 16).map_err(|_| format!("line {n}: block"))?;
        let table = u32::from_str_radix(table, 16).map_err(|_| format!("line {n}: table"))?;
        let token = u32::from_str_radix(token, 16).map_err(|_| format!("line {n}: token"))?;
        if !FROM.contains(&from) {
            return Err(format!("line {n}: {from:?} is not where a name comes from"));
        }
        if out.insert(block, (table, token)).is_some() {
            return Err(format!("line {n}: block {block:04X} twice"));
        }
    }
    Ok(out)
}

/// The words of each name in `tokens`, from the AC2 install in `dir`: the English the interface
/// shows, its flags cut off. A token the install doesn't hold, or that writes nothing a name can
/// be, has none.
///
/// # Errors
/// A file that does not open.
pub fn resolve(
    dir: &Path,
    tokens: &BTreeMap<u16, (u32, u32)>,
) -> Result<BTreeMap<u16, String>, String> {
    let open = |n: DatName| {
        let p = n.in_dir(dir);
        auberean_dat::DatFile::open(&p).map_err(|e| format!("{}: {e}", p.display()))
    };
    let mut text = auberean_text::DatSource::new(vec![
        Arc::new(open(DatName::Portal)?),
        Arc::new(open(DatName::Local("English".into()))?),
    ]);
    let mut out = BTreeMap::new();
    for (&block, &(table, token)) in tokens {
        let words = auberean_text::StringInfo::entry(table, token).text(&mut text);
        let name = words.split_whitespace().collect::<Vec<_>>().join(" ");
        if !name.is_empty() && !name.contains(['\t', '\n', '\r', '\\']) {
            out.insert(block, name);
        }
    }
    Ok(out)
}

/// The overlay's [`FILE`]: for each dungeon written, `(AC2 block, AC1 block)` in `placed`'s order,
/// the name `names` gives it; and how many are named.
#[must_use]
pub fn document(placed: &[(u16, u16)], names: &BTreeMap<u16, String>) -> (String, usize) {
    let mut s = String::from("ac1_block\tac2_block\tname\n");
    let mut named = 0;
    for (ac2, ac1) in placed {
        let name = names.get(ac2).map_or("", String::as_str);
        named += usize::from(!name.is_empty());
        let _ = writeln!(s, "{ac1:04X}\t{ac2:04X}\t{name}");
    }
    (s, named)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the AC2 world's dungeon names)
    use super::*;

    #[test]
    fn the_table_names_the_dungeons_portals_and_teleports_lead_into() {
        let t = tokens().unwrap();
        // 224 of the 426 dungeons beyond AC2's landmass, and Arwic's vault under its entrance.
        assert_eq!(t.len(), 225);
        let from = |w: &str| {
            TABLE
                .lines()
                .filter(|l| l.ends_with(&format!("\t{w}")))
                .count()
        };
        assert_eq!(
            (from("portal"), from("room"), from("teleport")),
            (206, 8, 11)
        );
    }

    #[test]
    fn the_table_holds_identifiers_only() {
        // Every field but where the name came from is a hex id: no words of a name, no key.
        for line in TABLE.lines().skip(1) {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 4, "{line}");
            for (c, width) in cols[..3].iter().zip([4, 8, 8]) {
                assert_eq!(c.len(), width, "{line}");
                assert!(c.chars().all(|ch| ch.is_ascii_hexdigit()), "{line}");
            }
            assert!(FROM.contains(&cols[3]), "{line}");
        }
    }

    #[test]
    #[cfg_attr(
        not(feature = "retail-dats"),
        ignore = "reads the AC2 install: --features retail-dats"
    )]
    fn every_name_in_the_table_reads_from_the_install() {
        let t = tokens().unwrap();
        let names = resolve(&auberean_dat::testing::require_target_install(), &t).unwrap();
        assert_eq!(names.len(), t.len());
        // The interface's words, their article and gender flags cut off.
        assert!(names.values().all(|n| !n.contains(['[', ']'])));
    }

    #[test]
    fn a_dungeon_with_no_name_is_written_with_an_empty_one() {
        let names = BTreeMap::from([(0x00A7, "A Cave".to_owned())]);
        let (doc, named) = document(&[(0x00A7, 0xFE01), (0x0487, 0xFE80)], &names);
        assert_eq!(
            doc,
            "ac1_block\tac2_block\tname\nFE01\t00A7\tA Cave\nFE80\t0487\t\n"
        );
        assert_eq!(named, 1);
    }
}
