//! One replay loop, and one shard a scenario can play.
//!
//! Every replaying scenario needs the same socket-free endpoint fed the same recorded datagrams
//! at the same clock. This module is that loop, once, together with the three things a scenario
//! then wants to do: move the session clock by the shard's own time-sync header, send an ordered
//! game event, and re-address a recorded blob to this session.
//!
//! # Nothing binds and nothing leaves
//!
//! [`endpoint`] builds a real [`ClientNetwork`] whose transport only queues: the address is a loopback
//! name for a direction (see [`dereth_client_net::client_session::testing::capture::peer`]) and no socket is opened.
//! Every datagram a scenario feeds was either recorded from a shard or built by [`Peer`] inside
//! this process, and every datagram the client writes stays in the endpoint's own queue until
//! [`crate::wire`] reads it.
//!
//! # Three clocks, and the difference is not cosmetic
//!
//! * A **recorded login into an `App`** feeds at the client's own local time. The login state
//!   machine's give-up timer is measured against the time a datagram arrives at, so a datagram fed
//!   at its recorded `t` arrives from the client's own future and the state machine stops.
//! * A **recorded login into the model backend** has no clock of its own to be ahead of, so it
//!   feeds at the recorded `t`, which keeps a replayed timestamp comparable with the recording's.
//! * The **session** clock is neither. It is the shard's, the client's only writer of it is the
//!   time-sync header, and [`Peer::set_clock`] is the only honest way a scenario moves it.

use std::ops::Range;

use dereth_client::net::ClientNetwork;
use dereth_client_net::client_session::testing::capture::{self, peer};
use dereth_primitives::ObjectId;

use crate::client::HeadlessClient;

/// The account a replay endpoint is named with. Only the recorded shard's own bytes drive the
/// login state machine, so this names the endpoint and nothing else.
pub const ACCOUNT: &str = "dereth-testkit";

/// The logon port every recording was made against.
pub const PORT: u16 = 7304;

/// The queue the ordered envelope arrives on.
pub const ORDERED_QUEUE: u16 = 9;

/// The queue an object create and the rest of the object stream arrive on.
pub const OBJECT_QUEUE: u16 = 10;

/// Every datagram of a recorded session, in recorded order.
///
/// # Panics
/// Panics when the recording does not load. The captures are committed to the repository, so a
/// missing one is a broken checkout, and a harness that carried on would be asserting over
/// nothing.
#[must_use]
pub fn records(session: &str) -> Vec<capture::Datagram> {
    capture::load_session(session).unwrap_or_else(|e| {
        panic!(
            "the recording {session} is this scenario's oracle and it is committed to the \
             repository; it did not load: {e}"
        )
    })
}

/// A socket-free endpoint addressed at `addr`, with `sequence` as its connection sequence number.
///
/// # Panics
/// Panics when `addr` is not an address. A replay endpoint binds nothing, so nothing else about
/// it can fail.
#[must_use]
pub fn endpoint(addr: &str, sequence: u32) -> ClientNetwork {
    ClientNetwork::new(addr, PORT, ACCOUNT, "unused", sequence)
        .expect("a replay endpoint binds nothing and cannot fail to be built")
}

/// A socket-free endpoint named with `account` rather than the harness's own, for the scenarios
/// whose claim is about the bytes the client's **own** login request carries.
///
/// # Panics
/// As [`endpoint`].
#[must_use]
pub fn endpoint_as(addr: &str, account: &str, password: &str, sequence: u32) -> ClientNetwork {
    ClientNetwork::new(addr, PORT, account, password, sequence)
        .expect("a replay endpoint binds nothing and cannot fail to be built")
}

/// The connection sequence number a recording's own authenticator carried.
#[must_use]
pub fn recorded_sequence(records: &[capture::Datagram]) -> u32 {
    dereth_headless::capture::connection_sequence_number(records).unwrap_or(0)
}

/// The endpoint a recording replays into: pair 0's address, and the connection sequence number
/// the recording's own authenticator carried.
#[must_use]
pub fn recorded_endpoint(records: &[capture::Datagram]) -> ClientNetwork {
    endpoint(&peer(0).to_string(), recorded_sequence(records))
}

/// [`recorded_endpoint`] under a named account. See [`endpoint_as`].
#[must_use]
pub fn recorded_endpoint_as(
    records: &[capture::Datagram],
    account: &str,
    password: &str,
) -> ClientNetwork {
    endpoint_as(
        &peer(0).to_string(),
        account,
        password,
        recorded_sequence(records),
    )
}

