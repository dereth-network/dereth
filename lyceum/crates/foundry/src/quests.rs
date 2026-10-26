//! The world's quests: every quest the packs and the server register, and every one something
//! names without registering it; where each is named and how; and the edits that register one
//! again in the pack, copy one under a new name, rename one through the pack's files, or take
//! the pack's registration of one away.
//!
//! A quest is known by its name before any `@` (what follows names a count or bits of it), in
//! lower case, as the server looks it up ([`quest_key`]). The packs' weenies name a quest as their
//! files have them (a later pack's file of a weenie over an earlier's); the server's own as its
//! database has them, but for a weenie a pack writes again, whose pack's file stands for it.
//!
//! A rename changes the pack being edited's files alone: its quest's own row, and every emote
//! action, answering set and string property of its weenies that names the quest, each `@` suffix
//! kept. What the server's database and the other packs name is left as it is.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use crate::content::{self, SqlDoc, EMOTE_ACTIONS};
use crate::content_check::{quest_key, QUEST_ACTIONS};
use crate::edit::{token_symbol, Op, Session, Weenie};
use crate::error::Error;
use crate::sql_ids::{self, Quest, QuestUse, QUEST_ANSWERS, QUEST_PROPERTIES};

/// One weenie's naming of a quest: the weenie, how, and the pack file that names it (`None` for
/// the server's database).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QuestUser {
    pub by: Weenie,
    pub how: QuestUse,
    pub file: Option<PathBuf>,
}

/// Whose a quest is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Whose {
    /// A pack's own, which the server does not register (`mine`: the pack being edited's).
    Pack { mine: bool },
    /// The server's, as its database registers it.
    Server,
    /// The server's, registered again by a pack, whose row stands for it.
    Changed { mine: bool },
    /// Named, but registered nowhere.
    Nobody,
}

/// A quest of the world.
#[derive(Clone, Debug, PartialEq)]
pub struct WorldQuest {
    /// Its name: as registered (a pack's spelling over the server's), else as first named.
    pub name: String,
    /// Its row as it stands (a pack's over the server's), and the server's own.
    pub quest: Option<Quest>,
    pub server: Option<Quest>,
    /// The pack file that registers it, and that pack's name.
    pub file: Option<PathBuf>,
    pub pack: Option<String>,
    pub whose: Whose,
    pub uses: Vec<QuestUser>,
}

impl WorldQuest {
    /// How many weenies name it.
    #[must_use]
    pub fn places(&self) -> usize {
        let mut seen: HashSet<&Weenie> = HashSet::new();
        self.uses.iter().filter(|u| seen.insert(&u.by)).count()
    }
}

/// A quest renamed: each file of the pack being edited it writes (its new text, or `None` for one
/// it takes away), and each naming of the quest it leaves as it is, the server's and the other
/// packs'.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Renamed {
    pub files: Vec<(PathBuf, Option<String>)>,
    pub left: Vec<QuestUser>,
}

impl Renamed {
    /// The rename as one change of the files.
    #[must_use]
    pub fn op(&self) -> Op {
        Op::Batch(
            self.files
                .iter()
                .map(|(file, text)| Op::WriteSql {
                    file: file.clone(),
                    text: text.clone(),
                })
                .collect(),
        )
    }
}

/// A quoted value naming quest `key` (as registered), renamed `new`: its `@` suffix kept. `None`
/// for a value naming another quest, or none.
fn renamed_value(raw: &str, key: &str, new: &str) -> Option<String> {
    let v = content::unquote(raw);
    let at = v.find('@').unwrap_or(v.len());
    let (base, rest) = v.split_at(at);
    (!base.trim().is_empty() && base.trim().to_lowercase() == key)
        .then(|| content::quote(&format!("{new}{rest}")))
}

