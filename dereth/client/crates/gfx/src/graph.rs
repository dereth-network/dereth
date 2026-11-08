//! The render graph's barrier placement, device-free.
//!
//! A pass says how it touches each resource ([`Use`]); the graph keeps, for every resource, how
//! it was last touched ([`Track`]), and puts in front of the pass the one barrier each resource
//! needs ([`Track::to`]). The rule is conservative and simple:
//!
//! - a change of image layout, a use after a write, and a write after any use each take a
//!   barrier from every stage that touched the resource since the last barrier to the stages of
//!   the new use, making the last write's memory visible to it;
//! - a read after reads in the same layout takes none, and joins its stages to the ones a later
//!   write must wait for.
//!
//! Passes run in the order they are declared: the graph places barriers, it does not reorder or
//! cull. The recorder ([`crate::Recorder`]) applies this to the passes it records.

use ash::vk;

/// How a pass touches a resource.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Use {
    /// Drawn into, and blended over, as a colour target.
    ColorTarget,
    /// Depth tested and written, as a depth target.
    DepthTarget,
    /// Depth tested without being written.
    DepthTest,
    /// Sampled by a shader of any stage.
    Sampled,
    /// Read by a shader as a storage buffer.
    StorageRead,
    /// Read and written by a shader as a storage buffer.
    StorageWrite,
    /// Read as indirect arguments or a draw count.
    Indirect,
    /// Read as an index buffer.
    Index,
    /// Read by a copy.
    TransferSrc,
    /// Written by a copy, a fill or a clear.
    TransferDst,
    /// Read by the processor after the device wrote it.
    HostRead,
    /// Lent by, or handed back to, an owner outside the layer, in the state it says.
    External(State),
}

/// One state a resource is in: its image layout (`UNDEFINED` for a buffer), and the stages and
/// accesses of its use.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct State {
    /// The image layout.
    pub layout: vk::ImageLayout,
    /// The pipeline stages.
    pub stages: vk::PipelineStageFlags2,
    /// The memory accesses.
    pub access: vk::AccessFlags2,
}

const SHADERS: vk::PipelineStageFlags2 = vk::PipelineStageFlags2::from_raw(
    vk::PipelineStageFlags2::VERTEX_SHADER.as_raw()
        | vk::PipelineStageFlags2::FRAGMENT_SHADER.as_raw()
        | vk::PipelineStageFlags2::COMPUTE_SHADER.as_raw(),
);

const DEPTH_TESTS: vk::PipelineStageFlags2 = vk::PipelineStageFlags2::from_raw(
    vk::PipelineStageFlags2::EARLY_FRAGMENT_TESTS.as_raw()
        | vk::PipelineStageFlags2::LATE_FRAGMENT_TESTS.as_raw(),
);

impl Use {
    /// The state the use puts an image or buffer in.
    #[must_use]
    pub const fn state(self) -> State {
        use vk::{AccessFlags2 as A, ImageLayout as L, PipelineStageFlags2 as S};
        let (layout, stages, access) = match self {
            Self::ColorTarget => (
                L::COLOR_ATTACHMENT_OPTIMAL,
                S::COLOR_ATTACHMENT_OUTPUT,
                A::from_raw(A::COLOR_ATTACHMENT_READ.as_raw() | A::COLOR_ATTACHMENT_WRITE.as_raw()),
            ),
            Self::DepthTarget => (
                L::DEPTH_STENCIL_ATTACHMENT_OPTIMAL,
                DEPTH_TESTS,
                A::from_raw(
                    A::DEPTH_STENCIL_ATTACHMENT_READ.as_raw()
                        | A::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw(),
                ),
            ),
            Self::DepthTest => (
                L::DEPTH_STENCIL_READ_ONLY_OPTIMAL,
                DEPTH_TESTS,
                A::DEPTH_STENCIL_ATTACHMENT_READ,
            ),
            Self::Sampled => (L::SHADER_READ_ONLY_OPTIMAL, SHADERS, A::SHADER_SAMPLED_READ),
            Self::StorageRead => (L::UNDEFINED, SHADERS, A::SHADER_STORAGE_READ),
            Self::StorageWrite => (
                L::UNDEFINED,
                SHADERS,
                A::from_raw(A::SHADER_STORAGE_READ.as_raw() | A::SHADER_STORAGE_WRITE.as_raw()),
            ),
            Self::Indirect => (L::UNDEFINED, S::DRAW_INDIRECT, A::INDIRECT_COMMAND_READ),
            Self::Index => (L::UNDEFINED, S::INDEX_INPUT, A::INDEX_READ),
            Self::TransferSrc => (L::TRANSFER_SRC_OPTIMAL, S::ALL_TRANSFER, A::TRANSFER_READ),
            Self::TransferDst => (L::TRANSFER_DST_OPTIMAL, S::ALL_TRANSFER, A::TRANSFER_WRITE),
            Self::HostRead => (L::UNDEFINED, S::HOST, A::HOST_READ),
            Self::External(s) => (s.layout, s.stages, s.access),
        };
        State {
            layout,
            stages,
            access,
        }
    }

