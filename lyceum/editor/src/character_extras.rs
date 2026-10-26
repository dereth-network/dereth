//! What the Characters tab offers beyond a character's build, worked out from the world: what
//! each augmentation's gem costs there, the titles the data files name, the quests the world
//! registers, and the quests the place round the camera checks.
//!
//! The costs are the world's own: the gems as the world database has them, or as a pack writes
//! them again. Until the world database is read they are not known, and nothing that costs
//! experience can be priced.

use std::collections::BTreeMap;

use dereth_primitives::DataId;
use dereth_progression::augmentations::Costs;
use dereth_progression::Finding;
use lyceum_foundry::edit::Session;

/// The mapper from a title's id to the token its words are found by.
const TITLE_TOKENS: DataId = DataId(0x2200_0041);

/// The words of each title, by the hash of its token.
const TITLE_WORDS: DataId = DataId(0x2300_000E);

/// What the tab offers, as last worked out.
#[derive(Debug, Default)]
pub struct Offered {
    /// What each augmentation costs in the world: `None` until the world database is read.
    pub costs: Option<Costs>,
    /// How many of the server's weenies the costs were worked out from.
    pub costs_from: usize,
    /// What the extras' own checks found of the selected recipe's build.
    pub found: Vec<Finding>,
    /// The titles the data files name, by id, in the order of their words; `None` before they are
    /// read.
    pub titles: Option<Vec<(u32, String)>>,
    /// The quests the world registers, and how many of the server's weenies were known then.
    pub quests: Option<(usize, Vec<String>)>,
    /// The quests the place checks, each with what checks it, as gathered with its creatures.
    pub place: Option<Vec<(String, String)>>,
    /// What is typed: the search for a title, the name of a quest.
    pub title_search: String,
    pub quest_name: String,
    /// The most times each quest the world registers can be solved (0 for no most), by its key,
    /// and what it was worked out from: how many of the server's weenies were known, and the
    /// edits' history.
    pub limits: Option<(WorldKey, BTreeMap<String, i32>)>,
    /// What the checks of the titles and quests found, and the recipe and how many of the
    /// server's weenies they were worked out from.
    pub checked: Option<(crate::characters::Recipe, usize, Vec<Finding>)>,
    /// The archetypes offered, by id and name, and whether the world's tables were read then.
    pub archetypes: Option<(bool, Vec<(String, String)>)>,
    /// The Build tab's skill rows, and the recipe and augmentations' costs they were worked out
    /// from.
    pub skill_rows: Option<(
        crate::characters::Recipe,
        Option<Costs>,
        Vec<crate::characters::SkillRow>,
    )>,
}

/// The most times each quest the session's world registers can be solved (0 for no most), by
/// its key: a pack's (the last pack's first) over the server's own, as the server takes it.
#[must_use]
pub fn quest_limits(session: &Session) -> BTreeMap<String, i32> {
    let mut out = BTreeMap::new();
    for p in session.packs().iter().rev() {
        for f in &p.sql {
            for (name, q) in lyceum_foundry::sql_ids::quests_in(&f.text) {
                out.entry(lyceum_foundry::content_check::quest_key(&name))
                    .or_insert(q.max_solves);
            }
        }
    }
    for (name, q) in session.server_quests() {
        out.entry(lyceum_foundry::content_check::quest_key(name))
            .or_insert(q.max_solves);
    }
    out
}

/// What a world's quests are read from: how many of the server's weenies are known, and the
/// edits' history.
pub type WorldKey = (usize, (usize, usize));

/// What each augmentation's gem costs in the session's world: a pack's copy of the gem, else
/// the server's own; `None` before the server's weenies are known.
#[must_use]
pub fn costs_of(session: &Session) -> Option<Costs> {
    let server = session.server_weenies();
    if server.is_empty() {
        return None;
    }
    let packs = session.pack_classes();
    Some(Costs::of(|_, class| {
        packs
            .get(&class)
            .or_else(|| server.get(&class))?
            .augmentation_cost
            .and_then(|c| u64::try_from(c).ok())
    }))
}

