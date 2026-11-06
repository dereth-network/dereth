//! One connection's transport, either end: receive window, send queue, reassembly.
//!
//! Built from the shared transport's role-neutral pieces the way a client connection assembles
//! them: the receive window checks sequence numbers and the key-stream checksums and parks keys
//! for gaps, the send queue numbers, keys, coalesces and caches packets for retransmission, the
//! indicator reassembles messages. The game's transport is the same machine at both ends with the
//! seeds swapped. What differs by role:
//!
//! - the **client** sends its clock request and an echo request every six half-second intervals;
//! - the **server** sends its clock with its first data and every [`SERVER_TIME_SYNC_INTERVAL`]
//!   seconds, and answers each echo request at once with an acknowledgement;
//! - the **server** sends its large messages at a pace ([`crate::pace`]).
//!
//! Sans-IO: the caller passes the time and sends the datagrams [`Endpoint::tick`] returns.

use std::net::SocketAddr;

use dereth_primitives::LocalTime;
use dereth_transport::blob::{NetBlob, NetBlobId};
use dereth_transport::conn::{ConnectSeeds, Referral, ServerSwitch};
use dereth_transport::flow::{FlowQueue, IntervalEvent, TimeSensitive, ACK_INTERVAL, NAK_INTERVAL};
use dereth_transport::indicator::{CompletedBlob, Indicator};
use dereth_transport::session::{lhs_newer, ReceiverData, RejectReason, MAX_NAK_IDS};
use dereth_transport::wire::optional::seq_ids;
use dereth_transport::wire::{OutPacket, PacketFlags, ParsedPacket, ProtoHeader};
use dereth_transport::CryptoSystem;

use crate::pace::Pace;

/// How often the server sends its clock: the January 2017 servers sent it with the first data
/// packet and then about every 20 seconds.
pub const SERVER_TIME_SYNC_INTERVAL: f64 = 20.0;

/// The most packets resent in one [`Endpoint::tick`]: as many as one retransmit request can ask
/// for. The rest of a longer list is dropped; the peer asks again for what is still missing.
pub const MAX_RESENDS_PER_TICK: usize = MAX_NAK_IDS;

/// Which end of the connection this is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server,
    Client,
}

/// Counters for logs and tests.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Stats {
    pub packets_in: u64,
    pub packets_out: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub rejected: u64,
    pub naks_sent: u64,
    pub retransmitted: u64,
    pub blobs_in: u64,
    pub blobs_out: u64,
}

/// What one received packet carried for the layer above.
#[derive(Debug, Default)]
pub struct Incoming {
    pub blobs: Vec<CompletedBlob>,
    pub referral: Option<Referral>,
    pub server_switch: Option<ServerSwitch>,
    pub time_sync: Option<f64>,
    /// The peer said goodbye.
    pub disconnect: bool,
}

/// One connection.
#[derive(Debug)]
pub struct Endpoint {
    pub role: Role,
    /// Where data goes: the client's address (server), the server's even port (client).
    pub peer: SocketAddr,
    pub recv: ReceiverData,
    pub flow: FlowQueue,
    pub indicator: Indicator,
    /// The id this end puts in its headers' recipient field.
    pub own_id: u16,
    flush_num: u32,
    /// Server: the game clock at `clock_base`. Client: the last clock received.
    game_time: f64,
    clock_base: LocalTime,
    /// Server: the game clock is held at `game_time`.
    held: bool,
    last_time_sync: Option<LocalTime>,
    ack_now: bool,
    /// The highest sequence the peer has acknowledged: by its acknowledgement, or by asking for
    /// the one after it again (it has everything before the first it asks for).
    pub peer_acked: u32,
    last_message_seq: u32,
    unsent_blobs: usize,
    /// Server: the large messages, sent at a pace.
    pub pace: Pace,
    pub stats: Stats,
}

