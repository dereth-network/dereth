//! The player's options and the shortcuts among them, as scripts read and change them.
//!
//! **Shortcuts.** A shortcut's getters answer its kind, its item (64 bits), its enumerated value,
//! its data id and a new string holding its text; the setters store each.
//!
//! **The options profile.** Asking for a shortcut by slot copies the slot's shortcut into the
//! object an out reference names and answers 1, or answers 0 when the slot is past the array;
//! setting one copies a shortcut into a slot within the array. Asking for the whole array fills
//! the list an out reference names with a new shortcut per slot. The yes-or-no options are bits of
//! one word (each getter and setter has its bit); the shortcut set, the shortcut height and the
//! radar's mask are words, others' damage range a float. The windows' saved places and opacities
//! are copied into the objects out references name, as the engine copies one table into another:
//! the target's buckets are kept, emptied, and each entry of the source, in the source's order,
//! goes to the head of its chain.
//!
//! **The chat options.** A chat window's filter, a text kind's size, color and popup flag, and a
//! devoted window's channel are read by key from the profile's tables; a key a table lacks reads
//! as 0 (a devoted window's channel as the say channel). A profile no packed form filled holds the
//! tables a new profile does.
//!
//! The setters set an entry in their table as a window's opacity is set (an entry changed, or a
//! new one at the head of its chain); a table the profile does not hold yet is first made as a
//! new profile holds it, with a new player's values. Kinds of text are added to a window's filter
//! (or'ed in; set when the window has none) and taken out (a window with none is left so).
//!
//! **The aliases.** Setting one stores the name and text as given (the scripts lower the name's
//! case first) and answers 1; asking copies what a name stands for into the string an out
//! reference names and answers 1, or answers 0 and leaves the string alone; removing answers
//! whether the name was held; the list is a new string of `alias [name] == [text]` lines.

use std::sync::Arc;

use auberean_script::library::Elem;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, Item, NativeObject};
use auberean_script::options::{GameplayOptions, Shortcut, Table};
use auberean_script::options_file;
use auberean_script::ui_locations::{buckets_of, copy_into, UiLocations};
use auberean_script::value::{tag, NULL};

const PROFILE: u32 = 0x9B;
const SHORTCUT: u32 = 0xAC;
const UI_SAVE_LOCATIONS: u32 = 0x6F;

/// The yes-or-no options: the getter's number, the setter's number and the bit.
const BITS: &[(u32, u32, u32)] = &[
    (0x10, 0x0F, 0x1),
    (0x12, 0x11, 0x4),
    (0x14, 0x13, 0x8),
    (0x1A, 0x19, 0x10),
    (0x24, 0x23, 0x20),
    (0x26, 0x25, 0x40),
    (0x28, 0x27, 0x80),
    (0x2A, 0x29, 0x100),
    (0x3B, 0x3A, 0x200),
    (0x45, 0x44, 0x400),
    (0x33, 0x32, 0x800),
    (0x37, 0x36, 0x1000),
    (0x35, 0x34, 0x2000),
    (0x47, 0x46, 0x4000),
    (0x49, 0x48, 0x8000),
    (0x4B, 0x4A, 0x1_0000),
    (0x4D, 0x4C, 0x2_0000),
];

/// The options whose bit set means off: fellows on the map and the pet panel. A cleared profile
/// shows both.
const OFF_WHEN_SET: [u32; 2] = [0x8000, 0x1_0000];