/// The recipient id the established connection is opened with, and the two seeds its crypto
/// streams start from. They are this harness's own numbers -- nothing recorded carries them --
/// and they exist so that [`Peer`]'s stream and the client's agree.
const RECIPIENT: u16 = 0xB;
const CRYPTO_SEED: u32 = 0xDEAD_BEEF;
const ISSAC_SEED: u32 = 0x1234_5678;

/// The address the harness's own shard speaks from. Pair 1's, because pair 0 is the logon server
/// a recorded login uses and a scenario must not be able to confuse the two.
pub const PEER_ADDR: &str = "127.0.0.1:19001";

/// An endpoint with one already-established connection and no handshake, for a scenario whose
/// shard is [`Peer`] rather than a recording.
///
/// # Panics
/// As [`endpoint`].
#[must_use]
pub fn connected_endpoint(addr: &str) -> ClientNetwork {
    let mut net = endpoint(addr, 0);
    net.session.transport.add_connection(
        RECIPIENT,
        0,
        1,
        CRYPTO_SEED,
        ISSAC_SEED,
        Some(addr.parse().expect("the endpoint address parses")),
    );
    net
}

/// The shard, as far as a scenario that has no recording of one needs it: a crypto stream, a
/// sequence number, a blob counter and an ordered stamp, feeding real datagrams into the client's
/// own socket-free endpoint.
///
/// **It is not the recorded-login loop and does not replace it.** Nothing in it is recorded and it
/// never logs anybody in. What it exists for is the three things a recording cannot be asked for:
/// the **session clock** ([`Peer::set_clock`]), an **ordered game event** addressed to this
/// session ([`Peer::event`]), and a **recorded blob re-stamped** so that a session whose ordered
/// counter starts at zero does not stall on it ([`Peer::replay_blob`]).
pub struct Peer {
    crypto: dereth_transport::CryptoSystem,
    addr: std::net::SocketAddr,
    sequence: u32,
    blob: u32,
    stamp: u32,
    subject: ObjectId,
    /// The last server time this shard announced, so [`Peer::keep_alive`] can re-announce it
    /// without moving the session clock.
    server_time: f64,
}

impl std::fmt::Debug for Peer {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Peer")
            .field("addr", &self.addr)
            .field("sequence", &self.sequence)
            .field("blob", &self.blob)
            .field("stamp", &self.stamp)
            .field("subject", &self.subject)
            .field("server_time", &self.server_time)
            .finish()
    }
}

impl Peer {
    /// Attach a shard to `client` and answer it. `subject` is the object an ordered event is
    /// addressed to, which is the player's own body in every scenario that has one.
    ///
    /// **The caller must create `subject`, and [`Peer::attach_creating`] is the one that does it.**
    /// The failure is silent, so it is stated here: the client's ordered router asks its
    /// own object table whether it knows the object *before* it delivers, and parks the blob when
    /// it does not -- for as long as the scenario runs, with no event, no drop and no message. A
    /// shard attached to an id nothing ever created therefore delivers nothing at all, and every
    /// assertion after it is an assertion over silence. Seeding the **game** is not enough:
    /// [`crate::Given::APlayer`] writes a weenie into the world and tells the session nothing.
    ///
    /// It stays the low-level one because three scenario files already send their own create
    /// immediately after it, with fields of their own choosing.
    ///
    /// # Panics
    /// Panics when the client already has a link: a scenario gets one shard.
    pub fn attach(client: &mut HeadlessClient, subject: ObjectId) -> Self {
        let addr: std::net::SocketAddr = PEER_ADDR.parse().expect("the endpoint address parses");
        client.attach_replay(connected_endpoint(PEER_ADDR));
        Self {
            crypto: dereth_transport::CryptoSystem::new(CRYPTO_SEED),
            addr,
            sequence: 1,
            blob: 0,
            stamp: 0,
            subject,
            server_time: 0.0,
        }
    }

