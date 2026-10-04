//! The game-object half of the object triple, plus `ITEM_TYPE`, `ITEM_USEABLE`, the
//! `_bitfield` accessors and the property→`PublicWeenieDesc` mirror.
//!
//! The `PublicWeenieDesc` wire form itself belongs to the protocol crate
//! ([`dereth_protocol::types::PublicWeenieDesc`]); this module owns the *behaviour* hung off it.
//!
//! The unpack order is not the header-bit order. The protocol crate reproduces that; nothing here
//! may re-derive a field order from flags.
//!
//! The pure rules of this module live in [`dereth_rules::weenie`]; they are re-exported
//! here, so every `dereth_client_model::weenie::item_type` path resolves. So are the
//! `bitfield` bits, and the property→`PublicWeenieDesc` mirror's id→field table is
//! [`dereth_rules::pwd_mirror`], which [`mirror_stat_update`] reads.

use crate::qualities::{PropertySequenceGate, Qualities, StatKey, StatType, StatValue};
use dereth_primitives::ObjectId;
use dereth_protocol::types::PublicWeenieDesc;
pub use dereth_rules::weenie::{bitfield, item_type};

/// `ITEM_USEABLE` (`pwd._useability`): a source half in the low 16 bits and a target half in the
/// high 16.
pub mod item_useable {
    /// `USEABLE_UNDEF` — no `_useability` at all, which
    /// answers **true** for: the predicate is `~bitfield & 1`, so the absence of a useability is
    /// not a refusal. See the item-use eligibility check.
    pub const UNDEF: u32 = 0x0000_0000;
    /// `USEABLE_NO` — **bit 0**, the one value that makes the usable predicate false.
    ///
    /// Not `0x0000_0000`: that is the value of `USEABLE_UNDEF`, and a caller that set
    /// `_useability` to it would get an object the usable predicate answers *yes* for, so every
    /// not-useable arm would take the use-request branch instead.
    pub const NO: u32 = 0x0000_0001;
    /// `USEABLE_SELF` — the object may be used on itself; the last bit of both least-limited
    /// ladders.
    pub const SELF: u32 = 0x0000_0002;
    /// `USEABLE_WIELDED` — the one source bit names by itself,
    /// in its refusal of an unwielded item whose least-limited source use includes it.
    /// It is the fourth bit tests, and its position
    /// in that ladder — remote `0x20`, viewed `0x10`, contained `0x08`, wielded `0x04` — is what
    /// fixes the value.
    pub const WIELDED: u32 = 0x0000_0004;
    /// `USEABLE_CONTAINED` — usable from a container.
    pub const CONTAINED: u32 = 0x0000_0008;
    /// `USEABLE_VIEWED` — usable while viewed.
    pub const VIEWED: u32 = 0x0000_0010;
    /// `USEABLE_REMOTE` — usable at a distance.
    pub const REMOTE: u32 = 0x0000_0020;
    /// `USEABLE_OBJSELF` — the target ladder's final fallback, which the source ladder lacks.
    pub const OBJSELF: u32 = 0x0000_0080;
    /// The low half — where the *used* object must be.
    pub const SOURCE_MASK: u32 = 0x0000_FFFF;
    /// The high half — what an `Inventory_UseWithTargetEvent` may target.
    pub const TARGET_MASK: u32 = 0xFFFF_0000;
}

/// Current object-position state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u32)]
pub enum PositionState {
    #[default]
    In3dView = 0,
    Wielded = 1,
    InContainer = 2,
    BeingRemoved = 3,
}

/// Requested form of an object's display name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NameType {
    Singular,
    Plural,
    /// Plural iff the stack size is above 1.
    Appropriate,
}

/// Append `"es"` after `s` or `x`, else `"s"`;
/// fall back to the literal `"items"` when there is no name at all.
#[must_use]
pub fn plural_of(singular: &str) -> String {
    if singular.is_empty() {
        return "items".to_string();
    }
    match singular.as_bytes()[singular.len() - 1] {
        b's' | b'x' => format!("{singular}es"),
        _ => format!("{singular}s"),
    }
}

/// The client's distinct synthesized plural: only a final
/// lowercase `s` takes `es`; every other byte takes `s`, and an empty name stays empty.
fn object_plural_of(singular: &str) -> String {
    if singular.is_empty() {
        return String::new();
    }
    if singular.as_bytes()[singular.len() - 1] == b's' {
        format!("{singular}es")
    } else {
        format!("{singular}s")
    }
}

