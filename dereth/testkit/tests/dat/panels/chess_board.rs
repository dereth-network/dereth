use super::*;

/// The player, and the two boards: a landblock-static guid is the shape a fixed piece of furniture
/// in the world has.
const CHESS_PLAYER: ObjectId = ObjectId(0x5000_0001);
const CHESS_BOARD: ObjectId = ObjectId(0x79DA_F100);
const CHESS_OTHER_BOARD: ObjectId = ObjectId(0x79DA_F200);

/// The shipped mini-game lamp in the indicator strip.
const CHESS_LAMP: ElementId = ElementId(0x1000_00F3);

/// A client in the world with a shard behind it and two boards standing in front of it.
fn chess_a_client_and_two_boards() -> (HeadlessClient, Peer) {
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(4));
    // The shard creates the player, because an ordered event is addressed to an object and the
    // client's ordered queue only has somewhere to put one for an object it has been told about.
    let peer = Peer::attach_creating(&mut c, CHESS_PLAYER);
    assert!(
        c.world_mut().set_player(CHESS_PLAYER),
        "the identity is adopted once"
    );
    c.world_mut()
        .weenie_mut(CHESS_PLAYER)
        .expect("the shard's create reached the world")
        .pwd
        .name = "Larktest".to_string();
    for id in [CHESS_BOARD, CHESS_OTHER_BOARD] {
        chess_put_a_board_in_the_world(&mut c, id);
    }
    c.tick(6);
    (c, peer)
}

/// A board as the shard describes one, and **every one of these is load-bearing** for a
/// double-click on it to come out as *begin a game* rather than *put it in your backpack*: it is a
/// game piece, it is stuck to the world, it is in no container and no hand, and it is not the
/// player's own.
fn chess_put_a_board_in_the_world(c: &mut HeadlessClient, id: ObjectId) {
    let mut w = dereth_client_model::Weenie::default();
    w.pwd.obj_type = dereth_client_model::weenie::item_type::GAMEBOARD;
    w.pwd.bitfield |= dereth_client_model::weenie::bitfield::STUCK;
    w.pwd.name = format!("Chess Board {:X}", id.0);
    c.world_mut().tables.weenies.insert(id, w);
}

/// Use a board, which is the gesture every double-click in this client funnels into.
fn chess_use_the_board(c: &mut HeadlessClient, id: ObjectId) {
    c.when(Player::DoubleClick(id));
    c.tick(6);
}

/// One thing the shard says about a game, as a real ordered datagram, and the frames that read it.
fn chess_the_shard_says<M: dereth_protocol::Message>(
    c: &mut HeadlessClient,
    peer: &mut Peer,
    m: &M,
) {
    peer.event(c, m);
    c.tick(6);
}

fn chess_panel(c: &HeadlessClient) -> &dereth_ui_screens::panels::minigame::MiniGamePanel {
    &c.view().expect_app().hud().panels.minigame
}

fn chess_model(c: &HeadlessClient) -> &dereth_client_model::minigame::MiniGame {
    &c.view().world().minigame
}

/// Every request about a game this client has put in its outbox, in order.
fn chess_wire(c: &HeadlessClient) -> Vec<dereth_client_model::Request> {
    use dereth_client_model::Request as R;
    c.view()
        .outbound()
        .iter()
        .filter(|r| {
            matches!(
                r,
                R::GameJoin(_)
                    | R::GameQuit(_)
                    | R::GameMove(_)
                    | R::GameMovePass(_)
                    | R::GameStalemate(_)
            )
        })
        .cloned()
        .collect()
}

/// The ones sent since `mark`, which is what a scenario takes before the gesture it is about.
fn chess_wire_since(c: &HeadlessClient, mark: usize) -> Vec<dereth_client_model::Request> {
    chess_wire(c).into_iter().skip(mark).collect()
}

/// The **main chat window**'s log, which is where every line this window composes ends up. Not the
/// world's scroll: the frame drains that into the chat windows, so a scenario that settles and
/// then reads the scroll reads an empty one.
fn chess_chat_log(c: &mut HeadlessClient) -> String {
    let (_, screen) = gameplay_screen(c.app_mut());
    screen
        .chat
        .iter()
        .find(|w| w.window_id == 8)
        .expect("the main chat window is in the shipped frame")
        .log_text()
}

/// The last thing the window said. The lines it composes carry their own newline and the scroll
/// trims both ends, so every expectation below is trimmed too.
fn chess_last_said(c: &mut HeadlessClient) -> String {
    chess_chat_log(c)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .next_back()
        .unwrap_or_default()
        .to_owned()
}

