//! Conservation: a random script of takes, trades, drops, stacks split and merged, money moved, effects
//! put on items, deaths, log-offs, stops and crashes, from fixed seeds. After every action each item is held exactly once
//! across every record, in the world and in the store, and what a player has is neither more nor
//! less than it was given: a stop loses only what lay on the ground or in a corpse; a crash, at most
//! what the store had not taken.

use std::collections::BTreeMap;

use auberean_protocol::login::CharacterRef;
use auberean_protocol::{queue, ClientMessage};
use auberean_script::args::Arg;
use yalain_content::{Prop, Props};
use yalain_net::ConnId;
use yalain_testkit::play::use_of;
use yalain_testkit::{test_rules, test_template, Harness, TEST_ITEM};
use yalain_world::graph::{tag, Builder, Slot};
use yalain_world::inventory::{self, move_graph, Move};
use yalain_world::state::PlayState;
use yalain_world::things::{Corpse, Kind, Thing};
use yalain_world::{drops, pack, requests, trade, Clock, ServerConfig, Timer};

/// A stack the world makes up, and what an item is worth.
pub(super) const GEM: u32 = 0x4700_7002;
const COINS: u32 = pack::COINS;
const WORTH: u64 = 10;
/// A skill on any item a player holds, an effect that attunes it for a while and one that lasts longer.
const ON_ITEM: u32 = 7004;
const ATTUNES: u32 = 0x7000_7A07;
const LASTS: u32 = 0x7000_7A08;

/// A small generator of its own, so that a seed is the same script everywhere.
pub(super) struct Dice(u64);

impl Dice {
    pub(super) fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    pub(super) fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }

    pub(super) fn pick<T: Copy>(&mut self, v: &[T]) -> Option<T> {
        (!v.is_empty()).then(|| v[self.below(v.len())])
    }
}

pub(super) const ACCOUNTS: [&str; 3] = ["one", "two", "three"];
const NAMES: [&str; 3] = ["HolderA", "HolderB", "HolderC"];

pub(super) struct Script {
    pub(super) h: Harness,
    /// Each player: its client (none while logged off), its character.
    pub(super) who: Vec<(Option<usize>, u64)>,
    pub(super) dice: Dice,
    /// Every thing's worth the players were given, and the gems.
    pub(super) wealth: u64,
    pub(super) gems: u64,
    pub(super) log: Vec<String>,
    /// What this world is, as it comes back from a stop or a crash.
    fixture: fn(&mut Harness),
    /// The world takes a share of a sale: worth may fall, and never rises.
    pub(super) spends: bool,
}

pub(super) fn catalog(h: &mut Harness) {
    h.server.catalog = yalain_testkit::test_catalog();
    let mut worth = Props::default();
    worth.set("Item_Value", Prop::Int(10));
    h.server
        .catalog
        .insert(test_template(TEST_ITEM, 0x1067, worth));
    let mut stacks = Props::default();
    stacks.set("Item_MaxStackSize", Prop::Int(100));
    h.server.catalog.insert(test_template(GEM, 0x1067, stacks));
    h.server
        .catalog
        .insert(test_template(COINS, 0x1067, Props::default()));
    // What a skill may be aimed at, stacks among them; on a polearm its effects land after its use.
    for (t, kind) in [
        (TEST_ITEM, yalain_world::skill_use::implement::SWORD),
        (GEM, yalain_world::skill_use::implement::POLEARM),
    ] {
        h.server.catalog.implements.insert(t, kind);
    }
    for (state, seconds, class) in [(ATTUNES, 7.0, 114), (LASTS, 40.0, 9)] {
        let d = yalain_world::effect_defs::EffectDef {
            duration: yalain_world::effect_defs::Scale(vec![(0.0, seconds)]),
            priority: yalain_world::effect_defs::Scale(vec![(0.0, 1.0)]),
            eq_class: class,
            fx: 0x4100_029A,
            ..yalain_testkit::plain_effect(state)
        };
        h.server.effect_table.effects.insert(state, d);
    }
    h.server.skill_table.skills.insert(
        ON_ITEM,
        yalain_world::skills::SkillDef {
            id: ON_ITEM,
            active: true,
            cap: 150,
            usage: Some(yalain_world::skills::Usage {
                implements_right: -1,
                implements_left: -1,
                valid_targets: yalain_world::skill_use::target_bit::OWN_ITEM
                    | yalain_world::skill_use::target_bit::OF_KINDS,
                valid_object_kinds: vec![yalain_world::skill_use::object_kind::WEAPON],
                target_effects: vec![ATTUNES, LASTS],
                hooks: vec![yalain_world::skills::Hook {
                    chance: 1.0,
                    ..yalain_world::skills::Hook::default()
                }],
                ui_flags: 0x12C1,
                ..yalain_world::skills::Usage::default()
            }),
            ..yalain_world::skills::SkillDef::default()
        },
    );
}