/// The game half of the object triple.
///
/// The physics half belongs to the physics side and is referenced only by the `has_phys_obj`
/// flag here — this crate holds a handle, never a physics object.
#[derive(Debug, Clone, Default)]
pub struct Weenie {
    pub id: ObjectId,
    /// The public weenie description.
    pub pwd: PublicWeenieDesc,
    /// The object has been declared valid.
    pub valid: bool,
    /// `awaiting_authentication`: the object-description request's "already asked" latch.
    ///
    /// The object-description request transition is the only thing in the client besides the
    /// constructor that writes it: it stores 0 or 1 immediately after sending the forced
    /// object-description request. It has no log-on role, and step 5's flag is this same field —
    /// the client has one, not two.
    pub awaiting_authentication: bool,
    /// The server has said "delete"; destruction is deferred.
    pub marked_for_deletion: bool,
    /// Moved after being marked, so the deletion path must still run the removal.
    pub moved_while_marked_for_deletion: bool,
    /// Re-entrancy guard around the removal and the server-says-remove path.
    pub being_removed: bool,
    /// Where the object currently is: in the 3D view, wielded, in a container, or being removed.
    pub current_state: PositionState,
    /// Whether this is the client's one selected object; changing the selection clears the old
    /// object's flag and sets the new one's.
    pub selected: bool,
    /// A UI operation is awaiting a server answer; the icon is drawn ghosted.
    pub waiting: bool,
    /// The vendor panel's mark for an item placed on the sell list; a change refreshes the item's
    /// icons.
    pub sell_state: u32,
    /// The mark an item carries while it is committed to a secure-trade, salvage or
    /// housing-payment panel (1 while committed, 0 when taken back).
    pub trade_state: u32,
    /// The toolbar shortcut slot the object sits in; `-1` when it has none.
    pub shortcut_num: i32,
    /// Whether the object's toolbar shortcut is drawn ghosted; set together with
    /// [`Self::shortcut_num`].
    pub shortcut_ghosted: bool,
    /// Captured just before a removal so the UI can undo.
    pub pre_remove_container: ObjectId,
    pub pre_remove_wielder: ObjectId,
    pub pre_remove_location: u32,
    pub pre_remove_place: u32,
    /// Whether the object has a physics presence. Contained items legitimately have none.
    pub has_phys_obj: bool,
    /// True while the physics object is in no cell. The deletion path's conditional cascade
    /// needs exactly this and nothing more of physics.
    pub phys_has_cell: bool,
    /// Allocated lazily by [`Self::setup_stamper`].
    pub stamper: Option<PropertySequenceGate>,
    /// `PlayerDesc`, **player only**. Non-player objects keep all their game state in
    /// `pwd` plus a transient `AppraisalProfile`.
    pub qualities: Option<Qualities>,
    /// The `PhysicsDesc` instance timestamp this descriptor arrived with.
    /// The instance gate runs before anything else in every object handler.
    pub instance_seq: u16,
    /// The house-restriction timestamp, a byte kept outside the stamper table.
    pub house_restriction_ts: Option<u8>,
}

impl Weenie {
    #[must_use]
    pub fn new(id: ObjectId) -> Self {
        Self {
            id,
            ..Self::default()
        }
    }

    /// Whether this is a player.
    #[must_use]
    pub fn is_player(&self) -> bool {
        self.pwd.bitfield & bitfield::PLAYER != 0
    }

    /// The object's item type.
    #[must_use]
    pub fn inq_type(&self) -> u32 {
        self.pwd.obj_type
    }

    /// Whether this is a creature.
    #[must_use]
    pub fn is_creature(&self) -> bool {
        self.pwd.obj_type & item_type::CREATURE != 0
    }

    /// Whether any player-killer bit is set.
    #[must_use]
    pub fn is_player_killer(&self) -> bool {
        self.pwd.bitfield & bitfield::PLAYER_KILLER_ANY != 0
    }

    /// Whether the player-killer bit is set.
    #[must_use]
    pub fn is_pk(&self) -> bool {
        self.pwd.bitfield & bitfield::PLAYER_KILLER != 0
    }

    /// Whether the PK-lite bit is set.
    #[must_use]
    pub fn is_pk_lite(&self) -> bool {
        self.pwd.bitfield & bitfield::PK_LITE != 0
    }

