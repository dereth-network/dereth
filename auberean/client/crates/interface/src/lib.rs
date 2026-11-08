//! The game's own interface in the shell: the retail screens and panels opened as the session
//! moves, fed from the client's model, the player's input to them first, and what they ask for
//! carried out as the client's actions.
//!
//! **Depends on** the shell's interface seam (`auberean-shell`), the element framework
//! (`auberean-ui`), the game's panels (`auberean-ui-game`), the client (`auberean-client-sdk`), the
//! screens' scenes and their figures (`auberean-scene`), the world's records, looks and character
//! generation's choices (`auberean-world`, `auberean-assets`, `auberean-primitives`), the graphics
//! preferences and their quality presets (`auberean-draw`), the requests its sounds become
//! (`auberean-audio`) and the shared float-to-integer conversion (`dereth-primitives`).
//! **Used by** the desktop client, which hands a [`Retail`] to the shell.
//!
//! A [`Retail`] holds one element tree ([`Ui`]) and the game's flow between its screens
//! ([`GameFlow`]): a screen asked for comes up at the next tick of the interface's clock. The
//! session's phase asks for them: the character list when the account's characters are listed,
//! the play screen when the player enters the world, back to the list when the character logs off.
//! The screens before the world are laid out on a 1024 by 768 screen whatever the window's size,
//! as the client sets its display to 1024 by 768 for them, and show as large as the window lets
//! them at those proportions, in its middle; the play screen sets the display to the window's own
//! size:
//!
//! - **the character list**: its list from the server's; Create leads through the three creation
//!   screens (the avatar, its appearance, the name) and the character is made with the server;
//!   Credits shows the credits; Enter (or entering from the name screen) enters the world;
//! - **the play screen** ([`GamePlay`]): its panels (the status bar, the toolbar with its windows,
//!   the shortcut bar and the chat window, the radar, the camera's), fed what the game shows and the
//!   notices of the player's scripts; what they ask becomes the client's actions and the camera's
//!   settings.
//!
//! Input reaches the interface before the world's controls, as the player's keys bind it: the
//! pointer and its buttons and wheel, and the interface keys (Escape, Enter as the use key, the
//! arrows, Home, End, the page keys; a right press is also the key that looks with the mouse). On
//! the screens before the world it takes everything; in the world it takes the pointer over its
//! panels, the panels' and the camera's keys (the "Defaults" key map's), the middle button (mouse
//! look's switch), and the keyboard while a line is typed in the chat window (Enter starts and ends
//! a line, Escape drops it). An arrow reaches the world's controls unless an element that uses the
//! arrows has the keyboard. The interface's own clock runs every frame, and its cursor shows over
//! the window.
//!
//! What it draws is two pictures of the screen's size: its elements deeper than the scene a
//! screen holds in a viewport, behind the world, and the rest in front. They are redrawn only when
//! the tree changed.
//!
//! **Must never** reach a platform or a device: it sees the game through the shell's
//! [`auberean_shell::interface::Game`] and draws into pictures the host shows.

mod chat;
pub mod data;
pub mod doll;
pub mod hall;
mod map;
mod options;
pub mod places;
pub mod preferences;
mod pregame;
mod records;
mod sounds;
pub mod strings;
mod world;

use std::any::Any;
use std::net::SocketAddr;
use std::sync::Arc;

use auberean_client_sdk::contract::ToolbarShortcut;
use auberean_client_sdk::model::Object;
use auberean_client_sdk::Action;
use auberean_shell::dev::DevLogin;
use auberean_shell::host::InputArea;
use auberean_shell::input::{Button, Event, Key};
use auberean_shell::interface::{
    CameraRequest, Canvas, CursorImage, Game, Interface, Overlay, Phase, Request,
};
use auberean_ui::draw::{self, Surface};
use auberean_ui::framework::flow::{Factory, Screen as FlowScreen};
use auberean_ui::types::{button_id, key_id};
use auberean_ui::{Assets, Token, Ui};
use auberean_ui_game::camera;
use auberean_ui_game::character_select::CharacterSelect;
use auberean_ui_game::chat::{self as chat_panel, ChatSettings, ChatUi};
use auberean_ui_game::craft;
use auberean_ui_game::creation::{AvatarModification, AvatarSelection};
use auberean_ui_game::credits::Credits;
use auberean_ui_game::dialog::{self, Dialogs};
use auberean_ui_game::flow::{mode, GameFlow};
use auberean_ui_game::gameplay::{self, GamePlay, View};
use auberean_ui_game::inventory;
use auberean_ui_game::shortcuts::{self, Shortcuts};
use auberean_ui_game::skill_tree;
use auberean_ui_game::statusbar::{self, StatusBar};
use auberean_ui_game::story_quests;
use auberean_ui_game::toolbar::Toolbar;
use auberean_ui_game::{names, selection};
use dereth_primitives::num::to_i32;

/// The interface's string table (the retail layouts' and the client's own messages').
pub const STRINGS: u32 = 0x2500_0012;
/// The string table of places: "Inside", the map coordinates' form.
pub const PLACE_STRINGS: u32 = 0x2500_07B8;
/// The string table of the appearance's row labels.
pub const LABEL_STRINGS: u32 = 0x2500_07C9;

/// The screen the screens before the world are laid out on.
pub const PREGAME: (u32, u32) = (1024, 768);

/// The Delete key's code as a text element takes it.
const DELETE_KEY: u16 = 0x2E;

/// How many pictures are kept to be drawn on again.
const RETIRED_PICTURES: usize = 6;

/// The action the player's keys bind the right button to (looking with the mouse while held): an
/// interface key.
const MOUSE_LOOK_KEY: u32 = camera::action::INSTANT_MOUSE_LOOK;

/// An account to log in with, as the launcher hands the client one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub server: SocketAddr,
    pub account: String,
    pub password: String,
}

/// The avatar chosen in the hall.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Avatar {
    pub race: u32,
    pub female: bool,
}

/// What lives across the flow's screens: what the play screen is given this frame, and what it
/// asks.
// D2-11874, D2-11865
#[derive(Debug, Default)]
struct Shared {
    /// The player is in the world.
    in_world: bool,
    /// How many times the player's body has come to be placed with its cells loaded.
    arrivals: u32,
    /// The window's size (the play screen's display).
    window: (u32, u32),
    vitals: Option<[u32; 4]>,
    place: Option<String>,
    lines: Vec<(u32, String)>,
    /// For each of `lines`, the string reference it was written from, when it came as one.
    line_infos: Vec<Option<auberean_client_sdk::text::StringInfo>>,
    /// How many of `lines` came before the scripts said the options changed, if they did.
    lines_before_options: usize,
    /// The chat's part of the player's options, as last read, and a new player's.
    chat: Option<ChatSettings>,
    chat_defaults: Option<ChatSettings>,
    /// What the shortcut bar asks of the player's scripts, as the client last asked it.
    shortcuts: Option<shortcuts::Source>,
    /// The effects on the player as the client last asked the scripts for them.
    effects: Option<statusbar::EffectList>,
    /// The connection as the client last measured it.
    link: Option<statusbar::Link>,
    notices: Vec<gameplay::Notice>,
    /// The world as the panels see it; every notice about any object; the objects gone.
    world: Option<auberean_ui_game::world::World>,
    everyone: Vec<gameplay::Notice>,
    gone: Vec<u64>,
    /// How many pixels at the screen's top and foot the panels there cover.
    preclude: (i32, i32),
    /// The inventory as the scripts answer for it, and shift held.
    inventory: Option<inventory::Facts>,
    /// The skill window's panels and skills as the scripts last answered.
    skills: Option<skill_tree::Facts>,
    /// The story's acts as the scripts last listed them, and the list's version.
    acts: Option<(u64, Vec<story_quests::Act>)>,
    /// The crafting page's facts as the scripts last answered.
    craft: Option<craft::Facts>,
    /// What the scripts last answered for the shop windows.
    shop: Option<auberean_ui_game::shop::Facts>,
    /// A scene the story page asked to play again (its act and scene), made into the story's
    /// notice from the scripts' acts at the next frame.
    play_scene: Option<(u32, u32)>,
    /// The player's options as last read, for the options window.
    options: Option<auberean_ui_game::options::Values>,
    /// The client's own preferences as last listed, for the options window.
    preferences: Option<auberean_ui_game::options::client::Listing>,
    /// The client's own preferences it changed itself since the last frame.
    preferences_changed: Vec<auberean_ui_game::options::helper::Change>,
    shift: bool,
    /// What the map draws on (made once from the region), and the game's date and time.
    atlas: Option<std::sync::Arc<auberean_ui_game::map::Atlas>>,
    clock: Option<auberean_ui_game::map::Clock>,
    game_time: f64,
    /// The program is ending.
    // D2-01656
    shutting_down: bool,
    /// What the play screen asked.
    out: Vec<gameplay::Request>,
    /// What lives across the screens before the world, what they asked, and what the client
    /// tells the one up.
    pre: auberean_ui_game::pregame::Data,
    pre_out: Vec<auberean_ui_game::pregame::Request>,
    pre_notices: Vec<auberean_ui_game::pregame::Notice>,
}

/// A screen of the flow whose panels the interface holds itself (the screens before the world):
/// its mode is up.
struct ModeScreen;

impl FlowScreen<Shared> for ModeScreen {
    fn show(&mut self, _ui: &mut Ui, _data: &mut Shared, _on: bool) {}
    fn update(&mut self, _ui: &mut Ui, _data: &mut Shared) {}
    fn close(self: Box<Self>, _ui: &mut Ui, _data: &mut Shared) {}
}

/// The play screen as the flow holds it.
#[derive(Debug)]
struct PlayScreen {
    play: GamePlay,
    logged_in: bool,
    /// The arrivals the play screen has heard of.
    arrivals: u32,
}

impl FlowScreen<Shared> for PlayScreen {
    fn show(&mut self, ui: &mut Ui, data: &mut Shared, on: bool) {
        self.play.show(ui, on, &mut data.out);
    }

    /// Once the player is in the world, the panels come; then, each tick, the panels are fed.
    fn update(&mut self, ui: &mut Ui, data: &mut Shared) {
        if data.in_world && !self.logged_in {
            self.logged_in = true;
            // D2-03994
            self.play.login_finished(ui, &mut data.out);
        }
        // The player arrived last frame, before what the scripts said this frame.
        if data.arrivals != self.arrivals {
            self.arrivals = data.arrivals;
            // D2-01944
            self.play.player_ready_to_enter_world(ui, &mut data.out);
        }
        let view = View {
            vitals: data.vitals,
            place: data.place.as_deref(),
            lines: &data.lines,
            line_infos: &data.line_infos,
            lines_before_options: data.lines_before_options,
            chat: data.chat.as_ref(),
            chat_defaults: data.chat_defaults.as_ref(),
            shortcuts: data.shortcuts.as_ref(),
            effects: data.effects.as_ref(),
            link: data.link,
            world: data.world.as_ref(),
            everyone: &data.everyone,
            gone: &data.gone,
            preclude: data.preclude,
            inventory: data.inventory.as_ref(),
            options: data.options,
            preferences: data.preferences.as_ref(),
            preferences_changed: &data.preferences_changed,
            shift: data.shift,
            skills: data.skills.as_ref(),
            acts: data.acts.as_ref().map(|(_, a)| a.as_slice()),
            craft: data.craft.as_ref(),
            shop: data.shop.as_ref(),
            atlas: data.atlas.as_ref(),
            clock: data.clock,
            game_time: data.game_time,
        };
        let notices = std::mem::take(&mut data.notices);
        self.play.tick(ui, &view, &notices, &mut data.out);
    }

