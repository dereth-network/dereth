//! The effects on the player as the status bar's effects panel lists them, asked of the player's
//! scripts.
//!
//! The panel asks the interface for the effect list: a new list in a slot, the player's scripts
//! put their own list there (a node for each top-level effect on the player that is shown,
//! with its number, name, description, icon, duration and cast time, and whether it is toggled or
//! pulsed), and the panel copies each node of the list the slot then holds. It asks when it is
//! made and each time the scripts tell it an effect changed.
//!
//! The client asks again after a frame in which the scripts said an effect changed, when a session
//! starts (the panel is made anew) and when the player arrives; [`EffectList::version`] counts the changes, and [`EffectList::now`] is the game
//! time it was asked at, the clock the effects' times are on.

use auberean_script::native_object::{EffectNode, NativeObject};

use crate::host::{CallResult, Scripts};

/// The interface's function that fills the effect list (a reference to the slot holding a list).
pub const INQ_EFFECT_LIST: u32 = 0x00B4_0062;

/// The scripts' notice that an effect on the player changed.
pub const UPDATE_EFFECT: u32 = 0x0085 << 16 | 0x0002;

/// The server's call that starts the player's session.
pub const SESSION_START: u32 = 0x8212_0027;

/// A list of references, as the interface's glue makes one.
const REF_LIST: u32 = 0x65;

/// The effects on the player as the panel lists them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct EffectList {
    /// Counts the changes.
    pub version: u64,
    /// The game time the list was asked at.
    pub now: f64,
    /// The scripts' nodes, in their order.
    pub nodes: Vec<EffectNode>,
}

impl Scripts {
    /// The effects on the player, as the effects panel asks for them: a copy of each node of the
    /// list the interface's function leaves in the slot (none when it leaves no list). `None`
    /// when there is no player here.
    // D2-00519, D2-01164, D2-01039, D2-01054, D2-01001, D2-01033
    pub fn effect_list(&mut self) -> Option<(CallResult, Vec<EffectNode>)> {
        let (r, held) = self.call_out(INQ_EFFECT_LIST, REF_LIST)?;
        let NativeObject::Sequence { items, .. } = held else {
            return Some((r, Vec::new()));
        };
        let nodes = items
            .iter()
            .filter_map(|i| i.reference())
            .filter_map(
                |id| match self.vm.heap.native(id).map(NativeObject::unpacked) {
                    Ok(NativeObject::EffectNode(n)) => Some(*n),
                    _ => None,
                },
            )
            .collect();
        Some((r, nodes))
    }
}
