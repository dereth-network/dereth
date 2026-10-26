//! Turning each frame's state into events.
//!
//! The host compares what the frame shows with what it saw the frame before and names the
//! differences, in a fixed order: session, objects created, objects removed, the selection,
//! vitals, experience, luminance, enchantments, the open container, the player's items,
//! appraisals, the vendor, the trade, then the frame's chat lines in the order they arrived. What
//! no plugin listens for is not compared. The comparison reads the same state the retail UI reads, so an event is exactly a change the player
//! could have seen, and a replayed recording yields the same events in the same order.
//!
//! The first frame after login sets the baseline: the values a character logs in with are not
//! changes.

use std::collections::{BTreeMap, BTreeSet};

use dereth_client_contract::chat::interface::ChatMessage;
use dereth_client_contract::view::Vital;
use dereth_client_runtime::frame_hook::HookFrame;
use dereth_plugin_api::{Event, Value};

/// The chat line kinds, by text type. A type with no name here is reported by number.
const CHAT_KINDS: &[(u8, &str)] = &[
    (0, "default"),
    (1, "all_channels"),
    (2, "speech"),
    (3, "tell"),
    (4, "tell_sent"),
    (5, "system"),
    (6, "combat"),
    (7, "magic"),
    (8, "channel"),
    (9, "channel_sent"),
    (10, "social"),
    (11, "social_sent"),
    (12, "emote"),
    (13, "advancement"),
    (14, "abuse"),
    (15, "help"),
    (16, "appraisal"),
    (17, "spellcasting"),
    (18, "allegiance"),
    (19, "fellowship"),
    (20, "world_broadcast"),
    (21, "combat_enemy"),
    (22, "combat_self"),
    (23, "recall"),
    (24, "craft"),
    (25, "salvaging"),
    (0x1A, "error"),
];

/// The kind name of a chat text type.
#[must_use]
pub fn chat_kind(ty: u8) -> Value {
    CHAT_KINDS
        .iter()
        .find(|(t, _)| *t == ty)
        .map_or(Value::Int(i64::from(ty)), |(_, n)| Value::from(*n))
}

/// The text type of a kind name, for `chat.print` styles.
#[must_use]
pub fn chat_type(kind: &str) -> Option<u8> {
    CHAT_KINDS.iter().find(|(_, n)| *n == kind).map(|(t, _)| *t)
}

/// The chat line event for line `index` of the frame.
#[must_use]
pub fn chat_event(index: usize, line: &ChatMessage) -> Event {
    Event::new(
        "chat.line",
        Value::map()
            .with("line", i64::try_from(index).unwrap_or(i64::MAX))
            .with("text", line.body.as_str())
            .with("kind", chat_kind(line.ty))
            .with("type", u32::from(line.ty))
            .with("window", line.window)
            .with("prefix", line.prefix.clone()),
    )
}

/// What the host saw last frame.
#[derive(Debug, Default)]
pub struct Observer {
    player: Option<u32>,
    entered: bool,
    /// Whether the character was in portal space.
    portal: bool,
    /// The character's count of deaths.
    deaths: Option<i32>,
    vitals: BTreeMap<&'static str, (u32, u32)>,
    xp: Option<(i64, i64)>,
    luminance: Option<(i64, i64)>,
    selection: Option<u32>,
    objects: Option<BTreeSet<u32>>,
    /// What each object showed, for `world.object_changed`.
    object_states: Option<BTreeMap<u32, ObjectState>>,
    /// Where each object was last said to be, for `world.object_moved`.
    object_places: Option<BTreeMap<u32, dereth_primitives::Position>>,
    enchantments: Option<BTreeMap<u32, String>>,
    /// The open corpse or chest, once its contents have arrived.
    container: Option<u32>,
    /// The player's items and the container each is in.
    items: Option<BTreeMap<u32, u32>>,
    /// The appraisal cache's delivery count, and each appraised object's delivery.
    appraisals: Option<(u64, BTreeMap<u32, u64>)>,
    vendor: Option<u32>,
    /// The fellowship: its name, leader and members, or none.
    fellowship: Option<Option<(String, u32, Vec<u32>)>>,
    /// The trade: whether it is open, its partner, and a digest of both sides and acceptance.
    trade: Option<(bool, u32, u64)>,
}

