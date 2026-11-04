//! The event queue: the order received messages take effect in, and which wait.
//!
//! Every message from the game's servers passes here before it touches the model. What decides
//! its fate is its blob id (the ordering type in the top byte, an ordering stamp) and, for a
//! message about an object, the object's instance and visual stamps.
//!
//! **Entering the world.** Messages flagged as out of the world (the login connection's) are never
//! held. Otherwise: before the client asks to enter the world, nothing is held; while it is
//! entering, everything but the player's creation and a character refusal waits for the player's
//! creation; once in the world, everything but object creations waits while the player's object
//! does not exist yet.
//!
//! **Order.** A private message (ordering type 1, the player's own stream) takes effect only after
//! the one stamped before it: the first one accepted is stamped 0 or 1, and a stamp at or before
//! the last one taken is dropped. A stamp arriving again while it is held past a gap is held again,
//! and takes effect twice, the later arrival first. With twenty held and five minutes blocked, a
//! message arriving gives up one missing stamp, and the five minutes start again if the next held
//! one is still not next; they also start again whenever a look at the held messages finds that.
//! A visual message (type 2) is about an object and is ordered by the visual stamp it carries
//! against that object's: the object's creation sets the start, an older stamp is dropped, a newer
//! one waits for the stamps between. Unordered messages (type 3) take effect as they come. Any other type is
//! dropped. A private or visual message this client cannot read still counts for its stamp, so what
//! comes after it is not held for it: a visual message's object and stamps stand at its head,
//! whatever its type.
//!
//! **Objects not here yet.** A message about an object the client does not have, or about a newer
//! incarnation of it (a higher instance stamp), waits in that object's queue; one about an older
//! incarnation is dropped. An object's creation flushes its queue. An object created as a part of
//! another waits for its parent the same way. A queue nothing has been added to for 25 seconds is
//! given up: the client asks the server to send the object again and drops the queue (the player's
//! is kept). A visual queue blocked for more than 20 seconds is given up the same way, but only
//! when it is looked at again: when the next visual message about the object comes, or the object
//! is created again. A queue nothing more comes for stays blocked, and a missing message that comes
//! late still lets everything after it through.

use std::collections::{BTreeMap, HashMap, VecDeque};

use auberean_protocol::physics::CreateObject;
use auberean_protocol::types::ObjectStamp;
use auberean_protocol::ServerMessage;

/// One received message and its blob id.
#[derive(Clone, Debug, PartialEq)]
pub struct Item {
    pub blob_id: u64,
    pub message: ServerMessage,
}

/// The blob id's ordering type: private, visual or unordered (the low five bits of the top byte).
#[must_use]
pub fn ordering_type(blob_id: u64) -> u8 {
    #[allow(clippy::cast_possible_truncation)]
    let t = ((blob_id >> 56) & 0x1F) as u8;
    t
}

/// The blob id's ordering stamp.
#[must_use]
pub fn ordering_stamp(blob_id: u64) -> u16 {
    #[allow(clippy::cast_possible_truncation)]
    let s = (blob_id >> 32) as u16;
    s
}

/// Whether the blob id says the message is out of the world (the login connection's).
#[must_use]
pub fn out_of_world(blob_id: u64) -> bool {
    blob_id & 0x2000_0000_0000_0000 != 0
}

/// Whether the blob id carries the cell flag (a message about a cell's objects to everyone in it).
#[must_use]
pub fn cell_flag(blob_id: u64) -> bool {
    blob_id & 0x4000_0000_0000_0000 != 0
}

/// Message types the entering rule lets through.
const PLAYER_OBJECT: u32 = 0x10060;
const CHARACTER_REFUSED: u32 = 0x10056;
const CREATE_OBJECT: u32 = 0x10058;

/// How long an object's queue may wait for its creation after its last message, seconds.
pub const OBJECT_QUEUE_PATIENCE: f64 = 25.0;
/// How long a visual queue may stay blocked, seconds.
pub const VISUAL_QUEUE_PATIENCE: f64 = 20.0;
/// How long the private stream may stay blocked with twenty messages held, seconds.
pub const PRIVATE_STREAM_PATIENCE: f64 = 300.0;

/// Where the client is in entering the world.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorldState {
    NotInWorld,
    Entering,
    InWorld,
}

/// `a` is newer than `b` on a 16-bit stamp circle: the larger number is newer while they are less
/// than half the circle apart, and the smaller from there on, exactly half included.
#[must_use]
pub fn newer(a: u16, b: u16) -> bool {
    if a.abs_diff(b) < 0x8000 {
        b < a
    } else {
        a < b
    }
}

