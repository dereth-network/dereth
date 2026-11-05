//! What an element's state plays, in order, as the clock runs: pictures, animations whose frames
//! follow the time, pauses of a random length, sounds, cursors, jumps back in the list, messages
//! and changes of state; and the fades a top-level element runs as it enters or leaves a state.
//!
//! A state's media items play from the first until one has to wait (a pause not yet over, an
//! animation not yet at its last frame, a change of state); the clock then plays on from that
//! item. What an animation or a pause has reached belongs to the item itself, so elements made
//! from one description share it, as the original's do.

use std::collections::HashMap;

use auberean_assets::ui::{Media, Transition, TransitionChange};

use crate::desc::DescIx;
use crate::manager::Cursor;
use crate::system::{Handle, Picture, Ui};
use crate::token::{attr, state, Token};

/// When a transition runs: as its element enters a state, or as it leaves one.
pub mod when {
    pub const ENTER: u32 = 1;
    pub const LEAVE: u32 = 2;
}

/// A transition's state that stands for every state, and for none (an element just made).
const ALL_STATES: Token = Token::of("all_states");
const NO_STATE: Token = Token::of("no_state");

/// Which media item of which state of which description: the item's own identity.
pub(crate) type ItemKey = (DescIx, usize, usize);

/// What a media item has reached, kept with the item.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum ItemRun {
    /// An animation: when it started (`-1` before it does) and the frame shown (`-1`: none).
    Animation { start: f64, shown: i32 },
    /// A pause: when it ends (`-1` before it starts).
    Pause { end: f64 },
    /// A video: whether it was handed to the host, and whether the host has played it out.
    Video { playing: bool, done: bool },
}

/// A sound a media item asked for, for the host to play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Sound {
    pub element: Handle,
    pub sound: u32,
}

/// A video a media item asked for, for the host to play over the element.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Video {
    pub element: Handle,
    pub file: String,
    pub x: i32,
    pub y: i32,
}

/// A transition under way on an element.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Running {
    /// The transition's own identity: its description, state and place in the state's list.
    pub key: ItemKey,
    pub transition: Transition,
    /// When it started on the clock, and the alpha it started from.
    pub start: f64,
    pub from: f32,
}

/// The transitions an element runs: kept as the original keeps them, an array whose items past
/// its count stay where they were when items before them are taken out.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct RunningList {
    buf: Vec<Running>,
    len: usize,
}

impl RunningList {
    pub(crate) const fn len(&self) -> usize {
        self.len
    }

    fn push(&mut self, r: Running) {
        if self.len < self.buf.len() {
            self.buf[self.len] = r;
        } else {
            self.buf.push(r);
        }
        self.len += 1;
    }

    /// The item in the array's slot `i`, even past the count.
    fn slot(&self, i: usize) -> Option<&Running> {
        self.buf.get(i)
    }

    /// Take out the first item with this identity, those after it moving up one.
    fn remove_ordered(&mut self, key: ItemKey) -> bool {
        let Some(j) = self.buf[..self.len].iter().position(|r| r.key == key) else {
            return false;
        };
        for k in j..self.len - 1 {
            self.buf[k] = self.buf[k + 1].clone();
        }
        self.len -= 1;
        true
    }

    /// Whether the transition in the first slot runs as the element leaves a state.
    pub(crate) fn first_leaves(&self) -> bool {
        self.buf
            .first()
            .is_some_and(|r| r.transition.when == when::LEAVE)
    }

    /// Take out every item with this identity.
    fn remove_all(&mut self, key: ItemKey) {
        while self.remove_ordered(key) {}
    }
}

/// The numbers of the pictures the interface makes itself: never a picture record's.
const MADE: u32 = 0xF000_0000;

/// Each texel of a picture multiplied by a color: blue, green, red and alpha by the color's.
fn multiply(img: &mut crate::source::Image, color: u32) {
    let c = |shift: u32| f32::from(((color >> shift) & 0xFF) as u8) / 255.0;
    let (r, g, b, a) = (c(16), c(8), c(0), c(24));
    for t in img.bgra.as_chunks_mut::<4>().0 {
        for (v, k) in t.iter_mut().zip([b, g, r, a]) {
            *v = dereth_primitives::num::to_i32(f32::from(*v) * k).clamp(0, 255) as u8;
        }
    }
}