    /// [`Peer::attach`], and then **the shard creates its own subject** -- an `ItemCreateObject` on
    /// the object queue, which is how a real shard introduces one.
    ///
    /// A scenario whose shard talks about an object wants this and not the bare `attach`.
    ///
    /// The object is created and nothing else: whether it is the *player* is the scenario's own
    /// decision (`world_mut().player = Some(..)`), and so is its name.
    ///
    /// # Panics
    /// Panics when the client already has a link, and when the create did not reach the world --
    /// which names `subject`, because the alternative is the silence this exists to remove.
    pub fn attach_creating(client: &mut HeadlessClient, subject: ObjectId) -> Self {
        let mut me = Self::attach(client, subject);
        let mut p = dereth_protocol::objects::ObjectCreatePayload {
            id: subject,
            ..Default::default()
        };
        p.physicsdesc.bitfield |= dereth_protocol::types::physicsdesc::flags::SETUP;
        p.physicsdesc.setup_id = Some(0x0200_0001);
        p.physicsdesc.timestamps.instance = 1;
        let blob = dereth_protocol::write_blob(&dereth_protocol::objects::ItemCreateObject(p))
            .expect("an object create encodes");
        me.send(client, OBJECT_QUEUE, blob);
        client.tick(1);
        assert!(
            client.view().world().weenie(subject).is_some(),
            "this shard's create of {subject:?} did not reach the client's world, so every \
             ordered event addressed to it would have been parked in silence"
        );
        me
    }

    /// The object an ordered event is addressed to.
    #[must_use]
    pub const fn subject(&self) -> ObjectId {
        self.subject
    }

    /// Feed one datagram, stamped at the **client's own clock**.
    ///
    /// A fixed stamp such as `LocalTime(0.0)` would leave the 140-second silence clock the
    /// connection sweep measures against unmoved, and a scenario that ran long enough would have
    /// its link swept out from under it while the shard was talking. A datagram arrives when it
    /// arrives.
    fn feed(&self, client: &mut HeadlessClient, raw: &[u8]) {
        let now = crate::inbound::now_of(client);
        let out = client
            .replay_net_mut()
            .expect("the replay endpoint was attached when this peer was built")
            .session
            .transport
            .feed(raw, Some(self.addr), now);
        out.unwrap_or_else(|e| {
            panic!(
                "this shard's datagram was rejected ({e:?}) at t={:.3}, and there are two things \
                 it usually is. (1) **The datagram did not parse.** A blob longer than one \
                 fragment that was framed as one fragment is over the wire's own bound, and the \
                 reason reads NoConnection rather than \"too long\". \
                 Peer::send fragments, so a scenario that hits this is framing \
                 its own. (2) **The connection sweep took the link.** One that has heard nothing \
                 for 140 seconds of the client's own clock goes to DisconnectReceived and is then \
                 dropped, after which every datagram this peer writes is addressed to a \
                 connection that is gone; a scenario that runs a long silence says \
                 Peer::keep_alive(client) inside it.",
                now.0
            )
        });
    }

