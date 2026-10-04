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
