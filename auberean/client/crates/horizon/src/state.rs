//! The game as Horizon draws it: its own plain state, followed once a frame from the game's view,
//! the scripts' notices, the client's events and the chat windows' lines.

pub mod craft;
pub mod goods;
pub mod pets;

use std::collections::VecDeque;

use auberean_client_sdk::contract::event::{ChatEvent, GameEvent};
use auberean_client_sdk::contract::notice::{vital, Notice, PlayerNotice, Said, SelectionNotice};
use auberean_client_sdk::contract::question::Asked;
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::{self as v, View};
use auberean_client_sdk::contract::ChatHistory;

pub use crate::abilities::{Bar, Combat, Light, Skills, Slot};
pub use crate::examined::Examined;

/// How many lines the log keeps.
pub const LOG_LINES: usize = 400;

/// The kind of text a line the client tells the player is: an error's.
pub const ERROR_TEXT: u32 = 0x1;

/// The kind of text a line a plugin printed is: this client's own, past the game's kinds.
pub const PLUGIN_TEXT: u32 = 0x8000_0000;

/// A vital: its value and its greatest.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Vital {
    pub current: u64,
    pub max: u64,
}

impl Vital {
    /// How full it is, 0 to 1 (empty with no greatest).
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn ratio(self) -> f32 {
        if self.max == 0 {
            0.0
        } else {
            (self.current as f32 / self.max as f32).clamp(0.0, 1.0)
        }
    }
}

/// An effect on the player, as the status row shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Effect {
    pub id: u32,
    pub name: String,
    /// Its icon, a picture of the game's records (0 for none).
    pub icon: u32,
    /// Game seconds until it runs out; none for one that lasts.
    pub left: Option<f64>,
}

/// What is selected, as the target bar shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct Target {
    pub id: u64,
    pub name: String,
    /// Its health and its greatest, once the scripts told them.
    pub health: Option<Vital>,
}

/// The link as the link line shows it: the world's name, the machine's clock, the round trip, the
/// share of packets lost and how lately the server was heard.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Link {
    pub world: Option<String>,
    /// The hour (0 to 23) and the minute.
    pub clock: Option<(u8, u8)>,
    /// The average round trip, in milliseconds, once one was measured.
    pub round_trip_ms: Option<f64>,
    /// The share of packets lost, 0 to 1.
    pub loss: f32,
    /// Seconds since the world's server was last heard; none with no connection to it.
    pub since_heard: Option<f64>,
}

impl Link {
    /// The link as the view gives it.
    #[must_use]
    pub fn of(view: &View) -> Self {
        let world = [
            view.player.as_ref().map(|p| p.world_name.as_str()),
            Some(view.pregame.world_name.as_str()),
        ]
        .into_iter()
        .flatten()
        .find(|w| !w.is_empty())
        .map(str::to_owned);
        #[allow(clippy::cast_possible_truncation)] // a share, 0 to 1
        let loss = view.link.map_or(0.0, |h| h.loss as f32);
        Self {
            world,
            clock: view.local_clock,
            round_trip_ms: view
                .link
                .filter(|h| h.latency > 0.0)
                .map(|h| h.latency * 1000.0),
            loss,
            since_heard: view.since_heard,
        }
    }

    /// The clock as the link line writes it: `9:57 PM`.
    #[must_use]
    pub fn clock_text(&self) -> Option<String> {
        let (h, m) = self.clock?;
        let (h12, half) = match h {
            0 => (12, "AM"),
            1..=11 => (h, "AM"),
            12 => (12, "PM"),
            _ => (h - 12, "PM"),
        };
        Some(format!("{h12}:{m:02} {half}"))
    }
}

/// A line of the log: the kind of text it is and its words.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Line {
    pub kind: u32,
    pub text: String,
}

/// What a frame brings: the view as last refreshed, the scripts' notices, the client's events,
/// what the client tells the player and the lines for the chat windows, in order.
#[derive(Clone, Copy)]
pub struct Frame<'a> {
    pub view: Option<&'a View>,
    pub notices: &'a [Notice],
    pub events: &'a [GameEvent],
    pub told: &'a [String],
    /// The chat windows' lines: each its kind of text and its words.
    pub lines: &'a [(u32, String)],
    /// A name the view gives only as a reference, written out; none while its strings are not
    /// at hand.
    pub write: &'a dyn Fn(&TextRef) -> Option<String>,
    /// The line a typed line naming no command is answered with.
    pub unknown_command: &'a str,
}

impl std::fmt::Debug for Frame<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Frame")
            .field("notices", &self.notices.len())
            .field("events", &self.events.len())
            .field("lines", &self.lines.len())
            .finish_non_exhaustive()
    }
}

