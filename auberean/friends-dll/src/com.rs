//! The client's side: the class factory the client asks for, the object, and its ten interfaces'
//! tables, built for the 32-bit Windows client.
//!
//! The client loads the library by path, asks [`DllGetClassObject`] for the class factory of
//! [`ids::CLASS`], creates one object for the connection interface and queries it for the other
//! nine. Every entry is `extern "system"` (COM's calling convention on 32-bit Windows, the callee
//! clearing the stack) with the interface first, and answers an `HRESULT`; each turns the client's
//! arguments into owned values at once (the client's texts live only for the call) and hands them
//! to the [`Host`]. Results and events reach the client only inside the pump, through the first
//! entry of the object the client passes to it, which is not a COM object: the library never
//! counts references on it. Every entry catches a panic and answers `E_FAIL`; none holds the lock
//! while it calls the client.
//!
//! The entries the client never calls answer `E_NOTIMPL`. Those whose arguments are not known take
//! the interface alone: they are there so that each table is as long as the client's interface.

#![allow(unsafe_code)]

use core::ffi::c_void;
use core::panic::AssertUnwindSafe;
use core::ptr::null_mut;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Mutex, MutexGuard, PoisonError};

use stream::Request;

use crate::engine::{self, Context, Message};
use crate::host::Host;
use crate::ids::{self, Face, Guid};
use crate::layout;

type Hresult = i32;

/// A code as the `HRESULT` it is.
#[allow(clippy::cast_possible_wrap)]
const fn code(c: u32) -> Hresult {
    c as i32
}

const S_OK: Hresult = 0;
const S_FALSE: Hresult = 1;
const E_PENDING: Hresult = code(0x8000_000A);
const E_NOTIMPL: Hresult = code(0x8000_4001);
const E_NOINTERFACE: Hresult = code(0x8000_4002);
const E_POINTER: Hresult = code(0x8000_4003);
const E_FAIL: Hresult = code(0x8000_4005);
const CLASS_E_NOAGGREGATION: Hresult = code(0x8004_0110);
const CLASS_E_CLASSNOTAVAILABLE: Hresult = code(0x8004_0111);
/// The client's code for a buffer too small, answered with the size it needs.
const BUFFER_TOO_SMALL: Hresult = code(0xC112_0002);
/// No ticket: the client was started without one.
const NO_TICKET: Hresult = code(stream::Hr::TICKET_REFUSED.0);

/// The longest text read from the client, in UTF-16 units: a bound against a text with no end.
const MAX_TEXT: usize = 64 * 1024;

/// The most messages one pump hands over, so that a client that asks for more inside each one
/// cannot keep a pump going for ever. What is left waits for the next pump.
const MAX_PER_PUMP: usize = 1_000;

/// Bytes with their size, as Windows lays them out.
#[repr(C)]
#[derive(Debug)]
struct Blob {
    size: u32,
    data: *mut u8,
}

/// The entry the pump hands each message in.
#[repr(C)]
#[derive(Debug)]
struct QueueEntry {
    flags: u32,
    kind: u32,
    param: u32,
    size: u32,
    message: *mut c_void,
}

/// The first entry of the client's object: it takes one message.
type Read = unsafe extern "system" fn(sink: *mut c_void, entry: *mut QueueEntry) -> Hresult;

type Query = unsafe extern "system" fn(*mut c_void, *const Guid, *mut *mut c_void) -> Hresult;
type Count = unsafe extern "system" fn(*mut c_void) -> u32;
type Bare = unsafe extern "system" fn(*mut c_void) -> Hresult;
type WithContext = unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult;
type Named = unsafe extern "system" fn(*mut c_void, *const u16, *mut c_void) -> Hresult;
type Out = unsafe extern "system" fn(*mut c_void, *mut u32) -> Hresult;

/// The object: one table pointer for each interface, in [`Face::ALL`]'s order, the count of
/// references, and the library's state.
#[repr(C)]
struct Library {
    faces: [*const c_void; 10],
    refs: AtomicU32,
    host: Mutex<Host>,
}