/// The object and stamps at the head of a visual message's bytes, whatever its type, after its
/// type word. None for one too short to hold them.
// D2-03936
#[must_use]
pub fn visual_head(raw: &[u8]) -> Option<ObjectStamp> {
    let head = raw.get(4..16)?;
    let word = |at: usize| u16::from_le_bytes([head[at], head[at + 1]]);
    Some(ObjectStamp {
        id: u64::from_le_bytes(head[..8].try_into().ok()?),
        instance_stamp: word(8),
        other_stamp: word(10),
    })
}

/// The object a message is about: the sender of an object event, the object of a visual quality
/// or of a call to an object, or the object of a removal.
#[must_use]
pub fn subject(m: &ServerMessage) -> Option<ObjectStamp> {
    use ServerMessage as S;
    Some(match m {
        S::PlayBehavior(e) => e.sender,
        S::StopBehavior(e) | S::DoMode(e) | S::MotionInterpChange(e) | S::EndLayer(e) => e.sender,
        S::StopFx(e) | S::GlobalModifier(e) | S::SetMode(e) | S::DoFxScript(e) => e.sender,
        S::MoveSlider(e) => e.sender,
        S::MoveTo(e) => e.sender,
        S::Parent(e) => e.sender,
        S::HoverHeight(e) | S::VelocityScale(e) | S::AccelerationScale(e) | S::JumpScale(e) => {
            e.sender
        }
        S::LookAt(e) => e.sender,
        S::Push(e) => e.sender,
        S::PushToward(e) => e.sender,
        S::LetGoBehavior(e) => e.sender,
        S::UpdateVisualDesc(e) => e.sender,
        S::DoFx(e) => e.sender,
        S::DoTargetedFx(e) => e.sender,
        S::TerrainChange(e) => e.sender,
        S::ClearTerrainChanges(e) | S::StopAllBehaviors(e) | S::StopMoveCancelsBehaviors(e) => {
            e.sender
        }
        S::DoStory(e) => e.sender,
        S::InitializeMissile(e) => e.sender,
        S::ObjectLeaves(e) => e.sender,
        S::DeParent(e) => e.sender,
        S::TurnHead(e) => e.sender,
        S::Position(p) | S::PositionCell(p) => p.sender,
        S::ForgetObject(o) | S::Contain(o) => o.0,
        S::CellObjectCall(c) | S::VisualObjectCall(c) => c.object,
        S::UpdateIntVisual(q) | S::UpdateBoolVisual(q) => q.object,
        S::UpdateFloatVisual(q) => q.object,
        S::UpdateStringVisual(q) => q.object,
        S::UpdateDataIdVisual(q) => q.object,
        S::UpdateInstanceIdVisual(q) | S::UpdateLongIntVisual(q) => q.object,
        S::UpdatePositionVisual(q) => q.object,
        S::UpdateTimestampVisual(q) => q.object,
        S::UpdateStringInfoVisual(q) => q.object,
        _ => return None,
    })
}

/// The private stream's order keeper: blocking on a gap is the only way it runs, and what it still
/// holds goes with it.
// D2-14302, D2-14304
#[derive(Debug, Default)]
struct PrivateStream {
    started: bool,
    last: u32,
    /// Held messages in stamp order, nothing for one that did not read; a stamp arriving again
    /// goes before the one already held.
    held: VecDeque<(u32, Option<Item>)>,
    blocked_since: Option<f64>,
}

impl PrivateStream {
    /// Take a message, or the stamp of one that did not read; what may now take effect goes to
    /// `out`, in order.
    // D2-14307, D2-14308, D2-05618
    fn add(
        &mut self,
        stamp: u32,
        item: Option<Item>,
        now: f64,
        out: &mut VecDeque<Item>,
        dropped: &mut u32,
    ) {
        if self.started && stamp <= self.last {
            *dropped += 1;
            return;
        }
        self.started = true;
        if stamp == self.last || stamp == self.last + 1 {
            self.last = stamp;
            out.extend(item);
            self.flush(now, out);
            return;
        }
        let was_empty = self.held.is_empty();
        self.hold(stamp, item);
        if was_empty {
            self.blocked_since = Some(now);
        }
        if self.held.len() > 19
            && self
                .blocked_since
                .is_some_and(|t| now - t > PRIVATE_STREAM_PATIENCE)
        {
            // Give up on one missing stamp, as if it had arrived empty.
            self.last += 1;
            self.flush(now, out);
        }
    }

