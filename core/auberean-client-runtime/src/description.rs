//! The qualities an object's description answers for: asked for one of these, the client reads
//! the description before the object's own qualities and its qualities record, and setting one
//! writes the description.
//!
//! - **Numbers**: the quantity, the value, the faction, the two player-killer flags, the physics
//!   type's halves, movement and placement ethereality, and the current and maximum durability;
//! - **objects**: the container, the wielder, the monarch, the originator, the claimant, the killer
//!   and the pet's summoner;
//! - **yes-or-no qualities**: four bits of the description's flag word, each with a mask bit of
//!   its own that says the description holds it;
//! - the **scale** (a float), the **icon** (a data id), and the **name** and **plural name**
//!   (string references).
//!
//! A quality the description holds is answered from it only while its mask bit is set; else the
//! object's own qualities and record are asked. Setting one sets its mask bit. A qualities record
//! carries a description of its own, asked the same way before the record's tables.
//!
//! The server's updates of a quality are set the same way: one the description holds goes to the
//! description, and the object's own qualities are left as they were.

use auberean_assets::script::ObjectDescription;
use auberean_client_model::Model;
use auberean_protocol::text::StringInfo;
use auberean_protocol::types::WeenieDesc;
use auberean_protocol::ServerMessage;

/// Qualities from here on are never the description's.
const FIRST_NOT_HELD: u32 = 0x1_0000;

/// A server's update of a quality, set in the object's description when the description holds the
/// quality (the player's own updates are the player's); whether it did. An update it does not
/// take is the model's, for the object's own qualities. An object quality set (the container, the
/// wielder) changes what holds the object.
pub fn take_update(model: &mut Model, m: &ServerMessage) -> bool {
    use ServerMessage as S;
    let player = model.player;
    let mut set = |id: Option<u64>, q: u32, f: &dyn Fn(&mut WeenieDesc) -> bool| {
        q < FIRST_NOT_HELD
            && id
                .and_then(|id| model.object_mut(id))
                .is_some_and(|o| f(&mut o.weenie))
    };
    match m {
        S::UpdateInt(u) => set(player, u.quality, &|w| set_int(w, u.quality, u.value)),
        S::UpdateIntVisual(u) => set(Some(u.object.id), u.quality, &|w| {
            set_int(w, u.quality, u.value)
        }),
        S::UpdateBool(u) => set(player, u.quality, &|w| set_bool(w, u.quality, u.value)),
        S::UpdateBoolVisual(u) => set(Some(u.object.id), u.quality, &|w| {
            set_bool(w, u.quality, u.value)
        }),
        S::UpdateFloat(u) => set(player, u.quality, &|w| set_float(w, u.quality, u.value)),
        S::UpdateFloatVisual(u) => set(Some(u.object.id), u.quality, &|w| {
            set_float(w, u.quality, u.value)
        }),
        S::UpdateDataId(u) => set(player, u.quality, &|w| set_data_id(w, u.quality, u.value)),
        S::UpdateDataIdVisual(u) => set(Some(u.object.id), u.quality, &|w| {
            set_data_id(w, u.quality, u.value)
        }),
        S::UpdateInstanceId(u) => set(player, u.quality, &|w| {
            set_instance_id(w, u.quality, u.value)
        }),
        S::UpdateInstanceIdVisual(u) => {
            let taken = set(Some(u.object.id), u.quality, &|w| {
                set_instance_id(w, u.quality, u.value)
            });
            if taken {
                if let Some(o) = model.object_mut(u.object.id) {
                    o.take_holders();
                }
            }
            taken
        }
        _ => false,
    }
}

/// The numbers: quality, mask bit.
const NUMBERS: [(u32, u32); 13] = [
    (0x106, 0x800),
    (3, 0x8000),
    (4, 0x1_0000),
    (5, 0x8_0000),
    (6, 0x10_0000),
    (7, 0x2_0000),
    (8, 0x4_0000),
    (0x26A, 0x1000_0000),
    (0x226, 0x20_0000),
    (0x227, 0x40_0000),
    (0x264, 0x1000),
    (0x26B, 0x800_0000),
    (2000, 0x2000),
];

