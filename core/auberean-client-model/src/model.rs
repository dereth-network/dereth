//! The model: what the client knows of the game, as the messages that took effect built it.
//!
//! - **The character list**: the account's characters with their looks, the world's name.
//! - **The player**: its id (from the player's creation), its object, its qualities (its
//!   description, then each private quality update).
//! - **Objects** by instance id: how each looks, its physics and weenie descriptions, where it is,
//!   what it hangs from, what holds or wields it, its qualities as the server updated them; whether
//!   it is in the world or was taken out of it.
//! - **Inventory**: the objects held, directly or through containers, by the player.
//!
//! Script calls are not run here: they come out as [`Effect::Call`] for the script host, in order
//! with everything else.

use std::collections::{BTreeMap, BTreeSet, HashMap};

use auberean_protocol::login::{
    CharacterCreated, CharacterList, CharacterRefused, DataRecord, DataRefused, DownloadBegins,
};
use auberean_protocol::physics::{CreateObject, ObjectPosition};
use auberean_protocol::script::ScriptArgs;
use auberean_protocol::table::Table;
use auberean_protocol::text::{NarrowString, StringInfo};
use auberean_protocol::types::{
    PhysicsDesc, Placement, Qualities as Description, VisualDesc, WeenieDesc,
};
use auberean_protocol::ServerMessage;

use crate::events::Item;

/// One kind of quality value, by quality id.
pub type Values<V> = BTreeMap<u32, V>;

/// An object's qualities as the server set them; what is not here comes from the record named by
/// `base` and the records it inherits from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Qualities {
    /// The qualities record the object's qualities derive from (0 for none).
    pub base: u32,
    pub ints: Values<i32>,
    pub bools: Values<i32>,
    pub floats: Values<f32>,
    pub timestamps: Values<f64>,
    pub strings: Values<NarrowString>,
    pub data_ids: Values<u32>,
    pub instance_ids: Values<u64>,
    pub positions: Values<Placement>,
    pub string_infos: Values<StringInfo>,
    pub package_ids: Values<u32>,
    pub long_ints: Values<u64>,
}

fn take<V: Clone>(t: &Table<V>) -> Values<V> {
    t.entries.iter().cloned().collect()
}

impl Qualities {
    /// The qualities a description sends.
    #[must_use]
    pub fn from_description(d: &Description) -> Self {
        Self {
            base: d.did,
            ints: take(&d.ints),
            bools: take(&d.bools),
            floats: take(&d.floats),
            timestamps: take(&d.timestamps),
            strings: take(&d.strings),
            data_ids: take(&d.data_ids),
            instance_ids: take(&d.instance_ids),
            positions: take(&d.positions),
            string_infos: take(&d.string_infos),
            package_ids: take(&d.package_ids),
            long_ints: take(&d.long_ints),
        }
    }