impl Library {
    fn new() -> Self {
        Self {
            faces: [
                (&raw const CONNECTION).cast(),
                (&raw const ACCOUNTS).cast(),
                (&raw const NEWS).cast(),
                (&raw const CHAT).cast(),
                (&raw const FRIENDS).cast(),
                (&raw const HOSTING).cast(),
                (&raw const BROWSER).cast(),
                (&raw const PERSONAS).cast(),
                (&raw const TICKET).cast(),
                (&raw const SUPPORT).cast(),
            ],
            refs: AtomicU32::new(1),
            host: Mutex::new(Host::new()),
        }
    }

    fn host(&self) -> MutexGuard<'_, Host> {
        self.host.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// An interface's pointer: the place of its table in the object.
fn face(library: *mut Library, face: Face) -> *mut c_void {
    library
        .cast::<*const c_void>()
        .wrapping_add(face.index())
        .cast()
}

/// The object behind interface `FACE`'s pointer.
///
/// # Safety
/// `this` is a pointer this library handed out for interface `FACE`, and the object is alive.
unsafe fn object(this: *mut c_void, face: usize) -> *mut Library {
    this.cast::<*const c_void>().wrapping_sub(face).cast()
}

/// The object behind interface `FACE`'s pointer, borrowed.
///
/// # Safety
/// As [`object`].
unsafe fn library<'a>(this: *mut c_void, face: usize) -> &'a Library {
    // SAFETY: the caller holds a live interface pointer, which sits `face` pointers into the object.
    unsafe { &*object(this, face) }
}

/// Run an entry, answering `E_FAIL` for a panic.
fn guard(f: impl FnOnce() -> Hresult) -> Hresult {
    std::panic::catch_unwind(AssertUnwindSafe(f)).unwrap_or(E_FAIL)
}

/// A text from the client: UTF-16 up to its NUL; none for NULL.
///
/// # Safety
/// `p` is NULL or points to UTF-16 ending in a NUL.
unsafe fn wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: the text runs to its NUL, and the count stops there.
    while n < MAX_TEXT && unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: the `n` units before the NUL were just read.
    String::from_utf16_lossy(unsafe { core::slice::from_raw_parts(p, n) })
}

/// A text from the client in its code page (the world's host): bytes up to the NUL.
///
/// # Safety
/// `p` is NULL or points to bytes ending in a NUL.
unsafe fn narrow(p: *const u8) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut n = 0;
    // SAFETY: the text runs to its NUL, and the count stops there.
    while n < MAX_TEXT && unsafe { *p.add(n) } != 0 {
        n += 1;
    }
    // SAFETY: the `n` bytes before the NUL were just read.
    String::from_utf8_lossy(unsafe { core::slice::from_raw_parts(p, n) }).into_owned()
}

/// The bytes of the client's blob, copied; none for NULL.
///
/// # Safety
/// `b` is NULL or points to a blob whose data, when not NULL, holds its size in bytes.
unsafe fn blob(b: *const Blob) -> Vec<u8> {
    // SAFETY: the blob is the caller's, alive for the call.
    let Some(b) = (unsafe { b.as_ref() }) else {
        return Vec::new();
    };
    if b.data.is_null() || b.size == 0 {
        return Vec::new();
    }
    let size = usize::try_from(b.size).unwrap_or(0);
    // SAFETY: the blob's data holds its size in bytes.
    unsafe { core::slice::from_raw_parts(b.data, size) }.to_vec()
}

/// The client's context, as the opaque 32 bits it hands back.
fn context(ctx: *mut c_void) -> Context {
    u32::try_from(ctx as usize).unwrap_or(0)
}

/// A port pushed as 32 bits: its low 16.
fn port(p: u32) -> u16 {
    u16::try_from(p & 0xFFFF).unwrap_or(0)
}

/// A request with the client's context, made on the object behind interface `face`.
///
/// # Safety
/// As [`object`].
unsafe fn ask(this: *mut c_void, face: Face, ctx: *mut c_void, request: Request) -> Hresult {
    // SAFETY: as this function's.
    let library = unsafe { library(this, face.index()) };
    library.host().request(context(ctx), request);
    S_OK
}

// The three entries every interface starts with.

#[repr(C)]
struct Unknown {
    query: Query,
    add_ref: Count,
    release: Count,
}

