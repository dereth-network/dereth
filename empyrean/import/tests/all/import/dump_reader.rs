//! Behaviour: none (tooling: how the importer reads a world dump, and when it refuses to build a pack from one)
//! ACE: Source/ACE.Database/Models/World/WorldDbContext.cs::WorldDbContext (the world tables and columns a dump is checked against)
//! The dump reader reads statements, not lines: SQLyog's multi-line `insert  into` gives the same
//! rows as mysqldump's one-line `INSERT INTO`, as do comments, executable comments, column lists
//! and literal spellings MySQL accepts. An empty dump, one without weenies, one it cannot parse,
//! and one with statements, tables or columns it cannot read are refused, by name and count.
//! Fixture: the synthetic world dump and small synthetic dumps; the built importer.

use std::path::PathBuf;
use std::process::Command;

use empyrean_content::import::patch::Source;
use empyrean_content::import::{self, build_from, check, default_now, Allow, Imported};
use empyrean_content::models::world::Event;
use empyrean_content::pack::{Pack, TableId};
use empyrean_content::ImportError;

use crate::fixture;

/// The dump laid out as SQLyog writes it: `insert  into \`t\`(\`c\`,…) values` and then one
/// tuple per line. Tuples are split outside string literals only.
fn sqlyog_layout(dump: &str) -> String {
    let mut out = String::new();
    for line in dump.split_inclusive('\n') {
        let Some(rest) = line.strip_prefix("INSERT INTO ") else {
            out.push_str(line);
            continue;
        };
        let at = rest.find(" VALUES ").expect("a VALUES list");
        out.push_str("insert  into ");
        out.push_str(&rest[..at].replace("` (`", "`(`").replace("`, `", "`,`"));
        out.push_str(" values \n");
        let (mut in_str, mut escaped, mut depth, mut new_line) = (false, false, 0u32, false);
        for c in rest[at + " VALUES ".len()..].chars() {
            if new_line && c == ' ' {
                continue;
            }
            new_line = false;
            out.push(c);
            if in_str {
                match (escaped, c) {
                    (true, _) => escaped = false,
                    (false, '\\') => escaped = true,
                    (false, '\'') => in_str = false,
                    _ => {}
                }
                continue;
            }
            match c {
                '\'' => in_str = true,
                '(' => depth += 1,
                ')' => depth -= 1,
                ',' if depth == 0 => {
                    out.push('\n');
                    new_line = true;
                }
                _ => {}
            }
        }
    }
    out
}

fn no_sources() -> std::iter::Empty<Result<Source, ImportError>> {
    std::iter::empty()
}

fn assert_same_records(a: &[u8], b: &[u8]) {
    let (a, b) = (
        Pack::from_bytes(a.to_vec()).unwrap(),
        Pack::from_bytes(b.to_vec()).unwrap(),
    );
    let diffs = check::diff(&a, &b).unwrap();
    assert!(diffs.is_empty(), "{}", check::render(&diffs, &a, &b));
}

fn rows(imp: &Imported) -> Vec<(&'static str, u64)> {
    imp.rows.clone()
}

#[test]
fn a_multi_line_insert_imports_the_same_rows_as_a_single_line_one() {
    let one_line = fixture::dump();
    let multi_line = sqlyog_layout(&one_line);
    assert!(
        multi_line.contains("insert  into `weenie` values \n(") && multi_line.contains("),\n("),
        "the layout under test"
    );
    assert!(
        !multi_line.contains("INSERT INTO"),
        "every INSERT is reflowed"
    );

    let (a, ia) = import::import(one_line.as_bytes()).unwrap();
    let (b, ib) = import::import(multi_line.as_bytes()).unwrap();
    assert_eq!(rows(&ia), rows(&ib));
    assert!(
        ib.skipped_statements.is_empty(),
        "{:?}",
        ib.skipped_statements
    );
    assert_same_records(&a, &b);

    // The patched-build path keeps rows as text; the same rows go in there too.
    let (c, _) = build_from(one_line.as_bytes(), no_sources(), default_now()).unwrap();
    let (d, id) = build_from(multi_line.as_bytes(), no_sources(), default_now()).unwrap();
    assert_eq!(rows(&ia), rows(&id));
    assert_same_records(&c, &d);
}

