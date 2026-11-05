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

/// Selecting a person whose template names no health or vigor is answered with a person's own
/// numbers, so the meters under its name show full rather than empty; a template that names them
/// keeps them.
///
/// Behaviour: server.selection.a-person-with-no-vitals-of-its-own-has-30-and-130
#[test]
fn a_person_with_no_vitals_of_its_own_has_30_and_130() {
    use yalain_content::{ContentObject, ContentPack, Description, Kind};
    let mut h = Harness::new();
    let (a, id) = h.player_in_world("one", "Elara");
    let at = h
        .server
        .players
        .values()
        .find(|p| p.character == id)
        .expect("in the world")
        .position;
    let person = |object: u64, facts: Facts| ContentObject {
        object: Some(object),
        template: 0x4700_2F80,
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
        facts,
        walks: None,
        gate: None,
    };
    let (plain, tough) = (0x40DF_8A00_0000_001E, 0x40DF_8A00_0000_001F);
    h.server.content = ContentPack {
        objects: vec![
            person(plain, Facts::default()),
            person(
                tough,
                Facts {
                    max_health: Some(500),
                    max_vigor: Some(600),
                    ..Facts::default()
                },
            ),
        ],
        ..ContentPack::default()
    };
    let mut words_of = |who: u64| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::SET_SELECTION, &[Arg::LongInt(who)]),
        );
        let values = h
            .wait_for(a, from, |m| match m {
                ServerMessage::PlayerCall(c) if c.function.0 == answered::SELECTION_INFO => {
                    c.args.as_ref().and_then(|x| calls::values(&x.0).ok())
                }
                _ => None,
            })
            .expect("answered");
        let Some(Node::Native { ty: 171, bytes }) = values[0].root() else {
            panic!("a selection's information: {values:?}")
        };
        bytes
            .chunks(4)
            .map(|c| u32::from_le_bytes(c.try_into().expect("words")))
            .collect::<Vec<u32>>()
    };
    assert_eq!(words_of(plain), [0x4000_0001, 30, 0, 30, 130, 0, 130]);
    assert_eq!(words_of(tough), [0x4000_0001, 500, 0, 500, 600, 0, 600]);
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
        None,
        Vitals {
            health: 100,
            max_health: 100,
            vigor: 100,
            max_vigor: 100,
            pk_damage: 0,
            pk_vigor_loss: 0,
        },
        0,
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

/// A placed object for monarchs alone says so on its requirements tab, as 2017's Allegiance Sign's
/// profile did word for word.
/// Behaviour: server.examination.monarch-only-object
#[test]
fn an_object_for_monarchs_alone_says_so_when_examined() {
    const SIGN: u64 = 0x400D_FA00_0000_0003;
    const TEMPLATE: u32 = 0x4700_11C4;
    let build = |template: u32, name: Option<(u32, u32)>| yalain_content::ContentObject {
        object: Some(SIGN),
        template,
        description: yalain_content::Description {
            name_text: None,
            package: 0x13C1,
            look: 0x1F00_0ED5,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind: yalain_content::Kind::Static,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: yalain_content::Facts {
            name,
            description: Some((0x6C45_9364, 0x2500_1E91)),
            ..yalain_content::Facts::default()
        },
        walks: None,
        gate: None,
    };
    let mut h = Harness::new();
    let mut props = yalain_content::Props::default();
    props.set("Usage_MonarchOnly", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(TEMPLATE, 0x13C1, props));
    h.server.content = yalain_content::ContentPack {
        objects: vec![build(TEMPLATE, Some((0xA860_7CEC, 0x2500_1E91)))],
        ..yalain_content::ContentPack::default()
    };
    let (a, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(SIGN, 0x3FF)]),
    );
    h.run(30);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "one profile");
    let recorded: [u32; 49] = [
        0x0000_000E,
        0x4000_0004,
        0x02,
        0xBE5B_6513,
        0xA860_7CEC,
        0x2500_1E91,
        0,
        0x4000_0006,
        0x02,
        0x4000_0004,
        0x02,
        0x33BE_4035,
        0x6C45_9364,
        0x2500_1E91,
        0,
        0x4000_0006,
        0x02,
        0x4000_0006,
        0x20,
        0x4000_0004,
        0x20,
        0x33BE_4035,
        0x4F39_B658,
        0x2500_0011,
        0,
        0x4000_0006,
        0x20,
        0x4000_0007,
        0x20,
        0,
        0x4000_0004,
        0x20,
        0x33BE_4035,
        0x9D7D_54C6,
        0x2500_0011,
        0,
        0x4000_0006,
        0x20,
        0x4000_0007,
        0x20,
        0,
        0x4000_0007,
        0x20,
        0,
        0x4000_0004,
        0x20,
        0xBE5B_6513,
        0x8ADC_E384,
        0x2500_0011,
    ];
    let got: Vec<u32> = got[0][20..]
        .as_chunks::<4>()
        .0
        .iter()
        .map(|c| u32::from_le_bytes(*c))
        .collect();
    assert_eq!(got[..recorded.len()], recorded[..]);
    assert_eq!(got[recorded.len()..], [0, 0x4000_0006, 0x20, 0x22][..]);
}

