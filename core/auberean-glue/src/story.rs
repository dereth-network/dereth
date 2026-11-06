//! The story's acts, scenes and keyframes as scripts build them for the quest window's story page
//! and for a scene to be played (the records are `auberean_script::story`'s).
//!
//! - **An act** is filled whole from its id, its name (a copy of the string reference), its
//!   number, whether it is visible and whether it may be played, and its scenes: copied as given
//!   when the last argument says they are in order, else each added after the scenes already held
//!   whose number is no greater. A scene added on its own goes in the same way. Its id is read
//!   back.
//! - **A list of acts** keeps copies, each put after the acts whose number is no greater; taking
//!   the first copies it over the act the argument names and removes it, answering whether there
//!   was one.
//! - **A scene** is read and set field by field (its id, also by its name in the scenes'
//!   enumeration; its name, as a new string reference holding a copy; its act; its number;
//!   whether hidden and playable), set from another whole, cleared, its keyframes' tracks emptied
//!   (the scene a player may not see), and given keyframes: each a copy with the scene's id and
//!   the next number.
//! - **A list of scenes** keeps copies: added at the end or after those whose number is no
//!   greater, the first taken as for acts, its size read.
//! - **A keyframe** is read and set field by field: setting a track, the subtitle or the movie
//!   also marks it defined (a track named by its file is the id the client's table of file names
//!   gives, 0 on an install of the relaunched game: the track is then defined and empty); its
//!   duration is a 64-bit float stored through the floating-point unit
//!   (a signalling not-a-number comes out quiet); its subtitle and movie are read back as new
//!   copies.
//!
//! A word flag (visible, playable, in order) is read from the low byte of its slot. A null
//! receiver crashes the call; a null record argument stops it.

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject, TextInfo};
use auberean_script::story::{self, track, ActInfo, Keyframe, SceneInfo};
use auberean_script::value::{split, tag, NULL};

use crate::machine::{new_text, text_of};

const ACT_INFO: u32 = 0x9C;
const ACT_INFO_LIST: u32 = 0x9D;
const KEYFRAME: u32 = 0x9F;
const SCENE_INFO: u32 = 0xA3;
const SCENE_INFO_LIST: u32 = 0xA4;

/// The enumeration scenes are named in.
const SCENE_ENUMERATION: u32 = 0x4100_0019;

/// The enumeration acts are named in.
const ACT_ENUMERATION: u32 = 0x4100_0018;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (ACT_INFO, 0x01, act_init),
    (ACT_INFO, 0x02, act_add_scene),
    (ACT_INFO, 0x03, act_id),
    (ACT_INFO, 0x04, act_dump),
    (ACT_INFO_LIST, 0x01, acts_insert_sort),
    (ACT_INFO_LIST, 0x02, acts_pop_front),
    (KEYFRAME, 0x01, key_dump),
    (KEYFRAME, 0x02, key_clear),
    (KEYFRAME, 0x03, key_scene),
    (KEYFRAME, 0x04, key_set_scene),
    (KEYFRAME, 0x05, key_number),
    (KEYFRAME, 0x06, key_set_number),
    (KEYFRAME, 0x07, key_video),
    (KEYFRAME, 0x08, key_set_video),
    (KEYFRAME, 0x09, key_set_video_by_name),
    (KEYFRAME, 0x0A, key_audio),
    (KEYFRAME, 0x0B, key_set_audio),
    (KEYFRAME, 0x0C, key_set_audio_by_name),
    (KEYFRAME, 0x0D, key_voice),
    (KEYFRAME, 0x0E, key_set_voice),
    (KEYFRAME, 0x0F, key_set_voice_by_name),
    (KEYFRAME, 0x10, key_subtitle),
    (KEYFRAME, 0x11, key_set_subtitle),
    (KEYFRAME, 0x12, key_movie),
    (KEYFRAME, 0x13, key_set_movie),
    (KEYFRAME, 0x14, key_duration),
    (KEYFRAME, 0x15, key_set_duration),
    (SCENE_INFO, 0x01, scene_dump),
    (SCENE_INFO, 0x02, scene_records),
    (SCENE_INFO, 0x04, scene_clear),
    (SCENE_INFO, 0x05, scene_id),
    (SCENE_INFO, 0x06, scene_set_id),
    (SCENE_INFO, 0x07, scene_set_id_by_name),
    (SCENE_INFO, 0x08, scene_name),
    (SCENE_INFO, 0x09, scene_set_name),
    (SCENE_INFO, 0x0A, scene_act),
    (SCENE_INFO, 0x0B, scene_set_act),
    (SCENE_INFO, 0x0C, scene_number),
    (SCENE_INFO, 0x0D, scene_set_number),
    (SCENE_INFO, 0x0E, scene_hidden),
    (SCENE_INFO, 0x0F, scene_set_hidden),
    (SCENE_INFO, 0x10, scene_playable),
    (SCENE_INFO, 0x11, scene_set_playable),
    (SCENE_INFO, 0x12, scene_clear_for_hackers),
    (SCENE_INFO, 0x13, scene_set),
    (SCENE_INFO, 0x14, scene_add_keyframe),
    (SCENE_INFO_LIST, 0x01, scenes_size),
    (SCENE_INFO_LIST, 0x02, scenes_push_tail),
    (SCENE_INFO_LIST, 0x03, scenes_pop_front),
    (SCENE_INFO_LIST, 0x04, scenes_insert_sort),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The object a reference names, as the engine holds it (a packed one read first).
