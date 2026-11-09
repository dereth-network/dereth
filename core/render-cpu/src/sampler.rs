//! Retail request-level sampler policy, separate from texture ownership and mip generation.
//!
//! The retail sampler filter setup: requested linear
//! min/mag promote only for preference 3, with independent capability bits `0x400`/`0x4000000`;
//! requested linear mip becomes point only for preference0. Explicit point stays point.

/// Startup selects overall graphics quality 3 before loading preferences (and not
/// the options panel's Restore Defaults value 1). Defined in
/// [`dereth_client_contract::sampler`], because `dereth_client_runtime::render_prefs` is its other reader.
pub use dereth_client_contract::sampler::STARTUP_FILTERING;
/// Sharp-preview LOD bias used at scene begin and during the creature-preview pass; literal
/// `0xBFB33333`.
pub const SHARP_LOD_BIAS: f32 = -1.4;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Filter {
    Point,
    Linear,
    Anisotropic,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FilterCaps {
    pub min_anisotropic: bool,
    pub mag_anisotropic: bool,
    pub max_anisotropy: u32,
}

/// Exact requested-to-effective mode rewrite, including explicit anisotropic fallback.
#[must_use]
pub fn resolve(request: [Filter; 3], preference: u32, caps: FilterCaps) -> [Filter; 3] {
    let axis = |f: Filter, supported: bool| {
        let f = if f == Filter::Linear && preference == 3 {
            Filter::Anisotropic
        } else {
            f
        };
        if f == Filter::Anisotropic && !supported {
            Filter::Linear
        } else {
            f
        }
    };
    [
        axis(request[0], caps.min_anisotropic),
        axis(request[1], caps.mag_anisotropic),
        if request[2] == Filter::Linear && preference == 0 {
            Filter::Point
        } else {
            request[2]
        },
    ]
}

#[must_use]
pub fn preview_sharp_enabled(preference: u32, requested: bool) -> bool {
    requested && preference < 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn actual_mode_rewrite_preserves_points_and_independent_caps() {
        use Filter::{Anisotropic as A, Linear as L, Point as P};
        for min in [false, true] {
            for mag in [false, true] {
                let caps = FilterCaps {
                    min_anisotropic: min,
                    mag_anisotropic: mag,
                    max_anisotropy: 8,
                };
                assert_eq!(resolve([L, L, L], 0, caps), [L, L, P]);
                for pref in [1, 2, 4, u32::MAX] {
                    assert_eq!(resolve([L, L, L], pref, caps), [L, L, L]);
                }
                assert_eq!(
                    resolve([L, L, L], 3, caps),
                    [if min { A } else { L }, if mag { A } else { L }, L]
                );
                for pref in [0, 1, 2, 3, u32::MAX] {
                    assert_eq!(resolve([P, P, P], pref, caps), [P, P, P]);
                    assert_eq!(
                        resolve([A, A, P], pref, caps),
                        [if min { A } else { L }, if mag { A } else { L }, P]
                    );
                    assert_eq!(
                        resolve([P, L, P], pref, caps),
                        [P, if pref == 3 && mag { A } else { L }, P]
                    );
                }
            }
        }
    }

    #[test]
    fn sharp_is_a_bias_not_an_alternate_filtering_preference() {
        assert_eq!(STARTUP_FILTERING, 0);
        assert_eq!(SHARP_LOD_BIAS.to_bits(), 0xBFB3_3333);
        for pref in [0, 1, 2, 3, 4, u32::MAX] {
            assert_eq!(preview_sharp_enabled(pref, true), pref < 2);
            assert!(!preview_sharp_enabled(pref, false));
        }
    }
}

/// Number of legacy sampler requests before the guard descriptor.
pub const SAMPLER_COUNT: u32 = 8;
// Eight legacy request indices plus a guard for the existing two-sampler tail.
// Stage1 still inherits the NEXT legacy request, not an independent texture stage. That existing
// limitation is deliberately preserved; index7 must not read bank(n+1)'s descriptor0.
pub const BANK_STRIDE: u32 = SAMPLER_COUNT + 1;
pub const BANK_KEYS: [(u32, bool); 6] = [
    (1, false),
    (0, false),
    (1, true),
    (0, true),
    (3, false),
    (3, true),
];
#[allow(clippy::cast_possible_truncation)] // the fixed array has six entries
pub const DESCRIPTOR_COUNT: u32 = BANK_STRIDE * BANK_KEYS.len() as u32;

pub fn bank(preference: u32, sharp_bias: bool) -> u32 {
    match (preference, sharp_bias) {
        (0, false) => 1,
        (0, true) => 3,
        (3, false) => 4,
        (3, true) => 5,
        (_, true) => 2,
        (_, false) => 0,
    }
}

/// The filter a bank entry resolved to, in the vocabulary of `D3DTEXTUREFILTERTYPE` triples the
/// D3D12 build named its descriptors with. A portable description so a test can read a sampler
/// back without a Vulkan handle telling it nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SamplerFilter {
    /// `MIN_MAG_MIP_POINT`.
    Point,
    /// `MIN_MAG_LINEAR_MIP_POINT` -- bilinear, nearest mip.
    LinearMipPoint,
    /// `MIN_MAG_MIP_LINEAR` -- trilinear.
    Linear,
    /// `ANISOTROPIC`.
    Anisotropic,
}

