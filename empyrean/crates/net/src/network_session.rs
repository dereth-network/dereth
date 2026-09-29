// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Network/NetworkSession.cs

use std::collections::{BTreeMap, HashMap, VecDeque};

use dereth_transport::conn::NetErrorCode;
use dereth_transport::wire::PacketFlags;
use empyrean_common::cryptography::crypto_system::CryptoSystem;
use empyrean_common::dotnet::datetime::{TimeSpan, TICKS_PER_MILLISECOND, TICKS_PER_SECOND};
use empyrean_common::dotnet::{CsCast, DotNetDateTime};

use crate::client_message::ClientMessage;
use crate::client_packet::{ClientPacket, ClientPacketFragment};
use crate::enums::{SessionState, SessionTerminationReason};
use crate::game_message_group::GameMessageGroup;
use crate::handlers::authentication_handler;
use crate::message_buffer::MessageBuffer;
use crate::message_fragment::MessageFragment;
use crate::network_bundle::NetworkBundle;
use crate::packets::packet_reject_retransmit::packet_reject_retransmit;
use crate::server_packet::{ServerPacket, MAX_PACKET_SIZE};
use crate::session::{NetIo, SessionCore};
use crate::session_connection_data::{PacketSequence, SessionConnectionData, SessionRandom};
use crate::{Event, OutboundMessage, Outgoing, PortKind};

/// ACE `minimumTimeBetweenBundles` (5 ms): after a bundle goes out, no other bundle of the session
/// goes out for this long.
pub const MINIMUM_TIME_BETWEEN_BUNDLES: TimeSpan = TimeSpan::from_ticks(5 * TICKS_PER_MILLISECOND);
/// ACE `timeBetweenTimeSync` (20 s).
pub const TIME_BETWEEN_TIME_SYNC: TimeSpan = TimeSpan::from_ticks(20_000 * TICKS_PER_MILLISECOND);
/// Not ACE's (retail's): the retail server re-sent its `ConnectRequest`
/// every second until the `ConnectResponse` arrived.
pub const TIME_BETWEEN_CONNECT_REQUESTS: TimeSpan = TimeSpan::from_ticks(TICKS_PER_SECOND);
/// ACE `timeBetweenAck` (2 s).
pub const TIME_BETWEEN_ACK: TimeSpan = TimeSpan::from_ticks(2_000 * TICKS_PER_MILLISECOND);
/// ACE `cachedPacketPruneInterval` (5 s).
pub const CACHED_PACKET_PRUNE_INTERVAL: TimeSpan = TimeSpan::from_ticks(5 * TICKS_PER_SECOND);
/// ACE `cachedPacketRetentionTime`: seconds of `PortalYearTicks` a sent packet is kept for resend.
pub const CACHED_PACKET_RETENTION_TIME: i32 = 120;
/// ACE `MaxNumNakSeqIds`.
pub const MAX_NUM_NAK_SEQ_IDS: u32 = 115;
/// The least time between two of our retransmit requests (ACE `new TimeSpan(0, 0, 1)`).
pub const REQUEST_FOR_RETRANSMIT_INTERVAL: TimeSpan = TimeSpan::from_ticks(TICKS_PER_SECOND);
/// ACE `EchoLogInterval`.
pub const ECHO_LOG_INTERVAL: i32 = 5;
/// ACE `EchoInterval`.
pub const ECHO_INTERVAL: i32 = 10;
/// ACE `EchoThreshold` (a `float`).
pub const ECHO_THRESHOLD: f32 = 2.0;
/// ACE `DiffThreshold` (a `float`, compared after widening to `double`).
pub const DIFF_THRESHOLD: f32 = 0.01;

/// ACE's `(ushort)Timers.PortalYearTicks`: the C# narrowing from `double` ([`CsCast`]), which
/// truncates toward zero and keeps the low 16 bits (the value wraps every 65536 s, as ACE's
/// comment in `PruneCachedPackets` expects).
#[must_use]
pub fn portal_year_ticks_u16(portal_year_ticks: f64) -> u16 {
    portal_year_ticks.cs_cast()
}

/// Whether a cached packet stamped at `stamped` (the 16-bit portal-year time) is still kept at
/// `current`: at most [`CACHED_PACKET_RETENTION_TIME`] old, across the 16-bit wrap.
///
/// Not ACE's (a fix): the wrap adds 65536, one full turn of the 16-bit
/// time; ACE added `ushort.MaxValue` (65535), so across the wrap a packet was kept one tick longer.
#[must_use]
pub fn cached_packet_is_kept(current: u16, stamped: u16) -> bool {
    let (current, stamped) = (i32::from(current), i32::from(stamped));
    let current = if current >= stamped {
        current
    } else {
        current + 0x1_0000
    };
    current - stamped <= CACHED_PACKET_RETENTION_TIME
}

