//! The soak's invariants, checked over the whole run:
//!
//! - **no panic**: a process-wide panic hook counts every panic, the ones the world loop catches
//!   (`catch_unwind` around each action and stage) included;
//! - **no guid reused while held**: a guid the world shows under a new identity (weenie and
//!   `CreationTimestamp`) while the shard still holds the old one, or any possession of a
//!   logged-off character appearing in the world while its owner is away;
//! - **every logged-out character reloads deep-equal**: at log-off (the player offline, still in
//!   the world) its biota and every possession equal the shard's; after it enters again they equal
//!   what they were at log-off, but for the login bookkeeping (`Age`, `LoginTimestamp`,
//!   `HeartbeatTimestamp`, `CheckpointTimestamp`, as `persistence_roundtrip.rs` allows);
//! - the **bounded object count and queues** are judged from the samples (`super::metrics`).
//!
//! Nothing here is an ACE port; there are no ACE anchors.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use empyrean_entity::enums::PropertyInt;
use empyrean_entity::{Biota, ObjectGuid};
use empyrean_store::adapter::biota_converter::BiotaConverter;
use empyrean_world::managers::landblock_manager;
use empyrean_world::world_objects::{container, creature_equipment};
use empyrean_world::World;

/// At most this many examples of each failure are kept.
const EXAMPLES: usize = 20;

/// Counts every panic in the process while installed.
#[derive(Debug, Clone, Default)]
pub struct PanicCounter {
    pub count: Arc<AtomicU64>,
    pub examples: Arc<Mutex<Vec<String>>>,
}

impl PanicCounter {
    /// Installs the hook (chained to the previous one, which still prints).
    #[must_use]
    pub fn install() -> Self {
        let counter = Self::default();
        let (count, examples) = (Arc::clone(&counter.count), Arc::clone(&counter.examples));
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            count.fetch_add(1, Ordering::SeqCst);
            if let Ok(mut e) = examples.lock() {
                if e.len() < EXAMPLES {
                    e.push(info.to_string());
                }
            }
            previous(info);
        }));
        counter
    }

    #[must_use]
    pub fn count(&self) -> u64 {
        self.count.load(Ordering::SeqCst)
    }

    /// Restores the default hook.
    pub fn uninstall(&self) {
        let _ = std::panic::take_hook();
    }
}

/// A biota as an order-free set of lines (`persistence_roundtrip.rs`'s `canon`): one per property,
/// row or record; an absent and an empty collection read the same. Row ids
/// (`database_record_id`) are the shard's own and are left out: a biota built from a weenie has
/// none until it is saved.
#[must_use]
pub fn canon(b: &Biota) -> BTreeSet<String> {
    canon_with_ids(b)
        .into_iter()
        .map(|l| strip_record_ids(&l))
        .collect()
}

fn strip_record_ids(line: &str) -> String {
    const KEY: &str = "database_record_id: ";
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(i) = rest.find(KEY) {
        out.push_str(&rest[..i + KEY.len()]);
        rest = &rest[i + KEY.len()..];
        let digits = rest
            .find(|c: char| !c.is_ascii_digit())
            .unwrap_or(rest.len());
        out.push('_');
        rest = &rest[digits..];
    }
    out.push_str(rest);
    out
}

