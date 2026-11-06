//! Effects: particle systems, streaks, decals and portal space.

mod decal;
mod particles;
mod portal;
mod rain;
mod streak;

/// An entry that moves a slider and stores no intensity moves it to full, not to nothing.
///
/// Behaviour: draw.effects.a-slider-with-no-intensity-goes-to-full
#[test]
fn a_slider_entry_with_no_intensity_goes_to_full() {
    use auberean_assets::fx::FxNode;
    use auberean_draw::effects::node_sliders;
    let asked = |intensity| {
        node_sliders(&FxNode {
            slider: Some(0x4000_001A),
            intensity,
            ..FxNode::default()
        })[0]
            .value
    };
    assert!((asked(None) - 1.0).abs() < f32::EPSILON);
    assert!((asked(Some(0.25)) - 0.25).abs() < f32::EPSILON);
}