    /// Whether the impenetrable bit is set.
    #[must_use]
    pub fn is_impenetrable(&self) -> bool {
        self.pwd.bitfield & bitfield::IMPENETRABLE != 0
    }

    /// Whether the object can bypass move restrictions.
    ///
    /// The check reads the public descriptor's bitfield and requires both the administrator
    /// and cell-barrier-immunity bits.
    ///
    /// Both bits, not either. ACE's `WeenieObject::CanBypassMoveRestrictions` comments the admin
    /// half out and answers on `IgnoreHouseBarriers` (`PropertyBool 25`, which is the property
    /// behind `ObjectDescriptionFlag.ImmuneCellRestrictions`) alone; the retail client is stricter
    /// and that is what is transcribed. The one consumer checks cell-entry restrictions.
    #[must_use]
    pub fn can_bypass_move_restrictions(&self) -> bool {
        const BOTH: u32 = bitfield::ADMIN | bitfield::CELL_BARRIER_IMMUNE;
        self.pwd.bitfield & BOTH == BOTH
    }

    /// Whether this is a corpse.
    #[must_use]
    pub fn is_corpse(&self) -> bool {
        self.pwd.bitfield & bitfield::CORPSE != 0
    }

    /// Whether this is a hook — **both** fields must be non-zero.
    #[must_use]
    pub fn is_hook(&self) -> bool {
        self.pwd.hook_type.unwrap_or(0) != 0 && self.pwd.hook_item_types.unwrap_or(0) != 0
    }

    /// Return whether the public object description marks this object talkable.
    #[must_use]
    pub fn is_talkable(&self) -> bool {
        self.pwd.obj_type & item_type::CREATURE != 0
    }

    /// Return whether this object is a container.
    ///
    /// ```text
    /// (bitfield & 0x800000) != 0 || items_capacity != 0 || containers_capacity != 0
    /// ```
    ///
    /// This function does not look at `ITEM_TYPE` (`_itemType`) at all -- it is the same
    /// three-way capacity test as [`Self::goes_in_containers_list`].
    /// `place_in_container_item_legal` pairs it with `containers_capacity != 0`, which the
    /// predicate implies, so that caller would behave identically under an item-type reading.
    ///
    /// The using-item tail is the caller where it matters. It asks the container test
    /// about a **corpse**, and a corpse in the recorded captures carries a capacity and
    /// **not** `TYPE_CONTAINER` -- so with an `_itemType` reading, `attempt_set_ground_object`
    /// would never be reached for any of the twelve corpses the recordings contain and the ground
    /// panel could not open.
    #[must_use]
    pub fn is_container(&self) -> bool {
        self.pwd.bitfield & crate::inventory::use_object::use_bitfield::REQUIRES_PACK_SLOT != 0
            || self.pwd.items_capacity.unwrap_or(0) != 0
            || self.pwd.containers_capacity.unwrap_or(0) != 0
    }

    /// Which of a container's two ordered lists this object belongs in.
    ///
    /// All content insertion, removal, and server-containment operations choose the side-pack
    /// list over the items list with the same three-way test, and it is **not**
    /// [`Self::is_container`]:
    ///
    /// ```text
    /// (pwd.bitfield & 0x800000) || pwd.items_capacity != 0 || pwd.containers_capacity != 0
    /// ```
    ///
    /// `0x800000` is the client's "needs a pack slot" bit; the same triple is the gate named in
    /// [`crate::inventory::use_object::use_bitfield`]. A foci pouch has capacity and lands in the
    /// side-pack list whatever `ITEM_TYPE` says.
    ///
    /// The client's container test asks *this* question, with this arithmetic, and the two are the
    /// same predicate under two names. The name is kept because the call sites read better with
    /// it -- content insertion is choosing a list, not classifying an object -- but it delegates,
    /// so the two cannot drift apart.
    #[must_use]
    pub fn goes_in_containers_list(&self) -> bool {
        self.is_container()
    }

    /// Return whether this object is a coin stack.
    #[must_use]
    pub fn is_coinstack(&self) -> bool {
        self.pwd.obj_type & item_type::MONEY != 0
    }

    /// Determine the object's position state.
    pub fn determine_position_state(&mut self) {
        self.current_state = if self.being_removed {
            PositionState::BeingRemoved
        } else if self.pwd.container_id.unwrap_or_default().0 != 0 {
            PositionState::InContainer
        } else if self.pwd.location.unwrap_or(0) != 0 {
            PositionState::Wielded
        } else {
            PositionState::In3dView
        };
    }

