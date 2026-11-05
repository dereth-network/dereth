//! The client's script host: the script machine, the engine objects' instances, and the natives
//! that reach the client's model.
//!
//! A call from the server runs on the instance of the engine object it is for (the player's, or
//! the object a cell or visual call names), made the first time a call reaches the object, from the
//! object's entity's saved state, as the engine makes it when it creates the object. The host
//! keeps which instance is which object's, so a native given an instance finds the object.
//!
//! The model's natives, ported here (the rest are the shared natives of `auberean-glue`):
//!
//! - **the object's qualities**: an object's own value of a quality, else its qualities record's,
//!   else the default the script gives; whether one is set; setting one on the client's copy; the
//!   object's id;
//! - **the interface**: whether an object is the player; the instance of an object by id; the
//!   player and its instance;
//! - **the physics object**: freezing an object's movement and animation; effects played on an
//!   object, kept for a front end to show;
//! - **notices to the interface's panels**: every panel notice, read as its own native reads it
//!   ([`crate::notices`]: its words, and whether the panels of the object it names or every panel
//!   hear it), recorded for the interface; it answers 1, as the client's does;
//! - **files** ([`crate::files`]): the files scripts write, kept for a front end;
//! - **behaviors**: an object asked to play or stop a behavior, kept for a front end (it
//!   answers 1, or 8 for an object that is not here).

use std::any::Any;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use auberean_assets::names::FileNames;
use auberean_assets::script::Qualities as Record;
use auberean_client_model::Model;
use auberean_dat::{DatFile, MasterMap};
use auberean_script::bridge::{object_process, receive, Outcome};
use auberean_script::graph::SavedState;
use auberean_script::library::Library;
use auberean_script::local::{bare_name, LocalSystem, LocalTime};
use auberean_script::native::{Binding, Call, NativeFn, NativeTable, Status, Stop};
use auberean_script::native_object::{native_type, Item, NativeObject, QuestInfo, TextInfo};
use auberean_script::opcode;
use auberean_script::options::{GameplayOptions, Shortcut};
use auberean_script::states::StateCache;
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Host, Program, Vm};

use crate::description;
use crate::repository::{find, write_long};

/// Steps one call may take.
pub const BUDGET: u64 = 2_000_000;

/// An effect a script played on an object: the object, the effect's number in the object's
/// effect table, and its intensity.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Effect {
    pub object: u64,
    pub effect: u32,
    pub intensity: f32,
}

/// A behavior a script asked an object to play, or to stop.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Behavior {
    /// Play one, with its parameters.
    Play {
        object: u64,
        params: auberean_script::native_object::BehaviorParams,
    },
    /// Stop one by its id.
    Stop { object: u64, behavior: u32 },
}

/// A notice a script sent to the interface's panels.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// The notice's native id.
    pub native: u32,
    /// The object it is about.
    pub about: Option<u64>,
}

/// A notice the scripts sent the interface's panels, with its arguments: the native that sent it,
/// the object it is about (its first argument; none for a notice every panel hears, whose first
/// argument is one of its words), and the words of its other arguments in order, as
/// the script machine holds them; for each word that names a native object (an examination
/// profile, a request, a quest), a copy of the object as it was when the notice was sent; for an
/// examination request, the whole request as the engine holds it (its packed form leaves out what
/// its kind does not pack, such as a recipe category's number); for an examination profile, the
/// packed profile of each of its effect lines' own lines, by line, as the player's scripts made it
/// when the notice came; for a list of references, a copy of the native object each item names
/// (a catalog's shops).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PanelNotice {
    pub native: u32,
    pub about: Option<u64>,
    pub words: Vec<u32>,
    pub objects: Vec<Option<NativeObject>>,
    pub requests: Vec<Option<crate::examination::Request>>,
    pub effect_lines: Vec<Vec<(usize, Vec<u8>)>>,
    pub listed: Vec<Vec<Option<NativeObject>>>,
}

/// A line the scripts gave the chat windows: the object it is about, the kind of text (the chat
/// windows each show some kinds) and the string reference to write.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub about: Option<u64>,
    pub kind: u32,
    pub text: TextInfo,
    /// The notices the panels had been sent when it was written: panels hear both as they come,
    /// so what a notice changes applies only to the lines after it.
    pub notices_before: usize,
}

/// The engine side of the scripts: the saved states, the model, and what the scripts did to it.
pub struct ClientHost {
    pub states: StateCache,
    pub program: Arc<Program>,
    /// The portal file the qualities records come from.
    pub portal: Arc<DatFile>,
    /// The model, while a call runs (the runtime lends it).
    pub model: Model,
    /// Instance to object, and object to instance.
    pub links: HashMap<u32, u64>,
    pub instances: HashMap<u64, u32>,
    /// What scripts sent to the server: the function, its packed arguments, encrypted or not.
    pub sends: Vec<(u32, Vec<u8>, bool)>,
    /// Notices to the panels, by native, counted.
    pub notices: BTreeMap<u32, u64>,
    /// The notices to the panels themselves, in order, for the interface to take.
    pub panel_notices: Vec<PanelNotice>,
    /// Lines for the chat windows, in the order the scripts gave them, for the interface to take.
    pub shown: Vec<Shown>,
    /// Objects whose movement or animation a script froze.
    pub frozen: HashMap<u64, (bool, bool)>,
    /// What the scripts set on objects' bodies (their collision state, whether they are drawn),
    /// for a front end to apply.
    pub bodies: crate::body::Bodies,
    /// What the bodies say of themselves when the scripts ask, as a front end last read them.
    pub readings: crate::body::Readings,
    /// What the cells the bodies have been in made of them, as a front end last read it.
    pub materials: crate::body::Materials,
    /// What the scripts asked of the objects' bodies, in order, for a front end to carry out.
    pub body_requests: Vec<crate::physics_object::BodyRequest>,
    /// The kinds of log line the client keeps.
    pub logs: crate::log_system::LogMasks,
    /// The objects' own descriptions the scripts changed and their bodies have not taken yet.
    pub looks: crate::looks::Looks,
    /// Effects scripts played on objects, in order, for a front end to show and clear.
    pub effects: Vec<Effect>,
    /// Behaviors scripts asked objects to play or stop, in order, for a front end to show and
    /// clear.
    pub behaviors: Vec<Behavior>,
    /// How often scripts stopped the camera's effects on the player (the camera plays none of its
    /// own yet), for a front end.
    pub camera_fx_stops: u32,
    /// The game clock as scripts read it.
    pub clock: f64,
    /// The world's clock, while a call runs (the runtime lends it).
    pub game_clock: Option<auberean_world::calendar::GameClock>,
    /// Examination requests whole, by reference: the machine holds only their packed forms.
    pub requests: HashMap<u32, crate::examination::Request>,
    /// Selections whole, by reference, likewise.
    pub selections: HashMap<u32, crate::selection::Selection>,
    /// The string tables, the language and the enumeration mappings, from the client's data
    /// files (the portal file first).
    pub text: auberean_text::DatSource,
    /// Objects scripts asked to be destroyed in a while (true) or no longer (false), in order, for
    /// the runtime's event queue.
    pub destruction: Vec<(u64, bool)>,
    /// Where the client writes files a player exports (scripts build file names on it).
    pub output_path: Vec<u16>,
    /// The files scripts wrote (a chat log, an export), for a front end to save.
    pub files: crate::files::Files,
    /// When set, every string reference a script writes, with its text, is kept in `written`.
    pub keep_written: bool,
    pub written: Vec<(TextInfo, Vec<u16>)>,
    /// The commands the scripts made known to the player's command line.
    pub commands: crate::shell::Commands,
    /// The lines typed, newest first, as the runtime last lent them.
    pub history: crate::shell::History,
    /// Calls that began to sleep in the last call, and for how many seconds, for the runtime to
    /// wake.
    pub sleeps: Vec<(crate::latent::Waiting, f64)>,
    /// The friends service's layer, while a call runs (the runtime lends it).
    pub friends: Option<crate::friends::Friends>,
    /// Rooms, while a call runs (the runtime lends them).
    pub rooms: Option<crate::rooms::RoomChat>,
    /// Where the player is (its cell, its origin in the cell's landblock and its rotation, scalar
    /// first), as the runtime last lent it: none before the player has a place.
    pub my_position: Option<(u32, [f32; 3], [f32; 4])>,
    /// The client's random numbers, shared with everything else in the client that draws
    /// ([`crate::Client::set_random`]).
    pub dice: auberean_glue::numbers::Random,
    /// The machine's wall clock and the files in the client's own folder, when the front end
    /// gives them (none: the natives that need them answer as the client does when they fail).
    pub local: Option<Box<dyn LocalSystem + Send>>,
    records: HashMap<u32, Arc<Record>>,
    /// The client's one data cache: the data files' records, decoded once and kept (opened the
    /// first time one is asked for), shared with the data cache's requests.
    pub(crate) cache: Option<crate::cache::DataCache>,
    /// The client's table of every record's file name, which the scripts ask.
    pub file_names: FileNames,
    /// Where the record types' folders would be, for that table.
    master: MasterMap,
}

