//! Instantiation: the type-id → constructor registry and the recursive builder.
//!
//! The factory table and recursive builder are modeled here.
//!

use dereth_primitives::{AssetSource, DataId};

use crate::desc::{ElementDesc, LayoutDesc, StateDesc};
use crate::element::{Element, ElementNode, PlainElement};
use crate::layout::{EdgeMode, Edges};
use crate::msg::element::id as msgid;
use crate::region::Box2D;
use crate::{DroppedSubtree, ElemHandle, ElementId, ElementType, UiError, UiSystem};

/// Every concrete element type has a constructor of this shape and a
/// registration step that installs it.
pub type ElementCtor = fn(&LayoutDesc, &ElementDesc) -> Box<dyn Element>;

/// The engine element type ids used by UI layouts.
pub mod ty {
    use crate::ElementType;
    pub const BUTTON: ElementType = ElementType(1);
    pub const DRAGBAR: ElementType = ElementType(2);
    pub const FIELD: ElementType = ElementType(3);
    pub const LISTBOX: ElementType = ElementType(5);
    pub const MENU: ElementType = ElementType(6);
    pub const METER: ElementType = ElementType(7);
    pub const PANEL: ElementType = ElementType(8);
    pub const RESIZEBAR: ElementType = ElementType(9);
    /// The scrollable element is an **abstract base**: it identifies its type but is not registered.
    pub const SCROLLABLE: ElementType = ElementType(0x0A);
    pub const SCROLLBAR: ElementType = ElementType(0x0B);
    pub const TEXT: ElementType = ElementType(0x0C);
    pub const VIEWPORT: ElementType = ElementType(0x0D);
    /// Loads `Trowser.dll`, which is **not present in the shipped install**, so the element is inert.
    pub const BROWSER: ElementType = ElementType(0x0E);
    pub const COLOR_PICKER: ElementType = ElementType(0x10);
    pub const GROUP_BOX: ElementType = ElementType(0x11);
    pub const CONFIRMATION_DIALOG: ElementType = ElementType(0x13);
    pub const CONFIRMATION_MENU_DIALOG: ElementType = ElementType(0x14);
    pub const CONFIRMATION_TEXT_INPUT_DIALOG: ElementType = ElementType(0x15);
    pub const MENU_DIALOG: ElementType = ElementType(0x16);
    pub const MESSAGE_DIALOG: ElementType = ElementType(0x17);
    pub const TEXT_INPUT_DIALOG: ElementType = ElementType(0x18);
    pub const WAIT_DIALOG: ElementType = ElementType(0x19);

    /// The four ids with **no** registration in the client. They stay
    /// unregistered here too, exactly as the client leaves them, so that a shipped layout
    /// referencing one produces the same silent subtree drop (which [`crate::UiSystem::dropped`]
    /// records).
    pub const UNREGISTERED: [ElementType; 4] = [
        ElementType(4),
        SCROLLABLE,
        ElementType(0x0F),
        ElementType(0x12),
    ];
}

impl UiSystem {
    /// Register an element class — the element's own registration call, which forwards to the
    /// manager's.
    ///
    /// Registration is **global and permanent**: the table is never cleared between UI modes.
    pub fn register_element_class(&mut self, ty: ElementType, ctor: ElementCtor) {
        self.factories.insert(ty, ctor);
    }

    #[must_use]
    pub fn is_registered(&self, ty: ElementType) -> bool {
        self.factories.contains_key(&ty)
    }

    #[must_use]
    pub fn registered_types(&self) -> Vec<ElementType> {
        self.factories.keys().copied().collect()
    }

    /// Create a hollow element.
    ///
    /// The two singleton descriptions used for elements with no dat backing:
    ///
    /// * the hollow `LayoutDesc` takes the current display size, falling back to **800 × 600** when
    ///   there is no device yet;
    /// * the hollow `ElementDesc` has element id `8`, type `3` (the field type),
    ///   `width`/`height` = the display size with incorporation bits 8 and 0x10 set, and **all
    ///   four edge modes = 1** — left/top pinned, right/bottom following the display size.
    pub fn create_hollow(&mut self, parent: Option<ElemHandle>) -> ElemHandle {
        let (w, h) = if self.display().0 > 0 && self.display().1 > 0 {
            self.display()
        } else {
            (800, 600)
        };
        let desc = ElementDesc {
            base: StateDesc {
                incorporation: crate::desc::incorporation::WIDTH
                    | crate::desc::incorporation::HEIGHT,
                width: w,
                height: h,
                ..StateDesc::default()
            },
            element_id: ElementId(8),
            ty: ty::FIELD,
            edges: Edges {
                left: EdgeMode::AnchorStart,
                top: EdgeMode::AnchorStart,
                right: EdgeMode::AnchorStart,
                bottom: EdgeMode::AnchorStart,
            },
            ..ElementDesc::default()
        };
        let layout = LayoutDesc {
            did: DataId(0),
            display_width: w,
            display_height: h,
            ..LayoutDesc::default()
        };
        let node = ElementNode::new(
            DataId(0),
            layout.design_box(),
            desc,
            Box::new(PlainElement) as Box<dyn Element>,
        );
        let handle = self.alloc(node);
        if let Some(p) = parent {
            self.set_parent(handle, Some(p));
        }
        handle
    }

