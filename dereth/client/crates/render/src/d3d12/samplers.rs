//! Immutable descriptor banks: changing preference or entering a preview never rewrites a
//! descriptor referenced by an already-recorded or in-flight draw.
use super::*;
use crate::sampler::{self, FilterCaps};

use crate::sampler::{bank, AddressMode, SamplerFilter, BANK_KEYS};
pub(super) use crate::sampler::{BANK_STRIDE, DESCRIPTOR_COUNT};

impl Gpu {
    pub(super) fn create_samplers(&mut self) {
        // Retail's default device-state setup loads the caps' MaxAnisotropy, not a chosen value.
        // create_device requires FL11_0 for hardware AND WARP. D3D12's required anisotropy
        // maximum is the API contract for these devices, not a fabricated per-adapter query.
        let caps = FilterCaps {
            min_anisotropic: true,
            mag_anisotropic: true,
            max_anisotropy: D3D12_REQ_MAXANISOTROPY,
        };
        for (preference, sharp_bias) in BANK_KEYS {
            for which in 0..BANK_STRIDE {
                let resolved = sampler::describe(which, preference, sharp_bias, caps);
                let filter = match resolved.filter {
                    SamplerFilter::Point => D3D12_FILTER_MIN_MAG_MIP_POINT,
                    SamplerFilter::LinearMipPoint => D3D12_FILTER_MIN_MAG_LINEAR_MIP_POINT,
                    SamplerFilter::Linear => D3D12_FILTER_MIN_MAG_MIP_LINEAR,
                    SamplerFilter::Anisotropic => D3D12_FILTER_ANISOTROPIC,
                };
                let address = |mode| match mode {
                    AddressMode::Wrap => D3D12_TEXTURE_ADDRESS_MODE_WRAP,
                    AddressMode::Clamp => D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                };
                let desc = D3D12_SAMPLER_DESC {
                    Filter: filter,
                    AddressU: address(resolved.address_u),
                    AddressV: address(resolved.address_v),
                    AddressW: D3D12_TEXTURE_ADDRESS_MODE_CLAMP,
                    MipLODBias: resolved.mip_lod_bias,
                    MaxAnisotropy: resolved.max_anisotropy,
                    ComparisonFunc: D3D12_COMPARISON_FUNC_NEVER,
                    BorderColor: [0.0; 4],
                    MinLOD: 0.0,
                    MaxLOD: f32::MAX,
                };
                // SAFETY: this writes each descriptor exactly once during device construction;
                // heap size is DESCRIPTOR_COUNT and desc is retained for diagnostic readback.
                unsafe {
                    let mut handle = self.sampler_heap.GetCPUDescriptorHandleForHeapStart();
                    handle.ptr += self.sampler_descriptions.len() * self.sampler_size as usize;
                    self.device.CreateSampler(&desc, handle);
                }
                self.sampler_descriptions.push(desc);
            }
        }
        debug_assert_eq!(self.sampler_descriptions.len(), DESCRIPTOR_COUNT as usize);
    }

    /// The live UInt32 `Render.TextureFiltering`. Unknown values retain retail's branch behavior.
    pub fn set_texture_filtering(&mut self, preference: u32) {
        self.texture_filtering = preference;
    }

    #[must_use]
    pub const fn texture_filtering(&self) -> u32 {
        self.texture_filtering
    }

    /// Only the sharp-enabled, filtering-below-2 bracket changes bias. It restores
    /// even when the draw closure returns an error, before the caller's next world/UI draw.
    /// The normal baseline is zero; the separate debug registry command
    /// RenderD3D.MipmapLODBias is not exposed by the current host.
    pub fn with_preview_sharp<T>(&mut self, enabled: bool, draw: impl FnOnce(&mut Self) -> T) -> T {
        let changed = self.begin_preview_sharp(enabled);
        let result = draw(self);
        if changed {
            self.end_preview_sharp();
        }
        result
    }

    /// The opening half of [`Self::with_preview_sharp`], for a caller that cannot hand this type
    /// to a closure -- `dereth_render::device::Gpu`, whose closure takes the enum.
    /// Returns whether the bias actually changed, which is what the caller must pair with
    /// [`Self::end_preview_sharp`].
    pub fn begin_preview_sharp(&mut self, enabled: bool) -> bool {
        let changed = sampler::preview_sharp_enabled(self.texture_filtering, enabled);
        if changed {
            self.sharp_lod_bias = true;
        }
        changed
    }

    /// The closing half. Retail restores the renderer's GLOBAL baseline, not an arbitrary
    /// previous bias.
    pub fn end_preview_sharp(&mut self) {
        self.sharp_lod_bias = false;
    }

    pub(super) fn sampler_descriptor_index(&self, which: u32) -> u32 {
        bank(self.texture_filtering, self.sharp_lod_bias) * BANK_STRIDE + which
    }

    /// Description supplied to CreateSampler for the most recent actual binding; no recomputation
    /// from the requested preference, so tests can distinguish bank selection from owner state.
    #[must_use]
    pub fn bound_sampler_description(&self) -> Option<D3D12_SAMPLER_DESC> {
        self.bound_sampler
            .get()
            .map(|i| self.sampler_descriptions[i as usize])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn effective_bank_keys_are_complete_and_the_two_stage_tail_stays_in_its_bank() {
        for (i, (pref, sharp)) in BANK_KEYS.into_iter().enumerate() {
            assert_eq!(bank(pref, sharp), i as u32);
            assert!((i as u32 * BANK_STRIDE + SAMPLER_COUNT) < (i as u32 + 1) * BANK_STRIDE);
        }
        assert_eq!(bank(1, true), bank(2, true));
        assert_ne!(bank(3, true), bank(3, false));
        assert_eq!(bank(u32::MAX, true), bank(1, true));
    }
}
