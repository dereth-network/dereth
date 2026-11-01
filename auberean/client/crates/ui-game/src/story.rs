//! The story's scenes played over the screen: a scene's pictures one after another, each with
//! its music, its voice and its subtitle.
//!
//! The play screen makes [`Story`] when the scripts ask for scenes to be played (or the story page
//! plays one again), and when a scene plays on the way through a portal. Made, it puts up the
//! story's background (as large as the screen), its two picture frames and their subtitles, and
//! takes every step of the playable scenes in their order; mouse look is kept off while it is up.
//! Once the background has faded in and the steps' records are at hand, the world is hidden and
//! the steps play:
//!
//! - a step with a picture shows it in the frame not showing the last one (the frames take turns,
//!   so one picture fades into the next); one without keeps the last picture;
//! - a step with music stops the music playing and plays its own; one with a voice stops the voice
//!   speaking and plays its own;
//! - a step with a subtitle shows it under the picture; one without keeps the last;
//! - a step lasts its own time, five seconds when it has none; then the next plays.
//!
//! After the last step the music and voice stop, the world shows again and the play screen hears
//! that the scenes are done (it tells the scripts which scenes were played). A story the scripts
//! or the story page asked for may be cut short with Escape; one on the way through a portal may
//! not.

use std::any::Any;

use auberean_text::StringInfo;
use auberean_ui::framework::FrameId;
use auberean_ui::manager::global;
use auberean_ui::token::state;
use auberean_ui::types::key_id;
use auberean_ui::{Handle, Mail, Ui};

use crate::gameplay::{Notice, Record, Request, PLAY_LAYOUT_ENUM};

mod name {
    use auberean_ui::Token;
    pub const BACKGROUND: Token = Token::of("BackgroundField");
    pub const STORY_FIELD_1: Token = Token::of("StoryField1");
    pub const STORY_FIELD_2: Token = Token::of("StoryField2");
    pub const TEXT_FIELD: Token = Token::of("TextField");
    pub const IMAGE_FIELD: Token = Token::of("ImageField");
    /// The frames' states: which of the two shows.
    pub const FRAME_1: Token = Token::of("Frame1");
    pub const FRAME_2: Token = Token::of("Frame2");
    /// What the background says when it has faded in.
    pub const PLAY: Token = Token::of("play");
}

/// The panels' own notices about the story.
pub mod notice {
    /// The scenes are done: the scenes played (the record).
    pub const FINISH_PLAY_SEQUENCE: u32 = 0x442;
}

/// How long a step lasts that says nothing of its own, in seconds.
const STEP: f64 = 5.0;

/// A step of a scene: a picture, music, a voice, a subtitle (each when it has that track), and
/// how long it lasts (when it says).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Keyframe {
    pub picture: Option<u32>,
    pub music: Option<u32>,
    pub voice: Option<u32>,
    pub subtitle: Option<StringInfo>,
    pub duration: Option<f64>,
}

/// A scene of the story: whether it may be played (only these play) and its steps in order.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scene {
    pub id: u32,
    pub playable: bool,
    pub keyframes: Vec<Keyframe>,
}

/// The scenes the scripts asked to be played, in order, and the list as the scripts handed it
/// (given back to them when the scenes are done).
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Scenes {
    pub scenes: Vec<Scene>,
    pub packed: Vec<u8>,
}

/// What the story asks to be heard.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Sound {
    /// Music played (the last stopped first), or the music stopped.
    Music(Option<u32>),
    /// A voice played (the last stopped first), or the voice stopped.
    Voice(Option<u32>),
}

/// The story playing over the screen.
#[derive(Debug)]
pub struct Story {
    frame: FrameId,
    background: Option<Handle>,
    field_1: Option<Handle>,
    field_2: Option<Handle>,
    /// The steps' records are at hand; the background has faded in; the steps play.
    prefetched: bool,
    faded_in: bool,
    playing: bool,
    /// Escape cuts it short.
    skippable: bool,
    /// When the step showing ends (the interface's clock).
    end_time: f64,
    music: Option<u32>,
    voice: Option<u32>,
    scenes: Scenes,
    frames: Vec<Keyframe>,
    current: usize,
    /// Which frame shows, and the picture last shown.
    last_state: auberean_ui::Token,
    last_image: u32,
    last_subtitle: Option<StringInfo>,
}

