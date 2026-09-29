//! The one named flag type the client's state structs share.
//!
//! It lives here because `character.rs` names it for its own `enabled` field.
//! `dereth_client::interaction::StartsTrue` is a `pub use` of this type.

/// A `bool` whose zero value is `true`.
///
/// Two UI state flags are initialized to `true` at construction
/// (radar visibility) or answer `true` when the thing they describe is absent
/// (the standing-still query with no body). `Interaction` derives
/// `Default`, so a derived false value would leave no producer of the required true state:
/// a `bool` field whose un-set value is the common
/// case fails silently. Naming the type makes the non-zero default visible at the field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartsTrue(pub bool);

impl Default for StartsTrue {
    fn default() -> Self {
        Self(true)
    }
}