const fn unknown<const FACE: usize>() -> Unknown {
    Unknown {
        query: query::<FACE>,
        add_ref: add_ref::<FACE>,
        release: release::<FACE>,
    }
}

unsafe extern "system" fn query<const FACE: usize>(
    this: *mut c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    guard(|| {
        if out.is_null() {
            return E_POINTER;
        }
        // SAFETY: `out` is the caller's place for the answer.
        unsafe { *out = null_mut() };
        // SAFETY: `iid` is NULL or the caller's identifier.
        let Some(id) = (unsafe { iid.as_ref() }) else {
            return E_POINTER;
        };
        let Some(wanted) = Face::of(id) else {
            return E_NOINTERFACE;
        };
        // SAFETY: `this` is interface `FACE`'s pointer.
        let object = unsafe { object(this, FACE) };
        // SAFETY: the object is alive while the caller holds an interface of it.
        unsafe { &*object }.refs.fetch_add(1, Ordering::Relaxed);
        // SAFETY: as above.
        unsafe { *out = face(object, wanted) };
        S_OK
    })
}

unsafe extern "system" fn add_ref<const FACE: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is interface `FACE`'s pointer to a live object.
    unsafe { library(this, FACE) }
        .refs
        .fetch_add(1, Ordering::Relaxed)
        .wrapping_add(1)
}

unsafe extern "system" fn release<const FACE: usize>(this: *mut c_void) -> u32 {
    // SAFETY: `this` is interface `FACE`'s pointer to a live object.
    let object = unsafe { object(this, FACE) };
    // SAFETY: as above.
    let left = unsafe { &*object }
        .refs
        .fetch_sub(1, Ordering::AcqRel)
        .saturating_sub(1);
    if left == 0 {
        // SAFETY: the last reference is gone: the object made by `Box::into_raw` is freed once.
        let _ =
            std::panic::catch_unwind(AssertUnwindSafe(|| drop(unsafe { Box::from_raw(object) })));
    }
    left
}

