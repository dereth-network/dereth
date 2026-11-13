//! What effects place in the world as a world's `[[effect_rule]]` `places` rows say: an item's use
//! naming an effect the install holds no state of, and an effect with a state put on a player.
//! Fixture: one player beside the start, holding a decoration whose use names the placing effect.

use yalain_content::effect_rules::Placing;
use yalain_content::{Prop, Props};
use yalain_testkit::Harness;
use yalain_world::effect_defs::{EffectDef, Kind as EffectKind, Scale};
use yalain_world::things::Kind;

/// The decoration held, the effect its use names, and the template that effect places.
const DECORATION: u32 = 0x4700_0821;
const PLACE_TORCH: &str = "Eff_PlaceTorchFestival";
const TORCH: u32 = 0x4700_18CF;
/// An effect with a saved state that places the torch too.
const PLACE_BY_STATE: u32 = 0x6F00_0F01;

fn torch(lasts: Option<u32>) -> Placing {
    Placing {
        template: TORCH,
        offset: [0.0, 1.5],
        lasts,
        name: None,
    }
}

/// A world whose decoration's use places the torch, and a player holding one; the player's world
/// and character, and the decoration.
fn world(lasts: Option<u32>) -> (Harness, yalain_net::ConnId, u64, u64) {
    let mut h = Harness::new();
    let (_, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    h.server
        .effect_table
        .placings
        .insert(PLACE_TORCH.into(), torch(lasts));
    h.server.catalog.insert(yalain_testkit::test_template(
        TORCH,
        0x0777,
        Props::default(),
    ));
    let mut props = Props::default();
    props
        .set("Usage_Effect2", Prop::Text(PLACE_TORCH.into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(DECORATION, 0x0778, props));
    let world = *h.server.players.keys().next().expect("in");
    let item = yalain_world::inventory::give(&mut h.server, &mut h.net, world, DECORATION, 1)
        .expect("given");
    h.run(2);
    (h, world, me, item)
}

/// Sends a use of a thing, held or standing.
fn uses(h: &mut Harness, me: u64, item: u64) {
    use yalain_world::graph::{Builder, Slot};
    let mut use_of = Builder::new();
    use_of.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(me),
            Slot::Int((-1.0f32).to_bits()),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Long(0),
            Slot::Int(0),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Int(316),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    let arg = auberean_script::args::Arg::Object(use_of.graph());
    h.bots[0].send(
        true,
        auberean_protocol::queue::WEENIE,
        &yalain_testkit::play::server_call(0x8216_0016, &[arg]),
    );
    h.run(20);
}

/// The torches standing in the world.
fn torches(h: &Harness) -> Vec<u64> {
    let mut v: Vec<u64> = h
        .server
        .things
        .by_id
        .values()
        .filter(|t| t.kind == Kind::Placed && t.template == TORCH)
        .map(|t| t.id)
        .collect();
    v.sort_unstable();
    v
}

/// An item whose use names an effect a row says places a template, with no saved state of its own,
/// places it at the row's offset from its user as it faces, made for the user's client and no one's
/// to take or use; the item is spent.
/// Behaviour: server.effects.a-rule-places-what-an-effect-places
#[test]
fn an_items_use_places_what_its_effects_row_names_at_its_user() {
    let (mut h, world, me, item) = world(None);
    let at = h.server.players[&world].position;
    let from = h.bots[0].inbox.len();
    uses(&mut h, me, item);
    let placed = torches(&h);
    assert_eq!(placed.len(), 1, "one torch");
    let t = h.server.things.get(placed[0]).expect("standing");
    let got = t.at.expect("in the world");
    let (dx, dy) = (got.origin[0] - at.origin[0], got.origin[1] - at.origin[1]);
    assert!(
        ((dx * dx + dy * dy).sqrt() - 1.5).abs() < 0.05,
        "{got:?} from {at:?}"
    );
    assert!(t.container.is_none() && t.creature.is_none());
    assert!(
        h.world_messages(0, from).iter().any(|m| matches!(
            m,
            auberean_protocol::ServerMessage::CreateObject(o) if o.id == placed[0]
        )),
        "made for the client that sees it"
    );
    assert!(h.server.things.get(item).is_none(), "spent");
    // A use of it does nothing: it stays where it stands.
    uses(&mut h, me, placed[0]);
    assert_eq!(torches(&h), placed);
    assert!(h
        .server
        .things
        .get(placed[0])
        .is_some_and(|t| t.at.is_some()));
}

/// A placed object with a time of its own goes when its time is up, and not before.
/// Behaviour: server.effects.a-rule-places-what-an-effect-places
#[test]
fn a_placed_object_goes_when_its_time_is_up() {
    let (mut h, _, me, item) = world(Some(60));
    uses(&mut h, me, item);
    assert_eq!(torches(&h).len(), 1);
    h.step_by(50.0);
    h.run(2);
    assert_eq!(torches(&h).len(), 1, "within its time");
    h.step_by(15.0);
    h.run(2);
    assert!(torches(&h).is_empty(), "its time is up");
}

/// An item whose use would place what this world can't make is refused and kept.
/// Behaviour: server.makes.nothing-used-up-for-what-cannot-be-made
#[test]
fn an_item_whose_use_places_what_cannot_be_made_is_kept() {
    let (mut h, world, me, item) = world(None);
    h.server.catalog.templates.remove(&TORCH);
    uses(&mut h, me, item);
    assert!(torches(&h).is_empty());
    assert!(
        h.server.players[&world]
            .play
            .items
            .iter()
            .any(|i| i.id == item),
        "kept"
    );
}

/// An effect with a saved state that a row says places a template places it at the player it is
/// put on, wherever it is put on; on a creature it places nothing.
/// Behaviour: server.effects.a-rule-places-what-an-effect-places
#[test]
fn an_effect_with_a_state_places_its_rows_template_at_its_player() {
    let (mut h, world, _, _) = world(None);
    h.server.effect_table.effects.insert(
        PLACE_BY_STATE,
        EffectDef {
            state: PLACE_BY_STATE,
            kind: EffectKind::Places(torch(None)),
            duration: Scale(Vec::new()),
            priority: Scale(vec![(0.0, 1.0)]),
            fx: 0,
            external_flags: 0,
            internal_flags: 0,
            ends_at_logout: false,
            ends_at_teleport: false,
            enum_value: 0,
            eq_class: 0,
            name: None,
        },
    );
    let clock = h.clock();
    let cast = yalain_world::effects::Cast {
        spellcraft: 1.0,
        ..yalain_world::effects::Cast::default()
    };
    yalain_world::effects::apply(
        &mut h.server,
        &mut h.net,
        world,
        PLACE_BY_STATE,
        cast,
        clock,
    );
    h.run(2);
    assert_eq!(torches(&h).len(), 1);
}

/// What effects placed goes as a tally that clears its template resets.
/// Behaviour: server.tallies.a-full-count-starts-an-event-and-resets
#[test]
fn a_tally_that_clears_a_template_takes_what_was_placed_away_as_it_resets() {
    use yalain_content::tallies::{Reset, Tally, TallyTable};
    let (mut h, _, me, item) = world(None);
    h.server.content.tallies = TallyTable {
        tallies: vec![Tally {
            key: "decorations".into(),
            full: 20,
            starts: None,
            resets: Reset::Every(600),
            clears: vec![TORCH],
        }],
        ..TallyTable::default()
    };
    h.run(4);
    uses(&mut h, me, item);
    assert_eq!(torches(&h).len(), 1);
    let next = (h.unix.div_euclid(600) + 1) * 600;
    while h.unix <= next {
        h.step_by(1.0);
    }
    h.run(4);
    assert!(torches(&h).is_empty(), "cleared as it reset");
}

/// A rule that names what it places has it told to its player's client by that name, here new
/// prose, in place of its template's.
/// Behaviour: server.places.a-made-thing-takes-its-rows-name
#[test]
fn what_a_rule_places_is_told_by_the_rules_name() {
    let (mut h, _, me, item) = world(None);
    h.server.effect_table.placings.insert(
        PLACE_TORCH.into(),
        Placing {
            name: Some(yalain_content::Words::Literal("Festival Torch".into())),
            ..torch(None)
        },
    );
    let from = h.bots[0].inbox.len();
    uses(&mut h, me, item);
    let placed = torches(&h);
    assert_eq!(placed.len(), 1, "one torch");
    let told = h.world_messages(0, from).into_iter().find_map(|m| match m {
        auberean_protocol::ServerMessage::CreateObject(o) if o.id == placed[0] => Some(o.weenie),
        _ => None,
    });
    let name = told.expect("told to its client").name;
    let shown: Vec<u16> = "Festival Torch".encode_utf16().collect();
    assert_eq!(name.literal.map(|l| l.0), Some(shown));
}
