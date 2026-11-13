//! Portals an item's use makes: a world's row names, for the item's use effect, a portal template,
//! and the portal stands before the user, leads where its template's destination names, and lets
//! in anyone its template and row are for while it stands. Fixture: made-up gem and portal
//! templates, a place named for the portal, two players.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use yalain_content::{DeathSpawn, Prop, Props};
use yalain_net::ConnId;
use yalain_store::StoredPosition;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::effect_defs::{EffectDef, Kind, Scale};
use yalain_world::graph::{Builder, Slot};
use yalain_world::item_use::{refusal, UsageEffect};
use yalain_world::places::kind;

const GEM: u32 = 0x4700_9101;
const PORTAL: u32 = 0x4700_9102;
const EFFECT: &str = "Eff_OpensTheLair";
const DESTINATION: &str = "Lair_Landing";
const LUCK: u32 = 0x6F00_0321;
/// Two kingdoms' numbers, as a description's faction names them.
const SHADOW: u32 = 1;
const ORDER: u32 = 2;

/// Where the portal leads: another landblock's.
const LANDING: StoredPosition = StoredPosition {
    cell: 0x7000_0001,
    origin: [10.0, 10.0, 50.0],
    rotation: [1.0, 0.0, 0.0, 0.0],
};

/// The portal's template: it leads to [`DESTINATION`], may be recalled to, is for heroes alone when
/// `hero`, and puts [`LUCK`] on whoever comes through.
fn portal_template(h: &mut Harness, hero: bool) {
    let mut props = Props::default();
    props
        .set("PortalDestination", Prop::Text(DESTINATION.into()))
        .set("PortalFlags", Prop::Word(0x1000_0001))
        .set("Usage_HeroOnly", Prop::Bool(hero));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PORTAL, 0xF12, props));
    h.server.item_uses.insert(
        PORTAL,
        vec![UsageEffect {
            state: LUCK,
            spellcraft: 1.0,
        }],
    );
    h.server.effect_table.effects.insert(
        LUCK,
        EffectDef {
            state: LUCK,
            kind: Kind::Other,
            duration: Scale(vec![(0.0, 600.0)]),
            priority: Scale(vec![(0.0, 1.0)]),
            fx: 0,
            external_flags: 0,
            internal_flags: 0x1100_0003,
            ends_at_logout: false,
            ends_at_teleport: false,
            enum_value: 0,
            eq_class: 4175,
            name: None,
        },
    );
}

/// A world with the gem, its row and the portal's template, the place the world names for the
/// portal when `placed`, and a player holding a gem; returns the harness, the player's client, its
/// character and its world connection, and the gem.
fn world(
    hero: bool,
    placed: bool,
    lasts: Option<f32>,
    kingdom: Option<u32>,
) -> (Harness, usize, u64, ConnId, u64) {
    let mut h = Harness::new();
    portal_template(&mut h, hero);
    if placed {
        h.server.places_named.insert(
            PORTAL,
            yalain_content::Place {
                cell: LANDING.cell,
                origin: LANDING.origin,
                rotation: LANDING.rotation,
            },
        );
    }
    h.server.death_spawns.insert(
        EFFECT.into(),
        DeathSpawn {
            template: PORTAL,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts,
            kingdom,
            at: None,
            to: None,
        },
    );
    let mut props = Props::default();
    props
        .set("Usage_Effect1", Prop::Text(EFFECT.into()))
        .set("Item_IsDestroyOnUse", Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(GEM, 0xF16, props));
    let (b, me) = h.player_in_world("one", "Kestrel");
    h.run(20);
    let world = world_of(&h, me);
    let gem =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, GEM, 1).expect("given");
    h.run(2);
    (h, b, me, world, gem)
}

fn world_of(h: &Harness, me: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == me)
        .expect("in the world")
        .0
}