/// Every item the player holds (the main pack, the side packs and what is wielded), with the
/// container each is in (the player itself for the main pack and for what is wielded).
#[must_use]
pub fn held_items(frame: &HookFrame<'_>) -> BTreeMap<u32, u32> {
    let mut out = BTreeMap::new();
    let view = frame.view;
    let Some(player) = frame.world.player else {
        return out;
    };
    let mut packs = vec![player];
    packs.extend(view.contained_containers(player).iter().copied());
    for c in packs {
        if c != player {
            out.insert(c.0, player.0);
        }
        for i in view.container_contents(c) {
            out.insert(i.0, c.0);
        }
    }
    for (i, _) in view.equipment(player) {
        out.insert(i.0, player.0);
    }
    // What the server has said the character wields since its description, which the list of
    // worn items may not show yet.
    for (id, w) in frame.world.tables.weenies.iter() {
        if w.pwd.wielder_id == Some(player) {
            out.insert(id.0, player.0);
        }
    }
    out
}

/// The player's total and unassigned experience, straight off its qualities (integer-64
/// properties 1 and 2), once the player's description has arrived.
#[must_use]
pub fn experience(world: &dereth_client_model::World) -> Option<(i64, i64)> {
    use dereth_client_model::{StatKey, StatType, StatValue};
    let q = world.player_qualities()?;
    let int64 = |p: u32| match q.get(StatKey::new(StatType::Int64, p)) {
        Some(StatValue::Int64(n)) => Some(n),
        _ => None,
    };
    Some((int64(1)?, int64(2).unwrap_or(0)))
}

/// The integer property that counts the character's deaths.
const NUM_DEATHS: u32 = 43;

/// How far, in metres, an object moves before `world.object_moved` says so again.
pub const MOVE_STEP: f64 = 1.0;

/// What `world.object_changed` compares of an object, each under the key it reports.
#[derive(Debug, Clone, PartialEq)]
struct ObjectState {
    name: String,
    stack: Option<u16>,
    value: Option<u32>,
    burden: Option<u16>,
    container: Option<u32>,
    wielder: Option<u32>,
    icon: u32,
    slot: Option<u32>,
    uses: Option<u16>,
    bitfield: u32,
    appraisal: u64,
    health: Option<(u32, u32)>,
}

impl ObjectState {
    fn of(frame: &HookFrame<'_>, w: &dereth_client_model::Weenie) -> Self {
        let d = &w.pwd;
        let health = frame
            .view
            .fellowship()
            .and_then(|f| {
                f.members
                    .iter()
                    .find(|m| m.id == w.id)
                    .map(|m| (m.current_health, m.max_health))
            })
            .or_else(|| {
                frame
                    .world
                    .appraisal
                    .get(w.id)
                    .and_then(|a| a.creature_profile.as_ref())
                    .map(|c| (c.health, c.max_health))
            });
        Self {
            name: d.name.clone(),
            stack: d.stack_size,
            value: d.value,
            burden: d.burden,
            container: d.container_id.map(|c| c.0),
            wielder: d.wielder_id.map(|c| c.0),
            icon: d.icon_id,
            slot: d.location,
            uses: d.structure,
            bitfield: d.bitfield,
            appraisal: frame.world.appraisal.delivery(w.id),
            health,
        }
    }

    /// The keys that differ from `before`, in a fixed order.
    fn changed(&self, before: &Self) -> Vec<&'static str> {
        let mut out = Vec::new();
        let mut note = |differs: bool, key: &'static str| {
            if differs {
                out.push(key);
            }
        };
        note(self.name != before.name, "name");
        note(self.stack != before.stack, "stack");
        note(self.value != before.value, "value");
        note(self.burden != before.burden, "burden");
        note(self.container != before.container, "container");
        note(self.wielder != before.wielder, "wielder");
        note(self.icon != before.icon, "icon");
        note(self.slot != before.slot, "slot");
        note(self.uses != before.uses, "uses");
        note(self.bitfield != before.bitfield, "flags");
        note(self.appraisal != before.appraisal, "appraisal");
        note(self.health != before.health, "health");
        out
    }
}

