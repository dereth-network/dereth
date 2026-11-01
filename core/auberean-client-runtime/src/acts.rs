//! The story's acts as the quest window's story page lists them, asked of the player's scripts.
//!
//! The page asks the interface for the act list each time it lays itself out: a new list of acts
//! in a slot, the player's scripts fill it (an act for each act of the story, by its number:
//! its id, name, number, whether it is visible and whether the player holds one of its scenes,
//! and its scenes in order, each with its id, name, number, whether hidden and whether it may be
//! played again), and the page takes a copy of the list the slot then holds.
//!
//! The client asks again after a frame in which the scripts said the story changed, and when the
//! player arrives; [`ActList::version`] counts the changes.

use auberean_script::native_object::{native_type, NativeObject};
use auberean_script::story::ActInfo;

use crate::host::{CallResult, Scripts};

/// The interface's function that fills the act list (a reference to the slot holding a list).
pub const INQ_ACT_INFO_LIST: u32 = 0x00B4_008F;

/// The scripts' notice that the story changed (how, the act).
pub const UPDATE_STORY_QUESTS: u32 = 0x008E << 16 | 0x0002;

/// The story's acts as the page lists them.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ActList {
    /// Counts the changes.
    pub version: u64,
    /// The acts, in the scripts' order.
    pub acts: Vec<ActInfo>,
    /// How the last asking went.
    pub result: Option<CallResult>,
}

impl Scripts {
    /// The story's acts, as the story page asks for them: a copy of the list the interface's
    /// function leaves in the slot (none when it leaves no list). `None` when there is no player
    /// here.
    // D2-00592, D2-01157
    pub fn act_list(&mut self) -> Option<(CallResult, Vec<ActInfo>)> {
        let (r, held) = self.call_out(INQ_ACT_INFO_LIST, native_type::ACT_INFO_LIST)?;
        let acts = match held.unpacked() {
            NativeObject::Acts(v) => v,
            _ => Vec::new(),
        };
        Some((r, acts))
    }
}
