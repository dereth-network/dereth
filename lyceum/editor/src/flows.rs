//! Making content by what it is for: "New ..." for an NPC who talks, a vendor, a creature with
//! loot, a quest giver, an item, a door, a portal and a chest, each from a blank of its kind with
//! its first edits; and every weenie's spawns listed, and a portal's destination set.

use dereth_primitives::Vec3;
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::Weenie;

use super::form::FormEdit;
use super::kinds::Kind;
use super::EditorApp;

/// What a weenie is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Talker,
    Vendor,
    Creature,
    QuestGiver,
    Item,
    Door,
    Portal,
    Chest,
}

/// The weenie types the flows write, as the server numbers them.
mod kind {
    pub const VENDOR: u32 = 12;
}

/// The emote categories and actions the flows write, as the server numbers them.
mod emote {
    pub const USE: u32 = 7;
    pub const QUEST_SUCCESS: u32 = 12;
    pub const QUEST_FAILURE: u32 = 13;
    pub const TELL: u32 = 10;
    pub const INQ_QUEST: u32 = 21;
    pub const STAMP_QUEST: u32 = 22;
}

impl Flow {
    pub const ALL: [Self; 8] = [
        Self::Talker,
        Self::Vendor,
        Self::Creature,
        Self::QuestGiver,
        Self::Item,
        Self::Door,
        Self::Portal,
        Self::Chest,
    ];

    /// Its key in the editor's text (`flows.<key>`) and in a weenie's symbol.
    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Talker => "talker",
            Self::Vendor => "vendor",
            Self::Creature => "creature",
            Self::QuestGiver => "quest_giver",
            Self::Item => "item",
            Self::Door => "door",
            Self::Portal => "portal",
            Self::Chest => "chest",
        }
    }

    /// The server's class the flow starts from a blank of.
    #[must_use]
    pub const fn blank(self) -> u32 {
        match self {
            Self::Talker | Self::Vendor | Self::QuestGiver => 5773,
            Self::Creature => 7,
            Self::Item => 280,
            Self::Door => 278,
            Self::Portal => 371,
            Self::Chest => 143,
        }
    }

    /// The kind of weenie it makes.
    #[must_use]
    pub const fn kind(self) -> Kind {
        match self {
            Self::Talker | Self::QuestGiver => Kind::Npc,
            Self::Vendor => Kind::Vendor,
            Self::Creature => Kind::Creature,
            Self::Item => Kind::Item,
            Self::Door => Kind::Door,
            Self::Portal => Kind::Portal,
            Self::Chest => Kind::Chest,
        }
    }
}

/// The symbol of a weenie named `words` as players see it: lowercase letters and digits, the rest
/// hyphens.
#[must_use]
pub fn symbol_of(words: &str) -> String {
    words
        .to_lowercase()
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() { c } else { '-' })
        .collect::<String>()
        .split('-')
        .filter(|w| !w.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

/// The emote set of `category` (for `quest`, when one is named) in `doc`.
fn set_of(doc: &SqlDoc, category: u32, quest: Option<&str>) -> Option<usize> {
    doc.emote_sets().iter().position(|s| {
        let c = doc
            .cell(s.emote, "category")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        let q = doc.cell(s.emote, "quest").map(|v| content::unquote(&v));
        c == Some(category)
            && quest.is_none_or(|want| q.as_deref() == Some(want) || q.as_deref() == Some(""))
    })
}

/// The quest `doc`'s use emote asks after.
#[must_use]
pub fn quest_of(doc: &SqlDoc) -> Option<String> {
    let set = *doc.emote_sets().get(set_of(doc, emote::USE, None)?)?;
    doc.actions(&set).into_iter().find_map(|a| {
        let t = doc
            .cell(a, "type")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        (t == Some(emote::INQ_QUEST))
            .then(|| doc.cell(a, "message").map(|m| content::unquote(&m)))
            .flatten()
    })
}

/// `doc` saying `text` when an emote of `category` runs (for `quest`): the set's first tell set
/// to it, or the set and its tell added.
pub fn set_says(doc: &mut SqlDoc, category: u32, quest: Option<&str>, text: &str) {
    let message = content::quote(text);
    let index = match set_of(doc, category, quest) {
        Some(i) => i,
        None => {
            let mut fields = Vec::new();
            if let Some(q) = quest {
                fields.push(("quest", content::quote(q)));
            }
            doc.add_emote_set(&category.to_string(), &fields)
        }
    };
    let Some(set) = doc.emote_sets().get(index).copied() else {
        return;
    };
    let tell = doc.actions(&set).into_iter().find(|a| {
        doc.cell(*a, "type")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v))
            == Some(emote::TELL)
    });
    match tell {
        Some(a) => {
            doc.set_cell(a, "message", &message);
        }
        None => {
            let n = doc.actions(&set).len();
            doc.add_action(
                index,
                n,
                &[
                    ("type", emote::TELL.to_string()),
                    ("delay", "0".into()),
                    ("extent", "1".into()),
                    ("message", message),
                ],
            );
        }
    }
}

