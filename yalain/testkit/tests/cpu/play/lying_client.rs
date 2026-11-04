//! A client that lies: every call the server answers, with values chosen to hurt (ids it does not
//! hold, others' items, zeroes, the largest numbers, objects of the wrong shape and size), alive,
//! dead, in portal space and on its way out; and whole messages cut short and bitten, from fixed
//! seeds. Nothing panics, and nothing of another player's changes.

use auberean_protocol::physics::Target;
use auberean_protocol::queue;
use auberean_script::args::Arg;
use yalain_net::ConnId;
use yalain_testkit::play::server_call;
use yalain_testkit::{Harness, TEST_ITEM};
use yalain_world::graph::{Builder, Slot};
use yalain_world::inventory::{move_graph, Move};
use yalain_world::{drops, inventory, pack, requests, session, trade};

struct Dice(u64);

impl Dice {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn below(&mut self, n: usize) -> usize {
        usize::try_from(self.next() % (n.max(1) as u64)).unwrap_or(0)
    }
}

/// Every ordinary call the server answers, by its function.
fn calls() -> Vec<u32> {
    use yalain_world::{allegiance, combat, craft, death, equip, fellowship, follow, market};
    use yalain_world::{mounts, personal, pets, quests, scenes, skills, store};
    vec![
        requests::function::SET_SELECTION,
        requests::function::EXAMINE,
        requests::function::SAY,
        requests::function::TELL,
        requests::function::EMOTE,
        requests::function::TYPED_EMOTE,
        requests::function::QUERY_ALLEGIANCE,
        requests::function::USE,
        requests::function::ATTACK,
        allegiance::function::SWEAR,
        allegiance::function::BREAK,
        allegiance::function::ACCEPT_VASSAL,
        allegiance::function::DECLINE_VASSAL,
        allegiance::function::TREE_EXPORT,
        fellowship::function::CREATE,
        fellowship::function::RECRUIT,
        fellowship::function::ACCEPT,
        fellowship::function::DECLINE,
        fellowship::function::LEAVE,
        fellowship::function::DISMISS,
        fellowship::function::DISBAND,
        trade::function::OPEN,
        trade::function::OFFER,
        trade::function::REVOKE,
        trade::function::ACCEPT,
        trade::function::DECLINE,
        trade::function::CLOSE,
        scenes::function::DONE,
        market::function::ENTER_STORE,
        market::function::ENTER_CONSIGNMENT,
        market::function::SELL,
        market::function::COLLECT,
        market::function::LEAVE_CATALOG,
        market::function::LEAVE_CONSIGNMENT,
        follow::FOLLOW,
        mounts::DISMOUNT,
        personal::function::SET_ALIAS,
        personal::function::INSCRIBE,
        personal::function::LIST_SKILLS,
        quests::CANCEL_QUEST,
        combat::function::START_ATTACK,
        combat::function::STOP_ATTACK,
        combat::function::STOP_ATTACKING,
        combat::function::SET_NEXT_TARGET,
        craft::function::ADD_INGREDIENT,
        craft::function::EXECUTE,
        craft::function::RAISE,
        inventory::function::CLOSE,
        drops::function::MOVE_ITEM,
        equip::function::DIRECTIVE_EQUIP,
        equip::function::DIRECTIVE_UNEQUIP,
        pack::function::REORGANIZE,
        pack::function::TO_MONEY_BAG,
        pack::function::FROM_MONEY_BAG,
        pack::function::TAKE_ALL,
        inventory::function::TRANSMUTE_ALL,
        store::function::NEXT_SALES,
        store::function::PREV_SALES,
        store::function::LEAVE_STORE,
        store::function::PURCHASE,
        skills::function::TRAIN,
        skills::function::UNTRAIN,
        skills::function::UNTRAIN_CANCEL,
        skills::function::RAISE,
        death::player_fn::RESURRECT,
        pets::function::ATTACK,
        pets::function::RETURN,
        pets::function::MODE,
        pets::function::DIE,
        pets::function::NAME,
        session::request::GAME_OPTIONS,
        session::request::SHORTCUT,
        session::request::WINDOW_SETTINGS,
        session::request::CHAT_OPTIONS,
    ]
}