/// ACE `NetworkSession`.
#[derive(Debug)]
pub struct NetworkSession {
    current_bundles: Vec<NetworkBundle>,
    out_of_order_packets: HashMap<u32, ClientPacket>,
    partial_fragments: HashMap<u32, MessageBuffer>,
    /// ACE `outOfOrderFragments`, with the fragment queue kept for [`Event::Message`].
    out_of_order_fragments: HashMap<u32, (u16, ClientMessage)>,
    next_send: DotNetDateTime,
    /// Set when the `ConnectResponse` arrives: TimeSync is sent then and every 20 s after.
    pub send_resync: bool,
    next_resync: DotNetDateTime,
    send_ack: bool,
    next_ack: DotNetDateTime,
    last_received_packet_sequence: u32,
    last_received_fragment_sequence: u32,
    cached_packets: BTreeMap<u32, ServerPacket>,
    /// ACE's `default(DateTime)` at first, so the first `Update` prunes.
    last_cached_packet_prune_time: DotNetDateTime,
    packet_queue: VecDeque<ServerPacket>,
    pub connection_data: SessionConnectionData,
    /// ACE `TimeoutTick`: when an active session times out.
    pub timeout_tick: DotNetDateTime,
    pub client_id: u16,
    pub server_id: u16,
    /// `DateTime.MinValue` at first.
    last_request_for_retransmit_time: DotNetDateTime,
    last_client_time: f32,
    last_server_time: DotNetDateTime,
    last_diff: f64,
    echo_diff: i32,
    is_released: bool,
    /// Not ACE's: the `ConnectRequest` exactly as sent, kept (seeds and all) until the
    /// `ConnectResponse` completes the handshake, so it can be re-sent on the retail timer.
    sent_connect_request: Option<Vec<u8>>,
    /// Not ACE's: when the kept `ConnectRequest` is next re-sent.
    next_connect_request_resend: DotNetDateTime,
    /// Whether a refusal before the `ConnectRequest` has gone out (see [`Self::flush_packets`]).
    pre_connection_refusal_sent: bool,
}

impl NetworkSession {
    // ACE: NetworkSession.NetworkSession
    #[must_use]
    pub fn new(
        client_id: u16,
        server_id: u16,
        utc_now: DotNetDateTime,
        rand: &mut SessionRandom,
    ) -> Self {
        Self {
            current_bundles: vec![NetworkBundle::default(); GameMessageGroup::QUEUE_MAX],
            out_of_order_packets: HashMap::new(),
            partial_fragments: HashMap::new(),
            out_of_order_fragments: HashMap::new(),
            next_send: utc_now,
            send_resync: false,
            next_resync: utc_now,
            // "Ack should be sent after a 2 second delay, so start enabled with the delay."
            send_ack: true,
            next_ack: utc_now + TIME_BETWEEN_ACK,
            last_received_packet_sequence: 1,
            last_received_fragment_sequence: 0,
            cached_packets: BTreeMap::new(),
            last_cached_packet_prune_time: DotNetDateTime::MIN_VALUE,
            packet_queue: VecDeque::new(),
            connection_data: SessionConnectionData::new(rand),
            // "New network auth session timeouts will always be low."
            timeout_tick: utc_now
                .add_seconds(f64::from(authentication_handler::DEFAULT_AUTH_TIMEOUT)),
            client_id,
            server_id,
            last_request_for_retransmit_time: DotNetDateTime::MIN_VALUE,
            last_client_time: 0.0,
            last_server_time: DotNetDateTime::MIN_VALUE,
            last_diff: 0.0,
            echo_diff: 0,
            is_released: false,
            sent_connect_request: None,
            next_connect_request_resend: DotNetDateTime::MIN_VALUE,
            pre_connection_refusal_sent: false,
        }
    }

    /// The last in-order packet sequence processed (what `AckSequence` reports).
    #[must_use]
    pub const fn last_received_packet_sequence(&self) -> u32 {
        self.last_received_packet_sequence
    }

    /// The sequences of the sent packets held for retransmission, ascending.
    #[must_use]
    pub fn cached_sequences(&self) -> Vec<u32> {
        self.cached_packets.keys().copied().collect()
    }

