// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Shard/CharacterExtensions.cs
//! `CharacterExtensions` over the database row model [`Character`]: contracts, fill components,
//! friends, quests, shortcuts, spell bars, squelches and titles.
//!
//! DIVERGE (arch, as in empyrean-entity's Biota extensions): the `ReaderWriterLockSlim` parameters are
//! dropped (the world owns its `Character`; the database thread only ever sees an owned snapshot),
//! list getters return copies (ACE's `ToList()` shares the row objects), single lookups return
//! references, and `GetOrCreate*` return `&mut`. Rows removed through ACE's `out` parameters are
//! returned in the `Option`.

use super::{
    Character, CharacterPropertiesContractRegistry, CharacterPropertiesFillCompBook,
    CharacterPropertiesFriendList, CharacterPropertiesQuestRegistry,
    CharacterPropertiesShortcutBar, CharacterPropertiesSpellBar, CharacterPropertiesSquelch,
    CharacterPropertiesTitleBook,
};
use crate::cast::from_index;
use empyrean_common::dotnet::CsCast;

/// `string.Equals(a, b, StringComparison.OrdinalIgnoreCase)`.
fn ordinal_ignore_case_eq(a: &str, b: &str) -> bool {
    // Ordinal case-insensitive comparison upper-cases each UTF-16 unit invariantly.
    a.chars().count() == b.chars().count()
        && a.chars()
            .zip(b.chars())
            .all(|(x, y)| x == y || x.to_uppercase().eq(y.to_uppercase()))
}

// ACE: CharacterExtensions
impl Character {
    // =====================================
    // CharacterPropertiesContract
    // =====================================

    // ACE: CharacterExtensions.GetContracts
    #[must_use]
    pub fn get_contracts(&self) -> Vec<CharacterPropertiesContractRegistry> {
        self.character_properties_contract_registry.clone()
    }

    // ACE: CharacterExtensions.GetContractsCount
    #[must_use]
    pub fn get_contracts_count(&self) -> i32 {
        from_index(self.character_properties_contract_registry.len())
    }

    // ACE: CharacterExtensions.GetContractsIds
    #[must_use]
    pub fn get_contracts_ids(&self) -> Vec<u32> {
        self.character_properties_contract_registry
            .iter()
            .map(|r| r.contract_id)
            .collect()
    }

    // ACE: CharacterExtensions.GetContract
    #[must_use]
    pub fn get_contract(&self, contract_id: u32) -> Option<&CharacterPropertiesContractRegistry> {
        self.character_properties_contract_registry
            .iter()
            .find(|c| c.contract_id == contract_id)
    }

    // ACE: CharacterExtensions.GetOrCreateContract
    /// The contract row and whether it was created. As in ACE, a new row's `character_id` is left 0;
    /// the save writes the owning character's id (Entity Framework's relationship fix-up).
    pub fn get_or_create_contract(
        &mut self,
        contract_id: u32,
    ) -> (&mut CharacterPropertiesContractRegistry, bool) {
        let list = &mut self.character_properties_contract_registry;
        if let Some(i) = list.iter().position(|c| c.contract_id == contract_id) {
            return (&mut list[i], false);
        }
        list.push(CharacterPropertiesContractRegistry {
            contract_id,
            ..Default::default()
        });
        let last = list.len() - 1;
        (&mut list[last], true)
    }

    // ACE: CharacterExtensions.EraseContract
    pub fn erase_contract(
        &mut self,
        contract_id: u32,
    ) -> Option<CharacterPropertiesContractRegistry> {
        let list = &mut self.character_properties_contract_registry;
        let i = list.iter().position(|c| c.contract_id == contract_id)?;
        Some(list.remove(i))
    }

    // ACE: CharacterExtensions.EraseAllContracts
    pub fn erase_all_contracts(&mut self) -> Vec<CharacterPropertiesContractRegistry> {
        std::mem::take(&mut self.character_properties_contract_registry)
    }

    // =====================================
    // CharacterPropertiesFillCompBook
    // =====================================