const VITALS: [(Vital, &str); 3] = [
    (Vital::Health, "health"),
    (Vital::Stamina, "stamina"),
    (Vital::Mana, "mana"),
];

impl Observer {
    /// This frame's events, other than chat. `wants` says whether any plugin listens for an event;
    /// what only unheard events need is not compared.
    pub fn observe(&mut self, frame: &HookFrame<'_>, wants: &dyn Fn(&str) -> bool) -> Vec<Event> {
        let objects = wants("world.object_created") || wants("world.object_removed");
        let mut out = Vec::new();
        let view = frame.view;
        // Logged in once the character is known by its name too (its object can come first).
        let player = view
            .player()
            .filter(|_| view.character_name().is_some())
            .map(|p| p.0);
        if player != self.player {
            if self.player.is_some() {
                out.push(Event::new("session.logout", Value::map()));
                *self = Self::default();
            }
            if let Some(id) = player {
                self.player = Some(id);
                out.push(Event::new(
                    "session.login",
                    Value::map()
                        .with("id", id)
                        .with("character", view.character_name().map(str::to_owned)),
                ));
            }
        }
        let Some(id) = self.player else {
            return out;
        };
        let pid = dereth_primitives::ObjectId(id);
        if !self.entered && frame.world.physics(pid).is_some() {
            self.entered = true;
            out.push(Event::new(
                "session.enter_world",
                Value::map().with("id", id),
            ));
        }
        let portal = frame.session.portal_space;
        if portal != self.portal {
            self.portal = portal;
            let name = if portal {
                "session.portal_enter"
            } else {
                "session.portal_exit"
            };
            out.push(Event::new(name, Value::map()));
        }

        if objects {
            let now: BTreeSet<u32> = frame
                .world
                .tables
                .weenies
                .keys()
                .into_iter()
                .map(|k| k.0)
                .collect();
            if let Some(before) = &self.objects {
                for &o in now.difference(before) {
                    let oid = dereth_primitives::ObjectId(o);
                    let name = view.name(oid).map(str::to_owned);
                    let class = frame.world.weenie(oid).map(|w| crate::reads::class_of(w).1);
                    out.push(Event::new(
                        "world.object_created",
                        Value::map()
                            .with("id", o)
                            .with("name", name)
                            .with("class", class),
                    ));
                }
                for &o in before.difference(&now) {
                    out.push(Event::new(
                        "world.object_removed",
                        Value::map().with("id", o),
                    ));
                }
            }
            self.objects = Some(now);
        } else {
            self.objects = None;
        }
        self.object_changes(frame, wants, &mut out);

        let selection = frame.world.selected.map(|s| s.0);
        if selection != self.selection {
            out.push(Event::new(
                "world.selection_changed",
                Value::map()
                    .with("id", selection)
                    .with("previous", self.selection),
            ));
            self.selection = selection;
        }

        for (vital, name) in VITALS {
            let Some(now) = view.vital(pid, vital) else {
                continue;
            };
            match self.vitals.insert(name, now) {
                Some(before) if before != now => out.push(Event::new(
                    "character.vital",
                    Value::map()
                        .with("vital", name)
                        .with("current", now.0)
                        .with("max", now.1)
                        .with("previous", before.0),
                )),
                _ => {}
            }
        }

        if let Some(now) = experience(frame.world) {
            if let Some(before) = self.xp.replace(now) {
                if before != now {
                    out.push(Event::new(
                        "character.xp",
                        Value::map()
                            .with("total", now.0)
                            .with("unassigned", now.1)
                            .with("gained", now.0.saturating_sub(before.0)),
                    ));
                }
            }
        }

        let lum = view.luminance();
        if let Some(before) = self.luminance.replace(lum) {
            if before != lum {
                out.push(Event::new(
                    "character.luminance",
                    Value::map()
                        .with("earned", lum.0)
                        .with("max", lum.1)
                        .with("gained", lum.0 - before.0),
                ));
            }
        }

        // Integer property 43: the character's count of deaths, which the server raises as the
        // character dies.
        if let Some(q) = frame.world.player_qualities() {
            let deaths = q.inq_int(NUM_DEATHS);
            if self
                .deaths
                .replace(deaths)
                .is_some_and(|before| deaths > before)
            {
                out.push(Event::new(
                    "character.death",
                    Value::map().with("deaths", deaths),
                ));
            }
        }

        if wants("character.fellowship") {
            let now = frame.view.fellowship().map(|f| {
                (
                    f.name.clone(),
                    f.leader.0,
                    f.members.iter().map(|m| m.id.0).collect::<Vec<_>>(),
                )
            });
            if let Some(before) = self.fellowship.replace(now.clone()) {
                let change = match (&before, &now) {
                    (None, Some(_)) => Some("joined"),
                    (Some(_), None) => Some("left"),
                    (Some(a), Some(b)) if a != b => Some("changed"),
                    _ => None,
                };
                if let Some(change) = change {
                    let (name, members) = now
                        .as_ref()
                        .map_or((None, Vec::new()), |(n, _, m)| (Some(n.clone()), m.clone()));
                    out.push(Event::new(
                        "character.fellowship",
                        Value::map().with("change", change).with("name", name).with(
                            "members",
                            Value::List(members.into_iter().map(Value::from).collect()),
                        ),
                    ));
                }
            }
        } else {
            self.fellowship = None;
        }
        self.enchantments(frame, wants, &mut out);
        self.inventory(frame, wants, &mut out);
        self.commerce(frame, &mut out);
        out
    }

