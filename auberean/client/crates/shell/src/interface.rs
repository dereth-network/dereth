//! The interface seam: where the game's interface plugs into the shell.
//!
//! An [`Interface`] is everything drawn over the world and everything the player does that is not
//! the world's own controls: the login and character screens, the panels, chat. The shell holds one
//! and, each frame:
//!
//! 1. hands it every device event **first** ([`Interface::event`]); an event it takes never reaches
//!    the world's controls;
//! 2. lets it see the game and ask for things ([`Interface::update`]): to connect to a server with an
//!    account, to have the client act (enter the world as a character, make or delete one, log off,
//!    call the server's scripts), to play or stop a sound, to write or read one of the player's
//!    files, to disconnect or quit ([`Request`]);
//! 3. lets it draw ([`Interface::draw`]): pictures of its own the window's size, one behind the
//!    world and one in front of it (an interface whose screens hold the world in a viewport draws
//!    its deeper layers behind and its nearer ones in front), textured triangle batches over
//!    textures of its own (what a widget kit lowers its drawing to), and rectangles in window
//!    pixels, each a solid color or a picture of the world's data, drawn in order over everything
//!    ([`Overlay`]).
//!
//! The interface sees the game through [`Game`]: the phase of the session, the game's view (the
//! character list, the player, the objects around it and what the panels read of them, the
//! panels' caches: [`View`]), what the client told since the last frame (its events, the answers
//! to what the interface asked among them, the scripts' notices to the panels), the world's
//! records (pictures, strings, layouts), the window's size, the time, the scene's status, the
//! client's own preferences. It never holds the client, and never reaches the platform or the
//! device: a host draws its overlay, and the shell carries out its requests. Before the view is
//! refreshed each frame, the interface may ask for what the refresh is to answer
//! ([`Interface::before_view`]).
//!
//! **Choosing an interface.** The client can show more than one interface ([`Choice`]): the
//! retail one, and Horizon. The player chooses with the client's own preference `[UI] Interface`
//! (the client page's last section), or an interface asks with [`Request::ChooseInterface`]. A host
//! hands the shell an [`InterfaceMaker`], which builds each interface the first time it is chosen
//! or refuses it, saying why; the shell keeps each one built. A refused choice leaves the interface
//! shown as it is, and the player is told why ([`Game::told`]). A choice taken is followed at the
//! start of the next frame: the interface shown is left ([`Interface::leave`]) and lets go of its
//! textures ([`Interface::release_textures`]), the world's controls let go of what is held and mouse
//! look ends, and the one chosen is entered ([`Interface::enter`]).

use core::net::SocketAddr;

use auberean_client_sdk::contract::event::GameEvent;
pub use auberean_client_sdk::contract::view::View;
use auberean_client_sdk::Action;
use auberean_scene::{Status, WorldData};

pub use auberean_scene::view::{ViewCamera, ViewFigure, ViewLight, ViewScene};

use crate::input::Event;

pub use auberean_client_sdk::Phase;

/// The overlay's batch and texture types, which name no game.
pub use dereth_ui_overlay::{
    GroundMarker, OverlayDepth, OverlayItem, OverlayMaterial, OverlayReleased, OverlaySampler,
    OverlaySpace, OverlayTexture, OverlayVertex, PreviewSpace, Viewport,
};

/// The interfaces the client can show, as the `[UI] Interface` preference names them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Choice {
    /// The game's own interface, as the September 2013 client shows it.
    #[default]
    Retail,
    /// Horizon, this client's own.
    Horizon,
}

impl Choice {
    /// Every interface, in the order the options list them.
    pub const ALL: [Self; 2] = [Self::Retail, Self::Horizon];

    /// The preference's value.
    #[must_use]
    pub const fn value(self) -> i32 {
        match self {
            Self::Retail => 0,
            Self::Horizon => 1,
        }
    }

    /// The interface a preference value names; anything else is the retail one.
    #[must_use]
    pub const fn from_value(v: i32) -> Self {
        match v {
            1 => Self::Horizon,
            _ => Self::Retail,
        }
    }

