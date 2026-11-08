//! The Client page of the settings ends, in a build with the remaster, with its boxes under a
//! heading of their own. The art pack's box is there only while a pack is open.
//!
//! Behaviour: none (experimental Horizon interface)
#![cfg_attr(not(feature = "fidelity"), allow(unused_imports, dead_code))]

use dereth_client_contract::options::names::fidelity as names;
use dereth_client_contract::options::store;
use dereth_client_contract::view::PrefValue;
use dereth_client_contract::UiRequest;
use dereth_horizon::ui::game::GameState;
use dereth_horizon::ui::panels::options::tab;
use dereth_horizon::ui::panels::WindowId;

use crate::Harness;

/// The Client page, scrolled to its end, with an art pack open or not.
fn client_page(#[cfg(feature = "fidelity")] art_pack_open: bool) -> (Harness, GameState) {
    store::init();
    #[allow(unused_mut)]
    let mut state = GameState {
        in_world: true,
        name: "Tester".into(),
        ..GameState::default()
    };
    #[cfg(feature = "fidelity")]
    {
        state.art_pack_open = art_pack_open;
    }
    let mut h = Harness::new(Default::default());
    h.frame(&state);
    h.ui.windows.open(WindowId::Options, 0.0);
    h.ui.windows.options_page.tab = tab::CLIENT;
    h.frame(&state);
    h.ui.windows.options_page.scroll[tab::CLIENT] = 1.0e6;
    h.frame(&state);
    h.frame(&state);
    (h, state)
}

/// Scroll the Client page so the row captioned `caption` shows whole in its list, from the page's
/// top down a few pixels at a time; whether it was found. The remaster's rows are more than the
/// list shows at once.
fn show(h: &mut Harness, state: &GameState, caption: &str) -> bool {
    let whole = |h: &Harness| {
        let page = &h.ui.windows.options_page;
        match (page.control("list"), page.control(caption)) {
            (Some(list), Some(r)) => r.y >= list.y && r.bottom() <= list.bottom(),
            _ => false,
        }
    };
    if whole(h) {
        return true;
    }
    h.ui.windows.options_page.scroll[tab::CLIENT] = 0.0;
    h.frame(state);
    for _ in 0..2000 {
        if whole(h) {
            return true;
        }
        h.ui.windows.options_page.scroll[tab::CLIENT] += 6.0;
        h.frame(state);
    }
    false
}

/// Click the box of the row captioned `caption`, and what the click asked the game for.
fn tick(h: &mut Harness, state: &GameState, caption: &str) -> Vec<UiRequest> {
    assert!(show(h, state, caption), "no {caption} row");
    let r =
        h.ui.windows
            .options_page
            .control(caption)
            .unwrap_or_else(|| panic!("no {caption} row"));
    h.move_to(r.x + 10.0, r.y + r.h / 2.0);
    h.press();
    let mut out = h.frame(state).requests;
    h.release();
    out.extend(h.frame(state).requests);
    out
}

fn sets(requests: &[UiRequest], name: &str) -> Option<PrefValue> {
    requests.iter().find_map(|r| match r {
        UiRequest::SetPreference(n, v) if *n == name => Some(v.clone()),
        _ => None,
    })
}

/// The remaster's own box, as the page captions and notes it.
struct Box {
    caption: &'static str,
    note: &'static str,
}

fn remaster_box() -> Box {
    Box {
        caption: dereth_client_contract::options::remaster::CAPTION,
        note: dereth_client_contract::options::remaster::NOTE,
    }
}

#[test]
#[cfg(not(feature = "fidelity"))]
fn without_the_remaster_the_client_page_has_none_of_its_boxes() {
    let (h, _) = client_page();
    assert!(h
        .ui
        .windows
        .options_page
        .control(dereth_client_contract::options::remaster::CAPTION)
        .is_none());
}

/// The remaster's box ticks as any other, and what it asks for is the remaster's preference.
#[test]
#[cfg(feature = "fidelity")]
fn the_remaster_box_ticks_on_and_off() {
    let (mut h, state) = client_page(false);
    let caption = dereth_client_contract::options::remaster::CAPTION;
    let asked = tick(&mut h, &state, caption);
    assert_eq!(sets(&asked, names::REMASTER), Some(PrefValue::Bool(true)));
    store::set_value(names::REMASTER, PrefValue::Bool(true));
    h.frame(&state);
    let asked = tick(&mut h, &state, caption);
    assert_eq!(sets(&asked, names::REMASTER), Some(PrefValue::Bool(false)));
}

