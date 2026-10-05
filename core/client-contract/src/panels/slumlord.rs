//! The house payment window's operation: which of a house's two payment lists is in play.
//!
//! The housing model dispatches its six payment-list operations on it and the slumlord panel
//! holds it as its current tab, so both read it here. `dereth_client_model::housing` and
//! `dereth_ui_screens::panels::slumlord` re-export it.

/// Which payment list an operation is about: `1` selects purchase, `2` rent. The undefined
/// operation selects neither, so each list operation answers **0 / false / empty** for it -- which
/// is what makes a payment panel with no selected tab refuse every drop and disable both buttons.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum HouseOp {
    /// Before either tab is chosen.
    #[default]
    Undef = 0,
    /// The Buy tab and the purchase list.
    Buy = 1,
    /// The Rent tab and the rent list.
    Rent = 2,
}

impl HouseOp {
    /// Whether this op names the rent list, which is the one bit the host needs.
    #[must_use]
    pub fn is_rent(self) -> bool {
        self == Self::Rent
    }
}

/// One offered object, retaining its payment facts until removed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PaymentItem {
    pub id: dereth_primitives::ObjectId,
    pub wcid: u32,
    pub amount: i32,
    pub trade_note_value: Option<i32>,
}

/// Read-only state shared by the two housing windows.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct PaymentListsView {
    pub op: HouseOp,
    pub buy: Vec<PaymentItem>,
    pub rent: Vec<PaymentItem>,
    pub buy_payment: crate::view::SlumlordPayment,
    pub rent_payment: crate::view::SlumlordPayment,
    pub visible: bool,
}

/// Gestures on the shared offered payment lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaymentAction {
    Select(HouseOp),
    Add(dereth_primitives::ObjectId),
    Remove(dereth_primitives::ObjectId),
    Clear,
    Close,
    /// The caller has completed the shared confirmation, if one was required.
    Submit,
}

/// `HouseType::Apartment`.
pub const APARTMENT: u32 = 4;

/// Whether buying a house of `house_type` asks the player first: every house but an apartment,
/// and an apartment too on a world whose rules ask before every purchase.
#[must_use]
pub fn purchase_asks_first(house_type: u32, rules: &dereth_primitives::WorldRules) -> bool {
    house_type != APARTMENT || !rules.apartment_buys_without_asking
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Behaviour: world.rules.a-world-profile-sets-its-clients-rules-over-the-end-of-retails
    #[test]
    fn an_apartment_is_bought_at_once_unless_the_world_asks_before_every_purchase() {
        let retail = dereth_primitives::WorldRules::default();
        assert!(purchase_asks_first(1, &retail), "a cottage asks");
        assert!(!purchase_asks_first(APARTMENT, &retail));
        let every = dereth_primitives::WorldRules {
            apartment_buys_without_asking: false,
            ..retail
        };
        assert!(purchase_asks_first(APARTMENT, &every));
        assert!(purchase_asks_first(2, &every));
    }
}