    /// Whether the use writes.
    #[must_use]
    pub const fn writes(self) -> bool {
        match self {
            Self::ColorTarget | Self::DepthTarget | Self::StorageWrite | Self::TransferDst => true,
            Self::External(s) => s.access.as_raw() & WRITES.as_raw() != 0,
            _ => false,
        }
    }
}

/// Every access that writes.
const WRITES: vk::AccessFlags2 = vk::AccessFlags2::from_raw(
    vk::AccessFlags2::COLOR_ATTACHMENT_WRITE.as_raw()
        | vk::AccessFlags2::DEPTH_STENCIL_ATTACHMENT_WRITE.as_raw()
        | vk::AccessFlags2::SHADER_STORAGE_WRITE.as_raw()
        | vk::AccessFlags2::SHADER_WRITE.as_raw()
        | vk::AccessFlags2::TRANSFER_WRITE.as_raw()
        | vk::AccessFlags2::HOST_WRITE.as_raw()
        | vk::AccessFlags2::MEMORY_WRITE.as_raw(),
);

/// One barrier: a memory dependency, with an image layout transition when the layouts differ.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Barrier {
    /// The stages waited for.
    pub src_stages: vk::PipelineStageFlags2,
    /// The writes made available.
    pub src_access: vk::AccessFlags2,
    /// The stages that wait.
    pub dst_stages: vk::PipelineStageFlags2,
    /// The accesses the writes are made visible to.
    pub dst_access: vk::AccessFlags2,
    /// The layout before.
    pub old_layout: vk::ImageLayout,
    /// The layout after.
    pub new_layout: vk::ImageLayout,
}

/// How a resource was last touched.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Track {
    /// The image layout it is in (`UNDEFINED` for a buffer, or an image never touched).
    pub layout: vk::ImageLayout,
    /// The stages that touched it since its last barrier.
    pub stages: vk::PipelineStageFlags2,
    /// The accesses those stages made.
    pub access: vk::AccessFlags2,
    /// Whether one of them wrote it.
    pub written: bool,
}

impl Default for Track {
    fn default() -> Self {
        Self::new()
    }
}

