//! The records a saved state needs: the ordered set of record ids scripts fill when the client
//! asks a saved state which records go with it, and the natives that fill it.
//!
//! When the client loads a saved script state through its data cache (an object's state as the
//! object is made, and every state that one names in turn), it makes a process from the state and,
//! when the state's package is a preprocessed one (every entity and every singleton is), calls the
//! preprocessing interface's first function with the type of record asked about and a new set; the
//! ids the scripts add are fetched along with the state.
//!
//! - **The set** holds each id once, with its type, in id order; adding id 0 adds nothing; an id
//!   added without a type takes the type its id range divines. It has no packed form: no saved
//!   state and no call across the connection holds one.
//! - **A string reference** adds its table, then each nested reference's, the same way; a literal
//!   adds nothing.
//! - **A scene** adds each keyframe's: its video, audio and voice records (unless the type asked
//!   about is a record type the keyframes' tracks are never fetched for), then its subtitle's.
//! - **A race and sex's look** adds its body, every value of its appearance table, its pile's
//!   look and its music.
//!
//! [`StateNamer`] answers the data cache with this, on the client's own machine: each saved state
//! the cache loads is asked as it was loaded, and the ids it names are asked for in turn.

use std::cell::RefCell;

use auberean_assets::requests::StateRecords;
use auberean_assets::script::ScriptState;
use auberean_dat::QualifiedIds;
use auberean_script::graph::SavedState;
use auberean_script::heap::Heap;
use auberean_script::native::{Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{Engine, EngineValue, NativeObject};
use auberean_script::value::{tag, FunctionId, NULL};
use auberean_script::{Host, Vm};

/// The native type of the set of record ids.
pub const DATA_ID_ARRAY: u32 = 0x5E;
/// The preprocessing interface's package; its function 1 asks for the records.
const PREPROCESS: u32 = 0x31;
const GET_SUB_DATA_IDS: u32 = 0x31_0001;
/// The string reference and a race and sex's look.
const STRING_INFO: u32 = 0x6A;
const RACE_SEX_INFO: u32 = 0xA2;
/// The record type for which a keyframe adds no tracks.
const NO_TRACKS: u32 = 0x4000_0008;

/// A set of record ids, as the engine keeps it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DataIds(pub QualifiedIds);

impl EngineValue for DataIds {
    fn native_type(&self) -> u32 {
        DATA_ID_ARRAY
    }

    fn clone_value(&self) -> Box<dyn EngineValue> {
        Box::new(self.clone())
    }

    fn same(&self, other: &dyn EngineValue) -> bool {
        other.as_any().downcast_ref::<Self>() == Some(self)
    }

    /// No packed form: the factory refuses to pack one.
    fn pack(&self) -> Option<Vec<u8>> {
        None
    }

    fn as_any(&self) -> &dyn core::any::Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn core::any::Any {
        self
    }
}

/// This module's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (DATA_ID_ARRAY, 0x01, add_to_end),
    (STRING_INFO, 0x1C, string_info_ids),
    (RACE_SEX_INFO, 0x03, race_sex_body),
    (RACE_SEX_INFO, 0x08, race_sex_ids),
];

/// Bind this module's natives.
pub fn bind(t: &mut NativeTable) {
    crate::bind_table(t, NATIVES);
}

/// The set a reference names, made the engine's own the first time a native touches it (a new
/// one is empty).
// D2-04601
pub fn ids_mut(heap: &mut Heap, native: u32, r: u32) -> Result<&mut QualifiedIds, Stop> {
    let o = heap.native_mut(r)?;
    if matches!(o, NativeObject::Opaque { ty } if *ty == DATA_ID_ARRAY) {
        *o = NativeObject::Engine(Engine::new(DataIds::default()));
    }
    match o {
        NativeObject::Engine(e) => e.get_mut::<DataIds>().map(|d| &mut d.0),
        _ => None,
    }
    .ok_or_else(|| Stop::Native {
        native,
        what: "not a set of record ids".into(),
    })
}

/// The set the by-reference argument at `fp[k]` holds.
pub fn out_ids<'a>(c: &'a mut Call<'_>, k: i64) -> Result<&'a mut QualifiedIds, Stop> {
    let at = c.cx.at(k)?;
    let prog = std::sync::Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, at, 0, &prog)?;
    if target == NULL {
        return Err(Stop::NullReference);
    }
    let native = c.native;
    ids_mut(&mut c.vm.heap, native, target)
}

