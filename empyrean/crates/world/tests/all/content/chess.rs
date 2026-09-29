//! Vectors: fixtures/vectors/chess/
//! ChessLogic, pieces and ChessPieceCoord replay ACE golden vectors; retail match flow
//! differences.
//! Fixture: ACE vectors and explicit expected values.

// V311, V326.

use dereth_rules::chess as rules;
use empyrean_common::thread_safe_random::ThreadSafeRandom;
use empyrean_common::vectors::{self, f32_of, i64_of, same_f32, Case};
use empyrean_entity::enums::{ChessColor, ChessMoveFlag, ChessMoveResult, ChessPieceType};
use empyrean_entity::ObjectGuid;
use empyrean_world::entity::chess::chess;
use empyrean_world::entity::chess::chess_ai_async_turn_key::ChessAiAsyncTurnKey;
use empyrean_world::entity::chess::chess_logic::ChessLogic;
use empyrean_world::entity::chess::chess_move::ChessMove;
use empyrean_world::entity::chess::chess_piece_coord::ChessPieceCoord;
use rules::move_result as mr;
use serde_json::Value;

// ---------------------------------------------------------------------------------- harness

fn int(v: &Value) -> i64 {
    i64_of(v).unwrap_or_else(|| panic!("not an integer: {v}"))
}

fn i32v(v: &Value) -> i32 {
    i32::try_from(int(v)).expect("fits i32")
}

fn coord_str(c: Option<&ChessPieceCoord>) -> String {
    c.map_or_else(|| "n".to_owned(), |c| format!("{},{}", c.x, c.y))
}

/// `ChessVectors.Move`.
fn move_str(m: &ChessMove) -> String {
    format!(
        "{}|{}|{}|{}|{}|{}|{}|{}|{}|{}|{:x}|{:x}",
        m.flags.0,
        m.color.0,
        m.r#type.0,
        coord_str(Some(&m.from)),
        coord_str(Some(&m.to)),
        m.promotion.0,
        m.captured.0,
        m.r#move,
        m.half_move,
        coord_str(m.en_passant_coord.as_ref()),
        m.guid.full(),
        m.captured_guid.full()
    )
}

/// `ChessVectors.Board`.
fn board_str(logic: &ChessLogic) -> String {
    let squares: Vec<String> = (0..64)
        .map(|i| match &logic.board[i] {
            None => String::new(),
            Some(p) => {
                let mut s = format!(
                    "{}{}{}:{:x}",
                    p.color.0,
                    p.r#type.0,
                    p.class.0,
                    p.guid.full()
                );
                if usize::try_from(p.coord.offset()).ok() != Some(i) {
                    s.push_str(&format!("@{},{}", p.coord.x, p.coord.y));
                }
                s
            }
        })
        .collect();
    squares.join(",")
}

/// `ChessVectors.State`.
fn state(logic: &ChessLogic) -> Value {
    serde_json::json!([
        logic.turn.0,
        logic.r#move,
        logic.half_move,
        logic.castling.get(0).0,
        logic.castling.get(1).0,
        coord_str(logic.en_passant_coord.as_ref()),
        logic.history.len()
    ])
}

/// `ChessVectors.NewLogic`: guids 0x80000000 + each piece's start square.
fn new_logic() -> ChessLogic {
    let mut logic = ChessLogic::new();
    for c in logic.walk_piece_coords() {
        let piece = logic.get_piece_mut(&c).expect("walked");
        piece.guid =
            ObjectGuid::new(0x8000_0000 + u32::try_from(c.offset()).expect("on the board"));
    }
    logic
}

fn ai_triple(
    r: (
        ChessMoveResult,
        Option<ChessPieceCoord>,
        Option<ChessPieceCoord>,
    ),
) -> Value {
    serde_json::json!([r.0 .0, coord_str(r.1.as_ref()), coord_str(r.2.as_ref())])
}

// ---------------------------------------------------------------------------------- coords, pieces

#[test]
fn chess_piece_coord_vectors() {
    let file = vectors::load_named("chess", "coords");
    let mut n = 0;
    for Case { input, output } in &file.cases {
        let (c, extra) = if input.get("offset").is_some() {
            (ChessPieceCoord::from_offset(i32v(&input["offset"])), None)
        } else if input.get("default").is_some() {
            (ChessPieceCoord::new(), None)
        } else {
            let c = ChessPieceCoord::new_xy(i32v(&input["x"]), i32v(&input["y"]));
            let mut moved = c;
            moved.move_offset_vec((1, -2));
            (c, Some(moved))
        };
        if let Some(x) = output.get("x") {
            assert_eq!(c.x, i32v(x), "{input}");
            assert_eq!(c.y, i32v(&output["y"]), "{input}");
        }
        if let Some(r) = output.get("rank") {
            assert_eq!(c.rank(), i32v(r), "{input}");
        }
        if let Some(o) = output.get("offset") {
            assert_eq!(c.offset(), i32v(o), "{input}");
        }
        assert_eq!(
            c.is_valid(),
            output["valid"].as_bool().expect("bool"),
            "{input}"
        );
        assert_eq!(
            c.to_string(),
            output["str"].as_str().expect("str"),
            "{input}"
        );
        if let Some(moved) = extra {
            assert_eq!(
                coord_str(Some(&moved)),
                output["moved"].as_str().expect("str"),
                "{input}"
            );
            assert_eq!(
                c.equals(Some(&moved)),
                output["eq_moved"].as_bool().expect("bool"),
                "{input}"
            );
            assert_eq!(
                c.equals(Some(&ChessPieceCoord::new_xy(c.x, c.y))),
                output["eq_copy"].as_bool().expect("bool")
            );
            assert_eq!(c.equals(None), output["eq_null"].as_bool().expect("bool"));
        }
        n += 1;
    }
    assert!(n > 200, "{n} cases");
}