/// The table as the client starts it with its data cache: folders off, ids not to be made, only
/// read. No install has the file of names, so it stays empty.
fn started_file_names(master: &MasterMap) -> FileNames {
    let mut names = FileNames::new();
    names.set_uses_folders(false);
    names.set_ids_may_be_made(false);
    names.init(4, 0, master);
    names.verify_types();
    names
}

/// The host's files, by bare names only: any other name is refused, as is every file when the
/// front end gave no local system.
pub(crate) struct Checked<'a>(pub(crate) &'a mut Option<Box<dyn LocalSystem + Send>>);

impl LocalSystem for Checked<'_> {
    fn folder(&self) -> Option<String> {
        self.0.as_ref()?.folder()
    }

    fn local_time(&self) -> Option<LocalTime> {
        self.0.as_ref()?.local_time()
    }

    fn real_time(&self) -> Option<i64> {
        self.0.as_ref()?.real_time()
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        bare_name(file) && self.0.as_mut().is_some_and(|l| l.append_line(file, bytes))
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        bare_name(file) && self.0.as_mut().is_some_and(|l| l.write_file(file, bytes))
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        if !bare_name(file) {
            return None;
        }
        self.0.as_mut()?.read_file(file)
    }
}

impl core::fmt::Debug for ClientHost {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ClientHost")
            .field("links", &self.links.len())
            .finish_non_exhaustive()
    }
}

impl Host for ClientHost {
    fn saved_state(&mut self, id: u32) -> Option<Arc<SavedState>> {
        self.states.get(id, &self.program.library)
    }

    fn send(&mut self, function: u32, args: &[u8], encrypted: bool) -> Result<(), Stop> {
        self.sends.push((function, args.to_vec(), encrypted));
        Ok(())
    }

    fn time(&mut self) -> f64 {
        self.clock
    }

    fn random(&mut self) -> Result<f64, Stop> {
        Ok(self.dice.draw())
    }

    // D2-14094
    fn string_exists(&mut self, table: u32, string: u32) -> Result<bool, Stop> {
        use auberean_text::Source as _;
        Ok(self
            .text
            .table(table)
            .is_some_and(|t| t.entry(string).is_some()))
    }

    fn data_id_by_enum(&mut self, value: u32, mapping: u32) -> u32 {
        self.text.data_id_by_enum(value, mapping)
    }

    fn data_id_by_name(&mut self, name: &str, record_type: u32) -> u32 {
        let db = self
            .cache
            .as_ref()
            .filter(|c| !c.is_empty())
            .map(crate::cache::DataCache::database);
        self.file_names.id(name, record_type, db, &self.master)
    }

    fn file_name_by_id(&mut self, id: u32) -> Vec<u16> {
        let db = self
            .cache
            .as_ref()
            .filter(|c| !c.is_empty())
            .map(crate::cache::DataCache::database);
        self.file_names.file_name_of_wide(id, db, &self.master)
    }

    fn text(&mut self) -> Option<&mut dyn auberean_text::Source> {
        Some(&mut self.text)
    }

    fn database(&mut self) -> Option<&auberean_assets::Database> {
        self.data_cache().map(crate::cache::DataCache::database)
    }

    fn wrote_text(&mut self, info: &TextInfo, text: &[u16]) {
        if self.keep_written {
            self.written.push((info.clone(), text.to_vec()));
        }
    }

    fn local_time(&self) -> Option<LocalTime> {
        self.local.as_ref()?.local_time()
    }

    fn real_time(&self) -> Option<i64> {
        self.local.as_ref()?.real_time()
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        Checked(&mut self.local).append_line(file, bytes)
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        Checked(&mut self.local).write_file(file, bytes)
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        Checked(&mut self.local).read_file(file)
    }

    fn as_any(&mut self) -> &mut dyn Any {
        self
    }
}

impl ClientHost {
    /// Keep the front end's clock and files (none: the natives that need them fail as the
    /// client's do). The folder the files are kept in becomes the path scripts build file names on
    /// and show a player, as the client shows its own folder.
    pub fn set_local(&mut self, local: Option<Box<dyn LocalSystem + Send>>) {
        self.output_path = local
            .as_ref()
            .and_then(|l| l.folder())
            .map(|f| f.encode_utf16().collect())
            .unwrap_or_default();
        self.local = local;
    }

    /// Look text and records up in these data files (the portal file first), through a data
    /// cache of the host's own.
    pub fn set_data_files(&mut self, files: &[Arc<DatFile>]) {
        let clock = Arc::new(std::sync::atomic::AtomicU32::new(0));
        self.set_data_cache(crate::cache::DataCache::new(files.to_vec(), clock));
    }

    /// Look text and records up through the client's data cache (shared with its requests).
    pub fn set_data_cache(&mut self, cache: crate::cache::DataCache) {
        self.text = cache.text_source();
        self.cache = Some(cache);
    }

    /// The data cache, when the host has data files.
    pub(crate) fn data_cache(&self) -> Option<&crate::cache::DataCache> {
        self.cache.as_ref().filter(|c| !c.is_empty())
    }

    /// The qualities record with this id: an object's defaults, the record its own qualities
    /// derive from. It is read through the data cache, so one the server sent is read as the
    /// files' own are. A record is kept once read and a missing one is looked for again next time:
    /// it may come. A record the portal file holds that does not read is read again first.
    // D2-07253, D2-12600
    pub(crate) fn record(&mut self, id: u32) -> Option<Arc<Record>> {
        if id == 0 {
            return None;
        }
        if let Some(r) = self.records.get(&id) {
            return Some(r.clone());
        }
        let read = || {
            let bytes = match self.data_cache() {
                Some(c) => c.database().read(id),
                None => self.portal.read(id).ok(),
            };
            bytes.and_then(|b| {
                auberean_assets::decode::<Record>(id, &b, &auberean_assets::Context::empty()).ok()
            })
        };
        let held = self.portal.entry(id).is_some();
        let mut r = read();
        for _ in 1..RECORD_TRIES {
            if r.is_some() || !held {
                break;
            }
            r = read();
        }
        let r = r.map(Arc::new);
        if let Some(r) = &r {
            self.records.insert(id, r.clone());
        }
        r
    }

    /// A quality of the qualities record an object's own qualities derive from: the record's
    /// own description first, then its tables.
    fn base_quality<V>(
        &mut self,
        id: u64,
        desc: impl Fn(&auberean_protocol::types::WeenieDesc) -> Option<V>,
        table: impl Fn(&Record) -> Option<V>,
    ) -> Option<V> {
        let r = self.record(self.base_of(id))?;
        r.description
            .as_ref()
            .map(description::of_record)
            .and_then(|w| desc(&w))
            .or_else(|| table(&r))
    }

    /// An object's whole-number quality as the scripts read it: its description's value (for the
    /// qualities it holds), else its own, else its record's.
    pub fn int_quality(&mut self, id: u64, key: u32) -> Option<i32> {
        let o = self.model.object(id)?;
        if let Some(v) = description::int(&o.weenie, key) {
            return Some(v);
        }
        if let Some(v) = o.qualities.ints.get(&key).copied() {
            return Some(v);
        }
        self.base_quality(id, |w| description::int(w, key), |r| find(&r.ints, key))
    }