fn canon_with_ids(b: &Biota) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    out.insert(format!(
        "id {} wcid {} type {:?}",
        b.id, b.weenie_class_id, b.weenie_type
    ));
    macro_rules! dict {
        ($f:ident) => {
            for (k, v) in b.$f.iter().flat_map(|d| d.iter()) {
                out.insert(format!("{} {k:?} = {v:?}", stringify!($f)));
            }
        };
    }
    macro_rules! list {
        ($f:ident) => {
            for v in b.$f.iter().flat_map(|d| d.iter()) {
                out.insert(format!("{} {v:?}", stringify!($f)));
            }
        };
    }
    dict!(properties_bool);
    dict!(properties_did);
    dict!(properties_float);
    dict!(properties_iid);
    dict!(properties_int);
    dict!(properties_int64);
    dict!(properties_string);
    dict!(properties_position);
    dict!(properties_spell_book);
    dict!(properties_attribute);
    dict!(properties_attribute_2nd);
    dict!(properties_skill);
    dict!(properties_allegiance);
    dict!(house_permissions);
    list!(properties_anim_part);
    list!(properties_palette);
    list!(properties_texture_map);
    list!(properties_book_page_data);
    list!(properties_enchantment_registry);
    for v in b.properties_create_list.iter().flat_map(|d| d.iter()) {
        out.insert(format!("create_list {v:?}"));
    }
    for v in b.properties_emote.iter().flat_map(|d| d.iter()) {
        out.insert(format!("emote {v:?}"));
    }
    for v in b.properties_generator.iter().flat_map(|d| d.iter()) {
        out.insert(format!("generator {v:?}"));
    }
    for (k, v) in b.properties_body_part.iter().flat_map(|d| d.iter()) {
        out.insert(format!("body_part {k:?} = {v:?}"));
    }
    if let Some(book) = &b.properties_book {
        out.insert(format!("book {book:?}"));
    }
    out
}

/// What logging out and in changes on the player's own biota, and what its heartbeat advances:
/// the login bookkeeping (`persistence_roundtrip.rs`'s four), the portal-space collision flags
/// (`LogOut` sets them, entering the world clears them), and the clock-driven values the
/// heartbeat moves on every 5 s (`LifestoneProtectionTimestamp` and, when it runs out,
/// `UnderLifestoneProtection`: `Player.LifestoneProtectionTick`; and each enchantment's
/// `start_time`, which is compared without it).
const VOLATILE: [&str; 8] = [
    "Age",
    "LoginTimestamp",
    "HeartbeatTimestamp",
    "CheckpointTimestamp",
    "IgnoreCollisions",
    "ReportCollisions",
    "LifestoneProtectionTimestamp",
    "UnderLifestoneProtection",
];

fn without_volatile(s: &BTreeSet<String>) -> BTreeSet<String> {
    s.iter()
        .filter(|l| !VOLATILE.iter().any(|k| l.contains(k)))
        .map(|l| {
            if l.starts_with("properties_enchantment_registry") {
                strip_field(l, "start_time: ")
            } else {
                l.clone()
            }
        })
        .collect()
}

/// A vitae entry worked off to 1.0 (`StatModValue.EpsilonEquals(1.0f) || > 1.0f`): the login's
/// `EnchantmentRegistry` dispels it (ACE: `Network/Structure/EnchantmentRegistry.cs`).
fn spent_vitae(line: &str) -> bool {
    let key = "stat_mod_value: ";
    line.starts_with("properties_enchantment_registry")
        && line.contains("SpellCategory::Vitae")
        && line
            .find(key)
            .and_then(|i| {
                line[i + key.len()..]
                    .split([',', ' ', '}'])
                    .next()?
                    .parse::<f32>()
                    .ok()
            })
            .is_some_and(|v| (v - 1.0).abs() < 0.0001 || v > 1.0)
}

/// `line` with the value of `key` replaced by `_`.
fn strip_field(line: &str, key: &str) -> String {
    let Some(i) = line.find(key) else {
        return line.to_owned();
    };
    let rest = &line[i + key.len()..];
    let end = rest.find([',', ' ', '}']).unwrap_or(rest.len());
    format!("{}_{}", &line[..i + key.len()], &rest[end..])
}

/// A character's possessions in the world: its inventory (side packs' contents included) and what
/// it wields.
#[must_use]
pub fn possessions(w: &World, player: ObjectGuid) -> Vec<ObjectGuid> {
    let mut out = Vec::new();
    let mut stack = container::inventory_values(w, player);
    while let Some(g) = stack.pop() {
        out.push(g);
        if w.objects.get(g).is_some_and(|o| o.is_container()) {
            stack.extend(container::inventory_values(w, g));
        }
    }
    out.extend(creature_equipment::equipped_objects_values(w, player));
    out.sort_unstable_by_key(|g| g.full());
    out.dedup();
    out
}