/// `doc` asking after quest `quest` when used: the use emote asks after it, the quest's failure
/// answer stamps it (once the player has heard what it asks), and both answers name it.
pub fn set_quest(doc: &mut SqlDoc, quest: &str) {
    let q = content::quote(quest);
    // The use emote's ask.
    let index = match set_of(doc, emote::USE, None) {
        Some(i) => i,
        None => doc.add_emote_set(&emote::USE.to_string(), &[]),
    };
    if let Some(set) = doc.emote_sets().get(index).copied() {
        let ask = doc.actions(&set).into_iter().find(|a| {
            doc.cell(*a, "type")
                .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                == Some(emote::INQ_QUEST)
        });
        match ask {
            Some(a) => {
                doc.set_cell(a, "message", &q);
            }
            None => {
                let n = doc.actions(&set).len();
                doc.add_action(
                    index,
                    n,
                    &[
                        ("type", emote::INQ_QUEST.to_string()),
                        ("delay", "0".into()),
                        ("extent", "1".into()),
                        ("message", q.clone()),
                    ],
                );
            }
        }
    }
    // The two answers, by the quest.
    for category in [emote::QUEST_SUCCESS, emote::QUEST_FAILURE] {
        let index = match set_of(doc, category, None) {
            Some(i) => i,
            None => doc.add_emote_set(&category.to_string(), &[("quest", q.clone())]),
        };
        let Some(set) = doc.emote_sets().get(index).copied() else {
            continue;
        };
        doc.set_cell(set.emote, "quest", &q);
        if category == emote::QUEST_FAILURE {
            let stamp = doc.actions(&set).into_iter().find(|a| {
                doc.cell(*a, "type")
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                    == Some(emote::STAMP_QUEST)
            });
            match stamp {
                Some(a) => {
                    doc.set_cell(a, "message", &q);
                }
                None => {
                    let n = doc.actions(&set).len();
                    doc.add_action(
                        index,
                        n,
                        &[
                            ("type", emote::STAMP_QUEST.to_string()),
                            ("delay", "0".into()),
                            ("extent", "1".into()),
                            ("message", q.clone()),
                        ],
                    );
                }
            }
        }
    }
}

