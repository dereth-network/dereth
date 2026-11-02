//! The built library, loaded as the retail client loads it (32-bit Windows only): by path, its
//! class factory found by name, one object created and queried for ten interfaces in the client's
//! order; then the client's calls, made through the interfaces' tables at the client's offsets,
//! against a world on loopback, and every result and event read at the offsets the client reads
//! them, by a stand-in for the client's own object that, like the client's, calls connect again from
//! inside the pump when it hears the connection is gone.

#![allow(unsafe_code)]

use core::ffi::c_void;
use core::ptr::{null, null_mut};
use std::path::PathBuf;

use auberean_friends_dll::ids::{self, Face, Guid};

use crate::common::{friends, ticket, until, LoopbackWorld, WORLD_NAME};

type Hresult = i32;

#[allow(clippy::cast_possible_wrap)]
const fn code(c: u32) -> Hresult {
    c as i32
}

const S_OK: Hresult = 0;
const E_PENDING: Hresult = code(0x8000_000A);
const E_NOTIMPL: Hresult = code(0x8000_4001);
const E_NOINTERFACE: Hresult = code(0x8000_4002);
const CLASS_E_CLASSNOTAVAILABLE: Hresult = code(0x8004_0111);
const BUFFER_TOO_SMALL: Hresult = code(0xC112_0002);

#[link(name = "kernel32")]
extern "system" {
    fn LoadLibraryW(name: *const u16) -> *mut c_void;
    fn GetProcAddress(module: *mut c_void, name: *const core::ffi::c_char) -> *mut c_void;
}

#[repr(C)]
struct Blob {
    size: u32,
    data: *mut u8,
}

#[repr(C)]
struct QueueEntry {
    flags: u32,
    kind: u32,
    param: u32,
    size: u32,
    message: *const u8,
}

/// A text as the client passes one: UTF-16 with a NUL.
fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain([0]).collect()
}

/// The built library: beside the test, or where `AUBEREAN_FRIENDS_DLL` says.
fn load() -> *mut c_void {
    let path = std::env::var_os("AUBEREAN_FRIENDS_DLL")
        .map(PathBuf::from)
        .or_else(|| {
            let exe = std::env::current_exe().ok()?;
            let deps = exe.parent()?;
            [
                deps.join("auberean_friends_dll.dll"),
                deps.parent()?.join("auberean_friends_dll.dll"),
            ]
            .into_iter()
            .find(|p| p.is_file())
        })
        .expect("the built library, beside the test");
    let name: Vec<u16> = path.to_string_lossy().encode_utf16().chain([0]).collect();
    // SAFETY: a NUL-terminated path.
    let module = unsafe { LoadLibraryW(name.as_ptr()) };
    assert!(!module.is_null(), "the library loads: {}", path.display());
    module
}

/// Entry `offset` of an interface's table, as a function of type `F`.
///
/// # Safety
/// `this` is an interface pointer whose table has an entry of type `F` at `offset`.
unsafe fn entry<F: Copy>(this: *mut c_void, offset: usize) -> F {
    // SAFETY: the interface's first word is its table.
    let table = unsafe { *this.cast::<*const usize>() };
    // SAFETY: the table holds the entry.
    let f = unsafe { *table.add(offset / 4) };
    // SAFETY: the entry is a function of type `F`, which is pointer-sized.
    unsafe { core::mem::transmute_copy::<usize, F>(&f) }
}

type GetClassObject =
    unsafe extern "system" fn(*const Guid, *const Guid, *mut *mut c_void) -> Hresult;
type Query = unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult;
type Count = unsafe extern "system" fn(*mut c_void) -> u32;
type CreateInstance =
    unsafe extern "system" fn(*mut c_void, *mut c_void, *const Guid, *mut *mut c_void) -> Hresult;
