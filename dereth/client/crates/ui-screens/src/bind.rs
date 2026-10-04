//! Child binding: the post-init pattern every panel in this crate shares.
//!
//! Every panel's post-init in the retail client is the same three steps repeated: find the child by
//! id, downcast, store in a named field. The recursive child search is depth-first
//! search of the subtree, and a miss leaves the field null — which the client then dereferences,
//! so a miss is a crash there and must be a **failure** here: a null child handle is a failure,
//! not a warning.

use dereth_ui::{ElemHandle, ElementId, UiSystem};

/// One documented child binding: the field name the panel stores it in and the element id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ChildBinding {
    /// The binding key the panel looks the child up by (e.g. the health meter's key).
    pub field: &'static str,
    pub id: ElementId,
    /// The element the post-init searches **inside**, when it does not search the panel root.
    ///
    /// Most post-inits are one flat run of recursive child lookups off the panel, and those leave this
    /// `None`. A few do the lookup in two steps — bind a child, then search *it* — and guard the
    /// second step on the first having succeeded. The spellcasting panel's post-init is the
    /// worked example: it skips `0x10000452`, `0x10000453` and
    /// `0x10000454` entirely when the endowment icon `0x100000B1` is absent, because all three are
    /// looked up inside the endowment icon.
    ///
    /// Recording the parent is not cosmetic: the recursive child search is depth-first over the whole
    /// subtree, so a flat table finds the same handles **only while the id is unique in the
    /// panel**, and the `0x1000_xxxx` namespace is shared by element ids, attribute ids and state
    /// ids. It also records where in the tree a child *is*, not just its id.
    pub parent: Option<ElementId>,
}

/// Shorthand for a [`ChildBinding`] table entry the panel's own post-init looks up off the root.
#[must_use]
pub const fn child(field: &'static str, id: u32) -> ChildBinding {
    ChildBinding {
        field,
        id: ElementId(id),
        parent: None,
    }
}

/// Shorthand for a [`ChildBinding`] the post-init looks up **inside another bound child**.
/// See [`ChildBinding::parent`].
#[must_use]
pub const fn child_under(parent: u32, field: &'static str, id: u32) -> ChildBinding {
    ChildBinding {
        field,
        id: ElementId(id),
        parent: Some(ElementId(parent)),
    }
}

/// The result of resolving a whole binding table against a live subtree.
#[derive(Debug, Clone, Default)]
pub struct Bound {
    /// `field -> handle`, in table order.
    pub found: Vec<(&'static str, ElemHandle)>,
    /// The bindings whose recursive child search found nothing.
    pub missing: Vec<ChildBinding>,
}

impl Bound {
    #[must_use]
    pub fn get(&self, field: &str) -> Option<ElemHandle> {
        self.found
            .iter()
            .find(|(f, _)| *f == field)
            .map(|(_, h)| *h)
    }