    /// The object-name formatting suffix branch — the
    /// singular/plural pick, without the material prefix.
    ///
    /// [`Self::display_name`] is the whole function. This half is kept separate because the
    /// prefix needs the `MaterialType` string table out of the dats and this crate has no dat
    /// access — the same split [`crate::inventory::salvage::materials_salvaged_string`] takes.
    ///
    /// **Every caller of this method in `dereth-client-model` therefore shows the *unprefixed* name**, and
    /// that is a declared gap rather than a claim: the client's material-aware name formatter
    /// would make the use path's "Using the %s" and the rest all read
    /// "Bronze Salvage (100)" where ours reads "Salvage (100)". The gap is only visible on an
    /// object whose `_material_type` is set *and* whose `_name` does not already contain the
    /// material name, which outside salvage bags is nothing the shard sends — see
    /// [`Self::display_name`]'s note on why the prefix is a no-op for ordinary loot.
    #[must_use]
    pub fn object_name(&self, kind: NameType) -> String {
        let plural = match kind {
            NameType::Singular => false,
            NameType::Plural => true,
            NameType::Appropriate => self.pwd.stack_size.unwrap_or(0) > 1,
        };
        if !plural {
            return self.pwd.name.clone();
        }
        match &self.pwd.plural_name {
            Some(p) if !p.is_empty() => p.clone(),
            _ => object_plural_of(&self.pwd.name),
        }
    }

    /// Behavior: **all of it** — the name a player reads.
    /// Without the material prefix, a salvage bag displays only `Salvage (100)`.
    ///
    /// [`Self::object_name`] is the first two thirds and this is the rest:
    ///
    /// ```text
    ///   material = "";                                      ; initially empty
    ///   mat = w.pwd._material_type;
    ///   if ((int)mat <= 0) skip material prefix;             ; SIGNED, and 0 is "none"
    ///           base = name;
    ///   look up the material name into material;            ; return value IGNORED
    ///   n = base.replace(material, "");
    ///   if (n) base.trim(true, true, whitespace);
    ///   sprintf(out, "%s %s", material, base);
    ///           name = out;
    ///   if (name[0] == '+' && (w.pwd._bitfield & 0x40)) return name + 1;
    ///           return name;
    /// ```
    ///
    /// # It is keyed on nothing
    ///
    /// What makes retail show *"Bronze Salvage"* but not *"Bronze Sword"*? Nothing
    /// does: `_material_type > 0` is the only gate, so **every** object with a
    /// material gets the prefix — armour and weapons included. The reason a sword's name does not
    /// double up is that the material name is **removed from the base name first** and the
    /// result trimmed, so a shard-supplied *"Bronze Long Sword"* becomes `"Bronze"` + `" "` +
    /// `"Long Sword"` — the same string back. The step is an idempotence guard, not a filter.
    /// A salvage bag is the case where it bites, because ACE names the bag
    /// `$"Salvage ({salvageBag.Structure})"` (`Player_Crafting.cs`, `TryAddSalvage`) and sets
    /// `MaterialType` beside it, so the material appears in exactly one of the two places.
    ///
    /// **So the `(N)` is not the structure read by the client**: it is part of the name the shard
    /// sent. The client contributes only the material word and the space.
    ///
    /// # Two arms that look like defects and are retail's
    ///
    /// * the client's bool is **dropped**, and the material string starts as
    ///   the null string, so a material id the dat has no row for gives `" Salvage (100)"` with a
    ///   leading space. The salvage-material formatter answers the same miss with `"Unknown"` —
    ///   the two miss arms are *different*, and conflating them would be wrong in
    ///   one of the two places. `material_name` is `None` for that case here.
    /// * The `'+'` strip is guarded on `_bitfield & 0x40`
    ///   ([`bitfield::HIDDEN_ADMIN`]) and runs on every path, prefix or not.
    #[must_use]
    pub fn display_name(&self, kind: NameType, material_name: Option<&str>) -> String {
        let base = self.object_name(kind);
        // The material gate is signed: `_material_type` is read as an `int`.
        let composed = if self.pwd.material_type.unwrap_or(0) as i32 > 0 {
            material_prefixed_name(&base, material_name.unwrap_or(""))
        } else {
            base
        };
        strip_hidden_admin_plus(&composed, self.pwd.bitfield).to_owned()
    }
}

