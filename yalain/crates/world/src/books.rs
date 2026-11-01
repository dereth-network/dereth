//! Books: an item that is a book (its template names the string table its pages are in), used
//! from the pack, is opened for the player: the book (its table, with its page controls and no
//! picture), then the use answered, in one tick.
//!
//! **A person who shows a book** (the Shi who show the Shaper Architect's book), used in reach
//! (walked to within five meters first): the use answered, then the book with its page
//! controls and no picture.
//!
//! **A card that shows a picture** (the greater trumps of the tarot whose pictures are known) is
//! opened the same way, with the picture's table, no page controls and its picture, the use
//! answered with the picture's own flags; the card is kept.

use auberean_protocol::ServerMessage;
use auberean_script::args::Arg;
use auberean_script::native_object::TextInfo;
use yalain_content::Prop;
use yalain_net::ConnId;
use yalain_store::Store;

use crate::{session, Server};

/// The book's call: its source (a string table), whether its page controls show, its picture.
pub const USE_BOOK: u32 = 0x8216_002E;

/// The template property naming a book's string table.
pub const SOURCE: &str = "Book_Source";

/// The string table of a book's pages, when an item of this template is a book.
#[must_use]
pub fn source<S: Store>(s: &Server<S>, template: u32) -> Option<u32> {
    match s.catalog.get(template)?.props.get(SOURCE)? {
        Prop::Str(_, table) if *table != 0 => Some(*table),
        _ => None,
    }
}

/// What a card shows: its string table, its page controls shown or not, its picture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shown {
    pub table: u32,
    pub controls: bool,
    pub picture: u32,
}

/// The use answer's control flags for a card's picture shown.
pub const PICTURE_USE_FLAGS: u32 = 444;

/// The string table a card's picture is shown with (no title, no pages).
pub const PICTURE_TABLE: u32 = 0x2500_1D03;

/// The greater trumps whose pictures the 2017 servers showed: template, picture.
pub const CARDS: [(u32, u32); 4] = [
    (0x4700_13C1, 0x4100_91EC),
    (0x4700_13CE, 0x4100_91F0),
    (0x4700_13D2, 0x4100_91F2),
    (0x4700_13DC, 0x4100_91F7),
];

/// What a held item shows when used, when it is a card with a picture.
#[must_use]
pub fn shown_by<S: Store>(s: &Server<S>, world: ConnId, item: u64) -> Option<Shown> {
    let p = s.players.get(&world)?;
    let h = p.play.items.iter().find(|h| h.id == item)?;
    let (_, picture) = CARDS.iter().find(|(t, _)| *t == h.template)?;
    Some(Shown {
        table: PICTURE_TABLE,
        controls: false,
        picture: *picture,
    })
}

/// The people who show a book when used (template, the book's string table), as the 2017 servers
/// held them: the two Shi who show the Shaper Architect's book.
pub const NPC_BOOKS: [(u32, u32); 2] = [(0x4700_3367, 0x2500_2C90), (0x4700_3375, 0x2500_2C90)];

/// The book a person of this template shows when used.
#[must_use]
pub fn npc_book(template: u32) -> Option<u32> {
    NPC_BOOKS
        .iter()
        .find(|(t, _)| *t == template)
        .map(|(_, b)| *b)
}

/// The book opened: its table, its controls shown or not, its picture (0: none).
#[must_use]
pub fn open(table: u32, controls: bool, picture: u32) -> ServerMessage {
    let source = TextInfo {
        string_id: 0,
        table,
        ..TextInfo::default()
    };
    session::call(
        USE_BOOK,
        &[
            Arg::Int(picture),
            Arg::Int(u32::from(controls)),
            crate::chat::text_arg(source),
        ],
    )
    .expect("a book packs")
}

/// Is the item this player holds a book? Its table if so.
#[must_use]
pub fn held<S: Store>(s: &Server<S>, world: ConnId, item: u64) -> Option<u32> {
    let p = s.players.get(&world)?;
    let h = p.play.items.iter().find(|h| h.id == item)?;
    source(s, h.template)
}