/// A player's profile names the quest rating its completed quests have added.
/// Behaviour: server.examination.player-quest-rating
#[test]
fn examining_a_player_names_its_quest_rating() {
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let (_, idb) = h.player_in_world("two", "Babyfare");
    h.run(10);
    h.server
        .players
        .values_mut()
        .find(|p| p.character == idb)
        .expect("in the world")
        .play
        .quest_rating = 104_295;
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(idb, 0x203)]),
    );
    h.run(30);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "one profile");
    let at = got[0]
        .windows(4)
        .position(|w| w == yalain_world::profile::var::QUEST_RATING.to_le_bytes())
        .expect("the rating's variable");
    assert_eq!(
        got[0][at + 8..at + 12],
        104_295u32.to_le_bytes(),
        "its value in the line"
    );
    let shown = 104_295i32.to_le_bytes();
    assert!(
        got[0][at + 12..].windows(4).any(|w| w == shown),
        "and on its own beside it"
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
            amount: None,
            ends: None,
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
            spellcraft: Some(100),
            amount: None,
            ends: None,
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

/// A treasure item's own least level and Arcane Lore stay with it across a logout, its profile
/// listing them under its requirements, and its holder's hero skill credits are kept.
/// Behaviour: server.world.treasure-own-kept-across-sessions
#[test]
fn a_treasure_items_own_requirements_and_the_hero_credits_are_kept_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
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
    let own = yalain_world::treasure::Own {
        min_level: Some(33),
        arcane_lore: Some(17),
        ..yalain_world::treasure::Own::default()
    };
    h.server.things.get_mut(item).expect("made").own = own;
    h.server
        .players
        .get_mut(&world)
        .expect("in")
        .play
        .hero_credits = 7;
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
    let entered = h.bots[a].inbox.len();
    h.enter_world(a, "one", me);
    assert_eq!(h.server.things.get(item).expect("back").own, own);
    let p = h.server.players.values().next().expect("back in the world");
    assert_eq!(p.play.hero_credits, 7, "the hero skill credits kept");
    let repository = h
        .world_messages(a, entered)
        .iter()
        .filter_map(|m| match m {
            ServerMessage::PlayerCall(c) if c.function.0 == 0x8212_0027 => {
                c.args.as_ref().and_then(|x| calls::values(&x.0).ok())
            }
            _ => None,
        })
        .flatten()
        .find_map(|v| match v {
            Value::Graph(g) => g.iter().find_map(|(_, n)| match n {
                Node::Script {
                    package: 0x233,
                    slots,
                } => Some(slots.iter().map(|(_, w)| *w).collect::<Vec<u32>>()),
                _ => None,
            }),
            _ => None,
        })
        .expect("the session start's skill repository");
    assert_eq!(repository[3], 7, "the session start sends them");
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
    view.min_level = 33;
    view.arcane_lore = 17;
    let p = profile::item(0x3FF, &view);
    assert_eq!(
        got[0][20..],
        p.packed(&got[0][..20])[20..],
        "its own requirements listed"
    );
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

/// An inscribed item's profile ends with who wrote on it and the words, on their own tab; one asked
/// without that tab leaves them out and still notes the tab.
/// Behaviour: server.examination.an-inscription-is-shown
#[test]
fn an_inscribed_item_shows_who_wrote_it_and_the_words_when_examined() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.server.things.get_mut(item).expect("made").inscription =
        Some(("Sharp".to_owned(), "Elara [F]".to_owned()));
    h.run(2);
    let ask = |h: &mut Harness, asked: u32| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::EXAMINE, &[about(item, asked)]),
        );
        h.run(10);
        profile_nodes(h, a, from).pop().expect("answered")
    };
    let mut lines = profile::Profile::new(0x3FF);
    profile::inscription_lines(&mut lines, "Sharp", "Elara [F]");
    let lines = lines.packed(&[]);
    let want = &lines[4..lines.len() - 4];
    let all = ask(&mut h, 0x3FF);
    assert!(
        all[..all.len() - 4].ends_with(want),
        "the profile ends with the lines"
    );
    assert_ne!(
        u32::from_le_bytes(all[all.len() - 4..].try_into().expect("a word")) & 0x80,
        0
    );
    let without = ask(&mut h, 0x203);
    assert!(
        !without.windows(want.len()).any(|w| w == want),
        "not asked for"
    );
    assert_ne!(
        u32::from_le_bytes(without[without.len() - 4..].try_into().expect("a word")) & 0x80,
        0,
        "the tab is still noted"
    );
}

