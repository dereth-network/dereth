//! Character generation's appearance choices from the September 2013 install, held to a creation
//! the 2017 client sent.

use auberean_world::chargen::{
    choices, middle, Avatar, Random, BUILD, CLOTHING, EMPYREAN, FEMALE, HEIGHT, HUMAN, MALE,
    TUMEROK, TYPES,
};
use auberean_world::look::Look;

use crate::common::need_world;

/// A human man's choices: build in 31 steps of 0.01 from 0.85 (the middle stop, where a new avatar
/// starts, a hair under 1), height in 16 from 0.95 (middle 1.02), skin tone, hairstyle, beard, hair
/// color, the shirt's, pants' and boots' colors and the face, each with its table key's stops
/// (11 skin tones); every race and sex's build and height stop counts; an Empyrean woman's boots
/// have their 73 colors twice over. A new avatar's choices are all stops, and its look puts
/// together with the body and wears its three garments.
///
/// Behaviour: world.chargen.a-race-and-sexs-choices-and-their-stops
#[test]
fn a_race_and_sexs_choices_and_their_stops() {
    let w = need_world!();
    let man = choices(&w.records, HUMAN, MALE);
    let types: Vec<u32> = man.iter().map(|c| c.physique_type).collect();
    assert_eq!(
        types,
        vec![1, 2, 4, 0x10, 0x20, 0x40, 0x100, 0x200, 0x400, 0x800]
    );
    let counts: Vec<usize> = man.iter().map(|c| c.values.len()).collect();
    assert_eq!(counts, vec![31, 16, 11, 12, 8, 12, 26, 26, 23, 11]);
    let build = &man[0].values;
    assert_eq!(middle(build.len()), 15);
    assert_eq!(build[0].to_bits(), 0x3F59_999A, "0.85");
    assert_eq!(build[15].to_bits(), 0x3F7F_FFFE, "the middle build");
    assert_eq!(middle(16), 7);
    assert!((man[1].values[7] - 1.02).abs() < 1e-6, "the middle height");
    // Build and height in every race and sex.
    for (race, sex, n_build, n_height) in [
        (HUMAN, MALE, 31, 16),
        (HUMAN, FEMALE, 31, 16),
        (2, MALE, 21, 16),
        (2, FEMALE, 21, 16),
        (TUMEROK, MALE, 21, 16),
        (TUMEROK, FEMALE, 21, 16),
        (8, MALE, 11, 11),
        (8, FEMALE, 21, 21),
        (EMPYREAN, MALE, 26, 26),
        (EMPYREAN, FEMALE, 26, 26),
    ] {
        let c = choices(&w.records, race, sex);
        assert_eq!(
            (c[0].values.len(), c[1].values.len()),
            (n_build, n_height),
            "race {race} sex {sex:#x}"
        );
    }
    let boots = choices(&w.records, EMPYREAN, FEMALE)
        .into_iter()
        .find(|c| c.physique_type == 0x400)
        .expect("boots");
    assert_eq!(boots.values.len(), 146);
    assert_eq!(boots.values[..73], boots.values[73..]);

    let mut random = Random::default();
    let a = Avatar::new(&man, HUMAN, MALE, &mut random);
    assert_eq!(a.value(BUILD), Some(build[15]));
    assert_eq!(a.value(HEIGHT), Some(man[1].values[7]));
    for c in &man {
        let v = a.value(c.physique_type).expect("every type chosen");
        assert!(c.values.contains(&v), "{:#x} = {v}", c.physique_type);
    }
    let body = auberean_world::place::PlaceRecords::entity(
        &w.records,
        auberean_primitives::DataId(0x4700_00CD),
    )
    .and_then(|e| e.object)
    .expect("the human man's body");
    let look = Look::of(&w.records, &a.look(&man, auberean_primitives::DataId(body)))
        .expect("the look puts together");
    for c in man
        .iter()
        .filter(|c| c.physique_type & CLOTHING == c.physique_type)
    {
        let worn = look.global.appearance.get(c.table).expect("the garment");
        assert_eq!(worn.get(0x10), Some(&1.0), "{:#x} worn", c.physique_type);
    }
}