/// Whether two canonical biotas differ only in their vitals' `current_level`.
fn only_vitals_differ(a: &BTreeSet<String>, b: &BTreeSet<String>) -> bool {
    let vital = |l: &&String| l.starts_with("properties_attribute_2nd");
    let strip = |s: &BTreeSet<String>| -> BTreeSet<String> {
        s.iter()
            .map(|l| {
                if l.starts_with("properties_attribute_2nd") {
                    strip_field(l, "current_level: ")
                } else {
                    l.clone()
                }
            })
            .collect()
    };
    a.symmetric_difference(b).all(|l| vital(&l)) && strip(a) == strip(b)
}

/// What a blow or a death that lands after the log-off save changes on the character (an
/// ACE-BUG kept at `Player.FinalizeLogout`): the vitals, the vitae, `Killer`, `NumDeaths`,
/// `DeathLevel`, `VitaeCpPool`, the death timestamp, and the defense skill's use (its points and
/// the experience they come from).
fn after_save_line(l: &str) -> bool {
    const KEYS: [&str; 9] = [
        "PropertyInstanceId::Killer ",
        "PropertyInt::NumDeaths ",
        "PropertyInt::DeathLevel ",
        "PropertyInt::VitaeCpPool ",
        "PropertyFloat::DeathTimestamp ",
        "PropertyInt64::AvailableExperience ",
        "PropertyInt64::TotalExperience ",
        "PropertyFloat::LastPkAttackTimestamp ",
        "PropertyInstanceId::CurrentAttacker ",
    ];
    l.starts_with("properties_attribute_2nd")
        || l.starts_with("properties_skill")
        || (l.starts_with("properties_enchantment_registry") && l.contains("SpellCategory::Vitae"))
        || KEYS.iter().any(|k| l.contains(k))
}

/// Whether the character in the world differs from the shard only in what a blow or a death after
/// the log-off save changes (see [`after_save_line`]).
fn only_after_save_differs(world: &BTreeSet<String>, shard: &BTreeSet<String>) -> bool {
    world
        .symmetric_difference(shard)
        .all(|l| after_save_line(l))
}

/// An ACE-BUG, kept: a Location orientation left all NaN by
/// `Creature.Rotate` toward an object on exactly the same point (two bots trading on the Academy
/// entry spot: `Normalize(Vector3.Zero)`). The SQLite shard cannot hold NaN and reads it back as
/// the identity. Whether the two differ only there.
fn only_nan_orientation_differs(world: &BTreeSet<String>, shard: &BTreeSet<String>) -> bool {
    let nan = |l: &String| l.starts_with("properties_position") && l.contains("rotation_w: NaN");
    let strip = |set: &BTreeSet<String>| -> BTreeSet<String> {
        set.iter()
            .map(|l| {
                if l.starts_with("properties_position") {
                    [
                        "rotation_w: ",
                        "rotation_x: ",
                        "rotation_y: ",
                        "rotation_z: ",
                    ]
                    .iter()
                    .fold(l.clone(), |l, k| strip_field(&l, k))
                } else {
                    l.clone()
                }
            })
            .collect()
    };
    world.iter().any(nan) && strip(world) == strip(shard)
}

/// `(cell, x, y, z)` from a canonical `PropertiesPosition` line.
fn position_of(line: &str) -> Option<(u32, f32, f32, f32)> {
    let field = |name: &str| -> Option<&str> {
        let i = line.find(&format!("{name}: "))? + name.len() + 2;
        let rest = &line[i..];
        Some(&rest[..rest.find([',', ' ', '}']).unwrap_or(rest.len())])
    };
    Some((
        field("obj_cell_id")?.parse().ok()?,
        field("position_x")?.parse().ok()?,
        field("position_y")?.parse().ok()?,
        field("position_z")?.parse().ok()?,
    ))
}

fn short(l: &str) -> String {
    if l.len() > 240 {
        format!(
            "{}...",
            &l[..l.char_indices().nth(240).map_or(l.len(), |(i, _)| i)]
        )
    } else {
        l.to_owned()
    }
}

fn diff(a: &BTreeSet<String>, b: &BTreeSet<String>) -> String {
    let only_a: Vec<_> = a.difference(b).take(6).map(|l| short(l)).collect();
    let only_b: Vec<_> = b.difference(a).take(6).map(|l| short(l)).collect();
    format!("only in first: {only_a:?}; only in second: {only_b:?}")
}

