//! Connecting: the screen that logs the account in and brings the game's data up to date before
//! the character list. Made, it connects and says so at its foot; the connection's progress, the
//! data downloaded (a meter, the share done of how many kilobytes, the rate) and its end are
//! written there as they come. A connection that fails leads to the disconnected screen with the
//! reason. Once the account's characters are listed, the game goes on to the list, or straight
//! into the world with the character the client was started with.

use std::any::Any;

use auberean_text::Value;
use auberean_ui::framework::FrameId;
use auberean_ui::token::{attr, msg};
use auberean_ui::{Handle, Mail, Ui, Value as Attr};

use crate::flow::mode;
use crate::pregame::{
    self, message, ConnectError, Connection, Data, Notice, Request, Screen, INVALID_ID,
};

/// The layout the screen is built from, by enumeration value.
pub const LAYOUT_ENUM: u32 = 0x4100_000D;

mod name {
    use auberean_ui::Token;
    pub const DATA_PATCH_FIELD: Token = Token::of("DataPatchField");
    pub const ERROR_TEXT: Token = Token::of("ErrorText");
    pub const PATCH_TEXT: Token = Token::of("PatchText");
    pub const RATE_TEXT: Token = Token::of("RateText");
    pub const PATCH_PROGRESS: Token = Token::of("PatchProgress");
    pub const ANIMATE_ME: Token = Token::of("AnimateMe");
    pub const QUIT_BUTTON: Token = Token::of("QuitButton");
    /// The busy picture's state while data comes.
    pub const ANIMATE: Token = Token::of("animate");
}

/// The data download's events, by number.
pub mod event {
    /// Looking for data to download.
    pub const INTERROGATION: u32 = 0;
    /// Waiting in line (the value: the place).
    pub const WAITING: u32 = 1;
    /// Downloading (the value: how many bytes).
    pub const PATCHING: u32 = 2;
    /// Done.
    pub const DONE: u32 = 3;
    /// More bytes came (the value: how many).
    pub const PROGRESS: u32 = 4;
}

/// The connection errors' string table, by enumeration value.
pub const CONNECTION_ERRORS: u32 = 0x4000_0005;

/// The errors the connecting screen words its own way, by identifier.
const GENERIC: &str = "ID_ConnectionError_Generic";
const CANT_CRYPTO: &str = "ID_NetError_CantCrypto";
const LOGON_SERVER_MIGRATED: &str = "ID_ConnectionError_LogonServerMigrated";
const SERVER_CLOSED_CONNECTION: &str = "ID_ConnectionError_ServerClosedConnection";

/// A connection error's own line, written with its variables; empty when the table has none.
fn connection_error(ui: &Ui, error: &str, vars: &[(&str, Value)]) -> String {
    let table = ui
        .assets()
        .data_id_by_enum(CONNECTION_ERRORS, crate::flow::TABLE_MAPPING);
    let mut info = auberean_text::StringInfo::entry(table, auberean_ui::Token::of(error).raw());
    for (n, v) in vars {
        info.add_var(auberean_ui::Token::of(n).raw(), v.clone());
    }
    ui.assets().write(&info).unwrap_or_default()
}

/// The connecting screen.
#[derive(Debug)]
pub struct DataPatch {
    frame: FrameId,
    meter: Option<Handle>,
    animate: Option<Handle>,
    error_text: Option<Handle>,
    patch_text: Option<Handle>,
    rate_text: Option<Handle>,
    /// The bytes to download (-1: not known yet), and those that came.
    expected: i32,
    received: i32,
    /// Listening to the data download.
    downloading: bool,
}

impl DataPatch {
    /// The screen made: its field and its texts, the meter and its busy picture, Quit heard, the
    /// connection's health heard; then it connects.
    // D2-01745, D2-01747, D2-01748
    pub fn new(ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) -> Self {
        let frame = ui.new_main_frame();
        let field = ui.frame_create_root(frame, LAYOUT_ENUM, name::DATA_PATCH_FIELD);
        let child = |ui: &Ui, n| field.and_then(|f| ui.child_recursive(f, n));
        let error_text = child(ui, name::ERROR_TEXT);
        let patch_text = child(ui, name::PATCH_TEXT);
        let rate_text = child(ui, name::RATE_TEXT);
        let meter = child(ui, name::PATCH_PROGRESS);
        let animate = meter.and_then(|m| ui.child_recursive(m, name::ANIMATE_ME));
        pregame::listen(ui, frame, name::QUIT_BUTTON, msg::BUTTON_RELEASE_ON);
        let mut me = Self {
            frame,
            meter,
            animate,
            error_text,
            patch_text,
            rate_text,
            expected: -1,
            received: 0,
            downloading: false,
        };
        me.connect(ui, out);
        // A session the client had already brought this far (the developer's own login) goes on
        // as the list's coming would.
        if data.characters.is_some() {
            me.list_arrived(ui, data, out);
        }
        me
    }

    /// Connect: saying so, and asking the client.
    // D2-01738
    fn connect(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let text = message(ui, "ID_DataPatch_ConnectingGUN", &[]);
        self.set_error_msg(ui, &text);
        out.push(Request::Connect);
    }

