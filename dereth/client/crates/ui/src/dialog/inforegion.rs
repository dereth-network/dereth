//! `InfoRegion` — the labelled, tooltipped rows of the character sheet.
//!
//! This module implements the information-region dialog behavior.
//!
//! An [`InfoRegion`] handles quality changes for one UI element. What each region kind
//! *displays* — the attribute tables, the skill training text, the enchantment durations — is
//! the game model's; what this crate owns is the binding, the element message it raises and the state
//! forwarding that turns a stat row red or green.

use crate::msg::element::id as msgid;
use crate::{ElemHandle, StateId, UiSystem};

/// The four information-region kinds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InfoRegionKind {
    /// The attribute region — Strength … Self.
    Attribute,
    /// The secondary-attribute region — Health/Stamina/Mana.
    Attribute2nd,
    /// The skill region — one skill.
    Skill,
    /// The effect region — one active enchantment.
    Effect,
}

/// One stat row.
#[derive(Debug, Clone)]
pub struct InfoRegion {
    pub kind: InfoRegionKind,
    /// The region's id binds it to a `StatType`.
    pub stat: u32,
    /// The element the row lives on.
    pub element: ElemHandle,
    pub value: i32,
    pub label: String,
    pub tooltip: String,
}

impl InfoRegion {
    #[must_use]
    pub fn new(kind: InfoRegionKind, element: ElemHandle) -> Self {
        Self {
            kind,
            stat: 0,
            element,
            value: 0,
            label: String::new(),
            tooltip: String::new(),
        }
    }

    /// A tracked quality changed — recompute the displayed value and broadcast
    /// element message **`0x10000004`** with `p1 = StatType`, `p2 = new value`.
    pub fn on_quality_changed(&mut self, ui: &mut UiSystem, value: i32) {
        self.value = value;
        ui.broadcast_element_message(
            self.element,
            msgid::QUALITY_CHANGED,
            self.stat,
            u32::from_ne_bytes(value.to_ne_bytes()),
        );
    }

    /// Forwards to [`UiSystem::set_state`], which is how a stat
    /// row turns red or green when it is buffed or debuffed.
    pub fn set_state(&self, ui: &mut UiSystem, s: StateId) {
        ui.set_state(self.element, s);
    }

    /// Rewrite the label and the tooltip.
    pub fn update(&mut self, ui: &mut UiSystem, label: String, tooltip: String) {
        self.label = label;
        self.tooltip.clone_from(&tooltip);
        ui.set_tooltip(self.element, Some(tooltip));
    }
}
