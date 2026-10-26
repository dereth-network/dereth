//! The editor's footer: what the checks found in its weenie's file, each going to the field it is
//! about when clicked. A finding names the line its row starts on; the row is a property, an
//! entry of a list or an emote, and that is a field of one of the weenie's pages.

use lyceum_foundry::content::{PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;

use super::collections::{self, in_list, CREATE_LIST};
use super::definition::Drill;
use super::fields::property_key;
use super::kinds::Kind;
use super::{EditorApp, UiAction};

/// One finding of a weenie's file: its line, its words without the file's name or the table it
/// names (the table apart), and whether it is a warning.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Finding {
    pub line: usize,
    pub table: Option<String>,
    pub text: String,
    pub warning: bool,
}

/// The findings `faults` (the checks' words, each `file:line: words`) say of `file`, in order.
#[must_use]
pub fn findings_of(faults: &[lyceum_foundry::edit::Fault], file: &std::path::Path) -> Vec<Finding> {
    let prefix = format!("{}:", file.display());
    faults
        .iter()
        .filter_map(|f| {
            let rest = f.text.strip_prefix(&prefix)?;
            let (line, words) = rest.split_once(':')?;
            let line = line.trim().parse::<usize>().ok()?;
            let (table, text) = plain(words.trim());
            Some(Finding {
                line,
                table: table.map(str::to_owned),
                text: text.to_owned(),
                warning: f.warning,
            })
        })
        .collect()
}

/// A finding's words, and the table it starts by naming (`weenie_properties_int: ...`) apart.
fn plain(words: &str) -> (Option<&str>, &str) {
    match words.split_once(": ") {
        Some((table, rest)) if table.starts_with("weenie") && !table.contains(' ') => {
            (Some(table), rest)
        }
        _ => (None, words),
    }
}

/// A finding as the footer says it: a property's by its type's plain name, and starting with a
/// capital.
#[must_use]
pub fn finding_words(f: &Finding, t: &crate::text::Table) -> String {
    let kind = f
        .table
        .as_deref()
        .and_then(PropertyKind::of_table)
        .map(|k| t.get(super::form::kind_name(k)).to_owned());
    let mut chars = f.text.chars();
    let text = chars.next().map_or_else(String::new, |c| {
        c.to_uppercase().chain(chars).collect::<String>()
    });
    match kind {
        Some(k) => format!("{k}: {text}"),
        None => text,
    }
}

/// The key of the field of `doc`'s form that its row starting on line `line` is: a property's,
/// an entry of one of its lists (what it wields, holds and sells, spawns, casts; its skills,
/// body parts, attributes and vitals), one of its emote sets or actions, or its destination.
/// `None` for a row no field shows.
#[must_use]
pub fn key_at_line(doc: &SqlDoc, line: usize) -> Option<String> {
    let r = doc.row_at_line(line)?;
    let table = doc.table_of(r)?.to_owned();
    let place = |t: &str| doc.rows(t).iter().position(|x| *x == r);
    let number = |c: &str| {
        doc.cell(r, c)
            .and_then(|v| lyceum_foundry::sql_ids::number(&v))
    };
    if let Some(kind) = PropertyKind::of_table(&table) {
        return Some(property_key(kind, number("type")?));
    }
    Some(match table.as_str() {
        "weenie" if Kind::of(doc) == Kind::Portal => "flow:destination".to_owned(),
        "weenie" => property_key(PropertyKind::String, 1),
        CREATE_LIST => {
            let all = collections::rows_of(doc, CREATE_LIST);
            let at = place(CREATE_LIST)?;
            let creature = Kind::of(doc).is_creature();
            let d = all[at].bits("destination_Type");
            let (list, name) = [
                (Drill::Wields, "wields"),
                (Drill::Holds, "holds"),
                (Drill::Sells, "sells"),
            ]
            .into_iter()
            .find(|(l, _)| in_list(*l, d, creature))?;
            let i = all[..at]
                .iter()
                .filter(|x| in_list(list, x.bits("destination_Type"), creature))
                .count();
            format!("list:{name}:{i}:stack")
        }
        collections::GENERATOR => format!("spawn:{}:init_Create", place(collections::GENERATOR)?),
        collections::SPELL_BOOK => format!("spell:{}:chance", place(collections::SPELL_BOOK)?),
        collections::SKILLS => format!("skill:{}:level", number("type")?),
        collections::BODY_PARTS => format!("part:{}:d_Val", number("key")?),
        collections::ATTRIBUTES => format!("attribute:{}", number("type")?),
        collections::VITALS => format!("vital:{}:base", number("type")?),
        collections::POSITIONS if number("position_Type") == Some(2) => {
            "flow:destination".to_owned()
        }
        "weenie_properties_emote" => {
            let i = doc.emote_sets().iter().position(|s| s.emote == r)?;
            format!("emote:{i}")
        }
        lyceum_foundry::content::EMOTE_ACTIONS => {
            let i = doc
                .emote_sets()
                .iter()
                .position(|s| s.actions == Some(r.piece))?;
            format!("action:{i}:{}", r.row)
        }
        _ => return None,
    })
}

