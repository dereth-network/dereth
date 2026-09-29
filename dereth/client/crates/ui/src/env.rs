//! The asset source and the layout-enum resolver a screen constructs from.
//!
//! The pair is a value, and it lives here, beside the [`UiSystem`] that holds it.
//!
//! The client's framework registry uses no-argument factories, and [`crate::framework::UiFlow`]
//! mirrors that constraint, so a screen cannot be handed an asset source when it is built. In the
//! client this is not a problem: a lookup reaches the dat cache through a singleton. Here the pair is an
//! [`Env`] value that the host gives its `UiSystem` once ([`UiSystem::set_env`]), and everything
//! that builds elements -- a `Screen::create`, a panel adding list rows, an item slot looking up
//! its background tile -- reads it back from the `UiSystem` it was handed
//! ([`UiSystem::env`], [`UiSystem::require_env`]). Two UIs, or a test matrix running in
//! parallel, each hold their own.
//!
//! **Never hard-code a DataID at a call site**: a screen names a
//! [`LayoutEnum`] and the installed [`LayoutEnumResolver`] answers it, so a DDD patch that moves a
//! layout still resolves.
//!
//! The dat-cache stand-in memo for [`Env::did_by_enum`] lives inside the value, so clones of one
//! `Env` share one memo and two different `Env`s never answer each other's lookups.

use std::cell::RefCell;
use std::rc::Rc;

use crate::framework::{LayoutEnum, LayoutEnumResolver};
use crate::{ElemHandle, ElementId, UiError, UiSystem};
use dereth_primitives::AssetSource;

/// `DBCache`'s stand-in for [`Env::did_by_enum`]: `(group, value)` to the answer, `None` included.
// ORDER-OK: a memo keyed by (group, value) and only ever looked up.
type EnumDidMemo = std::collections::HashMap<(u32, u32), Option<dereth_primitives::DataId>>;

/// The asset source and layout-enum resolver a screen constructs from, as a value.
///
/// Cloning shares both `Rc`s **and** the [`Self::did_by_enum`] memo, so a clone taken at a
/// `Screen::create` entry point is the same environment, not a second one.
#[derive(Clone)]
pub struct Env {
    assets: Rc<dyn AssetSource>,
    resolver: Rc<dyn LayoutEnumResolver>,
    /// `DBCache`'s stand-in for [`Self::did_by_enum`]. See its doc comment.
    // ORDER-OK: a memo keyed by (group, value) and only ever looked up.
    enum_dids: Rc<RefCell<EnumDidMemo>>,
}

impl std::fmt::Debug for Env {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Neither `AssetSource` nor `LayoutEnumResolver` is `Debug`; the memo's size is the only
        // thing here a reader can act on.
        f.debug_struct("Env")
            .field("memoised_enum_dids", &self.enum_dids.borrow().len())
            .finish()
    }
}

impl Env {
    /// Build an environment. No install, no global: the caller owns it.
    ///
    /// The production pair is the retail dat store and
    /// `dereth_ui::framework::DidMapperResolver::load_via_master`; the test pair is any
    /// [`AssetSource`] and `dereth_ui::framework::TableResolver`.
    #[must_use]
    pub fn new(assets: Rc<dyn AssetSource>, resolver: Rc<dyn LayoutEnumResolver>) -> Self {
        Self {
            assets,
            resolver,
            enum_dids: Rc::new(RefCell::new(EnumDidMemo::new())),
        }
    }

    /// The asset source this environment was built from.
    #[must_use]
    pub fn assets(&self) -> &dyn AssetSource {
        self.assets.as_ref()
    }

    /// The layout-enum resolver this environment was built from.
    #[must_use]
    pub fn resolver(&self) -> &dyn LayoutEnumResolver {
        self.resolver.as_ref()
    }

    /// the client's create-and-add-root-element `(layout enum, element id)`, against this
    /// environment.
    pub fn create_and_add_root_element(
        &self,
        ui: &mut UiSystem,
        layout: LayoutEnum,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        crate::framework::create_and_add_root_element(
            ui,
            self.assets.as_ref(),
            self.resolver.as_ref(),
            layout,
            element,
        )
    }

    /// Create a child element by enum — used by `MessageLogPanel` for its bubbles and by
    /// `MapPanel` for its 53 location notes.
    pub fn create_child_element_by_enum(
        &self,
        ui: &mut UiSystem,
        parent: ElemHandle,
        layout: LayoutEnum,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        let did = self
            .resolver
            .resolve(layout)
            .ok_or(UiError::UnresolvedLayoutEnum(layout))?;
        let h = ui.create_child_by_data_id(self.assets.as_ref(), parent, did, element)?;
        ui.initialize_tree(h);
        Ok(h)
    }

