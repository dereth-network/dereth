//! Sequences: two and three players, each step a place reported and then something done, or something
//! done and then the time passing, from fixed seeds, through trades, corpses, a broker, a craft,
//! wielding, a fellowship, an allegiance, pets, fights in which what is hit hits back, leaving, and a
//! second login of the same account. Nothing panics, nothing waits for ever, and after every step
//! each item is held exactly once and neither gems nor worth come from nowhere.

use auberean_primitives::Vec3;
use auberean_protocol::login::{CharacterRef, Empty};
use auberean_protocol::{queue, ClientMessage};
use auberean_script::args::Arg;
use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind as Placed, Props};
use yalain_net::ConnId;
use yalain_testkit::play::{report, use_of};
use yalain_testkit::{
    test_camp, test_template, Harness, IRON, MINES_RECIPE, MINES_SWORD, TEST_ITEM,
};
use yalain_world::effect_defs::{EffectDef, Kind, Scale, Share, Watcher};
use yalain_world::effects::{self, Cast, On};
use yalain_world::graph::{Builder, Slot};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::market::{self, Broker};
use yalain_world::player::quality;
use yalain_world::state::{PlayState, SkillState};
use yalain_world::{
    allegiance, combat, craft, death, drops, equip, fellowship, inventory, pack, pets, quals,
    requests, skill_use, store, things, trade,
};

use super::conservation::{self, ask, census, check, pass, restart, someone, world_of, Script};

/// An item whose use makes a creature for a few seconds, and the effect it names.
const MAKES_A_CREATURE: u32 = 0x4700_7100;
const SUMMONS: &str = "Eff_SequencesSummon";
const BROKER: u64 = 0x40FE_FA00_0000_0003;
const BROKER_TEMPLATE: u32 = 0x4700_2E8B;
const STORE: u32 = 0x6F00_150B;
const BROKER_CELL: u32 = 0x0299_0109;
const NEAR_BROKER: [f32; 3] = [59.06, 240.2, -44.6];
/// What a creature leaves, of no worth.
const LOOT: u32 = 0x4700_7010;
const SWORD: u32 = 0x4700_051C;
const NATURES_AID: u32 = 227;
/// Effects made up for this world.
const SENDS_BACK: u32 = 0x6F00_7A01;
const REPLIES: u32 = 0x6F00_7A02;
const ANSWER: u32 = 0x6F00_7A03;
const HARM: u32 = 0x6F00_7A04;
const COUNTERS: u32 = 0x6F00_7A05;
const AWAY: u32 = 0x6F00_7A06;

fn lasting(state: u32, class: u32, kind: Kind, harmful: bool) -> EffectDef {
    EffectDef {
        state,
        kind,
        duration: Scale(vec![(0.0, 40.0)]),
        priority: Scale(vec![(0.0, 1.0)]),
        fx: 0,
        external_flags: 0,
        internal_flags: 0x1100_0003
            | if harmful {
                yalain_world::effect_defs::HARMFUL
            } else {
                0
            },
        enum_value: 0,
        eq_class: class,
        name: None,
    }
}

/// The world a sequence plays in: what conservation counts, a broker, creatures whose corpses hold
/// what nothing counts, a pet's skill, a recipe's iron, and effects that answer attacks.
fn fixture(h: &mut Harness) {
    conservation::catalog(h);
    let mut gem = Props::default();
    gem.set("Usage_Effect1", yalain_content::Prop::Text(SUMMONS.into()))
        .set("Item_IsDestroyOnUse", yalain_content::Prop::Bool(true));
    h.server
        .catalog
        .insert(test_template(MAKES_A_CREATURE, 0x777, gem));
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(BROKER),
            template: BROKER_TEMPLATE,
            description: Description {
                name_text: None,
                package: 0x13A3,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: BROKER_CELL,
            origin: [61.0, 240.2, -44.6],
            rotation: [1.0, 0.0, 0.0, 0.0],
            kind: Placed::Static,
            label: String::new(),
            placed: "authored for a test".into(),
            destination: None,
            facts: Facts::default(),
            walks: None,
            gate: None,
        }],
        ..ContentPack::default()
    };
    h.server.brokers.insert(
        BROKER_TEMPLATE,
        Broker {
            catalog: 0x6F00_1503,
            location: (0x0D3C_90C4, 0x2500_0CE6),
        },
    );
    for t in [LOOT, IRON, MINES_SWORD] {
        h.server
            .catalog
            .insert(test_template(t, 0x200, Props::default()));
    }
    h.server
        .catalog
        .insert(test_template(SWORD, 0x1067, Props::default()));
    let mut loot = yalain_testkit::test_loot();
    loot.profiles[0].items = vec![(LOOT, 1)];
    h.server.loot = loot;
    h.server.spawns.camps.push(test_camp(2));
    h.server
        .camps
        .push(yalain_world::spawn::CampState::default());
    super::pets::with_natures_aid(h);
    let share = |amount: f32| Share {
        chance: 1.0,
        amount: Scale(vec![(0.0, amount)]),
        spread: 0.0,
    };
    let table = &mut h.server.effect_table.effects;
    table.insert(
        SENDS_BACK,
        lasting(
            SENDS_BACK,
            70,
            Kind::Watcher(Box::new(Watcher {
                absorbs: share(0.5),
                sends_back: share(5.0),
                against_a_player: 1.0,
                shares: true,
                watches: true,
            })),
            false,
        ),
    );
    table.insert(
        REPLIES,
        lasting(
            REPLIES,
            71,
            Kind::Reply {
                effect: ANSWER,
                chance: Scale(vec![(0.0, 1.0)]),
                classes: vec![],
            },
            false,
        ),
    );
    table.insert(
        COUNTERS,
        lasting(
            COUNTERS,
            72,
            Kind::Counter {
                effect: ANSWER,
                chance: Scale(vec![(0.0, 1.0)]),
            },
            false,
        ),
    );
    table.insert(ANSWER, lasting(ANSWER, 73, Kind::Other, true));
    table.insert(HARM, lasting(HARM, 74, Kind::Other, true));
    table.insert(
        AWAY,
        EffectDef {
            duration: Scale(vec![(0.0, 2.0)]),
            ..lasting(
                AWAY,
                75,
                Kind::Teleport(yalain_store::StoredPosition {
                    cell: 0x0299_0109,
                    origin: [40.0, 200.0, -44.6],
                    rotation: [1.0, 0.0, 0.0, 0.0],
                }),
                false,
            )
        },
    );
}