    #[must_use]
    pub fn is_complete(&self) -> bool {
        self.missing.is_empty()
    }
}

/// The post-init's body: search for each documented id recursively off `root`.
///
/// The root itself is checked too, because several classes bind a child id that is the element they
/// are attached to (the panel pages do this).
#[must_use]
pub fn bind_children(ui: &UiSystem, root: ElemHandle, table: &[ChildBinding]) -> Bound {
    let mut out = Bound::default();
    for b in table {
        // A binding with a `parent` is the post-init's two-step lookup: bind the
        // parent first, then search *it*. A missing parent skips the child rather than falling
        // back to the root, which is what the spellcasting panel's post-init does.
        let from = match b.parent {
            None => Some(root),
            Some(p) => {
                let p_self = ui.node(root).is_some_and(|n| n.element_id() == p);
                if p_self {
                    Some(root)
                } else {
                    ui.get_child_recursive(root, p)
                }
            }
        };
        let h = from.and_then(|from| {
            let self_match = ui.node(from).is_some_and(|n| n.element_id() == b.id);
            if self_match {
                Some(from)
            } else {
                ui.get_child_recursive(from, b.id)
            }
        });
        match h {
            Some(h) => out.found.push((b.field, h)),
            None => out.missing.push(*b),
        }
    }
    out
}

/// Read one enum attribute of an element, through the merged property set.
///
/// The value a panel reads is the three-collection
/// merge (element base, current state, instance overrides), never the raw description.
#[must_use]
pub fn attr_enum(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<u32> {
    ui.node(h)?.merged_properties().get_enum(id)
}

/// Read one integer attribute.
#[must_use]
pub fn attr_int(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<i32> {
    ui.node(h)?.merged_properties().get_int(id)
}

/// Read one float attribute.
#[must_use]
pub fn attr_float(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<f32> {
    ui.node(h)?.merged_properties().get_float(id)
}

/// One integer member of a **struct-valued** attribute.
///
/// A `BasePropertyType::Struct` attribute carries a
/// nested `PropertyCollection`; a caller that wants one member reads the outer attribute and then
/// the inner property name out of it. The radar panel's center point is the
/// one place in this crate that needs it — see [`crate::screens::gameplay::RadarChildren`].
#[must_use]
pub fn attr_struct_int(ui: &UiSystem, h: ElemHandle, outer: u32, inner: u32) -> Option<i32> {
    let props = ui.node(h)?.merged_properties();
    let dereth_assets::ui::PropertyValue::Struct(members) = props.get(outer)? else {
        return None;
    };
    let (_, p) = members.iter().find(|(k, _)| *k == inner)?;
    match &p.value {
        dereth_assets::ui::PropertyValue::Integer(v) => Some(*v),
        dereth_assets::ui::PropertyValue::Float(v) => Some(dereth_primitives::num::to_i32(*v)),
        _ => None,
    }
}

/// Read one data-id attribute.
#[must_use]
pub fn attr_data_id(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<dereth_primitives::DataId> {
    ui.node(h)?.merged_properties().get_data_id(id)
}

/// Read one bool attribute.
#[must_use]
pub fn attr_bool(ui: &UiSystem, h: ElemHandle, id: u32) -> Option<bool> {
    ui.node(h)?.merged_properties().get_bool(id)
}

/// Set a float attribute — write an instance property and run the element's
/// [`UiSystem::on_set_attribute`], which is what the client's setter does.
pub fn set_attr_float(ui: &mut UiSystem, h: ElemHandle, id: u32, v: f32) {
    let value = dereth_assets::ui::PropertyValue::Float(v);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, value.clone());
    }
    ui.on_set_attribute(h, id, Some(&value));
}

/// Set a bool attribute.
pub fn set_attr_bool(ui: &mut UiSystem, h: ElemHandle, id: u32, v: bool) {
    let value = dereth_assets::ui::PropertyValue::Bool(v);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, value.clone());
    }
    ui.on_set_attribute(h, id, Some(&value));
}

/// Set an enum attribute.
pub fn set_attr_enum(ui: &mut UiSystem, h: ElemHandle, id: u32, v: u32) {
    let value = dereth_assets::ui::PropertyValue::Enum(v);
    if let Some(n) = ui.node_mut(h) {
        n.instance_properties.set(id, value.clone());
    }
    ui.on_set_attribute(h, id, Some(&value));
}

/// The meter fill level, attribute **`0x69`**, a float in 0…1.
///
/// The meter fill attribute is `0x69`; the marker/notch attribute is `0x85`; the slider position
/// attribute is `0x86`; the data-patch meters use `0x66`.
pub mod attr {
    /// `0x66` — the data-patch meters' level (`DataPatchScreen`).
    pub const PATCH_METER_LEVEL: u32 = 0x66;
    /// `0x69` — every other meter's fill level.
    pub const METER_LEVEL: u32 = 0x69;
    /// `0x85` — the "desired power" notch on the combat bar.
    pub const MARKER: u32 = 0x85;
    /// `0x86` — a scrollbar/slider position.
    pub const SLIDER_POSITION: u32 = 0x86;
    /// `0x0E` — a check box's checked state.
    pub const CHECKED: u32 = 0x0E;
    /// `0x0D` — a button's disabled state (the burden indicator).
    pub const DISABLED: u32 = 0x0D;
    /// `0xB1` — the media state an element should switch to.
    pub const MEDIA_STATE: u32 = 0xB1;
    /// `0x3B` — **`UICore_Element_hide`**, so visibility is `!value`: `true` means *hidden*. See
    /// [`dereth_ui::props::attr::HIDE`] for the four facts that fix the sign.
    pub const HIDE: u32 = 0x3B;