fn held<'a>(c: &'a mut Call<'_>, r: u32, ty: u32) -> Result<&'a mut NativeObject, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let o = c.vm.heap.native_mut(r)?;
    if matches!(o, NativeObject::Packed { ty: t, .. } if *t == ty) {
        *o = o.unpacked();
    }
    Ok(o)
}

fn not_a(c: &Call<'_>, what: &str) -> Stop {
    c.fail(format!("not {what}"))
}

macro_rules! accessor {
    ($name:ident, $ty:expr, $variant:ident, $t:ty, $what:expr) => {
        fn $name<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut $t, Stop> {
            let native = c.native;
            match held(c, r, $ty)? {
                NativeObject::$variant(x) => Ok(x),
                _ => Err(Stop::Native {
                    native,
                    what: concat!("not ", $what).into(),
                }),
            }
        }
    };
}

accessor!(act_mut, native_type::ACT_INFO, Act, Box<ActInfo>, "an act");
accessor!(
    acts_mut,
    native_type::ACT_INFO_LIST,
    Acts,
    Vec<ActInfo>,
    "a list of acts"
);
accessor!(
    scene_mut,
    native_type::SCENE_INFO,
    Scene,
    Box<SceneInfo>,
    "a scene"
);
accessor!(
    scenes_mut,
    native_type::SCENE_INFO_LIST,
    Scenes,
    Vec<SceneInfo>,
    "a list of scenes"
);
accessor!(
    key_mut,
    native_type::KEYFRAME,
    Keyframe,
    Box<Keyframe>,
    "a keyframe"
);

/// The receiver, or `None` for the null reference (the call then crashes).
fn receiver(c: &Call<'_>) -> Result<Option<u32>, Stop> {
    let r = c.receiver()?;
    Ok((r != NULL).then_some(r))
}

macro_rules! with_receiver {
    ($c:ident, $r:ident) => {
        let Some($r) = receiver($c)? else {
            return Ok(Status::Crash);
        };
    };
}

/// A flag argument: the low byte of its slot.
fn flag(c: &Call<'_>, k: i64) -> Result<u32, Stop> {
    Ok(c.cx.at(k)? & 0xFF)
}

/// A string reference argument, copied.
fn text_info_arg(c: &mut Call<'_>, k: i64) -> Result<TextInfo, Stop> {
    let r = c.cx.at(k)?;
    if r == NULL {
        return Err(Stop::NullReference);
    }
    match c.vm.heap.native(r)? {
        NativeObject::TextInfo(t) => Ok(t.clone()),
        _ => Err(not_a(c, "a string reference")),
    }
}

/// A new string reference holding a copy, into the result slot below the receiver.
fn give_text_info(c: &mut Call<'_>, t: TextInfo) -> Result<Status, Stop> {
    let id =
        c.vm.heap
            .add_native(native_type::STRING_INFO, NativeObject::TextInfo(t));
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A word of the receiver's record, into the result slot below it.
fn get<T: ?Sized>(
    c: &mut Call<'_>,
    find: for<'a> fn(&'a mut Call<'_>, u32) -> Result<&'a mut T, Stop>,
    f: fn(&T) -> u32,
) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let v = f(find(c, r)?);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The word argument stored into the receiver's record.
fn set<T: ?Sized>(
    c: &mut Call<'_>,
    find: for<'a> fn(&'a mut Call<'_>, u32) -> Result<&'a mut T, Stop>,
    f: fn(&mut T, u32),
) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let v = c.arg(0)?;
    f(find(c, r)?, v);
    Ok(Status::Success)
}

