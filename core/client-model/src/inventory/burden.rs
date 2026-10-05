//! `EncumbranceSystem` — burden, capacity and the load thresholds.
//!
//! The client shows burden and uses load for local movement/jump permission and scaling.
//! Server authority is separate; these are the client's own inquiry kernels.
//!
//! Shared arithmetic and inquiries live in `dereth-rules`; this module keeps the
//! client model's state adapters and value tests.

#[cfg(test)]
use dereth_rules::burden::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: §4's load enquiry — strength defaults to 10 when the attribute cache is absent.
    #[test]
    fn inq_load_defaults_strength_to_ten() {
        use crate::qualities::{Qualities, StatKey, StatType, StatValue};
        let mut q = Qualities::new();
        q.set(StatKey::new(StatType::Int, 5), StatValue::Int(750));
        // capacity = 10 * 150 = 1500; 750 / 1500 = 0.5
        assert_eq!(inq_load(&q), 0.5);
    }
}