/// The script packages a call's description may be of.
const PACKAGES: [u16; 9] = [
    0x245,
    inventory::MOVE_DESC,
    inventory::TRANSMUTE_DESC,
    pack::TAKE_ALL_DESC,
    trade::RECORD,
    0x1B76,
    0x1F8,
    1,
    0xFFFF,
];

/// A string of a length chosen to hurt.
fn text(d: &mut Dice) -> Arg {
    let n = [0, 1, 40, 255, 256, 5_000][d.below(6)];
    let c = [0u16, b'a'.into(), b'@'.into(), b'/'.into(), 0xD800, 0xFFFF][d.below(6)];
    let mut b = Builder::new();
    b.native(
        0x76,
        auberean_script::native_object::NativeObject::Text(vec![c; n]),
    );
    Arg::Object(b.graph())
}

/// A move of an item as the client asks for one, every number a lie.
fn a_move(d: &mut Dice, ids: &[u64], word: fn(&mut Dice) -> u32) -> Arg {
    let pick = |d: &mut Dice| ids[d.below(ids.len())];
    move_graph(&Move {
        move_type: [0, drops::DROP, yalain_world::gives::GIVE, word(d)][d.below(4)],
        item: pick(d),
        split_item: pick(d),
        target: [0, pick(d)][d.below(2)],
        from: pick(d),
        quantity: word(d),
        split_quantity: word(d),
        target_slot: word(d).cast_signed(),
        from_slot: word(d).cast_signed(),
        ..Move::default()
    })
}

/// Any byte.
fn byte(d: &mut Dice) -> u8 {
    d.next().to_le_bytes()[0]
}

/// A word chosen to hurt.
fn hurting_word(d: &mut Dice) -> u32 {
    match d.below(8) {
        0 => 0,
        1 => 1,
        2 => u32::MAX,
        3 => 0x7FFF_FFFF,
        4 => 0x8000_0000,
        5 => 100,
        6 => 1000,
        _ => {
            #[allow(clippy::cast_possible_truncation)]
            let w = d.next() as u32;
            w
        }
    }
}

/// A value chosen to hurt: `ids` are what there is to name.
fn value(d: &mut Dice, ids: &[u64]) -> Arg {
    let word = hurting_word;
    let long = |d: &mut Dice| -> u64 {
        match d.below(4) {
            0 => 0,
            1 => u64::MAX,
            _ => ids[d.below(ids.len())],
        }
    };
    match d.below(11) {
        0 | 1 => Arg::Int(word(d)),
        2 | 3 => Arg::LongInt(long(d)),
        4 => Arg::Float(f32::NAN.to_bits()),
        5 => Arg::LongFloat(f64::INFINITY.to_bits()),
        6 => text(d),
        7 => a_move(d, ids, hurting_word),
        8 => yalain_testkit::play::use_of(long(d), long(d)),
        _ => {
            let package = PACKAGES[d.below(PACKAGES.len())];
            let n = match d.below(4) {
                0 => 0,
                1 => 3,
                2 => 16,
                _ => 51,
            };
            let slots: Vec<Slot> = (0..n)
                .map(|_| match d.below(4) {
                    0 => Slot::Int(word(d)),
                    1 => Slot::Long(long(d)),
                    2 => Slot::Ref(None),
                    _ => Slot::Int(0),
                })
                .collect();
            let mut b = Builder::new();
            b.object(package, &slots);
            Arg::Object(b.graph())
        }
    }
}

fn world_of(h: &Harness, id: u64) -> ConnId {
    *h.server
        .players
        .iter()
        .find(|(_, p)| p.character == id)
        .expect("in the world")
        .0
}

/// What is another player's alone: its items and its money.
fn belongings(h: &Harness, id: u64) -> (Vec<(u64, u32, u32)>, u32) {
    let p = &h.server.players[&world_of(h, id)];
    let mut items: Vec<_> = p
        .play
        .items
        .iter()
        .map(|x| (x.id, x.quantity, x.location))
        .collect();
    items.sort_unstable();
    (items, yalain_world::store::money(&p.qualities))
}

/// What each value of a call is, as the handler reads it.
#[derive(Clone, Copy)]
enum V {
    I,
    L,
    T,
    /// A move's description.
    Mv,
    /// A use's description.
    Us,
    /// An equipping's description.
    Eq,
    /// A take-all's, a turning into money's, a purchase's description.
    Ta,
    Tm,
    Pu,
    /// A craft's table of ingredients.
    Tb,
}

