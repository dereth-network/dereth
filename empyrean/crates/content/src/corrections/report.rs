//! Not ACE: what this build's corrections do to one world database, for `empyrean-import
//! --corrections` and the server's start-up log.
//!
//! Each weenie and spell entry either applies (the content still stores the value it names),
//! is stale (the content stores something else: an upstream fix, or an edit), is absent (the
//! content has no such weenie or spell), or is for another era (the content was built for an era
//! the entries are not scoped to, [`super::ENTRY_ERAS`]). Each rule lists the values it changes. Everything is in
//! class id (spell id) order, so two runs over the same content print the same report.

use std::collections::BTreeMap;

use empyrean_entity::enums::{MotionCommand, PlayScript};

use super::{
    current, digest, emote_motion_shifts, entries_apply_to, play_script_shifts, spell_holds,
    EmoteMotionField, EmoteMotionShift, PlayScriptShift, SpellCorrection, SpellValue, Value,
    WeenieCorrection, DIGEST_FORMAT, EMOTE_MOTION_SHIFT_DIVERGENCE, EMOTE_MOTION_SHIFT_EVIDENCE,
    PLAY_SCRIPT_SHIFT_DIVERGENCE, PLAY_SCRIPT_SHIFT_EVIDENCE, SPELL_CORRECTIONS,
    WEENIE_CORRECTIONS,
};
use crate::models::world::Spell;
use crate::pack::TableId;
use crate::world_database::WorldDatabaseBase;

/// Whether an entry applies to the content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntryState {
    /// The content stores the value the entry names; the server reads the corrected one.
    Applies,
    /// The content stores another value (shown); the entry does nothing.
    Stale(String),
    /// The content has no such weenie or spell.
    Absent,
    /// The content was built for an era the entries do not apply to.
    OtherEra,
}

impl EntryState {
    #[must_use]
    pub fn label(&self) -> &'static str {
        match self {
            Self::Applies => "applies",
            Self::Stale(_) => "stale",
            Self::Absent => "absent",
            Self::OtherEra => "other era",
        }
    }
}

/// One weenie entry and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WeenieEntry {
    pub correction: &'static WeenieCorrection,
    pub state: EntryState,
}

/// One spell entry and its state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpellEntry {
    pub correction: &'static SpellCorrection,
    pub state: EntryState,
}

/// What the corrections do to one world database.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CorrectionsReport {
    /// This build's corrections digest.
    pub digest: String,
    /// The era the content was built for.
    pub era: empyrean_common::era::EraId,
    pub weenie_entries: Vec<WeenieEntry>,
    pub spell_entries: Vec<SpellEntry>,
    pub play_script_shifts: Vec<PlayScriptShift>,
    pub emote_motion_shifts: Vec<EmoteMotionShift>,
    /// A weenie's class name and `Name`, for every class id the report mentions.
    pub names: BTreeMap<u32, String>,
}

/// Counts: entries that apply, are stale or absent (weenie and spell entries together), and the
/// values each rule changes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Summary {
    pub entries: usize,
    pub applies: usize,
    pub stale: usize,
    pub absent: usize,
    /// Entries for another era than the content's.
    pub other_era: usize,
    pub play_script_shifts: usize,
    pub emote_motion_shifts: usize,
}

