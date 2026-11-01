//! Selection and examination: the selected player's vitals, profiles.

use auberean_protocol::{queue, ServerMessage};
use auberean_script::args::Arg;
use auberean_script::native_object::NativeObject;
use yalain_content::Facts;
use yalain_testkit::play::server_call;
use yalain_testkit::Harness;
use yalain_world::calls::{self, Node, Value};
use yalain_world::examine::function as answered;
use yalain_world::examine::Vitals;
use yalain_world::graph::Builder;
use yalain_world::profile;
use yalain_world::requests::function;

/// Selecting another player is answered with that player's selection information: the selected
/// object and its health and vigor, current and greatest.
/// Behaviour: server.selection.vitals-of-the-selected
#[test]
fn selecting_a_player_is_answered_with_its_vitals() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::SET_SELECTION, &[Arg::LongInt(idb)]),
    );
    let values = h
        .wait_for(a, from, |m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == answered::SELECTION_INFO => {
                c.args.as_ref().and_then(|x| calls::values(&x.0).ok())
            }
            _ => None,
        })
        .expect("answered");
    assert_eq!(
        values.get(1),
        Some(&Value::Long(idb)),
        "the selected object"
    );
    let Some(Node::Native { ty: 171, bytes }) = values[0].root() else {
        panic!("a selection's information: {values:?}")
    };
    let words: Vec<u32> = bytes
        .chunks(4)
        .map(|c| u32::from_le_bytes(c.try_into().expect("words")))
        .collect();
    assert_eq!(words, [0x4000_0001, 100, 0, 100, 100, 0, 100]);
}

/// The bytes of an examination request about an object, asking for the tabs given.
fn about(object: u64, asked: u32) -> Arg {
    let mut bytes = Vec::new();
    for w in [0x4000_0002u32, 0] {
        bytes.extend(w.to_le_bytes());
    }
    bytes.extend(object.to_le_bytes());
    bytes.extend(asked.to_le_bytes());
    let mut b = Builder::new();
    b.native(154, NativeObject::Packed { ty: 154, bytes });
    Arg::Object(b.graph())
}

fn profile_nodes(h: &Harness, b: usize, from: usize) -> Vec<Vec<u8>> {
    h.world_messages(b, from)
        .into_iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == profile::EXAMINATION_PROFILE => {
                let v = calls::values(&c.args?.0).ok()?;
                match v[0].root() {
                    Some(Node::Native { ty: 153, bytes }) => Some(bytes.clone()),
                    _ => None,
                }
            }
            _ => None,
        })
        .collect()
}

/// Examining another player is answered with its profile: the request repeated, then its name,
/// craft title, race and sex, level, kingdom, health and vigor, kingdom points and quest rating;
/// a request to stop examining is not answered.
/// Behaviour: server.examination.player-profile
#[test]
fn examining_a_player_is_answered_with_its_profile_and_stopping_is_not_answered() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(idb, 0x203)]),
    );
    h.run(30);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "one profile");
    let bytes = &got[0];
    let request: Vec<u8> = [0x4000_0002u32, 0]
        .iter()
        .flat_map(|w| w.to_le_bytes())
        .chain(idb.to_le_bytes())
        .chain(0x203u32.to_le_bytes())
        .collect();
    assert_eq!(&bytes[..20], &request[..], "the request repeated");
    let want = profile::player(
        0x203,
        "Babyfare [M]",
        1,
        0x1000,
        1,
        Vitals {
            health: 100,
            max_health: 100,
            vigor: 100,
            max_vigor: 100,
            pk_damage: 0,
            pk_vigor_loss: 0,
        },
    );
    assert_eq!(bytes[..], want.packed(&request)[..]);
    let from = h.bots[a].inbox.len();
    let mut stop = Builder::new();
    stop.native(
        154,
        NativeObject::Packed {
            ty: 154,
            bytes: vec![0; 8],
        },
    );
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[Arg::Object(stop.graph())]),
    );
    h.run(30);
    assert!(
        profile_nodes(&h, a, from).is_empty(),
        "stopping is not answered"
    );
}