    // ACE: NetworkSession.EnqueueSend
    /// The `GameMessage` overloads: the message joins its group's bundle.
    pub fn enqueue_send(&mut self, message: OutboundMessage) {
        if self.is_released {
            return;
        }
        let bundle = &mut self.current_bundles[message.group.index()];
        bundle.encrypted_checksum = true;
        bundle.enqueue(message);
    }

    /// The `ServerPacket` overload of ACE `EnqueueSend`: the packet goes straight to the flush
    /// queue.
    pub fn enqueue_send_packet(&mut self, packet: ServerPacket) {
        if self.is_released {
            return;
        }
        self.packet_queue.push_back(packet);
    }

    // ACE: NetworkSession.Update
    /// Prunes the resend cache, sends at most one due bundle per 5 ms, and flushes every queued
    /// packet.
    pub fn update(&mut self, core: &SessionCore, io: &mut NetIo<'_>) {
        if self.is_released {
            return;
        }
        let now = io.now.utc;

        if now - self.last_cached_packet_prune_time > CACHED_PACKET_PRUNE_INTERVAL {
            self.prune_cached_packets(io);
        }

        for (i, group) in GameMessageGroup::ALL.into_iter().enumerate() {
            let mut bundle_to_send = None;
            {
                let current_bundle = &mut self.current_bundles[i];
                if group == GameMessageGroup::InvalidQueue {
                    if self.send_resync && !current_bundle.time_sync() && now > self.next_resync {
                        current_bundle.set_time_sync(true);
                        current_bundle.encrypted_checksum = true;
                        self.next_resync = now + TIME_BETWEEN_TIME_SYNC;
                    }
                    if self.send_ack && !current_bundle.send_ack() && now > self.next_ack {
                        current_bundle.set_send_ack(true);
                        self.next_ack = now + TIME_BETWEEN_ACK;
                    }
                }
                if current_bundle.needs_sending() && now >= self.next_send {
                    // "Swap out bundle so we can process it"
                    bundle_to_send = Some(std::mem::take(current_bundle));
                }
            }
            if let Some(bundle) = bundle_to_send {
                self.send_bundle(bundle, group, core, io);
                self.next_send = now + MINIMUM_TIME_BETWEEN_BUNDLES;
            }
        }

        self.flush_packets(core, io);
        self.resend_connect_request_if_due(core, io);
    }

    // ACE: NetworkSession.PruneCachedPackets
    fn prune_cached_packets(&mut self, io: &NetIo<'_>) {
        self.last_cached_packet_prune_time = io.now.utc;
        let current_time = portal_year_ticks_u16(io.now.portal_year_ticks);
        self.cached_packets
            .retain(|_, p| cached_packet_is_kept(current_time, p.header.interval));
    }

    // ACE: NetworkSession.ProcessPacket
    /// Verifies, half-processes, reorders and then handles one client packet.
    pub fn process_packet(
        &mut self,
        packet: ClientPacket,
        core: &mut SessionCore,
        io: &mut NetIo<'_>,
    ) {
        if self.is_released {
            return;
        }
        io.stats.c2s_packets_aggregate_increment();

        if !packet.verify_crc(&mut self.connection_data.crypto_client, io.stats) {
            return;
        }

        // "If the client sent a NAK with a cleartext CRC then process it"
        if packet.has_flag(PacketFlags::REQUEST_RETRANSMIT)
            && !packet.has_flag(PacketFlags::ENCRYPTED_CHECKSUM)
        {
            let mut uncached: Option<Vec<u32>> = None;
            for &sequence in packet
                .header_optional
                .retransmit_data
                .as_deref()
                .unwrap_or(&[])
            {
                if !self.retransmit(sequence, core, io) {
                    uncached.get_or_insert_with(Vec::new).push(sequence);
                }
            }
            if let Some(uncached) = uncached {
                // "Sends a response packet w/ PacketHeader.RejectRetransmit"
                self.enqueue_send_packet(packet_reject_retransmit(&uncached));
            }
            io.stats.c2s_requests_for_retransmit_aggregate_increment();
            // "cleartext crc NAK is never accompanied by additional data needed by the rest of the
            // pipeline"
            return;
        }

        // Order-insensitive "half-processing".
        if packet.has_flag(PacketFlags::DISCONNECT) {
            core.terminate(
                SessionTerminationReason::PacketHeaderDisconnect,
                String::new(),
                io.now.utc,
            );
            return;
        }
        if packet.has_flag(PacketFlags::NET_ERROR_DISCONNECT) {
            core.terminate(
                SessionTerminationReason::ClientSentNetworkErrorDisconnect,
                String::new(),
                io.now.utc,
            );
            return;
        }

        // Sessions still at the login request keep the short auth timeout (15 s); later ones get the
        // configured session timeout (60 s by default).
        self.timeout_tick = if core.state == SessionState::AuthLoginRequest {
            io.now
                .utc
                .add_seconds(f64::from(authentication_handler::DEFAULT_AUTH_TIMEOUT))
        } else {
            io.now
                .utc
                .add_seconds(f64::from(io.default_session_timeout))
        };

        // Reordering stage. Sequence 0 (the handshake) is always processed, and so is an ACK-only
        // packet that repeats the last sequence.
        let sequence = packet.header.seq_id;
        if sequence <= self.last_received_packet_sequence
            && sequence != 0
            && !(packet.flags() == PacketFlags::ACK_SEQUENCE
                && sequence == self.last_received_packet_sequence)
        {
            log::warn!(
                "[{}] Packet {sequence} received again",
                core.logging_identifier
            );
            return;
        }

        // A packet from the future is kept until the gap is filled.
        let desired_seq = self.last_received_packet_sequence.wrapping_add(1);
        if sequence > desired_seq {
            self.out_of_order_packets.entry(sequence).or_insert(packet);
            if desired_seq.wrapping_add(2) <= sequence
                && io.now.utc - self.last_request_for_retransmit_time
                    > REQUEST_FOR_RETRANSMIT_INTERVAL
            {
                self.do_request_for_retransmission(sequence, core, io);
            }
            return;
        }

        // Final processing stage.
        self.handle_ordered_packet(packet, core, io);
        self.check_out_of_order_packets(core, io);
        self.check_out_of_order_fragments(core, io);
    }