/// Each player has drawn its world, stands by the broker and has what it plays with.
fn ready(s: &mut Script, first: bool) {
    let who = s.who.clone();
    for (bot, id) in who {
        let Some(b) = bot else { continue };
        let Some(w) = world_of(&s.h, id) else {
            continue;
        };
        s.h.bots[b].send(
            true,
            queue::WEENIE,
            &ClientMessage::SceneRenderingComplete(Empty),
        );
        let p = s.h.server.players.get_mut(&w).expect("in the world");
        p.position.cell = BROKER_CELL;
        p.position.origin = NEAR_BROKER;
        if !first {
            continue;
        }
        quals::set_int(&mut p.qualities, quality::VIGOR_MAX, 400);
        quals::set_int(&mut p.qualities, quality::VIGOR, 400);
        quals::set_int(&mut p.qualities, store::MONEY, 5_000);
        for skill in [NATURES_AID, yalain_testkit::ESPIRIT] {
            p.play.skills.push(SkillState {
                skill,
                granted: 0,
                mask: 0x1,
                cost: 1,
                children: 0,
                dependents: 0,
                spent: 0,
            });
        }
        s.h.arm(b, id, SWORD);
        let clock = s.h.clock();
        craft::learn(&mut s.h.server, &mut s.h.net, w, MINES_RECIPE, clock);
        inventory::give(&mut s.h.server, &mut s.h.net, w, IRON, 40);
    }
    s.h.run(3);
}

fn start(seed: u64) -> Script {
    let mut s = conservation::new_script_in(seed, fixture);
    ready(&mut s, true);
    let c = census(&mut s);
    s.wealth = c.wealth;
    s.gems = c.gems;
    s.spends = true;
    s
}

/// The creatures and pets alive, and the other players: what there is to swing at.
fn targets(s: &Script, me: u64) -> Vec<u64> {
    let mut v: Vec<u64> =
        s.h.server
            .things
            .by_id
            .values()
            .filter(|t| t.kind == things::Kind::Creature && t.alive())
            .map(|t| t.id)
            .collect();
    v.extend(
        s.h.server
            .players
            .values()
            .map(|p| p.character)
            .filter(|c| *c != me),
    );
    v.sort_unstable();
    v
}

fn text_arg(s: &str) -> Arg {
    let mut b = Builder::new();
    b.native(
        0x76,
        auberean_script::native_object::NativeObject::Text(s.encode_utf16().collect()),
    );
    Arg::Object(b.graph())
}

/// A report of where its player is, this kind of place. Some are no place at all, some from before
/// the server last put it somewhere.
fn place(s: &mut Script, b: usize, w: ConnId) {
    let Some(p) = s.h.server.players.get(&w) else {
        return;
    };
    let (cell, origin, stamp) = (p.position.cell, p.position.origin, p.teleport_stamp);
    let (mut cell, mut at, mut stamp) = (cell, Vec3::new(origin[0], origin[1], origin[2]), stamp);
    match s.dice.below(12) {
        0..=3 => {
            at.x += 1.0 - s.dice.below(20) as f32 * 0.1;
            at.y += 1.0 - s.dice.below(20) as f32 * 0.1;
        }
        4 | 5 => {
            cell = BROKER_CELL;
            at = Vec3::new(NEAR_BROKER[0], NEAR_BROKER[1], NEAR_BROKER[2]);
        }
        6 => {
            // Outdoors, about a block's edge.
            cell = 0x7A64_0001;
            at = Vec3::new(159.999, 192.0, 40.0);
        }
        7 => {
            cell = 0x7A64_0040;
            at = Vec3::new(160.0, -0.001, 40.0);
        }
        8 => {
            // The land's far side.
            cell = 0xFEFE_0001;
            at = Vec3::new(40_960.0, -40_960.0, 400.0);
        }
        9 => at = Vec3::new(f32::NAN, 1.0e30, f32::INFINITY),
        10 => stamp = stamp.wrapping_sub(1),
        _ => cell = 0xFFFF_FFFF,
    }
    let heading = (s.dice.next() & 0xFF) as u8;
    let m = report(cell, at, heading, yalain_world::world::IDLE_MOTION, stamp);
    s.h.bots[b].send(true, queue::WEENIE, &m);
    s.h.run(1);
}