/// The newest line in the strip across the top of the viewport. The two refusals a board's window
/// makes go there and **not** to a chat window -- a distinction the client makes, and one a
/// scenario reading one surface for both would have papered over.
fn chess_last_spewed(c: &HeadlessClient) -> String {
    c.view()
        .expect_app()
        .hud()
        .panels
        .spew
        .model
        .items
        .first()
        .cloned()
        .unwrap_or_default()
}

fn chess_lamp(c: &HeadlessClient) -> ElemHandle {
    let ui = &c.view().expect_app().ui().expect("the ui shell is up").ui;
    ui.get_child_recursive(ui.root(), CHESS_LAMP)
        .expect("the shipped mini-game lamp")
}

fn chess_lamp_state(c: &HeadlessClient) -> u32 {
    let h = chess_lamp(c);
    let ui = &c.view().expect_app().ui().expect("the ui shell is up").ui;
    ui.node(h).expect("the lamp is alive").state.0
}

/// The lit lamp must resolve through the shipped state description all the way to a visible image
/// draw, or the scenario would be accepting a state nothing painted.
fn chess_lamp_is_lit(c: &HeadlessClient) -> dereth_primitives::DataId {
    let h = chess_lamp(c);
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    let node = ui.node(h).expect("the lamp is alive");
    assert_eq!(
        node.state.0, 1,
        "the notice puts the lamp in its own lit state"
    );
    assert!(ui.is_visible(h), "the lit lamp is visible");
    assert_eq!(
        node.merged_properties()
            .get_bool(dereth_ui::props::attr::DISABLED),
        Some(false),
        "the lit state re-enables the lamp"
    );
    let did = node
        .region
        .image
        .as_ref()
        .expect("the lit state supplies its shipped image")
        .did;
    let draw = app
        .ui_draw_list()
        .iter()
        .find(|cmd| cmd.who == h)
        .expect("the lit lamp reaches the frame's draw list");
    assert_eq!(draw.image, Some(did), "the drawn image is the shipped one");
    did
}

/// The twelve piece pictures, read off the board's own shipped property rather than out of a table
/// of this file's own.
fn chess_piece_pictures(c: &HeadlessClient) -> [dereth_primitives::DataId; 12] {
    let list = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .handle;
    let app = c.view().expect_app();
    let props = app
        .ui()
        .expect("the ui shell is up")
        .ui
        .node(list)
        .expect("the board's list node")
        .merged_properties();
    let dereth_ui::PropertyValue::Array(members) = props
        .get(minigame::PIECE_ICON_ARRAY)
        .expect("the shipped piece picture array")
    else {
        panic!("the piece picture property is an array")
    };
    members
        .iter()
        .map(|member| match member.value {
            dereth_ui::PropertyValue::DataFile(did) => did,
            ref value => panic!("a piece picture is a data file, got {value:?}"),
        })
        .collect::<Vec<_>>()
        .try_into()
        .expect("the shipped array has exactly twelve piece pictures")
}

/// Every one of those pictures really decodes to pixels out of the shipped data.
fn chess_pieces_have_pixels(c: &HeadlessClient, dids: &[dereth_primitives::DataId; 12]) -> bool {
    use dereth_primitives::AssetSource as _;
    let store = std::sync::Arc::clone(c.dat_store().expect("a retail client opens the dats"));
    dids.iter().all(|did| {
        let Ok(bytes) = store.read(*did) else {
            return false;
        };
        match <dereth_assets::RenderSurface as dereth_assets::Decode>::decode_payload(*did, &bytes)
        {
            Ok(s) => s.width > 0 && s.height > 0 && s.image_size > 0,
            Err(_) => false,
        }
    })
}

/// A square has gone through the panel and the renderer: its own region carries the shipped
/// picture, the draw mode the board asks for, and this frame asks the renderer to blit it.
fn chess_square_draws(c: &HeadlessClient, cell: usize, did: dereth_primitives::DataId) -> bool {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    let Some(node) = ui.node(h) else { return false };
    let cmd = app.ui_draw_list().iter().find(|cmd| cmd.who == h);
    ui.is_visible(h)
        && node.region.image.as_ref().map(|i| i.did) == Some(did)
        && node.region.blit_mode == dereth_ui::BlitMode::Alpha4
        && cmd.is_some_and(|cmd| {
            cmd.image == Some(did) && cmd.blit_mode == dereth_ui::BlitMode::Alpha4
        })
}

/// An empty square draws nothing, and leaves no stale command behind either.
fn chess_square_is_empty(c: &HeadlessClient, cell: usize) -> bool {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let app = c.view().expect_app();
    let ui = &app.ui().expect("the ui shell is up").ui;
    !ui.is_visible(h) && !app.ui_draw_list().iter().any(|cmd| cmd.who == h)
}

