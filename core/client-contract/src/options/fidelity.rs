//! The words and rules of the optional high-fidelity presentation's options: seven check boxes on
//! the Horizon interface's Client page, under a heading of their own, each off until the player
//! ticks it.
//!
//! Three of the effects are drawn inside the better lighting and draw nothing without it: the sun
//! shadows, the bounced light and the lamps. The lamps also need a graphics device that traces
//! rays. [`blocked`] says which of these holds for a box, and the presentation follows the same
//! rules, so a box the page shows greyed draws nothing. (The bounced light darkens the creases
//! itself, so the ambient occlusion stands aside in the outdoor frames it is drawn into; indoors
//! and underground, where it draws nothing, the occlusion is drawn.)
//!
//! In a build with the `hifi` feature the options are registered with the value store
//! ([`register`]), so the page reads and writes them like any other option and the profile keeps
//! them in its `[Fidelity]` section. Without it nothing here is registered and no page shows them.

use super::names::fidelity;
use crate::PrefValue;

/// What a box needs before it draws anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Needs {
    /// Nothing: it draws on its own.
    Nothing,
    /// The better lighting, which it is drawn inside.
    Lighting,
    /// The better lighting, and a graphics device that traces rays.
    LightingAndRays,
}

/// One box as the page shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FidelityOption {
    /// The preference name, `Fidelity.<Key>`.
    pub name: &'static str,
    /// The caption on the page.
    pub caption: &'static str,
    /// What the effect does, for the row's tooltip.
    pub note: &'static str,
    /// What it needs.
    pub needs: Needs,
}

/// The seven boxes, in page order.
pub const OPTIONS: [FidelityOption; 7] = [
    FidelityOption {
        name: fidelity::LIGHTING,
        caption: "Better Lighting (HDR Sun and Sky Light)",
        note: "Lights the outdoor world again per pixel, in high dynamic range, with bloom and \
               exposure. The sun shadows, the bounced light and the lamps are drawn inside it.",
        needs: Needs::Nothing,
    },
    FidelityOption {
        name: fidelity::SHADOWS,
        caption: "Real-Time Sun Shadows",
        note: "Soft shadows from the sun, cast by the landscape, the buildings, the trees and the \
               scenery in view.",
        needs: Needs::Lighting,
    },
    FidelityOption {
        name: fidelity::GLOBAL_ILLUMINATION,
        caption: "Global Illumination (Bounced Light)",
        note: "Light bounced from the surfaces around, which fills the shadows outdoors. It \
               darkens the creases itself, so outdoors the ambient occlusion stands aside while \
               it is on.",
        needs: Needs::Lighting,
    },
    FidelityOption {
        name: fidelity::AMBIENT_OCCLUSION,
        caption: "Ambient Occlusion",
        note: "Darkens the creases and corners the sky's light reaches less, indoors and out.",
        needs: Needs::Nothing,
    },
    FidelityOption {
        name: fidelity::LAMPS,
        caption: "Lamps and Torches",
        note: "The outdoor lamps, lanterns, torches and braziers light the world after dusk, \
               stopped by the walls between.",
        needs: Needs::LightingAndRays,
    },
    FidelityOption {
        name: fidelity::SKY,
        caption: "Sky and Atmosphere",
        note: "A physical sky behind the authored clouds and sun, and air that blues the far \
               hills.",
        needs: Needs::Nothing,
    },
    FidelityOption {
        name: fidelity::WEATHER,
        caption: "Weather (Rain, Wet Ground, Puddles, Snow)",
        note: "On the game's rainy days: rain that roofs keep off, wet ground and puddles, and \
               snow instead over snowy land. It replaces the game's own falling rain.",
        needs: Needs::Nothing,
    },
];

/// The heading's caption.
pub const HEADING: &str = "Highly Experimental Rendering Effects";

/// The line under the heading.
pub const WARNING: &str =
    "Highly experimental. May look wrong or cost frame rate. Desktop wgpu renderer only.";