    /// `world.object_changed` for each object whose state differs from the last frame's, and
    /// `world.object_moved` for each that is [`MOVE_STEP`] or more from where it was last said to
    /// be; both in id order, an object first seen setting its baseline.
    fn object_changes(
        &mut self,
        frame: &HookFrame<'_>,
        wants: &dyn Fn(&str) -> bool,
        out: &mut Vec<Event>,
    ) {
        if wants("world.object_changed") {
            let mut now = BTreeMap::new();
            for (id, w) in frame.world.tables.weenies.iter() {
                now.insert(id.0, ObjectState::of(frame, w));
            }
            if let Some(before) = &self.object_states {
                for (id, s) in &now {
                    let Some(b) = before.get(id) else { continue };
                    let keys = s.changed(b);
                    if !keys.is_empty() {
                        out.push(Event::new(
                            "world.object_changed",
                            Value::map().with("id", *id).with(
                                "changed",
                                Value::List(keys.into_iter().map(Value::from).collect()),
                            ),
                        ));
                    }
                }
            }
            self.object_states = Some(now);
        } else {
            self.object_states = None;
        }
        if wants("world.object_moved") {
            let mut places = self.object_places.take().unwrap_or_default();
            let mut seen = BTreeSet::new();
            for id in frame.world.tables.weenies.keys() {
                let Some(at) = frame.places.object(id) else {
                    continue;
                };
                seen.insert(id.0);
                match places.get(&id.0) {
                    Some(was) if crate::places::distance(was, &at) >= MOVE_STEP => {
                        out.push(Event::new(
                            "world.object_moved",
                            Value::map()
                                .with("id", id.0)
                                .with("position", crate::places::to_value(&at))
                                .with(
                                    "moved",
                                    (crate::places::distance(was, &at) * 100.0).round() / 100.0,
                                ),
                        ));
                        places.insert(id.0, at);
                    }
                    Some(_) => {}
                    None => {
                        places.insert(id.0, at);
                    }
                }
            }
            places.retain(|id, _| seen.contains(id));
            self.object_places = Some(places);
        } else {
            self.object_places = None;
        }
    }

    fn enchantments(
        &mut self,
        frame: &HookFrame<'_>,
        wants: &dyn Fn(&str) -> bool,
        out: &mut Vec<Event>,
    ) {
        if !(wants("character.enchantment_added") || wants("character.enchantment_removed")) {
            self.enchantments = None;
            return;
        }
        let mut now = BTreeMap::new();
        let mut remaining = BTreeMap::new();
        for e in frame.view.active_effects() {
            remaining.insert(e.spell, (!e.permanent).then_some(e.remaining));
            now.insert(e.spell, e.name);
        }
        if let Some(before) = &self.enchantments {
            for (s, name) in &now {
                if !before.contains_key(s) {
                    out.push(Event::new(
                        "character.enchantment_added",
                        Value::map()
                            .with("spell", *s)
                            .with("name", name.as_str())
                            .with("remaining", remaining.get(s).copied().flatten()),
                    ));
                }
            }
            for (s, name) in before {
                if !now.contains_key(s) {
                    out.push(Event::new(
                        "character.enchantment_removed",
                        Value::map().with("spell", *s).with("name", name.as_str()),
                    ));
                }
            }
        }
        self.enchantments = Some(now);
    }

