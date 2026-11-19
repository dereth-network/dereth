//! The player's pets and its mount, kept for every front end: what the notices say of them, and
//! the pets and the mount as the view carries them.

use auberean_client_contract::notice::{Notice, PetNotice, PlayerNotice, Said};
use auberean_client_contract::text::TextRef;
use auberean_client_contract::view::Pet;
use auberean_client_contract::Action;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::pets::Pets;
use auberean_client_runtime::said::native;
use auberean_client_runtime::{Client, ClientData, ClientSettings};
use auberean_protocol::physics::CreateObject;
use auberean_protocol::ServerMessage;
use auberean_script::native_object::NativeObject;

use super::download::file;

const PLAYER: u64 = 0x5000_0001;
const WOLF: u64 = 0x2140_0000_0000_0011;
const BEAR: u64 = 0x2140_0000_0000_0012;

fn said(said: Said) -> Notice {
    Notice { about: None, said }
}

fn added(pet: u64, name: &str, mode: u32) -> Notice {
    said(Said::Pet(PetNotice::Added {
        pet,
        name: Some(TextRef::literal(name)),
        icon: 0x0600_0101,
        mode,
    }))
}

fn names(p: &Pets) -> Vec<(u64, Option<TextRef>, u32)> {
    p.pets()
        .iter()
        .map(|p| (p.id, p.name.clone(), p.mode))
        .collect()
}

/// Pets come in order and keep their place when they come again; a pet is named, takes a mode
/// and goes; the player mounts and gets off; leaving the world takes every pet and the mount
/// away.
///
/// Behaviour: ui.pet.the-pets-and-the-mount-follow-the-notices
#[test]
fn the_pets_and_the_mount_follow_what_the_notices_say() {
    let mut p = Pets::default();
    p.hear(&added(WOLF, "Wolf (Cara)", 0x20));
    p.hear(&added(BEAR, "Bear", 0x1000));
    p.hear(&added(WOLF, "Wolf", 0x2000));
    let lit = |s: &str| Some(TextRef::literal(s));
    assert_eq!(
        names(&p),
        vec![(WOLF, lit("Wolf"), 0x2000), (BEAR, lit("Bear"), 0x1000)],
        "a pet that comes again keeps its place"
    );
    p.hear(&said(Said::Pet(PetNotice::Named {
        pet: BEAR,
        name: lit("Bruin"),
    })));
    p.hear(&said(Said::Pet(PetNotice::Mode {
        pet: BEAR,
        mode: 0x4000_0005,
    })));
    assert_eq!(names(&p)[1], (BEAR, lit("Bruin"), 0x4000_0005));
    p.hear(&said(Said::Pet(PetNotice::Removed { pet: WOLF })));
    assert_eq!(names(&p), vec![(BEAR, lit("Bruin"), 0x4000_0005)]);
    p.hear(&said(Said::Pet(PetNotice::Attacked { pet: BEAR })));
    assert_eq!(p.pets().len(), 1, "an attack changes nothing kept");

    assert!(!p.mounted());
    p.hear(&said(Said::Player(PlayerNotice::Mounted { mounted: true })));
    assert!(p.mounted());
    p.hear(&said(Said::Player(PlayerNotice::Mounted {
        mounted: false,
    })));
    assert!(!p.mounted());
    p.hear(&said(Said::Player(PlayerNotice::Mounted { mounted: true })));
    p.clear();
    assert!(p.pets().is_empty() && !p.mounted(), "left the world");
}

fn client() -> Client {
    Client::new(
        ClientSettings {
            login_server: "127.0.0.1:9000".parse().expect("an address"),
            account: b"a".to_vec(),
            ticket: Vec::new(),
            timestamp: 0x5000_0000,
            session_key: [0; 64],
            script_sends: false,
        },
        ClientData {
            files: DataFiles {
                files: vec![file(0x1_0000_0000, &[]), file(0x2_0000_0001, &[])],
                language: ENGLISH,
            },
            scripts: None,
        },
    )
}

/// A notice as the scripts send it to every panel: its words, and the text at a word's place.
fn sent(native: u32, words: Vec<u32>, texts: &[(usize, &str)]) -> PanelNotice {
    let n = words.len();
    let mut objects = vec![None; n];
    for (at, text) in texts {
        objects[*at] = Some(NativeObject::Text(text.encode_utf16().collect()));
    }
    PanelNotice {
        native,
        about: None,
        words,
        objects,
        requests: vec![None; n],
        effect_lines: vec![Vec::new(); n],
        listed: vec![Vec::new(); n],
        selections: vec![None; n],
    }
}

fn refresh(c: &mut Client) {
    c.refresh_view(&|_| Vec::new());
}

/// The view carries the player's pets as the scripts' notices told of them, each heard once
/// however often the view is refreshed before the notices are taken, and the player's mount on
/// its player; logging off takes the pets away.
///
/// Behaviour: ui.pet.the-view-carries-the-pets-and-the-mount
#[test]
fn the_view_carries_the_pets_and_the_mount_the_notices_told_of() {
    let mut c = client();
    c.model.player = Some(PLAYER);
    c.take_message(
        0x0300_0000_0000_0000,
        ServerMessage::CreateObject(CreateObject {
            id: PLAYER,
            ..CreateObject::default()
        }),
    );
    let high = |id: u64| u32::try_from(id >> 32).expect("high");
    let low = |id: u64| u32::try_from(id & 0xFFFF_FFFF).expect("low");
    c.notices.push(sent(
        native::PET_ADDED,
        vec![high(WOLF), low(WOLF), 0, 0x0600_0101, 0x20],
        &[(2, "Wolf (Cara)")],
    ));
    c.notices.push(sent(native::MOUNTED, vec![1], &[]));
    refresh(&mut c);
    refresh(&mut c);
    assert_eq!(
        c.view().pets,
        vec![Pet {
            id: WOLF,
            name: Some(TextRef::literal("Wolf (Cara)")),
            icon: 0x0600_0101,
            mode: 0x20,
        }],
        "heard once"
    );
    assert!(
        c.view().player.as_ref().is_some_and(|p| p.mounted),
        "the player mounted"
    );
    let _ = c.take_notices();
    c.notices.push(sent(
        native::PET_MODE,
        vec![high(WOLF), low(WOLF), 0x4000_0005],
        &[],
    ));
    c.notices.push(sent(native::MOUNTED, vec![0], &[]));
    refresh(&mut c);
    assert_eq!(c.view().pets[0].mode, 0x4000_0005);
    assert!(c.view().player.as_ref().is_some_and(|p| !p.mounted));
    let _ = c.take_notices();
    c.act(Action::LogOff);
    refresh(&mut c);
    assert!(c.view().pets.is_empty(), "logged off");
}