    /// How many values are set.
    #[must_use]
    pub fn len(&self) -> usize {
        self.ints.len()
            + self.bools.len()
            + self.floats.len()
            + self.timestamps.len()
            + self.strings.len()
            + self.data_ids.len()
            + self.instance_ids.len()
            + self.positions.len()
            + self.string_infos.len()
            + self.package_ids.len()
            + self.long_ints.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Where an object is, as its last position said.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Place {
    pub cell: u32,
    pub origin: auberean_primitives::Vec3,
    /// The heading byte and motion bytes as sent.
    pub heading_motion: u32,
    pub position_stamp: u16,
    pub teleport_stamp: u16,
    pub force_position_stamp: u16,
    /// The server's clock when it was sent, seconds (0 for a place from a creation).
    pub time: f64,
    /// The position's flags: touching the ground, a jump, an impulse.
    pub flags: u32,
    /// The jump's or impulse's velocity.
    pub velocity: auberean_primitives::Vec3,
}

/// How many positions an object keeps for the world to take; older ones go first.
pub const KEPT_REPORTS: usize = 16;

impl Place {
    fn of(p: &ObjectPosition) -> Self {
        Self {
            cell: p.cell,
            origin: p.origin,
            heading_motion: p.heading_motion,
            position_stamp: p.position_stamp,
            teleport_stamp: p.teleport_stamp,
            force_position_stamp: p.force_position_stamp,
            time: p.time,
            flags: p.flags,
            velocity: p.velocity,
        }
    }

    /// The heading sent, degrees clockwise from north: the top byte as 255ths of a turn.
    // D2-13355
    #[must_use]
    pub fn heading(&self) -> f32 {
        let b = self.heading_motion >> 24;
        // In the client's wider precision: the byte times 1/255 (single) times 360, then the
        // remainder of a turn.
        let d = f64::from(b) * f64::from(1.0f32 / 255.0) * 360.0;
        #[allow(clippy::cast_possible_truncation)]
        let mut h = (d % 360.0) as f32;
        if h < 0.0 {
            h += 360.0;
            if h >= 360.0 {
                h = 0.0;
            }
        }
        h
    }

    /// The motion asked for (to the side, ahead and up, each -1 to 1): each of the low three
    /// bytes as 255ths of the way from -1 to 1.
    // D2-13124
    #[must_use]
    pub fn motion(&self) -> auberean_primitives::Vec3 {
        let part = |shift: u32| {
            #[allow(clippy::cast_precision_loss)]
            let f = ((self.heading_motion >> shift) & 0xFF) as f32 * (1.0 / 255.0);
            (f + f) - 1.0
        };
        auberean_primitives::Vec3::new(part(16), part(8), part(0))
    }

    /// Whether the position touches the ground.
    #[must_use]
    pub fn contact(&self) -> bool {
        self.flags & auberean_protocol::physics::position_flags::CONTACT != 0
    }

    /// Whether the position carries a jump.
    #[must_use]
    pub fn jump(&self) -> bool {
        self.flags & auberean_protocol::physics::position_flags::JUMP != 0
    }

    /// Whether the position carries an impulse.
    #[must_use]
    pub fn impulse(&self) -> bool {
        self.flags & auberean_protocol::physics::position_flags::IMPULSE != 0
    }
}

/// One object.
#[derive(Clone, Debug, PartialEq)]
pub struct Object {
    pub id: u64,
    pub instance_stamp: u16,
    pub visual: VisualDesc,
    /// How many descriptions the server has sent it since its creation, changed or not: each is
    /// applied, so its effects play again.
    pub visual_updates: u32,
    pub physics: PhysicsDesc,
    pub weenie: WeenieDesc,
    /// Its place in the world, if it has one.
    pub place: Option<Place>,
    /// What it hangs from, and where.
    pub parent: Option<(u64, u32)>,
    /// The pose of its setup it hangs in, as the server last said it was taken up.
    pub parent_placement: u32,
    /// What holds it, and what wields it.
    pub container: Option<u64>,
    pub wielder: Option<u64>,
    /// In the world, as opposed to taken out of it (picked up, contained, left).
    pub in_world: bool,
    /// The teleport stamp of its last position.
    pub teleport_stamp: Option<u16>,
    /// The newest position stamp heard: what it was told of where it is, leaving the world and
    /// being held by something included, counts only when stamped later than this.
    pub position_stamp: u16,
    pub qualities: Qualities,
    /// The positions heard since the world last took them, oldest first (at most
    /// [`KEPT_REPORTS`]).
    pub reports: Vec<Place>,
    /// How many creations of this incarnation took effect: the first makes it, each later one
    /// updates it (its descriptions again, and its place when it has one).
    pub creations: u32,
}

impl Object {
    /// Whether a position stamp is later than the newest heard; a later one is kept.
    // D2-06190
    fn newer_position(&mut self, stamp: u16) -> bool {
        let later = crate::events::newer(stamp, self.position_stamp);
        if later {
            self.position_stamp = stamp;
        }
        later
    }

    /// The engine object a creation makes: its descriptions taken, its own qualities empty (the
    /// record they derive from is named by its descriptions), nothing listening to it yet.
    // D2-07980, D2-00742, D2-00410, D2-08040, D2-08290, D2-08120, D2-08023, D2-08034, D2-08103
    fn from_create(c: &CreateObject) -> Self {
        let mut o = Self {
            id: c.id,
            instance_stamp: c.physics.instance_stamp,
            visual: c.visual.clone(),
            visual_updates: 0,
            // The creation's physics description, copied whole.
            // D2-05721
            physics: c.physics.clone(),
            weenie: c.weenie.copy(),
            place: None,
            parent: None,
            parent_placement: 0,
            container: None,
            wielder: None,
            in_world: true,
            teleport_stamp: None,
            position_stamp: 0,
            qualities: Qualities::default(),
            reports: Vec::new(),
            creations: 1,
        };
        o.take_descriptions();
        o
    }

    /// The descriptions taken: its place and stamps (as the physics object takes its update
    /// times from the description), what it hangs from, what holds it. An object that hangs from
    /// nothing is placed where its description puts it, when that is in a cell (a description
    /// without a place puts it in none, and it stays where it was); one that hangs from another is
    /// in no place of its own.
    fn take_descriptions(&mut self) {
        let p = &self.physics;
        let parent = p.parent_id();
        // D2-06366
        self.position_stamp = p.stamp(0);
        if parent == 0 {
            let at = p.position();
            if at.cell != 0 {
                self.place = Some(Place {
                    cell: at.cell,
                    origin: at.frame.origin,
                    heading_motion: 0,
                    position_stamp: p.stamp(0),
                    teleport_stamp: p.stamp(1),
                    force_position_stamp: p.stamp(2),
                    time: 0.0,
                    flags: 0,
                    velocity: auberean_primitives::Vec3::ZERO,
                });
                // Being put in a cell brings back an object that had left the world.
                self.in_world = true;
            }
        } else {
            self.parent = Some((parent, p.location_id()));
            self.place = None;
        }
        self.take_holders();
    }

    /// A description message's weenie description takes the place of the creation's (the
    /// player's description carries one; it lacks the package and entity the object was made
    /// with, which the object keeps). A description without one leaves the creation's.
    // D2-07354
    fn take_description_weenie(&mut self, d: &Description) {
        if d.header & Description::WEENIE == 0 {
            return;
        }
        let mut w = d.weenie.clone();
        if w.mask & WeenieDesc::PACKAGE == 0 && self.weenie.mask & WeenieDesc::PACKAGE != 0 {
            w.mask |= WeenieDesc::PACKAGE;
            w.package_id = self.weenie.package_id;
        }
        if w.mask & WeenieDesc::ENTITY == 0 && self.weenie.mask & WeenieDesc::ENTITY != 0 {
            w.mask |= WeenieDesc::ENTITY;
            w.entity_did = self.weenie.entity_did;
        }
        self.weenie = w;
        self.take_holders();
    }

    /// Whether the object hangs from another.
    // D2-05625
    #[must_use]
    pub fn has_parent(&self) -> bool {
        self.parent.is_some()
    }

    /// What holds the object and what wields it, as its description says: held, it is out of
    /// the world. Taken again whenever the description's objects change (an item put down is no
    /// longer held, and in the world once placed).
    pub fn take_holders(&mut self) {
        let w = &self.weenie;
        let object = |i: usize| {
            (w.mask & WeenieDesc::OBJECT_BITS[i] != 0 && w.objects[i] != 0).then_some(w.objects[i])
        };
        self.container = object(0);
        self.wielder = object(1);
        if self.container.is_some() {
            self.in_world = false;
        }
    }
}

/// What the runtime does next because a message took effect.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// A server's call into the client's scripts: to the player's (`object` none) or to an
    /// object's.
    Call {
        object: Option<u64>,
        function: u32,
        args: Option<ScriptArgs>,
    },
    /// The server asked which data the client has.
    DataQuery,
    /// The records a world's overlay was made over, for this client to check its files against.
    OverlayBases(auberean_protocol::login::OverlayBases),
    /// A data download begins.
    DownloadBegins(DownloadBegins),
    /// A data record came.
    DataRecord(DataRecord),
    /// The server could not send a data record.
    DataRefused(DataRefused),
    /// The client waits in line for its download (the value: its place).
    DownloadWaiting(u32),
    /// The server ended the data download.
    DownloadEnds,
    /// The character list arrived (or changed).
    Characters,
    /// The player's creation: the client is in the world.
    PlayerCreated(u64),
    /// The player's object was created (or created again).
    PlayerObject,
    /// The server moved the player to a new place outright.
    Teleported,
    /// The character left the world.
    LeftWorld,
    /// The server refused the character's entry (or ended the login) for a reason.
    Refused(u32),
    /// A character of the account was deleted (the list it is gone from comes after).
    CharacterDeleted(u64),
}

/// Counts for logs and tests.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ModelStats {
    pub created: u32,
    pub updated: u32,
    pub forgotten: u32,
    pub positions: u32,
    pub qualities: u32,
    pub calls: u32,
}