/// The game as Horizon draws it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct GameState {
    /// The player's object is here.
    pub in_world: bool,
    pub player: Option<u64>,
    pub health: Vital,
    pub vigor: Vital,
    /// The player is a hero, as the view last said: only a hero has focus, and sees it.
    pub hero: bool,
    pub focus: Vital,
    /// The player's way through its level: the experience past the level's start, of what the
    /// next level takes from there, as the player's scripts last said.
    pub experience: Vital,
    /// The player's level, its experience in all and to spend, and its vitae.
    pub progress: v::Progress,
    /// The link to the server.
    pub link: Link,
    pub effects: Vec<Effect>,
    /// The game's time, which the effects run out on.
    pub game_time: f64,
    pub target: Option<Target>,
    /// Where the selection is on the screen, as the last world frame drew it; none with nothing
    /// selected or nothing of it drawn.
    pub selection_at: Option<crate::ui::marker::Where>,
    /// The selection is ringed on the ground: a player or a creature. A thing (an item, a door,
    /// something wielded) is marked by its name and the arrow alone.
    pub ringed: bool,
    /// The log, oldest first.
    pub log: VecDeque<Line>,
    /// How many lines have come to the log since the game began, those let go of too.
    pub said: u64,
    /// The chat's part of the player's options, as the view last gave it: what each chat window
    /// shows and the channel it is given over to.
    pub chat_options: Option<v::ChatOptions>,
    /// The chat entry's lines asked for this frame, in order: the line, or none where the list
    /// had nothing more that way.
    pub recalled: Vec<Option<String>>,
    /// The shortcut bar, as the view last gave it.
    pub bar: Option<Bar>,
    /// Attacking, and the special attacks the shortcut bar lights.
    pub combat: Combat,
    /// The skill window's panel, its skills and the player's points.
    pub skills: Option<Skills>,
    /// What is examined and its profile.
    pub examined: Examined,
    /// What the minimap shows.
    pub radar: Radar,
    /// What the map window shows.
    pub map: crate::atlas::MapState,
    /// The quests, the story's acts, the book open and the scenes asked for.
    pub journal: crate::journal::JournalState,
    /// The game's questions open, in the order they were asked.
    pub questions: Vec<Asking>,
    /// The player's fellowship, allegiance, friends and kingdom.
    pub social: crate::social::Social,
    /// The player's things: the pack, what is worn, the containers open, a trade, the shops.
    pub goods: goods::Goods,
    /// The craft skills and their recipes, and the recipe opened for making.
    pub craft: craft::Crafting,
    /// The player's pets and its mount.
    pub pets: pets::Pets,
    /// The vitals the view last gave, so a notice since is not undone by the same view.
    view_vitals: Option<[u32; 4]>,
    /// The focus the view last gave, for the same.
    view_focus: Option<[u32; 2]>,
}

impl GameState {
    /// Follow one frame.
    pub fn follow(&mut self, f: &Frame<'_>) {
        let view = f.view;
        let player = view.and_then(|v| v.player.as_ref());
        self.in_world = player.is_some();
        self.player = view.and_then(|v| v.player_id);
        if let Some(v) = player.and_then(|p| p.vitals) {
            if self.view_vitals != Some(v) {
                self.view_vitals = Some(v);
                let [health, max_health, vigor, max_vigor] = v;
                self.health = Vital {
                    current: health.into(),
                    max: max_health.into(),
                };
                self.vigor = Vital {
                    current: vigor.into(),
                    max: max_vigor.into(),
                };
            }
        }
        self.hero = player.is_some_and(|p| p.hero);
        if let Some(focus) = player.and_then(|p| p.focus) {
            if self.view_focus != Some(focus) {
                self.view_focus = Some(focus);
                self.focus = Vital {
                    current: focus[0].into(),
                    max: focus[1].into(),
                };
            }
        }
        self.follow_target(f);
        self.follow_radar(f);
        self.follow_questions(f);
        self.goods.follow(f.view, f.notices, f.write);
        self.craft.follow(f.view, f.notices, f.write);
        self.pets.follow(f.view, f.notices, f.write);
        for n in f.notices {
            self.notice(n);
        }
        crate::abilities::follow(self, f);
        crate::examined::follow(self, f);
        crate::social::follow(self, f);
        self.follow_journal(f);
        self.link = view.map(Link::of).unwrap_or_default();
        if let Some(p) = player {
            self.progress = p.progress;
        }
        if let Some(v) = view {
            let chat = v.options.as_ref().map(|o| &o.chat);
            if self.chat_options.as_ref() != chat {
                self.chat_options = chat.cloned();
            }
            self.game_time = v.clock.game_time;
            if let Some(e) = &v.effects {
                self.effects = e
                    .effects
                    .iter()
                    .map(|e| Effect {
                        id: e.id,
                        name: e.name.clone(),
                        icon: e.icon,
                        left: (e.duration >= 0.0)
                            .then(|| (e.time_cast + e.duration - v.clock.game_time).max(0.0)),
                    })
                    .collect();
            }
        }
        self.recalled.clear();
        for line in f.told {
            self.say(ERROR_TEXT, line.clone());
        }
        for e in f.events {
            match e {
                GameEvent::Chat(ChatEvent::UnknownCommand { .. }) => {
                    self.say(ERROR_TEXT, f.unknown_command.to_owned());
                }
                GameEvent::Chat(ChatEvent::History {
                    list: ChatHistory::Commands,
                    text,
                }) => self.recalled.push(text.clone()),
                _ => {}
            }
        }
        for (kind, text) in f.lines {
            self.say(*kind, text.clone());
        }
    }

