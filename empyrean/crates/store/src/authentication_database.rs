// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/AuthenticationDatabase.cs
//! `AuthenticationDatabase`: accounts. A trait with backend primitives (not ACE) and ACE's methods
//! (provided), implemented by [`crate::SqliteAuth`] and [`crate::MemAuth`].
//!
//! The backend carries what ACE reads from globals: the `Server.Accounts` configuration
//! (`ConfigManager.Config`) and the clock (`DateTime.UtcNow`). Database exceptions ACE lets escape
//! are panics with the exception's name, except `CreateAccount`'s documented duplicate-name
//! `MySqlException`, which is returned.

use std::net::IpAddr;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::Clock;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use empyrean_entity::enums::AccessLevel;

use crate::error::StoreError;
use crate::models::auth::Account;

/// An account query (backend primitive). Rows come back by `accountId`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AccountQuery<'a> {
    /// `accountId == id`.
    Id(u32),
    /// `accountName == name` (case-insensitive, as MySQL's collation compares).
    Name(&'a str),
    /// `accessLevel == level`.
    AccessLevel(u32),
    /// `ban_Expire_Time > now`.
    BannedAfter(DotNetDateTime),
}

/// `IPAddress.GetAddressBytes()`.
#[must_use]
pub fn address_bytes(address: IpAddr) -> Vec<u8> {
    match address {
        IpAddr::V4(a) => a.octets().to_vec(),
        IpAddr::V6(a) => a.octets().to_vec(),
    }
}

/// Storage for accounts: backend primitives and ACE's `AuthenticationDatabase` methods.
pub trait AuthDatabase: Send {
    // ---------------------------------------------------------------------------------------------
    // backend primitives
    // ---------------------------------------------------------------------------------------------

    /// `ConfigManager.Config.Server.Accounts`.
    fn accounts_config(&self) -> &AccountDefaults;

    /// The clock `DateTime.UtcNow` reads.
    fn clock(&self) -> &dyn Clock;

    /// Inserts `account` and sets its new `account_id`.
    ///
    /// # Errors
    /// A duplicate name (or any backend failure).
    fn insert_account(&mut self, account: &mut Account) -> Result<(), StoreError>;