    fn packet(&mut self) -> dereth_transport::OutPacket {
        self.sequence += 1;
        dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: RECIPIENT,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        })
    }

    /// One blob on `queue`, **fragmented the way the transport fragments it**.
    ///
    /// One fragment with `num_frags: 1` would be a lie for anything over 448 bytes: the receiving
    /// blob sizes its buffer from the fragment headers, so a longer message would be rejected or
    /// reassembled into a truncated one. A recorded character description is 1,864 bytes, so the
    /// very first thing a scenario wants to send is already over the line.
    ///
    /// The fragmenter is the client's own, with 448 bytes per fragment. The packing rule puts
    /// fragments into the
    /// current datagram while the payload stays under `MAX_BUILD_PAYLOAD + 1` and the datagram
    /// holds fewer than `MAX_FRAGS_PER_PACKET`, and a new one starts otherwise. So a long message
    /// arrives as several datagrams in order, which is what reassembly is for.
    ///
    /// # Panics
    /// Panics when a datagram does not frame, which is a defect in the writer rather than a state
    /// the client could be in.
    pub fn send(&mut self, client: &mut HeadlessClient, queue: u16, bytes: Vec<u8>) {
        self.blob += 1;
        let mut blob = dereth_transport::blob::NetBlob::for_send(bytes, queue);
        // The id this harness's shard numbers its blobs with: the same high dword the one-fragment
        // writer used, so nothing about how the client keys a blob changes.
        blob.id = dereth_transport::blob::NetBlobId(0x8000_0000_0000_0000 | u64::from(self.blob));
        let mut datagrams: Vec<dereth_transport::OutPacket> = Vec::new();
        for frag in blob.fragmentize() {
            let need = frag.wire_len();
            let fits = datagrams
                .last()
                .is_some_and(|p: &dereth_transport::OutPacket| {
                    p.payload_len() + need < dereth_transport::wire::MAX_BUILD_PAYLOAD + 1
                        && p.fragments.len() < dereth_transport::wire::MAX_FRAGS_PER_PACKET
                });
            if !fits {
                datagrams.push(self.packet());
            }
            datagrams
                .last_mut()
                .expect("a datagram was just pushed if none fitted")
                .add_fragment(frag)
                .expect("the bounds were checked before the fragment was added");
        }
        assert!(
            !datagrams.is_empty(),
            "a zero-length blob fragmentises into nothing, so this send would have delivered \
             nothing at all"
        );
        for mut packet in datagrams {
            let raw = packet
                .serialize(Some(self.crypto.next()))
                .expect("a datagram");
            self.feed(client, &raw);
        }
    }

    /// Say something that is not a message, so the link is not silent.
    ///
    /// The connection sweep drops a link that has heard nothing for
    /// 140 seconds of the client's own clock, and a scenario that ticks through that much
    /// simulated time loses its shard mid-scenario. This re-announces the last server time this
    /// peer set -- a time-sync header, which is exactly what a real shard's keep-alive is -- and
    /// runs the frame that reads it. It moves no clock: [`Self::set_clock`] is the only thing that
    /// does.
    ///
    /// # Panics
    /// Panics when the datagram does not frame.
    pub fn keep_alive(&mut self, client: &mut HeadlessClient) {
        let t = self.server_time;
        self.time_sync(client, t);
    }

    /// One ordered game event addressed to [`Self::subject`], which is the envelope most of what a
    /// shard says to one player really arrives in.
    ///
    /// # Panics
    /// Panics when the message does not frame.
    pub fn event<M: dereth_protocol::Message>(&mut self, client: &mut HeadlessClient, m: &M) {
        self.stamp += 1;
        let blob = dereth_protocol::events::pack_event(self.subject, self.stamp, m)
            .expect("the event frames");
        self.send(client, ORDERED_QUEUE, blob);
    }

    /// Replay one recorded ordered blob, re-addressed and re-stamped for this session.
    ///
    /// The recorded sequence numbers are the ones that recording had reached; replaying them into
    /// a session whose ordered counter starts at zero would stall every one of them.
    ///
    /// # Panics
    /// Panics when `blob` is shorter than an ordered header.
    pub fn replay_blob(&mut self, client: &mut HeadlessClient, mut blob: Vec<u8>) {
        assert!(
            blob.len() >= 16,
            "an ordered blob is at least its own header"
        );
        self.stamp += 1;
        blob[4..8].copy_from_slice(&self.subject.0.to_le_bytes());
        blob[8..12].copy_from_slice(&self.stamp.to_le_bytes());
        self.send(client, ORDERED_QUEUE, blob);
    }

    /// Move the session clock, with the shard's own time-sync header, and run the frame that reads
    /// it.
    ///
    /// # Panics
    /// Panics when the datagram does not frame.
    pub fn set_clock(&mut self, client: &mut HeadlessClient, server_time: f64) {
        self.server_time = server_time;
        self.time_sync(client, server_time);
    }

    /// One time-sync datagram carrying `server_time`, and the frame that reads it.
    fn time_sync(&mut self, client: &mut HeadlessClient, server_time: f64) {
        let mut packet = self.packet();
        packet
            .add_optional_header(
                dereth_transport::PacketFlags::TIME_SYNC,
                server_time.to_le_bytes().to_vec(),
            )
            .expect("one time-sync section");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a datagram");
        self.feed(client, &raw);
        client.tick(1);
    }
}

// -------------------------------------------------------------------------------------------
// The raw-datagram loop
// -------------------------------------------------------------------------------------------

/// Feed a recording's **server-to-client datagrams** through the client's real transport, one
/// frame each, and enter the world when the shard offers a character set.
///
/// The lost-fragment, retry and unclean-logout scenarios are written over this. It is datagrams and not
/// blobs on purpose: reassembly, ordering and the connection sweep are the client's, and a
/// scenario whose subject is a *silence* or a *lost fragment* has nothing to assert if the harness
/// has already reassembled its stream for it.
///
/// [`crate::Inbound::from_raw_capture`] is the `when` step over this.
///
/// # Panics
/// Panics when `range` selects no server-to-client datagram, which would read as a pass over
/// nothing.
pub fn feed(client: &mut HeadlessClient, records: &[capture::Datagram], range: Range<usize>) {
    let mut fed = 0usize;
    let mut entered = false;
    for (i, r) in records.iter().enumerate() {
        if !range.contains(&i) {
            continue;
        }
        // Both directions are walked. Only the shard's half is delivered; the client's half moves
        // the recording's clock, which is what the connection sweep measures a silence against.
        let payload = if r.c2s {
            None
        } else {
            Some((r.raw.as_slice(), r.peer()))
        };
        fed += usize::from(payload.is_some());
        client.replay_datagram(payload, r.t, &mut entered);
    }
    assert!(
        fed > 0,
        "the recording carries no server-to-client datagram in {range:?}; a slice that delivers \
         nothing would read as a pass over nothing"
    );
}