const EVENT_TABLE: &str = "CREATE TABLE `event` (`id` int unsigned NOT NULL AUTO_INCREMENT, `name` varchar(255) NOT NULL, `start_Time` int NOT NULL DEFAULT '-1', `end_Time` int NOT NULL DEFAULT '-1', `state` int NOT NULL, `last_Modified` datetime NOT NULL DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP, PRIMARY KEY (`id`), UNIQUE KEY `name_UNIQUE` (`name`)) ENGINE=InnoDB COMMENT='a; (comment)';\n";

/// Two events, the second named `Be;ta 'q' (x),(y)`, in several spellings MySQL reads the same.
const EVENT_ROWS: &[(&str, &str)] = &[
    (
        "mysqldump",
        "INSERT INTO `event` VALUES (1,'Alpha',-1,-1,1,'2021-11-01 00:00:00'),(2,'Be;ta \\'q\\' (x),(y)',5,6,2,'2021-11-01 00:00:00');\n",
    ),
    (
        "SQLyog",
        "insert  into `event`(`id`,`name`,`start_Time`,`end_Time`,`state`,`last_Modified`) values \n(1,'Alpha',-1,-1,1,'2021-11-01 00:00:00'),\n(2,'Be;ta \\'q\\' (x),(y)',5,6,2,'2021-11-01 00:00:00');\n",
    ),
    (
        "two statements on a line, comments between",
        "/* rows; */ INSERT INTO event VALUES (1,'Alpha',-1,-1,1,'2021-11-01 00:00:00'); -- first;\nINSERT INTO `ace_world`.`event` VALUES # second;\n(2,'Be;ta \\'q\\' (x),(y)',5,6,2,'2021-11-01 00:00:00');\n",
    ),
    (
        "a column list in another order, spacing, keyword case, double quotes, an introducer",
        "Insert Ignore Into `event` ( `name` , `id`, `state`, `start_Time`, `end_Time`, `last_Modified` )\n  VALUES\n  ( 'Alpha' , 1 , 1 , -1 , -1 , '2021-11-01 00:00:00' ) ,\n  ( \"Be;ta 'q' (x),(y)\" , 2, 2, 5, 6, _utf8mb4'2021-11-01 00:00:00' )\n;\n",
    ),
    (
        "executable comments, doubled quotes, hex, and no final semicolon",
        "/*!40000 ALTER TABLE `event` DISABLE KEYS */;\nINSERT INTO `event` VALUES (1,0x416C706861,-1,-1,1,'2021-11-01 00:00:00'),(2,'Be;ta ''q'' (x),(y)',5,6,2,X'323032312D31312D30312030303A30303A3030');\n/*!40000 ALTER TABLE `event` ENABLE KEYS */;\n/*!40101 SET SQL_MODE=@OLD_SQL_MODE */",
    ),
];

fn events(bytes: Vec<u8>) -> Vec<Event> {
    Pack::from_bytes(bytes)
        .unwrap()
        .all::<Event>(TableId::EVENT)
        .unwrap()
        .into_iter()
        .map(|(_, e)| e)
        .collect()
}

#[test]
fn every_statement_layout_mysql_reads_gives_the_same_rows() {
    let (_, first) = EVENT_ROWS[0];
    let expected = events(
        import::import(format!("{EVENT_TABLE}{first}").as_bytes())
            .unwrap()
            .0,
    );
    assert_eq!(expected.len(), 2);
    assert_eq!(expected[1].name, "Be;ta 'q' (x),(y)");
    for (what, rows) in EVENT_ROWS {
        let dump = format!("{EVENT_TABLE}{rows}");
        let (bytes, imp) =
            import::import(dump.as_bytes()).unwrap_or_else(|e| panic!("{what}: {e}"));
        assert!(
            imp.skipped_statements.is_empty(),
            "{what}: {:?}",
            imp.skipped_statements
        );
        assert_eq!(events(bytes), expected, "{what}");
        let (bytes, _) = build_from(dump.as_bytes(), no_sources(), default_now())
            .unwrap_or_else(|e| panic!("{what}, patched path: {e}"));
        assert_eq!(events(bytes), expected, "{what}, patched path");
    }
}

