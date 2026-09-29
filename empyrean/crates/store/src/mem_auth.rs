//! `MemAuth`: the authentication database in plain Rust collections, for tests. Not ACE.
//! Same row order (by account id), the same name matching and unique name (the [`crate::collation`]),
//! and the same access-level foreign key (levels 0..=5) as [`crate::SqliteAuth`].

use std::collections::BTreeMap;
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::Clock;

use crate::authentication_database::{AccountQuery, AuthDatabase};
use crate::collation;
use crate::error::StoreError;
use crate::models::auth::Account;

/// The in-memory authentication database.
pub struct MemAuth {
    accounts: BTreeMap<u32, Account>,
    last_id: u32,
    config: AccountDefaults,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for MemAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MemAuth")
            .field("accounts", &self.accounts.len())
            .finish_non_exhaustive()
    }
}

impl MemAuth {
    /// An empty store.
    #[must_use]
    pub fn new(config: AccountDefaults, clock: Arc<dyn Clock>) -> Self {
        Self {
            accounts: BTreeMap::new(),
            last_id: 0,
            config,
            clock,
        }
    }

    fn check(&self, a: &Account) -> Result<(), StoreError> {
        if a.access_level > 5 {
            return Err(StoreError::Constraint(
                "FOREIGN KEY constraint failed".into(),
            ));
        }
        if self.accounts.values().any(|o| {
            o.account_id != a.account_id && collation::eq(&o.account_name, &a.account_name)
        }) {
            return Err(StoreError::Constraint(
                "UNIQUE constraint failed: account.accountName".into(),
            ));
        }
        Ok(())
    }
}

impl AuthDatabase for MemAuth {
    fn accounts_config(&self) -> &AccountDefaults {
        &self.config
    }

    fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    fn insert_account(&mut self, account: &mut Account) -> Result<(), StoreError> {
        let mut a = account.clone();
        if a.account_id == 0 {
            a.account_id = self.last_id + 1;
        }
        if self.accounts.contains_key(&a.account_id) {
            return Err(StoreError::Constraint(
                "UNIQUE constraint failed: account.accountId".into(),
            ));
        }
        self.check(&a)?;
        self.last_id = self.last_id.max(a.account_id);
        account.account_id = a.account_id;
        self.accounts.insert(a.account_id, a);
        Ok(())
    }

    fn select_accounts(&mut self, query: AccountQuery<'_>) -> Result<Vec<Account>, StoreError> {
        Ok(self
            .accounts
            .values()
            .filter(|a| match query {
                AccountQuery::Id(id) => a.account_id == id,
                AccountQuery::Name(name) => collation::eq(&a.account_name, name),
                AccountQuery::AccessLevel(level) => a.access_level == level,
                AccountQuery::BannedAfter(now) => a.ban_expire_time.is_some_and(|t| t > now),
            })
            .cloned()
            .collect())
    }

    fn update_account_row(&mut self, account: &Account) -> Result<usize, StoreError> {
        if !self.accounts.contains_key(&account.account_id) {
            return Ok(0);
        }
        self.check(account)?;
        self.accounts.insert(account.account_id, account.clone());
        Ok(1)
    }

    fn count_accounts(&mut self) -> Result<i64, StoreError> {
        Ok(i64::try_from(self.accounts.len()).unwrap_or(i64::MAX))
    }
}