// --- Acts ---

/// Fill an act: its id, name, number, whether visible and playable, its scenes, and whether they
/// come in order.
// D2-01572, D2-12315, D2-12312
fn act_init(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let id = c.cx.at(-2)?;
    let name = text_info_arg(c, -3)?;
    let number = c.cx.at(-4)?;
    let visible = flag(c, -5)?;
    let playable = flag(c, -6)?;
    let list = c.cx.at(-7)?;
    let scenes = scenes_mut(c, list)?.clone();
    let ordered = flag(c, -8)? != 0;
    act_mut(c, r)?.init(id, &name, number, visible, playable, &scenes, ordered);
    Ok(Status::Success)
}

/// A scene added in its place by number; answers 1.
// D2-01573, D2-12309
fn act_add_scene(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let s = c.cx.at(-2)?;
    let scene = (**scene_mut(c, s)?).clone();
    let v = act_mut(c, r)?.add_scene(&scene);
    c.write(-3, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// An act's id.
// D2-01574, D2-16069
fn act_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, act_mut, |a| a.id)
}

/// A copy of an act put after the acts whose number is no greater; answers 1.
// D2-01579, D2-01578
fn acts_insert_sort(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let a = c.cx.at(-2)?;
    let act = (**act_mut(c, a)?).clone();
    let v = story::insert_sorted(acts_mut(c, r)?, act, |x| x.number);
    c.write(-3, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// The first act copied over the argument's and removed; answers whether there was one.
// D2-01577, D2-00765, D2-12318
fn acts_pop_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let into = c.cx.at(-2)?;
    let first = {
        let list = acts_mut(c, r)?;
        (!list.is_empty()).then(|| list.remove(0))
    };
    let had = u32::from(first.is_some());
    if let Some(a) = first {
        **act_mut(c, into)? = a;
    }
    c.write(-3, had, tag::MIXED)?;
    Ok(Status::Success)
}

// --- Scenes ---

// D2-01634, D2-12291
fn scene_clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    scene_mut(c, r)?.clear();
    Ok(Status::Success)
}

// D2-18959
fn scene_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, scene_mut, |s| s.id)
}

// D2-01370
fn scene_set_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, scene_mut, |s, v| s.id = v)
}

/// The id the scenes' enumeration gives the name (letter case ignored); a name it lacks gives no
/// id any scene has, and the id is left as it was.
// D2-01639, D2-12282
fn scene_set_id_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let name: String = char::decode_utf16(text_of(c, c.cx.at(-2)?)?)
        .map(|u| u.unwrap_or('?'))
        .collect();
    let id = c
        .host
        .database()
        .and_then(|db| db.enumeration_value(SCENE_ENUMERATION, &name));
    let s = scene_mut(c, r)?;
    if let Some(id) = id {
        s.id = id;
    }
    Ok(Status::Success)
}

/// A scene's name: a new string reference holding a copy.
// D2-01492, D2-01633
fn scene_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let t = scene_mut(c, r)?.name.clone();
    give_text_info(c, t)
}

// D2-01487
fn scene_set_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let t = text_info_arg(c, -2)?;
    scene_mut(c, r)?.name = t;
    Ok(Status::Success)
}

// D2-01641
fn scene_act(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, scene_mut, |s| s.act)
}

// D2-01637
fn scene_set_act(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, scene_mut, |s, v| s.act = v)
}

/// A scene's number in its act.
// D2-08511
fn scene_number(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, scene_mut, |s| s.number)
}

// D2-00948
fn scene_set_number(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, scene_mut, |s, v| s.number = v)
}

// D2-01642
fn scene_hidden(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, scene_mut, |s| s.hidden)
}

// D2-01638
fn scene_set_hidden(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, scene_mut, |s, v| s.hidden = v)
}

// D2-01643
fn scene_playable(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, scene_mut, |s| s.playable)
}