    /// A message the flow was asked with: one of the client's own errors.
    fn argument(&mut self, _ui: &mut Ui, data: &mut Shared, argument: &str) {
        GamePlay::set_error_msg(argument, &mut data.out);
    }

    fn close(self: Box<Self>, ui: &mut Ui, data: &mut Shared) {
        self.play.close(ui, data.shutting_down, &mut data.out);
        display(ui, PREGAME);
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn Any> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn Any> {
        Some(self)
    }
}

/// An object's icon as its look makes it: the layers of its description's chain and its
/// appearance, each a picture and the color it is tinted by.
fn icon_layers(
    model: &auberean_client_sdk::model::Model,
    records: &auberean_world::records::WorldRecords,
    id: u64,
) -> Vec<(u32, u32)> {
    let Some(o) = model.object(id) else {
        return Vec::new();
    };
    let desc = auberean_scene::desc_of(records, &o.visual);
    let own: Vec<auberean_assets::visual::IconLayer> = o
        .visual
        .icon
        .iter()
        .map(|l| auberean_assets::visual::IconLayer {
            layer: l.0,
            picture: auberean_primitives::DataId(l.1),
            shift_color: auberean_assets::common::Color32(l.2),
        })
        .collect();
    auberean_world::look::total_icon(records, desc.parent, &own, &desc.global)
        .iter()
        .map(|l| (l.picture.0, l.shift_color.0))
        .collect()
}

/// The inventory as the panels read it, from the client's.
fn facts(
    inv: &auberean_client_sdk::runtime::inventory::Inventory,
    model: &auberean_client_sdk::model::Model,
    records: &auberean_world::records::WorldRecords,
) -> inventory::Facts {
    inventory::Facts {
        version: inv.version,
        player: inv.player,
        containers: inv
            .containers
            .iter()
            .map(|(id, c)| {
                let segments = c
                    .segments
                    .iter()
                    .map(|s| inventory::Contents {
                        items: s.items.clone(),
                        first: s.first,
                        count: s.count,
                    })
                    .collect();
                (
                    *id,
                    inventory::Container {
                        size: c.size,
                        segments,
                        slots: c.slots.clone(),
                    },
                )
            })
            .collect(),
        segments: inv.segments.clone(),
        worn: inv.worn.clone(),
        items: inv
            .items
            .iter()
            .map(|(id, i)| {
                (
                    *id,
                    inventory::ItemFacts {
                        quantity: i.quantity,
                        icon: i.icon,
                        icon_layers: icon_layers(model, records, *id),
                        stackable: i.stackable,
                        stack_size: i.stack_size,
                        name: i.name.clone(),
                        wear: i.wear,
                        preferred_location: i.preferred_location,
                        had: i.had,
                        worn: i.worn,
                        melts_at_once: i.melts_at_once,
                        offered_name: i.offered_name.clone(),
                        data_id: model.object(*id).map_or(0, |o| o.weenie.entity_did),
                    },
                )
            })
            .collect(),
        money: inv.money,
        coin_stack: inv.coin_stack,
        no_melt_question: inv.no_melt_question,
        selection: inv.selection,
        amount_name: inv.amount_name.clone(),
        picked: None,
        screen: std::collections::BTreeMap::new(),
    }
}

/// The skill window's panels and skills as the panels read them, from the client's.
fn skill_facts(s: &auberean_client_sdk::runtime::skills::Skills) -> skill_tree::Facts {
    use auberean_client_sdk::runtime::text::from_script;
    let (panel, nodes) = s.panel.as_ref().map_or((0, Vec::new()), |p| {
        let nodes = p
            .nodes
            .iter()
            .map(|n| skill_tree::Node {
                skill: n.skill,
                kind: n.kind,
                name: from_script(&n.name),
                description: from_script(&n.description),
                icon: n.icon,
                x: n.x,
                y: n.y,
                parents: n.parents.clone(),
                dependencies: n.dependencies.clone(),
                passive: n.passive,
                untrainable: n.untrainable,
                perk: n.perk,
            })
            .collect();
        (p.id, nodes)
    });
    skill_tree::Facts {
        version: s.version,
        panel,
        nodes,
        panels: s
            .panels
            .iter()
            .map(|(k, (name, icon))| (*k, (from_script(name), *icon)))
            .collect(),
        states: s
            .states
            .iter()
            .map(|(k, v)| {
                let state = skill_tree::State {
                    trained: v.trained,
                    being_untrained: v.being_untrained,
                    hero: v.hero,
                    can_train: v.can_train,
                    can_untrain: v.can_untrain,
                    can_raise: v.can_raise,
                    cost_to_train: v.cost_to_train,
                    experience_to_raise: v.experience_to_raise,
                    experience: v.experience,
                    base_level: v.base_level,
                    level: v.level,
                };
                (*k, state)
            })
            .collect(),
        roots: s.roots,
        experience: s.experience,
        credits: s.credits,
        hero_credits: s.hero_credits,
    }
}

/// What an inventory panel asks, as the client's action (none for what has no action yet).
fn inventory_action(r: inventory::Request) -> Option<Action> {
    use inventory::Request as I;
    Some(match r {
        I::Put {
            item,
            from,
            to,
            old_slot,
            new_slot,
            quantity,
        } => Action::PutItem {
            item,
            from,
            to,
            old_slot,
            new_slot,
            quantity,
        },
        I::Wear { item, location } => Action::Wear { item, location },
        I::TakeOff { item, slot } => Action::TakeOff { item, slot },
        I::IntoMoney {
            item,
            quantity,
            slot,
            from,
        } => Action::IntoMoney {
            item,
            quantity,
            slot,
            from,
        },
        I::OutOfMoney {
            amount,
            slot,
            container,
        } => Action::OutOfMoney {
            amount,
            slot,
            container,
        },
        I::TakeAll { container } => Action::TakeAll { container },
        I::MeltAll { container } => Action::MeltAll { container },
        I::Close { container } => Action::SetOpen {
            container,
            open: false,
        },
        I::Select(target) => Action::SetSelection { target },
        I::NameAmount { item, quantity } => Action::NameAmount { item, quantity },
        r @ (I::Help { .. } | I::Pick | I::PickWorld | I::ShowDoll { .. }) => {
            tracing::debug!(?r, "not carried out yet");
            return None;
        }
    })
}

/// The display set up again at `size` when it is not that size already.
fn display(ui: &mut Ui, size: (u32, u32)) {
    let s = (signed(size.0), signed(size.1));
    if ui.display() != s {
        ui.refresh(s.0, s.1);
    }
}

/// A screen not written yet: its mode up, nothing of its own.
fn make_mode(_ui: &mut Ui, _data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    Some(Box::new(ModeScreen))
}

/// The play screen: made, it sets the display to the window's size.
fn make_play(ui: &mut Ui, data: &mut Shared) -> Option<Box<dyn FlowScreen<Shared>>> {
    let play = GamePlay::new(ui, &mut data.out);
    if data.out.contains(&gameplay::Request::DisplayInWorld(true)) {
        display(ui, data.window);
    }
    Some(Box::new(PlayScreen {
        play,
        logged_in: false,
        arrivals: 0,
    }))
}

/// What makes each of the game's screens.
fn factory(m: Token) -> Option<Factory<Shared>> {
    Some(match m {
        m if m == mode::GAMEPLAY => make_play,
        m if m == mode::INTRO => pregame::make_intro,
        m if m == mode::DATA_PATCH => pregame::make_datapatch,
        m if m == mode::DISCONNECTED => pregame::make_disconnected,
        m if m == mode::CHARACTER_SELECTION => pregame::make_select,
        m if m == mode::AVATAR_SELECTION => pregame::make_avatar,
        m if m == mode::AVATAR_MODIFICATION => pregame::make_appearance,
        m if m == mode::CREDITS => pregame::make_credits,
        m if m == mode::KEYBOARD => pregame::make_keyboard,
        _ => make_mode,
    })
}

/// The retail interface.
pub struct Retail {
    ui: Ui,
    /// The records the elements draw from, with the client's string downloads once it is here.
    text: Arc<strings::TextAssets>,
    flow: GameFlow<Shared>,
    /// The window's own size.
    window: (u32, u32),
    /// Log in with this account when there is no session.
    login: Option<Login>,
    connected: bool,
    /// The developer's automatic login, driving the session while the screens follow it.
    auto: Option<DevLogin>,
    /// The server's answers about a character counted so far, and the connection as the
    /// connecting screen was last told.
    answers: u32,
    net: pregame::Net,
    /// The connecting screen was told the session's connection failure.
    failure_told: bool,
    /// How many of the session's data download events the connecting screen was told.
    ddd_told: usize,
    pointer: (i32, i32),
    /// Behaviors the figures of the screen's scene are to begin, and the frames until those asked
    /// show as played.
    behaviors: Vec<(usize, u32)>,
    behaviors_begin: u8,
    behind: Option<Arc<Canvas>>,
    middle: Option<Arc<Canvas>>,
    front: Option<Arc<Canvas>>,
    drawn: Option<(u64, (i32, i32))>,
    canvases: u64,
    /// The pictures last drawn, until no one holds them and they can be drawn on again.
    retired: Vec<Arc<Canvas>>,
    /// The layers elements are drawn into on their way to a picture.
    layers: draw::Layers,
    /// What keys asked for, carried out at the next update.
    pending: Vec<Request>,
    /// The client's random numbers (creation's choices, the media's rolls), shared with the
    /// world.
    random: auberean_world::chargen::Random,
    /// Creation's avatars, while creating.
    creation: Option<hall::Creation>,
    /// A pick asked at the pointer, carried in the next drawn scene, and awaiting its answer.
    pick: Option<(f32, f32)>,
    picking: bool,
    /// Whether the appearance screen's view frames the head.
    zoomed: bool,
    /// The cursor's picture as last handed to the host.
    cursor: Option<Arc<CursorImage>>,
    /// Shift and Ctrl held.
    mods: (bool, bool),
    /// The modifier keys held: left Shift 1, right Shift 2, left Ctrl 4, right Ctrl 8.
    held_mods: u8,
    /// Each key held for a play action: the modifier bits it was pressed with (its release ends
    /// the action its press began).
    play_keys: std::collections::HashMap<u32, u32>,
    /// The keys and pointer buttons the elements have heard held: let go when the window goes
    /// inactive.
    held_keys: std::collections::BTreeSet<u32>,
    held_buttons: std::collections::BTreeSet<u32>,
    /// How many entry refusals the screens have shown.
    refusals: u32,
    /// The client's own error lines for the chat windows, shown at the next frame.
    errors: Vec<String>,
    /// What the chat panel reads of the client and asks of it.
    chat: chat::ChatClient,
    /// The session had the player in the world, or was connected (for its end).
    was_in_world: bool,
    was_connected: bool,
    /// The chat service refused the login's ticket, and the screens said so.
    refused: bool,
    /// The dialogs, one up at a time, and their notices for the panels (their answers, mouse
    /// look waiting on them), handed on at the next frame.
    dialogs: Dialogs,
    said: Vec<gameplay::Notice>,
    /// The play screen's kept places, to keep or put back at the next frame.
    places: places::Places,
    /// The world gathered for the panels, and the notices of the last frame (for the next
    /// frame's questions of the scripts).
    gather: world::Gather,
    last_notices: Vec<gameplay::Notice>,
    /// The scripts' notices of changed radar looks, heard by the panels at the next frame.
    held_looks: Vec<gameplay::Notice>,
    /// The player's options read for the options window.
    options: options::Reader,
    /// The client's own preferences, for the options window's client page.
    preferences: preferences::Preferences,
    /// The left button went to the world's controls as well (mouse look runs with it).
    left_to_world: bool,
    /// The pointer when the left button was last pressed or let go, when it was last let go
    /// after a single press, and whether the press held is a double press.
    click_at: (i32, i32),
    released_at: Option<f64>,
    double_held: bool,
    /// The inventory panels' search of the world under the pointer: asked and not yet answered,
    /// and what the last one found (`Some(0)`: nothing; none while one is under way).
    world_search: bool,
    world_found: Option<u64>,
    /// The panels' viewports that show the player's figure.
    dolls: doll::Dolls,
    /// The paper doll's search of its figure under the pointer: the pointer, until its scene is
    /// drawn with it; then that scene's place among the frame's scenes, until it is answered.
    doll_pick: Option<(f32, f32)>,
    doll_scene: Option<usize>,
    /// Where the host's input method was last told to write.
    input_method: Option<InputArea>,
}

impl std::fmt::Debug for Retail {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Retail")
            .field("screen", &self.screen())
            .field("mode", &self.flow.flow.mode())
            .finish_non_exhaustive()
    }
}

