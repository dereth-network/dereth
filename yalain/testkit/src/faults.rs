//! A store that fails on demand: for what the world does when its database cannot be read or
//! written (a full disk, a read-only file, a locked database).

use yalain_store::{
    Account, Ban, Character, Listed, MemoryStore, NetworkList, Store, StoreError, StoreResult,
    WorldState,
};

/// A [`MemoryStore`] whose writes, or reads, fail while the test says they do.
#[derive(Debug, Default)]
pub struct FaultStore {
    /// What it holds.
    pub inner: MemoryStore,
    /// Every write fails.
    pub writes_fail: bool,
    /// Every read fails.
    pub reads_fail: bool,
    /// Reads of a character's world state fail.
    pub state_reads_fail: bool,
    /// Reads of a character's world state fail once this many more have gone through.
    pub state_reads_after: Option<u32>,
    /// Reads of the world's records fail.
    pub records_read_fail: bool,
    /// Writes of world state that failed.
    pub failed_writes: u32,
    /// Noting when this character last played panics: it comes as its leaving ends, after its state
    /// is kept, so it stands for any bug partway through a piece of the world's work.
    pub last_played_panics: Option<u64>,
    /// Asking whether a name is taken panics: a bug in one event, whatever came before it in its pass.
    pub name_taken_panics: bool,
}

impl FaultStore {
    fn write(&mut self) -> StoreResult<()> {
        if self.writes_fail {
            self.failed_writes += 1;
            return Err(StoreError("database or disk is full".into()));
        }
        Ok(())
    }

    fn read(&self) -> StoreResult<()> {
        if self.reads_fail {
            return Err(StoreError("database is locked".into()));
        }
        Ok(())
    }
}

impl Store for FaultStore {
    fn account_by_name(&mut self, name: &str) -> StoreResult<Option<Account>> {
        self.read()?;
        self.inner.account_by_name(name)
    }

    fn create_account(
        &mut self,
        name: &str,
        secret: Option<Vec<u8>>,
        now: i64,
    ) -> StoreResult<Account> {
        self.write()?;
        self.inner.create_account(name, secret, now)
    }

    fn set_account_secret(&mut self, account: u64, secret: Vec<u8>) -> StoreResult<()> {
        self.write()?;
        self.inner.set_account_secret(account, secret)
    }

    fn accounts(&mut self) -> StoreResult<Vec<Account>> {
        self.read()?;
        self.inner.accounts()
    }

    fn set_account_admin(&mut self, account: u64, admin: bool) -> StoreResult<()> {
        self.write()?;
        self.inner.set_account_admin(account, admin)
    }

    fn set_account_groups(&mut self, account: u64, groups: &[String]) -> StoreResult<()> {
        self.write()?;
        self.inner.set_account_groups(account, groups)
    }

    fn set_character_entity(&mut self, character: u64, entity: u32) -> StoreResult<()> {
        self.write()?;
        self.inner.set_character_entity(character, entity)
    }

    fn set_account_ban(&mut self, account: u64, ban: Option<&Ban>) -> StoreResult<()> {
        self.write()?;
        self.inner.set_account_ban(account, ban)
    }

    fn rename_character(&mut self, character: u64, name: &str) -> StoreResult<()> {
        self.write()?;
        self.inner.rename_character(character, name)
    }

    fn gag(&mut self, character: u64) -> StoreResult<Option<f64>> {
        self.read()?;
        self.inner.gag(character)
    }

    fn set_gag(&mut self, character: u64, seconds: Option<f64>) -> StoreResult<()> {
        self.write()?;
        self.inner.set_gag(character, seconds)
    }

    fn characters(&mut self, account: u64) -> StoreResult<Vec<Character>> {
        self.read()?;
        self.inner.characters(account)
    }

    fn character(&mut self, id: u64) -> StoreResult<Option<Character>> {
        self.read()?;
        self.inner.character(id)
    }

    fn character_named(&mut self, name: &str) -> StoreResult<Option<Character>> {
        self.read()?;
        self.inner.character_named(name)
    }