    /// `0x10000029` — the **panel id**. Both a toolbar button and a panel page carry it, and it is
    /// the only association between them.
    pub const PANEL_ID: u32 = 0x1000_0029;
    /// `0x10000049` — "this page is a transient overlay", the flag that decides whether the page it
    /// covers becomes the previously shown panel.
    pub const TRANSIENT_OVERLAY: u32 = 0x1000_0049;
    /// `0x1000007E` — the per-window id used for the server-side position/filter/opacity blob.
    pub const WINDOW_ID: u32 = 0x1000_007E;
    /// `0x1000007F` — the per-window 64-bit chat text filter.
    pub const CHAT_FILTER: u32 = 0x1000_007F;
    /// `0x10000080` / `0x10000081` — the per-window default (idle) and active opacity.
    pub const DEFAULT_OPACITY: u32 = 0x1000_0080;
    /// See [`DEFAULT_OPACITY`].
    pub const ACTIVE_OPACITY: u32 = 0x1000_0081;
    /// `0x10000086` / `0x10000087` — the per-window saved X and Y.
    pub const WINDOW_X: u32 = 0x1000_0086;
    /// See [`WINDOW_X`].
    pub const WINDOW_Y: u32 = 0x1000_0087;
    /// `0x10000040` — a floaty chat window's title.
    pub const WINDOW_TITLE: u32 = 0x1000_0040;

    /// `0x3C`–`0x3F` — the panel resize's four clamps.
    ///
    /// **The id order carries no information about direction.** Read in ascending order the ids
    /// look like *"min width, min height, max width, max height"* — the exact reverse of what the
    /// client does. Only the comparison decides, and three code paths agree on it exactly: the
    /// element base's resize, the panel's resize (the same body, one class down) and the
    /// mouse-driven resize.
    ///
    /// ```text
    /// 0x3C: height larger than v is lowered to v   -> MAX height
    /// 0x3E: height smaller than v is raised to v   -> MIN height
    /// 0x3D: width larger than v is lowered to v    -> MAX width
    /// 0x3F: width smaller than v is raised to v    -> MIN width
    /// ```
    ///
    /// `panels::panel_stack::size_clamps` reads these four names, so reversing them makes every
    /// panel's minimum its maximum and its width clamp its height clamp. See
    /// [`dereth_ui::props::attr::MIN_WIDTH`], which carries the same block.
    pub const MAX_HEIGHT: u32 = 0x3C;
    /// See [`MAX_HEIGHT`].
    pub const MAX_WIDTH: u32 = 0x3D;
    /// See [`MAX_HEIGHT`].
    pub const MIN_HEIGHT: u32 = 0x3E;
    /// See [`MAX_HEIGHT`].
    pub const MIN_WIDTH: u32 = 0x3F;
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Oracle: the HUD behavior's closing paragraph, which names all four meter attributes
    /// in one sentence, and the toolbar and panel behavior, which names the panel-id attribute.
    #[test]
    fn the_named_attributes_are_the_documented_numbers() {
        assert_eq!(attr::PATCH_METER_LEVEL, 0x66);
        assert_eq!(attr::METER_LEVEL, 0x69);
        assert_eq!(attr::MARKER, 0x85);
        assert_eq!(attr::SLIDER_POSITION, 0x86);
        assert_eq!(attr::PANEL_ID, 0x1000_0029);
        assert_eq!(attr::TRANSIENT_OVERLAY, 0x1000_0049);
        assert_eq!(attr::WINDOW_ID, 0x1000_007E);
    }

    /// A nested binding records the element post init searches inside.
    #[test]
    fn a_nested_binding_records_the_element_post_init_searches_inside() {
        let flat = child("spell_name", 0x1000_048B);
        assert_eq!(flat.parent, None);
        assert_eq!(flat.id, ElementId(0x1000_048B));
        let nested = child_under(0x1000_00B1, "endowment_icon_underlay", 0x1000_0452);
        assert_eq!(nested.parent, Some(ElementId(0x1000_00B1)));
        assert_eq!(nested.id, ElementId(0x1000_0452));
        assert_eq!(nested.field, "endowment_icon_underlay");
    }
}
