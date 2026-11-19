//! The marks an interface asks laid on the ground under world objects (a selection's ring), and
//! the pictures they are drawn from: each of the interface's textures is given a picture id of the
//! frame's own ([`auberean_draw::decal::OWN_PICTURES`] on) the first time a mark names it, kept
//! for as long as the texture stays the same upload, and a new one when it is uploaded again.

use std::sync::Arc;

use auberean_draw::picture::Image;
use dereth_primitives::{TextureData, TextureFormat};
use dereth_ui_overlay::{GroundMarker, OverlayTexture};

/// One texture a mark was drawn from: its key, the upload it was, its id and its picture.
#[derive(Clone, Debug)]
struct Kept {
    texture: OverlayTexture,
    upload: Arc<TextureData>,
    id: u32,
    picture: Arc<Image>,
}

/// This frame's marks, and every texture a mark was drawn from.
#[derive(Clone, Debug, Default)]
pub struct GroundMarks {
    marks: Vec<GroundMarker<u64>>,
    kept: Vec<Kept>,
}

impl GroundMarks {
    /// The marks asked for this frame, in place of the last's: each with the picture `picture`
    /// finds for its texture (a mark whose texture has none is left out).
    pub fn set(
        &mut self,
        marks: Vec<GroundMarker<u64>>,
        picture: &dyn Fn(&OverlayTexture) -> Option<Arc<TextureData>>,
    ) {
        self.marks.clear();
        for m in marks {
            let Some(upload) = picture(&m.texture) else {
                continue;
            };
            let same = self
                .kept
                .iter()
                .position(|k| k.texture == m.texture && Arc::ptr_eq(&k.upload, &upload));
            if same.is_none() {
                let Some(image) = image_of(&upload) else {
                    continue;
                };
                let next = self.kept.iter().map(|k| k.id + 1).max();
                self.kept.retain(|k| k.texture != m.texture);
                self.kept.push(Kept {
                    texture: m.texture,
                    upload,
                    id: next.unwrap_or(auberean_draw::decal::OWN_PICTURES),
                    picture: Arc::new(image),
                });
            }
            self.marks.push(m);
        }
    }

    /// This frame's marks, each with the picture id it is drawn from.
    pub fn marks(&self) -> impl Iterator<Item = (&GroundMarker<u64>, u32)> + '_ {
        self.marks.iter().filter_map(|m| {
            let k = self.kept.iter().find(|k| k.texture == m.texture)?;
            Some((m, k.id))
        })
    }

    /// The pictures this frame's marks are drawn from, by their ids.
    #[must_use]
    pub fn pictures(&self) -> Vec<(u32, Arc<Image>)> {
        self.kept
            .iter()
            .filter(|k| self.marks.iter().any(|m| m.texture == k.texture))
            .map(|k| (k.id, k.picture.clone()))
            .collect()
    }
}

/// A texture's first level as a picture the world's device draws; none in another format or with
/// no texels for its size.
fn image_of(t: &TextureData) -> Option<Image> {
    let bgra = t.levels.first()?;
    let whole = bgra.len() as u64 == u64::from(t.width) * u64::from(t.height) * 4;
    (t.format == TextureFormat::Bgra8 && whole && !bgra.is_empty()).then(|| Image {
        width: t.width,
        height: t.height,
        bgra: bgra.clone(),
        smaller: Vec::new(),
    })
}