fn refusal(imp: &Imported, allow: Allow) -> String {
    match imp.verify(allow) {
        Err(ImportError::Refused(lines)) => lines.join("\n"),
        other => panic!("expected a refusal, got {other:?}"),
    }
}

#[test]
fn an_empty_dump_is_refused() {
    let (_, imp) = import::import(&b""[..]).unwrap();
    assert!(refusal(&imp, Allow::default()).contains("no rows for any world table"));
    // Declared tables without rows are still empty.
    let (_, imp) = import::import(EVENT_TABLE.as_bytes()).unwrap();
    assert!(refusal(&imp, Allow::default()).contains("no rows for any world table"));
    // So is text that is not SQL, and what it could not read is named too.
    let (_, imp) = import::import(&b"this is not a dump\n"[..]).unwrap();
    let why = refusal(&imp, Allow::default());
    assert!(why.contains("no rows for any world table"), "{why}");
    assert!(
        why.contains("1 statements could not be read: THIS x1 (first at line 1)"),
        "{why}"
    );
}

#[test]
fn a_dump_without_weenies_or_landblock_instances_is_refused() {
    let (_, first) = EVENT_ROWS[0];
    let (_, imp) = import::import(format!("{EVENT_TABLE}{first}").as_bytes()).unwrap();
    let everything = Allow {
        skipped_statements: true,
        unknown_tables: true,
        unknown_columns: true,
    };
    let why = refusal(&imp, everything);
    assert!(why.contains("table `weenie` has no rows"), "{why}");
    assert!(
        why.contains("table `landblock_instance` has no rows"),
        "{why}"
    );
    assert!(
        fixture::imported().1.verify(Allow::default()).is_ok(),
        "the full synthetic world is accepted"
    );
}

#[test]
fn an_unparseable_dump_fails_with_its_line() {
    let unterminated = format!(
        "{EVENT_TABLE}INSERT INTO `event` VALUES (1,'Alpha',-1,-1,1,'2021-11-01 00:00:00'),\n(2,'Beta,5,6,2);\n"
    );
    let err = import::import(unterminated.as_bytes()).unwrap_err();
    assert!(
        matches!(err, ImportError::Sql { line: 3, ref what } if what.contains("unterminated string")),
        "{err}"
    );
    let trailing = format!(
        "{EVENT_TABLE}INSERT INTO `event` VALUES\n(1,'Alpha',-1,-1,1,'2021-11-01 00:00:00')\nON DUPLICATE KEY UPDATE state=2;\n"
    );
    let err = import::import(trailing.as_bytes()).unwrap_err();
    assert!(
        matches!(err, ImportError::Sql { line: 4, ref what } if what.contains("expected '('")),
        "{err}"
    );
    let stranger = format!("{EVENT_TABLE}INSERT INTO `event` (`id`, `colour`) VALUES (1,2);\n");
    let err = import::import(stranger.as_bytes()).unwrap_err();
    assert!(err.to_string().contains("`colour`"), "{err}");
}

#[test]
fn a_statement_the_reader_cannot_read_is_counted_and_refused_unless_allowed() {
    let dump = format!(
        "{}REPLACE INTO `event` VALUES (9,'Late',-1,-1,1,'2021-11-01 00:00:00');\nUPDATE `weenie` SET `type` = 1;\nupdate `weenie` SET `type` = 2;\nINSERT INTO `event` SELECT * FROM `event`;\n",
        fixture::dump()
    );
    let (_, imp) = import::import(dump.as_bytes()).unwrap();
    let kinds: Vec<(&str, u64)> = imp
        .skipped_statements
        .iter()
        .map(|(k, s)| (k.as_str(), s.count))
        .collect();
    assert_eq!(
        kinds,
        [("INSERT … SELECT", 1), ("REPLACE", 1), ("UPDATE", 2)]
    );
    let why = refusal(&imp, Allow::default());
    assert!(why.contains("4 statements could not be read"), "{why}");
    assert!(why.contains("UPDATE x2"), "{why}");
    imp.verify(Allow {
        skipped_statements: true,
        ..Allow::default()
    })
    .unwrap();
    assert_eq!(
        imp.report_json()["skipped_statements"]["UPDATE"]["count"],
        2
    );
}