/// Press one square **through the real tree**: the pointer goes to the middle of that square's own
/// drawn box, so the hit test and the list's own working-out of which square was pressed are both
/// production. A scenario that handed the panel a square number would measure nothing.
fn chess_press_square(c: &mut HeadlessClient, cell: usize) {
    let h = chess_panel(c)
        .board
        .as_ref()
        .expect("the board is bound")
        .items[cell];
    let at = {
        let (ui, _) = gameplay_screen(c.app_mut());
        let b = ui.screen_box(h);
        ScreenPoint::new((b.x0 + b.x1) / 2, (b.y0 + b.y1) / 2)
    };
    c.when(Player::Click(Target::Point(at)));
    c.tick(6);
}

/// Press one of the window's three buttons.
///
/// **This channel is kept deliberately**: the click a completed button press raises,
/// broadcast on the shipped element. One of the three -- the pass button -- is never shown, so a
/// pointer gesture cannot reach it at all, and driving the other two differently from the third
/// would make one scenario measure two things.
fn chess_press_button(c: &mut HeadlessClient, id: ElementId) {
    {
        let (ui, screen) = gameplay_screen(c.app_mut());
        let root = screen.root().expect("the gameplay screen has a root");
        let h = ui
            .get_child_recursive(root, id)
            .unwrap_or_else(|| panic!("{id:?} is in the shipped tree"));
        ui.broadcast_element_message(h, dereth_ui::msg::element::id::BUTTON_CLICKED, 0, 0);
    }
    c.tick(6);
}

/// Whether the window's question has gone away.
fn chess_no_question_is_up(c: &HeadlessClient) -> bool {
    c.view()
        .expect_app()
        .ui()
        .expect("the ui shell is up")
        .ui
        .dialogs
        .non_queued()
        .is_empty()
}

/// The one question the window puts up, on the all-at-once dialog list: its context and its root.
fn chess_resign_question(c: &HeadlessClient) -> (u64, ElemHandle) {
    let dialogs = c
        .view()
        .expect_app()
        .ui()
        .expect("the ui shell is up")
        .ui
        .dialogs
        .non_queued();
    let [info] = dialogs else {
        panic!(
            "resigning raises one visible question, got {}",
            dialogs.len()
        )
    };
    assert_eq!(info.kind, dereth_ui::dialog::base::DialogKind::Confirmation);
    (
        info.context,
        info.element.expect("the shipped question has a tree"),
    )
}

/// What one element of the question says.
fn chess_dialog_text(c: &mut HeadlessClient, root: ElemHandle, child: ElementId) -> String {
    let h = {
        let (ui, _) = gameplay_screen(c.app_mut());
        ui.get_child_recursive(root, child)
            .expect("the question carries this element")
    };
    glyph_runs(c.app_mut(), h).0
}

/// Answer the question with a **real hit-tested press**, the way
/// [`answer_the_dialog`] answers the one on the default queue.
fn chess_answer(
    c: &mut HeadlessClient,
    hand: &mut dereth_testkit::adapters_chat::Hand,
    root: ElemHandle,
    child: ElementId,
) {
    let at = {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the ui shell is up").ui;
        let h = ui
            .get_child_recursive(root, child)
            .expect("the question carries this answer");
        let r = ui.screen_clip_box(h);
        assert!(r.is_valid(), "the answer has a real visible clip");
        let at = ((r.x0 + r.x1) / 2, (r.y0 + r.y1) / 2);
        assert!(
            ui.hit_test_screen(at.0, at.1)
                .is_some_and(|hit| hit == h || ui.is_ancestor_of(h, hit)),
            "the answer is the thing under the pointer"
        );
        at
    };
    hand.press_at(c, at.0, at.1);
    c.tick(6);
}

/// Take a seat as `my_team` and let `first_team` have the first move.
///
/// The two are separate because the local engine always gives the first move to team 0, so a start
/// naming team 1 leaves a team-0 player waiting and a team-1 player to move. That is the shard's
/// arrangement, measured, and it is why no scenario here takes the second seat and then moves.
///
/// **This is a fixture and not a claim**: the seat request it sends is what
/// [`using_a_board_raises_the_window_and_asks_for_a_seat`] is about, so every caller takes its own
/// mark afterwards and never counts it.
fn chess_sit_down(c: &mut HeadlessClient, peer: &mut Peer, my_team: i32, first_team: i32) {
    chess_use_the_board(c, CHESS_BOARD);
    chess_the_shard_says(
        c,
        peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: my_team,
        },
    );
    chess_the_shard_says(
        c,
        peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: first_team,
        },
    );
}

