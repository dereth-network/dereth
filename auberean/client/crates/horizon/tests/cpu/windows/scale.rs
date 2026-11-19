//! The interface scale: the steps a window offers, the step drawn at, and the settings window's
//! choice of it, kept in Horizon's own file.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use auberean_horizon::own::Own;
use auberean_horizon::scale::{automatic, in_effect, layout_scale_at, note, offered, STEPS};
use auberean_horizon::ui::settings::{Settings, SCALES};
use auberean_horizon::vista::{Choice, Vista};

use crate::{in_world, Harness};

#[test]
fn each_monitor_offers_the_steps_its_pixels_hold_and_auto_takes_the_largest_up_to_200() {
    let offers =
        |w: (f32, f32)| -> Vec<f32> { STEPS.iter().copied().filter(|s| offered(*s, w)).collect() };
    assert_eq!(offers((1920.0, 1080.0)), [1.0]);
    assert_eq!(offers((2560.0, 1440.0)), [1.0, 1.5]);
    assert_eq!(offers((3840.0, 2160.0)), [1.0, 1.5, 2.0]);
    assert_eq!(offers((5120.0, 2880.0)), STEPS);
    assert_eq!(automatic((2560.0, 1440.0)), 1.5);
    assert_eq!(automatic((5120.0, 2880.0)), 2.0, "never 300% unasked");
    // Each step from nine tenths of the canvas at that step, and not a pixel before.
    assert!(offered(1.5, (2160.0, 1296.0)) && !offered(1.5, (2159.0, 1296.0)));
    assert!(offered(2.0, (2880.0, 1728.0)) && !offered(2.0, (2880.0, 1727.0)));
}

#[test]
fn a_chosen_step_is_drawn_while_the_window_offers_it_and_auto_s_while_it_does_not() {
    let uhd = (3840.0, 2160.0);
    assert_eq!(in_effect(Some(1.0), uhd), 1.0, "below Auto's");
    assert_eq!(in_effect(Some(3.0), uhd), 2.0, "not offered: Auto's");
    assert_eq!(in_effect(Some(2.0), (1920.0, 1080.0)), 1.0);
    assert_eq!(layout_scale_at(uhd, Some(1.5)), 1.5);
    // A window shorter than 1080 lines shrinks the layouts.
    assert_eq!(layout_scale_at((1280.0, 720.0), None), 720.0 / 1080.0);
    assert_eq!(
        note(Some(2.0), (1920.0, 1080.0)).as_deref(),
        Some("200% needs a window of 2880 \u{d7} 1728 or larger: 100% until then.")
    );
    assert_eq!(
        note(None, (1920.0, 1080.0)).as_deref(),
        Some("150% and larger need a window of 2160 \u{d7} 1296 or larger.")
    );
}

#[test]
fn choosing_a_scale_in_the_settings_draws_at_it_from_the_next_frame_and_asks_to_keep_it() {
    let mut h = Harness::new();
    h.screen = (3840.0, 2160.0);
    let s = in_world();
    h.ui.settings.toggle();
    h.frame(&s);
    let k = h.ui.layout_scale(h.screen);
    assert_eq!(k, 2.0, "Auto at 4K");
    let c = Settings::controls(h.ui.settings.body().expect("open"), k);
    // The second choice of five: 100%.
    #[allow(clippy::cast_precision_loss)]
    let at = (
        c.scale.x + c.scale.w / SCALES.len() as f32 + 9.0 * k,
        c.scale.y + c.scale.h / 2.0,
    );
    h.click(&s, at);
    assert_eq!(h.ui.settings.own.scale, Some(1.0));
    assert!(h.ui.settings.take_own_changed());
    assert!(!h.ui.settings.take_own_changed(), "told once");
    assert_eq!(h.ui.layout_scale(h.screen), 1.0);
    // Auto again.
    let k = h.ui.layout_scale(h.screen);
    let c = Settings::controls(h.ui.settings.body().expect("open"), k);
    h.click(&s, (c.scale.x + 9.0 * k, c.scale.y + c.scale.h / 2.0));
    assert_eq!(h.ui.settings.own.scale, None);
    assert_eq!(h.ui.layout_scale(h.screen), 2.0);
}

#[test]
fn horizon_s_own_file_reads_back_what_it_wrote_and_ignores_what_it_does_not_know() {
    let own = Own {
        scale: Some(1.5),
        vista: Choice::One(Vista::DrudgeCitadel),
        ..Own::default()
    };
    assert_eq!(own.write(), "scale 1.5\nvista drudge-citadel\n");
    assert_eq!(Own::read(&own.write()), own);
    let picked = Own {
        vista_last: Some(Vista::LinvakTukal),
        ..Own::default()
    };
    assert_eq!(
        picked.write(),
        "scale auto\nvista random\nvista-last linvak-tukal\n"
    );
    assert_eq!(Own::read(&picked.write()), picked);
    assert_eq!(Own::read("vista nowhere\n").vista, Choice::Random);
    assert_eq!(Own::read("vista-last nowhere\n").vista_last, None);
    assert_eq!(Own::read("scale auto\n"), Own::default());
    assert_eq!(
        Own::read("scale 9\nnonsense here\n"),
        Own::default(),
        "out of range"
    );
    assert_eq!(Own::read("").scale, None);
}