type Init = unsafe extern "system" fn(
    *mut c_void,
    *const Guid,
    *const u16,
    u32,
    *const u16,
    u32,
) -> Hresult;
type Connect = unsafe extern "system" fn(*mut c_void, *const u8, u32) -> Hresult;
type Bare = unsafe extern "system" fn(*mut c_void) -> Hresult;
type Pump = unsafe extern "system" fn(*mut c_void, *mut Sink) -> Hresult;
type WithContext = unsafe extern "system" fn(*mut c_void, usize) -> Hresult;
type Named = unsafe extern "system" fn(*mut c_void, *const u16, usize) -> Hresult;
type Flags = unsafe extern "system" fn(*mut c_void, u32, u32, u32, usize) -> Hresult;
type SetLoc = unsafe extern "system" fn(*mut c_void, u32, *const u16, usize) -> Hresult;
type Tell =
    unsafe extern "system" fn(*mut c_void, *const u16, *const u16, *const Blob, usize) -> Hresult;
type RoomSay =
    unsafe extern "system" fn(*mut c_void, u32, *const u16, *const Blob, usize) -> Hresult;
type TicketOut = unsafe extern "system" fn(*mut c_void, *mut Blob) -> Hresult;
type TicketIn = unsafe extern "system" fn(*mut c_void, *const Blob) -> Hresult;
type Present = unsafe extern "system" fn(*mut c_void, *const Blob, usize) -> Hresult;
type Call = unsafe extern "system" fn(
    *mut c_void,
    u32,
    *const u16,
    *const u16,
    *const Blob,
    u32,
    usize,
) -> Hresult;

/// A friends entry as read: name, state, location.
type Entry = (String, u32, String);

/// What the stand-in for the client read from one block.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Seen {
    Connect(u32),
    Result {
        kind: u32,
        context: u32,
        hr: u32,
        words: [u32; 2],
        friends: Vec<Entry>,
        names: Vec<String>,
    },
    FriendsStart(Vec<Entry>),
    FriendState(String, u32),
    Tell {
        user: String,
        text: String,
        extra: Vec<u8>,
    },
}

/// The stand-in for the client's object: its first word is a table whose first entry takes one
/// message.
#[repr(C)]
struct Sink {
    table: *const SinkTable,
    seen: Vec<Seen>,
    connection: *mut c_void,
    host: Vec<u8>,
    port: u16,
    connected_before: bool,
    reconnects: Vec<Hresult>,
}

#[repr(C)]
struct SinkTable {
    read: unsafe extern "system" fn(*mut Sink, *const QueueEntry) -> Hresult,
}

static SINK_TABLE: SinkTable = SinkTable { read };

/// The `u32` at `at` in a block.
///
/// # Safety
/// The block holds four bytes at `at`.
unsafe fn word(block: *const u8, at: usize) -> u32 {
    // SAFETY: as this function's.
    unsafe { block.add(at).cast::<u32>().read_unaligned() }
}

/// A text the client would copy: UTF-16 up to its NUL.
///
/// # Safety
/// `p` points to UTF-16 ending in a NUL.
unsafe fn text(p: *const u16) -> String {
    assert!(!p.is_null(), "a text is never NULL");
    let mut n = 0;
    // SAFETY: the text runs to its NUL.
    while unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: the `n` units were just read.
    String::from_utf16(unsafe { core::slice::from_raw_parts(p, n) }).expect("UTF-16")
}

/// A count and its friends entries, as the client copies them.
///
/// # Safety
/// `list` holds `n` 12-byte entries.
unsafe fn entries(n: u32, list: u32) -> Vec<Entry> {
    let list = list as usize as *const u8;
    (0..n as usize)
        .map(|i| {
            // SAFETY: entry `i` of the list.
            unsafe {
                let e = list.add(12 * i);
                (
                    text(word(e, 0) as usize as *const u16),
                    word(e, 4),
                    text(word(e, 8) as usize as *const u16),
                )
            }
        })
        .collect()
}