/// The shard's *"the opponent moved"*, in the from-and-to form.
fn chess_opponent_turn(
    game: ObjectId,
    team: i32,
    from: (u32, u32),
    to: (u32, u32),
) -> dereth_protocol::trade::GameOpponentTurn {
    dereth_protocol::trade::GameOpponentTurn {
        game_id: game.0,
        team,
        move_data: dereth_protocol::trade::GameMoveData {
            move_type: dereth_protocol::trade::GameMoveData::FROM_TO,
            player: ObjectId(0),
            from: Some(from),
            to: Some(to),
            piece_index: None,
        },
    }
}

// ---------------------------------------------------------------------------------------------

/// **The denominator, measured rather than looked up.** Each of the six the shard can send is
/// delivered to both consumers as a real event and the client is asked whether either took an arm,
/// because a decoder that works proves nothing about whether anything calls it; each of the five
/// the client can send is checked to have a sender and to be addressed at the shard.
///
/// **Do not shorten either list**: they are the denominator, and a census that drops rows as they
/// are fixed cannot tell *closed* from *forgotten*.
pub(super) fn every_chess_message_reaches_a_receiver_or_a_sender() {
    use dereth_protocol::Opcode;

    const INBOUND: [Opcode; 6] = [
        Opcode::GAME_JOIN_GAME_RESPONSE,
        Opcode::GAME_START_GAME,
        Opcode::GAME_MOVE_RESPONSE,
        Opcode::GAME_OPPONENT_TURN,
        Opcode::GAME_OPPONENT_STALEMATE_STATE,
        Opcode::GAME_GAME_OVER,
    ];
    const OUTBOUND: [Opcode; 5] = [
        Opcode::GAME_JOIN,
        Opcode::GAME_QUIT,
        Opcode::GAME_MOVE,
        Opcode::GAME_MOVE_PASS,
        Opcode::GAME_STALEMATE,
    ];

    let mut received = Vec::new();
    let mut dropped = Vec::new();
    for op in INBOUND {
        dereth_client::dropped::clear();
        let mut blob = op.0.to_le_bytes().to_vec();
        blob.extend(std::iter::repeat_n(0_u8, 64));
        let events =
            [dereth_client_net::client_session::SessionEvent::UiEvent { opcode: op, blob }];

        let mut world = dereth_client_model::World::new();
        let mut hud = dereth_client::hud::Hud::new();
        hud.apply_events(&events, &mut world);

        let mut world = dereth_client_model::World::new();
        let mut inter = dereth_client::interaction::Interaction::new();
        dereth_client::interaction::apply_events(&mut inter, &events, &mut world);

        if dereth_client::dropped::unreceived(op) {
            dropped.push(op);
        } else {
            received.push(op);
        }
    }

    // The outbound five, discriminated by construction: each has a request of its own **and** a
    // sender arm, which is what "a sender exists" means here.
    let senders: Vec<(Opcode, dereth_client_model::Request)> = vec![
        (
            Opcode::GAME_JOIN,
            dereth_client_model::Request::GameJoin(Default::default()),
        ),
        (
            Opcode::GAME_QUIT,
            dereth_client_model::Request::GameQuit(dereth_protocol::trade::GameQuit),
        ),
        (
            Opcode::GAME_MOVE,
            dereth_client_model::Request::GameMove(Default::default()),
        ),
        (
            Opcode::GAME_MOVE_PASS,
            dereth_client_model::Request::GameMovePass(dereth_protocol::trade::GameMovePass),
        ),
        (
            Opcode::GAME_STALEMATE,
            dereth_client_model::Request::GameStalemate(Default::default()),
        ),
    ];
    let all_are_client_to_server = senders.iter().all(|(op, _)| {
        op.info().map(|i| i.direction) == Some(dereth_protocol::opcodes::Direction::C2S)
    });
    let sender_count = senders.len();

    let mut c = HeadlessClient::model();
    c.assert_behaviour(
        "minigame.messages.every-message-the-game-speaks-reaches-a-receiver-or-a-sender",
        move |_| {
            INBOUND.len() == 6
                && dropped.is_empty()
                && received.len() == 6
                && sender_count == 5
                && all_are_client_to_server
                && INBOUND.len() + OUTBOUND.len() == 11
        },
    );
    c.shutdown();
}

