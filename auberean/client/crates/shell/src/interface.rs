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
//!    call the server's scripts), to disconnect or quit ([`Request`]);
//! 3. lets it draw ([`Interface::draw`]): pictures of its own the window's size, one behind the
//!    world and one in front of it (an interface whose screens hold the world in a viewport draws
//!    its deeper layers behind and its nearer ones in front), and rectangles in window pixels, each
//!    a solid color or a picture of the world's data, drawn in order over everything
//!    ([`Overlay`]).
//!
//! The interface sees the game through [`Game`]: the phase of the session, the client (its model:
//! the character list, the player, every object and its qualities), the world's records (pictures,
//! strings, layouts), the window's size, the time, the scene's status. It never reaches the
//! platform or the device: a host draws its overlay, and the shell carries out its requests.

use core::net::SocketAddr;

use auberean_client_sdk::{Action, Client};
use auberean_scene::{Status, WorldData};

pub use auberean_scene::view::{ViewCamera, ViewFigure, ViewLight, ViewScene};

use crate::input::Event;

/// Where the session is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// No server: before connecting, or after the session ended.
    Disconnected,
    /// Connecting: the login sent, the character list not here yet.
    Connecting,
    /// The account's characters are listed: choosing, making or deleting one.
    Characters,
    /// Entering the world as a character: asked, the player not here yet.
    Entering,
    /// In the world.
    World,
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
    /// Close the program.
    Quit,
    /// Set the camera: the interface's camera panel decides mouse look and first person, and
    /// zooms by the wheel over the world.
    Camera(CameraRequest),
    /// Find the body under the pointer (window pixels) in the world as last drawn: the answer
    /// comes with the next frame's game ([`Game::world_pick`]).
    PickWorld { x: f32, y: f32 },
    /// The graphics preferences the player applied: the world is drawn by them from now on, and
    /// they are kept with the settings.
    Graphics(auberean_draw::quality::Graphics),
    /// A movement action the interface asks for as a key would (the camera's arrows, its left
    /// button in mouse look): pressed with an extent of 1, let go with 0.
    Move { action: u32, extent: f32 },
    /// The key maps the player applied on the key map screen, each a list of an input and its
    /// action (keys alone, with Shift, with Ctrl): written to their files and bound from now on.
    KeyMaps([Vec<(u32, u32)>; 3]),
}

/// What an interface asks of the camera.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CameraRequest {
    /// Mouse look on or off.
    MouseLook(bool),
    /// First person on or off.
    FirstPerson(bool),
    /// A step of the zoom: in, toward the player, or out.
    Zoom { inward: bool },
}

/// The game as an interface sees it.
#[derive(Clone, Copy)]
pub struct Game<'a> {
    pub phase: Phase,
    /// The client, while there is a session.
    pub client: Option<&'a Client>,
    pub world: &'a WorldData,
    /// The window's drawing area, in pixels.
    pub size: (u32, u32),
    /// Seconds since the program started.
    pub now: f64,
    /// The world's status (the player's place, the bodies drawn).
    pub status: &'a Status,
    /// The object the world's controls have selected (0 for none).
    pub selected: u64,
    /// What the player heard since the last frame, oldest first.
    pub heard: &'a [Heard],
    /// The lines the player's scripts wrote for the chat windows since the last frame, oldest
    /// first.
    pub shown: &'a [Shown],
    /// What the last frame's picks met: per viewport scene that asked, its index in the
    /// overlay's scenes and the figure under the pointer, with the kind of mesh met there.
    pub picked: &'a [(usize, Option<auberean_scene::view::ViewHit>)],
    /// The notices the player's scripts sent the interface's panels since the last frame, oldest
    /// first.
    pub notices: &'a [Notice],
    /// What the last world frame showed: its camera and the bodies drawn in it.
    pub seen: &'a auberean_scene::seen::Seen,
    /// The answer to the last frame's [`Request::PickWorld`]: the body under the pointer, or
    /// none; `None` when no pick was asked.
    pub world_pick: Option<Option<u64>>,
    /// The key maps bound and what their actions do.
    pub keys: &'a crate::keys::Keys,
}

/// A notice the player's scripts sent the interface's panels: the native that sent it, the object
/// it is about, and the words of its other arguments, with a copy of the native object each word
/// names (none for a plain value), and the whole examination request a word names (its packed
/// copy leaves out what its kind does not pack).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    pub native: u32,
    pub about: Option<u64>,
    pub words: Vec<u32>,
    pub objects: Vec<Option<auberean_client_sdk::script::native_object::NativeObject>>,
    pub requests: Vec<Option<auberean_client_sdk::runtime::examination::Request>>,
    /// For a profile, the packed profile of each of its effect lines' own lines, by line.
    pub effect_lines: Vec<Vec<(usize, Vec<u8>)>>,
    /// For a list of references, a copy of the native object each item names.
    pub listed: Vec<Vec<Option<auberean_client_sdk::script::native_object::NativeObject>>>,
}

/// A line the player's scripts wrote for the chat windows, as the game's text: what kind of text
/// it is (each chat window shows some kinds) and the object it was shown about.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Shown {
    pub kind: u32,
    pub about: Option<u64>,
    pub text: String,
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
/// front, then the rectangles in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Overlay {
    /// Drawn first, where the world does not cover it.
    pub behind: Option<std::sync::Arc<Canvas>>,
    /// Drawn over the world and under the scenes, blended by its alpha: in the world, the
    /// elements deeper than the viewports showing scenes (a panel's figure stands over them).
    pub middle: Option<std::sync::Arc<Canvas>>,
    /// Drawn over the world and the scenes, blended by its alpha.
    pub front: Option<std::sync::Arc<Canvas>>,
    pub quads: Vec<Quad>,
    /// The scenes shown in viewports, drawn over `behind` and `middle` (what is drawn there
    /// stays where the scene draws nothing) and under `front`.
    pub scenes: Vec<ViewScene>,
    /// The pointer's picture over the window, when the interface sets one.
    pub cursor: Option<std::sync::Arc<CursorImage>>,
    /// The size the interface lays its screen out at, when it is not the window's: its pictures
    /// fill the window, and the viewports' rectangles are in its pixels, their scenes stretched
    /// with it.
    pub screen: Option<(u32, u32)>,
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

impl Overlay {
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

    /// Before each frame's [`Interface::update`], once the client's messages took effect: the
    /// client to ask what the interface's panels need of the player's scripts.
    fn prepare(&mut self, _client: &mut Client) {}

    /// Whether the interface wants the pointer shown and free (a screen to click on): mouse look
    /// is suspended while it does.
    fn wants_pointer(&self, _game: &Game<'_>) -> bool {
        false
    }

    /// The graphics preferences the client starts with (read from the settings, else the
    /// highest quality's), as applied.
    fn set_graphics(&mut self, _graphics: &auberean_draw::quality::Graphics) {}

    /// The client's random numbers, the one generator everything in the program draws from.
    fn set_random(&mut self, _random: &auberean_primitives::random::Random) {}
}

/// The interface with nothing in it: no screen, no request. The world's controls get every event.
#[derive(Clone, Copy, Debug, Default)]
pub struct NoInterface;

impl Interface for NoInterface {
    fn update(&mut self, _game: &Game<'_>, _requests: &mut Vec<Request>) {}
}