    /// Its name, as the options list it and the preferences file holds it.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Retail => "Retail",
            Self::Horizon => "Horizon",
        }
    }
}

/// What a refused interface is said to be when it isn't written yet.
pub const NOT_AVAILABLE_YET: &str = "The Horizon interface is not available yet";

/// What builds the interfaces a host offers: each the first time it is chosen, or why it can't be
/// shown, in words for the player.
pub trait InterfaceMaker<'w> {
    /// The interface `choice` names, built; or why it can't be shown.
    ///
    /// # Errors
    ///
    /// Why the interface can't be shown, when it can't.
    fn make(&mut self, choice: Choice) -> Result<Box<dyn Interface + 'w>, String>;
}

impl<'w, F> InterfaceMaker<'w> for F
where
    F: FnMut(Choice) -> Result<Box<dyn Interface + 'w>, String>,
{
    fn make(&mut self, choice: Choice) -> Result<Box<dyn Interface + 'w>, String> {
        self(choice)
    }
}

/// An account to log in with, as the launcher hands the client one: the server's address, the
/// account and its password (the ticket).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Login {
    pub server: SocketAddr,
    pub account: String,
    pub password: String,
}

/// What an interface asks of the shell.
#[derive(Clone, Debug, PartialEq)]
pub enum Request {
    /// Log in to the server at `server` with this account and password (the ticket).
    Connect {
        server: SocketAddr,
        account: String,
        password: String,
    },
    /// Have the client act: enter the world as a character, make or delete one, log off, chat,
    /// call the server's scripts.
    Act(Action),
    /// End the session.
    Disconnect,
    /// Open the web page where players ask for help.
    OpenAssistancePage,
    /// Close the program.
    Quit,
    /// Save a screenshot of this frame.
    Screenshot,
    /// Set the camera: the interface's camera panel decides mouse look and first person, and
    /// zooms by the wheel over the world.
    Camera(CameraRequest),
    /// The world shown again, or hidden while the story plays.
    ShowWorld(bool),
    /// A scene on the way through a door plays in place of portal space, or no longer does.
    PortalScenePlaying(bool),
    /// As the session next leaves the world, portal space, if it is on, stays in sight this many
    /// seconds before it goes (as a log-off ends in it).
    PortalSpaceLingers(f64),
    /// Before the world, the world seen from `camera` at the game clock `time` (seconds), drawn
    /// from the install with no server behind the screens: asked each frame it is wanted, and let
    /// go of in the first frame that does not ask. Never drawn in the world.
    Vista {
        camera: auberean_scene::explore::FreeCamera,
        time: f64,
    },
    /// Lay these pictures on the ground under the world objects they name (by id), drawn with
    /// the world this frame, each from one of [`Overlay::textures`]: asked each frame they are
    /// wanted, and gone in the first frame that does not ask.
    GroundMarks(Vec<GroundMarker<u64>>),
    /// Find the body under the pointer (window pixels) in the world as last drawn: the answer
    /// comes with the next frame's game ([`Game::world_pick`]).
    PickWorld { x: f32, y: f32 },
    /// What an options page asks of the client's own preferences: what is applied takes effect
    /// (the world's drawing, the camera, the sound, how the game is shown, the interface), and
    /// what is kept goes to the preferences file.
    Preferences(auberean_options::page::Request),
    /// Show this interface from the next frame, as the `[UI] Interface` preference does; refused
    /// with a line saying why when it can't be shown.
    ChooseInterface(Choice),
    /// The key maps the player applied on the key map screen, each a list of an input and its
    /// action (keys alone, with Shift, with Ctrl): written to their files and bound from now on.
    KeyMaps([Vec<(u32, u32)>; 3]),
    /// The player's three key maps bound from now on, their files left as they are.
    BindKeyMaps([Vec<(u32, u32)>; 3]),
    /// The shown interface's own key maps as its bindings page applies them, each a list of an
    /// input and its action (keys alone, with Shift, with Ctrl): written to its own files and
    /// bound from now on, in place of the ones it had.
    InterfaceKeyMaps([Vec<(u32, u32)>; 3]),
    /// Play or stop a sound.
    Sound(SoundOrder),
    /// Where the platform's input method writes now (none: off), asked when it changes.
    InputMethod(Option<crate::host::InputArea>),
    /// Write one of the player's files (in the folder the host keeps the client's files in, by a
    /// bare name), anew.
    WritePlayerFile { name: String, bytes: Vec<u8> },
    /// Read one of the player's files (as for writing): the answer comes with the next frame's
    /// game ([`Game::files_read`]).
    ReadPlayerFile { name: String },
    /// The player pressed node `node` of a plugin's window (a button, a toggle): the plugin hears
    /// it at the next frame.
    PluginClick {
        plugin: String,
        window: String,
        node: String,
    },
    /// The player opened (`open`) or closed one of a plugin's windows.
    PluginWindow {
        plugin: String,
        window: String,
        open: bool,
    },
}