/// A profile keeps its lines in tab order, only for the tabs asked for and the general ones,
/// and notes every tab any line belongs to.
/// Behaviour: server.examination.lines-by-tab
#[test]
fn a_profile_keeps_lines_in_tab_order_for_the_tabs_asked_and_notes_every_tab() {
    let f = Facts {
        min_level: Some(15),
        ..Facts::default()
    };
    let name = profile::literal("Gateway");
    let all = profile::place(0x3FF, name.clone(), profile::line::CANNOT_RECALL_PORTAL, &f);
    let general = profile::place(0x203, name, profile::line::CANNOT_RECALL_PORTAL, &f);
    assert_eq!((all.orders, general.orders), (34, 34));
    assert!(
        general.nodes.len() < all.nodes.len(),
        "the requirements tab left out"
    );
    let orders: Vec<u32> = all
        .nodes
        .iter()
        .map(|n| match n {
            profile::Node::Int { order, .. }
            | profile::Node::Text { order, .. }
            | profile::Node::Break { order }
            | profile::Node::Tab { order, .. }
            | profile::Node::Icon { order, .. }
            | profile::Node::Image { order, .. }
            | profile::Node::Countdown { order, .. } => *order,
        })
        .collect();
    assert!(orders.windows(2).all(|w| w[0] <= w[1]), "{orders:?}");
}

/// Examining an item in the player's own pack is answered with the item's profile: its picture,
/// its name, its value, its description; a weapon's damage (from its least to its most), combat
/// delay and vigor cost on the stats tab; the races that may use it on the requirements tab. The
/// profile repeats the request.
/// Behaviour: server.examination.item-profiles-as-2017
#[test]
fn examining_an_item_in_the_pack_is_answered_with_its_profile() {
    use yalain_content::Prop;
    const BLADE: u32 = 0x4700_7001;
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Elara");
    let mut props = yalain_content::Props::default();
    props
        .set("Name", Prop::Str(0xA860_7CEC, 0x2500_7001))
        .set("Description", Prop::Str(0x6C45_9364, 0x2500_7001))
        .set("Icon", Prop::Word(0x4100_7001))
        .set("Item_Value", Prop::Int(12))
        .set("Weapon_Damage", Prop::Int(9))
        .set("Weapon_Variance", Prop::Float(0.24))
        .set("Weapon_Speed", Prop::Int(70))
        .set("Weapon_VigorCost", Prop::Int(4))
        .set("Usage_RequiredRace", Prop::Word(1));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BLADE, 0x1145, props));
    let item = h.arm(a, me, BLADE);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    let mut view =
        profile::ItemView::of(&h.server.catalog.get(BLADE).expect("made").props, 1, None);
    view.icon.clear();
    let mut request = Vec::new();
    for w in [0x4000_0002u32, 0] {
        request.extend(w.to_le_bytes());
    }
    request.extend(item.to_le_bytes());
    request.extend(0x3FFu32.to_le_bytes());
    let want = profile::item(0x3FF, &view);
    assert_eq!(got, vec![want.packed(&request)], "the item's profile");
    // Its lines: the picture, the name, the value, the description, the weapon's stats, the race.
    assert!(matches!(
        want.nodes[0],
        profile::Node::Image {
            picture: 0x4100_7001,
            ..
        }
    ));
    assert_eq!(want.orders, 0x2A, "general, stats and requirements");
    let damage = want.nodes.iter().find_map(|n| match n {
        profile::Node::Text { text, .. } if text.string_id == profile::item_line::DAMAGE_VALUE => {
            Some(text.variables.iter().map(|v| v.id).collect::<Vec<_>>())
        }
        _ => None,
    });
    assert!(damage.is_some(), "the damage line");
}