/// Add the id at `fp[-2]`, its type divined; whether it was added (not 0, not held) at `fp[-3]`.
/// A null receiver crashes the call.
// D2-08916, D2-12838
fn add_to_end(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let id = c.cx.at(-2)?;
    let native = c.native;
    let added = ids_mut(&mut c.vm.heap, native, r)?.add(id);
    c.write(-3, u32::from(added), tag::MIXED)?;
    Ok(Status::Success)
}

/// The tables the receiving string reference reads, into the set the reference at `fp[-3]`
/// holds (the type asked about, `fp[-2]`, changes nothing). A null receiver crashes the call.
// D2-09001, D2-14139
fn string_info_ids(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let info = match c.vm.heap.native(r)? {
        NativeObject::TextInfo(t) => crate::text::to_formatter(t),
        _ => return Err(c.fail("the receiver is not a string reference")),
    };
    info.sub_data_ids(out_ids(c, -3)?);
    Ok(Status::Success)
}

/// A race and sex's look as packed: its body, its pile's look and its music, then its appearance
/// table (a header word, then key and record pairs).
fn race_sex_records(bytes: &[u8]) -> Option<Vec<u32>> {
    let word = |i: usize| -> Option<u32> {
        Some(u32::from_le_bytes(
            bytes.get(4 * i..4 * i + 4)?.try_into().ok()?,
        ))
    };
    let n = (word(3)? & 0x00FF_FFFF) as usize;
    let mut ids = vec![word(0)?];
    for k in 0..n {
        ids.push(word(5 + 2 * k)?);
    }
    ids.extend([word(1)?, word(2)?]);
    Some(ids)
}

/// The receiving race and sex's body (the entity its figure is made from, its look's first word),
/// into the result slot. A null receiver crashes the call.
// D2-18959
fn race_sex_body(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let body = match c.vm.heap.native(r)? {
        NativeObject::Packed { ty, bytes } if *ty == RACE_SEX_INFO => race_sex_records(bytes),
        _ => None,
    }
    .and_then(|ids| ids.first().copied())
    .ok_or_else(|| c.fail("the receiver is not a race and sex's look"))?;
    c.write(-2, body, tag::MIXED)?;
    Ok(Status::Success)
}

/// The records of the receiving race and sex's look, into the set the reference at `fp[-3]`
/// holds: its body, every record of its appearance table, its pile's look and its music. A null
/// receiver crashes the call.
// D2-01632, D2-11936
fn race_sex_ids(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let ids = match c.vm.heap.native(r)? {
        NativeObject::Packed { ty, bytes } if *ty == RACE_SEX_INFO => race_sex_records(bytes),
        _ => None,
    }
    .ok_or_else(|| c.fail("the receiver is not a race and sex's look"))?;
    let out = out_ids(c, -3)?;
    for id in ids {
        out.add(id);
    }
    Ok(Status::Success)
}

/// Whether a keyframe asked about records of type `kind` adds its tracks.
#[must_use]
pub fn tracks_asked(kind: u32) -> bool {
    kind != NO_TRACKS
}

/// The records a saved state names, asked about records of type `kind`: a process made from the
/// state, the preprocessing interface's first function called on it with a new set when its
/// package is a preprocessed one, and the process ended. `None` when the package is not one (the
/// set then stays empty in the client); an error when the state is not in the data or its call
/// stops.
pub fn state_records(
    vm: &mut Vm,
    host: &mut dyn Host,
    state: u32,
    kind: u32,
) -> Result<Option<QualifiedIds>, Stop> {
    let s = host.saved_state(state).ok_or(Stop::BadState {
        state,
        what: "not in the data".into(),
    })?;
    let (set, stopped) = saved_state_records(vm, host, &s, state, kind)?;
    match stopped {
        Some(e) => Err(e),
        None => Ok(set),
    }
}