/// The changes a new weenie of `flow` starts with, over its blank, its words from `t`.
#[must_use]
pub fn start(flow: Flow, t: &crate::text::Table) -> Vec<FormEdit> {
    let mut out = Vec::new();
    let prop = |kind, id, v: &str| FormEdit::Property {
        kind,
        id,
        value: Some(v.to_owned()),
    };
    match flow {
        Flow::Talker => {
            out.push(prop(PropertyKind::Bool, 19, "False"));
            out.push(prop(PropertyKind::Int, 16, "32"));
            out.push(FormEdit::Says {
                category: emote::USE,
                text: t.get("flows.start_says").to_owned(),
            });
        }
        Flow::Vendor => {
            out.push(FormEdit::WeenieType(kind::VENDOR));
            out.push(prop(PropertyKind::Bool, 19, "False"));
            out.push(prop(PropertyKind::Int, 16, "32"));
            out.push(prop(PropertyKind::Int, 74, "2304"));
            out.push(prop(PropertyKind::Int, 76, "2000"));
            out.push(prop(PropertyKind::Float, 37, "0.8"));
            out.push(prop(PropertyKind::Float, 38, "1.2"));
        }
        Flow::QuestGiver => {
            out.push(prop(PropertyKind::Bool, 19, "False"));
            out.push(prop(PropertyKind::Int, 16, "32"));
            out.push(FormEdit::QuestGiver {
                quest: t.get("flows.start_quest").to_owned(),
            });
            out.push(FormEdit::Says {
                category: emote::QUEST_FAILURE,
                text: t.get("flows.start_asks").to_owned(),
            });
            out.push(FormEdit::Says {
                category: emote::QUEST_SUCCESS,
                text: t.get("flows.start_thanks").to_owned(),
            });
        }
        Flow::Creature => {
            out.push(prop(PropertyKind::Bool, 19, "True"));
            out.push(prop(PropertyKind::Int, 25, "5"));
        }
        Flow::Item => {
            out.push(prop(PropertyKind::Int, 19, "10"));
        }
        Flow::Door | Flow::Chest => {
            out.push(prop(PropertyKind::Bool, 3, "False"));
        }
        Flow::Portal => {}
    }
    out
}

impl EditorApp {
    /// Where weenie `w` appears: every spawn of it in the world, the packs' and the server's
    /// own in every landblock, loaded or not, by landblock.
    #[must_use]
    pub fn spawns_of(&self, w: &Weenie) -> Vec<lyceum_foundry::edit::Spawn> {
        self.session.appearances(w)
    }

    /// The camera's place as portal `w`'s destination: its cell and spot.
    ///
    /// # Errors
    /// The camera is off the world, or the weenie is not the pack's to change.
    pub fn destination_here(&mut self, w: &Weenie) -> Result<(), String> {
        let (lb, x, y) = self
            .work_block()
            .ok_or_else(|| self.say("messages.off_world"))?;
        let o = lyceum_foundry::pick::block_origin(lb);
        let z = self.camera.position.z - o.z;
        let (cell, floor) = match self.camera_cell() {
            Some((c, _)) => {
                let index = u16::try_from(c.0 & 0xFFFF).unwrap_or(0);
                let floor = self
                    .session
                    .cell_at(lb, Vec3::new(x, y, z))
                    .map_or(z, |(_, f)| f);
                (u32::from(lb) << 16 | u32::from(index), floor)
            }
            None => (
                (u32::from(lb) << 16) | lyceum_foundry::edit::outdoor_cell(x, y),
                self.session.ground(lb, x, y).unwrap_or(z),
            ),
        };
        self.form_edit(
            w,
            &FormEdit::Destination {
                cell,
                at: [x, y, floor],
            },
        )
    }

    /// Dungeon `lb`'s drop point as portal `w`'s destination.
    ///
    /// # Errors
    /// The dungeon has no cell, or the weenie is not the pack's to change.
    pub fn destination_dungeon(&mut self, w: &Weenie, lb: u16) -> Result<(), String> {
        let (cell, at) = self.drop_point(lb)?;
        self.form_edit(
            w,
            &FormEdit::Destination {
                cell,
                at: [at.x, at.y, at.z],
            },
        )
    }