/// The statements `DELETE FROM `quest` WHERE `name` = '...';` of `text`, each with the byte range
/// of its quoted name and the range of the whole statement with the line break after it.
fn quest_deletes(text: &str) -> Vec<(std::ops::Range<usize>, std::ops::Range<usize>)> {
    const HEAD: &str = "DELETE FROM `quest` WHERE `name` = ";
    let mut out = Vec::new();
    let mut from = 0;
    while let Some(i) = text[from..].find(HEAD) {
        let start = from + i;
        let value = start + HEAD.len();
        let Some(end) = text[value..].find(';').map(|e| value + e) else {
            break;
        };
        let mut whole = end + 1;
        if text[whole..].starts_with("\r\n") {
            whole += 2;
        } else if text[whole..].starts_with('\n') {
            whole += 1;
        }
        out.push((value..end, start..whole));
        from = end;
    }
    out
}

/// `text` with quest `old` (as registered) renamed `new` wherever it names it: its emote actions'
/// messages, the labels of the sets that answer a test of it, the string properties that name a
/// quest, its `quest` rows and the `DELETE`s before them; each `@` suffix kept. `None` when it
/// names no such quest.
#[must_use]
pub fn rename_in(text: &str, old: &str, new: &str) -> Option<String> {
    let key = quest_key(old);
    if key.is_empty() || !text.to_lowercase().contains(&key) {
        return None;
    }
    let mut doc = SqlDoc::parse(text);
    let mut changed = false;
    let mut rename = |doc: &mut SqlDoc, table: &str, kind: Option<(&str, &[u32])>, column: &str| {
        for r in doc.rows(table) {
            if let Some((c, kinds)) = kind {
                let k = doc.cell(r, c).and_then(|v| sql_ids::number(&v));
                if !k.is_some_and(|k| kinds.contains(&k)) {
                    continue;
                }
            }
            let Some(v) = doc
                .raw(r, column)
                .and_then(|raw| renamed_value(raw, &key, new))
            else {
                continue;
            };
            changed |= doc.set_cell(r, column, &v);
        }
    };
    rename(
        &mut doc,
        EMOTE_ACTIONS,
        Some(("type", &QUEST_ACTIONS[..])),
        "message",
    );
    rename(
        &mut doc,
        "weenie_properties_emote",
        Some(("category", &QUEST_ANSWERS[..])),
        "quest",
    );
    rename(
        &mut doc,
        "weenie_properties_string",
        Some(("type", &QUEST_PROPERTIES[..])),
        "value",
    );
    rename(&mut doc, "quest", None, "name");
    let mut out = doc.render();
    for (value, _) in quest_deletes(&out).into_iter().rev() {
        if let Some(v) = renamed_value(&out[value.clone()], &key, new) {
            out.replace_range(value, &v);
            changed = true;
        }
    }
    changed.then_some(out)
}

/// Whether `text` holds nothing but comments and blank lines.
fn holds_nothing(text: &str) -> bool {
    let mut rest = text;
    let mut kept = String::new();
    while let Some(i) = rest.find("/*") {
        kept.push_str(&rest[..i]);
        rest = rest[i..].find("*/").map_or("", |e| &rest[i + e + 2..]);
    }
    kept.push_str(rest);
    kept.lines()
        .all(|l| l.trim().is_empty() || l.trim_start().starts_with("--"))
}

/// `text` without its `quest` rows of `name` (as registered) and the `DELETE`s before them:
/// `Some(None)` when nothing else is left in it, `None` when it registers no such quest.
#[must_use]
pub fn without_quest(text: &str, name: &str) -> Option<Option<String>> {
    let key = quest_key(name);
    let mut doc = SqlDoc::parse(text);
    let rows: Vec<_> = doc
        .rows("quest")
        .into_iter()
        .filter(|r| {
            doc.raw(*r, "name")
                .is_some_and(|n| quest_key(&content::unquote(n)) == key)
        })
        .collect();
    if rows.is_empty() {
        return None;
    }
    for r in rows.into_iter().rev() {
        doc.remove_row(r);
    }
    let mut out = doc.render();
    for (value, whole) in quest_deletes(&out).into_iter().rev() {
        if quest_key(&content::unquote(&out[value])) == key {
            out.replace_range(whole, "");
        }
    }
    Some((!holds_nothing(&out)).then_some(out))
}