fn gems_of(s: &Script, w: ConnId) -> Vec<(u64, u32)> {
    s.h.server.players[&w]
        .play
        .items
        .iter()
        .filter(|x| x.location == 0 && x.template == conservation::GEM)
        .map(|x| (x.id, x.quantity.max(1)))
        .collect()
}

fn of_template(s: &Script, w: ConnId, template: u32) -> Vec<u64> {
    s.h.server.players[&w]
        .play
        .items
        .iter()
        .filter(|x| x.template == template)
        .map(|x| x.id)
        .collect()
}

/// A pet its owner has, by what the world holds.
fn pets_of(s: &Script, me: u64) -> Vec<u64> {
    s.h.server.pets.of(me)
}

fn equip_call(item: u64) -> Arg {
    let mut slots = vec![Slot::Int(0); 3];
    slots.push(Slot::Long(item));
    slots.resize(18, Slot::Int(0));
    slots.push(Slot::Int(0));
    let mut b = Builder::new();
    b.object(equip::EQUIP_DESC.0, &slots);
    Arg::Object(b.graph())
}

fn purchase(item: u64, me: u64, n: u32) -> Arg {
    let mut g = Builder::new();
    g.object(
        0x1B76,
        &[
            Slot::Long(item),
            Slot::Ref(None),
            Slot::Long(me),
            Slot::Int(n),
            Slot::Long(BROKER),
            Slot::Int(0),
            Slot::Int(0),
        ],
    );
    Arg::Object(g.graph())
}

fn execute(iron: u64) -> Arg {
    use auberean_script::library::Elem;
    use auberean_script::native_object::Item;
    let mut g = Builder::new();
    g.native(
        8,
        yalain_world::graph::hash(
            Elem::Word,
            Elem::Long,
            0,
            &[(Item::Word(1), Item::Long(iron))],
        ),
    );
    Arg::Object(g.graph())
}

