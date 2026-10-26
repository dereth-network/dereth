//! The client's widget kit: an immediate-mode interface toolkit, drawn in a skin through the
//! shared overlay every interface ends in.
//!
//! Every frame an interface draws its widgets ([`widgets`]) into a draw list ([`draw::DrawList`])
//! with a painter ([`paint::Painter`]) over a skin's art ([`art::Art`]): its own pieces and fonts
//! where it has them ([`pieces`], [`font`]), plain fills where it has not. The widgets answer
//! this frame's input ([`input::InputFrame`]), edit text ([`edit`]) and note themselves for a
//! pad's focus ([`nav`], [`pad`]) as they draw; the list becomes the overlay's batches
//! ([`draw::Overlay`]) through the interface's own texture sink, every quad on whole pixels and a
//! picture drawn at another size than its own drawn from a copy made at that size ([`resample`]).
//!
//! **Depends on** `dereth-primitives` and `dereth-client-contract` (the overlay's types). **Used
//! by** the Horizon interface (`dereth-horizon`), which skins it with its own art, and the plugin
//! windows (`dereth-plugin-ui`), drawn in the skin of the interface they are drawn under.
//!
//! **Must never** reach the runtime, the scene or the platform (`cargo xtask seams`, `seam: client
//! crates`): pictures leave through [`draw::TextureSink`] and the game's icons arrive through
//! [`art::ArtSource`], each implemented by the interface that has them.

pub mod art;
pub mod colours;
pub mod draw;
pub mod edit;
pub mod font;
pub mod input;
pub mod nav;
pub mod pad;
pub mod paint;
pub mod palette;
pub mod pieces;
pub mod resample;
pub mod widgets;

/// A time or a small fraction of one, narrowed for drawing arithmetic.
#[must_use]
pub fn narrow(v: f64) -> f32 {
    // A drawing fraction or a few seconds; never beyond f32's range.
    #[allow(clippy::cast_possible_truncation)]
    let f = v as f32;
    f
}

/// A sine wave of `time` at `rate` radians a second, from `0` to `1`.
#[must_use]
pub fn wave(time: f64, rate: f64) -> f32 {
    narrow(dereth_primitives::num::math::sin(time * rate) * 0.5 + 0.5)
}
