//! The performance: pieces playing on paths, in music time, turned into notes for each path's
//! synthesizer.
//!
//! Music time runs at the performance's tempo, 768 ticks a quarter note. A piece plays from a
//! time (at once, or on the next grid, beat, measure or marker, or at the end of the piece that
//! leads); each of its tracks plays as its time passes: bands set the channels' instruments,
//! volumes and pans; sequences play their notes and curves; segment triggers start other pieces on
//! the same path; style tracks play their style's patterns (the pattern for the groove level, a
//! variation of each part chosen as the pattern starts, each note made a key over the chord); a
//! script track runs its routine. A piece ends at its length, or repeats its loop as many times as
//! it says. A piece a script plays leads the performance: its tempo, time signature, chord and
//! groove level are everyone's.
//!
//! Each path has its own synthesizer and channels; its frames are rendered ahead as they are read.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Arc;

use super::chord::{self, music_to_midi, ChordKey};
use super::effects::{self, Compressor};
use super::form::{self, Segment, Style, TimeSig, TrackKind, PPQ};
use super::library::{Object, Piece};
use super::script;
use super::synth::{Event, Patch, Synth};

/// The tempo when nothing has set one, in beats a minute.
pub const DEFAULT_TEMPO: f64 = 120.0;

/// Frames rendered between the performance's turns (where tempo and controls change).
pub const BLOCK: usize = 64;

/// Frames a path keeps rendered and unread before the oldest are dropped.
pub const MAX_BUFFERED: usize = 2 * super::synth::RATE as usize;

/// A path.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct PathId(pub u32);

/// A playing piece.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct StateId(pub u32);

/// When a piece starts (or stops).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boundary {
    Immediate,
    Grid,
    Beat,
    Measure,
    Marker,
    /// At the end of the leading piece.
    LeadEnd,
    /// After the leading piece and any queued to follow it.
    Queue,
}

/// The rules the performance follows where the music does not say.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rules {
    /// A tempo track of a piece that does not lead still sets the tempo.
    pub secondary_tempo: bool,
    /// The groove level when no leading piece gives one.
    pub default_groove: u8,
    /// The chord when no leading piece gives one.
    pub default_chord: ChordKey,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            secondary_tempo: false,
            default_groove: 62,
            default_chord: chord::DEFAULT_CHORD,
        }
    }
}

/// How a piece is played.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlayOptions {
    pub boundary: Boundary,
    /// Repeats after the first play (`u32::MAX` for ever); `None` keeps the piece's own.
    pub repeats: Option<u32>,
    /// It leads (as a piece a script plays does).
    pub lead: bool,
}

impl Default for PlayOptions {
    fn default() -> Self {
        Self {
            boundary: Boundary::Immediate,
            repeats: None,
            lead: false,
        }
    }
}

#[derive(Debug)]
struct Path {
    id: PathId,
    synth: Synth,
    buf: VecDeque<f32>,
    width: usize,
    compressor: Option<Compressor>,
}

/// A style track as it plays: the pattern playing, from when, and each part's variation.
#[derive(Clone, Debug)]
struct StyleRun {
    style: Arc<Style>,
    /// The segment time the style track starts at.
    from: f64,
    pattern: Option<usize>,
    pattern_start: f64,
    pattern_len: f64,
    variations: Vec<u32>,
    last: Vec<Option<u32>>,
}

#[derive(Debug)]
struct State {
    id: StateId,
    path: PathId,
    piece: Arc<Piece>,
    seg: Arc<Segment>,
    lead: bool,
    /// The performance time it starts at.
    start: f64,
    started: bool,
    /// The segment time reached.
    pos: f64,
    repeats_left: u32,
    /// No more events: it has ended or was stopped.
    finished: bool,
    stop_at: Option<f64>,
    styles: Vec<StyleRun>,
    /// Notes sounding: when each ends, its channel and key.
    offs: Vec<(f64, u32, u8)>,
    /// The leading piece's controls as last set.
    tempo: Option<f64>,
    timesig: Option<TimeSig>,
    /// The chord's stack: each sub-chord with the levels it serves.
    chord: Option<Vec<(u32, ChordKey)>>,
    groove: Option<u8>,
    markers: Vec<i32>,
}

/// A note played: at a music time, on a path's channel, its key and velocity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Played {
    pub time: f64,
    pub path: PathId,
    pub channel: u32,
    pub key: u8,
    pub velocity: u8,
}

/// What happens at a time in a block.
#[derive(Clone, Debug)]
enum Act {
    Synth(PathId, Event),
    Tempo(f64),
    Start {
        piece: Arc<Piece>,
        seg: Arc<Segment>,
        path: PathId,
        options: PlayOptions,
    },
}