/// One picture laid over another of the same place by its alpha (the part they share).
fn lay_over(base: &mut crate::source::Image, top: &crate::source::Image) {
    let (w, h) = (base.width.min(top.width), base.height.min(top.height));
    for y in 0..h {
        for x in 0..w {
            let bi = ((y * base.width + x) * 4) as usize;
            let ti = ((y * top.width + x) * 4) as usize;
            let a = f32::from(top.bgra[ti + 3]) / 255.0;
            for k in 0..3 {
                let v = f32::from(top.bgra[ti + k]) * a + f32::from(base.bgra[bi + k]) * (1.0 - a);
                base.bgra[bi + k] = dereth_primitives::num::to_i32(v).clamp(0, 255) as u8;
            }
            let ba = f32::from(base.bgra[bi + 3]) / 255.0;
            let out = a + ba * (1.0 - a);
            base.bgra[bi + 3] = dereth_primitives::num::to_i32(out * 255.0).clamp(0, 255) as u8;
        }
    }
}

/// What the media keep in the interface: what each item reached, the sounds and videos asked
/// for, and the dice the items roll.
pub(crate) struct MediaState {
    pub runs: HashMap<ItemKey, ItemRun>,
    pub sounds: Vec<Sound>,
    pub videos: Vec<Video>,
    /// Whether the host plays videos (without a player a video item is passed over).
    pub plays_video: bool,
    /// A draw in [0, 1) from the process's random numbers.
    pub dice: Box<dyn FnMut() -> f64 + Send + Sync>,
    /// Pictures the interface made itself (icons laid together from their layers), by the number
    /// each was given, and each one's number by its layers.
    pub made: HashMap<u32, std::sync::Arc<crate::source::Image>>,
    pub made_by_layers: HashMap<Vec<(u32, u32)>, u32>,
}

impl Default for MediaState {
    fn default() -> Self {
        Self {
            runs: HashMap::new(),
            sounds: Vec::new(),
            videos: Vec::new(),
            plays_video: false,
            dice: Box::new(|| 0.5),
            made: HashMap::new(),
            made_by_layers: HashMap::new(),
        }
    }
}

impl std::fmt::Debug for MediaState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("MediaState")
            .field("runs", &self.runs.len())
            .finish_non_exhaustive()
    }
}

impl Ui {
    /// Give the interface the process's random numbers: a draw in [0, 1) each call.
    pub fn set_dice(&mut self, draw: Box<dyn FnMut() -> f64 + Send + Sync>) {
        self.media_state.dice = draw;
    }

    /// Say whether the host plays videos.
    pub fn set_plays_video(&mut self, plays: bool) {
        self.media_state.plays_video = plays;
    }

    /// The sounds asked for since the last call, oldest first.
    pub fn take_sounds(&mut self) -> Vec<Sound> {
        std::mem::take(&mut self.media_state.sounds)
    }

    /// The videos asked for since the last call.
    pub fn take_videos(&mut self) -> Vec<Video> {
        std::mem::take(&mut self.media_state.videos)
    }

    /// The host has played out the video an element asked for.
    pub fn finish_video(&mut self, h: Handle) {
        let Some((d, s)) = self.node(h).and_then(|n| n.media) else {
            return;
        };
        for (key, run) in &mut self.media_state.runs {
            if key.0 == d && key.1 == s {
                if let ItemRun::Video { done, .. } = run {
                    *done = true;
                }
            }
        }
    }

    /// A number between two bounds from the dice, at double precision (the low bound itself when
    /// the bounds are equal), as the client rolls one.
    fn roll(&mut self, a: f32, b: f32) -> f64 {
        if a == b {
            return f64::from(a);
        }
        let (lo, hi) = if a < b { (a, b) } else { (b, a) };
        let r = (self.media_state.dice)();
        (f64::from(hi) - f64::from(lo)) * r + f64::from(lo)
    }

    // -----------------------------------------------------------------------------------------
    // The media list
    // -----------------------------------------------------------------------------------------

    /// Which state's media list plays: the template's current state's, else its base state's,
    /// else (for a reference) the reference's current state's, else its base state's. Every item
    /// of that list starts over, and the list plays from its first item.
    // D2-11347
    pub(crate) fn reset_media(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let (desc, refdesc, s) = (n.desc, n.refdesc, n.state);
        let has = |d: DescIx, st: Token| -> Option<(DescIx, usize)> {
            let e = self.library().get(d);
            let i = e.states.iter().position(|x| x.id == st)?;
            (!e.states[i].media.is_empty()).then_some((d, i))
        };
        let mut pick = None;
        if s != state::STATELESS {
            pick = has(desc, s);
        }
        if pick.is_none() {
            pick = has(desc, state::STATELESS);
        }
        if pick.is_none() && refdesc != desc {
            if s != state::STATELESS {
                pick = has(refdesc, s);
            }
            if pick.is_none() {
                pick = has(refdesc, state::STATELESS);
            }
        }
        if let Some((d, i)) = pick {
            let n = self.library().get(d).states[i].media.len();
            for k in 0..n {
                self.reset_item((d, i, k));
            }
        }
        if let Some(n) = self.node_mut(h) {
            n.media = pick;
            n.media_index = 0;
        }
        self.update_media(h);
    }

