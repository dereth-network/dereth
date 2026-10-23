//! Making content by what it is for, over the weenie's full set of properties: "New ..." for an
//! NPC who talks, a vendor, a creature with loot, a quest giver, an item, a door, a portal and a
//! chest, each from a blank of its kind; and every weenie's form shown as the fields its kind
//! needs, by plain names, grouped by what they are for, each value chosen rather than typed as a
//! number. The full set of properties stays under Advanced; and every weenie shows where it
//! appears, with a button to place one where the camera is.

use dereth_primitives::Vec3;
use lyceum_foundry::content::{self, PropertyKind, SqlDoc};
use lyceum_foundry::edit::{Op, Weenie};

use super::form::FormEdit;
use super::{EditorApp, Selection, UiAction};
use crate::camera::landblock_of;

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

/// The weenie types, as the server numbers them.
mod kind {
    pub const PORTAL: u32 = 7;
    pub const CREATURE: u32 = 10;
    pub const VENDOR: u32 = 12;
    pub const DOOR: u32 = 19;
    pub const CHEST: u32 = 20;
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

    /// What the weenie of `doc` is for: by its type, whether it can be attacked, and whether its
    /// emotes ask after a quest.
    #[must_use]
    pub fn of(doc: &SqlDoc) -> Self {
        let ty = doc
            .rows("weenie")
            .first()
            .and_then(|r| doc.cell(*r, "type"))
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        let attackable = doc
            .property(PropertyKind::Bool, 19)
            .is_none_or(|v| !matches!(content::bare(&v).as_str(), "False" | "false" | "0"));
        let asks = super::form::quests_named(doc);
        match ty {
            Some(kind::PORTAL) => Self::Portal,
            Some(kind::VENDOR) => Self::Vendor,
            Some(kind::DOOR) => Self::Door,
            Some(kind::CHEST) => Self::Chest,
            Some(kind::CREATURE) if attackable => Self::Creature,
            Some(kind::CREATURE) if !asks.is_empty() => Self::QuestGiver,
            Some(kind::CREATURE) => Self::Talker,
            _ => Self::Item,
        }
    }

    /// The fields its form shows, in order.
    #[must_use]
    pub fn fields(self) -> Vec<&'static Field> {
        FIELDS
            .iter()
            .filter(|f| f.flows.is_empty() || f.flows.contains(&self))
            .collect()
    }
}

/// What a field is for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Group {
    /// What it is: its name, its looks, its size, its worth. More properties join it by naming it
    /// as their group.
    Basic,
    Use,
    Combat,
    Trade,
    Speech,
}

impl Group {
    pub const ALL: [Self; 5] = [
        Self::Basic,
        Self::Use,
        Self::Combat,
        Self::Trade,
        Self::Speech,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Basic => "flows.basic",
            Self::Use => "flows.use",
            Self::Combat => "flows.combat",
            Self::Trade => "flows.trade",
            Self::Speech => "flows.speech",
        }
    }
}

/// How a field's value is chosen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Widget {
    Text,
    Whole,
    Decimal,
    /// Yes or no; the value the server takes while the weenie sets none.
    YesNo(bool),
    /// One of these values, each by its key in the text (`fields.<key>`).
    Choice(&'static [(u32, &'static str)]),
    /// Any of these bits.
    Flags(&'static [(u32, &'static str)]),
    /// One of the server's death treasures.
    Treasure,
    /// One of the server's creature kinds.
    CreatureKind,
    /// A data file's id: a model or a picture.
    DataId,
}

/// Where a field's value lives in the weenie's SQL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Target {
    Property(PropertyKind, u32),
    /// What it says when an emote of this category runs (a tell).
    Says(u32),
    /// The quest it asks after.
    Quest,
    /// Where it sends who uses it.
    Destination,
    /// What it holds, sells or wields: its create list rows of this destination.
    Holds(u32),
}

/// A field of a flow's form.
#[derive(Debug)]
pub struct Field {
    /// Its plain name's key in the text, `fields.<key>`; its description is `fields.<key>_about`.
    pub key: &'static str,
    pub group: Group,
    pub target: Target,
    pub widget: Widget,
    /// The flows that show it; every flow when empty.
    pub flows: &'static [Flow],
}

const USABLE: &[(u32, &str)] = &[
    (0x01, "usable_no"),
    (0x20, "usable_remote"),
    (0x08, "usable_contained"),
    (0x04, "usable_wielded"),
];