/// A sound the interface asks for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SoundOrder {
    /// Play a sound or music setting, kept in `slot` when there is one (to be stopped by it later).
    Play {
        data: auberean_audio::PlayRequest,
        slot: Option<SoundSlot>,
    },
    /// Stop the sound a slot keeps, fading out over `fade` seconds.
    Stop { slot: SoundSlot, fade: f32 },
    /// The groove level the music follows.
    Groove(i32),
    /// A sound preference (`auberean_audio::options`) changed or applied: its key and value (the
    /// number, the features' number, or 1 or 0 for yes or no).
    Preference { key: u32, value: f32, applied: bool },
}

/// The sounds the interface keeps by name: the sound playing across the screens before the
/// world, and a story's music and voice.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SoundSlot {
    Screens,
    StoryMusic,
    StoryVoice,
}

/// What an interface asks of the camera.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CameraRequest {
    /// Mouse look on or off.
    MouseLook(bool),
    /// First person on or off.
    FirstPerson(bool),
    /// A step of the zoom: in, toward the player, or out.
    Zoom { inward: bool },
    /// The camera and the movement the interface shows the world with: the orbit camera with
    /// these settings, or with none the game's own camera. Asked again with other settings, the
    /// orbit camera keeps its place and takes them. The game's own camera keeps its state while
    /// the orbit camera is in use, and is back as it was when it ends; mouse look ends as the
    /// orbit camera begins. Every interface starts with the game's own: switching interfaces
    /// ends the orbit camera before the next one is entered, so one that wants it asks in its
    /// first update.
    ///
    /// While the orbit camera is in use, the world's controls turn it with the mouse while a
    /// button they hear is held (an interface that sees a press over the world and declines it,
    /// returning `false` from [`Interface::event`], lets it turn the camera), zoom it with the
    /// wheel and the zoom controls, and turn and tilt it with the camera's rotate and tilt
    /// controls; the player's orbit preferences (movement, reversed directions), as applied, are
    /// laid over the settings asked.
    Scheme(Option<OrbitSettings>),
}

/// The orbit camera's settings, and how the movement controls move the player under it.
pub use auberean_client_sdk::contract::orbit::{MovementMode, OrbitSettings};

