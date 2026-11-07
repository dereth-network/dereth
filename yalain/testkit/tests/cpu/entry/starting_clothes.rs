//! A new character's starting clothes: made as items at its first entry, worn, colored by its
//! physique's choices, drawn in its look from then on by what it wears; and a character kept
//! before they were made keeps its look as made. Fixture: a human man, made-up garments that start
//! worn on the chest, legs and feet (the boots giving another appearance than the physique type
//! that colors them names), a made-up weapon that starts in the pack.

use auberean_protocol::login::{CharacterCreated, CreateCharacter};
use auberean_protocol::table::{Table, SMALL};
use auberean_protocol::text::{NarrowString, WideString};
use auberean_protocol::types::VisualDesc;
use auberean_protocol::{queue, ClientMessage, ServerMessage};
use yalain_content::{Prop, Props};
use yalain_store::Store as _;
use yalain_testkit::play::{server_call, use_of};
use yalain_testkit::Harness;
use yalain_world::chargen::MALE;
use yalain_world::start::{StartItem, CLOTHES_COLORS};

const SWORD: u32 = 0x4700_7300;
const SHIRT: u32 = 0x4700_7301;
const PANTS: u32 = 0x4700_7302;
const BOOTS: u32 = 0x4700_7303;
/// What the shirt and pants give a human man: what their physique types name.
const SHIRT_LOOK: u32 = 0x2000_0016;
const PANTS_LOOK: u32 = 0x2000_0014;
/// What the boots give: not what their physique type names.
const BOOTS_LOOK: u32 = 0x2000_7303;
const BOOTS_NAMED: u32 = 0x2000_0019;
const SHIRT_COLOR: f32 = 0.3;
const PANTS_COLOR: f32 = 0.6;
const BOOTS_COLOR: f32 = 0.9;

fn garment(id: u32, place: u32, look: u32) -> yalain_content::Template {
    let mut p = Props::default();
    p.set("Item_ValidInventoryLocations", Prop::Word(place))
        .set("Item_PreferredInventoryLocation", Prop::Word(place))
        .set("Item_WornAppearanceRace1", Prop::Word(1))
        .set("Item_WornAppearanceSex1", Prop::Word(MALE))
        .set("Item_WornAppearanceFile1", Prop::Word(look));
    yalain_testkit::test_template(id, 0x180, p)
}

/// A server whose human starts with the weapon in its pack and the three garments worn.
fn world() -> Harness {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.insert(yalain_testkit::test_template(
        SWORD,
        0x1067,
        Props::default(),
    ));
    for t in [
        garment(SHIRT, 2, SHIRT_LOOK),
        garment(PANTS, 8, PANTS_LOOK),
        garment(BOOTS, 16, BOOTS_LOOK),
    ] {
        h.server.catalog.insert(t);
    }
    let worn = |template, color| StartItem {
        template,
        worn: true,
        color,
    };
    h.server.start.items.insert(
        1,
        vec![
            StartItem::held(SWORD),
            worn(SHIRT, 0x100),
            worn(BOOTS, 0x400),
            worn(PANTS, 0x200),
        ],
    );
    h
}

/// A human man with the garments' colors chosen; its id and the look it was made with.
fn create(h: &mut Harness, b: usize, account: &str) -> (u64, VisualDesc) {
    let before = h.bots[b].inbox.len();
    let m = ClientMessage::CreateCharacter(CreateCharacter {
        account: NarrowString::from(account),
        name: WideString::from("Newcomer"),
        entity_did: 0x4700_110F,
        start_area: 0x4000_0001,
        race: 1,
        sex: MALE,
        class: 2,
        implement: 1,
        physique: Table::from_inserted(
            SMALL,
            [
                (1, 1.0),
                (2, 1.0),
                (0x100, SHIRT_COLOR),
                (0x200, PANTS_COLOR),
                (0x400, BOOTS_COLOR),
            ],
        ),
    });
    h.bots[b].send(false, queue::LOGON, &m);
    assert!(h.run_until(1000, |h| h.bots[b].inbox.len() > before));
    match h.bots[b].inbox[before].message() {
        Some(ServerMessage::CharacterCreated(CharacterCreated { identity, .. })) => {
            (identity.id, identity.visual)
        }
        other => panic!("not a creation answer: {other:?}"),
    }
}

/// The value an appearance holds for a key, if the look holds the appearance.
fn value(look: &VisualDesc, table: u32, key: u32) -> Option<f32> {
    look.global
        .appearance
        .get(table)
        .and_then(|t| t.get(key).copied())
}