/// The titles the data files `dats` name, by id, each with its words, in the order of their
/// words; none when the files have no titles.
#[must_use]
pub fn titles_in(dats: &lyceum_foundry::dats::DatSet) -> Vec<(u32, String)> {
    let tokens: Option<dereth_assets::tables::EnumMapper> = dats
        .read_portal(TITLE_TOKENS)
        .and_then(|b| dats.decode(TITLE_TOKENS, &b).ok());
    let words: Option<dereth_assets::ui::StringTable> = dats
        .file(dereth_dat::ModernDat::Local)
        .and_then(|f| f.read(TITLE_WORDS).ok())
        .and_then(|b| dats.decode(TITLE_WORDS, &b).ok());
    let (Some(tokens), Some(words)) = (tokens, words) else {
        return Vec::new();
    };
    let by_hash: BTreeMap<u32, String> = words
        .strings
        .iter()
        .filter_map(|(h, e)| Some((*h, e.strings.first()?.clone())))
        .collect();
    let mut out: Vec<(u32, String)> = tokens
        .id_to_string
        .iter()
        .filter(|(id, _)| *id != 0)
        .filter_map(|(id, token)| {
            let hash = dereth_primitives::num::hash::str_hash(token.as_bytes());
            let words = by_hash.get(&hash)?.trim();
            (!words.is_empty()).then(|| (*id, words.to_owned()))
        })
        .collect();
    out.sort_by(|a, b| {
        a.1.to_lowercase()
            .cmp(&b.1.to_lowercase())
            .then(a.0.cmp(&b.0))
    });
    out
}

/// A large count of experience as a builder reads it: `4 billion`, `500 million`, `2.5 billion`.
#[must_use]
pub fn experience_words(xp: u64) -> String {
    const BILLION: u64 = 1_000_000_000;
    const MILLION: u64 = 1_000_000;
    let tenths = |n: u64, unit: u64| {
        let t = (n * 10 + unit / 2) / unit;
        if t.is_multiple_of(10) {
            format!("{}", t / 10)
        } else {
            format!("{}.{}", t / 10, t % 10)
        }
    };
    if xp >= BILLION {
        format!("{} billion", tenths(xp, BILLION))
    } else if xp >= MILLION {
        format!("{} million", tenths(xp, MILLION))
    } else {
        xp.to_string()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the Characters tab's augmentation costs and words)
    use super::*;

    #[test]
    fn experience_reads_in_billions_and_millions_to_a_tenth() {
        assert_eq!(experience_words(4_000_000_000), "4 billion");
        assert_eq!(experience_words(500_000_000), "500 million");
        assert_eq!(experience_words(2_500_000_000), "2.5 billion");
        assert_eq!(experience_words(53_000_000_000), "53 billion");
        assert_eq!(experience_words(1_234), "1234");
    }

    #[test]
    fn a_recipes_augmentations_auras_titles_and_quests_round_trip_and_reach_its_order() {
        use crate::characters::Recipe;
        use dereth_progression::extras::Luminous;
        let r = Recipe {
            level: 240,
            augmentations: BTreeMap::from([("quick-learner".to_owned(), 1)]),
            luminance: Some(Luminous {
                auras: BTreeMap::from([("aura-of-valor".to_owned(), 7)]),
                seer: Some("liam-of-gelid".into()),
            }),
            titles: vec![12, 3],
            quests: BTreeMap::from([("Portal Flag@Done".to_owned(), 2)]),
            quest_credits: true,
            ..Recipe::default()
        };
        let text = r.text().unwrap();
        assert!(text.contains("[augmentations]"), "{text}");
        assert!(text.contains("[luminance.auras]"), "{text}");
        assert_eq!(toml::from_str::<Recipe>(&text).unwrap(), r);
        assert_ne!(
            r.hash(),
            Recipe {
                level: 240,
                ..Recipe::default()
            }
            .hash()
        );
        let o = r.order(None, |_| None).unwrap();
        assert_eq!(o.extras, r.extras());
        assert_eq!(o.extras.titles, [12, 3], "the one shown first");
        // A recipe with none of it writes none of it, and its hash is as before.
        let plain = Recipe::default().text().unwrap();
        for word in ["augmentations", "luminance", "titles", "quests"] {
            assert!(!plain.contains(word), "{plain}");
        }
    }
}