/// The game as an interface sees it.
#[derive(Clone, Copy)]
pub struct Game<'a> {
    pub phase: Phase,
    /// The game as the client last refreshed it (before this frame's update), while there is a
    /// session.
    pub view: Option<&'a View>,
    /// The client's strings, while there is a session: string references written through its
    /// data cache, and downloads of the string tables it lacks ([`Game::write_text`] writes a
    /// view's).
    pub strings: Option<&'a dyn auberean_client_sdk::text::CachedStrings>,
    /// The looks of the figures the interface's viewports show, as the world's records make
    /// them; as the client last had them before the frame's drawing.
    pub looks: &'a Looks,
    pub world: &'a WorldData,
    /// The window's drawing area, in pixels.
    pub size: (u32, u32),
    /// Seconds since the program started.
    pub now: f64,
    /// The world's status (the player's place, the bodies drawn).
    pub status: &'a Status,
    /// What the player heard since the last frame, oldest first.
    pub heard: &'a [Heard],
    /// The lines the player's scripts wrote for the chat windows since the last frame, oldest
    /// first.
    pub shown: &'a [Shown],
    /// What the last frame's picks met: per viewport scene that asked, its index in the
    /// overlay's scenes and the figure under the pointer, with the kind of mesh met there.
    /// The figures still playing a behavior they were asked to: the scene, the figure.
    pub behaving: &'a [(usize, usize)],
    pub picked: &'a [(usize, Option<auberean_scene::view::ViewHit>)],
    /// The notices the player's scripts sent the interface's panels since the last frame, oldest
    /// first, as the game reads them.
    pub notices: &'a [auberean_client_sdk::contract::notice::Notice],
    /// What the last world frame showed: its camera and the bodies drawn in it.
    pub seen: &'a auberean_scene::seen::Seen,
    /// The answer to the last frame's [`Request::PickWorld`]: the body under the pointer, or
    /// none; `None` when no pick was asked.
    pub world_pick: Option<Option<u64>>,
    /// How many times the player's body has come to be placed with its cells loaded.
    pub arrivals: u32,
    /// The key maps bound and what their actions do.
    pub keys: &'a crate::keys::Keys,
    /// Whether the sound kept across the screens before the world plays; `None` with no sound
    /// device.
    pub screens_sound: Option<bool>,
    /// The notice that stopped the last session, when one did (a record the data download could
    /// not store).
    pub stopped: Option<u32>,
    /// How the last session ended under the player, when the server ended it (the login failed,
    /// or the server is gone).
    pub ended: Option<auberean_client_sdk::runtime::SessionEnd>,
    /// The interface shown.
    pub interface: Choice,
    /// The client's own preferences: how they are listed for an options page, what the tooltips
    /// follow.
    pub preferences: &'a crate::client_preferences::ClientPreferences,
    /// The preferences the client changed itself since the last frame (full screen flipped, an
    /// interface refused), for an options page to show.
    pub preferences_changed: &'a [auberean_options::page::Change],
    /// What the client tells the player since the last frame, as a line each (why an interface
    /// chosen can't be shown).
    pub told: &'a [String],
    /// What the client told since the last frame, oldest first: among them the answers to what
    /// the interface asked (a chat list's item, a line that named no command).
    pub events: &'a [GameEvent],
    /// The interface's actions the keys began (`true`) and ended since the last frame, in order,
    /// as the key maps in force name them: the game's interface actions (a window's key, the chat
    /// entry's, a screenshot's) and this client's own. They reached the world's controls, which
    /// carry out the game's own selection, use and camera actions among them; the rest are the
    /// interface's to carry out. An event the interface took names none.
    pub keyed: &'a [(u32, bool)],
    /// The answers to the last frame's [`Request::ReadPlayerFile`]s, in order: each file's name,
    /// and what it holds (none when there is no such file).
    pub files_read: &'a [(String, Option<Vec<u8>>)],
    /// The host's pad as read this frame, for the interface to turn into the game's controls and
    /// actions; none with no pad.
    pub pad: Option<crate::pad::PadState>,
    /// The lines the plugins printed since the last frame, oldest first, each `plugin: text` (the
    /// plugin host's own without a name).
    pub plugin_lines: &'a [String],
    /// The plugins' windows and HUDs, open or not, as their host has them ([`crate::plugins`]).
    pub plugin_windows: &'a [crate::plugins::PluginWindow],
}

/// The looks of the figures an interface's viewports show, as the world's records make them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Looks {
    /// The player's id and its look as a figure of it stands (its own record's when its
    /// description names one, else the description sent), while in the world.
    pub player: Option<(u64, auberean_world::look::Desc)>,
    /// The account's characters' looks as their descriptions make them, in the character list's
    /// order, while the list is shown.
    pub characters: Vec<auberean_world::look::Desc>,
}