/// The model.
#[derive(Clone, Debug, Default)]
pub struct Model {
    pub world_name: Option<StringInfo>,
    pub characters: Option<CharacterList>,
    /// The player's id, from its creation.
    pub player: Option<u64>,
    /// The player's description as it arrived.
    pub description: Option<Description>,
    /// The server's last answer to making a character: its response (1 made, 3 name in use, 4
    /// name refused, 8 no free slot), the character, how many answers came so far, and the error
    /// its response names.
    pub creation: Option<(u32, u64, u32, u32)>,
    /// The server's last refusal of an entry (or end of the login): its reason (11 the character
    /// is still in the world, 20 booted), and how many refusals came so far.
    pub refused: Option<(u32, u32)>,
    /// The server's own words with that refusal: a boot's; none for an entry refused.
    pub refused_with: Option<StringInfo>,
    objects: HashMap<u64, Object>,
    /// The temporary ids the client has given out and not taken back.
    temporary_ids: BTreeSet<u64>,
    pub stats: ModelStats,
}

impl Model {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// The server's answer to a character asked for, kept for the creation screens.
    // D2-03956
    fn answer_creation(&mut self, c: &CharacterCreated) {
        let n = self.creation.map_or(0, |(_, _, n, _)| n) + 1;
        self.creation = Some((c.response, c.identity.id, n, c.result));
    }

