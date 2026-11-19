//! The interface's textured batches as the composer draws them: their vertices in the overlay's
//! buffer, and runs of them that sample one texture one way and combine with the frame one way.

use auberean_shell::interface::{OverlayItem, OverlayMaterial, OverlaySampler, OverlayTexture};

use crate::QuadVertex;

/// Batches in a row that sample one texture in one way and combine in one way: drawn with one
/// binding, `count` vertices from `start`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct ItemRun {
    pub texture: OverlayTexture,
    pub material: OverlayMaterial,
    pub sampler: OverlaySampler,
    pub start: u32,
    pub count: u32,
}

/// A color packed `0xAARRGGBB`, as red, green, blue and alpha from 0 to 1.
pub(crate) fn unpack(color: u32) -> [f32; 4] {
    let [a, r, g, b] = color.to_be_bytes();
    [r, g, b, a].map(|c| f32::from(c) / 255.0)
}

/// The batches of `items`, in order, appended to `verts`: each batch's whole triangles, and a run
/// for each stretch of batches that share a texture, its sampling and its material. A preview
/// space adds nothing (the interface's viewports are its scenes), and a batch hidden behind the
/// world is drawn over it, since the composed frame keeps no depth of the world's.
pub(crate) fn lower(items: &[OverlayItem], verts: &mut Vec<QuadVertex>) -> Vec<ItemRun> {
    let mut runs: Vec<ItemRun> = Vec::new();
    for item in items {
        let OverlayItem::Triangles {
            material,
            texture,
            sampler,
            vertices,
            ..
        } = item
        else {
            continue;
        };
        let whole = vertices.len() - vertices.len() % 3;
        if whole == 0 {
            continue;
        }
        let start = u32::try_from(verts.len()).unwrap_or(u32::MAX);
        verts.extend(vertices[..whole].iter().map(|v| QuadVertex {
            pos: [v.position[0], v.position[1]],
            uv: v.uv,
            color: unpack(v.color),
        }));
        let count = u32::try_from(whole).unwrap_or(u32::MAX);
        match runs.last_mut() {
            Some(last)
                if last.texture == *texture
                    && last.material == *material
                    && last.sampler == *sampler
                    && last.start + last.count == start =>
            {
                last.count += count;
            }
            _ => runs.push(ItemRun {
                texture: *texture,
                material: *material,
                sampler: *sampler,
                start,
                count,
            }),
        }
    }
    runs
}

/// The batches drawn under the viewports' scenes and those drawn over them: an interface that
/// stands a viewport among its own batches (a window holding the player's figure) marks the place
/// with a preview space, and the batches before the first one are drawn under the scenes, the rest
/// over the picture in front. With no preview space every batch is drawn over the scenes.
pub(crate) fn split_at_scenes(items: &[OverlayItem]) -> (&[OverlayItem], &[OverlayItem]) {
    let at = items
        .iter()
        .position(|i| matches!(i, OverlayItem::Preview { .. }))
        .unwrap_or(0);
    items.split_at(at)
}