/// The values each call reads, in packing order: the function's last parameter first.
fn shape(f: u32) -> &'static [V] {
    use yalain_world::{allegiance, combat, craft, death, equip, fellowship, follow, market};
    use yalain_world::{mounts, personal, pets, quests, skills, store};
    use V::{Eq, Mv, Pu, Ta, Tb, Tm, Us, I, L, T};
    match f {
        requests::function::SET_SELECTION
        | requests::function::QUERY_ALLEGIANCE
        | allegiance::function::SWEAR
        | allegiance::function::BREAK
        | allegiance::function::ACCEPT_VASSAL
        | allegiance::function::DECLINE_VASSAL
        | fellowship::function::RECRUIT
        | fellowship::function::ACCEPT
        | fellowship::function::DECLINE
        | fellowship::function::DISMISS
        | trade::function::OPEN
        | trade::function::REVOKE
        | market::function::ENTER_CONSIGNMENT
        | follow::FOLLOW
        | combat::function::START_ATTACK
        | combat::function::STOP_ATTACKING
        | inventory::function::CLOSE
        | pets::function::ATTACK
        | pets::function::RETURN
        | pets::function::DIE => &[L],
        requests::function::SAY | requests::function::TELL => &[I, T, T],
        requests::function::EMOTE
        | quests::CANCEL_QUEST
        | craft::function::RAISE
        | skills::function::TRAIN
        | skills::function::UNTRAIN
        | skills::function::RAISE => &[I],
        requests::function::TYPED_EMOTE | allegiance::function::TREE_EXPORT => &[T],
        requests::function::USE => &[Us],
        requests::function::ATTACK => &[I, I, L],
        fellowship::function::CREATE => &[I, I, T],
        trade::function::OFFER => &[I, L],
        market::function::ENTER_STORE => &[I, L],
        market::function::SELL => &[I, I, I, L, L],
        market::function::COLLECT => &[I, I, L],
        personal::function::SET_ALIAS => &[I, T, T],
        craft::function::ADD_INGREDIENT => &[I, L, I, I],
        craft::function::EXECUTE => &[L, Tb, I, I],
        yalain_world::drops::function::MOVE_ITEM | pack::function::REORGANIZE => &[Mv],
        equip::function::DIRECTIVE_EQUIP | equip::function::DIRECTIVE_UNEQUIP => &[Eq],
        pack::function::TO_MONEY_BAG => &[L, I, I, L],
        pack::function::FROM_MONEY_BAG => &[L, I, I],
        pack::function::TAKE_ALL => &[Ta],
        inventory::function::TRANSMUTE_ALL => &[Tm],
        store::function::PURCHASE => &[Pu],
        pets::function::MODE => &[I, L],
        pets::function::NAME => &[T, L],
        session::request::GAME_OPTIONS => &[I, I, I, I],
        session::request::SHORTCUT => &[I, I],
        death::player_fn::RESURRECT | mounts::DISMOUNT => &[],
        _ => &[],
    }
}

/// A long slot and its high half, as the descriptions pack them.
fn long_slots(v: u64) -> [Slot; 2] {
    #[allow(clippy::cast_possible_truncation)]
    [
        Slot::Tagged(yalain_world::graph::tag::LONG, v as u32),
        Slot::Tagged(yalain_world::graph::tag::HIGH, (v >> 32) as u32),
    ]
}

