//! ACE: Source/ACE.Server/WorldObjects/Player_Trade.cs::HandleActionOpenTradeNegotiations
//! Trade open refused out of reach/ignored; full trade swaps items; change after accept resets;
//! attuned refused; receiver cannot carry fails; fellowship invite through ConfirmationResponse;
//! item in trade cannot be used; rotation kept on exact spot.
//! Fixture: a virtual-time TestServer, isolated stores and synthetic dats.

pub(crate) use std::sync::Arc;

pub(crate) use dereth_primitives::ObjectId;
pub(crate) use dereth_protocol::comms::{
    CharacterConfirmationRequest, CharacterConfirmationResponse, CommunicationTransientString,
    CommunicationWeenieError,
};
pub(crate) use dereth_protocol::events::split_ui_blob;
pub(crate) use dereth_protocol::social::{
    FellowshipCreate, FellowshipFullUpdate, FellowshipRecruit,
};
pub(crate) use dereth_protocol::trade::{
    Trade, TradeAcceptTradeRecv, TradeAcceptTradeRequest, TradeAddToTrade, TradeAddToTradeRecv,
    TradeOpenTradeNegotiations, TradeRegisterTrade, TradeResetTradeRecv, TradeTradeFailure,
};
pub(crate) use dereth_protocol::Message;
pub(crate) use empyrean_content::models::world::Weenie;
pub(crate) use empyrean_content::MemContent;
pub(crate) use empyrean_dat::FakeDats;
pub(crate) use empyrean_entity::enums::{
    CharacterOption, CharacterOptions1, ConfirmationType, PropertyAttribute, PropertyDataId,
    PropertyInt, PropertyString, WeenieError, WeenieType,
};
pub(crate) use empyrean_entity::{ObjectGuid, Position};
pub(crate) use empyrean_net::{SessionId, SessionState};
pub(crate) use empyrean_testkit::land::{self, TEST_SETUP};
pub(crate) use empyrean_testkit::{ClientId, TestServer};
pub(crate) use empyrean_world::managers::guid_manager;
pub(crate) use empyrean_world::managers::landblock_manager;
pub(crate) use empyrean_world::world_objects::world_object::CtorEnv;
pub(crate) use empyrean_world::world_objects::{container, player_fellowship, player_trade};
pub(crate) use empyrean_world::World;

pub(crate) const LB: u32 = 0xA9B4_0000;

pub(crate) const PLAYER_WCID: u32 = 1;
pub(crate) const GEM: u32 = 3; // burden 5
pub(crate) const CUP: u32 = 4; // burden 10
pub(crate) const RING: u32 = 5; // attuned
pub(crate) const ANVIL: u32 = 6; // burden 5000

pub(crate) const ALPHA: u32 = 0x5000_0001;
pub(crate) const BRAVO: u32 = 0x5000_0002;

// message kinds: the game-event type inside a 0xF7B0, else the opcode
pub(crate) const REGISTER_TRADE: u32 = 0x01FD;
pub(crate) const ADD_TO_TRADE: u32 = 0x0200;
pub(crate) const ACCEPT_TRADE: u32 = 0x0202;
pub(crate) const RESET_TRADE: u32 = 0x0205;
pub(crate) const TRADE_FAILURE: u32 = 0x0207;
pub(crate) const CLEAR_ACCEPTANCE: u32 = 0x0208;
pub(crate) const CONFIRMATION_REQUEST: u32 = 0x0274;
pub(crate) const WEENIE_ERROR: u32 = 0x028A;
pub(crate) const FULL_UPDATE: u32 = 0x02BE;
pub(crate) const TRANSIENT: u32 = 0x02EB;

/// The UI-queue game events a trade sends (one queue, so the client sees ACE's send order).
pub(crate) const TRADE_EVENTS: &[u32] = &[
    REGISTER_TRADE,
    ADD_TO_TRADE,
    ACCEPT_TRADE,
    RESET_TRADE,
    TRADE_FAILURE,
    CLEAR_ACCEPTANCE,
    CONFIRMATION_REQUEST,
    WEENIE_ERROR,
    TRANSIENT,
];

pub(crate) fn weenie(wcid: u32, name: &str, weenie_type: WeenieType) -> Weenie {
    Weenie::new(wcid, name, weenie_type)
        .with_string(PropertyString::Name, name)
        .with_did(PropertyDataId::Setup, TEST_SETUP)
}

