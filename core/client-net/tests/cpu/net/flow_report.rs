//! The `Flow` report: the bytes the client received in each of the server's half-second intervals,
//! sent back to the server as each interval closes.
//! Fixture: the recorded sessions, whose server datagrams are fed through the transport and whose
//! client datagrams hold the retail client's own reports.

#![cfg(feature = "replay")]

use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;

use dereth_client_net::client_session::testing::session_names;
use dereth_client_net::replay::{Capture, Direction};
use dereth_client_net::{Net, NetConfig, RecipientId};
use dereth_primitives::LocalTime;
use dereth_transport::{PacketFlags, ParsedPacket};

fn captures_dir() -> PathBuf {
    const REL: &str = "fixtures/packet-captures";
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    loop {
        if dir.join(REL).join("index.json").is_file() {
            return dir.join(REL);
        }
        if !dir.pop() {
            return PathBuf::from(REL);
        }
    }
}

/// One report: `(bytes received, the interval it closes)`.
type Report = (u32, u16);

fn report_of(packet: &ParsedPacket) -> Option<Report> {
    let b = packet.optional.get(&PacketFlags::FLOW)?;
    Some((
        u32::from_le_bytes([b[0], b[1], b[2], b[3]]),
        u16::from_le_bytes([b[4], b[5]]),
    ))
}

/// The retail client's reports, in the order it sent them.
fn recorded_reports(capture: &Capture) -> Vec<Report> {
    capture
        .client_datagrams()
        .iter()
        .filter_map(|d| report_of(&ParsedPacket::parse(&d.raw).expect("the capture parses")))
        .collect()
}

/// Feed the recording's server datagrams through a transport set up from its `ConnectRequest`,
/// ticking it between them as a running client would, and collect the reports it sends.
fn our_reports(capture: &Capture) -> Vec<Report> {
    let server: SocketAddr = "127.0.0.1:9000".parse().expect("an address");
    let mut net = Net::new(NetConfig::default());
    let mut reports = Vec::new();
    let mut rec: Option<RecipientId> = None;
    let mut t = 0.0f64;
    let collect = |net: &mut Net, reports: &mut Vec<Report>| {
        for (bytes, _) in net.take_outgoing() {
            if let Ok(p) = ParsedPacket::parse(&bytes) {
                if let Some(r) = report_of(&p) {
                    reports.push(r);
                }
            }
        }
    };
    for dg in &capture.packets {
        // Frames every 20 ms up to this datagram.
        while rec.is_some() && t + 0.02 < dg.t_rel {
            t += 0.02;
            net.tick(LocalTime(t), Duration::ZERO);
            collect(&mut net, &mut reports);
        }
        if dg.dir != Direction::S2c {
            continue;
        }
        let parsed = ParsedPacket::parse(&dg.raw).expect("the capture parses");
        if parsed.header.header.contains(PacketFlags::CONNECT_REQUEST) {
            // The connection the request builds, counted from the request itself.
            let id = parsed.header.rec_id;
            net.add_connection_at(
                id,
                capture.meta.net_id,
                capture.meta.iteration,
                capture.meta.outgoing_seed,
                capture.meta.incoming_seed,
                Some(server),
                LocalTime(dg.t_rel),
            );
            let recv = net
                .receiver_mut(RecipientId(id))
                .expect("the connection was just added");
            recv.local_time_last_got_data = LocalTime(dg.t_rel);
            let _ = recv.account_datagram(
                parsed.header.interval,
                u32::try_from(dg.raw.len()).expect("a datagram length"),
            );
            rec = Some(RecipientId(id));
            t = dg.t_rel;
            continue;
        }
        if rec.is_none() {
            continue;
        }
        let _ = net.feed(&dg.raw, Some(server), LocalTime(dg.t_rel));
        t = t.max(dg.t_rel);
        net.tick(LocalTime(t), Duration::ZERO);
        collect(&mut net, &mut reports);
    }
    reports
}

/// The recordings whose client sends no `Flow` report at all.
const SILENT: [&str; 3] = [
    "long-movement-run",
    "post-relog-attribute-training",
    "pre-relog-play",
];

/// Behaviour: link.flow.each-closed-server-interval-is-reported-with-its-byte-count
///
/// Every `Flow` report the retail client sent in a recorded session -- the bytes received in one
/// of the server's intervals, header included, and that interval's id -- is sent by this
/// transport too, from the server's datagrams alone, and it sends no other: the reports agree
/// report for report. The last interval of a session is never closed, so it is never reported by
/// either.
#[test]
fn every_recorded_flow_report_is_reproduced_from_the_server_datagrams() {
    let root = captures_dir();
    let mut total = 0usize;
    let mut lossy = Vec::new();
    let mut silent = Vec::new();
    for name in session_names() {
        let capture = Capture::load(&root, name).unwrap_or_else(|e| {
            panic!("scenario `{name}`: {e}; the recordings are part of the checkout")
        });
        // A recording is made between the client and the server, so a datagram lost on the way to
        // the client is in the recording but never reached the client, and its bytes are in no
        // report the client sent. The client's request for a retransmit is the mark of it; such a
        // recording cannot say which datagrams the client actually counted.
        if capture.client_datagrams().iter().any(|d| {
            ParsedPacket::parse(&d.raw)
                .is_ok_and(|p| p.header.header.contains(PacketFlags::REQUEST_RETRANSMIT))
        }) {
            lossy.push(name);
            continue;
        }
        let mut want = recorded_reports(&capture);
        if want.is_empty() {
            silent.push(name);
            continue;
        }
        let mut have = our_reports(&capture);
        // The order two reports leave in can differ with the frame timing (a report queued while
        // another waits goes out first), so they are compared as sets of reports.
        want.sort_by_key(|r| (r.1, r.0));
        have.sort_by_key(|r| (r.1, r.0));
        // The recording ends with a report or two still waiting on a held packet the retail
        // client never sent; this transport, which does not know the session ended, sends them.
        // Those are the only reports it may send beyond the recorded ones, and they close the
        // last few intervals of the session.
        let last = want.last().expect("not empty").1;
        let missing: Vec<Report> = want.iter().copied().filter(|r| !have.contains(r)).collect();
        let extra: Vec<Report> = have.iter().copied().filter(|r| !want.contains(r)).collect();
        let held_at_the_end = |r: &Report| r.1 > last || last - r.1 < 8;
        assert!(
            missing.is_empty() && extra.len() <= 2 && extra.iter().all(held_at_the_end),
            "{name}: the Flow reports differ; the retail client's that this transport did not \
             send: {missing:?}; this transport's that the retail client did not: {extra:?}"
        );
        have.retain(|r| !extra.contains(r));
        assert_eq!(have, want, "{name}: the reports repeat differently");
        total += want.len();
    }
    assert!(
        lossy.len() <= 2,
        "only the two recordings with a natural packet loss are set aside, not {lossy:?}"
    );
    // Three recordings made on one day carry no `Flow` section from their client at all, with
    // the server's intervals advancing throughout, and their client also skips the
    // acknowledgement every other recording sends straight after its connect response. They are
    // not read as evidence about when the report is sent.
    let mut silent: Vec<String> = silent.iter().map(ToString::to_string).collect();
    silent.sort();
    assert_eq!(
        silent, SILENT,
        "the recordings whose client sends no Flow report are exactly the known three"
    );
    assert!(total > 1000, "only {total} reports compared");
    eprintln!("{total} Flow reports reproduced; set aside for a lost datagram: {lossy:?}");
}