/// The performance.
#[derive(Debug)]
pub struct Performance {
    rate: u32,
    rules: Rules,
    /// Music time now.
    mt: f64,
    tempo: f64,
    master_groove: i32,
    paths: Vec<Path>,
    states: Vec<State>,
    next_path: u32,
    next_state: u32,
    lead: Option<StateId>,
    queued: VecDeque<StateId>,
    rng: u32,
    /// Notes played, when kept.
    log: Option<Vec<Played>>,
}

impl Performance {
    /// A performance rendering at `rate` with `rules`; `seed` seeds its variation choices.
    #[must_use]
    pub fn new(rate: u32, rules: Rules, seed: u32) -> Self {
        Self {
            rate: rate.max(1),
            rules,
            mt: 0.0,
            tempo: DEFAULT_TEMPO,
            master_groove: 0,
            paths: Vec::new(),
            states: Vec::new(),
            next_path: 1,
            next_state: 1,
            lead: None,
            queued: VecDeque::new(),
            rng: seed | 1,
            log: None,
        }
    }

    /// Keep a log of the notes played (for tests and tools).
    pub fn keep_log(&mut self) {
        self.log = Some(Vec::new());
    }

    /// The notes played so far.
    #[must_use]
    pub fn log(&self) -> &[Played] {
        self.log.as_deref().unwrap_or(&[])
    }

    /// The rate it renders at.
    #[must_use]
    pub fn rate(&self) -> u32 {
        self.rate
    }

    /// Music time now.
    #[must_use]
    pub fn music_time(&self) -> f64 {
        self.mt
    }

    /// The tempo now.
    #[must_use]
    pub fn tempo(&self) -> f64 {
        self.tempo
    }

    /// Set the master groove level, added to the leading piece's groove level.
    pub fn set_master_groove(&mut self, level: i32) {
        self.master_groove = level;
    }

    /// The master groove level.
    #[must_use]
    pub fn master_groove(&self) -> i32 {
        self.master_groove
    }

    /// A new path: stereo, or mono (pan ignored) for a placed sound; with no effects.
    pub fn add_path(&mut self, mono: bool) -> PathId {
        self.add(mono, None)
    }

    /// A new path for the game's music: as [`Performance::add_path`], with the compressor of its
    /// kind (placed or heard everywhere).
    pub fn add_music_path(&mut self, mono: bool) -> PathId {
        let settings = if mono {
            effects::PLACED_MUSIC
        } else {
            effects::FLAT_MUSIC
        };
        self.add(mono, Some(settings))
    }

    fn add(&mut self, mono: bool, compressor: Option<effects::CompressorSettings>) -> PathId {
        let id = PathId(self.next_path);
        self.next_path += 1;
        let width = if mono { 1 } else { 2 };
        self.paths.push(Path {
            id,
            synth: Synth::new(self.rate, mono),
            buf: VecDeque::new(),
            width,
            compressor: compressor.map(|c| Compressor::new(c, width, self.rate)),
        });
        id
    }

    /// Remove a path and every piece on it.
    pub fn remove_path(&mut self, path: PathId) {
        self.paths.retain(|p| p.id != path);
        self.states.retain(|s| s.path != path);
        if self
            .lead
            .is_some_and(|l| !self.states.iter().any(|s| s.id == l))
        {
            self.lead = None;
        }
        let alive: Vec<StateId> = self.states.iter().map(|s| s.id).collect();
        self.queued.retain(|q| alive.contains(q));
    }

    /// How many voices a path's synthesizer is sounding.
    #[must_use]
    pub fn voices(&self, path: PathId) -> usize {
        self.paths
            .iter()
            .find(|p| p.id == path)
            .map_or(0, |p| p.synth.active())
    }

    /// Play a piece on a path.
    pub fn play(&mut self, piece: Arc<Piece>, path: PathId, options: PlayOptions) -> StateId {
        let seg = piece.segment.clone();
        self.start(piece, seg, path, options, self.mt)
    }