/// One step of another kind than conservation's. Returns what it was.
#[allow(clippy::too_many_lines)]
fn step(s: &mut Script) -> String {
    let Some((i, b, me, w)) = someone(s) else {
        return "nobody in the world".into();
    };
    let other = s
        .who
        .iter()
        .enumerate()
        .filter(|(j, (_, id))| *j != i && world_of(&s.h, *id).is_some())
        .map(|(_, x)| (x.0, x.1))
        .next();
    match s.dice.below(30) {
        0 | 1 => {
            // A swing at something, the player perhaps having moved first.
            let t = targets(s, me);
            let Some(target) = s.dice.pick(&t) else {
                return "nothing to swing at".into();
            };
            ask(
                s,
                b,
                combat::function::START_ATTACK,
                &[Arg::LongInt(target)],
            );
            let seconds = 0.5 * (1 + s.dice.below(6)) as f64;
            pass(s, seconds);
            format!("{me:#x} swings at {target:#x}")
        }
        2 => {
            ask(s, b, combat::function::STOP_ATTACK, &[]);
            format!("{me:#x} stops swinging")
        }
        3 | 4 => {
            let t = targets(s, me);
            let target = s.dice.pick(&t).unwrap_or(0);
            let skills = [
                NATURES_AID,
                yalain_testkit::ESPIRIT,
                yalain_testkit::VACUUM,
                yalain_testkit::PARALYZE,
                yalain_testkit::SINEW_SNAP,
                yalain_testkit::LILITHAS_VENGEANCE,
            ];
            let skill = skills[s.dice.below(skills.len())];
            let id = u32::try_from(s.dice.below(1 << 20)).unwrap_or(1);
            let args = [Arg::Int(id), Arg::Int(skill), Arg::LongInt(target)];
            s.h.all_hold(skill);
            ask(s, b, skill_use::function::USE, &args);
            format!("{me:#x} uses {skill} on {target:#x}")
        }
        5 => {
            // What answers an attack, on the player or on what it would hit.
            let on = match s.dice.pick(&targets(s, me)) {
                Some(t) if s.dice.below(2) == 0 => match world_of(&s.h, t) {
                    Some(c) => On::Player(c),
                    None => On::Creature(t),
                },
                _ => On::Player(w),
            };
            let state = [SENDS_BACK, REPLIES, COUNTERS][s.dice.below(3)];
            let clock = s.h.clock();
            let cast = Cast {
                caster: me,
                spellcraft: 10.0,
                ..Cast::default()
            };
            effects::apply_on(&mut s.h.server, &mut s.h.net, on, state, cast, clock);
            format!("{me:#x} puts {state:#x} on {on:?}")
        }
        6 => {
            // Harm from one to another, as a skill's effect comes.
            let Some(target) = s.dice.pick(&targets(s, me)) else {
                return "nothing to harm".into();
            };
            let on = world_of(&s.h, target).map_or(On::Creature(target), On::Player);
            let clock = s.h.clock();
            let cast = Cast {
                caster: me,
                spellcraft: 10.0,
                pk: true,
                ..Cast::default()
            };
            effects::apply_on(&mut s.h.server, &mut s.h.net, on, HARM, cast, clock);
            format!("{me:#x} harms {target:#x}")
        }
        7 => {
            let clock = s.h.clock();
            let cast = Cast {
                caster: me,
                spellcraft: 1.0,
                ..Cast::default()
            };
            effects::apply(&mut s.h.server, &mut s.h.net, w, AWAY, cast, clock);
            pass(s, 3.0);
            s.h.bots[b].send(
                true,
                queue::WEENIE,
                &ClientMessage::SceneRenderingComplete(Empty),
            );
            format!("{me:#x} is sent away")
        }
        8 => {
            let swords = of_template(s, w, SWORD);
            match s.dice.pick(&swords) {
                Some(sword) => {
                    ask(s, b, requests::function::USE, &[use_of(sword, me)]);
                    format!("{me:#x} uses its sword {sword:#x}")
                }
                None => "no sword".into(),
            }
        }
        9 => {
            let swords = of_template(s, w, SWORD);
            match s.dice.pick(&swords) {
                Some(sword) => {
                    let f = if s.dice.below(2) == 0 {
                        equip::function::DIRECTIVE_EQUIP
                    } else {
                        equip::function::DIRECTIVE_UNEQUIP
                    };
                    ask(s, b, f, &[equip_call(sword)]);
                    format!("{me:#x} moves its sword {sword:#x}: {f:#x}")
                }
                None => "no sword".into(),
            }
        }
        10 => {
            let args = [Arg::Int(1), Arg::Int(0), text_arg("Sequence")];
            ask(s, b, fellowship::function::CREATE, &args);
            format!("{me:#x} makes a fellowship")
        }
        11 => match other {
            Some((Some(ob), oid)) => {
                ask(s, b, fellowship::function::RECRUIT, &[Arg::LongInt(oid)]);
                ask(s, ob, fellowship::function::ACCEPT, &[Arg::LongInt(me)]);
                format!("{me:#x} recruits {oid:#x}")
            }
            _ => "nobody to recruit".into(),
        },
        12 => {
            let f = [
                fellowship::function::LEAVE,
                fellowship::function::DISBAND,
                fellowship::function::DISMISS,
            ][s.dice.below(3)];
            let t = other.map_or(0, |o| o.1);
            let args: &[Arg] = if f == fellowship::function::DISMISS {
                &[Arg::LongInt(t)]
            } else {
                &[]
            };
            ask(s, b, f, args);
            format!("{me:#x} answers the fellowship {f:#x}")
        }
        13 => match other {
            Some((Some(ob), oid)) => {
                ask(s, b, allegiance::function::SWEAR, &[Arg::LongInt(oid)]);
                ask(
                    s,
                    ob,
                    allegiance::function::ACCEPT_VASSAL,
                    &[Arg::LongInt(me)],
                );
                format!("{me:#x} swears to {oid:#x}")
            }
            _ => "nobody to swear to".into(),
        },
        14 => {
            let t = other.map_or(0, |o| o.1);
            ask(s, b, allegiance::function::BREAK, &[Arg::LongInt(t)]);
            format!("{me:#x} breaks with {t:#x}")
        }
        15 => match s.dice.pick(&pets_of(s, me)) {
            Some(pet) => {
                let t = s.dice.pick(&targets(s, me)).unwrap_or(0);
                match s.dice.below(5) {
                    0 => ask(s, b, pets::function::ATTACK, &[Arg::LongInt(t)]),
                    1 => ask(s, b, pets::function::RETURN, &[Arg::LongInt(pet)]),
                    2 => ask(
                        s,
                        b,
                        pets::function::MODE,
                        &[Arg::Int(1), Arg::LongInt(pet)],
                    ),
                    3 => ask(
                        s,
                        b,
                        pets::function::NAME,
                        &[text_arg("Named"), Arg::LongInt(pet)],
                    ),
                    _ => ask(s, b, pets::function::DIE, &[Arg::LongInt(pet)]),
                }
                format!("{me:#x} orders {pet:#x}")
            }
            None => "no pet".into(),
        },
        16 => {
            ask(s, b, requests::function::USE, &[use_of(BROKER, me)]);
            let args = [Arg::Int(STORE), Arg::LongInt(BROKER)];
            ask(s, b, market::function::ENTER_STORE, &args);
            ask(
                s,
                b,
                market::function::ENTER_CONSIGNMENT,
                &[Arg::LongInt(BROKER)],
            );
            format!("{me:#x} visits the broker")
        }
        17 | 18 => {
            if gems_of(s, w).is_empty() {
                inventory::give(&mut s.h.server, &mut s.h.net, w, conservation::GEM, 20);
                s.gems += 20;
            }
            let Some((gem, n)) = s.dice.pick(&gems_of(s, w)) else {
                return "no gem".into();
            };
            let q = 1 + u32::try_from(s.dice.below(n as usize)).unwrap_or(0);
            let price = 1 + u32::try_from(s.dice.below(40)).unwrap_or(0);
            let args = [
                Arg::Int(STORE),
                Arg::Int(price),
                Arg::Int(q),
                Arg::LongInt(gem),
                Arg::LongInt(BROKER),
            ];
            ask(s, b, market::function::SELL, &args);
            format!("{me:#x} puts up {q} of {gem:#x} at {price}")
        }
        19 | 20 => {
            let sales: Vec<(u64, u32)> =
                s.h.server
                    .market
                    .list
                    .iter()
                    .filter(|c| c.sold < c.offered)
                    .map(|c| (c.item, c.offered - c.sold))
                    .collect();
            // Now and then a sale's time has run out: buying it is refused, its seller still has it.
            if s.dice.below(4) == 0 {
                let now = s.h.server.game_now;
                if let Some(c) = s.h.server.market.list.iter_mut().find(|c| c.in_stock()) {
                    c.entered = now - market::ON_SALE_SECONDS - 1.0;
                }
            }
            match s.dice.pick(&sales) {
                Some((item, n)) => {
                    let f = store::function::PURCHASE;
                    ask(s, b, f, &[purchase(item, me, n)]);
                    format!("{me:#x} buys {item:#x}")
                }
                None => "nothing on sale".into(),
            }
        }
        21 if s.dice.below(2) == 0 => {
            let mine: Vec<u32> =
                s.h.server
                    .market
                    .list
                    .iter()
                    .filter(|c| c.owner == me)
                    .map(|c| c.sale)
                    .collect();
            let (Some(sale), Some((gem, n))) = (s.dice.pick(&mine), s.dice.pick(&gems_of(s, w)))
            else {
                return "nothing to restock".into();
            };
            let q = 1 + u32::try_from(s.dice.below(n as usize)).unwrap_or(0);
            let args = [
                Arg::Int(sale),
                Arg::Int(q),
                Arg::LongInt(gem),
                Arg::LongInt(BROKER),
            ];
            ask(s, b, market::function::RESTOCK, &args);
            format!("{me:#x} restocks {sale} with {q} of {gem:#x}")
        }
        21 => {
            let mine: Vec<u32> =
                s.h.server
                    .market
                    .list
                    .iter()
                    .filter(|c| c.owner == me)
                    .map(|c| c.sale)
                    .collect();
            match s.dice.pick(&mine) {
                Some(sale) => {
                    let profits = u32::try_from(s.dice.below(2)).unwrap_or(0);
                    let args = [Arg::Int(profits), Arg::Int(sale), Arg::LongInt(BROKER)];
                    ask(s, b, market::function::COLLECT, &args);
                    format!("{me:#x} collects {sale} ({profits})")
                }
                None => "nothing to collect".into(),
            }
        }
        22 | 23 => {
            let iron = of_template(s, w, IRON);
            match s.dice.pick(&iron) {
                Some(iron) => {
                    let args = [
                        Arg::Int(0),
                        Arg::LongInt(iron),
                        Arg::Int(1),
                        Arg::Int(MINES_RECIPE),
                    ];
                    ask(s, b, craft::function::ADD_INGREDIENT, &args);
                    let args = [
                        Arg::LongInt(0),
                        execute(iron),
                        Arg::Int(0),
                        Arg::Int(MINES_RECIPE),
                    ];
                    ask(s, b, craft::function::EXECUTE, &args);
                    format!("{me:#x} makes something of {iron:#x}")
                }
                None => {
                    inventory::give(&mut s.h.server, &mut s.h.net, w, IRON, 40);
                    "iron given".into()
                }
            }
        }
        24 => {
            // Its iron moved while it is being made of.
            let iron = of_template(s, w, IRON);
            if let Some(iron) = s.dice.pick(&iron) {
                let m = Move {
                    item: iron,
                    quantity: 1,
                    from: me,
                    target: me,
                    target_slot: 3,
                    ..Move::default()
                };
                ask(s, b, pack::function::REORGANIZE, &[move_graph(&m)]);
            }
            format!("{me:#x} moves its iron")
        }
        25 => {
            // It logs off and goes on playing through its lingering, then comes back.
            s.h.bots[b].send(
                false,
                queue::LOGON,
                &ClientMessage::LogOff(CharacterRef { character: me }),
            );
            for _ in 0..6 {
                if world_of(&s.h, me).is_none() {
                    break;
                }
                let n = s.dice.below(30);
                if n < 10 {
                    place(s, b, w);
                }
                let what = busy(s, b, me, w);
                s.log.push(format!("  lingering: {what}"));
                pass(s, 3.0);
            }
            pass(s, 26.0);
            back(s, i, me);
            format!("{me:#x} logs off, busy, and back")
        }
        26 => {
            // Its account logs in again on another connection while it plays on the first.
            let new = s.h.at_character_list(conservation::ACCOUNTS[i]);
            let what = busy(s, b, me, w);
            s.log.push(format!("  an old connection: {what}"));
            pass(s, 26.0);
            s.who[i].0 = None;
            s.h.enter_world(new, conservation::ACCOUNTS[i], me);
            s.who[i].0 = Some(new);
            s.h.bots[new].send(
                true,
                queue::WEENIE,
                &ClientMessage::SceneRenderingComplete(Empty),
            );
            s.h.run(2);
            format!("{me:#x} logs in again")
        }
        27 => {
            // Its connection ends, one way or the other, with something under way.
            let what = busy(s, b, me, w);
            s.log.push(format!("  going: {what}"));
            let login = s.h.server.players.get(&w).map(|p| p.login);
            let conn = if s.dice.below(2) == 0 { Some(w) } else { login };
            if let Some(conn) = conn {
                s.h.net.close(conn);
            }
            s.h.run(1);
            pass(s, 26.0);
            back(s, i, me);
            format!("{me:#x} is cut off and comes back")
        }
        28 => {
            let clock = s.h.clock();
            death::player_dies(&mut s.h.server, &mut s.h.net, w, None, clock);
            s.h.run(1);
            let what = busy(s, b, me, w);
            s.log.push(format!("  dead: {what}"));
            pass(s, 11.0);
            ask(s, b, death::player_fn::RESURRECT, &[]);
            pass(s, 3.0);
            format!("{me:#x} dies, busy, and comes back")
        }
        29 => {
            let what = match s.dice.below(2) {
                0 => gem_used(s, b, me, w),
                _ => kill_sends_on(s, me, w),
            };
            let seconds = 0.5 * s.dice.below(8) as f64;
            pass(s, seconds);
            what
        }
        _ => {
            let seconds = 0.5 * (1 + s.dice.below(8)) as f64;
            pass(s, seconds);
            "time passes".into()
        }
    }
}