#[cfg(test)]
mod tests {
    use dereth_client_net::client_session::testing::{Corpus, Direction};
    use dereth_primitives::ObjectId;

    use super::*;
    use crate::HeadlessClient;

    /// The body of the ordered **event** envelope starts at byte 16; the sub-type is the dword at
    /// 12. See `crate::inbound::event_of`, which is the one decision about which envelope a
    /// recorded blob is in.
    const ORDERED_EVENT: u32 = 0xF7B0;
    const PLAYER_DESCRIPTION: u32 = 0x0013;

    /// The first recorded `0x0013 Login_PlayerDescription` in the corpus, with the recording it
    /// came from.
    ///
    /// Nothing of what it says leaves this function: what the test asserts is its length and the
    /// fact that the client received it.
    fn a_recorded_description() -> (&'static str, Vec<u8>) {
        for name in dereth_client_net::client_session::testing::session_names() {
            let Ok(Some(corpus)) = Corpus::load(name) else {
                continue;
            };
            for b in &corpus.blobs {
                if b.dir != Direction::ServerToClient || b.opcode != ORDERED_EVENT {
                    continue;
                }
                let sub = b
                    .payload
                    .get(12..16)
                    .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]));
                if sub == Some(PLAYER_DESCRIPTION) {
                    return (name, b.payload.clone());
                }
            }
        }
        panic!(
            "no recording in the corpus carries a player description; the captures are committed to the repository, so this is a broken checkout"
        );
    }

    /// **The fragment proof.**
    ///
    /// A recorded character description is longer than one fragment can carry, and
    /// `Peer::send` must split it into fragments. This drives one end to end: the harness's
    /// own shard fragments it the way the transport does, the client's **real** transport
    /// reassembles it, and the HUD records that its description arrived.
    ///
    /// The premise is asserted first, so that a corpus whose descriptions became short enough to
    /// fit in one fragment fails here rather than quietly stopping being a test of fragmentation.
    #[test]
    fn a_recorded_character_description_is_longer_than_one_fragment_and_still_arrives() {
        let (session, blob) = a_recorded_description();
        assert!(
            blob.len() > dereth_transport::wire::MAX_FRAG_DATA,
            "{session}'s player description is {} bytes, which fits in one fragment, so this is \
             no longer a test of fragmentation",
            blob.len()
        );

        let player = ObjectId(0x5000_0001);
        let mut c = HeadlessClient::model();
        assert!(
            !c.view().hud().player_desc_received,
            "the premise: this client has not been told who it is"
        );

        let mut shard = Peer::attach_creating(&mut c, player);
        c.world_mut().player = Some(player);
        shard.replay_blob(&mut c, blob);
        c.tick(4);

        assert!(
            c.view().hud().player_desc_received,
            "the client did not receive the description the shard sent it; a blob longer than one fragment must be split into fragments for the transport to reassemble it"
        );
    }

    /// **The keep-alive.** A shard that says nothing for long enough loses its
    /// link to the connection sweep; `Peer::keep_alive` is the traffic that stops it, and this
    /// proves the shard is still able to deliver after one.
    #[test]
    fn a_shard_that_keeps_the_link_alive_can_still_deliver() {
        let (_, blob) = a_recorded_description();
        let player = ObjectId(0x5000_0002);
        let mut c = HeadlessClient::model();
        let mut shard = Peer::attach_creating(&mut c, player);
        c.world_mut().player = Some(player);
        shard.set_clock(&mut c, 1_000.0);
        for _ in 0..4 {
            c.tick(8);
            shard.keep_alive(&mut c);
        }
        shard.replay_blob(&mut c, blob);
        c.tick(4);
        assert!(
            c.view().hud().player_desc_received,
            "the shard could not deliver after keeping its link alive"
        );
    }

    /// A shard whose subject nothing created is a shard whose every ordered event
    /// is dropped in silence, so `attach` refuses it by name.
    #[test]
    fn a_shard_creates_its_own_subject() {
        let subject = ObjectId(0x5000_00FF);
        let mut c = HeadlessClient::model();
        assert!(
            c.view().world().weenie(subject).is_none(),
            "the premise: nothing has created this object"
        );
        let _shard = Peer::attach_creating(&mut c, subject);
        assert!(
            c.view().world().weenie(subject).is_some(),
            "attach_creating did not create its own subject"
        );
    }
}