    /// The lines the plugins printed, added to the log in their own color.
    pub fn plugins_said(&mut self, lines: &[String]) {
        for line in lines {
            self.say(PLUGIN_TEXT, line.clone());
        }
    }

    /// A line added to the log, the oldest let go past [`LOG_LINES`]. The game's texts end in
    /// the line break that starts the next one in its own log, where each is written on after
    /// the last; here each line is a row of its own already, so one break at the end is dropped.
    /// A blank line inside the text is kept.
    pub fn say(&mut self, kind: u32, mut text: String) {
        if text.ends_with('\n') {
            text.pop();
            if text.ends_with('\r') {
                text.pop();
            }
        }
        self.log.push_back(Line { kind, text });
        self.said += 1;
        while self.log.len() > LOG_LINES {
            self.log.pop_front();
        }
    }

    /// The selection as the view has it, named.
    fn follow_target(&mut self, f: &Frame<'_>) {
        let Some(world) = f.view.and_then(|v| v.world.as_ref()) else {
            self.target = None;
            self.ringed = false;
            return;
        };
        let id = world.selection.current;
        if id == 0 {
            self.target = None;
            self.ringed = false;
            return;
        }
        let thing = world.things.iter().find(|t| t.id == id);
        self.ringed = thing.is_some_and(|t| ringed(t.kind));
        let name = thing
            .and_then(|t| {
                t.name
                    .clone()
                    .filter(|n| !n.is_empty())
                    .or_else(|| t.name_ref.as_ref().and_then(f.write))
                    .or_else(|| (f.write)(&t.own_name))
            })
            .or_else(|| {
                (Some(id) == f.view.and_then(|v| v.player_id))
                    .then(|| f.view.and_then(|v| v.player.as_ref()?.name.clone()))
                    .flatten()
            })
            .unwrap_or_default();
        match &mut self.target {
            Some(t) if t.id == id => t.name = name,
            _ => {
                self.target = Some(Target {
                    id,
                    name,
                    health: None,
                });
            }
        }
    }

    fn notice(&mut self, n: &Notice) {
        match &n.said {
            Said::Player(p) if n.about.is_none() || n.about == self.player => match *p {
                PlayerNotice::Vital {
                    vital: which,
                    current,
                    max,
                } => {
                    let v = Vital { current, max };
                    match which {
                        vital::HEALTH => self.health = v,
                        vital::VIGOR => self.vigor = v,
                        vital::FOCUS => self.focus = v,
                        vital::EXPERIENCE => self.experience = v,
                        _ => {}
                    }
                }
                PlayerNotice::Hero {
                    focus, max_focus, ..
                } => {
                    self.focus = Vital {
                        current: focus,
                        max: max_focus,
                    };
                }
                _ => {}
            },
            // The selection's vitals come in a notice about the player.
            Said::Selection(SelectionNotice::Vitals(Some(v))) => {
                let about_player = n.about.is_none() || n.about == self.player;
                if let Some(t) = &mut self.target {
                    if about_player {
                        t.health = Some(Vital {
                            current: v.health.into(),
                            max: v.max_health.into(),
                        });
                    }
                }
            }
            _ => {}
        }
    }
}

/// Whether an object of kind bits `kind` is ringed on the ground when selected: a player or a
/// creature, as the selection keys tell them.
#[must_use]
pub const fn ringed(kind: u32) -> bool {
    auberean_client_sdk::contract::view::kind::is_being(kind)
}

/// What the minimap shows: where the player faces, how far it reaches, where the player stands,
/// and the objects on it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Radar {
    /// The player's heading, degrees clockwise from north.
    pub heading: f32,
    /// How far it reaches where the player stands, in meters.
    pub range: f32,
    /// The player stands inside (a building, a dungeon).
    pub inside: bool,
    /// The player's map coordinates outdoors: east and north.
    pub coordinates: Option<(f32, f32)>,
    /// The player chose to see the coordinates.
    pub show_coordinates: bool,
    /// The objects on it within its reach, each where it stands from the player.
    pub blips: Vec<Blip>,
}