/// What a character was when it went offline.
#[derive(Debug, Clone)]
struct Snapshot {
    /// guid -> canonical biota, the player first: the last state in the world.
    biotas: BTreeMap<u32, BTreeSet<String>>,
    /// The player's biota when it went offline (`PlayerManager.SwitchPlayerFromOnlineToOffline`).
    player_at_switch: BTreeSet<String>,
}

/// The relog and guid invariants.
#[derive(Debug, Default)]
pub struct Checks {
    snapshots: HashMap<u32, Snapshot>,
    /// Possessions of characters that are fully out of the world, with their weenie.
    held_offline: HashMap<u32, (u32, u32)>,
    /// guid -> (weenie, CreationTimestamp) the last time the scan saw it.
    identities: HashMap<u32, (u32, Option<i32>)>,

    pub saved_ok: u64,
    pub saved_mismatch: u64,
    pub reload_ok: u64,
    pub reload_mismatch: u64,
    /// Re-entries back as the player was when it went offline rather than as it left the world
    /// (`OfflinePlayer` holding a copy of the biota taken at the switch; ACE shares the
    /// Player's). A failure, since the final biota is handed over at release.
    pub reload_stale_offline_biota: u64,
    pub stale_examples: Vec<String>,
    /// Characters whose vitals moved after their log-off save (the shard has the earlier value).
    pub vitals_after_save: u64,
    /// An ACE-BUG, kept: characters changed after their log-off save by more than their
    /// vitals (a death landing during the log-off: `NumDeaths`, `Killer`, ...). The shard holds
    /// exactly what the character was at the save (the offline switch), so ACE loses the change
    /// at the next restart too; within the run the character re-enters with it.
    pub changed_after_save: u64,
    pub changed_after_save_examples: Vec<String>,
    /// An ACE-BUG, kept: last states whose Location orientation is NaN, from
    /// `Creature.Rotate` on the partner's exact spot (the shard reads it back as the identity);
    /// counted, not failed, since ACE does the same.
    pub nan_orientation: u64,
    /// Re-entries the physics placed off the spot the character left (crowding).
    pub reload_displaced: u64,
    /// Re-entries the physics could not place at all: `WorldManager.DoPlayerEnterWorld` sends the
    /// character to its Sanctuary (ACE's relocation), everything else as it left.
    pub reload_relocated: u64,
    /// Re-entries whose vitae, fully worked off (at 1.0) just before the log-off, was dispelled by
    /// the login's `EnchantmentRegistry` (ACE's own; the delayed `RemoveVitae` of
    /// `Player.UpdateXpVitae` never ran on the player leaving the world).
    pub reload_vitae_dispelled: u64,
    /// Log-offs whose offline moment the driver missed (the object was already gone).
    pub snapshot_missed: u64,
    /// Releases whose player changed after the driver's last snapshot (in the last ticks
    /// before the release); its last state is then the `OfflinePlayer`'s biota.
    pub changed_after_last_snapshot: u64,
    /// The shard held possessions the world did not at log-off (a destroyed item never removed).
    pub shard_orphans: u64,
    /// A guid seen again under another identity whose old holder the shard no longer has (ACE's
    /// normal recycle, after `recycleTime`).
    pub guid_recycles: u64,
    /// A guid in the world under a new identity while its old holder is still held.
    pub guid_reuse_while_held: u64,
    pub scans: u64,
    pub scanned_objects_max: usize,
    pub failures: Vec<String>,
}

impl Checks {
    fn fail(&mut self, what: String) {
        if self.failures.len() < EXAMPLES {
            self.failures.push(what);
        }
    }

    /// Whether a snapshot is pending for `player` (taken, not yet compared after re-entry).
    #[must_use]
    pub fn has_snapshot(&self, player: u32) -> bool {
        self.snapshots.contains_key(&player)
    }