unsafe extern "system" fn read(sink: *mut Sink, queued: *const QueueEntry) -> Hresult {
    // SAFETY: the library passes back this object, and an entry alive for the call.
    let (sink, queued) = unsafe { (&mut *sink, &*queued) };
    let b = queued.message;
    // SAFETY: every block holds its header; each type's fields are read as the client reads them.
    unsafe {
        assert_eq!(*b.add(4), 1, "the block is marked valid");
        let kind = word(b, 8);
        let seen = match kind & 0xF00 {
            0x300 => match kind {
                0x1300..=0x1302 => Seen::Connect(kind),
                0x430C => Seen::FriendsStart(entries(word(b, 0x1C), word(b, 0x20))),
                0x430D => {
                    Seen::FriendState(text(word(b, 0x1C) as usize as *const u16), word(b, 0x20))
                }
                0x5317 => {
                    let blob = word(b, 0x28) as usize as *const u8;
                    assert!(!blob.is_null(), "a tell's extra data is never NULL");
                    let (size, data) = (word(blob, 0), word(blob, 4) as usize as *const u8);
                    let extra = if size == 0 {
                        Vec::new()
                    } else {
                        core::slice::from_raw_parts(data, size as usize).to_vec()
                    };
                    Seen::Tell {
                        user: text(word(b, 0x1C) as usize as *const u16),
                        text: text(word(b, 0x24) as usize as *const u16),
                        extra,
                    }
                }
                other => panic!("an event the client does not know: {other:#x}"),
            },
            0x200 => {
                let (count, list) = (word(b, 0x24), word(b, 0x28));
                let ok = word(b, 0x20) == 0;
                Seen::Result {
                    kind,
                    context: word(b, 0x1C),
                    hr: word(b, 0x20),
                    words: if matches!(kind, 0x4208 | 0xF201) {
                        [count, if kind == 0x4208 { list } else { 0 }]
                    } else {
                        [0, 0]
                    },
                    friends: if kind == 0x4200 && ok {
                        entries(count, list)
                    } else {
                        Vec::new()
                    },
                    names: if kind == 0x4203 && ok {
                        let list = list as usize as *const u32;
                        (0..count as usize)
                            .map(|i| text(*list.add(i) as usize as *const u16))
                            .collect()
                    } else {
                        Vec::new()
                    },
                }
            }
            other => panic!("neither a result nor an event: {other:#x}"),
        };
        // As the client does: a connection it had and lost, it asks for again at once.
        if matches!(seen, Seen::Connect(0x1301 | 0x1302)) && sink.connected_before {
            let connect: Connect = entry(sink.connection, 0x10);
            let hr = connect(sink.connection, sink.host.as_ptr(), u32::from(sink.port));
            sink.reconnects.push(hr);
        }
        if seen == Seen::Connect(0x1300) {
            sink.connected_before = true;
        }
        sink.seen.push(seen);
    }
    S_OK
}

/// The library's object, as the client holds it: the ten interfaces.
struct Client {
    faces: [*mut c_void; 10],
    sink: Box<Sink>,
}

impl Client {
    fn face(&self, f: Face) -> *mut c_void {
        self.faces[f.index()]
    }

    /// Load, create and query, as the client does at startup.
    fn start(port: u16) -> Self {
        let module = load();
        // SAFETY: the library's export, by name, called as the client calls it.
        unsafe {
            let get = GetProcAddress(module, c"DllGetClassObject".as_ptr());
            assert!(!get.is_null(), "the library exports its class factory");
            let get: GetClassObject = core::mem::transmute::<*mut c_void, GetClassObject>(get);
            let mut factory = null_mut();
            let other = Guid::new(1, 2, 3, [4; 8]);
            assert_eq!(
                get(&other, &ids::CLASS_FACTORY, &mut factory),
                CLASS_E_CLASSNOTAVAILABLE
            );
            assert_eq!(get(&ids::CLASS, &ids::CLASS_FACTORY, &mut factory), S_OK);
            assert!(!factory.is_null());
            let create: CreateInstance = entry(factory, 0x0C);
            let mut first = null_mut();
            let connection = Face::Connection.id();
            assert_eq!(create(factory, null_mut(), &connection, &mut first), S_OK);
            let release: Count = entry(factory, 0x08);
            release(factory);

            let mut faces = [null_mut(); 10];
            faces[0] = first;
            let query: Query = entry(first, 0x00);
            for f in &Face::ALL[1..] {
                let mut p = null_mut();
                assert_eq!(query(first, &f.id(), &mut p), S_OK, "{f:?}");
                assert!(!p.is_null());
                faces[f.index()] = p;
            }
            let mut p = null_mut();
            assert_eq!(query(first, &ids::CLASS, &mut p), E_NOINTERFACE);
            assert!(p.is_null());
            // Any interface answers for all the others.
            let query_chat: Query = entry(faces[Face::Chat.index()], 0x00);
            assert_eq!(
                query_chat(faces[Face::Chat.index()], &ids::UNKNOWN, &mut p),
                S_OK
            );
            assert_eq!(p, first);
            let release: Count = entry(p, 0x08);
            release(p);

            let mut host = b"127.0.0.1".to_vec();
            host.push(0);
            let sink = Box::new(Sink {
                table: &SINK_TABLE,
                seen: Vec::new(),
                connection: first,
                host,
                port,
                connected_before: false,
                reconnects: Vec::new(),
            });
            Self { faces, sink }
        }
    }