impl EditorApp {
    /// The findings of weenie `w`'s file, when a pack's file writes it.
    #[must_use]
    pub fn weenie_findings(&self, w: &Weenie) -> Vec<Finding> {
        self.session
            .weenie_file(w)
            .map(|(file, _)| findings_of(&self.faults, &file))
            .unwrap_or_default()
    }

    /// The editor on weenie `w` at the field its file's row on line `line` is, on that field's
    /// page; at Advanced for a row no field shows.
    pub fn jump_to_line(&mut self, w: &Weenie, line: usize) {
        let doc = self
            .session
            .weenie_file(w)
            .and_then(|(file, _)| self.session.sql_text(&file))
            .map(|t| SqlDoc::parse(&t));
        match doc.as_ref().and_then(|d| key_at_line(d, line)) {
            Some(key) => self.editor.focus = Some((w.clone(), key)),
            None => {
                self.editor.focus = None;
                self.editor.page = super::fields::Page::Advanced;
            }
        }
    }

    /// The editor's footer, when the checks found something in its weenie's file: how many, and
    /// each finding, which goes to its field when clicked.
    pub(super) fn findings_footer(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        out: &mut Vec<UiAction>,
    ) {
        self.editor.finding_rows.clear();
        let found = self.weenie_findings(w);
        if found.is_empty() {
            return;
        }
        let t = self.text.table();
        egui::Panel::bottom(egui::Id::new("editor findings"))
            .resizable(false)
            .show(ui, |ui| {
                let faults = found.iter().filter(|f| !f.warning).count();
                let warnings = found.len() - faults;
                let count = |one: &str, many: &str, n: usize| {
                    if n == 1 {
                        t.get(one).to_owned()
                    } else {
                        t.fill(many, &[("n", &n)])
                    }
                };
                let mut head = Vec::new();
                if faults > 0 {
                    head.push(count("checks.error", "checks.errors", faults));
                }
                if warnings > 0 {
                    head.push(count("checks.warning", "checks.warnings", warnings));
                }
                ui.add_space(2.0);
                ui.weak(head.join(", "));
                egui::ScrollArea::vertical()
                    .id_salt("editor findings list")
                    .max_height(66.0)
                    .auto_shrink([false, true])
                    .show(ui, |ui| {
                        for f in &found {
                            let words = finding_words(f, &t);
                            let text =
                                egui::RichText::new(format!("{words} ›")).color(if f.warning {
                                    super::panels::mark::WARN
                                } else {
                                    super::panels::mark::FAULT
                                });
                            let r = ui.add(
                                egui::Label::new(text)
                                    .truncate()
                                    .sense(egui::Sense::click()),
                            );
                            self.editor.finding_rows.push((f.line, r.rect));
                            if r.clicked() {
                                out.push(UiAction::FindingAt(w.clone(), f.line));
                            }
                        }
                    });
            });
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's findings of a weenie)
    use super::*;
    use std::path::Path;

    const CREATURE: &str = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)
VALUES (9, 'thing', 10, '2005-02-09 10:00:00');

INSERT INTO `weenie_properties_int` (`object_Id`, `type`, `value`)
VALUES (9, 25, 5)
     , (9, 81, 3);

INSERT INTO `weenie_properties_create_list` (`object_Id`, `destination_Type`, `weenie_Class_Id`, `stack_Size`, `palette`, `shade`, `try_To_Bond`)
VALUES (9, 1, 5, 1, 0, 0, False)
     , (9, 2, 6, 1, 0, 0, False)
     , (9, 10, 7, 1, 0, 0.5, False);

INSERT INTO `weenie_properties_body_part` (`object_Id`, `key`, `d_Type`, `d_Val`)
VALUES (9, 3, 4, 2);

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`)
VALUES (9, 7, 1);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `message`)
VALUES (@parent_id, 0, 10, 'one')
     , (@parent_id, 1, 10, 'two');
";

    #[test]
    fn a_findings_line_is_the_field_its_row_writes() {
        let doc = SqlDoc::parse(CREATURE);
        let at = |line: usize| key_at_line(&doc, line);
        assert_eq!(at(2).as_deref(), Some("string:1"));
        assert_eq!(at(5).as_deref(), Some("int:25"));
        assert_eq!(at(6).as_deref(), Some("int:81"));
        // What it holds is its own list; its wields are counted apart, a treasure wield among them.
        assert_eq!(at(9).as_deref(), Some("list:holds:0:stack"));
        assert_eq!(at(10).as_deref(), Some("list:wields:0:stack"));
        assert_eq!(at(11).as_deref(), Some("list:wields:1:stack"));
        assert_eq!(at(14).as_deref(), Some("part:3:d_Val"));
        assert_eq!(at(17).as_deref(), Some("emote:0"));
        assert_eq!(at(22).as_deref(), Some("action:0:0"));
        assert_eq!(at(23).as_deref(), Some("action:0:1"));
        assert_eq!(at(1), None, "no row starts before the first");
        // A portal's own row is about where it leads.
        let portal = SqlDoc::parse(&CREATURE.replacen(", 10, '2005", ", 7, '2005", 1));
        assert_eq!(key_at_line(&portal, 2).as_deref(), Some("flow:destination"));
    }

    #[test]
    fn a_files_findings_are_its_own_by_line_without_the_table_they_name() {
        use lyceum_foundry::edit::Fault;
        let file = Path::new("C:/pack/server/sql/weenies/guide.sql");
        let fault = |text: &str, warning: bool| Fault {
            text: text.to_owned(),
            warning,
            at: None,
        };
        let faults = [
            fault(
                &format!(
                    "{}:12: weenie_properties_create_list: a create-list row names class 99999",
                    file.display()
                ),
                false,
            ),
            fault(
                "C:/pack/server/sql/weenies/other.sql:3: weenie: a portal",
                true,
            ),
            fault(
                &format!(
                    "{}:4: quest \"x\" is registered and nothing uses it",
                    file.display()
                ),
                true,
            ),
        ];
        assert_eq!(
            findings_of(&faults, file),
            [
                Finding {
                    line: 12,
                    table: Some("weenie_properties_create_list".into()),
                    text: "a create-list row names class 99999".into(),
                    warning: false
                },
                Finding {
                    line: 4,
                    table: None,
                    text: "quest \"x\" is registered and nothing uses it".into(),
                    warning: true
                },
            ]
        );
        // Said with a capital, a property's by its type.
        let t = crate::text::Text::built_in().table();
        let f = Finding {
            line: 3,
            table: Some("weenie_properties_bool".into()),
            text: "property 19 is not one the server has for this type".into(),
            warning: false,
        };
        assert_eq!(
            finding_words(&f, &t),
            format!(
                "{}: Property 19 is not one the server has for this type",
                t.get("weenie.kind_bool")
            )
        );
    }
}