    fn start(
        &mut self,
        piece: Arc<Piece>,
        seg: Arc<Segment>,
        path: PathId,
        options: PlayOptions,
        now: f64,
    ) -> StateId {
        let id = StateId(self.next_state);
        self.next_state += 1;
        let start =
            if options.lead && matches!(options.boundary, Boundary::Queue | Boundary::LeadEnd) {
                // Queued: it starts when what leads and is queued has ended.
                match self.queue_end() {
                    Some(t) => t.max(now),
                    None => now,
                }
            } else {
                self.resolve(options.boundary, now)
            };
        let repeats = options.repeats.unwrap_or(seg.repeats);
        self.states.push(State {
            id,
            path,
            piece,
            pos: f64::from(seg.play_start),
            seg,
            lead: options.lead,
            start,
            started: false,
            repeats_left: repeats,
            finished: false,
            stop_at: None,
            styles: Vec::new(),
            offs: Vec::new(),
            tempo: None,
            timesig: None,
            chord: None,
            groove: None,
            markers: Vec::new(),
        });
        if options.lead {
            if self.lead.is_none() && start <= now {
                self.lead = Some(id);
            } else {
                self.queued.push_back(id);
            }
        }
        id
    }

    /// When what leads, and everything queued after it, ends (`None`: nothing leads).
    fn queue_end(&self) -> Option<f64> {
        let mut end = None;
        for id in self.lead.iter().chain(self.queued.iter()) {
            if let Some(s) = self.states.iter().find(|s| s.id == *id) {
                let e = s.start.max(end.unwrap_or(f64::MIN)) + Self::remaining(s);
                end = Some(e);
            }
        }
        end
    }

    /// Ticks a piece has left to play (from its start, if it has not started).
    fn remaining(s: &State) -> f64 {
        if s.repeats_left == u32::MAX {
            return f64::INFINITY;
        }
        let seg = &s.seg;
        let loop_end = if seg.loop_end > seg.loop_start {
            seg.loop_end
        } else {
            seg.length
        };
        let loop_len = f64::from(loop_end - seg.loop_start);
        f64::from(seg.length) - s.pos + loop_len * f64::from(s.repeats_left)
    }

    /// The time signature boundaries count in, and where they count from.
    fn grid(&self) -> (TimeSig, f64) {
        match self
            .lead
            .and_then(|l| self.states.iter().find(|s| s.id == l))
        {
            Some(s) => (s.timesig.unwrap_or_default(), s.start),
            None => (TimeSig::default(), 0.0),
        }
    }

    /// The first time at or after `now` on a boundary.
    fn resolve(&self, b: Boundary, now: f64) -> f64 {
        let (ts, origin) = self.grid();
        let next = |unit: i32| -> f64 {
            let u = f64::from(unit.max(1));
            origin + ((now - origin) / u).ceil() * u
        };
        match b {
            Boundary::Immediate => now,
            Boundary::Grid => next(ts.grid_ticks()),
            Boundary::Beat => next(ts.beat_ticks()),
            Boundary::Measure => next(ts.measure_ticks()),
            Boundary::Marker => {
                let lead = self
                    .lead
                    .and_then(|l| self.states.iter().find(|s| s.id == l));
                lead.and_then(|s| {
                    s.markers
                        .iter()
                        .map(|m| s.start + f64::from(*m))
                        .filter(|t| *t >= now)
                        .reduce(f64::min)
                })
                .unwrap_or(now)
            }
            Boundary::LeadEnd | Boundary::Queue => self
                .lead
                .and_then(|l| self.states.iter().find(|s| s.id == l))
                .map_or(now, |s| (s.start + Self::remaining(s)).max(now)),
        }
    }

    /// Stop a piece (and what it started) at a boundary.
    pub fn stop(&mut self, id: StateId, boundary: Boundary) {
        let at = self.resolve(boundary, self.mt);
        if let Some(s) = self.states.iter_mut().find(|s| s.id == id) {
            s.stop_at = Some(at);
        }
    }

    /// Whether a piece has ended (or was stopped, or is unknown).
    #[must_use]
    pub fn is_done(&self, id: StateId) -> bool {
        self.states
            .iter()
            .find(|s| s.id == id)
            .is_none_or(|s| s.finished)
    }

    /// The groove level the patterns follow now: the leading piece's command plus the master
    /// groove level, from 1 to 100; with no command, the default level, which the master level
    /// does not move.
    #[must_use]
    pub fn groove(&self) -> u8 {
        let command = self
            .lead
            .and_then(|l| self.states.iter().find(|s| s.id == l))
            .and_then(|s| s.groove);
        match command {
            Some(c) => {
                u8::try_from((i32::from(c) + self.master_groove).clamp(1, 100)).unwrap_or(100)
            }
            None => self.rules.default_groove,
        }
    }