/// The objects: quality, mask bit.
const OBJECTS: [(u32, u32); 7] = [
    (0x102, 0x20),
    (0x105, 0x40),
    (0x140, 0x4000_0000),
    (0x141, 0x200),
    (0x142, 0x2000_0000),
    (0x1C4, 0x100),
    (0x1F9, 0x400_0000),
];

/// The yes-or-no qualities: quality, mask bit, the flag word's bit.
const BOOLS: [(u32, u32, u32); 4] = [
    (0x270, 0x80_0000, 0x2),
    (10, 0x100_0000, 0x4),
    (0x117, 0x4000, 0x8),
    (0x2BE, 0x400, 0x1),
];

const SCALE: u32 = 0x12A;
const ICON: u32 = 0x101;
const NAME: u32 = 0x100;
const PLURAL_NAME: u32 = 0x101;

fn number_at(bit: u32) -> Option<usize> {
    WeenieDesc::NUMBER_BITS.iter().position(|b| *b == bit)
}

fn object_at(bit: u32) -> Option<usize> {
    WeenieDesc::OBJECT_BITS.iter().position(|b| *b == bit)
}

/// Whether the description holds a number quality.
// D2-12015
#[must_use]
pub fn holds_int(q: u32) -> bool {
    NUMBERS.iter().any(|(k, _)| *k == q)
}

/// A number quality, when the description has it.
// D2-12028
#[must_use]
pub fn int(w: &WeenieDesc, q: u32) -> Option<i32> {
    let (_, bit) = NUMBERS.iter().find(|(k, _)| *k == q)?;
    (w.mask & bit != 0)
        .then(|| number_at(*bit).map(|i| w.numbers[i]))
        .flatten()
}

/// Set a number quality the description holds; whether it holds it.
// D2-12030
pub fn set_int(w: &mut WeenieDesc, q: u32, v: i32) -> bool {
    let Some((_, bit)) = NUMBERS.iter().find(|(k, _)| *k == q) else {
        return false;
    };
    let Some(i) = number_at(*bit) else {
        return false;
    };
    w.mask |= bit;
    w.numbers[i] = v;
    true
}

/// Take a number quality out of the description (its mask bit cleared); whether it had it.
// D2-12026, D2-12027
pub fn remove_int(w: &mut WeenieDesc, q: u32) -> bool {
    let Some((_, bit)) = NUMBERS.iter().find(|(k, _)| *k == q) else {
        return false;
    };
    let had = w.mask & bit != 0;
    w.mask &= !bit;
    had
}

/// Whether the description holds an object quality.
// D2-12019
#[must_use]
pub fn holds_instance_id(q: u32) -> bool {
    OBJECTS.iter().any(|(k, _)| *k == q)
}

/// An object quality, when the description has it.
// D2-12055
#[must_use]
pub fn instance_id(w: &WeenieDesc, q: u32) -> Option<u64> {
    let (_, bit) = OBJECTS.iter().find(|(k, _)| *k == q)?;
    (w.mask & bit != 0)
        .then(|| object_at(*bit).map(|i| w.objects[i]))
        .flatten()
}

/// Set an object quality the description holds; whether it holds it.
// D2-12056
pub fn set_instance_id(w: &mut WeenieDesc, q: u32, v: u64) -> bool {
    let Some((_, bit)) = OBJECTS.iter().find(|(k, _)| *k == q) else {
        return false;
    };
    let Some(i) = object_at(*bit) else {
        return false;
    };
    w.mask |= bit;
    w.objects[i] = v;
    true
}

/// Whether the description holds a yes-or-no quality.
// D2-12016
#[must_use]
pub fn holds_bool(q: u32) -> bool {
    BOOLS.iter().any(|(k, _, _)| *k == q)
}

/// A qualities record's description as the description an object carries: the same mask, flag
/// word, numbers and objects. Its names are not carried here (their mask bits are cleared): a
/// record's names are read as the record holds them.
#[must_use]
pub fn of_record(d: &ObjectDescription) -> WeenieDesc {
    let numbers = [
        d.quantity,
        d.value,
        d.faction,
        d.pk_always_true,
        d.pk_always_false,
        d.physics_type_low,
        d.physics_type_high,
        d.movement_ethereal_low,
        d.movement_ethereal_high,
        d.placement_ethereal_low,
        d.placement_ethereal_high,
        d.durability,
        d.durability_max,
    ];
    let objects = [
        d.container,
        d.wielder,
        d.monarch,
        d.originator,
        d.claimant,
        d.killer,
        d.pet_summoner,
    ];
    WeenieDesc {
        mask: d.mask & !(WeenieDesc::NAME | WeenieDesc::PLURAL_NAME),
        package_id: d.package.unwrap_or(0),
        entity_did: d.entity.map_or(0, |e| e.0),
        bitfield: d.flags.unwrap_or(0),
        icon: d.icon.map_or(0, |i| i.0),
        objects: objects.map(|o| o.unwrap_or(0)),
        scale: d.scale.unwrap_or(0.0),
        numbers: numbers.map(|n| n.unwrap_or(0)),
        ..WeenieDesc::default()
    }
}