    /// The accounts matching `query`, by id.
    ///
    /// # Errors
    /// A backend failure.
    fn select_accounts(&mut self, query: AccountQuery<'_>) -> Result<Vec<Account>, StoreError>;

    /// Writes every column of the account with `account.account_id`; the number of rows changed.
    ///
    /// # Errors
    /// A constraint violation or backend failure.
    fn update_account_row(&mut self, account: &Account) -> Result<usize, StoreError>;

    /// `SELECT COUNT(*) FROM account`.
    ///
    /// # Errors
    /// A backend failure.
    fn count_accounts(&mut self) -> Result<i64, StoreError>;

    // ---------------------------------------------------------------------------------------------
    // ACE
    // ---------------------------------------------------------------------------------------------

    // ACE: AuthenticationDatabase.Exists
    /// Always true for an opened embedded database (ACE's retry loop never runs).
    fn exists(&mut self, _retry_until_found: bool) -> bool {
        log::info!("[DATABASE] Successfully connected to authentication database.");
        true
    }

    // ACE: AuthenticationDatabase.GetAccountCount
    fn get_account_count(&mut self) -> i32 {
        i32::try_from(self.count_accounts().unwrap_or_else(|e| panic!("{e}"))).unwrap_or(i32::MAX)
    }

    // ACE: AuthenticationDatabase.CreateAccount
    /// Creates an account with a bcrypt password hash.
    ///
    /// # Errors
    /// An account with this name already exists (ACE's `MySqlException`).
    fn create_account(
        &mut self,
        name: &str,
        password: &str,
        access_level: AccessLevel,
        address: IpAddr,
    ) -> Result<Account, StoreError> {
        let mut account = Account {
            account_name: name.to_owned(),
            ..Default::default()
        };

        let config = self.accounts_config().clone();
        account.set_password(password, &config);
        account.set_salt_for_bcrypt();
        account.access_level = access_level.0 as u32;

        account.create_time = self.clock().utc_now();
        account.create_ip = Some(address_bytes(address));

        self.insert_account(&mut account)?;

        Ok(account)
    }

    // ACE: AuthenticationDatabase.GetAccountById
    /// Will return `None` if the account id was not found.
    fn get_account_by_id(&mut self, account_id: u32) -> Option<Account> {
        self.select_accounts(AccountQuery::Id(account_id))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .next()
    }

    // ACE: AuthenticationDatabase.GetAccountByName
    /// Will return `None` if the account name was not found.
    fn get_account_by_name(&mut self, account_name: &str) -> Option<Account> {
        self.select_accounts(AccountQuery::Name(account_name))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .next()
    }

    // ACE: AuthenticationDatabase.GetAccountIdByName
    /// The id will be 0 if the account name was not found.
    fn get_account_id_by_name(&mut self, account_name: &str) -> u32 {
        let result = self
            .select_accounts(AccountQuery::Name(account_name))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .next();

        result.map_or(0, |r| r.account_id)
    }

    // ACE: AuthenticationDatabase.UpdateAccount
    /// Writes every column of `account`.
    ///
    /// # Panics
    /// When no row has its id (`DbUpdateConcurrencyException`) or the write fails.
    fn update_account(&mut self, account: &Account) {
        let n = self
            .update_account_row(account)
            .unwrap_or_else(|e| panic!("DbUpdateException: {e}"));
        assert!(n == 1, "DbUpdateConcurrencyException: expected to affect 1 row(s), but actually affected {n} row(s)");
    }

    // ACE: AuthenticationDatabase.UpdateAccountAccessLevel
    // ACE-BUG: `.First(...)` throws InvalidOperationException for an unknown id, so the following
    // `if (account == null) return false;` is dead code and the method never returns false.
    ///
    /// # Panics
    /// For an unknown `account_id` (`InvalidOperationException`), as ACE throws.
    fn update_account_access_level(&mut self, account_id: u32, access_level: AccessLevel) -> bool {
        let mut account = self
            .select_accounts(AccountQuery::Id(account_id))
            .unwrap_or_else(|e| panic!("{e}"))
            .into_iter()
            .next()
            .expect("InvalidOperationException: Sequence contains no matching element");

        account.access_level = access_level.0 as u32;

        self.update_account(&account);

        true
    }

    // ACE: AuthenticationDatabase.GetListofAccountsByAccessLevel
    /// The names of the accounts at `access_level`.
    ///
    /// # Panics
    /// For a negative access level (`Convert.ToUInt32` throws `OverflowException`).
    fn get_listof_accounts_by_access_level(&mut self, access_level: AccessLevel) -> Vec<String> {
        let level = u32::try_from(access_level.0)
            .expect("OverflowException: Value was either too large or too small for a UInt32.");
        let results = self
            .select_accounts(AccountQuery::AccessLevel(level))
            .unwrap_or_else(|e| panic!("{e}"));

        results
            .into_iter()
            .map(|account| account.account_name)
            .collect()
    }

    // ACE: AuthenticationDatabase.GetListofBannedAccounts
    /// One line per account whose ban has not expired.
    ///
    /// DIVERGE: ACE prints the expiry in the server's local time zone (`ToLocalTime()`); the injected
    /// clock has no zone, so it is printed in UTC.
    ///
    /// # Panics
    /// When a banned account has no `banned_by_account_id` (`InvalidOperationException`) or names an
    /// account that does not exist (`NullReferenceException`), as ACE throws.
    fn get_listof_banned_accounts(&mut self) -> Vec<String> {
        let now = self.clock().utc_now();
        let results = self
            .select_accounts(AccountQuery::BannedAfter(now))
            .unwrap_or_else(|e| panic!("{e}"));

        let mut result = Vec::new();
        for account in results {
            let banned_by = account
                .banned_by_account_id
                .expect("InvalidOperationException: Nullable object must have a value.");
            let bannedby_account = if banned_by > 0 {
                let by = self
                    .get_account_by_id(banned_by)
                    .expect("NullReferenceException: banning account not found");
                format!("account {}", by.account_name)
            } else {
                "CONSOLE".to_owned()
            };
            let expire = account
                .ban_expire_time
                .expect("InvalidOperationException: Nullable object must have a value.");
            let reason = match &account.ban_reason {
                Some(r) if !r.trim().is_empty() => format!(" -- Reason: {r}"),
                _ => String::new(),
            };
            result.push(format!(
                "{} -- banned by {} until server time {}{}",
                account.account_name,
                bannedby_account,
                expire.format("MMM dd yyyy  h:mmtt"),
                reason
            ));
        }

        result
    }
}