unsafe extern "system" fn not_implemented(_this: *mut c_void) -> Hresult {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_out(_this: *mut c_void, _out: *mut u32) -> Hresult {
    E_NOTIMPL
}

unsafe extern "system" fn not_implemented_named(
    _this: *mut c_void,
    _user: *const u16,
    _ctx: *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// A table for an interface the client only queries and releases.
#[repr(C)]
struct Held<const N: usize> {
    unknown: Unknown,
    unused: [Bare; N],
}

const fn held<const FACE: usize, const N: usize>() -> Held<N> {
    Held {
        unknown: unknown::<FACE>(),
        unused: [not_implemented as Bare; N],
    }
}

const CONNECTION_FACE: usize = Face::Connection.index();
const CHAT_FACE: usize = Face::Chat.index();
const FRIENDS_FACE: usize = Face::Friends.index();
const TICKET_FACE: usize = Face::Ticket.index();
const SUPPORT_FACE: usize = Face::Support.index();

static ACCOUNTS: Held<7> = held::<{ Face::Accounts.index() }, 7>();
static NEWS: Held<1> = held::<{ Face::News.index() }, 1>();
static HOSTING: Held<11> = held::<{ Face::Hosting.index() }, 11>();
static BROWSER: Held<4> = held::<{ Face::Browser.index() }, 4>();
static PERSONAS: Held<5> = held::<{ Face::Personas.index() }, 5>();

// The connection: set up, connect, disconnect, and the pump.

#[repr(C)]
struct ConnectionTable {
    unknown: Unknown,
    init: unsafe extern "system" fn(
        *mut c_void,
        *const Guid,
        *const u16,
        u32,
        *const u16,
        u32,
    ) -> Hresult,
    connect: unsafe extern "system" fn(*mut c_void, *const u8, u32) -> Hresult,
    disconnect: Bare,
    state: Out,
    last_error: Out,
    pump: unsafe extern "system" fn(*mut c_void, *mut c_void) -> Hresult,
}

static CONNECTION: ConnectionTable = ConnectionTable {
    unknown: unknown::<CONNECTION_FACE>(),
    init,
    connect,
    disconnect,
    state: not_implemented_out,
    last_error: not_implemented_out,
    pump,
};

/// Set up, at every login: the application, its version, the locale, a key (ignored) and a local
/// port (ignored).
unsafe extern "system" fn init(
    this: *mut c_void,
    _app: *const Guid,
    _version: *const u16,
    locale: u32,
    _key: *const u16,
    _local_port: u32,
) -> Hresult {
    guard(|| {
        // SAFETY: `this` is the connection interface's pointer.
        unsafe { library(this, CONNECTION_FACE) }
            .host()
            .init(locale);
        S_OK
    })
}

/// Connect to the world's host and port: always pending, answered by one connect event.
unsafe extern "system" fn connect(this: *mut c_void, host: *const u8, port_: u32) -> Hresult {
    guard(|| {
        // SAFETY: the host is the client's text, alive for the call.
        let host = unsafe { narrow(host) };
        // SAFETY: `this` is the connection interface's pointer.
        unsafe { library(this, CONNECTION_FACE) }
            .host()
            .connect(&host, port(port_));
        E_PENDING
    })
}

/// The client is leaving.
unsafe extern "system" fn disconnect(this: *mut c_void) -> Hresult {
    guard(|| {
        // SAFETY: `this` is the connection interface's pointer.
        unsafe { library(this, CONNECTION_FACE) }
            .host()
            .disconnect();
        S_OK
    })
}

/// Hand the client everything that came, each through the first entry of `sink`.
unsafe extern "system" fn pump(this: *mut c_void, sink: *mut c_void) -> Hresult {
    guard(|| {
        // SAFETY: `this` is the connection interface's pointer.
        let library = unsafe { library(this, CONNECTION_FACE) };
        library.host().intake();
        if sink.is_null() {
            return S_OK;
        }
        for _ in 0..MAX_PER_PUMP {
            let next = library.host().next_message();
            let Some(message) = next else {
                break;
            };
            // SAFETY: the sink is the client's object, alive for the pump.
            unsafe { deliver(sink, &message) };
        }
        S_OK
    })
}

/// Build a message in the client's layout, hand it over, and free it when the client returns.
///
/// # Safety
/// `sink`'s first word points to a table whose first entry is a [`Read`].
unsafe fn deliver(sink: *mut c_void, message: &Message) {
    let block = layout::block(message);
    let mut words = vec![0u32; block.words()];
    let base = u32::try_from(words.as_ptr() as usize).unwrap_or(0);
    block.fill(base, &mut words);
    let mut entry = QueueEntry {
        flags: 4,
        kind: 1,
        param: 0,
        size: u32::try_from(block.len()).unwrap_or(0),
        message: words.as_mut_ptr().cast(),
    };
    // SAFETY: the sink's first word is its table, whose first entry takes a message.
    let read: Read = unsafe { *(*sink.cast::<*const Read>()) };
    // SAFETY: the entry and the block it points to live until the client returns.
    unsafe { read(sink, &raw mut entry) };
    drop(words);
}

// Tells.

#[repr(C)]
struct ChatTable {
    unknown: Unknown,
    unused: [Bare; 18],
    room_say: unsafe extern "system" fn(
        *mut c_void,
        u32,
        *const u16,
        *const Blob,
        *mut c_void,
    ) -> Hresult,
    tell: unsafe extern "system" fn(
        *mut c_void,
        *const u16,
        *const u16,
        *const Blob,
        *mut c_void,
    ) -> Hresult,
}

static CHAT: ChatTable = ChatTable {
    unknown: unknown::<CHAT_FACE>(),
    unused: [not_implemented as Bare; 18],
    room_say,
    tell,
};

/// A line to a room: the rooms are the game's own, never this service's.
unsafe extern "system" fn room_say(
    _this: *mut c_void,
    _room: u32,
    _text: *const u16,
    _extra: *const Blob,
    _ctx: *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

/// A tell to `user`, with the client's extra data.
unsafe extern "system" fn tell(
    this: *mut c_void,
    user: *const u16,
    text: *const u16,
    extra: *const Blob,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the texts and the blob are the client's, alive for the call.
        let (user, text, extra) = unsafe { (wide(user), wide(text), blob(extra)) };
        let extra = engine::extra(&extra);
        // SAFETY: `this` is the chat interface's pointer.
        unsafe {
            ask(
                this,
                Face::Chat,
                ctx,
                Request::SendToUserEx { user, text, extra },
            )
        }
    })
}

// Friends, the ignore list and the player's own state.

#[repr(C)]
struct FriendsTable {
    unknown: Unknown,
    friends: WithContext,
    befriend: Named,
    unfriend: Named,
    ignored: WithContext,
    ignore: Named,
    unignore: Named,
    state_flags: unsafe extern "system" fn(*mut c_void, u32, u32, u32, *mut c_void) -> Hresult,
    where_is: Named,
    set_state: unsafe extern "system" fn(*mut c_void, u32, *const u16, *mut c_void) -> Hresult,
    by_byte: unsafe extern "system" fn(*mut c_void, u32, *const u16, *mut c_void) -> Hresult,
}

static FRIENDS: FriendsTable = FriendsTable {
    unknown: unknown::<FRIENDS_FACE>(),
    friends,
    befriend,
    unfriend,
    ignored,
    ignore,
    unignore,
    state_flags,
    where_is: not_implemented_named,
    set_state,
    by_byte,
};

unsafe extern "system" fn friends(this: *mut c_void, ctx: *mut c_void) -> Hresult {
    // SAFETY: `this` is the friends interface's pointer.
    guard(|| unsafe { ask(this, Face::Friends, ctx, Request::GetFriendsList) })
}

unsafe extern "system" fn befriend(
    this: *mut c_void,
    user: *const u16,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the name is the client's text, alive for the call.
        let user = unsafe { wide(user) };
        // SAFETY: `this` is the friends interface's pointer.
        unsafe { ask(this, Face::Friends, ctx, Request::FriendAdd { user }) }
    })
}