/// A value of a kind, lying.
fn of_kind(d: &mut Dice, ids: &[u64], k: V) -> Arg {
    let long = |d: &mut Dice| ids[d.below(ids.len())];
    match k {
        V::I => Arg::Int(hurting_word(d)),
        V::L => Arg::LongInt(long(d)),
        V::T => text(d),
        V::Mv => a_move(d, ids, hurting_word),
        V::Us => yalain_testkit::play::use_of(long(d), long(d)),
        V::Eq => {
            let mut slots = vec![Slot::Int(0); 3];
            slots.extend(long_slots(long(d)));
            slots.push(Slot::Int(hurting_word(d)));
            slots.push(Slot::Int(hurting_word(d)));
            slots.push(Slot::Int(hurting_word(d)));
            slots.resize(18, Slot::Int(0));
            slots.push(Slot::Int(hurting_word(d)));
            let mut b = Builder::new();
            b.object(yalain_world::equip::EQUIP_DESC.0, &slots);
            Arg::Object(b.graph())
        }
        V::Ta => {
            let mut slots = vec![Slot::Int(0); 6];
            slots.extend(long_slots(long(d)));
            slots.extend([Slot::Ref(None), Slot::Int(0), Slot::Int(0), Slot::Ref(None)]);
            slots.extend(long_slots(long(d)));
            let mut b = Builder::new();
            b.object(pack::TAKE_ALL_DESC, &slots);
            Arg::Object(b.graph())
        }
        V::Tm => {
            let mut slots = vec![Slot::Int(0); 8];
            slots.extend(long_slots(long(d)));
            slots.extend(std::iter::repeat_n(Slot::Int(0), 6));
            let mut b = Builder::new();
            b.object(inventory::TRANSMUTE_DESC, &slots);
            Arg::Object(b.graph())
        }
        V::Pu => {
            let mut b = Builder::new();
            let mut slots = long_slots(long(d)).to_vec();
            slots.push(Slot::Ref(None));
            slots.extend(long_slots(long(d)));
            slots.push(Slot::Int(hurting_word(d)));
            slots.extend(long_slots(long(d)));
            slots.extend([Slot::Int(0), Slot::Int(0)]);
            b.object(0x1B76, &slots);
            Arg::Object(b.graph())
        }
        V::Tb => {
            use auberean_script::library::Elem;
            use auberean_script::native_object::Item;
            let n = d.below(5);
            let entries: Vec<(Item, Item)> = (0..n)
                .map(|_| (Item::Word(hurting_word(d)), Item::Long(long(d))))
                .collect();
            let mut b = Builder::new();
            b.native(
                8,
                yalain_world::graph::hash(Elem::Word, Elem::Long, 0, &entries),
            );
            Arg::Object(b.graph())
        }
    }
}

