//! A host with an empty portal and a few objects, and a native called on it as the machine calls
//! one: its frame built by hand (the result slots, the arguments last first, the receiver).
//! Behaviour: none (test support)

// The file built here is a few hundred bytes: every length and offset fits in 32 bits.
#![allow(clippy::cast_possible_truncation)]

use std::sync::Arc;

use auberean_client_model::Item;
use auberean_client_runtime::host::{model_natives, new_host, ClientHost};
use auberean_dat::container::{FIRST_BLOCK, HEADER_OFFSET, NODE_SIZE};
use auberean_dat::DatFile;
use auberean_protocol::physics::CreateObject;
use auberean_protocol::ServerMessage;
use auberean_script::context::Context;
use auberean_script::library::Library;
use auberean_script::native::{Binding, Call};
use auberean_script::value::{tag, NativeId};
use auberean_script::{Program, Status, Vm};

pub const ITEM: u64 = 0x8130_0000_1000_0002;
pub const HOLDER: u64 = 0x8130_0000_1000_0003;
pub const ABSENT: u64 = 0x8130_0000_1000_0009;
/// The script references of the objects.
pub const ITEM_REF: u32 = 0x10;
pub const HOLDER_REF: u32 = 0x11;
pub const ABSENT_REF: u32 = 0x12;

/// A portal file with no records: one empty directory node.
fn empty_portal() -> DatFile {
    const BLOCK: usize = 0x40;
    let per = BLOCK - 4;
    let blocks = NODE_SIZE.div_ceil(per);
    let mut f = vec![0u8; FIRST_BLOCK as usize + blocks * BLOCK];
    let node = vec![0u8; NODE_SIZE];
    for k in 0..blocks {
        let at = FIRST_BLOCK as usize + k * BLOCK;
        let next = if k + 1 < blocks {
            (at + BLOCK) as u32
        } else {
            0
        };
        f[at..at + 4].copy_from_slice(&next.to_le_bytes());
        let chunk = &node[k * per..((k + 1) * per).min(node.len())];
        f[at + 4..at + 4 + chunk.len()].copy_from_slice(chunk);
    }
    let words: [u32; 21] = [
        0x5442,
        BLOCK as u32,
        f.len() as u32,
        1,
        0,
        0,
        0,
        0,
        FIRST_BLOCK,
        0,
        0,
        0,
        0,
        104,
        3,
        0,
        0,
        0,
        0,
        0,
        0,
    ];
    let h = HEADER_OFFSET as usize;
    for (i, w) in words.iter().enumerate() {
        f[h + i * 4..h + i * 4 + 4].copy_from_slice(&w.to_le_bytes());
    }
    DatFile::from_bytes(f).expect("an empty portal reads")
}

/// The scripts over an empty portal and an empty library.
pub fn scripts() -> auberean_client_runtime::host::Scripts {
    let lib = Library::default();
    let table = model_natives(&lib);
    let prog = Arc::new(Program::link(lib, table).expect("links"));
    auberean_client_runtime::host::Scripts::new(Arc::new(empty_portal()), prog)
}

pub struct Bench {
    pub vm: Vm,
    pub host: ClientHost,
    prog: Arc<Program>,
}

impl Bench {
    /// The item, held by its holder, and an object the client was never sent.
    pub fn new() -> Self {
        let lib = Library::default();
        let table = model_natives(&lib);
        let prog = Arc::new(Program::link(lib, table).expect("links"));
        let mut host = new_host(Arc::new(empty_portal()), Arc::clone(&prog));
        for id in [HOLDER, ITEM] {
            host.model.apply(&Item {
                blob_id: 0,
                message: ServerMessage::CreateObject(CreateObject {
                    id,
                    ..CreateObject::default()
                }),
            });
        }
        host.model.object_mut(ITEM).expect("the item").parent = Some((HOLDER, 1));
        host.links.insert(ITEM_REF, ITEM);
        host.links.insert(HOLDER_REF, HOLDER);
        host.links.insert(ABSENT_REF, ABSENT);
        Self {
            vm: Vm::new(Arc::clone(&prog)),
            host,
            prog,
        }
    }

    /// Call a native: `results` result slots, the arguments (`args[0]` at
    /// `fp[-2]`), the receiver. Returns the status and the result slots, nearest first.
    pub fn call(
        &mut self,
        package: u32,
        number: u32,
        receiver: u32,
        args: &[(u32, u8)],
        results: usize,
    ) -> (Status, Vec<(u32, u8)>) {
        let mut cx = Context::new(usize::MAX, 0);
        for _ in 0..results {
            cx.push(0, tag::INT).unwrap();
        }
        for (v, t) in args.iter().rev() {
            cx.push(*v, *t).unwrap();
        }
        cx.push(receiver, tag::REFERENCE).unwrap();
        cx.fp = cx.sp;
        let id = NativeId::new(package, number).0;
        let Some(Binding::Ported(f)) = self.prog.table.get(id) else {
            panic!("native {id:#x} is not bound");
        };
        let status = f(&mut Call {
            vm: &mut self.vm,
            cx: &mut cx,
            host: &mut self.host,
            native: id,
        })
        .expect("the native runs");
        let slots = (0..results).rev().map(|k| cx.slot(k).unwrap()).collect();
        (status, slots)
    }

    /// A word a native answers below the receiver and its arguments.
    pub fn answer(&mut self, package: u32, number: u32, receiver: u32, args: &[(u32, u8)]) -> u32 {
        let (status, slots) = self.call(package, number, receiver, args, 1);
        assert_eq!(status, Status::Success, "native {number:#x}");
        slots[0].0
    }
}