/// An item in another player's pack answers no examination until the other side knows of it, as
/// an offer in a trade makes it known.
/// Behaviour: server.selection.examining-what-another-holds
#[test]
fn an_item_in_another_players_pack_answers_only_once_offered() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, ida) = h.player_in_world("one", "Elara");
    let (b, idb) = h.player_in_world("two", "Birch");
    let world_of = |h: &Harness, id: u64| {
        *h.server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .expect("in the world")
            .0
    };
    let (owner, viewer) = (world_of(&h, ida), world_of(&h, idb));
    let mut at = h.server.players[&owner].position;
    at.origin[0] += 1.0;
    h.server.players.get_mut(&viewer).expect("in").position = at;
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        owner,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.run(2);
    let ask = |h: &mut Harness| {
        let from = h.bots[b].inbox.len();
        h.bots[b].send(
            true,
            queue::WEENIE,
            &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
        );
        h.run(10);
        profile_nodes(h, b, from).len()
    };
    assert_eq!(ask(&mut h), 0, "a stranger's pack");
    use yalain_world::trade::function as trade;
    h.ask(a, trade::OPEN, &[Arg::LongInt(idb)], 10);
    h.ask(b, trade::OPEN, &[Arg::LongInt(ida)], 10);
    h.ask(a, trade::OFFER, &[Arg::Int(1), Arg::LongInt(item)], 10);
    assert_eq!(ask(&mut h), 1, "offered to it");
}