    /// The qualities record an object's own qualities derive from.
    pub(crate) fn base_of(&self, id: u64) -> u32 {
        self.model.object(id).map_or(0, qualities_record)
    }
}

/// The qualities record object `o`'s own qualities derive from: the one its qualities name, else
/// the one its entity template's number names; with no template, its package's (the low half of
/// the package's number; an object with no package names `0x8000_FFFF`); a description naming
/// something other than a template names none (0).
// D2-08216, D2-07250
#[must_use]
pub fn qualities_record(o: &auberean_client_model::Object) -> u32 {
    if o.qualities.base != 0 {
        return o.qualities.base;
    }
    let w = &o.weenie;
    let entity = w.entity_did;
    if entity == 0 {
        0x8000_0000 | (w.package_id & 0xFFFF)
    } else if entity & 0xFF00_0000 == 0x4700_0000 {
        0x8100_0000 | (entity & 0x00FF_FFFF)
    } else {
        0
    }
}

/// Read the code for every native call's argument slots: the call is followed by the pop of its
/// arguments and receiver.
#[must_use]
pub fn argument_slots(lib: &Library) -> HashMap<u32, i64> {
    let code = &lib.code;
    let mut out = HashMap::new();
    let mut at = 0;
    while at < code.len() {
        let Some(len) = opcode::length(code, at) else {
            break;
        };
        if opcode::low_byte(code[at]) == opcode::op::CALL_NATIVE {
            let id = code.get(at + 1).copied().unwrap_or(0);
            if let Some(next) = code.get(at + len) {
                if opcode::low_byte(*next) == opcode::op::POP_MANY {
                    let n = i64::from(opcode::unsigned(*next)) - 1;
                    out.entry(id).or_insert(n.max(0));
                }
            }
        }
        at += len;
    }
    out
}

pub(crate) fn host<'a>(c: &'a mut Call<'_>) -> Result<&'a mut ClientHost, Stop> {
    c.host
        .as_any()
        .downcast_mut::<ClientHost>()
        .ok_or_else(|| Stop::Host("not the client's host".into()))
}

/// The engine object behind a script object.
fn object_of(c: &mut Call<'_>, r: u32) -> Result<u64, Stop> {
    if r == NULL {
        return Err(Stop::NullReference);
    }
    let native = c.native;
    host(c)?.links.get(&r).copied().ok_or_else(|| Stop::Native {
        native,
        what: format!("{r:#x} is not an engine object's"),
    })
}

/// The receiver's engine object.
pub(crate) fn receiver_object(c: &mut Call<'_>) -> Result<u64, Stop> {
    let r = c.receiver()?;
    object_of(c, r)
}

/// The object (and the objects it carries) to be destroyed in a while unless that is called off:
/// not the player (answers 0), else 1.
// D2-04653, D2-08025, D2-05633
fn add_object_to_be_destroyed(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let h = host(c)?;
    let ok = if h.model.player == Some(id) {
        false
    } else {
        h.destruction.push((id, true));
        for child in h.model.children(id) {
            h.destruction.push((child, true));
        }
        true
    };
    c.write(-2, u32::from(ok), tag::MIXED)?;
    Ok(Status::Success)
}

