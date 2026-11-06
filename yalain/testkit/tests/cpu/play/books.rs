//! Books: one used from the pack is opened, then the use answered; a card shows its picture; a
//! person shows its book.

use yalain_content::{Prop, Props};
use yalain_testkit::play::use_of;
use yalain_testkit::{test_template, Harness};
use yalain_world::books::USE_BOOK;
use yalain_world::calls::{Node, Value};
use yalain_world::places::function::USE_DONE;

const GUIDE: u32 = 0x4700_06CD;
const TABLE: u32 = 0x2500_1CEE;

/// A book used from the pack is opened with its page table and its controls, then the use is
/// answered, in one tick.
/// Behaviour: server.books.a-book-used-is-opened
#[test]
fn a_book_used_from_the_pack_is_opened_then_the_use_answered() {
    let mut h = Harness::new();
    let mut props = Props::default();
    props.set("Book_Source", Prop::Str(0, TABLE));
    h.server.catalog.insert(test_template(GUIDE, 0x280B, props));
    let (b, id) = h.player_in_world("one", "Reader");
    let book = yalain_world::inventory::make_item(&mut h.server, GUIDE, 1, 0).expect("made");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: book,
        template: GUIDE,
        slot: 0,
        location: 0,
        quantity: 1,
    });
    if let Some(t) = h.server.things.get_mut(book) {
        t.container = Some((id, 0));
    }
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        yalain_world::requests::function::USE,
        &[use_of(book, id)],
        10,
    );
    let c = h.calls(b, from);
    let opened = c
        .iter()
        .position(|(f, _)| *f == USE_BOOK)
        .expect("the book opened");
    let done = c
        .iter()
        .position(|(f, _)| *f == USE_DONE)
        .expect("the use answered");
    assert!(opened < done);
    let v = &c[opened].1;
    assert!(v.iter().any(
        |x| matches!(x.root(), Some(Node::TextInfo(t)) if t.table == TABLE && t.string_id == 0)
    ));
    assert!(v.contains(&Value::Int(1)), "its controls shown");
}

/// Hold an item of a template in a player's pack.
fn held(h: &mut Harness, id: u64, template: u32) -> u64 {
    if h.server.catalog.get(template).is_none() {
        h.server
            .catalog
            .insert(test_template(template, 0x2817, Props::default()));
    }
    let item = yalain_world::inventory::make_item(&mut h.server, template, 1, 0).expect("made");
    let p = h
        .server
        .players
        .values_mut()
        .find(|p| p.character == id)
        .expect("in the world");
    p.play.items.push(yalain_world::state::Held {
        id: item,
        template,
        slot: 0,
        location: 0,
        quantity: 1,
    });
    if let Some(t) = h.server.things.get_mut(item) {
        t.container = Some((id, 0));
    }
    item
}

/// A tarot card whose picture is known, used from the pack, shows its picture (no page controls)
/// and then the use is answered with the picture's flags; the card is kept.
/// Behaviour: server.books.a-card-shows-its-picture
#[test]
fn a_greater_trump_used_shows_its_picture_and_is_kept() {
    let mut h = Harness::new();
    let (b, id) = h.player_in_world("one", "Reader");
    let (template, picture) = yalain_world::books::CARDS[0];
    let card = held(&mut h, id, template);
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        yalain_world::requests::function::USE,
        &[use_of(card, id)],
        10,
    );
    let c = h.calls(b, from);
    let opened = c
        .iter()
        .position(|(f, _)| *f == USE_BOOK)
        .expect("the picture shown");
    let done = c
        .iter()
        .position(|(f, _)| *f == USE_DONE)
        .expect("the use answered");
    assert!(opened < done);
    let v = &c[opened].1;
    assert!(v.contains(&Value::Int(picture)), "{v:?}");
    assert!(v.contains(&Value::Int(0)), "no page controls");
    assert!(v.iter().any(|x| matches!(x.root(), Some(Node::TextInfo(t))
        if t.table == yalain_world::books::PICTURE_TABLE)));
    let flags = c[done].1.iter().any(|x| match x {
        Value::Graph(g) => g.iter().any(|(_, n)| {
            matches!(n, Node::Script { slots, .. } if slots.iter().any(|s| s.1 == yalain_world::books::PICTURE_USE_FLAGS))
        }),
        _ => false,
    });
    assert!(flags, "the picture's use flags");
    assert!(
        h.server
            .players
            .values()
            .any(|p| p.play.items.iter().any(|x| x.id == card)),
        "the card is kept"
    );
}

/// A person who shows a book, used in reach, answers the use and then shows the book with its page
/// controls.
/// Behaviour: server.books.a-person-shows-a-book
#[test]
fn a_shi_used_answers_the_use_then_shows_the_architects_book() {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};
    let mut h = Harness::new();
    let (b, id) = h.player_in_world("one", "Reader");
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world")
        .position;
    let (template, table) = yalain_world::books::NPC_BOOKS[1];
    let shi = 0x40DF_8A00_0000_001E;
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(shi),
            template,
            description: Description {
                name_text: None,
                package: 0x13A4,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: at.cell,
            origin: [at.origin[0] + 1.0, at.origin[1], at.origin[2]],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Kind::Npc,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    let from = h.bots[b].inbox.len();
    h.ask(
        b,
        yalain_world::requests::function::USE,
        &[use_of(shi, id)],
        10,
    );
    let c = h.calls(b, from);
    let done = c
        .iter()
        .position(|(f, _)| *f == USE_DONE)
        .expect("the use answered");
    let opened = c
        .iter()
        .position(|(f, _)| *f == USE_BOOK)
        .expect("the book shown");
    assert!(done < opened, "the answer first");
    let v = &c[opened].1;
    assert!(v.iter().any(
        |x| matches!(x.root(), Some(Node::TextInfo(t)) if t.table == table && t.string_id == 0)
    ));
    assert!(v.contains(&Value::Int(1)), "its controls shown");
}
