//! Icon descriptions in recorded script calls: what a copy of an object holding one keeps.

use auberean_script::args::{self, Arg};
use auberean_script::graph::{Body, Graph};
use auberean_script::heap::Heap;
use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::wire::Out;
use auberean_testkit::captures::{self, carrier};

use crate::args::library;

/// The script call that lists a player's sales, each with its product's icon description.
const LIST_SALES: u32 = 0x9B5E_005D;

/// The packed form of each icon description in a graph, with its id.
fn icons(g: &Graph) -> Vec<(u32, Vec<u8>)> {
    g.ids
        .iter()
        .zip(&g.entries)
        .filter_map(|(id, e)| {
            let Body::Object(o) = &e.body else {
                return None;
            };
            if u32::from(o.native_type) != native_type::ICON_DESC {
                return None;
            }
            let n: &NativeObject = o.native.as_ref()?;
            let mut w = Out::new();
            n.write(&mut w).expect("an icon description packs");
            Some((*id, w.buf))
        })
        .collect()
}

/// The script object that holds a reference to an entry of a graph.
fn holder(g: &Graph, of: u32) -> Option<u32> {
    g.ids
        .iter()
        .zip(&g.entries)
        .find_map(|(id, e)| match &e.body {
            Body::Object(o) if o.native.is_none() && o.slots.contains(&of) => Some(*id),
            _ => None,
        })
}

/// **A sale recorded in January 2017, copied whole as a script's copy copies it, holds its
/// product's icon as the server sent it**: the same count and layers, byte for byte.
///
/// Behaviour: script.graph.an-object-copied-keeps-its-icons-layers
#[test]
fn a_recorded_sale_copied_keeps_the_layers_of_its_products_icon() {
    let lib = library().expect("the September 2013 install opens");
    let index = auberean_testkit::data::require_capture_index();
    let (mut copied, mut layered, mut differ) = (0usize, 0usize, 0usize);
    captures::messages(&index, &carrier::ALL, &[2017], |m| {
        let Some(call) = m.script_call() else { return };
        if call.function != LIST_SALES {
            return;
        }
        let Ok(values) = args::decode(&call.args, lib) else {
            return;
        };
        for v in &values {
            let Arg::Object(g) = v else { continue };
            let sent = icons(g);
            if sent.is_empty() {
                continue;
            }
            let mut heap = Heap::new();
            let root = heap
                .import(g, None)
                .expect("the graph reads into the table");
            let held = heap.export(root).expect("the graph packs again");
            for (id, bytes) in icons(&held) {
                let sale = holder(&held, id).expect("a sale holds its product's icon");
                let graph = heap.export(sale).expect("the sale packs");
                let copy = heap.import(&graph, None).expect("the copy reads");
                let back = heap.export(copy).expect("the copy packs");
                let kept = icons(&back);
                copied += 1;
                layered += usize::from(bytes.len() > 4);
                if kept.len() != 1 || kept[0].1 != bytes {
                    differ += 1;
                }
            }
            let mut before: Vec<Vec<u8>> = sent.into_iter().map(|(_, b)| b).collect();
            let mut after: Vec<Vec<u8>> = icons(&held).into_iter().map(|(_, b)| b).collect();
            before.sort();
            after.sort();
            assert_eq!(
                before, after,
                "the graph read in holds the icons the server sent"
            );
        }
    })
    .expect("the capture index reads");
    println!("{copied} sales copied, {layered} with layers, {differ} differ");
    assert!(copied > 0, "the index holds recorded sales");
    assert!(layered > 0, "some recorded sale's icon has layers");
    assert_eq!(differ, 0, "a copy holds its icon as sent");
}
