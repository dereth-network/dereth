//! The screens before the world in the interface: each made by the game's flow when its mode
//! comes up (the opening pictures, connecting, the disconnected screen, the character list,
//! creation's hall and appearance, the key map, the credits), fed what lives across them from the
//! client (the account's characters, the server's answers about a character, the connection's
//! progress, creation's avatars and their choices, what the scenes found under the pointer), and
//! what they ask carried out as the client's actions, the flow's next screen, dialogs and the
//! scenes' views.

use std::any::Any;

use auberean_client_sdk::Action;
use auberean_shell::interface::{Game, Overlay, Phase, Request};
use auberean_ui::framework::flow::Screen as FlowScreen;
use auberean_ui::{Handle, Token, Ui};
use auberean_ui_game::character_select::CharacterSelect;
use auberean_ui_game::creation::{self, AvatarModification, AvatarSelection};
use auberean_ui_game::credits::Credits;
use auberean_ui_game::datapatch::DataPatch;
use auberean_ui_game::disconnected::Disconnected;
use auberean_ui_game::flow::mode;
use auberean_ui_game::intro::Intro;
use auberean_ui_game::keyboard::{self, KeyData, KeyMaps, Keyboard};
use auberean_ui_game::pregame::{self, Avatar, CharacterSet, ConnectError, Connection, Notice};

use crate::{hall, Retail, Shared, LABEL_STRINGS};

/// A screen before the world as the flow holds it: the screen, and how many character lists it
/// has seen (a new one tells it).
#[derive(Debug)]
pub(crate) struct PreScreen {
    pub screen: Box<dyn pregame::Screen>,
    lists: u64,
}

impl PreScreen {
    fn made(
        screen: Box<dyn pregame::Screen>,
        data: &Shared,
    ) -> Option<Box<dyn FlowScreen<Shared>>> {
        Some(Box::new(Self {
            screen,
            lists: data.pre.lists,
        }))
    }
}

impl FlowScreen<Shared> for PreScreen {
    fn show(&mut self, ui: &mut Ui, _data: &mut Shared, on: bool) {
        self.screen.show(ui, on);
    }

    /// What the client told it, the account's characters when they came again, then its tick.
    fn update(&mut self, ui: &mut Ui, data: &mut Shared) {
        for n in std::mem::take(&mut data.pre_notices) {
            self.screen.notice(ui, &mut data.pre, &n, &mut data.pre_out);
        }
        if data.pre.lists != self.lists {
            self.lists = data.pre.lists;
            self.screen.update(ui, &mut data.pre, &mut data.pre_out);
        }
        self.screen.tick(ui, &mut data.pre, &mut data.pre_out);
    }

    fn argument(&mut self, ui: &mut Ui, _data: &mut Shared, argument: &str) {
        self.screen.set_error_msg(ui, argument);
    }