impl Endpoint {
    /// A connection whose handshake has been made: `own_id` goes in this end's headers (the
    /// server's id for a server, the id the server gave the client for a client), `peer_id` is the
    /// other end's, `seeds` are the handshake's, and `game_time` is the server's clock now.
    #[allow(clippy::too_many_arguments)]
    #[must_use]
    pub fn new(
        role: Role,
        own_id: u16,
        peer_id: u16,
        iteration: u16,
        seeds: ConnectSeeds,
        peer: SocketAddr,
        game_time: f64,
        now: LocalTime,
    ) -> Self {
        let (incoming, outgoing) = match role {
            Role::Server => (seeds.client_to_server, seeds.server_to_client),
            Role::Client => (seeds.server_to_client, seeds.client_to_server),
        };
        // The receiver's id must be non-zero for sequenced packets to be accepted.
        let mut recv = ReceiverData::new(peer_id.max(1), incoming, outgoing);
        recv.net_id = own_id;
        recv.iteration = iteration;
        recv.addr = Some(peer.ip());
        // The first sequenced packet either side sends is 2.
        recv.window.highest_id_received = 1;
        recv.local_time_last_got_data = now;
        // The first acknowledgement goes after a full interval, not with the first data.
        recv.time_stamp = now;
        Self {
            role,
            peer,
            recv,
            flow: FlowQueue::new(own_id, iteration),
            indicator: Indicator::new(),
            own_id,
            flush_num: 0,
            game_time,
            clock_base: now,
            held: false,
            last_time_sync: None,
            ack_now: false,
            peer_acked: 1,
            last_message_seq: 1,
            unsent_blobs: 0,
            pace: Pace::default(),
            stats: Stats::default(),
        }
    }

    /// The server's game clock now (server), or the last one received (client).
    #[must_use]
    pub fn game_time(&self, now: LocalTime) -> f64 {
        match self.role {
            Role::Server if self.held => self.game_time,
            Role::Server => self.game_time + now.seconds_since(self.clock_base),
            Role::Client => self.game_time,
        }
    }

    /// Server: hold the game clock where it stands at `now`; it reads the same from then on.
    pub fn hold_clock(&mut self, now: LocalTime) {
        self.game_time = self.game_time(now);
        self.clock_base = now;
        self.held = true;
    }

    /// When the peer was last heard from.
    #[must_use]
    pub fn last_heard(&self) -> LocalTime {
        self.recv.local_time_last_got_data
    }

    /// Take in one parsed packet from the peer.
    ///
    /// # Errors
    /// The receive window refused it (duplicate, bad checksum, too far ahead).
    pub fn feed(
        &mut self,
        p: &ParsedPacket,
        len: usize,
        now: LocalTime,
    ) -> Result<Incoming, RejectReason> {
        let r = self.recv.accept(&p.header, |key| p.checksum_ok(key));
        if let Err(e) = r {
            self.stats.rejected += 1;
            return Err(e);
        }
        self.stats.packets_in += 1;
        self.stats.bytes_in += len as u64;
        if let Some(flow) = self
            .recv
            .account_datagram(p.header.interval, u32::try_from(len).unwrap_or(u32::MAX))
        {
            let _ = self
                .flow
                .enqueue_optional_header(PacketFlags::FLOW, flow.to_vec());
        }
        self.recv.local_time_last_got_data = now;

        let mut inc = Incoming::default();
        let h = p.header.header;
        if h.contains(PacketFlags::NET_ERROR_DISCONNECT) || h.contains(PacketFlags::DISCONNECT) {
            inc.disconnect = true;
        }
        if let Some(b) = p
            .optional
            .get(&PacketFlags::TIME_SYNC)
            .filter(|b| b.len() >= 8)
        {
            let t = f64::from_le_bytes([b[0], b[1], b[2], b[3], b[4], b[5], b[6], b[7]]);
            inc.time_sync = Some(t);
            if self.role == Role::Client {
                self.game_time = t;
            }
        }
        if self.role == Role::Server {
            if let Some(b) = p
                .optional
                .get(&PacketFlags::ECHO_REQUEST)
                .filter(|b| b.len() >= 4)
            {
                let mut body = b[..4].to_vec();
                body.extend_from_slice(&0f32.to_le_bytes());
                let _ = self
                    .flow
                    .enqueue_optional_header(PacketFlags::ECHO_RESPONSE, body);
                self.ack_now = true;
            }
        }
        if let Some(b) = p.optional.get(&PacketFlags::REQUEST_RETRANSMIT) {
            let ids = seq_ids(b);
            if let Some(first) = self.flow.enqueue_acks(&ids) {
                if first != self.flush_num && lhs_newer(first, self.flush_num) {
                    self.flush_num = first;
                }
                let has = first.wrapping_sub(1);
                if lhs_newer(has, self.peer_acked) {
                    self.peer_acked = has;
                }
            }
        }
        if let Some(b) = p.optional.get(&PacketFlags::REJECT_RETRANSMIT) {
            self.recv.handle_empty_ack(&seq_ids(b));
        }
        if let Some(b) = p
            .optional
            .get(&PacketFlags::ACK_SEQUENCE)
            .filter(|b| b.len() >= 4)
        {
            let flush = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
            if flush != 0 && flush != self.flush_num && lhs_newer(flush, self.flush_num) {
                self.flush_num = flush;
            }
            if lhs_newer(flush, self.peer_acked) {
                self.peer_acked = flush;
            }
        }
        if !p.fragments.is_empty() {
            inc.blobs = self
                .indicator
                .check_in_packet(&p.fragments, p.header.rec_id, now);
            self.stats.blobs_in += inc.blobs.len() as u64;
        }
        if let Some(b) = p.optional.get(&PacketFlags::SERVER_SWITCH) {
            inc.server_switch = ServerSwitch::from_bytes(b).ok().flatten();
        }
        if let Some(b) = p.optional.get(&PacketFlags::REFERRAL) {
            inc.referral = Referral::from_bytes(b).ok();
        }
        Ok(inc)
    }