impl CorrectionsReport {
    /// The report over `db` as stored (its overlay included).
    #[must_use]
    pub fn of(db: &WorldDatabaseBase) -> Self {
        let era = db.pack().era();
        let in_scope = entries_apply_to(era);
        let weenie_entries = WEENIE_CORRECTIONS
            .iter()
            .map(|c| {
                let state = match db.get_stored_weenie(c.weenie_class_id) {
                    _ if !in_scope => EntryState::OtherEra,
                    None => EntryState::Absent,
                    Some(w) => match current(&w, c.stored) {
                        Some(v) if v == c.stored => EntryState::Applies,
                        Some(v) => EntryState::Stale(value_text(v)),
                        None => EntryState::Stale("no such emote action".to_owned()),
                    },
                };
                WeenieEntry {
                    correction: c,
                    state,
                }
            })
            .collect();
        let spell_entries = SPELL_CORRECTIONS
            .iter()
            .map(|c| {
                let state = match db.get::<Spell>(TableId::SPELL, u64::from(c.spell_id)) {
                    _ if !in_scope => EntryState::OtherEra,
                    None => EntryState::Absent,
                    Some(s) if spell_holds(&s, c.stored) => EntryState::Applies,
                    Some(s) => EntryState::Stale(spell_value_text(SpellValue::Wcid(s.wcid))),
                };
                SpellEntry {
                    correction: c,
                    state,
                }
            })
            .collect();
        let play_script_shifts = play_script_shifts(db);
        let emote_motion_shifts = emote_motion_shifts(db);
        let mut wanted: Vec<u32> = WEENIE_CORRECTIONS
            .iter()
            .map(|c| c.weenie_class_id)
            .collect();
        wanted.extend(play_script_shifts.iter().map(|c| c.weenie_class_id));
        wanted.extend(emote_motion_shifts.iter().map(|c| c.weenie_class_id));
        wanted.sort_unstable();
        wanted.dedup();
        let names = db
            .weenie_index()
            .into_iter()
            .filter(|(k, _)| wanted.binary_search(k).is_ok())
            .map(|(k, i)| (k, weenie_label(&i.class_name, i.name.as_deref())))
            .collect();
        Self {
            digest: digest().to_owned(),
            era,
            weenie_entries,
            spell_entries,
            play_script_shifts,
            emote_motion_shifts,
            names,
        }
    }

    #[must_use]
    pub fn summary(&self) -> Summary {
        let states = self
            .weenie_entries
            .iter()
            .map(|e| &e.state)
            .chain(self.spell_entries.iter().map(|e| &e.state));
        let mut s = Summary {
            play_script_shifts: self.play_script_shifts.len(),
            emote_motion_shifts: self.emote_motion_shifts.len(),
            ..Summary::default()
        };
        for state in states {
            s.entries += 1;
            match state {
                EntryState::Applies => s.applies += 1,
                EntryState::Stale(_) => s.stale += 1,
                EntryState::Absent => s.absent += 1,
                EntryState::OtherEra => s.other_era += 1,
            }
        }
        s
    }

    fn name(&self, weenie_class_id: u32) -> String {
        match self.names.get(&weenie_class_id) {
            Some(n) => format!("{weenie_class_id} {n}"),
            None => weenie_class_id.to_string(),
        }
    }