unsafe extern "system" fn unfriend(
    this: *mut c_void,
    user: *const u16,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the name is the client's text, alive for the call.
        let user = unsafe { wide(user) };
        // SAFETY: `this` is the friends interface's pointer.
        unsafe { ask(this, Face::Friends, ctx, Request::FriendDelete { user }) }
    })
}

unsafe extern "system" fn ignored(this: *mut c_void, ctx: *mut c_void) -> Hresult {
    // SAFETY: `this` is the friends interface's pointer.
    guard(|| unsafe { ask(this, Face::Friends, ctx, Request::GetIgnoredList) })
}

unsafe extern "system" fn ignore(this: *mut c_void, user: *const u16, ctx: *mut c_void) -> Hresult {
    guard(|| {
        // SAFETY: the name is the client's text, alive for the call.
        let user = unsafe { wide(user) };
        // SAFETY: `this` is the friends interface's pointer.
        unsafe { ask(this, Face::Friends, ctx, Request::IgnoredAdd { user }) }
    })
}

unsafe extern "system" fn unignore(
    this: *mut c_void,
    user: *const u16,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the name is the client's text, alive for the call.
        let user = unsafe { wide(user) };
        // SAFETY: `this` is the friends interface's pointer.
        unsafe { ask(this, Face::Friends, ctx, Request::IgnoredDelete { user }) }
    })
}

/// The client's start-of-session state flags.
unsafe extern "system" fn state_flags(
    this: *mut c_void,
    op: u32,
    word: u32,
    flags: u32,
    ctx: *mut c_void,
) -> Hresult {
    // SAFETY: `this` is the friends interface's pointer.
    guard(|| unsafe { ask(this, Face::Friends, ctx, Request::Flags { op, word, flags }) })
}

/// The player's state: 1 available, 2 away, 4 not to be disturbed.
unsafe extern "system" fn set_state(
    this: *mut c_void,
    flags: u32,
    location: *const u16,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the location is the client's text, alive for the call.
        let location = unsafe { wide(location) };
        // SAFETY: `this` is the friends interface's pointer.
        unsafe {
            ask(
                this,
                Face::Friends,
                ctx,
                Request::SetLoc { flags, location },
            )
        }
    })
}