    /// The chord now for a part's sub-chord level: the leading piece's sub-chord that serves the
    /// level (else its first), or the default chord.
    fn chord_now(&self, level: u8) -> ChordKey {
        let stack = self
            .lead
            .and_then(|l| self.states.iter().find(|s| s.id == l))
            .and_then(|s| s.chord.as_ref());
        let Some(stack) = stack else {
            return self.rules.default_chord;
        };
        stack
            .iter()
            .find(|(levels, _)| level < 32 && levels & (1 << level) != 0)
            .or(stack.first())
            .map_or(self.rules.default_chord, |(_, c)| *c)
    }

    /// A random offset within half a range either way (none for no range).
    fn jitter(&mut self, range: u8) -> f64 {
        if range == 0 {
            return 0.0;
        }
        let r = i32::from(range);
        let n = u32::try_from(r + 1).unwrap_or(1);
        let pick = i32::try_from(self.random() % n).unwrap_or(0);
        f64::from(pick - r / 2)
    }

    fn random(&mut self) -> u32 {
        // xorshift32
        let mut x = self.rng;
        x ^= x << 13;
        x ^= x >> 17;
        x ^= x << 5;
        self.rng = x;
        x
    }

    /// Read a path's next frames (interleaved for a stereo path), rendering as needed; how many
    /// frames were read.
    pub fn read(&mut self, path: PathId, out: &mut [f32]) -> usize {
        let Some(width) = self.paths.iter().find(|p| p.id == path).map(|p| p.width) else {
            return 0;
        };
        let want = out.len() / width * width;
        loop {
            let have = self
                .paths
                .iter()
                .find(|p| p.id == path)
                .map_or(0, |p| p.buf.len());
            if have >= want {
                break;
            }
            self.render(BLOCK);
        }
        let Some(p) = self.paths.iter_mut().find(|p| p.id == path) else {
            return 0;
        };
        for (o, x) in out[..want].iter_mut().zip(p.buf.drain(..want)) {
            *o = x;
        }
        want / width
    }

    /// Render `frames` frames on every path.
    pub fn render(&mut self, frames: usize) {
        let mut done = 0;
        while done < frames {
            let n = BLOCK.min(frames - done);
            self.block(n);
            done += n;
        }
    }

    fn block(&mut self, n: usize) {
        let rate = f64::from(self.rate);
        let ticks_per_frame = self.tempo / 60.0 * f64::from(PPQ) / rate;
        let mt0 = self.mt;
        #[allow(clippy::cast_precision_loss)]
        // LINT-OK: a block's frame count, a few dozen.
        let mt1 = mt0 + n as f64 * ticks_per_frame;
        // Collect every piece's events in the block; pieces started in it add theirs.
        let mut acts: Vec<(f64, u8, Act)> = Vec::new();
        let mut seen = 0;
        loop {
            let count = self.states.len();
            for i in seen..count {
                self.collect(i, mt0, mt1, &mut acts);
            }
            seen = count;
            // Starts happen now, so the new pieces' events in this block are collected too.
            let mut started = false;
            let mut k = 0;
            while k < acts.len() {
                if matches!(acts[k].2, Act::Start { .. }) {
                    let (t, _, a) = acts.swap_remove(k);
                    if let Act::Start {
                        piece,
                        seg,
                        path,
                        options,
                    } = a
                    {
                        self.start(piece, seg, path, options, t);
                        started = true;
                    }
                } else {
                    k += 1;
                }
            }
            if !started {
                break;
            }
        }
        // Notes ending in the block.
        for s in &mut self.states {
            let path = s.path;
            s.offs.retain(|&(t, ch, key)| {
                if t < mt1 {
                    acts.push((t, 0, Act::Synth(path, Event::NoteOff { channel: ch, key })));
                    false
                } else {
                    true
                }
            });
        }
        acts.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        // The synthesizers' events at their frames.
        let mut per_path: BTreeMap<PathId, Vec<(usize, Event)>> = BTreeMap::new();
        for (t, _, a) in acts {
            match a {
                Act::Synth(path, e) => {
                    let f = ((t - mt0) / ticks_per_frame).floor().max(0.0);
                    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
                    // LINT-OK: a frame within the block, from 0 to its length.
                    let f = (f as usize).min(n - 1);
                    if let (
                        Some(log),
                        Event::NoteOn {
                            channel,
                            key,
                            velocity,
                        },
                    ) = (&mut self.log, &e)
                    {
                        log.push(Played {
                            time: t,
                            path,
                            channel: *channel,
                            key: *key,
                            velocity: *velocity,
                        });
                    }
                    per_path.entry(path).or_default().push((f, e));
                }
                Act::Tempo(t) => self.tempo = t,
                Act::Start { .. } => {}
            }
        }
        for p in &mut self.paths {
            let events = per_path.remove(&p.id).unwrap_or_default();
            let mut block = vec![0.0f32; n * p.width];
            let mut at = 0;
            for (f, e) in events {
                if f > at {
                    p.synth.render(&mut block[at * p.width..f * p.width]);
                    at = f;
                }
                p.synth.apply(&e);
            }
            if at < n {
                p.synth.render(&mut block[at * p.width..]);
            }
            if let Some(c) = p.compressor.as_mut() {
                c.run(&mut block);
            }
            p.buf.extend(block);
            let max = MAX_BUFFERED * p.width;
            if p.buf.len() > max {
                let extra = p.buf.len() - max;
                p.buf.drain(..extra);
            }
        }
        self.mt = mt1;
        // Pieces that ended go once their notes have; the queue moves on.
        if let Some(l) = self.lead {
            if self.is_done(l) {
                self.lead = None;
            }
        }
        if self.lead.is_none() {
            if let Some(next) = self.queued.pop_front() {
                self.lead = Some(next);
            }
        }
        self.states.retain(|s| !(s.finished && s.offs.is_empty()));
    }

