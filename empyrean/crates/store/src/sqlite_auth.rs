// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/Models/Auth/AuthDbContext.cs
//! `SqliteAuth`: the authentication database in embedded SQLite (ACE's MySQL `AuthDbContext`).
//! It may share a file with [`crate::SqliteShard`]; the two schemas have no common tables.

use std::path::Path;
use std::sync::Arc;

use empyrean_common::account_defaults::AccountDefaults;
use empyrean_common::clock::Clock;
use empyrean_common::dotnet::datetime::DotNetDateTime;
use rusqlite::{params, Connection, Row};

use crate::authentication_database::{AccountQuery, AuthDatabase};
use crate::error::StoreError;
use crate::models::auth::Account;
use crate::sqlite_shard::prepare_connection;
use crate::upgrade::{self, Schema, UpgradePolicy, Versioning};

/// Schema migrations, applied in order. The auth tables keep their own version in the table
/// `serv_store_auth_version`, so the file can also hold the shard schema (`PRAGMA user_version`).
/// Version 1 is the schema Empyrean 0.1.0 shipped.
const AUTH_MIGRATIONS: &[&str] = &[include_str!("schema/auth_v001.sql")];

/// The authentication schema: [`AUTH_MIGRATIONS`], versioned in `serv_store_auth_version`.
pub const AUTH_SCHEMA: Schema = Schema {
    name: "authentication",
    label: "auth",
    migrations: AUTH_MIGRATIONS,
    versioning: Versioning::Table("serv_store_auth_version"),
};

/// The authentication database over one SQLite connection.
pub struct SqliteAuth {
    conn: Connection,
    config: AccountDefaults,
    clock: Arc<dyn Clock>,
}

impl std::fmt::Debug for SqliteAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SqliteAuth")
            .field("path", &self.conn.path())
            .finish_non_exhaustive()
    }
}

impl SqliteAuth {
    /// Opens (creating if needed) the authentication database file at `path`, upgrading its
    /// schema as [`crate::upgrade`] describes (backups beside the file).
    ///
    /// # Errors
    /// When the file cannot be opened, was written by a newer release, cannot be backed up before
    /// an upgrade, or cannot be migrated.
    pub fn open(
        path: impl AsRef<Path>,
        config: AccountDefaults,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, StoreError> {
        Self::open_with(path, config, clock, &UpgradePolicy::default())
    }

    /// [`SqliteAuth::open`] with the backups kept as `policy` says.
    ///
    /// # Errors
    /// As [`SqliteAuth::open`].
    pub fn open_with(
        path: impl AsRef<Path>,
        config: AccountDefaults,
        clock: Arc<dyn Clock>,
        policy: &UpgradePolicy,
    ) -> Result<Self, StoreError> {
        let (conn, _) = upgrade::open(path.as_ref(), &AUTH_SCHEMA, policy)?;
        Ok(Self {
            conn,
            config,
            clock,
        })
    }

    /// A fresh in-memory authentication database.
    ///
    /// # Errors
    /// When SQLite cannot create it.
    pub fn open_in_memory(
        config: AccountDefaults,
        clock: Arc<dyn Clock>,
    ) -> Result<Self, StoreError> {
        let conn = Connection::open_in_memory()?;
        prepare_connection(&conn, false)?;
        upgrade::upgrade(&conn, None, &AUTH_SCHEMA, &UpgradePolicy::default())?;
        Ok(Self {
            conn,
            config,
            clock,
        })
    }
}

const COLS: &str = "SELECT accountId, accountName, passwordHash, passwordSalt, accessLevel, email_Address, create_Time, create_I_P, last_Login_Time, last_Login_I_P, total_Times_Logged_In, banned_Time, banned_By_Account_Id, ban_Expire_Time, ban_Reason FROM account";

fn map_account(r: &Row<'_>) -> rusqlite::Result<Account> {
    let dt = |v: Option<i64>| v.map(DotNetDateTime::from_ticks);
    Ok(Account {
        account_id: r.get(0)?,
        account_name: r.get(1)?,
        password_hash: r.get(2)?,
        password_salt: r.get(3)?,
        access_level: r.get(4)?,
        email_address: r.get(5)?,
        create_time: DotNetDateTime::from_ticks(r.get(6)?),
        create_ip: r.get(7)?,
        last_login_time: dt(r.get(8)?),
        last_login_ip: r.get(9)?,
        total_times_logged_in: r.get(10)?,
        banned_time: dt(r.get(11)?),
        banned_by_account_id: r.get(12)?,
        ban_expire_time: dt(r.get(13)?),
        ban_reason: r.get(14)?,
    })
}

impl AuthDatabase for SqliteAuth {
    fn accounts_config(&self) -> &AccountDefaults {
        &self.config
    }