/// **Using the board.** No round trip raises this window: the board is raised by
/// the use itself, and the first thing it does is ask for a seat.
pub(super) fn using_a_board_raises_the_window_and_asks_for_a_seat() {
    let (mut c, _peer) = chess_a_client_and_two_boards();

    // Before: built, and down.
    let built = chess_panel(&c).bound()
        && chess_panel(&c).cells_built == minigame::CELLS
        && chess_panel(&c).resign.is_some()
        && chess_panel(&c).pass.is_some()
        && chess_panel(&c).stalemate.is_some()
        && !chess_panel(&c).visible;
    let nothing_yet = chess_model(&c).begin_games == 0
        && chess_model(&c).current_game == ObjectId(0)
        && chess_wire(&c).is_empty();

    chess_use_the_board(&mut c, CHESS_BOARD);

    // The use reached the game-board arm **and** the notice reached the window -- both, because
    // either alone is a state this client has been in.
    let reached_the_window = c.view().expect_app().interaction().stats.panels_requested == 1
        && c.view()
            .expect_app()
            .interaction()
            .stats
            .minigame_boards_used
            == 1
        && chess_model(&c).begin_games == 1;
    let the_window_is_up = chess_panel(&c).visible
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).state == GameState::AttemptingToJoinGame;
    let asked_for_a_seat = chess_wire(&c)
        == vec![dereth_client_model::Request::GameJoin(
            dereth_protocol::trade::GameJoin {
                game_id: CHESS_BOARD.0,
                which_team: dereth_client_model::minigame::JOIN_ANY_TEAM,
            },
        )];
    let said_so =
        chess_last_said(&mut c) == dereth_client_model::minigame::ATTEMPTING_TO_JOIN.trim();

    // The two refusals, and that neither of them asks the shard for anything.
    let mark = chess_wire(&c).len();
    chess_use_the_board(&mut c, CHESS_BOARD);
    let same_board = chess_last_spewed(&c) == dereth_client_model::minigame::ALREADY_THIS_GAME;
    // The client's own short throttle on using a thing is measured in its own clock, and six
    // headless frames do not always clear it: settling again is obeying the client's rule, not
    // making the scenario pass.
    c.tick(12);
    chess_use_the_board(&mut c, CHESS_OTHER_BOARD);
    let other_board = chess_last_spewed(&c) == dereth_client_model::minigame::ALREADY_ANOTHER_GAME;
    let both_refused = chess_model(&c).joins_refused == 2;
    let nothing_more_was_asked = chess_wire_since(&c, mark).is_empty();
    let the_first_game_is_untouched = chess_model(&c).current_game == CHESS_BOARD;

    c.assert_behaviour(
        "minigame.board.using-a-board-raises-the-window-and-asks-for-a-seat",
        move |_| {
            built
                && nothing_yet
                && reached_the_window
                && the_window_is_up
                && asked_for_a_seat
                && said_so
                && same_board
                && other_board
                && both_refused
                && nothing_more_was_asked
                && the_first_game_is_untouched
        },
    );
    c.shutdown();
}

/// **The lamp has two notices and only two**: the one that begins a game and the one that ends it.
/// Everything in between changes the board window and must not select other lamp art.
pub(super) fn the_game_lamp_is_lit_from_the_first_use_until_the_game_is_over() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    let dark_to_begin = chess_lamp_state(&c) == dereth_ui_screens::hud::indicators::STATE_NOTHING;

    chess_use_the_board(&mut c, CHESS_BOARD);
    let lit = chess_lamp_is_lit(&c);
    let mut stays_the_same = chess_model(&c).state == GameState::AttemptingToJoinGame;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::WaitingForGameStart && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingMyTurn && chess_lamp_is_lit(&c) == lit;

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square");
    chess_press_square(&mut c, from);
    chess_press_square(&mut c, to);
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingTryingToMove && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: 1,
        },
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingNotMyTurn && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 1, (4, 6), (4, 4)),
    );
    stays_the_same &=
        chess_model(&c).state == GameState::PlayingMyTurn && chess_lamp_is_lit(&c) == lit;

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameGameOver {
            game_id: CHESS_BOARD.0,
            team_winner: dereth_client_model::minigame::GAME_OVER_ABORTED,
        },
    );
    let dark_again = chess_lamp_state(&c) == dereth_ui_screens::hud::indicators::STATE_NOTHING;
    let the_window_went_down = !chess_model(&c).visible;

    c.assert_behaviour(
        "minigame.indicator.the-lamp-is-lit-from-the-first-use-until-the-game-is-over",
        move |_| dark_to_begin && stays_the_same && dark_again && the_window_went_down,
    );
    c.shutdown();
}