    /// The player's id, once its creation has taken effect; 0 (no id) before, and after the
    /// character left the world.
    // D2-05642
    #[must_use]
    pub fn player_id(&self) -> u64 {
        self.player.unwrap_or(0)
    }

    /// An object, by id.
    // D2-05693, D2-05641
    #[must_use]
    pub fn object(&self, id: u64) -> Option<&Object> {
        self.objects.get(&id)
    }

    /// An object, by id, to change.
    pub fn object_mut(&mut self, id: u64) -> Option<&mut Object> {
        self.objects.get_mut(&id)
    }

    /// The objects an object carries (whose parent it is), in id order.
    #[must_use]
    pub fn children(&self, id: u64) -> Vec<u64> {
        let mut c: Vec<u64> = self
            .objects
            .values()
            .filter(|o| o.parent.is_some_and(|(p, _)| p == id))
            .map(|o| o.id)
            .collect();
        c.sort_unstable();
        c
    }

    /// Every object.
    pub fn objects(&self) -> impl Iterator<Item = &Object> {
        self.objects.values()
    }

    /// The player's object.
    // D2-05646
    #[must_use]
    pub fn player_object(&self) -> Option<&Object> {
        self.player.and_then(|p| self.objects.get(&p))
    }

    /// Whether `id` is held by `holder`, directly or through containers it holds.
    #[must_use]
    pub fn is_held_by(&self, id: u64, holder: u64) -> bool {
        let mut at = id;
        for _ in 0..16 {
            let Some(o) = self.objects.get(&at) else {
                return false;
            };
            match o.container.or(o.wielder) {
                Some(h) if h == holder => return true,
                Some(h) => at = h,
                None => return false,
            }
        }
        false
    }