/// An object on the minimap.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Blip {
    pub id: u64,
    /// Where it stands from the player, in meters: east and north.
    pub east: f32,
    pub north: f32,
    /// Its color and its shape, as the player's scripts give them.
    pub colour: u32,
    pub shape: u32,
    pub name: String,
    /// It is the selection.
    pub selected: bool,
}

/// A color as the scripts give it (red, green, blue, alpha, each 0 to 1), as one word: alpha,
/// red, green, blue.
#[must_use]
pub fn argb(c: [f32; 4]) -> u32 {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let byte = |v: f32| (f64::from(v) * 255.0).clamp(0.0, 255.0) as u32;
    byte(c[3]) << 24 | byte(c[0]) << 16 | byte(c[1]) << 8 | byte(c[2])
}

/// One of the game's questions, as Horizon asks it.
#[derive(Clone, Debug, PartialEq)]
pub struct Asking {
    /// Its number, which the answer names.
    pub id: u32,
    pub asked: Asked,
    /// Its words.
    pub text: String,
    /// It has one answer, rather than yes or no.
    pub one_answer: bool,
    /// It goes in front of the others.
    pub at_once: bool,
}

/// The words for a question whose own the game can't write: its kind's, with the names it carries.
fn plain_words(asked: &Asked, write: &dyn Fn(&TextRef) -> Option<String>) -> String {
    let name = |t: &Option<TextRef>| {
        t.as_ref()
            .and_then(write)
            .filter(|n| !n.is_empty())
            .unwrap_or_else(|| "Someone".to_owned())
    };
    match asked {
        Asked::Died => "You have died.".to_owned(),
        Asked::Resurrection { by } => {
            format!("{} has offered to resurrect you. Do you accept?", name(by))
        }
        Asked::Fellowship {
            leader_name,
            fellowship,
            ..
        } => {
            let fellowship = fellowship
                .as_ref()
                .and_then(write)
                .filter(|n| !n.is_empty());
            match fellowship {
                Some(f) => format!(
                    "{} invites you to join the {f} fellowship. Will you join?",
                    name(leader_name)
                ),
                None => format!(
                    "{} invites you to join a fellowship. Will you join?",
                    name(leader_name)
                ),
            }
        }
        Asked::Oath { name: who, .. } => {
            format!("{} offers allegiance. Do you accept?", name(who))
        }
        Asked::Trade { name: who, .. } => {
            format!("{} wishes to trade with you. Do you accept?", name(who))
        }
        _ => String::new(),
    }
}

impl GameState {
    /// The minimap from the view: the objects the player's scripts put on the radar within its
    /// reach, each where it stands from the player (what a body holds, where the body stands).
    fn follow_radar(&mut self, f: &Frame<'_>) {
        let (Some(view), Some(world)) = (f.view, f.view.and_then(|v| v.world.as_ref())) else {
            self.radar = Radar::default();
            return;
        };
        let Some(me) = view.player.as_ref().map(|p| p.body) else {
            self.radar = Radar::default();
            return;
        };
        let range = world.radar_range();
        let from = (me.cell, me.origin);
        let place_of = |t: &v::Thing| {
            t.place.or_else(|| {
                let parent = t.parent?;
                world.things.iter().find(|p| p.id == parent)?.place
            })
        };
        let mut blips = Vec::new();
        for t in &world.things {
            let Some((colour, shape)) = t.radar else {
                continue;
            };
            if t.id == world.player {
                continue;
            }
            let Some(at) = place_of(t) else { continue };
            let [east, north, _] = v::offset(from, at);
            if east * east + north * north > range * range {
                continue;
            }
            let name = t
                .name
                .clone()
                .filter(|n| !n.is_empty())
                .or_else(|| t.name_ref.as_ref().and_then(f.write))
                .unwrap_or_default();
            blips.push(Blip {
                id: t.id,
                east,
                north,
                colour: argb(colour),
                shape,
                name,
                selected: t.id == world.selection.current,
            });
        }
        self.radar = Radar {
            heading: me.heading,
            range,
            inside: world.inside,
            coordinates: world.map_coordinates,
            show_coordinates: world.show_coordinates,
            blips,
        };
    }

    /// The game's questions open, each with its words written (or its kind's plain words where
    /// the game's strings are not at hand).
    fn follow_questions(&mut self, f: &Frame<'_>) {
        let Some(view) = f.view else {
            self.questions.clear();
            return;
        };
        self.questions = view
            .questions
            .iter()
            .map(|q| {
                let written = (f.write)(&q.text).filter(|t| !t.trim().is_empty());
                Asking {
                    id: q.id,
                    asked: q.asked.clone(),
                    text: written.unwrap_or_else(|| plain_words(&q.asked, f.write)),
                    one_answer: q.one_answer(),
                    at_once: q.at_once,
                }
            })
            .collect();
    }
}