/// Why a ticked box draws nothing, as the page says it under the row.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Blocked {
    /// The better lighting is off.
    NeedsLighting,
    /// The graphics device does not trace rays.
    NeedsRays,
}

impl Blocked {
    /// The reason line.
    #[must_use]
    pub const fn reason(self) -> &'static str {
        match self {
            Self::NeedsLighting => "Needs Better Lighting",
            Self::NeedsRays => {
                "Needs ray tracing, which this graphics card, driver or backend lacks"
            }
        }
    }
}

/// The box named `name`.
#[must_use]
pub fn find(name: &str) -> Option<&'static FidelityOption> {
    OPTIONS.iter().find(|o| o.name.eq_ignore_ascii_case(name))
}

/// Why the box `name` draws nothing however it is ticked, with `ticked` saying which boxes are
/// ticked and `rays` whether the graphics device traces rays (`None` while that is not known yet,
/// which blocks nothing). `None` when it draws when ticked.
#[must_use]
pub fn blocked(name: &str, ticked: impl Fn(&str) -> bool, rays: Option<bool>) -> Option<Blocked> {
    let o = find(name)?;
    let lighting = ticked(fidelity::LIGHTING);
    match o.needs {
        Needs::Lighting | Needs::LightingAndRays if !lighting => Some(Blocked::NeedsLighting),
        Needs::LightingAndRays if rays == Some(false) => Some(Blocked::NeedsRays),
        _ => None,
    }
}

/// Register every box with the value store, each off. The store's load then reads the
/// `[Fidelity]` section of the profile into them, and the page reads and writes them.
pub fn register() -> usize {
    OPTIONS
        .iter()
        .map(|o| {
            usize::from(super::store::register_preference(
                o.name,
                PrefValue::Bool(false),
                super::store::DataType::Bool,
            ))
        })
        .sum()
}

/// Whether the store's save leaves `name` out of the profile: a box still off that the profile
/// does not name, so a profile that never ticked one has no `[Fidelity]` section.
#[must_use]
pub fn left_out_of_save(
    name: &str,
    value: &PrefValue,
    ini: &crate::persist::preferences::UserPreferences,
) -> bool {
    find(name).is_some()
        && *value == PrefValue::Bool(false)
        && !ini
            .entries
            .iter()
            .any(|(k, _)| k.eq_ignore_ascii_case(name))
}

/// Whether this build shows the boxes: the `hifi` feature.
pub const ON_THE_PAGE: bool = cfg!(feature = "hifi");

/// Where the effects stand on the device the client is drawing with, for the options page.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct Availability {
    /// The client was built with the effects.
    pub built: bool,
    /// The device is the `wgpu` renderer, the only one they draw on.
    pub wgpu: bool,
    /// The device was asked at start-up for what they draw with, as it is when the client
    /// starts in the Horizon interface on the `wgpu` renderer with a box ticked.
    pub widened: bool,
    /// The device has what they draw with.
    pub supported: bool,
    /// Whether the effects trace rays here, which the lamps need; `None` where that is not known.
    /// It is the graphics card's answer, so a `wgpu` device started without the effects says what
    /// the start with them will have.
    pub rays: Option<bool>,
    /// They drew the last frame.
    pub active: bool,
    /// Why they stopped, when they failed on this device and were taken off; they stay off until
    /// a box changes.
    pub failed: Option<String>,
    /// Where the renderer choice on the page leaves `wgpu`, while the device is another renderer
    /// and that choice says more than how to get it.
    pub wgpu_next: Option<WgpuNext>,
}

/// What the renderer choice says of `wgpu`, the renderer the effects draw on, while the client
/// draws with another.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WgpuNext {
    /// It is chosen, and the next start comes up on it.
    AtRestart,
    /// It is chosen, but `--renderer` holds this run to another: a start without it comes up on
    /// `wgpu`.
    WithoutCommandLine,
    /// This start asked for it and could not create it, so a restart asking again would not
    /// draw the effects either.
    DidNotStart,
}

