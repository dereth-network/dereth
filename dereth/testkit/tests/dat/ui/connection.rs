//! UI fixtures and scenarios for connection.

use super::*;
// =============================================================================================
// hud.link-lamp.* -- the lamp that always showed red
//
// The state before the shard sets the clock, and the same claim over a longer silence, are legs
// of the first scenario here rather than rows of their own.
// =============================================================================================

/// The lamp, and the four pictures it can be in.
const LINK_LAMP: ElementId = ElementId(0x1000_00F8);

/// A shard on the far end of the client's own socket-free endpoint, sending real datagrams.
struct LampShard {
    crypto: dereth_transport::CryptoSystem,
    sequence: u32,
    from: std::net::SocketAddr,
}

impl LampShard {
    fn attach(c: &mut HeadlessClient) -> Self {
        let net = dereth_client::net::ClientNetwork::new(
            "127.0.0.1:19000",
            7304,
            "acct0001",
            "unused",
            0,
        )
        .expect("a socket-free endpoint");
        c.attach_replay(net);
        let mut shard = Self {
            crypto: dereth_transport::CryptoSystem::new(0xDEAD_BEEF),
            sequence: 1,
            from: "127.0.0.1:19000".parse().expect("the peer address"),
        };
        shard.connect_request(c);
        // The first datagram that is not the opening answer finishes the handshake.
        shard.echo_request(c);
        assert_eq!(
            c.replay_net_mut().expect("the endpoint").status(),
            dereth_client::net::LinkStatus::Connected,
            "the scenario is worthless unless the link is genuinely up"
        );
        shard
    }

    fn send(&self, c: &mut HeadlessClient, raw: Vec<u8>) {
        let now = dereth_primitives::LocalTime(c.view().expect_app().clock().local_time);
        let from = self.from;
        c.replay_net_mut()
            .expect("the endpoint")
            .feed(&raw, from, now);
    }

    fn connect_request(&mut self, c: &mut HeadlessClient) {
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 0,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::CONNECT_REQUEST,
                dereth_transport::conn::ConnectRequest {
                    server_time: 0.0,
                    cookie: 0,
                    net_id: 0,
                    outgoing_seed: 0xDEAD_BEEF,
                    incoming_seed: 0x1234_5678,
                }
                .to_bytes()
                .to_vec(),
            )
            .expect("one optional header");
        self.send(c, packet.serialize(None).expect("a serialisable packet"));
    }

    fn echo_request(&mut self, c: &mut HeadlessClient) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::ECHO_REQUEST,
                0f32.to_le_bytes().to_vec(),
            )
            .expect("one optional header");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a serialisable packet");
        self.send(c, raw);
    }

    /// The shard setting the game clock, which every shard does within seconds of a login.
    fn set_the_clock(&mut self, c: &mut HeadlessClient, t: f64) {
        self.sequence += 1;
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: self.sequence,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::TIME_SYNC,
                t.to_le_bytes().to_vec(),
            )
            .expect("one optional header");
        let raw = packet
            .serialize(Some(self.crypto.next()))
            .expect("a serialisable packet");
        self.send(c, raw);
    }

    /// A real failure of the link, as the shard reports one.
    fn fail(&mut self, c: &mut HeadlessClient) {
        let mut packet = dereth_transport::OutPacket::new(dereth_transport::ProtoHeader {
            seq_id: 0,
            rec_id: 0xB,
            interval: 0x100,
            iteration: 1,
            ..Default::default()
        });
        packet
            .add_optional_header(
                dereth_transport::wire::PacketFlags::NET_ERROR,
                dereth_transport::conn::NetErrorCode::ServerFull
                    .pack()
                    .to_vec(),
            )
            .expect("one optional header");
        self.send(c, packet.serialize(None).expect("a serialisable packet"));
    }
}

/// The picture the lamp is really in.
fn lamp_state(c: &mut HeadlessClient) -> u32 {
    let ui = &mut c.app_mut().ui_mut().expect("the UI shell is up").ui;
    let root = ui
        .get_element(ElementId(0x1000_0495))
        .expect("the gameplay root");
    let h = ui
        .get_child_recursive(root, LINK_LAMP)
        .expect("the lamp is in the tree");
    ui.node(h).expect("live").state.0
}