pub(super) fn world_of(h: &Harness, id: u64) -> Option<ConnId> {
    h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .map(|(c, _)| *c)
}

/// Everywhere an item is: each place it is held, by what holds it.
#[derive(Default)]
pub(super) struct Census {
    places: BTreeMap<u64, Vec<String>>,
    /// Bad records found on the way.
    wrong: Vec<String>,
    pub(super) wealth: u64,
    pub(super) gems: u64,
    /// Of those, what would not outlive a stop: on the ground and in corpses.
    ground_wealth: u64,
    ground_gems: u64,
}

impl Census {
    fn count(&mut self, template: u32, quantity: u32, ground: bool) {
        let n = u64::from(quantity.max(1));
        let (wealth, gems) = match template {
            TEST_ITEM => (WORTH * n, 0),
            COINS => (n, 0),
            GEM => (0, n),
            _ => (0, 0),
        };
        self.wealth += wealth;
        self.gems += gems;
        if ground {
            self.ground_wealth += wealth;
            self.ground_gems += gems;
        }
    }
}

pub(super) fn census(s: &mut Script) -> Census {
    let mut c = Census::default();
    let h = &mut s.h;
    for (_, id) in &s.who {
        if let Some(w) = world_of(h, *id) {
            let p = &h.server.players[&w];
            c.wealth += u64::from(yalain_world::store::money(&p.qualities));
            for x in &p.play.items {
                c.places
                    .entry(x.id)
                    .or_default()
                    .push(format!("the pack of {id:#x}"));
                c.count(x.template, x.quantity, false);
                match h.server.things.get(x.id) {
                    None => c.wrong.push(format!("{:#x} held, not in the world", x.id)),
                    Some(t) if t.quantity != x.quantity => c.wrong.push(format!(
                        "{:#x}: {} held, {} in the world",
                        x.id, x.quantity, t.quantity
                    )),
                    Some(_) => {}
                }
            }
        } else {
            let Some(st) = yalain_world::saves::state_of(&mut h.server, *id).expect("reads") else {
                continue;
            };
            let play = PlayState::from_bytes(&st.play).expect("reads");
            c.wealth += u64::from(yalain_world::store::money(&st.qualities));
            for x in &play.items {
                c.places
                    .entry(x.id)
                    .or_default()
                    .push(format!("the kept pack of {id:#x}"));
                c.count(x.template, x.quantity, false);
            }
        }
    }
    // What is on sale is the broker's to hold; what is sold is worth its price to its seller.
    let on_sale: Vec<u64> = h
        .server
        .market
        .list
        .iter()
        .filter(|k| k.sold < k.offered)
        .map(|k| k.item)
        .collect();
    for k in &h.server.market.list {
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let paid = (f64::from(k.sold) * f64::from(k.price)).floor() as u64;
        c.wealth += paid;
    }
    let things: Vec<Thing> = h.server.things.by_id.values().cloned().collect();
    let in_corpses: BTreeMap<u64, u64> = things
        .iter()
        .filter_map(|t| t.corpse.as_ref().map(|k| (t.id, k)))
        .flat_map(|(id, k)| k.contents.iter().map(move |i| (*i, id)))
        .collect();
    let dropping: Vec<u64> = h
        .server
        .timers
        .iter()
        .filter_map(|(_, t)| match t {
            Timer::PutDown { asked, .. } => Some(asked.item),
            _ => None,
        })
        .collect();
    for t in things.iter().filter(|t| t.kind == Kind::Item) {
        if on_sale.contains(&t.id) {
            c.places.entry(t.id).or_default().push("on sale".into());
            c.count(t.template, t.quantity, false);
            continue;
        }
        match (t.container, t.at) {
            (Some((owner, _)), _) if in_corpses.get(&t.id) == Some(&owner) => {
                c.places
                    .entry(t.id)
                    .or_default()
                    .push(format!("the corpse {owner:#x}"));
                c.count(t.template, t.quantity, true);
            }
            (Some((owner, _)), _) => {
                if !c.places.contains_key(&t.id) {
                    c.wrong.push(format!(
                        "{:#x} says {owner:#x} holds it: nothing does",
                        t.id
                    ));
                }
            }
            (None, Some(_)) => {
                c.places.entry(t.id).or_default().push("the ground".into());
                c.count(t.template, t.quantity, true);
            }
            (None, None) if c.places.contains_key(&t.id) => {}
            (None, None) => {
                if dropping.contains(&t.id) {
                    c.places
                        .entry(t.id)
                        .or_default()
                        .push("a player's hand, going down".into());
                    c.count(t.template, t.quantity, false);
                } else {
                    c.wrong.push(format!("{:#x} is nowhere", t.id));
                }
            }
        }
    }
    for (item, corpse) in &in_corpses {
        if !h.server.things.by_id.contains_key(item) {
            c.wrong
                .push(format!("{corpse:#x} lists {item:#x}, not in the world"));
        }
    }
    c
}