    // ACE: NetworkSession.DoRequestForRetransmission
    /// Asks the client to resend the gap below `rcvd_seq`, at most 115 sequences.
    fn do_request_for_retransmission(
        &mut self,
        rcvd_seq: u32,
        core: &mut SessionCore,
        io: &mut NetIo<'_>,
    ) {
        let desired_seq = self.last_received_packet_sequence.wrapping_add(1);
        let mut need_seq = vec![desired_seq];
        let bottom = desired_seq.wrapping_add(1);
        if rcvd_seq < bottom
            || i64::from(rcvd_seq - bottom) > i64::from(CryptoSystem::MAXIMUM_EFFORT_LEVEL)
        {
            core.terminate(
                SessionTerminationReason::AbnormalSequenceReceived,
                String::new(),
                io.now.utc,
            );
            return;
        }
        let mut seq_id_count = 1u32;
        for a in bottom..rcvd_seq {
            if !self.out_of_order_packets.contains_key(&a) {
                need_seq.push(a);
                seq_id_count += 1;
                if seq_id_count >= MAX_NUM_NAK_SEQ_IDS {
                    break;
                }
            }
        }

        let mut req_packet = ServerPacket::new();
        let data = req_packet.initialize_data_writer();
        data.extend_from_slice(
            &u32::try_from(need_seq.len())
                .unwrap_or(u32::MAX)
                .to_le_bytes(),
        );
        for k in &need_seq {
            data.extend_from_slice(&k.to_le_bytes());
        }
        req_packet.header.header = PacketFlags(PacketFlags::REQUEST_RETRANSMIT);
        self.enqueue_send_packet(req_packet);

        self.last_request_for_retransmit_time = io.now.utc;
        io.stats.s2c_requests_for_retransmit_aggregate_increment();
    }

    // ACE: NetworkSession.HandleOrderedPacket
    /// A verified, in-order packet: its headers, then its fragments.
    fn handle_ordered_packet(
        &mut self,
        packet: ClientPacket,
        core: &mut SessionCore,
        io: &mut NetIo<'_>,
    ) {
        if packet.has_flag(PacketFlags::ECHO_REQUEST) {
            self.flag_echo(packet.header_optional.echo_request_client_time);
            self.verify_echo(packet.header_optional.echo_request_client_time, core, io);
        }

        if packet.has_flag(PacketFlags::ACK_SEQUENCE) {
            self.acknowledge_sequence(packet.header_optional.ack_sequence);
        }

        // Incoming TimeSync: ACE reads it and does nothing with it.

        // The first packet of the three-way handshake (LoginRequest, ConnectRequest,
        // ConnectResponse).
        if packet.has_flag(PacketFlags::LOGIN_REQUEST) {
            authentication_handler::handle_login_request(&packet, self, core, io);
            return;
        }

        let sequence = packet.header.seq_id;
        let flags = packet.flags();
        for fragment in packet.fragments {
            self.process_fragment(fragment, core, io);
        }

        if sequence != 0 && flags != PacketFlags::ACK_SEQUENCE {
            self.last_received_packet_sequence = sequence;
        }
    }