/// A use as the client asks for it.
fn use_of(item: u64, player: u64) -> Arg {
    let mut b = Builder::new();
    b.object(
        0x245,
        &[
            Slot::Ref(None),
            Slot::Ref(None),
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Int(0),
            Slot::Long(player),
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
    Arg::Object(b.graph())
}

/// A client asks to use something; returns where its inbox stood.
fn uses(h: &mut Harness, b: usize, me: u64, item: u64) -> usize {
    let from = h.bots[b].inbox.len();
    h.bots[b].send(
        true,
        queue::WEENIE,
        &server_call(yalain_world::requests::function::USE, &[use_of(item, me)]),
    );
    h.run(20);
    from
}

/// The portals standing in the world, by id.
fn portals(h: &Harness) -> Vec<u64> {
    h.server
        .things
        .by_id
        .values()
        .filter(|t| t.template == PORTAL && t.kind == yalain_world::things::Kind::Portal)
        .map(|t| t.id)
        .collect()
}

fn position_of(h: &Harness, me: u64) -> StoredPosition {
    h.server.players[&world_of(h, me)].position
}

fn holds(h: &Harness, me: u64, item: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .items
        .iter()
        .any(|i| i.id == item)
}

fn lucky(h: &Harness, me: u64) -> bool {
    h.server.players[&world_of(h, me)]
        .play
        .effects
        .iter()
        .any(|e| e.state == LUCK)
}

fn run_for(h: &mut Harness, seconds: f64) {
    let t = h.now.0 + seconds;
    while h.now.0 < t {
        h.step();
    }
}

/// A gem whose use names a row's portal makes it 2 m straight ahead of its user at its height,
/// facing its way, told to its client, leading where the world names for its template; the gem is
/// spent; and with no time on its row it still stands an hour later.
/// Behaviour: server.places.an-items-use-makes-a-portal
#[test]
fn a_gems_use_makes_its_portal_two_meters_ahead_leading_where_its_template_names() {
    let (mut h, b, me, _, gem) = world(false, true, None, None);
    let at = position_of(&h, me);
    let from = uses(&mut h, b, me, gem);
    let made = portals(&h);
    assert_eq!(made.len(), 1, "one portal");
    let portal = h.server.things.get(made[0]).expect("it");
    let stands = portal.at.expect("standing");
    // The way its user faces: a turn about the vertical from north.
    let turn = 2.0 * dereth_primitives::num::math::atan2f(at.rotation[3], at.rotation[0]);
    let (fx, fy) = (
        -dereth_primitives::num::math::sinf(turn),
        dereth_primitives::num::math::cosf(turn),
    );
    let (dx, dy) = (
        stands.origin[0] - at.origin[0],
        stands.origin[1] - at.origin[1],
    );
    assert!(
        (dx * fx + dy * fy - 2.0).abs() < 0.01 && (dx * fy - dy * fx).abs() < 0.01,
        "2 m straight ahead: {stands:?} from {at:?}"
    );
    assert!(
        (stands.origin[2] - at.origin[2]).abs() < 0.01,
        "at its height"
    );
    assert_eq!(stands.rotation, at.rotation, "facing its way");
    let to = portal.destination.expect("it leads somewhere");
    assert_eq!((to.cell, to.origin), (LANDING.cell, LANDING.origin));
    let told = h.world_messages(b, from).iter().any(|m| {
        matches!(m, ServerMessage::CreateObject(o) if o.id == made[0] && o.weenie.entity_did == PORTAL)
    });
    assert!(told, "its user sees it");
    assert!(!holds(&h, me, gem), "the gem spent");
    run_for(&mut h, 3600.0);
    assert_eq!(portals(&h), made, "standing an hour on");
}

/// A row that gives a portal its time: there still a second before it, gone a second after, told
/// to those who see it.
/// Behaviour: server.places.a-made-portal-stands-its-rows-time
#[test]
fn a_made_portal_goes_when_its_rows_time_is_up() {
    let (mut h, b, me, _, gem) = world(false, true, Some(60.0), None);
    let from = uses(&mut h, b, me, gem);
    let made = portals(&h);
    assert_eq!(made.len(), 1);
    run_for(&mut h, 58.0);
    assert_eq!(portals(&h), made, "still there before its time");
    run_for(&mut h, 2.0);
    assert!(portals(&h).is_empty(), "gone after its time");
    let forgot = h
        .world_messages(b, from)
        .iter()
        .any(|m| matches!(m, ServerMessage::ForgetObject(o) if o.0.id == made[0]));
    assert!(forgot, "those who see it are told");
}

/// Another player uses the portal its maker made: through portal space to where it leads, its
/// template's effects on that player as it comes out, and a place to recall to there, as its
/// template's flags say; the maker may go after.
/// Behaviour: server.places.a-made-portal-lets-anyone-in
#[test]
fn anyone_may_use_a_made_portal_and_its_templates_effects_come_on_them() {
    let (mut h, a, me, _, gem) = world(false, true, None, None);
    uses(&mut h, a, me, gem);
    let portal = portals(&h)[0];
    let (b, other) = h.player_in_world("two", "Tysis");
    h.run(10);
    uses(&mut h, b, other, portal);
    h.run(60);
    let there = position_of(&h, other);
    assert_eq!((there.cell, there.origin), (LANDING.cell, LANDING.origin));
    assert!(lucky(&h, other), "its template's effect on the user");
    assert!(!lucky(&h, me), "not on its maker");
    let recall = h.server.players[&world_of(&h, other)]
        .places
        .iter()
        .find(|(k, _)| *k == kind::LAST_PORTAL)
        .map(|(_, at)| at.cell);
    assert_eq!(recall, Some(LANDING.cell), "recallable");
    uses(&mut h, a, me, portal);
    h.run(60);
    assert_eq!(position_of(&h, me).cell, LANDING.cell, "its maker after");
}

/// Arriving through a made portal moves the quests whose steps its template's arrival triggers, as
/// arriving through a placed portal of its template does.
/// Behaviour: server.places.a-made-portal-lets-anyone-in
#[test]
fn arriving_through_a_made_portal_moves_the_quests_of_its_templates_arrival() {
    use yalain_content::quests::{Trigger, Update};
    let (mut h, b, me, world, gem) = world(false, true, None, None);
    let quest = super::kill_quests::quest(1);
    let id = quest.id;
    h.server.quests.quests.push(quest);
    h.server.quests.steps.push(super::quests::step(
        id,
        0,
        Trigger::Arrive(PORTAL),
        Update::Bestow,
    ));
    uses(&mut h, b, me, gem);
    let portal = portals(&h)[0];
    assert!(h.server.players[&world].play.quests.is_empty());
    uses(&mut h, b, me, portal);
    h.run(60);
    let given = h.server.players[&world]
        .play
        .quests
        .iter()
        .any(|q| q.quest == id && q.status == yalain_world::quests::status::UNDERWAY);
    assert!(given, "{:?}", h.server.players[&world].play.quests);
}

/// A made portal keeps out whom its template and row keep out, refused at once where the player
/// stands: anyone but a hero when its template is for heroes alone (10049), anyone of another
/// kingdom than its row's (10011); those it is for go through.
/// Behaviour: server.places.a-made-portal-keeps-out
#[test]
fn a_made_portal_refuses_those_its_template_and_row_keep_out() {
    let (mut h, a, me, world, gem) = world(true, true, None, Some(SHADOW));
    {
        let p = h.server.players.get_mut(&world).expect("in");
        yalain_world::focus::set_hero_kept(&mut p.qualities, true);
        yalain_world::kingdoms::set_kingdom(&mut p.qualities, SHADOW);
    }
    uses(&mut h, a, me, gem);
    let portal = portals(&h)[0];
    let (b, other) = h.player_in_world("two", "Tysis");
    h.run(10);
    let w = world_of(&h, other);
    yalain_world::kingdoms::set_kingdom(
        &mut h.server.players.get_mut(&w).expect("in").qualities,
        SHADOW,
    );
    // Far from it: refused where it stands, not walked there first.
    {
        let p = h.server.players.get_mut(&w).expect("in");
        p.position.origin[1] += 20.0;
    }
    let before = position_of(&h, other);
    let from = uses(&mut h, b, other, portal);
    let statuses: Vec<u32> = super::items::answers(&h, b, from)
        .iter()
        .map(|x| x.status)
        .collect();
    assert_eq!(statuses, [refusal::HERO_ONLY], "no hero");
    {
        let p = h.server.players.get_mut(&w).expect("in");
        yalain_world::focus::set_hero_kept(&mut p.qualities, true);
        yalain_world::kingdoms::set_kingdom(&mut p.qualities, ORDER);
    }
    let from = uses(&mut h, b, other, portal);
    let statuses: Vec<u32> = super::items::answers(&h, b, from)
        .iter()
        .map(|x| x.status)
        .collect();
    assert_eq!(statuses, [refusal::WRONG_KINGDOM], "of another kingdom");
    h.run(60);
    let kept = position_of(&h, other);
    assert_eq!(
        (kept.cell, kept.origin),
        (before.cell, before.origin),
        "kept out"
    );
    assert!(!lucky(&h, other));
    h.server.players.get_mut(&w).expect("in").position.origin[1] -= 20.0;
    yalain_world::kingdoms::set_kingdom(
        &mut h.server.players.get_mut(&w).expect("in").qualities,
        SHADOW,
    );
    uses(&mut h, b, other, portal);
    h.run(60);
    assert_eq!(
        position_of(&h, other).cell,
        LANDING.cell,
        "a hero of its kingdom goes"
    );
}

/// A made portal whose row names no kingdom keeps the one its template requires: anyone of another
/// kingdom, or of none, is refused at once (10011); one of its kingdom goes through.
/// Behaviour: server.places.a-made-portal-keeps-its-templates-kingdom
#[test]
fn a_made_portal_whose_row_names_no_kingdom_is_for_the_kingdom_its_template_requires() {
    let (mut h, a, me, _, gem) = world(false, true, None, None);
    let mut t = h
        .server
        .catalog
        .get(PORTAL)
        .cloned()
        .expect("the portal's template");
    t.props.set("Usage_RequiredFaction", Prop::Word(ORDER));
    h.server.catalog.insert(t);
    uses(&mut h, a, me, gem);
    let portal = portals(&h)[0];
    let (b, other) = h.player_in_world("two", "Tysis");
    h.run(10);
    let w = world_of(&h, other);
    let before = position_of(&h, other);
    for (kingdom, why) in [
        (None, "of no kingdom"),
        (Some(SHADOW), "of another kingdom"),
    ] {
        if let Some(k) = kingdom {
            yalain_world::kingdoms::set_kingdom(
                &mut h.server.players.get_mut(&w).expect("in").qualities,
                k,
            );
        }
        let from = uses(&mut h, b, other, portal);
        let statuses: Vec<u32> = super::items::answers(&h, b, from)
            .iter()
            .map(|x| x.status)
            .collect();
        assert_eq!(statuses, [refusal::WRONG_KINGDOM], "{why}");
    }
    h.run(60);
    let kept = position_of(&h, other);
    assert_eq!(
        (kept.cell, kept.origin),
        (before.cell, before.origin),
        "kept out"
    );
    yalain_world::kingdoms::set_kingdom(
        &mut h.server.players.get_mut(&w).expect("in").qualities,
        ORDER,
    );
    uses(&mut h, b, other, portal);
    h.run(60);
    assert_eq!(
        position_of(&h, other).cell,
        LANDING.cell,
        "one of its kingdom goes"
    );
}

/// A made portal keeps out, as a placed portal of its template would, a player below the
/// template's least level (10009) and one without the quest the template needs (10019); one who
/// has both goes through.
/// Behaviour: server.places.a-made-portal-keeps-out
#[test]
fn a_made_portal_refuses_those_below_its_least_level_or_without_its_quest() {
    const LEVEL: u32 = 302;
    let (mut h, b, me, world, gem) = world(false, true, None, None);
    let quest = super::kill_quests::quest(1);
    let id = quest.id;
    h.server.quests.quests.push(quest);
    let mut props = h
        .server
        .catalog
        .get(PORTAL)
        .expect("its template")
        .props
        .clone();
    props
        .set("Usage_MinLevel", Prop::Int(20))
        .set("Usage_RequiredQuest", Prop::Word(id));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(PORTAL, 0xF12, props));
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).expect("in").qualities,
        LEVEL,
        10,
    );
    uses(&mut h, b, me, gem);
    let portal = portals(&h)[0];
    let refused = |h: &mut Harness| -> Vec<u32> {
        let from = uses(h, b, me, portal);
        super::items::answers(h, b, from)
            .iter()
            .map(|x| x.status)
            .collect()
    };
    assert_eq!(refused(&mut h), [refusal::LEVEL_TOO_LOW], "below its level");
    yalain_world::quals::set_int(
        &mut h.server.players.get_mut(&world).expect("in").qualities,
        LEVEL,
        30,
    );
    assert_eq!(
        refused(&mut h),
        [refusal::LACKING_QUEST],
        "without its quest"
    );
    h.run(60);
    assert_ne!(position_of(&h, me).cell, LANDING.cell, "kept out");
    h.server.players.get_mut(&world).expect("in").play.quests =
        vec![yalain_world::state::QuestState {
            quest: id,
            status: yalain_world::quests::status::COMPLETED,
            phase: 0,
            started: 0,
            updated: 0,
        }];
    uses(&mut h, b, me, portal);
    h.run(60);
    assert_eq!(position_of(&h, me).cell, LANDING.cell, "both: through");
}