    // ACE: CharacterExtensions.GetFillComponent
    #[must_use]
    pub fn get_fill_component(&self, wcid: u32) -> Option<&CharacterPropertiesFillCompBook> {
        // C# compares int SpellComponentId with uint wcid by widening both to long.
        self.character_properties_fill_comp_book
            .iter()
            .find(|i| i64::from(i.spell_component_id) == i64::from(wcid))
    }

    // ACE: CharacterExtensions.GetFillComponents
    #[must_use]
    pub fn get_fill_components(&self) -> Vec<CharacterPropertiesFillCompBook> {
        self.character_properties_fill_comp_book.clone()
    }

    // ACE: CharacterExtensions.AddFillComponent
    /// The row and whether it already existed (ACE's `out bool alreadyExists`).
    pub fn add_fill_component(
        &mut self,
        wcid: u32,
        amount: u32,
    ) -> (&mut CharacterPropertiesFillCompBook, bool) {
        let id = self.id;
        let list = &mut self.character_properties_fill_comp_book;
        if let Some(i) = list
            .iter()
            .position(|i| i64::from(i.spell_component_id) == i64::from(wcid))
        {
            return (&mut list[i], true);
        }
        list.push(CharacterPropertiesFillCompBook {
            character_id: id,
            spell_component_id: wcid.cs_cast(),
            quantity_to_rebuy: amount.cs_cast(),
        });
        let last = list.len() - 1;
        (&mut list[last], false)
    }

    // ACE: CharacterExtensions.TryRemoveFillComponent
    pub fn try_remove_fill_component(
        &mut self,
        wcid: u32,
    ) -> Option<CharacterPropertiesFillCompBook> {
        let list = &mut self.character_properties_fill_comp_book;
        let i = list
            .iter()
            .position(|i| i64::from(i.spell_component_id) == i64::from(wcid))?;
        Some(list.remove(i))
    }

    // =====================================
    // CharacterPropertiesFriendList
    // =====================================

    // ACE: CharacterExtensions.GetFriends
    #[must_use]
    pub fn get_friends(&self) -> Vec<CharacterPropertiesFriendList> {
        self.character_properties_friend_list.clone()
    }

    // ACE: CharacterExtensions.HasAsFriend
    #[must_use]
    pub fn has_as_friend(&self, friend_id: u32) -> bool {
        self.character_properties_friend_list
            .iter()
            .any(|record| record.friend_id == friend_id)
    }

    // ACE: CharacterExtensions.AddFriend
    /// The row and whether it already existed (ACE's `out bool friendAlreadyExists`).
    pub fn add_friend(&mut self, friend_id: u32) -> (&mut CharacterPropertiesFriendList, bool) {
        let id = self.id;
        let list = &mut self.character_properties_friend_list;
        if let Some(i) = list.iter().position(|x| x.friend_id == friend_id) {
            return (&mut list[i], true);
        }
        list.push(CharacterPropertiesFriendList {
            character_id: id,
            friend_id,
        });
        let last = list.len() - 1;
        (&mut list[last], false)
    }

    // ACE: CharacterExtensions.TryRemoveFriend
    pub fn try_remove_friend(&mut self, friend_id: u32) -> Option<CharacterPropertiesFriendList> {
        let list = &mut self.character_properties_friend_list;
        let i = list.iter().position(|x| x.friend_id == friend_id)?;
        Some(list.remove(i))
    }

    // ACE: CharacterExtensions.ClearAllFriends
    pub fn clear_all_friends(&mut self) -> bool {
        self.character_properties_friend_list.clear();
        true
    }

    // =====================================
    // CharacterPropertiesQuestRegistry
    // =====================================

    // ACE: CharacterExtensions.GetQuests
    #[must_use]
    pub fn get_quests(&self) -> Vec<CharacterPropertiesQuestRegistry> {
        self.character_properties_quest_registry.clone()
    }

    // ACE: CharacterExtensions.GetQuest
    // ACE-BUG: quest names match ordinal-ignore-case in memory, but the shard's quest_Name key is
    // accent-insensitive (utf8mb4_uca1400_ai_ci), so `Amulet` and `Ämulet` can both be
    // registered in memory and the next character save fails on the primary key.
    #[must_use]
    pub fn get_quest(&self, quest_name: &str) -> Option<&CharacterPropertiesQuestRegistry> {
        self.character_properties_quest_registry
            .iter()
            .find(|q| ordinal_ignore_case_eq(&q.quest_name, quest_name))
    }