pub(super) fn check(s: &mut Script, what: &str) {
    let c = census(s);
    let twice: Vec<String> = c
        .places
        .iter()
        .filter(|(_, p)| p.len() > 1)
        .map(|(i, p)| format!("{i:#x} in {p:?}"))
        .collect();
    let mut bad = Vec::new();
    if !twice.is_empty() {
        bad.push(format!("held twice: {twice:?}"));
    }
    bad.extend(c.wrong.iter().cloned());
    for t in s.h.server.things.by_id.values() {
        for e in &t.timed {
            if t.timed.iter().filter(|x| x.state == e.state).count() > 1 {
                bad.push(format!("{:#x} holds an effect twice: {:?}", t.id, t.timed));
            }
        }
    }
    if c.wealth > s.wealth || (c.wealth < s.wealth && !s.spends) {
        bad.push(format!("worth {}, should be {}", c.wealth, s.wealth));
    }
    if s.spends {
        s.wealth = c.wealth;
    }
    if c.gems != s.gems {
        bad.push(format!("gems {}, should be {}", c.gems, s.gems));
    }
    assert!(
        bad.is_empty(),
        "after {what}: {bad:?}\nlast: {:#?}",
        &s.log[s.log.len().saturating_sub(14)..]
    );
}

fn new_script(seed: u64) -> Script {
    new_script_in(seed, catalog)
}

/// Players in a world of this kind, with these dice.
pub(super) fn new_script_in(seed: u64, fixture: fn(&mut Harness)) -> Script {
    let mut h = Harness::new();
    fixture(&mut h);
    let who = ACCOUNTS
        .iter()
        .zip(NAMES)
        .map(|(a, name)| {
            let (b, id) = h.player_in_world(a, name);
            (Some(b), id)
        })
        .collect();
    let mut s = Script {
        h,
        who,
        dice: Dice(seed.max(1)),
        wealth: 0,
        gems: 0,
        log: Vec::new(),
        fixture,
        spends: false,
    };
    together(&mut s);
    let c = census(&mut s);
    s.wealth = c.wealth;
    s.gems = c.gems;
    s
}

