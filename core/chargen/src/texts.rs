//! The creation texts a world's own files carry.
//!
//! The character-generation table from before Throne of Destiny names its texts as records of their
//! own: each heritage's description, each profession's description for each sex, the naming help of
//! each sex and the help of each page of the wizard. The later table names none; its interface's
//! string tables hold the texts instead. [`CreationTexts::read`] reads every record the table names,
//! so either creation interface can show the world's own words where the world has them.

use std::collections::BTreeMap;

use dereth_assets::tables::CharGen;
use dereth_primitives::{AssetSource, DataId};

/// The texts the table names, by record id. Empty for a table that names none.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CreationTexts(BTreeMap<DataId, String>);

impl CreationTexts {
    /// Every text record the table names that the files hold and that decodes.
    #[must_use]
    pub fn read(assets: &dyn AssetSource, chargen: &CharGen) -> Self {
        let mut out = BTreeMap::new();
        for id in Self::named(chargen) {
            if out.contains_key(&id) {
                continue;
            }
            if let Some(text) = assets.read(id).ok().and_then(|b| decode(id, &b)) {
                out.insert(id, text);
            }
        }
        Self(out)
    }

    /// The text of record `id`, when the world's files carry it.
    #[must_use]
    pub fn get(&self, id: Option<DataId>) -> Option<&str> {
        self.0.get(&id?).map(String::as_str)
    }

    /// Whether the world names no text of its own.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// A text set from records already read, for a world built in memory.
    #[must_use]
    pub fn from_records(records: impl IntoIterator<Item = (DataId, String)>) -> Self {
        Self(records.into_iter().collect())
    }

    /// Every text record id the table names: the page help, then each heritage's description,
    /// each sex's naming help and each profession's description for each sex.
    fn named(chargen: &CharGen) -> impl Iterator<Item = DataId> + '_ {
        let heritages = chargen.heritage_groups.values();
        chargen
            .help_strings
            .iter()
            .copied()
            .chain(heritages.clone().filter_map(|h| h.description))
            .chain(
                heritages
                    .clone()
                    .flat_map(|h| h.sexes.values())
                    .filter_map(|s| s.naming_help),
            )
            .chain(
                heritages
                    .flat_map(|h| h.template_presentations.values().flatten())
                    .filter_map(|p| p.description),
            )
            .filter(|id| id.0 != 0)
    }
}

/// A text record: its own id, then the text, a 16-bit length (`0xFFFF` and a 32-bit one for a
/// long text) and that many Windows-1252 bytes.
#[must_use]
pub fn decode(id: DataId, bytes: &[u8]) -> Option<String> {
    let word = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };
    if word(0)? != id.0 {
        return None;
    }
    let short = u16::from_le_bytes(bytes.get(4..6)?.try_into().ok()?);
    let (len, start): (usize, usize) = if short == 0xFFFF {
        (usize::try_from(word(6)?).ok()?, 10)
    } else {
        (usize::from(short), 6)
    };
    let text = bytes.get(start..start.checked_add(len)?)?;
    Some(dereth_primitives::text::cp1252::decode(text))
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a record decoder; the dat test reads the February 2005 texts).
    use super::*;

    #[test]
    fn a_text_record_decodes_its_short_and_long_forms_and_refuses_another_id() {
        let mut short = 0x3100_0001u32.to_le_bytes().to_vec();
        short.extend_from_slice(&5u16.to_le_bytes());
        short.extend_from_slice(b"Hello\0\0\0");
        assert_eq!(
            decode(DataId(0x3100_0001), &short).as_deref(),
            Some("Hello")
        );
        assert_eq!(decode(DataId(0x3100_0002), &short), None);
        let mut long = 0x3100_0003u32.to_le_bytes().to_vec();
        long.extend_from_slice(&0xFFFFu16.to_le_bytes());
        long.extend_from_slice(&3u32.to_le_bytes());
        long.extend_from_slice(&[b'A', 0xE9, b'B']);
        assert_eq!(
            decode(DataId(0x3100_0003), &long).as_deref(),
            Some("A\u{e9}B")
        );
        assert_eq!(decode(DataId(0x3100_0003), &long[..9]), None);
    }
}
