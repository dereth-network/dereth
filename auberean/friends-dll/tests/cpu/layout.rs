//! Each result and event as the client reads it: the bytes of a block placed at an address, read at
//! the client's offsets, with every pointer followed inside the block.

use auberean_friends::engine::{Message, Reply};
use auberean_friends::layout::{self, Block};
use stream::{Answer, Event, Extra, FriendStatus, Hr, Kind, Mode};

use crate::common::friends;

/// Where the tests place a block.
const BASE: u32 = 0x0A00_0000;

/// A block placed at [`BASE`], read as the client reads it.
struct Placed {
    bytes: Vec<u8>,
}

impl Placed {
    fn of(message: &Message) -> Self {
        let block: Block = layout::block(message);
        let bytes = block.at(BASE);
        assert_eq!(bytes.len(), block.len());
        let mut words = vec![0u32; block.words()];
        block.fill(BASE, &mut words);
        let from_words: Vec<u8> = words.iter().flat_map(|w| w.to_le_bytes()).collect();
        assert_eq!(
            &from_words[..bytes.len()],
            &bytes[..],
            "the words hold the same block"
        );
        Self { bytes }
    }

    fn u8_at(&self, at: usize) -> u8 {
        self.bytes[at]
    }

    fn u32_at(&self, at: usize) -> u32 {
        u32::from_le_bytes(self.bytes[at..at + 4].try_into().expect("four bytes"))
    }

    /// Where a pointer at `at` points, inside the block; `None` for NULL.
    fn follow(&self, at: usize) -> Option<usize> {
        let p = self.u32_at(at);
        if p == 0 {
            return None;
        }
        assert!(p >= BASE, "a pointer outside the block: {p:#x}");
        let offset = usize::try_from(p - BASE).expect("an offset");
        assert!(
            offset < self.bytes.len(),
            "a pointer past the block: {p:#x}"
        );
        Some(offset)
    }

    /// The text a pointer at `at` points to: UTF-16 up to its NUL, as the client measures it.
    fn text_at(&self, at: usize) -> String {
        let start = self.follow(at).expect("a text, never NULL");
        assert_eq!(start % 2, 0, "a text on a two-byte boundary");
        let mut units = Vec::new();
        let mut i = start;
        loop {
            let u = u16::from_le_bytes([self.bytes[i], self.bytes[i + 1]]);
            if u == 0 {
                break;
            }
            units.push(u);
            i += 2;
        }
        String::from_utf16(&units).expect("UTF-16")
    }

    /// A friends list: the count at `at`, the entries pointed to from `at + 4`.
    fn friends_at(&self, at: usize) -> Vec<FriendStatus> {
        let n = usize::try_from(self.u32_at(at)).expect("a count");
        let Some(list) = self.follow(at + 4) else {
            assert_eq!(n, 0, "a list with no entries is empty");
            return Vec::new();
        };
        (0..n)
            .map(|i| {
                let entry = list + 12 * i;
                FriendStatus {
                    user: self.text_at(entry),
                    mode: Mode(self.u32_at(entry + 4)),
                    location: self.text_at(entry + 8),
                }
            })
            .collect()
    }

    /// The header every block opens with: valid, and its type.
    fn header(&self) -> (u8, u32) {
        (self.u8_at(4), self.u32_at(8))
    }
}

fn result(context: u32, hr: Hr, answer: Answer) -> Message {
    Message::Result {
        context,
        hr,
        reply: Reply::Answer(answer),
    }
}

/// Behaviour: friends.library.block-header
#[test]
fn every_block_is_marked_valid_and_carries_its_type_and_a_result_its_context_and_code() {
    let types = [
        (Kind::GetFriendsList, 0x4200),
        (Kind::FriendAdd, 0x4201),
        (Kind::FriendDelete, 0x4202),
        (Kind::GetIgnoredList, 0x4203),
        (Kind::IgnoredAdd, 0x4204),
        (Kind::IgnoredDelete, 0x4205),
        (Kind::Flags, 0x4208),
        (Kind::SetLoc, 0x420A),
        (Kind::SendToRoomEx, 0x521C),
        (Kind::SendToUserEx, 0x521D),
        (Kind::PresentTicket, 0xF201),
        (Kind::CallCreate, 0x1_0200),
    ];
    for (kind, code) in types {
        for hr in [Hr::OK, Hr::NOT_CONNECTED] {
            let p = Placed::of(&result(0x0012_3450, hr, Answer::empty(kind)));
            assert_eq!(p.header(), (1, code), "{kind:?}");
            assert_eq!(p.u32_at(0), 0);
            assert_eq!(
                p.u32_at(0x1C),
                0x0012_3450,
                "{kind:?}: the client's context"
            );
            assert_eq!(p.u32_at(0x20), hr.0, "{kind:?}: the code");
            assert!(p.bytes.len() >= 0x24);
        }
    }
    let released = Placed::of(&Message::Result {
        context: 9,
        hr: Hr::OK,
        reply: Reply::TicketReleased,
    });
    assert_eq!(released.header(), (1, 0xF202));
    assert_eq!(released.u32_at(0x1C), 9);

    for (event, code) in [
        (Message::Connected, 0x1300),
        (Message::Disconnected, 0x1301),
        (Message::Lost, 0x1302),
    ] {
        let p = Placed::of(&event);
        assert_eq!(p.header(), (1, code));
        assert_eq!(p.bytes.len(), 0x1C);
    }
}