    /// Collect state `i`'s events in the block `[mt0, mt1)`.
    fn collect(&mut self, i: usize, mt0: f64, mt1: f64, acts: &mut Vec<(f64, u8, Act)>) {
        if self.states[i].finished {
            return;
        }
        let s = &self.states[i];
        if s.start >= mt1 {
            return;
        }
        // Where in performance time the block starts for this piece.
        let mut t = mt0.max(s.start);
        let mut end = mt1;
        if let Some(stop) = s.stop_at {
            if stop < mt1 {
                end = stop.max(t);
            }
        }
        if !self.states[i].started {
            self.states[i].started = true;
        }
        while t < end {
            let s = &self.states[i];
            let seg = s.seg.clone();
            let loop_end = if seg.loop_end > seg.loop_start {
                seg.loop_end
            } else {
                seg.length
            };
            let looping = s.repeats_left > 0;
            let seg_end = if looping { loop_end } else { seg.length };
            let span = (end - t).min(f64::from(seg_end) - s.pos).max(0.0);
            let a = s.pos;
            let b = a + span;
            let at = t;
            self.span(i, a, b, at, acts);
            self.states[i].pos = b;
            t += span;
            if b >= f64::from(seg_end) {
                let s = &mut self.states[i];
                if looping {
                    if s.repeats_left != u32::MAX {
                        s.repeats_left -= 1;
                    }
                    s.pos = f64::from(seg.loop_start);
                    // Styles start their patterns again with the loop.
                    s.styles.clear();
                    if loop_end <= seg.loop_start {
                        s.finished = true;
                        break;
                    }
                } else {
                    s.finished = true;
                    break;
                }
            } else if span <= 0.0 {
                break;
            }
        }
        if let Some(stop) = self.states[i].stop_at {
            if stop < mt1 && !self.states[i].finished {
                let s = &mut self.states[i];
                s.finished = true;
                // Its notes end at the stop.
                for o in &mut s.offs {
                    o.0 = o.0.min(stop);
                }
            }
        }
    }

