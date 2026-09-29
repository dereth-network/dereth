//! Combat rules the client and the server share.

/// A caster's elemental damage bonus against **players**: `(m - 1) * 0.25 + 1`.
///
/// The PK factor is the `double` **0.25**, the client's own constant rather than one guessed from
/// the name. The client prints it as the examine panel's "vs. Players" line; the server applies it
/// to a caster's elemental bonus against a player (server divergence V381), so the two agree.
#[must_use]
pub fn elemental_mod_pk_modifier(m: f64) -> f64 {
    (m - 1.0) * 0.25 + 1.0
}