/// A world with something in it to lie about: two players side by side, each holding items and with
/// money, a corpse, a store and a broker, creatures. The players' ids, and all there is to name.
fn a_world() -> (Harness, (usize, u64), (usize, u64), Vec<u64>) {
    use yalain_content::{ContentObject, ContentPack, Description, Facts, Kind as Placed, Props};
    let mut h = Harness::new();
    h.add_creatures(2);
    const BROKER: u64 = 0x40FE_FA00_0000_0003;
    const BROKER_TEMPLATE: u32 = 0x4700_2E8B;
    h.server.content = ContentPack {
        objects: vec![ContentObject {
            object: Some(BROKER),
            template: BROKER_TEMPLATE,
            description: Description {
                package: 0x13A3,
                look: 0x1F00_0100,
                enter_fx: 0,
                scale: None,
                name: None,
                numbers: [0; 6],
                source: "authored for a test".into(),
            },
            cell: 0x0299_0109,
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
        yalain_world::market::Broker {
            catalog: 0x6F00_1503,
            location: (0x0D3C_90C4, 0x2500_0CE6),
        },
    );
    let mut stacks = Props::default();
    stacks.set("Item_MaxStackSize", yalain_content::Prop::Int(100));
    stacks.set("Item_Value", yalain_content::Prop::Int(3));
    h.server
        .catalog
        .insert(yalain_testkit::test_template(0x4700_7003, 0x1067, stacks));
    let a = h.player_in_world("one", "Liar");
    let b = h.player_in_world("two", "Honest");
    let (wa, wb) = (world_of(&h, a.1), world_of(&h, b.1));
    let at = h.server.players[&wa].position;
    h.server
        .players
        .get_mut(&wb)
        .expect("in the world")
        .position = at;
    let mut ids = vec![a.1, b.1, BROKER, 0, 1, u64::MAX];
    for w in [wa, wb] {
        for (template, n) in [(TEST_ITEM, 1), (0x4700_7003, 50), (0x4700_7003, 7)] {
            ids.extend(inventory::give(&mut h.server, &mut h.net, w, template, n));
        }
        let p = h.server.players.get_mut(&w).expect("in the world");
        yalain_world::quals::set_int(&mut p.qualities, yalain_world::store::MONEY, 1000);
    }
    h.run(50);
    ids.extend(h.server.things.by_id.keys().copied().take(12));
    (h, a, b, ids)
}

/// Every call of the shape it reads with every value a lie, alive, dead and in portal space: nothing
/// panics, the other player keeps what it has, and the liar has no more than it was given.
/// Behaviour: server.lying-client.calls-hurt-nobody
#[test]
fn every_call_of_its_shape_with_every_value_a_lie_does_no_harm() {
    for seed in [0x5EED_4000, 0x5EED_4001, 0x5EED_4002] {
        let (mut h, (a, me), (_, them), ids) = a_world();
        let before = belongings(&h, them);
        let mut d = Dice(seed);
        for f in calls().into_iter().cycle().take(6000) {
            let args: Vec<Arg> = shape(f).iter().map(|k| of_kind(&mut d, &ids, *k)).collect();
            h.bots[a].send(true, queue::WEENIE, &server_call(f, &args));
            h.run(1);
            if d.below(300) == 0 {
                let clock = h.clock();
                let w = world_of(&h, me);
                yalain_world::death::player_dies(&mut h.server, &mut h.net, w, None, clock);
            }
            if d.below(20) == 0 {
                let t = h.now.0 + 2.0;
                h.run_to(yalain_net::LocalTime(t));
            }
        }
        let t = h.now.0 + 30.0;
        h.run_to(yalain_net::LocalTime(t));
        assert_eq!(
            belongings(&h, them),
            before,
            "the other's own, seed {seed:#x}"
        );
    }
}

/// Every call with values chosen to hurt: nothing panics, and the other player keeps what it has.
/// Behaviour: server.lying-client.calls-hurt-nobody
#[test]
fn every_call_with_values_chosen_to_hurt_panics_nothing_and_takes_nothing_of_another() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, me) = h.player_in_world("one", "Liar");
    let (_, them) = h.player_in_world("two", "Honest");
    let (wa, wb) = (world_of(&h, me), world_of(&h, them));
    let at = h.server.players[&wa].position;
    h.server
        .players
        .get_mut(&wb)
        .expect("in the world")
        .position = at;
    let mine = inventory::give(&mut h.server, &mut h.net, wa, TEST_ITEM, 1).expect("given");
    let theirs = inventory::give(&mut h.server, &mut h.net, wb, TEST_ITEM, 1).expect("given");
    h.run(5);
    let before = belongings(&h, them);
    let ids = [mine, theirs, me, them, 1, 0x8131_0000_0000_0000];
    let mut d = Dice(0x5EED_1000);
    for (n, f) in calls().into_iter().enumerate().cycle().take(4000) {
        let count = d.below(5);
        let args: Vec<Arg> = (0..count).map(|_| value(&mut d, &ids)).collect();
        h.bots[a].send(true, queue::WEENIE, &server_call(f, &args));
        // Now and then it is dead, on its way out in portal space, for what comes next.
        if n % 37 == 0 {
            let clock = h.clock();
            let w = world_of(&h, me);
            yalain_world::death::player_dies(&mut h.server, &mut h.net, w, None, clock);
        }
        h.run(1 + d.below(2));
    }
    let t = h.now.0 + 30.0;
    h.run_to(yalain_net::LocalTime(t));
    assert_eq!(belongings(&h, them), before, "the other's own");
}

