//! The marks an interface asks laid on the ground: which are kept, and the picture ids they are
//! drawn from.
//!
//! Behaviour: none (this client's own mark on the ground, not a behaviour of the retail client)

use std::sync::Arc;

use auberean_draw::decal::OWN_PICTURES;
use auberean_scene::marks::GroundMarks;
use dereth_primitives::{TextureData, TextureFormat};
use dereth_ui_overlay::{GroundMarker, OverlaySpace, OverlayTexture};

const RING: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 1,
};
const INNER: OverlayTexture = OverlayTexture {
    space: OverlaySpace::Local,
    key: 2,
};

fn picture(texel: u8) -> Arc<TextureData> {
    Arc::new(TextureData {
        width: 2,
        height: 2,
        format: TextureFormat::Bgra8,
        levels: vec![vec![texel; 16]],
    })
}

fn mark(object: u64, texture: OverlayTexture) -> GroundMarker<u64> {
    GroundMarker {
        object,
        texture,
        uv: [0.0, 0.0, 1.0, 1.0],
        tint: 0xFFFF_FFFF,
        scale: 1.25,
        turn: 0.0,
    }
}

#[test]
fn each_texture_is_drawn_from_a_picture_of_the_frame_s_own_kept_while_it_is_the_same_upload() {
    let (ring, inner) = (picture(9), picture(7));
    let found = |t: &OverlayTexture| match *t {
        RING => Some(ring.clone()),
        INNER => Some(inner.clone()),
        _ => None,
    };
    let mut m = GroundMarks::default();
    m.set(vec![mark(5, INNER), mark(5, RING)], &found);
    let ids: Vec<u32> = m.marks().map(|(_, id)| id).collect();
    assert_eq!(ids.len(), 2);
    assert!(ids.iter().all(|id| *id >= OWN_PICTURES) && ids[0] != ids[1]);
    let pictures = m.pictures();
    assert_eq!(pictures.len(), 2);
    let ring_id = ids[1];
    let p = &pictures.iter().find(|(id, _)| *id == ring_id).unwrap().1;
    assert_eq!((p.width, p.height, p.bgra[0]), (2, 2, 9));
    // The same uploads next frame: the same ids.
    m.set(vec![mark(5, RING)], &found);
    assert_eq!(m.marks().map(|(_, id)| id).collect::<Vec<_>>(), [ring_id]);
    assert_eq!(
        m.pictures().len(),
        1,
        "only the pictures this frame draws from"
    );
    // Uploaded again: a new id, so a device holding the old picture draws the new.
    let again = picture(3);
    m.set(vec![mark(5, RING)], &|_| Some(again.clone()));
    let id = m.marks().next().unwrap().1;
    assert_ne!(id, ring_id);
    assert_eq!(m.pictures()[0].1.bgra[0], 3);
}

#[test]
fn a_mark_whose_texture_is_not_uploaded_is_left_out_and_none_asked_is_none() {
    let mut m = GroundMarks::default();
    m.set(vec![mark(5, RING)], &|_| None);
    assert_eq!(m.marks().count(), 0);
    let ring = picture(9);
    m.set(vec![mark(5, RING)], &|_| Some(ring.clone()));
    assert_eq!(m.marks().count(), 1);
    m.set(Vec::new(), &|_| Some(ring.clone()));
    assert_eq!(m.marks().count(), 0);
    assert!(m.pictures().is_empty());
}