/// A gem that makes a creature for a few seconds is used.
fn gem_used(s: &mut Script, b: usize, me: u64, w: ConnId) -> String {
    let Some(template) = s.h.server.things.by_id.values().find_map(|t| {
        (t.kind == things::Kind::Creature && !s.h.server.pets.is_pet(t.id)).then_some(t.template)
    }) else {
        return "no creature to make".into();
    };
    s.h.server.death_spawns.insert(
        SUMMONS.into(),
        yalain_content::DeathSpawn {
            template,
            count: 1,
            chance: 1.0,
            offset: [0.0, 2.0],
            lasts: Some(3.0),
        },
    );
    let Some(item) = inventory::give(&mut s.h.server, &mut s.h.net, w, MAKES_A_CREATURE, 1) else {
        return "no room for a gem".into();
    };
    s.h.run(1);
    ask(s, b, 0x8216_0016, &[use_of(item, me)]);
    format!("{me:#x} uses a gem {item:#x}")
}

/// A creature dies by a player whose quest's step for that kill leads somewhere.
fn kill_sends_on(s: &mut Script, me: u64, w: ConnId) -> String {
    use super::kill_quests;
    use yalain_world::state::QuestState;
    const QUEST: u32 = 0x4000_0123;
    let Some((beetle, template)) = s.h.server.things.by_id.values().find_map(|t| {
        (t.kind == things::Kind::Creature && t.alive() && !s.h.server.pets.is_pet(t.id))
            .then_some((t.id, t.template))
    }) else {
        return "no creature to kill".into();
    };
    let to = yalain_content::quests::Place {
        cell: BROKER_CELL,
        origin: NEAR_BROKER,
        heading: 0,
    };
    let mut q = kill_quests::quest(3);
    q.effects = Vec::new();
    s.h.server.quests.quests = vec![q];
    s.h.server.quests.steps = vec![kill_quests::leading_step(template, 1, to)];
    if let Some(p) = s.h.server.players.get_mut(&w) {
        p.play.quests.retain(|x| x.quest != QUEST);
        p.play.quests.push(QuestState {
            quest: QUEST,
            status: yalain_world::quests::status::UNDERWAY,
            phase: 1,
            started: 0,
            updated: 0,
        });
    }
    let clock = s.h.clock();
    combat::hurt_creature(&mut s.h.server, &mut s.h.net, beetle, me, 1_000_000, clock);
    format!("{me:#x} kills {beetle:#x} for a step that leads on")
}

