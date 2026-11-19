//! The client's frames, run for a host: a [`Driver`] owns the client, reads a [`Clock`] once a
//! frame, hands the client what arrived on a [`Port`], runs the client's frame (and the frame's
//! hook, [`crate::runtime::frame_hook`]) and sends what the client has to send.
//!
//! A host drives the game with actions ([`Driver::act`]) and learns what happened from the events
//! ([`Driver::take_events`]). A host that has more to do between the datagrams arriving and the
//! frame (a window's interface and world) runs the frame's two halves itself
//! ([`Driver::receive`], then [`Driver::finish`]); [`Driver::frame`] runs both. A host without a
//! window that walks the player's body through the world runs [`Driver::frame_walking`]: the body
//! moves by the controls in force between the two halves, as the window's scene moves it.
//!
//! The clock and the port are the host's: a [`FixedStep`] clock and a network in memory make a run
//! that is the same every time; a native host has a socket and its own steady clock (the `net`
//! feature's [`crate::udp::UdpPort`] and [`crate::host_clock::HostClock`]).
//!
//! The driver reads no device and does no I/O of its own.

use core::net::SocketAddr;
use core::ops::{Deref, DerefMut};

use auberean_client_contract::control::ControlFrame;
use auberean_client_contract::event::GameEvent;
use auberean_client_contract::notice::Notice;
use auberean_client_contract::view::{IconLayers, View};
use auberean_client_contract::Action;
use auberean_client_net::LocalTime;
use auberean_client_runtime::frame_hook::FrameHook;
use auberean_client_runtime::view::Icons;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_client_sim::Walk;

/// The client's own clock is the host clock and this many seconds, as every host has given it.
pub const CLIENT_CLOCK_START: f64 = 1.0;

/// The time frames run at.
pub trait Clock {
    /// The time of the frame about to run, in seconds since the clock started: asked once a frame,
    /// and never earlier than the time it last gave.
    fn frame_time(&mut self) -> f64;
}

/// A clock that moves on by a fixed step each frame, whatever the time is: a run on it is the same
/// every time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FixedStep {
    step: f64,
    now: f64,
}

impl FixedStep {
    /// A clock at zero that moves `step` seconds each frame.
    #[must_use]
    pub const fn new(step: f64) -> Self {
        Self { step, now: 0.0 }
    }

    /// The step from the next frame on.
    pub fn set_step(&mut self, step: f64) {
        self.step = step;
    }

    /// The time of the last frame (zero before the first).
    #[must_use]
    pub const fn now(&self) -> f64 {
        self.now
    }
}

impl Clock for FixedStep {
    fn frame_time(&mut self) -> f64 {
        self.now += self.step.max(0.0);
        self.now
    }
}

/// Where the client's datagrams arrive and go: a socket, or a network in memory. `now` is the
/// client's own clock as the frame stands.
pub trait Port {
    /// The next datagram that arrived, and where it came from; none when no more wait.
    fn receive(&mut self, now: LocalTime) -> Option<(SocketAddr, Vec<u8>)>;

    /// Send a datagram to `to`.
    fn send(&mut self, to: SocketAddr, bytes: &[u8], now: LocalTime);
}

/// The client and its frames (see the module documentation). Until the game's views are part of
/// the contract, the client itself is read through the driver (it dereferences to it).
#[derive(Debug)]
pub struct Driver {
    client: Client,
    now: LocalTime,
}

impl Driver {
    /// Drive `client`.
    #[must_use]
    pub fn new(client: Client) -> Self {
        let now = LocalTime(client.local_time());
        Self { client, now }
    }

    /// Drive `client`, made during a frame that stands at `now` on the host's clock: its first
    /// half ran without the client, and [`Self::finish`] runs the client's frame at that time.
    #[must_use]
    pub fn during_frame(client: Client, now: f64) -> Self {
        Self {
            client,
            now: LocalTime(CLIENT_CLOCK_START + now),
        }
    }

    /// A new client that logs in to the server the settings name, from its first frame on.
    #[must_use]
    pub fn connect(settings: ClientSettings, data: ClientData) -> Self {
        Self::new(Client::new(settings, data))
    }

    /// The client.
    #[must_use]
    pub const fn client(&self) -> &Client {
        &self.client
    }

    /// The client, mutably.
    pub fn client_mut(&mut self) -> &mut Client {
        &mut self.client
    }

    /// The client, no longer driven.
    #[must_use]
    pub fn into_client(self) -> Client {
        self.client
    }

    /// The client's own clock at the last frame, seconds.
    #[must_use]
    pub const fn local_time(&self) -> f64 {
        self.now.0
    }

    /// The player asks for something: carried out at once.
    pub fn act(&mut self, action: Action) {
        self.client.act(action);
    }

