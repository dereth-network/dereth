//! Horizon's HUD on a graphics device: the attack button at peace, then attacking with the player
//! selected, the target bar over it, and the parameter bar's vitals in both; and the HUD Layout
//! editor open over a HUD showing no effects, target, fellowship or pets, every element outlined;
//! each written to look at (`hud-peace`, `hud-attacking`, `hud-layout-editor`).
//!
//! Behaviour: none (reference pictures to look at, claimed by the HUD's and the hotbar's own tests)

use auberean_client_sdk::contract::Action;
use auberean_client_sdk::protocol::login::PlayerQualities;
use auberean_client_sdk::protocol::table::{Table, SMALL};
use auberean_client_sdk::protocol::types::Qualities;
use auberean_client_sdk::protocol::ServerMessage;
use auberean_client_sdk::runtime::host::PanelNotice;
use auberean_client_sdk::runtime::selection::{Selection, WITH_NUMBERS};
use auberean_testkit::double::script::Event as Step;

use super::composed;
use super::session::{self, doing, guarded, Session, PLAYER};

/// The player's qualities: health 80 of 100, vigor 55 of 120.
fn qualities() -> Vec<Step> {
    vec![session::s2c(
        8.10,
        1,
        1,
        session::WORLD | 7,
        &ServerMessage::PlayerQualities(PlayerQualities(Qualities {
            header: Qualities::INTS,
            ints: Table::from_inserted(SMALL, [(258, 80), (263, 100), (259, 55), (264, 120)]),
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

/// The frame composed on the device, after the screen settles.
fn shot(s: &mut Session, renderer: &mut auberean_gpu::Renderer) -> ((u32, u32), Vec<u8>) {
    for _ in 0..120 {
        s.step();
    }
    let (w, h, pic) = composed(s, renderer);
    ((w, h), pic)
}

/// The HUD at peace, then attacking with the player selected at most of its health.
#[test]
fn horizons_attack_button_and_target_bar_at_peace_and_attacking() {
    let shots = guarded(10, |note| {
        doing(note, "entering the world");
        let mut s = Session::going_on(Some("[UI]\r\nInterface=Horizon\r\n"), qualities());
        s.enter_the_world();
        let told = s.until(10.0, |s| s.horizon().borrow().state().vigor.max == 120);
        assert!(told, "the vitals: {:?}", s.horizon().borrow().state().vigor);
        for _ in 0..60 {
            s.step();
        }
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(note, "composing the frame at peace");
        let peace = shot(&mut s, &mut renderer);

        doing(note, "selecting the player and attacking");
        let client = s.shell.client.as_mut().expect("the client").client_mut();
        client.act(Action::Select { target: PLAYER });
        client.notices.push(PanelNotice {
            native: 0x0085 << 16 | 0x000B,
            selections: vec![Some(Selection {
                kind: WITH_NUMBERS,
                cur_health: 70,
                max_health: 100,
                cur_vigor: 55,
                max_vigor: 120,
                ..Selection::default()
            })],
            ..PanelNotice::default()
        });
        client.notices.push(PanelNotice {
            native: 0x0086 << 16 | 0x0001,
            words: vec![1],
            ..PanelNotice::default()
        });
        let attacking = s.until(2.0, |s| {
            let h = s.horizon();
            let h = h.borrow();
            let st = h.state();
            st.combat.attacking && st.target.as_ref().is_some_and(|t| t.health.is_some())
        });
        assert!(
            attacking,
            "attacking with a target: {:?}",
            s.horizon().borrow().state().target
        );
        doing(note, "composing the frame attacking");
        let war = shot(&mut s, &mut renderer);
        (peace, war)
    });
    for (name, (size, pic)) in [("hud-peace", shots.0), ("hud-attacking", shots.1)] {
        keep(name, size, &pic);
        let colors: std::collections::HashSet<&[u8]> = pic.chunks(4).collect();
        assert!(colors.len() > 500, "{name}: {} colors", colors.len());
    }
}

/// The HUD Layout editor open with nothing selected, for a player who is no hero with no effects,
/// nothing selected, no fellowship and no pets: every element's box shown.
#[test]
fn horizons_hud_layout_editor_outlines_every_element_with_nothing_to_show() {
    let shot = guarded(10, |note| {
        doing(note, "entering the world");
        let mut s = Session::going_on(Some("[UI]\r\nInterface=Horizon\r\n"), qualities());
        s.enter_the_world();
        let told = s.until(10.0, |s| s.horizon().borrow().state().vigor.max == 120);
        assert!(told, "the vitals: {:?}", s.horizon().borrow().state().vigor);
        doing(note, "opening the editor");
        s.horizon().borrow_mut().ui_mut().layout.open_editor();
        for _ in 0..60 {
            s.step();
        }
        let outlined = s.horizon().borrow().ui().layout.outlines.len();
        assert_eq!(
            outlined,
            auberean_horizon::ui::layout::element::ALL.len(),
            "every element outlined"
        );
        doing(note, "asking for a graphics device");
        let mut renderer =
            auberean_gpu::Renderer::new().unwrap_or_else(|e| panic!("a hardware device: {e}"));
        eprintln!("adapter: {}", renderer.adapter);
        doing(note, "composing the frame with the editor open");
        shot(&mut s, &mut renderer)
    });
    let (size, pic) = shot;
    keep("hud-layout-editor", size, &pic);
    let colors: std::collections::HashSet<&[u8]> = pic.chunks(4).collect();
    assert!(colors.len() > 500, "{} colors", colors.len());
}