/// The yes-or-no options' getters and setters (each finds its bit in [`BITS`]).
const OPTIONS: &[(u32, u32, NativeFn)] = &[
    (PROFILE, 0x10, option),
    (PROFILE, 0x0F, set_option),
    (PROFILE, 0x12, option),
    (PROFILE, 0x11, set_option),
    (PROFILE, 0x14, option),
    (PROFILE, 0x13, set_option),
    (PROFILE, 0x1A, option),
    (PROFILE, 0x19, set_option),
    (PROFILE, 0x24, option),
    (PROFILE, 0x23, set_option),
    (PROFILE, 0x26, option),
    (PROFILE, 0x25, set_option),
    (PROFILE, 0x28, option),
    (PROFILE, 0x27, set_option),
    (PROFILE, 0x2A, option),
    (PROFILE, 0x29, set_option),
    (PROFILE, 0x3B, option),
    (PROFILE, 0x3A, set_option),
    (PROFILE, 0x45, option),
    (PROFILE, 0x44, set_option),
    (PROFILE, 0x33, option),
    (PROFILE, 0x32, set_option),
    (PROFILE, 0x37, option),
    (PROFILE, 0x36, set_option),
    (PROFILE, 0x35, option),
    (PROFILE, 0x34, set_option),
    (PROFILE, 0x47, option),
    (PROFILE, 0x46, set_option),
    (PROFILE, 0x49, option),
    (PROFILE, 0x48, set_option),
    (PROFILE, 0x4B, option),
    (PROFILE, 0x4A, set_option),
    (PROFILE, 0x4D, option),
    (PROFILE, 0x4C, set_option),
];

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (PROFILE, 0x2B, export),
    (PROFILE, 0x2C, import),
    (SHORTCUT, 0x01, shortcut_kind),
    (SHORTCUT, 0x02, shortcut_item),
    (SHORTCUT, 0x03, shortcut_enum),
    (SHORTCUT, 0x04, shortcut_text),
    (SHORTCUT, 0x05, set_shortcut_kind),
    (SHORTCUT, 0x06, set_shortcut_item),
    (SHORTCUT, 0x07, set_shortcut_enum),
    (SHORTCUT, 0x08, set_shortcut_text),
    (SHORTCUT, 0x09, shortcut_data_id),
    (SHORTCUT, 0x0A, set_shortcut_data_id),
    (PROFILE, 0x01, set_alias),
    (PROFILE, 0x02, inq_alias),
    (PROFILE, 0x05, flush_alias_table),
    (PROFILE, 0x06, remove_alias),
    (PROFILE, 0x07, build_alias_list),
    (PROFILE, 0x08, set_shortcut),
    (PROFILE, 0x09, inq_shortcut),
    (PROFILE, 0x0A, set_shortcut_array),
    (PROFILE, 0x0B, inq_shortcut_array),
    (PROFILE, 0x0C, init_shortcut_array),
    (PROFILE, 0x0D, set_shortcut_set),
    (PROFILE, 0x0E, shortcut_set),
    (PROFILE, 0x15, set_damage_text_range),
    (PROFILE, 0x16, damage_text_range),
    (PROFILE, 0x17, set_ui_locations),
    (PROFILE, 0x18, inq_ui_locations),
    (PROFILE, 0x1B, add_radar_mask),
    (PROFILE, 0x1C, remove_radar_mask),
    (PROFILE, 0x1D, set_radar_mask),
    (PROFILE, 0x1E, radar_mask),
    (PROFILE, 0x2D, bitfield),
    (PROFILE, 0x2E, set_bitfield),
    (PROFILE, 0x31, ui_locations),
    (UI_SAVE_LOCATIONS, 0x01, clear_ui_locations),
    (PROFILE, 0x38, set_shortcut_height),
    (PROFILE, 0x39, shortcut_height),
    (PROFILE, 0x4E, window_opacities),
    (PROFILE, 0x42, chat_options),
    (PROFILE, 0x22, filter),
    (PROFILE, 0x3E, chat_font_size),
    (PROFILE, 0x3F, chat_font_color),
    (PROFILE, 0x41, devoted_channel),
    (PROFILE, 0x51, chat_popup_flag),
    (PROFILE, 0x50, set_window_opacity),
    (PROFILE, 0x21, set_filter),
    (PROFILE, 0x1F, add_filter),
    (PROFILE, 0x20, remove_filter),
    (PROFILE, 0x3C, set_chat_font_size),
    (PROFILE, 0x3D, set_chat_font_color),
    (PROFILE, 0x40, set_devoted_channel),
    (PROFILE, 0x52, set_chat_popup_flag),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
    crate::bind_table(t, OPTIONS);
}

// ---------------------------------------------------------------------------------------------
// Shortcuts
// ---------------------------------------------------------------------------------------------

/// The shortcut a reference names (`None` for the null reference: the call crashes).
fn shortcut_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<Option<&'a mut Shortcut>, Stop> {
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let o = c.vm.heap.native_mut(r)?;
    if let NativeObject::Packed { ty, .. } = o {
        if *ty == native_type::SHORTCUT_INFO {
            *o = o.unpacked();
        }
    }
    match o {
        NativeObject::Shortcut(s) => Ok(Some(s)),
        _ => Err(Stop::Native {
            native,
            what: "not a shortcut".into(),
        }),
    }
}