    /// Create a root element by data id: load the `LayoutDesc`, pick the
    /// `ElementDesc` whose id is `element`, build the tree and mark it as a root element.
    pub fn create_root_by_data_id(
        &mut self,
        assets: &dyn AssetSource,
        did: DataId,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        let h = self.create_from_data_id(assets, did, element)?;
        // Creating a root by data id goes through the manager's
        // root-element creation, which is where a **layout root** gets
        // its UI object: it asks for one and then marks the element a root. Without this
        // nothing in the tree ever has
        // the object-ownership bit, the current-object-mode walk falls off
        // the top for every element, and attribute `0xCD` has no observable effect at all.
        // The order matters: the ownership setter re-reads `0xCD`, so an authored `0` on a root
        // still clears the bit.
        self.set_should_own_object(h, true);
        if let Some(n) = self.node_mut(h) {
            n.flags.set_is_root_element(true);
        }
        let root = self.root();
        self.set_parent(h, Some(root));
        Ok(h)
    }

    /// Create a child element by data id.
    pub fn create_child_by_data_id(
        &mut self,
        assets: &dyn AssetSource,
        parent: ElemHandle,
        did: DataId,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        let h = self.create_from_data_id(assets, did, element)?;
        self.set_parent(h, Some(parent));
        Ok(h)
    }

    fn create_from_data_id(
        &mut self,
        assets: &dyn AssetSource,
        did: DataId,
        element: ElementId,
    ) -> Result<ElemHandle, UiError> {
        if !self.lib.contains(did) {
            let l = LayoutDesc::load(assets, did, &self.property_types)?;
            self.lib.insert(l);
        }
        let layout = self
            .lib
            .get(did)
            .cloned()
            .ok_or(UiError::MissingRoot { did, element })?;
        let desc = layout
            .access_element(element)
            .cloned()
            .ok_or(UiError::MissingRoot { did, element })?;
        self.create_element(assets, &layout, &desc)?
            .ok_or(UiError::UnknownElementType {
                ty: desc.ty,
                engine_ty: desc.engine_ty,
            })
    }

    /// The dispatcher: a full desc goes straight to
    /// the recursive builder, a partial one is resolved with `inq_full_desc` first
    /// by the partial-desc builder.
    pub fn create_element(
        &mut self,
        assets: &dyn AssetSource,
        layout: &LayoutDesc,
        desc: &ElementDesc,
    ) -> Result<Option<ElemHandle>, UiError> {
        if desc.is_partial() {
            let mut lib = std::mem::take(&mut self.lib);
            let resolved =
                layout.inq_full_desc_cached(assets, &self.property_types, &mut lib, desc);
            self.lib = lib;
            let Ok(resolved) = resolved else {
                // The partial-desc builder resolves the partial first, and a
                // failed resolution means it returns nothing: the subtree is dropped,
                // exactly as an unknown type is. The shipped data has one such node — layout
                // 0x21000040 element 0x100002C2 names base element 0x10000480, which exists in no
                // layout — so this is a live path, not a defensive one.
                self.dropped.push(DroppedSubtree {
                    element_id: desc.element_id,
                    ty: desc.ty,
                    engine_ty: desc.engine_ty,
                    descendants: desc.subtree_len(),
                });
                return Ok(None);
            };
            self.create_element_recursive_from_full_desc(assets, layout, &resolved)
        } else {
            self.create_element_recursive_from_full_desc(assets, layout, desc)
        }
    }