pub(crate) fn content() -> MemContent {
    MemContent::new()
        .weenie(
            weenie(PLAYER_WCID, "human", WeenieType::Creature)
                .with_did(
                    empyrean_entity::enums::PropertyDataId::CombatTable,
                    0x3000_0000,
                )
                .with_int(PropertyInt::ItemsCapacity, 102)
                .with_int(PropertyInt::ContainersCapacity, 7),
        )
        .weenie(
            weenie(GEM, "Gem", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 5)
                .with_int(PropertyInt::Value, 7),
        )
        .weenie(
            weenie(CUP, "Cup", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 10)
                .with_int(PropertyInt::Value, 3),
        )
        .weenie(
            weenie(RING, "Ring", WeenieType::Generic)
                .with_int(PropertyInt::EncumbranceVal, 1)
                .with_int(PropertyInt::Attuned, 1),
        )
        .weenie(
            weenie(ANVIL, "Anvil", WeenieType::Generic).with_int(PropertyInt::EncumbranceVal, 5000),
        )
}

pub(crate) use empyrean_testkit::EmptyShard;

pub(crate) fn at(x: f32, y: f32) -> Position {
    Position::from_components(LB | 0x0001, x, y, 0.0, 0.0, 0.0, 0.0, 1.0, false)
}

pub(crate) fn server() -> TestServer {
    let mut ts = TestServer::with_setup(
        empyrean_testkit::dats::with_stat_tables(FakeDats::new())
            .build()
            .expect("fake dats"),
        |w| {
            w.content = Arc::new(content());
            guid_manager::initialize(w, &mut EmptyShard);
        },
    );
    land::use_flat_land_with_test_setup(&mut ts.world, &[0xA9B4], 0);
    ts
}

/// A client logged in as `account` whose session plays `guid`, a level-5 player standing at `pos`
/// with `strength` (burden 0) and no character options set (so a fellowship invitation asks).
/// Stands in for enter-world, as `inventory.rs` joins.
pub(crate) fn join(
    ts: &mut TestServer,
    account: &str,
    guid: u32,
    name: &str,
    pos: Position,
    strength: u32,
) -> (ClientId, SessionId) {
    let before: Vec<SessionId> = ts.world.sessions.iter().map(|(id, _)| id).collect();
    let id = ts.connect(account, "pw");
    ts.advance(0.1);
    let session = ts
        .world
        .sessions
        .iter()
        .map(|(id, _)| id)
        .find(|s| !before.contains(s))
        .expect("the new session");

    let w = &mut ts.world;
    let weenie = w
        .content
        .get_cached_weenie(PLAYER_WCID)
        .expect("player weenie");
    let mut o = CtorEnv::with_world(w, |env| {
        empyrean_world::world_objects::player::player_from_weenie(
            env,
            empyrean_world::dispatch::Class::Player,
            weenie,
            ObjectGuid::new(guid),
            1,
        )
    });
    o.set_property(PropertyString::Name, name.to_owned());
    let rec = o
        .biota
        .properties_attribute
        .get_or_insert_with(Default::default)
        .get_or_insert_with(PropertyAttribute::Strength, Default::default);
    rec.init_level = strength;
    o.set_level(Some(5));
    o.set_encumbrance_val(Some(0));
    o.set_value(Some(0));
    let character = empyrean_store::models::shard::Character {
        character_options_1: i32::try_from(CharacterOptions1::AllowGive.0).unwrap(),
        ..Default::default()
    };
    o.player.as_mut().expect("a player").player.character = Some(character);
    o.set_location(Some(pos));
    w.objects.insert(o).expect("fresh");
    w.player_manager.online_players.insert(
        guid,
        empyrean_world::managers::player_manager::OnlinePlayer {
            guid: ObjectGuid::new(guid),
            account: None,
        },
    );

    let s = w.sessions.get_mut(session).expect("the game half");
    s.state = SessionState::WorldConnected;
    s.set_player(Some(ObjectGuid::new(guid)));
    assert!(
        landblock_manager::add_object(w, ObjectGuid::new(guid), false),
        "the player joins its landblock"
    );
    ts.advance(0.1);
    (id, session)
}