/// **The round trip.** The seat deals the board, the start names whose turn it is -- both arms --
/// and a refused seat puts the window back to no game at all.
pub(super) fn the_seat_deals_the_board_and_the_start_names_whose_turn_it_is() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let reached_an_arm = c.view().expect_app().hud().stats.minigame_events == 1
        && c.view().expect_app().hud().stats.minigame_guarded == 0;
    let dealt = chess_model(&c).team == 0
        && chess_model(&c).state == GameState::WaitingForGameStart
        && chess_model(&c).board.logic.pieces.len() == 32
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 0 })
            .map(|p| p.piece_type)
            == Some(PieceType::King);
    let said_joined = chess_last_said(&mut c) == dereth_client_model::minigame::JOINED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    let my_turn = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::BEGUN_YOUR_TURN.trim();
    c.shutdown();

    // The other arm of the same message: the shard says the *other* side moves first.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: 0,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameStartGame {
            game_id: CHESS_BOARD.0,
            team: 1,
        },
    );
    let their_turn = chess_model(&c).state == GameState::PlayingNotMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::BEGUN_THEIR_TURN.trim();
    c.shutdown();

    // And a seat the shard refuses.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_use_the_board(&mut c, CHESS_BOARD);
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: CHESS_BOARD.0,
            team: -1,
        },
    );
    let refused = chess_last_said(&mut c) == dereth_client_model::minigame::COULD_NOT_JOIN.trim()
        && chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).current_game == ObjectId(0)
        && chess_model(&c).board.logic.pieces.is_empty();

    c.assert_behaviour(
        "minigame.board.the-shards-answer-deals-the-board-or-takes-the-window-away",
        move |_| reached_an_arm && dealt && said_joined && my_turn && their_turn && refused,
    );
    c.shutdown();
}

/// **The move.** Two presses through the real tree, the request they make, the move the client's
/// own rules refuse where the player made it, and the move the shard refuses afterwards.
pub(super) fn two_presses_move_a_piece_and_a_refused_move_never_leaves() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square");

    chess_press_square(&mut c, from);
    let picked_up = chess_panel(&c).presses_routed == 1
        && chess_model(&c).board.selected == Some(ChessCoord { x: 4, y: 1 })
        && chess_wire_since(&c, mark).is_empty();

    chess_press_square(&mut c, to);
    let asked = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameMove(
            dereth_protocol::trade::GameMove {
                x_from: 4,
                y_from: 1,
                x_to: 4,
                y_to: 3,
            },
        )];
    let on_its_way = chess_model(&c).state == GameState::PlayingTryingToMove
        && chess_model(&c).moves_sent == 1
        && chess_last_said(&mut c) == dereth_client_model::minigame::MOVE_IN_PROGRESS;

    // Let the move stand and give the turn back, so the next press is the player's to make.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: 1,
        },
    );
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 1, (4, 6), (4, 4)),
    );
    let our_turn_again = chess_model(&c).state == GameState::PlayingMyTurn;

    // A move the client's own rules refuse never reaches the shard -- the whole reason the rules
    // are in the client at all. The king cannot step on to its own pawn.
    let before = chess_model(&c).moves_sent;
    let mark = chess_wire(&c).len();
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 0 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 1 }, 0).expect("a square"),
    );
    let refused_here = chess_model(&c).moves_sent == before
        && chess_model(&c).moves_refused_locally == 1
        && chess_wire_since(&c, mark).is_empty()
        && chess_last_said(&mut c)
            == format!(
                "You cannot attack your own pieces{}{}",
                dereth_client_model::minigame::TRY_AGAIN,
                dereth_client_model::minigame::YOUR_TURN
            )
            .trim();
    c.shutdown();

    // And the shard's own refusal, which puts the piece back and gives the turn back.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 1 }, 0).expect("a square"),
    );
    chess_press_square(
        &mut c,
        ChessBoard::cell_of_coord(ChessCoord { x: 4, y: 3 }, 0).expect("a square"),
    );
    let waiting = chess_model(&c).state == GameState::PlayingTryingToMove;
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: CHESS_BOARD.0,
            result: chess_mr::BAD_MOVE_NOT_YOUR_TURN,
        },
    );
    let undone = chess_model(&c).moves_undone == 1
        && chess_model(&c).state == GameState::PlayingMyTurn
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 4, y: 1 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_last_said(&mut c) == dereth_client_model::minigame::NOT_YOUR_TURN.trim();

    c.assert_behaviour(
        "minigame.board.two-presses-move-a-piece-and-a-move-the-rules-refuse-never-leaves",
        move |_| {
            picked_up && asked && on_its_way && our_turn_again && refused_here && waiting && undone
        },
    );
    c.shutdown();
}

