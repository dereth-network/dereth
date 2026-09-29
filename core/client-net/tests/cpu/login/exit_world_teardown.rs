//! Exit-world teardown drops the world connection and keeps the login server, leaves the survivor
//! NAK state clean, removes nothing on a single-server shard, never re-enters log-off, empties the
//! table after the goodbye, and a second entry starts from the first entry state.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_net::net::{Net, NetConfig, RecipientId};
use dereth_transport::conn::ConnectionState;
use dereth_transport::session::ReceiverState;
use dereth_transport::wire::{PacketFlags, ParsedPacket};

use std::net::SocketAddr;

fn addr(port: u16) -> SocketAddr {
    format!("127.0.0.1:{port}").parse().expect("addr")
}

/// The recipient id the login server is registered on here.
///
/// **Not 0, and that is the client's constraint rather than this file's.**
/// The handler's step 6 sets both the logon and the current-server recipient to the new
/// connection's id when the logon recipient is still 0, and both start at 0 — so `0` is the
/// *unset* marker, and a deployment that registered its login server on recipient 0 would
/// re-point the logon recipient at every connection that followed. Against a single-process shard
/// there is never a second connection, which is why every recorded ACE session gets away with it;
/// a split deployment cannot, and this file is the split-deployment case. \[verified\]
const LOGON: u16 = 1;
/// The world server the referral adds.
const WORLD: u16 = 3;

/// The login server alone, registered the same way as the connection-request handler does.
/// Against a single-process shard such as ACE this is the whole connection table for the life of
/// the session.
fn one_server() -> Net {
    let mut net = Net::new(NetConfig::default());
    net.add_connection(LOGON, 0, 1, 0xDEAD_BEEF, 0xF00D_F00D, Some(addr(19000)));
    net
}

/// The login server **and** a world server, which is the split deployment
/// `docs/networking/04-netblobs-and-queues.md` §5.1 exists for and the case that separates a
/// teardown from a no-op. The exit-world disconnect is what adds the second one in retail.
fn split_deployment() -> Net {
    let mut net = one_server();
    net.add_connection(WORLD, 3, 7, 0x1111_1111, 0x2222_2222, Some(addr(19100)));
    net
}

// ---------------------------------------------------------------------------------------------
// 1. The teardown itself
// ---------------------------------------------------------------------------------------------

/// Behaviour: login.exit-world.the-teardown-drops-the-world-connection-and-keeps-the-login-server
/// The claim: the exit-world teardown drops every connection that is **not** the login server,
/// keeps the login server, and points the current-server recipient back at it.
///
/// Two stations, because a census taken only afterwards cannot tell a teardown from a table that
/// was always that shape: the world connection is asserted **present and current** first.
#[test]
fn the_teardown_drops_the_world_and_keeps_the_login_server() {
    let mut net = split_deployment();
    net.enter_world();

    // Station 1 — in the world, two connections.
    assert_eq!(
        net.connection_state(RecipientId(LOGON)),
        ConnectionState::ConnectionRequestAcked
    );
    assert_eq!(
        net.connection_state(RecipientId(WORLD)),
        ConnectionState::ConnectionRequestAcked
    );
    assert_eq!(net.logon_recipient(), RecipientId(LOGON));
    assert!(net.in_game(), "entering the world set the in-game flag");

    // Station 2 — out of the world.
    let removed = net.exit_world_disconnect();
    assert_eq!(removed, 1, "one non-login connection went");
    assert_eq!(
        net.connection_state(RecipientId(WORLD)),
        ConnectionState::Disconnected,
        "the world server is gone after receiver removal and connection-table clearing"
    );
    assert_eq!(
        net.connection_state(RecipientId(LOGON)),
        ConnectionState::ConnectionRequestAcked,
        "the login server is kept -- the loop skips the logon recipient"
    );
    assert_eq!(
        net.logon_recipient(),
        RecipientId(LOGON),
        "the logon recipient is untouched here"
    );
    assert_eq!(
        net.world_recipient(),
        RecipientId(LOGON),
        "current server = logon recipient, the first statement after the in-game flag"
    );
    assert!(!net.in_game(), "the in-game flag is cleared");
}

/// The surviving connection's NAK state is reset to the no-NAK state,
/// which is what stops a retransmit request raised for the world we just left being re-asked on the
/// login connection.
///
/// The no-NAK state is **2** (`ReceiverState::NoNak`), pinned here as a literal so the
/// symbol and the number cannot agree with each other while both are wrong — the test-design constraints,
/// *"a test that reads a constant through the same symbol it writes it through"*.
#[test]
fn the_surviving_connection_leaves_its_nak_state_behind() {
    assert_eq!(ReceiverState::NoNak as u32, 2, "the no-NAK receiver state");

    let mut net = one_server();
    net.receiver_mut(RecipientId(LOGON))
        .expect("the login server")
        .nak_state = ReceiverState::Nak;
    assert_eq!(
        net.receiver(RecipientId(LOGON)).map(|r| r.nak_state),
        Some(ReceiverState::Nak),
        "station 1: the connection is in the NAK state"
    );

    net.exit_world_disconnect();
    assert_eq!(
        net.receiver(RecipientId(LOGON)).map(|r| r.nak_state),
        Some(ReceiverState::NoNak),
        "station 2: the no-NAK state"
    );
}