// D2-00946
fn scene_set_playable(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, scene_mut, |s, v| s.playable = v)
}

/// A keyframe's records, asked about records of type `kind`: its video, audio and voice records
/// (unless the type is one the tracks are never fetched for), then its subtitle's tables.
// D2-12320
fn keyframe_records(k: &Keyframe, kind: u32, out: &mut auberean_dat::QualifiedIds) {
    if crate::data_ids::tracks_asked(kind) {
        out.add(k.video);
        out.add(k.audio);
        out.add(k.voice);
    }
    crate::text::to_formatter(&k.subtitle).sub_data_ids(out);
}

/// Every keyframe's records, asked about records of the type at `fp[-2]`, into the set the
/// reference at `fp[-3]` holds.
// D2-01645, D2-12288
fn scene_records(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let kind = c.cx.at(-2)?;
    let frames = scene_mut(c, r)?.keyframes.clone();
    let out = crate::data_ids::out_ids(c, -3)?;
    for k in &frames {
        keyframe_records(k, kind, out);
    }
    Ok(Status::Success)
}

// D2-01635, D2-12287, D2-12337
fn scene_clear_for_hackers(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    scene_mut(c, r)?.clear_for_hackers();
    Ok(Status::Success)
}

/// Every field copied from the scene the argument names.
// D2-01636, D2-12305, D2-12304, D2-12300, D2-12330
fn scene_set(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let from = c.cx.at(-2)?;
    let copy = (**scene_mut(c, from)?).clone();
    **scene_mut(c, r)? = copy;
    Ok(Status::Success)
}

/// A copy of a keyframe added with the scene's id and the next number; answers 1.
// D2-01640, D2-12289, D2-12335
fn scene_add_keyframe(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let k = c.cx.at(-2)?;
    let key = (**key_mut(c, k)?).clone();
    let v = scene_mut(c, r)?.add_keyframe(&key);
    c.write(-3, v, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-08629
fn scenes_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    #[allow(clippy::cast_possible_truncation)]
    get(c, scenes_mut, |l| l.len() as u32)
}

/// A copy added at the end; answers 1.
// D2-01649, D2-00259
fn scenes_push_tail(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let s = c.cx.at(-2)?;
    let scene = (**scene_mut(c, s)?).clone();
    scenes_mut(c, r)?.push(scene);
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The first scene copied over the argument's and removed; answers whether there was one.
// D2-01648, D2-00262
fn scenes_pop_front(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let into = c.cx.at(-2)?;
    let first = {
        let list = scenes_mut(c, r)?;
        (!list.is_empty()).then(|| list.remove(0))
    };
    let had = u32::from(first.is_some());
    if let Some(s) = first {
        **scene_mut(c, into)? = s;
    }
    c.write(-3, had, tag::MIXED)?;
    Ok(Status::Success)
}

/// A copy put after the scenes whose number is no greater; answers 1.
// D2-01651, D2-01650
fn scenes_insert_sort(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let s = c.cx.at(-2)?;
    let scene = (**scene_mut(c, s)?).clone();
    let v = story::insert_sorted(scenes_mut(c, r)?, scene, |x| x.number);
    c.write(-3, v, tag::MIXED)?;
    Ok(Status::Success)
}

// --- Keyframes ---

// D2-01601, D2-12336
fn key_clear(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    key_mut(c, r)?.clear();
    Ok(Status::Success)
}

// D2-18959
fn key_scene(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, key_mut, |k| k.scene)
}

// D2-01370
fn key_set_scene(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, key_mut, |k, v| k.scene = v)
}

// D2-08505
fn key_number(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, key_mut, |k| k.number)
}

// D2-01365
fn key_set_number(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, key_mut, |k, v| k.number = v)
}

// D2-01611
fn key_video(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, key_mut, |k| k.video)
}

// D2-01607, D2-12326
fn key_set_video(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, key_mut, |k, v| {
        k.video = v;
        k.tracks |= track::VIDEO;
    })
}

// D2-01521
fn key_audio(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, key_mut, |k| k.audio)
}

// D2-01602, D2-12327
fn key_set_audio(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, key_mut, |k, v| {
        k.audio = v;
        k.tracks |= track::AUDIO;
    })
}