/// Something a player may do at any moment of its leaving, its dying or its trip.
fn busy(s: &mut Script, b: usize, me: u64, w: ConnId) -> String {
    if !s.h.server.players.contains_key(&w) {
        return "it has gone".into();
    }
    let first = s.h.server.players[&w].play.items.first().copied();
    match s.dice.below(5) {
        0 => {
            if let Some(held) = first {
                let m = Move {
                    move_type: yalain_world::drops::DROP,
                    item: held.id,
                    quantity: held.quantity.max(1),
                    from: me,
                    ..Move::default()
                };
                ask(
                    s,
                    b,
                    yalain_world::drops::function::MOVE_ITEM,
                    &[move_graph(&m)],
                );
            }
            "puts down".into()
        }
        1 => {
            let t = targets(s, me);
            if let Some(target) = s.dice.pick(&t) {
                ask(
                    s,
                    b,
                    combat::function::START_ATTACK,
                    &[Arg::LongInt(target)],
                );
            }
            "swings".into()
        }
        2 => {
            if let Some(held) = first {
                let args = [
                    Arg::Int(STORE),
                    Arg::Int(5),
                    Arg::Int(1),
                    Arg::LongInt(held.id),
                    Arg::LongInt(BROKER),
                ];
                ask(s, b, market::function::SELL, &args);
            }
            "sells".into()
        }
        3 => {
            let iron = of_template(s, w, IRON);
            if let Some(iron) = s.dice.pick(&iron) {
                let args = [
                    Arg::LongInt(0),
                    execute(iron),
                    Arg::Int(0),
                    Arg::Int(MINES_RECIPE),
                ];
                ask(s, b, craft::function::EXECUTE, &args);
            }
            "crafts".into()
        }
        _ => {
            if let Some(held) = first {
                let args = [Arg::Int(held.quantity.max(1)), Arg::LongInt(held.id)];
                ask(s, b, yalain_world::trade::function::OFFER, &args);
            }
            "offers".into()
        }
    }
}