    /// Where the portal of `doc` sends who uses it, in words: its dungeon's name, else its cell.
    pub(super) fn destination_words(&mut self, doc: &SqlDoc) -> String {
        let t = self.text.table();
        let now = doc
            .rows("weenie_properties_position")
            .into_iter()
            .find(|r| doc.cell(*r, "position_Type").as_deref() == Some("2"))
            .and_then(|r| doc.cell(r, "obj_Cell_Id"))
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        now.map_or_else(
            || t.get("fields.nowhere").to_owned(),
            |c| {
                let lb = u16::try_from(c >> 16).unwrap_or(0);
                self.dungeons()
                    .iter()
                    .find(|d| d.landblock == lb && c & 0xFFFF >= 0x0100)
                    .map_or_else(
                        || format!("0x{c:08X}"),
                        |d| format!("{} (0x{c:08X})", self.dungeon_name(d)),
                    )
            },
        )
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's new weenies by what they are for)
    use super::*;

    /// A blank NPC with the first changes of `flow`, its words from `t`.
    fn made(flow: Flow, t: &crate::text::Table) -> SqlDoc {
        let mut doc = SqlDoc::parse(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES ({{wcid:sage}}, 'sage', 10, '2005-02-09 10:00:00');\n",
        );
        for e in start(flow, t) {
            assert!(super::super::form::apply_edit(&mut doc, &e, None), "{e:?}");
        }
        doc
    }

    /// Each emote set of `doc`: what calls it, the quest it answers to, and its actions as the
    /// Emotes page's lines.
    fn sets(doc: &SqlDoc, t: &crate::text::Table) -> Vec<(u32, String, Vec<String>)> {
        let words = |r: content::RowRef, c: &str| {
            doc.cell(r, c)
                .filter(|v| v.trim() != "NULL")
                .map(|v| content::unquote(&v))
                .unwrap_or_default()
        };
        let number = |r: content::RowRef, c: &str| {
            doc.cell(r, c)
                .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                .unwrap_or(0)
        };
        doc.emote_sets()
            .iter()
            .map(|s| {
                let lines = doc
                    .actions(s)
                    .into_iter()
                    .map(|a| {
                        super::super::emotes::action_words(t, number(a, "type"), "", &|c| {
                            words(a, c)
                        })
                    })
                    .collect();
                (number(s.emote, "category"), words(s.emote, "quest"), lines)
            })
            .collect()
    }

    #[test]
    fn a_new_npc_tells_its_greeting_when_used() {
        let t = crate::text::Text::built_in().table();
        let tell = t
            .get("emotes.action_tell")
            .replace("{message}", t.get("flows.start_says"));
        assert_eq!(
            sets(&made(Flow::Talker, &t), &t),
            [(emote::USE, String::new(), vec![tell])]
        );
    }

    #[test]
    fn a_new_quest_giver_asks_after_its_quest_stamps_it_and_answers_both_ways() {
        let t = crate::text::Text::built_in().table();
        let line = |key: &str, message: &str| t.get(key).replace("{message}", message);
        let quest = t.get("flows.start_quest");
        let mut doc = made(Flow::QuestGiver, &t);
        // Used, it asks after its quest; done, it thanks them; not done, it stamps it and asks.
        assert_eq!(
            sets(&doc, &t),
            [
                (
                    emote::USE,
                    String::new(),
                    vec![line("emotes.action_asks_quest", quest)]
                ),
                (
                    emote::QUEST_SUCCESS,
                    quest.to_owned(),
                    vec![line("emotes.action_tell", t.get("flows.start_thanks"))]
                ),
                (
                    emote::QUEST_FAILURE,
                    quest.to_owned(),
                    vec![
                        line("emotes.action_stamps", quest),
                        line("emotes.action_tell", t.get("flows.start_asks"))
                    ]
                ),
            ]
        );
        assert_eq!(quest_of(&doc).as_deref(), Some(quest));
        assert_eq!(super::super::form::quests_named(&doc), [quest]);
        // Asked again, the same sets: nothing doubled.
        set_quest(&mut doc, "a-bigger-favor");
        set_says(
            &mut doc,
            emote::QUEST_SUCCESS,
            Some("a-bigger-favor"),
            "Thanks again.",
        );
        let now = sets(&doc, &t);
        assert_eq!(now.len(), 3);
        assert_eq!(quest_of(&doc).as_deref(), Some("a-bigger-favor"));
        assert_eq!(
            now[1],
            (
                emote::QUEST_SUCCESS,
                "a-bigger-favor".to_owned(),
                vec![line("emotes.action_tell", "Thanks again.")]
            )
        );
        assert_eq!(super::super::form::quests_named(&doc), ["a-bigger-favor"]);
    }
}