impl Track {
    /// A resource nothing has touched.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            layout: vk::ImageLayout::UNDEFINED,
            stages: vk::PipelineStageFlags2::NONE,
            access: vk::AccessFlags2::NONE,
            written: false,
        }
    }

    /// A resource lent in `state`, as its owner last left it.
    #[must_use]
    pub const fn lent(state: State) -> Self {
        Self {
            layout: state.layout,
            stages: state.stages,
            access: state.access,
            written: true,
        }
    }

    /// Touch the resource with `next`: the barrier that must come first, if one must, and the
    /// track updated. `image` says whether the resource is an image, whose layout matters; a
    /// buffer's never does.
    pub fn to(&mut self, next: Use, image: bool) -> Option<Barrier> {
        let want = next.state();
        let layout = if image {
            want.layout
        } else {
            vk::ImageLayout::UNDEFINED
        };
        let relayout = image && layout != self.layout;
        let writes = next.writes();
        if !relayout && !self.written && !writes {
            // A read after reads: no barrier, and a later write waits for this one too.
            self.stages |= want.stages;
            self.access |= want.access;
            return None;
        }
        let barrier = Barrier {
            src_stages: self.stages,
            src_access: if self.written {
                self.access & WRITES
            } else {
                vk::AccessFlags2::NONE
            },
            dst_stages: want.stages,
            dst_access: want.access,
            old_layout: self.layout,
            new_layout: layout,
        };
        // A layout transition is itself a write the next use in another stage must wait for.
        *self = Self {
            layout,
            stages: want.stages,
            access: want.access,
            written: writes || relayout,
        };
        Some(barrier)
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (experimental)
    use super::*;

    /// A colour target drawn and then sampled is moved to the sampled layout once, after the
    /// draw's writes, and sampling it again in the same layout needs nothing.
    #[test]
    fn a_target_drawn_then_sampled_twice_takes_one_barrier() {
        let mut t = Track::new();
        let first = t
            .to(Use::ColorTarget, true)
            .expect("the first use lays the image out");
        assert_eq!(first.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(first.new_layout, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        assert_eq!(first.src_access, vk::AccessFlags2::NONE);
        let to_read = t.to(Use::Sampled, true).expect("sampling after drawing");
        assert_eq!(
            to_read.src_stages,
            vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT
        );
        assert_eq!(to_read.src_access, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE);
        assert_eq!(
            to_read.new_layout,
            vk::ImageLayout::SHADER_READ_ONLY_OPTIMAL
        );
        assert_eq!(to_read.dst_access, vk::AccessFlags2::SHADER_SAMPLED_READ);
        // The transition counts as a write, so the second sampling waits on it; the third,
        // after a sampling with nothing between, does not.
        assert!(t.to(Use::Sampled, true).is_some());
        assert_eq!(t.to(Use::Sampled, true), None);
    }

    /// Reads of a buffer from different stages take no barrier between them, and the write after
    /// them waits for every one of their stages, with no memory to make available.
    #[test]
    fn a_write_after_reads_waits_for_every_read_stage() {
        let mut b = Track::new();
        assert_eq!(b.to(Use::Indirect, false), None);
        assert_eq!(b.to(Use::StorageRead, false), None);
        let w = b.to(Use::StorageWrite, false).expect("a write after reads");
        assert!(w
            .src_stages
            .contains(vk::PipelineStageFlags2::DRAW_INDIRECT));
        assert!(w
            .src_stages
            .contains(vk::PipelineStageFlags2::COMPUTE_SHADER));
        assert_eq!(w.src_access, vk::AccessFlags2::NONE);
        assert_eq!(w.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(w.new_layout, vk::ImageLayout::UNDEFINED);
        let r = b.to(Use::Indirect, false).expect("a read after the write");
        assert_eq!(r.src_access, vk::AccessFlags2::SHADER_STORAGE_WRITE);
        assert_eq!(r.dst_stages, vk::PipelineStageFlags2::DRAW_INDIRECT);
    }

    /// An image lent in its owner's state is moved from that state, waiting for the owner's own
    /// writes, and handed back in the state the owner asks for.
    #[test]
    fn a_lent_image_is_taken_from_and_returned_to_its_owners_state() {
        let owner = State {
            layout: vk::ImageLayout::TRANSFER_SRC_OPTIMAL,
            stages: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        };
        let mut t = Track::lent(owner);
        let take = t.to(Use::ColorTarget, true).expect("taking the image");
        assert_eq!(take.old_layout, vk::ImageLayout::TRANSFER_SRC_OPTIMAL);
        assert_eq!(take.src_access, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE);
        let back = State {
            layout: vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL,
            stages: vk::PipelineStageFlags2::COLOR_ATTACHMENT_OUTPUT,
            access: vk::AccessFlags2::COLOR_ATTACHMENT_WRITE,
        };
        let give = t.to(Use::External(back), true).expect("handing it back");
        assert_eq!(give.old_layout, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        assert_eq!(give.new_layout, vk::ImageLayout::COLOR_ATTACHMENT_OPTIMAL);
        assert_eq!(give.src_access, vk::AccessFlags2::COLOR_ATTACHMENT_WRITE);
    }

    /// A buffer's layout never matters: a copy into it and a read of it by the processor take one
    /// barrier, with no layout in it.
    #[test]
    fn a_buffer_never_changes_layout() {
        let mut b = Track::new();
        assert!(b.to(Use::TransferDst, false).is_some());
        let host = b
            .to(Use::HostRead, false)
            .expect("the host waits for the copy");
        assert_eq!(host.old_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(host.new_layout, vk::ImageLayout::UNDEFINED);
        assert_eq!(host.src_access, vk::AccessFlags2::TRANSFER_WRITE);
        assert_eq!(host.dst_access, vk::AccessFlags2::HOST_READ);
    }
}