/// The synthetic world with a table ACE's world database does not have.
fn with_unknown_table() -> String {
    format!(
        "{}CREATE TABLE `exploration_sites` (\n  `id` int NOT NULL,\n  `landblock` int NOT NULL\n) ENGINE=InnoDB;\nINSERT INTO `exploration_sites` VALUES (1,2),(3,4);\n",
        fixture::dump()
    )
}

#[test]
fn an_unknown_table_is_reported_and_refused_unless_allowed() {
    let (_, imp) = import::import(with_unknown_table().as_bytes()).unwrap();
    assert_eq!(imp.unknown_tables.get("exploration_sites"), Some(&2));
    let why = refusal(&imp, Allow::default());
    assert!(
        why.contains("1 tables are not ACE world tables: `exploration_sites` (2 rows)"),
        "{why}"
    );
    imp.verify(Allow {
        unknown_tables: true,
        ..Allow::default()
    })
    .unwrap();
}

#[test]
fn an_unknown_column_is_reported_and_refused_unless_allowed() {
    let dump = fixture::dump().replacen(
        "CREATE TABLE `landblock_instance` (\n",
        "CREATE TABLE `landblock_instance` (\n  `secondary_To` int unsigned DEFAULT NULL,\n",
        1,
    );
    let (_, imp) = import::import(dump.as_bytes()).unwrap();
    assert_eq!(
        imp.unread_columns,
        [("landblock_instance".to_owned(), "secondary_To".to_owned())]
    );
    let why = refusal(&imp, Allow::default());
    assert!(
        why.contains(
            "1 columns are not in ACE's world schema: `landblock_instance.secondary_To` (6 rows)"
        ),
        "{why}"
    );
    imp.verify(Allow {
        unknown_columns: true,
        ..Allow::default()
    })
    .unwrap();
}

fn temp_dir(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("serv-content-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

#[test]
fn the_importer_writes_no_pack_from_a_dump_it_refuses() {
    let dir = temp_dir("dump-reader");
    let run = |dump: &str, out: &str, flags: &[&str]| {
        let sql = dir.join(format!("{out}.sql"));
        std::fs::write(&sql, dump).unwrap();
        let o = Command::new(env!("CARGO_BIN_EXE_empyrean-import"))
            .arg("--sql")
            .arg(&sql)
            .args(flags)
            .arg("--out")
            .arg(dir.join(out))
            .output()
            .unwrap();
        (
            o.status.code(),
            String::from_utf8_lossy(&o.stdout).into_owned(),
            String::from_utf8_lossy(&o.stderr).into_owned(),
            dir.join(out).exists(),
        )
    };

    let (code, _, err, written) = run("", "empty.pack", &[]);
    assert_eq!(code, Some(1));
    assert!(err.contains("no rows for any world table"), "{err}");
    assert!(!written);

    let (code, _, err, written) = run(&with_unknown_table(), "unknown.pack", &[]);
    assert_eq!(code, Some(1));
    assert!(err.contains("`exploration_sites` (2 rows)"), "{err}");
    assert!(!written);

    let (code, out, _, written) = run(
        &with_unknown_table(),
        "allowed.pack",
        &["--allow-unknown-tables"],
    );
    assert_eq!(code, Some(0));
    assert!(written);
    assert!(
        out.contains("warning: table `exploration_sites` (2 rows) is not an ACE world table"),
        "{out}"
    );

    let (code, _, err, written) = run(&sqlyog_layout(&fixture::dump()), "sqlyog.pack", &[]);
    assert_eq!(code, Some(0), "{err}");
    assert!(written);
    let _ = std::fs::remove_dir_all(&dir);
}