/// Where the renderer choice leaves `wgpu`, from `renderers` and the choice the store holds
/// (`stored`): `None` while the device is `wgpu`, or while nothing says more than how to choose
/// it.
#[must_use]
pub fn wgpu_next(
    renderers: &super::renderer::RendererStatus,
    stored: Option<crate::renderer::RendererChoice>,
) -> Option<WgpuNext> {
    use crate::renderer::RendererChoice::Wgpu;
    let running = renderers.running?;
    if running == Wgpu {
        return None;
    }
    if renderers.asked() == Some(Wgpu) {
        return Some(WgpuNext::DidNotStart);
    }
    if renderers.next(stored) != Some(Wgpu) {
        return None;
    }
    Some(if renderers.command_line.is_some() {
        WgpuNext::WithoutCommandLine
    } else {
        WgpuNext::AtRestart
    })
}

/// How the player gets the renderer the effects draw on: the renderer choice above them on the
/// page, which takes effect at the next start.
const CHOOSE_WGPU: &str = "Off on this renderer: choose wgpu as the Renderer above, then restart";

/// The longest a failure's reason is shown on the status line, in characters; the log has it
/// whole.
const REASON_SHOWN: usize = 48;

/// What the chat window tells a player who ticks a box while the client draws with a renderer
/// other than `wgpu`.
pub const REFUSED_RENDERER: &str = "The experimental rendering effects are off on this renderer: they draw only on the wgpu renderer (the Renderer choice on Horizon's Client page, or Renderer=wgpu), and a box first ticked takes effect at the next start.";

/// What the chat window tells a player who ticks a box on another renderer while `wgpu` is chosen
/// for the next start: that start has them.
pub const REFUSED_UNTIL_RESTART_ONTO_WGPU: &str =
    "The experimental rendering effects take effect at the restart onto wgpu: restart to apply them.";

/// What the chat window tells a player who ticks a box on the `wgpu` renderer started without
/// the effects: the next start has them.
pub const REFUSED_UNTIL_RESTART: &str =
    "The experimental rendering effects take effect at the next start: restart to apply them.";

/// What the chat window tells a player who ticks a box on a `wgpu` device that lacks what the
/// effects draw with.
pub const REFUSED_BACKEND: &str =
    "The experimental rendering effects are off: this graphics backend can't run them.";

impl Availability {
    /// Whether the boxes can be ticked here: the client has the effects and draws with the
    /// `wgpu` renderer, or `wgpu` is chosen for the next start, which applies them. Elsewhere the
    /// page shows them greyed, and the status line says how to get them.
    #[must_use]
    pub fn offered(&self) -> bool {
        self.built && (self.wgpu || self.wgpu_next == Some(WgpuNext::AtRestart))
    }

    /// The status line under the heading: whether the effects are drawn, or what stops them,
    /// with `ticked` saying whether any box is ticked. `None` when there is nothing to say: no
    /// box is ticked and a tick would draw, or would take effect at the next start.
    #[must_use]
    pub fn status(&self, ticked: bool) -> Option<String> {
        if !self.built {
            return None;
        }
        if !self.wgpu {
            return Some(
                match self.wgpu_next {
                    Some(WgpuNext::AtRestart) if ticked => "Restart onto wgpu to apply",
                    Some(WgpuNext::AtRestart) => "Off until the restart onto wgpu",
                    Some(WgpuNext::WithoutCommandLine) => {
                        "Off until a restart onto wgpu without --renderer"
                    }
                    Some(WgpuNext::DidNotStart) => "Off: wgpu did not start here",
                    None => CHOOSE_WGPU,
                }
                .to_owned(),
            );
        }
        if !ticked {
            return None;
        }
        if let Some(why) = &self.failed {
            let mut why: String = why.chars().take(REASON_SHOWN).collect();
            if why.len() < self.failed.as_ref().map_or(0, String::len) {
                why.push_str("...");
            }
            return Some(format!(
                "Stopped after an error ({why}): untick and tick a box to retry"
            ));
        }
        if self.active {
            return Some("Active".to_owned());
        }
        if !self.widened {
            return Some("Restart to apply".to_owned());
        }
        if !self.supported {
            return Some("This graphics backend can't run these effects".to_owned());
        }
        None
    }