    /// The list-box "add item from a template list" path's other half: a template named by a
    /// **layout DataID** rather than by a layout enum.
    ///
    /// The template list is element property **`0x64`**, an `Array` of `Struct`s whose members are
    /// `0x63` (the template's layout, a `DataFile`) and `0x62` (the template's element id, an
    /// `Enum`). Those layouts have no entry in `DidMapper 0x2500000E` at all — the shipped char-gen
    /// templates live in `0x2100004C`, which no enum names — so the enum path above cannot reach
    /// one and the DataID has to come from the property. Nothing here is hard-coded; the id is read
    /// out of the list box's own description.
    pub fn create_child_element_by_data_id(
        &self,
        ui: &mut UiSystem,
        parent: ElemHandle,
        layout: dereth_primitives::DataId,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        let h = ui.create_child_by_data_id(self.assets.as_ref(), parent, layout, element)?;
        ui.initialize_tree(h);
        Ok(h)
    }

    /// Run `f` against this environment's asset source.
    ///
    /// The pop-up menu constructor and its text-item insert are `dereth-ui`'s, and
    /// both take a `&dyn AssetSource` because a widget has none — the same reason
    /// [`Self::create_child_element_by_enum`] exists. They take *several* calls in a row
    /// (the main chat window's talk-focus menu makes fourteen), so rather than a wrapper per
    /// entry point this hands the source out for the duration of one call.
    pub fn with_assets<T>(&self, f: impl FnOnce(&dyn AssetSource) -> T) -> T {
        f(self.assets.as_ref())
    }

    /// The general two-hop enum lookup `(enum_value, group, db_type)`,
    /// against this environment's asset source.
    ///
    /// The body is [`dereth_assets::did_by_enum`]; this is the `Env`-shaped wrapper, exactly as
    /// [`Self::create_and_add_root_element`] is for the client's root-element creation. It
    /// exists because an item slot's background tile is the enum lookup of `(index, 0x10000004
    /// UIIconBackgrounds, RenderSurface)` and a widget has no `AssetSource` of its own, for the
    /// same reason [`Self::create_child_element_by_enum`] does not: `Screen::create` is handed only
    /// a `&mut UiSystem`.
    ///
    /// `db_type` is **not** a parameter, because the mapper is keyed by group and value alone; the
    /// type is the client's own assertion about what it expects back and there is nothing to assert
    /// it against here. `None` is a missing group, a missing value, or the mapper's zero row.
    ///
    /// **It memoises, because retail's lookup goes through the dat cache and this does not.**
    /// Every lookup is two dat reads and two hash-table decodes, and an item slot asks one per
    /// filled cell per changed frame — a hundred of them for one inventory move. The client pays
    /// that once: its DID-from-enum cache reads the two mappers out of a cache that is
    /// already resident. The memo is that cache's stand-in and nothing more; it is keyed by
    /// `(group, value)`, it remembers a `None`, and it lives inside this `Env` so a test that swaps
    /// the environment is not answered from the previous one's dats.
    #[must_use]
    pub fn did_by_enum(&self, group: u32, value: u32) -> Option<dereth_primitives::DataId> {
        if let Some(hit) = self.enum_dids.borrow().get(&(group, value)).copied() {
            return hit;
        }
        let got = dereth_assets::did_by_enum(self.assets.as_ref(), group, value);
        self.enum_dids.borrow_mut().insert((group, value), got);
        got
    }
}

/// The error a missing environment answers with.
///
/// A missing environment is [`UiError::Persist`] rather than a panic, because a screen that is
/// constructed before the dat is open is a host bug that should be reported, not a crash.
pub(crate) fn no_env() -> UiError {
    UiError::Persist("no asset source installed; call UiSystem::set_env".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: track spec §8 — "a missing entry is `UiError::UnresolvedLayoutEnum`, which is loud on
    /// purpose". The same applies one level up: with no environment at all, a screen must fail
    /// rather than silently build nothing.
    #[test]
    fn constructing_with_no_environment_is_a_loud_error() {
        let ui = UiSystem::new((800, 600));
        assert!(ui.env().is_none());
        assert!(matches!(ui.require_env(), Err(UiError::Persist(_))));
    }
}