    /// The connection failed with `error` (its identifier: the login refused, or the login
    /// server letting its connection go): the disconnected screen, with why. The server's address
    /// not understood names the host; the generic error, the server and port the client could not
    /// reach; any other, its own line. An error with no line of its own leaves the screen as it is
    /// (a migrated login server's says the server closed the connection).
    // D2-01737
    fn on_connect_failed(
        &mut self,
        ui: &mut Ui,
        host: &str,
        port: u16,
        error: &ConnectError,
        out: &mut Vec<Request>,
    ) {
        let host_value = || Value::Text(Box::new(auberean_text::StringInfo::literal(host)));
        let ConnectError::Named(error) = error else {
            let text = message(ui, "ID_NetErr_ResolveHostname", &[("host", host_value())]);
            Self::connect_failed_with(ui, &text, out);
            return;
        };
        let error = error.as_str();
        let mut text = match error {
            GENERIC => message(
                ui,
                "ID_NetErr_Error",
                &[
                    ("host", host_value()),
                    ("port", Value::Int(i32::from(port))),
                ],
            ),
            // The host's last crypto error, which this client never has.
            CANT_CRYPTO => connection_error(ui, error, &[("error", Value::Int(0))]),
            _ => connection_error(ui, error, &[]),
        };
        if text.is_empty() && error == LOGON_SERVER_MIGRATED {
            text = connection_error(ui, SERVER_CLOSED_CONNECTION, &[]);
        }
        Self::connect_failed_with(ui, &text, out);
    }

    /// The disconnected screen, saying the connection failed with this error's line (none: the
    /// screen stays as it is).
    fn connect_failed_with(ui: &Ui, text: &str, out: &mut Vec<Request>) {
        if text.is_empty() {
            return;
        }
        let text = message(
            ui,
            "ID_DataPatch_ConnectFailed",
            &[(
                "error",
                Value::Text(Box::new(auberean_text::StringInfo::literal(text))),
            )],
        );
        out.push(Request::Mode(mode::DISCONNECTED, text));
    }

    /// The connection's progress, written at the foot; connected, the data download heard.
    // D2-01749
    fn on_net_status_change(&mut self, ui: &mut Ui, status: &Connection, out: &mut Vec<Request>) {
        let text = match status {
            Connection::Connecting { so_far, total } => message(
                ui,
                "ID_DataPatch_Connecting1",
                &[
                    ("so_far", Value::Int(i32::try_from(*so_far).unwrap_or(0))),
                    ("total", Value::Int(i32::try_from(*total).unwrap_or(0))),
                ],
            ),
            Connection::Waiting => message(ui, "ID_DataPatch_Connecting2", &[]),
            Connection::Connected => {
                let t = message(ui, "ID_DataPatch_Connected", &[]);
                self.set_error_msg(ui, &t);
                self.downloading = true;
                return;
            }
            Connection::Failed { host, port, error } => {
                self.on_connect_failed(ui, host, *port, error, out);
                return;
            }
        };
        self.set_error_msg(ui, &text);
    }

    /// The data download's event: what it is doing at the foot (until the size is known), the
    /// bytes to come and come, the meter.
    // D2-01744
    pub fn on_ddd_event(&mut self, ui: &mut Ui, event: u32, value: i32) {
        let say = |me: &mut Self, ui: &mut Ui, text: String| {
            if !text.is_empty() {
                me.set_error_msg(ui, &text);
            }
        };
        match event {
            event::INTERROGATION if self.expected == -1 => {
                let t = message(ui, "ID_DataPatch_Interrogation", &[]);
                say(self, ui, t);
            }
            event::WAITING if self.expected == -1 => {
                let t = message(ui, "ID_DataPatch_Waiting", &[("total", Value::Int(value))]);
                say(self, ui, t);
            }
            event::PATCHING => {
                self.expected = value;
                let t = message(ui, "ID_DataPatch_Patching", &[]);
                say(self, ui, t);
            }
            event::DONE => {
                self.downloading = false;
                if self.expected == -1 {
                    self.expected = 0;
                }
                self.received = self.expected;
                let t = message(ui, "ID_DataPatch_PatchingDone", &[]);
                say(self, ui, t);
            }
            event::PROGRESS => self.received = self.received.wrapping_add(value),
            _ => {}
        }
        self.refresh(ui);
    }

    /// The meter at the share of the bytes come, and the share and the size in kilobytes beside
    /// it, once the size is known.
    // D2-01732
    fn refresh(&mut self, ui: &mut Ui) {
        let (Some(meter), Some(text)) = (self.meter, self.patch_text) else {
            return;
        };
        if self.expected == -1 {
            return;
        }
        #[allow(clippy::cast_precision_loss)]
        let share = if self.expected == 0 {
            1.0
        } else {
            (self.received as f32 / self.expected as f32).clamp(0.0, 1.0)
        };
        ui.set_attr(meter, attr::GOAL_POSITION, Attr::Float(share));
        let percent = dereth_primitives::num::to_i32(share * 100.0);
        let total = self.expected / 1024;
        let t = message(
            ui,
            "ID_DataPatch_PatchProgress",
            &[
                ("percent", Value::UInt(u32::try_from(percent).unwrap_or(0))),
                ("total", Value::UInt(total.cast_unsigned())),
            ],
        );
        ui.set_text(text, &t);
    }