    /// The player's inventory: everything it holds or wields, directly or through containers.
    #[must_use]
    pub fn inventory(&self) -> Vec<u64> {
        let Some(p) = self.player else {
            return Vec::new();
        };
        let mut v: Vec<u64> = self
            .objects
            .keys()
            .copied()
            .filter(|id| *id != p && self.is_held_by(*id, p))
            .collect();
        v.sort_unstable();
        v
    }

    /// The objects in the world around the player (not held by anything).
    pub fn surroundings(&self) -> impl Iterator<Item = &Object> {
        let p = self.player;
        self.objects
            .values()
            .filter(move |o| o.in_world && Some(o.id) != p)
    }

    /// A temporary id for an object the client makes itself: the first from the start of the
    /// temporary ids not given out yet, unless it would not be a temporary id or an object has it.
    // D2-05714
    pub fn allocate_temporary_id(&mut self) -> Option<u64> {
        let mut id = crate::ids::temporary(0);
        while self.temporary_ids.contains(&id) {
            id = id.wrapping_add(1);
        }
        if !crate::ids::is_temporary(id) || self.objects.contains_key(&id) {
            return None;
        }
        self.temporary_ids.insert(id);
        Some(id)
    }

    /// Takes a temporary id back.
    // D2-05718
    pub fn deallocate_temporary_id(&mut self, id: u64) {
        self.temporary_ids.remove(&id);
    }

    /// The server had the client forget an object, and its time came: its engine object goes,
    /// with its qualities.
    // D2-08109, D2-08107, D2-08035, D2-08029, D2-08027, D2-08121, D2-08291, D2-08217, D2-08075
    // D2-08041, D2-07268, D2-07262, D2-07256
    pub fn forget(&mut self, id: u64) {
        if self.objects.remove(&id).is_some() {
            self.stats.forgotten += 1;
        }
    }

    /// The character left the world: the world's objects go, and the player with them.
    // D2-05732, D2-05728
    pub fn leave_world(&mut self) {
        self.objects.clear();
        self.player = None;
        self.description = None;
    }

    fn quality<V: Clone>(table: &mut Values<V>, key: u32, value: &V) {
        table.insert(key, value.clone());
    }

