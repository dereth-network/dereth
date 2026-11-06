//! The quest natives, each called as the machine calls a native: its frame built by hand (the
//! result slot, the arguments last first, the receiver), then the native by id.

use std::sync::Arc;

use auberean_script::context::Context;
use auberean_script::native::{Binding, Call};
use auberean_script::native_object::{native_type, NativeObject, QuestInfo, TextInfo};
use auberean_script::opcode::op;
use auberean_script::value::{tag, NativeId, NULL};
use auberean_script::{Program, Status, Vm};

use crate::asm::{library, w, TestHost};

const QUEST: u32 = 0xA0;
const LIST: u32 = 0xA1;

struct Bench {
    vm: Vm,
    host: TestHost,
    prog: Arc<Program>,
}

impl Bench {
    fn new() -> Self {
        let lib = library(&[w(op::END)], &[], &[]);
        let table = auberean_glue::natives(&lib);
        let prog = Arc::new(Program::link(lib, table).expect("links"));
        Self {
            vm: Vm::new(Arc::clone(&prog)),
            host: TestHost::default(),
            prog,
        }
    }

    fn record(&mut self, q: QuestInfo) -> u32 {
        self.vm
            .heap
            .add_native(native_type::QUEST_INFO, NativeObject::Quest(Box::new(q)))
    }

    fn list(&mut self) -> u32 {
        self.vm.heap.add_native(
            native_type::QUEST_INFO_LIST,
            NativeObject::QuestList(Vec::new()),
        )
    }

    /// Call a native: `local` (a slot below the result, which an out reference may name), the
    /// result slot, the arguments (`args[0]` the first, at `fp[-2]`), the receiver. Returns the
    /// status, the result slot and the local slot, each with its tag.
    fn call(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        args: &[(u32, u8)],
        local: u32,
    ) -> (Status, (u32, u8), (u32, u8)) {
        let mut cx = Context::new(usize::MAX, 0);
        cx.push(local, tag::REFERENCE).unwrap();
        cx.push(0, tag::INT).unwrap();
        for (v, t) in args.iter().rev() {
            cx.push(*v, *t).unwrap();
        }
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(package, number).0;
        let Some(Binding::Ported(f)) = self.prog.table.get(id) else {
            panic!("native {id:#x} is not ported");
        };
        let status = f(&mut Call {
            vm: &mut self.vm,
            cx: &mut cx,
            host: &mut self.host,
            native: id,
        })
        .expect("the native runs");
        (status, cx.slot(1).unwrap(), cx.slot(0).unwrap())
    }

    /// A reference to the local slot (slot 0 of the stack).
    fn local_ref(&mut self) -> u32 {
        self.vm.heap.add_relative(NULL, 0)
    }

    fn ids(&self, list: u32) -> Vec<(u32, u32)> {
        let NativeObject::QuestList(l) = self.vm.heap.native(list).unwrap() else {
            panic!("a list")
        };
        l.iter().map(|q| (q.id, q.status)).collect()
    }

    fn quest(&self, r: u32) -> QuestInfo {
        let NativeObject::Quest(q) = self.vm.heap.native(r).unwrap() else {
            panic!("a record")
        };
        (**q).clone()
    }
}

fn given(id: u32, at: f64, status: u32) -> QuestInfo {
    QuestInfo {
        id,
        status,
        bestowed: at.to_bits(),
        ..QuestInfo::default()
    }
}