    // D2-14306
    fn hold(&mut self, stamp: u32, item: Option<Item>) {
        let at = self.held.partition_point(|(s, _)| *s < stamp);
        self.held.insert(at, (stamp, item));
    }

    /// The held message that follows the last one taken goes to `out`, if the first held does,
    /// and the answer is whether one did; if not, the wait for it starts again from now.
    // D2-14303
    fn next_ready(&mut self, now: f64, out: &mut VecDeque<Item>) -> bool {
        let Some(&(first, _)) = self.held.front() else {
            return false;
        };
        if first == self.last || first == self.last + 1 {
            self.last = first;
            if let Some((_, item)) = self.held.pop_front() {
                out.extend(item);
            }
            if self.held.is_empty() {
                self.blocked_since = None;
            }
            true
        } else {
            self.blocked_since = Some(now);
            false
        }
    }

    // D2-05630
    fn flush(&mut self, now: f64, out: &mut VecDeque<Item>) {
        while self.next_ready(now, out) {}
    }
}

/// A held visual message and its visual stamp; nothing for one that did not read.
#[derive(Debug)]
struct Stamped {
    stamp: u16,
    item: Option<Item>,
}

/// An object's visual messages, ordered by their visual stamps; made empty at stamp 0 and never
/// blocked, and dropped with whatever it still holds.
// D2-05755, D2-05759
#[derive(Debug, Default)]
struct VisualQueue {
    current: u16,
    /// Held messages as a binary heap, the oldest stamp round the circle on top. The circle has no
    /// first or last stamp, so what comes off when stamps span it, and which of two under one stamp
    /// goes first, depends on the heap's shape: these steps keep it.
    held: Vec<Stamped>,
    blocked_since: Option<f64>,
}

impl VisualQueue {
    // D2-05756, D2-05753
    fn push(&mut self, stamp: u16, item: Option<Item>) {
        self.held.push(Stamped { stamp, item });
        self.sift_up(self.held.len() - 1);
    }

    /// The message at `at` rises past each parent stamped newer than it.
    // D2-05750
    fn sift_up(&mut self, mut at: usize) {
        while at > 0 {
            let parent = (at - 1) / 2;
            if !newer(self.held[parent].stamp, self.held[at].stamp) {
                break;
            }
            self.held.swap(parent, at);
            at = parent;
        }
    }

    /// The top of the heap comes off: the last message takes its place, sinks along the older
    /// child at each step (the right one when they are equal) to the bottom, and rises from there.
    // D2-05754, D2-05751
    fn pop(&mut self) -> Option<Stamped> {
        if self.held.is_empty() {
            return None;
        }
        let top = self.held.swap_remove(0);
        let len = self.held.len();
        let mut at = 0;
        let mut right = 2;
        while right < len {
            let older = if newer(self.held[right].stamp, self.held[right - 1].stamp) {
                right - 1
            } else {
                right
            };
            self.held.swap(at, older);
            at = older;
            right = at * 2 + 2;
        }
        if right == len {
            self.held.swap(at, right - 1);
            at = right - 1;
        }
        self.sift_up(at);
        Some(top)
    }

    /// The next message whose stamp follows the current one; older ones are dropped.
    // D2-05757
    fn next(&mut self, now: f64, dropped: &mut u32) -> Option<Stamped> {
        loop {
            let Some(stamp) = self.held.first().map(|h| h.stamp) else {
                self.blocked_since = None;
                return None;
            };
            let want = self.current.wrapping_add(1);
            if stamp == want {
                self.current = stamp;
                self.blocked_since = None;
                return self.pop();
            }
            if newer(stamp, want) {
                if self.blocked_since.is_none() {
                    self.blocked_since = Some(now);
                }
                return None;
            }
            self.pop();
            *dropped += 1;
        }
    }

    /// The object was (re)created at a visual stamp: nothing at or before it is waited for.
    // D2-05749
    fn created_at(&mut self, stamp: u16) {
        if newer(stamp, self.current) {
            self.current = stamp;
        }
    }

    // D2-05748
    fn heavily_blocked(&self, now: f64) -> bool {
        self.blocked_since
            .is_some_and(|t| now - t > VISUAL_QUEUE_PATIENCE)
    }

    // D2-05758
    fn discard_all(&mut self) {
        self.held.clear();
        self.blocked_since = None;
    }
}

