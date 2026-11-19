//! The story's scenes, played over the whole screen in Horizon's frame: each step's picture
//! fading in over the last, its subtitle under it, its music and its voice.
//!
//! The game asks for scenes to play (the story page's Play asks it too), and for a scene on the
//! way through a portal. Asked, the world is hidden and the playable scenes' steps play in order:
//!
//! - a step with a picture shows it, fading in over the last; one without keeps the last;
//! - a step with music stops the music playing and plays its own; with a voice, the same;
//! - a step with a subtitle shows it under the picture; one without keeps the last;
//! - a step lasts its own time, five seconds when it has none; then the next plays.
//!
//! After the last step the music and the voice stop, the world shows again, and the game hears
//! which scenes were played. Scenes the game asked for may be cut short with Escape; a portal's
//! may not. While a portal's scene is on (from the game saying so until the player arrives) the
//! game is told nothing as scenes end; when the player arrives, what plays stops and the game
//! hears that the portal's scenes are done. Asked for a portal's scenes while none is on, the game
//! hears at once that they are done.

use auberean_audio::{Looping, PlayRequest, PlayType};
use auberean_client_sdk::contract::notice::{Keyframe, Scenes};
use auberean_client_sdk::contract::text::TextRef;
use auberean_client_sdk::Action;
use auberean_shell::interface::{Request, SoundOrder, SoundSlot};
use dereth_ui_kit::art::Family;
use dereth_ui_kit::paint::{Align, Painter, TextStyle};

use crate::draw::{with_alpha, Rect, WHITE};
use crate::journal::{SceneAsk, SceneOrder};
use crate::state::GameState;

/// How long a step lasts that says nothing of its own, in seconds.
pub const STEP: f64 = 5.0;
/// How long a new picture takes to fade in, in seconds.
pub const FADE_IN: f64 = 1.0;
/// Seconds a stopped sound fades out over.
pub const SOUND_FADE: f32 = 2.0;

/// What plays.
#[derive(Clone, Debug, PartialEq)]
pub struct Playing {
    pub scenes: Scenes,
    pub skippable: bool,
    steps: Vec<Keyframe>,
    /// The step showing, from 0.
    pub step: usize,
    /// When the step showing ends, on the interface's clock.
    ends_at: f64,
    /// The picture showing and the one before it, and when the one showing came.
    pub picture: Option<u32>,
    previous: Option<u32>,
    came_at: f64,
    /// The subtitle showing.
    pub subtitle: Option<TextRef>,
}

/// The story's scenes as Horizon plays them.
#[derive(Debug, Default)]
pub struct StoryPlayer {
    pub playing: Option<Playing>,
    /// A portal's scene plays, until the player arrives.
    pub portal: bool,
    /// Escape asked to cut the scenes short.
    skip: bool,
}

impl StoryPlayer {
    /// Whether scenes play.
    #[must_use]
    pub const fn is_playing(&self) -> bool {
        self.playing.is_some()
    }

    /// Whether Escape may cut them short.
    #[must_use]
    pub fn skippable(&self) -> bool {
        self.playing.as_ref().is_some_and(|p| p.skippable)
    }

    /// Cut them short at the next frame, when they may be.
    pub fn skip(&mut self) {
        if self.skippable() {
            self.skip = true;
        }
    }

    /// The scenes stopped where they are, as the interface is left: the world shown again, the
    /// sounds stopped, and the game told which scenes played.
    pub fn stop(&mut self, requests: &mut Vec<Request>, acts: &mut Vec<Action>) {
        if let Some(p) = self.playing.take() {
            finish(requests);
            acts.push(Action::PlayScenesDone {
                scenes: p.scenes.packed,
            });
        }
        self.portal = false;
        self.skip = false;
    }