/// A yes-or-no quality, when the description has it: 1 or 0.
// D2-12037
#[must_use]
pub fn bool(w: &WeenieDesc, q: u32) -> Option<i32> {
    let (_, bit, flag) = BOOLS.iter().find(|(k, _, _)| *k == q)?;
    (w.mask & bit != 0).then_some(i32::from(w.bitfield & flag != 0))
}

/// Set a yes-or-no quality the description holds (turning it on also marks the flag word as
/// held); whether it holds it.
// D2-12038
pub fn set_bool(w: &mut WeenieDesc, q: u32, v: i32) -> bool {
    let Some((_, bit, flag)) = BOOLS.iter().find(|(k, _, _)| *k == q) else {
        return false;
    };
    w.mask |= bit;
    if v != 0 {
        w.mask |= WeenieDesc::BITFIELD;
        w.bitfield |= flag;
    } else {
        w.bitfield &= !flag;
    }
    true
}

/// Whether the description holds a float quality (the scale).
// D2-12017
#[must_use]
pub fn holds_float(q: u32) -> bool {
    q == SCALE
}

/// The scale, when the description has it.
// D2-12042
#[must_use]
pub fn float(w: &WeenieDesc, q: u32) -> Option<f32> {
    (q == SCALE && w.mask & WeenieDesc::SCALE != 0).then_some(w.scale)
}

/// Set the scale; whether the quality is the scale.
// D2-12043
pub fn set_float(w: &mut WeenieDesc, q: u32, v: f32) -> bool {
    if q != SCALE {
        return false;
    }
    w.mask |= WeenieDesc::SCALE;
    w.scale = v;
    true
}

/// Whether the description holds a data id quality (the icon).
// D2-12018
#[must_use]
pub fn holds_data_id(q: u32) -> bool {
    q == ICON
}

/// The icon, when the description has it.
// D2-12050
#[must_use]
pub fn data_id(w: &WeenieDesc, q: u32) -> Option<u32> {
    (q == ICON && w.mask & WeenieDesc::ICON != 0).then_some(w.icon)
}

/// Set the icon; whether the quality is the icon.
// D2-12051
pub fn set_data_id(w: &mut WeenieDesc, q: u32, v: u32) -> bool {
    if q != ICON {
        return false;
    }
    w.mask |= WeenieDesc::ICON;
    w.icon = v;
    true
}

/// The name or the plural name, when the description has it.
// D2-12063, D2-12020
#[must_use]
pub fn string_info(w: &WeenieDesc, q: u32) -> Option<&StringInfo> {
    match q {
        NAME if w.mask & WeenieDesc::NAME != 0 => Some(&w.name),
        PLURAL_NAME if w.mask & WeenieDesc::PLURAL_NAME != 0 => Some(&w.plural_name),
        _ => None,
    }
}

/// Set the name or the plural name; whether the quality is one of them.
// D2-12064
pub fn set_string_info(w: &mut WeenieDesc, q: u32, v: StringInfo) -> bool {
    match q {
        NAME => {
            w.mask |= WeenieDesc::NAME;
            w.name = v;
        }
        PLURAL_NAME => {
            w.mask |= WeenieDesc::PLURAL_NAME;
            w.plural_name = v;
        }
        _ => return false,
    }
    true
}

/// Take the name or the plural name out of the description (its mask bit cleared): its value,
/// when the description had it.
// D2-12061, D2-12062
pub fn remove_string_info(w: &mut WeenieDesc, q: u32) -> Option<StringInfo> {
    let had = string_info(w, q).cloned()?;
    w.mask &= !if q == NAME {
        WeenieDesc::NAME
    } else {
        WeenieDesc::PLURAL_NAME
    };
    Some(had)
}