/// A line the player's scripts wrote for the chat windows, as the game's text: what kind of text
/// it is (each chat window shows some kinds) and the object it was shown about; and the string
/// reference it was written from, which a chat window writes again once the string tables it
/// reads have come.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub kind: u32,
    pub about: Option<u64>,
    pub text: String,
    pub info: Option<auberean_client_sdk::text::StringInfo>,
    /// How many of the frame's notices the scripts had sent when they wrote it.
    pub notices_before: usize,
}

/// A line the player heard: a saying, a tell, a text for the chat window.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Heard {
    /// The server's call that carried it.
    pub function: u32,
    /// Who it is from, when the call names an object.
    pub from: Option<u64>,
    /// Its words.
    pub text: String,
}

impl Game<'_> {
    /// A view's string reference written as the game writes it: a literal as it is, any other
    /// through the client's strings; `None` while there is no session, or when it cannot be
    /// written yet.
    #[must_use]
    pub fn write_text(&self, t: &auberean_client_sdk::contract::text::TextRef) -> Option<String> {
        if let Some(l) = &t.literal {
            return Some(String::from_utf16_lossy(l));
        }
        let info = auberean_client_sdk::runtime::text::from_view(t);
        self.strings?
            .write(&info)
            .map(|u| String::from_utf16_lossy(&u))
    }
}

impl std::fmt::Debug for Game<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Game")
            .field("phase", &self.phase)
            .field("size", &self.size)
            .finish_non_exhaustive()
    }
}

/// One rectangle drawn over the world.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Quad {
    /// Left, top, right, bottom, in window pixels.
    pub rect: [f32; 4],
    /// The picture's part drawn: left, top, right, bottom, 0 to 1.
    pub uv: [f32; 4],
    /// Multiplies the picture (or is the color, with no picture): red, green, blue, alpha, 0 to 1.
    pub color: [f32; 4],
    /// A picture of the world's data, by id; `None` for a solid rectangle.
    pub picture: Option<u32>,
}

/// A picture an interface drew itself, as large as the window: blue, green, red, alpha, row by
/// row from the top.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub bgra: Vec<u8>,
    /// Changes whenever the picture does, so a host uploads it only then.
    pub revision: u64,
}

/// What an interface draws this frame: its picture behind the world, the world, its picture in
/// front, its textured batches, then the rectangles in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// Drawn first, where the world does not cover it.
    pub behind: Option<std::sync::Arc<Canvas>>,
    /// Drawn over the world and under the scenes, blended by its alpha: in the world, the
    /// elements deeper than the viewports showing scenes (a panel's figure stands over them).
    pub middle: Option<std::sync::Arc<Canvas>>,
    /// Drawn over the world and the scenes, blended by its alpha.
    pub front: Option<std::sync::Arc<Canvas>>,
    /// Textured, tinted triangle batches in clip space over the window, drawn in order over
    /// `front` and under the rectangles. Each samples one of [`Overlay::textures`]; a batch whose
    /// texture is not there draws nothing, and a preview space draws nothing itself (the
    /// interface's viewports are [`Overlay::scenes`]) but marks where they stand among the
    /// batches: those before the first preview space are drawn under the scenes (over `middle`),
    /// so a window holding a figure is drawn under it. A batch hidden behind the world is drawn
    /// over it.
    pub items: Vec<OverlayItem>,
    /// The textures the batches sample, by their keys, kept from frame to frame: what is here is
    /// on the device, and what leaves is let go of there. The interface that uploaded them lets
    /// them go ([`Overlay::release`]), on leaving as on anything else.
    pub textures: OverlayTextures,
    pub quads: Vec<Quad>,
    /// The scenes shown in viewports, drawn over `behind` and `middle` (what is drawn there
    /// stays where the scene draws nothing) and under `front`.
    pub scenes: Vec<ViewScene>,
    /// The pointer's picture over the window, when the interface sets one.
    pub cursor: Option<std::sync::Arc<CursorImage>>,
    /// The size the interface lays its screen out at, when it is not the window's: its pictures
    /// show in the part of the window [`fitted`] gives, and the viewports' rectangles are in its
    /// pixels, their scenes scaled with it.
    pub screen: Option<(u32, u32)>,
}