    fn pump(&mut self) {
        let c = self.face(Face::Connection);
        // SAFETY: the connection interface's pump, with the stand-in for the client's object.
        let hr = unsafe {
            let pump: Pump = entry(c, 0x20);
            pump(c, &raw mut *self.sink)
        };
        assert_eq!(hr, S_OK);
    }

    /// Pump until something is read that `f` picks.
    fn wait_for<T>(&mut self, what: &str, mut f: impl FnMut(&Seen) -> Option<T>) -> T {
        until(what, || {
            self.pump();
            let at = self.sink.seen.iter().position(|s| f(s).is_some())?;
            let seen = self.sink.seen.remove(at);
            f(&seen)
        })
    }

    fn result(&mut self, context: usize) -> Seen {
        let want = u32::try_from(context).expect("a context");
        self.wait_for("a result", |s| match s {
            Seen::Result { context, .. } if *context == want => Some(s.clone()),
            _ => None,
        })
    }

    fn connect_event(&mut self) -> u32 {
        self.wait_for("a connect event", |s| match s {
            Seen::Connect(k) => Some(*k),
            _ => None,
        })
    }

    /// The ticket, in the client's two calls.
    fn get_ticket(&self) -> Vec<u8> {
        let t = self.face(Face::Ticket);
        // SAFETY: the ticket interface's entry, with a blob of the client's.
        unsafe {
            let get: TicketOut = entry(t, 0x10);
            let mut blob = Blob {
                size: 0,
                data: null_mut(),
            };
            assert_eq!(get(t, &mut blob), BUFFER_TOO_SMALL, "first, the size");
            let mut buf = vec![0u8; blob.size as usize];
            blob.data = buf.as_mut_ptr();
            assert_eq!(get(t, &mut blob), S_OK, "then the ticket");
            assert_eq!(blob.size as usize, buf.len());
            buf
        }
    }

    fn present(&self, ticket: &mut [u8], context: usize) {
        let t = self.face(Face::Ticket);
        // SAFETY: the ticket interface's entry, with the client's copy of the ticket.
        unsafe {
            let present: Present = entry(t, 0x1C);
            let blob = Blob {
                size: u32::try_from(ticket.len()).expect("a size"),
                data: ticket.as_mut_ptr(),
            };
            assert_eq!(present(t, &blob, context), S_OK);
        }
    }
}

fn result(kind: u32, context: u32, hr: u32) -> Seen {
    result_with(kind, context, hr, [0, 0], Vec::new(), Vec::new())
}

fn result_with(
    kind: u32,
    context: u32,
    hr: u32,
    words: [u32; 2],
    friends: Vec<Entry>,
    names: Vec<String>,
) -> Seen {
    Seen::Result {
        kind,
        context,
        hr,
        words,
        friends,
        names,
    }
}

fn entry_tuples() -> Vec<Entry> {
    friends()
        .into_iter()
        .map(|f| (f.user, f.mode.0, f.location))
        .collect()
}