/// A word of the receiver's shortcut, into the result slot.
fn shortcut_word(c: &mut Call<'_>, f: fn(&Shortcut) -> u32) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let Some(s) = shortcut_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let v = f(s);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Store the argument into a word of the receiver's shortcut.
fn set_shortcut_word(c: &mut Call<'_>, f: fn(&mut Shortcut, u32)) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let v = if r == NULL { 0 } else { c.arg(0)? };
    let Some(s) = shortcut_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    f(s, v);
    Ok(Status::Success)
}

// D2-18959
fn shortcut_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    shortcut_word(c, |s| s.kind)
}

// D2-01611
fn shortcut_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    shortcut_word(c, |s| s.data_enum)
}

// D2-01521
fn shortcut_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    shortcut_word(c, |s| s.data_id)
}

/// The item, 64 bits below the receiver (only the low slot is tagged).
// D2-01522
fn shortcut_item(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let Some(s) = shortcut_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let v = s.item;
    #[allow(clippy::cast_possible_truncation)]
    let (lo, hi) = (v as u32, (v >> 32) as u32);
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// The text: a new string holding a copy.
// D2-01524, D2-01519
fn shortcut_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let Some(s) = shortcut_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let text = s.text.clone();
    let id =
        c.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(text));
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-01370
fn set_shortcut_kind(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_shortcut_word(c, |s, v| s.kind = v)
}

// D2-01514
fn set_shortcut_enum(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_shortcut_word(c, |s, v| s.data_enum = v)
}

// D2-01517
fn set_shortcut_data_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_shortcut_word(c, |s, v| s.data_id = v)
}

/// The item: a 64-bit argument, low slot first.
// D2-01518
fn set_shortcut_item(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let v = if r == NULL { 0 } else { c.long_at(-3)? };
    let Some(s) = shortcut_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    s.item = v;
    Ok(Status::Success)
}

/// The text: a copy of the string the argument names.
// D2-01523, D2-01520
fn set_shortcut_text(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let t = c.arg(0)?;
    let text = match c.vm.heap.native(t)? {
        NativeObject::Text(u) => u.clone(),
        _ => return Err(c.fail("not a string")),
    };
    if let Some(s) = shortcut_mut(c, r)? {
        s.text = text;
    }
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// The options profile
// ---------------------------------------------------------------------------------------------

/// The profile a reference names, read as the engine holds it the first time.
fn profile_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<Option<&'a mut GameplayOptions>, Stop> {
    if r == NULL {
        return Ok(None);
    }
    let native = c.native;
    let o = c.vm.heap.native_mut(r)?;
    if let NativeObject::Packed { ty, .. } = o {
        if *ty == native_type::GAMEPLAY_OPTIONS_PROFILE {
            *o = o.unpacked();
        }
    }
    match o {
        NativeObject::Options(p) => Ok(Some(p)),
        NativeObject::Packed { .. } => Err(Stop::Native {
            native,
            what: "an options profile that does not read".into(),
        }),
        _ => Err(Stop::Native {
            native,
            what: "not an options profile".into(),
        }),
    }
}

macro_rules! with_profile {
    ($c:ident, $p:ident) => {
        let r = $c.receiver()?;
        let Some($p) = profile_mut($c, r)? else {
            return Ok(Status::Crash);
        };
    };
}