impl Session {
    /// Every quest of the world, by name: those the packs and the server register, and those
    /// something names that nothing registers; each with where it is registered and every
    /// weenie that names it.
    #[must_use]
    pub fn world_quests(&self) -> Vec<WorldQuest> {
        let mut all: BTreeMap<String, WorldQuest> = BTreeMap::new();
        let blank = |name: &str| WorldQuest {
            name: name.to_owned(),
            quest: None,
            server: None,
            file: None,
            pack: None,
            whose: Whose::Nobody,
            uses: Vec::new(),
        };
        for (n, q) in self.server_quests() {
            let e = all.entry(quest_key(n)).or_insert_with(|| blank(n));
            e.quest = Some(q.clone());
            e.server = Some(q.clone());
            e.whose = Whose::Server;
        }
        // A later pack's registration over an earlier's, and in a pack, its first file's.
        let mut registered: HashSet<String> = HashSet::new();
        for (pi, p) in self.packs().iter().enumerate().rev() {
            for f in &p.sql {
                for (n, q) in sql_ids::quests_in(&f.text) {
                    let key = quest_key(&n);
                    if !registered.insert(key.clone()) {
                        continue;
                    }
                    let e = all.entry(key).or_insert_with(|| blank(&n));
                    let mine = pi == self.target();
                    e.name = n;
                    e.quest = Some(q);
                    e.file = Some(f.path.clone());
                    e.pack = Some(p.name.clone());
                    e.whose = if e.server.is_some() {
                        Whose::Changed { mine }
                    } else {
                        Whose::Pack { mine }
                    };
                }
            }
        }
        // The packs' weenies, a later pack's file of one over an earlier's.
        let mut written: HashSet<Weenie> = HashSet::new();
        for p in self.packs().iter().rev() {
            let as_weenie = |v: &str| match sql_ids::number(v) {
                Some(c) => Some(Weenie::Class(c)),
                None => token_symbol(&p.name, v).map(Weenie::Symbol),
            };
            for f in &p.sql {
                let uses = sql_ids::quest_uses(&f.text);
                let owners: Vec<Weenie> = sql_ids::weenie_facts(&f.text)
                    .keys()
                    .filter_map(|k| as_weenie(k))
                    .collect();
                if owners.iter().any(|o| written.contains(o)) {
                    continue;
                }
                for u in uses {
                    let Some(by) = as_weenie(&u.by) else { continue };
                    let base = u.quest.split('@').next().unwrap_or("").trim().to_owned();
                    all.entry(quest_key(&u.quest))
                        .or_insert_with(|| blank(&base))
                        .uses
                        .push(QuestUser {
                            by,
                            how: u.how,
                            file: Some(f.path.clone()),
                        });
                }
                written.extend(owners);
            }
        }
        // The server's own, but those a pack writes again.
        let classes = self.pack_classes();
        for (key, named) in self.server_quest_uses() {
            let users: Vec<QuestUser> = named
                .by
                .iter()
                .filter(|(c, _)| !written.contains(&Weenie::Class(*c)) && !classes.contains_key(c))
                .map(|(c, how)| QuestUser {
                    by: Weenie::Class(*c),
                    how: *how,
                    file: None,
                })
                .collect();
            if !users.is_empty() {
                all.entry(key.clone())
                    .or_insert_with(|| blank(&named.name))
                    .uses
                    .extend(users);
            }
        }
        let mut out: Vec<WorldQuest> = all.into_values().collect();
        out.sort_by_cached_key(|q| (q.name.to_lowercase(), q.name.clone()));
        out
    }

    /// Quest `name` of the world, as [`Self::world_quests`] has it.
    #[must_use]
    pub fn world_quest(&self, name: &str) -> Option<WorldQuest> {
        let key = quest_key(name);
        self.world_quests()
            .into_iter()
            .find(|q| quest_key(&q.name) == key)
    }

    /// The pack being edited's quest `name`, of `whose`, or the refusal.
    fn quest_of(&self, name: &str, whose: Whose) -> Result<WorldQuest, Error> {
        self.world_quest(name)
            .filter(|q| q.whose == whose)
            .ok_or_else(|| {
                Error::Refused(format!(
                    "{name} is not a quest the pack being edited may change so"
                ))
            })
    }