    /// A message takes effect, by its type. What the runtime does because of it comes back in
    /// order; a type the model does not keep changes nothing.
    // D2-03937, D2-00148
    pub fn apply(&mut self, item: &Item) -> Vec<Effect> {
        use ServerMessage as S;
        let mut fx = Vec::new();
        match &item.message {
            // The world's name, kept for the panels.
            // D2-03961, D2-03996
            // D2-00094
            S::WorldName(t) => self.world_name = Some(t.0.clone()),
            // The account's characters, kept for the character list, which is told.
            // D2-03953, D2-03993
            S::CharacterList(l) => {
                self.characters = Some(l.clone());
                fx.push(Effect::Characters);
            }
            // A character made: it joins the end of the list, unless the list has it already; one
            // the list has, brought back, takes the answer's grey.
            // D2-01899, D2-13110, D2-03990
            S::CharacterCreated(c) if c.response == CharacterCreated::OK => {
                self.answer_creation(c);
                if let Some(list) = self.characters.as_mut() {
                    match u32::try_from(crate::characters::which_slot(list, c.identity.id)) {
                        Ok(slot) => crate::characters::set_greyed_out_for(
                            list,
                            slot,
                            c.identity.seconds_greyed_out,
                        ),
                        Err(_) => list.characters.push(c.identity.clone()),
                    }
                }
                fx.push(Effect::Characters);
            }
            // D2-03990
            S::CharacterCreated(c) => {
                self.answer_creation(c);
                fx.push(Effect::Characters);
            }
            // A character deleted: told on, and the list is left as it is (the server sends the
            // account's list again after it).
            // D2-03954, D2-03991
            S::CharacterDeleted(d) => fx.push(Effect::CharacterDeleted(d.character)),
            S::DataQuery(_) => fx.push(Effect::DataQuery),
            S::OverlayBases(b) => fx.push(Effect::OverlayBases(b.clone())),
            S::DownloadBegins(b) => fx.push(Effect::DownloadBegins(b.clone())),
            S::DataRecord(r) => fx.push(Effect::DataRecord(r.clone())),
            S::DataRefused(r) => fx.push(Effect::DataRefused(*r)),
            S::DataPatchTimePending(v) => fx.push(Effect::DownloadWaiting(v.0)),
            S::DownloadEnds(_) => fx.push(Effect::DownloadEnds),
            // An entry refused: kept for the screens, which show why.
            // D2-03984, D2-03955, D2-03992
            S::CharacterRefused(r) => {
                let n = self.refused.map_or(0, |(_, n)| n) + 1;
                self.refused = Some((r.reason, n));
                self.refused_with = None;
                fx.push(Effect::Refused(r.reason));
            }
            // The login ended by a boot: kept as a refusal for the character booted, with the
            // server's words, for the screens to show.
            // D2-03974, D2-03951, D2-03992
            S::AccountBooted(t) => {
                let n = self.refused.map_or(0, |(_, n)| n) + 1;
                self.refused = Some((CharacterRefused::BOOTED, n));
                self.refused_with = Some(t.0.clone());
                fx.push(Effect::Refused(CharacterRefused::BOOTED));
            }
            // D2-03952, D2-03989
            S::CharExitGame(_) => {
                self.leave_world();
                fx.push(Effect::LeftWorld);
            }
            // D2-05643, D2-04058, D2-04059
            S::PlayerObject(p) => {
                self.player = Some(p.id);
                fx.push(Effect::PlayerCreated(p.id));
            }
            // The player's description, copied whole over the player's qualities: its tables and
            // its weenie description.
            // D2-03985, D2-08062, D2-08219, D2-07351, D2-07349
            S::PlayerQualities(q) => {
                self.description = Some(q.0.clone());
                if let Some(o) = self.player.and_then(|p| self.objects.get_mut(&p)) {
                    o.qualities = Qualities::from_description(&q.0);
                    o.take_description_weenie(&q.0);
                }
            }
            // An object made: the objects take it.
            // D2-03958
            S::CreateObject(c) => {
                let player = Some(c.id) == self.player;
                match self.objects.get_mut(&c.id) {
                    // The incarnation the client has: its descriptions again.
                    // D2-05660
                    Some(o) if o.instance_stamp == c.physics.instance_stamp => {
                        o.visual = c.visual.clone();
                        o.physics = c.physics.clone();
                        o.weenie.assign(&c.weenie);
                        o.take_descriptions();
                        o.creations += 1;
                        self.stats.updated += 1;
                    }
                    // A new object, or a newer incarnation in the old one's place.
                    // D2-05704, D2-05692
                    _ => {
                        let mut o = Object::from_create(c);
                        if player {
                            if let Some(d) = &self.description {
                                o.qualities = Qualities::from_description(d);
                                o.take_description_weenie(d);
                            }
                        }
                        self.objects.insert(c.id, o);
                        self.stats.created += 1;
                    }
                }
                if player {
                    // D2-04051
                    fx.push(Effect::PlayerObject);
                }
            }
            // The object goes at the next upkeep ([`Self::forget`]).
            S::ForgetObject(_) => {}
            // D2-06194
            S::Contain(o) => {
                if let Some(obj) = self.objects.get_mut(&o.0.id) {
                    if obj.newer_position(o.0.other_stamp) {
                        obj.parent = None;
                        obj.in_world = false;
                        obj.place = None;
                    }
                }
            }
            // D2-06191
            S::ObjectLeaves(e) => {
                let player = Some(e.sender.id) == self.player;
                if let Some(obj) = self.objects.get_mut(&e.sender.id) {
                    if obj.newer_position(e.action.stamp) && !player {
                        obj.in_world = false;
                        obj.place = None;
                        obj.parent = None;
                    }
                }
            }
            S::Position(p) | S::PositionCell(p) => {
                self.stats.positions += 1;
                let player = Some(p.sender.id) == self.player;
                if let Some(obj) = self.objects.get_mut(&p.sender.id) {
                    obj.newer_position(p.position_stamp);
                    let teleported = player
                        && obj
                            .teleport_stamp
                            .is_some_and(|old| old != p.teleport_stamp);
                    obj.teleport_stamp = Some(p.teleport_stamp);
                    obj.place = Some(Place::of(p));
                    if obj.reports.len() >= KEPT_REPORTS {
                        obj.reports.remove(0);
                    }
                    obj.reports.push(Place::of(p));
                    // A place in the world says it is out of any pack, whatever was said of that.
                    obj.in_world = true;
                    obj.parent = None;
                    obj.container = None;
                    if teleported {
                        fx.push(Effect::Teleported);
                    }
                }
            }
            // D2-06192
            S::Parent(e) => {
                if let Some(obj) = self.objects.get_mut(&e.sender.id) {
                    if obj.newer_position(e.action.parent.other_stamp) {
                        obj.parent = Some((e.action.parent.id, e.action.location));
                        obj.parent_placement = e.action.orientation;
                        obj.place = None;
                    }
                }
            }
            // The holder sends it; the object let go of is the one the message names, in the
            // incarnation it names.
            // D2-04014, D2-06193
            S::DeParent(e) => {
                if let Some(obj) = self.objects.get_mut(&e.action.id) {
                    if obj.instance_stamp == e.action.instance_stamp
                        && obj.newer_position(e.action.other_stamp)
                    {
                        obj.parent = None;
                        obj.in_world = false;
                        obj.place = None;
                    }
                }
            }
            S::UpdateVisualDesc(e) => {
                if let Some(obj) = self.objects.get_mut(&e.sender.id) {
                    obj.visual = e.action.clone();
                    obj.visual_updates = obj.visual_updates.wrapping_add(1);
                }
            }
            // An object's speed and jump scales, kept with its physics description: a body made
            // for it later starts with them, as one made at its creation would have taken them.
            S::VelocityScale(e) => {
                if let Some(obj) = self.objects.get_mut(&e.sender.id) {
                    obj.physics.velocity_scale = e.action.0;
                    obj.physics.bits |= PhysicsDesc::VELOCITY_SCALE;
                }
            }
            S::JumpScale(e) => {
                if let Some(obj) = self.objects.get_mut(&e.sender.id) {
                    obj.physics.jump_scale = e.action.0;
                    obj.physics.bits |= PhysicsDesc::JUMP_SCALE;
                }
            }
            S::PlayerCall(c) | S::PlayerCallAlternate(c) => {
                self.stats.calls += 1;
                fx.push(Effect::Call {
                    object: None,
                    function: c.function.0,
                    args: c.args.clone(),
                });
            }
            S::CellObjectCall(c) | S::VisualObjectCall(c) => {
                self.stats.calls += 1;
                fx.push(Effect::Call {
                    object: Some(c.object.id),
                    function: c.call.function.0,
                    args: c.call.args.clone(),
                });
            }
            m => self.apply_quality(m),
        }
        fx
    }