/// The client's book on one object: its incarnation, the order of its visual messages, the cell its
/// physics object stands in, and whether it has a physics object and a game object. A book with no
/// game object is a stand-in: one of the camera's, or an object the client made itself.
#[derive(Debug, Default)]
struct Bookie {
    instance_stamp: u16,
    visual: VisualQueue,
    cell: u32,
    physics: bool,
    game: bool,
}

impl Bookie {
    /// A book on an incarnation of an object: no physics object or game object yet, in no cell,
    /// its visual messages starting from stamp 0.
    // D2-05617
    fn new(instance_stamp: u16) -> Self {
        Self {
            instance_stamp,
            visual: VisualQueue::default(),
            cell: 0,
            physics: false,
            game: false,
        }
    }

    /// The object's physics object is made (or let go of): it is tied to the book by the book's
    /// id, as the game object is.
    // D2-05631, D2-05626
    fn set_physics_obj(&mut self, made: bool) {
        self.physics = made;
    }

    /// The object's game object is made (or let go of), tied to the book by its id.
    // D2-05632, D2-05626
    fn set_game_obj(&mut self, made: bool) {
        self.game = made;
    }

    /// The physics object entered cell `cell`, or left every cell (`None`).
    // D2-05619
    fn change_cell(&mut self, cell: Option<u32>) {
        self.cell = cell.unwrap_or(0);
    }
}

/// Messages waiting for an object's creation.
#[derive(Debug, Default)]
struct Waiting {
    last_push: f64,
    wants_force: bool,
    items: Vec<Item>,
    /// The blob ids and heads of visual messages that did not read.
    unread: Vec<(u64, ObjectStamp)>,
}

/// Counts for logs and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct QueueStats {
    /// Messages held while entering the world.
    pub held_entering: u32,
    /// Messages that waited for an object.
    pub waited: u32,
    /// Visual messages that waited for the stamps before their own.
    pub held_visual: u32,
    /// Messages dropped: older stamps, older incarnations, unknown ordering types.
    pub dropped: u32,
    /// Objects the client asked the server to send again.
    pub force_creates: u32,
    /// Object queues given up.
    pub queues_given_up: u32,
    /// Objects a creation of a newer incarnation replaced.
    pub replaced: u32,
}

/// The event queue, with the client's book on each object it has. Dropping it lets go of what it
/// holds.
// D2-03945, D2-05736
#[derive(Debug)]
pub struct EventQueue {
    state: WorldState,
    held: VecDeque<Item>,
    waiting: BTreeMap<u64, Waiting>,
    bookies: HashMap<u64, Bookie>,
    player: Option<u64>,
    private: PrivateStream,
    /// The stamps of private messages that did not read, waiting as those messages would have.
    unread: Vec<u32>,
    ready: VecDeque<Item>,
    force: Vec<u64>,
    /// Objects the server had the client forget: they go at the next upkeep, so what came with
    /// the forgetting still finds them.
    destroying: Vec<u64>,
    destroyed: Vec<u64>,
    /// Objects to be destroyed at a time unless that is called off, by when.
    to_destroy: HashMap<u64, f64>,
    /// Objects the client made itself with a lifetime, by when it ends.
    temporary_ends: HashMap<u64, f64>,
    stats: QueueStats,
}

/// How long an object asked to be destroyed stays: a quarter second for a temporary object (its
/// id's top byte `0x13`), else 25 seconds.
// D2-16951, D2-16947
#[must_use]
pub fn destruction_delay(id: u64) -> f64 {
    if id >> 56 == 0x13 {
        0.25
    } else {
        25.0
    }
}