    /// Queue one message with the blob id it carries. A server's large message goes by its
    /// [`Pace`].
    pub fn send_blob(&mut self, queue: u16, blob_id: u64, payload: Vec<u8>) {
        let paced = self.role == Role::Server && self.pace.takes(queue, blob_id, payload.len());
        let mut b = NetBlob::for_send(payload, queue);
        b.id = NetBlobId(blob_id);
        b.saved_net_blob_id = b.id;
        if paced {
            self.pace.push(&b);
        } else {
            self.flow.enqueue_blob(b);
            self.unsent_blobs += 1;
        }
        self.stats.blobs_out += 1;
    }

    /// Queue an optional header (a referral, a server switch...).
    pub fn send_section(&mut self, mask: u32, data: Vec<u8>) {
        let _ = self.flow.enqueue_optional_header(mask, data);
    }

    /// Send what is queued now, without the acknowledgement and clock sections a full
    /// [`Self::tick`] adds: a server switch goes alone in a world connection's first packet.
    pub fn flush_alone(&mut self, now: LocalTime) -> Vec<Vec<u8>> {
        let mut crypto = std::mem::replace(&mut self.recv.crypto_outgoing, CryptoSystem::new(0));
        let out = self.flow.transmit_new_packets(&mut crypto, now);
        self.recv.crypto_outgoing = crypto;
        self.stats.packets_out += out.len() as u64;
        out
    }

