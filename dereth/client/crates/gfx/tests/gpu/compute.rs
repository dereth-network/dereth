//! A compute pass writes a storage buffer through the bindless table, a copy reads it back, and
//! the marks around them are written.
//!
//! Behaviour: none (experimental)

use dereth_gfx::{BufferDesc, BufferUsage, ComputeDesc, Location, Touch, Use};

use crate::shaders::Entry;

/// What invocation `i` writes: this plus `i`.
const VALUE: u32 = 7_000;

/// 1,024 words written by a dispatch through the table's storage binding, each invocation its
/// index plus a pushed value, and copied into a read-back buffer hold exactly what each
/// invocation wrote; and the three marks around the work
/// give two spans, in order.
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "draws on the hardware GPU: --features retail-dats, one GPU run at a time"
)]
fn a_dispatch_writes_a_storage_buffer_and_its_marks_are_read() {
    const N: u64 = 1024;
    let mut d = crate::hardware();
    let cs = crate::spirv(Entry::FillCsMain);
    let p = d
        .create_compute_pipeline(&ComputeDesc {
            label: "fill",
            shader: crate::shader(Entry::FillCsMain, &cs),
        })
        .expect("the pipeline builds");
    let words = d
        .create_buffer(&BufferDesc {
            label: "words",
            size: N * 4,
            usage: BufferUsage::STORAGE | BufferUsage::TRANSFER_SRC,
            location: Location::Gpu,
        })
        .expect("a buffer");
    let index = d.bind_buffer(words).expect("an index");
    let back = d
        .create_buffer(&BufferDesc {
            label: "back",
            size: N * 4,
            usage: BufferUsage::TRANSFER_DST,
            location: Location::Readback,
        })
        .expect("a buffer");
    {
        let mut r = d.begin().expect("a recording");
        r.timestamp("fill");
        r.compute(&[Touch::Buffer(words, Use::StorageWrite)], |c| {
            c.set_pipeline(p)?;
            let push: Vec<u8> = [index, VALUE, N as u32]
                .iter()
                .flat_map(|w| w.to_le_bytes())
                .collect();
            c.push(0, &push)?;
            c.dispatch((N / 64) as u32, 1, 1);
            Ok(())
        })
        .expect("the dispatch records");
        r.timestamp("copy");
        r.copy_buffer(words, 0, back, 0, N * 4).expect("the copy");
        r.touch(&[Touch::Buffer(back, Use::HostRead)])
            .expect("the read");
        r.timestamp("end");
        let ticket = r.submit().expect("submits");
        d.wait(ticket).expect("finishes");
    }
    let bytes = d.read_buffer(back, 0, N * 4).expect("reads back");
    let got: Vec<u32> = bytes
        .as_chunks::<4>()
        .0
        .iter()
        .map(|w| u32::from_le_bytes(*w))
        .collect();
    let want: Vec<u32> = (0..N as u32).map(|i| VALUE + i).collect();
    assert_eq!(got, want);
    if d.caps().timestamps {
        let spans = d.timings();
        let labels: Vec<&str> = spans.iter().map(|t| t.label).collect();
        assert_eq!(labels, ["fill", "copy"], "{spans:?}");
    }
    crate::no_validation_errors();
}