/// A new object of `wcid` in `player`'s main pack (no burden check: the anvil goes in regardless).
pub(crate) fn in_pack(w: &mut World, player: u32, wcid: u32) -> ObjectGuid {
    let weenie = w.content.get_cached_weenie(wcid).expect("test weenie");
    let guid = guid_manager::new_dynamic_guid(w);
    let o = CtorEnv::with_world(w, |env| {
        empyrean_world::factories::world_object_factory::create_world_object(
            env,
            Some(weenie),
            guid,
        )
    })
    .expect("constructible");
    w.objects.insert(o).expect("fresh");
    assert!(container::try_add_to_inventory(
        w,
        ObjectGuid::new(player),
        guid,
        0,
        false,
        false
    ));
    guid
}

/// A received message: its kind and the whole blob.
#[derive(Debug, Clone)]
pub(crate) struct Got {
    pub(crate) kind: u32,
    pub(crate) blob: Vec<u8>,
}

impl Got {
    pub(crate) fn decode<M: Message>(&self) -> M {
        let split = split_ui_blob(&self.blob).expect("a blob");
        let mut body = split.body;
        M::read(&mut body).unwrap_or_else(|e| panic!("0x{:04X} decodes: {e:?}", self.kind))
    }
}

pub(crate) fn got(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    ts.received_raw(id)[from..]
        .iter()
        .map(|m| {
            let mut blob = m.opcode.to_le_bytes().to_vec();
            blob.extend_from_slice(&m.body);
            let kind = if m.opcode == 0xF7B0 {
                u32::from_le_bytes(m.body[8..12].try_into().unwrap())
            } else {
                m.opcode
            };
            Got { kind, blob }
        })
        .collect()
}

/// The trade events a client received since message `from`, in order.
pub(crate) fn trade_events(ts: &TestServer, id: ClientId, from: usize) -> Vec<Got> {
    got(ts, id, from)
        .into_iter()
        .filter(|g| TRADE_EVENTS.contains(&g.kind))
        .collect()
}

pub(crate) fn kinds(g: &[Got]) -> Vec<u32> {
    g.iter().map(|g| g.kind).collect()
}

pub(crate) fn transients(g: &[Got]) -> Vec<String> {
    g.iter()
        .filter(|m| m.kind == TRANSIENT)
        .map(|m| m.decode::<CommunicationTransientString>().text)
        .collect()
}

pub(crate) fn weenie_errors(g: &[Got]) -> Vec<u32> {
    g.iter()
        .filter(|m| m.kind == WEENIE_ERROR)
        .map(|m| m.decode::<CommunicationWeenieError>().error_type)
        .collect()
}

pub(crate) fn accept() -> TradeAcceptTradeRequest {
    TradeAcceptTradeRequest(Trade::default())
}

/// Two players side by side with a registered trade, opened over the wire: Alpha's
/// `OpenTradeNegotiations` runs `CreateMoveToChain` (Bravo is already within use radius), whose
/// callback sends Alpha `RegisterTrade(Guid, tradePartner.Guid)` and then runs Bravo's
/// `HandleActionOpenTradeNegotiations(Guid.Full, false)`, which sends Bravo
/// `RegisterTrade(tradePartner.Guid, tradePartner.Guid)`.
pub(crate) fn open_trade(
    alpha_strength: u32,
    bravo_strength: u32,
) -> (TestServer, ClientId, ClientId) {
    let mut ts = server();
    let (a, _) = join(
        &mut ts,
        "alpha",
        ALPHA,
        "Alpha",
        at(20.0, 20.0),
        alpha_strength,
    );
    let (b, _) = join(
        &mut ts,
        "bravo",
        BRAVO,
        "Bravo",
        at(20.5, 20.0),
        bravo_strength,
    );

    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(
        a,
        &TradeOpenTradeNegotiations {
            partner: ObjectId(BRAVO),
        },
    );
    assert!(
        ts.run_until(5.0, |ts| !trade_events(ts, b, nb).is_empty()),
        "the MoveTo arrives and the partner is registered"
    );
    ts.advance(0.1);
    let ga = trade_events(&ts, a, na);
    assert_eq!(kinds(&ga), [REGISTER_TRADE], "initiator");
    // the initiator: `GameEventRegisterTrade(Session, Guid, tradePartner.Guid)`
    let r: TradeRegisterTrade = ga[0].decode();
    assert_eq!((r.initiator.0, r.partner.0, r.stamp), (ALPHA, BRAVO, 0.0));
    let gb = trade_events(&ts, b, nb);
    assert_eq!(kinds(&gb), [REGISTER_TRADE], "partner");
    // the partner: `GameEventRegisterTrade(Session, tradePartner.Guid, tradePartner.Guid)`
    let r: TradeRegisterTrade = gb[0].decode();
    assert_eq!((r.initiator.0, r.partner.0, r.stamp), (ALPHA, ALPHA, 0.0));
    for g in [ALPHA, BRAVO] {
        assert!(player_trade::is_trading(&ts.world, ObjectGuid::new(g)));
    }
    assert_eq!(
        player_trade::trade_partner(&ts.world, ObjectGuid::new(ALPHA)),
        ObjectGuid::new(BRAVO)
    );
    assert_eq!(
        player_trade::trade_partner(&ts.world, ObjectGuid::new(BRAVO)),
        ObjectGuid::new(ALPHA)
    );
    (ts, a, b)
}