    // ACE: NetworkSession.ProcessFragment
    /// Joins split messages and hands complete ones on in fragment-sequence order.
    fn process_fragment(
        &mut self,
        fragment: ClientPacketFragment,
        core: &mut SessionCore,
        io: &mut NetIo<'_>,
    ) {
        let mut message = None;
        let (sequence, queue) = (fragment.sequence, fragment.queue);

        if fragment.count != 1 {
            if let Some(buffer) = self.partial_fragments.get_mut(&sequence) {
                buffer.add_fragment(fragment);
                if buffer.complete() {
                    message = buffer.try_get_message();
                    self.partial_fragments.remove(&sequence);
                }
            } else {
                let mut new_buffer = MessageBuffer::new(sequence, u32::from(fragment.count));
                new_buffer.add_fragment(fragment);
                self.partial_fragments.entry(sequence).or_insert(new_buffer);
            }
        } else if fragment.data.len() >= 4 {
            // "ClientMessage must be a minimum of 4 bytes in length"
            message = ClientMessage::new(fragment.data);
        }

        if let Some(message) = message {
            if sequence == self.last_received_fragment_sequence.wrapping_add(1) {
                self.handle_fragment(queue, message, core, io);
            } else {
                self.out_of_order_fragments
                    .entry(sequence)
                    .or_insert((queue, message));
            }
        }
    }

    // ACE: NetworkSession.HandleFragment
    fn handle_fragment(
        &mut self,
        queue: u16,
        message: ClientMessage,
        core: &SessionCore,
        io: &mut NetIo<'_>,
    ) {
        // `InboundMessageManager.HandleClientMessage` (in the world) is reached through this event.
        io.events.push_back(Event::Message {
            session: core.id,
            queue,
            message,
        });
        self.last_received_fragment_sequence = self.last_received_fragment_sequence.wrapping_add(1);
    }

    // ACE: NetworkSession.CheckOutOfOrderPackets
    fn check_out_of_order_packets(&mut self, core: &mut SessionCore, io: &mut NetIo<'_>) {
        while let Some(packet) = self
            .out_of_order_packets
            .remove(&self.last_received_packet_sequence.wrapping_add(1))
        {
            self.handle_ordered_packet(packet, core, io);
        }
    }

    // ACE: NetworkSession.CheckOutOfOrderFragments
    fn check_out_of_order_fragments(&mut self, core: &mut SessionCore, io: &mut NetIo<'_>) {
        while let Some((queue, message)) = self
            .out_of_order_fragments
            .remove(&self.last_received_fragment_sequence.wrapping_add(1))
        {
            self.handle_fragment(queue, message, core, io);
        }
    }

    // ACE: NetworkSession.VerifyEcho
    /// The speed-hack check: the client's echo clock against ours, measured from the first echo
    /// after the player entered the world.
    fn verify_echo(&mut self, client_time: f32, core: &SessionCore, io: &mut NetIo<'_>) {
        // `session.Player == null || session.logOffRequestTime != DateTime.MinValue`.
        if !core.player_active {
            return;
        }
        let server_time = io.now.utc;

        if self.last_client_time == 0.0 {
            self.last_client_time = client_time;
            self.last_server_time = server_time;
            return;
        }

        let server_time_diff = (server_time - self.last_server_time).total_seconds();
        let client_time_diff = client_time - self.last_client_time;
        let diff = (server_time_diff - f64::from(client_time_diff)).abs();

        if diff > f64::from(ECHO_THRESHOLD) && diff - self.last_diff > f64::from(DIFF_THRESHOLD) {
            self.last_diff = diff;
            self.echo_diff += 1;

            if self.echo_diff >= ECHO_LOG_INTERVAL {
                log::warn!(
                    "{} - TimeSync error: {} (diff: {diff})",
                    core.logging_identifier,
                    self.echo_diff
                );
            }
            if self.echo_diff >= ECHO_INTERVAL {
                log::error!(
                    "{} - disconnected for speedhacking",
                    core.logging_identifier
                );
                // The action chain (system chat, LogOffPlayer, then the reset below) is the
                // world's; it calls `reset_echo_verification` when it runs.
                io.events.push_back(Event::SpeedHack { session: core.id });
            }
        } else if self.echo_diff > 0 {
            if self.echo_diff > ECHO_LOG_INTERVAL {
                log::warn!("{} - Diff: {diff}", core.logging_identifier);
            }
            self.last_diff = 0.0;
            self.echo_diff = 0;
        }
    }