/// Take a yes-or-no quality out of the description: its mask bit cleared, the flag word left as
/// it is; the value it had (1 or 0), when the description had it.
// D2-12035, D2-12036
pub fn remove_bool(w: &mut WeenieDesc, q: u32) -> Option<i32> {
    let had = bool(w, q)?;
    if let Some((_, bit, _)) = BOOLS.iter().find(|(k, _, _)| *k == q) {
        w.mask &= !bit;
    }
    Some(had)
}

/// Take the scale out of the description: the scale it had, when it had it.
// D2-12040, D2-12041
pub fn remove_float(w: &mut WeenieDesc, q: u32) -> Option<f32> {
    let had = float(w, q)?;
    w.mask &= !WeenieDesc::SCALE;
    Some(had)
}

/// Take the icon out of the description: the icon it had, when it had it.
// D2-12048, D2-12049
pub fn remove_data_id(w: &mut WeenieDesc, q: u32) -> Option<u32> {
    let had = data_id(w, q)?;
    w.mask &= !WeenieDesc::ICON;
    Some(had)
}

/// Take an object quality out of the description (its mask bit cleared): the object it had, when
/// it had it.
// D2-12053, D2-12054
pub fn remove_instance_id(w: &mut WeenieDesc, q: u32) -> Option<u64> {
    let had = instance_id(w, q)?;
    if let Some((_, bit)) = OBJECTS.iter().find(|(k, _)| *k == q) {
        w.mask &= !bit;
    }
    Some(had)
}

/// The kinds of quality the description never holds (64-bit numbers, times, texts, positions and
/// packages): it answers none of them, sets none and takes none out.
// D2-12033, D2-12046, D2-12059, D2-12066, D2-12075
#[must_use]
pub fn holds_none<V>(_: &WeenieDesc, _: u32) -> Option<V> {
    None
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the description's quality map on hand-built values)
    use super::*;

    #[test]
    fn a_container_set_on_the_description_is_read_back_from_it_and_marked_held() {
        let mut w = WeenieDesc::default();
        assert_eq!(instance_id(&w, 0x102), None);
        assert!(set_instance_id(&mut w, 0x102, 0x2130_0000_0000_0001));
        assert_eq!(w.mask & 0x20, 0x20);
        assert_eq!(instance_id(&w, 0x102), Some(0x2130_0000_0000_0001));
        assert!(!set_instance_id(&mut w, 0x103, 1));
    }

    #[test]
    fn a_yes_or_no_quality_lives_in_the_flag_word_and_turning_one_on_marks_the_word_held() {
        let mut w = WeenieDesc::default();
        assert!(set_bool(&mut w, 0x117, 1));
        assert_eq!(
            w.mask & (0x4000 | WeenieDesc::BITFIELD),
            0x4000 | WeenieDesc::BITFIELD
        );
        assert_eq!(bool(&w, 0x117), Some(1));
        assert!(set_bool(&mut w, 0x117, 0));
        assert_eq!(bool(&w, 0x117), Some(0));
        assert_eq!(bool(&w, 10), None);
    }

    #[test]
    fn a_number_is_answered_only_while_its_mask_bit_is_set() {
        let mut w = WeenieDesc::default();
        assert_eq!(int(&w, 0x106), None);
        assert!(set_int(&mut w, 0x106, 7));
        assert_eq!(int(&w, 0x106), Some(7));
        assert!(holds_int(2000) && !holds_int(720));
    }

    #[test]
    fn a_records_description_answers_its_flags_numbers_and_objects_but_not_its_names() {
        let d = ObjectDescription {
            mask: 0x80_0000 | 0x2 | 0x4 | 0x800 | 0x20,
            flags: Some(0x2),
            quantity: Some(3),
            container: Some(0x2130_0000_0000_0007),
            ..ObjectDescription::default()
        };
        let w = of_record(&d);
        assert_eq!(bool(&w, 0x270), Some(1), "selectable");
        assert_eq!(bool(&w, 10), None);
        assert_eq!(int(&w, 0x106), Some(3));
        assert_eq!(instance_id(&w, 0x102), Some(0x2130_0000_0000_0007));
        assert_eq!(string_info(&w, 0x100), None);
    }
}