/// Opening over the wire with a partner out of reach: the MoveTo gives up after 15 s and ACE
/// answers `TradeMaxDistanceExceeded`; no trade is registered. A partner ignoring trade requests
/// is refused at once with `TradeIgnoringRequests`.
#[test]
pub(crate) fn opening_a_trade_out_of_reach_or_ignored_is_refused() {
    let mut ts = server();
    let (a, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let (_b, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(70.0, 20.0), 100);

    let na = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &TradeOpenTradeNegotiations {
            partner: ObjectId(BRAVO),
        },
    );
    ts.advance(14.5);
    assert!(trade_events(&ts, a, na).is_empty(), "still walking");
    ts.advance(1.0);
    let ga = trade_events(&ts, a, na);
    assert_eq!(kinds(&ga), [WEENIE_ERROR]);
    assert_eq!(
        weenie_errors(&ga),
        [WeenieError::TradeMaxDistanceExceeded.0.cast_unsigned()]
    );
    assert!(!player_trade::is_trading(&ts.world, ObjectGuid::new(ALPHA)));
    assert!(!player_trade::is_trading(&ts.world, ObjectGuid::new(BRAVO)));

    // Bravo ignores all trade requests
    let character = ts
        .world
        .objects
        .get_mut(ObjectGuid::new(BRAVO))
        .unwrap()
        .player
        .as_mut()
        .unwrap()
        .player
        .character
        .as_mut()
        .unwrap();
    character.character_options_1 |= i32::try_from(
        CharacterOption::IgnoreAllTradeRequests
            .character_options1()
            .expect("an options-1 flag")
            .0,
    )
    .unwrap();
    let na = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &TradeOpenTradeNegotiations {
            partner: ObjectId(BRAVO),
        },
    );
    ts.advance(0.2);
    assert_eq!(
        weenie_errors(&trade_events(&ts, a, na)),
        [WeenieError::TradeIgnoringRequests.0.cast_unsigned()]
    );
}

/// `player` offers `item`: its own window shows it on its side, the partner's (1 ms later) on the
/// partner side.
pub(crate) fn offer(ts: &mut TestServer, player: ClientId, partner: ClientId, item: ObjectGuid) {
    let (np, nq) = (
        ts.received_raw(player).len(),
        ts.received_raw(partner).len(),
    );
    ts.send_game_action(
        player,
        &TradeAddToTrade {
            item: ObjectId(item.full()),
            slot: 0,
        },
    );
    ts.advance(0.2);
    let gp = trade_events(ts, player, np);
    let gq = trade_events(ts, partner, nq);
    assert_eq!(kinds(&gp), [ADD_TO_TRADE]);
    assert_eq!(kinds(&gq), [ADD_TO_TRADE]);
    let m: TradeAddToTradeRecv = gp[0].decode();
    assert_eq!((m.item.0, m.side), (item.full(), 1));
    let m: TradeAddToTradeRecv = gq[0].decode();
    assert_eq!((m.item.0, m.side), (item.full(), 2));
    // `target.TrackObject(wo)`: the partner is sent the item
    assert!(
        got(ts, partner, nq).iter().any(|g| g.kind == 0xF745
            && u32::from_le_bytes(g.blob[4..8].try_into().unwrap()) == item.full()),
        "the partner is sent the offered item"
    );
}