/// Behaviour: friends.library.retail-loading
#[test]
fn the_built_library_loads_as_the_client_loads_it_and_keeps_its_ticket_inside_its_guard() {
    let client = Client::start(0);
    let t = client.face(Face::Ticket);
    // SAFETY: the ticket interface's entries, called as the client calls them.
    unsafe {
        let get: TicketOut = entry(t, 0x10);
        let mut blob = Blob {
            size: 0,
            data: null_mut(),
        };
        assert_eq!(get(t, &mut blob), code(0xC112_0806), "no ticket yet");

        // The ticket as the client hands it: the text, a NUL, then four guard bytes.
        let text = ticket("ann");
        let mut given = text.clone().into_bytes();
        given.push(0);
        let size = u32::try_from(given.len()).expect("a size");
        given.extend_from_slice(&[0xEF, 0xFE, 0xAD, 0xDE]);
        let set: TicketIn = entry(t, 0x18);
        let blob = Blob {
            size,
            data: given.as_mut_ptr(),
        };
        assert_eq!(set(t, &blob), S_OK);
        assert_eq!(
            &given[given.len() - 4..],
            &[0xEF, 0xFE, 0xAD, 0xDE],
            "the guard holds"
        );
        assert_eq!(
            client.get_ticket(),
            text.into_bytes(),
            "the ticket, without its NUL"
        );
    }
}