/// Compose the material prefix on its own.
///
/// See [`Weenie::display_name`] for the retail logic and for why the `replace` is here.
#[must_use]
pub fn material_prefixed_name(base: &str, material: &str) -> String {
    // Replacing an empty needle changes nothing and returns 0 in the client; Rust's
    // `str::replace("")` splices the replacement between every char, so the empty case is taken
    // out explicitly rather than left to differ silently.
    let stripped = if material.is_empty() {
        base.to_owned()
    } else {
        let replaced = base.replace(material, "");
        // The trim runs only when `replace` reported at least one substitution.
        if replaced.len() == base.len() {
            replaced
        } else {
            replaced.trim().to_owned()
        }
    };
    format!("{material} {stripped}")
}

/// Behavior: an admin-hidden object's name loses a leading
/// `'+'`, and nothing else does.
#[must_use]
pub fn strip_hidden_admin_plus(name: &str, pwd_bitfield: u32) -> &str {
    if pwd_bitfield & bitfield::HIDDEN_ADMIN == 0 {
        return name;
    }
    name.strip_prefix('+').unwrap_or(name)
}

impl Weenie {
    /// Behavior: allocated on first use, so an object that never receives a
    /// property update carries no per-key state.
    pub fn setup_stamper(&mut self) -> &mut PropertySequenceGate {
        self.stamper.get_or_insert_with(PropertySequenceGate::new)
    }

    /// Set or clear the waiting state. It **only** ghosts the icon: nothing is predicted.
    pub fn set_waiting_state(&mut self, waiting: bool) {
        self.waiting = waiting;
    }

    /// Behavior: int property 134 onto three bits.
    pub fn set_player_killer_status(&mut self, value: i32) {
        dereth_rules::pwd_mirror::set_player_killer_status(&mut self.pwd.bitfield, value);
    }
}

/// What a mirrored property update asked the UI to redo.
///
/// An icon update composites base, underlay, and overlay, then notifies the item holder that
/// attributes changed with reason 2; the icon cache itself belongs to the UI.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct MirrorEffect {
    /// The `PublicWeenieDesc` changed.
    pub changed: bool,
    /// One of the five icon inputs changed, so the icon cache would re-render.
    pub icon_changed: bool,
    /// The containment fields changed and a full server move-item notice is due.
    pub move_item: bool,
}

/// The integer-stat update and its three sibling property handlers — the property→PWD
/// mirror, applied to **every** object whether or not it has a `PlayerDesc`.
///
/// `guarded` is the two-part condition
/// the client applies to `CurrentWieldedLocation`, `Container` and `Wielder`: skip when this object
/// is the subject of the in-flight inventory request, or is owned by the player.
///
/// The ids name numeric literals: the client references the numbers, not
/// the names.
pub fn mirror_stat_update(
    w: &mut Weenie,
    key: StatKey,
    v: &StatValue,
    guarded: bool,
) -> MirrorEffect {
    use dereth_rules::pwd_mirror::{
        apply_pwd_field, pwd_mirror_field, MirrorStat, MirrorValue, PwdField,
    };
    let mut e = MirrorEffect::default();
    let (stat, value) = match (key.stat_type(), v) {
        (StatType::Int, StatValue::Int(n)) => (MirrorStat::Int, MirrorValue::Int(*n)),
        (StatType::Did, StatValue::Did(d)) => (MirrorStat::DataId, MirrorValue::DataId(d.0)),
        (StatType::Iid, StatValue::Iid(id)) => {
            (MirrorStat::InstanceId, MirrorValue::InstanceId(*id))
        }
        (StatType::Bool, StatValue::Bool(b)) => (MirrorStat::Bool, MirrorValue::Bool(*b)),
        _ => return e,
    };
    let Some(field) = pwd_mirror_field(stat, key.property()) else {
        return e;
    };
    // CurrentWieldedLocation, Container and Wielder are guarded: the update is skipped.
    if guarded
        && matches!(
            field,
            PwdField::Location | PwdField::Container | PwdField::Wielder
        )
    {
        return e;
    }
    // ItemType re-renders the icon only for a valid object; the effects word and the three icon
    // ids always do. Container and Wielder are followed by a full move-item notice.
    let icon_changed = match field {
        PwdField::ObjType => w.valid,
        PwdField::Effects | PwdField::IconId | PwdField::IconOverlay | PwdField::IconUnderlay => {
            true
        }
        _ => false,
    };
    e.changed = apply_pwd_field(&mut w.pwd, field, value);
    e.icon_changed = icon_changed;
    e.move_item = matches!(field, PwdField::Container | PwdField::Wielder);
    e
}