    fn name_taken(&mut self, name: &str) -> StoreResult<bool> {
        assert!(!self.name_taken_panics, "a bug in an event");
        self.read()?;
        self.inner.name_taken(name)
    }

    fn next_character_number(&mut self) -> StoreResult<u64> {
        self.write()?;
        self.inner.next_character_number()
    }

    fn reserve_thing_numbers(&mut self, n: u64) -> StoreResult<u64> {
        self.write()?;
        self.inner.reserve_thing_numbers(n)
    }

    fn insert_character(&mut self, c: &Character) -> StoreResult<()> {
        self.write()?;
        self.inner.insert_character(c)
    }

    fn insert_character_with_state(
        &mut self,
        c: &Character,
        state: &WorldState,
    ) -> StoreResult<()> {
        self.write()?;
        self.inner.insert_character_with_state(c, state)
    }

    fn set_deleted(&mut self, id: u64, at: Option<i64>) -> StoreResult<()> {
        self.write()?;
        self.inner.set_deleted(id, at)
    }

    fn set_last_played(&mut self, id: u64, at: i64) -> StoreResult<()> {
        assert!(
            self.last_played_panics != Some(id),
            "a bug in the middle of a leaving"
        );
        self.write()?;
        self.inner.set_last_played(id, at)
    }

    fn purge_character(&mut self, id: u64) -> StoreResult<()> {
        self.write()?;
        self.inner.purge_character(id)
    }

    fn world_state(&mut self, id: u64) -> StoreResult<Option<WorldState>> {
        self.read()?;
        if let Some(n) = &mut self.state_reads_after {
            if *n == 0 {
                self.state_reads_fail = true;
            } else {
                *n -= 1;
            }
        }
        if self.state_reads_fail {
            return Err(StoreError("database is locked".into()));
        }
        self.inner.world_state(id)
    }

    fn set_world_state(&mut self, id: u64, state: &WorldState) -> StoreResult<()> {
        self.write()?;
        self.inner.set_world_state(id, state)
    }

    fn save_all(
        &mut self,
        states: &[(u64, &WorldState)],
        records: &[(&str, &[u8])],
    ) -> StoreResult<()> {
        self.write()?;
        self.inner.save_all(states, records)
    }

    fn world_record(&mut self, name: &str) -> StoreResult<Option<Vec<u8>>> {
        self.read()?;
        if self.records_read_fail {
            return Err(StoreError("database is locked".into()));
        }
        self.inner.world_record(name)
    }

    fn set_world_record(&mut self, name: &str, value: &[u8]) -> StoreResult<()> {
        self.write()?;
        self.inner.set_world_record(name, value)
    }

    fn network_characters(&mut self) -> StoreResult<Vec<u64>> {
        self.read()?;
        self.inner.network_characters()
    }

    fn set_network_on(&mut self, id: u64, on: bool) -> StoreResult<()> {
        self.write()?;
        self.inner.set_network_on(id, on)
    }

    fn network_blocks(&mut self) -> StoreResult<Vec<String>> {
        self.read()?;
        self.inner.network_blocks()
    }

    fn set_network_block(&mut self, block: &str, on: bool) -> StoreResult<()> {
        self.write()?;
        self.inner.set_network_block(block, on)
    }

    fn network_lists(&mut self) -> StoreResult<Vec<Listed>> {
        self.read()?;
        self.inner.network_lists()
    }

    fn set_listed(
        &mut self,
        character: u64,
        list: NetworkList,
        entry: &str,
        on: bool,
    ) -> StoreResult<()> {
        self.write()?;
        self.inner.set_listed(character, list, entry, on)
    }
}

/// A folder of a test's own under the temporary folder, removed when the test ends.
#[derive(Debug)]
pub struct TempDir(std::path::PathBuf);

impl TempDir {
    /// Make one, named for this `what` and this process and made unique by a counter.
    ///
    /// # Panics
    /// The folder cannot be made.
    #[must_use]
    pub fn new(what: &str) -> Self {
        static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
        let n = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        let dir =
            std::env::temp_dir().join(format!("yalain-faults-{what}-{}-{n}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("the temporary folder");
        Self(dir)
    }

    /// Where it is.
    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