/// Behaviour: script.glue.quest-list-natives
#[test]
fn a_quest_list_keeps_quests_newest_given_first_and_updates_a_quest_in_place() {
    let mut b = Bench::new();
    let list = b.list();
    // Update: a new quest goes before the first given earlier (after those given at the same
    // time); a known one is copied over in place, wherever its new time would put it.
    for (id, at, status, new) in [
        (1, 10.0, 0x1000_0000, 1),
        (2, 30.0, 0x1000_0000, 1),
        (3, 20.0, 0x1000_0000, 1),
        (4, 20.0, 0x1000_0000, 1),
        (5, f64::NAN, 0x1000_0000, 1),
        (2, 5.0, 0x3000_0000, 0),
    ] {
        let q = b.record(given(id, at, status));
        let out = b.local_ref();
        let (s, result, local) = b.call(
            LIST,
            0x02,
            list,
            &[(q, tag::REFERENCE), (out, tag::REFERENCE)],
            7,
        );
        assert_eq!(s, Status::Success);
        assert_eq!(result, (1, tag::MIXED), "updating answers 1");
        assert_eq!(
            local,
            (new, tag::MIXED),
            "the out argument says whether it was new"
        );
    }
    let underway = 0x1000_0000;
    assert_eq!(
        b.ids(list),
        [
            (2, 0x3000_0000),
            (3, underway),
            (4, underway),
            (1, underway),
            (5, underway)
        ]
    );
    let (_, size, _) = b.call(LIST, 0x06, list, &[], 0);
    assert_eq!(size, (5, tag::MIXED));

    // Erase: the last record with the record's id; a second erase finds none.
    let three = b.record(given(3, 0.0, 0));
    let (_, erased, _) = b.call(LIST, 0x04, list, &[(three, tag::REFERENCE)], 0);
    assert_eq!(erased, (1, tag::MIXED));
    let (_, erased, _) = b.call(LIST, 0x04, list, &[(three, tag::REFERENCE)], 0);
    assert_eq!(erased, (0, tag::MIXED));

    // Find: the record is copied into the one the out argument holds; when there is none, that
    // record stays as it was. Both answer 1.
    let into = b.record(QuestInfo::default());
    let out = b.local_ref();
    let (_, found, _) = b.call(
        LIST,
        0x03,
        list,
        &[(4, tag::INT), (out, tag::REFERENCE)],
        into,
    );
    assert_eq!(found, (1, tag::MIXED));
    assert_eq!(b.quest(into), given(4, 20.0, underway));
    let (_, found, _) = b.call(
        LIST,
        0x03,
        list,
        &[(99, tag::INT), (out, tag::REFERENCE)],
        into,
    );
    assert_eq!(found, (1, tag::MIXED));
    assert_eq!(b.quest(into).id, 4);

    // Pop: the first record, copied out and removed; push adds at the end.
    let (_, popped, _) = b.call(LIST, 0x05, list, &[(out, tag::REFERENCE)], into);
    assert_eq!(popped, (1, tag::MIXED));
    assert_eq!(b.quest(into).id, 2);
    let six = b.record(given(6, 100.0, underway));
    let (_, pushed, _) = b.call(LIST, 0x01, list, &[(six, tag::REFERENCE)], 0);
    assert_eq!(pushed, (1, tag::MIXED));
    assert_eq!(
        b.ids(list).iter().map(|x| x.0).collect::<Vec<_>>(),
        [4, 1, 5, 6]
    );
    // Pushing keeps a second record of a quest; erasing takes out the last of them.
    let again = b.record(given(4, 1.0, 0x2000_0000));
    b.call(LIST, 0x01, list, &[(again, tag::REFERENCE)], 0);
    let (_, erased, _) = b.call(LIST, 0x04, list, &[(again, tag::REFERENCE)], 0);
    assert_eq!(erased, (1, tag::MIXED));
    assert_eq!(
        b.ids(list),
        [(4, underway), (1, underway), (5, underway), (6, underway)]
    );

    // An empty list pops nothing and leaves the out argument's record alone.
    let empty = b.list();
    let (_, popped, _) = b.call(LIST, 0x05, empty, &[(out, tag::REFERENCE)], into);
    assert_eq!(popped, (0, tag::MIXED));
    assert_eq!(b.quest(into).id, 2);

    // A null receiver crashes the call.
    let (s, _, _) = b.call(LIST, 0x06, NULL, &[], 0);
    assert_eq!(s, Status::Crash);
}