/// A remaster box's note, shown while the pointer is over its caption, is drawn over the rows under
/// it: after every row of the page, and not cut to the page's list.
#[test]
#[cfg(feature = "fidelity")]
fn a_remaster_box_s_note_is_drawn_over_every_row_of_the_page() {
    let (mut h, state) = client_page(false);
    // The remaster's own box, with its others in the rows under it.
    let o = remaster_box();
    assert!(show(&mut h, &state, o.caption), "the remaster's box");
    let page = &h.ui.windows.options_page;
    let list = page.control("list").expect("the page's list");
    let r = page.control(o.caption).expect("the remaster's box");
    let (x, y) = (list.x + 40.0, r.y + r.h / 2.0);
    h.move_to(x, y);
    h.frame(&state);
    assert_eq!(
        h.ui.windows.tip(),
        Some(&(o.caption.to_owned(), vec![o.note.to_owned()]))
    );
    // Its ground stands beside the pointer.
    let at = h
        .list
        .quads
        .iter()
        .position(|q| {
            q.tex.is_none()
                && (q.dst.x - (x + 18.0)).abs() < 0.5
                && (q.dst.y - (y + 18.0)).abs() < 0.5
        })
        .expect("the note's ground");
    h.assert_over_rows(at, list, "the note");
}

/// A remaster box's row the list shows only in part shows its note only over the part shown: the
/// pointer under the list, over the Defaults button, shows none.
#[test]
#[cfg(feature = "fidelity")]
fn a_remaster_box_s_note_shows_only_over_the_part_of_its_row_the_list_shows() {
    let (mut h, state) = client_page(false);
    let o = remaster_box();
    assert!(show(&mut h, &state, o.caption), "the remaster's box");
    let page = &h.ui.windows.options_page;
    let list = page.control("list").expect("the page's list");
    let r = page.control(o.caption).expect("the remaster's box");
    // Scrolled back until the row's top is ten pixels above the list's foot.
    let row_top = r.y - 3.0;
    h.ui.windows.options_page.scroll[tab::CLIENT] -= list.bottom() - 10.0 - row_top;
    h.frame(&state);
    let r =
        h.ui.windows
            .options_page
            .control(o.caption)
            .expect("the row, in part");
    assert!(
        r.y < list.bottom() && r.bottom() > list.bottom(),
        "{r:?} {list:?}"
    );
    h.move_to(list.x + 40.0, list.bottom() + 8.0);
    h.frame(&state);
    assert_eq!(h.ui.windows.tip(), None);
    h.move_to(list.x + 40.0, list.bottom() - 4.0);
    h.frame(&state);
    assert_eq!(h.ui.windows.tip().map(|(t, _)| t.as_str()), Some(o.caption));
}

/// The art pack's box is not on the page while no pack is open, since it would draw nothing; with
/// one open it is there, ticks as any box does, and unticked draws the world's own models.
#[test]
#[cfg(feature = "fidelity")]
fn the_art_pack_box_is_on_the_page_only_while_a_pack_is_open() {
    let (mut h, state) = client_page(false);
    assert!(!show(&mut h, &state, "Art pack"), "no pack, no box");
    let (mut h, state) = client_page(true);
    let name = names::REMASTER_ART_PACK;
    let asked = tick(&mut h, &state, "Art pack");
    assert_eq!(sets(&asked, name), Some(PrefValue::Bool(true)));
}

/// Where the client carries NVIDIA's DLSS library, the page names it under the remaster's boxes as
/// NVIDIA's license asks; where it does not, it says nothing of it.
#[test]
#[cfg(feature = "fidelity")]
fn the_page_names_nvidia_dlss_only_where_the_client_carries_it() {
    use dereth_client_contract::options::remaster::DLSS_ATTRIBUTION;
    let (mut h, mut state) = client_page(false);
    assert!(
        !show(&mut h, &state, DLSS_ATTRIBUTION),
        "no library, no line"
    );
    state.dlss_carried = true;
    h.frame(&state);
    assert!(
        show(&mut h, &state, DLSS_ATTRIBUTION),
        "the line under the boxes"
    );
}

