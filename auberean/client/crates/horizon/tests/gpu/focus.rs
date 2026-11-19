//! The focus bar on a graphics device: the game's own interface's status bar for a player who is
//! no hero and for a hero at half its focus, and Horizon's parameter bar for that hero, each
//! written to look at (`focus-retail-not-hero`, `focus-retail-hero`, `focus-horizon-hero`).
//!
//! Behaviour: none (reference pictures to look at, claimed by the status bar's own tests)

use auberean_client_sdk::protocol::login::PlayerQualities;
use auberean_client_sdk::protocol::table::{Table, SMALL};
use auberean_client_sdk::protocol::types::Qualities;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::runtime::host::PanelNotice;
use auberean_horizon::state::Vital;
use auberean_testkit::double::script::Event as Step;
use auberean_ui::Token;

use super::composed;
use super::session::{self, doing, guarded, Session};

/// The player's qualities, its health and vigor as the session's own, a hero at half its focus.
fn a_hero() -> Vec<Step> {
    vec![session::s2c(
        8.10,
        1,
        1,
        session::WORLD | 7,
        &ServerMessage::PlayerQualities(PlayerQualities(Qualities {
            header: Qualities::INTS | Qualities::BOOLS,
            ints: Table::from_inserted(
                SMALL,
                [(258, 80), (263, 100), (259, 55), (264, 120), (266, 50)],
            ),
            bools: Table::from_inserted(SMALL, [(4000, 1)]),
            ..Qualities::default()
        })),
    )]
}

/// Write a picture of `w` by `h` texels as `name`, when a folder is named for it.
fn keep(name: &str, (w, h): (u32, u32), pic: &[u8]) {
    let Some(dir) = std::env::var_os("AUBEREAN_VISTA_SHOT_DIR").map(std::path::PathBuf::from)
    else {
        return;
    };
    std::fs::create_dir_all(&dir).expect("the shot folder");
    let path = dir.join(format!("{name}.png"));
    let file = std::fs::File::create(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w, h);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(pic))
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
}

/// The game's own interface in the world, its status bar's focus meter in the state `state`
/// and its numbers `numbers` (shown or not), drawn on the device and kept as `name`.
fn retail(name: &'static str, more: Vec<Step>, state: &'static str, numbers: &str) {
    let numbers = numbers.to_owned();
    let (size, shot) = guarded(30, move |note| {
        doing(note, "entering the world");
        let mut s = Session::going_on(None, more);
        s.enter_the_world();
        doing(note, "waiting for the focus bar");
        let focus = |s: &Session| {
            let r = s.retail.borrow();
            let ui = r.ui();
            let meter = ui.element(Token::of("FocusMeter"))?;
            let text = ui.element(Token::of("FocusText"))?;
            Some((ui.node(meter)?.state, ui.text(text).unwrap_or_default()))
        };
        let shown = s.until(10.0, |s| {
            focus(s) == Some((Token::of(state), numbers.clone()))
        });
        assert!(shown, "{name}: the focus bar {:?}", focus(&s));
        // The screen settles: the world's entry fades out.
        for _ in 0..180 {
            s.step();
        }
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(
            note,
            "composing the frame on the device and reading it back",
        );
        let (w, h, shot) = composed(&mut s, &mut renderer);
        ((w, h), shot)
    });
    keep(name, size, &shot);
    let colors: std::collections::HashSet<&[u8]> = shot.chunks(4).collect();
    assert!(colors.len() > 500, "{name}: {} colors", colors.len());
}

/// The game's own interface shows a player who is no hero the focus bar's ornament, without
/// numbers.
#[test]
fn the_retail_focus_bar_for_a_player_who_is_no_hero() {
    retail("focus-retail-not-hero", Vec::new(), "not_hero", "1 / 1");
}

/// The game's own interface shows a hero its focus, half of its greatest.
#[test]
fn the_retail_focus_bar_for_a_hero_at_half_its_focus() {
    retail("focus-retail-hero", a_hero(), "hero", "50 / 100");
}

/// Horizon's parameter bar for a hero at half its focus, as the player's scripts say it.
#[test]
fn horizons_focus_gauge_for_a_hero_at_half_its_focus() {
    let (size, shot) = guarded(30, |note| {
        doing(note, "entering the world");
        let mut s = Session::going_on(Some("[UI]\r\nInterface=Horizon\r\n"), a_hero());
        s.enter_the_world();
        for _ in 0..60 {
            s.step();
        }
        // The scripts' word that the player is a hero, at half its focus.
        let long = |v: u64| [(v >> 32) as u32, (v & 0xFFFF_FFFF) as u32];
        let mut words = vec![1];
        words.extend(long(50));
        words.extend(long(100));
        s.shell
            .client
            .as_mut()
            .expect("the client")
            .client_mut()
            .notices
            .push(PanelNotice {
                native: 0x0085 << 16 | 0x0012,
                words,
                ..PanelNotice::default()
            });
        doing(note, "waiting for the focus");
        let told = s.until(2.0, |s| {
            s.horizon().borrow().state().focus
                == Vital {
                    current: 50,
                    max: 100,
                }
        });
        assert!(told, "the focus: {:?}", s.horizon().borrow().state().focus);
        for _ in 0..180 {
            s.step();
        }
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(
            note,
            "composing the frame on the device and reading it back",
        );
        let (w, h, shot) = composed(&mut s, &mut renderer);
        ((w, h), shot)
    });
    keep("focus-horizon-hero", size, &shot);
    let colors: std::collections::HashSet<&[u8]> = shot.chunks(4).collect();
    assert!(colors.len() > 500, "{} colors", colors.len());
}