    /// The three resets at the end of ACE's speed-hack action chain in `VerifyEcho`.
    pub fn reset_echo_verification(&mut self) {
        self.echo_diff = 0;
        self.last_diff = 0.0;
        self.last_client_time = 0.0;
    }

    // ACE: NetworkSession.FlagEcho
    fn flag_echo(&mut self, client_time: f32) {
        let current_bundle = &mut self.current_bundles[GameMessageGroup::InvalidQueue.index()];
        current_bundle.set_client_time(client_time);
        current_bundle.encrypted_checksum = true;
    }

    // ACE: NetworkSession.AcknowledgeSequence
    fn acknowledge_sequence(&mut self, sequence: u32) {
        self.cached_packets.retain(|&k, _| k >= sequence);
    }

    // ACE: NetworkSession.Retransmit
    fn retransmit(&mut self, sequence: u32, core: &SessionCore, io: &mut NetIo<'_>) -> bool {
        if let Some(cached_packet) = self.cached_packets.get_mut(&sequence) {
            if !cached_packet.has_flag(PacketFlags::RETRANSMISSION) {
                cached_packet.add_flags(PacketFlags::RETRANSMISSION);
            }
            let bytes = cached_packet.create_ready_to_send_packet();
            push_raw(bytes, core, io);
            return true;
        }
        false
    }

    // ACE: NetworkSession.FlushPackets
    /// Stamps and sends every queued packet, caching the sequenced ones for resend.
    fn flush_packets(&mut self, core: &SessionCore, io: &mut NetIo<'_>) {
        while let Some(mut packet) = self.packet_queue.pop_front() {
            // Not ACE's (a fix): before the ConnectRequest has gone out
            // (the sequence has not started) the client has no connection and no keys, so it drops
            // every encrypted packet, and ACE's boot message went out as one with sequence 0 and was
            // never seen. What a client reads before connecting is an unencrypted, unsequenced
            // packet with a lone NetError header, which ends its login with that error's text. So
            // the session's first encrypted packet is replaced by one such refusal and every other
            // is dropped.
            if packet.has_flag(PacketFlags::ENCRYPTED_CHECKSUM)
                && self.connection_data.packet_sequence.current_value == u32::MAX
            {
                if !self.pre_connection_refusal_sent {
                    self.pre_connection_refusal_sent = true;
                    let reason = core.pending_termination.as_ref().map(|p| p.reason);
                    let mut refusal = pre_connection_refusal(reason);
                    refusal.header.rec_id = self.server_id;
                    refusal.header.iteration = 0x01;
                    refusal.header.interval = portal_year_ticks_u16(io.now.portal_year_ticks);
                    self.send_packet(&mut refusal, core, io);
                }
                continue;
            }
            let cd = &mut self.connection_data;
            // The ConnectRequest took sequence 0; the encrypted packets after it count from 2.
            if packet.has_flag(PacketFlags::ENCRYPTED_CHECKSUM)
                && cd.packet_sequence.current_value == 0
            {
                cd.packet_sequence = PacketSequence::starting_at(1);
            }

            let is_nak = packet.has_flag(PacketFlags::REQUEST_RETRANSMIT);

            // "If we are only ACKing, then we don't seem to have to increment the sequence"
            packet.header.seq_id = if packet.flags() == PacketFlags::ACK_SEQUENCE || is_nak {
                cd.packet_sequence.current_value
            } else {
                cd.packet_sequence.next_value()
            };
            packet.header.rec_id = self.server_id;
            packet.header.iteration = 0x01;
            packet.header.interval = portal_year_ticks_u16(io.now.portal_year_ticks);

            let sequence = packet.header.seq_id;
            self.send_packet(&mut packet, core, io);
            if sequence >= 2 && !is_nak {
                // `cachedPackets.TryAdd`: an existing entry for the sequence is kept.
                self.cached_packets.entry(sequence).or_insert(packet);
            }
        }
    }

    // ACE: NetworkSession.SendPacket
    fn send_packet(&mut self, packet: &mut ServerPacket, core: &SessionCore, io: &mut NetIo<'_>) {
        io.stats.s2c_packets_aggregate_increment();
        if packet.has_flag(PacketFlags::ENCRYPTED_CHECKSUM) {
            let issac_xor = self.connection_data.issac_server.next();
            packet.set_issac_xor(issac_xor);
        }
        let bytes = packet.create_ready_to_send_packet();
        if packet.has_flag(PacketFlags::CONNECT_REQUEST) {
            self.sent_connect_request = Some(bytes.clone());
            self.next_connect_request_resend = io.now.utc + TIME_BETWEEN_CONNECT_REQUESTS;
        }
        push_raw(bytes, core, io);
    }