fn player(h: &Harness, me: u64) -> &yalain_world::world::Player {
    h.server
        .players
        .values()
        .find(|p| p.character == me)
        .expect("in the world")
}

fn of(h: &Harness, me: u64, template: u32) -> Vec<yalain_world::state::Held> {
    player(h, me)
        .play
        .items
        .iter()
        .filter(|x| x.template == template)
        .copied()
        .collect()
}

fn made_look(h: &Harness, b: usize, from: usize, me: u64) -> VisualDesc {
    h.world_messages(b, from)
        .iter()
        .find_map(|m| match m {
            ServerMessage::CreateObject(o) if o.id == me => Some(o.visual.clone()),
            _ => None,
        })
        .expect("the player made")
}

fn listed_look(h: &Harness, b: usize, from: usize, me: u64) -> VisualDesc {
    h.bots[b].inbox[from..]
        .iter()
        .rev()
        .find_map(|r| match r.message() {
            Some(ServerMessage::CharacterList(l)) => l
                .characters
                .iter()
                .find(|c| c.id == me)
                .map(|c| c.visual.clone()),
            _ => None,
        })
        .expect("listed")
}

fn log_off(h: &mut Harness, b: usize, me: u64) -> usize {
    let before = h.bots[b].inbox.len();
    h.bots[b].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(auberean_protocol::login::CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[b].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    before
}

fn use_it(h: &mut Harness, b: usize, me: u64, item: u64) {
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(10);
}

/// A new character is made wearing its starting garments, each in the color its physique chose and
/// with the appearance the garment gives its body (the boots' own, not what their physique type
/// names). At its first entry each garment is made an item worn where it goes, contained and worn
/// by the player, its color on the appearance a starting garment lies in a pack with; the weapon
/// is in the pack; and the player is drawn as it was made.
/// Behaviour: server.characters.starting-clothes-made-worn
#[test]
fn a_new_character_wears_its_starting_clothes_as_items_in_the_colors_it_chose() {
    let mut h = world();
    let b = h.at_character_list("one");
    let (me, made) = create(&mut h, b, "one");
    for (table, color) in [
        (SHIRT_LOOK, SHIRT_COLOR),
        (PANTS_LOOK, PANTS_COLOR),
        (BOOTS_LOOK, BOOTS_COLOR),
    ] {
        assert_eq!(value(&made, table, 2), Some(color), "{table:#x} colored");
        assert_eq!(value(&made, table, 0x10), Some(1.0), "{table:#x} shown");
    }
    assert_eq!(
        value(&made, BOOTS_NAMED, 2),
        None,
        "the boots' own appearance, not the physique type's"
    );

    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let p = player(&h, me);
    assert!(p.play.clothes_made, "its clothes are items");
    let places: Vec<(u32, u32)> = p
        .play
        .items
        .iter()
        .map(|x| (x.template, x.location))
        .collect();
    assert_eq!(
        places,
        [(SWORD, 0), (SHIRT, 2), (BOOTS, 16), (PANTS, 8)],
        "the data's order, the garments worn"
    );
    for (template, color) in [
        (SHIRT, SHIRT_COLOR),
        (PANTS, PANTS_COLOR),
        (BOOTS, BOOTS_COLOR),
    ] {
        let id = of(&h, me, template)[0].id;
        let o = h
            .world_messages(b, from)
            .iter()
            .find_map(|m| match m {
                ServerMessage::CreateObject(o) if o.id == id => Some(o.clone()),
                _ => None,
            })
            .expect("made");
        assert_eq!(o.weenie.objects[..2], [me, me], "contained and worn");
        assert_eq!(o.physics.bits, 0x82, "hung from nothing");
        assert_eq!(
            value(&o.visual, CLOTHES_COLORS, 2),
            Some(color),
            "its color where a starting garment keeps it"
        );
    }
    assert_eq!(made_look(&h, b, from, me), made, "drawn as it was made");
}

/// A starting garment taken off leaves the look and the pack holds it; at the next entry the
/// player is drawn without it and listed so, nothing new is made, and put on again it is drawn as
/// it was made, in its color: one of each garment all along.
/// Behaviour: server.characters.starting-clothes-off-stay-off
#[test]
fn a_starting_garment_taken_off_stays_off_across_entries_and_put_on_again_is_drawn_as_made() {
    let mut h = world();
    let b = h.at_character_list("one");
    let (me, made) = create(&mut h, b, "one");
    h.enter_world(b, "one", me);
    let shirt = of(&h, me, SHIRT)[0].id;
    use_it(&mut h, b, me, shirt);
    // The boots too: their appearance is their own, not what their physique type names.
    let boots = of(&h, me, BOOTS)[0].id;
    use_it(&mut h, b, me, boots);
    assert_eq!(of(&h, me, SHIRT)[0].location, 0, "in the pack");
    assert_eq!(value(&player(&h, me).look, SHIRT_LOOK, 2), None, "off");

    let listed = log_off(&mut h, b, me);
    assert_eq!(
        value(&listed_look(&h, b, listed, me), SHIRT_LOOK, 2),
        None,
        "listed without it"
    );
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    let back = made_look(&h, b, from, me);
    assert_eq!(value(&back, SHIRT_LOOK, 2), None, "drawn without it");
    assert_eq!(value(&back, BOOTS_LOOK, 2), None, "or the boots");
    assert_eq!(
        value(&back, PANTS_LOOK, 2),
        Some(PANTS_COLOR),
        "the rest worn"
    );
    for t in [SWORD, SHIRT, PANTS, BOOTS] {
        assert_eq!(of(&h, me, t).len(), 1, "one of {t:#x}");
    }
    assert_eq!(
        h.server
            .things
            .by_id
            .values()
            .filter(|t| t.template == SHIRT)
            .count(),
        1,
        "one shirt in the world"
    );

    use_it(&mut h, b, me, shirt);
    use_it(&mut h, b, me, boots);
    assert_eq!(of(&h, me, SHIRT)[0].location, 2, "on again");
    let look = player(&h, me).look.clone();
    assert_eq!(
        look.global.appearance.get(SHIRT_LOOK),
        made.global.appearance.get(SHIRT_LOOK),
        "drawn as it was made"
    );
    let listed = log_off(&mut h, b, me);
    assert_eq!(listed_look(&h, b, listed, me), made, "listed as made");
}

/// A character kept before starting clothes were made as items, holding none, is drawn as it was
/// made, clothes and all, and is given none at its next entry.
/// Behaviour: server.characters.kept-before-clothes-were-items
#[test]
fn a_character_kept_before_starting_clothes_were_items_keeps_its_look_and_is_given_none() {
    let mut h = world();
    let b = h.at_character_list("one");
    let (me, made) = create(&mut h, b, "one");
    h.enter_world(b, "one", me);
    // As an older build kept it: no garments, and no word of them.
    if let Some(p) = h.server.players.values_mut().find(|p| p.character == me) {
        let garments: Vec<u64> = p
            .play
            .items
            .iter()
            .filter(|x| x.template != SWORD)
            .map(|x| x.id)
            .collect();
        p.play.items.retain(|x| !garments.contains(&x.id));
        p.play.item_states.retain(|x| !garments.contains(&x.id));
        p.play.clothes_made = false;
    }
    let listed = log_off(&mut h, b, me);
    assert_eq!(listed_look(&h, b, listed, me), made, "listed as made");
    let from = h.bots[b].inbox.len();
    h.enter_world(b, "one", me);
    assert_eq!(made_look(&h, b, from, me), made, "drawn as made");
    let held: Vec<u32> = player(&h, me)
        .play
        .items
        .iter()
        .map(|x| x.template)
        .collect();
    assert_eq!(held, [SWORD], "nothing given");
    assert!(!player(&h, me).play.clothes_made);
}

/// A character the operator makes, its look drawn from its physique alone, enters wearing its
/// starting garments as items and drawn by them from its first entry: the boots' own appearance in
/// place of what their physique type names, never both, then and at the next entry.
/// Behaviour: server.characters.starting-clothes-made-worn
#[test]
fn a_character_the_operator_makes_is_drawn_by_its_garments_from_its_first_entry() {
    let mut h = world();
    let unix = h.unix;
    let r = yalain_world::characters::Recipe {
        account: "one".into(),
        name: "Made".into(),
        race: 1,
        sex: MALE,
        ..yalain_world::characters::Recipe::default()
    };
    yalain_world::characters::make(&mut h.server, &r, unix).expect("made");
    let me = h
        .server
        .store
        .character_named("Made")
        .expect("the store reads")
        .expect("there")
        .id;
    let b = h.at_character_list("one");
    for entry in ["first", "second"] {
        let from = h.bots[b].inbox.len();
        h.enter_world(b, "one", me);
        let drawn = made_look(&h, b, from, me);
        assert_eq!(
            value(&drawn, BOOTS_LOOK, 0x10),
            Some(1.0),
            "{entry}: the boots'"
        );
        assert_eq!(value(&drawn, BOOTS_NAMED, 0x10), None, "{entry}: not both");
        assert_eq!(of(&h, me, BOOTS).len(), 1, "{entry}: one pair");
        log_off(&mut h, b, me);
    }
}