pub(crate) fn inventory(ts: &TestServer, player: u32) -> Vec<ObjectGuid> {
    let mut v = container::inventory_values(&ts.world, ObjectGuid::new(player));
    v.sort_unstable_by_key(|g| g.full());
    v
}

/// A full trade: both offer, both accept, and 0.5 s later the items change hands.
#[test]
pub(crate) fn a_full_trade_swaps_the_items() {
    let (mut ts, a, b) = open_trade(100, 100);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    let cup = in_pack(&mut ts.world, BRAVO, CUP);
    ts.advance(0.1);

    offer(&mut ts, a, b, gem);
    offer(&mut ts, b, a, cup);

    // Alpha accepts: both are told
    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(a, &accept());
    ts.advance(0.1);
    let ga = trade_events(&ts, a, na);
    let gb = trade_events(&ts, b, nb);
    assert_eq!(kinds(&ga), [ACCEPT_TRADE, TRANSIENT]);
    assert_eq!(kinds(&gb), [ACCEPT_TRADE, TRANSIENT]);
    assert_eq!(ga[0].decode::<TradeAcceptTradeRecv>().source.0, ALPHA);
    assert_eq!(gb[0].decode::<TradeAcceptTradeRecv>().source.0, ALPHA);
    assert_eq!(transients(&ga), ["You have accepted the offer"]);
    assert_eq!(transients(&gb), ["Alpha has accepted the offer"]);
    assert_eq!(inventory(&ts, ALPHA), [gem], "nothing moves on one accept");

    // Bravo accepts: the double accept finalises (Bravo is `this`, Alpha the target)
    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(b, &accept());
    ts.advance(0.1);
    let gb_now = trade_events(&ts, b, nb);
    assert_eq!(
        transients(&gb_now),
        ["You have accepted the offer", "The items are being traded"]
    );
    assert!(
        ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .unwrap()
            .wo
            .world_object
            .is_busy,
        "both busy during the transfer"
    );
    assert!(player_trade::trade_transfer_in_progress(
        &ts.world,
        ObjectGuid::new(BRAVO)
    ));

    ts.advance(0.5);
    let ga = trade_events(&ts, a, na);
    let gb = trade_events(&ts, b, nb);
    assert_eq!(
        kinds(&ga),
        [
            ACCEPT_TRADE,
            TRANSIENT,
            TRANSIENT,
            WEENIE_ERROR,
            RESET_TRADE
        ]
    );
    assert_eq!(
        transients(&ga),
        ["Bravo has accepted the offer", "The items are being traded"]
    );
    assert_eq!(
        kinds(&gb),
        [
            ACCEPT_TRADE,
            TRANSIENT,
            TRANSIENT,
            WEENIE_ERROR,
            RESET_TRADE
        ]
    );
    assert_eq!(
        weenie_errors(&ga),
        [WeenieError::TradeComplete.0.cast_unsigned()]
    );
    assert_eq!(
        weenie_errors(&gb),
        [WeenieError::TradeComplete.0.cast_unsigned()]
    );
    // `HandleActionResetTrade(Guid)` / `target.HandleActionResetTrade(target.Guid)`: each names itself
    assert_eq!(
        ga.last().unwrap().decode::<TradeResetTradeRecv>().source.0,
        ALPHA
    );
    assert_eq!(
        gb.last().unwrap().decode::<TradeResetTradeRecv>().source.0,
        BRAVO
    );

    assert_eq!(inventory(&ts, ALPHA), [cup]);
    assert_eq!(inventory(&ts, BRAVO), [gem]);
    let enc = |g: u32| {
        ts.world
            .objects
            .get(ObjectGuid::new(g))
            .unwrap()
            .encumbrance_val()
    };
    assert_eq!((enc(ALPHA), enc(BRAVO)), (Some(10), Some(5)));
    assert!(
        !ts.world
            .objects
            .get(ObjectGuid::new(ALPHA))
            .unwrap()
            .wo
            .world_object
            .is_busy
    );
    assert!(!player_trade::trade_transfer_in_progress(
        &ts.world,
        ObjectGuid::new(BRAVO)
    ));
    assert!(player_trade::items_in_trade_window(&ts.world, ObjectGuid::new(ALPHA)).is_empty());
    assert!(
        player_trade::is_trading(&ts.world, ObjectGuid::new(ALPHA)),
        "the window stays open after a trade"
    );
}