    /// One frame at `now` (the interface's clock): what the game asked of the scenes, Escape's
    /// cut, the next step when the one showing is over; the world, sounds and the game's being
    /// told asked in `requests` and `acts`.
    pub fn update(
        &mut self,
        state: &GameState,
        now: f64,
        requests: &mut Vec<Request>,
        acts: &mut Vec<Action>,
    ) {
        for order in &state.journal.scene_orders {
            match order {
                SceneOrder::Play(ask) => {
                    if self.playing.is_none() {
                        self.start(ask.clone(), now, requests);
                    }
                }
                SceneOrder::Portal { playing, scenes } => {
                    if *playing {
                        self.portal = true;
                        requests.push(Request::PortalScenePlaying(true));
                    }
                    self.check_portal(scenes, now, requests, acts);
                }
            }
        }
        if state.journal.arrived && self.portal {
            self.portal = false;
            requests.push(Request::PortalScenePlaying(false));
            self.check_portal(&Scenes::default(), now, requests, acts);
        }
        if std::mem::take(&mut self.skip) {
            self.done(requests, acts);
            return;
        }
        let over = self.playing.as_ref().is_some_and(|p| now >= p.ends_at);
        if over {
            self.next(now, requests, acts);
        }
    }

    /// A portal's scenes: while none plays, what plays stops and the game hears the scenes
    /// played; while one does, they play unless some already do.
    fn check_portal(
        &mut self,
        scenes: &Scenes,
        now: f64,
        requests: &mut Vec<Request>,
        acts: &mut Vec<Action>,
    ) {
        if self.portal {
            if self.playing.is_none() {
                self.start(
                    SceneAsk {
                        scenes: scenes.clone(),
                        skippable: false,
                    },
                    now,
                    requests,
                );
            }
        } else {
            if self.playing.take().is_some() {
                finish(requests);
            }
            acts.push(Action::PlayScenesDone {
                scenes: scenes.packed.clone(),
            });
        }
    }

    /// The scenes begin: the world hidden, the first step.
    fn start(&mut self, ask: SceneAsk, now: f64, requests: &mut Vec<Request>) {
        let steps = ask
            .scenes
            .scenes
            .iter()
            .filter(|s| s.playable)
            .flat_map(|s| s.keyframes.iter().cloned())
            .collect();
        self.playing = Some(Playing {
            scenes: ask.scenes,
            skippable: ask.skippable,
            steps,
            step: 0,
            ends_at: now,
            picture: None,
            previous: None,
            came_at: now,
            subtitle: None,
        });
        requests.push(Request::ShowWorld(false));
        self.show_step(now, requests);
    }

    /// The step showing over: the next, or after the last the scenes done.
    fn next(&mut self, now: f64, requests: &mut Vec<Request>, acts: &mut Vec<Action>) {
        if let Some(p) = &mut self.playing {
            p.step += 1;
        }
        if self
            .playing
            .as_ref()
            .is_some_and(|p| p.step >= p.steps.len())
        {
            self.done(requests, acts);
        } else {
            self.show_step(now, requests);
        }
    }

    /// The scenes done: the world back, the sounds stopped; unless a portal's scene plays, the
    /// game hears which scenes were played.
    fn done(&mut self, requests: &mut Vec<Request>, acts: &mut Vec<Action>) {
        let Some(p) = self.playing.take() else { return };
        finish(requests);
        if !self.portal {
            acts.push(Action::PlayScenesDone {
                scenes: p.scenes.packed,
            });
        }
    }

    /// The step showing: its picture, its music and voice, its subtitle; how long it lasts. With
    /// no steps at all, the scenes end at the next frame.
    fn show_step(&mut self, now: f64, requests: &mut Vec<Request>) {
        let Some(p) = &mut self.playing else { return };
        let Some(k) = p.steps.get(p.step).cloned() else {
            p.ends_at = now;
            return;
        };
        if let Some(pic) = k.picture {
            if p.picture != Some(pic) {
                p.previous = p.picture;
                p.picture = Some(pic);
                p.came_at = now;
            }
        }
        if let Some(m) = k.music {
            requests.extend(sound(SoundSlot::StoryMusic, Some(m)));
        }
        if let Some(v) = k.voice {
            requests.extend(sound(SoundSlot::StoryVoice, Some(v)));
        }
        if k.subtitle.is_some() {
            p.subtitle.clone_from(&k.subtitle);
        }
        p.ends_at = now + k.seconds.unwrap_or(STEP);
    }

