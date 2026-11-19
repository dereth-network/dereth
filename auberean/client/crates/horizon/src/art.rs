//! The interface's art: the kit's ([`dereth_ui_kit::art`]) over Horizon's own pieces and fonts,
//! as the host hands them over, and the game's own pictures (the effects' icons) read from the
//! game's records behind it.

use std::sync::Arc;

pub use dereth_horizon_art::{Pieces, FILES};
pub use dereth_ui_kit::art::{Art, ArtSource, Family, Image, Sprite, TexId};

/// The game's records an interface reads: its pictures and its string tables.
pub type Records = Arc<dyn auberean_ui::Assets + Send + Sync>;

/// The game's records behind the art, for its own pictures.
struct GameSource(Records);

impl std::fmt::Debug for GameSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("GameSource")
    }
}

impl ArtSource for GameSource {
    fn icon(&self, did: u32) -> Option<Image> {
        let image = self.0.image(did)?;
        Some(Image {
            width: image.width,
            height: image.height,
            bgra: image.bgra.clone(),
        })
    }

    fn fresh(&self) -> Arc<dyn ArtSource> {
        Arc::new(Self(Arc::clone(&self.0)))
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

/// The art of Horizon's own `pieces` (their files, or the files as a host shares them), with the
/// game's `records` behind it for the game's own pictures when there are any.
#[must_use]
pub fn art(pieces: impl Into<Arc<Pieces>>, records: Option<Records>) -> Art {
    let pieces: Arc<Pieces> = pieces.into();
    let art = Art::new(pieces);
    if let Some(r) = records {
        art.set_source(Arc::new(GameSource(r)));
    }
    art
}