    /// The character is offline and still in the world (the session holds it for the log-off):
    /// snapshots it and its possessions. Called again each step until it leaves the world, so the
    /// snapshot is its last state there (a death under way at log-off still finishes).
    pub fn track(&mut self, w: &World, player: u32) {
        let g = ObjectGuid::new(player);
        if !w.objects.contains(g) {
            return;
        }
        let mut ids = vec![g];
        ids.extend(possessions(w, g));
        let biotas: BTreeMap<u32, BTreeSet<String>> = ids
            .iter()
            .filter_map(|id| w.objects.get(*id).map(|o| (id.full(), canon(&o.biota))))
            .collect();
        let player_at_switch = match self.snapshots.get(&player) {
            Some(s) => s.player_at_switch.clone(),
            None => biotas[&player].clone(),
        };
        self.snapshots.insert(
            player,
            Snapshot {
                biotas,
                player_at_switch,
            },
        );
    }

    /// The character (and its possessions) left the world: the last snapshot must be what the
    /// shard holds. Its possessions are now held by the shard only.
    ///
    /// The player's last state is the biota it left the store with, which its `OfflinePlayer` took
    /// at the release (V196; ACE's `OfflinePlayer` shares the `Player`'s biota). The driver's
    /// snapshots come once per bot step, so a blow or a death that lands in the last ticks before
    /// the release (a strike queued before the log-off still resolves, V219) is after the last
    /// snapshot, so the player's last state is taken from the `OfflinePlayer` instead.
    pub fn on_released(&mut self, w: &World, player: u32) {
        let Some(mut s) = self.snapshots.get(&player).cloned() else {
            self.snapshot_missed += 1;
            return;
        };
        if let Some(offline) = w.player_manager.offline_players.get(&player) {
            let last = canon(&offline.biota);
            if s.biotas.get(&player) != Some(&last) {
                self.changed_after_last_snapshot += 1;
                s.biotas.insert(player, last);
                self.snapshots.insert(player, s.clone());
            }
        }
        let mut ok = true;
        for (id, world) in &s.biotas {
            let shard = w
                .shard
                .base_database()
                .get_biota(*id, false)
                .map(|b| canon(&BiotaConverter::convert_to_entity_biota(&b, false)));
            match shard {
                Some(sh) if &sh == world => {}
                // a vital that moved after the last save (a blow landing during the log-off): ACE
                // saves at log-off only, and would lose it too
                Some(sh) if *id == player && only_vitals_differ(world, &sh) => {
                    self.vitals_after_save += 1
                }
                // the shard is the character as `FinalizeLogout` saved it, and what differs
                // came after that save (ACE-BUG at `Player.FinalizeLogout`)
                Some(sh)
                    if *id == player
                        && (sh == s.player_at_switch || only_after_save_differs(world, &sh)) =>
                {
                    self.changed_after_save += 1;
                    if self.changed_after_save_examples.len() < 5 {
                        self.changed_after_save_examples.push(format!(
                            "{player:08X} after its log-off save: {}",
                            diff(&sh, world)
                        ));
                    }
                }
                Some(sh) if only_nan_orientation_differs(world, &sh) => self.nan_orientation += 1,
                Some(sh) => {
                    ok = false;
                    self.fail(format!(
                        "log-off save of {player:08X}: {id:08X} differs from the shard: {}",
                        diff(world, &sh)
                    ));
                }
                None => {
                    ok = false;
                    self.fail(format!(
                        "log-off save of {player:08X}: {id:08X} is not in the shard"
                    ));
                }
            }
        }
        // the shard's possessions are the world's
        let possessed = w
            .shard
            .base_database()
            .get_possessed_biotas_in_parallel(player);
        let shard_ids: BTreeSet<u32> = possessed
            .inventory
            .iter()
            .chain(possessed.wielded_items.iter())
            .map(|b| b.id)
            .collect();
        let world_ids: BTreeSet<u32> = s.biotas.keys().copied().filter(|i| *i != player).collect();
        let orphans: Vec<u32> = shard_ids.difference(&world_ids).copied().collect();
        if !orphans.is_empty() {
            self.shard_orphans += orphans.len() as u64;
            self.fail(format!("log-off of {player:08X}: the shard holds possessions the world did not: {orphans:08X?}"));
        }
        if ok {
            self.saved_ok += 1;
        } else {
            self.saved_mismatch += 1;
        }
        for (id, lines) in s.biotas.iter().filter(|(i, _)| **i != player) {
            let wcid = lines
                .iter()
                .next()
                .and_then(|l| l.split(' ').nth(3))
                .and_then(|w| w.parse().ok())
                .unwrap_or(0);
            self.held_offline.insert(*id, (wcid, player));
        }
    }