/// A word of the receiver's profile, into the result slot.
fn profile_word(c: &mut Call<'_>, f: fn(&GameplayOptions) -> u32) -> Result<Status, Stop> {
    with_profile!(c, p);
    let v = f(p);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// Change the receiver's profile with the argument below it.
fn set_profile_word(c: &mut Call<'_>, f: fn(&mut GameplayOptions, u32)) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let v = if r == NULL { 0 } else { c.arg(0)? };
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    f(p, v);
    Ok(Status::Success)
}

/// A file name as the options' files take it: narrow, each unit past the narrow range a `?`.
fn narrow_name(w: &[u16]) -> String {
    w.iter()
        .map(|&u| char::from(u8::try_from(u).unwrap_or(b'?')))
        .collect()
}

/// Write the receiver's profile to a file in the client's folder named by a string (`fp[-2]`);
/// answers 0, or the error (`fp[-3]`): no name, or the file not written.
// D2-01409
fn export(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let name = narrow_name(&crate::machine::text_of(c, c.arg(0)?)?);
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let answer = match p.export(&name) {
        Err(e) => e,
        Ok(bytes) if c.host.write_file(&name, &bytes) => 0,
        Ok(_) => options_file::NOT_WRITTEN,
    };
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

/// Read the receiver's profile from a file in the client's folder named by a string (`fp[-2]`);
/// answers 0, or the error (`fp[-3]`): no name, no file or one that does not read, or one holding
/// nothing. The profile changes only when the file reads.
// D2-01434
fn import(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let name = narrow_name(&crate::machine::text_of(c, c.arg(0)?)?);
    let answer = if name.is_empty() {
        options_file::NO_FILE_NAME
    } else {
        match c.host.read_file(&name) {
            None => options_file::NOT_READ,
            Some(bytes) => match GameplayOptions::import(&name, &bytes) {
                Err(e) => e,
                Ok(read) => {
                    let Some(p) = profile_mut(c, r)? else {
                        return Ok(Status::Crash);
                    };
                    *p = read;
                    0
                }
            },
        }
    };
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

/// The slot an out reference names: the object reference it holds.
fn out_target(c: &mut Call<'_>, k: i64) -> Result<u32, Stop> {
    let out = c.cx.at(k)?;
    let prog = Arc::clone(c.vm.program());
    let (t, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    Ok(t)
}

/// A float through the floating-point unit: a signalling not-a-number comes out quiet.
fn through_fpu(v: u32) -> u32 {
    if f32::from_bits(v).is_nan() {
        v | 1 << 22
    } else {
        v
    }
}

/// The shortcut in a slot (`fp[-2]`), copied into the shortcut an out reference (`fp[-3]`)
/// names: 1, or 0 when the slot is past the array (the shortcut is left alone).
// D2-01483, D2-11966, D2-12176
fn inq_shortcut(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let slot = c.arg(0)?;
    let target = out_target(c, -3)?;
    let found = profile_mut(c, r)?.and_then(|p| p.shortcut(slot)).cloned();
    if let Some(s) = &found {
        match shortcut_mut(c, target)? {
            Some(t) => t.clone_from(s),
            None => return Ok(Status::Crash),
        }
    }
    c.write(-4, u32::from(found.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// A copy of a shortcut (`fp[-3]`) into a slot (`fp[-2]`): 1, or 0 past the array.
// D2-01445, D2-11951, D2-12176
fn set_shortcut(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let slot = c.arg(0)?;
    let from = c.arg(1)?;
    let Some(s) = shortcut_mut(c, from)?.map(|s| s.clone()) else {
        return Ok(Status::Crash);
    };
    let done = profile_mut(c, r)?.is_some_and(|p| p.set_shortcut(slot, &s));
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// Every shortcut, into the list an out reference (`fp[-2]`) names: the list emptied, then a new
/// shortcut holding a copy of each slot's; answers 1.
// D2-01484, D2-11954, D2-01055, D2-01453
fn inq_shortcut_array(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let target = out_target(c, -2)?;
    let all = match profile_mut(c, r)? {
        Some(p) => p.shortcuts.clone(),
        None => return Ok(Status::Crash),
    };
    let ids: Vec<Item> = all
        .into_iter()
        .map(|s| {
            Item::Ref(
                c.vm.heap
                    .add_native(native_type::SHORTCUT_INFO, NativeObject::Shortcut(s)),
            )
        })
        .collect();
    match c.vm.heap.native_mut(target)? {
        NativeObject::Sequence { items, .. } => *items = ids,
        _ => return Err(c.fail("the out reference holds no list")),
    }
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The profile's shortcuts become a copy of each shortcut the list (`fp[-2]`) names, in its
/// order. No result.
// D2-01460, D2-11953
fn set_shortcut_array(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let list = c.arg(0)?;
    if list == NULL {
        return Err(Stop::NullReference);
    }
    let items = match c.vm.heap.native(list)? {
        NativeObject::Sequence { items, .. } => items.clone(),
        _ => return Err(c.fail("not a list of shortcuts")),
    };
    let mut shortcuts = Vec::with_capacity(items.len());
    for item in items {
        let Item::Ref(id) = item else {
            return Err(c.fail("a list entry that is not a shortcut"));
        };
        let Some(s) = shortcut_mut(c, id)? else {
            return Err(Stop::NullReference);
        };
        shortcuts.push(s.clone());
    }
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    p.shortcuts = shortcuts;
    Ok(Status::Success)
}

/// A new array of that many empty shortcuts (`fp[-2]`, signed: none for less than one);
/// answers 1.
// D2-01435, D2-11970
fn init_shortcut_array(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let n = if r == NULL {
        0
    } else {
        c.arg(0)?.cast_signed()
    };
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    p.shortcuts = vec![Shortcut::default(); usize::try_from(n.max(0)).unwrap_or(0)];
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01402, D2-00411
fn set_shortcut_set(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.shortcut_set = v)
}

// D2-08460, D2-00412
fn shortcut_set(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_word(c, |p| p.shortcut_set)
}

// D2-01390, D2-11945
fn set_shortcut_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.shortcut_height = v)
}

// D2-01423, D2-11946
fn shortcut_height(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_word(c, |p| p.shortcut_height)
}

// D2-01387, D2-00413
fn set_damage_text_range(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.damage_text_range = through_fpu(v))
}

// D2-08689, D2-15472
fn damage_text_range(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_word(c, |p| through_fpu(p.damage_text_range))
}

// D2-01379
fn add_radar_mask(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.radar_mask |= v)
}

// D2-01381
fn remove_radar_mask(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.radar_mask &= !v)
}

// D2-01584
fn set_radar_mask(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.radar_mask = v)
}

/// A chat option by its key (the argument below the receiver), into the slot below that.
fn profile_lookup(c: &mut Call<'_>, f: fn(&GameplayOptions, u32) -> u32) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let key = if r == NULL { 0 } else { c.arg(0)? };
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let v = f(p, key);
    c.write(-3, v, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-01420, D2-11971
fn filter(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_lookup(c, GameplayOptions::filter)
}

// D2-01417, D2-11973
fn chat_font_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_lookup(c, GameplayOptions::chat_font_size)
}

// D2-01416, D2-11974
fn chat_font_color(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_lookup(c, GameplayOptions::chat_font_color)
}

// D2-01419, D2-11972
fn devoted_channel(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_lookup(c, GameplayOptions::devoted_channel)
}

// D2-01418, D2-11975
fn chat_popup_flag(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_lookup(c, GameplayOptions::chat_popup_flag)
}

// D2-01422
fn radar_mask(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_word(c, |p| p.radar_mask)
}

// D2-01414
fn bitfield(c: &mut Call<'_>) -> Result<Status, Stop> {
    profile_word(c, |p| p.bits)
}

// D2-00797
fn set_bitfield(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_profile_word(c, |p, v| p.bits = v)
}

/// The bit of a yes-or-no option's native.
fn bit_of(native: u32) -> u32 {
    let n = native & 0xFFFF;
    BITS.iter()
        .find(|(g, s, _)| *g == n || *s == n)
        .map_or(0, |(_, _, b)| *b)
}

/// Whether one of the yes-or-no options is on.
// D2-01410, D2-01411, D2-01412, D2-01413, D2-01415, D2-01421, D2-01424, D2-01425, D2-01426
// D2-01427, D2-01428, D2-01429, D2-01430, D2-01431, D2-01432, D2-01433, D2-01436, D2-11949
fn option(c: &mut Call<'_>) -> Result<Status, Stop> {
    let bit = bit_of(c.native);
    with_profile!(c, p);
    let on = p.test_bits(bit) != OFF_WHEN_SET.contains(&bit);
    c.write(-2, u32::from(on), tag::MIXED)?;
    Ok(Status::Success)
}

/// Turn one of the yes-or-no options on or off (`fp[-2]`).
// D2-01382, D2-01383, D2-01384, D2-01385, D2-01386, D2-01388, D2-01389, D2-01391, D2-01392
// D2-01393, D2-01394, D2-01395, D2-01396, D2-01397, D2-01398, D2-01399, D2-01400, D2-11950
fn set_option(c: &mut Call<'_>) -> Result<Status, Stop> {
    let bit = bit_of(c.native);
    let r = c.receiver()?;
    let on = if r == NULL { 0 } else { c.arg(0)? };
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    p.set_bits((on != 0) != OFF_WHEN_SET.contains(&bit), bit);
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// Tables copied as the engine copies them
// ---------------------------------------------------------------------------------------------

/// The windows' saved places of a native (a new one: an empty table of 47 buckets, as the 32 a new
/// table asks for become).
// D2-13084
fn locations_of(c: &Call<'_>, r: u32) -> Result<UiLocations, Stop> {
    let bytes = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } => bytes.clone(),
        _ => auberean_script::options::NO_UI_LOCATIONS.to_vec(),
    };
    UiLocations::read(&bytes).ok_or_else(|| c.fail("a table of saved places that does not read"))
}

fn store_locations(c: &mut Call<'_>, r: u32, l: &UiLocations) -> Result<(), Stop> {
    let ty = native_type::UI_SAVE_LOCATIONS;
    *c.vm.heap.native_mut(r)? = NativeObject::Packed {
        ty,
        bytes: l.write(),
    };
    Ok(())
}

/// Every window's saved places taken out of the receiver's table; its buckets stay. No result.
// D2-09007, D2-13081
fn clear_ui_locations(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let mut l = locations_of(c, r)?;
    l.entries.iter_mut().for_each(Vec::clear);
    store_locations(c, r, &l)?;
    Ok(Status::Success)
}

/// The profile's saved places copied over the table an out reference (`fp[-2]`) names.
// D2-01479, D2-13085, D2-13083, D2-13076
fn ui_locations(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let target = out_target(c, -2)?;
    let held = match profile_mut(c, r)? {
        Some(p) => p.ui_locations.clone(),
        None => return Ok(Status::Crash),
    };
    let from = UiLocations::read(&held).ok_or_else(|| c.fail("saved places that do not read"))?;
    let into = locations_of(c, target)?;
    store_locations(c, target, &from.copied_over(&into))?;
    Ok(Status::Success)
}

/// As [`ui_locations`], answering 1 below the out reference.
// D2-01485, D2-11948
fn inq_ui_locations(c: &mut Call<'_>) -> Result<Status, Stop> {
    let s = ui_locations(c)?;
    if s == Status::Success {
        c.write(-3, 1, tag::MIXED)?;
    }
    Ok(s)
}

/// A table of saved places (`fp[-2]`) copied over the profile's.
// D2-01401, D2-11947
fn set_ui_locations(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let from_ref = c.arg(0)?;
    let from = locations_of(c, from_ref)?;
    let held = match profile_mut(c, r)? {
        Some(p) => p.ui_locations.clone(),
        None => return Ok(Status::Crash),
    };
    let into = UiLocations::read(&held).ok_or_else(|| c.fail("saved places that do not read"))?;
    let bytes = from.copied_over(&into).write();
    if let Some(p) = profile_mut(c, r)? {
        p.ui_locations = bytes;
    }
    Ok(Status::Success)
}

/// The profile's window opacities copied into the word table an out reference (`fp[-2]`) names:
/// through a table of 47 buckets, as the engine does.
// D2-01480, D2-07291, D2-01461, D2-08415
fn window_opacities(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let target = out_target(c, -2)?;
    let held = match profile_mut(c, r)? {
        Some(p) => p.opacities.entries.clone(),
        None => return Ok(Status::Crash),
    };
    copy_to_word_table(c, target, held)?;
    Ok(Status::Success)
}

/// A table's entries (in its own order) copied into the script's word table `target` names,
/// through a table of 47 buckets, as the engine does: each copy empties its target, keeps the
/// target's bucket count and puts each entry, in the source's order, at the head of its chain.
// D2-01461, D2-01463
fn copy_to_word_table(c: &mut Call<'_>, target: u32, held: Vec<(u32, u32)>) -> Result<(), Stop> {
    let local = copy_into(buckets_of(2), held);
    match c.vm.heap.native_mut(target)? {
        NativeObject::Hash(h) if h.key == Elem::Word && h.value == Elem::Word => {
            let n = h.buckets.len();
            let b = copy_into(n, local.into_iter().flatten());
            h.buckets = b
                .into_iter()
                .map(|chain| {
                    chain
                        .into_iter()
                        .map(|(k, v)| (Item::Word(k), Item::Word(v)))
                        .collect()
                })
                .collect();
            Ok(())
        }
        _ => Err(c.fail("the out reference holds no word table")),
    }
}

/// The profile's chat tables copied into the word tables five out references name: the filters
/// (`fp[-2]`), the font sizes (`fp[-3]`), the font colors (`fp[-4]`), the devoted windows
/// (`fp[-5]`) and the popup flags (`fp[-6]`), each as [`copy_to_word_table`] copies; a table the
/// profile does not hold yet is a new profile's. No result.
// D2-01478
fn chat_options(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::options::chat_defaults as d;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let mut targets = [0; 5];
    for (k, t) in (2..=6).zip(targets.iter_mut()) {
        *t = out_target(c, -k)?;
    }
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    if p.popup_flags.is_empty() {
        p.popup_flags = new_table(&d::POPUP_FLAGS);
    }
    let tables = [
        chat_table(&mut p.filters, &d::FILTERS).entries.clone(),
        chat_table(&mut p.font_sizes, &d::FONT_SIZES)
            .entries
            .clone(),
        chat_table(&mut p.font_colors, &d::FONT_COLORS)
            .entries
            .clone(),
        chat_table(&mut p.devoted_windows, &d::DEVOTED_WINDOWS)
            .entries
            .clone(),
        p.popup_flags.entries.clone(),
    ];
    for (t, held) in targets.into_iter().zip(tables) {
        copy_to_word_table(c, t, held)?;
    }
    Ok(Status::Success)
}

/// An entry of a word table set, or added at the head of its chain when new (before the first
/// entry of its bucket); the header's count kept.
// D2-07384
fn set_entry(t: &mut Table<(u32, u32)>, key: u32, v: u32) {
    match t.entries.iter_mut().find(|(k, _)| *k == key) {
        Some(e) => e.1 = v,
        None => {
            let n = buckets_of(u8::try_from(t.header >> 24).unwrap_or(0));
            let b = key as usize % n;
            let at = t
                .entries
                .iter()
                .position(|(k, _)| *k as usize % n >= b)
                .unwrap_or(t.entries.len());
            t.entries.insert(at, (key, v));
            t.header = (t.header & 0xFF00_0000) | u32::try_from(t.entries.len()).unwrap_or(0);
        }
    }
}

/// A window's opacity (`fp[-2]` the window, `fp[-3]` the opacity's bits, copied as they are):
/// set, or added at the head of its chain when new; answers 1.
// D2-01446, D2-11993
fn set_window_opacity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let window = c.arg(0)?;
    let v = c.arg(1)?;
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    set_entry(&mut p.opacities, window, v);
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// A new profile's empty word table (its buckets as the profile makes them).
const NEW_TABLE: u32 = 0x0200_0000;

/// One of the chat's tables, made as a new profile makes it (a new player's values set in the
/// engine's order) when the profile does not hold it yet.
fn chat_table<'a>(
    t: &'a mut Option<Table<(u32, u32)>>,
    defaults: &[(u32, u32)],
) -> &'a mut Table<(u32, u32)> {
    t.get_or_insert_with(|| new_table(defaults))
}

/// A new profile's table holding a new player's values, set in the engine's order.
fn new_table(defaults: &[(u32, u32)]) -> Table<(u32, u32)> {
    let mut new = Table {
        header: NEW_TABLE,
        entries: Vec::new(),
    };
    for (k, v) in defaults {
        set_entry(&mut new, *k, *v);
    }
    new
}

/// Which of the chat's tables a setter sets.
#[derive(Clone, Copy)]
enum Chat {
    Filters,
    Devoted,
    FontSizes,
    FontColors,
    Popups,
}

/// One of the chat's settings (`fp[-2]` the window or the kind of text, `fp[-3]` the value; a
/// popup flag is the value's low byte): set in its table; answers 1.
fn set_chat(c: &mut Call<'_>, which: Chat) -> Result<Status, Stop> {
    use auberean_script::options::chat_defaults as d;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let key = c.arg(0)?;
    let v = c.arg(1)?;
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    match which {
        Chat::Filters => set_entry(chat_table(&mut p.filters, &d::FILTERS), key, v),
        Chat::Devoted => set_entry(
            chat_table(&mut p.devoted_windows, &d::DEVOTED_WINDOWS),
            key,
            v,
        ),
        Chat::FontSizes => set_entry(chat_table(&mut p.font_sizes, &d::FONT_SIZES), key, v),
        Chat::FontColors => set_entry(chat_table(&mut p.font_colors, &d::FONT_COLORS), key, v),
        Chat::Popups => {
            // An empty popup table is a new profile's: its defaults first.
            if p.popup_flags.is_empty() {
                p.popup_flags = new_table(&d::POPUP_FLAGS);
            }
            set_entry(&mut p.popup_flags, key, v & 0xFF);
        }
    }
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// A chat window's filter.
// D2-01444, D2-11985
fn set_filter(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_chat(c, Chat::Filters)
}

/// Kinds of text added to a chat window's filter (`fp[-2]` the window, `fp[-3]` the kinds): the
/// window's filter with them or'ed in, or set to them when the window has none; answers 1.
// D2-01408, D2-12000
fn add_filter(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::options::chat_defaults as d;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (window, kinds) = (c.arg(0)?, c.arg(1)?);
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let t = chat_table(&mut p.filters, &d::FILTERS);
    let held = t
        .entries
        .iter()
        .find(|(k, _)| *k == window)
        .map_or(0, |e| e.1);
    set_entry(t, window, held | kinds);
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Kinds of text taken out of a chat window's filter (`fp[-2]` the window, `fp[-3]` the kinds); a
/// window with no filter is left without one; answers 1.
// D2-01438, D2-11984
fn remove_filter(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::options::chat_defaults as d;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (window, kinds) = (c.arg(0)?, c.arg(1)?);
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    // A profile that holds no filters yet answers from a new one's.
    let held = match &p.filters {
        Some(t) => t.entries.iter().find(|(k, _)| *k == window).map(|e| e.1),
        None => d::FILTERS.iter().find(|(k, _)| *k == window).map(|e| e.1),
    };
    if let Some(held) = held {
        set_entry(
            chat_table(&mut p.filters, &d::FILTERS),
            window,
            held & !kinds,
        );
    }
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The channel a chat window is devoted to.
// D2-01443, D2-11987
fn set_devoted_channel(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_chat(c, Chat::Devoted)
}

/// A kind of text's font size.
// D2-01441, D2-11988
fn set_chat_font_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_chat(c, Chat::FontSizes)
}

/// A kind of text's font color.
// D2-01440, D2-11989
fn set_chat_font_color(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_chat(c, Chat::FontColors)
}

/// Whether a line of a kind of text shows a hidden chat window.
// D2-01442, D2-11991
fn set_chat_popup_flag(c: &mut Call<'_>) -> Result<Status, Stop> {
    set_chat(c, Chat::Popups)
}

// ---------------------------------------------------------------------------------------------
// The aliases
// ---------------------------------------------------------------------------------------------

/// A string argument's text (the null reference: the client reads past its table).
fn text_at(c: &Call<'_>, k: i64) -> Result<Vec<u16>, Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    crate::machine::text_of(c, r)
}

/// An alias set: the name `fp[-2]`, what it stands for `fp[-3]`; answers 1 at `fp[-4]`.
// D2-01439, D2-11997
fn set_alias(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let (name, text) = (text_at(c, -2)?, text_at(c, -3)?);
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let done = p.set_alias(&name, &text);
    c.write(-4, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// What an alias (`fp[-2]`) stands for, copied into the string the out reference `fp[-3]` names:
/// 1 at `fp[-4]`, or 0 with the string left alone.
// D2-01481, D2-11981
fn inq_alias(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let name = text_at(c, -2)?;
    let target = out_target(c, -3)?;
    let found = profile_mut(c, r)?.and_then(|p| p.alias(&name).map(<[u16]>::to_vec));
    if let Some(text) = &found {
        match c.vm.heap.native_mut(target)? {
            NativeObject::Text(t) => t.clone_from(text),
            _ => return Err(c.fail("the out reference names no string")),
        }
    }
    c.write(-4, u32::from(found.is_some()), tag::MIXED)?;
    Ok(Status::Success)
}

/// Every alias out.
// D2-01380, D2-11982
fn flush_alias_table(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_profile!(c, p);
    p.flush_aliases();
    Ok(Status::Success)
}

/// An alias (`fp[-2]`) taken out: whether it was held, at `fp[-3]`.
// D2-01437, D2-11983
fn remove_alias(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let name = text_at(c, -2)?;
    let Some(p) = profile_mut(c, r)? else {
        return Ok(Status::Crash);
    };
    let done = p.remove_alias(&name);
    c.write(-3, u32::from(done), tag::MIXED)?;
    Ok(Status::Success)
}

/// The aliases listed, a new string at `fp[-2]`.
// D2-01465, D2-11969
fn build_alias_list(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_profile!(c, p);
    let list = p.alias_list();
    let id = crate::machine::new_text(c, list);
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}