    /// Whether `new` may be a new quest's name: a name, no `@` in it, that no quest of the world
    /// has (or names) yet.
    fn free_quest_name(&self, new: &str) -> Result<String, Error> {
        let new = new.trim();
        if new.is_empty() || new.contains('@') || new.chars().any(char::is_control) {
            return Err(Error::Refused(format!("{new:?} is not a quest's name")));
        }
        if self.world_quest(new).is_some() {
            return Err(Error::Refused(format!(
                "a quest named {new} is there already"
            )));
        }
        Ok(new.to_owned())
    }

    /// Server quest `name` registered again in the pack being edited, as the server has it: the
    /// pack's row then stands for it, to be changed.
    ///
    /// # Errors
    /// No quest of the server's by that name, or one a pack registers already.
    pub fn quest_change_here(&self, name: &str) -> Result<Op, Error> {
        let q = self.quest_of(name, Whose::Server)?;
        let row = q.server.unwrap_or_default();
        Ok(Op::WriteSql {
            file: self.quest_file(&q.name),
            text: Some(self.quest_text(&q.name, &row)),
        })
    }

    /// The pack being edited's registration of quest `name` taken away: the server's (put back),
    /// or the pack's own quest, which nothing may name still (deleted).
    ///
    /// # Errors
    /// Not the pack being edited's registration of a server's quest (`server`), or of a quest of
    /// its own that nothing names.
    pub fn quest_unregister(&self, name: &str, server: bool) -> Result<Op, Error> {
        let whose = if server {
            Whose::Changed { mine: true }
        } else {
            Whose::Pack { mine: true }
        };
        let q = self.quest_of(name, whose)?;
        if !server && !q.uses.is_empty() {
            return Err(Error::Refused(format!(
                "{} is still named in {} place(s)",
                q.name,
                q.places()
            )));
        }
        let file = q
            .file
            .ok_or_else(|| Error::Refused(format!("{name} is in no file")))?;
        let text = self
            .packs()
            .iter()
            .flat_map(|p| &p.sql)
            .find(|f| f.path == file)
            .map(|f| f.text.clone())
            .unwrap_or_default();
        let text = without_quest(&text, name)
            .ok_or_else(|| Error::Refused(format!("{name} is not in {}", file.display())))?;
        Ok(Op::WriteSql { file, text })
    }

    /// Quest `name` copied under the name `new`, registered in the pack being edited with its
    /// row as it stands: nothing names the copy yet.
    ///
    /// # Errors
    /// No quest of that name registered, or a name that is not free.
    pub fn quest_copy(&self, name: &str, new: &str) -> Result<Op, Error> {
        let row = self
            .world_quest(name)
            .and_then(|q| q.quest)
            .ok_or_else(|| Error::Refused(format!("{name} is registered nowhere")))?;
        let new = self.free_quest_name(new)?;
        Ok(Op::WriteSql {
            file: self.quest_file(&new),
            text: Some(self.quest_text(&new, &row)),
        })
    }

