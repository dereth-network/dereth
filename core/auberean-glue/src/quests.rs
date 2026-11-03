//! Quests as the client keeps them for the journal: a quest's record and the player's list of
//! them.
//!
//! **A record** ([`QuestInfo`]) is read field by field: its id, its status, whether an update
//! plays the quest's effect; its name, description and current journal entry, each as a new
//! string reference holding a copy. A script may fill one whole (every field, the maxed-out flag
//! too, which the server never sends).
//!
//! **The list** holds copies of records, newest given first:
//!
//! - **updating** a quest finds the first record with its id and copies the new record over it
//!   (answering, through the out argument, that it was not new), or else adds a copy before the
//!   first record given earlier than it (after every record given at the same time or later, so
//!   records given at once keep their order) and answers that it was new; it always answers 1;
//! - **erasing** one removes the last record with its id, answering whether there was one;
//! - **finding** one copies the first record with its id into the record the out argument holds,
//!   and leaves that record as it was when there is none; it always answers 1;
//! - **pushing** adds a copy at the end and answers 1; **popping** copies the first record into
//!   the out argument's record and removes it, answering whether there was one;
//! - its size is how many records it holds.
//!
//! A record or a list the client would read through a null reference stops the run.

use std::sync::Arc;

use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, NativeObject, QuestInfo, TextInfo};
use auberean_script::value::{tag, NULL};

const QUEST_INFO: u32 = 0xA0;
const QUEST_INFO_LIST: u32 = 0xA1;

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (QUEST_INFO_LIST, 0x07, list_dump),
    (QUEST_INFO, 0x01, quest_id),
    (QUEST_INFO, 0x02, quest_name),
    (QUEST_INFO, 0x03, quest_journal_entry),
    (QUEST_INFO, 0x04, quest_dump),
    (QUEST_INFO, 0x05, quest_init),
    (QUEST_INFO, 0x06, quest_status),
    (QUEST_INFO, 0x07, quest_description),
    (QUEST_INFO, 0x08, quest_plays_fx),
    (QUEST_INFO_LIST, 0x01, list_push),
    (QUEST_INFO_LIST, 0x02, list_update),
    (QUEST_INFO_LIST, 0x03, list_find),
    (QUEST_INFO_LIST, 0x04, list_erase),
    (QUEST_INFO_LIST, 0x05, list_pop),
    (QUEST_INFO_LIST, 0x06, list_size),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The record a reference names (a crash for the null reference as the receiver; a stop for the
/// null reference as an argument, which the client reads through).
fn record<'a>(c: &'a Call<'_>, r: u32) -> Result<&'a QuestInfo, Stop> {
    match c.vm.heap.native(r)? {
        NativeObject::Quest(q) => Ok(&**q),
        _ => Err(c.fail("not a quest record")),
    }
}

fn record_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut QuestInfo, Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::Quest(q) => Ok(&mut **q),
        _ => Err(Stop::Native {
            native,
            what: "not a quest record".into(),
        }),
    }
}

fn list_mut<'a>(c: &'a mut Call<'_>, r: u32) -> Result<&'a mut Vec<QuestInfo>, Stop> {
    let native = c.native;
    match c.vm.heap.native_mut(r)? {
        NativeObject::QuestList(l) => Ok(l),
        _ => Err(Stop::Native {
            native,
            what: "not a quest list".into(),
        }),
    }
}

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

/// A word of the receiver's record, into the result slot.
fn word(c: &mut Call<'_>, f: fn(&QuestInfo) -> u32) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let v = f(record(c, r)?);
    c.write(-2, v, tag::MIXED)?;
    Ok(Status::Success)
}

/// A string reference of the receiver's record: a new one holding a copy.
fn text(c: &mut Call<'_>, f: fn(&QuestInfo) -> &TextInfo) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let t = f(record(c, r)?).clone();
    let id =
        c.vm.heap
            .add_native(native_type::STRING_INFO, NativeObject::TextInfo(t));
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A quest's id.
// D2-01574, D2-16069
fn quest_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |q| q.id)
}

/// A quest's status.
// D2-01616, D2-12245
fn quest_status(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |q| q.status)
}