fn back(s: &mut Script, i: usize, me: u64) {
    s.who[i].0 = None;
    let account = conservation::ACCOUNTS[i];
    let new = s.h.at_character_list(account);
    s.h.enter_world(new, account, me);
    s.who[i].0 = Some(new);
    s.h.bots[new].send(
        true,
        queue::WEENIE,
        &ClientMessage::SceneRenderingComplete(Empty),
    );
    s.h.run(2);
}

/// What a world keeps for each step stays in proportion.
fn bounded(s: &Script, what: &str) {
    assert!(
        s.h.server.timers.len() < 5_000,
        "after {what}: {} timers\nlast: {:#?}",
        s.h.server.timers.len(),
        &s.log[s.log.len().saturating_sub(14)..]
    );
}

/// Each item held once and nothing made from nowhere, whatever order two players do things in.
/// Behaviour: server.inventory.conservation
#[test]
fn whatever_order_players_do_things_in_nothing_panics_or_hangs_and_each_item_is_held_once() {
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        for seed in [0x5EED_6001, 0x5EED_6002, 0x5EED_6003] {
            let mut s = start(seed);
            for n in 0..300 {
                if let Some((_, b, _, w)) = someone(&mut s) {
                    if s.dice.below(3) == 0 {
                        place(&mut s, b, w);
                    }
                }
                let what = if s.dice.below(4) == 0 {
                    conservation::act(&mut s)
                } else {
                    step(&mut s)
                };
                s.log.push(what.clone());
                let what = format!("{seed:#x} step {n}: {what}");
                check(&mut s, &what);
                bounded(&s, &what);
                if s.dice.below(60) == 0 {
                    ready(&mut s, false);
                }
                if s.dice.below(150) == 0 {
                    conservation::stop(&mut s);
                    ready(&mut s, false);
                    check(&mut s, &format!("{seed:#x} step {n}: a stop"));
                }
                if s.dice.below(150) == 0 {
                    conservation::crash(&mut s);
                    ready(&mut s, false);
                    check(&mut s, &format!("{seed:#x} step {n}: a crash"));
                }
            }
            let _ = restart;
        }
        let _ = done.send(());
    });
    assert_eq!(
        ended.recv_timeout(std::time::Duration::from_secs(300)),
        Ok(()),
        "a sequence ended"
    );
}

/// What is marked in a world, as everything that holds an item sees it.
#[derive(Debug)]
struct Marked {
    /// Pieces of an attuned stack with its first holder, and with anyone else.
    attuned: (u64, u64),
    /// Items bound to the first holder that a character holds.
    bound_held: Vec<u64>,
    /// Marked items where none may be: on sale, on the ground, or bound to another.
    wrong: Vec<String>,
}

fn marked(s: &mut Script, first: u64) -> Marked {
    let mut wrong = Vec::new();
    let on_sale: Vec<u64> = s.h.server.market.list.iter().map(|k| k.item).collect();
    for t in s.h.server.things.by_id.values() {
        if (t.own.bound_to != 0 || t.own.attuned) && on_sale.contains(&t.id) {
            wrong.push(format!("{:#x} is on sale", t.id));
        }
        if t.own.attuned && t.at.is_some() {
            wrong.push(format!("{:#x} is on the ground", t.id));
        }
        if t.own.bound_to != 0 && t.own.bound_to != first {
            wrong.push(format!("{:#x} is bound to {:#x}", t.id, t.own.bound_to));
        }
    }
    let mut attuned = (0, 0);
    let mut bound_held = Vec::new();
    for (_, id) in s.who.clone() {
        let (items, states) = if let Some(w) = world_of(&s.h, id) {
            let items = s.h.server.players[&w].play.items.clone();
            let states = items
                .iter()
                .filter_map(|x| Some((x.id, s.h.server.things.get(x.id)?.own)))
                .collect();
            (items, states)
        } else {
            let st = yalain_world::saves::state_of(&mut s.h.server, id).expect("reads");
            let Some(st) = st else { continue };
            let play = PlayState::from_bytes(&st.play).expect("reads");
            let states = play.item_states.iter().map(|k| (k.id, k.own)).collect();
            (play.items, states)
        };
        let own: std::collections::BTreeMap<u64, _> = states;
        for x in &items {
            let Some(o) = own.get(&x.id) else { continue };
            if o.attuned {
                let n = u64::from(x.quantity);
                if id == first {
                    attuned.0 += n;
                } else {
                    attuned.1 += n;
                }
            }
            if o.bound_to == first {
                bound_held.push(x.id);
            }
        }
    }
    bound_held.sort_unstable();
    Marked {
        attuned,
        bound_held,
        wrong,
    }
}