/// Not a claim: draws the Client page scrolled to its end, the remaster's boxes and NVIDIA's
/// attribution under them, as a client that carries NVIDIA's DLSS library shows it, with the
/// interface's own art, into the PNG file `DERETH_HORIZON_SHOT` names (`<path>.png`), for a person
/// to look at. The quads are composited in software: nearest texels, straight alpha.
#[test]
#[cfg(feature = "fidelity")]
#[ignore = "an instrument: set DERETH_HORIZON_SHOT to an output path without its extension"]
fn draw_the_remasters_section_for_a_person_to_look_at() {
    use dereth_horizon::art::Art;
    use dereth_horizon::draw::DrawList;
    use dereth_horizon::ui::input::InputFrame;
    use dereth_horizon::ui::HorizonUi;
    use std::sync::Arc;
    let Some(path) = std::env::var_os("DERETH_HORIZON_SHOT") else {
        return;
    };
    let path = std::path::PathBuf::from(path);
    {
        store::init();
        let state = GameState {
            in_world: true,
            name: "Tester".into(),
            dlss_carried: true,
            ..GameState::default()
        };
        let art = Arc::new(Art::new(Arc::new(
            dereth_horizon::pieces::Pieces::built_in().expect("the pieces built in"),
        )));
        let mut h = Harness {
            ui: HorizonUi::new(Arc::clone(&art), Default::default()),
            list: DrawList::default(),
            input: InputFrame::default(),
            screen: (1920.0, 1080.0),
        };
        h.frame(&state);
        h.ui.windows.open(WindowId::Options, 0.0);
        h.ui.windows.options_page.tab = tab::CLIENT;
        for _ in 0..3 {
            h.frame(&state);
        }
        h.ui.windows.options_page.scroll[tab::CLIENT] = 1.0e6;
        for _ in 0..30 {
            h.frame(&state);
        }
        let (w, hgt) = (1920usize, 1080usize);
        let mut px = vec![[24.0f32, 26.0, 30.0]; w * hgt];
        for q in &h.list.quads {
            let image = q.tex.and_then(|t| art.image(t));
            let mut r = q.dst;
            if let Some(c) = q.clip {
                match r.intersect(&c) {
                    Some(v) => r = v,
                    None => continue,
                }
            }
            #[allow(
                clippy::cast_possible_truncation,
                clippy::cast_sign_loss,
                clippy::cast_precision_loss
            )]
            for y in (r.y.max(0.0) as usize)..(r.bottom().min(hgt as f32).max(0.0) as usize) {
                for x in (r.x.max(0.0) as usize)..(r.right().min(w as f32).max(0.0) as usize) {
                    let ca = [
                        ((q.colour >> 16) & 0xFF) as f32 / 255.0,
                        ((q.colour >> 8) & 0xFF) as f32 / 255.0,
                        (q.colour & 0xFF) as f32 / 255.0,
                        ((q.colour >> 24) & 0xFF) as f32 / 255.0,
                    ];
                    let texel = match &image {
                        Some(img) => {
                            let u = q.src.x + (x as f32 + 0.5 - q.dst.x) / q.dst.w * q.src.w;
                            let v = q.src.y + (y as f32 + 0.5 - q.dst.y) / q.dst.h * q.src.h;
                            let tx = (u.max(0.0) as usize).min(img.width as usize - 1);
                            let ty = (v.max(0.0) as usize).min(img.height as usize - 1);
                            let i = (ty * img.width as usize + tx) * 4;
                            let b = &img.bgra[i..i + 4];
                            [
                                f32::from(b[2]) / 255.0,
                                f32::from(b[1]) / 255.0,
                                f32::from(b[0]) / 255.0,
                                f32::from(b[3]) / 255.0,
                            ]
                        }
                        None if q.tex.is_some() => continue,
                        None => [1.0, 1.0, 1.0, 1.0],
                    };
                    let a = texel[3] * ca[3];
                    let p = &mut px[y * w + x];
                    for k in 0..3 {
                        p[k] = p[k] * (1.0 - a) + 255.0 * texel[k] * ca[k] * a;
                    }
                }
            }
        }
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let rgb: Vec<u8> = px
            .iter()
            .flat_map(|p| p.map(|c| c.clamp(0.0, 255.0) as u8))
            .collect();
        let out = path.with_extension("png");
        let file = std::fs::File::create(&out).expect("the picture's file");
        #[allow(clippy::cast_possible_truncation)]
        let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, hgt as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .and_then(|mut wr| wr.write_image_data(&rgb))
            .expect("the picture writes");
        eprintln!("wrote {}", out.display());
    }
}