/// Whether an update of the quest plays its effect.
// D2-01617, D2-12249
fn quest_plays_fx(c: &mut Call<'_>) -> Result<Status, Stop> {
    word(c, |q| q.plays_fx)
}

/// A quest's name.
// D2-01621, D2-12665
fn quest_name(c: &mut Call<'_>) -> Result<Status, Stop> {
    text(c, |q| &q.name)
}

/// A quest's description.
// D2-01620, D2-00088
fn quest_description(c: &mut Call<'_>) -> Result<Status, Stop> {
    text(c, |q| &q.description)
}

/// The journal's entry for the phase a quest is at.
// D2-01619, D2-00092
fn quest_journal_entry(c: &mut Call<'_>) -> Result<Status, Stop> {
    text(c, |q| &q.journal)
}

/// A string reference argument, as a copy of its value.
fn text_arg(c: &Call<'_>, k: i64) -> Result<TextInfo, Stop> {
    match c.vm.heap.native(c.cx.at(k)?)? {
        NativeObject::TextInfo(t) => Ok(t.clone()),
        _ => Err(c.fail("an argument is not a string reference")),
    }
}

/// Fill a record whole: its id, name, description, icon, phase, journal entry, status,
/// challenge, when it was given, the maxed-out flag, whether it runs out, the time left, when it
/// was done, the time before a retry, the effect, whether it may be cancelled. Answers nothing.
// D2-01615, D2-12243
fn quest_init(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let q = QuestInfo {
        id: c.cx.at(-2)?,
        name: text_arg(c, -3)?,
        description: text_arg(c, -4)?,
        icon: c.cx.at(-5)?,
        phase: c.cx.at(-6)?,
        journal: text_arg(c, -7)?,
        status: c.cx.at(-8)?,
        challenge: c.cx.at(-9)?.cast_signed(),
        bestowed: c.long_at(-11)?,
        maxed_out: c.cx.at(-12)?,
        expires: c.cx.at(-13)?,
        seconds_remaining: c.long_at(-15)?,
        done: c.long_at(-17)?,
        seconds_until_retry: c.long_at(-19)?,
        plays_fx: c.cx.at(-20)?,
        cancellable: c.cx.at(-21)?,
    };
    *record_mut(c, r)? = q;
    Ok(Status::Success)
}

/// The record an argument slot's reference names, copied.
fn record_arg(c: &Call<'_>, k: i64) -> Result<QuestInfo, Stop> {
    record(c, c.cx.at(k)?).cloned()
}