/// A dyed item's profile shows its icon with what its dye gives it through its appearance table,
/// and a potion's its color through the potions' table.
/// Behaviour: server.examination.an-items-icon-takes-its-appearance
#[test]
fn a_dyed_items_icon_takes_its_dyes_color() {
    const TABLE: u32 = 0x2000_7001;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    h.server.catalog.icons.insert(
        yalain_testkit::TEST_ITEM,
        vec![
            [1000, 0x4100_0001, 0xFFFF_FFFF],
            [2000, 0x4100_0002, 0xFFFF_FFFF],
        ],
    );
    let steps = |key: u32| {
        vec![yalain_content::IconTable {
            keys: vec![(
                key,
                vec![
                    (0.0, vec![[2000, 0, 0xFF00_0001]]),
                    (0.1, vec![[2000, 0, 0xFF00_0002]]),
                    (0.2, vec![[2000, 0, 0xFF00_0003]]),
                ],
            )],
        }]
    };
    let (potions, key) = yalain_world::inventory::POTION_COLOR;
    h.server.catalog.icon_tables.insert(TABLE, steps(2));
    h.server.catalog.icon_tables.insert(potions, steps(key));
    let (a, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    h.server.things.get_mut(item).expect("made").own.dye = Some((TABLE, 0.15));
    h.run(2);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    let t = h.server.things.get(item).expect("held");
    let icon = yalain_world::inventory::icon_of(&h.server, t);
    assert_eq!(
        icon,
        [
            [1000, 0x4100_0001, 0xFFFF_FFFF],
            [2000, 0x4100_0002, 0xFF00_0002]
        ],
        "the color of the step its dye falls in"
    );
    let mut view = profile::ItemView::of(
        &h.server
            .catalog
            .get(yalain_testkit::TEST_ITEM)
            .expect("made")
            .props,
        1,
        None,
    );
    view.icon = icon;
    let mut request = Vec::new();
    for w in [0x4000_0002u32, 0] {
        request.extend(w.to_le_bytes());
    }
    request.extend(item.to_le_bytes());
    request.extend(0x3FFu32.to_le_bytes());
    assert_eq!(got, vec![profile::item(0x3FF, &view).packed(&request)]);
    let t = h.server.things.get_mut(item).expect("held");
    t.own.dye = None;
    t.own.effect = Some(yalain_world::treasure::OwnEffect {
        state: 0,
        spellcraft: 0.0,
        table: 0,
        color: 0.25,
    });
    let t = h.server.things.get(item).expect("held");
    assert_eq!(
        yalain_world::inventory::icon_of(&h.server, t)[1],
        [2000, 0x4100_0002, 0xFF00_0003]
    );
}

/// A shield no one made shows its template's numbers as a shield's: its rating leading, the delay
/// it wears with, and its effects when wielded indented once, under no heading of armor or weapons.
/// Behaviour: server.examination.a-plain-shield-shows-shield-stats
#[test]
fn a_plain_shield_shows_shield_stats_from_its_template() {
    use profile::{item_line as l, ItemEffect, ItemView, Node, When};
    use yalain_content::Prop;
    let mut props = yalain_content::Props::default();
    props.set("Item_ArmorLevel", Prop::Int(69));
    props.set("Weapon_Damage", Prop::Int(142));
    props.set("Weapon_Variance", Prop::Float(0.21));
    props.set("Weapon_Speed", Prop::Int(15));
    props.set("Weapon_VigorCost", Prop::Int(15));
    let mut view = ItemView::of(&props, 1, None);
    assert_eq!(view.shield, Some(69));
    assert_eq!(view.armor, None);
    assert_eq!(view.weapon, Some((142, 0.21, 0, 15)), "no delay of its own");
    props.set("Item_CombatDelay", Prop::Int(10));
    assert_eq!(ItemView::of(&props, 1, None).weapon.map(|w| w.2), Some(10));

    view.effects.push(ItemEffect {
        when: When::Equipped,
        table: 1,
        spellcraft: None,
        amount: None,
        ends: None,
    });
    let nodes = profile::item(0x3FF, &view).nodes;
    let line = |id: u32| {
        nodes
            .iter()
            .position(|n| matches!(n, Node::Text { text, .. } if text.string_id == id))
    };
    assert!(line(l::SHIELD_STATS).is_some());
    assert_eq!(line(l::ARMOR_STATS), None);
    assert_eq!(line(l::WEAPON_STATS), None);
    let rating = line(l::SHIELD_RATING).expect("its rating");
    assert!(matches!(nodes[rating + 1], Node::Int { value: 69, .. }));
    assert!(rating < line(l::DAMAGE).expect("its damage"), "it leads");
    let heading = line(l::EFFECTS_WIELDED).expect("its effects");
    assert!(matches!(nodes[heading - 1], Node::Tab { count: 1, .. }));
}

/// A stack with a value of its own shows what all its pieces are worth, as a sale of it pays.
/// Behaviour: server.inventory.a-stack-pays-for-each-piece
#[test]
fn a_stack_with_its_own_value_shows_the_value_of_all_its_pieces() {
    const STACK: u32 = 0x4700_7002;
    let mut h = Harness::new();
    let (a, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("a player");
    let mut props = yalain_content::Props::default();
    props.set("Item_MaxStackSize", yalain_content::Prop::Int(1000));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(STACK, 0x200, props));
    let item =
        yalain_world::inventory::give(&mut h.server, &mut h.net, world, STACK, 3).expect("given");
    h.server.things.get_mut(item).expect("made").value = 40;
    h.run(2);
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    let view = profile::ItemView::of(
        &h.server.catalog.get(STACK).expect("made").props,
        3,
        Some(120),
    );
    let mut request = Vec::new();
    for w in [0x4000_0002u32, 0] {
        request.extend(w.to_le_bytes());
    }
    request.extend(item.to_le_bytes());
    request.extend(0x3FFu32.to_le_bytes());
    assert_eq!(got, vec![profile::item(0x3FF, &view).packed(&request)]);
}

/// A treasure item keeps the value it was drawn across a logout: it is still named and valued as
/// one on its next entry.
/// Behaviour: server.world.treasure-value-kept-across-sessions
#[test]
fn a_treasure_items_drawn_value_is_kept_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
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
    h.server.things.get_mut(item).expect("made").value = 77;
    let shown = yalain_world::inventory::item_create(h.server.things.get(item).expect("made"));
    let before = h.bots[a].inbox.len();
    h.bots[a].send(
        false,
        queue::LOGON,
        &ClientMessage::LogOff(CharacterRef { character: me }),
    );
    assert!(h.run_until(400, |h| h.bots[a].inbox[before..]
        .iter()
        .any(|r| matches!(r.message(), Some(ServerMessage::CharacterList(_))))));
    h.enter_world(a, "one", me);
    let t = h.server.things.get(item).expect("back in the world");
    assert_eq!(t.value, 77, "the value it was drawn");
    assert_eq!(
        yalain_world::inventory::item_create(t).weenie,
        shown.weenie,
        "named and valued as before"
    );
}