fn named(id: u32) -> TextInfo {
    TextInfo {
        string_id: id,
        table: 0x2300_0001,
        ..TextInfo::default()
    }
}

/// Behaviour: script.glue.quest-record-natives
#[test]
fn a_quest_record_is_filled_whole_and_read_field_by_field() {
    let mut b = Bench::new();
    // A new record: never given or done, an update plays the quest's effect.
    let q = b.record(QuestInfo::default());
    assert_eq!(b.quest(q).bestowed_at(), -1.0);
    assert_eq!(b.quest(q).done_at(), -1.0);
    let (_, fx, _) = b.call(QUEST, 0x08, q, &[], 0);
    assert_eq!(fx, (1, tag::MIXED));

    let texts: Vec<u32> = [11, 12, 13]
        .iter()
        .map(|k| {
            b.vm.heap
                .add_native(native_type::STRING_INFO, NativeObject::TextInfo(named(*k)))
        })
        .collect();
    // A 64-bit argument: its high word nearer the receiver, its low word below it.
    let long = |v: f64| {
        let bits = v.to_bits();
        #[allow(clippy::cast_possible_truncation)]
        [
            ((bits >> 32) as u32, tag::DOUBLE),
            (bits as u32, tag::DOUBLE),
        ]
    };
    // Init: id, name, description, icon, phase, journal entry, status, challenge, given at,
    // maxed out, expires, time left, done at, time before a retry, effect, may be cancelled.
    let mut args = vec![
        (0x4000_0123, tag::MIXED),
        (texts[0], tag::REFERENCE),
        (texts[1], tag::REFERENCE),
        (0x0600_1234, tag::MIXED),
        (3, tag::MIXED),
        (texts[2], tag::REFERENCE),
        (0x3000_0000, tag::MIXED),
        ((-999i32).cast_unsigned(), tag::MIXED),
    ];
    args.extend(long(500.0));
    args.extend([(1, tag::MIXED), (1, tag::MIXED)]);
    args.extend(long(60.5));
    args.extend(long(900.0));
    args.extend(long(-3.75));
    args.extend([(0, tag::MIXED), (1, tag::MIXED)]);
    let (s, _, _) = b.call(QUEST, 0x05, q, &args, 0);
    assert_eq!(s, Status::Success);
    let filled = b.quest(q);
    assert_eq!(
        filled,
        QuestInfo {
            id: 0x4000_0123,
            name: named(11),
            description: named(12),
            icon: 0x0600_1234,
            challenge: -999,
            status: 0x3000_0000,
            phase: 3,
            journal: named(13),
            bestowed: 500f64.to_bits(),
            done: 900f64.to_bits(),
            expires: 1,
            maxed_out: 1,
            seconds_remaining: 60.5f64.to_bits(),
            seconds_until_retry: (-3.75f64).to_bits(),
            plays_fx: 0,
            cancellable: 1,
        }
    );
    // Done, so its time before a retry counts, cut toward zero; and it is maxed out.
    assert_eq!(filled.seconds_remaining(), -3);
    assert!(filled.maxed_out());

    let (_, id, _) = b.call(QUEST, 0x01, q, &[], 0);
    assert_eq!(id, (0x4000_0123, tag::MIXED));
    let (_, status, _) = b.call(QUEST, 0x06, q, &[], 0);
    assert_eq!(status, (0x3000_0000, tag::MIXED));
    for (number, want) in [(0x02, 11), (0x07, 12), (0x03, 13)] {
        let (_, (r, t), _) = b.call(QUEST, number, q, &[], 0);
        assert_eq!(t, tag::REFERENCE);
        assert!(!texts.contains(&r), "a new string reference");
        assert_eq!(
            b.vm.heap.native(r).unwrap(),
            &NativeObject::TextInfo(named(want))
        );
    }
}