    /// What happened since the host last asked, oldest first.
    pub fn take_events(&mut self) -> Vec<GameEvent> {
        self.client.take_events()
    }

    /// What the player's scripts told the panels since the host last asked, in order, read as the
    /// contract carries them ([`auberean_client_contract::notice`]). A host that reads none takes
    /// them all the same, or they pile up.
    pub fn take_notices(&mut self) -> Vec<Notice> {
        self.client.take_notices()
    }

    /// Say whether the world takes this client's own word that it keeps a world's overlay over its
    /// data files: given, with the data the client has, when the server asks for them.
    pub fn set_world_takes_overlays(&mut self, on: bool) {
        self.client.data_download_mut().world_takes_overlays = on;
    }

    /// The game as the last refresh left it.
    #[must_use]
    pub fn view(&self) -> &View {
        self.client.view()
    }

    /// Bring the view up to date: what the model holds and what the player's scripts answer for
    /// the panels, now (see [`auberean_client_runtime::view`]). Items' icons need the world's
    /// records: a host without them passes [`no_icons`].
    pub fn refresh_view(&mut self, icons: Icons<'_>) {
        self.client.refresh_view(icons);
    }

    /// Install the frame's hook, replacing any installed before; answers the one it replaced.
    pub fn install_hook(&mut self, hook: Box<dyn FrameHook>) -> Option<Box<dyn FrameHook>> {
        self.client.install_frame_hook(hook)
    }

    /// Take the installed hook out.
    pub fn remove_hook(&mut self) -> Option<Box<dyn FrameHook>> {
        self.client.remove_frame_hook()
    }

    /// The installed hook.
    #[must_use]
    pub fn hook(&self) -> Option<&dyn FrameHook> {
        self.client.frame_hook()
    }

    /// The installed hook, mutably.
    pub fn hook_mut(&mut self) -> Option<&mut (dyn FrameHook + 'static)> {
        self.client.frame_hook_mut()
    }

    /// One frame: the clock read, what arrived handed to the client, the client's frame and its
    /// hook, and what the client has to send sent.
    pub fn frame(&mut self, clock: &mut dyn Clock, port: &mut dyn Port) {
        self.receive(clock.frame_time(), port);
        self.finish(port);
    }

    /// One frame with the player's body walked through the world (`walk`): the clock read, what
    /// arrived handed to the client, then the controls in force this frame taken and the body
    /// moved by them (at the host clock's time, as a window's scene moves it), then the client's
    /// frame and its hook, and what the client has to send sent.
    pub fn frame_walking(
        &mut self,
        clock: &mut dyn Clock,
        port: &mut dyn Port,
        walk: &mut Walk<'_>,
    ) {
        let now = clock.frame_time();
        self.receive(now, port);
        let controls = self.client.take_control_frame();
        walk.update(&mut self.client, &controls, now);
        self.finish(port);
    }

    /// What the controls in force ask of the player's body and camera this frame, and what they
    /// asked since the last: taken once a frame by whatever moves the body ([`Self::frame_walking`]
    /// takes it itself). A host that moves no body takes it all the same, or the presses pile up.
    pub fn take_control_frame(&mut self) -> ControlFrame {
        self.client.take_control_frame()
    }

    /// The frame's first half: the frame stands at `now` on the host's clock, and every datagram
    /// that arrived is handed to the client.
    pub fn receive(&mut self, now: f64, port: &mut dyn Port) {
        self.now = LocalTime(CLIENT_CLOCK_START + now);
        while let Some((from, bytes)) = port.receive(self.now) {
            self.client.handle_datagram(from, &bytes, self.now);
        }
    }

    /// The frame's second half: the client's frame at the time the first half took, its hook,
    /// then what the client has to send sent.
    pub fn finish(&mut self, port: &mut dyn Port) {
        self.client.frame(self.now);
        self.send(port);
    }

    /// Send everything the client has to send.
    pub fn send(&mut self, port: &mut dyn Port) {
        while let Some((to, bytes)) = self.client.poll_transmit() {
            port.send(to, &bytes, self.now);
        }
    }

    /// Close every connection: a goodbye on each, sent now, as the client says when it closes.
    pub fn disconnect(&mut self, port: &mut dyn Port) {
        self.client.disconnect();
        self.send(port);
    }
}

/// The icons of a host without the world's records: none.
#[must_use]
pub fn no_icons(_: &auberean_protocol::types::VisualDesc) -> IconLayers {
    Vec::new()
}

impl Deref for Driver {
    type Target = Client;

    fn deref(&self) -> &Client {
        &self.client
    }
}

impl DerefMut for Driver {
    fn deref_mut(&mut self) -> &mut Client {
        &mut self.client
    }
}