/// A blade examined: its maker named after its description, its durability first on the stats
/// tab, and the effect it puts on whoever wields it on the effects tab after a break, unindented
/// (a weapon's): the heading with how many, the effect's name and description from its own table,
/// and the spellcraft it is cast with; its name drawn as a made item's.
/// Behaviour: server.examination.item-maker-durability-effects
#[test]
fn an_item_names_its_maker_its_durability_and_its_effects() {
    use profile::{item_line as l, look, Node};
    use yalain_content::Prop;
    const BLADE: u32 = 0x4700_7002;
    const EFFECT_TABLE: u32 = 0x2500_7102;
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Elara");
    let mut props = yalain_content::Props::default();
    props
        .set("Name", Prop::Str(l::NAME, 0x2500_7002))
        .set("Item_Value", Prop::Int(12))
        .set("Weapon_Damage", Prop::Int(9))
        .set("Item_Durability_MaxLevel", Prop::Int(50))
        .set("Item_Durability_CurrentLevel", Prop::Int(48));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(BLADE, 0x1145, props));
    h.server.item_effects.insert(
        BLADE,
        vec![profile::ItemEffect {
            when: profile::When::Equipped,
            table: EFFECT_TABLE,
            spellcraft: Some(50),
        }],
    );
    let item = h.arm(a, me, BLADE);
    if let Some(t) = h.server.things.get_mut(item) {
        t.crafter = Some("Ravel".into());
    }
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "answered once");
    let mut view =
        profile::ItemView::of(&h.server.catalog.get(BLADE).expect("made").props, 1, None);
    view.crafter = Some("Ravel".into());
    view.effects = h.server.item_effects[&BLADE].clone();
    let p = profile::item(0x3FF, &view);
    assert_eq!(got[0][20..], p.packed(&got[0][..20])[20..], "the profile");
    let texts: Vec<(u32, u32, u32)> = p
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text { order, look, text } => Some((*order, *look, text.string_id)),
            _ => None,
        })
        .collect();
    assert_eq!(
        texts[1],
        (profile::GENERAL, profile::name_look::CRAFTED, l::NAME),
        "a made item's name"
    );
    let maker = texts
        .iter()
        .position(|t| t.2 == l::CRAFTED_BY)
        .expect("its maker");
    let value = texts
        .iter()
        .position(|t| t.2 == l::VALUE)
        .expect("its value");
    assert_eq!(maker, value + 2, "after the description");
    let stats: Vec<u32> = texts
        .iter()
        .filter(|t| t.0 == profile::STATS)
        .map(|t| t.2)
        .collect();
    assert_eq!(
        stats[..2],
        [l::DURABILITY, l::DURABILITY_VALUE],
        "durability first"
    );
    let effects: Vec<&Node> = p
        .nodes
        .iter()
        .filter(|n| match n {
            Node::Text { order, .. } | Node::Int { order, .. } | Node::Tab { order, .. } => {
                *order == profile::EFFECTS
            }
            Node::Break { order } => *order == profile::EFFECTS,
            _ => false,
        })
        .collect();
    assert!(matches!(effects[0], Node::Break { .. }), "after a break");
    assert!(
        matches!(effects[1], Node::Tab { count: 0, .. }),
        "unindented"
    );
    assert!(
        matches!(effects[2], Node::Text { look: look::LABEL, text, .. } if text.string_id == l::EFFECTS_WIELDED && text.variables[0].value == auberean_script::native_object::VariableValue::Word(1))
    );
    let lines: Vec<(u32, u32)> = effects
        .iter()
        .filter_map(|n| match n {
            Node::Text { text, .. } => Some((text.table, text.string_id)),
            _ => None,
        })
        .collect();
    assert_eq!(
        lines[1..],
        [
            (0x2500_0011, l::EFFECT_NAME),
            (EFFECT_TABLE, l::DESCRIPTION),
            (0x2500_0011, l::SPELLCRAFT)
        ]
    );
    assert!(
        effects
            .iter()
            .any(|n| matches!(n, Node::Int { value: 50, .. })),
        "cast at 50"
    );
}

/// An effect's spellcraft shows unless the effect happens at once or changes qualities by the
/// same amount whatever the spellcraft; an effect the client never sees is not listed.
/// Behaviour: server.examination.item-effect-spellcraft-shown
#[test]
fn an_effects_spellcraft_shows_unless_it_happens_at_once_or_never_varies() {
    use yalain_world::effect_defs::{EffectDef, Kind, Scale};
    let def = |kind: Kind, internal: u32| EffectDef {
        state: 1,
        kind,
        duration: Scale::default(),
        priority: Scale::default(),
        fx: 0,
        external_flags: 0,
        internal_flags: internal,
        enum_value: 0,
        eq_class: 0,
        name: Some((0xA860_7CEC, 0x2500_7103)),
    };
    let numeric = |m: Vec<(f32, f32)>| Kind::Numeric {
        flags: 5,
        stats: vec![],
        magnitude: Scale(m),
    };
    let shown = |d: EffectDef| profile::EffectShown::of(&d).expect("named");
    assert!(shown(def(numeric(vec![(0.0, 40.0), (100.0, 50.0)]), 0x0140_0713)).spellcraft_shown);
    assert!(!shown(def(numeric(vec![(0.0, 2.3)]), 0x0140_2713)).spellcraft_shown);
    assert!(!shown(def(Kind::Other, 0x4140_0513)).spellcraft_shown);
    assert!(shown(def(Kind::Other, 0x0940_0783)).spellcraft_shown);
    assert!(!shown(def(Kind::Other, 0x0140_0782)).visible);
    let mut props = yalain_content::Props::default();
    props
        .set("Usage_Effect1", yalain_content::Prop::Text("Seen".into()))
        .set(
            "Usage_Effect1_Spellcraft",
            yalain_content::Prop::Float(100.0),
        )
        .set("Usage_Effect2", yalain_content::Prop::Text("Unseen".into()));
    let list = profile::item_effects(&props, &mut |n| {
        Some(shown(def(
            Kind::Other,
            if n == "Seen" {
                0x0940_0783
            } else {
                0x0140_0782
            },
        )))
    });
    assert_eq!(
        list,
        [profile::ItemEffect {
            when: profile::When::Used,
            table: 0x2500_7103,
            spellcraft: Some(100)
        }]
    );
}