/// Its holder tries to part with something marked: puts it down, trades it, or puts it up for sale.
fn try_to_part_with(s: &mut Script, first: u64) -> String {
    let Some(w) = world_of(&s.h, first) else {
        return "the holder is away".into();
    };
    let Some(b) = s.who.iter().find(|x| x.1 == first).and_then(|x| x.0) else {
        return "the holder is away".into();
    };
    let mine: Vec<(u64, u32, i32)> = s.h.server.players[&w]
        .play
        .items
        .iter()
        .filter(|x| x.location == 0)
        .filter(|x| {
            s.h.server
                .things
                .get(x.id)
                .is_some_and(|t| t.own.attuned || t.own.bound_to != 0)
        })
        .map(|x| (x.id, x.quantity, x.slot.cast_signed()))
        .collect();
    let Some((item, quantity, slot)) = s.dice.pick(&mine) else {
        return "nothing marked in the pack".into();
    };
    let q = 1 + u32::try_from(s.dice.below(quantity as usize)).unwrap_or(0);
    match s.dice.below(3) {
        0 => {
            let m = Move {
                move_type: drops::DROP,
                item,
                quantity,
                from: first,
                from_slot: slot,
                ..Move::default()
            };
            ask(s, b, drops::function::MOVE_ITEM, &[move_graph(&m)]);
            format!("puts down {item:#x}")
        }
        1 => {
            let other = s
                .who
                .iter()
                .find(|x| x.1 != first && x.0.is_some() && world_of(&s.h, x.1).is_some())
                .and_then(|x| Some((x.0?, x.1)));
            let Some((ob, oid)) = other else {
                return "nobody to trade with".into();
            };
            ask(s, b, trade::function::OPEN, &[Arg::LongInt(oid)]);
            ask(s, ob, trade::function::OPEN, &[Arg::LongInt(first)]);
            let args = [Arg::Int(q), Arg::LongInt(item)];
            ask(s, b, trade::function::OFFER, &args);
            ask(s, b, trade::function::ACCEPT, &[]);
            ask(s, ob, trade::function::ACCEPT, &[]);
            format!("trades {q} of {item:#x}")
        }
        _ => {
            let args = [
                Arg::Int(STORE),
                Arg::Int(5),
                Arg::Int(q),
                Arg::LongInt(item),
                Arg::LongInt(BROKER),
            ];
            ask(s, b, market::function::SELL, &args);
            format!("puts up {q} of {item:#x}")
        }
    }
}

/// Through trades, a broker, the ground, stacks split, a corpse, a death, a log-off, a stop and a
/// crash, an attuned stack stays with its holder whole, and a bound item stays bound to whom it was;
/// neither is ever on sale, and a stop keeps both.
/// Behaviour: server.inventory.conservation
#[test]
fn an_attuned_stack_stays_with_its_holder_and_a_bound_item_stays_bound_whatever_players_do() {
    for seed in [0x5EED_7001, 0x5EED_7002, 0x5EED_7003] {
        let mut s = start(seed);
        let me = s.who[0].1;
        let w = world_of(&s.h, me).expect("in the world");
        let stack = inventory::give(&mut s.h.server, &mut s.h.net, w, conservation::GEM, 40);
        let held = inventory::give(&mut s.h.server, &mut s.h.net, w, TEST_ITEM, 1);
        let (stack, held) = (stack.expect("given"), held.expect("given"));
        s.h.server.things.get_mut(stack).expect("made").own.attuned = true;
        s.h.server.things.get_mut(held).expect("made").own.bound_to = me;
        let c = census(&mut s);
        s.wealth = c.wealth;
        s.gems = c.gems;
        yalain_world::world::keep_player(&mut s.h.server, w);
        s.h.run(1);
        let whole = marked(&mut s, me).attuned;
        assert_eq!(whole, (40, 0));
        for n in 0..300 {
            if let Some((_, b, _, w)) = someone(&mut s) {
                if s.dice.below(3) == 0 {
                    place(&mut s, b, w);
                }
            }
            let what = match s.dice.below(4) {
                0 => try_to_part_with(&mut s, me),
                1 => conservation::act(&mut s),
                _ => step(&mut s),
            };
            s.log.push(what.clone());
            let what = format!("{seed:#x} step {n}: {what}");
            check(&mut s, &what);
            let now = marked(&mut s, me);
            let last = &s.log[s.log.len().saturating_sub(14)..];
            assert!(now.wrong.is_empty(), "{what}: {now:?}\n{last:#?}");
            assert_eq!(now.attuned, whole, "{what}\n{last:#?}");
            if s.dice.below(60) == 0 {
                ready(&mut s, false);
            }
            if s.dice.below(100) == 0 {
                conservation::stop(&mut s);
                ready(&mut s, false);
                check(&mut s, &format!("{seed:#x} step {n}: a stop"));
                let after = marked(&mut s, me);
                assert_eq!(after.attuned, whole, "a stop at {what}");
                assert!(after.wrong.is_empty(), "a stop at {what}: {after:?}");
                // One on its way to the ground comes back to the hand that held it.
                assert!(
                    now.bound_held.iter().all(|i| after.bound_held.contains(i)),
                    "a stop at {what}: {now:?} became {after:?}"
                );
            }
            if s.dice.below(100) == 0 {
                conservation::crash(&mut s);
                ready(&mut s, false);
                check(&mut s, &format!("{seed:#x} step {n}: a crash"));
                let after = marked(&mut s, me);
                assert!(after.wrong.is_empty(), "a crash at {what}: {after:?}");
                assert_eq!(after.attuned, whole, "a crash at {what}");
            }
        }
    }
}