/// Where a screen of size `screen` shows in a window of size `window`, in the window's pixels: as
/// large as it goes at its own proportions, in the middle (left, top, width, height).
#[must_use]
pub fn fitted(screen: (u32, u32), window: (u32, u32)) -> [f32; 4] {
    #[allow(clippy::cast_precision_loss)]
    let (sw, sh, ww, wh) = (
        screen.0.max(1) as f32,
        screen.1.max(1) as f32,
        window.0 as f32,
        window.1 as f32,
    );
    let scale = (ww / sw).min(wh / sh);
    let (w, h) = (sw * scale, sh * scale);
    [((ww - w) / 2.0).floor(), ((wh - h) / 2.0).floor(), w, h]
}

/// A pointer's picture: `width * height` texels of red, green, blue, alpha, row by row from the
/// top, and the point of it that is the pointer's position. `picture` names it (the same picture
/// is the same cursor).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct CursorImage {
    pub picture: u32,
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
    pub hot_x: u32,
    pub hot_y: u32,
}

impl CursorImage {
    /// A pointer's picture from the game's own: `picture`'s texels of blue, green, red, alpha, and
    /// its point (a point before the picture's corner taken as the corner).
    #[must_use]
    pub fn from_bgra(
        picture: u32,
        (width, height): (u32, u32),
        bgra: &[u8],
        (hot_x, hot_y): (i32, i32),
    ) -> Self {
        Self {
            picture,
            width,
            height,
            rgba: bgra
                .as_chunks::<4>()
                .0
                .iter()
                .flat_map(|p| [p[2], p[1], p[0], p[3]])
                .collect(),
            hot_x: u32::try_from(hot_x).unwrap_or(0),
            hot_y: u32::try_from(hot_y).unwrap_or(0),
        }
    }
}

/// The overlay's textures, by key: each a picture of blue, green, red, alpha texels.
#[derive(Clone, Default)]
pub struct OverlayTextures(std::collections::BTreeMap<OverlayTexture, std::sync::Arc<TextureData>>);

impl OverlayTextures {
    /// The texture under `key`, when it is there.
    #[must_use]
    pub fn get(&self, key: &OverlayTexture) -> Option<&std::sync::Arc<TextureData>> {
        self.0.get(key)
    }

    /// Each texture with its key, in key order.
    pub fn iter(
        &self,
    ) -> impl Iterator<Item = (&OverlayTexture, &std::sync::Arc<TextureData>)> + '_ {
        self.0.iter()
    }

    /// How many there are.
    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are none.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl std::fmt::Debug for OverlayTextures {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_map()
            .entries(self.0.iter().map(|(k, t)| (k, (t.width, t.height))))
            .finish()
    }
}

/// The same keys over the same pictures (the same upload, not equal texels).
impl PartialEq for OverlayTextures {
    fn eq(&self, other: &Self) -> bool {
        self.0.len() == other.0.len()
            && self
                .0
                .iter()
                .zip(&other.0)
                .all(|((a, x), (b, y))| a == b && std::sync::Arc::ptr_eq(x, y))
    }
}

pub use dereth_primitives::{TextureData, TextureFormat};

impl Overlay {
    /// Upload `data` under `key`: blue, green, red, alpha texels, the first level only. Under a key
    /// of [`OverlaySpace::Image`] or [`OverlaySpace::Glyphs`] that is already there, the one there
    /// stays (the same key is the same picture); under one of [`OverlaySpace::Local`] it is
    /// replaced. Whether it was taken: a picture of another format, or with no texels for its
    /// size, is not.
    pub fn upload(&mut self, key: OverlayTexture, data: &TextureData) -> bool {
        let texels = u64::from(data.width) * u64::from(data.height) * 4;
        let whole = data
            .levels
            .first()
            .is_some_and(|l| l.len() as u64 == texels && texels > 0);
        if data.format != TextureFormat::Bgra8 || !whole {
            return false;
        }
        if key.space != OverlaySpace::Local && self.textures.0.contains_key(&key) {
            return true;
        }
        let first = TextureData {
            width: data.width,
            height: data.height,
            format: data.format,
            levels: data.levels[..1].to_vec(),
        };
        self.textures.0.insert(key, std::sync::Arc::new(first));
        true
    }