/// A track's id from the file name at `fp[-2]` (typed by its extension), for a keyframe that is
/// there: none for a null receiver, which crashes the call.
fn track_by_name(c: &mut Call<'_>) -> Result<Option<(u32, u32)>, Stop> {
    let Some(r) = receiver(c)? else {
        return Ok(None);
    };
    let n = c.cx.at(-2)?;
    if n == NULL {
        return Err(c.fail("a track's file name is the null reference"));
    }
    let name = text_of(c, n)?;
    Ok(Some((r, crate::names::id_by_file_name(c, &name))))
}

/// The video track by its file's name.
// D2-01608, D2-12331
fn key_set_video_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, id)) = track_by_name(c)? else {
        return Ok(Status::Crash);
    };
    let k = key_mut(c, r)?;
    k.video = id;
    k.tracks |= track::VIDEO;
    Ok(Status::Success)
}

/// The audio track by its file's name.
// D2-01603, D2-12332
fn key_set_audio_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, id)) = track_by_name(c)? else {
        return Ok(Status::Crash);
    };
    let k = key_mut(c, r)?;
    k.audio = id;
    k.tracks |= track::AUDIO;
    Ok(Status::Success)
}

/// The voice track by its file's name.
// D2-01610, D2-12333
fn key_set_voice_by_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    let Some((r, id)) = track_by_name(c)? else {
        return Ok(Status::Crash);
    };
    let k = key_mut(c, r)?;
    k.voice = id;
    k.tracks |= track::VOICE;
    Ok(Status::Success)
}

// D2-01372
fn key_voice(c: &mut Call<'_>) -> Result<Status, Stop> {
    get(c, key_mut, |k| k.voice)
}

// D2-01609, D2-12328
fn key_set_voice(c: &mut Call<'_>) -> Result<Status, Stop> {
    set(c, key_mut, |k, v| {
        k.voice = v;
        k.tracks |= track::VOICE;
    })
}

/// The subtitle: a new string reference holding a copy.
// D2-01614, D2-01600
fn key_subtitle(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let t = key_mut(c, r)?.subtitle.clone();
    give_text_info(c, t)
}

// D2-01606, D2-12329
fn key_set_subtitle(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let t = text_info_arg(c, -2)?;
    let k = key_mut(c, r)?;
    k.subtitle = t;
    k.tracks |= track::SUBTITLE;
    Ok(Status::Success)
}

