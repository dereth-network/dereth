//! What the journal window, the book window and the story's scenes show of the game: the player's
//! quests (from the view's journal), the story's acts and their scenes (from the view's acts), the
//! book the scripts last opened, and the scenes they asked to play (from the notices).

use auberean_client_sdk::contract::notice::{
    BookNotice, InterfaceNotice, QuestNotice, Said, Scenes,
};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::contract::view::View;

use crate::state::{Frame, GameState};

/// A quest's status, as the scripts give it.
pub mod status {
    pub const UNDER_WAY: u32 = 0x1000_0000;
    pub const FAILED: u32 = 0x2000_0000;
    pub const COMPLETED: u32 = 0x3000_0000;
    pub const CANCELLED: u32 = 0x8000_0000;
}

/// How a journal notice says the quests changed: one quest, the whole list, one added, one
/// taken out.
pub mod change {
    pub const ONE: u32 = 0x4000_0001;
    pub const ALL: u32 = 0x4000_0002;
    pub const ADDED: u32 = 0x4000_0003;
    pub const REMOVED: u32 = 0x4000_0004;
}

/// How an acts notice says the whole story was given, rather than one act changed.
pub const WHOLE_STORY: u32 = 0x4000_0002;

/// A quest, as the journal window shows it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Quest {
    pub id: u32,
    pub name: String,
    pub description: String,
    pub journal: String,
    pub status: u32,
    /// How hard it is; none for any level.
    pub challenge: Option<i32>,
    /// When it was given, in game seconds.
    pub given: f64,
    /// It may be taken no more.
    pub maxed_out: bool,
    /// The game time it runs out at (under way) or may be taken again (finished); none for no
    /// time.
    pub until: Option<f64>,
    pub cancellable: bool,
}

impl Quest {
    /// Whether it is under way (else it is finished: completed, failed or given up).
    #[must_use]
    pub const fn under_way(&self) -> bool {
        self.status == status::UNDER_WAY
    }

    /// Whether the player may give it up: under way, and the game lets it.
    #[must_use]
    pub const fn may_give_up(&self) -> bool {
        self.under_way() && self.cancellable
    }

    /// How it ended, for a quest finished.
    #[must_use]
    pub const fn ending(&self) -> Option<&'static str> {
        match self.status {
            status::COMPLETED => Some("Completed"),
            status::FAILED => Some("Failed"),
            status::CANCELLED => Some("Abandoned"),
            _ => None,
        }
    }
}

/// A scene of an act, as the story page lists it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SceneRow {
    pub id: u32,
    pub name: String,
    /// The player has seen it, so it may be played again.
    pub playable: bool,
}

/// An act of the story, as the story page lists it: its scenes not hidden, in order.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Act {
    pub id: u32,
    pub name: String,
    pub scenes: Vec<SceneRow>,
}

/// A book open: its title, its text (pages ending `<BR>`, each with a heading between `<H>` and
/// `</H>`), its picture, whether it shows its page controls; and how many books have opened, so
/// a book opened again opens at its first page.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Book {
    pub title: String,
    pub text: String,
    pub picture: u32,
    pub controls: bool,
    pub opened: u64,
}

/// A story's scenes the scripts asked to play: the scenes, and whether Escape may cut them short.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneAsk {
    pub scenes: Scenes,
    pub skippable: bool,
}

/// What a frame asked of the story's scenes.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneOrder {
    /// Play these scenes (unless some already play).
    Play(SceneAsk),
    /// A scene on the way through a portal: one plays from now until the player arrives
    /// (`playing`), or not; and its scenes.
    Portal { playing: bool, scenes: Scenes },
}

/// The journal window's part of the game.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct JournalState {
    /// The quests under way, then those finished, each newest given first.
    pub quests: Vec<Quest>,
    /// The quests the scripts said changed this frame (one given, one changing), in order.
    pub changed_now: Vec<u32>,
    /// The story's acts, in the scripts' order; none until the scripts list them.
    pub acts: Option<Vec<Act>>,
    /// The acts the scripts said changed this frame.
    pub acts_changed_now: Vec<u32>,
    /// The book the scripts last opened.
    pub book: Option<Book>,
    /// What this frame asked of the story's scenes, in order.
    pub scene_orders: Vec<SceneOrder>,
    /// The subtitles of the scenes asked for, each written once its strings are at hand.
    pub subtitles: Vec<(TextRef, Option<String>)>,
    /// The player's body came to be placed in the world this frame (after a portal, say).
    pub arrived: bool,
    /// How many times it has.
    pub arrivals: u32,
    journal_version: Option<u64>,
    acts_version: Option<u64>,
    books: u64,
}

/// The quests from the view's journal: under way first, then finished, each newest given first;
/// each time left counted from `now` (game seconds).
#[must_use]
pub fn quests(view: &View, write: &dyn Fn(&TextRef) -> Option<String>) -> Vec<Quest> {
    let Some(j) = &view.journal else {
        return Vec::new();
    };
    let now = view.clock.game_time;
    let text = |t: &TextRef| write(t).unwrap_or_default().trim().to_owned();
    let mut all: Vec<Quest> = j
        .quests
        .iter()
        .filter(|q| {
            matches!(
                q.status,
                status::UNDER_WAY | status::COMPLETED | status::FAILED | status::CANCELLED
            )
        })
        .map(|q| Quest {
            id: q.id,
            name: text(&q.name),
            description: text(&q.description),
            journal: text(&q.journal),
            status: q.status,
            challenge: (q.challenge != -999).then_some(q.challenge),
            given: q.given,
            maxed_out: q.maxed_out,
            until: (q.seconds_remaining > 0).then(|| now + f64::from(q.seconds_remaining)),
            cancellable: q.cancellable,
        })
        .collect();
    // Stable: the scripts' order within each, newest given first.
    all.sort_by(|a, b| {
        b.under_way()
            .cmp(&a.under_way())
            .then(b.given.total_cmp(&a.given))
    });
    all
}