impl Default for EventQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl EventQueue {
    /// A queue out of the world, holding nothing and knowing no object or player.
    // D2-03935, D2-00147, D2-05735
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: WorldState::NotInWorld,
            held: VecDeque::new(),
            waiting: BTreeMap::new(),
            bookies: HashMap::new(),
            player: None,
            private: PrivateStream::default(),
            unread: Vec::new(),
            ready: VecDeque::new(),
            force: Vec::new(),
            destroying: Vec::new(),
            destroyed: Vec::new(),
            to_destroy: HashMap::new(),
            temporary_ends: HashMap::new(),
            stats: QueueStats::default(),
        }
    }

    /// Object `id`'s physics object entered cell `cell`, or left every cell (`None`), as the scene
    /// placing the bodies says; an object the client does not have is ignored.
    pub fn change_cell(&mut self, id: u64, cell: Option<u32>) {
        if let Some(b) = self.bookies.get_mut(&id) {
            b.change_cell(cell);
        }
    }

    /// The cell object `id`'s physics object stands in (0 for none); `None` for an object the
    /// client does not have.
    #[must_use]
    pub fn cell_of(&self, id: u64) -> Option<u32> {
        self.bookies.get(&id).map(|b| b.cell)
    }

    /// The objects whose physics objects stand in cell `cell`, in id order.
    #[must_use]
    pub fn objects_in_cell(&self, cell: u32) -> Vec<u64> {
        let mut v: Vec<u64> = self
            .bookies
            .iter()
            .filter(|(_, b)| cell != 0 && b.cell == cell)
            .map(|(id, _)| *id)
            .collect();
        v.sort_unstable();
        v
    }

    /// Whether the client's object `id` has a game object (a stand-in has only a physics object);
    /// `None` for an object the client does not have.
    #[must_use]
    pub fn has_game_object(&self, id: u64) -> Option<bool> {
        self.bookies.get(&id).map(|b| b.game)
    }

    /// Whether the client's object `id` has a physics object; `None` for an object the client does
    /// not have.
    #[must_use]
    pub fn has_physics_object(&self, id: u64) -> Option<bool> {
        self.bookies.get(&id).map(|b| b.physics)
    }

    /// A stand-in the client keeps under id `id` (the camera's place and target): a book with a
    /// physics object and no game object, in the objects. Whether it was made: an id the client
    /// has already makes none.
    // D2-05696
    pub fn create_dummy_object(&mut self, id: u64) -> bool {
        if self.bookies.contains_key(&id) {
            return false;
        }
        let mut b = Bookie::new(0);
        b.set_physics_obj(true);
        self.bookies.insert(id, b);
        true
    }

    /// An object the client made itself (under a temporary id), in the objects: a book with a
    /// physics object and no game object; with a lifetime (seconds, more than 0) it ends that long
    /// after `now`. An id the client has already is made again.
    // D2-05709
    pub fn initialize_temporary_object(&mut self, id: u64, lifetime: f64, now: f64) {
        let mut b = Bookie::new(0);
        b.set_physics_obj(true);
        b.set_game_obj(false);
        self.bookies.insert(id, b);
        if lifetime > 0.0 {
            self.temporary_ends.insert(id, now + lifetime);
        }
    }

    /// When the lifetime of object `id`, one the client made itself, ends.
    #[must_use]
    pub fn temporary_ends(&self, id: u64) -> Option<f64> {
        self.temporary_ends.get(&id).copied()
    }

    /// Object `id` goes from the objects at once (an object the client made itself, removed by
    /// the scene), with whatever waited for it and its times to be destroyed or to end.
    pub fn remove_object(&mut self, id: u64) {
        self.bookies.remove(&id);
        self.waiting.remove(&id);
        self.to_destroy.remove(&id);
        self.temporary_ends.remove(&id);
        self.destroying.retain(|d| *d != id);
    }

    /// An object the client has, to be destroyed after [`destruction_delay`] from `now` (a later
    /// ask moves the time); an object it does not have is ignored.
    // D2-05705, D2-05702
    pub fn add_object_to_be_destroyed(&mut self, id: u64, now: f64) {
        if id != 0 && self.bookies.contains_key(&id) {
            self.to_destroy.insert(id, now + destruction_delay(id));
        }
    }

    /// Call off an object's destruction.
    // D2-05672, D2-05669
    pub fn remove_object_to_be_destroyed(&mut self, id: u64) {
        if id != 0 {
            self.to_destroy.remove(&id);
        }
    }

    /// Whether an object is to be destroyed, and when.
    #[must_use]
    pub fn to_be_destroyed(&self, id: u64) -> Option<f64> {
        self.to_destroy.get(&id).copied()
    }

    #[must_use]
    pub fn stats(&self) -> QueueStats {
        self.stats
    }

    #[must_use]
    pub fn state(&self) -> WorldState {
        self.state
    }

    /// The player's id, once its creation has taken effect.
    #[must_use]
    pub fn player(&self) -> Option<u64> {
        self.player
    }

    /// Whether the client holds an object.
    #[must_use]
    pub fn has_object(&self, id: u64) -> bool {
        self.bookies.contains_key(&id)
    }

    /// Objects with messages waiting for their creation.
    #[must_use]
    pub fn waiting_objects(&self) -> usize {
        self.waiting.len()
    }

    /// The client asked to enter the world.
    pub fn start_entering(&mut self) {
        self.state = WorldState::Entering;
    }

    /// The client left the world: every queue and book goes, the messages held while entering
    /// with them.
    // D2-05730, D2-03944
    pub fn leave_world(&mut self) {
        self.state = WorldState::NotInWorld;
        self.held.clear();
        self.waiting.clear();
        self.bookies.clear();
        self.to_destroy.clear();
        self.temporary_ends.clear();
        self.player = None;
        self.private = PrivateStream::default();
        self.unread.clear();
    }

    /// A message as it came off its queue.
    // D2-03949, D2-03943, D2-03946
    pub fn receive(&mut self, now: f64, item: Item) {
        let t = item.message.message_type();
        if !out_of_world(item.blob_id) {
            match self.state {
                WorldState::NotInWorld => {}
                WorldState::Entering => {
                    if t != PLAYER_OBJECT && t != CHARACTER_REFUSED {
                        // D2-03942
                        self.stats.held_entering += 1;
                        self.held.push_back(item);
                        return;
                    }
                }
                WorldState::InWorld => {
                    if let Some(p) = self.player.filter(|p| !self.bookies.contains_key(p)) {
                        if t != CREATE_OBJECT {
                            self.wait_for(p, item, now);
                            return;
                        }
                    }
                }
            }
        }
        self.process(now, item);
    }

    fn process(&mut self, now: f64, item: Item) {
        match ordering_type(item.blob_id) {
            // D2-03940
            1 => {
                let Some(p) = self.player.filter(|p| self.bookies.contains_key(p)) else {
                    match self.player {
                        Some(p) => self.wait_for(p, item, now),
                        None => self.ready.push_back(item),
                    }
                    return;
                };
                let _ = p;
                let stamp = u32::from(ordering_stamp(item.blob_id));
                self.take_private(now, stamp, Some(item));
            }
            2 => self.visual(now, item),
            3 => self.post(now, item),
            _ => self.stats.dropped += 1,
        }
    }

    /// A private message, or the stamp of one that did not read, comes to the private stream,
    /// after the stamps that were waiting to; what that lets through takes effect.
    // D2-03940
    fn take_private(&mut self, now: f64, stamp: u32, item: Option<Item>) {
        let mut out = VecDeque::new();
        for s in std::mem::take(&mut self.unread) {
            self.private
                .add(s, None, now, &mut out, &mut self.stats.dropped);
        }
        self.private
            .add(stamp, item, now, &mut out, &mut self.stats.dropped);
        for i in out {
            self.post(now, i);
        }
    }

    /// A message this client cannot read came under `blob_id`, `raw` its bytes. If it is private
    /// or visual its stamp still counts, so that the messages after it are not held for it: a
    /// private one waits as a private message would for the player's object, a visual one as a
    /// visual message would for its own, and out of the world a private one counts for nothing.
    // D2-05618, D2-03940, D2-03936
    pub fn receive_unread(&mut self, now: f64, blob_id: u64, raw: &[u8]) {
        match ordering_type(blob_id) {
            1 if self.state != WorldState::NotInWorld => {
                let stamp = u32::from(ordering_stamp(blob_id));
                if self.player.is_some_and(|p| self.bookies.contains_key(&p)) {
                    self.take_private(now, stamp, None);
                } else {
                    self.unread.push(stamp);
                }
            }
            2 => {
                if let Some(s) = visual_head(raw) {
                    self.visual_unread(now, blob_id, s);
                }
            }
            _ => {}
        }
    }

    /// The stamp of a visual message that did not read: as [`Self::visual`] with nothing to take
    /// effect.
    fn visual_unread(&mut self, now: f64, blob_id: u64, s: ObjectStamp) {
        match self.bookies.get_mut(&s.id) {
            Some(b) if b.instance_stamp == s.instance_stamp => {
                if newer(s.other_stamp, b.visual.current.wrapping_add(1)) {
                    self.stats.held_visual += 1;
                }
                b.visual.push(s.other_stamp, None);
                self.flush_visual(s.id, now);
            }
            Some(b) if !newer(s.instance_stamp, b.instance_stamp) => self.stats.dropped += 1,
            _ => {
                let w = self.waiting.entry(s.id).or_default();
                if !cell_flag(blob_id) {
                    w.wants_force = true;
                }
                w.last_push = now;
                w.unread.push((blob_id, s));
                self.stats.waited += 1;
            }
        }
    }

    /// A visual message: wait for its object, then for the stamps before its own.
    // D2-03936, D2-05639
    fn visual(&mut self, now: f64, item: Item) {
        let Some(s) = subject(&item.message).or_else(|| visual_head(&item.message.encode())) else {
            self.post(now, item);
            return;
        };
        if !self.object_or_wait(s, &item, now) {
            return;
        }
        let Some(b) = self.bookies.get_mut(&s.id) else {
            return;
        };
        if newer(s.other_stamp, b.visual.current.wrapping_add(1)) {
            self.stats.held_visual += 1;
        }
        b.visual.push(s.other_stamp, Some(item));
        self.flush_visual(s.id, now);
    }

    /// What the object's visual queue lets through goes on; a queue then blocked for more than
    /// 20 seconds is given up (the object asked for again, what it held discarded). The client
    /// looks at a blocked queue only here, as a visual message or a creation of the object comes.
    // D2-05628
    fn flush_visual(&mut self, id: u64, now: f64) {
        loop {
            let Some(b) = self.bookies.get_mut(&id) else {
                return;
            };
            let Some(next) = b.visual.next(now, &mut self.stats.dropped) else {
                break;
            };
            if let Some(item) = next.item {
                self.post(now, item);
            }
        }
        if let Some(b) = self.bookies.get_mut(&id) {
            if b.visual.heavily_blocked(now) {
                self.force.push(id);
                self.stats.force_creates += 1;
                b.visual.discard_all();
            }
        }
    }

    /// Whether the object a message is about is here, in the incarnation the message is about;
    /// if not, the message waits for it (or is dropped, for an older incarnation).
    // D2-05742
    fn object_or_wait(&mut self, s: ObjectStamp, item: &Item, now: f64) -> bool {
        match self.bookies.get(&s.id) {
            None => {
                self.wait_for(s.id, item.clone(), now);
                false
            }
            Some(b) if b.instance_stamp != s.instance_stamp => {
                if newer(s.instance_stamp, b.instance_stamp) {
                    self.wait_for(s.id, item.clone(), now);
                } else {
                    self.stats.dropped += 1;
                }
                false
            }
            Some(_) => true,
        }
    }

    // D2-05741, D2-05717
    fn wait_for(&mut self, id: u64, item: Item, now: f64) {
        let w = self.waiting.entry(id).or_default();
        if !cell_flag(item.blob_id) {
            w.wants_force = true;
        }
        w.last_push = now;
        w.items.push(item);
        self.stats.waited += 1;
    }

    /// A message takes effect: object creations and removals keep the books first.
    // D2-03938
    fn post(&mut self, now: f64, item: Item) {
        match &item.message {
            ServerMessage::PlayerObject(p) => {
                // D2-03957, D2-03948
                self.player = Some(p.id);
                self.state = WorldState::InWorld;
                self.ready.push_back(item);
                // What waited while entering goes again, in order, under the world's rules: a
                // message other than an object's creation now waits for the player's object.
                // D2-03947
                let held: Vec<Item> = self.held.drain(..).collect();
                for i in held {
                    self.receive(now, i);
                }
            }
            ServerMessage::CreateObject(c) => {
                let c = c.clone();
                self.create(now, item, &c);
            }
            ServerMessage::ForgetObject(o) => {
                // D2-05744, D2-05706, D2-03959
                let s = o.0;
                if Some(s.id) == self.player {
                    return;
                }
                if !self.object_or_wait(s, &item, now) {
                    return;
                }
                if !self.destroying.contains(&s.id) {
                    self.destroying.push(s.id);
                }
                self.ready.push_back(item);
            }
            m => {
                if let Some(s) = subject(m) {
                    if !self.object_or_wait(s, &item, now) {
                        return;
                    }
                }
                self.ready.push_back(item);
            }
        }
    }

    // D2-05743, D2-05629
    fn create(&mut self, now: f64, item: Item, c: &CreateObject) {
        let parent = c.physics.parent_id();
        if parent != 0 {
            let s = ObjectStamp {
                id: parent,
                instance_stamp: c.physics.parent_instance_stamp(),
                other_stamp: 0,
            };
            if !self.object_or_wait(s, &item, now) {
                return;
            }
        }
        let stamp = c.physics.instance_stamp;
        let visual = c.physics.visual_ordering_stamp;
        if let Some(b) = self.bookies.get_mut(&c.id) {
            if b.instance_stamp == stamp {
                b.visual.created_at(visual);
                self.ready.push_back(item);
                self.flush_visual(c.id, now);
                return;
            }
            if !newer(stamp, b.instance_stamp) {
                self.stats.dropped += 1;
                return;
            }
            // A newer incarnation: the one the client has goes first, with its times to be
            // destroyed. What waited for the object (what came about the newer one before its
            // creation) is kept for the newer one (divergence
            // `CD2-newer-incarnation-keeps-what-waited`).
            let waited = self.waiting.remove(&c.id);
            self.remove_object(c.id);
            if let Some(w) = waited {
                self.waiting.insert(c.id, w);
            }
            self.stats.replaced += 1;
        }
        let mut b = Bookie::new(stamp);
        b.set_game_obj(true);
        b.set_physics_obj(true);
        b.visual.created_at(visual);
        self.bookies.insert(c.id, b);
        self.ready.push_back(item);
        self.flush_visual(c.id, now);
        self.flush_waiting(c.id, now);
    }

    /// The object is here: what waited for it goes again, in the order it came. Only the messages
    /// waiting as it was made go (one that must wait again joins the end of the same queue); the
    /// queue goes once it is empty.
    // D2-05727
    fn flush_waiting(&mut self, id: u64, now: f64) {
        let Some(w) = self.waiting.get_mut(&id) else {
            return;
        };
        let held = std::mem::take(&mut w.items);
        let unread = std::mem::take(&mut w.unread);
        for i in held {
            self.receive(now, i);
        }
        for (blob_id, s) in unread {
            self.visual_unread(now, blob_id, s);
        }
        if self
            .waiting
            .get(&id)
            .is_some_and(|w| w.items.is_empty() && w.unread.is_empty())
        {
            self.waiting.remove(&id);
        }
    }

    /// The next message to take effect.
    pub fn pop(&mut self) -> Option<Item> {
        self.ready.pop_front()
    }

    /// The upkeep before a frame's messages: give up on objects' queues that waited too long for
    /// their creations (the objects the client asks the server to send again come out of
    /// [`Self::take_force_creates`], with those a blocked visual queue gave up on), then
    /// let go of the objects forgotten since the last upkeep, and of whatever waited for them
    /// (they come out of [`Self::take_destroyed`]).
    // D2-05739
    pub fn use_time(&mut self, now: f64) {
        self.use_time_creating(now, |_| false);
    }

    /// The upkeep ([`Self::use_time`]), where `being_created` says which objects are being made
    /// from their records: a queue waiting for one of them is never given up.
    // D2-05723
    pub fn use_time_creating(&mut self, now: f64, being_created: impl Fn(u64) -> bool) {
        // Objects whose time to be destroyed came go with the forgotten ones, earliest first.
        let mut due: Vec<(f64, u64)> = self
            .to_destroy
            .iter()
            .filter(|(_, t)| **t <= now)
            .map(|(id, t)| (*t, *id))
            .collect();
        // D2-05654, D2-05657, D2-05663
        due.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        // D2-05662
        for (_, id) in due {
            self.to_destroy.remove(&id);
            if self.bookies.contains_key(&id) && !self.destroying.contains(&id) {
                self.destroying.push(id);
            }
        }
        let expired: Vec<u64> = self
            .waiting
            .iter()
            .filter(|(id, w)| {
                !(w.items.is_empty() && w.unread.is_empty())
                    && now - w.last_push > OBJECT_QUEUE_PATIENCE
                    && !being_created(**id)
            })
            .map(|(id, _)| *id)
            .collect();
        for id in expired {
            if self.waiting.get(&id).is_some_and(|w| w.wants_force) {
                self.force.push(id);
                self.stats.force_creates += 1;
            }
            if Some(id) != self.player {
                self.waiting.remove(&id);
                self.stats.queues_given_up += 1;
            }
        }
        // D2-05726, D2-05725
        for id in std::mem::take(&mut self.destroying) {
            // D2-05627
            self.bookies.remove(&id);
            self.waiting.remove(&id);
            self.to_destroy.remove(&id);
            self.temporary_ends.remove(&id);
            self.destroyed.push(id);
        }
    }

    /// Objects let go of at the last upkeep, for the model to forget.
    pub fn take_destroyed(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.destroyed)
    }

    /// Objects the client asks the server to send again.
    pub fn take_force_creates(&mut self) -> Vec<u64> {
        core::mem::take(&mut self.force)
    }
}