#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)]
fn signed(v: u32) -> i32 {
    v.min(i32::MAX as u32) as i32
}

#[allow(clippy::cast_sign_loss)]
fn unsigned((w, h): (i32, i32)) -> (u32, u32) {
    (w.max(0) as u32, h.max(0) as u32)
}

impl Retail {
    /// The interface over these records, for a window `size` pixels. With `login`, it logs in
    /// when there is no session.
    #[must_use]
    pub fn new(
        assets: Arc<dyn Assets + Send + Sync>,
        size: (u32, u32),
        login: Option<Login>,
    ) -> Self {
        let text = Arc::new(strings::TextAssets::new(assets));
        let mut ui = Ui::new(text.clone(), signed(PREGAME.0), signed(PREGAME.1));
        let flow = GameFlow::new(&mut ui, Shared::default(), factory);
        Self {
            ui,
            text,
            flow,
            window: size,
            login,
            connected: false,
            auto: None,
            answers: 0,
            net: pregame::Net::Idle,
            failure_told: false,
            ddd_told: 0,
            pointer: (0, 0),
            behaviors: Vec::new(),
            behaviors_begin: 0,
            behind: None,
            middle: None,
            front: None,
            drawn: None,
            canvases: 0,
            retired: Vec::new(),
            layers: draw::Layers::default(),
            pending: Vec::new(),
            random: auberean_world::chargen::Random::default(),
            creation: None,
            pick: None,
            picking: false,
            zoomed: false,
            cursor: None,
            mods: (false, false),
            held_mods: 0,
            play_keys: std::collections::HashMap::new(),
            held_keys: std::collections::BTreeSet::new(),
            held_buttons: std::collections::BTreeSet::new(),
            refusals: 0,
            errors: Vec::new(),
            chat: chat::ChatClient::default(),
            was_in_world: false,
            was_connected: false,
            refused: false,
            input_method: None,
            dialogs: Dialogs::default(),
            said: Vec::new(),
            places: places::Places::default(),
            gather: world::Gather::default(),
            last_notices: Vec::new(),
            held_looks: Vec::new(),
            options: options::Reader::default(),
            preferences: preferences::Preferences::default(),
            left_to_world: false,
            click_at: (0, 0),
            released_at: None,
            double_held: false,
            world_search: false,
            world_found: None,
            dolls: doll::Dolls::default(),
            doll_pick: None,
            doll_scene: None,
        }
    }

    /// Let the developer's automatic login drive the session; the screens follow it.
    #[must_use]
    pub fn with_auto(mut self, auto: DevLogin) -> Self {
        self.auto = Some(auto);
        self
    }

    /// The gore level and blood color the command line forces (0: the language's).
    #[must_use]
    pub fn with_forced_gore(mut self, gore: u32, blood: u32) -> Self {
        self.preferences.force_tops(gore, blood);
        self
    }

    /// The characters the command line named: the one to play and the one to make and play. The
    /// first the account's list has goes into the world as soon as the list comes.
    #[must_use]
    pub fn with_start_character(mut self, user: Option<&str>, create: Option<&str>) -> Self {
        let pre = &mut self.flow.flow.data.pre;
        user.unwrap_or_default()
            .clone_into(&mut pre.start_character);
        create
            .unwrap_or_default()
            .clone_into(&mut pre.create_character);
        self
    }

    /// The creation screen offers the administrator's button; the server decides whether the
    /// account may have such a character.
    #[must_use]
    pub fn with_admin_client(mut self, on: bool) -> Self {
        self.flow.flow.data.pre.admin_client = on;
        self
    }

    /// The element tree, to look at (a test, a screenshot's caption).
    #[must_use]
    pub const fn ui(&self) -> &Ui {
        &self.ui
    }

    /// The element tree, to drive by hand.
    pub fn ui_mut(&mut self) -> &mut Ui {
        &mut self.ui
    }

    /// The screen's size: the 1024 by 768 the screens before the world are laid out on, or the
    /// window's in the world.
    fn size(&self) -> (u32, u32) {
        unsigned(self.ui.display())
    }