/// The record an out argument holds: the reference in the slot the out reference names.
fn out_record(c: &mut Call<'_>, k: i64) -> Result<u32, Stop> {
    let out = c.cx.at(k)?;
    let prog = Arc::clone(c.vm.program());
    let (r, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    // The record must be there to be written: the client reads through the reference first.
    record(c, r)?;
    Ok(r)
}

/// Add a copy at the end; answers 1.
// D2-01625, D2-01622
fn list_push(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let q = record_arg(c, -2)?;
    list_mut(c, r)?.push(q);
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Update a quest: copy the record over the first with its id, or add it in its place by when it
/// was given. The out argument says whether it was new; answers 1.
// D2-01628, D2-12264, D2-12262, D2-12259, D2-12255, D2-01622
fn list_update(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let q = record_arg(c, -2)?;
    let out = c.cx.at(-3)?;
    let prog = Arc::clone(c.vm.program());
    c.vm.write_slot(c.host, c.cx, out, 0, None, tag::MIXED, &prog)?;
    let list = list_mut(c, r)?;
    let new = if let Some(at) = list.iter().position(|x| x.id == q.id) {
        list[at] = q;
        false
    } else {
        // Before the first record given strictly earlier (an unordered time is never earlier).
        let t = q.bestowed_at();
        let at = list
            .iter()
            .position(|x| t > x.bestowed_at())
            .unwrap_or(list.len());
        list.insert(at, q);
        true
    };
    c.vm.write_data(c.host, c.cx, out, 0, u32::from(new), &prog)?;
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Find a quest by id: copied into the out argument's record when there; answers 1.
// D2-01627, D2-12260
fn list_find(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let id = c.cx.at(-2)?;
    let into = out_record(c, -3)?;
    let found = list_mut(c, r)?.iter().find(|x| x.id == id).cloned();
    if let Some(q) = found {
        *record_mut(c, into)? = q;
    }
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Erase a quest: the last record with the argument's id; answers whether there was one.
// D2-01623, D2-12266, D2-12263, D2-12261
fn list_erase(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let id = record(c, c.cx.at(-2)?)?.id;
    let list = list_mut(c, r)?;
    let erased = match list.iter().rposition(|x| x.id == id) {
        Some(at) => {
            list.remove(at);
            1
        }
        None => 0,
    };
    c.write(-3, erased, tag::MIXED)?;
    Ok(Status::Success)
}

/// Take the first record: copied into the out argument's record and removed; answers whether
/// there was one.
// D2-01629, D2-01624
fn list_pop(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    let into = out_record(c, -2)?;
    let list = list_mut(c, r)?;
    let first = (!list.is_empty()).then(|| list.remove(0));
    let had = u32::from(first.is_some());
    if let Some(q) = first {
        *record_mut(c, into)? = q;
    }
    c.write(-3, had, tag::MIXED)?;
    Ok(Status::Success)
}

/// How many records the list holds.
// D2-08629
fn list_size(c: &mut Call<'_>) -> Result<Status, Stop> {
    with_receiver!(c, r);
    #[allow(clippy::cast_possible_truncation)]
    let n = list_mut(c, r)?.len() as u32;
    c.write(-2, n, tag::MIXED)?;
    Ok(Status::Success)
}

/// A quest record as text: its id, status and phase, then its challenge's line (whose format the
/// client leaves without a value, so it shows none), when it was given; under way, the time left
/// and its cancel line (whose format likewise shows a letter in place of a value) when it runs out;
/// else when it was done, the time before it may be taken again, and whether it may be taken no
/// more.
// D2-12258
#[must_use]
pub fn quest_text(q: &QuestInfo) -> String {
    const UNDER_WAY: u32 = 0x1000_0000;
    let time = |bits: u64| crate::text::fixed(f64::from_bits(bits));
    let mut s = format!(
        "\n-- quest contains: --\n  QuestID: 0x{:08X}.\n  Quest Status: 0x{:08X}.\n  \
         Current Phase: {}.\n  Challenge Level: .\n  Bestowal Timestamp: {}.\n",
        q.id,
        q.status,
        q.phase,
        time(q.bestowed)
    );
    if q.status == UNDER_WAY {
        if q.expires != 0 {
            s.push_str(&format!(
                "    Seconds remaining: {}.\n    Can Be Cancelled: b.\n",
                time(q.seconds_remaining)
            ));
        }
    } else {
        s.push_str(&format!(
            "  Finished Timestamp: {}.\n  Seconds until retry: {}.\n",
            time(q.done),
            time(q.seconds_until_retry)
        ));
        if q.maxed_out != 0 {
            s.push_str("  Quest is maxed out.\n");
        }
    }
    s.push_str("-----------------------\n");
    s
}

/// A quest record as text, as a new string at `fp[-2]`.
// D2-01618
fn quest_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let NativeObject::Quest(q) = c.vm.heap.native(r)? else {
        return Err(Stop::Host("not a quest".into()));
    };
    let text = quest_text(q);
    let id = crate::machine::new_text_in(c, text.encode_utf16().collect());
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// A list of quest records as text, as a new string at `fp[-2]`: each element numbered from 1.
// D2-01626, D2-12278
fn list_dump(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    let mut text = String::from("-- quest list elements: --\n");
    if let NativeObject::QuestList(list) = c.vm.heap.native(r)? {
        for (i, q) in list.iter().enumerate() {
            text.push_str(&format!("  Element {}) {}\n", i + 1, quest_text(q)));
        }
    }
    text.push_str("-- end of the quest list elements --\n");
    let id = crate::machine::new_text_in(c, text.encode_utf16().collect());
    c.write(-2, id, tag::REFERENCE)?;
    Ok(Status::Success)
}