/// Every call from places no player should be, whether reported or kept from before.
/// Behaviour: server.lying-client.calls-hurt-nobody
#[test]
fn every_call_from_a_place_off_the_land_takes_nothing_of_another() {
    let (done, ended) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let cells = [
            0x0299_0009u32,
            0x0299_0109,
            0xFFFF_0041,
            0x0000_0001,
            0xFFFF_FFFF,
        ];
        let coords = [
            0.0,
            -1.0,
            159.999,
            160.0,
            40_960.0,
            -40_960.0,
            1.0e10,
            -1.0e10,
            1.0e30,
            f32::MAX,
            f32::MIN,
            f32::INFINITY,
            f32::NAN,
        ];
        let (mut h, (a, me), (_, them), ids) = a_world();
        let before = belongings(&h, them);
        let mut d = Dice(0x5EED_5000);
        for f in calls().into_iter().cycle().take(4000) {
            let w = world_of(&h, me);
            if d.below(4) == 0 {
                let origin = [
                    coords[d.below(coords.len())],
                    coords[d.below(coords.len())],
                    coords[d.below(coords.len())],
                ];
                let cell = cells[d.below(cells.len())];
                if let Some(p) = h.server.players.get_mut(&w) {
                    p.position.cell = cell;
                    p.position.origin = origin;
                }
                // What it holds is put down from there, as a client asks.
                if let Some(held) = h.server.players[&w].play.items.first().copied() {
                    let m = Move {
                        move_type: drops::DROP,
                        item: held.id,
                        split_item: held.id,
                        from: me,
                        actual_from: me,
                        quantity: held.quantity.max(1),
                        quantity_left: held.quantity.max(1),
                        ..Move::default()
                    };
                    let call = server_call(drops::function::MOVE_ITEM, &[move_graph(&m)]);
                    h.bots[a].send(true, queue::WEENIE, &call);
                }
            }
            let args: Vec<Arg> = shape(f).iter().map(|k| of_kind(&mut d, &ids, *k)).collect();
            h.bots[a].send(true, queue::WEENIE, &server_call(f, &args));
            h.run(1);
            if d.below(20) == 0 {
                let t = h.now.0 + 2.0;
                h.run_to(yalain_net::LocalTime(t));
            }
        }
        let t = h.now.0 + 30.0;
        h.run_to(yalain_net::LocalTime(t));
        let _ = done.send(belongings(&h, them) == before);
    });
    assert_eq!(
        ended.recv_timeout(std::time::Duration::from_secs(120)),
        Ok(true),
        "the other's own"
    );
}

/// A call far bigger than any client sends is neither kept nor said to anyone.
/// Behaviour: server.lying-client.a-call-too-big-is-dropped
#[test]
fn a_call_far_too_big_is_neither_kept_nor_said() {
    use auberean_script::native_object::NativeObject;
    let (mut h, (a, me), (b, _), _) = a_world();
    let w = world_of(&h, me);
    let big = |n: usize| {
        let mut g = Builder::new();
        g.native(0x76, NativeObject::Text(vec![u16::from(b'a'); n]));
        Arg::Object(g.graph())
    };
    let from = h.bots[b].inbox.len();
    let f = requests::function::SAY;
    h.ask(a, f, &[Arg::Int(0), big(40_000), big(1)], 10);
    assert_eq!(h.calls(b, from).len(), 0, "said to nobody");
    // A shortcut holding as much is not kept.
    let mut g = Builder::new();
    let mut packed = 0x4000_0003u32.to_le_bytes().to_vec();
    let n = 40_000u32;
    packed.extend(n.to_le_bytes());
    packed.resize(packed.len() + 80_000, b'a');
    g.native(
        yalain_world::uploads::kind::SHORTCUT,
        NativeObject::Packed {
            ty: yalain_world::uploads::kind::SHORTCUT.into(),
            bytes: packed,
        },
    );
    let f = session::request::SHORTCUT;
    h.ask(a, f, &[Arg::Object(g.graph()), Arg::Int(3)], 10);
    assert!(
        h.server.players[&w]
            .options
            .shortcuts
            .iter()
            .all(|x| x.len() < 1000),
        "not kept"
    );
}

/// Datagrams bitten on their way to the server, and rubbish from nowhere: the transport refuses what
/// does not read, and nothing panics.
/// Behaviour: server.lying-client.messages-cut-and-bitten-do-no-harm
#[test]
fn datagrams_bitten_on_the_way_panic_nothing() {
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Liar");
    let mut d = Dice(0x5EED_6000);
    for n in 0..6000 {
        h.bots[a].send(
            true,
            queue::WEENIE,
            &server_call(requests::function::SET_SELECTION, &[Arg::LongInt(me)]),
        );
        h.run(1);
        for packet in h.wire.take() {
            let to_server = yalain_net::Port::ALL
                .into_iter()
                .find(|p| yalain_testkit::port_addr(*p) == packet.to);
            let Some(port) = to_server else {
                h.wire.send(packet.from, packet.to, packet.bytes);
                continue;
            };
            let mut bytes = packet.bytes;
            if n % 2 == 0 {
                match d.below(3) {
                    0 => {
                        let cut = d.below(bytes.len() + 1);
                        bytes.truncate(cut);
                    }
                    1 => {
                        let at = d.below(bytes.len().max(1));
                        if let Some(b) = bytes.get_mut(at) {
                            *b ^= 1 << d.below(8);
                        }
                    }
                    _ => {
                        bytes.clear();
                        for _ in 0..d.below(600) {
                            bytes.push(byte(&mut d));
                        }
                    }
                }
            }
            let now = h.now;
            h.net.on_datagram(port, packet.from, &bytes, now);
        }
    }
}