    /// Which screen is up: `"nothing"`, `"intro"`, `"datapatch"`, `"disconnected"`,
    /// `"select"`, `"avatar"`, `"appearance"`, `"name"` (the appearance with the name's panel
    /// up), `"keyboard"`, `"credits"`, `"play"`.
    #[must_use]
    pub fn screen(&self) -> &'static str {
        use auberean_ui_game::{datapatch, disconnected, intro};
        let m = self.flow.flow.mode();
        if m == mode::GAMEPLAY {
            return "play";
        }
        if self.pre::<intro::Intro>().is_some() {
            "intro"
        } else if self.pre::<datapatch::DataPatch>().is_some() {
            "datapatch"
        } else if self.pre::<disconnected::Disconnected>().is_some() {
            "disconnected"
        } else if self.pre::<CharacterSelect>().is_some() {
            "select"
        } else if self.pre::<AvatarSelection>().is_some() {
            "avatar"
        } else if let Some(a) = self.pre::<AvatarModification>() {
            if a.confirmation().is_some() {
                "name"
            } else {
                "appearance"
            }
        } else if self.pre::<Credits>().is_some() {
            "credits"
        } else if m == mode::KEYBOARD {
            "keyboard"
        } else {
            "nothing"
        }
    }

    /// The key map's rows while it is up: each action's name and its two keys, in turn.
    #[must_use]
    pub fn key_map_rows(&self) -> Vec<String> {
        self.pre::<auberean_ui_game::keyboard::Keyboard>()
            .map(|k| k.rows(&self.ui))
            .unwrap_or_default()
    }

    /// The flow's mode up.
    #[must_use]
    pub fn mode(&self) -> Token {
        self.flow.flow.mode()
    }

    /// The world as the panels last heard it, while the player is in one.
    #[must_use]
    pub fn world(&self) -> Option<&auberean_ui_game::world::World> {
        self.flow.flow.data().world.as_ref()
    }

    /// The play screen, while it is up.
    #[must_use]
    pub fn play(&self) -> Option<&GamePlay> {
        let s = self.flow.flow.current()?;
        s.as_any()?.downcast_ref::<PlayScreen>().map(|p| &p.play)
    }

    fn play_mut(&mut self) -> Option<&mut GamePlay> {
        let (s, _) = self.flow.flow.current_mut();
        s?.as_any_mut()?
            .downcast_mut::<PlayScreen>()
            .map(|p| &mut p.play)
    }

    fn toolbar(&self) -> Option<&Toolbar> {
        self.play()?.child::<Toolbar>()
    }

    /// The chat panel, while playing.
    #[must_use]
    pub fn chat(&self) -> Option<&ChatUi> {
        self.toolbar()?.child::<ChatUi>()
    }

    /// The first chat window's log, oldest first, while playing.
    #[must_use]
    pub fn chat_lines(&self) -> Vec<String> {
        self.chat()
            .and_then(|c| c.windows().first())
            .map(|w| w.lines(&self.ui))
            .unwrap_or_default()
    }

    /// The status bar's vitals as shown, while playing.
    #[must_use]
    pub fn vitals_shown(&self) -> Option<[u32; 4]> {
        self.play()?.child::<StatusBar>()?.vitals_shown()
    }

    /// The war button lit (attacking, as the player's scripts last said), while playing.
    #[must_use]
    pub fn attacking(&self) -> Option<bool> {
        Some(self.toolbar()?.child::<Shortcuts>()?.attacking())
    }

    /// Ask for a screen: it comes up at the next tick of the clock.
    fn go(&mut self, m: Token) {
        self.flow.queue_mode(m, "");
    }

    /// The character logged off while the play screen is up: back to the list.
    fn pregame(&mut self, requests: &mut Vec<Request>) {
        let (now, queued) = (self.flow.flow.mode(), self.flow.flow.queued_mode());
        if now == mode::GAMEPLAY && queued == Token(0) {
            let mut out = Vec::new();
            if let Some(p) = self.play_mut() {
                p.update(&mut out);
            }
            self.carry_out(out, requests);
        }
    }

    /// What the play screen is given this frame: the player's vitals, the radar's place line, the
    /// chat window's lines (the scripts' and the client's own errors), the scripts' notices.
    fn feed(&mut self, game: &Game<'_>) {
        let window = self.scale();
        let d = &mut self.flow.flow.data;
        d.in_world = game.phase == Phase::World;
        d.arrivals = game.arrivals;
        d.window = game.size;
        d.lines.clear();
        d.line_infos.clear();
        d.notices.clear();
        let player = game.client.and_then(|c| c.model.player_object());
        d.vitals = player.and_then(vitals);
        // The radar's place line: inside a building or dungeon (a cell of a block's own cells), the
        // place strings' "Inside"; outdoors the map coordinates (not shown yet: they need the
        // strings' formatting of numbers).
        // D2-13199
        if let Some(cell) = player.and_then(|o| o.place.map(|p| p.cell)) {
            d.place = Some(if cell & 0xFFFF >= 0x100 {
                self.ui
                    .assets()
                    .string(PLACE_STRINGS, Token::of("Inside").raw())
                    .unwrap_or_default()
            } else {
                String::new()
            });
        }
        for e in self.errors.drain(..) {
            d.lines.push((chat_panel::text_type::ERROR, e));
            d.line_infos.push(None);
        }
        let own_lines = d.lines.len();
        for s in game.shown {
            d.lines.push((s.kind, s.text.clone()));
            d.line_infos.push(s.info.clone());
        }
        let player_id = game.client.and_then(|c| c.model.player);
        let options_said = game.notices.iter().position(|n| {
            (n.native == chat_panel::notice::RELOAD_CHAT_SETTINGS
                || n.native == gameplay::notice::RELOAD_OPTIONS)
                && (n.about.is_none() || n.about == player_id)
        });
        d.lines_before_options = own_lines
            + game
                .shown
                .iter()
                .take_while(|s| options_said.is_none_or(|at| s.notices_before <= at))
                .count();
        d.chat = self.chat.settings().cloned();
        if d.chat_defaults.is_none() {
            d.chat_defaults = Some(chat::defaults());
        }
        if let Some(c) = game.client {
            let t = c.toolbar();
            if d.shortcuts.as_ref().map(|s| s.version) != Some(t.version) {
                d.shortcuts = Some(shortcut_source(t, &c.model, &game.world.records));
                tracing::debug!(
                    version = t.version,
                    shortcuts = t.shortcuts.len(),
                    set = t.set,
                    height = t.height,
                    "the toolbar from the scripts"
                );
            }
        }
        if let Some(e) = game.client.map(auberean_client_sdk::Client::effects) {
            if d.effects.as_ref().map(|l| l.version) != Some(e.version) {
                d.effects = Some(effect_list(e));
            }
        }
        d.link = game
            .client
            .and_then(auberean_client_sdk::Client::heartbeat)
            .map(|h| statusbar::Link {
                beat: h.count,
                latency: h.latency,
                loss: h.loss,
                bytes_in: h.bytes_in,
                bytes_out: h.bytes_out,
                span: h.span,
                since_heard: h.since_heard,
            });
        // An object's radar look (and with it its name) changed: the gathered world reads them
        // again before the next frame, so the panels hear it then, with the world that has them.
        let held = std::mem::take(&mut self.held_looks);
        let (looks, now): (Vec<_>, Vec<_>) = game
            .notices
            .iter()
            .partition(|n| n.native == world::notice::CHANGE_RADAR_LOOK);
        let heard: Vec<gameplay::Notice> = held
            .into_iter()
            .chain(now.into_iter().map(|n| gameplay::Notice {
                native: n.native,
                about: n.about,
                words: n.words.clone(),
                records: records::of_notice(n),
            }))
            .collect();
        d.notices.extend(
            heard
                .iter()
                .filter(|n| n.about.is_none() || n.about == player_id)
                .cloned(),
        );
        for n in game.notices {
            if n.native == gameplay::notice::UPDATE_MOUNT_STATE && n.about == player_id {
                self.dolls
                    .set_mounted(n.words.first().is_some_and(|w| *w != 0));
            }
        }
        self.held_looks = looks
            .into_iter()
            .map(|n| gameplay::Notice {
                native: n.native,
                about: n.about,
                words: n.words.clone(),
                records: records::of_notice(n),
            })
            .collect();
        d.everyone = heard
            .into_iter()
            .map(|mut n| {
                if n.native == names::notice::UPDATE_SELECTION_INFO {
                    n.words = world::selection_info(game.client, n.words.first().copied());
                }
                n
            })
            .collect();
        self.last_notices = d
            .everyone
            .iter()
            .filter(|n| n.native != world::notice::CHANGE_RADAR_LOOK)
            .chain(&self.held_looks)
            .cloned()
            .collect();
        self.chat.heard(&d.notices);
        d.shift = self.mods.0;
        if let Some(c) = game.client {
            let inv = c.inventory();
            if d.inventory
                .as_ref()
                .is_none_or(|f| f.version != inv.version || f.player != inv.player)
            {
                d.inventory = Some(facts(inv, &c.model, &game.world.records));
            }
            // The world under the pointer, for the inventory panels' search.
            if self.world_search {
                if let Some(p) = game.world_pick {
                    self.world_search = false;
                    self.world_found = Some(p.unwrap_or(0));
                }
            }
            // The figure under the pointer in the paper doll: the kind of mesh met there.
            if let Some(scene) = self.doll_scene {
                if let Some(&(_, hit)) = game.picked.iter().find(|(s, _)| *s == scene) {
                    self.doll_scene = None;
                    self.world_found = Some(u64::from(hit.map_or(0, |h| h.mesh_type)));
                }
            }
            if let Some(f) = d.inventory.as_mut() {
                f.picked = self.world_found;
                // Where the containers opening stand on the screen, for their windows to open
                // over them.
                f.screen.clear();
                let (w, h) = (game.size.0 as f32, game.size.1 as f32);
                for o in d
                    .everyone
                    .iter()
                    .filter(|n| n.native == inventory::notice::CONTAINER_STATE_UPDATE)
                    .filter_map(|n| n.about)
                {
                    if let Some((x, y)) = game.seen.place_on_screen(o, w, h) {
                        let (left, top, scale) = window;
                        f.screen.insert(
                            o,
                            (
                                to_i32(((x - left) / scale).floor()),
                                to_i32(((y - top) / scale).floor()),
                            ),
                        );
                    }
                }
            }
            if d.skills
                .as_ref()
                .is_none_or(|f| f.version != c.skills.version)
            {
                d.skills = Some(skill_facts(&c.skills));
            }
            let a = c.acts();
            if a.version != 0 && d.acts.as_ref().is_none_or(|(v, _)| *v != a.version) {
                d.acts = Some((a.version, a.acts.iter().map(story_act).collect()));
            }
            let k = c.craft();
            let now = c.game_time();
            match d.craft.as_mut() {
                Some(f) if f.version == k.version => f.now = now,
                _ => d.craft = Some(craft_facts(k, now)),
            }
            let st = c.store();
            if d.shop.as_ref().is_none_or(|f| f.version != st.version) {
                d.shop = Some(auberean_ui_game::shop::Facts {
                    version: st.version,
                    max_purchase: st.max_purchase.clone(),
                    consignment_charge: st.consignment_charge,
                });
            }
            // The story page's scene played again: the client's own notice to the play screen,
            // with the scene from the scripts' acts.
            if let Some((act, scene)) = d.play_scene.take() {
                let found = c
                    .acts()
                    .acts
                    .iter()
                    .filter(|a| a.id == act)
                    .flat_map(|a| a.scenes.iter())
                    .find(|s| s.id == scene && s.playable != 0)
                    .cloned();
                if let Some(s) = found {
                    d.notices.push(gameplay::Notice {
                        native: gameplay::notice::PLAY_SEQUENCE,
                        about: player_id,
                        records: vec![records::of(&Some(
                            auberean_client_sdk::script::native_object::NativeObject::Scenes(vec![
                                s,
                            ]),
                        ))],
                        ..gameplay::Notice::default()
                    });
                }
            }
        } else {
            d.inventory = None;
            d.skills = None;
            d.acts = None;
            d.craft = None;
            d.shop = None;
        }
        let ui = &self.ui;
        let write = |info: &auberean_client_sdk::protocol::text::StringInfo| {
            ui.assets()
                .write(&auberean_client_sdk::runtime::text::from_wire(info))
                .unwrap_or_default()
        };
        if let Some(p) = game.world_pick {
            tracing::debug!(picked = ?p, "the world under the pointer");
        }
        if d.atlas.is_none() && game.phase == Phase::World {
            d.atlas = Some(std::sync::Arc::new(map::atlas(game.world)));
        }
        d.clock = game
            .client
            .and_then(|c| map::clock(game.world, c.world_time()));
        d.game_time = game.client.map_or(0.0, |c| c.game_time());
        match self.gather.world(game, &write, &**ui.assets()) {
            Some((w, gone)) => {
                d.world = Some(w);
                d.gone = gone;
            }
            None => {
                d.world = None;
                d.gone.clear();
            }
        }
    }

    /// How many pixels at the screen's top and foot the panels there cover.
    #[must_use]
    pub fn preclude(&self) -> (i32, i32) {
        self.flow.flow.data.preclude
    }

    /// What the play screen asked, carried out as the client's actions, the camera's settings and
    /// the flow's screens. Every kind of request has its arm.
    pub fn carry_out(&mut self, out: Vec<gameplay::Request>, requests: &mut Vec<Request>) {
        use gameplay::Request as R;
        for r in out {
            match r {
                R::Camera(camera::Request::MouseLook(on)) => {
                    requests.push(Request::Camera(CameraRequest::MouseLook(on)));
                }
                R::Camera(camera::Request::FirstPerson(on)) => {
                    requests.push(Request::Camera(CameraRequest::FirstPerson(on)));
                }
                R::Camera(camera::Request::Zoom { inward }) => {
                    requests.push(Request::Camera(CameraRequest::Zoom { inward }));
                }
                // The camera's movement (its arrows, its left button in mouse look), as keys.
                R::Camera(camera::Request::Act { action, extent }) => {
                    requests.push(Request::Move { action, extent });
                }
                R::DisplayInWorld(_) => {}
                R::Attack { start } => requests.push(Request::Act(Action::Attack { start })),
                R::Examination { tabs } => {
                    requests.push(Request::Act(Action::Examination { tabs }));
                }
                R::CancelQuest { quest } => {
                    requests.push(Request::Act(Action::CancelQuest { quest }));
                }
                R::UseSelected => requests.push(Request::Act(Action::UseSelected)),
                R::UseSlot(slot) => requests.push(Request::Act(Action::UseToolbarSlot { slot })),
                R::PutInSlot { slot, shortcut } => {
                    let shortcut = ToolbarShortcut {
                        kind: shortcut.kind,
                        item: shortcut.item,
                        data_enum: shortcut.data_enum,
                        data_id: shortcut.data_id,
                        text: shortcut.text.encode_utf16().collect(),
                    };
                    requests.push(Request::Act(Action::SetToolbarSlot { slot, shortcut }));
                }
                R::ShortcutSet(set) => requests.push(Request::Act(Action::ToolbarSet { set })),
                R::Select(target) => {
                    tracing::debug!(target = format_args!("{target:#x}"), "select");
                    requests.push(Request::Act(Action::Select { target }));
                }
                R::SetSelection(target) => {
                    requests.push(Request::Act(Action::SetSelection { target }));
                }
                // The play screen's display is the window's own size.
                #[allow(clippy::cast_precision_loss)]
                R::Pick { x, y } => requests.push(Request::PickWorld {
                    x: x as f32,
                    y: y as f32,
                }),
                R::CeaseAttack(id) => self.gather.asks.push(world::Ask::CeaseAttack(id)),
                R::PrecludeFromBottom(n) => self.flow.flow.data.preclude.1 = n,
                R::ShowCoordinates(on) => {
                    requests.push(Request::Act(Action::ShowRadarCoordinates { on }));
                    self.gather.options_changed();
                    self.options.stale = true;
                }
                R::ShowNames(on) => {
                    self.gather.asks.push(world::Ask::ShowNames(on));
                    self.options.quiet = true;
                }
                R::Options(r) => match options::action(&r) {
                    Some(a) => {
                        requests.push(Request::Act(a));
                        // The bar's rows set by hand: the page set its own row already.
                        // D2-02831
                        let told = !matches!(
                            r,
                            auberean_ui_game::options::Request::Set { tell: false, .. }
                        );
                        self.options.stale |= told;
                        self.options.quiet |= !told;
                    }
                    None => match &r {
                        auberean_ui_game::options::Request::Preferences(p) => {
                            self.preferences.handle(p);
                            if let Some(g) = self.preferences.take_applied() {
                                requests.push(Request::Graphics(g));
                            }
                            if let Some(o) = self.preferences.take_camera() {
                                requests.push(Request::Camera(CameraRequest::Options(o)));
                            }
                            if let Some(p) = self.preferences.take_presentation() {
                                requests.push(Request::Presentation(p));
                            }
                            if let Some(kept) = self.preferences.take_kept() {
                                requests.push(Request::KeepPreferences(kept));
                            }
                        }
                        auberean_ui_game::options::Request::AssistanceSite => {
                            requests.push(Request::OpenAssistancePage);
                        }
                        o @ auberean_ui_game::options::Request::Assistance { .. } => {
                            tracing::debug!(?o, "no assistance service yet");
                        }
                        auberean_ui_game::options::Request::Set { .. } => {}
                    },
                },
                R::KeepOptions => requests.push(Request::Act(Action::KeepOptions)),
                R::LogOff => requests.push(Request::Act(Action::LogOff)),
                R::Quit => requests.push(Request::Quit),
                R::CharacterSelection => self.go(mode::CHARACTER_SELECTION),
                R::ErrorLine(text) => self.errors.push(text),
                R::Chat(a) => self.chat.ask(a),
                R::KnowCommands => self.chat.know_commands(),
                R::SaveLocations => {
                    if let Some(f) = self.play().map(GamePlay::frame) {
                        self.places.save(&self.ui, f);
                    }
                }
                R::PutBackLocations { reloaded } => self.places.put_back(reloaded),
                R::SaveLayout { name } => self.places.save_layout(name),
                R::LoadLayout { name } => self.places.load_layout(name),
                R::Dialog(d) => {
                    let parent = self.play().map(GamePlay::frame);
                    self.dialogs
                        .make_dialog(&mut self.ui, parent, &(&d).into(), &mut self.said);
                }
                R::DialogDown(id) => self.dialogs.close_dialog(&mut self.ui, id, &mut self.said),
                // The search under the pointer in the world, answered in the next frame's facts.
                R::Inventory(inventory::Request::PickWorld) => {
                    let (x, y) = Self::at(self.pointer);
                    self.world_search = true;
                    self.world_found = None;
                    if let Some(f) = self.flow.flow.data.inventory.as_mut() {
                        f.picked = None;
                    }
                    requests.push(Request::PickWorld { x, y });
                }
                R::Inventory(inventory::Request::ShowDoll { viewport, portrait }) => {
                    self.dolls.show(viewport, portrait);
                }
                // The search of the paper doll's figure under the pointer, answered by its scene.
                R::Inventory(inventory::Request::Pick) => {
                    self.world_search = false;
                    self.world_found = None;
                    if let Some(f) = self.flow.flow.data.inventory.as_mut() {
                        f.picked = None;
                    }
                    self.doll_pick = Some(Self::at(self.pointer));
                    self.doll_scene = None;
                }
                R::Inventory(r) => {
                    if let Some(a) = inventory_action(r) {
                        requests.push(Request::Act(a));
                    }
                }
                R::Skill(r) => {
                    use auberean_client_sdk::contract::SkillAction as A;
                    use skill_tree::SkillRequest as S;
                    requests.push(Request::Act(Action::Skill(match r {
                        S::Train(s) => A::Train(s),
                        S::Untrain(s) => A::Untrain(s),
                        S::Raise(s) => A::Raise(s),
                        S::CancelUntrain => A::CancelUntrain,
                    })));
                }
                R::Craft(r) => {
                    use auberean_client_sdk::contract::CraftAction as A;
                    requests.push(Request::Act(Action::Craft(match r {
                        craft::CraftRequest::Open(d) => A::Open(d),
                        craft::CraftRequest::RaiseCraftSkill(d) => A::RaiseCraftSkill(d),
                    })));
                }
                R::Ingredients(r) => {
                    use auberean_client_sdk::contract::CraftAction as A;
                    use auberean_ui_game::ingredients::IngredientRequest as I;
                    requests.push(Request::Act(Action::Craft(match r {
                        I::Inquire { recipe, spinner } => A::Inquire { recipe, spinner },
                        I::Add {
                            recipe,
                            ordinal,
                            item,
                            spinner,
                        } => A::AddIngredient {
                            recipe,
                            ordinal,
                            item,
                            spinner,
                        },
                        I::Execute {
                            recipe,
                            spinner,
                            ingredients,
                        } => A::ExecuteCraft {
                            recipe,
                            spinner,
                            ingredients,
                        },
                    })));
                }
                R::Trade(r) => {
                    use auberean_client_sdk::contract::TradeAction as A;
                    use auberean_ui_game::trade::TradeRequest as T;
                    requests.push(Request::Act(Action::Trade(match r {
                        T::Open(p) => A::Open(p),
                        T::Close => A::Close,
                        T::Accept => A::Accept,
                        T::Decline => A::Decline,
                        T::Offer { item, quantity } => A::Offer { item, quantity },
                        T::Revoke(i) => A::Revoke(i),
                    })));
                }
                R::Shop(r) => {
                    use auberean_client_sdk::contract::StoreAction as A;
                    use auberean_ui_game::shop::ShopRequest as S;
                    let a = match r {
                        S::Purchase {
                            item,
                            quantity,
                            slot,
                        } => A::Purchase {
                            item,
                            quantity,
                            slot,
                        },
                        S::Confirm(y) => A::Confirm(y),
                        S::LeaveStore => A::LeaveStore,
                        S::LeaveCatalog => A::LeaveCatalog,
                        S::LeaveConsignment => A::LeaveConsignment,
                        S::EnterConsignment => A::EnterConsignment,
                        S::EnterShop(id) => A::EnterShop(id),
                        S::NextSales => A::NextSales,
                        S::PrevSales => A::PrevSales,
                        S::Sales(n) => A::Sales(n),
                        S::Restock {
                            item,
                            quantity,
                            sale,
                        } => A::Restock {
                            item,
                            quantity,
                            sale,
                        },
                        S::Sell {
                            item,
                            quantity,
                            price,
                        } => A::Sell {
                            item,
                            quantity,
                            price,
                        },
                        S::Collect { sale, sold } => A::Collect { sale, sold },
                    };
                    requests.push(Request::Act(Action::Store(a)));
                }
                R::Pet(r) => {
                    use auberean_client_sdk::contract::PetAction as A;
                    use auberean_ui_game::pet::PetRequest as P;
                    requests.push(Request::Act(Action::Pet(match r {
                        P::Attack(t) => A::Attack(t),
                        P::Return => A::Return,
                        P::Buff => A::Buff,
                        P::Heal => A::Heal,
                        P::Mode { pet, mode } => A::Mode { pet, mode },
                        P::Name { pet, name } => A::Name { pet, name },
                    })));
                }
                R::PlayScenesDone(scenes) => {
                    requests.push(Request::Act(Action::PlayScenesDone { scenes }));
                }
                R::ShowSales => requests.push(Request::Act(Action::SaleReminders)),
                R::StopScreenSound => requests.push(sounds::screens_silence()),
                R::Sound(id) => requests.push(sounds::once(id)),
                R::StorySound(s) => requests.extend(sounds::story(s)),
                R::PlayScene { act, scene } => self.flow.flow.data.play_scene = Some((act, scene)),
                R::Examine(e) => {
                    requests.push(Request::Act(Action::Examine {
                        kind: e.kind,
                        data_enum: e.data_enum,
                        item: e.item,
                        data_id: e.data_id,
                        selection: e.selection,
                    }));
                }
                R::Social(r) => requests.push(Request::Act(map::social_action(r))),
                R::Resurrect => requests.push(Request::Act(Action::Resurrect)),
                R::DeclineResurrection => {
                    requests.push(Request::Act(Action::DeclineResurrection));
                }
                R::Dismount => requests.push(Request::Act(Action::Dismount)),
                R::Confirm { objects, value } => {
                    requests.push(Request::Act(Action::ApplyTotem {
                        target: objects[0],
                        source: objects[1],
                        effect: value,
                    }));
                }
                R::GrooveLevel(level) => requests.push(Request::Sound(
                    auberean_shell::interface::SoundOrder::Groove(level),
                )),
                // The examination panel opens on the selection: the client's own notice.
                R::OpenExaminationPanel => self.said.push(gameplay::Notice {
                    native: auberean_ui_game::inventory::notice::OPEN_EXAMINATION,
                    ..gameplay::Notice::default()
                }),
                R::ShowWorld(on) => requests.push(Request::ShowWorld(on)),
                R::PortalScenePlaying(on) => requests.push(Request::PortalScenePlaying(on)),
                R::PrecludeFromTop(n) => self.flow.flow.data.preclude.0 = n,
                // The play screen takes these itself, before they would come here.
                R::Notice(_) | R::ShortcutRows(_) => {}
                // Windows of their own, not made yet.
                r @ (R::Help { .. } | R::PluginManager { .. }) => {
                    tracing::debug!(?r, "no window for this yet");
                }
            }
        }
    }

    /// The dialogs' notices: what the dialog up heard (its answer), and those of the last frame,
    /// handed to the screen before the world up (a question's answer) and to the play screen's
    /// panels.
    /// The chat service's news from the client: its connection lost, the ticket taken again (or
    /// not) after it came back, the login's ticket refused (the session goes, the screens say why).
    fn chat_service(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        use auberean_client_sdk::runtime::friends::{notice, Refusal};
        for n in game.notices {
            match n.native {
                notice::CONNECTION_LOST => self.flow.chat_service_lost(&mut self.ui),
                notice::AUTHENTICATED => {
                    let taken = n.words.first() == Some(&1);
                    self.flow.chat_service_reauthenticated(&mut self.ui, taken);
                }
                _ => {}
            }
        }
        let refusal = game
            .client
            .and_then(auberean_client_sdk::Client::friends_refusal);
        match refusal {
            Some(r) if !self.refused => {
                self.refused = true;
                self.flow
                    .chat_service_refused(&self.ui, r == Refusal::Banned);
                // The session's end is the refusal's, not a connection that failed.
                self.net = pregame::Net::Idle;
                requests.push(Request::Disconnect);
            }
            None => self.refused = false,
            Some(_) => {}
        }
    }

    fn dialog_notices(&mut self) {
        self.dialogs.listen(&mut self.ui, &mut self.said);
        let said = std::mem::take(&mut self.said);
        let d = &mut self.flow.flow.data;
        for n in &said {
            if let Some((id, yes)) = dialog::confirmation_closed(n) {
                d.pre_notices
                    .push(auberean_ui_game::pregame::Notice::ConfirmationClosed { id, yes });
            }
        }
        d.notices.extend(said);
    }

    /// A pointer position in window pixels.
    #[allow(clippy::cast_precision_loss)]
    const fn at(p: (i32, i32)) -> (f32, f32) {
        (p.0 as f32, p.1 as f32)
    }

    /// The window's pixels per pixel of the screen.
    #[allow(clippy::cast_precision_loss)]
    fn scale(&self) -> (f32, f32, f32) {
        let size = self.size();
        let [left, top, width, _] = auberean_shell::interface::fitted(size, self.window);
        (left, top, width / size.0.max(1) as f32)
    }

    /// The host's input method follows the entry that has it on: the part of the entry that
    /// shows, in the window's pixels, told to the host when it changes (none once it is off).
    fn follow_input_method(&mut self, requests: &mut Vec<Request>) {
        let (left, top, scale) = self.scale();
        let area = self.ui.ime().map(|ime| {
            let r = ime.area;
            #[allow(clippy::cast_precision_loss)]
            let (x0, y0, x1, y1) = (
                to_i32((left + r.x0 as f32 * scale).floor()),
                to_i32((top + r.y0 as f32 * scale).floor()),
                to_i32((left + r.x1 as f32 * scale).ceil()),
                to_i32((top + r.y1 as f32 * scale).ceil()),
            );
            InputArea {
                x: x0,
                y: y0,
                width: u32::try_from(x1 - x0).unwrap_or(0),
                height: u32::try_from(y1 - y0).unwrap_or(0),
            }
        });
        if area != self.input_method {
            self.input_method = area;
            requests.push(Request::InputMethod(area));
        }
    }

    /// A point of the window in the screen's pixels.
    fn to_screen(&self, x: f32, y: f32) -> (i32, i32) {
        let (left, top, scale) = self.scale();
        (
            to_i32(((x - left) / scale).floor()),
            to_i32(((y - top) / scale).floor()),
        )
    }

    /// The cursor the interface shows, as a picture for the host.
    fn cursor_image(&mut self) -> Option<Arc<CursorImage>> {
        let c = self.ui.cursor()?;
        if self.cursor.as_ref().is_some_and(|i| i.picture == c.picture) {
            return self.cursor.clone();
        }
        let img = self.ui.assets().image(c.picture)?;
        let rgba = img
            .bgra
            .as_chunks::<4>()
            .0
            .iter()
            .flat_map(|p| [p[2], p[1], p[0], p[3]])
            .collect();
        let made = Arc::new(CursorImage {
            picture: c.picture,
            width: img.width,
            height: img.height,
            rgba,
            hot_x: u32::try_from(c.hot_x).unwrap_or(0),
            hot_y: u32::try_from(c.hot_y).unwrap_or(0),
        });
        self.cursor = Some(Arc::clone(&made));
        Some(made)
    }

    /// The depth the interface's picture splits at: a scene shown in a viewport stands at the
    /// viewport's depth, what is deeper drawn behind it (the scene's own viewport when the screen
    /// draws one, else the nearest viewport). In the world, none: the world fills the window
    /// behind every element, so nothing may be drawn behind it (a panel's viewport, such as the
    /// paper doll's, would put the panels deeper than it behind the world).
    #[must_use]
    pub fn scene_depth(&self) -> Option<i32> {
        if self.flow.flow.mode() == mode::GAMEPLAY {
            return None;
        }
        self.pregame_viewport()
            .and_then(|v| self.ui.node(v).map(|n| n.z))
            .or_else(|| {
                draw::viewports(&self.ui)
                    .iter()
                    .filter_map(|(v, _)| self.ui.node(*v).map(|n| n.z))
                    .min()
            })
    }

    /// The depth the interface's picture over the world splits at: in the world, a scene shown in
    /// a viewport (the paper doll's figure) stands at its viewport's depth, the elements deeper
    /// than it drawn over the world and under the scene (the nearest viewport's, when several
    /// show). Before the world, none.
    #[must_use]
    pub fn view_depth(&self) -> Option<i32> {
        if self.flow.flow.mode() != mode::GAMEPLAY {
            return None;
        }
        draw::viewports(&self.ui)
            .iter()
            .filter_map(|(v, _)| self.ui.node(*v).map(|n| n.z))
            .min()
    }

    /// Redraw the pictures when the tree changed.
    fn redraw(&mut self) {
        self.ui.lay_out_text_shown();
        let key = (self.ui.revision(), self.ui.display());
        if self.drawn == Some(key) {
            return;
        }
        self.drawn = Some(key);
        let (w, h) = self.ui.display();
        let size = self.size();
        let scene_z = self.scene_depth();
        let view_z = self.view_depth();
        // The pictures no one holds any more are drawn on again.
        let (idle, held): (Vec<_>, Vec<_>) = std::mem::take(&mut self.retired)
            .into_iter()
            .partition(|c| Arc::strong_count(c) == 1);
        self.retired = held;
        let mut spare: Vec<Vec<u8>> = idle
            .into_iter()
            .filter_map(Arc::into_inner)
            .map(|c| c.bgra)
            .collect();
        self.retired.extend(self.behind.take());
        self.retired.extend(self.middle.take());
        self.retired.extend(self.front.take());
        self.retired.truncate(RETIRED_PICTURES);
        let bytes = usize::try_from(w.max(0) * h.max(0)).unwrap_or(0) * 4;
        let mut picture = |keep: &dyn Fn(i32) -> bool| {
            let mut s = match spare.pop().filter(|b| b.len() == bytes) {
                Some(bgra) => Surface {
                    width: w,
                    height: h,
                    bgra,
                },
                None => Surface::new(w, h),
            };
            draw::draw_layers_with(&self.ui, &mut s, keep, &mut self.layers);
            self.canvases += 1;
            Arc::new(Canvas {
                width: size.0,
                height: size.1,
                bgra: s.bgra,
                revision: self.canvases,
            })
        };
        match (scene_z, view_z) {
            (Some(z), _) => {
                self.behind = Some(picture(&|d| d > z));
                self.front = Some(picture(&|d| d <= z));
            }
            (None, Some(z)) => {
                self.middle = Some(picture(&|d| d > z));
                self.front = Some(picture(&|d| d <= z));
            }
            (None, None) => {
                self.front = Some(picture(&|_| true));
            }
        }
    }

    /// In the world the play screen's display follows the window's size: set up again when the
    /// window changes.
    // D2-08346
    fn follow_window(&mut self) {
        if self.flow.flow.mode() == mode::GAMEPLAY {
            display(&mut self.ui, self.window);
        }
    }

    /// An interface key pressed or let go: the element with the keyboard first, else every
    /// screen that hears keys. Returns whether an element took it.
    fn interface_key(&mut self, key: u32, down: bool) -> bool {
        if down {
            self.held_keys.insert(key);
        } else {
            self.held_keys.remove(&key);
        }
        self.ui.key(key, down)
    }

    /// A pointer button pressed or let go, as the elements hear it.
    fn interface_button(&mut self, button: u32, down: bool) -> bool {
        if down {
            self.held_buttons.insert(button);
        } else {
            self.held_buttons.remove(&button);
        }
        self.ui.pointer_button(button, down)
    }

    /// The window went inactive: every key and button the elements hold is let go, as its release
    /// would be, since no release comes while the window is inactive; Shift and Ctrl read as up
    /// until pressed again.
    // D2-03459
    fn let_go(&mut self) {
        for key in std::mem::take(&mut self.held_keys) {
            self.ui.key(key, false);
        }
        for button in std::mem::take(&mut self.held_buttons) {
            self.ui.pointer_button(button, false);
        }
        self.play_keys.clear();
        self.left_to_world = false;
        self.mods = (false, false);
        self.held_mods = 0;
        self.ui.set_modifiers(false, false);
        self.ui.set_alt(false);
    }

    /// The left button pressed or let go, as the interface hears it: a press within the double
    /// click's time (half a second) of the last single press's release, and within its area
    /// (two pixels each way) of where the pointer was then, is a double press, and its release a
    /// double press's.
    // D2-04960
    fn check_double_click(&mut self, down: bool, now: f64) -> u32 {
        let (x, y) = self.pointer;
        let (px, py) = self.click_at;
        let double = if down {
            let near = (x - px).abs() <= 2 && (y - py).abs() <= 2;
            self.double_held = near && self.released_at.is_some_and(|t| now - t <= 0.5);
            self.double_held
        } else {
            let was = self.double_held;
            if !was {
                self.released_at = Some(now);
            }
            self.double_held = false;
            was
        };
        self.click_at = (x, y);
        if double {
            button_id::DOUBLE_LEFT
        } else {
            button_id::LEFT
        }
    }

    /// Whether a line is being typed: an entry has the keyboard.
    fn typing(&self) -> bool {
        self.ui.focus().is_some()
    }

    /// The interface action a key pressed or let go names in the player's key maps, with Shift
    /// and Ctrl as held at its press.
    /// A modifier key pressed or let go: Shift and Ctrl hold while either of their keys does.
    fn hold_modifier(&mut self, bit: u8, down: bool) {
        self.held_mods = if down {
            self.held_mods | bit
        } else {
            self.held_mods & !bit
        };
        self.mods = (self.held_mods & 3 != 0, self.held_mods & 12 != 0);
    }

    fn play_action(
        &mut self,
        keys: &auberean_shell::keys::Keys,
        key: Key,
        down: bool,
    ) -> Option<u32> {
        use auberean_shell::keys::{META_CTRL, META_SHIFT};
        let input = key.input_word();
        let meta = if down {
            let m = (u32::from(self.mods.0) * META_SHIFT) | (u32::from(self.mods.1) * META_CTRL);
            *self.play_keys.entry(input).or_insert(m)
        } else {
            self.play_keys.remove(&input)?
        };
        let a = keys.interface_action(input, meta);
        if a.is_none() {
            self.play_keys.remove(&input);
        }
        a
    }

    /// A key while playing, besides the interface key it is: Enter is the chat panel's (the use
    /// key it hears); Backspace while typing drops a character; while a line is typed every key
    /// is the entry's.
    fn play_key(&mut self, key: Key) -> bool {
        let typing = self.typing();
        match key {
            Key::Enter | Key::NumpadEnter => true,
            Key::Backspace if typing => {
                self.ui.backspace();
                true
            }
            _ => typing,
        }
    }
}

