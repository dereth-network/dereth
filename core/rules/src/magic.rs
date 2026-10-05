//! The spell-formula rules the client and the server share: power levels, the scarab-only (foci)
//! formula and its taper count. **One implementation**; `dereth-client-model` re-exports it and the server's
//! `SpellFormula.GetFociFormula` computes through it (server divergence V380).

/// Maps a scarab id to its power level.
///
/// Note that `0x6F` (111) is retained by the scarab-only formula but maps to power level **0** here.
#[must_use]
pub fn scarab_power_level(component_id: u32) -> u32 {
    match component_id {
        1..=6 => component_id,
        0x6E => 7,
        0x70 => 8,
        0xC0 => 9,
        0xC1 => 10,
        _ => 0,
    }
}

/// Counts the non-zero slots over **all
/// eight**, not the length of the leading run.
///
/// The distinction matters because casting checks slots
/// `0..n` where `n` is this count: on a formula with a hole the client reads a zero slot and asks
/// the component tracker about the WCID `scid_to_wcid(0)` returns, which it never owns.
#[must_use]
pub fn num_spell_components(comps: &[u32; 8]) -> usize {
    comps.iter().filter(|c| **c != 0).count()
}

/// Returns `(component id, power level)` for the strongest power component.
///
/// The client returns the id and writes the level through an out parameter; both are returned
/// here because [`scarab_only_formula`] switches on the **level** and nothing reads the id.
///
/// **The client's own loop bound is reproduced**: it counts the non-zero slots and then walks that
/// many slots **from index 0**, so a formula with a hole never examines its tail. That is
/// unobservable at its one call site, where the formula it is handed was just packed from index 0
/// with no gaps, and it is kept because deviating would be a silent change of a shipped bound.
#[must_use]
pub fn find_most_powerful_power_component(comps: &[u32; 8]) -> (u32, u32) {
    let n = num_spell_components(comps);
    let (mut best_id, mut best_level) = (0u32, 0u32);
    for c in comps.iter().take(n) {
        let level = scarab_power_level(*c);
        if *c != 0 && best_level < level {
            best_id = *c;
            best_level = level;
        }
    }
    (best_id, best_level)
}

/// The scarab-only formula pads with SCID **188** (`0xBC`).
///
/// **It is the *Prismatic Taper*.** The name comes from row 188 of the shipped
/// spell-component table:
/// `name "Prismatic Taper", category 5, component_type 6` (the taper type), WCID `20631` through
/// `DualDidMapper 0x27000002`. So "scarab + N prismatic tapers" is not a later-era rule: it is
/// what this 2013 client computes here, and the world's spell-formula resolver hands it to both
/// consumers.
///
/// The reference server uses the same `188` constant and describes the number of prismatic tapers
/// as depending on spell power.
pub const SCARAB_ONLY_FILLER_SCID: u32 = 0xBC;

/// `InqScarabOnlyFormula`'s power level -> filler count table, read arm by arm.
///
/// `switch (level) { 1: 1; 2: 2; 3,4,7: 3; 5,6,8,9,10: 4; default: 0 }`. The 7 sitting with 3 and
/// 4 rather than with 5 and 6 is the client's and is not a transcription slip.
#[must_use]
pub const fn scarab_only_filler_count(power_level: u32) -> u32 {
    match power_level {
        1 => 1,
        2 => 2,
        3 | 4 | 7 => 3,
        5 | 6 | 8 | 9 | 10 => 4,
        _ => 0,
    }
}

/// Builds the formula used when an Infused-Magic augmentation or school spell pack reduces the
/// component requirement.
///
/// Three steps, and the first is the one a "scarab only" name hides: it keeps **every power
/// component** of the decrypted formula, packed from slot 0, dropping everything else; then it
/// finds the most powerful of them and appends [`scarab_only_filler_count`] copies of
/// [`SCARAB_ONLY_FILLER_SCID`].
///
/// The keep-list is the client's literal `switch`: `1..=6`, `0x6E`, **`0x6F`**, `0x70`, `0xC0`,
/// `0xC1`. `0x6F` is in the keep-list and is **not** in [`scarab_power_level`]'s table, so a
/// formula whose only power component is `0x6F` keeps it and appends **zero** fillers.
///
/// It reads the decrypted dat formula rather than the per-account customized formula, so it is
/// independent of taper randomisation and fully computable from the dat alone.
#[must_use]
pub fn scarab_only_formula(comps: &[u32; 8]) -> [u32; 8] {
    let mut out = [0u32; 8];
    let mut w = 0usize;
    for c in comps {
        // The client's loop breaks on the first zero slot of the source formula.
        if *c == 0 {
            break;
        }
        if matches!(*c, 1..=6 | 0x6E | 0x6F | 0x70 | 0xC0 | 0xC1) {
            if w < out.len() {
                out[w] = *c;
            }
            w += 1;
        }
    }
    let (_, level) = find_most_powerful_power_component(&out);
    for _ in 0..scarab_only_filler_count(level) {
        if w < out.len() {
            out[w] = SCARAB_ONLY_FILLER_SCID;
        }
        w += 1;
    }
    out
}

/// Computes the 1–8 spell level the UI shows.
#[must_use]
pub fn spell_level_by_rough_heuristic(power: u32) -> u32 {
    if power <= 6 {
        power
    } else if power <= 8 {
        power - 1
    } else {
        power - 2
    }
}
