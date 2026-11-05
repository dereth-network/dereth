//! A store that keeps everything in memory: for tests, and for a server run that keeps nothing.

use std::collections::{BTreeMap, BTreeSet};

use crate::{
    Account, Ban, Character, Listed, NetworkList, Store, StoreError, StoreResult, WorldState,
};

/// The in-memory store.
#[derive(Debug, Default, Clone)]
pub struct MemoryStore {
    accounts: Vec<Account>,
    characters: Vec<Character>,
    next_character: u64,
    things_reserved: u64,
    worlds: BTreeMap<u64, WorldState>,
    records: BTreeMap<String, Vec<u8>>,
    network: BTreeSet<u64>,
    blocks: BTreeSet<String>,
    lists: Vec<Listed>,
    gags: BTreeMap<u64, f64>,
}

impl MemoryStore {
    fn account_mut(&mut self, account: u64) -> StoreResult<&mut Account> {
        self.accounts
            .iter_mut()
            .find(|a| a.id == account)
            .ok_or_else(|| StoreError(format!("no account {account}")))
    }
}

impl Store for MemoryStore {
    fn account_by_name(&mut self, name: &str) -> StoreResult<Option<Account>> {
        Ok(self
            .accounts
            .iter()
            .find(|a| a.name.eq_ignore_ascii_case(name))
            .cloned())
    }

    fn create_account(
        &mut self,
        name: &str,
        secret: Option<Vec<u8>>,
        now: i64,
    ) -> StoreResult<Account> {
        if self.account_by_name(name)?.is_some() {
            return Err(StoreError(format!("account {name} exists")));
        }
        let a = Account {
            id: self.accounts.len() as u64 + 1,
            name: name.to_string(),
            secret,
            created_at: now,
            admin: false,
            groups: Vec::new(),
            ban: None,
        };
        self.accounts.push(a.clone());
        Ok(a)
    }

    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()> {
        self.account_mut(account)?.secret = Some(secret);
        Ok(())
    }

    fn accounts(&mut self) -> StoreResult<Vec<Account>> {
        let mut v = self.accounts.clone();
        v.sort_by_key(|a| a.name.to_lowercase());
        Ok(v)
    }

    fn set_account_admin(&mut self, account: u64, admin: bool) -> StoreResult<()> {
        self.account_mut(account)?.admin = admin;
        Ok(())
    }

    fn set_account_groups(&mut self, account: u64, groups: &[String]) -> StoreResult<()> {
        self.account_mut(account)?.groups = groups.to_vec();
        Ok(())
    }

    fn set_character_entity(&mut self, character: u64, entity: u32) -> StoreResult<()> {
        let c = self
            .characters
            .iter_mut()
            .find(|c| c.id == character)
            .ok_or_else(|| StoreError(format!("no character {character}")))?;
        c.entity_did = entity;
        Ok(())
    }

    fn rename_character(&mut self, character: u64, name: &str) -> StoreResult<()> {
        if self.name_taken(name)? {
            return Err(StoreError(format!("the name {name} is taken")));
        }
        let c = self
            .characters
            .iter_mut()
            .find(|c| c.id == character)
            .ok_or_else(|| StoreError(format!("no character {character}")))?;
        let old = std::mem::replace(&mut c.name, name.to_owned()).to_lowercase();
        // A list that holds both names holds the new one once.
        let key = name.to_lowercase();
        let renamed: Vec<Listed> = self
            .lists
            .iter()
            .filter(|l| l.entry.to_lowercase() == old)
            .cloned()
            .collect();
        self.lists.retain(|l| {
            l.entry.to_lowercase() != old
                && !(l.entry.to_lowercase() == key
                    && renamed
                        .iter()
                        .any(|r| r.character == l.character && r.list == l.list))
        });
        self.lists.extend(renamed.into_iter().map(|l| Listed {
            entry: name.to_owned(),
            ..l
        }));
        Ok(())
    }

    fn set_account_ban(&mut self, account: u64, ban: Option<&Ban>) -> StoreResult<()> {
        self.account_mut(account)?.ban = ban.cloned();
        Ok(())
    }

    fn gag(&mut self, character: u64) -> StoreResult<Option<f64>> {
        Ok(self.gags.get(&character).copied())
    }

    fn set_gag(&mut self, character: u64, seconds: Option<f64>) -> StoreResult<()> {
        match seconds {
            Some(s) => self.gags.insert(character, s),
            None => self.gags.remove(&character),
        };
        Ok(())
    }

    fn characters(&mut self, account: u64) -> StoreResult<Vec<Character>> {
        let mut v: Vec<Character> = self
            .characters
            .iter()
            .filter(|c| c.account == account)
            .cloned()
            .collect();
        v.sort_by_key(|c| c.last_played);
        Ok(v)
    }

