//! Behaviour: none (experimental)
//!
//! The remaster's device holds a buffer for each list a frame draws. A list drawn by one frame
//! alone, which the frame made for itself and let go of, is let go of with it: frames that each
//! draw lists of their own draw on with no more buffers alive than about two frames' worth.

use std::collections::HashSet;
use std::sync::Arc;

use auberean_draw::scene::Frame;
use auberean_remaster::Remaster;

use crate::common::{frame, records, turn, STATIONS};

const W: u32 = 64;
const H: u32 = 48;

/// `f` with every object's vertices and indices copies of their own, as a frame that builds its
/// lists afresh draws them.
fn afresh(f: &Frame) -> Frame {
    let mut f = f.clone();
    for b in &mut f.objects {
        b.vertices = Arc::new(b.vertices.as_ref().clone());
        b.indices = Arc::new(b.indices.as_ref().clone());
    }
    f
}

/// **A frame's own lists go with it**: at the camp, frames that each draw every object from lists
/// of their own, enough frames for four times as many lists as the device's table holds, all draw, and
/// the buffers alive after them are within two frames' lists of those after the first.
#[test]
fn a_frames_own_lists_go_with_it() {
    let _turn = turn();
    let records = records();
    let mut load = |id: u32| auberean_draw::picture::load(records, id);
    let s = *STATIONS
        .iter()
        .find(|s| s.name == "camp-c156")
        .expect("the station");
    let base = frame(&s);
    let lists = base
        .objects
        .iter()
        .flat_map(|b| {
            [
                Arc::as_ptr(&b.vertices).cast::<u8>(),
                Arc::as_ptr(&b.indices).cast::<u8>(),
            ]
        })
        .collect::<HashSet<_>>()
        .len();
    // Each frame binds a vertex list of its own for every object, and makes an index list for it.
    let drawn = base.objects.len();
    let copies = 2 * drawn;
    let desc = dereth_gfx::DeviceDesc::default();
    let frames = (4 * desc.bindless_buffers as usize).div_ceil(drawn.max(1)) + 1;
    let mut r = Remaster::with(&desc).expect("the remaster's device");
    r.render(&afresh(&base), W, H, &mut load)
        .expect("the first frame");
    let first = r.device().stats().buffers;
    for n in 1..frames {
        r.render(&afresh(&base), W, H, &mut load)
            .unwrap_or_else(|e| panic!("frame {n} of {frames}: {e}"));
    }
    let after = r.device().stats().buffers;
    eprintln!(
        "{lists} lists, {copies} drawn afresh a frame, {frames} frames: {first} buffers alive after \
         the first, {after} after the last"
    );
    assert!(
        after as usize <= first as usize + 2 * copies,
        "{after} buffers alive, {first} after the first frame"
    );
}