    /// Draw the scenes over the whole screen, their subtitles as `state` has them written; where
    /// they cover.
    pub fn draw(&self, p: &mut Painter<'_>, now: f64, state: &GameState) -> Option<Rect> {
        let playing = self.playing.as_ref()?;
        let k = p.scale;
        let screen = Rect::new(0.0, 0.0, p.screen.0, p.screen.1);
        p.fill(screen, 0xFF00_0000);
        // The picture in a frame, as large as the screen holds at its own shape (4 by 3).
        let h = (screen.h - 160.0 * k).min((screen.w - 80.0 * k) * 0.75);
        let w = h / 0.75;
        let frame = Rect::new((screen.w - w) / 2.0, 40.0 * k, w, h);
        kit_frame(p, frame.inset(-6.0 * k));
        #[allow(clippy::cast_possible_truncation)]
        let fade = (((now - playing.came_at) / FADE_IN).clamp(0.0, 1.0)) as f32;
        if let Some(prev) = playing.previous.and_then(|id| p.art.icon(id)) {
            p.sprite(&prev, frame, WHITE);
        }
        if let Some(pic) = playing.picture.and_then(|id| p.art.icon(id)) {
            p.sprite(&pic, frame, with_alpha(WHITE, fade));
        }
        if let Some(text) = playing
            .subtitle
            .as_ref()
            .and_then(|t| state.journal.subtitle(t))
        {
            let style = TextStyle::new(Family::Body, 18.0, 0xFFF5_EBD2).edge(0xFF00_0000);
            let lh = p.line_height(&style);
            let mut y = frame.bottom() + 16.0 * k;
            for line in p.wrap(&style, text, frame.w) {
                p.text_in(
                    &style,
                    Rect::new(frame.x, y, frame.w, lh),
                    Align::Centre,
                    &line,
                );
                y += lh;
            }
        }
        if playing.skippable {
            let small = TextStyle::new(Family::Body, 12.0, 0xA0FF_FFFF).edge(0xFF00_0000);
            p.text_in(
                &small,
                Rect::new(0.0, screen.h - 28.0 * k, screen.w - 16.0 * k, 20.0 * k),
                Align::Right,
                "Esc to skip",
            );
        }
        Some(screen)
    }
}

/// The frame round a scene's picture: Horizon's window frame where its art has one, else a rim.
fn kit_frame(p: &mut Painter<'_>, r: Rect) {
    if !p.frame("window", "window.tl", r, WHITE) {
        p.fill(r, 0xFF3A_3020);
    }
}

/// The world shown again, and the story's music and voice stopped.
fn finish(requests: &mut Vec<Request>) {
    requests.push(Request::ShowWorld(true));
    requests.extend(sound(SoundSlot::StoryMusic, None));
    requests.extend(sound(SoundSlot::StoryVoice, None));
}

/// A story's music (looping) or voice played alone, stopping the one before, fading out; or
/// stopped, fading out.
#[must_use]
pub fn sound(slot: SoundSlot, id: Option<u32>) -> Vec<Request> {
    let mut out = vec![Request::Sound(SoundOrder::Stop {
        slot,
        fade: SOUND_FADE,
    })];
    if let Some(id) = id {
        let mut data = PlayRequest::new(id);
        data.kind = PlayType::AllExclusive;
        data.looping = if slot == SoundSlot::StoryMusic {
            Looping::ForceOn
        } else {
            Looping::Inherit
        };
        out.push(Request::Sound(SoundOrder::Play {
            data,
            slot: Some(slot),
        }));
    }
    out
}