/// **The opponent's move, on the player's own board, drawn from the player's own side.**
pub(super) fn the_opponents_move_is_replayed_on_the_players_own_board() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    // The second seat, with the first side moving first, so the start leaves the player waiting.
    chess_sit_down(&mut c, &mut peer, 1, 0);
    let mark = chess_wire(&c).len();
    let seated = chess_model(&c).team == 1 && chess_model(&c).state == GameState::PlayingNotMyTurn;

    // From the second side the board is drawn the other way round: the far corner piece is the
    // last square, and its picture is the one the board ships for it.
    let pictures = chess_piece_pictures(&c);
    let real_pixels = chess_pieces_have_pixels(&c, &pictures);
    let corner = ChessBoard::cell_of_coord(ChessCoord { x: 0, y: 7 }, 1).expect("a square");
    let drawn_from_our_side = corner == 63 && chess_square_draws(&c, corner, pictures[9]);

    let from = ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 1 }, 1).expect("a square");
    let to = ChessBoard::cell_of_coord(ChessCoord { x: 3, y: 3 }, 1).expect("a square");
    let before = chess_square_draws(&c, from, pictures[0]) && chess_square_is_empty(&c, to);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &chess_opponent_turn(CHESS_BOARD, 0, (3, 1), (3, 3)),
    );

    let moved = chess_model(&c).opponent_moves == 1
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 3 })
            .map(|p| p.piece_type)
            == Some(PieceType::Pawn)
        && chess_model(&c)
            .board
            .logic
            .at(ChessCoord { x: 3, y: 1 })
            .is_none();
    let redrawn = chess_square_is_empty(&c, from) && chess_square_draws(&c, to, pictures[0]);
    let turn_came_back = chess_model(&c).state == GameState::PlayingMyTurn
        && chess_last_said(&mut c) == dereth_client_model::minigame::YOUR_TURN.trim();
    let sent_nothing = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.the-opponents-move-is-replayed-on-the-players-own-board",
        move |_| {
            seated
                && real_pixels
                && drawn_from_our_side
                && before
                && moved
                && redrawn
                && turn_came_back
                && sent_nothing
        },
    );
    c.shutdown();
}

/// **The offer, and the end.** An offer of a draw and its withdrawal are both said in the window
/// and neither turns on the player's own offer -- agreeing is pressing your own button.
pub(super) fn the_offer_of_a_draw_and_the_end_of_a_game_are_said_in_the_window() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 1,
        },
    );
    let offered =
        chess_last_said(&mut c) == dereth_client_model::minigame::STALEMATE_OFFERED.trim();

    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: CHESS_BOARD.0,
            team: 1,
            on: 0,
        },
    );
    let retracted =
        chess_last_said(&mut c) == dereth_client_model::minigame::STALEMATE_RETRACTED.trim();
    let both_counted = chess_model(&c).stalemate_offers == 2;
    let ours_is_untouched = !chess_model(&c).stalemate;

    // The player's side is the first, so the first side winning is the player winning.
    chess_the_shard_says(
        &mut c,
        &mut peer,
        &dereth_protocol::trade::GameGameOver {
            game_id: CHESS_BOARD.0,
            team_winner: 0,
        },
    );
    let won = chess_last_said(&mut c)
        == format!(
            "{}{}",
            dereth_client_model::minigame::VICTORIOUS,
            dereth_client_model::minigame::DEFAULT_STATE
        )
        .trim();
    let put_away = chess_model(&c).state == GameState::NotPlaying
        && chess_model(&c).team == -1
        && chess_model(&c).current_game == ObjectId(0);

    c.assert_behaviour(
        "minigame.window.the-offer-of-a-stalemate-and-the-end-of-a-game-are-said-in-the-window",
        move |_| offered && retracted && both_counted && ours_is_untouched && won && put_away,
    );
    c.shutdown();
}