    /// What the chat window tells a player whose ticked box this device cannot draw: that the
    /// restart onto `wgpu` chosen for the next start applies them, that they draw only on `wgpu`,
    /// that the next start applies them, or that this backend cannot run them.
    #[must_use]
    pub fn refusal(&self) -> &'static str {
        if !self.wgpu {
            if self.wgpu_next == Some(WgpuNext::AtRestart) {
                REFUSED_UNTIL_RESTART_ONTO_WGPU
            } else {
                REFUSED_RENDERER
            }
        } else if !self.widened {
            REFUSED_UNTIL_RESTART
        } else {
            REFUSED_BACKEND
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the experimental presentation's option words and rules; visual only).
    use super::*;

    #[test]
    fn every_box_has_its_name_and_the_names_list_holds_every_box() {
        for o in &OPTIONS {
            assert!(fidelity::NAMES.contains(&o.name), "{}", o.name);
            assert_eq!(find(&o.name.to_ascii_lowercase()), Some(o));
        }
        assert!(find(fidelity::DEBUG).is_none(), "the debug view has no box");
    }

    #[test]
    fn a_box_whose_prerequisite_is_off_or_covered_says_why() {
        let with = |on: &'static [&'static str]| move |n: &str| on.contains(&n);
        use fidelity::{
            AMBIENT_OCCLUSION, GLOBAL_ILLUMINATION, LAMPS, LIGHTING, SHADOWS, SKY, WEATHER,
        };
        for name in [SHADOWS, GLOBAL_ILLUMINATION, LAMPS] {
            assert_eq!(
                blocked(name, with(&[]), Some(true)),
                Some(Blocked::NeedsLighting)
            );
            assert_eq!(blocked(name, with(&[LIGHTING]), Some(true)), None);
        }
        assert_eq!(
            blocked(LAMPS, with(&[LIGHTING]), Some(false)),
            Some(Blocked::NeedsRays)
        );
        assert_eq!(blocked(LAMPS, with(&[LIGHTING]), None), None);
        assert_eq!(blocked(AMBIENT_OCCLUSION, with(&[]), Some(false)), None);
        // The occlusion is never greyed: indoors it is the only effect drawn.
        assert_eq!(
            blocked(
                AMBIENT_OCCLUSION,
                with(&[LIGHTING, GLOBAL_ILLUMINATION]),
                None
            ),
            None
        );
        assert_eq!(blocked(SKY, with(&[]), Some(false)), None);
        // The weather stands alone: it draws over the ordinary picture or the better light.
        assert_eq!(blocked(WEATHER, with(&[]), Some(false)), None);
        assert_eq!(blocked(LIGHTING, with(&[]), Some(false)), None);
    }