/// Where the client says it stands: every kind of nonsense, through the world's own reading of it.
/// Behaviour: server.lying-client.calls-hurt-nobody
#[test]
fn a_place_reported_as_nonsense_panics_nothing() {
    let mut h = Harness::new();
    let (a, me) = h.player_in_world("one", "Liar");
    let (_, them) = h.player_in_world("two", "Honest");
    h.run(5);
    let mut d = Dice(0x5EED_3000);
    let cells = [
        0u32,
        1,
        0x0298_0109,
        0xFFFF_FFFF,
        0x0000_FFFF,
        0xFFFF_0000,
        0x8000_0000,
    ];
    let coords = [
        0.0,
        -0.0,
        1.0e30,
        -1.0e30,
        f32::MAX,
        f32::MIN,
        f32::INFINITY,
        f32::NAN,
        192.0,
        -1.0,
    ];
    for _ in 0..3000 {
        let w = h
            .server
            .players
            .iter()
            .find(|(_, p)| p.character == me)
            .map(|(_, p)| p.teleport_stamp)
            .unwrap_or(0);
        let origin = auberean_primitives::Vec3::new(
            coords[d.below(coords.len())],
            coords[d.below(coords.len())],
            coords[d.below(coords.len())],
        );
        let m = yalain_testkit::play::report(
            cells[d.below(cells.len())],
            origin,
            byte(&mut d),
            hurting_word(&mut d),
            w,
        );
        h.bots[a].send(true, queue::WEENIE, &m);
        let id = [me, them, 0, u64::MAX][d.below(4)];
        let more = match d.below(3) {
            0 => auberean_protocol::ClientMessage::LookAt(Target(id)),
            1 => auberean_protocol::ClientMessage::ForceCreate(Target(id)),
            _ => auberean_protocol::ClientMessage::SceneRenderingComplete(
                auberean_protocol::login::Empty,
            ),
        };
        h.bots[a].send(true, queue::WEENIE, &more);
        h.run(1);
    }
    assert!(h.server.players.values().any(|p| p.character == them));
}

/// Whole messages cut short and bitten, on both connections: nothing panics.
/// Behaviour: server.lying-client.messages-cut-and-bitten-do-no-harm
#[test]
fn messages_cut_short_and_bitten_panic_nothing() {
    let mut h = Harness::new();
    h.server.catalog = yalain_testkit::test_catalog();
    let (a, me) = h.player_in_world("one", "Liar");
    let w = world_of(&h, me);
    let item = inventory::give(&mut h.server, &mut h.net, w, TEST_ITEM, 1).expect("given");
    h.run(5);
    let ids = [item, me, 0];
    let mut d = Dice(0x5EED_2000);
    let functions = calls();
    let queues = [queue::WEENIE, queue::LOGON, queue::EVENT, queue::DATABASE];
    for _ in 0..6000 {
        let f = functions[d.below(functions.len())];
        let count = d.below(4);
        let args: Vec<Arg> = (0..count).map(|_| value(&mut d, &ids)).collect();
        let mut bytes = server_call(f, &args).encode();
        match d.below(4) {
            0 => {
                let cut = d.below(bytes.len() + 1);
                bytes.truncate(cut);
            }
            1 => {
                for _ in 0..1 + d.below(6) {
                    let at = d.below(bytes.len().max(1));
                    if let Some(b) = bytes.get_mut(at) {
                        *b ^= 1 << d.below(8);
                    }
                }
            }
            2 => {
                let extra = d.below(64);
                for _ in 0..extra {
                    bytes.push(byte(&mut d));
                }
            }
            _ => {
                // A count inflated: four bytes somewhere made the largest.
                let at = d.below(bytes.len().max(1));
                for k in 0..4 {
                    if let Some(b) = bytes.get_mut(at + k) {
                        *b = 0xFF;
                    }
                }
            }
        }
        let world = d.below(4) != 0;
        let q = queues[d.below(queues.len())];
        h.bots[a].send_bytes(world, q, bytes);
        h.run(1);
    }
    assert!(h.server.players.len() <= 1);
}

