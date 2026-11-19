//! The panels' notices read as the contract carries them: each notice the client binds named, its
//! words read where the panels read them, its records as plain data.

use auberean_client_contract::notice::{
    ExaminationNotice, FellowshipNotice, FriendState, FriendsNotice, InventoryNotice, PlayerNotice,
    Said, SelectionNotice, SelectionVitals, StoreNotice, TradeNotice,
};
use auberean_client_contract::text::TextRef;
use auberean_client_runtime::examination::Profile;
use auberean_client_runtime::friends::UserStatus;
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::notices::{Hearers, NOTICES};
use auberean_client_runtime::said::{native, read};
use auberean_client_runtime::selection::Selection;
use auberean_script::native_object::NativeObject;

/// The notices the client binds that no panel reads: kept as the scripts sent them.
const UNREAD: [u32; 4] = [
    0x0084 << 16 | 0x0004,
    0x0087 << 16 | 0x0001,
    0x0088 << 16 | 0x0004,
    0x008C << 16 | 0x0001,
];

/// A notice of `native` as the scripts send it: about object 7 (unless every panel hears it),
/// its words `words`, each word naming the object in `objects` at its place (none past them).
fn sent(native: u32, words: Vec<u32>, objects: Vec<Option<NativeObject>>) -> PanelNotice {
    let n = words.len();
    let mut objects = objects;
    objects.resize(n, None);
    PanelNotice {
        native,
        about: Some(7),
        words,
        objects,
        requests: vec![None; n],
        effect_lines: vec![Vec::new(); n],
        listed: vec![Vec::new(); n],
        selections: vec![None; n],
    }
}

fn literal(s: &str) -> NativeObject {
    NativeObject::Text(s.encode_utf16().collect())
}

/// Every notice the client binds that a panel reads comes named, and so do the client's own (the
/// chat service's connection lost and its ticket taken again, the options window's change); the
/// others come as the scripts sent them, words and all.
///
/// Behaviour: script.notices.every-notice-a-panel-reads-comes-named
#[test]
fn every_notice_a_panel_reads_comes_named() {
    let mut named = 0;
    for n in NOTICES {
        let words = usize::try_from(n.slots).expect("a count")
            - usize::from(n.hearers == Hearers::TheObjects);
        let words: Vec<u32> = (1..=u32::try_from(words).expect("a count")).collect();
        let said = read(&sent(n.native, words.clone(), Vec::new())).said;
        if UNREAD.contains(&n.native) {
            assert_eq!(
                said,
                Said::Unread {
                    native: n.native,
                    words
                },
                "{:#010x} is kept as sent",
                n.native
            );
        } else {
            assert!(
                !matches!(said, Said::Unread { .. }),
                "{:#010x} comes named",
                n.native
            );
            named += 1;
        }
    }
    assert_eq!(named, NOTICES.len() - UNREAD.len());
    for own in [
        native::CHAT_SERVICE_LOST,
        native::CHAT_SERVICE_TICKET,
        native::OPTIONS_CHANGED,
    ] {
        let said = read(&sent(own, vec![1], Vec::new())).said;
        assert!(!matches!(said, Said::Unread { .. }), "{own:#x} comes named");
    }
}