/// Call off the object's (and the objects it carries) destruction; answers 1.
// D2-04654, D2-08026, D2-05634
fn remove_object_to_be_destroyed(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let h = host(c)?;
    h.destruction.push((id, false));
    for child in h.model.children(id) {
        h.destruction.push((child, false));
    }
    c.write(-2, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The string an out reference (`fp[-2]`) holds: set to where the client writes exported files;
/// answers 1.
// D2-04675, D2-08086
fn get_output_files_path(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let out = c.cx.at(-2)?;
    let prog = Arc::clone(c.vm.program());
    let (target, _) = c.vm.read_slot(c.host, c.cx, out, 0, &prog)?;
    if target == NULL {
        return Ok(Status::Crash);
    }
    let path = host(c)?.output_path.clone();
    match c.vm.heap.native_mut(target)? {
        NativeObject::Text(t) => *t = path,
        _ => return Err(c.fail("the out reference holds no string")),
    }
    c.write(-3, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// The model's natives: package, number, function.
const NATIVES: &[(u32, u32, NativeFn)] = &[
    (INTERFACE, 0x64, get_output_files_path),
    (BOOK_KEEPER, 0x1E, add_object_to_be_destroyed),
    (BOOK_KEEPER, 0x1F, remove_object_to_be_destroyed),
    (INTERFACE, 0x0A, weenie_object),
    (INTERFACE, 0x0B, weenie_object),
    (INTERFACE, 0x40, is_the_player),
    (INTERFACE, 0x42, my_position),
    (DISTRIBUTOR, 0x02, my_package),
    (DISTRIBUTOR, 0x03, my_entity),
    (OBJECT_MAINTENANCE, 0x01, player_id),
    (OBJECT_MAINTENANCE, 0x02, player_reference),
    (STRING_INFO, 0x0A, set_string_id),
    (COMMUNICATION_NOTICES, 0x01, display_string_info),
    (EXAMINATION_PROFILE, 0x04, examination_request),
    (EXAMINATION_REQUEST, 0x05, examined_object),
    (PHYSICS_OBJECT, 0x45, do_fx),
    (PHYSICS_OBJECT, 0x2C, stop_all_camera_fx),
    (PHYSICS_OBJECT, 0x1C, do_behavior_non_latent),
    (PHYSICS_OBJECT, 0x27, stop_behavior),
    (PHYSICS_OBJECT, 0x54, set_movement_frozen),
    (PHYSICS_OBJECT, 0x56, set_animation_frozen),
];

/// How many times a qualities record that does not read is tried in all.
const RECORD_TRIES: usize = 3;

const INTERFACE: u32 = 0x7F;
const PHYSICS_OBJECT: u32 = 0x80;
const OBJECT_MAINTENANCE: u32 = 0x95;
const BOOK_KEEPER: u32 = 0x7D;
const DISTRIBUTOR: u32 = 0x7E;
const STRING_INFO: u32 = 0x6A;
const COMMUNICATION_NOTICES: u32 = 0x87;
const EXAMINATION_PROFILE: u32 = 0x99;
const EXAMINATION_REQUEST: u32 = 0x9A;

/// The shared natives with the model's bound over them (every panel notice among them).
#[must_use]
pub fn natives(lib: &Library) -> NativeTable {
    model_natives(lib)
}

/// The shared natives with the model's own bound over them: every notice to the panels, read as
/// its own native reads it ([`crate::notices`]), then the model's natives.
#[must_use]
pub fn model_natives(lib: &Library) -> NativeTable {
    let mut t = auberean_glue::natives(lib);
    for n in crate::notices::NOTICES {
        t.bind(NativeId(n.native), Binding::Ported(panel_notice));
    }
    for (package, number, f) in NATIVES {
        t.bind(NativeId::new(*package, *number), Binding::Ported(*f));
    }
    for (id, f) in crate::examination::natives()
        .chain(crate::files::natives())
        .chain(crate::friends::natives())
        .chain(crate::rooms::natives())
    {
        t.bind(id, Binding::Ported(f));
    }
    for (id, f) in crate::selection::natives()
        .chain(crate::repository::natives())
        .chain(crate::quality_dump::natives())
        .chain(crate::game_time::natives())
        .chain(crate::shell::natives())
        .chain(crate::latent::natives())
        .chain(crate::combat::natives())
        .chain(crate::body::natives())
        .chain(crate::physics_object::natives())
        .chain(crate::log_system::natives())
        .chain(crate::looks::natives())
    {
        t.bind(id, Binding::Ported(f));
    }
    t
}

/// Whether a native is a notice to the interface's panels: a function named `SendNotice_...` of
/// a panel-message package.
#[must_use]
pub fn is_panel_notice(lib: &Library, id: u32) -> bool {
    let name = lib.native_name(id);
    name.starts_with("CM_") && name.contains("::SendNotice_")
}

/// A host over a portal file and a linked program.
#[must_use]
pub fn new_host(portal: Arc<DatFile>, program: Arc<Program>) -> ClientHost {
    let mut master = MasterMap::default();
    master.init();
    ClientHost {
        file_names: started_file_names(&master),
        master,
        states: StateCache::new(Arc::clone(&portal)),
        program,
        portal,
        model: Model::new(),
        links: HashMap::new(),
        instances: HashMap::new(),
        sends: Vec::new(),
        notices: BTreeMap::new(),
        panel_notices: Vec::new(),
        shown: Vec::new(),
        frozen: HashMap::new(),
        bodies: HashMap::new(),
        readings: HashMap::new(),
        materials: HashMap::new(),
        body_requests: Vec::new(),
        logs: crate::log_system::LogMasks::default(),
        looks: HashMap::new(),
        effects: Vec::new(),
        behaviors: Vec::new(),
        camera_fx_stops: 0,
        clock: 0.0,
        game_clock: None,
        requests: HashMap::new(),
        selections: HashMap::new(),
        dice: auberean_glue::numbers::Random::new(),
        text: auberean_text::DatSource::new(Vec::new()),
        destruction: Vec::new(),
        output_path: Vec::new(),
        files: crate::files::Files::default(),
        keep_written: false,
        written: Vec::new(),
        commands: crate::shell::Commands::default(),
        history: crate::shell::History::default(),
        sleeps: Vec::new(),
        friends: None,
        rooms: None,
        my_position: None,
        records: HashMap::new(),
        local: None,
        cache: None,
    }
}

/// How a call went, from what the machine said.
pub(crate) fn how(outcome: Result<Status, Stop>) -> CallResult {
    match outcome {
        Ok(Status::Terminated) => CallResult::Completed,
        Ok(s) => CallResult::Ended(s),
        Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
        Err(Stop::NullReference) => CallResult::NullReference,
        Err(e) => CallResult::Stopped(e.to_string()),
    }
}

/// Call one of the interface's functions on a process's instance, as the client's interface does:
/// a new context of the process, the result slot, the arguments in order, the instance, then the
/// function, looked up in the instance's package. A call left suspended is kept with its process.
/// Returns how the call ended and its result slot.
pub(crate) fn call_on_process(
    vm: &mut Vm,
    host: &mut ClientHost,
    p: usize,
    function: u32,
    args: &[InterfaceArg],
) -> Result<(Status, u32), Stop> {
    let mut cx = vm.new_context(p);
    cx.push(0, tag::INT)?;
    for a in args {
        match a {
            InterfaceArg::Word(v) => cx.push(*v, tag::MIXED)?,
            InterfaceArg::Long(v) => cx.push_long(*v, tag::LONG | tag::MIXED)?,
            InterfaceArg::Ref(v) => cx.push(*v, tag::REFERENCE)?,
            InterfaceArg::Text(units) => {
                let r = vm
                    .heap
                    .add_native(native_type::WP_STRING, NativeObject::Text(units.clone()));
                cx.push(r, tag::REFERENCE)?;
            }
            InterfaceArg::Texts(all) => {
                let mut list = NativeObject::new(&host.program.library, REF_LIST);
                if let NativeObject::Sequence { items, .. } = &mut list {
                    for units in all {
                        let r = vm
                            .heap
                            .add_native(native_type::WP_STRING, NativeObject::Text(units.clone()));
                        items.push(Item::Ref(r));
                    }
                }
                let r = vm.heap.add_native(REF_LIST, list);
                cx.push(r, tag::REFERENCE)?;
            }
            InterfaceArg::Object { ty, object } => {
                let r = vm.heap.add_native(*ty, object.clone());
                cx.push(r, tag::REFERENCE)?;
            }
            InterfaceArg::Objects { ty, objects } => {
                let mut list = NativeObject::new(&host.program.library, REF_LIST);
                if let NativeObject::Sequence { items, .. } = &mut list {
                    for o in objects {
                        items.push(Item::Ref(vm.heap.add_native(*ty, o.clone())));
                    }
                }
                let r = vm.heap.add_native(REF_LIST, list);
                cx.push(r, tag::REFERENCE)?;
            }
            InterfaceArg::Native { ty, bytes } => {
                let lib = &host.program.library;
                let mut at = auberean_script::wire::In::new(bytes);
                let o = NativeObject::read(&mut at, lib, *ty)
                    .map_err(|e| Stop::Host(format!("{ty:#x} does not read: {e:?}")))?;
                let r = vm.heap.add_native(*ty, o.unpacked());
                cx.push(r, tag::REFERENCE)?;
            }
        }
    }
    let receiver = vm.instance_of(p);
    cx.push(receiver, tag::REFERENCE)?;
    let prog = Arc::clone(&host.program);
    let package = vm.package_of(host, receiver, &prog)?;
    let status = vm.call(
        host,
        &mut cx,
        package,
        auberean_script::value::FunctionId(function),
    )?;
    let result = cx.slot(0).map_or(0, |(v, _)| v);
    if status == Status::Interrupted {
        vm.park(cx);
    }
    Ok((status, result))
}

/// How a call from the server went.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CallResult {
    /// It ran to its end.
    Completed,
    /// It ended another way (suspended, crashed in script).
    Ended(Status),
    /// Its arguments did not read.
    Refused(String),
    /// It stopped at a native not written yet.
    NotPorted(u32),
    /// It stopped using the null reference.
    NullReference,
    /// The machine stopped it for another reason.
    Stopped(String),
    /// No instance to run it on (no player, no object).
    NoReceiver,
}

/// The native type of a collision profile: what an object's collisions began and ended.
pub const COLLISION_PROFILE: u32 = 27;
/// The interface's function an object's collisions are handed to.
pub const HANDLE_COLLISION: u32 = (0x7F << 16) | 0x43;

/// One argument the interface hands a function of the player's scripts.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum InterfaceArg {
    /// A 32-bit value.
    Word(u32),
    /// A 64-bit value (an instance id).
    Long(u64),
    /// A script object already in the table.
    Ref(u32),
    /// A new string.
    Text(Vec<u16>),
    /// A new list of new strings.
    Texts(Vec<Vec<u16>>),
    /// A new native object of a type, read from its packed form as the engine holds one (a
    /// shortcut, a table of the windows' places).
    Native { ty: u32, bytes: Vec<u8> },
    /// A new native object of a type, as given.
    Object { ty: u32, object: NativeObject },
    /// A new list of new native objects of a type.
    Objects { ty: u32, objects: Vec<NativeObject> },
}

/// The native type of a list of references.
const REF_LIST: u32 = 0x65;
/// The native type of an array of references.
const SHORTCUT_ARRAY: u32 = 0x61;

/// The scripts: the machine and the host. Dropped, the machine goes with them.
// D2-07979, D2-07985
pub struct Scripts {
    pub vm: Vm,
    pub host: ClientHost,
    processes: HashMap<u64, usize>,
}

impl core::fmt::Debug for Scripts {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Scripts")
            .field("processes", &self.processes.len())
            .finish_non_exhaustive()
    }
}

impl Scripts {
    #[must_use]
    pub fn new(portal: Arc<DatFile>, program: Arc<Program>) -> Self {
        Self {
            vm: Vm::new(Arc::clone(&program)),
            host: new_host(portal, program),
            processes: HashMap::new(),
        }
    }

    /// The character left the world: every instance goes, and the player's command line is made
    /// anew, knowing no command.
    // D2-07983, D2-07984
    pub fn reset(&mut self) {
        let stops = std::mem::take(&mut self.vm.stops);
        self.vm = Vm::new(Arc::clone(&self.host.program));
        self.vm.stops = stops;
        self.processes.clear();
        self.host.links.clear();
        self.host.instances.clear();
        self.host.frozen.clear();
        self.host.bodies.clear();
        self.host.body_requests.clear();
        self.host.requests.clear();
        self.host.selections.clear();
        self.host.effects.clear();
        self.host.looks.clear();
        self.host.commands = crate::shell::Commands::default();
    }

    /// The process of an engine object's instance, when one was made.
    #[must_use]
    pub fn process(&self, id: u64) -> Option<usize> {
        self.processes.get(&id).copied()
    }

    /// The process of an engine object's instance, made the first time.
    pub(crate) fn process_of(&mut self, id: u64) -> Result<usize, Stop> {
        if let Some(p) = self.processes.get(&id) {
            return Ok(*p);
        }
        // An instance the scripts made already, asking for the object by id, keeps its process.
        if let Some(p) = self
            .host
            .instances
            .get(&id)
            .and_then(|i| self.vm.process_with_instance(*i))
        {
            self.processes.insert(id, p);
            return Ok(p);
        }
        let (package, entity) = self
            .host
            .model
            .object(id)
            .map(|o| (o.weenie.package_id, o.weenie.entity_did))
            .ok_or_else(|| Stop::Host(format!("no object {id:#x}")))?;
        let p = object_process(&mut self.vm, &mut self.host, package, entity)?;
        let instance = self.vm.instance_of(p);
        self.processes.insert(id, p);
        self.host.links.insert(instance, id);
        self.host.instances.insert(id, instance);
        Ok(p)
    }

    /// Run a call from the server on an object's instance (the player's when `object` is none or
    /// not here).
    // D2-08038, D2-03978, D2-03979, D2-08039, D2-08031
    pub fn run(&mut self, object: Option<u64>, function: u32, args: &[u8]) -> CallResult {
        let target = object
            .filter(|o| self.host.model.object(*o).is_some())
            .or(self
                .host
                .model
                .player
                .filter(|p| self.host.model.object(*p).is_some()));
        let Some(target) = target else {
            return CallResult::NoReceiver;
        };
        self.vm.budget = BUDGET;
        let outcome = self
            .process_of(target)
            .and_then(|p| receive(&mut self.vm, &mut self.host, p, function, args));
        let result = match outcome {
            Ok(Outcome::Ran(Status::Terminated)) => CallResult::Completed,
            Ok(Outcome::Ran(s)) => CallResult::Ended(s),
            Ok(Outcome::Refused(why)) => CallResult::Refused(why),
            Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
            Err(Stop::NullReference) => CallResult::NullReference,
            Err(e) => CallResult::Stopped(e.to_string()),
        };
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        if self.vm.heap.added_since_collection > 500_000 {
            self.vm.collect();
        }
        result
    }

    /// An object's collisions handed to its scripts, as the client hands them to the object's
    /// interface: a new context of the object's process, the result slot, a new collision profile
    /// (`profile`, its packed form), the object's instance, then the interface's collision function,
    /// looked up in the instance's package.
    // D2-08065, D2-04678
    pub fn handle_collision(&mut self, object: u64, profile: Vec<u8>) -> CallResult {
        if self.host.model.object(object).is_none() {
            return CallResult::NoReceiver;
        }
        self.vm.budget = BUDGET;
        let outcome = self.process_of(object).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            cx.push(0, tag::INT)?;
            let r = self.vm.heap.add_native(
                COLLISION_PROFILE,
                NativeObject::Packed {
                    ty: COLLISION_PROFILE,
                    bytes: profile,
                },
            );
            cx.push(r, tag::REFERENCE)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(HANDLE_COLLISION),
            )?;
            if status == Status::Interrupted {
                self.vm.park(cx);
            }
            Ok(status)
        });
        let r = match outcome {
            Ok(Status::Terminated) => CallResult::Completed,
            Ok(s) => CallResult::Ended(s),
            Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
            Err(Stop::NullReference) => CallResult::NullReference,
            Err(e) => CallResult::Stopped(e.to_string()),
        };
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        r
    }

    /// Call one of the interface's functions on object `object`'s instance, as the client does
    /// when it asks an object's own scripts (see [`call_on_process`]). Returns how it went and the
    /// result slot.
    pub fn call_interface_on(
        &mut self,
        object: u64,
        function: u32,
        args: &[InterfaceArg],
    ) -> (CallResult, u32) {
        if self.host.model.object(object).is_none() {
            return (CallResult::NoReceiver, 0);
        }
        self.vm.budget = BUDGET;
        let mut result = 0;
        let outcome = self.process_of(object).and_then(|p| {
            let (status, r) = call_on_process(&mut self.vm, &mut self.host, p, function, args)?;
            result = r;
            Ok(status)
        });
        let r = how(outcome);
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        (r, result)
    }

    /// Call one of the interface's functions on the player's instance, as the client's interface
    /// does when a panel asks the player's scripts for something (see [`call_on_process`]).
    /// Returns how it went and the result slot. Without a player object standing there is no
    /// instance to call.
    // D2-00347
    pub fn call_interface(&mut self, function: u32, args: &[InterfaceArg]) -> (CallResult, u32) {
        let Some(player) = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())
        else {
            return (CallResult::NoReceiver, 0);
        };
        self.vm.budget = BUDGET;
        let mut result = 0;
        let outcome = self.process_of(player).and_then(|p| {
            let (status, r) = call_on_process(&mut self.vm, &mut self.host, p, function, args)?;
            result = r;
            Ok(status)
        });
        let r = how(outcome);
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        (r, result)
    }

    /// A line typed for the player's command line (see [`crate::shell`]): how it went, and each
    /// interface function it called with how that went.
    pub fn process_text(
        &mut self,
        text: &str,
    ) -> (crate::shell::ShellStatus, Vec<(u32, CallResult)>) {
        let player = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some());
        let process = player.and_then(|p| self.process_of(p).ok());
        let mut t = crate::shell::Typing {
            vm: &mut self.vm,
            host: &mut self.host,
            process,
            nested: false,
            asked: Vec::new(),
        };
        let status = t
            .process_text(text)
            .unwrap_or(crate::shell::ShellStatus::Failure);
        let asked = std::mem::take(&mut t.asked);
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        (status, asked)
    }

    /// Call one of the interface's functions whose result is 64 bits (an instance id) on the
    /// player's instance: two result slots, then the receiver; the result is their two words.
    // D2-00420, D2-01134, D2-00421, D2-01123
    pub fn call_interface_long(&mut self, function: u32) -> (CallResult, u64) {
        let Some(player) = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())
        else {
            return (CallResult::NoReceiver, 0);
        };
        self.vm.budget = BUDGET;
        let mut result = 0;
        let outcome = self.process_of(player).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            cx.push(0, tag::LONG_INT)?;
            cx.push(0, tag::LONG_INT)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(function),
            )?;
            let lo = cx.slot(0).map_or(0, |(v, _)| v);
            let hi = cx.slot(1).map_or(0, |(v, _)| v);
            result = auberean_script::value::join(lo, hi);
            if status == Status::Interrupted {
                self.vm.park(cx);
            }
            Ok(status)
        });
        let r = how(outcome);
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        (r, result)
    }

    /// How an object shows on the radar, as its scripts say when asked: on the object's own
    /// instance, a slot holding the shape given, a new color (the one given), the result slot,
    /// references to the shape's and the color's slots, then the receiver. Returns how the call
    /// went, the result (whether it shows), the color and the shape the scripts left. `None` when
    /// the object is not here.
    // D2-00625, D2-01170
    pub fn radar_look(
        &mut self,
        object: u64,
        color: [f32; 4],
        shape: u32,
    ) -> Option<(CallResult, u32, [f32; 4], u32)> {
        self.host.model.object(object)?;
        self.vm.budget = BUDGET;
        let mut out = (0, color, shape);
        let outcome = self.process_of(object).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            let shape_slot = cx.sp;
            cx.push(shape, tag::MIXED)?;
            let held = self.vm.heap.add_native(
                auberean_glue::color::PACKAGE,
                auberean_glue::color::object(color),
            );
            let color_slot = cx.sp;
            cx.push(held, tag::REFERENCE)?;
            let result_slot = cx.sp;
            cx.push(0, tag::INT)?;
            #[allow(clippy::cast_possible_truncation)]
            let (to_shape, to_color) = (
                self.vm.heap.add_relative(NULL, shape_slot as u32),
                self.vm.heap.add_relative(NULL, color_slot as u32),
            );
            cx.push(to_shape, tag::REFERENCE)?;
            cx.push(to_color, tag::REFERENCE)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(crate::interface::function::INQ_RADAR_LOOK),
            )?;
            let result = cx.slot(result_slot).map_or(0, |(v, _)| v);
            let shape = cx.slot(shape_slot).map_or(shape, |(v, _)| v);
            let c = cx
                .slot(color_slot)
                .ok()
                .and_then(|(r, _)| self.vm.heap.native(r).ok())
                .map_or(color, auberean_glue::color::of);
            out = (result, c, shape);
            Ok(status)
        });
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some((how(outcome), out.0, out.1, out.2))
    }

    /// An object's name as its scripts write it for a number of it (a plural for more than one):
    /// on the object's own instance, the result slot, the number, then the receiver; the result
    /// is a string reference, written in the client's language. `None` when the object is not
    /// here or the call gave no string reference.
    pub fn object_name(&mut self, object: u64, quantity: u32) -> Option<String> {
        let mut info = self.object_name_info(object, quantity)?;
        Some(info.text(&mut self.host.text))
    }

    /// An object's name as its scripts give it for a number of it, as a string reference (see
    /// [`Self::object_name`]).
    // D2-00417, D2-01130
    pub fn object_name_info(
        &mut self,
        object: u64,
        quantity: u32,
    ) -> Option<auberean_text::StringInfo> {
        self.host.model.object(object)?;
        self.vm.budget = BUDGET;
        let mut held = NULL;
        let outcome = self.process_of(object).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            cx.push(0, tag::INT)?;
            cx.push(quantity, tag::MIXED)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(
                    crate::interface::function::GET_POTENTIALLY_PLURALIZED_NAME,
                ),
            )?;
            held = cx.slot(0).map_or(NULL, |(v, _)| v);
            Ok(status)
        });
        let info = match (outcome, self.vm.heap.native(held)) {
            (Ok(Status::Terminated), Ok(NativeObject::TextInfo(t))) => Some(t.clone()),
            _ => None,
        };
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some(crate::text::from_script(&info?))
    }

    /// A name from one of the player's lists the chat window steps through (the tells sent, the
    /// tells answered), a step back or on, as the interface asks the scripts: on the player's
    /// instance, a slot holding a new string, the result slot, a reference to the string's slot,
    /// then the receiver. Returns how the call went, whether the scripts gave a name, and the
    /// string they left. `None` when there is no player.
    // D2-01165, D2-01166, D2-01167, D2-01168
    pub fn history_item(&mut self, function: u32) -> Option<(CallResult, u32, Vec<u16>)> {
        let player = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())?;
        self.vm.budget = BUDGET;
        let mut out = (0, Vec::new());
        let outcome = self.process_of(player).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            let text = self
                .vm
                .heap
                .add_native(native_type::WP_STRING, NativeObject::Text(Vec::new()));
            let text_slot = cx.sp;
            cx.push(text, tag::REFERENCE)?;
            let result_slot = cx.sp;
            cx.push(0, tag::INT)?;
            #[allow(clippy::cast_possible_truncation)]
            let to_text = self.vm.heap.add_relative(NULL, text_slot as u32);
            cx.push(to_text, tag::REFERENCE)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(function),
            )?;
            let result = cx.slot(result_slot).map_or(0, |(v, _)| v);
            let held = cx.slot(text_slot).map_or(NULL, |(v, _)| v);
            let units = match self.vm.heap.native(held) {
                Ok(NativeObject::Text(t)) => t.clone(),
                _ => Vec::new(),
            };
            out = (result, units);
            Ok(status)
        });
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some((how(outcome), out.0, out.1))
    }

    /// The radar's ranges, indoors and outdoors, meters, as the radar's control script gives
    /// them: its singleton asked, in a process of its own made for the call and ended after it,
    /// with the two values' slots by reference.
    // D2-01494, D2-16490
    pub fn radar_ranges(&mut self) -> Option<(f32, f32)> {
        self.host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())?;
        let control = self.host.program.library.package_id("RadarControl")?;
        self.vm.budget = BUDGET;
        let mut out = None;
        let singleton = self
            .vm
            .heap
            .singleton(auberean_script::library::STATE_BASE | (control & 0xFFFF));
        let p = self.vm.create_process(singleton);
        let outcome = (|| -> Result<Status, Stop> {
            let mut cx = self.vm.new_context(p);
            let outdoor = cx.sp;
            cx.push(0, tag::MIXED)?;
            let indoor = cx.sp;
            cx.push(0, tag::MIXED)?;
            cx.push(0, tag::INT)?;
            #[allow(clippy::cast_possible_truncation)]
            let (to_outdoor, to_indoor) = (
                self.vm.heap.add_relative(NULL, outdoor as u32),
                self.vm.heap.add_relative(NULL, indoor as u32),
            );
            cx.push(to_outdoor, tag::REFERENCE)?;
            cx.push(to_indoor, tag::REFERENCE)?;
            let receiver = singleton;
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(crate::interface::function::INQ_RADAR_RANGES),
            )?;
            let read = |k: usize| cx.slot(k).map_or(0.0, |(v, _)| f32::from_bits(v));
            out = Some((read(indoor), read(outdoor)));
            Ok(status)
        })();
        self.vm.delete_process(p);
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        match outcome {
            Ok(Status::Terminated) => out,
            _ => None,
        }
    }

    /// An object's number quality as the panels read it, as scripts read it: its description's,
    /// else its own value, else its record's.
    pub fn int_quality(&mut self, id: u64, key: u32) -> Option<i32> {
        let o = self.host.model.object(id)?;
        if let Some(v) =
            crate::description::int(&o.weenie, key).or_else(|| o.qualities.ints.get(&key).copied())
        {
            return Some(v);
        }
        self.host.base_quality(
            id,
            |w| crate::description::int(w, key),
            |r| find(&r.ints, key),
        )
    }

    /// An object's yes-or-no quality, likewise.
    pub fn bool_quality(&mut self, id: u64, key: u32) -> Option<bool> {
        let o = self.host.model.object(id)?;
        if let Some(v) = crate::description::bool(&o.weenie, key)
            .or_else(|| o.qualities.bools.get(&key).copied())
        {
            return Some(v != 0);
        }
        self.host
            .base_quality(
                id,
                |w| crate::description::bool(w, key),
                |r| find(&r.bools, key),
            )
            .map(|v| v != 0)
    }

    /// An object's instance-id quality, likewise.
    pub fn instance_quality(&mut self, id: u64, key: u32) -> Option<u64> {
        let o = self.host.model.object(id)?;
        if let Some(v) = crate::description::instance_id(&o.weenie, key)
            .or_else(|| o.qualities.instance_ids.get(&key).copied())
        {
            return Some(v);
        }
        self.host.base_quality(
            id,
            |w| crate::description::instance_id(w, key),
            |r| find(&r.instance_ids, key),
        )
    }

    /// Call one of the interface's functions that fills an out argument, as the client's
    /// interface glue does: a new context of the player's process holding a new native object of
    /// type `ty` in a slot, then the result slot, a reference to that slot and the player's
    /// instance; the function may leave another object in the slot. Returns how it went and the
    /// object the slot then holds. `None` when there is no player here.
    pub(crate) fn call_out(
        &mut self,
        function: u32,
        ty: u32,
    ) -> Option<(CallResult, NativeObject)> {
        self.call_out_answered(function, ty)
            .map(|(r, _, held)| (r, held))
    }

    /// As [`Scripts::call_out`], with the word the function answered.
    pub(crate) fn call_out_answered(
        &mut self,
        function: u32,
        ty: u32,
    ) -> Option<(CallResult, u32, NativeObject)> {
        let player = self
            .host
            .model
            .player
            .filter(|p| self.host.model.object(*p).is_some())?;
        self.vm.budget = BUDGET;
        let mut held_now = NativeObject::new(&self.host.program.library, ty);
        let mut answered = 0;
        let outcome = self.process_of(player).and_then(|p| {
            let mut cx = self.vm.new_context(p);
            let lib = &self.host.program.library;
            let fresh = NativeObject::new(lib, ty);
            let held = self.vm.heap.add_native(ty, fresh);
            let slot = cx.sp;
            cx.push(held, tag::REFERENCE)?;
            cx.push(0, tag::INT)?;
            #[allow(clippy::cast_possible_truncation)]
            let at = self.vm.heap.add_relative(NULL, slot as u32);
            cx.push(at, tag::REFERENCE)?;
            let receiver = self.vm.instance_of(p);
            cx.push(receiver, tag::REFERENCE)?;
            let prog = Arc::clone(&self.host.program);
            let package = self.vm.package_of(&mut self.host, receiver, &prog)?;
            let status = self.vm.call(
                &mut self.host,
                &mut cx,
                package,
                auberean_script::value::FunctionId(function),
            )?;
            answered = cx.slot(slot + 1)?.0;
            let (r, _) = cx.slot(slot)?;
            if let Ok(o) = self.vm.heap.native(r) {
                held_now = o.clone();
            }
            Ok(status)
        });
        let r = match outcome {
            Ok(Status::Terminated) => CallResult::Completed,
            Ok(s) => CallResult::Ended(s),
            Err(Stop::NotPorted { native, .. }) => CallResult::NotPorted(native),
            Err(Stop::NullReference) => CallResult::NullReference,
            Err(e) => CallResult::Stopped(e.to_string()),
        };
        self.vm.heap.abandon_builds();
        self.vm.stopped_at = None;
        Some((r, answered, held_now))
    }

    /// The player's quests, as the journal asks for them: the interface's quest-list function
    /// puts the player's own list in the slot, and the journal takes a copy of the list the slot
    /// then holds (the empty one when the player has none). `None` when there is no player here.
    // D2-00590, D2-01169
    pub fn quest_list(&mut self) -> Option<(CallResult, Vec<QuestInfo>)> {
        let (r, held) = self.call_out(
            crate::interface::function::QUEST_LIST,
            native_type::QUEST_INFO_LIST,
        )?;
        let list = match held {
            NativeObject::QuestList(l) => l,
            _ => Vec::new(),
        };
        Some((r, list))
    }

    /// The player's options as the options panel reads them: the player's current options
    /// profile, held as the engine holds it. `None` when there is no player here or it has none.
    // D2-00577, D2-01120
    pub fn options(&mut self) -> Option<GameplayOptions> {
        let (_, r) = self.call_interface(crate::interface::function::CURRENT_OPTIONS, &[]);
        let o = self.vm.heap.native(r).ok()?.unpacked();
        match o {
            NativeObject::Options(p) => Some(*p),
            _ => None,
        }
    }

    /// The windows' saved places the player's scripts keep, as the play screen asks for them:
    /// the interface's function fills a new table in the slot and answers 1 when it did. The
    /// table's packed form; `None` when there is no player here or the function answered 0.
    // D2-00564, D2-01182
    pub fn ui_save_locations(&mut self) -> Option<Vec<u8>> {
        let (_, answered, held) = self.call_out_answered(
            crate::interface::function::INQ_UI_SAVE_LOCATIONS,
            native_type::UI_SAVE_LOCATIONS,
        )?;
        if answered == 0 {
            return None;
        }
        match held {
            NativeObject::Packed { bytes, .. } => Some(bytes),
            _ => Some(auberean_script::options::NO_UI_LOCATIONS.to_vec()),
        }
    }

    /// The player's shortcuts, slot by slot, as the toolbar asks for them: the interface's
    /// function fills a new list in the slot with a new shortcut per slot of the player's options,
    /// and the toolbar copies each. `None` when there is no player here.
    // D2-00534, D2-01177, D2-01453
    pub fn shortcuts(&mut self) -> Option<(CallResult, Vec<Shortcut>)> {
        let (r, held) = self.call_out(
            crate::interface::function::INQ_SHORTCUT_ARRAY,
            SHORTCUT_ARRAY,
        )?;
        let NativeObject::Sequence { items, .. } = held else {
            return Some((r, Vec::new()));
        };
        let all = items
            .iter()
            .filter_map(|i| i.reference())
            .map(|id| match self.vm.heap.native(id) {
                Ok(NativeObject::Shortcut(s)) => s.clone(),
                _ => Shortcut::default(),
            })
            .collect();
        Some((r, all))
    }

    /// An object's instance, made if the object is here and has none yet.
    pub fn instance_of_object(&mut self, id: u64) -> Option<u32> {
        if let Some(i) = self.host.instances.get(&id) {
            return Some(*i);
        }
        self.process_of(id).ok().map(|p| self.vm.instance_of(p))
    }
}