    #[test]
    fn the_renderer_choice_leaves_wgpu_at_the_restart_behind_the_command_line_or_failed() {
        use crate::options::renderer::RendererStatus;
        use crate::renderer::RendererChoice::{Vulkan, Wgpu};
        let desktop = RendererStatus {
            offered: vec![Vulkan, Wgpu],
            running: Some(Vulkan),
            preference: None,
            command_line: None,
        };
        assert_eq!(wgpu_next(&desktop, None), None, "nothing chosen");
        assert_eq!(wgpu_next(&desktop, Some(Vulkan)), None);
        assert_eq!(
            wgpu_next(&desktop, Some(Wgpu)),
            Some(WgpuNext::AtRestart),
            "chosen on the page"
        );
        // `--renderer vulkan`, and wgpu chosen on the page or in the preferences file.
        let held = RendererStatus {
            command_line: Some(Vulkan),
            ..desktop.clone()
        };
        assert_eq!(
            wgpu_next(&held, Some(Wgpu)),
            Some(WgpuNext::WithoutCommandLine)
        );
        let held_by_file = RendererStatus {
            preference: Some(Wgpu),
            ..held.clone()
        };
        assert_eq!(
            wgpu_next(&held_by_file, None),
            Some(WgpuNext::WithoutCommandLine)
        );
        // The start asked for wgpu, from the preferences file or the command line, and came up
        // on Vulkan: a restart asks for it again in vain, whatever is chosen now.
        let fell_back = RendererStatus {
            preference: Some(Wgpu),
            ..desktop.clone()
        };
        for stored in [None, Some(Wgpu), Some(Vulkan)] {
            assert_eq!(
                wgpu_next(&fell_back, stored),
                Some(WgpuNext::DidNotStart),
                "{stored:?}"
            );
        }
        let named_and_fell_back = RendererStatus {
            command_line: Some(Wgpu),
            ..desktop.clone()
        };
        assert_eq!(
            wgpu_next(&named_and_fell_back, Some(Wgpu)),
            Some(WgpuNext::DidNotStart)
        );
        // On wgpu, or with no device, there is nothing to say.
        let on_wgpu = RendererStatus {
            running: Some(Wgpu),
            preference: Some(Wgpu),
            ..desktop.clone()
        };
        assert_eq!(wgpu_next(&on_wgpu, Some(Vulkan)), None);
        assert_eq!(wgpu_next(&RendererStatus::default(), Some(Wgpu)), None);
    }

    #[test]
    fn a_refused_box_is_told_what_is_still_needed_and_never_told_wgpu_on_wgpu() {
        let on_wgpu = Availability {
            built: true,
            wgpu: true,
            widened: false,
            supported: true,
            rays: Some(true),
            active: false,
            failed: None,
            wgpu_next: None,
        };
        // Started on wgpu without the effects: the restart is all that is left, as the status
        // line says.
        assert_eq!(on_wgpu.refusal(), REFUSED_UNTIL_RESTART);
        assert_eq!(on_wgpu.status(true).as_deref(), Some("Restart to apply"));
        // Started with them, on a backend that cannot run them.
        let short = Availability {
            widened: true,
            supported: false,
            ..on_wgpu.clone()
        };
        assert_eq!(short.refusal(), REFUSED_BACKEND);
        for a in [&on_wgpu, &short] {
            assert!(!a.refusal().contains("only on the wgpu"), "{a:?}");
        }
        // Another renderer, or a presentation that has no device to say.
        let native = Availability {
            wgpu: false,
            widened: false,
            supported: false,
            rays: None,
            ..on_wgpu
        };
        assert_eq!(native.refusal(), REFUSED_RENDERER);
        assert_eq!(Availability::default().refusal(), REFUSED_RENDERER);
        // Another renderer with wgpu chosen for the next start: the restart onto it applies them,
        // and only a plain restart onto it does.
        let chosen = |next| Availability {
            wgpu_next: Some(next),
            ..native.clone()
        };
        assert_eq!(
            chosen(WgpuNext::AtRestart).refusal(),
            REFUSED_UNTIL_RESTART_ONTO_WGPU
        );
        for next in [WgpuNext::WithoutCommandLine, WgpuNext::DidNotStart] {
            assert_eq!(chosen(next).refusal(), REFUSED_RENDERER, "{next:?}");
        }
    }

    #[test]
    fn the_boxes_tick_on_another_renderer_once_wgpu_is_chosen_for_a_plain_restart() {
        let native = Availability {
            built: true,
            ..Availability::default()
        };
        assert!(!native.offered(), "nothing chosen");
        let chosen = |next| Availability {
            wgpu_next: Some(next),
            ..native.clone()
        };
        assert!(chosen(WgpuNext::AtRestart).offered());
        // A restart that would not come up on wgpu applies nothing, so nothing ticks for it.
        for next in [WgpuNext::WithoutCommandLine, WgpuNext::DidNotStart] {
            assert!(!chosen(next).offered(), "{next:?}");
        }
        let unbuilt = Availability {
            built: false,
            ..chosen(WgpuNext::AtRestart)
        };
        assert!(!unbuilt.offered());
    }