/// The play screen's chat panel, while the play screen is up.
fn chat_of(flow: &mut GameFlow<Shared>) -> Option<&mut ChatUi> {
    let (s, _) = flow.flow.current_mut();
    s?.as_any_mut()?
        .downcast_mut::<PlayScreen>()?
        .play
        .child_mut::<Toolbar>()?
        .child_mut::<ChatUi>()
}

/// The interface key a physical key is bound to, as the client's default key map binds it for
/// the interface at all times.
const fn interface_key_of(key: Key) -> Option<u32> {
    Some(match key {
        Key::Escape => key_id::ESCAPE,
        Key::Enter | Key::NumpadEnter => key_id::USE,
        Key::ArrowLeft => key_id::LEFT,
        Key::ArrowRight => key_id::RIGHT,
        Key::ArrowUp => key_id::UP,
        Key::ArrowDown => key_id::DOWN,
        Key::Home => key_id::HOME,
        Key::End => key_id::END,
        Key::PageUp => key_id::PAGE_UP,
        Key::PageDown => key_id::PAGE_DOWN,
        _ => return None,
    })
}

/// The toolbar as the client last asked the scripts for it, as the shortcut bar takes it: each
/// item's icon as its look makes it.
fn shortcut_source(
    t: &auberean_client_sdk::runtime::toolbar::Toolbar,
    model: &auberean_client_sdk::model::Model,
    records: &auberean_world::records::WorldRecords,
) -> shortcuts::Source {
    shortcuts::Source {
        version: t.version,
        shortcuts: t
            .shortcuts
            .iter()
            .map(|s| shortcuts::Shortcut {
                kind: s.kind,
                item: s.item,
                data_enum: s.data_enum,
                data_id: s.data_id,
                text: String::from_utf16_lossy(&s.text),
            })
            .collect(),
        slots: t
            .slots
            .iter()
            .map(|s| shortcuts::SlotInfo {
                answer: s.answer,
                tooltip: s.tooltip.clone(),
                icon: s.icon,
            })
            .collect(),
        set: t.set,
        show_numbers: t.show_numbers,
        height: t.height,
        locked: t.locked,
        insta_scroll: t.insta_scroll,
        skills: t
            .skills
            .iter()
            .map(|k| shortcuts::SkillState {
                skill: k.skill,
                trained: k.trained,
                usable: k.usable,
                waiting: k.waiting,
            })
            .collect(),
        items: t
            .items
            .iter()
            .map(|i| shortcuts::ItemState {
                item: i.item,
                have: i.have,
                here: i.here.clone(),
                icon_layers: if i.here.is_some() {
                    icon_layers(model, records, i.item)
                } else {
                    Vec::new()
                },
            })
            .collect(),
    }
}