/// A span of game seconds as the journal writes a time left: days and hours past a day, hours
/// and minutes past an hour, else minutes and seconds.
#[must_use]
pub fn time_left(seconds: f64) -> String {
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let s = seconds.max(0.0).ceil() as u64;
    let (d, h, m, s) = (s / 86_400, s / 3_600 % 24, s / 60 % 60, s % 60);
    if d > 0 {
        format!("{d}d {h}h")
    } else if h > 0 {
        format!("{h}h {m:02}m")
    } else {
        format!("{m}m {s:02}s")
    }
}

impl GameState {
    /// The journal window's, the book window's and the story's part of one frame.
    pub(crate) fn follow_journal(&mut self, f: &Frame<'_>) {
        let j = &mut self.journal;
        j.scene_orders.clear();
        j.changed_now.clear();
        j.acts_changed_now.clear();
        match f.view {
            Some(view) => {
                let version = view.journal.as_ref().map(|j| j.version);
                if version != j.journal_version {
                    j.journal_version = version;
                    j.quests = quests(view, f.write);
                }
                let acts = view.acts.as_ref().map(|a| a.version);
                if acts != j.acts_version {
                    j.acts_version = acts;
                    let text = |t: &TextRef| (f.write)(t).unwrap_or_default().trim().to_owned();
                    j.acts = view.acts.as_ref().map(|a| {
                        a.acts
                            .iter()
                            .filter(|a| a.visible)
                            .map(|a| Act {
                                id: a.id,
                                name: text(&a.name),
                                scenes: a
                                    .scenes
                                    .iter()
                                    .filter(|s| !s.hidden)
                                    .map(|s| SceneRow {
                                        id: s.id,
                                        name: text(&s.name),
                                        playable: s.playable,
                                    })
                                    .collect(),
                            })
                            .collect()
                    });
                }
            }
            None => {
                *j = JournalState {
                    books: j.books,
                    arrivals: j.arrivals,
                    subtitles: std::mem::take(&mut j.subtitles),
                    ..JournalState::default()
                };
            }
        }
        let write = |t: &Option<TextRef>| t.as_ref().and_then(f.write).unwrap_or_default();
        for n in f.notices {
            match &n.said {
                Said::Quest(QuestNotice::Journal { change, quest, .. }) => {
                    if let (change::ONE | change::ADDED, Some(q)) = (*change, quest) {
                        j.changed_now.push(q.id);
                    }
                }
                Said::Quest(QuestNotice::Acts { change, act }) => {
                    if *change != WHOLE_STORY {
                        j.acts_changed_now.push(*act);
                    }
                }
                Said::Quest(QuestNotice::Scenes(Some(s))) => {
                    j.scene_orders.push(SceneOrder::Play(SceneAsk {
                        scenes: s.clone(),
                        skippable: true,
                    }));
                }
                Said::Interface(InterfaceNotice::PortalScene { playing, scenes }) => {
                    j.scene_orders.push(SceneOrder::Portal {
                        playing: *playing,
                        scenes: scenes.clone().unwrap_or_default(),
                    });
                }
                Said::Book(BookNotice::Open {
                    controls,
                    title,
                    text,
                    picture,
                    ..
                }) => {
                    j.books += 1;
                    j.book = Some(Book {
                        title: write(title),
                        text: write(text),
                        picture: *picture,
                        controls: *controls,
                        opened: j.books,
                    });
                }
                _ => {}
            }
        }
        // The subtitles of the scenes asked for kept to be written, the latest few hundred.
        for order in &j.scene_orders {
            let scenes = match order {
                SceneOrder::Play(a) => &a.scenes,
                SceneOrder::Portal { scenes, .. } => scenes,
            };
            for t in scenes
                .scenes
                .iter()
                .flat_map(|s| &s.keyframes)
                .filter_map(|k| k.subtitle.as_ref())
            {
                if !j.subtitles.iter().any(|(s, _)| s == t) {
                    j.subtitles.push((t.clone(), None));
                }
            }
        }
        let over = j.subtitles.len().saturating_sub(SUBTITLES_KEPT);
        j.subtitles.drain(..over);
        for (t, written) in j.subtitles.iter_mut().filter(|(_, w)| w.is_none()) {
            *written = (f.write)(t).filter(|w| !w.trim().is_empty());
        }
    }
}

/// How many scenes' subtitles are kept to be written.
const SUBTITLES_KEPT: usize = 256;

impl JournalState {
    /// A subtitle written, once its strings are at hand.
    #[must_use]
    pub fn subtitle(&self, t: &TextRef) -> Option<&str> {
        self.subtitles
            .iter()
            .find(|(s, _)| s == t)
            .and_then(|(_, w)| w.as_deref())
    }
}
