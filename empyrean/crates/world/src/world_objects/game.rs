// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/WorldObjects/Game.cs
//! Port of `Source/ACE.Server/WorldObjects/Game.cs`: a chessboard in game. Using it walks the player
//! to it and joins its match; its heartbeat drives the match. With `chess_enabled` off, ACE's
//! legacy stand-in spawns both armies, has them die, and ends the "game" as a loss.

use empyrean_entity::enums::ChessColor;
use empyrean_entity::{ObjectGuid, Position};

use crate::entity::actions::action_chain::ActionChain;
use crate::entity::actions::i_actor::Actor;
use crate::entity::chess::chess_match::{self, ChessMatchRef};
use crate::managers::player_manager;
use crate::network::game_event::events::game_event_game_over::game_event_game_over;
use crate::network::game_event::events::game_event_join_game_response::game_event_join_game_response;
use crate::network::game_event::events::game_event_popup_string::game_event_popup_string;
use crate::network::game_messages::game_message::enqueue_send;
use crate::world_objects::kinds::KindData;
use crate::world_objects::world_object::WorldObject;
use crate::world_objects::{game_piece, player_move};
use crate::World;

/// Non-property fields declared in `Game.cs`.
#[derive(Debug, Default)]
pub struct GameFields {
    // ACE: Game.ChessMatch
    pub chess_match: Option<ChessMatchRef>,

    // ACE: Game.active
    /// The legacy stand-in game is running.
    pub active: bool,
}

fn fields(o: &WorldObject) -> &GameFields {
    match &o.kind {
        KindData::Game(d) => &d.game,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Game",
            o.guid.full()
        ),
    }
}

fn fields_mut(o: &mut WorldObject) -> &mut GameFields {
    let guid = o.guid;
    match &mut o.kind {
        KindData::Game(d) => &mut d.game,
        _ => panic!(
            "System.InvalidCastException: 0x{:08X} is not a Game",
            guid.full()
        ),
    }
}

/// `Game.ChessMatch` (`None` for null or a missing board).
#[must_use]
pub fn chess_match(w: &World, this: ObjectGuid) -> Option<ChessMatchRef> {
    w.objects.get(this).and_then(|o| fields(o).chess_match)
}

/// `Game.ChessMatch = value`.
pub fn set_chess_match(w: &mut World, this: ObjectGuid, value: Option<ChessMatchRef>) {
    if let Some(o) = w.objects.get_mut(this) {
        let old = std::mem::replace(&mut fields_mut(o).chess_match, value);
        w.chess_matches.replace_reference(old, value);
    }
}

// ---- virtual-dispatch targets ----

// ACE: Game.ActOnUse
/// This is raised by Player.HandleActionUseItem. The item does not exist in the players
/// possession. If the item was outside of range, the player will have been commanded to move using
/// DoMoveTo before ActOnUse is called. When this is called, it should be assumed that the player is
/// within range.
pub fn game_act_on_use(w: &mut World, this: ObjectGuid, activator: ObjectGuid) {
    if !w.objects.get(activator).is_some_and(WorldObject::is_player) {
        return;
    }
    let player = activator;

    player_move::create_move_to_chain(
        w,
        player,
        this,
        Box::new(move |w: &mut World, success: bool| {
            if !success {
                return;
            }

            act_on_join(w, this, player);
        }),
        None,
        true,
    );
}

// ACE: Game.ActOnJoin
pub fn act_on_join(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    if !player_manager::property_manager_get_bool(w, "chess_enabled") {
        act_on_join_legacy(w, this, player);
        return;
    }

    let m = if let Some(m) = chess_match(w, this) {
        m
    } else {
        let m = chess_match::new(w, this);
        set_chess_match(w, this, Some(m));
        m
    };

    chess_match::join(w, &m, player);
}

