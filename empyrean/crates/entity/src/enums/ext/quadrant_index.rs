// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Entity/Enum/QuadrantIndex.cs

use crate::enums::{Quadrant, QuadrantIndex};

/// ACE's `QuadrantIndexExtensions` constants: each index's combined quadrant flags.
#[allow(non_upper_case_globals)]
pub mod quadrant_index_extensions {
    use crate::enums::Quadrant;

    const fn q(a: Quadrant, b: Quadrant, c: Quadrant) -> Quadrant {
        Quadrant(a.0 | b.0 | c.0)
    }

    // ACE: QuadrantIndexExtensions.HLF
    pub const HLF: Quadrant = q(Quadrant::High, Quadrant::Left, Quadrant::Front);
    // ACE: QuadrantIndexExtensions.MLF
    pub const MLF: Quadrant = q(Quadrant::Medium, Quadrant::Left, Quadrant::Front);
    // ACE: QuadrantIndexExtensions.LLF
    pub const LLF: Quadrant = q(Quadrant::Low, Quadrant::Left, Quadrant::Front);

    // ACE: QuadrantIndexExtensions.HRF
    pub const HRF: Quadrant = q(Quadrant::High, Quadrant::Right, Quadrant::Front);
    // ACE: QuadrantIndexExtensions.MRF
    pub const MRF: Quadrant = q(Quadrant::Medium, Quadrant::Right, Quadrant::Front);
    // ACE: QuadrantIndexExtensions.LRF
    pub const LRF: Quadrant = q(Quadrant::Low, Quadrant::Right, Quadrant::Front);

    // ACE: QuadrantIndexExtensions.HLB
    pub const HLB: Quadrant = q(Quadrant::High, Quadrant::Left, Quadrant::Back);
    // ACE: QuadrantIndexExtensions.MLB
    pub const MLB: Quadrant = q(Quadrant::Medium, Quadrant::Left, Quadrant::Back);
    // ACE: QuadrantIndexExtensions.LLB
    pub const LLB: Quadrant = q(Quadrant::Low, Quadrant::Left, Quadrant::Back);

    // ACE: QuadrantIndexExtensions.HRB
    pub const HRB: Quadrant = q(Quadrant::High, Quadrant::Right, Quadrant::Back);
    // ACE: QuadrantIndexExtensions.MRB
    pub const MRB: Quadrant = q(Quadrant::Medium, Quadrant::Right, Quadrant::Back);
    // ACE: QuadrantIndexExtensions.LRB
    pub const LRB: Quadrant = q(Quadrant::Low, Quadrant::Right, Quadrant::Back);
}

use quadrant_index_extensions as qx;

impl QuadrantIndex {
    // ACE: QuadrantIndexExtensions.ToQuadrant
    pub fn to_quadrant(self) -> Quadrant {
        match self {
            QuadrantIndex::HLF => qx::HLF,
            QuadrantIndex::MLF => qx::MLF,
            QuadrantIndex::LLF => qx::LLF,

            QuadrantIndex::HRF => qx::HRF,
            QuadrantIndex::MRF => qx::MRF,
            QuadrantIndex::LRF => qx::LRF,

            QuadrantIndex::HLB => qx::HLB,
            QuadrantIndex::MLB => qx::MLB,
            QuadrantIndex::LLB => qx::LLB,

            QuadrantIndex::HRB => qx::HRB,
            QuadrantIndex::MRB => qx::MRB,
            QuadrantIndex::LRB => qx::LRB,

            _ => Quadrant::None,
        }
    }
}

impl Quadrant {
    // ACE: QuadrantIndexExtensions.GetIndex
    pub fn get_index(self) -> QuadrantIndex {
        match self {
            qx::HLF => QuadrantIndex::HLF,
            qx::MLF => QuadrantIndex::MLF,
            qx::LLF => QuadrantIndex::LLF,

            qx::HRF => QuadrantIndex::HRF,
            qx::MRF => QuadrantIndex::MRF,
            qx::LRF => QuadrantIndex::LRF,

            qx::HLB => QuadrantIndex::HLB,
            qx::MLB => QuadrantIndex::MLB,
            qx::LLB => QuadrantIndex::LLB,

            qx::HRB => QuadrantIndex::HRB,
            qx::MRB => QuadrantIndex::MRB,
            qx::LRB => QuadrantIndex::LRB,

            _ => QuadrantIndex(0),
        }
    }
}
