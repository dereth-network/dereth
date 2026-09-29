//! Immutable sampler banks: changing preference or entering a preview never rewrites a
//! sampler referenced by an already-recorded or in-flight draw.
use super::*;
use crate::sampler::{self, FilterCaps};

use crate::sampler::{bank, BANK_KEYS};
pub use crate::sampler::{AddressMode, SamplerDescription, SamplerFilter};
pub(super) use crate::sampler::{BANK_STRIDE, DESCRIPTOR_COUNT};

impl Gpu {
    pub(super) fn create_samplers(&mut self) -> Result<(), RenderError> {
        // Retail's default device-state setup loads the caps' MaxAnisotropy, not a chosen value.
        // The device's own limit is that number here; a device without the feature at all
        // resolves every anisotropic request back to linear through `sampler::resolve`.
        let caps = FilterCaps {
            min_anisotropic: self.anisotropy_supported,
            mag_anisotropic: self.anisotropy_supported,
            max_anisotropy: self.max_anisotropy,
        };
        for (preference, sharp_bias) in BANK_KEYS {
            for which in 0..BANK_STRIDE {
                let desc = sampler::describe(which, preference, sharp_bias, caps);
                let filter = desc.filter;
                let (mag, min, mip, aniso) = match filter {
                    SamplerFilter::Point => (
                        vk::Filter::NEAREST,
                        vk::Filter::NEAREST,
                        vk::SamplerMipmapMode::NEAREST,
                        false,
                    ),
                    SamplerFilter::LinearMipPoint => (
                        vk::Filter::LINEAR,
                        vk::Filter::LINEAR,
                        vk::SamplerMipmapMode::NEAREST,
                        false,
                    ),
                    SamplerFilter::Linear => (
                        vk::Filter::LINEAR,
                        vk::Filter::LINEAR,
                        vk::SamplerMipmapMode::LINEAR,
                        false,
                    ),
                    SamplerFilter::Anisotropic => (
                        vk::Filter::LINEAR,
                        vk::Filter::LINEAR,
                        vk::SamplerMipmapMode::LINEAR,
                        true,
                    ),
                };
                let address = |m: AddressMode| match m {
                    AddressMode::Wrap => vk::SamplerAddressMode::REPEAT,
                    AddressMode::Clamp => vk::SamplerAddressMode::CLAMP_TO_EDGE,
                };
                #[allow(clippy::cast_precision_loss)] // a small integer limit
                let info = vk::SamplerCreateInfo::default()
                    .mag_filter(mag)
                    .min_filter(min)
                    .mipmap_mode(mip)
                    .address_mode_u(address(desc.address_u))
                    .address_mode_v(address(desc.address_v))
                    .address_mode_w(vk::SamplerAddressMode::CLAMP_TO_EDGE)
                    // Where the pixel shaders apply the bias, the sampler carries none; the
                    // description keeps the bias the draw samples with either way.
                    .mip_lod_bias(if self.shader_lod_bias {
                        0.0
                    } else {
                        desc.mip_lod_bias
                    })
                    .anisotropy_enable(aniso)
                    .max_anisotropy(if aniso {
                        desc.max_anisotropy as f32
                    } else {
                        1.0
                    })
                    .compare_enable(false)
                    .min_lod(0.0)
                    .max_lod(vk::LOD_CLAMP_NONE)
                    .border_color(vk::BorderColor::FLOAT_TRANSPARENT_BLACK)
                    .unnormalized_coordinates(false);
                // SAFETY: `info` is a live local; the returned sampler is owned by `self` and
                // destroyed in `Drop`.
                let sampler = vkr("vkCreateSampler", unsafe {
                    self.device.create_sampler(&info, None)
                })?;
                self.samplers.push(sampler);
                self.sampler_descriptions.push(desc);
            }
        }
        debug_assert_eq!(self.sampler_descriptions.len(), DESCRIPTOR_COUNT as usize);
        Ok(())
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

    /// Description of the sampler of the most recent actual binding, as the bank made it; no
    /// recomputation from the requested preference, so tests can distinguish bank selection from
    /// owner state. Its bias is the one the draw samples with, whether the sampler or the pixel
    /// shader applies it.
    #[must_use]
    pub fn bound_sampler_description(&self) -> Option<SamplerDescription> {
        self.bound_sampler
            .get()
            .map(|i| self.sampler_descriptions[i as usize])
    }

    /// The descriptor set that puts sampler `s0` in stage 0 and `s1` in stage 1, made on first
    /// use and kept for the life of the device. Samplers are immutable, so a pair set never needs
    /// rewriting and can be bound by any number of in-flight frames at once.
    pub(super) fn sampler_pair_set(
        &self,
        s0: u32,
        s1: u32,
    ) -> Result<vk::DescriptorSet, RenderError> {
        if let Some(set) = self.sampler_pair_sets.borrow().get(&(s0, s1)) {
            return Ok(*set);
        }
        let layouts = [self.sampler_set_layout];
        let alloc = vk::DescriptorSetAllocateInfo::default()
            .descriptor_pool(self.sampler_pool)
            .set_layouts(&layouts);
        // SAFETY: the pool and layout are owned by `self`; the pool is sized for every pair the
        // bank can produce (`DESCRIPTOR_COUNT` squared).
        let set = vkr("vkAllocateDescriptorSets(samplers)", unsafe {
            self.device.allocate_descriptor_sets(&alloc)
        })?[0];
        let infos = [
            vk::DescriptorImageInfo {
                sampler: self.samplers[s0 as usize],
                ..Default::default()
            },
            vk::DescriptorImageInfo {
                sampler: self.samplers[s1 as usize],
                ..Default::default()
            },
        ];
        let writes = [
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(0)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&infos[0..1]),
            vk::WriteDescriptorSet::default()
                .dst_set(set)
                .dst_binding(1)
                .descriptor_type(vk::DescriptorType::SAMPLER)
                .image_info(&infos[1..2]),
        ];
        // SAFETY: a freshly allocated set that nothing has bound yet; the samplers are live.
        unsafe { self.device.update_descriptor_sets(&writes, &[]) };
        self.sampler_pair_sets.borrow_mut().insert((s0, s1), set);
        Ok(set)
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