/// A gem whose portal leads to no place this world holds makes nothing: the use is refused as a
/// summoning that fails, and the gem is kept. A place of its template's destination's name, in any
/// case, is where it leads.
/// Behaviour: server.places.a-made-portal-leads-nowhere
#[test]
fn a_gem_whose_portal_leads_nowhere_here_is_refused_and_kept() {
    let (mut h, b, me, _, gem) = world(false, false, None, None);
    let from = uses(&mut h, b, me, gem);
    assert!(portals(&h).is_empty(), "nothing made");
    assert!(holds(&h, me, gem), "kept");
    assert!(
        h.texts(b, from)
            .iter()
            .any(|(_, t)| *t == yalain_world::pets::text::SUMMONING_FAILS.0),
        "told"
    );
    h.server.places_by_name.insert(
        DESTINATION.to_ascii_lowercase(),
        yalain_content::Place {
            cell: LANDING.cell,
            origin: LANDING.origin,
            rotation: LANDING.rotation,
        },
    );
    uses(&mut h, b, me, gem);
    let made = portals(&h);
    assert_eq!(made.len(), 1, "made once its place is known");
    let to = h.server.things.get(made[0]).and_then(|t| t.destination);
    assert_eq!(to.map(|p| p.cell), Some(LANDING.cell));
    assert!(!holds(&h, me, gem), "spent");
}