    fn close(self: Box<Self>, ui: &mut Ui, data: &mut Shared) {
        self.screen.close(ui, &mut data.pre, &mut data.pre_out);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

// D2-01720
pub(crate) fn make_intro(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = Intro::new(ui, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01748
pub(crate) fn make_datapatch(
    ui: &mut Ui,
    data: &mut Shared,
) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = DataPatch::new(ui, &mut data.pre, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01728
pub(crate) fn make_disconnected(
    ui: &mut Ui,
    data: &mut Shared,
) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = Disconnected::new(ui);
    PreScreen::made(Box::new(s), data)
}

// D2-01822
pub(crate) fn make_select(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = CharacterSelect::new(ui, &mut data.pre, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01858
pub(crate) fn make_avatar(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = AvatarSelection::new(ui, &mut data.pre, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01892
pub(crate) fn make_appearance(
    ui: &mut Ui,
    data: &mut Shared,
) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = AvatarModification::new(ui, &mut data.pre, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01774
pub(crate) fn make_keyboard(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let s = Keyboard::new(ui, &mut data.pre, &mut data.pre_out);
    PreScreen::made(Box::new(s), data)
}

// D2-01790
pub(crate) fn make_credits(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let (shift, ctrl) = ui.modifiers();
    let s = Credits::open(ui, ctrl && ui.alt_held() && !shift)?;
    PreScreen::made(Box::new(s), data)
}

/// The account's characters as the screens take them, from the server's list.
fn character_set(list: &auberean_client_sdk::protocol::login::CharacterList) -> CharacterSet {
    CharacterSet {
        characters: list
            .characters
            .iter()
            .map(|c| pregame::Character {
                id: c.id,
                name: c.name.text(),
                greyed: c.seconds_greyed_out,
            })
            .collect(),
        slots: i32::try_from(list.slots).unwrap_or(-1),
        account: list.account.text(),
    }
}

/// The mapping key maps are found by, the base map's value in it; the actions' enum map's value
/// and its mapping; the action map.
const KEY_MAPS: u32 = 0x4000_000F;
const BASE_MAP: u32 = 0x4000_0001;
const ACTION_NAMES: u32 = 0x4000_001D;
const ENUM_MAPS: u32 = 0x4000_0001;
const ACTION_MAP: u32 = 0x5100_0000;

/// The input word of a key, as the keyboard's scan code names it (a key the game reads), for the
/// key map screen.
#[must_use]
pub(crate) const fn input_word(key: auberean_shell::input::Key) -> u32 {
    key.input_word()
}

/// The left button's input word, as the key map screen hears it.
pub(crate) const LEFT_BUTTON_WORD: u32 = 0x8200_040C;

/// The error a connection that could not be made is told with.
const GENERIC_ERROR: &str = "ID_ConnectionError_Generic";

/// A session's connection error as the connecting screen is told it.
fn connect_error(e: Option<auberean_client_sdk::net::NetErrorCode>) -> ConnectError {
    use auberean_client_sdk::net::NetErrorCode;
    match e {
        Some(NetErrorCode::BadServerAddress) => ConnectError::BadAddress,
        Some(e) => ConnectError::Named(e.id_string().to_owned()),
        None => ConnectError::Named(String::new()),
    }
}

/// Where the session's connection stands, as the connecting screen was last told.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) enum Net {
    #[default]
    Idle,
    Connecting,
    Connected,
}

impl Retail {
    /// A key pressed (or the left button) while the key map is up: heard by it with the modifier
    /// held (left Shift, right Shift, left Ctrl, right Ctrl, the first of them), at its next tick.
    // D2-04867
    pub(crate) fn key_for_keyboard(&mut self, word: u32, held: u8) {
        if self.pre::<Keyboard>().is_none() {
            return;
        }
        let m = keyboard::meta::of_held(held);
        self.flow.flow.data.pre.key_hits.push((word, m));
    }

    /// The screen before the world up, if one is.
    pub(crate) fn pre_screen(&self) -> Option<&dyn pregame::Screen> {
        let s = self.flow.flow.current()?;
        s.as_any()?
            .downcast_ref::<PreScreen>()
            .map(|p| p.screen.as_ref())
    }

    /// The screen before the world up, as itself.
    pub(crate) fn pre<T: pregame::Screen>(&self) -> Option<&T> {
        self.pre_screen()?.as_any().downcast_ref::<T>()
    }

    /// What lives across the screens before the world, from the client: the account's characters
    /// (a new list counted), the races the account may make, the server's answers about a
    /// character asked, the connection's progress.
    pub(crate) fn feed_pregame(&mut self, game: &Game<'_>) {
        // An entry refused, or the login ended: the screen its reason leads to, saying why (the
        // character list for a character still in the world; the disconnected screen with the
        // server's own line for a boot).
        if let Some((reason, n)) = game.client.and_then(|c| c.model.refused) {
            if n != self.refusals {
                self.refusals = n;
                let given = game
                    .client
                    .and_then(|c| c.model.refused_with.as_ref())
                    .map(auberean_client_sdk::runtime::text::from_wire);
                self.flow.character_error(&self.ui, reason, given.as_ref());
            }
        }
        let d = &mut self.flow.flow.data;
        d.pre.pointer = self.pointer;
        d.pre.sound_playing = game.screens_sound;
        if let Some(list) = game.client.and_then(|c| c.model.characters.as_ref()) {
            let set = character_set(list);
            if d.pre.characters.as_ref() != Some(&set) {
                // D2-11860
                d.pre.characters = Some(set);
                d.pre.lists += 1;
            }
            d.pre.may_make = (if list.has_expansion != 0 {
                creation::EMPYREAN
            } else {
                0
            }) | (if list.unknown != 0 {
                creation::DRUDGE
            } else {
                0
            });
        }
        if let Some((response, id, n, error)) = game.client.and_then(|c| c.model.creation) {
            if n != self.answers {
                self.answers = n;
                let character = game
                    .client
                    .and_then(|c| c.model.characters.as_ref())
                    .and_then(|l| l.characters.iter().find(|c| c.id == id))
                    .map(|c| pregame::Character {
                        id: c.id,
                        name: c.name.text(),
                        greyed: c.seconds_greyed_out,
                    })
                    .or_else(|| {
                        (response == 1).then(|| pregame::Character {
                            id,
                            ..pregame::Character::default()
                        })
                    });
                d.pre_notices.push(Notice::CharGenVerification {
                    response,
                    character,
                    error,
                });
            }
        }
        // The connection's progress, for the connecting screen.
        let net = match game.phase {
            Phase::Connecting => Net::Connecting,
            Phase::Characters | Phase::Entering | Phase::World => Net::Connected,
            Phase::Disconnected => Net::Idle,
        };
        if net != self.net {
            match net {
                Net::Connecting => d.pre_notices.push(Notice::Net(Connection::Connecting {
                    so_far: 1,
                    total: 1,
                })),
                Net::Connected if self.net == Net::Connecting || self.net == Net::Idle => {
                    d.pre_notices.push(Notice::Net(Connection::Connected));
                }
                // The session ended while connecting with no reason given: the server could not
                // be reached.
                Net::Idle if self.net == Net::Connecting && game.ended.is_none() => {
                    if let Some(l) = &self.login {
                        let (host, port) = (l.server.ip().to_string(), l.server.port());
                        d.pre_notices.push(Notice::Net(Connection::Failed {
                            host,
                            port,
                            error: ConnectError::Named(GENERIC_ERROR.to_owned()),
                        }));
                    }
                }
                Net::Idle | Net::Connected => {}
            }
            self.net = net;
        }
        // The login refused, or the login server letting its connection go, with an error: the
        // connecting screen, while it is up, is told why, once a session.
        let state = game
            .client
            .map(auberean_client_sdk::Client::session_state)
            .or(game.ended);
        match state.and_then(|s| s.connect_failed) {
            None => self.failure_told = false,
            Some(error) if !self.failure_told && self.pre::<DataPatch>().is_some() => {
                self.failure_told = true;
                if let Some(l) = &self.login {
                    let (host, port) = (l.server.ip().to_string(), l.server.port());
                    self.flow
                        .flow
                        .data
                        .pre_notices
                        .push(Notice::Net(Connection::Failed {
                            host,
                            port,
                            error: connect_error(error),
                        }));
                }
            }
            Some(_) => {}
        }
        // The data download's events, each once (a new session's from its first).
        let events = game
            .client
            .map_or(&[][..], |c| c.data_download().events.as_slice());
        if events.len() < self.ddd_told {
            self.ddd_told = 0;
        }
        for &(event, value) in &events[self.ddd_told..] {
            ddd_notice(&mut self.flow.flow.data.pre_notices, event, value);
        }
        self.ddd_told = events.len();
        // The connection's health, while the connecting screen is up to show it.
        let connecting = self.pre::<DataPatch>().is_some();
        if let Some(h) = game
            .client
            .and_then(auberean_client_sdk::Client::heartbeat)
            .filter(|_| connecting)
        {
            // The rate over the last two measures; busy while packets came in the last.
            self.flow.flow.data.pre_notices.push(Notice::Heartbeat {
                rate: h.rate_in,
                received: h.packets_in > 0,
            });
        }
    }

    /// Creation's avatars made when the hall is asked for and none are kept; thrown away when the
    /// character list comes up.
    pub(crate) fn keep_creation(&mut self, game: &Game<'_>) {
        let queued = self.flow.flow.queued_mode();
        let listing = self.flow.flow.mode() == mode::CHARACTER_SELECTION;
        if listing && queued != mode::AVATAR_SELECTION && self.creation.is_some() {
            // D2-11863, D2-11862, D2-11864
            self.creation = None;
            let d = &mut self.flow.flow.data.pre;
            d.in_creation = false;
            d.avatar = None;
            d.choices.clear();
            d.choices_version += 1;
        }
        if queued == mode::AVATAR_SELECTION && self.creation.is_none() {
            self.creation = Some(hall::Creation::new(&game.world.records, &mut self.random));
        }
    }

    /// The client's key maps for the key map screen when it is asked for: the maps bound now (the
    /// base map and the player's, as the client read them from their files or the data); and,
    /// read from the records once, the actions in the game's order (named ones shown, the others
    /// kept for the reserved keys) and each preset.
    pub(crate) fn keep_keys(&mut self, game: &Game<'_>) {
        if self.flow.flow.queued_mode() != mode::KEYBOARD {
            return;
        }
        let bound = |l: Option<&auberean_shell::keys::Layer>| -> keyboard::KeyMap {
            l.map(|l| l.map.bindings.iter().map(|b| (b.input, b.action)).collect())
                .unwrap_or_default()
        };
        let layers = game.keys.stack.layers();
        let in_use = KeyMaps {
            base: bound(layers.first()),
            normal: bound(layers.get(1)),
            shift: bound(layers.get(2)),
            control: bound(layers.get(3)),
        };
        let screen_up = self.pre::<Keyboard>().is_some();
        if let Some(k) = &mut self.flow.flow.data.pre.keys {
            if !screen_up {
                k.maps = in_use;
            }
            return;
        }
        let records = &game.world.records;
        let map_of = |e: u32| -> keyboard::KeyMap {
            let id = records.data_id_by_enum(e, KEY_MAPS);
            records
                .get::<auberean_assets::ui::KeyMap>(id)
                .map(|m| m.bindings.iter().map(|b| (b.input, b.action)).collect())
                .unwrap_or_default()
        };
        let base = map_of(BASE_MAP);
        let maps_of = |e: [u32; 3]| KeyMaps {
            base: base.clone(),
            normal: map_of(e[0]),
            shift: map_of(e[1]),
            control: map_of(e[2]),
        };
        let mut actions = Vec::new();
        let mut unnamed = Vec::new();
        let texts = records.get::<auberean_assets::ui::ActionBindings>(ACTION_MAP);
        let mut id = records.data_id_by_enum(ACTION_NAMES, ENUM_MAPS);
        let mut order = Vec::new();
        for _ in 0..8 {
            let Some(m) = records.get::<auberean_assets::strings::EnumMap>(id) else {
                break;
            };
            let mut own: Vec<u32> = m.names.iter().map(|(v, _)| *v).collect();
            own.sort_unstable();
            order.extend(own);
            id = m.base;
            if id == 0 {
                break;
            }
        }
        for action in order {
            let text = texts.as_ref().and_then(|t| {
                let (name, _) = t.descrip_values(action)?;
                self.ui.assets().string(t.string_table.0, name)
            });
            match text {
                Some(s) if !s.is_empty() => actions.push((action, s)),
                _ => unnamed.push(action),
            }
        }
        let presets = keyboard::preset::MAPS
            .iter()
            .map(|(b, e)| (*b, maps_of(*e)))
            .collect();
        self.flow.flow.data.pre.keys = Some(KeyData {
            actions,
            unnamed,
            maps: in_use,
            presets,
        });
    }

    /// The chosen avatar's choices as the appearance shows them: each kind its race has, its
    /// label, its stops and the stop the avatar's value stands at.
    // D2-01870
    pub(crate) fn refresh_choices(&mut self) {
        let mut out = Vec::new();
        if let Some((avatar, choices)) = self.creation.as_ref().and_then(hall::Creation::chosen) {
            for (_, t) in creation::ROWS {
                let Some(c) = choices.iter().find(|c| c.physique_type == t) else {
                    continue;
                };
                let label = self
                    .ui
                    .assets()
                    .string(LABEL_STRINGS, Token::of(c.label).raw())
                    .unwrap_or_default();
                let at = avatar
                    .value(t)
                    .and_then(|v| auberean_world::chargen::stop_of(&c.values, v))
                    .unwrap_or(0);
                out.push(creation::Choice {
                    physique: t,
                    label,
                    stops: c.values.len(),
                    at,
                });
            }
        }
        let d = &mut self.flow.flow.data.pre;
        d.choices = out;
        d.choices_version += 1;
    }

    /// What the screens before the world asked, carried out. What the client told them and no
    /// screen before the world was up to hear (in the world) is dropped.
    pub(crate) fn carry_out_pregame(&mut self, requests: &mut Vec<Request>) {
        if self.pre_screen().is_none() {
            self.flow.flow.data.pre_notices.clear();
        }
        use pregame::Request as P;
        let asked = std::mem::take(&mut self.flow.flow.data.pre_out);
        for r in asked {
            match r {
                P::Mode(m, text) => self.flow.queue_mode(m, &text),
                P::Dialog { dialog, parent } => {
                    self.dialogs.make_dialog(
                        &mut self.ui,
                        Some(parent),
                        &(&dialog).into(),
                        &mut self.said,
                    );
                }
                P::DialogDown(id) => self.dialogs.close_dialog(&mut self.ui, id, &mut self.said),
                P::Connect => {
                    if let (Some(l), false) = (&self.login, self.connected) {
                        requests.push(Request::Connect {
                            server: l.server,
                            account: l.account.clone(),
                            password: l.password.clone(),
                        });
                        self.connected = true;
                    }
                }
                P::Play { character } => {
                    requests.push(Request::Act(Action::PlayCharacter { character }));
                }
                P::Delete { character } => {
                    requests.push(Request::Act(Action::DeleteCharacter { character }));
                }
                P::Create { name, admin } => {
                    let choice = self
                        .creation
                        .as_ref()
                        .and_then(|c| crate::new_character(&name, admin, c));
                    if let Some(c) = choice {
                        requests.push(Request::Act(Action::CreateCharacter(c)));
                    }
                }
                P::FindUnderPointer => {
                    self.pick = Some(Self::at(self.pointer));
                    self.picking = true;
                }
                P::Choose(Avatar { race, sex }) => {
                    if let Some(c) = &mut self.creation {
                        c.chosen = (race, sex);
                        c.turn = 180.0;
                    }
                    self.refresh_choices();
                }
                P::Behave { figure, behavior } => {
                    self.behaviors.push((figure, behavior));
                    self.behaviors_begin = 2;
                }
                P::Randomize { mask } => {
                    if let Some(c) = &mut self.creation {
                        let key = c.chosen;
                        if let (Some(av), Some(cs)) = (c.avatars.get_mut(&key), c.choices.get(&key))
                        {
                            av.randomize(cs, mask, &mut self.random);
                        }
                    }
                    self.refresh_choices();
                }
                P::Choice { physique, stop } => {
                    if let Some(c) = &mut self.creation {
                        let key = c.chosen;
                        let v = c
                            .choices
                            .get(&key)
                            .and_then(|cs| cs.iter().find(|x| x.physique_type == physique))
                            .and_then(|x| x.values.get(stop).copied());
                        if let (Some(v), Some(av)) = (v, c.avatars.get_mut(&key)) {
                            // D2-11925
                            av.values.insert(physique, v);
                        }
                    }
                    self.refresh_choices();
                }
                P::Turn { heading } => {
                    if let Some(c) = &mut self.creation {
                        c.turn = heading;
                    }
                }
                P::Zoom(on) => self.zoomed = on,
                P::Keys(maps) => {
                    // Written to their files and bound from now on.
                    requests.push(Request::KeyMaps([
                        maps.normal.clone(),
                        maps.shift.clone(),
                        maps.control.clone(),
                    ]));
                    if let Some(k) = &mut self.flow.flow.data.pre.keys {
                        k.maps = maps;
                    }
                }
                P::LiveKeys(maps) => {
                    // Bound from now on, nothing written.
                    requests.push(Request::BindKeyMaps([
                        maps.normal.clone(),
                        maps.shift.clone(),
                        maps.control.clone(),
                    ]));
                    if let Some(k) = &mut self.flow.flow.data.pre.keys {
                        k.maps = maps;
                    }
                }
                P::Quit => requests.push(Request::Quit),
                P::Sound(id) => requests.push(crate::sounds::screens(id)),
                P::Silence => requests.push(crate::sounds::screens_silence()),
                P::Restore { character, name } => {
                    requests.push(Request::Act(Action::RestoreCharacter { character, name }));
                }
            }
        }
        let d = &mut self.flow.flow.data.pre;
        d.avatar = self.creation.as_ref().map(|c| Avatar {
            race: c.chosen.0,
            sex: c.chosen.1,
        });
        if !d.in_creation {
            d.avatar = None;
        }
    }

    /// The scene of the screen up: the carousel of the account's characters, the hall, or the
    /// avatar being shaped; a selection asked carried in it (answered with none at once when the
    /// screen draws no scene).
    // D2-10447
    pub(crate) fn pregame_scenes(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        let records = &game.world.records;
        let now = self.ui.now();
        let mut scene = None;
        if let Some(s) = self.pre::<CharacterSelect>() {
            let portal = s.viewports().1.and_then(|v| self.ui.screen_rect(v));
            if let Some(v) = portal.and_then(|r| hall::portal_scene(records, r)) {
                overlay.scenes.push(v);
            }
            let list = game.client.and_then(|c| c.model.characters.as_ref());
            let rect = s.viewports().0.and_then(|v| self.ui.screen_rect(v));
            if let (Some(l), Some(r)) = (list, rect) {
                scene = hall::select_scene(records, r, &l.characters, &s.arm_headings(now));
            }
        } else if let Some(a) = self.pre::<AvatarSelection>() {
            let rect = a.viewport().and_then(|v| self.ui.screen_rect(v));
            if let (Some(c), Some(r)) = (&self.creation, rect) {
                let may = self.flow.flow.data.pre.may_make;
                let under = self.flow.flow.data.pre.under;
                if let Some((v, who)) = hall::hall_scene(records, r, c, may, under) {
                    self.flow.flow.data.pre.hall = who
                        .iter()
                        .map(|&(race, sex)| Avatar { race, sex })
                        .collect();
                    scene = Some(v);
                }
            }
        } else if let Some(a) = self.pre::<AvatarModification>() {
            let rect = a.viewport().and_then(|v| self.ui.screen_rect(v));
            if let (Some(c), Some(r)) = (&self.creation, rect) {
                scene = hall::shaping_scene(records, r, c, self.zoomed);
            }
        }
        match &mut scene {
            Some(v) => {
                v.pick = self.pick.take();
                v.behaviors = std::mem::take(&mut self.behaviors);
                overlay.scenes.push(v.clone());
            }
            None if self.picking => {
                self.pick = None;
                self.picking = false;
                self.flow.flow.data.pre.selection = Some(None);
            }
            None => {}
        }
    }

    /// What the last frame's pick found, for the screen that asked.
    // D2-01816, D2-01842
    pub(crate) fn pregame_picked(&mut self, game: &Game<'_>) {
        // A behavior asked shows as played only once a frame has drawn it.
        if self.behaviors_begin > 0 {
            self.behaviors_begin -= 1;
        } else {
            self.flow
                .flow
                .data
                .pre
                .behaving
                .retain(|f| game.behaving.contains(&(0, *f)));
        }
        if !self.picking || self.pick.is_some() {
            return;
        }
        if let Some(&(_, found)) = game.picked.first() {
            self.picking = false;
            self.flow.flow.data.pre.selection = Some(found.map(|hit| hit.figure));
        }
    }

    /// The viewport whose scene the screen up shows: what is deeper is drawn behind the scene.
    pub(crate) fn pregame_viewport(&self) -> Option<Handle> {
        if let Some(s) = self.pre::<CharacterSelect>() {
            return s.viewports().0;
        }
        if let Some(a) = self.pre::<AvatarSelection>() {
            return a.viewport();
        }
        self.pre::<AvatarModification>()
            .and_then(AvatarModification::viewport)
    }
}

/// A data download event for the connecting screen.
fn ddd_notice(out: &mut Vec<Notice>, event: u32, value: u32) {
    out.push(Notice::Ddd {
        event,
        value: value.cast_signed(),
    });
}