    /// The character is entering again: its possessions leave the held set.
    pub fn on_entering(&mut self, player: u32) {
        self.held_offline.retain(|_, (_, owner)| *owner != player);
    }

    /// The character is back in the world: compare with its snapshot.
    pub fn on_entered(&mut self, w: &World, player: u32) {
        let Some(s) = self.snapshots.remove(&player) else {
            return;
        };
        let g = ObjectGuid::new(player);
        let Some(p) = w.objects.get(g) else {
            self.reload_mismatch += 1;
            self.fail(format!("re-entry of {player:08X}: not in the world"));
            return;
        };
        let mut ok = true;
        let back_ids: BTreeSet<u32> = possessions(w, g).iter().map(|g| g.full()).collect();
        let before_ids: BTreeSet<u32> = s.biotas.keys().copied().filter(|i| *i != player).collect();
        if back_ids != before_ids {
            ok = false;
            self.fail(format!("re-entry of {player:08X}: possessions {before_ids:08X?} came back as {back_ids:08X?}"));
        }
        // The player's Location: the same landblock, within 10 m. Entering puts the character back
        // through physics, which settles it on the floor and places it clear of whoever stands on
        // its spot (the bots crowd the Academy's entrance hall, and a crowded spot sends it to the
        // same free place each time).
        let location = |l: &String| l.starts_with("properties_position PositionType::Location");
        let is = p
            .location()
            .map(|p| (p.cell(), p.position_x, p.position_y, p.position_z));
        let near = |line: Option<&String>| match (line.and_then(|l| position_of(l)), is) {
            (Some(a), Some(b)) => {
                a.0 >> 16 == b.0 >> 16
                    && empyrean_common::math::hypotf(a.1 - b.1, a.2 - b.2) <= 10.0
                    && (a.3 - b.3).abs() <= 1.0
            }
            (a, b) => a.is_none() && b.is_none(),
        };
        // vitals keep moving once it is back (a waiting monster's blow, the respawn's 75%)
        let strip = |set: &BTreeSet<String>| -> BTreeSet<String> {
            without_volatile(set)
                .into_iter()
                .filter(|l| !location(l))
                .map(|l| {
                    if l.starts_with("properties_attribute_2nd") {
                        strip_field(&l, "current_level: ")
                    } else {
                        l
                    }
                })
                .collect()
        };
        let me = strip(&canon(&p.biota));
        let mut last = strip(&s.biotas[&player]);
        // a vitae worked off to 1.0 just before the log-off (its removal, 2 s after the XP, never
        // ran: the player had left the world) is dispelled by the login's EnchantmentRegistry, as
        // in ACE
        let vitae = |l: &String| {
            l.starts_with("properties_enchantment_registry") && l.contains("SpellCategory::Vitae")
        };
        if !me.iter().any(vitae) && last.iter().any(|l| spent_vitae(l)) {
            last.retain(|l| !spent_vitae(l));
            self.reload_vitae_dispelled += 1;
        }
        let last_near = near(s.biotas[&player].iter().find(|l| location(l)));
        let displaced = s.biotas[&player]
            .iter()
            .find(|l| location(l))
            .and_then(|l| position_of(l))
            .zip(is)
            .is_some_and(|(a, b)| empyrean_common::math::hypotf(a.1 - b.1, a.2 - b.2) > 1e-3);
        if displaced {
            self.reload_displaced += 1;
        }
        // placement failed where it left (a crowded spot): ACE relocates it to its Sanctuary
        let at_sanctuary = p.sanctuary().zip(p.location()).is_some_and(|(a, b)| {
            a.cell() == b.cell()
                && empyrean_common::math::hypotf(
                    a.position_x - b.position_x,
                    a.position_y - b.position_y,
                ) <= 1.0
        });
        if me == last && !last_near && at_sanctuary {
            self.reload_relocated += 1;
        } else if me != last || !last_near {
            // the known stale copy: back as it was when it went offline, not as it left the world
            let switch = strip(&s.player_at_switch);
            if me == switch && near(s.player_at_switch.iter().find(|l| location(l))) {
                // the stale offline biota: a failure
                ok = false;
                self.reload_stale_offline_biota += 1;
                if self.stale_examples.len() < 5 {
                    self.stale_examples.push(format!("re-entry of {player:08X}: back as at the offline switch, not as it left the world: {}", diff(&last, &me)));
                }
            } else {
                ok = false;
                if last_near {
                    self.fail(format!("re-entry of {player:08X}: the player differs from its last state: {}; from its state at the offline switch: {}", diff(&last, &me), diff(&switch, &me)));
                } else {
                    self.fail(format!(
                        "re-entry of {player:08X}: left at {:?}, back at {is:?}",
                        s.biotas[&player]
                            .iter()
                            .find(|l| location(l))
                            .and_then(|l| position_of(l))
                    ));
                }
            }
        }
        for id in before_ids.intersection(&back_ids) {
            let Some(o) = w.objects.get(ObjectGuid::new(*id)) else {
                continue;
            };
            let mut now = canon(&o.biota);
            let mut then = s.biotas[id].clone();
            // a wielded item's Location is the wielder's, set again at login (`Creature.TrySetChild`)
            if o.wielder_id() == Some(player) {
                now.retain(|l| !location(l));
                then.retain(|l| !location(l));
            }
            if now != then {
                ok = false;
                self.fail(format!(
                    "re-entry of {player:08X}: possession {id:08X} differs: {}",
                    diff(&then, &now)
                ));
            }
        }
        if ok {
            self.reload_ok += 1;
        } else {
            self.reload_mismatch += 1;
        }
    }