    fn inventory(
        &mut self,
        frame: &HookFrame<'_>,
        wants: &dyn Fn(&str) -> bool,
        out: &mut Vec<Event>,
    ) {
        let w = frame.world;
        let open = w
            .ground_object
            .filter(|c| w.inventory(*c).is_some())
            .map(|c| c.0);
        if open != self.container {
            if let Some(c) = self.container {
                out.push(Event::new(
                    "inventory.container_closed",
                    Value::map().with("id", c),
                ));
            }
            if let Some(c) = open {
                out.push(Event::new(
                    "inventory.container_opened",
                    Value::map().with("id", c),
                ));
            }
            self.container = open;
        }

        if wants("inventory.item_added")
            || wants("inventory.item_removed")
            || wants("inventory.item_moved")
        {
            let now = held_items(frame);
            if let Some(before) = &self.items {
                for (i, c) in &now {
                    match before.get(i) {
                        None => out.push(Event::new(
                            "inventory.item_added",
                            Value::map().with("id", *i).with("container", *c),
                        )),
                        Some(p) if p != c => out.push(Event::new(
                            "inventory.item_moved",
                            Value::map()
                                .with("id", *i)
                                .with("container", *c)
                                .with("previous", *p),
                        )),
                        Some(_) => {}
                    }
                }
                for i in before.keys() {
                    if !now.contains_key(i) {
                        out.push(Event::new(
                            "inventory.item_removed",
                            Value::map().with("id", *i),
                        ));
                    }
                }
            }
            self.items = Some(now);
        } else {
            self.items = None;
        }

        if wants("items.appraised") {
            let delivered = w.appraisal.delivered;
            let first = self.appraisals.is_none();
            let (seen, known) = self
                .appraisals
                .get_or_insert_with(|| (u64::MAX, BTreeMap::new()));
            if *seen != delivered {
                *seen = delivered;
                for id in w.tables.weenies.keys() {
                    let d = w.appraisal.delivery(id);
                    if d != 0 && known.insert(id.0, d) != Some(d) && !first {
                        out.push(Event::new("items.appraised", Value::map().with("id", id.0)));
                    }
                }
            }
        } else {
            self.appraisals = None;
        }
    }

    fn commerce(&mut self, frame: &HookFrame<'_>, out: &mut Vec<Event>) {
        let vendor = frame.world.shop.vendor_id.map(|v| v.0);
        if vendor != self.vendor {
            if let Some(v) = self.vendor {
                out.push(Event::new(
                    "commerce.vendor_closed",
                    Value::map().with("id", v),
                ));
            }
            if let Some(v) = vendor {
                out.push(Event::new(
                    "commerce.vendor_opened",
                    Value::map().with("id", v),
                ));
            }
            self.vendor = vendor;
        }

        let t = frame.view.trade();
        let partner = t.partner.map_or(0, |p| p.0);
        let mut digest = u64::from(t.accepted) | u64::from(t.partner_accepted) << 1;
        for r in t.self_rows.iter() {
            digest = digest.rotate_left(7) ^ u64::from(r.item.0);
        }
        digest = digest.rotate_left(13);
        for r in t.partner_rows.iter() {
            digest = digest.rotate_left(7) ^ u64::from(r.item.0);
        }
        let was = self.trade.replace((t.open, partner, digest));
        let (was_open, was_digest) = was.map_or((false, digest), |(o, _, d)| (o, d));
        let partner_value = Value::map().with("partner", partner);
        if t.open && !was_open {
            out.push(Event::new("commerce.trade_opened", partner_value));
        } else if !t.open && was_open {
            out.push(Event::new("commerce.trade_closed", Value::map()));
        } else if t.open && digest != was_digest {
            out.push(Event::new("commerce.trade_changed", partner_value));
        }
    }
}
