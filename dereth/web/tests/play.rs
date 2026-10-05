//! Native device and live-session checks for the browser application.
//!
//! Behaviour: none (external device and optional live-server integration checks).
#![cfg(not(target_arch = "wasm32"))]

use dereth_web::{frame, play::Play};
use std::net::SocketAddr;
use std::sync::Arc;

/// How long the live tests let the character screen open before they click on it. Its opening
/// runs on the clock, not on frames, so a count of frames is too short on a fast machine.
const SPLASH: std::time::Duration = std::time::Duration::from_secs(15);

/// Frames, `step` each, for [`SPLASH`] of wall-clock time.
fn splash(step: &mut dyn FnMut()) {
    let until = std::time::Instant::now() + SPLASH;
    while std::time::Instant::now() < until {
        step();
    }
}

/// The client, brought up over the retail data on an off-screen device with no server to
/// answer, draws the modern interface's first screen, the connecting screen: its logo and
/// progress bars cover a tenth of the frame or more, the rest being the black it is cleared
/// to.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads the retail dats and needs a GPU: --features retail-dats"
)]
fn the_retail_interface_draws_on_the_web_device() {
    let store = Arc::new(dereth_dat::testing::open_store().unwrap_or_else(|| {
        panic!(
            "the retail dats are this test's oracle and they are not under {}",
            dereth_dat::testing::dat_dir().display()
        )
    }));
    let (width, height) = (800, 600);
    match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
        Ok(p) => dereth_render::wgpu::install(p),
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    }
    let mut play = Play::new(
        store,
        "127.0.0.1",
        "account",
        "password",
        1,
        width,
        height,
        None,
        Default::default(),
    )
    .expect("bring-up");
    for _ in 0..10 {
        assert!(play.frame(), "the client stopped");
    }
    let renderer = play.app_mut().renderer_mut();
    if let Ok(path) = std::env::var("DERETH_WEB_DUMP") {
        renderer
            .capture_png(std::path::Path::new(&path))
            .expect("a capture written");
    }
    let (w, h, bgra) = renderer.capture_bgra().expect("a capture");
    let drawn = bgra
        .as_chunks::<4>()
        .0
        .iter()
        .filter(|p| p[..3] != [0, 0, 0])
        .count();
    assert!(
        drawn > (w * h / 10) as usize,
        "{drawn} of {} pixels drawn",
        w * h
    );
}

/// Against a live server on this machine (`DERETH_TEST_LIVE_SERVER=127.0.0.1:9000`, account
/// `DERETH_TEST_LIVE_ACCOUNT`, its password `DERETH_TEST_LIVE_PASSWORD` or else the account
/// name): the client logs in through the WebSocket's framing and its interface reaches the
/// character screen, whose cursor comes out as an image once. With `DERETH_WEB_DUMP_DIR` set,
/// each screen is written there.
#[test]
#[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
fn a_live_login_reaches_the_character_screen() {
    use dereth_client_net::client_session::SessionState;

    let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
    let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
    let password = std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
    let dump = std::env::var("DERETH_WEB_DUMP_DIR").ok();
    let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
    let (width, height) = (800, 600);
    match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
        Ok(p) => dereth_render::wgpu::install(p),
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    }
    let server: SocketAddr = host.parse().expect("an address");
    let mut play = Play::new(
        store,
        &host,
        &account,
        &password,
        1,
        width,
        height,
        None,
        Default::default(),
    )
    .expect("bring-up");
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
    udp.set_nonblocking(true).expect("non-blocking");
    let mut buf = vec![0u8; 65_536];
    let mut step = |play: &mut Play| {
        while let Ok((n, from)) = udp.recv_from(&mut buf) {
            play.receive(&frame::encode(from.port(), &buf[..n]));
        }
        assert!(play.frame(), "the client stopped");
        for m in play.take_messages() {
            let (port, d) = frame::decode(&m).expect("framed");
            udp.send_to(d, SocketAddr::new(server.ip(), port))
                .expect("send");
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
    };
    let capture = |play: &mut Play, name: &str| {
        if let Some(dir) = &dump {
            let path = std::path::Path::new(dir).join(format!("{name}.png"));
            play.app_mut()
                .renderer_mut()
                .capture_png(&path)
                .expect("a capture");
        }
    };
    step(&mut play);
    capture(&mut play, "connecting");
    let state = |play: &mut Play| {
        play.app_mut()
            .replay_network_mut()
            .map(|n| n.session_state())
    };
    let mut frames = 0;
    while state(&mut play) != Some(SessionState::CharacterSelect) && frames < 600 {
        step(&mut play);
        frames += 1;
    }
    assert_eq!(state(&mut play), Some(SessionState::CharacterSelect));
    for i in 0..8 {
        for _ in 0..60 {
            step(&mut play);
        }
        capture(&mut play, &format!("characters-{i}"));
    }
    let cursor = play.cursor_change().expect("the interface set a cursor");
    assert!(cursor.width > 0 && cursor.height > 0);
    assert_eq!(
        cursor.rgba.len(),
        (cursor.width * cursor.height * 4) as usize
    );
    assert_eq!(play.cursor_change(), None, "reported once until it changes");
    let modes = play.app().ui().map(|u| u.transitions().to_vec());
    play.log_off();
    step(&mut play);
    eprintln!("character screen after {frames} frames; interface modes {modes:?}");
}

