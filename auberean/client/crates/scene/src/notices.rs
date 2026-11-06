//! What a body asks of its own figure and its game object after its ticks.
//!
//! A request for its animation (an effect script, a layer stopped, the behaviors a move cancels
//! or the server asked for stopped, a mode, an element, a slider) is played on its figure. What it tells its game object (its
//! missile failed or ran out, it entered or left the world, fade it out, hold it where it struck)
//! goes to the server's game objects, which this client does not hold: it is only noted.

use auberean_animation::body::Body as Figure;
use auberean_animation::CachedRecords;
use auberean_physics::body::{Animate, Notice};

/// How many rounds of notices one frame hands on: what a notice makes a body ask in turn is
/// handed on in the same frame, this many times at most.
pub(crate) const ROUNDS: usize = 4;

/// Plays an animation request on `figure`.
pub(crate) fn animate(figure: &mut Figure, anims: &CachedRecords<'_>, a: Animate) {
    use auberean_animation::queries::TemplateQueries;
    match a {
        Animate::FxScript(script) => {
            figure.do_fx_script(anims, script);
        }
        Animate::EndLayer(layer) => {
            figure.stop_layer(anims, layer);
        }
        Animate::StopMoveToCancels => figure.stop_move_to_cancels(anims),
        Animate::StopServerBehaviors => {
            figure.stop_all_server_behaviors(anims);
        }
        Animate::Slider {
            context,
            slider,
            value,
            time,
        } => {
            figure.sliders.do_slider(context, slider, value, time);
        }
        Animate::DefaultMode => {
            let mode = figure.template.as_ref().map_or(0, |t| t.default_mode());
            figure.do_mode(anims, 0, mode);
        }
        Animate::Mode { context, mode } => {
            figure.do_mode(anims, context, mode);
        }
        Animate::Element(element) => {
            figure.do_element(anims, element);
        }
        Animate::PlacementFrame(kind) => {
            figure.set_placement_frame(kind);
        }
        // The figure draws no effect of its own here.
        Animate::Fx { .. } => {
            tracing::trace!(?a, "an animation request the figure does not take");
        }
    }
}

/// How long an object takes to fade in as it enters the world, seconds.
const ENTERING_FADE: f32 = 0.5;

/// Hands on notice `n` of body `from`, which is for the body itself: an animation request to its
/// figure (when it has one), anything else noted.
pub(crate) fn hand_on(
    from: u64,
    n: Notice,
    figure: Option<&mut Figure>,
    anims: &CachedRecords<'_>,
) {
    match (n, figure) {
        (Notice::Animate(a), Some(f)) => animate(f, anims, a),
        // D2-06375
        (Notice::EnteredWorld, Some(f)) => {
            f.sliders.do_slider(
                0,
                auberean_physics::body::parent::FADE_SLIDER,
                1.0,
                ENTERING_FADE,
            );
        }
        (n, _) => tracing::debug!(
            body = format_args!("{from:#x}"),
            notice = ?n,
            "a body's notice to its game object"
        ),
    }
}
