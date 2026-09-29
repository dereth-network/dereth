// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Database/ShardConfigDatabase.cs
//! `ShardConfigDatabase`: the four `config_properties_*` tables (server properties set in game).
//!
//! In ACE each call opens its own context on the shard database; here the trait is implemented by
//! the shard backends ([`crate::SqliteShard`], [`crate::MemShard`]). Keys compare as MySQL's
//! collation does (case-insensitively). `GetAll*` return rows in key order (the primary key).
//! As in ACE, adding an existing key or saving a missing one throws (here: panics with the Entity
//! Framework exception name).

use crate::models::shard::{
    ConfigPropertiesBoolean, ConfigPropertiesDouble, ConfigPropertiesLong, ConfigPropertiesString,
};

// ACE: ShardConfigDatabase
/// ACE's `ShardConfigDatabase`.
pub trait ShardConfigDatabase {
    // ACE: ShardConfigDatabase.BoolExists
    fn bool_exists(&mut self, key: &str) -> bool;
    // ACE: ShardConfigDatabase.DoubleExists
    fn double_exists(&mut self, key: &str) -> bool;
    // ACE: ShardConfigDatabase.LongExists
    fn long_exists(&mut self, key: &str) -> bool;
    // ACE: ShardConfigDatabase.StringExists
    fn string_exists(&mut self, key: &str) -> bool;

    // ACE: ShardConfigDatabase.AddBool
    fn add_bool(&mut self, key: &str, value: bool, description: Option<&str>);
    // ACE: ShardConfigDatabase.AddLong
    fn add_long(&mut self, key: &str, value: i64, description: Option<&str>);
    // ACE: ShardConfigDatabase.AddDouble
    fn add_double(&mut self, key: &str, value: f64, description: Option<&str>);
    // ACE: ShardConfigDatabase.AddString
    fn add_string(&mut self, key: &str, value: &str, description: Option<&str>);

    // ACE: ShardConfigDatabase.GetBool
    fn get_bool(&mut self, key: &str) -> Option<ConfigPropertiesBoolean>;
    // ACE: ShardConfigDatabase.GetLong
    fn get_long(&mut self, key: &str) -> Option<ConfigPropertiesLong>;
    // ACE: ShardConfigDatabase.GetDouble
    fn get_double(&mut self, key: &str) -> Option<ConfigPropertiesDouble>;
    // ACE: ShardConfigDatabase.GetString
    fn get_string(&mut self, key: &str) -> Option<ConfigPropertiesString>;

    // ACE: ShardConfigDatabase.GetAllBools
    fn get_all_bools(&mut self) -> Vec<ConfigPropertiesBoolean>;
    // ACE: ShardConfigDatabase.GetAllLongs
    fn get_all_longs(&mut self) -> Vec<ConfigPropertiesLong>;
    // ACE: ShardConfigDatabase.GetAllDoubles
    fn get_all_doubles(&mut self) -> Vec<ConfigPropertiesDouble>;
    // ACE: ShardConfigDatabase.GetAllStrings
    fn get_all_strings(&mut self) -> Vec<ConfigPropertiesString>;

    // ACE: ShardConfigDatabase.SaveBool
    fn save_bool(&mut self, stat: &ConfigPropertiesBoolean);
    // ACE: ShardConfigDatabase.SaveLong
    fn save_long(&mut self, stat: &ConfigPropertiesLong);
    // ACE: ShardConfigDatabase.SaveDouble
    fn save_double(&mut self, stat: &ConfigPropertiesDouble);
    // ACE: ShardConfigDatabase.SaveString
    fn save_string(&mut self, stat: &ConfigPropertiesString);
}