/// Against a live server on this machine, with an account that has a character (the
/// variables of [`a_live_login_reaches_the_character_screen`]): the character is picked and
/// entered with the pointer, the body walks with the keymap's forward key, and a line typed
/// into chat is sent with Enter. With `DERETH_WEB_DUMP_DIR` set, each step's frame is
/// written there.
#[test]
#[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
fn a_live_session_enters_walks_and_talks_through_the_interface() {
    use dereth_client_net::client_session::SessionState;

    let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
    let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
    let password = std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
    let dump = std::env::var("DERETH_WEB_DUMP_DIR").ok();
    let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
    let (width, height) = (800, 600);
    match pollster::block_on(dereth_render::wgpu::prepare_offscreen(width, height, false)) {
        Ok(p) => dereth_render::wgpu::install(p),
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    }
    let server: SocketAddr = host.parse().expect("an address");
    let mut play = Play::new(
        store,
        &host,
        &account,
        &password,
        1,
        width,
        height,
        None,
        Default::default(),
    )
    .expect("bring-up");
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
    udp.set_nonblocking(true).expect("non-blocking");
    let mut buf = vec![0u8; 65_536];
    let mut run = |play: &mut Play, frames: u32| {
        for _ in 0..frames {
            while let Ok((n, from)) = udp.recv_from(&mut buf) {
                play.receive(&frame::encode(from.port(), &buf[..n]));
            }
            assert!(play.frame(), "the client stopped");
            for m in play.take_messages() {
                let (port, d) = frame::decode(&m).expect("framed");
                udp.send_to(d, SocketAddr::new(server.ip(), port))
                    .expect("send");
            }
            std::thread::sleep(std::time::Duration::from_millis(16));
        }
    };
    let capture = |play: &mut Play, name: &str| {
        if let Some(dir) = &dump {
            let path = std::path::Path::new(dir).join(format!("{name}.png"));
            play.app_mut()
                .renderer_mut()
                .capture_png(&path)
                .expect("a capture");
        }
    };
    let click = |play: &mut Play, x: f64, y: f64| {
        play.pointer_move(x, y);
        play.pointer_button(0, true);
        play.pointer_button(0, false);
    };
    let state = |play: &mut Play| {
        play.app_mut()
            .replay_network_mut()
            .map(|n| n.session_state())
    };
    let mut frames = 0;
    while state(&mut play) != Some(SessionState::CharacterSelect) && frames < 600 {
        run(&mut play, 1);
        frames += 1;
    }
    splash(&mut || run(&mut play, 1));
    capture(&mut play, "select");
    click(&mut play, 120.0, 222.0);
    run(&mut play, 10);
    click(&mut play, 345.0, 392.0);
    frames = 0;
    while state(&mut play) != Some(SessionState::Playable) && frames < 900 {
        run(&mut play, 1);
        frames += 1;
    }
    assert_eq!(state(&mut play), Some(SessionState::Playable));
    run(&mut play, 360);
    capture(&mut play, "arrived");
    let before = play
        .app()
        .world
        .as_ref()
        .and_then(|w| w.character.as_ref())
        .map(|c| c.position());
    assert!(play.key("KeyW", true, Some("w".into())));
    run(&mut play, 120);
    play.key("KeyW", false, None);
    run(&mut play, 30);
    capture(&mut play, "walked");
    let after = play
        .app()
        .world
        .as_ref()
        .and_then(|w| w.character.as_ref())
        .map(|c| c.position());
    // Each key's release comes a frame after its press, as a page's events can.
    let press = |play: &mut Play, run: &mut dyn FnMut(&mut Play, u32), code: &str, text: &str| {
        play.key(code, true, Some(text.to_string()));
        run(play, 1);
        play.key(code, false, None);
    };
    press(&mut play, &mut run, "Enter", "\r");
    run(&mut play, 10);
    for ch in "hello".chars() {
        let code = format!("Key{}", ch.to_ascii_uppercase());
        press(&mut play, &mut run, &code, &ch.to_string());
        run(&mut play, 2);
    }
    run(&mut play, 10);
    capture(&mut play, "typing");
    press(&mut play, &mut run, "Enter", "\r");
    run(&mut play, 60);
    capture(&mut play, "said");
    play.log_off();
    run(&mut play, 2);
    eprintln!("walked from {before:?} to {after:?}");
    assert_ne!(before, after, "the body walked");
}