/// What a recipe's item level gave an item, its value and its permanent effects are kept across a
/// logout, its name still its level's.
/// Behaviour: server.craft.made-item-kept
#[test]
fn a_made_items_own_numbers_are_kept_across_a_logout() {
    use auberean_protocol::login::CharacterRef;
    use auberean_protocol::ClientMessage;
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
    let made = yalain_world::made::Made {
        prefix: yalain_world::made::prefix(20),
        value: 7,
        damage: Some((110, 0.2)),
        delay: Some(13),
        vigor: Some(5),
        slots: 2,
        harmonic: true,
        damage_changed: true,
        durability: Some(200),
        tool_yield: Some(25),
        craft_needed: Some(60),
        ..yalain_world::made::Made::default()
    };
    let drawn = yalain_world::treasure::Look {
        table: 0x2000_0122,
        values: [(2, 0.31), (14, 0.02), (0, 0.0)],
    };
    {
        let t = h.server.things.get_mut(item).expect("made");
        t.own.made = Some(made);
        t.own.min_level = Some(20);
        t.value = 7;
        t.enchantments = vec![(0x6F00_0F2B, 20.0)];
        t.own.dye = Some((0x2000_036A, 0.573));
        t.own.look = Some(drawn);
        t.own.suffix = Some(0x2500_1292);
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
    h.enter_world(a, "one", me);
    let t = h.server.things.get(item).expect("back in the world");
    assert_eq!(t.own.made, Some(made));
    assert_eq!(t.own.min_level, Some(20));
    assert_eq!(t.value, 7);
    assert_eq!(t.enchantments, vec![(0x6F00_0F2B, 20.0)]);
    assert_eq!(t.own.dye, Some((0x2000_036A, 0.573)));
    assert_eq!(t.own.look, Some(drawn));
    assert_eq!(t.own.suffix, Some(0x2500_1292));
}

/// A treasure item with an enchantment is named for it: in the name it is made with and in its
/// profile, the suffix in the effect's own table with the item's name inside it; a piece of jewelry
/// has no word before it.
/// Behaviour: server.loot.treasure-named-for-an-enchantment
#[test]
fn a_treasure_item_is_named_for_its_enchantment_in_its_create_and_its_profile() {
    use yalain_world::inventory::{
        item_create, jewelry_name, treasure_item_name, treasure_name, treasure_word_name,
    };
    const NAME: (u32, u32) = (0x1234_5678, 0x2500_0777);
    const TABLE: u32 = 0x2500_1292;
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, _) = h.player_in_world("one", "Elara");
    let world = *h.server.players.keys().next().expect("in the world");
    let item = yalain_world::inventory::give(
        &mut h.server,
        &mut h.net,
        world,
        yalain_testkit::TEST_ITEM,
        1,
    )
    .expect("given");
    let shape = |h: &mut Harness, made: bool, suffix: Option<u32>| {
        let t = h.server.things.get_mut(item).expect("made");
        t.facts.name = Some(NAME);
        t.value = 7;
        t.own.made = made.then(yalain_world::made::Made::default);
        t.own.suffix = suffix;
        item_create(h.server.things.get(item).expect("made"))
            .weenie
            .name
    };
    assert_eq!(shape(&mut h, true, None), treasure_item_name(NAME, None));
    assert_eq!(
        shape(&mut h, false, Some(TABLE)),
        jewelry_name(NAME, Some(TABLE))
    );
    let named = shape(&mut h, true, Some(TABLE));
    assert_eq!(named, treasure_item_name(NAME, Some(TABLE)));
    let suffix = named
        .variables
        .iter()
        .find(|v| v.id == treasure_name::SUFFIX)
        .expect("a suffix");
    let auberean_protocol::text::VariableValue::Nested(suffix) = &suffix.value else {
        panic!("a nested string");
    };
    assert_eq!(
        (suffix.string_id, suffix.table_id),
        (treasure_name::EFFECT_SUFFIX, TABLE)
    );
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    assert_eq!(got.len(), 1, "answered once");
    let has = |id: u32| got[0].windows(4).any(|w| w == id.to_le_bytes());
    assert!(has(treasure_name::EFFECT_SUFFIX) && has(TABLE), "named");
    assert!(has(treasure_name::PREFIX_TEXT.0), "with treasure's word");
    // A word it drew, in its create and its profile; none at all with nothing of its own.
    const STEADY: u32 = 0xEF4F_89B9;
    let t = h.server.things.get_mut(item).expect("made");
    t.own.made = Some(yalain_world::made::Made {
        prefix: STEADY,
        ..yalain_world::made::Made::default()
    });
    assert_eq!(
        item_create(h.server.things.get(item).expect("made"))
            .weenie
            .name,
        treasure_word_name(NAME, Some(TABLE), STEADY)
    );
    let from = h.bots[a].inbox.len();
    h.bots[a].send(
        true,
        queue::WEENIE,
        &server_call(function::EXAMINE, &[about(item, 0x3FF)]),
    );
    h.run(10);
    let got = profile_nodes(&h, a, from);
    let has = |id: u32| got[0].windows(4).any(|w| w == id.to_le_bytes());
    assert!(has(STEADY) && has(TABLE), "its word and its suffix");
    assert!(!has(treasure_name::PREFIX_TEXT.0));
    assert_eq!(shape(&mut h, false, None), jewelry_name(NAME, None));
}