    /// An item starts over: an animation from its first frame, a pause rolls its length again, a
    /// video is let go.
    // D2-11111, D2-11140, D2-11118
    fn reset_item(&mut self, key: ItemKey) {
        if let Some(run) = self.media_state.runs.get_mut(&key) {
            *run = match run {
                ItemRun::Animation { .. } => ItemRun::Animation {
                    start: -1.0,
                    shown: -1,
                },
                ItemRun::Pause { .. } => ItemRun::Pause { end: -1.0 },
                ItemRun::Video { .. } => ItemRun::Video {
                    playing: false,
                    done: false,
                },
            };
        }
    }

    /// Play media items from the current one until one has to wait.
    // D2-11325
    pub(crate) fn update_media(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        let Some((d, s)) = n.media else { return };
        let count = self.library().get(d).states[s].media.len();
        loop {
            let Some(at) = self.node(h).map(|n| n.media_index) else {
                return;
            };
            let Ok(i) = usize::try_from(at) else { return };
            if i >= count {
                return;
            }
            if !self.update_item(h, (d, s, i)) {
                return;
            }
            if let Some(n) = self.node_mut(h) {
                n.media_index += 1;
            }
        }
    }

    /// Set the media item the list plays from next (it moves on one first).
    // D2-11298
    pub fn set_media_index(&mut self, h: Handle, index: i32) {
        if let Some(n) = self.node_mut(h) {
            n.media_index = index;
        }
    }

    /// One media item on an element; returns whether the list moves on.
    fn update_item(&mut self, h: Handle, key: ItemKey) -> bool {
        let item = self.library().get(key.0).states[key.1].media[key.2].clone();
        match item {
            // D2-11129
            Media::Image {
                picture, draw_mode, ..
            } => {
                self.set_image(h, picture.0, draw_mode);
                true
            }
            Media::Animation {
                duration,
                draw_mode,
                frames,
                ..
            } => self.update_animation(h, key, duration, draw_mode, &frames),
            Media::Pause { min, max } => self.update_pause(key, min, max),
            // D2-11108
            Media::Alpha { picture, .. } => {
                self.set_alpha_image(h, picture.0);
                true
            }
            // D2-11145
            Media::Sound { sound } => {
                self.media_state.sounds.push(Sound {
                    element: h,
                    sound: sound.0,
                });
                true
            }
            // D2-11133
            Media::Jump { index, probability } => {
                if self.roll(0.0, 1.0) <= f64::from(probability) {
                    let to = i32::try_from(index).unwrap_or(i32::MAX).saturating_sub(1);
                    self.set_media_index(h, to);
                }
                true
            }
            // D2-11137
            Media::Message {
                message,
                probability,
            } => {
                if self.roll(0.0, 1.0) <= f64::from(probability) {
                    self.broadcast(h, Token(message), 0);
                }
                true
            }
            // The state changes once the change under way is done; the list waits.
            // D2-11149
            Media::State { state } => {
                self.queue_state(h, Token(state));
                false
            }
            Media::Video { x, y, file } => self.update_video(h, key, x, y, &file),
            // D2-11126
            Media::Cursor { picture, x, y } => {
                self.set_element_cursor(
                    h,
                    Cursor {
                        picture: picture.0,
                        hot_x: x,
                        hot_y: y,
                    },
                );
                true
            }
        }
    }