    /// Not ACE's (retail's): while the handshake waits for the
    /// `ConnectResponse`, the kept `ConnectRequest` is sent again every second, byte for byte
    /// (same cookie, seeds, sequence and time), taking no sequence and no key, so a lost one is
    /// recovered without the client's help. It stops when the `ConnectResponse` discards it, or
    /// when the session ends (the auth timeout bounds it). The client ignores a `ConnectRequest`
    /// for the connection it already has, so a copy it did not need is harmless.
    fn resend_connect_request_if_due(&mut self, core: &SessionCore, io: &mut NetIo<'_>) {
        let waiting = matches!(
            core.state,
            SessionState::AuthLoginRequest | SessionState::AuthConnectResponse
        ) && core.pending_termination.is_none();
        if !waiting || io.now.utc < self.next_connect_request_resend {
            return;
        }
        let Some(bytes) = self.sent_connect_request.clone() else {
            return;
        };
        self.next_connect_request_resend = io.now.utc + TIME_BETWEEN_CONNECT_REQUESTS;
        io.stats.s2c_packets_aggregate_increment();
        push_raw(bytes, core, io);
    }

    /// Not ACE's: the handshake is complete, so the kept `ConnectRequest`, and with it the
    /// last copy of the seeds, is discarded.
    pub fn discard_sent_connect_request(&mut self) {
        self.sent_connect_request = None;
    }

    // ACE: NetworkSession.SendBundle
    /// Turns one bundle into packets: small messages share packets, a large one is cut into
    /// fragments, the optional headers ride the first packet with room for them.
    fn send_bundle(
        &mut self,
        mut bundle: NetworkBundle,
        group: GameMessageGroup,
        core: &SessionCore,
        io: &mut NetIo<'_>,
    ) {
        let _ = core;
        let mut write_optional_headers = true;
        let mut fragments: Vec<MessageFragment> = Vec::new();

        while let Some(message) = bundle.dequeue() {
            let fragment = MessageFragment::new(message, self.connection_data.fragment_sequence);
            self.connection_data.fragment_sequence =
                self.connection_data.fragment_sequence.wrapping_add(1);
            fragments.push(fragment);
        }

        while !fragments.is_empty() || write_optional_headers {
            let mut packet = ServerPacket::new();
            if !fragments.is_empty() {
                packet.add_flags(PacketFlags::BLOB_FRAGMENTS);
            }
            if bundle.encrypted_checksum {
                packet.add_flags(PacketFlags::ENCRYPTED_CHECKSUM);
            }

            let mut available_space = MAX_PACKET_SIZE;

            if let Some(first_message) = fragments.first_mut() {
                if first_message.data_remaining >= available_space {
                    // "If a large message send only this one, filling the whole packet"
                    let Some(spf) = first_message.get_next_fragment() else {
                        log::error!("SendBundle: fragment out of range; bundle abandoned");
                        return;
                    };
                    // ACE also subtracts `spf.Length` from `availableSpace` here; nothing reads it again.
                    packet.fragments.push(spf);
                    if first_message.data_remaining <= 0 {
                        fragments.remove(0);
                    }
                } else {
                    // "Otherwise we'll write any optional headers and process any small messages
                    // that will fit"
                    if write_optional_headers {
                        write_optional_headers = false;
                        self.write_optional_headers(&bundle, &mut packet, io);
                        if let Some(data) = &packet.data {
                            available_space -= i32::try_from(data.len()).unwrap_or(i32::MAX);
                        }
                    }

                    let mut remove_list = Vec::new();
                    for (i, fragment) in fragments.iter_mut().enumerate() {
                        let mut fragment_skipped = false;

                        // "Is this a large fragment and does it have a tail that needs sending?"
                        if !fragment.tail_sent && available_space >= fragment.tail_size() {
                            let Some(spf) = fragment.get_tail_fragment() else {
                                log::error!(
                                    "SendBundle: tail fragment out of range; bundle abandoned"
                                );
                                return;
                            };
                            available_space -= spf.length();
                            packet.fragments.push(spf);
                        }
                        // "Otherwise will this message fit in the remaining space?"
                        else if available_space >= fragment.next_size() {
                            let Some(spf) = fragment.get_next_fragment() else {
                                log::error!("SendBundle: fragment out of range; bundle abandoned");
                                return;
                            };
                            available_space -= spf.length();
                            packet.fragments.push(spf);
                        } else {
                            fragment_skipped = true;
                        }

                        if fragment.data_remaining <= 0 {
                            remove_list.push(i);
                        }

                        // "UIQueue messages must go out in order."
                        if fragment_skipped && group == GameMessageGroup::UIQueue {
                            break;
                        }
                    }
                    for i in remove_list.into_iter().rev() {
                        fragments.remove(i);
                    }
                }
            } else if write_optional_headers {
                // "No messages, just sending optional headers"
                write_optional_headers = false;
                self.write_optional_headers(&bundle, &mut packet, io);
            }
            self.enqueue_send_packet(packet);
        }
    }