/// Everyone in the world stands together.
fn together(s: &mut Script) {
    let mut at = None;
    for (_, id) in s.who.clone() {
        let Some(w) = world_of(&s.h, id) else {
            continue;
        };
        let p = s.h.server.players.get_mut(&w).expect("in the world");
        match at {
            None => at = Some(p.position),
            Some(here) => p.position = here,
        }
    }
}

/// Time passes, the corpses not rotting: a corpse rotting is no one's loss, and would hide one.
pub(super) fn pass(s: &mut Script, seconds: f64) {
    let t = s.h.now.0 + seconds;
    while s.h.now.0 < t {
        hold_corpses(s);
        let dt = (t - s.h.now.0).clamp(yalain_testkit::STEP, 1.0);
        s.h.step_by(dt);
    }
    hold_corpses(s);
}

fn hold_corpses(s: &mut Script) {
    for t in s.h.server.things.by_id.values_mut() {
        if let Some(c) = t.corpse.as_mut() {
            c.expires = 1.0e12;
        }
        // What a creature left is worth nothing here: its worth would be made from nothing.
        if t.kind == Kind::Item {
            t.value = 0;
        }
    }
}

/// A player in the world: its index, its client, its character, its connection.
pub(super) fn someone(s: &mut Script) -> Option<(usize, usize, u64, ConnId)> {
    let here: Vec<usize> = (0..s.who.len())
        .filter(|i| s.who[*i].0.is_some() && world_of(&s.h, s.who[*i].1).is_some())
        .collect();
    let i = s.dice.pick(&here)?;
    let (b, id) = s.who[i];
    Some((i, b?, id, world_of(&s.h, id)?))
}

fn pack_of(s: &Script, w: ConnId) -> Vec<(u64, u32, i32)> {
    s.h.server.players[&w]
        .play
        .items
        .iter()
        .filter(|x| x.location == 0)
        .map(|x| (x.id, x.quantity.max(1), i32::try_from(x.slot).unwrap_or(0)))
        .collect()
}

fn corpses(s: &Script) -> Vec<u64> {
    s.h.server
        .things
        .by_id
        .values()
        .filter(|t| t.corpse.is_some())
        .map(|t| t.id)
        .collect()
}

pub(super) fn ask(s: &mut Script, b: usize, f: u32, args: &[Arg]) {
    hold_corpses(s);
    let steps = 1 + s.dice.below(3);
    s.h.ask(b, f, args, steps);
}

fn take_all_request(corpse: u64, me: u64) -> Arg {
    #[allow(clippy::cast_possible_truncation)]
    let long = |v: u64| {
        [
            Slot::Tagged(tag::LONG, v as u32),
            Slot::Tagged(tag::HIGH, (v >> 32) as u32),
        ]
    };
    let mut slots = vec![Slot::Int(0), Slot::Int(0), Slot::Int(1), Slot::Int(0)];
    slots.extend([Slot::Int(0), Slot::Int(0)]);
    slots.extend(long(corpse));
    slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
    slots.extend(long(me));
    let mut g = Builder::new();
    g.object(pack::TAKE_ALL_DESC, &slots);
    Arg::Object(g.graph())
}

fn transmute_request(corpse: u64) -> Arg {
    let mut g = Builder::new();
    let mut slots = vec![Slot::Int(0); 8];
    slots.push(Slot::Long(corpse));
    slots.extend(std::iter::repeat_n(Slot::Int(0), 6));
    g.object(inventory::TRANSMUTE_DESC, &slots);
    Arg::Object(g.graph())
}