    /// An animation: started the first time it plays; the frame shown is the time since then over
    /// one frame's share of its duration, cut down, never past the last; a new frame is shown; the
    /// list waits until the last frame shows, then the animation starts over and the list moves
    /// on.
    // D2-11113
    fn update_animation(
        &mut self,
        h: Handle,
        key: ItemKey,
        duration: f32,
        draw_mode: u32,
        frames: &[u32],
    ) -> bool {
        let now = self.now();
        let run = self
            .media_state
            .runs
            .entry(key)
            .or_insert(ItemRun::Animation {
                start: -1.0,
                shown: -1,
            });
        let ItemRun::Animation { start, shown } = run else {
            return true;
        };
        if *start == -1.0 {
            *start = now;
            *shown = -1;
        }
        let count = i32::try_from(frames.len()).unwrap_or(i32::MAX);
        if count == 0 {
            self.reset_item(key);
            return true;
        }
        let per_frame = f64::from(duration) / f64::from(count);
        let reached = dereth_primitives::num::to_i32_f64((now - *start) / per_frame);
        let frame = (count - 1).min(reached);
        let changed = frame != *shown;
        if changed {
            // The frame number is kept in 16 bits.
            #[allow(clippy::cast_possible_truncation)]
            {
                *shown = i32::from(frame as i16);
            }
        }
        let shown_now = *shown;
        if changed {
            let pic = usize::try_from(frame)
                .ok()
                .and_then(|i| frames.get(i))
                .copied();
            if let Some(p) = pic {
                self.set_image(h, p, draw_mode);
            }
        }
        if shown_now + 1 < count {
            return false;
        }
        self.reset_item(key);
        true
    }

    /// A pause: its end rolled between its least and greatest length the first time it plays; the
    /// list waits until then, and the pause starts over.
    // D2-11141
    fn update_pause(&mut self, key: ItemKey, min: f32, max: f32) -> bool {
        let now = self.now();
        let end = match self.media_state.runs.get(&key) {
            Some(ItemRun::Pause { end }) if *end != -1.0 => *end,
            _ => {
                let end = self.roll(min, max) + now;
                self.media_state.runs.insert(key, ItemRun::Pause { end });
                end
            }
        };
        if now < end {
            return false;
        }
        self.reset_item(key);
        true
    }

    /// A video: handed to the host the first time it plays (passed over when the host plays no
    /// video); the list waits until the host has played it out, then it starts over.
    // D2-11119
    fn update_video(&mut self, h: Handle, key: ItemKey, x: i32, y: i32, file: &str) -> bool {
        let run = self.media_state.runs.get(&key).copied();
        match run {
            Some(ItemRun::Video {
                playing: true,
                done,
            }) => {
                if !done {
                    return false;
                }
                self.reset_item(key);
                true
            }
            _ => {
                if !self.media_state.plays_video {
                    return true;
                }
                self.media_state.runs.insert(
                    key,
                    ItemRun::Video {
                        playing: true,
                        done: false,
                    },
                );
                self.media_state.videos.push(Video {
                    element: h,
                    file: file.to_owned(),
                    x,
                    y,
                });
                false
            }
        }
    }

    // -----------------------------------------------------------------------------------------
    // Pictures
    // -----------------------------------------------------------------------------------------

    /// Show a picture on the element by a draw mode (0 copies its texels, 1 and 2 blend it over
    /// what is beneath, 3 copies it faded by the element's transparency). The element's
    /// transparency is taken from its `alpha_value` the first time it shows a picture (0 when it
    /// has none).
    // D2-11348, D2-11172
    pub fn set_image(&mut self, h: Handle, picture: u32, mode: u32) {
        let Some(n) = self.node(h) else { return };
        let alpha = n.float_attr(attr::ALPHA_VALUE).unwrap_or(0.0);
        let Some(n) = self.node_mut(h) else { return };
        // D2-11929
        n.blends = mode == 1 || mode == 2;
        if n.alpha == -1.0 && alpha != n.alpha {
            n.alpha = alpha;
        }
        n.picture = Some(Picture { id: picture, mode });
    }

    /// Take the element's picture away (the element shows nothing of its own).
    // D2-11173
    pub fn clear_image(&mut self, h: Handle) {
        if let Some(n) = self.node_mut(h) {
            n.picture = None;
        }
    }

    /// Use a picture as the element's transparency (in place of any it had).
    // D2-11300, D2-11153, D2-11154
    pub fn set_alpha_image(&mut self, h: Handle, picture: u32) {
        if let Some(n) = self.node_mut(h) {
            n.alpha_picture = Some(picture);
        }
    }

    /// The size of the picture the element shows, if any.
    // D2-11311
    #[must_use]
    pub fn image_size(&self, h: Handle) -> Option<(u32, u32)> {
        let p = self.node(h)?.picture?;
        let img = self.picture(p.id)?;
        Some((img.width, img.height))
    }