/// A client naming function after function it can't have keeps the list of what wasn't handled, and
/// with it the server's memory and log, from growing without end.
/// Behaviour: server.operator.the-list-of-what-is-not-handled-is-bounded
#[test]
fn a_client_naming_endless_unknown_calls_does_not_grow_the_servers_list_without_end() {
    let mut u = yalain_world::Unhandled::default();
    for i in 0..3 * yalain_world::unhandled::MOST_KINDS {
        u.note(format!("a call of function {i:#x}"));
    }
    assert_eq!(u.counts().len(), yalain_world::unhandled::MOST_KINDS + 1);
    assert_eq!(
        u.count(yalain_world::unhandled::TOO_MANY),
        2 * yalain_world::unhandled::MOST_KINDS as u64,
        "those past the limit are still counted"
    );
    u.note("a call of function 0x0");
    assert_eq!(
        u.count("a call of function 0x0"),
        2,
        "a kind kept goes on counting"
    );
}

/// A client past the limit has what is over dropped, unread: the others hear only what the limit
/// let through, a player beside goes on being answered, and the flooder is heard again once it has
/// slowed down.
/// Behaviour: server.operator.a-flood-of-calls-is-dropped
#[test]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
fn a_client_flooding_calls_has_what_is_over_dropped_and_others_go_on() {
    use auberean_protocol::{queue, ServerMessage};
    use yalain_testkit::play::server_call;
    use yalain_world::call_limit::{BURST, PER_SECOND};
    use yalain_world::requests::function::SAY;
    let mut h = Harness::new();
    let (flooder, id) = h.player_in_world("one", "Flo");
    let (victim, vid) = h.player_in_world("two", "Vera");
    h.run(10);
    let word = |i: usize| {
        let mut b = Builder::new();
        b.native(
            106,
            auberean_script::native_object::NativeObject::TextInfo(
                auberean_script::native_object::TextInfo {
                    has_literal: 1,
                    literal: Some(format!("word {i}").encode_utf16().collect()),
                    ..Default::default()
                },
            ),
        );
        Arg::Object(b.graph())
    };
    let heard_from = |h: &Harness, b: usize, from: usize, who: u64| {
        h.world_messages(b, from)
            .iter()
            .filter(|m| {
                matches!(m, ServerMessage::CellObjectCall(c)
                if c.object.id == who && c.call.function.0 == yalain_world::chat::function::SAYING)
            })
            .count()
    };
    let flood = 3 * BURST as usize;
    let from = h.bots[victim].inbox.len();
    for i in 0..flood {
        h.bots[flooder].send(
            true,
            queue::WEENIE,
            &server_call(SAY, &[Arg::Int(0), word(i)]),
        );
    }
    h.run(4);
    let heard = heard_from(&h, victim, from, id);
    assert!(heard >= BURST as usize, "the burst goes through: {heard}");
    assert!(heard < flood, "what is over is dropped: {heard}");
    let dropped = h.server.call_limits.dropped(
        *h.server
            .players
            .iter()
            .find(|(_, p)| p.character == id)
            .expect("in")
            .0,
    );
    assert_eq!(dropped as usize + heard, flood);
    // The player beside it is answered as ever.
    let from = h.bots[flooder].inbox.len();
    h.bots[victim].send(
        true,
        queue::WEENIE,
        &server_call(SAY, &[Arg::Int(0), word(0)]),
    );
    h.run(4);
    assert_eq!(heard_from(&h, flooder, from, vid), 1, "the other is heard");
    // Slowed down, it is heard again.
    h.run_to(yalain_net::LocalTime(h.now.0 + 2.0));
    let from = h.bots[victim].inbox.len();
    let some = PER_SECOND as usize;
    for i in 0..some {
        h.bots[flooder].send(
            true,
            queue::WEENIE,
            &server_call(SAY, &[Arg::Int(0), word(i)]),
        );
    }
    h.run(4);
    assert_eq!(heard_from(&h, victim, from, id), some, "heard again");
}
