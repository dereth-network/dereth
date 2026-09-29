//! [`ScenarioView`] -- everything a scenario may read, and nothing it may write.
//!
//! The borrow is the contract. A view holds `&HeadlessClient`, so an assertion physically cannot
//! drive the client it is asserting over -- the same argument `GameView` makes one layer down
//! ("a panel cannot write the value it is about to ask the shard for"). A scenario that needs to
//! set something up does it in a `given` or a `when`, where the mutation is visible as a step.
//!
//! The first six methods are the core vocabulary. The rest are escape hatches on to
//! the live client, for the claims whose evidence is a structure rather than a stream; each one is
//! read-only for the same reason.

use std::sync::Arc;

use dereth_client::app::App;
use dereth_client::frame_events::FrameEvents;
use dereth_client::hud::Hud;
use dereth_client::interaction::Interaction;
use dereth_client::objects::ObjectStream;
use dereth_client_contract::{GameSnapshot, UiRequest};
use dereth_client_model::{Notice, Request, World};
use dereth_dat::RetailDatStore;
use dereth_ui_screens::chat::interface::ChatMessage;

use crate::client::{Backend, HeadlessClient};

/// A read-only handle on the client under a scenario.
#[derive(Debug, Clone, Copy)]
pub struct ScenarioView<'a> {
    client: &'a HeadlessClient,
}

impl<'a> ScenarioView<'a> {
    pub(crate) const fn new(client: &'a HeadlessClient) -> Self {
        Self { client }
    }

    // -------------------------------------------------------------------------------------
    // The core six
    // -------------------------------------------------------------------------------------

    /// The frame, as an owned value, through the client's own projection.
    #[must_use]
    pub fn snapshot(&self) -> GameSnapshot {
        self.client.snapshot()
    }

    /// The opcode of every message the client sent during this scenario, in order.
    #[must_use]
    pub fn outbound_opcodes(&self) -> Vec<u32> {
        self.client.outbound_opcodes()
    }

    /// Every notice the scenario's own drives raised. See [`HeadlessClient::notices`].
    #[must_use]
    pub fn notices(&self) -> &'a [Notice] {
        self.client.notices()
    }

    /// Every UI request the scenario put into the client.
    #[must_use]
    pub fn ui_requests(&self) -> &'a [UiRequest] {
        self.client.ui_requests()
    }

    /// The frame log.
    #[must_use]
    pub fn frame_events(&self) -> &'a FrameEvents {
        self.client.frame_events()
    }

    /// Every chat line the client composed from an event this scenario delivered.
    #[must_use]
    pub fn chat_lines(&self) -> &'a [ChatMessage] {
        self.client.chat_lines()
    }

    // -------------------------------------------------------------------------------------
    // On to the live client
    // -------------------------------------------------------------------------------------

    /// The text of every chat line, in order -- the shape most claims about what the player read
    /// are written in.
    #[must_use]
    pub fn chat_text(&self) -> Vec<&'a str> {
        self.client
            .chat_lines()
            .iter()
            .map(|m| m.body.as_str())
            .collect()
    }

    /// The notice strings, in order.
    #[must_use]
    pub fn notice_text(&self) -> Vec<&'a str> {
        self.client
            .notices()
            .iter()
            .filter_map(|n| match n {
                Notice::DisplayString { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }

    /// Every request the client produced, before it became bytes.
    #[must_use]
    pub fn outbound(&self) -> &'a [Request] {
        self.client.outbound()
    }

    /// The game model.
    #[must_use]
    pub fn world(&self) -> &'a World {
        match self.client.backend() {
            Backend::App(app) => &app.objects().world,
            Backend::Model(m) => &m.objects.world,
        }
    }

    /// The object stream.
    #[must_use]
    pub fn objects(&self) -> &'a ObjectStream {
        match self.client.backend() {
            Backend::App(app) => app.objects(),
            Backend::Model(m) => &m.objects,
        }
    }

    /// The HUD.
    #[must_use]
    pub fn hud(&self) -> &'a Hud {
        match self.client.backend() {
            Backend::App(app) => app.hud(),
            Backend::Model(m) => &m.hud,
        }
    }

    /// The interaction layer -- the refusal line it last composed, and its own counters.
    ///
    /// Some claims are about what the client told the player when the shard refused something, and that line is the layer's and not the
    /// HUD's. See [`HeadlessClient::interaction_mut`].
    ///
    /// [`HeadlessClient::interaction_mut`]: crate::HeadlessClient::interaction_mut
    #[must_use]
    pub fn interaction(&self) -> &'a Interaction {
        match self.client.backend() {
            Backend::App(app) => app.interaction(),
            Backend::Model(m) => &m.interaction,
        }
    }

    /// The whole client, when the scenario was built with one.
    #[must_use]
    pub fn app(&self) -> Option<&'a App> {
        match self.client.backend() {
            Backend::App(app) => Some(app),
            Backend::Model(_) => None,
        }
    }

    /// The whole client, or a panic naming what the scenario should have asked for.
    ///
    /// # Panics
    /// Panics on the model backend.
    #[must_use]
    pub fn expect_app(&self) -> &'a App {
        self.app().expect(
            "this assertion reads the client's shell, so the scenario must be built with \
             ClientSpec::retail() or ClientSpec::gameplay(..)",
        )
    }

    /// The retail dat store the client was built over, for a claim whose subject is a shipped
    /// table rather than a running frame.
    ///
    /// # Panics
    /// Panics on the model backend, which opens no dat.
    #[must_use]
    pub fn dat_store(&self) -> &'a Arc<RetailDatStore> {
        self.client.dat_store().expect(
            "this assertion reads the shipped data, so the scenario must be built with \
             ClientSpec::retail() or ClientSpec::gameplay(..)",
        )
    }

    /// How many frames the scenario has run.
    #[must_use]
    pub const fn frames(&self) -> u64 {
        self.client.frames()
    }

    /// The frame log of this scenario's frames, in its text form.
    #[must_use]
    pub fn events_text(&self) -> String {
        self.client.events_text()
    }
}
