//! A small MySQL: enough of its parser and its InnoDB table semantics to apply ACE's content
//! files (the per-object `DELETE` + `INSERT` SQL of ACE's content repositories, and the SQL that
//! ACE's JSON import writes) over a base dump, as ACE's MySQL would.
//!
//! Not ACE-derived. What it models, because the world database's contents depend on it:
//! * primary-key order (a table scan returns rows in clustered-index order);
//! * `AUTO_INCREMENT` (the counter starts at the dump's `AUTO_INCREMENT=` table option or past the
//!   largest id, never goes back after a `DELETE`, and `LAST_INSERT_ID()` is the first id the
//!   last generating `INSERT` produced);
//! * unique keys (a duplicate is an error; `REPLACE` deletes the rows it collides with);
//! * foreign keys (`ON DELETE CASCADE`, and the parent must exist on insert) while
//!   `foreign_key_checks` is on;
//! * column types, defaults, `ON UPDATE CURRENT_TIMESTAMP`, the generated `landblock` column,
//!   strict mode, `NO_AUTO_VALUE_ON_ZERO`, and `utf8_general_ci` comparisons.

pub mod lex;
pub mod parse;
pub mod store;
pub mod value;