    fn apply_quality(&mut self, m: &ServerMessage) {
        use ServerMessage as S;
        let player = self.player;
        let mut on = |id: Option<u64>, f: &mut dyn FnMut(&mut Qualities)| {
            if let Some(o) = id.and_then(|id| self.objects.get_mut(&id)) {
                f(&mut o.qualities);
                self.stats.qualities += 1;
            }
        };
        match m {
            S::UpdateInt(q) | S::UpdateBool(q) => {
                let bool_ = matches!(m, S::UpdateBool(_));
                on(player, &mut |t| {
                    let table = if bool_ { &mut t.bools } else { &mut t.ints };
                    Self::quality(table, q.quality, &q.value);
                });
            }
            S::UpdateIntVisual(q) | S::UpdateBoolVisual(q) => {
                let bool_ = matches!(m, S::UpdateBoolVisual(_));
                on(Some(q.object.id), &mut |t| {
                    let table = if bool_ { &mut t.bools } else { &mut t.ints };
                    Self::quality(table, q.quality, &q.value);
                });
            }
            S::UpdateFloat(q) => on(player, &mut |t| {
                Self::quality(&mut t.floats, q.quality, &q.value)
            }),
            S::UpdateFloatVisual(q) => on(Some(q.object.id), &mut |t| {
                Self::quality(&mut t.floats, q.quality, &q.value)
            }),
            S::UpdateString(q) => on(player, &mut |t| {
                Self::quality(&mut t.strings, q.quality, &q.value)
            }),
            S::UpdateStringVisual(q) => {
                on(Some(q.object.id), &mut |t| {
                    Self::quality(&mut t.strings, q.quality, &q.value)
                });
            }
            S::UpdateDataId(q) => on(player, &mut |t| {
                Self::quality(&mut t.data_ids, q.quality, &q.value)
            }),
            S::UpdateDataIdVisual(q) => {
                on(Some(q.object.id), &mut |t| {
                    Self::quality(&mut t.data_ids, q.quality, &q.value)
                });
            }
            S::UpdateInstanceId(q) | S::UpdateLongInt(q) => {
                let long = matches!(m, S::UpdateLongInt(_));
                on(player, &mut |t| {
                    let table = if long {
                        &mut t.long_ints
                    } else {
                        &mut t.instance_ids
                    };
                    Self::quality(table, q.quality, &q.value);
                });
            }
            S::UpdateInstanceIdVisual(q) | S::UpdateLongIntVisual(q) => {
                let long = matches!(m, S::UpdateLongIntVisual(_));
                on(Some(q.object.id), &mut |t| {
                    let table = if long {
                        &mut t.long_ints
                    } else {
                        &mut t.instance_ids
                    };
                    Self::quality(table, q.quality, &q.value);
                });
            }
            S::UpdatePlacement(q) => on(player, &mut |t| {
                Self::quality(&mut t.positions, q.quality, &q.value)
            }),
            S::UpdatePositionVisual(q) => {
                on(Some(q.object.id), &mut |t| {
                    Self::quality(&mut t.positions, q.quality, &q.value)
                });
            }
            S::UpdateTimestamp(q) => on(player, &mut |t| {
                Self::quality(&mut t.timestamps, q.quality, &q.value)
            }),
            S::UpdateTimestampVisual(q) => {
                on(Some(q.object.id), &mut |t| {
                    Self::quality(&mut t.timestamps, q.quality, &q.value)
                });
            }
            S::UpdateStringInfo(q) => on(player, &mut |t| {
                Self::quality(&mut t.string_infos, q.quality, &q.value)
            }),
            S::UpdateStringInfoVisual(q) => {
                on(Some(q.object.id), &mut |t| {
                    Self::quality(&mut t.string_infos, q.quality, &q.value)
                });
            }
            _ => {}
        }
    }
}