/// A change after an accept clears both accepts (`HandleActionAddToTrade` sets both
/// `TradeAccepted` false), so the partner's accept does not finalise; accepting again does.
#[test]
pub(crate) fn a_change_after_accept_resets_the_accepts() {
    let (mut ts, a, b) = open_trade(100, 100);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    let cup = in_pack(&mut ts.world, BRAVO, CUP);
    ts.advance(0.1);

    offer(&mut ts, a, b, gem);
    ts.send_game_action(a, &accept());
    ts.advance(0.1);
    assert!(player_trade::trade_accepted(
        &ts.world,
        ObjectGuid::new(ALPHA)
    ));

    // Bravo adds the cup: Alpha's accept is gone
    offer(&mut ts, b, a, cup);
    assert!(!player_trade::trade_accepted(
        &ts.world,
        ObjectGuid::new(ALPHA)
    ));
    assert!(!player_trade::trade_accepted(
        &ts.world,
        ObjectGuid::new(BRAVO)
    ));

    let nb = ts.received_raw(b).len();
    ts.send_game_action(b, &accept());
    ts.advance(1.0);
    assert_eq!(
        transients(&trade_events(&ts, b, nb)),
        ["You have accepted the offer"],
        "no transfer"
    );
    assert_eq!(inventory(&ts, ALPHA), [gem]);

    ts.send_game_action(a, &accept());
    ts.advance(1.0);
    assert_eq!(inventory(&ts, ALPHA), [cup]);
    assert_eq!(inventory(&ts, BRAVO), [gem]);
}

/// An attuned item is refused: "You cannot trade that!" and a TradeFailure naming it.
#[test]
pub(crate) fn an_attuned_item_is_refused() {
    let (mut ts, a, b) = open_trade(100, 100);
    let ring = in_pack(&mut ts.world, ALPHA, RING);
    ts.advance(0.1);

    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(
        a,
        &TradeAddToTrade {
            item: ObjectId(ring.full()),
            slot: 0,
        },
    );
    ts.advance(0.2);
    let ga = trade_events(&ts, a, na);
    assert_eq!(kinds(&ga), [TRANSIENT, TRADE_FAILURE]);
    assert_eq!(transients(&ga), ["You cannot trade that!"]);
    let f: TradeTradeFailure = ga[1].decode();
    assert_eq!(
        (f.item.0, f.reason),
        (ring.full(), WeenieError::AttunedItem.0.cast_unsigned())
    );
    assert!(
        trade_events(&ts, b, nb).is_empty(),
        "the partner sees nothing"
    );
    assert!(player_trade::items_in_trade_window(&ts.world, ObjectGuid::new(ALPHA)).is_empty());
}

/// A trade the receiver cannot carry fails: `VerifyTrade_Inventory` tells each side who is too
/// encumbered and clears both acceptances; nothing moves.
#[test]
pub(crate) fn a_trade_the_receiver_cannot_carry_fails() {
    // Bravo, strength 10: capacity 1500, 4500 burden at most; the anvil weighs 5000
    let (mut ts, a, b) = open_trade(100, 10);
    let anvil = in_pack(&mut ts.world, ALPHA, ANVIL);
    ts.advance(0.1);

    offer(&mut ts, a, b, anvil);
    ts.send_game_action(a, &accept());
    ts.advance(0.1);

    let (na, nb) = (ts.received_raw(a).len(), ts.received_raw(b).len());
    ts.send_game_action(b, &accept());
    ts.advance(1.0);
    let ga = trade_events(&ts, a, na);
    let gb = trade_events(&ts, b, nb);
    // Bravo accepted last, so Bravo is `this`: its own inventory cannot take Alpha's items
    assert_eq!(
        kinds(&gb),
        [ACCEPT_TRADE, TRANSIENT, TRANSIENT, CLEAR_ACCEPTANCE]
    );
    assert_eq!(
        transients(&gb),
        [
            "You have accepted the offer",
            "You are too encumbered to complete the trade!"
        ]
    );
    assert_eq!(
        kinds(&ga),
        [ACCEPT_TRADE, TRANSIENT, TRANSIENT, CLEAR_ACCEPTANCE]
    );
    assert_eq!(
        transients(&ga),
        [
            "Bravo has accepted the offer",
            "Your trading partner is too encumbered to complete the trade!"
        ]
    );

    assert_eq!(inventory(&ts, ALPHA), [anvil]);
    assert!(inventory(&ts, BRAVO).is_empty());
    for g in [ALPHA, BRAVO] {
        assert!(!player_trade::trade_accepted(&ts.world, ObjectGuid::new(g)));
        assert!(player_trade::items_in_trade_window(&ts.world, ObjectGuid::new(g)).is_empty());
    }
}

