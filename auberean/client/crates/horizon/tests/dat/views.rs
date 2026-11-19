//! The view behind character select chosen in Horizon's settings, or picked at random: drawn from
//! the install with no server, as Cragstone's is.

use auberean_horizon::ui::pregame::Screen;
use auberean_horizon::vista::{Choice, Vista};

use crate::session::{Folder, Session};

const HORIZON: &str = "[UI]\r\nInterface=Horizon\r\n";

/// With the Drudge Citadel chosen in Horizon's own file, character select stands over it: the
/// camera seen at the citadel's spot, turning as its path says at the frame's time.
///
/// Behaviour: desktop.horizon.character-select-stands-over-the-view-chosen
#[test]
fn character_select_stands_over_the_drudge_citadel_once_it_is_chosen() {
    let mut s = Session::choosing(Some(HORIZON));
    let folder = Folder::default();
    folder.0.lock().expect("the folder").insert(
        auberean_horizon::own::FILE.to_owned(),
        b"scale auto\nvista drudge-citadel\n".to_vec(),
    );
    s.shell.host.folder = Some(folder);
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    s.step();
    let spot = Vista::DrudgeCitadel.spot();
    for _ in 0..30 {
        assert!(s.world_drawn(), "the view drawn behind character select");
        let seen = s.shell.scene.seen.camera.expect("a camera seen");
        let asked = Vista::DrudgeCitadel.camera(s.shell.host.time);
        assert_eq!(s.shell.scene.seen.block, spot.block);
        assert_eq!(seen.position, spot.at);
        assert!((seen.heading - asked.heading).abs() < 1e-4, "{seen:?}");
        assert!((seen.pitch - spot.pitch).abs() < 1e-4);
    }
}

/// With Random chosen (the default), character select comes up over a view picked at random, not
/// the one Horizon's own file says was shown last, and the file now keeps the one picked; the
/// camera stands at that view's spot, turning as its path says, for as long as character select
/// is up.
///
/// Behaviour: desktop.horizon.character-select-shows-a-different-view-each-time-unless-one-is-chosen
#[test]
fn character_select_comes_up_over_a_view_picked_at_random_and_keeps_it_in_horizon_s_file() {
    let mut s = Session::choosing(Some(HORIZON));
    let folder = Folder::default();
    folder.0.lock().expect("the folder").insert(
        auberean_horizon::own::FILE.to_owned(),
        b"scale auto\nvista random\nvista-last linvak-tukal\n".to_vec(),
    );
    s.shell.host.folder = Some(folder.clone());
    let listed = s.until(10.0, |s| {
        s.horizon().borrow().screen() == Some(Screen::Select)
    });
    assert!(listed, "character select up");
    s.step();
    let kept = folder
        .0
        .lock()
        .expect("the folder")
        .get(auberean_horizon::own::FILE)
        .cloned()
        .expect("kept");
    let own = auberean_horizon::own::Own::read(&String::from_utf8_lossy(&kept));
    assert_eq!(own.vista, Choice::Random, "still Random");
    let picked = own.vista_last.expect("the view picked kept");
    assert_ne!(picked, Vista::LinvakTukal, "not the one shown last");
    let spot = picked.spot();
    for _ in 0..30 {
        assert!(s.world_drawn(), "the view drawn behind character select");
        let seen = s.shell.scene.seen.camera.expect("a camera seen");
        let asked = picked.camera(s.shell.host.time);
        assert_eq!(s.shell.scene.seen.block, spot.block, "{picked:?}");
        assert_eq!(seen.position, spot.at);
        assert!((seen.heading - asked.heading).abs() < 1e-4, "{seen:?}");
    }
}