#[cfg(test)]
mod tests {
    use super::*;
    use dereth_primitives::DataId;

    /// Oracle: §3's `SetPlayerKillerStatus` table.
    #[test]
    fn player_killer_status_maps_onto_three_bits() {
        let mut w = Weenie::new(ObjectId(1));
        w.pwd.bitfield = 0xFFFF_FFFF;
        w.set_player_killer_status(4);
        assert_eq!(
            w.pwd.bitfield & bitfield::PLAYER_KILLER,
            bitfield::PLAYER_KILLER
        );
        assert_eq!(w.pwd.bitfield & bitfield::IMPENETRABLE, 0);
        assert_eq!(w.pwd.bitfield & bitfield::PK_LITE, 0);

        w.set_player_killer_status(0x40);
        assert_eq!(w.pwd.bitfield & bitfield::PK_LITE, bitfield::PK_LITE);
        assert_eq!(w.pwd.bitfield & bitfield::PLAYER_KILLER, 0);

        w.set_player_killer_status(0x20);
        assert_eq!(
            w.pwd.bitfield & bitfield::IMPENETRABLE,
            bitfield::IMPENETRABLE
        );
        assert_eq!(w.pwd.bitfield & bitfield::PK_LITE, 0);

        w.set_player_killer_status(7);
        assert_eq!(w.pwd.bitfield & bitfield::PLAYER_KILLER_ANY, 0);
        assert_eq!(w.pwd.bitfield & bitfield::IMPENETRABLE, 0);
    }

    /// Oracles: §8, and the distinct
    /// object-name suffix branch.
    #[test]
    fn plurals_follow_the_clients_two_rules() {
        assert_eq!(plural_of("Pyreal"), "Pyreals");
        assert_eq!(plural_of("Grass"), "Grasses");
        assert_eq!(plural_of("Lockpix"), "Lockpixes");
        assert_eq!(plural_of(""), "items");

        assert_eq!(object_plural_of("Pyreal"), "Pyreals");
        assert_eq!(object_plural_of("Grass"), "Grasses");
        assert_eq!(object_plural_of("Lockpix"), "Lockpixs");
        assert_eq!(
            object_plural_of("BOX"),
            "BOXs",
            "the byte comparison is case-sensitive"
        );
        assert_eq!(object_plural_of(""), "");
    }

    /// Oracle: §2's, which is an if/else chain in that order.
    #[test]
    fn position_state_follows_the_if_chain() {
        let mut w = Weenie::new(ObjectId(1));
        w.determine_position_state();
        assert_eq!(w.current_state, PositionState::In3dView);

        w.pwd.location = Some(0x0010_0000);
        w.determine_position_state();
        assert_eq!(w.current_state, PositionState::Wielded);

        // A container id wins over a location.
        w.pwd.container_id = Some(ObjectId(9));
        w.determine_position_state();
        assert_eq!(w.current_state, PositionState::InContainer);

        // `being_removed` wins over everything.
        w.being_removed = true;
        w.determine_position_state();
        assert_eq!(w.current_state, PositionState::BeingRemoved);
    }