    /// State `i`'s tracks over segment time `[a, b)`, which begins at performance time `at`.
    fn span(&mut self, i: usize, a: f64, b: f64, at: f64, acts: &mut Vec<(f64, u8, Act)>) {
        let st = &self.states[i];
        let (seg, piece, path, lead) = (st.seg.clone(), st.piece.clone(), st.path, st.lead);
        let to_mt = |s: f64| at + (s - a);
        let within = |time: i32| {
            let t = f64::from(time);
            t >= a && t < b
        };
        for track in &seg.tracks {
            match &track.kind {
                TrackKind::Tempo(items) => {
                    if lead || self.rules.secondary_tempo {
                        for (time, bpm) in items {
                            if within(*time) {
                                acts.push((to_mt(f64::from(*time).max(a)), 0, Act::Tempo(*bpm)));
                                if lead {
                                    self.states[i].tempo = Some(*bpm);
                                }
                            }
                        }
                    }
                }
                TrackKind::TimeSig(items) => {
                    for (time, ts) in items {
                        if within(*time) {
                            self.states[i].timesig = Some(*ts);
                        }
                    }
                }
                TrackKind::Band(items) => {
                    for item in items {
                        // A band is sent by its logical time, ahead of the notes there.
                        let time = item.time.max(seg.play_start);
                        if within(time) {
                            for e in band_events(&item.band, &piece) {
                                acts.push((to_mt(f64::from(time)), 1, Act::Synth(path, e)));
                            }
                        }
                    }
                }
                TrackKind::Sequence { events, curves } => {
                    for c in curves {
                        let time = c.start + i32::from(c.offset);
                        if within(time) {
                            for e in curve_events(c.kind, c.controller, c.to, c.pchannel) {
                                acts.push((to_mt(f64::from(time)), 2, Act::Synth(path, e)));
                            }
                        }
                    }
                    for e in events {
                        let time = e.time + i32::from(e.offset);
                        if !within(time) {
                            continue;
                        }
                        let t = to_mt(f64::from(time));
                        if e.status & 0xF0 == 0x90 && e.data2 > 0 {
                            acts.push((
                                t,
                                3,
                                Act::Synth(
                                    path,
                                    Event::NoteOn {
                                        channel: e.pchannel,
                                        key: e.data1,
                                        velocity: e.data2,
                                    },
                                ),
                            ));
                            self.states[i].offs.push((
                                t + f64::from(e.duration.max(1)),
                                e.pchannel,
                                e.data1,
                            ));
                        }
                    }
                }
                TrackKind::Trigger(items) => {
                    for item in items {
                        if within(item.time) {
                            if let Some(Object::Segment(child)) = piece.get(&item.segment) {
                                acts.push((
                                    to_mt(f64::from(item.time)),
                                    0,
                                    Act::Start {
                                        piece: piece.clone(),
                                        seg: child.clone(),
                                        path,
                                        options: PlayOptions {
                                            boundary: boundary_of(item.play_flags),
                                            repeats: None,
                                            lead: false,
                                        },
                                    },
                                ));
                            }
                        }
                    }
                }
                TrackKind::Command(items) => {
                    for c in items {
                        if within(c.time) && c.command == 0 {
                            self.states[i].groove = Some(c.groove);
                        }
                    }
                }
                TrackKind::Chord { chords, .. } => {
                    for c in chords {
                        if within(c.time) && !c.subchords.is_empty() {
                            let stack = c
                                .subchords
                                .iter()
                                .map(|sc| {
                                    let key = ChordKey {
                                        chord_pattern: sc.chord_pattern,
                                        scale_pattern: sc.scale_pattern,
                                        chord_root: sc.chord_root,
                                        scale_root: sc.scale_root,
                                    };
                                    (sc.levels, key)
                                })
                                .collect();
                            self.states[i].chord = Some(stack);
                        }
                    }
                }
                TrackKind::Marker(marks) => {
                    if self.states[i].markers.is_empty() {
                        self.states[i].markers.clone_from(marks);
                    }
                }
                TrackKind::Script(calls) => {
                    for c in calls {
                        if within(c.time) {
                            // A script reads the master groove level itself.
                            let groove = self.master_groove;
                            if let Some(Object::Script(sc)) = piece.get(&c.script) {
                                if let Ok(r) = script::parse(&sc.source) {
                                    for call in r.run(&c.routine, groove).unwrap_or_default() {
                                        let Some(seg) = sc
                                            .objects
                                            .iter()
                                            .find(|o| o.alias.eq_ignore_ascii_case(&call.name))
                                            .and_then(|o| o.segment.clone())
                                        else {
                                            continue;
                                        };
                                        let queue = call
                                            .flags
                                            .iter()
                                            .any(|f| f.eq_ignore_ascii_case("AtFinish"));
                                        acts.push((
                                            to_mt(f64::from(c.time)),
                                            0,
                                            Act::Start {
                                                piece: piece.clone(),
                                                seg: Arc::new(seg),
                                                path,
                                                options: PlayOptions {
                                                    boundary: if queue {
                                                        Boundary::Queue
                                                    } else {
                                                        Boundary::Immediate
                                                    },
                                                    repeats: None,
                                                    lead: true,
                                                },
                                            },
                                        ));
                                    }
                                }
                            }
                        }
                    }
                }
                TrackKind::Style(items) => {
                    for (time, r) in items {
                        let from = f64::from(*time);
                        if from < b && self.states[i].styles.iter().all(|x| x.from != from) {
                            if let Some(Object::Style(style)) = piece.get(r) {
                                // The style's own band first, as the style track sends it.
                                if let Some(band) = style.bands.first() {
                                    for e in band_events(band, &piece) {
                                        acts.push((to_mt(from.max(a)), 1, Act::Synth(path, e)));
                                    }
                                }
                                self.states[i].styles.push(StyleRun {
                                    style: style.clone(),
                                    from,
                                    pattern: None,
                                    pattern_start: from,
                                    pattern_len: 0.0,
                                    variations: Vec::new(),
                                    last: Vec::new(),
                                });
                            }
                        }
                    }
                }
                TrackKind::Other(_) => {}
            }
        }
        self.style_notes(i, a, b, at, acts);
    }