/// While a player examines an item, the item is looked at again every 3.5 s and its profile sent
/// again only when it changed; a stop sent with the request does not end that, a stop sent later
/// does.
/// Behaviour: server.examination.looked-at-again-while-examined
#[test]
fn an_examined_item_is_sent_again_when_it_changes_until_a_later_stop() {
    use yalain_testkit::STEP;
    const STACK: u32 = 0x4700_7004;
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Elara");
    let mut props = yalain_content::Props::default();
    props.set("Item_Value", yalain_content::Prop::Int(3));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACK, 0x1145, props));
    let item = h.arm(a, me, STACK);
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let period = (yalain_world::examine::AGAIN_SECONDS / STEP).ceil() as usize + 2;
    let from = h.bots[a].inbox.len();
    // The request and a stop sent with it (the client stopping its earlier examination).
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[stop()]),
    );
    h.run(period);
    assert_eq!(
        profile_nodes(&h, a, from).len(),
        1,
        "answered, not sent again unchanged"
    );
    if let Some(t) = h.server.things.get_mut(item) {
        t.quantity = 5;
    }
    let from = h.bots[a].inbox.len();
    h.run(period);
    assert_eq!(
        profile_nodes(&h, a, from).len(),
        1,
        "sent again once it changed"
    );
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[stop()]),
    );
    h.run(2);
    if let Some(t) = h.server.things.get_mut(item) {
        t.quantity = 7;
    }
    h.run(period * 2);
    assert!(
        profile_nodes(&h, a, from).is_empty(),
        "nothing after a later stop"
    );
}

/// A request to stop examining.
fn stop() -> Arg {
    let mut b = Builder::new();
    b.native(
        154,
        NativeObject::Packed {
            ty: 154,
            bytes: vec![0; 8],
        },
    );
    Arg::Object(b.graph())
}

/// A made item kept in the pack names its maker again after its holder logs out and back in.
/// Behaviour: server.examination.item-maker-kept-across-sessions
#[test]
fn a_made_items_maker_is_named_again_after_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
    use profile::{item_line as l, Node};
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, me) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    if let Some(t) = h.server.things.get_mut(item) {
        t.crafter = Some("Ravel".into());
    }
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    assert!(h.server.things.get(item).is_none(), "out of the world");
    h.enter_world(a, "one", me);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "answered once");
    let mut view = profile::ItemView::of(
        &h.server
            .catalog
            .get(yalain_testkit::TEST_ITEM)
            .expect("made")
            .props,
        1,
        None,
    );
    view.crafter = Some("Ravel".into());
    let p = profile::item(0x3FF, &view);
    assert!(
        p.nodes
            .iter()
            .any(|n| matches!(n, Node::Text { text, .. } if text.string_id == l::CRAFTED_BY)),
        "the expected profile names a maker"
    );
    assert_eq!(
        got[0][20..],
        p.packed(&got[0][..20])[20..],
        "its maker named"
    );
}