/// The quest and story records written out for an admin, each dump called as the machine calls it.
mod dumps {
    use auberean_script::native_object::{native_type, NativeObject, QuestInfo, TextInfo};
    use auberean_script::story::{track, ActInfo, Keyframe, SceneInfo};
    use auberean_script::Status;

    use crate::call::Bench;

    const ACT_INFO: u32 = 0x9C;
    const KEYFRAME: u32 = 0x9F;
    const QUEST_INFO: u32 = 0xA0;
    const QUEST_INFO_LIST: u32 = 0xA1;
    const SCENE_INFO: u32 = 0xA3;

    const UNDER_WAY: u32 = 0x1000_0000;
    const COMPLETED: u32 = 0x3000_0000;

    fn dump(b: &mut Bench, package: u32, number: u32, receiver: u32) -> String {
        let (status, r) = b.call(package, number, receiver, &[], 1);
        assert_eq!(status, Status::Success);
        String::from_utf16_lossy(&b.text_of(r[0].0))
    }

    fn literal(s: &str) -> TextInfo {
        TextInfo {
            has_literal: 1,
            literal: Some(s.encode_utf16().collect()),
            ..TextInfo::default()
        }
    }

    fn quest(status: u32) -> QuestInfo {
        QuestInfo {
            id: 0x4000_0123,
            name: TextInfo::default(),
            description: TextInfo::default(),
            icon: 0,
            challenge: 7,
            status,
            phase: 3,
            journal: TextInfo::default(),
            bestowed: 1.5f64.to_bits(),
            done: 2.5f64.to_bits(),
            expires: 0,
            maxed_out: 0,
            seconds_remaining: 60.0f64.to_bits(),
            seconds_until_retry: 90.0f64.to_bits(),
            plays_fx: 0,
            cancellable: 1,
        }
    }

    const QUEST_HEAD: &str = "\n-- quest contains: --\n  QuestID: 0x40000123.\n  Quest Status: \
                              0x10000000.\n  Current Phase: 3.\n  Challenge Level: .\n  Bestowal \
                              Timestamp: 1.500000.\n";

    /// **A quest dumps as the client writes it**: under way, the time left and the cancel line only
    /// when it runs out; otherwise when it was done and the time before it may be taken again, and
    /// whether it may be taken no more. The client's formats for the challenge and the cancel line
    /// take no value: the first shows none, the second the letter. A list numbers its quests from 1.
    ///
    /// Behaviour: script.glue.a-quest-dumps-as-the-client-writes-it
    #[test]
    fn a_quest_dumps_as_the_client_writes_it() {
        let mut b = Bench::new();
        let plain = b.object(
            native_type::QUEST_INFO,
            NativeObject::Quest(Box::new(quest(UNDER_WAY))),
        );
        assert_eq!(
            dump(&mut b, QUEST_INFO, 4, plain),
            format!("{QUEST_HEAD}-----------------------\n")
        );
        let timed = b.object(
            native_type::QUEST_INFO,
            NativeObject::Quest(Box::new(QuestInfo {
                expires: 1,
                ..quest(UNDER_WAY)
            })),
        );
        assert_eq!(
            dump(&mut b, QUEST_INFO, 4, timed),
            format!(
                "{QUEST_HEAD}    Seconds remaining: 60.000000.\n    Can Be Cancelled: b.\n\
                 -----------------------\n"
            )
        );
        let done = QuestInfo {
            maxed_out: 1,
            ..quest(COMPLETED)
        };
        let done_text = "\n-- quest contains: --\n  QuestID: 0x40000123.\n  Quest Status: \
                         0x30000000.\n  Current Phase: 3.\n  Challenge Level: .\n  Bestowal \
                         Timestamp: 1.500000.\n  Finished Timestamp: 2.500000.\n  Seconds until \
                         retry: 90.000000.\n  Quest is maxed out.\n-----------------------\n";
        let r = b.object(
            native_type::QUEST_INFO,
            NativeObject::Quest(Box::new(done.clone())),
        );
        assert_eq!(dump(&mut b, QUEST_INFO, 4, r), done_text);
        let list = b.object(
            native_type::QUEST_INFO_LIST,
            NativeObject::QuestList(vec![done]),
        );
        assert_eq!(
            dump(&mut b, QUEST_INFO_LIST, 7, list),
            format!(
                "-- quest list elements: --\n  Element 1) {done_text}\n-- end of the quest list \
                 elements --\n"
            )
        );
    }