    // ACE: NetworkSession.WriteOptionalHeaders
    fn write_optional_headers(
        &self,
        bundle: &NetworkBundle,
        packet: &mut ServerPacket,
        io: &NetIo<'_>,
    ) {
        if bundle.send_ack() {
            packet.add_flags(PacketFlags::ACK_SEQUENCE);
            let last = self.last_received_packet_sequence;
            packet
                .initialize_data_writer()
                .extend_from_slice(&last.to_le_bytes());
        }
        if bundle.time_sync() {
            packet.add_flags(PacketFlags::TIME_SYNC);
            let t = io.now.portal_year_ticks;
            packet
                .initialize_data_writer()
                .extend_from_slice(&t.to_le_bytes());
        }
        #[allow(clippy::float_cmp)]
        if bundle.client_time() != -1.0 {
            packet.add_flags(PacketFlags::ECHO_RESPONSE);
            let client_time = bundle.client_time();
            // `(float)Timers.PortalYearTicks - bundle.ClientTime`, in `float`.
            let holding = CsCast::<f32>::cs_cast(io.now.portal_year_ticks) - client_time;
            let w = packet.initialize_data_writer();
            w.extend_from_slice(&client_time.to_le_bytes());
            w.extend_from_slice(&holding.to_le_bytes());
        }
    }

    /// Whether [`Self::release_resources`] has run.
    #[must_use]
    pub const fn is_released(&self) -> bool {
        self.is_released
    }

    // ACE: NetworkSession.ReleaseResources
    pub fn release_resources(&mut self) {
        self.is_released = true;
        self.current_bundles.clear();
        self.out_of_order_packets.clear();
        self.partial_fragments.clear();
        self.out_of_order_fragments.clear();
        self.cached_packets.clear();
        self.packet_queue.clear();
        // Not ACE's: the client's keys are the shared receive window,
        // whose parked keys are dropped here (ACE releases its `CryptoSystem`).
        self.connection_data.crypto_client.seq_ids_we_naked.clear();
    }
}

/// Not ACE's (a fix): the refusal a client reads before it has a
/// connection. One unencrypted packet with sequence 0 and a lone NetError header (an exclusive
/// header, so nothing else may share the packet); the client ends its login with the error and
/// shows that error's text from its own string table. Only the code travels, so ACE's boot text
/// cannot: a wrong client version reads "You do not have the current version of the client
/// installed.", a shutdown "Server has closed this connection", anything else the generic
/// connection error.
fn pre_connection_refusal(reason: Option<SessionTerminationReason>) -> ServerPacket {
    let code = match reason {
        Some(SessionTerminationReason::ClientVersionIncorrect) => NetErrorCode::NetVersionMismatch,
        Some(SessionTerminationReason::ServerShuttingDown) => NetErrorCode::ServerClosedConnection,
        _ => NetErrorCode::Generic,
    };
    let mut packet = ServerPacket::new();
    packet
        .initialize_data_writer()
        .extend_from_slice(&code.pack());
    packet.header.header = PacketFlags(PacketFlags::NET_ERROR);
    packet
}

// ACE: NetworkSession.SendPacketRaw
/// "On connection to server, client expects response on the connection it initiated, once that
/// occurs, the client connects to the +1 port and then the server transmits on that connection,
/// while the client continues to transmit on the initial port."
fn push_raw(bytes: Vec<u8>, core: &SessionCore, io: &mut NetIo<'_>) {
    let (via_port_kind, to) = match core.end_point_s2c {
        None => (PortKind::C2S, core.end_point_c2s),
        Some(s2c) => (PortKind::S2C, s2c),
    };
    io.outgoing.push(Outgoing {
        to,
        via_port_kind,
        bytes,
        session: core.registered.then_some(core.id),
    });
}