/// An item whose kind is destroyed on use (a key, a potion) says so on its requirements tab even
/// when its template does not; the kind is the nearest of the classes that set it the item's class
/// derives from (a coin, though an item, is not).
/// Behaviour: server.examination.destroyed-on-use-by-kind
#[test]
fn a_potions_profile_says_it_is_destroyed_on_use_by_its_kind() {
    use profile::{item_line as l, Node};
    // A made-up class tree: a potion kind under the potions, a coin beside them.
    let id_of = |n: &str| match n {
        "PotionTemplate" => Some(0x10),
        "Coin" => Some(0x20),
        _ => None,
    };
    let derives = |a: u32, b: u32| a == b || (a == 0x11 && b == 0x10) || (a == 0x21 && b == 0x20);
    assert!(
        profile::destroyed_on_use_kind(0x11, id_of, derives),
        "a potion"
    );
    assert!(
        !profile::destroyed_on_use_kind(0x21, id_of, derives),
        "a coin"
    );
    assert!(
        !profile::destroyed_on_use_kind(0x30, id_of, derives),
        "anything else"
    );

    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let package = h
        .server
        .catalog
        .get(yalain_testkit::TEST_ITEM)
        .expect("in the catalog")
        .description
        .package;
    h.server.destroyed_on_use.insert(package);
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "answered once");
    let mut view = profile::ItemView::of(
        &h.server
            .catalog
            .get(yalain_testkit::TEST_ITEM)
            .expect("made")
            .props,
        1,
        None,
    );
    view.destroyed_on_use = true;
    let p = profile::item(0x3FF, &view);
    assert!(
        p.nodes
            .iter()
            .any(|n| matches!(n, Node::Text { text, .. } if text.string_id == l::DESTROYED_ON_USE)),
        "the expected profile says so"
    );
    assert_eq!(
        got[0][20..],
        p.packed(&got[0][..20])[20..],
        "destroyed on use"
    );
}

/// An item that takes a skill and Arcane Lore says so under its requirements: "Must have skills:"
/// and each skill by its own name, then the Arcane Lore needed.
/// Behaviour: server.examination.skills-needed-listed
#[test]
fn an_item_lists_the_skills_and_arcane_lore_it_takes() {
    use profile::{item_line as l, Node};
    use yalain_content::Prop;
    const HAMMER: u32 = 0x4700_7201;
    const SKILL: u32 = 731;
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Elara");
    h.server.skill_table.skills.insert(
        SKILL,
        yalain_world::skills::SkillDef {
            id: SKILL,
            name: (0xA860_7CEC, 0x2500_0566),
            ..Default::default()
        },
    );
    let mut props = yalain_content::Props::default();
    props
        .set("Item_Value", Prop::Int(3))
        .set("Usage_RequiredSkill1", Prop::Word(SKILL))
        .set("Usage_RequiredArcaneLore", Prop::Int(40));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(HAMMER, 0x200, props));
    let item = h.arm(a, me, HAMMER);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "answered once");
    let mut view =
        profile::ItemView::of(&h.server.catalog.get(HAMMER).expect("made").props, 1, None);
    assert_eq!(view.arcane_lore, 40, "the Arcane Lore from its template");
    view.skills = vec![profile::string(0x2500_0566, 0xA860_7CEC)];
    let p = profile::item(0x3FF, &view);
    let texts: Vec<u32> = p
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text { order, text, .. } if *order == profile::REQUIREMENTS => {
                Some(text.string_id)
            }
            _ => None,
        })
        .collect();
    let at = texts
        .iter()
        .position(|t| *t == l::SKILLS_NEEDED)
        .expect("skills");
    assert_eq!(texts[at + 1], 0xA860_7CEC, "the skill by its name");
    assert_eq!(texts[at + 2], l::ARCANE_LORE, "then the Arcane Lore");
    // The layout: the label, a break, each skill twice indented and broken.
    let shape: Vec<&str> = p
        .nodes
        .iter()
        .filter_map(|n| match n {
            Node::Text { order, .. } if *order == profile::REQUIREMENTS => Some("text"),
            Node::Int { order, .. } if *order == profile::REQUIREMENTS => Some("int"),
            Node::Break { order } if *order == profile::REQUIREMENTS => Some("break"),
            Node::Tab { order, .. } if *order == profile::REQUIREMENTS => Some("tab"),
            _ => None,
        })
        .collect();
    let end = shape.len();
    assert_eq!(
        shape[end - 11..],
        ["tab", "text", "break", "tab", "tab", "text", "break", "tab", "text", "int", "break"],
        "skills, then the Arcane Lore"
    );
    assert_eq!(got[0][20..], p.packed(&got[0][..20])[20..], "as built");
}