/// An item in the trade window cannot be used.
#[test]
pub(crate) fn an_item_in_the_trade_window_cannot_be_used() {
    use dereth_protocol::items::InventoryUseWithTargetEvent;
    use dereth_protocol::objects::ItemUseDone;
    let (mut ts, a, b) = open_trade(100, 100);
    let gem = in_pack(&mut ts.world, ALPHA, GEM);
    let cup = in_pack(&mut ts.world, ALPHA, CUP);
    ts.advance(0.1);
    offer(&mut ts, a, b, gem);

    let na = ts.received_raw(a).len();
    ts.send_game_action(
        a,
        &InventoryUseWithTargetEvent {
            object: ObjectId(gem.full()),
            target: ObjectId(cup.full()),
        },
    );
    ts.advance(0.2);
    let done: Vec<ItemUseDone> = got(&ts, a, na)
        .into_iter()
        .filter(|g| g.kind == 0x01C7)
        .map(|g| g.decode())
        .collect();
    assert_eq!(done.len(), 1);
    assert_eq!(
        done[0].failure_type,
        WeenieError::TradeItemBeingTraded.0.cast_unsigned()
    );
}

/// Opening a trade on the partners exact spot keeps the rotation.
/// V318.
#[test]
pub(crate) fn opening_a_trade_on_the_partners_exact_spot_keeps_the_rotation() {
    let mut ts = server();
    let (a, _) = join(&mut ts, "alpha", ALPHA, "Alpha", at(20.0, 20.0), 100);
    let (b, _) = join(&mut ts, "bravo", BRAVO, "Bravo", at(20.0, 20.0), 100);
    let rotation = |ts: &TestServer, g: u32| {
        ts.world
            .objects
            .get(ObjectGuid::new(g))
            .and_then(|o| o.location())
            .expect("a location")
            .rotation()
    };

    let nb = ts.received_raw(b).len();
    ts.send_game_action(
        a,
        &TradeOpenTradeNegotiations {
            partner: ObjectId(BRAVO),
        },
    );
    assert!(
        ts.run_until(5.0, |ts| !trade_events(ts, b, nb).is_empty()),
        "the partner is registered"
    );
    ts.advance(1.0);
    assert!(
        player_trade::is_trading(&ts.world, ObjectGuid::new(ALPHA)),
        "the trade opened"
    );

    let r = rotation(&ts, ALPHA);
    assert_eq!(
        (r.x, r.y, r.z, r.w),
        (0.0, 0.0, 0.0, 1.0),
        "nothing to face: the rotation is kept (ACE: all NaN)"
    );
    let r = rotation(&ts, BRAVO);
    assert_eq!(
        (r.x, r.y, r.z, r.w),
        (0.0, 0.0, 0.0, 1.0),
        "the partner does not turn"
    );

    // the client's next AutonomousPosition (its own facing) replaces the NaN
    use dereth_protocol::movement::{
        AutonomousPosition, MoveTimestamps, MovementAutonomousPosition,
    };
    use dereth_protocol::types::space::{Frame, PositionWire, Quat, Vec3};
    let facing = Quat {
        w: 0.707_106_77,
        x: 0.0,
        y: 0.0,
        z: 0.707_106_77,
    };
    let position = PositionWire {
        objcell_id: LB | 0x0001,
        frame: Frame {
            origin: Vec3 {
                x: 20.0,
                y: 20.0,
                z: 0.0,
            },
            orientation: facing,
        },
    };
    ts.send_game_action(
        a,
        &MovementAutonomousPosition(AutonomousPosition {
            position,
            timestamps: MoveTimestamps::default(),
            contact: 1,
        }),
    );
    ts.advance(0.5);
    let r = rotation(&ts, ALPHA);
    assert_eq!(
        (r.x, r.y, r.z, r.w),
        (0.0, 0.0, facing.z, facing.w),
        "the reported facing replaces it"
    );
}