    // ACE: CharacterExtensions.GetOrCreateQuest
    /// The quest row and whether it was created. A new row's `character_id` is left 0, as in ACE.
    pub fn get_or_create_quest(
        &mut self,
        quest_name: &str,
    ) -> (&mut CharacterPropertiesQuestRegistry, bool) {
        let list = &mut self.character_properties_quest_registry;
        if let Some(i) = list
            .iter()
            .position(|q| ordinal_ignore_case_eq(&q.quest_name, quest_name))
        {
            return (&mut list[i], false);
        }
        list.push(CharacterPropertiesQuestRegistry {
            quest_name: quest_name.to_owned(),
            ..Default::default()
        });
        let last = list.len() - 1;
        (&mut list[last], true)
    }

    // ACE: CharacterExtensions.EraseQuest
    pub fn erase_quest(&mut self, quest_name: &str) -> bool {
        let list = &mut self.character_properties_quest_registry;
        match list
            .iter()
            .position(|q| ordinal_ignore_case_eq(&q.quest_name, quest_name))
        {
            Some(i) => {
                list.remove(i);
                true
            }
            None => false,
        }
    }

    // ACE: CharacterExtensions.EraseAllQuests
    /// The erased quest names (ACE's `out List<string> questNamesErased`).
    pub fn erase_all_quests(&mut self) -> Vec<String> {
        std::mem::take(&mut self.character_properties_quest_registry)
            .into_iter()
            .map(|r| r.quest_name)
            .collect()
    }

    // =====================================
    // CharacterPropertiesShortcutBar
    // =====================================

    // ACE: CharacterExtensions.GetShortcuts
    #[must_use]
    pub fn get_shortcuts(&self) -> Vec<CharacterPropertiesShortcutBar> {
        self.character_properties_shortcut_bar.clone()
    }

    // ACE: CharacterExtensions.AddOrUpdateShortcut
    pub fn add_or_update_shortcut(&mut self, index: u32, object_id: u32) {
        let id = self.id;
        let bar_index = index.wrapping_add(1);
        let list = &mut self.character_properties_shortcut_bar;
        if let Some(entity) = list.iter_mut().find(|x| x.shortcut_bar_index == bar_index) {
            entity.shortcut_object_id = object_id;
        } else {
            list.push(CharacterPropertiesShortcutBar {
                character_id: id,
                shortcut_object_id: object_id,
                shortcut_bar_index: bar_index,
            });
        }
    }

    // ACE: CharacterExtensions.TryRemoveShortcut
    pub fn try_remove_shortcut(&mut self, index: u32) -> Option<CharacterPropertiesShortcutBar> {
        let bar_index = index.wrapping_add(1);
        let list = &mut self.character_properties_shortcut_bar;
        let i = list
            .iter()
            .position(|x| x.shortcut_bar_index == bar_index)?;
        Some(list.remove(i))
    }

    // =====================================
    // CharacterPropertiesSpellBar
    // =====================================

    // ACE: CharacterExtensions.GetSpellsInBar
    /// The bar's spells ordered by index (a stable sort, as LINQ's `OrderBy`).
    #[must_use]
    pub fn get_spells_in_bar(&self, bar_number: i32) -> Vec<CharacterPropertiesSpellBar> {
        // C# compares uint SpellBarNumber with int (barNumber + 1) by widening both to long.
        let bar = i64::from(bar_number.wrapping_add(1));
        let mut result: Vec<_> = self
            .character_properties_spell_bar
            .iter()
            .filter(|x| i64::from(x.spell_bar_number) == bar)
            .cloned()
            .collect();
        result.sort_by_key(|x| x.spell_bar_index);
        result
    }