/// A Tumerok woman created on 14 January 2017: each of the thirteen physique values her client
/// sent is one of her race and sex's stops for that type, and the types she does not have (beard,
/// boots, eyes) were sent as 0.
///
/// Behaviour: world.chargen.a-2017-creations-values-are-stops
#[test]
fn a_2017_creations_values_are_stops() {
    let w = need_world!();
    let her = choices(&w.records, TUMEROK, FEMALE);
    let sent: [(u32, f32); 13] = [
        (1, 0.939_999_9),
        (2, 1.009_999_9),
        (4, 0.4),
        (0x1000, 0.0),
        (8, 0.6),
        (0x10, 0.6),
        (0x40, 1.0),
        (0x100, 0.32),
        (0x800, 0.6),
        (0x20, 0.0),
        (0x80, 0.05),
        (0x400, 0.0),
        (0x200, 0.1),
    ];
    assert_eq!(sent.len(), TYPES.len());
    for (t, v) in sent {
        match her.iter().find(|c| c.physique_type == t) {
            Some(c) => assert!(
                c.values.iter().any(|x| x.to_bits() == v.to_bits()),
                "{t:#x} = {v} is not a stop: {:?}",
                c.values
            ),
            None => assert_eq!(v, 0.0, "{t:#x} she lacks"),
        }
    }
}

/// The character generator's scripts, run on the install's library and saved states.
mod scripts {
    use std::any::Any;
    use std::sync::Arc;

    use auberean_assets::Database;
    use auberean_dat::locate::{install_dats, DatName};
    use auberean_dat::testing::require_target_install;
    use auberean_dat::DatFile;
    use auberean_script::context::Context;
    use auberean_script::graph::SavedState;
    use auberean_script::library::{Library, RECORD, STATE_BASE};
    use auberean_script::native_object::NativeObject;
    use auberean_script::states::StateCache;
    use auberean_script::value::{tag, FunctionId, NULL};
    use auberean_script::{Host, Program, Status, Stop, Vm};

    /// The character generator's package.
    pub const PACKAGE: u32 = 0x96;

    struct States {
        states: StateCache,
        program: Arc<Program>,
        random: auberean_world::chargen::Random,
        database: Database,
    }

    impl Host for States {
        fn saved_state(&mut self, id: u32) -> Option<Arc<SavedState>> {
            self.states.get(id, &self.program.library)
        }

        fn random(&mut self) -> Result<f64, Stop> {
            Ok(self.random.draw())
        }

        fn database(&mut self) -> Option<&Database> {
            Some(&self.database)
        }

        fn as_any(&mut self) -> &mut dyn Any {
            self
        }
    }

    /// The machine with the install's library linked with every native, and its saved states.
    pub struct Machine {
        pub vm: Vm,
        host: States,
        pub program: Arc<Program>,
    }

    /// What a call left: how it went, its result word, and each out slot's value and what it
    /// names.
    #[derive(Debug)]
    pub struct Answer {
        pub status: Result<Status, String>,
        pub result: u32,
        pub outs: Vec<(u32, Option<NativeObject>)>,
    }

    impl Machine {
        pub fn new() -> Self {
            let dir = require_target_install();
            let (_, path) = install_dats(&dir)
                .expect("list the install")
                .into_iter()
                .find(|(n, _)| *n == DatName::Portal)
                .expect("a portal file");
            let dat = Arc::new(DatFile::open(&path).expect("the portal file"));
            let lib = dat
                .read_checked(RECORD, Library::from_record)
                .expect("the library reads")
                .value;
            let natives = auberean_glue::natives(&lib);
            let program = Arc::new(Program::link(lib, natives).expect("the library links"));
            let mut vm = Vm::new(Arc::clone(&program));
            vm.budget = 10_000_000;
            Self {
                vm,
                host: States {
                    states: StateCache::new(Arc::clone(&dat)),
                    program: Arc::clone(&program),
                    random: auberean_world::chargen::Random::default(),
                    database: Database::new(vec![Arc::clone(&dat)]),
                },
                program,
            }
        }

        /// Call one of the package's functions on its singleton, as the client's interface glue
        /// calls one: `outs` slots (each null), the result slot, a reference to each out slot,
        /// the arguments (`args[0]` the first parameter), the singleton.
        pub fn call(&mut self, function: u32, outs: usize, args: &[u32]) -> Answer {
            self.call_holding(function, &vec![NULL; outs], args)
        }