unsafe extern "system" fn by_byte(
    _this: *mut c_void,
    _byte: u32,
    _user: *const u16,
    _ctx: *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

// The login ticket.

#[repr(C)]
struct TicketTable {
    unknown: Unknown,
    make: unsafe extern "system" fn(*mut c_void, *const u16, *const u16) -> Hresult,
    ticket: unsafe extern "system" fn(*mut c_void, *mut Blob) -> Hresult,
    unused_a: Bare,
    set_ticket: unsafe extern "system" fn(*mut c_void, *const Blob) -> Hresult,
    present_ticket: unsafe extern "system" fn(*mut c_void, *const Blob, *mut c_void) -> Hresult,
    done_with_ticket: WithContext,
    unused_b: [Bare; 2],
}

static TICKET: TicketTable = TicketTable {
    unknown: unknown::<TICKET_FACE>(),
    make,
    ticket,
    unused_a: not_implemented,
    set_ticket,
    present_ticket,
    done_with_ticket,
    unused_b: [not_implemented as Bare; 2],
};

unsafe extern "system" fn make(_this: *mut c_void, _a: *const u16, _b: *const u16) -> Hresult {
    E_NOTIMPL
}

/// The ticket the client was started with, in two calls: the first, with no buffer, answers the
/// buffer-too-small code and the size; the second fills the buffer.
unsafe extern "system" fn ticket(this: *mut c_void, out: *mut Blob) -> Hresult {
    guard(|| {
        // SAFETY: `out` is NULL or the client's blob, alive for the call.
        let Some(out) = (unsafe { out.as_mut() }) else {
            return E_POINTER;
        };
        // SAFETY: `this` is the ticket interface's pointer.
        let library = unsafe { library(this, TICKET_FACE) };
        let host = library.host();
        let Some(ticket) = host.ticket() else {
            return NO_TICKET;
        };
        let need = u32::try_from(ticket.len()).unwrap_or(u32::MAX);
        if out.data.is_null() || out.size < need {
            out.size = need;
            return BUFFER_TOO_SMALL;
        }
        // SAFETY: the client's buffer holds `out.size` bytes, at least the ticket's.
        unsafe { core::ptr::copy_nonoverlapping(ticket.as_ptr(), out.data, ticket.len()) };
        out.size = need;
        S_OK
    })
}

/// The ticket the client was started with: its text and a NUL, with the client's guard bytes after
/// them, which the library never touches.
unsafe extern "system" fn set_ticket(this: *mut c_void, ticket: *const Blob) -> Hresult {
    guard(|| {
        if ticket.is_null() {
            return E_POINTER;
        }
        // SAFETY: the blob is the client's, alive for the call.
        let bytes = unsafe { blob(ticket) };
        // SAFETY: `this` is the ticket interface's pointer.
        unsafe { library(this, TICKET_FACE) }
            .host()
            .set_ticket(&bytes);
        S_OK
    })
}

/// Present the ticket to the world: its answer decides the login.
unsafe extern "system" fn present_ticket(
    this: *mut c_void,
    ticket: *const Blob,
    ctx: *mut c_void,
) -> Hresult {
    guard(|| {
        // SAFETY: the blob is the client's, alive for the call.
        let bytes = unsafe { blob(ticket) };
        // SAFETY: `this` is the ticket interface's pointer.
        unsafe { library(this, TICKET_FACE) }
            .host()
            .present_ticket(context(ctx), &bytes);
        S_OK
    })
}

/// The client is done with its ticket, at exit.
unsafe extern "system" fn done_with_ticket(this: *mut c_void, ctx: *mut c_void) -> Hresult {
    guard(|| {
        // SAFETY: `this` is the ticket interface's pointer.
        unsafe { library(this, TICKET_FACE) }
            .host()
            .release_ticket(context(ctx));
        S_OK
    })
}

// Help calls.

#[repr(C)]
struct SupportTable {
    unknown: Unknown,
    call: unsafe extern "system" fn(
        *mut c_void,
        u32,
        *const u16,
        *const u16,
        *const Blob,
        u32,
        *mut c_void,
    ) -> Hresult,
    status: Bare,
}

static SUPPORT: SupportTable = SupportTable {
    unknown: unknown::<SUPPORT_FACE>(),
    call,
    status: not_implemented,
};

/// A help call: not served in this version.
unsafe extern "system" fn call(
    _this: *mut c_void,
    _kind: u32,
    _target: *const u16,
    _description: *const u16,
    _data: *const Blob,
    _locale: u32,
    _ctx: *mut c_void,
) -> Hresult {
    E_NOTIMPL
}

// The class factory: one, for the whole library.

#[repr(C)]
struct FactoryTable {
    query: Query,
    add_ref: Count,
    release: Count,
    create_instance: unsafe extern "system" fn(
        *mut c_void,
        *mut c_void,
        *const Guid,
        *mut *mut c_void,
    ) -> Hresult,
    lock_server: unsafe extern "system" fn(*mut c_void, i32) -> Hresult,
}

#[repr(C)]
struct Factory {
    table: &'static FactoryTable,
}

static FACTORY_TABLE: FactoryTable = FactoryTable {
    query: factory_query,
    add_ref: factory_count,
    release: factory_count,
    create_instance,
    lock_server,
};

static FACTORY: Factory = Factory {
    table: &FACTORY_TABLE,
};

fn factory() -> *mut c_void {
    (&raw const FACTORY).cast_mut().cast()
}

unsafe extern "system" fn factory_query(
    _this: *mut c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    guard(|| {
        if out.is_null() {
            return E_POINTER;
        }
        // SAFETY: `iid` is NULL or the caller's identifier.
        let id = unsafe { iid.as_ref() };
        let found = id.is_some_and(|id| *id == ids::UNKNOWN || *id == ids::CLASS_FACTORY);
        // SAFETY: `out` is the caller's place for the answer.
        unsafe { *out = if found { factory() } else { null_mut() } };
        if found {
            S_OK
        } else {
            E_NOINTERFACE
        }
    })
}

/// The factory is the library's own, for its whole life: it counts nothing.
unsafe extern "system" fn factory_count(_this: *mut c_void) -> u32 {
    1
}

unsafe extern "system" fn create_instance(
    _this: *mut c_void,
    outer: *mut c_void,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    guard(|| {
        if out.is_null() {
            return E_POINTER;
        }
        // SAFETY: `out` is the caller's place for the answer.
        unsafe { *out = null_mut() };
        if !outer.is_null() {
            return CLASS_E_NOAGGREGATION;
        }
        crate::log::start();
        let object = Box::into_raw(Box::new(Library::new()));
        let first = face(object, Face::Connection);
        // SAFETY: `first` is the new object's connection interface, holding its one reference,
        // which the release gives back: the object lives on only if the query took one.
        unsafe {
            let hr = query::<CONNECTION_FACE>(first, iid, out);
            release::<CONNECTION_FACE>(first);
            hr
        }
    })
}

unsafe extern "system" fn lock_server(_this: *mut c_void, _lock: i32) -> Hresult {
    S_OK
}

/// The class factory for the friends library's class.
///
/// # Safety
/// The pointers are NULL or the caller's: two identifiers and a place for the answer.
#[no_mangle]
pub unsafe extern "system" fn DllGetClassObject(
    clsid: *const Guid,
    iid: *const Guid,
    out: *mut *mut c_void,
) -> Hresult {
    guard(|| {
        if out.is_null() {
            return E_POINTER;
        }
        // SAFETY: `out` is the caller's place for the answer.
        unsafe { *out = null_mut() };
        // SAFETY: the identifiers are NULL or the caller's.
        let (Some(clsid), Some(iid)) = (unsafe { clsid.as_ref() }, unsafe { iid.as_ref() }) else {
            return E_POINTER;
        };
        if *clsid != ids::CLASS {
            return CLASS_E_CLASSNOTAVAILABLE;
        }
        if *iid != ids::UNKNOWN && *iid != ids::CLASS_FACTORY {
            return E_NOINTERFACE;
        }
        // SAFETY: as above.
        unsafe { *out = factory() };
        S_OK
    })
}

/// Never: the library's connection threads may be running.
#[no_mangle]
pub extern "system" fn DllCanUnloadNow() -> Hresult {
    S_FALSE
}

/// Nothing to register: the client loads the library by path.
#[no_mangle]
pub extern "system" fn DllRegisterServer() -> Hresult {
    S_OK
}

/// Nothing was registered.
#[no_mangle]
pub extern "system" fn DllUnregisterServer() -> Hresult {
    S_OK
}
