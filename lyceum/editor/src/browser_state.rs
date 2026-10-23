//! The model browser's recent choices and favorites, kept beside the world.

use std::collections::BTreeSet;
use std::path::Path;

use dereth_primitives::DataId;

/// The shelf shown by the model browser.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum Shelf {
    #[default]
    All,
    Recent,
    Favorites,
}

/// Stable model ids, independent of the browser's current sorting and filters.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct BrowserState {
    pub recent: Vec<u32>,
    pub favorites: BTreeSet<u32>,
}

impl BrowserState {
    /// Read saved shelves; absent files start empty, malformed files are reported.
    ///
    /// # Errors
    /// An unreadable file or invalid saved model ids.
    pub fn read(path: &Path) -> Result<Self, String> {
        let text = match std::fs::read_to_string(path) {
            Ok(t) => t,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Self::default()),
            Err(e) => return Err(e.to_string()),
        };
        let table: toml::Table = text.parse().map_err(|e: toml::de::Error| e.to_string())?;
        let ids = |key: &str| -> Result<Vec<u32>, String> {
            let Some(value) = table.get(key) else {
                return Ok(Vec::new());
            };
            let values = value
                .as_array()
                .ok_or_else(|| format!("{key} must be a list of model ids"))?;
            values
                .iter()
                .map(|v| {
                    v.as_integer()
                        .and_then(|n| u32::try_from(n).ok())
                        .ok_or_else(|| format!("{key} contains an invalid model id"))
                })
                .collect()
        };
        let mut seen = BTreeSet::new();
        let recent = ids("recent")?
            .into_iter()
            .filter(|id| seen.insert(*id))
            .take(24)
            .collect();
        Ok(Self {
            recent,
            favorites: ids("favorites")?.into_iter().collect(),
        })
    }

    /// A choice becomes the newest recent model, without duplicates.
    pub fn choose(&mut self, model: DataId) {
        self.recent.retain(|id| *id != model.raw());
        self.recent.insert(0, model.raw());
        self.recent.truncate(24);
    }

    /// Toggle whether a model is a favorite.
    pub fn toggle_favorite(&mut self, model: DataId) {
        if !self.favorites.remove(&model.raw()) {
            self.favorites.insert(model.raw());
        }
    }

    /// Persist the shelves as creator-readable text.
    ///
    /// # Errors
    /// The shelves cannot be encoded or written.
    pub fn save(&self, path: &Path) -> Result<(), String> {
        let mut table = toml::Table::new();
        for (key, ids) in [
            ("recent", self.recent.clone()),
            ("favorites", self.favorites.iter().copied().collect()),
        ] {
            table.insert(
                key.into(),
                toml::Value::Array(
                    ids.into_iter()
                        .map(|id| toml::Value::Integer(i64::from(id)))
                        .collect(),
                ),
            );
        }
        std::fs::write(path, toml::to_string(&table).map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: saved model browser shelves)
    use super::*;

    #[test]
    fn shelves_keep_choice_order_and_favorites_across_reopening() {
        let dir = dereth_dat::testing::ScratchDir::new("browser-shelves").unwrap();
        let path = dir.path().join("browser.toml");
        let mut state = BrowserState::read(&path).unwrap();
        state.choose(DataId(2));
        state.choose(DataId(1));
        state.choose(DataId(2));
        assert_eq!(
            state.recent,
            [2, 1],
            "the active session has no duplicate choices"
        );
        state.toggle_favorite(DataId(1));
        state.save(&path).unwrap();
        let mut reopened = BrowserState::read(&path).unwrap();
        assert_eq!(reopened.recent, [2, 1]);
        assert_eq!(reopened.favorites, [1].into());
        reopened.toggle_favorite(DataId(1));
        assert!(reopened.favorites.is_empty());
        for id in 3..40 {
            reopened.choose(DataId(id));
        }
        assert_eq!(reopened.recent.len(), 24);
        assert_eq!(reopened.recent[0], 39);
    }

    #[test]
    fn invalid_saved_ids_are_reported_without_overwriting_the_file() {
        let dir = dereth_dat::testing::ScratchDir::new("browser-invalid").unwrap();
        let path = dir.path().join("browser.toml");
        let text = "favorites = [-1]";
        std::fs::write(&path, text).unwrap();
        assert!(BrowserState::read(&path).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), text);
    }
}
