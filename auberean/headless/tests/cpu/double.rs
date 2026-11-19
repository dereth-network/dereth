//! The server double as the driver's port, and a script run against it on a fixed-step clock.

use std::collections::VecDeque;
use std::net::{Ipv4Addr, SocketAddr};

use auberean_client_sdk::net::LocalTime;
use auberean_client_sdk::{ClientData, ClientSettings, Driver, FixedStep, Port};
use auberean_headless::run::Runner;
use auberean_headless::script::{self, Command};
use auberean_testkit::double::script::Script;
use auberean_testkit::double::ServerDouble;

/// A frame: a sixtieth of a second.
pub(crate) const FRAME: f64 = 1.0 / 60.0;
/// The most frames a run is given: two minutes.
pub(crate) const FRAMES: usize = 7200;

/// The double as the client's port: what the double sends in a frame is read in that frame.
pub(crate) struct DoublePort {
    double: ServerDouble,
    me: SocketAddr,
    inbox: VecDeque<(SocketAddr, Vec<u8>)>,
    ticked: Option<f64>,
}

impl Port for DoublePort {
    fn receive(&mut self, now: LocalTime) -> Option<(SocketAddr, Vec<u8>)> {
        if self.ticked != Some(now.0) {
            self.ticked = Some(now.0);
            self.inbox
                .extend(self.double.tick(now).into_iter().map(|d| (d.from, d.bytes)));
        }
        self.inbox.pop_front()
    }

    fn send(&mut self, to: SocketAddr, bytes: &[u8], now: LocalTime) {
        self.double.datagram(to, self.me, bytes, now);
    }
}

/// A driver with a new client (no data, no scripts) and the double playing `script` as its port.
pub(crate) fn driven_by(script: Script) -> (Driver, DoublePort) {
    let double = ServerDouble::new(script, Ipv4Addr::LOCALHOST, 1.0, 7);
    let settings = ClientSettings {
        login_server: double.login_addr(),
        account: b"tester".to_vec(),
        ticket: b"t".to_vec(),
        timestamp: 0,
        session_key: [0; 64],
        script_sends: false,
    };
    let port = DoublePort {
        double,
        me: SocketAddr::from(([127, 0, 0, 1], 50_000)),
        inbox: VecDeque::new(),
        ticked: None,
    };
    (Driver::connect(settings, ClientData::default()), port)
}

/// A run of the script `text` against the double playing `login`, on a sixtieth-of-a-second
/// fixed step, not yet started.
pub(crate) fn runner(login: Script, text: &str) -> (Runner<'static>, DoublePort, FixedStep) {
    runner_of(login, script::parse(text).expect("a script"))
}

/// A run of `commands` against the double playing `login`, not yet started.
pub(crate) fn runner_of(
    login: Script,
    commands: Vec<Command>,
) -> (Runner<'static>, DoublePort, FixedStep) {
    let (driver, port) = driven_by(login);
    (
        Runner::new(driver, None, commands),
        port,
        FixedStep::new(FRAME),
    )
}

/// The run of the script `text` played to its end: every line it told.
pub(crate) fn played(login: Script, text: &str) -> (Runner<'static>, Vec<String>, FixedStep) {
    played_of(login, script::parse(text).expect("a script"))
}

/// The run of `commands` played to its end: every line it told.
pub(crate) fn played_of(
    login: Script,
    commands: Vec<Command>,
) -> (Runner<'static>, Vec<String>, FixedStep) {
    let (mut r, mut port, mut clock) = runner_of(login, commands);
    let done = r
        .run_to_end(&mut clock, &mut port, FRAMES)
        .expect("the script runs");
    assert!(done, "the script ends in two minutes");
    let lines = r.take_lines();
    (r, lines, clock)
}

/// The lines that hold `text`.
pub(crate) fn with<'a>(lines: &'a [String], text: &str) -> Vec<&'a str> {
    lines
        .iter()
        .filter(|l| l.contains(text))
        .map(String::as_str)
        .collect()
}
