//! A game's dialect of the pack format: the words one world builder fills the shared grammar with.
//!
//! The grammar (a world's packs and overrides, symbols, the lockfile, format versions, the
//! conflict report) is the same for every game. What differs is the vocabulary: what a symbol can
//! name and how its ids are given out, the eras a world can be built on, and the items an
//! override can name. A world builder supplies them as a type implementing [`Dialect`], and reads
//! its worlds as [`crate::World`]`<ItsDialect>`.
//!
//! Every world and pack file can say which game it is for (`game = "..."`). A file that says none
//! is for [`DEFAULT_GAME`], so every file written before the key existed reads unchanged; a world
//! builder refuses a file of another game.

use std::fmt;

use crate::symbol::SymbolKind;

/// The game a world or pack file that names none is for.
pub const DEFAULT_GAME: &str = "ac1";

/// A game's words for the pack format, as one world builder speaks them.
pub trait Dialect:
    Clone + Copy + fmt::Debug + Default + PartialEq + Eq + Send + Sync + 'static
{
    /// The game's key, as a world or pack file writes it: `game = "ac1"`.
    const GAME: &'static str;

    /// The world builder's name, as its messages say it.
    const PRODUCT: &'static str;

    /// The command that numbers a world's symbols, as the lockfile's header says it.
    const BUILD_COMMAND: &'static str;

    /// Every form of item an override can name, as the error for one that names none lists them.
    const ITEM_FORMS: &'static str;

    /// Whether a world of the game may name a base database in `[base]` ([`crate::world::Base`]).
    /// A builder whose worlds lie over one fixed base of their own says no, and a world that
    /// names one is refused.
    const NAMES_BASE: bool = false;

    /// What the game's symbols name.
    type Kind: SymbolKind;

    /// An era of the game's data files, which a world is built on.
    type Era: Copy + Eq + fmt::Debug + Send + Sync + 'static;

    /// The era a world file names, or the game's own when it names none.
    ///
    /// # Errors
    /// A name that is no era of the game's, with the ones there are.
    fn era(name: Option<&str>) -> Result<Self::Era, String>;

    /// An item of the game's own, as an override writes it (`text`, split into `words`), in the
    /// one form the build's report names it; `None` when it names none of the game's. The shared
    /// forms, `landblock <XXYY>` and a new record's kind and symbol, are read by the grammar.
    fn item(text: &str, words: &[&str]) -> Option<String>;
}

/// Whether a file that says it is for `game` (`None`: it says nothing) is for `D`'s game.
///
/// # Errors
/// A file of another game, as a sentence.
pub fn check_game<D: Dialect>(game: Option<&str>, what: &str) -> Result<(), String> {
    let game = game.unwrap_or(DEFAULT_GAME);
    if game == D::GAME {
        Ok(())
    } else {
        Err(format!(
            "this {what} is for {game:?}; {} reads {what}s for {:?}",
            D::PRODUCT,
            D::GAME
        ))
    }
}