/// Against a live server on this machine (the variables of
/// [`a_live_login_reaches_the_character_screen`]): the character screen's Exit, confirmed,
/// ends the client -- the epilogue screen logs off and a frame reports the end -- so a page
/// can offer to play again.
#[test]
#[ignore = "needs a live server: DERETH_TEST_LIVE_SERVER and DERETH_TEST_LIVE_ACCOUNT"]
fn a_live_exit_from_the_character_screen_ends_the_client() {
    use dereth_client_net::client_session::SessionState;

    let host = std::env::var("DERETH_TEST_LIVE_SERVER").expect("DERETH_TEST_LIVE_SERVER");
    let account = std::env::var("DERETH_TEST_LIVE_ACCOUNT").expect("DERETH_TEST_LIVE_ACCOUNT");
    let password = std::env::var("DERETH_TEST_LIVE_PASSWORD").unwrap_or_else(|_| account.clone());
    let store = Arc::new(dereth_dat::testing::open_store().expect("the dats"));
    match pollster::block_on(dereth_render::wgpu::prepare_offscreen(800, 600, false)) {
        Ok(p) => dereth_render::wgpu::install(p),
        Err(e) => {
            eprintln!("skipped: {e}");
            return;
        }
    }
    let server: SocketAddr = host.parse().expect("an address");
    let mut play = Play::new(
        store,
        &host,
        &account,
        &password,
        1,
        800,
        600,
        None,
        Default::default(),
    )
    .expect("bring-up");
    let udp = std::net::UdpSocket::bind("127.0.0.1:0").expect("bind");
    udp.set_nonblocking(true).expect("non-blocking");
    let mut buf = vec![0u8; 65_536];
    let mut step = |play: &mut Play| -> bool {
        while let Ok((n, from)) = udp.recv_from(&mut buf) {
            play.receive(&frame::encode(from.port(), &buf[..n]));
        }
        let alive = play.frame();
        for m in play.take_messages() {
            let (port, d) = frame::decode(&m).expect("framed");
            udp.send_to(d, SocketAddr::new(server.ip(), port))
                .expect("send");
        }
        std::thread::sleep(std::time::Duration::from_millis(16));
        alive
    };
    let mut frames = 0;
    while play
        .app_mut()
        .replay_network_mut()
        .map(|n| n.session_state())
        != Some(SessionState::CharacterSelect)
        && frames < 600
    {
        step(&mut play);
        frames += 1;
    }
    splash(&mut || {
        step(&mut play);
    });
    let mut click = |play: &mut Play, x: f64, y: f64| {
        play.pointer_move(x, y);
        play.pointer_button(0, true);
        play.pointer_button(0, false);
        for _ in 0..30 {
            step(play);
        }
    };
    // Exit, then Yes to "Are you sure you want to leave?".
    click(&mut play, 700.0, 572.0);
    click(&mut play, 320.0, 326.0);
    let mut ended_after = None;
    for n in 0..1200 {
        if !step(&mut play) {
            ended_after = Some(n);
            break;
        }
    }
    if let Ok(dir) = std::env::var("DERETH_WEB_DUMP_DIR") {
        let path = std::path::Path::new(&dir).join("after-exit.png");
        let _ = play.app_mut().renderer_mut().capture_png(&path);
    }
    // The log-off a page sends before its cleanup, so the server lets the account go.
    play.log_off();
    for m in play.take_messages() {
        let (port, d) = frame::decode(&m).expect("framed");
        udp.send_to(d, SocketAddr::new(server.ip(), port))
            .expect("send");
    }
    eprintln!("ended after {ended_after:?} frames");
    assert!(ended_after.is_some(), "the client ended");
}