    fn clock(&self) -> &dyn Clock {
        self.clock.as_ref()
    }

    fn insert_account(&mut self, a: &mut Account) -> Result<(), StoreError> {
        let ticks = |d: Option<DotNetDateTime>| d.map(DotNetDateTime::ticks);
        let explicit: Option<u32> = if a.account_id == 0 {
            None
        } else {
            Some(a.account_id)
        };
        self.conn.execute(
            "INSERT INTO account (accountId, accountName, passwordHash, passwordSalt, accessLevel, email_Address, create_Time, create_I_P, last_Login_Time, last_Login_I_P, total_Times_Logged_In, banned_Time, banned_By_Account_Id, ban_Expire_Time, ban_Reason) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15)",
            params![
                explicit, a.account_name, a.password_hash, a.password_salt, a.access_level, a.email_address, a.create_time.ticks(),
                a.create_ip, ticks(a.last_login_time), a.last_login_ip, a.total_times_logged_in, ticks(a.banned_time),
                a.banned_by_account_id, ticks(a.ban_expire_time), a.ban_reason
            ],
        )?;
        a.account_id = u32::try_from(self.conn.last_insert_rowid()).unwrap_or(0);
        Ok(())
    }

    fn select_accounts(&mut self, query: AccountQuery<'_>) -> Result<Vec<Account>, StoreError> {
        let (sql, p): (String, rusqlite::types::Value) = match query {
            AccountQuery::Id(id) => (
                format!("{COLS} WHERE accountId = ?1 ORDER BY accountId"),
                i64::from(id).into(),
            ),
            AccountQuery::Name(name) => (
                format!("{COLS} WHERE accountName = ?1 ORDER BY accountId"),
                name.to_owned().into(),
            ),
            AccountQuery::AccessLevel(level) => (
                format!("{COLS} WHERE accessLevel = ?1 ORDER BY accountId"),
                i64::from(level).into(),
            ),
            AccountQuery::BannedAfter(now) => (
                format!("{COLS} WHERE ban_Expire_Time > ?1 ORDER BY accountId"),
                now.ticks().into(),
            ),
        };
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map([p], map_account)?;
        Ok(rows.collect::<Result<Vec<_>, _>>()?)
    }

    fn update_account_row(&mut self, a: &Account) -> Result<usize, StoreError> {
        let ticks = |d: Option<DotNetDateTime>| d.map(DotNetDateTime::ticks);
        Ok(self.conn.execute(
            "UPDATE account SET accountName = ?2, passwordHash = ?3, passwordSalt = ?4, accessLevel = ?5, email_Address = ?6, create_Time = ?7, create_I_P = ?8, last_Login_Time = ?9, last_Login_I_P = ?10, total_Times_Logged_In = ?11, banned_Time = ?12, banned_By_Account_Id = ?13, ban_Expire_Time = ?14, ban_Reason = ?15 WHERE accountId = ?1",
            params![
                a.account_id, a.account_name, a.password_hash, a.password_salt, a.access_level, a.email_address, a.create_time.ticks(),
                a.create_ip, ticks(a.last_login_time), a.last_login_ip, a.total_times_logged_in, ticks(a.banned_time),
                a.banned_by_account_id, ticks(a.ban_expire_time), a.ban_reason
            ],
        )?)
    }

    fn count_accounts(&mut self) -> Result<i64, StoreError> {
        Ok(self
            .conn
            .query_row("SELECT COUNT(*) FROM account", [], |r| r.get(0))?)
    }
}