/// Bring every player back from the store, in a new world.
pub(super) fn restart(s: &mut Script, what: &str) {
    let store = std::mem::take(&mut s.h.server.store);
    let cfg = ServerConfig::default();
    let mut h = Harness::with(cfg, store, test_rules());
    (s.fixture)(&mut h);
    s.h = h;
    for (i, account) in ACCOUNTS.iter().enumerate() {
        let b = s.h.at_character_list(account);
        let id = s.who[i].1;
        s.h.enter_world(b, account, id);
        s.who[i].0 = Some(b);
    }
    together(s);
    s.log.push(format!("back from {what}"));
}

/// One random action. Returns what it was.
#[allow(clippy::too_many_lines)]
pub(super) fn act(s: &mut Script) -> String {
    let Some((i, b, me, w)) = someone(s) else {
        return "nobody in the world".into();
    };
    let other = s
        .who
        .iter()
        .enumerate()
        .filter(|(j, (_, id))| *j != i && world_of(&s.h, *id).is_some())
        .map(|(j, x)| (j, x.0, x.1))
        .next();
    let n = s.dice.below(29);
    match n {
        0 | 1 => {
            let g = 1 + u32::try_from(s.dice.below(30)).unwrap_or(0);
            if inventory::give(&mut s.h.server, &mut s.h.net, w, GEM, g).is_some() {
                s.gems += u64::from(g);
            }
            format!("{me:#x} is given {g} gems")
        }
        2 => {
            if inventory::give(&mut s.h.server, &mut s.h.net, w, TEST_ITEM, 1).is_some() {
                s.wealth += WORTH;
            }
            format!("{me:#x} is given an item")
        }
        3 => {
            // A corpse beside it.
            let t = s.h.server.catalog.get(TEST_ITEM).cloned().expect("there");
            let id = s.h.server.things.new_id();
            let mut c = Thing::of(id, &t, Kind::Corpse);
            c.at = Some(s.h.server.players[&w].position);
            let count = 1 + s.dice.below(4);
            let items: Vec<u64> = (0..count)
                .map(|k| {
                    let item =
                        inventory::make_item(&mut s.h.server, TEST_ITEM, 1, 0).expect("made");
                    s.h.server.things.get_mut(item).expect("made").container =
                        Some((id, u32::try_from(k).unwrap_or(0)));
                    item
                })
                .collect();
            c.corpse = Some(Corpse {
                killer: me,
                originator: 0,
                contents: items,
                creature: 0,
                butchered: false,
                killer_level: 1,
                expires: 1.0e12,
            });
            s.h.server.things.insert(c);
            s.wealth += WORTH * count as u64;
            format!("a corpse of {count} by {me:#x}")
        }
        4 => match s.dice.pick(&corpses(s)) {
            Some(c) => {
                ask(s, b, requests::function::USE, &[use_of(c, me)]);
                format!("{me:#x} opens {c:#x}")
            }
            None => "no corpse to open".into(),
        },
        5 => {
            let open = s.h.server.players[&w].open_container;
            let items = open
                .and_then(|c| s.h.server.things.get(c))
                .and_then(|t| t.corpse.as_ref())
                .map(|k| k.contents.clone())
                .unwrap_or_default();
            match s.dice.pick(&items) {
                Some(item) => {
                    ask(s, b, requests::function::USE, &[use_of(item, me)]);
                    format!("{me:#x} takes {item:#x}")
                }
                None => "nothing to take".into(),
            }
        }
        6 => match s.h.server.players[&w].open_container {
            Some(c) => {
                ask(s, b, inventory::function::CLOSE, &[Arg::LongInt(c)]);
                format!("{me:#x} closes {c:#x}")
            }
            None => "nothing to close".into(),
        },
        7 => match s.h.server.players[&w].open_container {
            Some(c) => {
                ask(s, b, pack::function::TAKE_ALL, &[take_all_request(c, me)]);
                format!("{me:#x} takes all of {c:#x}")
            }
            None => "nothing to take all of".into(),
        },
        8 => match s.h.server.players[&w].open_container {
            Some(c) => {
                let f = inventory::function::TRANSMUTE_ALL;
                ask(s, b, f, &[transmute_request(c)]);
                format!("{me:#x} turns {c:#x} into money")
            }
            None => "nothing to turn into money".into(),
        },
        9 => match s.dice.pick(&pack_of(s, w)) {
            Some((item, quantity, slot)) => {
                let m = Move {
                    move_type: drops::DROP,
                    item,
                    quantity,
                    from: me,
                    from_slot: slot,
                    ..Move::default()
                };
                ask(s, b, drops::function::MOVE_ITEM, &[move_graph(&m)]);
                format!("{me:#x} puts down {item:#x}")
            }
            None => "nothing to put down".into(),
        },
        10 => {
            let lying: Vec<u64> =
                s.h.server
                    .things
                    .by_id
                    .values()
                    .filter(|t| t.kind == Kind::Item && t.container.is_none() && t.at.is_some())
                    .map(|t| t.id)
                    .collect();
            match s.dice.pick(&lying) {
                Some(item) => {
                    ask(s, b, requests::function::USE, &[use_of(item, me)]);
                    format!("{me:#x} picks up {item:#x}")
                }
                None => "nothing to pick up".into(),
            }
        }
        11 => match other {
            Some((_, Some(ob), oid)) => {
                ask(s, b, trade::function::OPEN, &[Arg::LongInt(oid)]);
                ask(s, ob, trade::function::OPEN, &[Arg::LongInt(me)]);
                format!("{me:#x} and {oid:#x} open a trade")
            }
            _ => "nobody to trade with".into(),
        },
        12 | 13 => match s.dice.pick(&pack_of(s, w)) {
            Some((item, quantity, _)) => {
                let q = 1 + u32::try_from(s.dice.below(quantity as usize)).unwrap_or(0);
                let args = [Arg::Int(q), Arg::LongInt(item)];
                ask(s, b, trade::function::OFFER, &args);
                format!("{me:#x} offers {q} of {item:#x}")
            }
            None => "nothing to offer".into(),
        },
        14 | 15 => {
            ask(s, b, trade::function::ACCEPT, &[]);
            format!("{me:#x} accepts")
        }
        16 => {
            let f = match s.dice.below(3) {
                0 => trade::function::DECLINE,
                1 => trade::function::CLOSE,
                _ => trade::function::ACCEPT,
            };
            ask(s, b, f, &[]);
            format!("{me:#x} answers the trade {f:#x}")
        }
        17 => match s.dice.pick(&pack_of(s, w)) {
            Some((item, quantity, slot)) => {
                let q = 1 + u32::try_from(s.dice.below(quantity as usize)).unwrap_or(0);
                let to = i32::try_from(s.dice.below(45)).unwrap_or(0);
                let m = Move {
                    item,
                    quantity: q,
                    from: me,
                    from_slot: slot,
                    target: me,
                    target_slot: to,
                    ..Move::default()
                };
                ask(s, b, pack::function::REORGANIZE, &[move_graph(&m)]);
                format!("{me:#x} moves {q} of {item:#x} to {to}")
            }
            None => "nothing to move".into(),
        },
        18 => {
            let items: Vec<(u64, u32, i32)> = pack_of(s, w)
                .into_iter()
                .filter(|(i, _, _)| {
                    s.h.server
                        .things
                        .get(*i)
                        .is_some_and(|t| t.template == TEST_ITEM || t.template == COINS)
                })
                .collect();
            if let Some((item, quantity, slot)) = s.dice.pick(&items) {
                let args = [
                    Arg::LongInt(me),
                    Arg::Int(slot.cast_unsigned()),
                    Arg::Int(quantity),
                    Arg::LongInt(item),
                ];
                ask(s, b, pack::function::TO_MONEY_BAG, &args);
                format!("{me:#x} bags {item:#x}")
            } else {
                let money = yalain_world::store::money(&s.h.server.players[&w].qualities);
                if money > 0 {
                    let q = 1 + (u32::try_from(s.dice.below(money as usize)).unwrap_or(0));
                    let args = [Arg::LongInt(me), Arg::Int(0), Arg::Int(q)];
                    ask(s, b, pack::function::FROM_MONEY_BAG, &args);
                    format!("{me:#x} takes {q} coins out")
                } else {
                    "no money".into()
                }
            }
        }
        19 => {
            // Log off, and back after the linger.
            s.h.bots[b].send(
                false,
                queue::LOGON,
                &ClientMessage::LogOff(CharacterRef { character: me }),
            );
            s.who[i].0 = None;
            pass(s, 25.0);
            let back = s.h.at_character_list(ACCOUNTS[i]);
            s.h.enter_world(back, ACCOUNTS[i], me);
            s.who[i].0 = Some(back);
            together(s);
            format!("{me:#x} logs off and back")
        }
        21 => match (other, s.dice.pick(&corpses(s))) {
            // Two after one corpse: the first closes it with its take under way.
            (Some((_, Some(ob), oid)), Some(c)) => {
                let item =
                    s.h.server
                        .things
                        .get(c)
                        .and_then(|t| t.corpse.as_ref())
                        .and_then(|k| k.contents.first().copied());
                if let Some(item) = item {
                    ask(s, b, requests::function::USE, &[use_of(c, me)]);
                    ask(s, b, requests::function::USE, &[use_of(item, me)]);
                    ask(s, b, inventory::function::CLOSE, &[Arg::LongInt(c)]);
                    ask(s, ob, requests::function::USE, &[use_of(c, oid)]);
                    if s.dice.below(2) == 0 {
                        ask(s, ob, requests::function::USE, &[use_of(item, oid)]);
                    } else {
                        ask(s, ob, pack::function::TAKE_ALL, &[take_all_request(c, oid)]);
                    }
                }
                format!("{me:#x} and {oid:#x} race for {c:#x}")
            }
            _ => "no race".into(),
        },
        22 => match other {
            // A trade right through, something perhaps happening to an offer on the way.
            Some((oj, Some(ob), oid)) => {
                let _ = oj;
                let f = trade::function::OPEN;
                ask(s, b, f, &[Arg::LongInt(oid)]);
                ask(s, ob, f, &[Arg::LongInt(me)]);
                let ow = world_of(&s.h, oid);
                if let (Some((a_item, a_n, a_slot)), Some(ow)) = (s.dice.pick(&pack_of(s, w)), ow) {
                    let q = 1 + u32::try_from(s.dice.below(a_n as usize)).unwrap_or(0);
                    let f = trade::function::OFFER;
                    ask(s, b, f, &[Arg::Int(q), Arg::LongInt(a_item)]);
                    if let Some((b_item, b_n, _)) = s.dice.pick(&pack_of(s, ow)) {
                        let q = 1 + u32::try_from(s.dice.below(b_n as usize)).unwrap_or(0);
                        ask(s, ob, f, &[Arg::Int(q), Arg::LongInt(b_item)]);
                    }
                    ask(s, b, trade::function::ACCEPT, &[]);
                    if s.dice.below(2) == 0 {
                        let m = Move {
                            item: a_item,
                            quantity: 1 + u32::try_from(s.dice.below(a_n as usize)).unwrap_or(0),
                            from: me,
                            from_slot: a_slot,
                            target: me,
                            target_slot: 7,
                            ..Move::default()
                        };
                        ask(s, b, pack::function::REORGANIZE, &[move_graph(&m)]);
                    }
                    ask(s, ob, trade::function::ACCEPT, &[]);
                }
                format!("{me:#x} and {oid:#x} trade right through")
            }
            _ => "no trade".into(),
        },
        20 => {
            let clock: Clock = s.h.clock();
            yalain_world::death::player_dies(&mut s.h.server, &mut s.h.net, w, None, clock);
            pass(s, 11.0);
            ask(s, b, yalain_world::death::player_fn::RESURRECT, &[]);
            pass(s, 3.0);
            together(s);
            format!("{me:#x} dies and comes back")
        }
        25 | 26 => {
            // A skill on an item of its own, of another's, on what it names by a stale id.
            let mine: Vec<u64> = pack_of(s, w).iter().map(|x| x.0).collect();
            let theirs: Vec<u64> = other
                .and_then(|(_, _, oid)| world_of(&s.h, oid))
                .map(|ow| pack_of(s, ow).iter().map(|x| x.0).collect())
                .unwrap_or_default();
            let target = match s.dice.below(5) {
                0 => s.dice.pick(&theirs).unwrap_or(me),
                1 => me,
                2 => 0,
                _ => s.dice.pick(&mine).unwrap_or(me),
            };
            let number = 1 + u32::try_from(s.dice.below(100)).unwrap_or(0);
            ask(
                s,
                b,
                yalain_world::skill_use::function::USE,
                &[Arg::Int(number), Arg::Int(ON_ITEM), Arg::LongInt(target)],
            );
            format!("{me:#x} uses a skill on {target:#x}")
        }
        _ => {
            // Let what is under way finish.
            let seconds = 0.5 * (1 + s.dice.below(6)) as f64;
            pass(s, seconds);
            "time passes".into()
        }
    }
}