/// A notice's words are read where the panels read them (an object's id as its high word and its
/// low), and the records its words name come as plain data: a name as its string reference, the
/// friends as their states, a profile with the tabs it asks for, a selection's kind and vitals.
///
/// Behaviour: script.notices.a-notice-names-its-words-and-records-as-the-panels-read-them
#[test]
fn a_notice_names_its_words_and_records_as_the_panels_read_them() {
    let vital = read(&sent(
        native::VITAL,
        vec![0x4000_0001, 0, 90, 0, 120],
        Vec::new(),
    ));
    assert_eq!(vital.about, Some(7));
    assert_eq!(
        vital.said,
        Said::Player(PlayerNotice::Vital {
            vital: 0x4000_0001,
            current: 90,
            max: 120
        })
    );
    assert_eq!(
        read(&sent(
            native::CONTENTS,
            vec![1, 2, 0x4000_0002, 3, 4, 9],
            Vec::new()
        ))
        .said,
        Said::Inventory(InventoryNotice::Contents {
            container: 1 << 32 | 2,
            change: 0x4000_0002,
            item: 3 << 32 | 4,
            slot: 9
        })
    );
    assert_eq!(
        read(&sent(
            native::TRADE_OFFERED,
            vec![0, 5, 0, 6, 12],
            Vec::new()
        ))
        .said,
        Said::Trade(TradeNotice::Offered {
            by: 5,
            item: 6,
            quantity: 12
        })
    );
    assert_eq!(
        read(&sent(native::FELLOW_HEALTH, vec![0, 8, 40, 3], Vec::new())).said,
        Said::Fellowship(FellowshipNotice::Health {
            fellow: 8,
            health: 40,
            taken_by_players: 3
        })
    );
    // The leader's and the fellowship's names, at their own words.
    let offered = read(&sent(
        native::FELLOWSHIP_OFFERED,
        vec![0, 9, 0x100, 0x101],
        vec![None, None, Some(literal("Cara")), Some(literal("The Hunt"))],
    ));
    assert_eq!(
        offered.said,
        Said::Fellowship(FellowshipNotice::Offered {
            leader: 9,
            leader_name: Some(TextRef::literal("Cara")),
            fellowship: Some(TextRef::literal("The Hunt")),
        })
    );
    // The friends list, an empty one too; a friend's state.
    let status = |name: &str, mode| {
        UserStatus {
            name: name.encode_utf16().collect(),
            location: Vec::new(),
            mode,
        }
        .native()
    };
    let state = |name: &str, mode| FriendState {
        name: name.into(),
        location: String::new(),
        mode,
    };
    let mut list = sent(native::FRIENDS, vec![0x100], Vec::new());
    list.listed = vec![vec![
        Some(status("Cara", 1)),
        Some(status("Bob@coldeve", 0)),
    ]];
    assert_eq!(
        read(&list).said,
        Said::Friends(FriendsNotice::List(vec![
            state("Cara", 1),
            state("Bob@coldeve", 0)
        ]))
    );
    assert_eq!(
        read(&sent(native::FRIENDS, vec![0x100], Vec::new())).said,
        Said::Friends(FriendsNotice::List(Vec::new()))
    );
    assert_eq!(
        read(&sent(
            native::FRIEND_STATE,
            vec![0x100],
            vec![Some(status("Cara", 3))]
        ))
        .said,
        Said::Friends(FriendsNotice::State(Some(state("Cara", 3))))
    );
    // The window shows the tabs a profile asks for, though its request's packed form says none.
    let profile = read(&sent(
        native::PROFILE,
        vec![0x100],
        vec![Some(Profile::asking(0x3FF).native())],
    ));
    let Said::Examination(ExaminationNotice::Profile(Some(p))) = profile.said else {
        panic!("a profile: {profile:?}");
    };
    assert_eq!(p.request.tabs, 0x3FF);
    // A selection's kind and its six numbers; none when the notice names no selection.
    let mut info = sent(native::SELECTION_VITALS, vec![0x100], Vec::new());
    info.selections = vec![Some(Selection {
        kind: 2,
        cur_health: 50,
        pk_damage: 4,
        max_health: 80,
        cur_vigor: 30,
        pk_vigor_loss: 1,
        max_vigor: 60,
    })];
    assert_eq!(
        read(&info).said,
        Said::Selection(SelectionNotice::Vitals(Some(SelectionVitals {
            kind: 2,
            health: 50,
            health_taken_by_players: 4,
            max_health: 80,
            vigor: 30,
            vigor_taken_by_players: 1,
            max_vigor: 60,
        })))
    );
    assert_eq!(
        read(&sent(native::SELECTION_VITALS, vec![0x100], Vec::new())).said,
        Said::Selection(SelectionNotice::Vitals(None))
    );
    // A shop's notice with no shop names none.
    assert_eq!(
        read(&sent(native::STORE, vec![0x100], Vec::new())).said,
        Said::Store(StoreNotice::Shown(None))
    );
}