    // ACE: CharacterExtensions.AddSpellToBar
    /// Adds `spell` at `index_in_bar`, first moving later spells in the same bar up by one.
    /// False when the spell is already in the bar.
    pub fn add_spell_to_bar(&mut self, bar_number: u32, mut index_in_bar: u32, spell: u32) -> bool {
        let id = self.id;
        let bar = bar_number.wrapping_add(1);
        let list = &mut self.character_properties_spell_bar;
        if list
            .iter()
            .any(|x| x.spell_bar_number == bar && x.spell_id == spell)
        {
            return false;
        }

        let spell_count_in_this_bar: u32 =
            from_index(list.iter().filter(|x| x.spell_bar_number == bar).count());

        if index_in_bar > spell_count_in_this_bar {
            index_in_bar = spell_count_in_this_bar;
        }

        // We must increment the position of existing spells in the bar that exist on or after this position
        for property in list.iter_mut() {
            if property.spell_bar_number == bar
                && property.spell_bar_index >= index_in_bar.wrapping_add(1)
            {
                property.spell_bar_index = property.spell_bar_index.wrapping_add(1);
            }
        }

        list.push(CharacterPropertiesSpellBar {
            character_id: id,
            spell_bar_number: bar,
            spell_bar_index: index_in_bar.wrapping_add(1),
            spell_id: spell,
        });
        true
    }

    // ACE: CharacterExtensions.TryRemoveSpellFromBar
    /// Removes `spell` from the bar and moves later spells in it down by one.
    pub fn try_remove_spell_from_bar(
        &mut self,
        bar_number: u32,
        spell: u32,
    ) -> Option<CharacterPropertiesSpellBar> {
        let bar = bar_number.wrapping_add(1);
        let list = &mut self.character_properties_spell_bar;
        let i = list
            .iter()
            .position(|x| x.spell_bar_number == bar && x.spell_id == spell)?;
        let entity = list.remove(i);

        // We must decrement the position of existing spells in the bar that exist after this position
        for property in list.iter_mut() {
            if property.spell_bar_number == bar && property.spell_bar_index > entity.spell_bar_index
            {
                property.spell_bar_index = property.spell_bar_index.wrapping_sub(1);
            }
        }
        Some(entity)
    }

    // =====================================
    // CharacterPropertiesSquelch
    // =====================================

    // ACE: CharacterExtensions.GetSquelches
    #[must_use]
    pub fn get_squelches(&self) -> Vec<CharacterPropertiesSquelch> {
        self.character_properties_squelch.clone()
    }

    // ACE: CharacterExtensions.AddOrUpdateSquelch
    pub fn add_or_update_squelch(
        &mut self,
        squelch_character_id: u32,
        squelch_account_id: u32,
        r#type: u32,
    ) {
        let id = self.id;
        let list = &mut self.character_properties_squelch;
        if let Some(entity) = list
            .iter_mut()
            .find(|x| x.squelch_character_id == squelch_character_id)
        {
            entity.squelch_account_id = squelch_account_id;
            entity.r#type = r#type;
        } else {
            list.push(CharacterPropertiesSquelch {
                character_id: id,
                squelch_character_id,
                squelch_account_id,
                r#type,
            });
        }
    }

    // ACE: CharacterExtensions.TryRemoveSquelch
    pub fn try_remove_squelch(
        &mut self,
        squelch_character_id: u32,
        squelch_account_id: u32,
    ) -> Option<CharacterPropertiesSquelch> {
        let list = &mut self.character_properties_squelch;
        let i = list.iter().position(|x| {
            x.squelch_character_id == squelch_character_id
                && x.squelch_account_id == squelch_account_id
        })?;
        Some(list.remove(i))
    }

    // =====================================
    // CharacterPropertiesTitleBook
    // =====================================

    // ACE: CharacterExtensions.GetTitles
    #[must_use]
    pub fn get_titles(&self) -> Vec<CharacterPropertiesTitleBook> {
        self.character_properties_title_book.clone()
    }

    // ACE: CharacterExtensions.AddTitleToRegistry
    /// `(titleAlreadyExists, numCharacterTitles)`.
    pub fn add_title_to_registry(&mut self, title: u32) -> (bool, i32) {
        let id = self.id;
        let list = &mut self.character_properties_title_book;
        if list.iter().any(|x| x.title_id == title) {
            return (true, from_index(list.len()));
        }
        list.push(CharacterPropertiesTitleBook {
            character_id: id,
            title_id: title,
        });
        (false, from_index(list.len()))
    }
}