        /// The same, with each out slot holding a value on the way in.
        pub fn call_holding(&mut self, function: u32, held: &[u32], args: &[u32]) -> Answer {
            let s = self.vm.heap.singleton(STATE_BASE | PACKAGE);
            let p = self.vm.create_process(s);
            let mut cx: Context = self.vm.new_context(p);
            let mut at = Vec::new();
            let mut answer = Answer {
                status: Ok(Status::Success),
                result: 0,
                outs: Vec::new(),
            };
            let run = (|| -> Result<Status, Stop> {
                for h in held {
                    at.push(cx.sp);
                    cx.push(*h, tag::REFERENCE)?;
                }
                let result = cx.sp;
                cx.push(0, tag::INT)?;
                for a in &at {
                    #[allow(clippy::cast_possible_truncation)]
                    let r = self.vm.heap.add_relative(NULL, *a as u32);
                    cx.push(r, tag::REFERENCE)?;
                }
                for a in args.iter().rev() {
                    cx.push(*a, tag::INT)?;
                }
                cx.push(s, tag::REFERENCE)?;
                let prog = Arc::clone(&self.program);
                let package = self.vm.package_of(&mut self.host, s, &prog)?;
                let status =
                    self.vm
                        .call(&mut self.host, &mut cx, package, FunctionId(function))?;
                answer.result = cx.slot(result)?.0;
                for a in &at {
                    let (v, t) = cx.slot(*a)?;
                    let o = (tag::is_reference(t) && v != NULL)
                        .then(|| self.vm.heap.native(v).ok().cloned())
                        .flatten();
                    answer.outs.push((v, o));
                }
                Ok(status)
            })();
            answer.status = run.map_err(|e| e.to_string());
            self.vm.delete_process(p);
            answer
        }

        /// Start the machine's random numbers from the top, as a new run of the client does.
        pub fn restart_random(&mut self) {
            self.host.random = auberean_world::chargen::Random::default();
        }

        /// A native object by reference.
        pub fn native(&self, r: u32) -> Option<NativeObject> {
            self.vm.heap.native(r).ok().cloned()
        }
    }
}

/// A label's string id: the client's hash of its name (each byte's low seven bits at the top of a
/// word, rotated right seven bits further than the byte before, xored together).
fn label_id(name: &str) -> u32 {
    let mut hash = 0u32;
    let mut shift = 0u32;
    for b in name.bytes() {
        hash ^= (u32::from(b) << 25).rotate_right(shift);
        shift = (shift + 7) & 31;
    }
    hash
}