    /// Let the texture under `key` go.
    pub fn release(&mut self, key: OverlayTexture) -> OverlayReleased {
        if self.textures.0.remove(&key).is_some() {
            OverlayReleased::Freed
        } else {
            OverlayReleased::Absent
        }
    }

    /// A solid rectangle.
    pub fn fill(&mut self, rect: [f32; 4], color: [f32; 4]) {
        self.quads.push(Quad {
            rect,
            uv: [0.0, 0.0, 1.0, 1.0],
            color,
            picture: None,
        });
    }

    /// A picture of the world's data stretched over a rectangle.
    pub fn picture(&mut self, rect: [f32; 4], picture: u32, uv: [f32; 4], color: [f32; 4]) {
        self.quads.push(Quad {
            rect,
            uv,
            color,
            picture: Some(picture),
        });
    }
}

/// The game's interface, as the shell drives it.
pub trait Interface {
    /// A device event, before the world's controls see it: `true` takes it.
    fn event(&mut self, _event: &Event, _game: &Game<'_>) -> bool {
        false
    }

    /// Once a frame, after the client's messages took effect: what the interface asks.
    fn update(&mut self, game: &Game<'_>, requests: &mut Vec<Request>);

    /// Once a frame, after the world moved on and its frame was built (its camera this frame's,
    /// in [`Game::seen`]), before [`Interface::draw`]: what is placed over the world takes this
    /// frame's camera. What it asks is carried out at once.
    fn after_world(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}

    /// What to draw over the world this frame.
    fn draw(&mut self, _game: &Game<'_>, _overlay: &mut Overlay) {}

    /// Before each frame's view is refreshed, once the client's messages took effect: what the
    /// refresh is to answer (the client's actions that ask), carried out at once.
    fn before_view(&mut self, _requests: &mut Vec<Request>) {}

    /// Shown from now on, at the start of a frame: it was chosen in place of another (or was
    /// first shown, before any frame).
    fn enter(&mut self, _game: &Game<'_>) {}

    /// No longer shown, at the start of a frame, another chosen in its place: what it holds let
    /// go of, anything it asks carried out at once. It is kept, to be entered again.
    fn leave(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}

    /// Just after [`Interface::leave`]: let go of the textures it uploaded to the overlay, which
    /// the interface shown next does not draw. Anything it uploads when it is entered again is
    /// uploaded anew.
    fn release_textures(&mut self, _overlay: &mut Overlay) {}

    /// Whether the interface wants the pointer shown and free (a screen to click on): mouse look
    /// is suspended while it does.
    fn wants_pointer(&self, _game: &Game<'_>) -> bool {
        false
    }

    /// Whether a text entry has the keys: the world's controls let go of what is held when it
    /// takes them.
    fn text_mode(&self) -> bool {
        false
    }

    /// Whether the pointer, its buttons and the wheel reach it over the window while the window is
    /// in the background, and the click that brings the window forward counts. The game's own
    /// interface hears neither, as the game does not.
    fn hears_the_pointer_in_background(&self) -> bool {
        false
    }

    /// The client's random numbers, the one generator everything in the program draws from.
    fn set_random(&mut self, _random: &auberean_primitives::random::Random) {}

    /// The text the system's clipboard holds now, for a paste; none when it holds no text.
    fn set_clipboard(&mut self, _text: Option<String>) {}

    /// The text cut or copied since last asked, for the system's clipboard.
    fn take_copied(&mut self) -> Option<String> {
        None
    }
}

/// The interface with nothing in it: no screen, no request. The world's controls get every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoInterface;

impl Interface for NoInterface {
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}
}