    /// Oracle: §7's three mirror tables.
    #[test]
    fn the_mirror_writes_the_documented_pwd_fields() {
        let mut w = Weenie::new(ObjectId(1));
        w.valid = true;

        let e = mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Int, 1),
            &StatValue::Int(0x200),
            false,
        );
        assert_eq!(w.pwd.obj_type, 0x200);
        assert!(
            e.icon_changed,
            "ItemType re-renders the icon when the object is valid"
        );

        mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Int, 12),
            &StatValue::Int(37),
            false,
        );
        assert_eq!(w.pwd.stack_size, Some(37));

        mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Did, 8),
            &StatValue::Did(DataId(0x0600_1234)),
            false,
        );
        assert_eq!(w.pwd.icon_id, 0x0600_1234);

        // Locked (bool 3) sets *openable* to the inverse.
        w.pwd.bitfield = 0;
        mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Bool, 3),
            &StatValue::Bool(false),
            false,
        );
        assert_eq!(w.pwd.bitfield & bitfield::OPENABLE, bitfield::OPENABLE);
        mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Bool, 3),
            &StatValue::Bool(true),
            false,
        );
        assert_eq!(w.pwd.bitfield & bitfield::OPENABLE, 0);

        // The guard suppresses exactly the three guarded properties.
        let before = w.pwd.location;
        let e = mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Int, 10),
            &StatValue::Int(5),
            true,
        );
        assert!(!e.changed);
        assert_eq!(w.pwd.location, before);
        let e = mirror_stat_update(
            &mut w,
            StatKey::new(StatType::Int, 10),
            &StatValue::Int(5),
            false,
        );
        assert!(e.changed);
        assert_eq!(w.pwd.location, Some(5));
    }

    /// Oracle: §8's object-name lookup, whose appropriate-form arm keys off a stack size above 1.
    #[test]
    fn appropriate_name_pluralises_only_above_one() {
        let mut w = Weenie::new(ObjectId(1));
        w.pwd.name = "Pyreal".into();
        assert_eq!(w.object_name(NameType::Appropriate), "Pyreal");
        w.pwd.stack_size = Some(1);
        assert_eq!(w.object_name(NameType::Appropriate), "Pyreal");
        w.pwd.stack_size = Some(2);
        assert_eq!(w.object_name(NameType::Appropriate), "Pyreals");
        w.pwd.plural_name = Some("Pyreal Coins".into());
        assert_eq!(w.object_name(NameType::Appropriate), "Pyreal Coins");
        assert_eq!(w.object_name(NameType::Singular), "Pyreal");

        w.pwd.name = "Lockpix".into();
        w.pwd.plural_name = Some("Lockpicks".into());
        assert_eq!(w.object_name(NameType::Plural), "Lockpicks");
        w.pwd.plural_name = Some(String::new());
        assert_eq!(w.object_name(NameType::Plural), "Lockpixs");
    }

    /// The material prefix is idempotent and keyed on nothing but the material.
    #[test]
    fn the_material_prefix_is_idempotent_and_keyed_on_nothing_but_the_material() {
        // The salvage bag: ACE names it `Salvage (100)` and sets `MaterialType` beside it, so the
        // material word appears in exactly one of the two places and the prefix is what shows it.
        assert_eq!(
            material_prefixed_name("Salvage (100)", "Bronze"),
            "Bronze Salvage (100)"
        );
        // Ordinary loot: the shard's own name already has it, so composition removes and trims the
        // material before restoring the prefix; the string comes back unchanged.
        assert_eq!(
            material_prefixed_name("Bronze Long Sword", "Bronze"),
            "Bronze Long Sword"
        );
        // …including when it is not the first word.
        assert_eq!(
            material_prefixed_name("Sturdy Iron Key", "Iron"),
            "Iron Sturdy  Key"
        );
        // The material-name lookup's miss: its bool is dropped and `matName` is the null
        // string, so `"%s %s"` leaves a leading space. Retail's own answer, not a placeholder.
        assert_eq!(
            material_prefixed_name("Salvage (100)", ""),
            " Salvage (100)"
        );
    }

    /// Oracle: the client's signed material gate and hidden-admin `'+'` strip.
    #[test]
    fn display_name_gates_on_a_positive_material_and_strips_an_admin_plus() {
        let mut w = Weenie::new(ObjectId(1));
        w.pwd.name = "Salvage (100)".into();
        // No material: the base name, untouched — the prefix arm is skipped entirely, so a
        // material name supplied anyway cannot leak in.
        assert_eq!(
            w.display_name(NameType::Singular, Some("Bronze")),
            "Salvage (100)"
        );
        w.pwd.material_type = Some(0);
        assert_eq!(
            w.display_name(NameType::Singular, Some("Bronze")),
            "Salvage (100)"
        );
        w.pwd.material_type = Some(0x33);
        assert_eq!(
            w.display_name(NameType::Singular, Some("Ivory")),
            "Ivory Salvage (100)"
        );
        // the `'+'` goes only when `_bitfield & 0x40` is set.
        let mut a = Weenie::new(ObjectId(2));
        a.pwd.name = "+Lark".into();
        assert_eq!(a.display_name(NameType::Singular, None), "+Lark");
        a.pwd.bitfield |= bitfield::HIDDEN_ADMIN;
        assert_eq!(a.display_name(NameType::Singular, None), "Lark");
    }
}