    /// The report as text: a summary, then every entry and every value each rule changes.
    #[must_use]
    pub fn render(&self) -> String {
        let s = self.summary();
        let mut out = format!("corrections {} ({DIGEST_FORMAT})\n", self.digest);
        out += &format!(
            "entries: {} apply, {} stale, {} absent (of {}); rules: {} default scripts, {} emote motions\n",
            s.applies, s.stale, s.absent, s.entries, s.play_script_shifts, s.emote_motion_shifts
        );
        if s.other_era > 0 {
            out += &format!(
                "the content is for era {}: the {} entries are for other eras and none applies\n",
                self.era, s.other_era
            );
        }
        out += &format!("\nweenie entries ({}):\n", self.weenie_entries.len());
        for e in &self.weenie_entries {
            let c = e.correction;
            out += &format!(
                "  {:<7} {}: {} {} -> {} [{}; {}]\n",
                e.state.label(),
                self.name(c.weenie_class_id),
                property_text(c.stored),
                value_text(c.stored),
                value_text(c.corrected),
                c.divergence,
                c.evidence
            );
            if let EntryState::Stale(found) = &e.state {
                out += &format!("          stores {found}\n");
            }
        }
        out += &format!("\nspell entries ({}):\n", self.spell_entries.len());
        for e in &self.spell_entries {
            let c = e.correction;
            out += &format!(
                "  {:<7} spell {}: {} -> {} [{}; {}]\n",
                e.state.label(),
                c.spell_id,
                spell_value_text(c.stored),
                spell_value_text(c.corrected),
                c.divergence,
                c.evidence
            );
            if let EntryState::Stale(found) = &e.state {
                out += &format!("          stores {found}\n");
            }
        }
        out += &format!(
            "\nrule play_script_shift ({} default scripts) [{PLAY_SCRIPT_SHIFT_DIVERGENCE}; {PLAY_SCRIPT_SHIFT_EVIDENCE}]:\n",
            self.play_script_shifts.len()
        );
        for c in &self.play_script_shifts {
            out += &format!(
                "  {}: PhysicsScript {} -> {}\n",
                self.name(c.weenie_class_id),
                script_text(c.stored),
                script_text(c.corrected)
            );
        }
        out += &format!(
            "\nrule emote_motion_shift ({} values) [{EMOTE_MOTION_SHIFT_DIVERGENCE}; {EMOTE_MOTION_SHIFT_EVIDENCE}]:\n",
            self.emote_motion_shifts.len()
        );
        for c in &self.emote_motion_shifts {
            out += &format!(
                "  {}: emote set {} {} {} -> {}\n",
                self.name(c.weenie_class_id),
                c.emote_set,
                field_text(c.field),
                motion_text(c.stored),
                motion_text(c.corrected)
            );
        }
        let skipped =
            |state: &EntryState| matches!(state, EntryState::Applies | EntryState::OtherEra);
        let stale: Vec<String> = self
            .weenie_entries
            .iter()
            .filter(|e| !skipped(&e.state))
            .map(|e| {
                format!(
                    "{} ({})",
                    self.name(e.correction.weenie_class_id),
                    e.state.label()
                )
            })
            .chain(
                self.spell_entries
                    .iter()
                    .filter(|e| !skipped(&e.state))
                    .map(|e| format!("spell {} ({})", e.correction.spell_id, e.state.label())),
            )
            .collect();
        if stale.is_empty() && s.other_era > 0 {
            out += "\nno entry is for this era\n";
        } else if stale.is_empty() {
            out += "\nevery entry applies\n";
        } else {
            out += &format!("\n{} entries do not apply:\n", stale.len());
            for s in stale {
                out += &format!("  {s}\n");
            }
        }
        out
    }

    /// The report as JSON.
    #[must_use]
    pub fn to_json(&self) -> serde_json::Value {
        use serde_json::json;
        let s = self.summary();
        let found = |state: &EntryState| match state {
            EntryState::Stale(f) => serde_json::Value::String(f.clone()),
            _ => serde_json::Value::Null,
        };
        json!({
            "format": "empyrean corrections report v1",
            "digest": self.digest,
            "digest_format": DIGEST_FORMAT,
            "era": self.era.name(),
            "summary": {
                "entries": s.entries,
                "applies": s.applies,
                "stale": s.stale,
                "absent": s.absent,
                "other_era": s.other_era,
                "play_script_shifts": s.play_script_shifts,
                "emote_motion_shifts": s.emote_motion_shifts,
            },
            "weenie_entries": self.weenie_entries.iter().map(|e| {
                let c = e.correction;
                json!({
                    "weenie_class_id": c.weenie_class_id,
                    "name": self.names.get(&c.weenie_class_id),
                    "property": property_text(c.stored),
                    "stored": value_text(c.stored),
                    "corrected": value_text(c.corrected),
                    "state": e.state.label(),
                    "found": found(&e.state),
                    "divergence": c.divergence,
                    "evidence": c.evidence,
                })
            }).collect::<Vec<_>>(),
            "spell_entries": self.spell_entries.iter().map(|e| {
                let c = e.correction;
                json!({
                    "spell_id": c.spell_id,
                    "field": "wcid",
                    "stored": spell_value_text(c.stored),
                    "corrected": spell_value_text(c.corrected),
                    "state": e.state.label(),
                    "found": found(&e.state),
                    "divergence": c.divergence,
                    "evidence": c.evidence,
                })
            }).collect::<Vec<_>>(),
            "rules": [
                {
                    "rule": "play_script_shift",
                    "divergence": PLAY_SCRIPT_SHIFT_DIVERGENCE,
                    "evidence": PLAY_SCRIPT_SHIFT_EVIDENCE,
                    "changes": self.play_script_shifts.iter().map(|c| json!({
                        "weenie_class_id": c.weenie_class_id,
                        "name": self.names.get(&c.weenie_class_id),
                        "property": "PropertyDataId.PhysicsScript",
                        "stored": c.stored,
                        "corrected": c.corrected,
                    })).collect::<Vec<_>>(),
                },
                {
                    "rule": "emote_motion_shift",
                    "divergence": EMOTE_MOTION_SHIFT_DIVERGENCE,
                    "evidence": EMOTE_MOTION_SHIFT_EVIDENCE,
                    "changes": self.emote_motion_shifts.iter().map(|c| json!({
                        "weenie_class_id": c.weenie_class_id,
                        "name": self.names.get(&c.weenie_class_id),
                        "emote_set": c.emote_set,
                        "field": field_text(c.field),
                        "stored": c.stored,
                        "corrected": c.corrected,
                    })).collect::<Vec<_>>(),
                },
            ],
        })
    }
}