    /// The recursive builder from a full description, the six documented steps.
    ///
    /// Step 1 is the silent drop: if neither the type nor the engine type resolves in the factory
    /// table the function returns nothing and **the whole subtree goes with it**. Every drop is
    /// recorded in [`UiSystem::dropped`], because a missing widget otherwise looks like
    /// a layout bug.
    ///
    /// Step 5 iterates the desc's children in hash order, **not** read order. Although
    /// `add_child` sorts by z-level on every insert, not every deterministic order here is
    /// equivalent: `add_child`'s comparison is z-level then
    /// read order, and two siblings that tie on both -- the skills footer's meter
    /// `0x10000247` and its label `0x10000242` -- keep their *creation* order, which the
    /// bucket-by-bucket walk reproduces. Walking the map's key
    /// order would create the label first and paint the meter's opaque trough over it.
    pub fn create_element_recursive_from_full_desc(
        &mut self,
        assets: &dyn AssetSource,
        layout: &LayoutDesc,
        desc: &ElementDesc,
    ) -> Result<Option<ElemHandle>, UiError> {
        // 1. the factory lookup, with the engine type as the fallback.
        let ctor = self.factories.get(&desc.ty).copied().or_else(|| {
            if desc.engine_ty.0 != 0 {
                self.factories.get(&desc.engine_ty).copied()
            } else {
                None
            }
        });
        let Some(ctor) = ctor else {
            self.dropped.push(DroppedSubtree {
                element_id: desc.element_id,
                ty: desc.ty,
                engine_ty: desc.engine_ty,
                descendants: desc.subtree_len(),
            });
            return Ok(None);
        };

        // 2. call the factory; 3. append to the element list (done by `alloc`).
        let behaviour = ctor(layout, desc);
        let node = ElementNode::new(layout.did, layout.design_box(), desc.clone(), behaviour);
        let h = self.alloc(node);

        // 4. the automatic "make the element hittable because someone wants its clicks" rule.
        let id = desc.element_id;
        if msgid::AUTO_MOUSE_VISIBLE
            .iter()
            .any(|m| self.element_listeners.contains_key(&(id, *m)))
        {
            self.set_mouse_visible(h, true);
        }

        // 5. recurse into the children and re-parent them, in the child table's own hash order.
        for id in desc.creation_order() {
            let Some(child) = desc.children.get(&id) else {
                continue;
            };
            if let Some(c) = self.create_element(assets, layout, child)? {
                self.set_parent(c, Some(h));
            }
        }

        // 6. return the element. Initialize/post-init are the owning framework's job.
        Ok(Some(h))
    }
}

fn plain(_l: &LayoutDesc, _d: &ElementDesc) -> Box<dyn Element> {
    Box::new(PlainElement)
}

/// The element manager's init, step 4, in the client's exact registration order.
///
/// Register fourteen widget types and seven dialog types. An earlier summary counted fifteen
/// generic widgets, but the actual enumeration has fourteen.
///
/// The browser type (0x0E) **is** registered here even though `Trowser.dll` is absent from the
/// shipped install: the client registers it, so a layout referencing it gets a real (inert) element
/// rather than a dropped subtree, and element counts match. Omitting it would be observable only
/// as a missing subtree, so registering an inert `PlainElement` is the closer behaviour.
pub fn register_engine_classes(s: &mut UiSystem) {
    use crate::widgets;
    s.register_element_class(ty::FIELD, widgets::field::create);
    s.register_element_class(ty::BUTTON, widgets::button::create);
    s.register_element_class(ty::TEXT, crate::text::element_text::create);
    s.register_element_class(ty::SCROLLBAR, widgets::scrollbar::create);
    s.register_element_class(ty::METER, widgets::meter::create);
    s.register_element_class(ty::LISTBOX, widgets::listbox::create);
    s.register_element_class(ty::MENU, widgets::menu::create);
    s.register_element_class(ty::DRAGBAR, widgets::dragbar::create);
    s.register_element_class(ty::PANEL, widgets::panel::create);
    s.register_element_class(ty::VIEWPORT, widgets::viewport::create);
    s.register_element_class(ty::RESIZEBAR, widgets::resizebar::create);
    s.register_element_class(ty::BROWSER, plain);
    s.register_element_class(ty::COLOR_PICKER, widgets::colorpicker::create);
    s.register_element_class(ty::GROUP_BOX, widgets::groupbox::create);
    for (t, kind) in [
        (
            ty::CONFIRMATION_DIALOG,
            crate::dialog::DialogKind::Confirmation,
        ),
        (
            ty::CONFIRMATION_MENU_DIALOG,
            crate::dialog::DialogKind::ConfirmationMenu,
        ),
        (
            ty::CONFIRMATION_TEXT_INPUT_DIALOG,
            crate::dialog::DialogKind::ConfirmationTextInput,
        ),
        (ty::MENU_DIALOG, crate::dialog::DialogKind::Menu),
        (ty::MESSAGE_DIALOG, crate::dialog::DialogKind::Message),
        (ty::TEXT_INPUT_DIALOG, crate::dialog::DialogKind::TextInput),
        (ty::WAIT_DIALOG, crate::dialog::DialogKind::Wait),
    ] {
        s.register_element_class(t, crate::dialog::types::ctor_for(kind));
    }
}

/// The hollow layout's design box, exposed for tests: 800 × 600 when there is no device.
#[must_use]
pub fn hollow_fallback_box() -> Box2D {
    Box2D::new(0, 0, 799, 599)
}