/// `player.Session` (a player without one is ACE's `NullReferenceException`).
fn session_of(w: &World, player: ObjectGuid) -> empyrean_net::SessionId {
    player_manager::player_session(w, player)
        .expect("ACE: Player.Session is null (NullReferenceException)")
}

/// `WorldObjectFactory.CreateNewWorldObject(name) as GamePiece`, at `location`, entered into the
/// world. A missing weenie (or one that is not a GamePiece) is ACE's `NullReferenceException` at
/// `.Location`.
fn spawn_legacy_piece(w: &mut World, name: &str, location: Position) -> ObjectGuid {
    let guid =
        crate::factories::world_object_factory::create_new_world_object_by_name_in_world(w, name)
            .filter(|&g| w.objects.get(g).is_some_and(WorldObject::is_game_piece))
            .expect("ACE: Game.ActOnJoin_Legacy: piece is null (NullReferenceException)");
    if let Some(o) = w.objects.get_mut(guid) {
        o.set_location(Some(location));
    }
    crate::dispatch::enter_world::enter_world(w, guid);
    guid
}

/// Sends the game event `build` makes on the player's session.
fn send_event(
    w: &mut World,
    player: ObjectGuid,
    build: impl FnOnce(
        &mut crate::sessions::SessionData,
    ) -> crate::network::game_messages::game_message::GameMessage,
) {
    let session = session_of(w, player);
    let data = w
        .sessions
        .get_mut(session)
        .expect("ACE: Player.Session is null (NullReferenceException)");
    let msg = build(data);
    enqueue_send(w, session, msg);
}