/// [`state_records`] for state `state` already read, as `s`: the set (`None` for a package not
/// preprocessed), and why the scripts' answer stopped, when it did (the set then holds what they
/// had added). An error when the state makes no process.
// D2-16781, D2-08605
pub fn saved_state_records(
    vm: &mut Vm,
    host: &mut dyn Host,
    s: &SavedState,
    state: u32,
    kind: u32,
) -> Result<(Option<QualifiedIds>, Option<Stop>), Stop> {
    let prog = std::sync::Arc::clone(vm.program());
    let instance = vm.instance_from_saved(s, state, 0)?;
    let p = vm.create_process(instance);
    let package = vm.package_of(host, instance, &prog)?;
    if !prog.library.derives_from(package, PREPROCESS) {
        vm.delete_process(p);
        return Ok((None, None));
    }
    let set = vm.heap.add_native(
        DATA_ID_ARRAY,
        NativeObject::Engine(Engine::new(DataIds::default())),
    );
    let mut cx = vm.new_context(p);
    // The set in a slot of the caller's own, the result, a reference to that slot, the type, the
    // receiver.
    cx.push(set, tag::REFERENCE)?;
    cx.push(0, tag::INT)?;
    let local = vm.heap.add_relative(NULL, 0);
    cx.push(local, tag::REFERENCE)?;
    cx.push(kind, tag::MIXED)?;
    cx.push(instance, tag::REFERENCE)?;
    let status = vm.call(host, &mut cx, package, FunctionId(GET_SUB_DATA_IDS));
    vm.delete_process(p);
    let held = cx.slot(0).map_or(set, |(v, _)| v);
    let native = 0;
    let ids = ids_mut(&mut vm.heap, native, held)?.clone();
    Ok((Some(ids), status.err()))
}

/// How the saved states a [`StateNamer`] was given were asked, counted: the states asked, those
/// whose scripts' answer ran to its end, the ids they named, and why the others stopped.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Named {
    pub asked: u64,
    pub completed: u64,
    pub records: u64,
    pub stopped: std::collections::BTreeMap<String, u64>,
}

impl Named {
    /// Add another count to this one.
    pub fn add(&mut self, other: &Self) {
        self.asked += other.asked;
        self.completed += other.completed;
        self.records += other.records;
        for (why, n) in &other.stopped {
            *self.stopped.entry(why.clone()).or_default() += n;
        }
    }
}

/// The data cache's answer for a saved state's records, from a machine and its host: the state
/// as the cache loaded it, read against the machine's library, and asked as the client asks
/// ([`saved_state_records`]). A state that does not read, makes no process or is not preprocessed
/// names none; one whose answer stops names what it had added.
pub struct StateNamer<'a> {
    on: RefCell<(&'a mut Vm, &'a mut dyn Host)>,
    named: RefCell<Named>,
}

impl<'a> StateNamer<'a> {
    #[must_use]
    pub fn new(vm: &'a mut Vm, host: &'a mut dyn Host) -> Self {
        Self {
            on: RefCell::new((vm, host)),
            named: RefCell::new(Named::default()),
        }
    }

    /// How the states it was given were asked.
    #[must_use]
    pub fn named(&self) -> Named {
        self.named.borrow().clone()
    }
}

impl core::fmt::Debug for StateNamer<'_> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("StateNamer").finish_non_exhaustive()
    }
}

impl StateRecords for StateNamer<'_> {
    fn state_records(&self, id: u32, state: &ScriptState, gather: u32, out: &mut QualifiedIds) {
        let Ok(mut on) = self.on.try_borrow_mut() else {
            return;
        };
        let (vm, host) = &mut *on;
        let prog = std::sync::Arc::clone(vm.program());
        let mut named = self.named.borrow_mut();
        let stopped = match SavedState::decode(&state.graph, state.package, &prog.library) {
            Ok(s) => match saved_state_records(vm, &mut **host, &s, id, gather) {
                Ok((Some(set), stopped)) => {
                    named.asked += 1;
                    named.records += set.len() as u64;
                    for q in set.iter() {
                        out.add_typed(q.id, q.kind);
                    }
                    stopped.map(|e| e.to_string())
                }
                Ok((None, _)) => return,
                Err(e) => {
                    named.asked += 1;
                    Some(e.to_string())
                }
            },
            Err(e) => {
                named.asked += 1;
                Some(format!("the state does not read: {e}"))
            }
        };
        match stopped {
            Some(why) => *named.stopped.entry(why).or_default() += 1,
            None => named.completed += 1,
        }
    }
}