    /// The notes of state `i`'s styles over `[a, b)`.
    fn style_notes(&mut self, i: usize, a: f64, b: f64, at: f64, acts: &mut Vec<(f64, u8, Act)>) {
        let path = self.states[i].path;
        let to_mt = |s: f64| at + (s - a);
        for k in 0..self.states[i].styles.len() {
            let mut from = a;
            while from < b {
                // A new pattern where the last one ends.
                let need = {
                    let r = &self.states[i].styles[k];
                    r.pattern.is_none() || from >= r.pattern_start + r.pattern_len
                };
                if need {
                    let groove = self.groove();
                    let r = &self.states[i].styles[k];
                    let start = if r.pattern.is_none() {
                        r.from
                    } else {
                        r.pattern_start + r.pattern_len
                    };
                    let style = r.style.clone();
                    let Some(p) = self.choose_pattern(&style, groove) else {
                        break;
                    };
                    let pattern = &style.patterns[p];
                    let ts = pattern.timesig;
                    let len = f64::from(ts.measure_ticks()) * f64::from(pattern.measures.max(1));
                    let mut variations = Vec::with_capacity(pattern.parts.len());
                    let last = self.states[i].styles[k].last.clone();
                    let mut new_last = Vec::with_capacity(pattern.parts.len());
                    for (j, pr) in pattern.parts.iter().enumerate() {
                        let prev = last.get(j).copied().flatten();
                        let v = style.part(&pr.guid).map_or(0, |part| {
                            self.choose_variation(part, pr.variation_mode, prev)
                        });
                        variations.push(v);
                        new_last.push(Some(v));
                    }
                    let r = &mut self.states[i].styles[k];
                    r.pattern = Some(p);
                    r.pattern_start = start;
                    r.pattern_len = len;
                    r.variations = variations;
                    r.last = new_last;
                }
                let r = self.states[i].styles[k].clone();
                let Some(p) = r.pattern else { break };
                let pattern = &r.style.patterns[p];
                let p_end = r.pattern_start + r.pattern_len;
                let to = b.min(p_end);
                for (j, pr) in pattern.parts.iter().enumerate() {
                    let Some(part) = r.style.part(&pr.guid) else {
                        continue;
                    };
                    let chord = self.chord_now(pr.subchord_level);
                    let var = r.variations.get(j).copied().unwrap_or(0);
                    let bit = 1u32 << (var & 31);
                    let pts = part.timesig;
                    let part_len = f64::from(pts.measure_ticks()) * f64::from(part.measures.max(1));
                    // The part repeats through the pattern.
                    let mut rep = r.pattern_start;
                    while rep < to {
                        for note in &part.notes {
                            if note.variations & bit == 0 {
                                continue;
                            }
                            let g = note.grid;
                            let grids = i32::from(pts.grids.max(1));
                            let beat = pts.beat_ticks();
                            let time = g.div_euclid(grids) * beat
                                + g.rem_euclid(grids) * beat / grids
                                + i32::from(note.offset);
                            let ts = rep + f64::from(time);
                            if f64::from(time) >= part_len || ts < from || ts >= to || ts >= p_end {
                                continue;
                            }
                            // A note with ranges moves within them, at random each time.
                            let ts = ts + self.jitter(note.time_range);
                            let duration =
                                f64::from(note.duration.max(1)) + self.jitter(note.duration_range);
                            let mode = if note.play_mode == chord::mode::NONE {
                                part.play_mode
                            } else {
                                note.play_mode
                            };
                            let Some(key) = music_to_midi(note.music_value, mode, &chord) else {
                                continue;
                            };
                            let t = to_mt(ts);
                            acts.push((
                                t,
                                3,
                                Act::Synth(
                                    path,
                                    Event::NoteOn {
                                        channel: pr.pchannel,
                                        key,
                                        velocity: note.velocity.clamp(1, 127),
                                    },
                                ),
                            ));
                            self.states[i]
                                .offs
                                .push((t + duration.max(1.0), pr.pchannel, key));
                        }
                        for c in &part.curves {
                            if c.variations & bit == 0 {
                                continue;
                            }
                            let g = c.grid;
                            let grids = i32::from(pts.grids.max(1));
                            let beat = pts.beat_ticks();
                            let time = g.div_euclid(grids) * beat
                                + g.rem_euclid(grids) * beat / grids
                                + i32::from(c.offset);
                            let ts = rep + f64::from(time);
                            if ts < from || ts >= to {
                                continue;
                            }
                            for e in curve_events(c.kind, c.controller, c.to, pr.pchannel) {
                                acts.push((to_mt(ts), 2, Act::Synth(path, e)));
                            }
                        }
                        if part_len <= 0.0 {
                            break;
                        }
                        rep += part_len;
                    }
                }
                from = to;
            }
        }
    }