    /// Every possession of a character that is away must stay out of the world (the held case of
    /// guid reuse).
    pub fn check_held(&mut self, w: &World) {
        let mut bad = Vec::new();
        for (id, (wcid, owner)) in &self.held_offline {
            if let Some(o) = w.objects.get(ObjectGuid::new(*id)) {
                bad.push(format!("guid {id:08X} (weenie {wcid}, held by logged-off {owner:08X}) is in the world as weenie {}", o.biota.weenie_class_id));
            }
        }
        for b in bad {
            self.guid_reuse_while_held += 1;
            self.fail(b);
        }
    }

    /// Walks every object the loaded landblocks hold (containers' contents included) and records
    /// each guid's identity; a guid under a new identity is a recycle, or a reuse while held if the
    /// shard still holds the old one. Returns how many objects it saw.
    pub fn scan(&mut self, w: &World) -> usize {
        self.scans += 1;
        let mut seen = HashSet::new();
        let mut stack = Vec::new();
        for lb in landblock_manager::get_loaded_landblocks(w) {
            if let Some(l) = w.landblock_manager.landblocks.get(lb) {
                stack.extend(l.get_all_world_objects_for_diagnostics());
            }
        }
        while let Some(g) = stack.pop() {
            if !seen.insert(g.full()) {
                continue;
            }
            let Some(o) = w.objects.get(g) else { continue };
            if o.is_container() {
                stack.extend(container::inventory_values(w, g));
            }
            if o.is_creature() {
                stack.extend(creature_equipment::equipped_objects_values(w, g));
            }
            let id = (
                o.biota.weenie_class_id,
                o.get_property(PropertyInt::CreationTimestamp),
            );
            if let Some(old) = self.identities.insert(g.full(), id) {
                if old != id {
                    let held = w
                        .shard
                        .base_database()
                        .get_biota(g.full(), false)
                        .is_some_and(|b| {
                            let b = BiotaConverter::convert_to_entity_biota(&b, false);
                            (
                                b.weenie_class_id,
                                b.get_property(PropertyInt::CreationTimestamp),
                            ) == old
                        });
                    if held {
                        self.guid_reuse_while_held += 1;
                        self.fail(format!(
                            "guid {:08X} reused as weenie {} while the shard holds it as weenie {}",
                            g.full(),
                            id.0,
                            old.0
                        ));
                    } else {
                        self.guid_recycles += 1;
                    }
                }
            }
        }
        self.scanned_objects_max = self.scanned_objects_max.max(seen.len());
        seen.len()
    }
}