    #[test]
    fn the_status_line_says_what_stops_the_effects_and_nothing_with_no_box_ticked() {
        let ready = Availability {
            built: true,
            wgpu: true,
            widened: true,
            supported: true,
            rays: Some(true),
            active: false,
            failed: None,
            wgpu_next: None,
        };
        assert!(ready.offered());
        for ticked in [false, true] {
            assert_eq!(ready.status(ticked), None, "ticked={ticked}");
        }
        let active = Availability {
            active: true,
            ..ready.clone()
        };
        assert_eq!(active.status(true).as_deref(), Some("Active"));
        assert_eq!(active.status(false), None);
        // The default renderer, not wgpu: never offered, and the line says how to get them,
        // ticked or not.
        let default_device = Availability {
            wgpu: false,
            widened: false,
            supported: false,
            rays: None,
            ..ready.clone()
        };
        assert!(!default_device.offered());
        for ticked in [false, true] {
            let line = default_device.status(ticked).unwrap();
            assert!(line.contains("choose wgpu as the Renderer"), "{line}");
            assert!(!line.starts_with("Restart to apply"), "{line}");
        }
        assert_eq!(
            default_device.status(false).as_deref(),
            Some("Off on this renderer: choose wgpu as the Renderer above, then restart")
        );
        // What the renderer choice leaves wgpu at, ticked or not; chosen for a plain restart, a
        // tick is applied by it.
        for (next, line) in [
            (
                WgpuNext::WithoutCommandLine,
                "Off until a restart onto wgpu without --renderer",
            ),
            (WgpuNext::DidNotStart, "Off: wgpu did not start here"),
        ] {
            let chosen = Availability {
                wgpu_next: Some(next),
                ..default_device.clone()
            };
            for ticked in [false, true] {
                assert_eq!(chosen.status(ticked).as_deref(), Some(line), "{next:?}");
            }
        }
        let chosen = Availability {
            wgpu_next: Some(WgpuNext::AtRestart),
            ..default_device.clone()
        };
        assert_eq!(
            chosen.status(false).as_deref(),
            Some("Off until the restart onto wgpu")
        );
        assert_eq!(
            chosen.status(true).as_deref(),
            Some("Restart onto wgpu to apply")
        );
        // wgpu, started without a box ticked or under another interface: a tick takes effect at
        // the next start, and only a tick is said to need it.
        let started_elsewhere = Availability {
            widened: false,
            ..ready.clone()
        };
        assert!(started_elsewhere.offered());
        assert_eq!(started_elsewhere.status(false), None);
        assert_eq!(
            started_elsewhere.status(true).as_deref(),
            Some("Restart to apply")
        );
        let short = Availability {
            supported: false,
            ..ready.clone()
        };
        assert_eq!(
            short.status(true).as_deref(),
            Some("This graphics backend can't run these effects")
        );
        let failed = Availability {
            failed: Some("the lighting pass's shader did not build".to_owned()),
            ..ready
        };
        assert_eq!(
            failed.status(true).as_deref(),
            Some(
                "Stopped after an error (the lighting pass's shader did not build): untick and \
                 tick a box to retry"
            )
        );
        let long = Availability {
            failed: Some("x".repeat(200)),
            ..failed.clone()
        };
        assert!(
            long.status(true).unwrap().len() < 120,
            "a long reason is cut short"
        );
        assert_eq!(
            failed.status(false),
            None,
            "nothing ticked, nothing stopped"
        );
        assert_eq!(Availability::default().status(true), None, "not built");
        assert!(!Availability::default().offered());
    }
}