/// Behaviour: friends.library.retail-session
#[test]
fn the_built_library_serves_the_client_s_calls_and_hands_back_blocks_the_client_reads() {
    let world = LoopbackWorld::start();
    let mut client = Client::start(world.port);
    let (c, chat, fr, t, support) = (
        client.face(Face::Connection),
        client.face(Face::Chat),
        client.face(Face::Friends),
        client.face(Face::Ticket),
        client.face(Face::Support),
    );
    let name = |n: &str| wide(&format!("{n}@{WORLD_NAME}"));
    // SAFETY: every call below is one the client makes, through the entry it calls, with
    // arguments laid out as the client lays them out.
    unsafe {
        let mut given = format!("{}\0", ticket("ann")).into_bytes();
        let set: TicketIn = entry(t, 0x18);
        let blob = Blob {
            size: u32::try_from(given.len()).expect("a size"),
            data: given.as_mut_ptr(),
        };
        assert_eq!(set(t, &blob), S_OK);

        // The pump runs before the set up, with nothing to hand over.
        client.pump();
        assert!(client.sink.seen.is_empty());

        let init: Init = entry(c, 0x0C);
        let app = Guid::new(
            0x2280_767D,
            0x1689,
            0x4D6B,
            [0x81, 0xDD, 0x3A, 0xD9, 0xE8, 0x0F, 0xB6, 0x55],
        );
        let (version, key) = (wide("1.0"), wide("XXXX-YYYY-ZZZZ"));
        assert_eq!(init(c, &app, version.as_ptr(), 1033, key.as_ptr(), 0), S_OK);
        let connect: Connect = entry(c, 0x10);
        let host = client.sink.host.clone();
        assert_eq!(connect(c, host.as_ptr(), u32::from(world.port)), E_PENDING);
        assert_eq!(client.connect_event(), 0x1300);

        let mut copy = client.get_ticket();
        client.present(&mut copy, 0x0100_0010);
        assert_eq!(client.result(0x0100_0010), result(0xF201, 0x0100_0010, 0));
        let start = client.wait_for("the friends at entry", |s| match s {
            Seen::FriendsStart(f) => Some(f.clone()),
            _ => None,
        });
        assert_eq!(start, entry_tuples());

        let flags: Flags = entry(fr, 0x24);
        assert_eq!(flags(fr, 1, 1, 1, 0x0100_0020), S_OK);
        assert_eq!(
            client.result(0x0100_0020),
            result_with(0x4208, 0x0100_0020, 0, [1, 1], Vec::new(), Vec::new())
        );

        let get_friends: WithContext = entry(fr, 0x0C);
        assert_eq!(get_friends(fr, 0x0100_0030), S_OK);
        assert_eq!(
            client.result(0x0100_0030),
            result_with(0x4200, 0x0100_0030, 0, [0, 0], entry_tuples(), Vec::new())
        );

        let get_ignored: WithContext = entry(fr, 0x18);
        assert_eq!(get_ignored(fr, 0x0100_0040), S_OK);
        assert_eq!(
            client.result(0x0100_0040),
            result_with(
                0x4203,
                0x0100_0040,
                0,
                [0, 0],
                Vec::new(),
                vec![format!("Mean@{WORLD_NAME}")]
            )
        );

        let add: Named = entry(fr, 0x10);
        assert_eq!(add(fr, name("Dan").as_ptr(), 0x0100_0050), S_OK);
        assert_eq!(client.result(0x0100_0050), result(0x4201, 0x0100_0050, 0));
        assert_eq!(
            client.wait_for("the new friend's state", |s| match s {
                Seen::FriendState(u, m) => Some((u.clone(), *m)),
                _ => None,
            }),
            (format!("Dan@{WORLD_NAME}"), 1)
        );
        for (offset, context, kind) in [
            (0x14, 0x0100_0060, 0x4202),
            (0x1C, 0x0100_0070, 0x4204),
            (0x20, 0x0100_0080, 0x4205),
        ] {
            let f: Named = entry(fr, offset);
            assert_eq!(f(fr, name("Dan").as_ptr(), context), S_OK);
            assert_eq!(
                client.result(context),
                result(kind, u32::try_from(context).expect("fits"), 0)
            );
        }
        let set_loc: SetLoc = entry(fr, 0x2C);
        assert_eq!(set_loc(fr, 2, wide("").as_ptr(), 0x0100_0090), S_OK);
        assert_eq!(client.result(0x0100_0090), result(0x420A, 0x0100_0090, 0));

        // A tell to Echo, with the client's extra data, comes back from Echo.
        let mut extra = vec![1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let blob = Blob {
            size: 12,
            data: extra.as_mut_ptr(),
        };
        let tell: Tell = entry(chat, 0x58);
        assert_eq!(
            tell(
                chat,
                name("Echo").as_ptr(),
                wide("hello").as_ptr(),
                &blob,
                0x0100_00A0
            ),
            S_OK
        );
        assert_eq!(client.result(0x0100_00A0), result(0x521D, 0x0100_00A0, 0));
        let mut padded = extra.clone();
        padded.resize(16, 0);
        assert_eq!(
            client.wait_for("the tell back", |s| match s {
                Seen::Tell { .. } => Some(s.clone()),
                _ => None,
            }),
            Seen::Tell {
                user: format!("Echo@{WORLD_NAME}"),
                text: "hello".into(),
                extra: padded,
            }
        );
        let none = Blob {
            size: 0,
            data: null_mut(),
        };
        assert_eq!(
            tell(
                chat,
                name("Bob").as_ptr(),
                wide("hi").as_ptr(),
                &none,
                0x0100_00B0
            ),
            S_OK
        );
        assert_eq!(
            client.result(0x0100_00B0),
            result(0x521D, 0x0100_00B0, 0xC112_0011)
        );

        // Rooms and help calls are not this service's.
        let room: RoomSay = entry(chat, 0x54);
        assert_eq!(
            room(chat, 1, wide("hi").as_ptr(), &none, 0x0100_00C0),
            E_NOTIMPL
        );
        let call: Call = entry(support, 0x0C);
        assert_eq!(
            call(support, 1, null(), null(), &none, 1033, 0x0100_00D0),
            E_NOTIMPL
        );

        // The world goes: one lost event, the client's connect from inside the pump is pending,
        // and the library comes back on its own first wait.
        world.cut();
        assert_eq!(client.connect_event(), 0x1302);
        assert_eq!(client.sink.reconnects, [E_PENDING]);
        assert_eq!(client.connect_event(), 0x1300);
        let mut copy = client.get_ticket();
        client.present(&mut copy, 0x0100_00E0);
        assert_eq!(client.result(0x0100_00E0), result(0xF201, 0x0100_00E0, 0));
        let again = client.wait_for("the friends again", |s| match s {
            Seen::FriendsStart(f) => Some(f.clone()),
            _ => None,
        });
        assert_eq!(
            again,
            entry_tuples(),
            "the world sends the friends again after a reconnect"
        );

        // At exit: done with the ticket, disconnect, and release all but the help calls.
        let done: WithContext = entry(t, 0x20);
        assert_eq!(done(t, 0x0100_00F0), S_OK);
        assert_eq!(client.result(0x0100_00F0), result(0xF202, 0x0100_00F0, 0));
        let disconnect: Bare = entry(c, 0x14);
        assert_eq!(disconnect(c), S_OK);
        for f in Face::ALL.into_iter().filter(|f| *f != Face::Support) {
            let p = client.face(f);
            let release: Count = entry(p, 0x08);
            release(p);
        }
    }
    assert!(
        client.sink.seen.is_empty(),
        "nothing more: {:?}",
        client.sink.seen
    );
}