    /// A picture by its number: one the interface made itself, else the data's.
    #[must_use]
    pub fn picture(&self, id: u32) -> Option<std::sync::Arc<crate::source::Image>> {
        if id & MADE == MADE {
            return self.media_state.made.get(&id).cloned();
        }
        self.assets().image(id)
    }

    /// Show an icon made of layers, bottom first, each a picture tinted by its color (red, green,
    /// blue and alpha in a word, alpha the top byte): the first layer's picture multiplied by its
    /// color, each next one's multiplied by its own and laid over it by its alpha. Nothing changes
    /// when the first layer's picture cannot be read.
    // D2-11333, D2-09625, D2-09615, D2-09961
    pub fn set_icon_desc(&mut self, h: Handle, layers: &[(u32, u32)]) {
        let Some(&(first, color)) = layers.first() else {
            return;
        };
        let id = if let Some(id) = self.media_state.made_by_layers.get(layers) {
            *id
        } else {
            let Some(base) = self.assets().image(first) else {
                return;
            };
            let mut base = (*base).clone();
            multiply(&mut base, color);
            for &(picture, color) in &layers[1..] {
                if let Some(top) = self.assets().image(picture) {
                    let mut top = (*top).clone();
                    multiply(&mut top, color);
                    lay_over(&mut base, &top);
                }
            }
            let id = MADE | u32::try_from(self.media_state.made.len() + 1).unwrap_or(0) & !MADE;
            self.media_state.made.insert(id, std::sync::Arc::new(base));
            self.media_state.made_by_layers.insert(layers.to_vec(), id);
            id
        };
        self.clear_image(h);
        self.set_image(h, id, 0);
    }

    /// Show another element's picture, and do the same for each child that has a namesake among
    /// the other element's children.
    // D2-11308
    pub fn match_graphics(&mut self, h: Handle, other: Handle) -> bool {
        let Some(o) = self.node(other) else {
            return false;
        };
        let theirs = o.picture;
        let their_kids = o.children.clone();
        if let Some(p) = theirs {
            if self.node(h).and_then(|n| n.picture) != Some(p) {
                if let Some(n) = self.node_mut(h) {
                    n.picture = Some(p);
                }
            }
        }
        let kids = self.node(h).map(|n| n.children.clone()).unwrap_or_default();
        for k in kids {
            if their_kids.is_empty() {
                break;
            }
            let Some(kid) = self.node(k).map(|n| self.library().get(n.refdesc).id) else {
                continue;
            };
            let twin = their_kids.iter().copied().find(|c| {
                self.node(*c)
                    .is_some_and(|cn| self.library().get(cn.refdesc).id == kid)
            });
            let Some(twin) = twin else {
                return true;
            };
            self.match_graphics(k, twin);
        }
        true
    }

    /// Add a picture to one of the element's states (the element takes a description of its own
    /// the first time); shown at once when the element is in that state or the state is its base
    /// state.
    // D2-11365
    pub fn add_image_to_state(&mut self, h: Handle, picture: u32, mode: u32, s: Token) -> bool {
        let Some(n) = self.node(h) else { return false };
        let (desc, refdesc, own, current) = (n.desc, n.refdesc, n.own_desc, n.state);
        let desc = if desc == refdesc && !own {
            let copy = self.library_mut().copy(desc);
            if let Some(n) = self.node_mut(h) {
                n.desc = copy;
                n.own_desc = true;
            }
            copy
        } else {
            desc
        };
        let image = Media::Image {
            picture: auberean_primitives::DataId(picture),
            draw_mode: mode,
            x: 0,
            y: 0,
        };
        if !self.library_mut().add_media(desc, s, image) {
            return false;
        }
        if s != state::STATELESS && s != current {
            return true;
        }
        self.set_image(h, picture, mode);
        true
    }

    // -----------------------------------------------------------------------------------------
    // Transitions
    // -----------------------------------------------------------------------------------------

    /// Start a transition on a top-level element, toward `target` (the state it leaves for): a
    /// fade from its start alpha (or from the element's alpha now, when the transition does not
    /// say), a turn from where the element stands. Other kinds, and elements in other elements,
    /// do not run them.
    // D2-11361
    fn start_transition(&mut self, h: Handle, key: ItemKey, t: &Transition, target: Token) -> bool {
        let Some(n) = self.node(h) else { return false };
        if !n.owns_object {
            return false;
        }
        let from = match &t.change {
            TransitionChange::Fade {
                start_alpha, flags, ..
            } => {
                if flags & 1 == 0 {
                    // D2-10416
                    n.object_alpha
                } else {
                    *start_alpha
                }
            }
            TransitionChange::Rotate { .. } => 0.0,
            _ => return false,
        };
        let now = self.now();
        if let Some(n) = self.node_mut(h) {
            n.pending_state = target;
            n.transitions.push(Running {
                key,
                transition: t.clone(),
                start: now,
                from,
            });
        }
        self.update_transitions(h);
        true
    }