/// The crafting page's facts as the page takes them, the game time now with them.
fn craft_facts(k: &auberean_client_sdk::runtime::craft::Craft, now: f64) -> craft::Facts {
    use auberean_client_sdk::runtime::craft as rc;
    use auberean_client_sdk::runtime::text::from_script;
    let skill = |s: &rc::CraftSkill| craft::CraftSkill {
        did: s.did,
        name: from_script(&s.name),
        level: s.level,
        raw_level: s.raw_level,
        cost_to_raise: s.cost_to_raise,
        icon: s.icon,
        usage_xp_mod: s.usage_xp_mod,
    };
    craft::Facts {
        version: k.version,
        skills: k
            .skills
            .iter()
            .map(|s| craft::SkillFacts {
                listed: skill(&s.listed),
                own: s.own.as_ref().map(skill),
                raw_level: s.raw_level,
                level: s.level,
                raiseable: s.raiseable,
                can_raise: s.can_raise,
                recipes: s
                    .recipes
                    .iter()
                    .map(|r| craft::Recipe {
                        did: r.did,
                        name: from_script(&r.name),
                        icon: r.icon,
                        shown: r.shown,
                        craft_skill: r.craft_skill,
                        category: r.category,
                        time_use_again: r.time_use_again,
                    })
                    .collect(),
            })
            .collect(),
        experience: k.experience,
        categories: k
            .categories
            .iter()
            .map(|(c, n)| (*c, from_script(n)))
            .collect(),
        now,
        opened: k.opened.as_ref().map(recipe_at),
    }
}