/// Behaviour: friends.library.friends-entries
#[test]
fn a_friends_list_is_a_count_and_twelve_byte_entries_of_name_state_and_location() {
    let list = friends();
    let answered = Placed::of(&result(
        1,
        Hr::OK,
        Answer::GetFriendsList {
            friends: list.clone(),
        },
    ));
    assert_eq!(answered.header(), (1, 0x4200));
    assert_eq!(answered.friends_at(0x24), list);

    let started = Placed::of(&Message::Event(Event::FStart {
        friends: list.clone(),
    }));
    assert_eq!(started.header(), (1, 0x430C));
    assert_eq!(started.friends_at(0x1C), list);

    // Twelve bytes an entry, one after another.
    let first = started.follow(0x20).expect("the entries");
    assert_eq!(started.text_at(first + 12), "Cara@Dawnsong");
    assert_eq!(started.u32_at(first + 12 + 4), 3, "away is online and away");

    // A list that failed, or is empty, has no entries to point to.
    let failed = Placed::of(&result(
        1,
        Hr::NOT_CONNECTED,
        Answer::GetFriendsList {
            friends: Vec::new(),
        },
    ));
    assert_eq!(failed.u32_at(0x24), 0);
    assert_eq!(failed.u32_at(0x28), 0);

    // Letters past ASCII come through as UTF-16.
    let accented = Placed::of(&Message::Event(Event::FStart {
        friends: vec![FriendStatus::new("Zoë@cœur", Mode::ONLINE)],
    }));
    assert_eq!(accented.friends_at(0x1C)[0].user, "Zoë@cœur");
}

/// Behaviour: friends.library.ignore-names
#[test]
fn the_ignore_list_is_a_count_and_an_array_of_names() {
    let names = vec!["Mean@Dawnsong".to_owned(), "Rude@coldeve".to_owned()];
    let p = Placed::of(&result(
        2,
        Hr::OK,
        Answer::GetIgnoredList {
            ignored: names.clone(),
        },
    ));
    assert_eq!(p.header(), (1, 0x4203));
    assert_eq!(p.u32_at(0x24), 2);
    let list = p.follow(0x28).expect("the names");
    let read: Vec<String> = (0..2).map(|i| p.text_at(list + 4 * i)).collect();
    assert_eq!(read, names);
}

/// Behaviour: friends.library.tell-extra-never-null
#[test]
fn a_tell_carries_its_teller_its_text_and_extra_data_that_is_never_null() {
    let bare = Placed::of(&Message::Event(Event::UTellEv {
        user: "Echo@Dawnsong".into(),
        text: "hello there".into(),
        extra: None,
    }));
    assert_eq!(bare.header(), (1, 0x5317));
    assert_eq!(bare.text_at(0x1C), "Echo@Dawnsong");
    assert_eq!(bare.u32_at(0x20), 0, "the type");
    assert_eq!(bare.text_at(0x24), "hello there");
    let blob = bare.follow(0x28).expect("extra data is never NULL");
    assert_eq!((bare.u32_at(blob), bare.u32_at(blob + 4)), (0, 0));

    let data: [u8; 16] = core::array::from_fn(|i| u8::try_from(i * 3).unwrap_or(0));
    let with = Placed::of(&Message::Event(Event::UTellEv {
        user: "Echo@Dawnsong".into(),
        text: "hi".into(),
        extra: Some(Extra(data)),
    }));
    let blob = with.follow(0x28).expect("extra data");
    assert_eq!(with.u32_at(blob), 16);
    let at = with.follow(blob + 4).expect("its bytes");
    assert_eq!(&with.bytes[at..at + 16], &data);
}

/// Behaviour: friends.library.friend-state-and-words
#[test]
fn a_friend_s_state_the_flags_and_the_ticket_s_status_sit_where_the_client_reads_them() {
    let state = Placed::of(&Message::Event(Event::FriendStat {
        user: "Cara@Dawnsong".into(),
        mode: Mode::ONLINE.with(Mode::NOT_DISTURBED),
    }));
    assert_eq!(state.header(), (1, 0x430D));
    assert_eq!(state.text_at(0x1C), "Cara@Dawnsong");
    assert_eq!(state.u32_at(0x20), 5);

    let flags = Placed::of(&result(
        3,
        Hr::OK,
        Answer::Flags {
            which_flag: 1,
            flags: 1,
        },
    ));
    assert_eq!(flags.header(), (1, 0x4208));
    assert_eq!((flags.u32_at(0x24), flags.u32_at(0x28)), (1, 1));

    let ticket = Placed::of(&result(4, Hr::OK, Answer::PresentTicket { status: 7 }));
    assert_eq!(ticket.header(), (1, 0xF201));
    assert_eq!(ticket.u32_at(0x24), 7);
}
