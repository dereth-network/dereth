//! Staged writes reach only the buffer they were made for.
//!
//! Behaviour: none (experimental)

use dereth_gfx::{Buffer, BufferDesc, BufferUsage, Device, Location, Touch, Use};

fn buffer(d: &mut Device, label: &'static str, size: u64) -> Buffer {
    d.create_buffer(&BufferDesc {
        label,
        size,
        usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC | BufferUsage::TRANSFER_DST,
        location: Location::Gpu,
    })
    .expect("a buffer")
}

/// **A write staged for a buffer destroyed before it is recorded lands nowhere**: a larger
/// buffer written and destroyed at once, and a smaller one made after it in its place and written
/// with its own bytes, read back hold only those bytes, and the validation layer reports nothing:
/// the destroyed buffer's write is not copied into whatever holds its place by then.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_write_staged_for_a_destroyed_buffer_lands_nowhere() {
    let mut d = crate::hardware();
    let gone = buffer(&mut d, "gone", 4096);
    d.write_buffer(gone, 0, &[0xAA; 4096]).expect("staged");
    d.destroy_buffer(gone);
    let kept = buffer(&mut d, "kept", 64);
    d.write_buffer(kept, 0, &[0x11; 64]).expect("staged");
    let back = d
        .create_buffer(&BufferDesc {
            label: "back",
            size: 64,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })
        .expect("a buffer");
    {
        let mut r = d.begin().expect("a recording");
        r.copy_buffer(kept, 0, back, 0, 64).expect("the copy");
        r.touch(&[Touch::Buffer(back, Use::HostRead)])
            .expect("the read");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let bytes = d.read_buffer(back, 0, 64).expect("reads back");
    assert!(bytes.iter().all(|b| *b == 0x11), "{bytes:?}");
    d.destroy_buffer(kept);
    d.destroy_buffer(back);
    crate::no_validation_errors();
}