    fn step() -> Keyframe {
        Keyframe {
            scene: 1,
            number: 2,
            duration: 1500.0f64.to_bits(),
            video: 0x10,
            audio: 0x20,
            voice: 0x30,
            subtitle: literal("words"),
            movie: "film".encode_utf16().collect(),
            tracks: track::VIDEO | track::VOICE | track::MOVIE,
        }
    }

    /// **A story's records dump as the client writes them**: a keyframe's number, its duration in the
    /// C library's exponent form, and only the tracks it defines; a scene's id, its act and its flags,
    /// then its keyframes; an act's flags and its scenes, each under a heading numbered from 1, or a
    /// line saying it has none.
    ///
    /// Behaviour: script.glue.a-storys-records-dump-as-the-client-writes-them
    #[test]
    fn a_storys_records_dump_as_the_client_writes_them() {
        let mut b = Bench::new();
        let k = b.object(
            native_type::KEYFRAME,
            NativeObject::Keyframe(Box::new(step())),
        );
        let key = "    Keyframe: 2\n     --Duration: 1.500000e+003 secs\n     --Video Track: \
                   0x00000010.\n     --Voice Track: 0x00000030.\n     --Text Track: film.\n    \
                   --------\n";
        assert_eq!(dump(&mut b, KEYFRAME, 1, k), key);

        let scene = SceneInfo {
            id: 0x51,
            name: literal("Opening"),
            act: 0x61,
            number: 4,
            hidden: 1,
            playable: 0,
            keyframes: vec![step()],
        };
        let scene_head =
            "\n-- scene contains: --\n  SceneID: 0x00000051.\n  Scene Name: (Opening).\n  \
                          Parent Act ID: 0x00000061 ().\n  Scene Number: 4.\n  Scene is hidden \
                          (locked).\n  Scene is not playable.\n\n";
        let scene_text = format!("{scene_head}{key}-----------------------\n");
        let s = b.object(
            native_type::SCENE_INFO,
            NativeObject::Scene(Box::new(scene.clone())),
        );
        assert_eq!(dump(&mut b, SCENE_INFO, 1, s), scene_text);

        let act = ActInfo {
            id: 0x61,
            number: 2,
            name: literal("First"),
            visible: 1,
            playable: 0,
            scenes: vec![scene],
        };
        let a = b.object(
            native_type::ACT_INFO,
            NativeObject::Act(Box::new(act.clone())),
        );
        assert_eq!(
            dump(&mut b, ACT_INFO, 4, a),
            format!(
                "\n-- act contains: --\n  Act ID: 0x00000061.\n  Act Name: (First).\n  Act Number: 2.\n  \
                 Act is visible.\n  Act is not playable.\n  Scene list contains 1 items:\n    \
                 ----------------\n    -- Scene 1 --\n    ----------------{scene_text}\n\n\
                 -----------------------\n"
            )
        );
        let empty = b.object(
            native_type::ACT_INFO,
            NativeObject::Act(Box::new(ActInfo {
                scenes: Vec::new(),
                ..act
            })),
        );
        assert!(dump(&mut b, ACT_INFO, 4, empty).contains(
            "Scene list contains 0 items:\n    < This Act has no scenes > \n\n-----------------------\n"
        ));
    }
}