impl Story {
    /// The story's background (as large as the screen), its two frames (the second's subtitle
    /// a space, the frames in their first state) and the steps of the playable scenes in their
    /// order; the background's fade and Escape heard; mouse look kept off. The steps' records are
    /// the install's own, so they are at hand at once.
    // D2-02014, D2-02009, D2-02012
    pub fn new(
        ui: &mut Ui,
        parent: FrameId,
        scenes: &Scenes,
        skippable: bool,
        out: &mut Vec<Request>,
    ) -> Self {
        let frame = ui.new_child_frame(parent);
        let background = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::BACKGROUND);
        if let Some(b) = background {
            let (w, h) = ui.display();
            ui.resize_to(b, w, h);
        }
        let field_1 = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::STORY_FIELD_1);
        let field_2 = ui.frame_create_root(frame, PLAY_LAYOUT_ENUM, name::STORY_FIELD_2);
        if let Some(f) = field_2 {
            if let Some(t) = ui.child_recursive(f, name::TEXT_FIELD) {
                ui.set_text(t, " ");
            }
            ui.set_state(f, name::FRAME_1);
        }
        if let Some(l) = ui.frame_listener(frame) {
            ui.listen_for_message(l, name::BACKGROUND, name::PLAY);
            ui.listen_for_global_message(l, global::KEY_DOWN);
        }
        let frames = scenes
            .scenes
            .iter()
            .filter(|s| s.playable)
            .flat_map(|s| s.keyframes.iter().cloned())
            .collect();
        out.push(Request::Notice(Notice {
            native: crate::dialog::notice::UPDATE_CAN_MOUSE_LOOK,
            words: vec![0],
            ..Notice::default()
        }));
        Self {
            frame,
            background,
            field_1,
            field_2,
            prefetched: true,
            faded_in: false,
            playing: false,
            skippable,
            end_time: 0.0,
            music: None,
            voice: None,
            scenes: scenes.clone(),
            frames,
            current: 0,
            last_state: name::FRAME_1,
            last_image: 0,
            last_subtitle: None,
        }
    }

    /// Whether its steps play.
    #[must_use]
    pub const fn playing(&self) -> bool {
        self.playing
    }

    /// The step showing (from 0), of how many.
    #[must_use]
    pub fn step(&self) -> (usize, usize) {
        (self.current, self.frames.len())
    }

    /// The scenes it plays.
    #[must_use]
    pub const fn scenes(&self) -> &Scenes {
        &self.scenes
    }

    /// The play screen hears the scenes are done, with the scenes played.
    fn finished(&self, out: &mut Vec<Request>) {
        out.push(Request::Notice(Notice {
            native: notice::FINISH_PLAY_SEQUENCE,
            records: vec![Some(Record::Scenes(self.scenes.clone()))],
            ..Notice::default()
        }));
    }

    /// No longer playing: the clock no longer heard, the world shown again, the music and the
    /// voice stopped.
    // D2-02002
    pub fn finish(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if !self.playing {
            return;
        }
        self.playing = false;
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.unregister_for_global_message(l, global::TIME);
        }
        out.push(Request::ShowWorld(true));
        out.push(Request::StorySound(Sound::Music(None)));
        out.push(Request::StorySound(Sound::Voice(None)));
        self.music = None;
        self.voice = None;
    }

    /// The next step: past the last, finished and the scenes done. Else its picture in the frame
    /// whose turn it is (a new picture turns the frames), its music and voice, its subtitle (or
    /// the last one), the frames' states turned with a new picture; it lasts its time, else five
    /// seconds.
    // D2-02007
    pub fn play(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        let Some(k) = self.frames.get(self.current).cloned() else {
            self.finish(ui, out);
            self.finished(out);
            return;
        };
        let mut changed = false;
        if let Some(p) = k.picture {
            changed = p != self.last_image;
            self.last_image = p;
            if changed {
                self.last_state = if self.last_state == name::FRAME_1 {
                    name::FRAME_2
                } else {
                    name::FRAME_1
                };
            }
        }
        let field = if self.last_state == name::FRAME_1 {
            self.field_1
        } else {
            self.field_2
        };
        let Some(field) = field else { return };
        let (Some(image), Some(text)) = (
            ui.child_recursive(field, name::IMAGE_FIELD),
            ui.child_recursive(field, name::TEXT_FIELD),
        ) else {
            return;
        };
        if self.last_image != 0 {
            ui.add_image_to_state(image, self.last_image, 0, state::STATELESS);
        }
        if let Some(m) = k.music {
            self.music = Some(m);
            out.push(Request::StorySound(Sound::Music(Some(m))));
        }
        if let Some(v) = k.voice {
            self.voice = Some(v);
            out.push(Request::StorySound(Sound::Voice(Some(v))));
        }
        if let Some(s) = &k.subtitle {
            self.last_subtitle = Some(s.clone());
        }
        // A subtitle that does not resolve leaves the text as it was.
        if let Some(t) = self
            .last_subtitle
            .as_ref()
            .and_then(|s| ui.assets().write(s))
        {
            ui.set_text(text, &t);
        }
        let lasts = k.duration.unwrap_or(STEP);
        if changed {
            for f in [self.field_2, self.field_1].into_iter().flatten() {
                ui.set_state(f, self.last_state);
            }
        }
        self.current += 1;
        self.end_time = ui.now() + lasts;
    }

    /// Once the steps' records are at hand and the background has faded in: the world hidden, the
    /// first frame's subtitle a space, the clock heard, the first step played.
    // D2-02008
    pub fn check_start(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        if !(self.prefetched && self.faded_in) {
            return;
        }
        out.push(Request::ShowWorld(false));
        if let Some(t) = self
            .field_1
            .and_then(|f| ui.child_recursive(f, name::TEXT_FIELD))
        {
            ui.set_text(t, " ");
        }
        self.playing = true;
        if let Some(l) = ui.frame_listener(self.frame) {
            ui.listen_for_global_message(l, global::TIME);
        }
        self.play(ui, out);
    }

    /// What it heard: the background faded in (the first frame in its state, then the start);
    /// the clock (the step over, the next played); Escape (cut short when it may be).
    // D2-02010, D2-02011
    pub fn listen(&mut self, ui: &mut Ui, mail: &Mail, out: &mut Vec<Request>) {
        match *mail {
            Mail::Element { element, what, .. }
                if element == name::BACKGROUND && what == name::PLAY =>
            {
                self.faded_in = true;
                if let Some(f) = self.field_1 {
                    ui.set_state(f, self.last_state);
                }
                self.check_start(ui, out);
            }
            Mail::Global {
                what: global::KEY_DOWN,
                param,
            } => {
                if param == key_id::ESCAPE && self.skippable {
                    self.finish(ui, out);
                    self.finished(out);
                }
            }
            Mail::Global {
                what: global::TIME, ..
            } if self.end_time <= ui.now() => self.play(ui, out),
            _ => {}
        }
    }

    /// A frame of the play screen: what it heard.
    pub fn tick(&mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        for m in crate::camera::take(ui, self.frame) {
            self.listen(ui, &m, out);
        }
    }

    /// It goes: mouse look allowed again, finished, its elements gone.
    // D2-02005, D2-02006
    pub fn close(mut self, ui: &mut Ui, out: &mut Vec<Request>) {
        out.push(Request::Notice(Notice {
            native: crate::dialog::notice::UPDATE_CAN_MOUSE_LOOK,
            words: vec![1],
            ..Notice::default()
        }));
        self.finish(ui, out);
        for r in [self.field_1, self.field_2, self.background]
            .into_iter()
            .flatten()
        {
            ui.frame_remove_root(self.frame, r);
            ui.remove(r);
        }
        ui.drop_frame(self.frame);
    }

    /// Its framework.
    #[must_use]
    pub const fn frame(&self) -> FrameId {
        self.frame
    }

    /// Itself, for whoever holds it.
    #[must_use]
    pub fn as_any(&self) -> &dyn Any {
        self
    }
}