#[test]
fn chess_piece_vectors() {
    let file = vectors::load_named("chess", "pieces");
    assert!(!file.cases.is_empty());
    for Case { input, output } in &file.cases {
        let (x, y) = (
            u32::try_from(int(&input["x"])).expect("x"),
            u32::try_from(int(&input["y"])).expect("y"),
        );
        let mut logic = ChessLogic::new();
        let piece = logic
            .add_piece(
                ChessColor(i32v(&input["color"])),
                ChessPieceType(i32v(&input["type"])),
                x,
                y,
            )
            .expect("a piece class")
            .clone();
        let (mut mv, mut at) = (String::new(), String::new());
        for dy in -8..=8 {
            for dx in -8..=8 {
                mv.push(if piece.can_move(dx, dy) { '1' } else { '0' });
                at.push(if piece.can_attack(dx, dy) { '1' } else { '0' });
            }
        }
        assert_eq!(mv, output["move"].as_str().expect("str"), "CanMove {input}");
        assert_eq!(
            at,
            output["attack"].as_str().expect("str"),
            "CanAttack {input}"
        );
        let target = ChessPieceCoord::new_xy(piece.coord.x + 1, piece.coord.y + 1);
        assert_eq!(
            piece.can_attack_coord(&target),
            output["attack_coord"].as_bool().expect("bool"),
            "{input}"
        );
    }
}

// ---------------------------------------------------------------------------------- games
// ---------------------------------------------------------------------------------- the client's rules

/// A square as the client's rules name it.
fn rc(c: ChessPieceCoord) -> rules::Coord {
    rules::Coord { x: c.x, y: c.y }
}

fn xy(x: i32, y: i32) -> ChessPieceCoord {
    ChessPieceCoord::new_xy(x, y)
}

/// A client result as the match records it: an en passant capture is a capture (V306 reports it to
/// the mover as en passant).
fn as_match(r: i32) -> i32 {
    if r > 0 && r & mr::OK_MOVE_MASK == mr::OK_MOVE_EN_PASSANT {
        (r & !mr::OK_MOVE_MASK) | mr::OK_MOVE_TO_OCCUPIED_SQUARE
    } else {
        r
    }
}

/// The client's rules playing the same game on their own board from the start position (V311/V326):
/// each move the port makes is made with the client's `do_move`, and a pawn reaching the last rank
/// becomes a queen with the check result computed again and added, as the client's board does.
/// Nothing here goes through the port.
#[derive(Clone)]
struct Oracle {
    board: rules::ChessLogic,
    before: Vec<rules::ChessLogic>,
}

impl Oracle {
    fn new() -> Self {
        use rules::PieceType as P;
        let mut board = rules::ChessLogic::new();
        let back = [
            P::Rook,
            P::Knight,
            P::Bishop,
            P::Queen,
            P::King,
            P::Bishop,
            P::Knight,
            P::Rook,
        ];
        for (x, t) in (0..8).zip(back) {
            board.place(t, 0, rules::Coord { x, y: 0 });
            board.place(P::Pawn, 0, rules::Coord { x, y: 1 });
            board.place(P::Pawn, 1, rules::Coord { x, y: 6 });
            board.place(t, 1, rules::Coord { x, y: 7 });
        }
        Self {
            board,
            before: Vec::new(),
        }
    }

    /// Makes the move if the client allows it; the client's result, as the match records it.
    fn play(&mut self, from: ChessPieceCoord, to: ChessPieceCoord) -> i32 {
        let before = self.board.clone();
        let mut r = self.board.do_move(rc(from), rc(to));
        if r > 0 {
            if r & mr::OK_MOVE_PROMOTION != 0 {
                assert!(self.board.promote_to_queen(rc(to)));
                r |= self.board.compute_check_result(self.board.cur_player);
            }
            self.before.push(before);
        }
        as_match(r)
    }

    /// What the server answers `color` moving `from` -> `to`: its own gates (off the board, then not
    /// that colour's turn), then the client's answer.
    fn result_of(&self, color: i32, from: ChessPieceCoord, to: ChessPieceCoord) -> i32 {
        if !from.is_valid() || !to.is_valid() {
            return mr::BAD_MOVE_DESTINATION;
        }
        if color != self.board.cur_player {
            return ChessMoveResult::BadMoveNotYourTurn.0;
        }
        self.clone().play(from, to)
    }

    fn undo(&mut self, n: u32) {
        for _ in 0..n {
            if let Some(b) = self.before.pop() {
                self.board = b;
            }
        }
    }

    /// Every move the side to move may make, by the client's rules.
    fn legal(&self) -> Vec<(ChessPieceCoord, ChessPieceCoord)> {
        let mut out = Vec::new();
        for p in &self.board.pieces {
            if p.player != self.board.cur_player || !p.cur_pos.is_valid() {
                continue;
            }
            for y in 0..8 {
                for x in 0..8 {
                    if self.board.test_move(p.cur_pos, rules::Coord { x, y }) > 0 {
                        out.push((xy(p.cur_pos.x, p.cur_pos.y), xy(x, y)));
                    }
                }
            }
        }
        out.sort_by_key(|(f, t)| (f.offset(), t.offset()));
        out
    }

    /// The castles `player` may still make: its king has not moved, nor the rook on that corner.
    fn castling(&self, player: i32) -> i32 {
        let home = if player == 0 { 0 } else { 7 };
        let unmoved = |x: i32, t: rules::PieceType| {
            self.board
                .at(rules::Coord { x, y: home })
                .is_some_and(|p| p.piece_type == t && p.player == player && !p.moved)
        };
        if !unmoved(4, rules::PieceType::King) {
            return 0;
        }
        let mut flags = 0;
        if unmoved(7, rules::PieceType::Rook) {
            flags |= ChessMoveFlag::KingSideCastle.0;
        }
        if unmoved(0, rules::PieceType::Rook) {
            flags |= ChessMoveFlag::QueenSideCastle.0;
        }
        flags
    }
}