/// A stop in the middle of whatever is under way: it loses what lay on the ground and in corpses, and
/// no more.
pub(super) fn stop(s: &mut Script) {
    let c = census(s);
    s.log.push("a stop".into());
    yalain_world::shutdown::save_everyone(&mut s.h.server, &mut s.h.net);
    s.wealth = c.wealth - c.ground_wealth;
    s.gems = c.gems - c.ground_gems;
    restart(s, "a stop");
}

/// A crash in the middle of whatever is under way: what the store had is what comes back, each item
/// once, and no more than there was.
pub(super) fn crash(s: &mut Script) {
    let c = census(s);
    s.log.push("a crash".into());
    restart(s, "a crash");
    let after = census(s);
    assert!(
        after.wealth <= c.wealth && after.gems <= c.gems,
        "a crash made worth: {} from {}, gems {} from {}\nlast: {:#?}",
        after.wealth,
        c.wealth,
        after.gems,
        c.gems,
        &s.log[s.log.len().saturating_sub(14)..]
    );
    s.wealth = after.wealth;
    s.gems = after.gems;
}

/// Each item held once and nothing made or lost, over random scripts with fixed seeds.
/// Behaviour: server.inventory.conservation
#[test]
fn every_item_is_held_once_and_nothing_is_made_or_lost_over_random_scripts() {
    let mut on_items = 0;
    for seed in [
        0x5EED_0001,
        0x5EED_0002,
        0x5EED_0003,
        0x5EED_0004,
        0x5EED_0005,
    ] {
        let mut s = new_script(seed);
        for step in 0..400 {
            let what = act(&mut s);
            s.log.push(what.clone());
            check(&mut s, &format!("{seed:#x} step {step}: {what}"));
            on_items +=
                s.h.server
                    .things
                    .by_id
                    .values()
                    .filter(|t| !t.timed.is_empty())
                    .count();
            match s.dice.below(150) {
                0 => {
                    stop(&mut s);
                    check(&mut s, &format!("{seed:#x} step {step}: a stop"));
                }
                1 => {
                    crash(&mut s);
                    check(&mut s, &format!("{seed:#x} step {step}: a crash"));
                }
                _ => {}
            }
        }
    }
    assert!(on_items > 0, "the scripts put effects on items");
}