    fn character(&mut self, id: u64) -> StoreResult<Option<Character>> {
        Ok(self.characters.iter().find(|c| c.id == id).cloned())
    }

    fn character_named(&mut self, name: &str) -> StoreResult<Option<Character>> {
        Ok(self
            .characters
            .iter()
            .find(|c| c.name.to_lowercase() == name.to_lowercase())
            .cloned())
    }

    fn name_taken(&mut self, name: &str) -> StoreResult<bool> {
        Ok(self
            .characters
            .iter()
            .any(|c| c.name.to_lowercase() == name.to_lowercase()))
    }

    fn next_character_number(&mut self) -> StoreResult<u64> {
        self.next_character += 1;
        Ok(self.next_character)
    }

    fn reserve_thing_numbers(&mut self, n: u64) -> StoreResult<u64> {
        let first = self.things_reserved + 1;
        self.things_reserved += n;
        Ok(first)
    }

    fn insert_character(&mut self, c: &Character) -> StoreResult<()> {
        if self.characters.iter().any(|x| x.id == c.id) || self.name_taken(&c.name)? {
            return Err(StoreError(format!("character {} exists", c.name)));
        }
        self.characters.push(c.clone());
        Ok(())
    }

    fn insert_character_with_state(
        &mut self,
        c: &Character,
        state: &WorldState,
    ) -> StoreResult<()> {
        self.insert_character(c)?;
        self.worlds.insert(c.id, state.clone());
        Ok(())
    }

    fn set_deleted(&mut self, id: u64, at: Option<i64>) -> StoreResult<()> {
        if let Some(c) = self.characters.iter_mut().find(|c| c.id == id) {
            c.deleted_at = at;
        }
        Ok(())
    }

    fn set_last_played(&mut self, id: u64, at: i64) -> StoreResult<()> {
        if let Some(c) = self.characters.iter_mut().find(|c| c.id == id) {
            c.last_played = at;
        }
        Ok(())
    }

    fn purge_character(&mut self, id: u64) -> StoreResult<()> {
        self.characters.retain(|c| c.id != id);
        self.worlds.remove(&id);
        self.network.remove(&id);
        self.lists.retain(|l| l.character != id);
        self.gags.remove(&id);
        Ok(())
    }

    fn world_state(&mut self, id: u64) -> StoreResult<Option<WorldState>> {
        Ok(self.worlds.get(&id).cloned())
    }

    fn set_world_state(&mut self, id: u64, state: &WorldState) -> StoreResult<()> {
        self.worlds.insert(id, state.clone());
        Ok(())
    }

    fn save_all(
        &mut self,
        states: &[(u64, &WorldState)],
        records: &[(&str, &[u8])],
    ) -> StoreResult<()> {
        for (id, state) in states {
            if self.characters.iter().any(|c| c.id == *id) {
                self.worlds.insert(*id, (*state).clone());
            }
        }
        for (name, value) in records {
            self.records.insert((*name).to_string(), value.to_vec());
        }
        Ok(())
    }

    fn world_record(&mut self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        Ok(self.records.get(name).cloned())
    }

    fn set_world_record(&mut self, name: &str, value: &[u8]) -> StoreResult<()> {
        self.records.insert(name.to_string(), value.to_vec());
        Ok(())
    }

    fn network_characters(&mut self) -> StoreResult<Vec<u64>> {
        Ok(self.network.iter().copied().collect())
    }

    fn set_network_on(&mut self, id: u64, on: bool) -> StoreResult<()> {
        if on {
            self.network.insert(id);
        } else {
            self.network.remove(&id);
        }
        Ok(())
    }

    fn network_blocks(&mut self) -> StoreResult<Vec<String>> {
        Ok(self.blocks.iter().cloned().collect())
    }

    fn set_network_block(&mut self, block: &str, on: bool) -> StoreResult<()> {
        if on {
            self.blocks.insert(block.to_string());
        } else {
            self.blocks.remove(block);
        }
        Ok(())
    }

    fn network_lists(&mut self) -> StoreResult<Vec<Listed>> {
        let mut v = self.lists.clone();
        v.sort();
        Ok(v)
    }

    fn set_listed(
        &mut self,
        character: u64,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()> {
        let same = |l: &Listed| {
            l.character == character
                && l.list == list
                && l.entry.to_lowercase() == entry.to_lowercase()
        };
        if on {
            if !self.lists.iter().any(same) {
                self.lists.push(Listed {
                    character,
                    list,
                    entry: entry.to_owned(),
                });
            }
        } else {
            self.lists.retain(|l| !same(l));
        }
        Ok(())
    }
}