/// The port and the client's rules see the same game: the same pieces on the same squares, the same
/// side to move and en passant square, the same castling rights, check for both sides and checkmate
/// for the side to move.
fn assert_agrees(port: &mut ChessLogic, oracle: &Oracle, at: &str) {
    let o = &oracle.board;
    assert_eq!(port.turn.0, o.cur_player, "{at}: side to move");
    for y in 0..8 {
        for x in 0..8 {
            let p = port.get_piece(&xy(x, y)).map(|p| (p.r#type.0, p.color.0));
            let q = o
                .at(rules::Coord { x, y })
                .map(|p| (p.piece_type as i32, p.player));
            assert_eq!(p, q, "{at}: square {x},{y}\n{}", port.debug_board());
        }
    }
    let site = Some(o.en_passant_attack_site).filter(|c| c.is_valid());
    assert_eq!(
        port.en_passant_coord.map(rc),
        site,
        "{at}: en passant square"
    );
    for color in [ChessColor::White, ChessColor::Black] {
        assert_eq!(
            port.castling.get(color.0).0,
            oracle.castling(color.0),
            "{at}: {color:?}'s castling rights"
        );
        assert_eq!(
            port.in_check(color),
            o.is_player_in_check(color.0),
            "{at}: {color:?} in check"
        );
    }
    let turn = port.turn;
    assert_eq!(
        port.in_checkmate(turn, false),
        o.is_player_in_check_mate(turn.0),
        "{at}: checkmate"
    );
}

/// The port's legal moves are the client's.
fn assert_same_legal_moves(port: &ChessLogic, oracle: &Oracle, at: &str) {
    let mut mine: Vec<(ChessPieceCoord, ChessPieceCoord)> = port
        .legal_moves(port.turn)
        .iter()
        .map(|m| (m.from, m.to))
        .collect();
    mine.sort_by_key(|(f, t)| (f.offset(), t.offset()));
    assert_eq!(mine, oracle.legal(), "{at}: legal moves");
}

fn from_to_str(s: &str) -> (i32, ChessPieceCoord, ChessPieceCoord) {
    let f: Vec<&str> = s.split('|').collect();
    let c = |s: &str| {
        let v: Vec<i32> = s.split(',').map(|n| n.parse().expect("coord")).collect();
        xy(v[0], v[1])
    };
    (f[0].parse().expect("flags"), c(f[3]), c(f[4]))
}

const CASTLES: i32 = ChessMoveFlag::KingSideCastle.0 | ChessMoveFlag::QueenSideCastle.0;

/// ACE's recorded state with the castling rights the client's rules give (V311/V326): the king and that
/// rook unmoved. ACE lost rights while its AI searched and kept them when a rook was taken on its
/// corner.
fn ruled_state(recorded: &Value, oracle: &Oracle) -> Value {
    let mut s = recorded.clone();
    s[3] = serde_json::json!(oracle.castling(0));
    s[4] = serde_json::json!(oracle.castling(1));
    s
}

// ---------------------------------------------------------------------------------- games

/// Where a recorded game stopped following ACE's line.
#[derive(Debug, Default)]
struct Tally {
    on_line: usize,
    ruled_results: usize,
    ended: std::collections::BTreeMap<&'static str, usize>,
}

/// One recorded game, replayed with the transform V311/V326 needs: the port follows ACE's recorded
/// line (its generated moves, the AI's picks, the tried moves and their results, the evaluation,
/// the state and the board) for as long as ACE's engine and the client's rules agree, and at every
/// ply, on the line or not, it must agree with the client's rules playing the same game
/// ([`Oracle`]). Where ACE's record differs from the client's rules the port follows the client:
/// a tried move's result (a refusal is the client's own, not ACE's BadMoveInvalidCommand; a mate is
/// checkmate, not check), the castles generated and the castling rights (ACE castled through
/// attacked squares and lost or kept rights wrongly), and ACE's quick checkmate test (it counted
/// pseudo-legal moves). A difference that changes the game (a move ACE allowed and the client
/// refuses, an AI pick scored for the other side or over a board ACE's search had corrupted, a
/// board or state ACE's undo left wrong) ends the replay of that game: ACE's later plies are for a
/// board the port never reaches.
fn replay_game(ai_color: i32, recs: &[Value], name: &str, tally: &mut Tally) {
    let mut port = new_logic();
    let mut oracle = Oracle::new();
    let end = |tally: &mut Tally, why: &'static str| {
        *tally.ended.entry(why).or_insert(0) += 1;
    };
    for (i, rec) in recs.iter().enumerate() {
        let at = format!("{name} ply {i}");
        assert_agrees(&mut port, &oracle, &at);
        let turn = port.turn;

        // ACE's generated moves; the castles among them are the client's
        let Some(recorded_moves) = rec.get("moves").and_then(Value::as_array) else {
            return end(tally, "ACE threw");
        };
        let mut storage = Vec::new();
        port.generate_moves(turn, &mut storage);
        let mine: Vec<String> = storage.iter().map(move_str).collect();
        let theirs: Vec<&str> = recorded_moves
            .iter()
            .map(|v| v.as_str().expect("str"))
            .collect();
        let not_castle = |s: &&str| from_to_str(s).0 & CASTLES == 0;
        assert_eq!(
            mine.iter()
                .map(String::as_str)
                .filter(not_castle)
                .collect::<Vec<_>>(),
            theirs
                .iter()
                .copied()
                .filter(not_castle)
                .collect::<Vec<_>>(),
            "{at}: GenerateMoves(Turn), castles aside"
        );
        for s in mine.iter().filter(|s| !not_castle(&s.as_str())) {
            let (_, from, to) = from_to_str(s);
            assert!(
                oracle.board.test_move(rc(from), rc(to)) > 0,
                "{at}: generated a castle the client refuses: {s}"
            );
        }

        if turn.0 == ai_color {
            let got = port.async_calculate_ai_simple_move(&ChessAiAsyncTurnKey);
            if got.0 == ChessMoveResult::NoMoveResult {
                assert!(
                    oracle.legal().is_empty(),
                    "{at}: the AI found no move but the client has one"
                );
                assert!(
                    port.game_over(turn).is_some(),
                    "{at}: no move and the game goes on"
                );
                return end(tally, "no move");
            }
            let r = oracle.play(got.1.expect("from"), got.2.expect("to"));
            assert_eq!(got.0 .0, r, "{at}: the AI's move by the client's rules");
            if ai_triple(got) != rec["ai"] {
                return end(tally, "the AI's pick");
            }
        } else if let Some(n) = rec.get("undo") {
            let n = u32::try_from(int(n)).expect("u32");
            port.undo_move(n);
            oracle.undo(n);
        } else {
            let tries = rec["tries"]
                .as_array()
                .unwrap_or_else(|| panic!("{at}: no tries"));
            for t in tries {
                let t = t.as_array().expect("try");
                let color = ChessColor(i32v(&t[0]));
                let (from, to) = (xy(i32v(&t[1]), i32v(&t[2])), xy(i32v(&t[3]), i32v(&t[4])));
                let want = oracle.result_of(color.0, from, to);
                let r = port.do_move(color, &from, &to);
                assert_eq!(
                    r.0, want,
                    "{at}: DoMove({color:?}, {from}, {to}) by the client's rules"
                );
                if r.0 > 0 {
                    assert_eq!(oracle.play(from, to), r.0);
                }
                let Some(ace) = t.get(5) else {
                    return end(tally, "ACE threw");
                };
                if i32v(ace) != r.0 {
                    tally.ruled_results += 1;
                    if (i32v(ace) > 0) != (r.0 > 0) {
                        return end(tally, "a move ACE allowed and the client refuses");
                    }
                }
            }
            if recorded_moves.is_empty() && rec.get("check").is_none() {
                assert!(
                    port.game_over(port.turn).is_some(),
                    "{at}: no move and the game goes on"
                );
                return end(tally, "no move");
            }
        }

        // after the ply, against ACE's record
        assert_agrees(&mut port, &oracle, &format!("{at}, after"));
        if rec.get("throws").is_some() {
            return end(tally, "ACE threw");
        }
        if state(&port) != ruled_state(&rec["state"], &oracle)
            || board_str(&port) != rec["board"].as_str().expect("board")
        {
            return end(tally, "ACE's board or state");
        }
        let check = [
            port.in_check(ChessColor::White),
            port.in_check(ChessColor::Black),
        ];
        assert_eq!(serde_json::json!(check), rec["check"], "{at}: InCheck");
        let mate = port.in_checkmate(port.turn, false);
        if Some(mate) != rec["mate"].as_bool() {
            assert!(
                mate,
                "{at}: ACE's quick test found a mate the client does not"
            );
            tally.ruled_results += 1;
        }
        let eval = port.evaluate_board();
        assert!(
            same_f32(eval, f32_of(&rec["eval"]).expect("eval")),
            "{at}: EvaluateBoard {eval} vs {}",
            rec["eval"]
        );
        tally.on_line += 1;

        if rec.get("full").is_some() {
            // ACE's last step, its full checkmate test: the client's, and the board stays as it is
            let before = (state(&port), board_str(&port));
            let got = port.in_checkmate(port.turn, true);
            assert_eq!(
                got,
                oracle.board.is_player_in_check_mate(port.turn.0),
                "{at}: InCheckmate(Turn, true)"
            );
            assert_eq!(
                (state(&port), board_str(&port)),
                before,
                "{at}: the checkmate test moves nothing"
            );
            return end(tally, "the recorded end");
        }
    }
    end(tally, "the recorded end");
}

fn replay_file(name: &str) -> Tally {
    let file = vectors::load_named("chess", name);
    let mut tally = Tally::default();
    for Case { input, output } in &file.cases {
        let seed = int(&input["seed"]);
        ThreadSafeRandom::seed(u64::try_from(seed).expect("seed"));
        let recs = output["plies"].as_array().expect("plies");
        replay_game(
            i32v(&input["ai_color"]),
            recs,
            &format!("{name} seed {seed}"),
            &mut tally,
        );
    }
    tally
}

#[test]
fn chess_logic_games_replay_ace() {
    let tally = replay_file("games");
    // every game ends somewhere: 15 of 60 at ACE's recorded end, the rest where V311/V326 rules (467
    // plies on ACE's line, of 1641 recorded)
    assert_eq!(tally.ended.values().sum::<usize>(), 60, "{tally:?}");
    assert!(tally.on_line >= 450, "{tally:?}");
    assert!(
        tally
            .ended
            .get("the recorded end")
            .is_some_and(|&n| n >= 15),
        "{tally:?}"
    );
    assert!(tally.ruled_results > 0, "{tally:?}");
    assert!(
        tally
            .ended
            .contains_key("a move ACE allowed and the client refuses"),
        "{tally:?}"
    );
    assert!(tally.ended.contains_key("the AI's pick"), "{tally:?}");
}

#[test]
fn chess_logic_games_directed_replay_ace() {
    let tally = replay_file("games_directed");
    // 9 of 40 games at ACE's recorded end (284 plies on ACE's line, of 823 recorded)
    assert_eq!(tally.ended.values().sum::<usize>(), 40, "{tally:?}");
    assert!(tally.on_line >= 270, "{tally:?}");
    assert!(
        tally.ended.get("the recorded end").is_some_and(|&n| n >= 9),
        "{tally:?}"
    );
}

/// A plain minimax over the client's legal moves, scored for the maximising side.
fn minimax(logic: &mut ChessLogic, depth: u32, max: bool) -> f32 {
    if depth == 0 {
        let side = if max {
            logic.turn
        } else {
            chess::inverse_color(logic.turn)
        };
        return logic.evaluate_board_for(side);
    }
    let moves = logic.legal_moves(logic.turn);
    let mut best = if max { -9999.0f32 } else { 9999.0f32 };
    for m in &moves {
        logic.finalize_move(m);
        let v = minimax(logic, depth - 1, !max);
        logic.undo_move(1);
        best = if max { best.max(v) } else { best.min(v) };
    }
    best
}

/// ACE's recorded complex-search positions (V311/V326: ACE's values came from a search whose
/// minimising branch maximised, scored every leaf for White and corrupted the board as it undid,
/// so they are not compared). The moves reaching each position are replayed against the client's
/// rules; the alpha-beta search must give the plain minimax's value at depths 1 and 2 and leave the
/// board as it found it, and the complex AI must play a move the client's rules allow.
#[test]
fn chess_ai_complex_and_minimax_on_aces_positions() {
    let file = vectors::load_named("chess", "ai_complex");
    let mut searched = 0;
    for Case { input, .. } in &file.cases {
        let seed = int(&input["seed"]);
        let mut logic = new_logic();
        let mut oracle = Oracle::new();
        for p in input["plies"].as_array().expect("plies") {
            let p = p.as_array().expect("ply");
            let turn = logic.turn;
            let (from, to) = (xy(i32v(&p[0]), i32v(&p[1])), xy(i32v(&p[2]), i32v(&p[3])));
            let want = oracle.result_of(turn.0, from, to);
            let r = logic.do_move(turn, &from, &to);
            assert_eq!(r.0, want, "seed {seed}: DoMove by the client's rules");
            if r.0 > 0 {
                oracle.play(from, to);
            }
        }
        assert_agrees(&mut logic, &oracle, &format!("seed {seed}"));
        let before = (state(&logic), board_str(&logic));
        for (depth, max) in [(1u32, true), (1, false), (2, true), (2, false)] {
            let mut counter = 0u32;
            let v = logic.minimax_alpha_beta(depth, -10000.0, 10000.0, max, &mut counter);
            assert!(counter > 0);
            assert_eq!(
                (state(&logic), board_str(&logic)),
                before,
                "seed {seed}: the search leaves the board as it was"
            );
            assert_eq!(
                v,
                minimax(&mut logic, depth, max),
                "seed {seed}: depth {depth} max {max}"
            );
        }
        let mut counter = 0u32;
        let got = logic.async_calculate_ai_complex_move(&ChessAiAsyncTurnKey, &mut counter);
        if got.0 == ChessMoveResult::NoMoveResult {
            assert!(oracle.legal().is_empty(), "seed {seed}");
        } else {
            assert_eq!(
                got.0 .0,
                oracle.play(got.1.expect("from"), got.2.expect("to")),
                "seed {seed}: the complex AI's move"
            );
            assert_agrees(&mut logic, &oracle, &format!("seed {seed}, after the AI"));
        }
        searched += 1;
    }
    assert!(searched >= 10, "{searched}");
}

/// Fixed lines: the fool's mate, promotions with the AI searching afterwards, en passant, both
/// castles. Each step is compared with ACE's record while the port is on ACE's line (with the
/// castling rights and results V311/V326 rules on, see [`replay_game`]) and with the client's rules at
/// every step; once ACE's board or state leaves the client's (its AI's search corrupted it) only the
/// client's rules are followed.
#[test]
fn chess_logic_scripted_replay_ace() {
    let file = vectors::load_named("chess", "scripted");
    let mut results: Vec<Vec<Value>> = Vec::new();
    let mut left_line = 0;
    for Case { input, output } in &file.cases {
        let seed = int(&input["seed"]);
        let mut logic = new_logic();
        let mut oracle = Oracle::new();
        let mut on_line = true;
        let mut got_steps = Vec::new();
        let expected = output["steps"].as_array().expect("steps");
        for (i, step) in input["steps"].as_array().expect("steps").iter().enumerate() {
            let step = step.as_str().expect("step");
            let at = format!("script {seed} step {i} ({step})");
            let got = match step {
                "ai" => {
                    let turn = logic.turn;
                    let r = logic.async_calculate_ai_simple_move(&ChessAiAsyncTurnKey);
                    if r.0 == ChessMoveResult::NoMoveResult {
                        assert!(oracle.legal().is_empty(), "{at}");
                        assert!(logic.game_over(turn).is_some(), "{at}");
                    } else {
                        assert_eq!(
                            r.0 .0,
                            oracle.play(r.1.expect("from"), r.2.expect("to")),
                            "{at}: the AI's move"
                        );
                        assert!(!logic.in_check(turn), "{at}: the AI left its king in check");
                    }
                    ai_triple(r)
                }
                "mate" => {
                    let m = logic.in_checkmate(logic.turn, true);
                    assert_eq!(
                        m,
                        oracle.board.is_player_in_check_mate(logic.turn.0),
                        "{at}"
                    );
                    serde_json::json!(m)
                }
                "undo" => {
                    logic.undo_move(1);
                    oracle.undo(1);
                    serde_json::json!(1)
                }
                _ => {
                    let b = step.as_bytes();
                    let c = |f: u8, r: u8| xy(i32::from(f - b'a'), i32::from(r - b'1'));
                    let (from, to) = (c(b[0], b[1]), c(b[2], b[3]));
                    let turn = logic.turn;
                    let want = oracle.result_of(turn.0, from, to);
                    let r = logic.do_move(turn, &from, &to).0;
                    assert_eq!(r, want, "{at}: by the client's rules");
                    if r > 0 {
                        oracle.play(from, to);
                    }
                    serde_json::json!(r)
                }
            };
            assert_agrees(&mut logic, &oracle, &at);
            if on_line {
                match expected.get(i).and_then(Value::as_array) {
                    Some(want) if want[0] == got || step != "ai" => {
                        if want[0] != got {
                            // a result V311/V326 rules on: the client's (checked above), not ACE's
                            assert!(step != "undo", "{at}: {got} vs ACE's {}", want[0]);
                        }
                        if state(&logic) != ruled_state(&want[1], &oracle)
                            || board_str(&logic) != want[2].as_str().expect("board")
                        {
                            on_line = false;
                            left_line += 1;
                        }
                    }
                    // ACE threw, or its AI picked another move: the line ends
                    _ => {
                        on_line = false;
                        left_line += 1;
                    }
                }
            }
            got_steps.push(got);
        }
        results.push(got_steps);
    }
    assert!(
        left_line > 0 && left_line < file.cases.len(),
        "{left_line} scripts left ACE's line"
    );

    // the fool's mate: Qh4 is checkmate (0x801; ACE reported check, 0x401), the full test agrees,
    // and White, mated, has no move for the AI to make (ACE threw)
    let fools = &results[0];
    assert_eq!(
        fools[3],
        serde_json::json!(mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_CHECKMATE)
    );
    assert_eq!(fools[4], serde_json::json!(true));
    assert_eq!(fools[5], serde_json::json!([0, "n", "n"]));
}

// ---------------------------------------------------------------------------------- V311/V326

fn at(s: &str) -> ChessPieceCoord {
    let b = s.as_bytes();
    xy(i32::from(b[0] - b'a'), i32::from(b[1] - b'1'))
}

/// Plays `moves` (e.g. "e2e4"), each by the side to move, and returns the results.
fn play(logic: &mut ChessLogic, moves: &[&str]) -> Vec<i32> {
    moves
        .iter()
        .map(|m| {
            let turn = logic.turn;
            logic.do_move(turn, &at(&m[0..2]), &at(&m[2..4])).0
        })
        .collect()
}

/// V311/V326: 1. f3 e5 2. g4 Qh4 is checkmate by the client's rules (ACE: check only), the game is
/// over with Black the winner, and White has no move left.
#[test]
fn the_fools_mate_is_checkmate() {
    let mut logic = new_logic();
    assert_eq!(play(&mut logic, &["f2f3", "e7e5", "g2g4"]), [1, 1, 1]);
    assert_eq!(
        play(&mut logic, &["d8h4"]),
        [mr::OK_MOVE_TO_EMPTY_SQUARE | mr::OK_MOVE_CHECKMATE]
    );
    assert!(logic.in_check(ChessColor::White));
    assert!(logic.in_checkmate(ChessColor::White, false));
    assert_eq!(
        logic.game_over(ChessColor::White),
        Some(ChessColor::Black.0)
    );
    assert!(logic.legal_moves(ChessColor::White).is_empty());
    // any move White tries is refused, with the client's reason
    assert_eq!(play(&mut logic, &["a2a3"]), [mr::BAD_MOVE_SELF_CHECK]);
    assert_eq!(
        logic.async_calculate_ai_simple_move(&ChessAiAsyncTurnKey).0,
        ChessMoveResult::NoMoveResult
    );
}

/// White to castle king side with the f-file open and a black rook on it: f1, the square the king
/// crosses, is attacked though empty.
fn castle_through_the_f_file() -> ChessLogic {
    let mut logic = new_logic();
    for sq in ["f1", "g1", "f2", "f7"] {
        logic.remove_piece_at(&at(sq));
    }
    logic.add_piece_at(ChessColor::Black, ChessPieceType::Rook, at("f5"));
    logic
}

/// V311/V326: castling through an attacked empty square is refused with BadMoveCantCastleThroughCheck
/// (ACE let a rook, bishop or queen attack only occupied squares, so it castled). The castle is not
/// generated either, and with the file closed again it is played.
#[test]
fn castling_through_an_attacked_empty_square_is_refused() {
    let mut logic = castle_through_the_f_file();
    assert!(
        logic.can_attack(ChessColor::Black, &at("f1")),
        "the rook attacks the empty f1"
    );
    let generated = |logic: &ChessLogic| {
        let mut storage = Vec::new();
        logic.generate_moves(ChessColor::White, &mut storage);
        storage
            .iter()
            .any(|m| m.flags.contains(ChessMoveFlag::KingSideCastle))
    };
    assert!(!generated(&logic));
    assert!(!logic
        .legal_moves(ChessColor::White)
        .iter()
        .any(|m| m.to == at("g1") && m.from == at("e1")));
    assert_eq!(
        play(&mut logic, &["e1g1"]),
        [mr::BAD_MOVE_CANT_CASTLE_THROUGH_CHECK]
    );
    assert_eq!(
        logic.get_piece(&at("e1")).map(|p| p.r#type),
        Some(ChessPieceType::King),
        "nothing moved"
    );

    // a white pawn back on f2 closes the file
    let mut logic = castle_through_the_f_file();
    logic.add_piece_at(ChessColor::White, ChessPieceType::Pawn, at("f2"));
    assert!(generated(&logic));
    assert_eq!(play(&mut logic, &["e1g1"]), [mr::OK_MOVE_TO_EMPTY_SQUARE]);
    assert_eq!(
        logic.get_piece(&at("g1")).map(|p| p.r#type),
        Some(ChessPieceType::King)
    );
    assert_eq!(
        logic.get_piece(&at("f1")).map(|p| p.r#type),
        Some(ChessPieceType::Rook)
    );
    assert_eq!(logic.castling.get(0), ChessMoveFlag::None);
}

/// V311/V326: a rook that has moved (and come back) no longer castles, on its side only; and a rook
/// taken on its corner ends that side's castle (ACE looked at the square the capturer left, so the
/// right stayed). The client refuses that castle because the first piece on the king's way is not
/// its own rook (`BadMoveWouldCollide`, V326).
#[test]
fn castling_rights_follow_the_rooks() {
    let mut logic = new_logic();
    for sq in ["f1", "g1", "b1", "c1", "d1"] {
        logic.remove_piece_at(&at(sq));
    }
    assert_eq!(
        play(&mut logic, &["h1g1", "a7a6", "g1h1", "a6a5"]),
        [1, 1, 1, 1]
    );
    assert_eq!(logic.castling.get(0), ChessMoveFlag::QueenSideCastle);
    assert_eq!(
        play(&mut logic, &["e1g1"]),
        [mr::BAD_MOVE_CANT_CASTLE_AFTER_MOVING]
    );
    assert_eq!(play(&mut logic, &["e1c1"]), [mr::OK_MOVE_TO_EMPTY_SQUARE]);
    assert_eq!(
        logic.get_piece(&at("d1")).map(|p| p.r#type),
        Some(ChessPieceType::Rook)
    );

    // a black bishop takes the rook on h1
    let mut logic = new_logic();
    for sq in ["f1", "g1", "g2"] {
        logic.remove_piece_at(&at(sq));
    }
    logic.add_piece_at(ChessColor::Black, ChessPieceType::Bishop, at("c6"));
    assert_eq!(play(&mut logic, &["a2a3", "c6h1"]), [1, 2]);
    assert_eq!(
        logic.castling.get(0),
        ChessMoveFlag::QueenSideCastle,
        "White's king-side castle is gone"
    );
    assert_eq!(
        play(&mut logic, &["e1g1"]),
        [mr::BAD_MOVE_WOULD_COLLIDE],
        "the bishop on h1 is in the way"
    );
}

/// V311/V326: undoing a move puts back the side to move, the castling rights and a promoted pawn (and
/// what it captured). ACE left the opponent to move, the rights cleared, and an opponent's pawn on
/// the promotion square with the queen on the pawn's.
#[test]
fn undo_puts_back_the_side_to_move_the_castling_rights_and_a_promoted_pawn() {
    let mut logic = new_logic();
    let before = (state(&logic), board_str(&logic));
    assert_eq!(play(&mut logic, &["e2e4"]), [1]);
    logic.undo_move(1);
    assert_eq!((state(&logic), board_str(&logic)), before);

    // a white pawn on g7 takes the rook on h8 and promotes
    let mut logic = new_logic();
    logic.remove_piece_at(&at("g7"));
    logic.remove_piece_at(&at("g2"));
    let pawn = logic
        .add_piece_at(ChessColor::White, ChessPieceType::Pawn, at("g7"))
        .expect("pawn");
    pawn.guid = ObjectGuid::new(0x8000_0100);
    let before = (state(&logic), board_str(&logic));
    let r = play(&mut logic, &["g7h8"])[0];
    assert_eq!(
        r & (mr::OK_MOVE_MASK | mr::OK_MOVE_PROMOTION),
        mr::OK_MOVE_TO_OCCUPIED_SQUARE | mr::OK_MOVE_PROMOTION,
        "{r:#X}"
    );
    assert_eq!(
        logic.get_piece(&at("h8")).map(|p| (p.r#type, p.color)),
        Some((ChessPieceType::Queen, ChessColor::White))
    );
    assert_eq!(
        logic.castling.get(1),
        ChessMoveFlag::QueenSideCastle,
        "Black's rook on h8 is gone"
    );
    logic.undo_move(1);
    assert_eq!((state(&logic), board_str(&logic)), before);
    assert_eq!(logic.turn, ChessColor::White);
}

/// V311/V326: the AI scores positions for the side it plays: as White it takes a queen left hanging
/// (1. e4 e5 2. Nf3 Qh4: Nxh4). ACE scored every position for Black.
#[test]
fn an_ai_playing_white_takes_a_free_queen() {
    let mut logic = new_logic();
    assert_eq!(
        play(&mut logic, &["e2e4", "e7e5", "g1f3", "d8h4"]),
        [1, 1, 1, 1]
    );
    let (r, from, to) = logic.async_calculate_ai_simple_move(&ChessAiAsyncTurnKey);
    assert_eq!(
        (from, to),
        (Some(at("f3")), Some(at("h4"))),
        "the knight takes the queen"
    );
    assert_eq!(r.0 & mr::OK_MOVE_MASK, mr::OK_MOVE_TO_OCCUPIED_SQUARE);
    assert!(logic
        .get_piece_by_type(ChessColor::Black, ChessPieceType::Queen)
        .is_none());

    // and the deeper search, from the same position
    let mut logic = new_logic();
    play(&mut logic, &["e2e4", "e7e5", "g1f3", "d8h4"]);
    let mut counter = 0;
    let (_, from, to) = logic.async_calculate_ai_complex_move(&ChessAiAsyncTurnKey, &mut counter);
    assert_eq!((from, to), (Some(at("f3")), Some(at("h4"))));
}

/// V311/V326: the AI plays only moves the client's rules allow, never leaves its own king in check, and
/// its game agrees with the client's rules at every ply (the same legal moves, check, checkmate,
/// castling and en passant, promotion's results); a game it cannot go on with is over by the
/// client's rules. The AI plays both sides, after a few opening moves picked from the client's
/// legal moves so the games differ.
#[test]
fn the_ai_plays_only_the_clients_legal_moves() {
    let mut promotions = 0;
    let mut ended = 0;
    for seed in 1u64..=8 {
        let mut logic = new_logic();
        let mut oracle = Oracle::new();
        let mut rng = seed.wrapping_mul(0x9E37_79B9_7F4A_7C15);
        for _ in 0..(seed % 4 + 2) {
            let legal = oracle.legal();
            rng = rng
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            let (from, to) = legal[usize::try_from(rng >> 33).expect("fits") % legal.len()];
            let turn = logic.turn;
            assert_eq!(
                logic.do_move(turn, &from, &to).0,
                oracle.result_of(turn.0, from, to)
            );
            oracle.play(from, to);
        }
        for ply in 0..300 {
            let at = format!("seed {seed} ply {ply}");
            assert_agrees(&mut logic, &oracle, &at);
            assert_same_legal_moves(&logic, &oracle, &at);
            let mover = logic.turn;
            let r = if ply % 3 == 2 {
                // every third move is a random legal one, so the games go somewhere
                let legal = oracle.legal();
                rng = rng
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1_442_695_040_888_963_407);
                let (from, to) = legal[usize::try_from(rng >> 33).expect("fits") % legal.len()];
                let r = logic.do_move(mover, &from, &to);
                assert_eq!(
                    r.0,
                    oracle.play(from, to),
                    "{at}: {from} -> {to} by the client's rules"
                );
                r
            } else {
                let (r, from, to) = logic.async_calculate_ai_simple_move(&ChessAiAsyncTurnKey);
                if r == ChessMoveResult::NoMoveResult {
                    assert!(
                        oracle.legal().is_empty(),
                        "{at}: the AI found no move but the client has one"
                    );
                    assert!(logic.game_over(mover).is_some(), "{at}");
                    ended += 1;
                    break;
                }
                assert_eq!(
                    r.0,
                    oracle.play(from.expect("from"), to.expect("to")),
                    "{at}: the AI's move by the client's rules"
                );
                r
            };
            assert!(
                !logic.in_check(mover),
                "{at}: the mover left its own king in check"
            );
            if r.0 & mr::OK_MOVE_PROMOTION != 0 {
                promotions += 1;
            }
            if let Some(winner) = logic.game_over(logic.turn) {
                if r.0 & mr::OK_MOVE_CHECKMATE != 0 {
                    assert_eq!(winner, mover.0, "{at}: the mover wins");
                } else {
                    assert_eq!(
                        winner,
                        empyrean_world::entity::chess::chess::CHESS_WINNER_STALEMATE,
                        "{at}"
                    );
                }
                assert_agrees(&mut logic, &oracle, &at);
                ended += 1;
                break;
            }
        }
    }
    // the games reach promotions and endings
    assert!(
        promotions > 0 && ended > 0,
        "{promotions} promotions, {ended} games ended"
    );
}

mod lifecycle {
    //! ACE: Source/ACE.Server/Entity/Chess/ChessMatch.cs::ChessMatch
    use std::time::Duration;

    use empyrean_common::clock::ClockSnapshot;
    use empyrean_common::dotnet::datetime::DotNetDateTime;
    use empyrean_dat::FakeDats;
    use empyrean_entity::ObjectGuid;
    use empyrean_world::dispatch::Class;
    use empyrean_world::entity::chess::chess_match;
    use empyrean_world::world_objects::game;
    use empyrean_world::world_objects::world_object::WorldObject;
    use empyrean_world::World;

    fn world() -> World {
        let now = ClockSnapshot {
            portal_year_ticks: 0.0,
            unix_time: 1_790_000_000.0,
            utc: DotNetDateTime::new(2026, 9, 1),
            monotonic: Duration::ZERO,
        };
        World::new(now, FakeDats::new().build().expect("fake dats"))
    }

    fn board(w: &mut World, guid: u32) -> ObjectGuid {
        let mut o = WorldObject::allocate(Class::Game);
        o.guid = ObjectGuid::new(guid);
        w.objects.insert(o).expect("fresh");
        ObjectGuid::new(guid)
    }

    /// A match lives in `World.chess_matches` while its board refers to it; one no board, player or
    /// piece refers to leaves the store when the next match is made.
    #[test]
    fn chess_matches_live_in_the_world_store_while_referenced() {
        let mut w = world();
        let (b1, b2, b3) = (
            board(&mut w, 0x7000_0001),
            board(&mut w, 0x7000_0002),
            board(&mut w, 0x7000_0003),
        );

        let m1 = chess_match::new(&mut w, b1);
        game::set_chess_match(&mut w, b1, Some(m1));
        let m2 = chess_match::new(&mut w, b2);
        game::set_chess_match(&mut w, b2, Some(m2));
        assert_ne!(m1, m2);
        assert_eq!(w.chess_matches.len(), 2, "both referenced");
        assert_eq!(m1.get(&w).chess_board, b1);

        game::set_chess_match(&mut w, b1, None);
        let m3 = chess_match::new(&mut w, b3);
        assert_eq!(
            w.chess_matches.len(),
            2,
            "the unreferenced first match is gone"
        );
        assert_eq!((m2.get(&w).chess_board, m3.get(&w).chess_board), (b2, b3));
    }

    // ------------------------------------------------------------------- the legacy game (V309)

    const LB: u16 = 0xA9B4;

    struct EmptyShard;

    impl empyrean_world::managers::guid_manager::ShardGuidQueries for EmptyShard {
        fn get_max_guid_found_in_range(&mut self, _min: u32, _max: u32) -> u32 {
            u32::MAX
        }

        fn get_sequence_gaps(&mut self, _min: u32, _limit: u32) -> Vec<(u32, u32)> {
            Vec::new()
        }
    }

    /// A world with the twelve legacy piece weenies, flat land and a loaded landblock.
    fn legacy_world() -> World {
        use empyrean_content::models::world::Weenie as WeenieRow;
        use empyrean_entity::enums::{PropertyDataId, PropertyString, WeenieType};
        let mut content = empyrean_content::MemContent::new();
        let names = ["rook", "knight", "bishop", "queen", "king", "pawn"];
        for (i, name) in ["drudge", "mosswart"]
            .iter()
            .flat_map(|a| names.iter().map(move |n| format!("{a}{n}")))
            .enumerate()
        {
            let wcid = 5000 + u32::try_from(i).expect("small");
            let row = WeenieRow::new(wcid, &name, WeenieType::GamePiece)
                .with_string(PropertyString::Name, &name)
                .with_did(PropertyDataId::Setup, empyrean_testkit::land::TEST_SETUP);
            content = content.weenie(row);
        }
        let now = ClockSnapshot {
            portal_year_ticks: 0.0,
            unix_time: 1_790_000_000.0,
            utc: DotNetDateTime::new(2026, 9, 1),
            monotonic: Duration::ZERO,
        };
        let mut w = World::new(
            now,
            empyrean_testkit::dats::with_stat_tables(FakeDats::new())
                .build()
                .expect("fake dats"),
        );
        w.content = std::sync::Arc::new(content);
        empyrean_world::managers::guid_manager::initialize(&mut w, &mut EmptyShard);
        empyrean_testkit::land::use_flat_land_with_test_setup(&mut w, &[LB], 0);
        empyrean_world::managers::landblock_manager::get_landblock(
            &mut w,
            empyrean_entity::LandblockId::new(u32::from(LB) << 16 | 0xFFFF),
            false,
            false,
        );
        w
    }

    /// Runs every due delayed chain and the actors' queues, `seconds` after now.
    fn run_after(w: &mut World, seconds: f64, actors: &[ObjectGuid]) {
        use empyrean_world::entity::actions::{action_queue, delay_manager, i_actor::Actor};
        w.now.portal_year_ticks += seconds;
        for _ in 0..4 {
            delay_manager::run_actions(w);
            action_queue::run_actions(w, Actor::World);
            action_queue::run_actions(
                w,
                Actor::Landblock(empyrean_entity::LandblockId::new(
                    u32::from(LB) << 16 | 0xFFFF,
                )),
            );
            for &a in actors {
                action_queue::run_actions(w, Actor::Object(a));
            }
        }
    }

    /// Not ACE's (V309, owner 2026-09-24, a fix): a player who never played is charged a lost game
    /// by the legacy game (total games 1, games lost 1); a second game counts again.
    #[test]
    fn a_legacy_game_counts_a_first_game_as_lost() {
        let mut w = legacy_world();
        let board = board(&mut w, 0x7000_0010);
        let spot = empyrean_entity::Position::from_components(
            u32::from(LB) << 16 | 1,
            100.0,
            100.0,
            0.0,
            0.0,
            0.0,
            0.0,
            1.0,
            false,
        );
        w.objects
            .get_mut(board)
            .expect("board")
            .set_location(Some(spot));
        let player = ObjectGuid::new(0x5000_0001);
        let mut o = WorldObject::allocate(Class::Player);
        o.guid = player;
        w.objects.insert(o).expect("fresh");
        let session = empyrean_net::SessionId {
            client_id: 1,
            generation: 1,
        };
        w.sessions.insert(
            session,
            empyrean_world::sessions::SessionData {
                player: Some(player),
                ..Default::default()
            },
        );
        let counters = |w: &World| {
            let o = w.objects.get(player).expect("player");
            (o.chess_total_games(), o.chess_games_lost())
        };
        assert_eq!(counters(&w), (None, None), "never played");

        for games in 1..=2 {
            game::act_on_join_legacy(&mut w, board, player);
            run_after(&mut w, 5.0, &[board, player]);
            run_after(&mut w, 2.0, &[board, player]);
            assert_eq!(counters(&w), (Some(games), Some(games)), "game {games}");
            run_after(&mut w, 10.0, &[board, player]);
        }
    }
}