/// The character generator's scripts and the native creation give the same answers over the
/// whole of the data: the races and each race's two sexes; each race and sex's body (its look's
/// first word, and its physical object); for each race, sex and physique type, the valid values,
/// value for value, or none (and an answer of 0) for a type the race and sex lack, and for each
/// type it has the label (the hashed name, with no string table, so the scripts answer 0); each
/// race's starting area for its class, and none for another class; the player's avatar entity.
///
/// Behaviour: world.chargen.the-native-creation-answers-as-the-scripts-do
#[test]
fn the_native_creation_answers_as_the_character_generators_scripts_do() {
    use auberean_script::native_object::{Item, NativeObject};
    use auberean_world::chargen::{avatar_entity, body_entity, RACES};
    const ADMIN_AVATAR: u32 = scripts::PACKAGE << 16 | 0x01;
    const PLAYER_AVATAR: u32 = scripts::PACKAGE << 16 | 0x02;
    const RACE_SEX_DATA: u32 = scripts::PACKAGE << 16 | 0x05;
    const RACE_SEX_INFO: u32 = scripts::PACKAGE << 16 | 0x06;
    const PHYS_TYPE_LABEL: u32 = scripts::PACKAGE << 16 | 0x07;
    const PHYS_TYPE_VALUES: u32 = scripts::PACKAGE << 16 | 0x08;
    const STARTING_AREAS: u32 = scripts::PACKAGE << 16 | 0x09;
    const PHYS_OBJ: u32 = scripts::PACKAGE << 16 | 0x0B;
    let w = need_world!();
    let mut m = scripts::Machine::new();
    let mut wrong = Vec::new();
    let words = |o: &Option<NativeObject>| -> Vec<u32> {
        match o {
            Some(NativeObject::Sequence { items, .. }) => items
                .iter()
                .filter_map(|i| match i {
                    Item::Word(v) => Some(*v),
                    _ => None,
                })
                .collect(),
            _ => Vec::new(),
        }
    };

    let a = m.call(PLAYER_AVATAR, 1, &[]);
    assert_eq!(a.outs[0].0, avatar_entity(false), "the player's avatar");
    let a = m.call(ADMIN_AVATAR, 1, &[]);
    assert_eq!(
        a.outs[0].0,
        avatar_entity(true),
        "the administrator's avatar"
    );

    // The races and sexes.
    let a = m.call(RACE_SEX_DATA, 1, &[]);
    assert_eq!(a.result, 1);
    let Some(NativeObject::Hash(h)) = &a.outs[0].1 else {
        panic!("a table of races: {a:?}");
    };
    let mut pairs = Vec::new();
    for (k, v) in h.entries() {
        let (Item::Word(race), Item::Ref(r)) = (k, v) else {
            panic!("{k:?} {v:?}");
        };
        for sex in words(&m.native(*r)) {
            pairs.push((*race, sex));
        }
    }
    pairs.sort_unstable();
    let mut native: Vec<(u32, u32)> = auberean_world::chargen::BODIES
        .iter()
        .map(|b| (b.0, b.1))
        .collect();
    native.sort_unstable();
    assert_eq!(pairs, native, "the races and sexes");

    // Each race and sex's look: its body first.
    let a = m.call(RACE_SEX_INFO, 1, &[]);
    let Some(NativeObject::Hash(h)) = &a.outs[0].1 else {
        panic!("a table of looks: {a:?}");
    };
    let mut looks = 0;
    for (k, v) in h.entries() {
        let (Item::Word(key), Item::Ref(r)) = (k, v) else {
            panic!("{k:?} {v:?}");
        };
        let (race, sex) = (key & 0xFF, key & 0xF000);
        let Some(NativeObject::Packed { bytes, .. }) = m.native(*r) else {
            panic!("a look: {key:#x}");
        };
        let body = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        if Some(body) != body_entity(race, sex) {
            wrong.push(format!("look {race} {sex:#x}: body {body:#x}"));
        }
        looks += 1;
    }
    assert_eq!(looks, native.len(), "a look per race and sex");

    let mut labelled = 0;
    for &(race, sex) in &native {
        let a = m.call(PHYS_OBJ, 0, &[race, sex]);
        if a.status.is_err() || Some(a.result) != body_entity(race, sex) {
            wrong.push(format!("body {race} {sex:#x}: {a:?}"));
        }
        let choices = choices(&w.records, race, sex);
        for t in TYPES {
            let c = choices.iter().find(|c| c.physique_type == t);
            let a = m.call(PHYS_TYPE_VALUES, 1, &[race, sex, t]);
            let script = words(&a.outs[0].1);
            let ours: Vec<u32> =
                c.map_or_else(Vec::new, |c| c.values.iter().map(|v| v.to_bits()).collect());
            // A type with no values answers 0: the appearance screen takes its row away before
            // it reads any label, so only the types it keeps are labelled.
            if script != ours || a.result != u32::from(c.is_some()) {
                wrong.push(format!(
                    "values {race} {sex:#x} {t:#x}: scripts {} {script:x?}, ours {ours:x?}",
                    a.result
                ));
            }
            let Some(c) = c else { continue };
            let a = m.call(PHYS_TYPE_LABEL, 1, &[race, sex, t]);
            let label = match &a.outs[0].1 {
                Some(NativeObject::TextInfo(i)) => Some((i.string_id, i.table)),
                _ => None,
            };
            if label != Some((label_id(c.label), 0)) || a.result != 0 {
                wrong.push(format!(
                    "label {race} {sex:#x} {t:#x}: scripts {} {label:x?}, ours {}",
                    a.result, c.label
                ));
            }
            labelled += 1;
        }
    }

    // The starting areas, for every race and class.
    for &(race, ..) in &RACES {
        for &(_, class, _, _) in &RACES {
            let a = m.call(STARTING_AREAS, 1, &[race, class]);
            let script = words(&a.outs[0].1);
            let ours: Vec<u32> = RACES
                .iter()
                .filter(|r| r.0 == race && r.1 == class)
                .map(|r| r.3)
                .collect();
            if script != ours {
                wrong.push(format!(
                    "areas {race} class {class}: scripts {} {script:x?}, ours {ours:x?}",
                    a.result
                ));
            }
        }
    }
    assert!(
        wrong.is_empty(),
        "{} differ:\n{}",
        wrong.len(),
        wrong.join("\n")
    );
    assert_eq!(
        labelled, 87,
        "the ten races and sexes have 87 types between them"
    );
}