/// Against a single-process shard the loop removes nothing — and the rest of the teardown still
/// runs. This is the reachability half of the row's acceptance: the function is **reached** in this
/// build, and on this deployment its connection-table work is a no-op while its other three
/// statements are not.
#[test]
fn against_a_single_server_shard_the_loop_removes_nothing_and_the_rest_still_runs() {
    let mut net = one_server();
    net.enter_world();
    assert_eq!(
        net.logon_recipient(),
        net.world_recipient(),
        "ACE: one id for both"
    );

    let removed = net.exit_world_disconnect();
    assert_eq!(
        removed, 0,
        "the only connection *is* the logon recipient, so the loop skips it"
    );
    assert_eq!(
        net.connection_state(RecipientId(LOGON)),
        ConnectionState::ConnectionRequestAcked,
        "and the session keeps talking to the shard"
    );
    assert!(!net.in_game(), "but the in-game flag still falls");
    assert_eq!(
        net.logon_recipient(),
        RecipientId(LOGON),
        "and the logon recipient is not zeroed"
    );
}

// ---------------------------------------------------------------------------------------------
// 2. Connection removal's re-entry, which the row asked to be resolved either way
// ---------------------------------------------------------------------------------------------

/// The claim: removing a connection **can** re-enter the server log-off, and
/// the exit-world teardown **can never make it do so**.
///
/// ```text
/// if (id is the logon or the current-server recipient) and log-off not yet sent:
///     log off the server; send the server-died notice;
/// ```
///
/// The teardown sets the current-server recipient to the logon recipient on the line above its
/// loop and the loop skips the logon recipient, so every id it passes differs from both halves of
/// the test. The ordering of those
/// two statements is the whole guarantee; this asserts both directions, because a guard that can
/// never fire and a guard that never fires here are different claims and only the second is true.
#[test]
fn the_teardown_can_never_re_enter_log_off_server() {
    // Direction 1 — the branch is live. A packet-level disconnect on the current server is what
    // reaches it in retail (the optional-header processor's Disconnect arm, and the other callers of
    // connection removal).
    let mut net = split_deployment();
    assert!(
        net.remove_connection(RecipientId(LOGON)),
        "removing the logon recipient before log-off was sent re-enters the server log-off"
    );
    assert!(net.log_off_sent(), "and the client goes silent");

    // Direction 2 — the teardown's own loop never takes it. Every connection here is doomed except
    // the login server, and `exit_world_disconnect` asserts the same thing internally with a
    // `debug_assert!`; this is the observable form.
    let mut net = split_deployment();
    net.add_connection(9, 9, 1, 5, 6, Some(addr(19200)));
    assert_eq!(
        net.exit_world_disconnect(),
        2,
        "two non-login connections went"
    );
    assert!(
        !net.log_off_sent(),
        "and not one of them re-entered the server log-off: no Disconnect was queued"
    );
    assert!(
        net.take_outgoing().is_empty(),
        "the teardown puts nothing on the wire"
    );
}

/// The server log-off zeroes the logon recipient **before** it calls the teardown, so reached from
/// there the loop's guard matches nothing and *every* connection goes — and the `else` arm is
/// taken.
///
/// This is the asymmetric case (the test-design constraints): the same function, two callers, two different
/// tables afterwards. A test that only ever reached it through the server log-off would read the
/// wholesale removal as the function's normal behaviour.
#[test]
fn reached_through_the_goodbye_the_teardown_empties_the_table() {
    let mut net = split_deployment();
    net.enter_world();
    let sent = net.log_off_server();
    assert_eq!(
        sent, 2,
        "one Disconnect per connection, before anything is removed"
    );

    // The datagrams were built first: the server log-off sends, *then* tears down.
    let out = net.take_outgoing();
    assert_eq!(out.len(), 2);
    for (bytes, _) in &out {
        let p = ParsedPacket::parse(bytes).expect("parses");
        assert!(p.header.header.contains(PacketFlags::DISCONNECT));
    }

    assert_eq!(
        net.connection_state(RecipientId(LOGON)),
        ConnectionState::Disconnected
    );
    assert_eq!(
        net.connection_state(RecipientId(WORLD)),
        ConnectionState::Disconnected
    );
    assert_eq!(
        net.logon_recipient(),
        RecipientId(0),
        "the `else` arm: nothing survived, so the logon recipient = 0"
    );
    assert!(!net.in_game());
}

// ---------------------------------------------------------------------------------------------
// 3. The second station that the row is actually about
// ---------------------------------------------------------------------------------------------

/// A second entry starts from the first entrys state.
#[test]
fn a_second_entry_starts_from_the_first_entrys_state() {
    let mut net = split_deployment();

    // Station 1 — first entry.
    net.enter_world();
    let first = (net.logon_recipient(), net.world_recipient(), net.in_game());
    assert_eq!(first, (RecipientId(LOGON), RecipientId(LOGON), true));

    // Station 2 — out.
    net.exit_world_disconnect();
    assert!(!net.in_game());
    assert_eq!(
        net.connection_state(RecipientId(WORLD)),
        ConnectionState::Disconnected
    );

    // Station 3 — in again, over the connection the teardown kept.
    net.add_connection(WORLD, 3, 8, 0x3333_3333, 0x4444_4444, Some(addr(19100)));
    net.enter_world();
    assert_eq!(
        (net.logon_recipient(), net.world_recipient(), net.in_game()),
        first,
        "the second entry is in the state the first one was"
    );
    assert!(
        !net.log_off_sent(),
        "and nothing along the way made the client go silent"
    );
}