    /// The pattern for a groove level: of the plain patterns whose range holds it, one at random;
    /// with none, the one whose range is nearest.
    fn choose_pattern(&mut self, style: &Style, groove: u8) -> Option<usize> {
        let fits: Vec<usize> = style
            .patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| p.embellishment == 0 && p.groove.0 <= groove && groove <= p.groove.1)
            .map(|(k, _)| k)
            .collect();
        if !fits.is_empty() {
            let n = u32::try_from(fits.len()).unwrap_or(1);
            let pick = self.random() % n;
            return fits.get(pick as usize).copied();
        }
        style
            .patterns
            .iter()
            .enumerate()
            .filter(|(_, p)| p.embellishment == 0)
            .min_by_key(|(_, p)| {
                if groove < p.groove.0 {
                    p.groove.0 - groove
                } else {
                    groove.saturating_sub(p.groove.1)
                }
            })
            .map(|(k, _)| k)
    }

    /// A part's variation: of the variations that may play over any chord, one at random (in
    /// the no-repeat mode, not the last one when another may play).
    fn choose_variation(&mut self, part: &form::Part, mode: u8, last: Option<u32>) -> u32 {
        let mut ok: Vec<u32> = (0u32..32)
            .filter(|&v| part.variation_choices[v as usize] & 0x001F_FFFF != 0)
            .collect();
        if ok.is_empty() {
            return 0;
        }
        if mode == 3 && ok.len() > 1 {
            if let Some(l) = last {
                ok.retain(|&v| v != l);
            }
        }
        let n = u32::try_from(ok.len()).unwrap_or(1);
        ok[(self.random() % n) as usize]
    }
}

/// The boundary a piece is started on, from its play flags.
#[must_use]
pub fn boundary_of(flags: u32) -> Boundary {
    if flags & (1 << 23) != 0 {
        Boundary::LeadEnd
    } else if flags & (1 << 24) != 0 {
        Boundary::Marker
    } else if flags & 0x2000 != 0 {
        Boundary::Measure
    } else if flags & 0x1000 != 0 {
        Boundary::Beat
    } else if flags & 0x800 != 0 {
        Boundary::Grid
    } else if flags & 0x100 != 0 {
        Boundary::Queue
    } else {
        Boundary::Immediate
    }
}

/// A band's events: each channel's instrument from its collection, its volume and pan, and the
/// pitch wheel's range.
fn band_events(band: &form::Band, piece: &Piece) -> Vec<Event> {
    let mut out = Vec::new();
    for ins in &band.instruments {
        let channel = ins.pchannel;
        if ins.flags & 0x1 != 0 {
            let patch = ins.collection.as_ref().and_then(|r| match piece.get(r) {
                Some(Object::Collection(c)) => {
                    let index = c
                        .instruments
                        .iter()
                        .position(|i| i.patch() == ins.patch & 0x807F_7F7F)?;
                    Some(Patch {
                        collection: c.clone(),
                        index,
                    })
                }
                _ => None,
            });
            out.push(Event::Program { channel, patch });
        }
        if ins.flags & 0x40 != 0 {
            out.push(Event::Control {
                channel,
                controller: 7,
                value: ins.volume.min(127),
            });
        }
        if ins.flags & 0x20 != 0 {
            out.push(Event::Control {
                channel,
                controller: 10,
                value: ins.pan.min(127),
            });
        }
        if ins.flags & 0x2000 != 0 {
            out.push(Event::BendRange {
                channel,
                semitones: u8::try_from(ins.bend_range.clamp(0, 127)).unwrap_or(2),
            });
        }
    }
    out
}

/// A curve's value as events: a controller, or the pitch wheel (0 to 16,383, 8,192 at rest);
/// pressure and parameter curves are not sent.
fn curve_events(kind: u8, controller: u8, value: i16, channel: u32) -> Vec<Event> {
    match kind {
        // The pitch wheel.
        3 => {
            vec![Event::PitchBend {
                channel,
                value: u16::try_from(value.clamp(0, 16383)).unwrap_or(8192),
            }]
        }
        // A controller.
        4 => vec![Event::Control {
            channel,
            controller,
            value: u8::try_from(value.clamp(0, 127)).unwrap_or(0),
        }],
        _ => Vec::new(),
    }
}