/// The character generator's scripts, handed a creation's race and sex in every race and sex, fill
/// it as the native creation fills a new avatar from the same random numbers, value for value: every
/// type picked among its stops, build and height then put at the middle stop, a type it lacks at
/// 0; and the race's class and implement.
///
/// Behaviour: world.chargen.a-new-avatars-defaults-are-what-the-scripts-set
#[test]
fn a_new_avatars_defaults_are_what_the_scripts_set() {
    use auberean_glue::chargen::{CreationChoices, CREATION_CHOICES};
    use auberean_world::chargen::{BODIES, RACES};
    const SET_DEFAULTS: u32 = scripts::PACKAGE << 16 | 0x0E;
    let w = need_world!();
    let mut m = scripts::Machine::new();
    for &(race, sex, _) in &BODIES {
        let r = m.vm.heap.add_native(
            CREATION_CHOICES,
            CreationChoices {
                race,
                sex,
                ..CreationChoices::default()
            }
            .native(),
        );
        m.restart_random();
        let a = m.call_holding(SET_DEFAULTS, &[r], &[]);
        assert!(a.status.is_ok(), "race {race} sex {sex:#x}: {a:?}");
        assert_eq!(a.result, 1, "race {race} sex {sex:#x}: {a:?}");
        let got = m.native(r).expect("the creation");
        let got = CreationChoices::of(&got).expect("a creation's choices");
        assert_eq!((got.race, got.sex), (race, sex));
        let &(_, class, implement, _) = RACES.iter().find(|x| x.0 == race).expect("its race");
        assert_eq!(
            (got.class, got.implement),
            (class, implement),
            "race {race} sex {sex:#x}"
        );
        let choices = choices(&w.records, race, sex);
        let ours = Avatar::new(&choices, race, sex, &mut Random::default());
        let mut wrong = Vec::new();
        for t in TYPES {
            let script = got.physique.get(&t).copied();
            let native = ours.value(t).map(f32::to_bits);
            if script != native {
                wrong.push(format!(
                    "type {t:#x}: scripts {script:x?}, ours {native:x?}"
                ));
            }
        }
        assert!(wrong.is_empty(), "race {race} sex {sex:#x}: {wrong:#?}");
    }
}

/// The end-of-retail files hold no word filter, so the character generator's scripts leave a typed
/// name's written form as it was and answer that nothing changed.
///
/// Behaviour: world.chargen.with-no-word-filter-the-scripts-leave-a-name-alone
#[test]
fn with_no_word_filter_the_scripts_leave_a_name_alone() {
    use auberean_script::native_object::{native_type, NativeObject};
    const FORMAT_NAME: u32 = scripts::PACKAGE << 16 | 0x11;
    let _ = need_world!();
    let mut m = scripts::Machine::new();
    let held: Vec<u16> = "held".encode_utf16().collect();
    let name = m.vm.heap.add_native(
        native_type::WP_STRING,
        NativeObject::Text("  jOHN  o'neil-ra ".encode_utf16().collect()),
    );
    let out =
        m.vm.heap
            .add_native(native_type::WP_STRING, NativeObject::Text(held.clone()));
    let a = m.call_holding(FORMAT_NAME, &[out], &[name]);
    assert!(a.status.is_ok(), "{a:?}");
    assert_eq!(a.result, 0, "{a:?}");
    assert_eq!(m.native(out), Some(NativeObject::Text(held)));
}