const MERCHANDISE: &[(u32, &str)] = &[
    (0x0000_0101, "buys_weapons"),
    (0x0000_0002, "buys_armor"),
    (0x0000_0004, "buys_clothing"),
    (0x0000_0008, "buys_jewelry"),
    (0x0000_0020, "buys_food"),
    (0x0000_0800, "buys_gems"),
    (0x0000_1000, "buys_components"),
    (0x0000_2000, "buys_writing"),
    (0x0000_4000, "buys_keys"),
    (0x0000_8000, "buys_casters"),
    (0x0008_0000, "buys_mana_stones"),
    (0x0000_0080, "buys_misc"),
];

use Flow::{Chest, Creature, Door, Item, Portal, QuestGiver, Talker, Vendor};

/// Every field a flow shows.
pub static FIELDS: &[Field] = &[
    Field {
        key: "name",
        group: Group::Basic,
        target: Target::Property(PropertyKind::String, 1),
        widget: Widget::Text,
        flows: &[],
    },
    Field {
        key: "model",
        group: Group::Basic,
        target: Target::Property(PropertyKind::DataId, 1),
        widget: Widget::DataId,
        flows: &[],
    },
    Field {
        key: "icon",
        group: Group::Basic,
        target: Target::Property(PropertyKind::DataId, 8),
        widget: Widget::DataId,
        flows: &[Item, Chest, Door, Portal],
    },
    Field {
        key: "description",
        group: Group::Basic,
        target: Target::Property(PropertyKind::String, 16),
        widget: Widget::Text,
        flows: &[Item, Chest, Door, Portal, Vendor],
    },
    Field {
        key: "scale",
        group: Group::Basic,
        target: Target::Property(PropertyKind::Float, 39),
        widget: Widget::Decimal,
        flows: &[],
    },
    Field {
        key: "usable",
        group: Group::Use,
        target: Target::Property(PropertyKind::Int, 16),
        widget: Widget::Choice(USABLE),
        flows: &[Item, Talker, QuestGiver, Vendor],
    },
    Field {
        key: "use_message",
        group: Group::Use,
        target: Target::Property(PropertyKind::String, 14),
        widget: Widget::Text,
        flows: &[Item],
    },
    Field {
        key: "value",
        group: Group::Basic,
        target: Target::Property(PropertyKind::Int, 19),
        widget: Widget::Whole,
        flows: &[Item],
    },
    Field {
        key: "weight",
        group: Group::Basic,
        target: Target::Property(PropertyKind::Int, 5),
        widget: Widget::Whole,
        flows: &[Item],
    },
    Field {
        key: "destination",
        group: Group::Use,
        target: Target::Destination,
        widget: Widget::Text,
        flows: &[Portal],
    },
    Field {
        key: "shows_destination",
        group: Group::Use,
        target: Target::Property(PropertyKind::Bool, 88),
        widget: Widget::YesNo(true),
        flows: &[Portal],
    },
    Field {
        key: "locked",
        group: Group::Use,
        target: Target::Property(PropertyKind::Bool, 3),
        widget: Widget::YesNo(false),
        flows: &[Door, Chest],
    },
    Field {
        key: "starts_locked",
        group: Group::Use,
        target: Target::Property(PropertyKind::Bool, 35),
        widget: Widget::YesNo(false),
        flows: &[Door, Chest],
    },
    Field {
        key: "lock_difficulty",
        group: Group::Use,
        target: Target::Property(PropertyKind::Int, 38),
        widget: Widget::Whole,
        flows: &[Door, Chest],
    },
    Field {
        key: "resets_after",
        group: Group::Use,
        target: Target::Property(PropertyKind::Float, 11),
        widget: Widget::Decimal,
        flows: &[Door, Chest],
    },
    Field {
        key: "holds",
        group: Group::Use,
        target: Target::Property(PropertyKind::Int, 6),
        widget: Widget::Whole,
        flows: &[Chest],
    },
    Field {
        key: "contents",
        group: Group::Use,
        target: Target::Holds(1),
        widget: Widget::Text,
        flows: &[Chest],
    },
    Field {
        key: "level",
        group: Group::Combat,
        target: Target::Property(PropertyKind::Int, 25),
        widget: Widget::Whole,
        flows: &[Creature],
    },
    Field {
        key: "attackable",
        group: Group::Combat,
        target: Target::Property(PropertyKind::Bool, 19),
        widget: Widget::YesNo(true),
        flows: &[Creature, Talker, QuestGiver, Vendor],
    },
    Field {
        key: "creature_kind",
        group: Group::Combat,
        target: Target::Property(PropertyKind::Int, 2),
        widget: Widget::CreatureKind,
        flows: &[Creature],
    },
    Field {
        key: "loot",
        group: Group::Combat,
        target: Target::Property(PropertyKind::DataId, 35),
        widget: Widget::Treasure,
        flows: &[Creature],
    },
    Field {
        key: "wields",
        group: Group::Combat,
        target: Target::Holds(2),
        widget: Widget::Text,
        flows: &[Creature],
    },
    Field {
        key: "buys",
        group: Group::Trade,
        target: Target::Property(PropertyKind::Int, 74),
        widget: Widget::Flags(MERCHANDISE),
        flows: &[Vendor],
    },
    Field {
        key: "pays_most",
        group: Group::Trade,
        target: Target::Property(PropertyKind::Int, 76),
        widget: Widget::Whole,
        flows: &[Vendor],
    },
    Field {
        key: "buy_rate",
        group: Group::Trade,
        target: Target::Property(PropertyKind::Float, 37),
        widget: Widget::Decimal,
        flows: &[Vendor],
    },
    Field {
        key: "sell_rate",
        group: Group::Trade,
        target: Target::Property(PropertyKind::Float, 38),
        widget: Widget::Decimal,
        flows: &[Vendor],
    },
    Field {
        key: "sells",
        group: Group::Trade,
        target: Target::Holds(4),
        widget: Widget::Text,
        flows: &[Vendor],
    },
    Field {
        key: "says",
        group: Group::Speech,
        target: Target::Says(emote::USE),
        widget: Widget::Text,
        flows: &[Talker, Vendor],
    },
    Field {
        key: "quest",
        group: Group::Speech,
        target: Target::Quest,
        widget: Widget::Text,
        flows: &[QuestGiver],
    },
    Field {
        key: "asks",
        group: Group::Speech,
        target: Target::Says(emote::QUEST_FAILURE),
        widget: Widget::Text,
        flows: &[QuestGiver],
    },
    Field {
        key: "thanks",
        group: Group::Speech,
        target: Target::Says(emote::QUEST_SUCCESS),
        widget: Widget::Text,
        flows: &[QuestGiver],
    },
];