    /// The connection's health: the busy picture moves while data comes, and the rate beside it.
    // D2-01733
    pub fn on_heartbeat(&mut self, ui: &mut Ui, rate: f64, received: bool) {
        let (Some(animate), Some(rate_text)) = (self.animate, self.rate_text) else {
            return;
        };
        let state = if received {
            name::ANIMATE
        } else {
            auberean_ui::token::state::STATELESS
        };
        if ui.node(animate).map(|n| n.state) != Some(state) {
            ui.set_state(animate, state);
        }
        let t = message(
            ui,
            "ID_DataPatch_PatchRate",
            &[(
                "rate",
                Value::Float {
                    value: rate * 0.000_976_562_5,
                    decimals: 2,
                },
            )],
        );
        ui.set_text(rate_text, &t);
    }

    /// The account's characters are listed: the download is done, and the game goes on, into
    /// the world as the character the client was started with (the one named to play, else the
    /// one named to make and play; a living one of that name, or of that name marked as an
    /// administrator's), else to the character list. The names are asked after once.
    // D2-01742
    fn list_arrived(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        self.received = self.expected;
        self.refresh(ui);
        // D2-01734, D2-01735
        let mut start = std::mem::take(&mut data.start_character);
        let create = std::mem::take(&mut data.create_character);
        if start.is_empty() {
            start = create;
        }
        if !start.is_empty() {
            let set = data.characters.as_ref();
            let lower = start.to_lowercase();
            let admin = format!("+{start}").to_lowercase();
            let found = set.and_then(|s| {
                s.characters.iter().enumerate().find_map(|(i, c)| {
                    (s.greyed_out_for(i) == 0
                        && c.id != INVALID_ID
                        && (c.name.to_lowercase() == lower || c.name.to_lowercase() == admin))
                        .then_some(c)
                })
            });
            if let Some(c) = found {
                out.push(Request::Play { character: c.id });
                pregame::queue(mode::GAMEPLAY, out);
                return;
            }
        }
        pregame::queue(mode::CHARACTER_SELECTION, out);
    }

    /// Where the meter stands: the bytes to come (-1: not known) and come.
    #[must_use]
    pub const fn progress(&self) -> (i32, i32) {
        (self.expected, self.received)
    }
}

impl Screen for DataPatch {
    fn frame(&self) -> FrameId {
        self.frame
    }

    /// Quit leaves the game.
    // D2-01730
    fn tick(&mut self, ui: &mut Ui, _data: &mut Data, out: &mut Vec<Request>) {
        for m in pregame::take(ui, self.frame) {
            if let Mail::Element { element, .. } = m {
                if element == name::QUIT_BUTTON {
                    out.push(Request::Quit);
                }
            }
        }
    }

    // D2-01742
    fn update(&mut self, ui: &mut Ui, data: &mut Data, out: &mut Vec<Request>) {
        self.list_arrived(ui, data, out);
    }

    /// The connection's progress and health; a character the client was started to make: made,
    /// into the world with it, else the character list.
    // D2-01743
    fn notice(&mut self, ui: &mut Ui, data: &mut Data, n: &Notice, out: &mut Vec<Request>) {
        match n {
            Notice::Net(s) => self.on_net_status_change(ui, s, out),
            Notice::Ddd { event, value } => self.on_ddd_event(ui, *event, *value),
            Notice::Heartbeat { rate, received } if self.downloading => {
                self.on_heartbeat(ui, *rate, *received);
            }
            Notice::CharGenVerification {
                response,
                character,
                ..
            } => match character {
                Some(c) if *response == 1 => {
                    if let Some(set) = data.characters.as_mut() {
                        if set.which_slot(c.id).is_none() {
                            set.characters.push(c.clone());
                        }
                    }
                    out.push(Request::Play { character: c.id });
                    pregame::queue(mode::GAMEPLAY, out);
                }
                _ => pregame::queue(mode::CHARACTER_SELECTION, out),
            },
            _ => {}
        }
    }

    /// The line at the foot.
    // D2-01736
    fn set_error_msg(&mut self, ui: &mut Ui, text: &str) {
        if let Some(e) = self.error_text {
            ui.set_text(e, text);
        }
    }

    /// The screen goes, and with it its listening to the connection.
    // D2-01739, D2-01740, D2-01741, D2-01746
    fn close(self: Box<Self>, ui: &mut Ui, _data: &mut Data, _out: &mut Vec<Request>) {
        ui.drop_frame(self.frame);
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The text of the connecting screen's foot, for a check.
#[must_use]
pub fn error_text(ui: &Ui, screen: &DataPatch) -> Option<String> {
    screen.error_text.and_then(|e| ui.text(e))
}