    /// The pack being edited's own quest `old` renamed `new` through its files: what it writes,
    /// and what names the quest that it leaves (the server's and the other packs').
    ///
    /// # Errors
    /// Not a quest of the pack being edited's own, or a name that is not free.
    pub fn quest_rename(&self, old: &str, new: &str) -> Result<Renamed, Error> {
        let q = self.quest_of(old, Whose::Pack { mine: true })?;
        let new = if quest_key(new) == quest_key(old) {
            new.trim().to_owned()
        } else {
            self.free_quest_name(new)?
        };
        if new.is_empty() || new.contains('@') {
            return Err(Error::Refused(format!("{new:?} is not a quest's name")));
        }
        let pack = &self.packs()[self.target()];
        let mut files: Vec<(PathBuf, Option<String>)> = Vec::new();
        for f in &pack.sql {
            let Some(text) = rename_in(&f.text, &q.name, &new) else {
                continue;
            };
            // The quest's own file, which registers it alone, takes its new name's.
            let to = self.quest_file(&new);
            let own = q.file.as_ref() == Some(&f.path)
                && f.path == self.quest_file(&q.name)
                && sql_ids::quests_in(&f.text).len() == 1
                && !f
                    .path
                    .to_string_lossy()
                    .eq_ignore_ascii_case(&to.to_string_lossy())
                && !pack.sql.iter().any(|g| g.path == to);
            if own {
                files.push((f.path.clone(), None));
                files.push((to, Some(text)));
            } else {
                files.push((f.path.clone(), Some(text)));
            }
        }
        let mine: HashSet<&PathBuf> = pack.sql.iter().map(|f| &f.path).collect();
        let left = q
            .uses
            .iter()
            .filter(|u| u.file.as_ref().is_none_or(|f| !mine.contains(f)))
            .cloned()
            .collect();
        Ok(Renamed { files, left })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: Lyceum's quests renamed in a file)
    use super::*;

    const WEENIE: &str = "\
INSERT INTO `weenie_properties_string` (`object_Id`, `type`, `value`)
VALUES ({{wcid:keeper}},   1, 'Keeper') /* Name */
     , ({{wcid:keeper}},  37, 'GateFlag') /* QuestRestriction */;

INSERT INTO `weenie_properties_emote` (`object_Id`, `category`, `probability`, `weenie_Class_Id`, `style`, `substyle`, `quest`, `vendor_Type`, `min_Health`, `max_Health`)
VALUES ({{wcid:keeper}}, 12 /* QuestSuccess */, 1, NULL, NULL, NULL, 'gateflag@2', NULL, NULL, NULL);

SET @parent_id = LAST_INSERT_ID();

INSERT INTO `weenie_properties_emote_action` (`emote_Id`, `order`, `type`, `delay`, `extent`, `motion`, `message`)
VALUES (@parent_id,  0,  22 /* StampQuest */, 0, 1, NULL, 'GateFlag')
     , (@parent_id,  1,  10 /* Tell */, 0, 1, NULL, 'GateFlag is a word here')
     , (@parent_id,  2,  30 /* InqQuestSolves */, 0, 1, NULL, 'GateFlag@2');
";

    /// The quests `text` names, each as written with how.
    fn named(text: &str) -> Vec<(String, QuestUse)> {
        sql_ids::quest_uses(text)
            .into_iter()
            .map(|q| (q.quest, q.how))
            .collect()
    }

    #[test]
    fn a_rename_keeps_each_suffix_and_leaves_a_tell_that_only_says_the_name() {
        let before = named(WEENIE);
        assert_eq!(before.len(), 4, "{before:?}");
        let out = rename_in(WEENIE, "GateFlag", "Opened").unwrap();
        let after = named(&out);
        let want: Vec<(String, QuestUse)> = before
            .iter()
            .map(|(q, how)| {
                let rest = q.find('@').map_or("", |i| &q[i..]);
                (format!("Opened{rest}"), *how)
            })
            .collect();
        assert_eq!(after, want);
        assert!(
            out.contains("'GateFlag is a word here'"),
            "a tell is no quest"
        );
        assert!(out.contains("'Keeper'"));
        assert_eq!(rename_in(WEENIE, "Other", "Opened"), None);
    }

    #[test]
    fn a_quests_own_file_is_renamed_and_without_it_is_nothing() {
        let file = "DELETE FROM `quest` WHERE `name` = 'GateFlag';\n\n\
                    INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)\n\
                    VALUES ('GateFlag', 60, -1, '', '2026-10-24 00:00:00');\n";
        let out = rename_in(file, "gateflag", "Opened").unwrap();
        assert!(out.starts_with("DELETE FROM `quest` WHERE `name` = 'Opened';"));
        assert!(out.contains("VALUES ('Opened', 60"), "{out}");
        assert_eq!(without_quest(file, "GATEFLAG"), Some(None), "nothing left");
        assert_eq!(without_quest(file, "Other"), None);
        // Beside another quest, only its own row and delete go.
        let two = format!(
            "{file}\nDELETE FROM `quest` WHERE `name` = 'Kept';\n\n\
             INSERT INTO `quest` (`name`, `min_Delta`, `max_Solves`, `message`, `last_Modified`)\n\
             VALUES ('Kept', 1, 1, '', '2026-10-24 00:00:00');\n"
        );
        let left = without_quest(&two, "GateFlag").unwrap().unwrap();
        assert!(
            !left.contains("GateFlag") && left.contains("'Kept'"),
            "{left}"
        );
    }
}