/// A yes-or-no property's value as the server takes it: as written (`value`), else `unset`, the
/// server's value while the weenie sets none.
#[must_use]
pub fn yes_no(value: Option<&str>, unset: bool) -> bool {
    value.map_or(unset, |v| {
        matches!(content::bare(v).as_str(), "True" | "true" | "1")
    })
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

/// What `doc` says when an emote of `category` runs: its first tell.
#[must_use]
pub fn says_of(doc: &SqlDoc, category: u32) -> Option<String> {
    let quest = (category != emote::USE).then(|| quest_of(doc)).flatten();
    let set = *doc
        .emote_sets()
        .get(set_of(doc, category, quest.as_deref())?)?;
    doc.actions(&set).into_iter().find_map(|a| {
        let t = doc
            .cell(a, "type")
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        (t == Some(emote::TELL))
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
    /// A new weenie of the pack being edited, made for `flow`: a blank of its kind named
    /// `name` (in plain words; its symbol made of them), with the flow's start. The weenie tab
    /// shows it.
    ///
    /// # Errors
    /// No server database to make it from, or a name taken.
    pub fn new_for(&mut self, flow: Flow, name: &str) -> Result<Weenie, String> {
        let words = name.trim();
        if words.is_empty() {
            return Err(self.say("messages.name_it"));
        }
        self.new_weenie(&symbol_of(words), flow.blank(), true)?;
        // The form shows the new weenie, not a selected spawn's.
        self.selection = None;
        self.also.clear();
        let w = self
            .panels
            .weenie
            .clone()
            .ok_or_else(|| self.say("messages.no_weenie_shown"))?;
        let t = self.text.table();
        let mut edits = vec![FormEdit::Property {
            kind: PropertyKind::String,
            id: 1,
            value: Some(words.to_owned()),
        }];
        edits.extend(start(flow, &t));
        self.form_edits(&edits)?;
        // The quest it asks for, registered in the pack.
        if flow == Flow::QuestGiver {
            self.form_edit(&FormEdit::RegisterQuest {
                name: t.get("flows.start_quest").to_owned(),
            })?;
        }
        self.show_weenie();
        Ok(w)
    }

    /// A new weenie of the pack being edited, a copy of server class `class`, named `name` as
    /// players will see it. The weenie tab shows it.
    ///
    /// # Errors
    /// No server database to copy it from, or a name taken.
    pub fn new_copy(&mut self, class: u32, name: &str) -> Result<Weenie, String> {
        let words = name.trim();
        if words.is_empty() {
            return Err(self.say("messages.name_it"));
        }
        self.new_weenie(&symbol_of(words), class, false)?;
        self.selection = None;
        self.also.clear();
        let w = self
            .panels
            .weenie
            .clone()
            .ok_or_else(|| self.say("messages.no_weenie_shown"))?;
        self.form_edit(&FormEdit::Property {
            kind: PropertyKind::String,
            id: 1,
            value: Some(words.to_owned()),
        })?;
        self.show_weenie();
        Ok(w)
    }

    /// Where weenie `w` appears: every spawn of it.
    #[must_use]
    pub fn spawns_of(&self, w: &Weenie) -> Vec<lyceum_foundry::edit::Spawn> {
        self.session
            .spawns()
            .into_iter()
            .filter(|s| self.session.spawn_weenie(s).as_ref() == Some(w))
            .collect()
    }

    /// A spawn of weenie `w` where the camera is: in its cell, else on the ground ahead of it.
    ///
    /// # Errors
    /// The camera is off the world.
    pub fn place_one_here(&mut self, w: &Weenie) -> Result<(), String> {
        let f = self.camera.forward();
        let len = (f.x * f.x + f.y * f.y).sqrt().max(1e-3);
        let p = self.camera.position;
        let ahead = Vec3::new(p.x + f.x / len * 4.0, p.y + f.y / len * 4.0, p.z);
        let op = match self.camera_cell() {
            Some((cell, _)) => {
                let lb = u16::try_from(cell.0 >> 16).unwrap_or(0);
                let o = lyceum_foundry::pick::block_origin(lb);
                let local = Vec3::new(ahead.x - o.x, ahead.y - o.y, ahead.z - o.z);
                let (c, floor) = self
                    .session
                    .cell_at(lb, local)
                    .unwrap_or((u16::try_from(cell.0 & 0xFFFF).unwrap_or(0), local.z));
                Op::PlaceSpawnInCell {
                    landblock: lb,
                    cell: c,
                    weenie: w.clone(),
                    origin: Vec3::new(local.x, local.y, floor),
                    heading: 0.0,
                }
            }
            None => {
                let (lb, x, y) =
                    landblock_of(ahead).ok_or_else(|| self.say("messages.off_world"))?;
                Op::PlaceSpawn {
                    landblock: lb,
                    weenie: w.clone(),
                    origin: Vec3::new(x, y, self.session.ground(lb, x, y).unwrap_or(0.0)),
                    heading: 0.0,
                }
            }
        };
        let applied = self.apply(&op, false)?;
        if let Some(file) = applied.spawned {
            if let Some(s) = self.session.spawns().into_iter().find(|s| s.file == file) {
                self.selection = Some(Selection::Spawn { file, guid: s.guid });
                self.also.clear();
            }
        }
        Ok(())
    }

    /// The weenie's form by what it is for: its fields by group, and where it appears.
    pub(super) fn flow_form(
        &mut self,
        ui: &mut egui::Ui,
        w: &Weenie,
        doc: &SqlDoc,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let flow = Flow::of(doc);
        for group in Group::ALL {
            let fields: Vec<&Field> = flow
                .fields()
                .into_iter()
                .filter(|f| f.group == group)
                .collect();
            if fields.is_empty() {
                continue;
            }
            egui::CollapsingHeader::new(t.get(group.key()))
                .id_salt(("flow group", group.key()))
                .default_open(true)
                .show(ui, |ui| {
                    egui::Grid::new(("flow fields", group.key()))
                        .num_columns(2)
                        .show(ui, |ui| {
                            for f in fields {
                                ui.label(t.get(&format!("fields.{}", f.key)))
                                    .on_hover_text(t.get(&format!("fields.{}_about", f.key)));
                                self.flow_field(ui, f, doc, out);
                                ui.end_row();
                            }
                        });
                });
        }
        self.appears_section(ui, w, out);
    }

    /// One field's value, chosen.
    fn flow_field(&mut self, ui: &mut egui::Ui, f: &Field, doc: &SqlDoc, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        // A property's field is known by the property, as the full form knows it.
        let key = match f.target {
            Target::Property(kind, id) => format!("{}:{id}", kind.key()),
            _ => format!("flow:{}", f.key),
        };
        match (f.target, f.widget) {
            (Target::Property(kind, id), Widget::YesNo(unset)) => {
                let now = yes_no(doc.property(kind, id).as_deref(), unset);
                ui.horizontal(|ui| {
                    for (v, label) in [(true, "fields.yes"), (false, "fields.no")] {
                        if ui.selectable_label(now == v, t.get(label)).clicked() && v != now {
                            out.push(UiAction::Form(FormEdit::Property {
                                kind,
                                id,
                                value: Some(if v { "True" } else { "False" }.into()),
                            }));
                        }
                    }
                });
            }
            (Target::Property(kind, id), Widget::Choice(choices)) => {
                let now = doc
                    .property(kind, id)
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v));
                let shown = choices.iter().find(|(v, _)| Some(*v) == now).map_or_else(
                    || now.map_or_else(String::new, |n| n.to_string()),
                    |(_, k)| t.get(&format!("fields.{k}")).to_owned(),
                );
                egui::ComboBox::from_id_salt(&key)
                    .selected_text(shown)
                    .show_ui(ui, |ui| {
                        for (v, k) in choices {
                            if ui
                                .selectable_label(now == Some(*v), t.get(&format!("fields.{k}")))
                                .clicked()
                            {
                                out.push(UiAction::Form(FormEdit::Property {
                                    kind,
                                    id,
                                    value: Some(v.to_string()),
                                }));
                            }
                        }
                    });
            }
            (Target::Property(kind, id), Widget::Flags(bits)) => {
                let now = doc
                    .property(kind, id)
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                    .unwrap_or(0);
                ui.vertical(|ui| {
                    for (bit, k) in bits {
                        let mut on = now & bit == *bit;
                        if ui
                            .checkbox(&mut on, t.get(&format!("fields.{k}")))
                            .changed()
                        {
                            let v = if on { now | bit } else { now & !bit };
                            out.push(UiAction::Form(FormEdit::Property {
                                kind,
                                id,
                                value: Some(v.to_string()),
                            }));
                        }
                    }
                });
            }
            (Target::Property(kind, id), Widget::Treasure) => {
                let now = doc
                    .property(kind, id)
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v));
                let name = |n: u32| match self.session.treasure(n) {
                    Some(tr) => t.fill(
                        "fields.treasure_choice",
                        &[("n", &n), ("tier", &tr.tier), ("quality", &tr.quality)],
                    ),
                    None => n.to_string(),
                };
                let all: Vec<u32> = self.session.treasures().keys().copied().collect();
                egui::ComboBox::from_id_salt(&key)
                    .selected_text(now.map_or_else(|| t.get("fields.none").to_owned(), name))
                    .height(240.0)
                    .show_ui(ui, |ui| {
                        if ui
                            .selectable_label(now.is_none(), t.get("fields.none"))
                            .clicked()
                        {
                            out.push(UiAction::Form(FormEdit::Property {
                                kind,
                                id,
                                value: None,
                            }));
                        }
                        for n in all {
                            if ui.selectable_label(now == Some(n), name(n)).clicked() {
                                out.push(UiAction::Form(FormEdit::Property {
                                    kind,
                                    id,
                                    value: Some(n.to_string()),
                                }));
                            }
                        }
                    });
            }
            (Target::Property(kind, id), Widget::CreatureKind) => {
                let now = doc
                    .property(kind, id)
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v));
                let kinds: Vec<(u32, String)> = self
                    .schema
                    .as_ref()
                    .and_then(|s| s.enums.get("CreatureType"))
                    .map(|m| m.iter().map(|(k, v)| (*k, v.clone())).collect())
                    .unwrap_or_default();
                if kinds.is_empty() {
                    let now = now.map(|n| n.to_string()).unwrap_or_default();
                    self.field(
                        ui,
                        &key,
                        &now,
                        80.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: Some(v),
                        },
                        out,
                    );
                } else {
                    let shown = kinds
                        .iter()
                        .find(|(k, _)| Some(*k) == now)
                        .map_or_else(String::new, |(_, n)| n.clone());
                    egui::ComboBox::from_id_salt(&key)
                        .selected_text(shown)
                        .height(240.0)
                        .show_ui(ui, |ui| {
                            for (k, n) in &kinds {
                                if ui.selectable_label(now == Some(*k), n).clicked() {
                                    out.push(UiAction::Form(FormEdit::Property {
                                        kind,
                                        id,
                                        value: Some(k.to_string()),
                                    }));
                                }
                            }
                        });
                }
            }
            (Target::Property(kind, id), Widget::DataId) => {
                let now = doc
                    .property(kind, id)
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v))
                    .map(|n| format!("0x{n:08X}"))
                    .unwrap_or_default();
                ui.horizontal(|ui| {
                    self.field(
                        ui,
                        &key,
                        &now,
                        100.0,
                        move |v| FormEdit::Property {
                            kind,
                            id,
                            value: Some(v),
                        },
                        out,
                    );
                });
            }
            (Target::Property(kind, id), _) => {
                let now = doc
                    .property(kind, id)
                    .map(|v| match kind {
                        PropertyKind::String => content::unquote(&v),
                        _ => content::bare(&v),
                    })
                    .unwrap_or_default();
                let width = if kind == PropertyKind::String {
                    200.0
                } else {
                    80.0
                };
                self.field(
                    ui,
                    &key,
                    &now,
                    width,
                    move |v| FormEdit::Property {
                        kind,
                        id,
                        value: Some(v),
                    },
                    out,
                );
            }
            (Target::Says(category), _) => {
                let now = says_of(doc, category).unwrap_or_default();
                self.field(
                    ui,
                    &key,
                    &now,
                    220.0,
                    move |v| FormEdit::Says { category, text: v },
                    out,
                );
            }
            (Target::Quest, _) => {
                let now = quest_of(doc).unwrap_or_default();
                self.field(
                    ui,
                    &key,
                    &now,
                    160.0,
                    |v| FormEdit::QuestGiver { quest: v },
                    out,
                );
            }
            (Target::Destination, _) => {
                self.destination_field(ui, doc, &key, out);
            }
            (Target::Holds(dest), _) => {
                self.holds_field(ui, doc, dest, &key, out);
            }
        }
    }

    /// A portal's destination: shown by place, chosen from the dungeons or set to where the
    /// camera is.
    fn destination_field(
        &mut self,
        ui: &mut egui::Ui,
        doc: &SqlDoc,
        key: &str,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let now = doc
            .rows("weenie_properties_position")
            .into_iter()
            .find(|r| doc.cell(*r, "position_Type").as_deref() == Some("2"))
            .and_then(|r| doc.cell(r, "obj_Cell_Id"))
            .and_then(|v| lyceum_foundry::sql_ids::number(&v));
        let dungeons = self.dungeons();
        let shown = now.map_or_else(
            || t.get("fields.nowhere").to_owned(),
            |c| {
                let lb = u16::try_from(c >> 16).unwrap_or(0);
                dungeons
                    .iter()
                    .find(|d| d.landblock == lb && c & 0xFFFF >= 0x0100)
                    .map_or_else(
                        || format!("0x{c:08X}"),
                        |d| format!("{} (0x{c:08X})", self.dungeon_name(d)),
                    )
            },
        );
        ui.vertical(|ui| {
            ui.label(shown);
            ui.horizontal(|ui| {
                if ui
                    .small_button(t.get("fields.destination_here"))
                    .on_hover_text(t.get("fields.destination_here_about"))
                    .clicked()
                {
                    out.push(UiAction::DestinationHere);
                }
                egui::ComboBox::from_id_salt(key)
                    .selected_text(t.get("fields.destination_dungeon"))
                    .height(240.0)
                    .show_ui(ui, |ui| {
                        for d in &dungeons {
                            if ui.selectable_label(false, self.dungeon_name(d)).clicked() {
                                out.push(UiAction::DestinationDungeon(d.landblock));
                            }
                        }
                    });
            });
        });
    }

    /// What it holds (`dest` 1), wields (2) or sells (4): each by name, taken away by a click;
    /// another added by its class or a pack's weenie's name.
    fn holds_field(
        &mut self,
        ui: &mut egui::Ui,
        doc: &SqlDoc,
        dest: u32,
        key: &str,
        out: &mut Vec<UiAction>,
    ) {
        let t = self.text.table();
        let table = "weenie_properties_create_list";
        let rows = doc.rows(table);
        ui.vertical(|ui| {
            for (n, r) in rows.iter().enumerate() {
                let d = doc
                    .cell(*r, "destination_Type")
                    .and_then(|v| lyceum_foundry::sql_ids::number(&v));
                if d.unwrap_or(0) & dest == 0 {
                    continue;
                }
                let class = doc.cell(*r, "weenie_Class_Id").unwrap_or_default();
                let name = self.class_name(&class).unwrap_or_else(|| class.clone());
                ui.horizontal(|ui| {
                    ui.label(name);
                    if ui
                        .small_button("x")
                        .on_hover_text(t.get("weenie.take_away"))
                        .clicked()
                    {
                        out.push(UiAction::Form(FormEdit::RemoveRow {
                            table: table.into(),
                            row: n,
                        }));
                    }
                });
            }
            let mut typed = self
                .panels
                .form_typing
                .get(key)
                .cloned()
                .unwrap_or_default();
            let r = ui.add(
                egui::TextEdit::singleline(&mut typed)
                    .hint_text(t.get("fields.add_held_hint"))
                    .desired_width(160.0),
            );
            self.panels
                .form_typing
                .insert(key.to_owned(), typed.clone());
            if r.lost_focus()
                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                && !typed.trim().is_empty()
            {
                let class = self.class_named(typed.trim());
                out.push(UiAction::Form(FormEdit::AddRow {
                    table: table.into(),
                    values: vec![
                        ("destination_Type".into(), dest.to_string()),
                        ("weenie_Class_Id".into(), class),
                    ],
                }));
                self.panels.form_typing.remove(key);
            }
        });
    }

    /// A weenie named as typed: a class id as it is, a pack weenie's symbol as its token, else a
    /// server class of that name.
    fn class_named(&self, typed: &str) -> String {
        if lyceum_foundry::sql_ids::number(typed).is_some() {
            return typed.to_owned();
        }
        let lower = typed.to_lowercase();
        for e in &self.weenies {
            if e.name.to_lowercase() == lower {
                return match &e.weenie {
                    Weenie::Class(c) => c.to_string(),
                    Weenie::Symbol(s) => {
                        format!("{{{{wcid:{}}}}}", s.rsplit(':').next().unwrap_or(s))
                    }
                };
            }
        }
        typed.to_owned()
    }

    /// Where the weenie appears: each spawn, flown to by a click; and one placed here.
    fn appears_section(&mut self, ui: &mut egui::Ui, w: &Weenie, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let spawns = self.spawns_of(w);
        egui::CollapsingHeader::new(t.fill("flows.appears", &[("n", &spawns.len())]))
            .id_salt("appears")
            .default_open(true)
            .show(ui, |ui| {
                if spawns.is_empty() {
                    ui.weak(t.get("flows.appears_nowhere"));
                }
                for s in spawns.iter().take(40) {
                    let lb = s.landblock();
                    let town = crate::catalog::town_of(lb).unwrap_or_default();
                    let at = t.fill(
                        "flows.appears_at",
                        &[
                            ("landblock", &format!("{lb:04X}")),
                            ("town", &town),
                            ("cell", &format!("0x{:08X}", s.cell)),
                        ],
                    );
                    if ui.small_button(at).clicked() {
                        out.push(UiAction::GoToSpawn(s.file.clone(), s.guid.clone()));
                    }
                }
                if ui
                    .button(t.get("flows.place_here"))
                    .on_hover_text(t.get("flows.place_here_about"))
                    .clicked()
                {
                    out.push(UiAction::PlaceHere(w.clone()));
                }
            });
    }

    /// New Weenie, folded till opened: a name, a button for each kind, and a copy of the weenie
    /// shown (a server's class).
    pub(super) fn new_flows(&mut self, ui: &mut egui::Ui, out: &mut Vec<UiAction>) {
        let t = self.text.table();
        let copy_of = match self.form_weenie() {
            Some(Weenie::Class(c)) => Some(c),
            _ => None,
        };
        egui::CollapsingHeader::new(t.get("flows.new"))
            .id_salt("new flows")
            .default_open(false)
            .show(ui, |ui| {
                if self.server_sql.is_none() {
                    ui.weak(t.get("flows.new_needs_server"));
                    return;
                }
                let mut name = self
                    .panels
                    .form_typing
                    .get("flow:new")
                    .cloned()
                    .unwrap_or_default();
                ui.add(
                    egui::TextEdit::singleline(&mut name)
                        .hint_text(t.get("flows.new_name_hint"))
                        .desired_width(200.0),
                );
                self.panels
                    .form_typing
                    .insert("flow:new".into(), name.clone());
                ui.horizontal_wrapped(|ui| {
                    for flow in Flow::ALL {
                        if ui
                            .button(t.get(&format!("flows.new_{}", flow.key())))
                            .on_hover_text(t.get(&format!("flows.{}_about", flow.key())))
                            .clicked()
                        {
                            out.push(UiAction::NewFor(flow, name.clone()));
                        }
                    }
                    if ui
                        .add_enabled(
                            copy_of.is_some(),
                            egui::Button::new(t.get("flows.new_copy")),
                        )
                        .clicked()
                    {
                        if let Some(c) = copy_of {
                            out.push(UiAction::NewCopy(c, name.clone()));
                        }
                    }
                });
            });
    }

    /// The camera's place as a portal's destination: its cell and spot.
    ///
    /// # Errors
    /// No weenie shown, or the camera is off the world.
    pub fn destination_here(&mut self) -> Result<(), String> {
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
        self.form_edit(&FormEdit::Destination {
            cell,
            at: [x, y, floor],
        })
    }

    /// Dungeon `lb`'s drop point as a portal's destination.
    ///
    /// # Errors
    /// No weenie shown, or the dungeon has no cell.
    pub fn destination_dungeon(&mut self, lb: u16) -> Result<(), String> {
        let (cell, at) = self.drop_point(lb)?;
        self.form_edit(&FormEdit::Destination {
            cell,
            at: [at.x, at.y, at.z],
        })
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's forms by what a weenie is for)
    use super::*;

    #[test]
    fn every_field_and_choice_has_a_plain_name_and_a_description_in_the_text() {
        let t = crate::text::read(crate::text::BUILT_IN).unwrap();
        let mut missing = Vec::new();
        let mut need = |k: String| {
            if !t.contains_key(&k) {
                missing.push(k);
            }
        };
        for f in FIELDS {
            need(format!("fields.{}", f.key));
            need(format!("fields.{}_about", f.key));
            if let Widget::Choice(c) | Widget::Flags(c) = f.widget {
                for (_, k) in c {
                    need(format!("fields.{k}"));
                }
            }
        }
        for flow in Flow::ALL {
            need(format!("flows.new_{}", flow.key()));
            need(format!("flows.{}_about", flow.key()));
        }
        for g in Group::ALL {
            need(g.key().to_owned());
        }
        assert!(missing.is_empty(), "{missing:?}");
    }

    #[test]
    fn a_yes_or_no_field_the_weenie_leaves_unset_shows_the_value_the_server_takes() {
        let field = |key: &str| FIELDS.iter().find(|f| f.key == key).unwrap();
        // As the server reads them unset: a portal shows where it leads, a creature can be
        // attacked, and a door is neither locked nor locks again.
        for (key, unset) in [
            ("shows_destination", true),
            ("attackable", true),
            ("locked", false),
            ("starts_locked", false),
        ] {
            assert_eq!(field(key).widget, Widget::YesNo(unset), "{key}");
            assert_eq!(yes_no(None, unset), unset, "{key}");
        }
        // Set, the weenie's own: a portal marked to hide where it leads.
        assert!(!yes_no(Some("False"), true));
        assert!(yes_no(Some("1"), false));
        let doc = SqlDoc::parse(PORTAL_BLANK);
        assert!(yes_no(
            doc.property(PropertyKind::Bool, 88).as_deref(),
            true
        ));
    }

    /// A portal as a blank of the server's makes it: no property saying whether it shows where it
    /// leads.
    const PORTAL_BLANK: &str = "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES ({{wcid:gate}}, 'gate', 7, '2005-02-09 10:00:00');\n";

    #[test]
    fn a_quest_giver_asks_after_its_quest_stamps_it_and_answers_both_ways() {
        let mut doc = SqlDoc::parse(
            "INSERT INTO `weenie` (`class_Id`, `class_Name`, `type`, `last_Modified`)\nVALUES ({{wcid:sage}}, 'sage', 10, '2005-02-09 10:00:00');\n",
        );
        set_quest(&mut doc, "a-favor");
        set_says(
            &mut doc,
            emote::QUEST_FAILURE,
            Some("a-favor"),
            "Will you help me?",
        );
        set_says(
            &mut doc,
            emote::QUEST_SUCCESS,
            Some("a-favor"),
            "Thank you.",
        );
        assert_eq!(quest_of(&doc).as_deref(), Some("a-favor"));
        assert_eq!(
            says_of(&doc, emote::QUEST_FAILURE).as_deref(),
            Some("Will you help me?")
        );
        assert_eq!(
            says_of(&doc, emote::QUEST_SUCCESS).as_deref(),
            Some("Thank you.")
        );
        // Asked again, the same sets: nothing doubled.
        set_quest(&mut doc, "a-bigger-favor");
        set_says(
            &mut doc,
            emote::QUEST_SUCCESS,
            Some("a-bigger-favor"),
            "Thanks again.",
        );
        assert_eq!(doc.emote_sets().len(), 3);
        assert_eq!(quest_of(&doc).as_deref(), Some("a-bigger-favor"));
        assert_eq!(
            says_of(&doc, emote::QUEST_SUCCESS).as_deref(),
            Some("Thanks again.")
        );
        assert_eq!(super::super::form::quests_named(&doc), ["a-bigger-favor"]);
    }
}