/// **The three buttons.** The draw button toggles and tells the shard the value it toggled *to*;
/// resigning asks first, through the client's own question, and only Yes quits; and the button
/// that passes a turn is never shown to the player at all.
pub(super) fn the_window_buttons_offer_a_draw_and_ask_before_resigning() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mut mark = chess_wire(&c).len();

    chess_press_button(&mut c, minigame::STALEMATE_BUTTON);
    let offered = chess_panel(&c).button_clicks == 1
        && chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![dereth_client_model::Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 1 },
            )];
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, minigame::STALEMATE_BUTTON);
    let withdrawn = !chess_model(&c).stalemate
        && chess_wire_since(&c, mark)
            == vec![dereth_client_model::Request::GameStalemate(
                dereth_protocol::trade::GameStalemate { on: 0 },
            )];

    // Resigning raises the question and sends nothing until it is answered.
    mark = chess_wire(&c).len();
    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let asked_first = chess_wire_since(&c, mark).is_empty();
    let (first_context, first_root) = chess_resign_question(&c);
    let question_is_the_windows_own = u64::from(chess_model(&c).resign_dialog) == first_context
        && !chess_model(&c).resign_prompt_pending;
    let prompt = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::TEXT);
    let yes = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::BUTTON1);
    let no = chess_dialog_text(&mut c, first_root, dereth_ui::dialog::base::child::BUTTON2);
    let asked_in_its_own_words =
        prompt == dereth_client_model::minigame::RESIGN_PROMPT && yes == "Yes" && no == "No";

    // A second press while the question is up asks nothing more.
    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let one_question_only =
        chess_resign_question(&c).0 == first_context && chess_wire_since(&c, mark).is_empty();

    // No closes it, sends nothing, and lets it be asked again.
    let mut hand = dereth_testkit::adapters_chat::Hand::new();
    chess_answer(
        &mut c,
        &mut hand,
        first_root,
        dereth_ui::dialog::base::child::BUTTON2,
    );
    let no_sends_nothing = chess_wire_since(&c, mark).is_empty()
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).resign_dialog == 0
        && !chess_model(&c).resign_prompt_pending
        && chess_no_question_is_up(&c);

    chess_press_button(&mut c, minigame::RESIGN_BUTTON);
    let (second_context, second_root) = chess_resign_question(&c);
    let a_fresh_question = second_context != first_context;
    chess_answer(
        &mut c,
        &mut hand,
        second_root,
        dereth_ui::dialog::base::child::BUTTON1,
    );
    let yes_quits = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameQuit(
            dereth_protocol::trade::GameQuit,
        )]
        && chess_model(&c).current_game == ObjectId(0)
        && !chess_panel(&c).visible;
    c.shutdown();

    // The pass button: never shown, and still a sender. With no game it says so and sends nothing;
    // with a game it sends the pass.
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    let never_shown = {
        let h = chess_panel(&c)
            .pass
            .expect("the pass button is in the shipped tree");
        minigame::MiniGamePanel::PASS_BUTTON_IS_NEVER_SHOWN
            && !c
                .view()
                .expect_app()
                .ui()
                .expect("the ui shell is up")
                .ui
                .is_visible(h)
    };
    let mark = chess_wire(&c).len();
    c.when(Player::ui(
        dereth_ui_screens::view::UiRequest::MiniGameButton(minigame::PASS_BUTTON.0),
    ));
    c.tick(6);
    let no_game = chess_last_spewed(&c) == dereth_client_model::minigame::NOT_PLAYING
        && chess_wire_since(&c, mark).is_empty();

    chess_sit_down(&mut c, &mut peer, 0, 0);
    let mark = chess_wire(&c).len();
    c.when(Player::ui(
        dereth_ui_screens::view::UiRequest::MiniGameButton(minigame::PASS_BUTTON.0),
    ));
    c.tick(6);
    let passes = chess_wire_since(&c, mark)
        == vec![dereth_client_model::Request::GameMovePass(
            dereth_protocol::trade::GameMovePass,
        )];

    c.assert_behaviour(
        "minigame.window.the-buttons-offer-a-stalemate-and-ask-before-resigning",
        move |_| {
            offered
                && withdrawn
                && asked_first
                && question_is_the_windows_own
                && asked_in_its_own_words
                && one_question_only
                && no_sends_nothing
                && a_fresh_question
                && yes_quits
                && never_shown
                && no_game
                && passes
        },
    );
    c.shutdown();
}

/// **The guard.** Every one of the six is guarded on which board it is about. A message about a
/// board this window did not sit down at must change nothing and say nothing -- and must still be
/// counted, so that "no traffic" and "all of it refused" are different answers.
pub(super) fn a_message_about_another_board_changes_nothing() {
    let (mut c, mut peer) = chess_a_client_and_two_boards();
    chess_sit_down(&mut c, &mut peer, 0, 0);
    let before = chess_model(&c).state;
    let events_before = c.view().expect_app().hud().stats.minigame_events;
    let said_before = chess_chat_log(&mut c);
    let mark = chess_wire(&c).len();

    let other = CHESS_OTHER_BOARD.0;
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameJoinGameResponse {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameStartGame {
            game_id: other,
            team: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameMoveResponse {
            game_id: other,
            result: 1,
        },
    );
    peer.event(
        &mut c,
        &chess_opponent_turn(CHESS_OTHER_BOARD, 1, (4, 6), (4, 4)),
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameOpponentStalemateState {
            game_id: other,
            team: 1,
            on: 1,
        },
    );
    peer.event(
        &mut c,
        &dereth_protocol::trade::GameGameOver {
            game_id: other,
            team_winner: 1,
        },
    );
    c.tick(6);

    let all_six_arrived = c.view().expect_app().hud().stats.minigame_events - events_before == 6;
    let all_six_were_refused = c.view().expect_app().hud().stats.minigame_guarded == 6;
    let nothing_moved = chess_model(&c).state == before
        && chess_model(&c).current_game == CHESS_BOARD
        && chess_model(&c).team == 0;
    let nothing_was_said = chess_chat_log(&mut c) == said_before;
    let nothing_was_sent = chess_wire_since(&c, mark).is_empty();

    c.assert_behaviour(
        "minigame.board.a-message-about-another-board-changes-nothing",
        move |_| {
            all_six_arrived
                && all_six_were_refused
                && nothing_moved
                && nothing_was_said
                && nothing_was_sent
        },
    );
    c.shutdown();
}