/// `D3DTEXTUREADDRESS`, the two the client uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AddressMode {
    Wrap,
    Clamp,
}

/// What one sampler in the bank was created with.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SamplerDescription {
    pub filter: SamplerFilter,
    pub address_u: AddressMode,
    pub address_v: AddressMode,
    pub mip_lod_bias: f32,
    pub max_anisotropy: u32,
}

/// Resolve one immutable sampler-bank entry before mapping it to a device API.
#[must_use]
pub fn describe(
    which: u32,
    preference: u32,
    sharp_bias: bool,
    caps: FilterCaps,
) -> SamplerDescription {
    let which = which.min(SAMPLER_COUNT - 1);
    let point = matches!(which, 2 | 3 | 6 | 7);
    let request = if point { Filter::Point } else { Filter::Linear };
    let effective = resolve([request; 3], preference, caps);
    let filter = match effective {
        [Filter::Point, Filter::Point, Filter::Point] => SamplerFilter::Point,
        [Filter::Linear, Filter::Linear, Filter::Point] => SamplerFilter::LinearMipPoint,
        [Filter::Anisotropic, Filter::Anisotropic, Filter::Linear] => SamplerFilter::Anisotropic,
        _ => SamplerFilter::Linear,
    };
    SamplerDescription {
        filter,
        address_u: if matches!(which, 1 | 3 | 5 | 7) {
            AddressMode::Clamp
        } else {
            AddressMode::Wrap
        },
        address_v: if matches!(which, 1 | 3 | 4 | 6) {
            AddressMode::Clamp
        } else {
            AddressMode::Wrap
        },
        mip_lod_bias: if sharp_bias { SHARP_LOD_BIAS } else { 0.0 },
        max_anisotropy: caps.max_anisotropy,
    }
}

#[cfg(test)]
mod bank_tests {
    //! Behaviour: none (sampler descriptions preserve the fixed-function request table).
    use super::*;

    #[test]
    fn bank_keys_cover_every_preference_and_guard_entries_repeat_the_last_request() {
        let caps = FilterCaps {
            min_anisotropic: true,
            mag_anisotropic: true,
            max_anisotropy: 16,
        };
        for (index, (preference, sharp)) in BANK_KEYS.into_iter().enumerate() {
            assert_eq!(usize::try_from(bank(preference, sharp)).unwrap(), index);
            assert_eq!(
                describe(8, preference, sharp, caps),
                describe(7, preference, sharp, caps)
            );
        }
        assert_eq!(bank(2, true), bank(1, true));
        assert_eq!(bank(u32::MAX, false), bank(1, false));
        for preference in [0, 1, 2, 3, 4, u32::MAX] {
            for sharp in [false, true] {
                for which in 0..8 {
                    let d = describe(which, preference, sharp, caps);
                    let expected_filter = if [2, 3, 6, 7].contains(&which) {
                        SamplerFilter::Point
                    } else if preference == 0 {
                        SamplerFilter::LinearMipPoint
                    } else if preference == 3 {
                        SamplerFilter::Anisotropic
                    } else {
                        SamplerFilter::Linear
                    };
                    assert_eq!(d.filter, expected_filter);
                    assert_eq!(
                        d.address_u,
                        if [1, 3, 5, 7].contains(&which) {
                            AddressMode::Clamp
                        } else {
                            AddressMode::Wrap
                        }
                    );
                    assert_eq!(
                        d.address_v,
                        if [1, 3, 4, 6].contains(&which) {
                            AddressMode::Clamp
                        } else {
                            AddressMode::Wrap
                        }
                    );
                    assert_eq!(d.mip_lod_bias, if sharp { -1.4 } else { 0.0 });
                    assert_eq!(d.max_anisotropy, 16);
                }
            }
        }
    }
}