/// A person, or an object, a world gave a name of its own is examined under that name, as it is
/// shown over its head; the template's stays for the rest.
/// Behaviour: server.examination.a-rows-own-name
#[test]
fn a_row_with_a_name_of_its_own_is_examined_under_it() {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind};

    const PERSON: u64 = 0x40DF_8A00_0000_0021;
    const OTHER: u64 = 0x40DF_8A00_0000_0022;
    let template_name = (0x1111_2222, 0x2500_1E91);
    let row = |object: u64, kind: Kind, name_text: Option<&str>| ContentObject {
        object: Some(object),
        template: 0x4700_2F80,
        description: Description {
            name_text: name_text.map(str::to_owned),
            package: 0x13A4,
            look: 0x1F00_0100,
            enter_fx: 0,
            scale: None,
            name: None,
            numbers: [0; 6],
            source: "authored for a test".into(),
        },
        cell: 0x0299_0109,
        origin: [61.0, 241.0, -44.6],
        rotation: [1.0, 0.0, 0.0, 0.0],
        kind,
        label: String::new(),
        placed: "authored for a test".into(),
        destination: None,
        facts: Facts {
            name: Some(template_name),
            ..Facts::default()
        },
        walks: None,
        gate: None,
    };
    let mut h = Harness::new();
    h.server.content = ContentPack {
        objects: vec![
            row(PERSON, Kind::Npc, Some("Old Tom")),
            row(OTHER, Kind::Npc, None),
        ],
        ..ContentPack::default()
    };
    let (a, _) = h.player_in_world("one", "Elara");
    h.run(10);
    let says = |h: &mut Harness, object: u64| {
        let from = h.bots[a].inbox.len();
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(function::EXAMINE, &[about(object, 0x3FF)]),
        );
        h.run(30);
        profile_nodes(h, a, from).concat()
    };
    // What follows the request repeated is the profile of a person of that name.
    let of_name = |name| profile::npc(0, name, &Facts::default()).packed(&[]);
    let own = says(&mut h, PERSON);
    assert!(own.ends_with(&of_name(profile::literal("Old Tom"))));
    let plain = says(&mut h, OTHER);
    assert!(plain.ends_with(&of_name(profile::string(template_name.1, template_name.0))));
}