/// Run frames until the lamp has settled, advancing the link's own clock as the socket loop does.
fn settle_the_lamp(c: &mut HeadlessClient, frames: u32, step: f64) -> u32 {
    let mut network_now = c.view().expect_app().clock().local_time;
    for _ in 0..frames {
        network_now += step;
        c.replay_net_mut()
            .expect("the endpoint")
            .receive_use_time(dereth_primitives::LocalTime(network_now));
        c.tick(1);
    }
    lamp_state(c)
}

/// The number of ticks a shard's clock carries, which is what made the lamp read a silence of
/// nine hundred million seconds.
const A_SHARDS_CLOCK: f64 = 1_073_741_828.0;

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link
// ---------------------------------------------------------------------------------------------

/// The lamp does not turn red when the shard sets the clock, nor when the link is merely quiet.
pub(super) fn setting_the_clock_does_not_change_the_lamp() {
    use dereth_ui_screens::hud::indicators::link_status::media;

    // Before the shard sets the clock. This is the state the rest is compared against: without
    // it a good lamp afterwards could be one that was never moved at all.
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let _shard = LampShard::attach(&mut c);
    let no_correction_yet = c.view().expect_app().clock().external_offset() == 0.0;
    let good_before = settle_the_lamp(&mut c, 200, 0.1) == media::GOOD;
    c.shutdown();

    // And with it set, which is what once reddened the lamp.
    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut shard = LampShard::attach(&mut c);
    shard.set_the_clock(&mut c, A_SHARDS_CLOCK);
    c.tick(1);
    // The scenario is worthless unless the shard really moved the clock.
    let the_clock_moved = c.view().expect_app().clock().external_offset() > 40.0;
    let still_good = settle_the_lamp(&mut c, 400, 0.1) == media::GOOD;

    // And a link that is up but quiet for the best part of a minute is still a link: silence
    // alone is not the same thing as a link that has gone.
    let quiet_is_still_good = settle_the_lamp(&mut c, 1500, 0.04) == media::GOOD;

    c.assert_behaviour(
        "hud.link-lamp.the-shard-setting-the-clock-does-not-change-it-and-nor-does-a-quiet-link",
        move |_| {
            no_correction_yet && good_before && the_clock_moved && still_good && quiet_is_still_good
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp
// ---------------------------------------------------------------------------------------------

/// A link that really fails does not leave a red lamp on a world the player is still in.
pub(super) fn a_real_failure_takes_the_player_off_the_world() {
    use dereth_ui_screens::hud::indicators::link_status::media;

    let mut c = HeadlessClient::new(ClientSpec::gameplay_in_world(6));
    let mut shard = LampShard::attach(&mut c);
    let good_first = settle_the_lamp(&mut c, 200, 0.1) == media::GOOD;
    // The change is measured, so "the link is down" cannot be read off a link that was never up.
    let up = c.replay_net_mut().expect("the endpoint").status()
        == dereth_client::net::LinkStatus::Connected;

    shard.fail(&mut c);
    c.tick(1);

    let down = {
        let net = c.replay_net_mut().expect("the endpoint");
        net.status() == dereth_client::net::LinkStatus::Disconnected && net.error().is_none()
    } && c.replay_net_mut().expect("the endpoint").session_state()
        == dereth_client_net::client_session::SessionState::Disconnected(
            dereth_client_net::client_session::DisconnectReason::ServerDied,
        );
    let holder_cleared = {
        let now = c.view().expect_app().clock().cur_time;
        dereth_client::net::link_status_holder::connection_status(now).is_none()
    };
    // And the world is gone with it: there is no lamp left to be red.
    let off_the_world = {
        let shell = c
            .app_mut()
            .ui_mut()
            .expect("the shell stays up to show the refusal");
        let screen = shell.flow.current_mut().expect("some screen is up");
        let any: &mut dyn std::any::Any = &mut **screen;
        any.downcast_mut::<dereth_ui_screens::screens::gameplay::GamePlayScreen>()
            .is_none()
    };

    c.assert_behaviour(
        "hud.link-lamp.a-real-failure-puts-the-player-off-the-world-rather-than-reddening-a-lamp",
        move |_| good_first && up && down && holder_cleared && off_the_world,
    );
    c.shutdown();
}