/// Set a string reference's string id (`fp[-2]`).
// D2-01370
fn set_string_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let id = c.arg(0)?;
    match c.vm.heap.native_mut(r)? {
        NativeObject::TextInfo(t) => t.string_id = id,
        _ => return Err(c.fail("not a string reference")),
    }
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// The interface
// ---------------------------------------------------------------------------------------------

/// Whether the receiver's object is the player.
// D2-04667, D2-08060
fn is_the_player(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let is = host(c)?.model.player == Some(id);
    c.write(-2, u32::from(is), tag::MIXED)?;
    Ok(Status::Success)
}

/// The instance of an object by id (`fp[-3..-2]`), or the null reference when the client does
/// not have it.
// D2-04665, D2-08057
fn weenie_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let id = c.long_at(-3)?;
    let r = instance(c, id)?;
    c.write(-4, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// An object's instance, made when it has none yet; null when the client lacks the object.
fn instance(c: &mut Call<'_>, id: u64) -> Result<u32, Stop> {
    if let Some(i) = host(c)?.instances.get(&id) {
        return Ok(*i);
    }
    let (package, entity) = match host(c)?.model.object(id) {
        Some(o) => (o.weenie.package_id, o.weenie.entity_did),
        None => return Ok(NULL),
    };
    let p = object_process(c.vm, c.host, package, entity)?;
    let i = c.vm.instance_of(p);
    let h = host(c)?;
    h.links.insert(i, id);
    h.instances.insert(id, i);
    Ok(i)
}

/// Where the player is: a new position (below the receiver), the player's cell and frame; the
/// zero position before the player has a place.
// D2-04671, D2-08172
fn my_position(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let (cell, o, q) = host(c)?
        .my_position
        .unwrap_or((0, [0.0; 3], [1.0, 0.0, 0.0, 0.0]));
    let frame = [
        o[0].to_bits(),
        o[1].to_bits(),
        o[2].to_bits(),
        q[0].to_bits(),
        q[1].to_bits(),
        q[2].to_bits(),
        q[3].to_bits(),
    ];
    let r = c.vm.heap.add_native(
        native_type::POSITION,
        NativeObject::Position { cell, frame },
    );
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The package of the receiver's object's scripts (below the receiver); every bit set when its
/// scripts have not been made.
// D2-04655, D2-08032, D2-16406
fn my_package(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let id = receiver_object(c)?;
    let package = match host(c)?.instances.get(&id).copied() {
        Some(i) => {
            let prog = Arc::clone(c.vm.program());
            c.vm.package_of(c.host, i, &prog)?
        }
        None => u32::MAX,
    };
    c.write(-2, package, tag::MIXED)?;
    Ok(Status::Success)
}

/// The entity data id of the receiver's object (below the receiver).
// D2-09203
fn my_entity(c: &mut Call<'_>) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let did = host(c)?.model.object(id).map_or(0, |o| o.weenie.entity_did);
    c.write(-2, did, tag::MIXED)?;
    Ok(Status::Success)
}

/// The player's id, 64 bits (0 before the player is created).
// D2-00961
fn player_id(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = host(c)?.model.player_id();
    write_long(c, -3, p)
}

/// The player's instance: none until the player's object is here.
// D2-00962, D2-05647
fn player_reference(c: &mut Call<'_>) -> Result<Status, Stop> {
    let p = host(c)?.model.player_object().map(|o| o.id);
    let r = match p {
        Some(p) => instance(c, p)?,
        None => NULL,
    };
    c.write(-2, r, tag::REFERENCE)?;
    Ok(Status::Success)
}

// ---------------------------------------------------------------------------------------------
// The physics object
// ---------------------------------------------------------------------------------------------

/// Freeze or thaw the receiver's body's movement or animation (`fp[-2]`); answers 1, or 0 with no
/// body (`fp[-3]`).
fn freeze(c: &mut Call<'_>, movement: bool) -> Result<Status, Stop> {
    let id = receiver_object(c)?;
    let on = c.arg(0)? != 0;
    let h = host(c)?;
    let f = h.frozen.entry(id).or_default();
    if movement {
        f.0 = on;
    } else {
        f.1 = on;
    }
    let answer = crate::body::body_answer(h, id);
    c.write(-3, answer, tag::MIXED)?;
    Ok(Status::Success)
}

// D2-04746, D2-08205
fn set_movement_frozen(c: &mut Call<'_>) -> Result<Status, Stop> {
    freeze(c, true)
}

// D2-04741, D2-08207
fn set_animation_frozen(c: &mut Call<'_>) -> Result<Status, Stop> {
    freeze(c, false)
}

/// Play an effect on the receiver's object (`fp[-2]` its number in the object's effect table,
/// `fp[-3]` the intensity): kept for a front end to show. Answers 1 (the engine answers whether
/// the object's drawn form played it; every script discards the answer).
// D2-04704, D2-08144, D2-06162
fn do_fx(c: &mut Call<'_>) -> Result<Status, Stop> {
    let object = receiver_object(c)?;
    let effect = c.arg(0)?;
    let intensity = f32::from_bits(c.arg(1)?);
    host(c)?.effects.push(Effect {
        object,
        effect,
        intensity,
    });
    c.write(-4, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// Stop the camera's effects, when the receiver's object is the player.
// D2-04693, D2-08209, D2-06086
fn stop_all_camera_fx(c: &mut Call<'_>) -> Result<Status, Stop> {
    let object = receiver_object(c)?;
    let h = host(c)?;
    if h.model.player == Some(object) {
        h.camera_fx_stops += 1;
    }
    Ok(Status::Success)
}

/// What a physics object's behavior call answers: 1 when the object is here (its physics object
/// played or stopped it), 8 when it has none.
const NO_PHYSICS_OBJECT: u32 = 8;

/// Play a behavior on the receiver's object without waiting for it (`fp[-2]` its parameters, whose
/// context is cleared: nothing waits on it): kept for a front end to show. Answers 1, or 8 when
/// the object is not here.
// D2-04703, D2-08148, D2-06263
fn do_behavior_non_latent(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let id = receiver_object(c)?;
    let r = c.arg(0)?;
    let params = match auberean_glue::behavior::params(c, r)? {
        Some(p) => {
            p.context = 0;
            p.clone()
        }
        None => return Err(Stop::NullReference),
    };
    let h = host(c)?;
    let here = h.model.object(id).is_some();
    if here {
        h.behaviors.push(Behavior::Play { object: id, params });
    }
    c.write(-3, if here { 1 } else { NO_PHYSICS_OBJECT }, tag::MIXED)?;
    Ok(Status::Success)
}

/// Stop a behavior (`fp[-2]` its id) on the receiver's object: kept for a front end. Answers 1, or
/// 8 when the object is not here.
// D2-04750, D2-08155, D2-05887
fn stop_behavior(c: &mut Call<'_>) -> Result<Status, Stop> {
    if c.receiver()? == NULL {
        return Ok(Status::Crash);
    }
    let id = receiver_object(c)?;
    let behavior = c.arg(0)?;
    let h = host(c)?;
    let here = h.model.object(id).is_some();
    if here {
        h.behaviors.push(Behavior::Stop {
            object: id,
            behavior,
        });
    }
    c.write(-3, if here { 1 } else { NO_PHYSICS_OBJECT }, tag::MIXED)?;
    Ok(Status::Success)
}

/// The request an examination profile answers (below the receiver): a new request, a copy of the
/// profile's own, which leads its packed form.
// D2-01362
fn examination_request(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::native_object::{native_type, step_over, NativeObject};
    use auberean_script::wire::In;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let bytes = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } if !bytes.is_empty() => {
            let mut at = In::new(bytes);
            step_over(&mut at, native_type::EXAMINATION_REQUEST)
                .map_err(|_| c.fail("an examination profile that does not read"))?;
            bytes[..at.pos()].to_vec()
        }
        _ => Vec::new(),
    };
    let ty = native_type::EXAMINATION_REQUEST;
    let request = c.vm.heap.add_native(ty, NativeObject::Packed { ty, bytes });
    c.write(-2, request, tag::REFERENCE)?;
    Ok(Status::Success)
}

/// The object an examination request is about, 64 bits below the receiver: the id its packed
/// form carries for the kinds that name an object, else none (0).
// D2-08538
fn examined_object(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::native_object::NativeObject;
    let r = c.receiver()?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let id = match c.vm.heap.native(r)? {
        NativeObject::Packed { bytes, .. } if bytes.len() >= 16 => {
            let word =
                |i: usize| u32::from_le_bytes([bytes[i], bytes[i + 1], bytes[i + 2], bytes[i + 3]]);
            match word(0) {
                0x4000_0002 | 0x4000_000D => u64::from(word(8)) | u64::from(word(12)) << 32,
                _ => 0,
            }
        }
        _ => 0,
    };
    write_long(c, -3, id)
}

// ---------------------------------------------------------------------------------------------
// Notices to the panels
// ---------------------------------------------------------------------------------------------

/// A line for the chat windows: the object it is shown about (`fp[-2]`), the kind of text
/// (`fp[-3]`) and the string reference (`fp[-4]`), kept for the interface in the order given;
/// answers 1 below its arguments.
// D2-00851, D2-00193
fn display_string_info(c: &mut Call<'_>) -> Result<Status, Stop> {
    use auberean_script::native_object::NativeObject;
    let native = c.native;
    let about = c.cx.at(-2)?;
    let kind = c.cx.at(-3)?;
    let r = c.cx.at(-4)?;
    if r == NULL {
        return Ok(Status::Crash);
    }
    let text = match c.vm.heap.native(r)? {
        NativeObject::TextInfo(t) => t.clone(),
        _ => return Err(c.fail("the line is not a string reference")),
    };
    let h = host(c)?;
    let about = h.links.get(&about).copied();
    *h.notices.entry(native).or_default() += 1;
    let notices_before = h.panel_notices.len();
    h.shown.push(Shown {
        about,
        kind,
        text,
        notices_before,
    });
    c.write(-5, 1, tag::MIXED)?;
    Ok(Status::Success)
}

/// A notice to the interface's panels, read as its native reads it ([`crate::notices`]): the words
/// below the receiver, the first naming the object whose panels hear it (unless every panel
/// does); recorded for the panels, and answered 1 in the slot below its words.
fn panel_notice(c: &mut Call<'_>) -> Result<Status, Stop> {
    let native = c.native;
    let Some(reading) = crate::notices::reading(native) else {
        return Err(c.fail("not a notice to the panels"));
    };
    let (slots, everyone) = (
        reading.slots,
        reading.hearers == crate::notices::Hearers::Everyone,
    );
    let mut args = Vec::new();
    let mut objects = Vec::new();
    let mut requests = Vec::new();
    let mut listed = Vec::new();
    for k in 0..slots {
        let (v, t) = c.cx.slot(c.cx.frame_index(-2 - k)?)?;
        args.push(v);
        if k > 0 || everyone {
            // A copy of the object as it is now (a shop's whole value, as the store's notices
            // copy one for the panels).
            // D2-12385, D2-12384
            let object = (tag::is_reference(t) && v != NULL)
                .then(|| c.vm.heap.native(v).ok().cloned())
                .flatten();
            let whole = match &object {
                Some(NativeObject::Packed { ty, bytes })
                    if *ty == auberean_script::native_object::native_type::EXAMINATION_REQUEST =>
                {
                    host(c)?
                        .requests
                        .get(&v)
                        .copied()
                        .filter(|w| w.pack() == *bytes)
                }
                _ => None,
            };
            let items = match &object {
                Some(NativeObject::Sequence { items, .. }) => items
                    .iter()
                    .map(|i| match i {
                        auberean_script::native_object::Item::Ref(r) => {
                            c.vm.heap.native(*r).ok().cloned()
                        }
                        _ => None,
                    })
                    .collect(),
                _ => Vec::new(),
            };
            listed.push(items);
            objects.push(object);
            requests.push(whole);
        }
    }
    let h = host(c)?;
    *h.notices.entry(native).or_default() += 1;
    let about = if everyone {
        None
    } else {
        args.first().and_then(|a| h.links.get(a).copied())
    };
    h.panel_notices.push(PanelNotice {
        native,
        about,
        words: args.into_iter().skip(usize::from(!everyone)).collect(),
        effect_lines: vec![Vec::new(); objects.len()],
        objects,
        requests,
        listed,
    });
    c.write(-2 - slots, 1, tag::MIXED)?;
    Ok(Status::Success)
}