/// `class_name (Name)`, or the class name alone.
#[must_use]
pub fn weenie_label(class_name: &str, name: Option<&str>) -> String {
    match name {
        Some(n) if !n.is_empty() => format!("{class_name} ({n})"),
        _ => class_name.to_owned(),
    }
}

/// The property an entry names, as a person reads it.
#[must_use]
pub fn property_text(v: Value) -> String {
    fn named(kind: &str, name: Option<&str>, n: u16) -> String {
        name.map_or_else(|| format!("{kind}.{n}"), |name| format!("{kind}.{name}"))
    }
    match v {
        Value::Int(p, _) => named("PropertyInt", p.name(), p.0),
        Value::Bool(p, _) => named("PropertyBool", p.name(), p.0),
        Value::DataId(p, _) => named("PropertyDataId", p.name(), p.0),
        Value::EmoteScript(a, _) => format!("emote set {} action {} script", a.emote_set, a.order),
    }
}

/// A value an entry names, as a person reads it (`none` is no row).
#[must_use]
pub fn value_text(v: Value) -> String {
    match v {
        Value::Int(_, Some(x)) => format!("{x} (0x{x:X})"),
        Value::Bool(_, Some(x)) => x.to_string(),
        Value::DataId(_, Some(x)) => format!("0x{x:08X}"),
        #[allow(clippy::cast_sign_loss)]
        Value::EmoteScript(_, Some(x)) => script_text(x as u32),
        Value::Int(_, None)
        | Value::Bool(_, None)
        | Value::DataId(_, None)
        | Value::EmoteScript(_, None) => "none".to_owned(),
    }
}

fn spell_value_text(v: SpellValue) -> String {
    match v {
        SpellValue::Wcid(Some(x)) => format!("wcid {x}"),
        SpellValue::Wcid(None) => "wcid none".to_owned(),
    }
}

fn script_text(x: u32) -> String {
    match PlayScript(x).name() {
        Some(n) => format!("{x} ({n})"),
        None => x.to_string(),
    }
}

fn motion_text(x: u32) -> String {
    match MotionCommand(x).name() {
        Some(n) => format!("0x{x:08X} ({n})"),
        None => format!("0x{x:08X}"),
    }
}

fn field_text(f: EmoteMotionField) -> String {
    match f {
        EmoteMotionField::Style => "style".to_owned(),
        EmoteMotionField::Substyle => "substyle".to_owned(),
        EmoteMotionField::Motion(order) => format!("action {order} motion"),
    }
}
