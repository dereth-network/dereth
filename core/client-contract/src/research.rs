//! The spell research formula, as the early clients' research page built it: up to eight carried
//! spell components, laid in order, tried on a target as one test. A formula that makes a spell
//! casts it, and teaches it if the character did not know it; one that makes none is refused as
//! any cast is. Every interface's research page edits one of these and sends it with
//! [`crate::UiRequest::TestSpellFormula`].

/// The most components a formula holds.
pub const FORMULA_SLOTS: usize = 8;

/// A formula being laid: component weenie class ids, in the order they were laid.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Formula {
    components: Vec<u32>,
}

impl Formula {
    /// The components laid so far.
    #[must_use]
    pub fn components(&self) -> &[u32] {
        &self.components
    }

    /// Whether nothing is laid: the page's Test and Clear wait for a component.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.components.is_empty()
    }

    /// Lays `wcid` at the formula's end. A full formula, and an empty id, take nothing.
    pub fn add(&mut self, wcid: u32) -> bool {
        if wcid == 0 || self.components.len() >= FORMULA_SLOTS {
            return false;
        }
        self.components.push(wcid);
        true
    }

    /// Takes the component in slot `index` out; the later ones close up behind it.
    pub fn remove(&mut self, index: usize) -> bool {
        if index < self.components.len() {
            self.components.remove(index);
            true
        } else {
            false
        }
    }

    /// Empties the formula.
    pub fn clear(&mut self) {
        self.components.clear();
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (a small value type; the research request's behaviour is tested where it
    //! is sent).
    use super::*;

    #[test]
    fn a_formula_holds_eight_components_in_the_order_they_were_laid() {
        let mut f = Formula::default();
        for wcid in 1..=10 {
            f.add(wcid);
        }
        assert_eq!(f.components(), [1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(!f.add(0));
        assert!(f.remove(0));
        assert_eq!(f.components()[0], 2);
        assert!(!f.remove(9));
        f.clear();
        assert!(f.is_empty());
    }
}
