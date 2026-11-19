//! The frame's hook and the notices: what the player's scripts tell the panels reaches the hook,
//! read, once and in order, whenever the host takes them.

use std::any::Any;

use auberean_client_contract::notice::{Notice, PlayerNotice, Said};
use auberean_client_net::LocalTime;
use auberean_client_runtime::data::{DataFiles, ENGLISH};
use auberean_client_runtime::frame_hook::{FrameHook, HookFrame};
use auberean_client_runtime::host::PanelNotice;
use auberean_client_runtime::said::native;
use auberean_client_runtime::{Client, ClientData, ClientSettings};

use super::download::file;

/// A hook that keeps every notice it is handed, a frame's at a time.
#[derive(Debug, Default)]
struct Heard(Vec<Vec<Notice>>);

impl FrameHook for Heard {
    fn step(&mut self, frame: &mut HookFrame<'_>) {
        self.0.push(frame.notices.to_vec());
    }

    fn as_any(&self) -> &dyn Any {
        self
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
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

/// The scripts telling the panels the player mounted (`true`) or got off.
fn mounted(on: bool) -> PanelNotice {
    PanelNotice {
        native: native::MOUNTED,
        about: None,
        words: vec![u32::from(on)],
        objects: vec![None],
        requests: vec![None],
        effect_lines: vec![Vec::new()],
        listed: vec![Vec::new()],
        selections: vec![None],
    }
}

fn read(on: bool) -> Notice {
    Notice {
        about: None,
        said: Said::Player(PlayerNotice::Mounted { mounted: on }),
    }
}

fn heard(c: &Client) -> &[Vec<Notice>] {
    &c.frame_hook()
        .and_then(|h| h.as_any().downcast_ref::<Heard>())
        .expect("the hook")
        .0
}

/// A hook is handed the notices told since it last ran, read, once and in the order told: those
/// the host takes after a frame reach it at the next, and the host still takes every one itself.
/// Notices from before it was installed are not handed to it.
///
/// Behaviour: session.driver.a-frame-hook-hears-each-notice-once
#[test]
fn a_frame_hook_hears_each_notice_once_in_order_whenever_the_host_takes_them() {
    let mut c = client();
    c.notices.push(mounted(true));
    assert!(c.install_frame_hook(Box::<Heard>::default()).is_none());
    c.notices.push(mounted(false));
    c.frame(LocalTime(1.0));
    assert_eq!(heard(&c), [vec![read(false)]], "only what came after it");
    // Taken by the host before the next frame, with one more told after the hook ran.
    c.notices.push(mounted(true));
    assert_eq!(
        c.take_notices(),
        [read(true), read(false), read(true)],
        "the host takes every one"
    );
    c.notices.push(mounted(false));
    c.frame(LocalTime(2.0));
    c.frame(LocalTime(3.0));
    assert_eq!(
        heard(&c),
        [vec![read(false)], vec![read(true), read(false)], Vec::new()],
        "each once, in order"
    );
}