/// A recipe at a spinner value as the ingredient panel takes it.
fn recipe_at(
    a: &auberean_client_sdk::runtime::craft::RecipeAt,
) -> auberean_ui_game::ingredients::RecipeAt {
    use auberean_client_sdk::runtime::text::from_script;
    use auberean_ui_game::ingredients as ing;
    let r = &a.recipe;
    ing::RecipeAt {
        recipe: r.did,
        spinner: a.spinner,
        name: from_script(&r.name),
        charges: r.charges,
        min_spinner: r.min_spinner,
        max_spinner: r.max_spinner,
        last_spinner: r.last_spinner,
        auto_populating: r.auto_populating,
        max_ingredients: r.max_ingredients,
        name_coloring: r.name_coloring,
        name_color: r.name_color,
        difficulty: r.difficulty,
        dynamic_difficulty: r.dynamic_difficulty,
        cutoff: r.difficulty_minus_cutoff,
        cost: r.cost,
        dynamic_cost: r.dynamic_cost,
        craft_skill: r.craft_skill,
        read_only_spinner: r.read_only_spinner,
        skill_level: a.skill_level,
        mask: a.mask,
        ingredients: a
            .ingredients
            .iter()
            .map(|i| ing::Ingredient {
                id: i.id,
                description: from_script(&i.description),
            })
            .collect(),
        spinner_cost: a.cost,
        spinner_difficulty: a.difficulty,
        auto_populate: a.auto_populate.clone(),
        items: a.items.clone(),
        fits: a.fits.clone(),
    }
}

/// An act of the story as the story page takes it: its name, and its scenes in their order.
// D2-12307, D2-12316
fn story_act(a: &auberean_client_sdk::script::story::ActInfo) -> story_quests::Act {
    use auberean_client_sdk::runtime::text::from_script;
    story_quests::Act {
        id: a.id,
        name: from_script(&a.name),
        visible: a.visible != 0,
        scenes: a
            .scenes
            .iter()
            .map(|s| story_quests::Scene {
                id: s.id,
                name: from_script(&s.name),
                hidden: s.hidden != 0,
                playable: s.playable != 0,
            })
            .collect(),
    }
}

/// The effects as the client last asked the scripts for them, as the effects panel takes them.
fn effect_list(e: &auberean_client_sdk::runtime::effects::EffectList) -> statusbar::EffectList {
    statusbar::EffectList {
        version: e.version,
        now: e.now,
        nodes: e
            .nodes
            .iter()
            .map(|n| statusbar::EffectNode {
                eid: n.eid,
                name: String::from_utf16_lossy(&n.name),
                icon: n.icon,
                time_cast: f64::from_bits(n.time_cast),
                duration: f64::from_bits(n.duration),
                pulsed: n.pulsed(),
            })
            .collect(),
    }
}

/// The player's health and vigor and their greatest, where the qualities have them.
fn vitals(o: &Object) -> Option<[u32; 4]> {
    let q = &o.qualities;
    let v = |id: u32| {
        q.ints
            .get(&id)
            .map(|v| u32::try_from(*v).unwrap_or(0))
            .or_else(|| {
                q.floats.get(&id).map(|f| {
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    let n = f.max(0.0) as u32;
                    n
                })
            })
    };
    Some([v(258)?, v(263)?, v(259)?, v(264)?])
}

