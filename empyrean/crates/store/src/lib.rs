//! Shard and authentication persistence: the Shard and Authentication sides of ACE's
//! `Source/ACE.Database` over embedded SQLite, with an in-memory store for tests.
//!
//! **Depends on** `empyrean-common` and `empyrean-entity`. **Used by** the world content database
//! (`empyrean-content`), the gameplay crate (`empyrean-world`), the commands (`empyrean-command`),
//! the server (`empyrean-server`) and the test kit (`empyrean-testkit`).
//!
//! **Must never** need a database server: SQLite is compiled into the binary (`rusqlite`'s
//! `bundled` feature), and nothing here depends on a server crate above `empyrean-entity`.
//!
//! Module map (ACE file → module):
//! * `ShardDatabase.cs` → [`shard_database`] (the [`ShardDatabase`] trait: backend primitives plus
//!   ACE's methods), backends [`SqliteShard`] (`ShardDbContext.cs`) and [`MemShard`];
//! * `ShardDatabaseWithCaching.cs` → [`shard_database_with_caching`];
//! * `ShardDatabaseOfflineTools.cs` → [`shard_database_offline_tools`];
//! * `SerializedShardDatabase.cs` → [`serialized_shard_database`] ([`ShardHandle`], the database
//!   thread);
//! * `ShardConfigDatabase.cs` → [`shard_config_database`];
//! * `AuthenticationDatabase.cs` → [`authentication_database`] ([`AuthDatabase`]), backends
//!   [`SqliteAuth`] (`AuthDbContext.cs`) and [`MemAuth`];
//! * `Models/Shard/*`, `Models/Auth/*` → [`models`] (row models and their extensions);
//! * `Adapter/BiotaConverter.cs`, `Adapter/BiotaUpdater.cs` → [`adapter`];
//! * `Entity/PossessedBiotas.cs` → [`entity`];
//! * `ACE.Common/Cryptography/BCryptProvider.cs` → [`bcrypt_provider`] (over [`bcrypt`], not ACE);
//! * the MariaDB collation of ACE's name and key columns → [`collation`] (not ACE).
//!
//! Schemas: `src/schema/shard_v001.sql` and `src/schema/auth_v001.sql` (ACE's `ShardBase.sql` and
//! `AuthenticationBase.sql` for SQLite), each version 1 of its schema, as Empyrean 0.1.0 shipped
//! them. [`upgrade`] (not ACE) brings an older file up to date when it is opened, backing it up
//! first, and refuses a file a newer release wrote; `UPGRADES.md` says what each release adds.

pub mod adapter;
pub mod authentication_database;
pub mod bcrypt;
pub mod bcrypt_provider;
mod blowfish_tables;
mod cast;
pub mod collation;
pub mod entity;
pub mod error;
pub mod mem_auth;
pub mod mem_shard;
pub mod models;
pub mod serialized_shard_database;
pub mod shard_config_database;
pub mod shard_database;
pub mod shard_database_offline_tools;
pub mod shard_database_with_caching;
pub mod sqlite_auth;
pub mod sqlite_shard;
pub mod upgrade;

pub use authentication_database::AuthDatabase;
pub use error::StoreError;
pub use mem_auth::MemAuth;
pub use mem_shard::MemShard;
pub use serialized_shard_database::ShardHandle;
pub use shard_config_database::ShardConfigDatabase;
pub use shard_database::ShardDatabase;
pub use shard_database_with_caching::ShardDatabaseWithCaching;
pub use sqlite_auth::SqliteAuth;
pub use sqlite_shard::SqliteShard;