    /// Run the connection for this moment: acknowledgements or retransmit requests, the clock,
    /// retransmissions, then everything queued, then what the pace lets go of a server's large
    /// messages. Returns the datagrams for [`Self::peer`].
    pub fn tick(&mut self, now: LocalTime) -> Vec<Vec<u8>> {
        self.indicator.flush_timed_out_eph_info(now);
        let mut sync_due = false;
        for ev in self.flow.advance_interval(now) {
            if ev == IntervalEvent::TimeSyncAndEcho {
                sync_due = true;
            }
        }
        self.pace.at_interval(self.flow.cur_local_interval);
        let mut sections: Vec<(u32, Vec<u8>)> = Vec::new();
        let since = now.seconds_since(self.recv.time_stamp);
        let naks = self.recv.get_naks();
        if naks.is_empty() {
            if since >= ACK_INTERVAL || self.ack_now {
                sections.push((
                    PacketFlags::ACK_SEQUENCE,
                    self.recv.window.highest_id_received.to_le_bytes().to_vec(),
                ));
                self.recv.time_stamp = now;
                self.ack_now = false;
            }
        } else if since > NAK_INTERVAL {
            let mut block = Vec::with_capacity(4 + 4 * naks.len());
            block.extend_from_slice(&u32::try_from(naks.len()).unwrap_or(0).to_le_bytes());
            for id in &naks {
                block.extend_from_slice(&id.to_le_bytes());
            }
            self.stats.naks_sent += naks.len() as u64;
            sections.push((PacketFlags::REQUEST_RETRANSMIT, block));
            self.recv.time_stamp = now;
        }
        match self.role {
            Role::Client if sync_due => {
                sections.push((PacketFlags::TIME_SYNC, 0f64.to_le_bytes().to_vec()));
                sections.push((PacketFlags::ECHO_REQUEST, 0f32.to_le_bytes().to_vec()));
            }
            Role::Server
                if self
                    .last_time_sync
                    .is_none_or(|t| now.seconds_since(t) >= SERVER_TIME_SYNC_INTERVAL) =>
            {
                sections.push((PacketFlags::TIME_SYNC, 0f64.to_le_bytes().to_vec()));
                self.last_time_sync = Some(now);
            }
            _ => {}
        }
        if !sections.is_empty() {
            let _ = self.flow.enqueue_optional_headers(&sections);
        }
        #[allow(clippy::cast_possible_truncation)]
        self.flow.set_time_sensitive(TimeSensitive {
            game_time: self.game_time(now),
            local_time: now.0 as f32,
        });
        if let Some(block) = self.flow.compile_empty_acks() {
            let _ = self
                .flow
                .enqueue_optional_header(PacketFlags::REJECT_RETRANSMIT, block);
        }
        let mut out = self.flow.transmit_acks();
        out.truncate(MAX_RESENDS_PER_TICK);
        self.stats.retransmitted += out.len() as u64;
        let mut crypto = std::mem::replace(&mut self.recv.crypto_outgoing, CryptoSystem::new(0));
        let new = self.flow.transmit_new_packets(&mut crypto, now);
        self.unsent_blobs = 0;
        for dg in &new {
            if let Ok(h) = ProtoHeader::from_bytes(dg) {
                if h.header.has_fragments() {
                    self.last_message_seq = h.seq_id;
                }
            }
        }
        out.extend(new);
        self.pace.spend(out.iter().map(Vec::len).sum());
        if !self.pace.is_empty() {
            let paced = self
                .pace
                .send(&mut self.flow, &mut crypto, self.peer_acked, now);
            if let Some(last) = paced.last().and_then(|d| ProtoHeader::from_bytes(d).ok()) {
                self.last_message_seq = last.seq_id;
            }
            out.extend(paced);
        }
        self.recv.crypto_outgoing = crypto;
        if self.flush_num != 0 {
            self.flow.flush_sent_packets(self.flush_num);
            self.flush_num = 0;
        }
        self.stats.packets_out += out.len() as u64;
        self.stats.bytes_out += out.iter().map(|d| d.len() as u64).sum::<u64>();
        out
    }

    /// Has every message this end queued gone out, and the peer acknowledged the packets that
    /// carried them?
    #[must_use]
    pub fn messages_acknowledged(&self) -> bool {
        self.unsent_blobs == 0
            && self.pace.is_empty()
            && !lhs_newer(self.last_message_seq, self.peer_acked)
    }

    /// The goodbye this end sends when it goes away: unsequenced, alone in its packet.
    #[must_use]
    pub fn disconnect_packet(&self) -> Vec<u8> {
        standalone(
            self.own_id,
            self.recv.iteration,
            PacketFlags::DISCONNECT,
            Vec::new(),
        )
    }
}

/// An unsequenced packet carrying one optional header (the handshakes, a goodbye).
#[must_use]
pub fn standalone(rec_id: u16, iteration: u16, mask: u32, body: Vec<u8>) -> Vec<u8> {
    let mut p = OutPacket::new(ProtoHeader {
        seq_id: 0,
        rec_id,
        iteration,
        ..Default::default()
    });
    let _ = p.add_optional_header(mask, body);
    p.serialize(None).unwrap_or_default()
}