/// The device's sampler for an overlay batch's sampling.
pub(crate) fn sampler_descriptor(s: OverlaySampler) -> wgpu::SamplerDescriptor<'static> {
    let filter = if s.point {
        wgpu::FilterMode::Nearest
    } else {
        wgpu::FilterMode::Linear
    };
    let address = |wrap: bool| {
        if wrap {
            wgpu::AddressMode::Repeat
        } else {
            wgpu::AddressMode::ClampToEdge
        }
    };
    wgpu::SamplerDescriptor {
        label: Some("overlay batch"),
        address_mode_u: address(s.wrap_u),
        address_mode_v: address(s.wrap_v),
        mag_filter: filter,
        min_filter: filter,
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (how the overlay's batches reach the device, which no game behavior names)

    use super::*;
    use auberean_shell::interface::{Overlay, OverlayDepth, OverlaySpace, OverlayVertex};
    use auberean_shell::interface::{TextureData, TextureFormat};
    use bytemuck::Zeroable;

    fn key(space: OverlaySpace, key: u64) -> OverlayTexture {
        OverlayTexture { space, key }
    }

    fn triangles(
        texture: OverlayTexture,
        material: OverlayMaterial,
        vertices: usize,
    ) -> OverlayItem {
        OverlayItem::Triangles {
            material,
            texture,
            sampler: OverlaySampler::POINT_CLAMP,
            depth: OverlayDepth::Over,
            vertices: (0..vertices)
                .map(|i| OverlayVertex {
                    #[allow(clippy::cast_precision_loss)]
                    position: [i as f32 * 0.1, -0.5, 0.0],
                    color: 0x80FF_4000,
                    uv: [0.25, 0.75],
                })
                .collect(),
        }
    }

    /// A batch an interface draws over a texture it uploaded reaches the composer: the texture
    /// is among the overlay's, and the batch becomes its triangles in clip space with their
    /// colors unpacked, in one run over that texture, after what the buffer already holds.
    #[test]
    fn a_textured_batch_an_interface_draws_reaches_the_composer_as_a_run_over_its_texture() {
        let mut overlay = Overlay::default();
        let image = key(OverlaySpace::Local, 7);
        let picture = TextureData {
            width: 2,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![0, 0, 255, 255, 255, 0, 0, 255]],
        };
        assert!(overlay.upload(image, &picture));
        overlay
            .items
            .push(triangles(image, OverlayMaterial::Image, 6));
        let mut verts = vec![QuadVertex::zeroed(); 4];
        let runs = lower(&overlay.items, &mut verts);
        assert_eq!(
            runs,
            [ItemRun {
                texture: image,
                material: OverlayMaterial::Image,
                sampler: OverlaySampler::POINT_CLAMP,
                start: 4,
                count: 6,
            }]
        );
        assert_eq!(verts.len(), 10);
        assert_eq!(verts[5].pos, [0.1, -0.5]);
        assert_eq!(verts[5].uv, [0.25, 0.75]);
        let [r, g, b, a] = verts[5].color;
        assert_eq!((r, g, b), (1.0, 64.0 / 255.0, 0.0));
        assert!((a - 128.0 / 255.0).abs() < 1e-6);
        let held = overlay
            .textures
            .get(&image)
            .expect("the texture is the overlay's");
        assert_eq!((held.width, held.height), (2, 1));
    }

    /// Batches in a row over one texture, sampled and combined alike, are one run; a change of
    /// texture or of material starts another; a preview space and a batch of no whole triangle
    /// add nothing, and a part triangle is left off.
    #[test]
    fn batches_alike_in_a_row_share_a_run_and_a_change_starts_another() {
        let a = key(OverlaySpace::Image, 1);
        let glyphs = key(OverlaySpace::Glyphs, 1);
        let items = [
            triangles(a, OverlayMaterial::Image, 3),
            triangles(a, OverlayMaterial::Image, 7),
            triangles(a, OverlayMaterial::Invert, 3),
            OverlayItem::Preview {
                space: auberean_shell::interface::PreviewSpace::Examine,
                rect: auberean_shell::interface::Viewport {
                    x: 0,
                    y: 0,
                    width: 8,
                    height: 8,
                },
            },
            triangles(glyphs, OverlayMaterial::Text, 2),
            triangles(glyphs, OverlayMaterial::Text, 3),
        ];
        let mut verts = Vec::new();
        let runs = lower(&items, &mut verts);
        let spans: Vec<_> = runs
            .iter()
            .map(|r| (r.texture, r.material, r.start, r.count))
            .collect();
        assert_eq!(
            spans,
            [
                (a, OverlayMaterial::Image, 0, 9),
                (a, OverlayMaterial::Invert, 9, 3),
                (glyphs, OverlayMaterial::Text, 12, 3),
            ]
        );
        assert_eq!(verts.len(), 15);
    }

    /// The batches before an interface's first preview space go under the viewports' scenes and
    /// the rest over them, the preview itself among the second; with none, all go over.
    #[test]
    fn batches_before_the_first_preview_space_are_drawn_under_the_viewports_scenes() {
        let a = key(OverlaySpace::Image, 1);
        let preview = OverlayItem::Preview {
            space: auberean_shell::interface::PreviewSpace::PaperDoll,
            rect: auberean_shell::interface::Viewport {
                x: 0,
                y: 0,
                width: 8,
                height: 8,
            },
        };
        let items = [
            triangles(a, OverlayMaterial::Image, 3),
            triangles(a, OverlayMaterial::Invert, 3),
            preview.clone(),
            triangles(a, OverlayMaterial::Image, 6),
        ];
        let (under, over) = split_at_scenes(&items);
        assert_eq!(under, &items[..2]);
        assert_eq!(over, &items[2..]);
        let plain = [triangles(a, OverlayMaterial::Image, 3)];
        let (under, over) = split_at_scenes(&plain);
        assert!(under.is_empty());
        assert_eq!(over, &plain[..]);
    }

    /// An image's key already there keeps the picture it holds, a local key's is replaced, a
    /// picture of another format or short of its texels is not taken, and a release lets the
    /// texture go.
    #[test]
    fn uploads_keep_a_shared_key_s_picture_replace_a_local_one_and_refuse_what_is_not_texels() {
        let mut overlay = Overlay::default();
        let pic = |w: u32| TextureData {
            width: w,
            height: 1,
            format: TextureFormat::Bgra8,
            levels: vec![vec![255; w as usize * 4]],
        };
        let image = key(OverlaySpace::Image, 3);
        let local = key(OverlaySpace::Local, 3);
        assert!(overlay.upload(image, &pic(1)));
        assert!(overlay.upload(image, &pic(2)));
        assert_eq!(overlay.textures.get(&image).unwrap().width, 1, "kept");
        assert!(overlay.upload(local, &pic(1)));
        assert!(overlay.upload(local, &pic(2)));
        assert_eq!(overlay.textures.get(&local).unwrap().width, 2, "replaced");
        let mut short = pic(2);
        short.levels[0].pop();
        assert!(!overlay.upload(key(OverlaySpace::Local, 4), &short));
        let mut packed = pic(4);
        packed.format = TextureFormat::Bc1;
        assert!(!overlay.upload(key(OverlaySpace::Local, 5), &packed));
        assert_eq!(overlay.textures.len(), 2);
        assert_eq!(
            overlay.release(image),
            auberean_shell::interface::OverlayReleased::Freed
        );
        assert_eq!(
            overlay.release(image),
            auberean_shell::interface::OverlayReleased::Absent
        );
        assert!(overlay.textures.get(&image).is_none());
    }

    #[test]
    fn a_packed_color_unpacks_to_red_green_blue_alpha() {
        assert_eq!(unpack(0xFF00_80FF), [0.0, 128.0 / 255.0, 1.0, 1.0]);
        assert_eq!(unpack(0x0000_0000), [0.0; 4]);
    }
}