    /// Run an element's transitions on: a fade sets the element's alpha between its start and end
    /// by the share of its duration gone (at most all of it). The transitions done (and any that
    /// cannot run) are counted, and as many are stopped, taken from the front of the element's
    /// list.
    // D2-11334
    pub(crate) fn update_transitions(&mut self, h: Handle) {
        let Some(n) = self.node(h) else { return };
        if n.transitions.len() == 0 {
            return;
        }
        let now = self.now();
        let owns = n.owns_object;
        let mut done = 0usize;
        for i in 0..n.transitions.len() {
            let Some(r) = self.node(h).and_then(|n| n.transitions.slot(i)).cloned() else {
                continue;
            };
            let finished = if owns {
                match r.transition.change {
                    TransitionChange::Fade {
                        end_alpha,
                        duration,
                        ..
                    } => {
                        let share = share(now, r.start, duration);
                        if let Some(n) = self.node_mut(h) {
                            n.object_alpha = (end_alpha - r.from) * share + r.from;
                        }
                        share >= 1.0
                    }
                    TransitionChange::Rotate { duration, .. } => {
                        share(now, r.start, duration) >= 1.0
                    }
                    _ => true,
                }
            } else {
                true
            };
            if finished {
                done += 1;
            }
        }
        for i in 0..done {
            let Some(r) = self.node(h).and_then(|n| n.transitions.slot(i)).cloned() else {
                continue;
            };
            self.stop_transition(h, &r, true);
        }
    }

    /// Stop one transition. With none left running, a transition that ran as the element left a
    /// state, stopped because it was done, lets the element go on to the state it was leaving
    /// for.
    // D2-11331
    fn stop_transition(&mut self, h: Handle, r: &Running, finished: bool) {
        let Some(n) = self.node_mut(h) else { return };
        n.transitions.remove_ordered(r.key);
        if n.transitions.len() != 0 {
            return;
        }
        if r.transition.when == when::LEAVE && finished {
            n.left = true;
            let target = n.pending_state;
            self.set_state(h, target);
        }
    }

    /// Stop every transition, each at once.
    // D2-11332
    pub(crate) fn stop_all_transitions(&mut self, h: Handle) {
        loop {
            let Some(n) = self.node_mut(h) else { return };
            if n.transitions.len() == 0 {
                return;
            }
            let Some(first) = n.transitions.slot(0).map(|r| r.key) else {
                return;
            };
            n.transitions.remove_all(first);
        }
    }

    /// The transitions of the element's template for a state (its current state, else its base
    /// state) that run at `which` time and match `from` (or every state; or, entering an element
    /// just made, no state), started toward `target`.
    pub(crate) fn start_transitions(
        &mut self,
        h: Handle,
        lookup: Token,
        which: u32,
        from: Token,
        target: Token,
    ) {
        let Some(desc) = self.node(h).map(|n| n.desc) else {
            return;
        };
        let e = self.library().get(desc);
        let s = e
            .states
            .iter()
            .position(|x| x.id == lookup)
            .or_else(|| e.states.iter().position(|x| x.id == state::STATELESS));
        let Some(s) = s else { return };
        let list: Vec<(usize, Transition)> = e.states[s]
            .transitions
            .iter()
            .cloned()
            .enumerate()
            .collect();
        for (i, t) in list {
            let matches = Token(t.state) == from
                || Token(t.state) == ALL_STATES
                || (which == when::ENTER && from == Token(0) && Token(t.state) == NO_STATE);
            if t.when == which && matches {
                self.start_transition(h, (desc, s, i), &t, target);
            }
        }
    }
}

impl crate::system::Node {
    /// Whether the first transition it runs is one run as it leaves a state.
    pub(crate) fn transitions_first_leaves(&self) -> bool {
        self.transitions.first_leaves()
    }
}

/// The share of a duration gone since `start`, at most 1.
fn share(now: f64, start: f64, duration: f64) -> f32 {
    #[allow(clippy::cast_possible_truncation)]
    let s = ((now - start) / duration) as f32;
    if s > 1.0 {
        1.0
    } else {
        s
    }
}