// ACE: Game.ActOnJoin_Legacy
/// The stand-in game ACE runs with `chess_enabled` off: the player joins as Black, all 32 pieces
/// appear on the board, 5 s later they die and the game ends, 2 s after that a popup and the
/// client's exit from game mode; the player is charged a lost game.
// Not ACE's (a fix, V309): an unset games-lost or total-games counter counts
// as 0, so a player's first legacy game sets both to 1. ACE's increments left a never-set counter
// unset, so a player's legacy games were never counted.
pub fn act_on_join_legacy(w: &mut World, this: ObjectGuid, player: ObjectGuid) {
    {
        let Some(o) = w.objects.get_mut(this) else {
            return;
        };
        if fields(o).active {
            return;
        }

        fields_mut(o).active = true;
    }

    // team is either 0 or 1. -1 means failed to join
    // 0 or 1 for winning team. -1 is used for stalemate, -2 (and gameId of 0) is used to exit game mode in client
    // var msgGameOver = new GameEventGameOver(player.Session, 0, -2);

    // player.Session.Network.EnqueueSend(msgJoinResponse, msgGameOver);
    send_event(w, player, |s| {
        game_event_join_game_response(s, this, ChessColor::Black)
    });

    // 0xA9B2002E [135.97 133.313 94.4447] 1 0 0 0 (holtburg game location)

    let location = w
        .objects
        .get(this)
        .and_then(WorldObject::location)
        .expect("ACE: Game.Location is null (NullReferenceException)");
    let at = |dx: f32, dy: f32, rz: f32, rw: f32| {
        Position::from_components(
            location.cell(),
            location.position_x + dx,
            location.position_y + dy,
            location.position_z,
            0.0,
            0.0,
            rz,
            rw,
            false,
        )
    };
    let files = [-3.5f32, -2.5, -1.5, -0.5, 0.5, 1.5, 2.5, 3.5];

    // Drudges: the back rank, then the pawns (rotation 0, 0, 0, 1)
    let back = [
        "drudgerook",
        "drudgeknight",
        "drudgebishop",
        "drudgequeen",
        "drudgeking",
        "drudgebishop",
        "drudgeknight",
        "drudgerook",
    ];
    let mut drudges = Vec::with_capacity(16);
    for (name, dx) in back.iter().zip(files) {
        drudges.push(spawn_legacy_piece(w, name, at(dx, -3.5, 0.0, 1.0)));
    }
    for dx in files {
        drudges.push(spawn_legacy_piece(w, "drudgepawn", at(dx, -2.5, 0.0, 1.0)));
    }

    // Mosswarts: the back rank, then the pawns (rotation 0, 0, 1, 0)
    let back = [
        "mosswartrook",
        "mosswartknight",
        "mosswartbishop",
        "mosswartqueen",
        "mosswartking",
        "mosswartbishop",
        "mosswartknight",
        "mosswartrook",
    ];
    let mut mosswarts = Vec::with_capacity(16);
    for (name, dx) in back.iter().zip(files) {
        mosswarts.push(spawn_legacy_piece(w, name, at(dx, 3.5, 1.0, 0.0)));
    }
    for dx in files {
        mosswarts.push(spawn_legacy_piece(w, "mosswartpawn", at(dx, 2.5, 1.0, 0.0)));
    }

    // ACE kills each army as rook, bishop, knight, queen, king, bishop, knight, rook, then the pawns
    let kill_order = [0usize, 2, 1, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15];

    // For HellsWrath...
    let mut gdl_chain = ActionChain::new();
    gdl_chain.add_delay_seconds(w, 5.0);
    gdl_chain.add_action(Actor::Object(this), move |w| {
        for army in [&drudges, &mosswarts] {
            for &i in &kill_order {
                game_piece::kill(w, army[i]);
            }
        }

        send_event(w, player, |s| game_event_game_over(s, this, 0));
    });
    gdl_chain.add_delay_seconds(w, 2.0);
    gdl_chain.add_action(Actor::Object(this), move |w| {
        // Convert.FromBase64String("Z2FtZXNkZWFkbG9s"), decoded as UTF-8
        let text = "gamesdeadlol";
        let session = session_of(w, player);
        let data = w
            .sessions
            .get_mut(session)
            .expect("ACE: Player.Session is null (NullReferenceException)");
        let popup_gdl = game_event_popup_string(data, text);
        let msg_game_over2 = game_event_game_over(data, ObjectGuid::new(0), -2);
        crate::network::game_messages::game_message::enqueue_send_many(
            w,
            session,
            vec![popup_gdl, msg_game_over2],
        );
        if let Some(o) = w.objects.get_mut(player) {
            // an unset counter counts as 0 (V309, above)
            let lost = o.chess_games_lost().unwrap_or(0).wrapping_add(1);
            o.set_chess_games_lost(Some(lost));
            let total = o.chess_total_games().unwrap_or(0).wrapping_add(1);
            o.set_chess_total_games(Some(total));
        }
        if let Some(o) = w.objects.get_mut(this) {
            fields_mut(o).active = false;
        }
    });
    gdl_chain.enqueue_chain(w);
}

// ACE: Game.Heartbeat
pub fn game_heartbeat(w: &mut World, this: ObjectGuid, current_unix_time: f64) {
    if let Some(m) = chess_match(w, this) {
        chess_match::update(w, &m);
    }

    crate::world_objects::world_object_tick::world_object_heartbeat(w, this, current_unix_time);
}

// ---- constructors and SetEphemeralValues ----

/// `new Game(weenie, guid)` / `new Game(biota)`: the `WorldObject` constructor, then
/// Game's `SetEphemeralValues`.
// ACE: Game.Game
pub fn game_ctor(
    o: &mut crate::world_objects::world_object::WorldObject,
    env: &crate::world_objects::world_object::CtorEnv<'_>,
    src: crate::world_objects::world_object::CtorSource,
) {
    crate::world_objects::world_object::world_object_ctor(o, env, src);
    game_set_ephemeral_values(o, env);
}

// ACE: Game.SetEphemeralValues
fn game_set_ephemeral_values(
    o: &mut crate::world_objects::world_object::WorldObject,
    _env: &crate::world_objects::world_object::CtorEnv<'_>,
) {
    o.set_use_radius(Some(6.5));
}