impl Interface for Retail {
    fn event(&mut self, event: &Event, game: &Game<'_>) -> bool {
        let pregame = game.phase != Phase::World;
        let playing = !pregame && self.flow.flow.mode() == mode::GAMEPLAY;
        match event {
            Event::Pointer { x, y } => {
                let (x, y) = self.to_screen(*x, *y);
                self.pointer = (x, y);
                let over = self.ui.pointer_move(x, y);
                over || pregame
            }
            Event::Button {
                button: Button::Right,
                down,
            } => {
                // The player's keys bind the right button only to looking with the mouse while
                // held (an interface key): the September 2013 client's maps give the interface no
                // right press.
                self.interface_key(MOUSE_LOOK_KEY, *down);
                pregame
            }
            Event::Button { button, down } => {
                let id = match button {
                    Button::Left => self.check_double_click(*down, game.now),
                    Button::Right => button_id::RIGHT,
                    Button::Middle => button_id::MIDDLE,
                };
                let took = self.interface_button(id, *down);
                if *down && *button == Button::Left && pregame {
                    self.key_for_keyboard(pregame::LEFT_BUTTON_WORD, self.held_mods);
                }
                // The left button runs forward in mouse look: the world's controls see it too,
                // even when it lands on the field over the world (or on a panel under a pointer
                // mouse look hides), and see it let go.
                if *button == Button::Left && playing {
                    let look = self
                        .play()
                        .and_then(GamePlay::camera)
                        .is_some_and(camera::Camera::mouse_look);
                    if *down {
                        self.left_to_world = look;
                    }
                    if self.left_to_world {
                        if !*down {
                            self.left_to_world = false;
                        }
                        return false;
                    }
                }
                // The player's keys bind the middle button to mouse look's switch: an interface
                // key. In the world the camera's panel decides mouse look, so the middle button is
                // the interface's.
                if *button == Button::Middle && playing {
                    self.interface_key(camera::action::TOGGLE_MOUSE_LOOK, *down);
                    return true;
                }
                took || pregame
            }
            Event::Wheel { notches } => {
                let step = if *notches > 0.0 {
                    button_id::WHEEL_UP
                } else {
                    button_id::WHEEL_DOWN
                };
                let mut took = false;
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                for _ in 0..(notches.abs().round() as u32).max(1) {
                    took |= self.ui.pointer_button(step, true);
                }
                took || pregame
            }
            Event::Text(t) => {
                let took = self.ui.type_text(t);
                took || pregame
            }
            Event::Composing(t) => {
                self.ui.set_composition(t);
                true
            }
            Event::Key { key, down, .. } => {
                // Shift, Ctrl and Alt as the interface reads them; Delete reaches the entry with the
                // keyboard as the key it is.
                match key {
                    Key::ShiftLeft => self.hold_modifier(1, *down),
                    Key::ShiftRight => self.hold_modifier(2, *down),
                    Key::ControlLeft => self.hold_modifier(4, *down),
                    Key::ControlRight => self.hold_modifier(8, *down),
                    Key::AltLeft | Key::AltRight => self.ui.set_alt(*down),
                    _ => {}
                }
                self.ui.set_modifiers(self.mods.0, self.mods.1);
                if *down && pregame {
                    let w = pregame::input_word(*key);
                    self.key_for_keyboard(w, self.held_mods);
                }
                if *down && *key == Key::Delete && self.ui.character_input(0, DELETE_KEY) {
                    return true;
                }
                // The play screen's and the camera's keys, unless a line is being typed: the
                // interface actions the player's key maps bind them to.
                if playing && !self.typing() {
                    if let Some(a) = self.play_action(game.keys, *key, *down) {
                        if *down && selection::key::wants_targets(a) {
                            self.gather.want_targets = true;
                        }
                        self.interface_key(a, *down);
                        return true;
                    }
                }
                let interface = interface_key_of(*key);
                let used = interface.is_some_and(|k| self.interface_key(k, *down));
                // An arrow no element used goes on to the world's controls, unless the element
                // with the keyboard uses the arrows itself (the camera sends no movement then).
                let arrow = matches!(
                    key,
                    Key::ArrowLeft | Key::ArrowRight | Key::ArrowUp | Key::ArrowDown
                );
                let held = arrow
                    && playing
                    && !used
                    && self
                        .ui
                        .active()
                        .is_some_and(|a| camera::takes_arrows(&self.ui, a));
                if !*down {
                    return pregame || used || held;
                }
                if pregame {
                    if *key == Key::Backspace {
                        self.ui.backspace();
                    }
                    return true;
                }
                let took = self.play_key(*key);
                took || used || held
            }
            Event::Focus(false) => {
                self.let_go();
                pregame
            }
            Event::Focus(true) | Event::Motion { .. } | Event::Resized { .. } | Event::Close => {
                pregame
            }
        }
    }

    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        if game.size.0 > 0 && game.size.1 > 0 {
            self.window = game.size;
            self.preferences.follow_window(game.size);
        }
        self.follow_window();
        self.ui.use_time(game.now);
        // What the screens are given; then the flow's tick: a screen asked for comes up, and the
        // screen up has its tick.
        self.feed(game);
        self.feed_pregame(game);
        self.pregame_picked(game);
        self.keep_creation(game);
        self.keep_keys(game);
        self.dialog_notices();
        for e in self.flow.tick(&mut self.ui) {
            match e {
                auberean_ui_game::flow::Event::Screenshot => {
                    requests.push(Request::Screenshot);
                }
            }
        }
        let out = std::mem::take(&mut self.flow.flow.data.out);
        self.carry_out(out, requests);
        self.carry_out_pregame(requests);
        // The effects' preferences changed or applied (and, first, as read).
        if let Some(c) = self.preferences.take_channels() {
            requests.push(Request::FxChannels(c));
        }
        // The tooltip preferences applied (and, first, their values as read).
        let (delay, enable) = self.preferences.take_tooltips();
        if delay.is_some() || enable.is_some() {
            self.ui.set_tooltip_preferences(delay, enable);
        }
        // The sound preferences changed or applied (and, first, their values as read).
        for (key, value, applied) in self.preferences.take_sound() {
            requests.push(Request::Sound(
                auberean_shell::interface::SoundOrder::Preference {
                    key,
                    value,
                    applied,
                },
            ));
        }
        // The sounds the elements' media asked for.
        // D2-11145
        for s in self.ui.take_sounds() {
            requests.push(sounds::once(s.sound));
        }
        requests.append(&mut self.pending);
        if let Some(auto) = &mut self.auto {
            auto.update(game, requests);
        }
        self.chat_service(game, requests);
        match game.phase {
            Phase::Disconnected => {
                if self.was_connected {
                    self.was_connected = false;
                    match game
                        .stopped
                        .and_then(auberean_client_sdk::runtime::download::notice_line)
                    {
                        // The client stopped (a record it was sent could not be stored): the
                        // disconnected screen says why, whatever screen was up.
                        Some(line) => self.flow.stopped(line),
                        // The server is gone: the game's flow hears it (unless the connecting
                        // screen already asked for the disconnected screen with why).
                        None if game.ended.is_some_and(|e| e.server_died) => {
                            self.flow.server_died(&self.ui);
                        }
                        // The session ended under the player otherwise: the connection lost
                        // (the connecting screen says why itself).
                        None if game.ended.is_none()
                            && self.flow.flow.mode() != mode::DATA_PATCH =>
                        {
                            self.flow.server_died(&self.ui);
                        }
                        None => {}
                    }
                }
            }
            Phase::Connecting => self.was_connected = true,
            Phase::Characters => {
                self.was_connected = true;
                self.pregame(requests);
            }
            Phase::Entering | Phase::World => {
                self.was_connected = true;
                let (now, queued) = (self.flow.flow.mode(), self.flow.flow.queued_mode());
                if now != mode::GAMEPLAY && queued != mode::GAMEPLAY {
                    self.go(mode::GAMEPLAY);
                }
            }
        }
        self.was_in_world = game.phase == Phase::World;
        self.follow_input_method(requests);
        // The screens hear their elements through their listeners: what the elements said is
        // not kept besides.
        self.ui.take_messages();
    }

    /// The names over heads and the selection's marks are placed as the game places them, after
    /// the world's update: from this frame's camera, over the bodies where the world last drew
    /// them.
    // D2-02126
    fn after_world(&mut self, game: &Game<'_>, requests: &mut Vec<Request>) {
        if game.phase != Phase::World {
            return;
        }
        let camera = self.gather.camera_in_world(game.seen);
        let mut out = Vec::new();
        let (screen, data) = self.flow.flow.current_mut();
        let Some(world) = data.world.as_mut() else {
            return;
        };
        if camera.is_some() {
            world.camera = camera;
        }
        let names = screen
            .and_then(|s| s.as_any_mut())
            .and_then(|a| a.downcast_mut::<PlayScreen>())
            .and_then(|p| p.play.child_mut::<names::Names>());
        if let Some(n) = names {
            n.after_world = true;
            n.place_after_world(&mut self.ui, world, &mut out);
        }
        self.carry_out(out, requests);
    }

    fn draw(&mut self, game: &Game<'_>, overlay: &mut Overlay) {
        if game.phase != Phase::World {
            self.pregame_scenes(game, overlay);
        } else if let Some(c) = game.client {
            let pick = self.doll_pick.take();
            let asked =
                self.dolls
                    .scenes(&self.ui, &game.world.records, c, pick, &mut overlay.scenes);
            if pick.is_some() {
                self.doll_scene = asked;
                // No figure shown to search: the search finds nothing.
                if asked.is_none() {
                    self.world_found = Some(0);
                }
            }
        }
        self.redraw();
        overlay.behind.clone_from(&self.behind);
        overlay.middle.clone_from(&self.middle);
        overlay.front.clone_from(&self.front);
        overlay.cursor = self.cursor_image();
        let size = self.size();
        overlay.screen = (size != self.window).then_some(size);
    }

    fn prepare(&mut self, client: &mut auberean_client_sdk::Client) {
        // The text elements' string downloads go through this client's data cache.
        self.text.connect(client.data_download().strings());
        let notices = std::mem::take(&mut self.last_notices);
        self.gather.prepare(client, &notices);
        self.chat.prepare(client);
        let play = self.play().map(GamePlay::frame);
        if self.places.prepare(client, &mut self.ui, play) {
            if let Some(c) = chat_of(&mut self.flow) {
                c.show_newest_lines(&mut self.ui);
            }
        }
        // A line naming no command: the client's error line that says so.
        for _ in 0..self.chat.take_unknown() {
            let line = self
                .ui
                .assets()
                .string(STRINGS, Token::of("ID_Chat_UnknownCommand").raw())
                .unwrap_or_default();
            self.errors.push(line);
        }
        let answers = self.chat.take_answers();
        if let Some(c) = chat_of(&mut self.flow) {
            for (window, list, text) in answers {
                c.history_item(&mut self.ui, window, list, &text);
            }
        }
        let d = &mut self.flow.flow.data;
        if d.in_world {
            self.options.prepare(client, &notices, &mut d.options);
        } else {
            d.options = None;
        }
        let listed = self.preferences.listing();
        if d.preferences.as_ref().map(|l| l.serial) != listed.map(|l| l.serial) {
            d.preferences = listed.cloned();
        }
        d.preferences_changed = self.preferences.take_changed();
    }

    fn wants_pointer(&self, game: &Game<'_>) -> bool {
        game.phase != Phase::World || self.typing()
    }

    fn text_mode(&self) -> bool {
        self.typing()
    }

    fn set_preferences(
        &mut self,
        settings: &auberean_shell::Settings,
        records: &auberean_world::records::WorldRecords,
    ) {
        self.preferences.start(settings, records);
    }

    fn set_graphics(&mut self, graphics: &auberean_draw::quality::Graphics) {
        self.preferences.set(graphics);
    }

    fn set_presentation(&mut self, modes: &[(u32, u32)], settings: &auberean_shell::Settings) {
        self.preferences.set_presentation(modes, settings);
    }

    fn toggle_full_screen(&mut self, requests: &mut Vec<Request>) {
        self.preferences.toggle_full_screen();
        if let Some(p) = self.preferences.take_presentation() {
            requests.push(Request::Presentation(p));
        }
    }

    fn set_random(&mut self, random: &auberean_world::chargen::Random) {
        self.random = random.clone();
        let media = random.clone();
        self.ui.set_dice(Box::new(move || media.draw()));
    }
}

/// The character the chosen avatar makes: the player's entity (an administrator's when asked), the
/// race's start area, class and implement, the race and sex, and every physique value in the order
/// the table lays them out.
// D2-11920
fn new_character(
    name: &str,
    admin: bool,
    c: &hall::Creation,
) -> Option<auberean_client_sdk::NewCharacter> {
    use auberean_client_sdk::protocol::table::{Table, SMALL};
    let (avatar, _) = c.chosen()?;
    let (class, implement, start_area) = avatar.class_implement_start()?;
    Some(auberean_client_sdk::NewCharacter {
        name: name.encode_utf16().collect(),
        entity: auberean_world::chargen::avatar_entity(admin),
        start_area,
        race: avatar.race,
        sex: avatar.sex,
        class,
        implement,
        physique: Table::from_inserted(SMALL, avatar.physique()).entries,
        physique_size_class: SMALL,
    })
}