/// The movie's file name: a new string holding a copy.
// D2-01613
fn key_movie(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let m = key_mut(c, r)?.movie.clone();
    let id = new_text(c, m);
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

// D2-01605, D2-12334
fn key_set_movie(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let m = text_of(c, c.cx.at(-2)?)?;
    let k = key_mut(c, r)?;
    k.movie = m;
    k.tracks |= track::MOVIE;
    Ok(Status::Success)
}

/// The duration, 64 bits below the receiver.
// D2-01522
fn key_duration(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let (lo, hi) = split(key_mut(c, r)?.duration);
    c.write(-3, lo, tag::LONG | tag::MIXED)?;
    let i = c.cx.frame_index(-2)?;
    c.cx.data[i] = hi;
    Ok(Status::Success)
}

/// The duration, through the floating-point unit.
// D2-01604
fn key_set_duration(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let v = c.long_at(-3)?;
    let v = if f64::from_bits(v).is_nan() {
        v | 1 << 51
    } else {
        v
    };
    key_mut(c, r)?.duration = v;
    Ok(Status::Success)
}

/// The acts a list holds, as the engine holds them (none when it is not a list of acts).
#[must_use]
pub fn acts_of(o: &NativeObject) -> Vec<ActInfo> {
    match o.unpacked() {
        NativeObject::Acts(v) => v,
        _ => Vec::new(),
    }
}

/// A number as the C library's `%e` writes it: six decimals and an exponent of at least three
/// digits, with its sign.
fn scientific(x: f64) -> String {
    if !x.is_finite() {
        return crate::text::fixed(x);
    }
    let s = format!("{x:.6e}");
    let (mantissa, exponent) = s.split_once('e').unwrap_or((&s, "0"));
    let (sign, digits) = exponent
        .strip_prefix('-')
        .map_or(('+', exponent), |d| ('-', d));
    format!("{mantissa}e{sign}{digits:0>3}")
}

/// A keyframe as text: its number and duration, then each track it defines.
// D2-12340
fn keyframe_text(c: &mut Call<'_>, k: &Keyframe) -> String {
    let mut s = format!(
        "    Keyframe: {}\n     --Duration: {} secs\n",
        k.number,
        scientific(f64::from_bits(k.duration))
    );
    for (bit, label, id) in [
        (track::VIDEO, "Video", k.video),
        (track::AUDIO, "Audio", k.audio),
        (track::VOICE, "Voice", k.voice),
    ] {
        if k.tracks & bit != 0 {
            s.push_str(&format!("     --{label} Track: 0x{id:08X}.\n"));
        }
    }
    if k.tracks & track::SUBTITLE != 0 {
        let line = crate::text::log_line(c, &k.subtitle);
        s.push_str(&format!("     --Text Track: {line}.\n"));
    }
    if k.tracks & track::MOVIE != 0 {
        let movie = String::from_utf16_lossy(&k.movie);
        s.push_str(&format!("     --Text Track: {movie}.\n"));
    }
    s.push_str("    --------\n");
    s
}

/// A scene as text: its id and name, its act by number and name, whether it is hidden and
/// playable, then its steps.
// D2-12301
fn scene_text(c: &mut Call<'_>, scene: &SceneInfo) -> String {
    let name = crate::text::log_line(c, &scene.name);
    let act = c
        .host
        .database()
        .and_then(|db| db.enumeration_name(ACT_ENUMERATION, scene.act))
        .unwrap_or_default();
    let mut s = format!(
        "\n-- scene contains: --\n  SceneID: 0x{:08X}.\n  Scene Name: {name}.\n  Parent Act ID: \
         0x{:08X} ({act}).\n  Scene Number: {}.\n  Scene is {}hidden (locked).\n  Scene is \
         {}playable.\n\n",
        scene.id,
        scene.act,
        scene.number,
        if scene.hidden != 0 { "" } else { "not " },
        if scene.playable != 0 { "" } else { "not " },
    );
    for k in &scene.keyframes {
        s.push_str(&keyframe_text(c, k));
    }
    s.push_str("-----------------------\n");
    s
}

/// An act's scenes as text: each under a heading numbered from 1, or a line saying it has none.
// D2-12313
fn act_scenes_text(c: &mut Call<'_>, act: &ActInfo) -> String {
    if act.scenes.is_empty() {
        return "    < This Act has no scenes > \n".to_owned();
    }
    let mut s = String::new();
    for (i, scene) in act.scenes.iter().enumerate() {
        s.push_str(&format!(
            "    ----------------\n    -- Scene {} --\n    ----------------{}\n",
            i + 1,
            scene_text(c, scene)
        ));
    }
    s
}

/// A keyframe as text, as a new string at `fp[-2]`.
// D2-01612
fn key_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let NativeObject::Keyframe(k) = c.vm.heap.native(r)? else {
        return Err(Stop::Host("not a keyframe".into()));
    };
    let k = k.clone();
    let text = keyframe_text(c, &k);
    let id = crate::machine::new_text_in(c, text.encode_utf16().collect());
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A scene as text, as a new string at `fp[-2]`.
// D2-01644
fn scene_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let NativeObject::Scene(scene) = c.vm.heap.native(r)? else {
        return Err(Stop::Host("not a scene".into()));
    };
    let scene = scene.clone();
    let text = scene_text(c, &scene);
    let id = crate::machine::new_text_in(c, text.encode_utf16().collect());
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// An act as text, as a new string at `fp[-2]`.
// D2-01575, D2-12317
fn act_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let NativeObject::Act(a) = c.vm.heap.native(r)? else {
        return Err(Stop::Host("not an act".into()));
    };
    let a = a.clone();
    let name = crate::text::log_line(c, &a.name);
    let scenes = act_scenes_text(c, &a);
    let text = format!(
        "\n-- act contains: --\n  Act ID: 0x{:08X}.\n  Act Name: {name}.\n  Act Number: {}.\n  \
         Act is {}visible.\n  Act is {}playable.\n  Scene list contains {} items:\n{scenes}\n\
         -----------------------\n",
        a.id,
        a.number,
        if a.visible != 0 { "" } else { "not " },
        if a.playable != 0 { "" } else { "not " },
        a.scenes.len(),
    );
    let id = crate::machine::new_text_in(c, text.encode_utf16().collect());
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}
