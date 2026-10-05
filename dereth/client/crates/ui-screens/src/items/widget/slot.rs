//! One item slot: the `UiItemWidget` subtree that shows one object.

use super::*;

/// The two decoration children **the layout does not hide by itself** — see [`ItemSlot::rest`].
///
/// The other sixteen children — the ghost, the selection
/// ring, the stack count, the shortcut numeral, the sell and trade markers, the open-container
/// frame and all ten cooldown wedges — each carry `0x3B = true` in the shipped `ItemSlot` layout
/// (`0x21000037`, under overlay container `0x10000346`), and `0x3B` is `UICore_Element_hide`, so
/// **the layout hides them and no code needs to**. Hiding them here as well would only be right
/// if `0x3B` were read the other way up.
///
/// These two are different: the capacity bar (`0x10000347`) and
/// the structure bar (`0x10000348`) are the only two children of `0x10000346` that carry
/// **no** `0x3B` at all, so the layout brings them up visible under either reading. In the client
/// they are taken down by the capacity-display and structure-display updates,
/// neither of which this crate implements — both need the object's items capacity and
/// structure, which belong to the object model. "Hidden" is the answer both functions give for every object this
/// build can describe, so it is applied once at creation and named for what it stands in for.
const QUIET_AT_REST: [u32; 2] = [child::CAPACITY_BAR, child::STRUCTURE_BAR];

/// One slot: a `UiItemWidget` subtree created from the `ItemSlot` layout.
#[derive(Debug, Clone)]
pub struct ItemSlot {
    pub handle: ElemHandle,
    /// The base icon — [`child::ICON`], the element `Self::set_state` acts on.
    pub icon: Option<ElemHandle>,
    /// The ghost.
    pub ghosted: Option<ElemHandle>,
    /// The selection ring.
    pub selected_ring: Option<ElemHandle>,
    /// The stack count.
    pub quantity: Option<ElemHandle>,
    /// The capacity bar — a `Meter`.
    pub capacity_bar: Option<ElemHandle>,
    /// The structure bar — ditto.
    pub structure_bar: Option<ElemHandle>,
    /// The overlay container every decoration below hangs off.
    pub overlays: Option<ElemHandle>,
    /// The text element.
    pub text: Option<ElemHandle>,
    /// The shortcut numeral — the plate [`Self::set_shortcut_num`] writes.
    pub shortcut_num_elem: Option<ElemHandle>,
    /// The drag icon — the hidden 32×32 element `Self::post_init` **creates** (it is not in the
    /// slot's own subtree) and the drag paints the item onto.
    pub drag_icon: Option<ElemHandle>,
    /// **The drop hint**: the child `0x1000045A`.
    ///
    /// It is bound here and driven by [`Self::set_drag_accept_state`]; it is the only thing the
    /// retail client changes while an item is being dragged over a slot. See
    /// [`ItemListWidget::drag_over`].
    pub drag_accept: Option<ElemHandle>,
    /// The drop hint's state, mirrored.
    ///
    /// The client reads the element's own state to skip a
    /// redundant state write; `dereth_ui` has a setter and no getter, so the last value written is
    /// kept here. `None` is "never set", which is not the same as any of the four states.
    pub drag_accept_state: Option<StateId>,
    /// The decorations the client hides whenever it has nothing to say with them. See
    /// [`ItemSlot::rest`].
    quiet: Vec<ElemHandle>,
    /// The item id.
    pub item: Option<ObjectId>,
    /// The spell id. Setting it clears the item id, so a slot
    /// holds **either** an object or a spell and never both. The spellbook uses it: it is the
    /// same `ItemListWidget` with spells in it.
    pub spell: Option<u32>,
    /// `waiting` — the waiting state, the ghost an outstanding move leaves.
    pub waiting: bool,
    /// The slot number this item's numeral was last set for, `-1` for none.
    pub shortcut_num: i32,
    /// Whether that numeral was last set ghosted.
    pub shortcut_ghosted: bool,
    /// Construction sets the slot's quantity to **-1**, and no other client path writes it. Thus
    /// the quantity refresh hides the stack count on every inventory,
    /// quickbar, trade and spellbook slot, and a stack's count reaches the player through
    /// [`Self::tooltip`] instead.
    pub quantity_value: i32,
    /// Whether the object is a container — the client's three-term test.
    pub is_container: bool,
    /// The object's items capacity, mirrored off the same weenie read
    /// [`Self::update_capacity_display`] takes it from.
    ///
    /// It exists so that [`ItemListWidget::drag_over`]'s container branch can be answered from
    /// inside an element-message handler. The client asks the weenie store directly
    /// for the slot's item; this crate has
    /// no store and the `Screen` seam carries no [`crate::view::GameView`] on a message, so the
    /// two fields that answer it ride on the slot. See [`Self::num_empty_item_slots`].
    pub items_capacity: i32,
    /// The number of **loose** items in the object's inventory record.
    /// Mirrored for the same reason as [`Self::items_capacity`].
    pub contained_items: i32,
    /// The tooltip text [`Self::update_tooltip`] last set, mirrored so a test can read it back
    /// without a hover. `None` is a cleared tooltip.
    pub tooltip: Option<String>,

    // ---- the rest of the slot -----------------------------------
    /// The sell marker.
    pub sell_state_elem: Option<ElemHandle>,
    /// The trade marker — ditto.
    pub trade_state_elem: Option<ElemHandle>,
    /// The open-container frame.
    ///
    /// **Only one of the twenty `ItemSlot` roots carries it**: `0x1000033F`, the 36×36 root the two
    /// `UI_ItemList_IsContainer` lists (`0x100001C9` the main pack, `0x100001CA` the side-pack
    /// strip) name in `UI_ItemList_ItemSlotID`. The default 32×32 root `0x1000033A` — the backpack
    /// grid, the paper doll and every quickbar tile — has **no** `0x10000450` at all, so
    /// [`Self::set_open_container_state`] is a no-op on those and that is retail's behaviour, not a
    /// gap.
    pub open_container: Option<ElemHandle>,
    /// The ten cooldown wedges, in [`child::COOLDOWN`] order.
    pub cooldown: [Option<ElemHandle>; 10],
    /// `selected` — the copy the slot update compares against the object's own `selected`.
    pub selected: bool,
    /// Construction sets it to `true`, so a slot is selectable until the list says otherwise.
    pub selectable: bool,
    /// The sell state, stored as the client stores it: `(object sell state != 0)`.
    pub sell_state: bool,
    /// The trade state.
    pub trade_state: bool,
    /// The delayed shortcut number, `-1` for none — see [`Self::set_delayed_shortcut_num`].
    pub delayed_shortcut_num: i32,
    /// The object's cooldown id, cached off the last decoration pass so the heartbeat can
    /// re-run [`Self::update_cooldown_display`] without another snapshot. `0` is "no cooldown".
    pub cooldown_id: u32,
    /// The object's cooldown duration, likewise.
    pub cooldown_duration: f64,
    /// The last heartbeat time — the global-message handler's gate.
    pub last_heartbeat: f64,
    /// Which of the ten wedges [`Self::update_cooldown_display`] last left up, `None` for none.
    /// Mirrored so a test can read the state machine without going through the element tree.
    pub cooldown_wedge: Option<usize>,
    /// Whether the object can be opened — the slot update's own two-armed test.
    ///
    /// **Write-only in the shipped client too**: the flag is assigned in the constructor and in
    /// the slot update and read by nothing else, so no frame can falsify it. The
    /// open-container *frame* comes from the list's open item id, which is a different
    /// mechanism — see
    /// [`ItemListWidget::update_open_container_indicator`].
    pub is_openable: bool,
    /// Whether the object holds containers — a non-zero containers capacity. Write-only in the client for the same
    /// reason.
    pub is_container_holder: bool,
}

/// Construction sets the heartbeat interval to **1.0** (a double).
pub const HEARTBEAT_INTERVAL: f64 = 1.0;

/// The global message the item slot registers for and acts on.
pub const HEARTBEAT_MESSAGE: u32 = 3;

impl ItemSlot {
    pub(super) fn bind(ui: &UiSystem, handle: ElemHandle) -> Self {
        let get = |id: u32| ui.get_child_recursive(handle, ElementId(id));
        let mut quiet = Vec::new();
        for id in QUIET_AT_REST {
            if let Some(h) = get(id) {
                quiet.push(h);
            }
        }
        Self {
            handle,
            icon: get(child::ICON),
            ghosted: get(child::GHOSTED),
            selected_ring: get(child::SELECTED),
            quantity: get(child::QUANTITY),
            capacity_bar: get(child::CAPACITY_BAR),
            structure_bar: get(child::STRUCTURE_BAR),
            overlays: get(child::OVERLAYS),
            text: get(child::TEXT),
            shortcut_num_elem: get(child::SHORTCUT_NUM),
            drag_icon: None,
            drag_accept: get(child::DRAG_ACCEPT),
            drag_accept_state: None,
            quiet,
            item: None,
            spell: None,
            waiting: false,
            shortcut_num: -1,
            shortcut_ghosted: false,
            // The constructor's own value.
            quantity_value: -1,
            is_container: false,
            items_capacity: 0,
            contained_items: 0,
            tooltip: None,
            sell_state_elem: get(child::SELL_STATE),
            trade_state_elem: get(child::TRADE_STATE),
            open_container: get(child::OPEN_CONTAINER),
            cooldown: child::COOLDOWN.map(get),
            selected: false,
            selectable: true,
            sell_state: false,
            trade_state: false,
            delayed_shortcut_num: -1,
            cooldown_id: 0,
            cooldown_duration: 0.0,
            last_heartbeat: 0.0,
            cooldown_wedge: None,
            is_openable: false,
            is_container_holder: false,
        }
    }

    /// A slot with nothing bound — what [`Self::bind`] produces against an element with no
    /// children. The tests below build their synthetic slots from it so that adding a child to
    /// [`Self`] does not mean editing every one of them.
    #[must_use]
    pub fn bare(handle: ElemHandle) -> Self {
        Self {
            handle,
            icon: None,
            ghosted: None,
            selected_ring: None,
            quantity: None,
            capacity_bar: None,
            structure_bar: None,
            overlays: None,
            text: None,
            shortcut_num_elem: None,
            drag_icon: None,
            drag_accept: None,
            drag_accept_state: None,
            quiet: Vec::new(),
            item: None,
            spell: None,
            waiting: false,
            shortcut_num: -1,
            shortcut_ghosted: false,
            quantity_value: -1,
            is_container: false,
            items_capacity: 0,
            contained_items: 0,
            tooltip: None,
            sell_state_elem: None,
            trade_state_elem: None,
            open_container: None,
            cooldown: [None; 10],
            selected: false,
            selectable: true,
            sell_state: false,
            trade_state: false,
            delayed_shortcut_num: -1,
            cooldown_id: 0,
            cooldown_duration: 0.0,
            last_heartbeat: 0.0,
            cooldown_wedge: None,
            is_openable: false,
            is_container_holder: false,
        }
    }

    /// The **run-time** half of item-widget initialization, which is everything after
    /// the twenty-two child lookups [`Self::bind`] already makes:
    ///
    /// ```text
    /// create the drag icon 0x10000345 from the same ItemSlot layout (enum 0x10000038)
    /// hide the drag icon
    /// make the slot mouse-visible
    /// property 0x36 = true    // drop catcher
    /// property 0x3A = false   // not dragable
    /// property 0x39 = false   // may spawn a proxy
    /// register for global message 3
    /// ```
    ///
    /// **Without three of those lines nothing in the inventory can be touched.** List boxes have
    /// the folded hit-test predicate — `true` except in state `0x0D` — but an item
    /// slot (type `0x10000032`) is registered through
    /// [`crate::game_element`] like every other game type and therefore *has* no such override; it
    /// is mouse-visible because **slot initialisation says so explicitly**, and
    /// with that line missing, hit testing returns the panel behind the
    /// slot and every press misses: the grid `0x100001C6` and all 102 of its `0x1000033A` slots
    /// read `is_mouse_visible == false`.
    ///
    /// `0x36` is the other half: the drop **catcher** is the slot, not the list.
    /// The item-list element-message handler's `0x15` arm takes the source element, verifies that
    /// it is an item slot, and restores drag-accept state `0x1000003F`. Drop handling raises that message on
    /// the element that accepted the drop, so for the cast to ever succeed the accepting
    /// element must be the slot in this implementation.
    ///
    /// `drag_icon` is the element [`crate::env::create_child_element_by_enum`] just made; it is
    /// passed in rather than created here so this stays testable without an installed environment.
    pub(super) fn post_init(&mut self, ui: &mut UiSystem, drag_icon: Option<ElemHandle>) {
        self.drag_icon = drag_icon;
        if let Some(h) = drag_icon {
            ui.set_visible(h, false);
        }
        ui.set_mouse_visible(self.handle, true);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::DROP_CATCHER, true);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::DRAGABLE, false);
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::NO_DRAG_PROXY, false);
    }

    /// Paint the drag icon with what this slot holds and label it with what is being dragged.
    ///
    /// With no drag icon, or an empty slot, it returns false — an empty slot cannot be picked
    /// up. For an object it sets the drag icon to the object's drag surface (blit 3-alpha) and
    /// writes `0x1000000F` = item id, `0x10000011` = is-container, and `0x10000013` / `0x10000012`
    /// / `0x10000014` = the list's vendor / shortcut / salvage flags. For a spell it sets the
    /// spell icon, `0x10000010` = spell id and `0x10000012` = the shortcut flag.
    ///
    /// **The five properties are the payload, not decoration.** [`inq_drop_icon_info`] reads
    /// them straight back off the drag proxy, and that — not the drag's owner — is how
    /// the drop learns *which object* was dropped. They survive onto the
    /// proxy because the proxy copies the drag icon's instance properties.
    ///
    /// **The image is the drag surface, not the main surface.** The client paints the drag icon with the *first* of
    /// the two surfaces the icon renderer builds: icon, custom overlay and the effects colour
    /// replacement, with **no** item-type tile and **no** custom underlay. Those two belong to
    /// the main surface, the one the slot itself draws.
    ///
    /// The base icon's `GraphicRef` carries a whole [`dereth_ui::region::IconRecipe`], and cloning
    /// it wholesale would put the opaque `UIIconBackgrounds` tile under the cursor. The proxy takes
    /// [`IconRecipe::drag_surface`](dereth_ui::region::IconRecipe::drag_surface), which is the same
    /// recipe with those two layers removed and is the drag surface exactly.
    ///
    /// A plain (non-recipe) image is passed through unchanged: a spell tile and a slot whose
    /// picture was set by hand have no second surface in the client either.
    pub(super) fn prepare_drag_icon(&self, ui: &mut UiSystem, list: &ItemListWidget) -> bool {
        let Some(d) = self.drag_icon else {
            return false;
        };
        if self.item.is_none() && self.spell.is_none() {
            return false;
        }
        let mut image = self
            .icon
            .and_then(|h| ui.node(h).and_then(|n| n.region.image.clone()));
        if let Some(g) = image.as_mut() {
            if let Some(dereth_ui::region::SurfaceOp::Icon(r)) = g.op {
                let drag = r.drag_surface();
                g.op = Some(dereth_ui::region::SurfaceOp::Icon(drag));
                // `GraphicRef::did` is the recipe's base and is part of the host's texture key;
                // it has to follow the recipe or the drag proxy would share a cache slot with the
                // slot's own composite.
                g.did = drag.base().unwrap_or(g.did);
            }
        }
        if let Some(n) = ui.node_mut(d) {
            n.region.image = image;
            n.region.blit_mode = dereth_ui::region::BlitMode::Alpha3;
        }
        if let Some(id) = self.item {
            set_attr(ui, d, drag_attr::ITEM_ID, PropertyValue::InstanceId(id.0));
            set_attr(
                ui,
                d,
                drag_attr::IS_CONTAINER,
                PropertyValue::Bool(list.container_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_VENDOR,
                PropertyValue::Bool(list.vendor_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_SHORTCUT,
                PropertyValue::Bool(list.shortcut_list),
            );
            set_attr(
                ui,
                d,
                drag_attr::IS_SALVAGE,
                PropertyValue::Bool(list.salvage_list),
            );
        } else if let Some(id) = self.spell {
            set_attr(ui, d, drag_attr::SPELL_ID, PropertyValue::InstanceId(id));
            set_attr(
                ui,
                d,
                drag_attr::IS_SHORTCUT,
                PropertyValue::Bool(list.shortcut_list),
            );
        }
        true
    }

    /// The state goes on **three** children and not on the item element itself: the base icon
    /// (without which it returns false), the text and the overlay container. Setting the empty
    /// state `0x1000001C` also clears the tooltip flag, the container flag and `selected`, clears the
    /// tooltip, and hides the selection ring.
    ///
    /// The overlays child matters: `0x1000033C` is the parent of the capacity and structure bars,
    /// the ghost, the selection ring, the shortcut numeral, the sell and trade markers and the ten
    /// cooldown wedges, and it declares the same two states, each carrying attribute `0x3B` —
    /// `true` (hidden) in `0x1000001C` and `false` (shown) in `0x1000001D`. A rebuild that sets
    /// only the icon's state leaves the whole container up on an empty slot, and each slot draws
    /// as a pile of overlapping markers.
    pub(super) fn set_state(&mut self, ui: &mut UiSystem, s: StateId) -> bool {
        let Some(icon) = self.icon else { return false };
        ui.set_state(icon, s);
        if let Some(h) = self.text {
            ui.set_state(h, s);
        }
        if let Some(h) = self.overlays {
            // The overlays container declares `0x3B = true` in state `0x1000001C` and
            // `0x3B = false` in `0x1000001D`; `0x3B` is `UICore_Element_hide`, so this one
            // state write is what takes the decorations down on an empty slot and puts them back on
            // a full one. No hand correction of the sense belongs here: `0x3B` is read the right way
            // up at the source, and a second inversion would undo it.
            ui.set_state(h, s);
        }
        if s == item_state::EMPTY {
            // Clear `selected` and hide the selected ring.
            self.selected = false;
            if let Some(h) = self.selected_ring {
                ui.set_visible(h, false);
            }
        }
        true
    }

    /// The UI item element's shortcut-number write — put the slot's numeral on the shortcut
    /// numeral element, or take it down.
    ///
    /// A negative number hides the numeral. Otherwise it picks an array — `0x1000005E` when the
    /// base icon is in the empty state `0x1000001C`, else `0x10000043` when ghosted, else
    /// `0x10000042` — and, only if the numeral element carries that array, sets the numeral's
    /// image to entry `num` (draw mode 3) and shows it. It then records the number and the
    /// ghosted flag on the slot and, when the slot has an object, writes the pair back to it.
    ///
    /// Both the image and the show sit **inside** the array lookup, which is
    /// load-bearing: nothing in the shipped tree carries
    /// [`attr::SHORTCUT_OVERLAY_ARRAY_EMPTY`], so a call on an empty slot leaves the numeral
    /// exactly where it was. The last line — the write back to the weenie — belongs to the object
    /// model and has no place in this crate, which never writes objects; the slot number reaches a slot from
    /// [`crate::toolbar::shortcuts::ShortcutBar`] instead, which is where the client's own
    /// shortcut insert puts it.
    ///
    /// Returns true when the numeral element was written.
    pub fn set_shortcut_num(&mut self, ui: &mut UiSystem, num: i32, ghosted: bool) -> bool {
        self.shortcut_num = num;
        self.shortcut_ghosted = ghosted;
        let Some(h) = self.shortcut_num_elem else {
            return false;
        };
        if num < 0 {
            ui.set_visible(h, false);
            return true;
        }
        let empty = self
            .icon
            .and_then(|i| ui.node(i))
            .is_some_and(|n| n.state == item_state::EMPTY);
        let which = if empty {
            attr::SHORTCUT_OVERLAY_ARRAY_EMPTY
        } else if ghosted {
            attr::SHORTCUT_OVERLAY_ARRAY_GHOSTED
        } else {
            attr::SHORTCUT_OVERLAY_ARRAY
        };
        let Some(entries) = shortcut_overlay_array(ui, h, which) else {
            return false;
        };
        if let Some(did) = entries
            .get(usize::try_from(num).unwrap_or(usize::MAX))
            .copied()
        {
            // Replace the media machine with one image
            // step. The third argument is the draw mode, which this rebuild carries on the step and
            // does not use when painting.
            if let Some(n) = ui.node_mut(h) {
                n.region.image = Some(dereth_ui::GraphicRef::opaque_surface(did, 0, 0));
            }
        }
        ui.set_visible(h, true);
        true
    }

    /// The two hides that are still this crate's to make — see
    /// [`QUIET_AT_REST`], which says why the other sixteen are not.
    ///
    /// The client hides the capacity bar when the object is not a
    /// container or its items capacity is `< 1`, and does the
    /// same for the structure bar. It runs once, when the slot is created, because that is the
    /// answer both give for every object this build can describe.
    pub(super) fn rest(&self, ui: &mut UiSystem) {
        for h in &self.quiet {
            ui.set_visible(*h, false);
        }
    }

    /// The ui item element's set-icon — clear the image, set blit mode normal, and set the
    /// object's icon on the base icon.
    pub(super) fn set_icon(&self, ui: &mut UiSystem, did: Option<DataId>) {
        let Some(icon) = self.icon else { return };
        if let Some(n) = ui.node_mut(icon) {
            n.region.image = did.map(|d| dereth_ui::GraphicRef::world_surface(d, 0, 0));
        }
    }

    /// The client's set-icon on the base icon, with the composite the object's icon actually
    /// is rather than the raw icon id.
    ///
    /// The recipe travels on the element's own `GraphicRef` and the host composites it once per
    /// distinct recipe, which is where the icon data lives in the client: keyed by object id in
    /// the icon-data table and rebuilt only when one of the client's five compared fields has
    /// moved. Here the recipe **is** those
    /// five fields, so an unchanged object produces an equal recipe and an equal cache key, and a
    /// changed one produces a different key — the invalidation is the identity rather than a
    /// second mechanism that could disagree with it.
    ///
    /// `None`, and a `Some` whose recipe resolves no base at all, are the client's
    /// null texture: the element draws nothing. That is the client's answer too — the icon
    /// renderer leaves the main surface null when creating the local surface fails — and it is
    /// deliberately **not**
    /// "fall back to the raw icon id", because a silent fallback to a picture that is nearly
    /// right is how a composite that stopped resolving would go unnoticed.
    ///
    /// Returns true when the element's picture changed, so a caller can count it.
    pub fn set_icon_composite(
        &mut self,
        ui: &mut UiSystem,
        recipe: Option<dereth_ui::region::IconRecipe>,
    ) -> bool {
        let Some(icon) = self.icon else { return false };
        let want = recipe.and_then(|r| {
            let base = r.base()?;
            Some(dereth_ui::GraphicRef {
                did: base,
                source: dereth_ui::ImageSource::World,
                width: 0,
                height: 0,
                opaque: None,
                op: Some(dereth_ui::region::SurfaceOp::Icon(r)),
            })
        });
        let Some(n) = ui.node_mut(icon) else {
            return false;
        };
        if n.region.image == want {
            return false;
        }
        n.region.image = want;
        true
    }

    /// The recipe this slot's icon element is currently drawing, if it is a composite at all.
    #[must_use]
    pub fn icon_recipe(&self, ui: &UiSystem) -> Option<dereth_ui::region::IconRecipe> {
        ui.node(self.icon?)?
            .region
            .image
            .as_ref()?
            .op?
            .icon_recipe()
    }

    /// Clear the slot, plus the empty state the three callers always pair it with.
    ///
    /// **It deliberately does not clear the image.** Clearing a slot writes four fields and touches
    /// no element; what puts the empty-slot frame back is the state, because `0x1000033B`'s state
    /// `0x1000001C` carries **one media step** and its `0x1000001D` carries none. Clearing the
    /// image here instead would leave an empty slot as a hole — the shipped `ItemSlot` layout has
    /// no base image on the icon at all.
    pub(super) fn clear(&mut self, ui: &mut UiSystem) {
        self.item = None;
        // The client's clear writes four fields: the item id, the spell id and the container
        // display to zero, and the object reference to null.
        self.spell = None;
        super::super::runtime::set_identity(ui, self.handle, ObjectId(0), 0);
        self.waiting = false;
        self.set_state(ui, item_state::EMPTY);
        if let Some(h) = self.ghosted {
            ui.set_visible(h, false);
        }
        // The client's no-object arm: clear the tooltip flag and the tooltip, which setting the
        // empty state `0x1000001C` does again for the same reason.
        //
        // **Clearing the tooltip re-evaluates mouse visibility**, so this is the line
        // that would take an empty slot back out of the hit test if the slot's mouse-visibility
        // rested on its tooltip. It does not: slot initialisation sets the explicit
        // mouse-visible flag, and the visibility update ORs the two. See
        // [`ItemSlot::update_tooltip`].
        self.tooltip = None;
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::TOOLTIP_ON, false);
        ui.clear_tooltip(self.handle);
        // The two bars the layout does not hide by itself go back down with the item — see
        // [`QUIET_AT_REST`]. In the client the same thing happens one level up, because
        // setting the slot's state puts the overlay container (their grandparent) into `0x1000001C`,
        // whose `0x3B` is `true`.
        self.is_container = false;
        self.quantity_value = -1;
        // Clearing a slot nulls its object, which is what makes the cooldown
        // display's next pass take the `on = false` arm and put all ten wedges
        // down; here the weenie's two fields are cached on the slot, so dropping them is the same
        // fact. The sell and trade states are deliberately **not** reset — the client does
        // not reset them either, and a slot reused for another object compares against what it
        // last drew.
        self.cooldown_id = 0;
        self.cooldown_duration = 0.0;
        self.is_openable = false;
        self.is_container_holder = false;
        // Clearing a slot nulls its object, and a slot
        // with no object has no icon data, so there is no main surface and no background tile —
        // the cell falls back to the base icon's own `0x1000001C` frame.
        //
        // **There is no line here for that, and its absence is the point.** A tile on the
        // `UiItemWidget` root would need taking down by hand, because the slot's state write never
        // touches that element. The composite lives on the base icon, and
        // `set_state(EMPTY)` five lines above **is** `0x1000001C`'s single media step putting
        // `0x06004D20` back over it — which is exactly why clearing an item writes four fields and
        // touches no additional element. A GPU test opens a side pack and asserts every
        // composite comes down.
        self.rest(ui);
    }

    /// Spell-shortcut initialisation + the spell arm of the ui item update + the set-icon
    /// `else` branch.
    ///
    /// Initialisation clears the item id, sets the spell id, clears the container display and
    /// sets no object. The
    /// update, with no object and a spell, sets state `0x1000001D`, sets the icon and returns. The
    /// set-icon clears the base icon's image, sets blit mode **3-alpha** (not normal), sets the
    /// spell icon and, when the slot has a text element, writes the spell name into it.
    ///
    /// Two things separate this from [`Self::set_icon`]. A spell slot uses a **3-alpha blit**
    /// where an item slot uses a normal blit, because a spell icon is a three-channel alpha
    /// surface; and a spell slot is the only caller that writes the text element (`0x10000344`),
    /// which is why an item slot's text child stays empty and a spellbook row shows a name.
    ///
    /// The slot update's spell path also never runs the weenie half — no selection ring, no capacity
    /// or structure bar, no quantity, no cooldown — because it returns before all of it.
    ///
    /// **The spell-icon read is a composite too, and a different one.** It
    /// caches in a spell-icon table and builds a miss with
    /// the magic system's spell-icon composite, which is four surfaces out of
    /// `0x10000006 UISpellBackgrounds` and `0x10000007 UISpellOverlays` — **not**
    /// the raw icon on its own. See
    /// [`spell_recipe`]. The blit mode above stays 3-alpha because that is what the
    /// set-icon's else-branch uses, whatever the picture is.
    pub(super) fn set_spell(
        &mut self,
        ui: &mut UiSystem,
        id: u32,
        icon: Option<DataId>,
        name: &str,
        recipe: Option<dereth_ui::region::IconRecipe>,
    ) {
        self.item = None;
        self.spell = Some(id);
        super::super::runtime::set_identity(ui, self.handle, ObjectId(0), id);
        self.set_state(ui, item_state::OCCUPIED);
        if let Some(h) = self.icon {
            if let Some(n) = ui.node_mut(h) {
                n.region.blit_mode = dereth_ui::region::BlitMode::Alpha3;
                n.region.image = icon.map(|d| dereth_ui::GraphicRef::world_surface(d, 0, 0));
            }
        }
        // After the plain image write, because the two are one image write in the client and the
        // composite is what it passes. `IconRecipe::base` is `background.or(icon)`, so a recipe
        // whose `UISpellBackgrounds` row did not resolve still names the spell's own icon and
        // still composites to it: a 4-alpha blit onto a surface with alpha 0 is the source.
        self.set_icon_composite(ui, recipe);
        if let Some(t) = self.text.and_then(|h| ui.text_element_mut(h)) {
            t.set_text(name);
        }
    }

    /// Show the ghost. The client
    /// has an `unghostable` opt-out per item; nothing in this build sets it.
    pub(super) fn set_waiting(&mut self, ui: &mut UiSystem, waiting: bool) {
        self.waiting = waiting;
        if let Some(h) = self.ghosted {
            ui.set_visible(h, waiting);
        }
    }

    /// Set the slot's quantity and nothing else.
    ///
    /// **Its only caller in the whole client writes the amount a vendor advertises and `-1` when
    /// that is below 1.** Nothing on the
    /// inventory path calls it, so [`Self::update_quantity_display`] hides the text on every
    /// backpack, paper-doll, quickbar and spellbook slot — which is why the recorded retail
    /// backpack shows **no numbers on its icons**, and
    /// why a rebuild that painted the stack size there would look wrong against retail rather than
    /// right.
    pub const fn set_quantity(&mut self, n: i32) {
        self.quantity_value = n;
    }

    /// The ui item element's quantity display update.
    ///
    /// A missing quantity element returns immediately. A negative signed quantity hides it;
    /// otherwise the element receives the decimal quantity text and becomes visible.
    ///
    /// The format is `"%d"` — the plain number, no separators and no brackets.
    ///
    /// Returns true when the text element existed to write.
    pub fn update_quantity_display(&mut self, ui: &mut UiSystem) -> bool {
        let Some(h) = self.quantity else { return false };
        if self.quantity_value < 0 {
            ui.set_visible(h, false);
            return true;
        }
        if let Some(t) = ui.text_element_mut(h) {
            t.set_text(&format!("{}", self.quantity_value));
        }
        ui.set_visible(h, true);
        true
    }

    /// The capacity bar's display update.
    ///
    /// With no bar or no object it does nothing. A non-container, or an items capacity `< 1`, hides
    /// the bar. Otherwise `r = contained items / items capacity` (as doubles); `r == 0.0` hides
    /// the bar, and anything else shows it with `0x69 = clamp(r, 0, 1)` as a float.
    ///
    /// **An empty pack shows no bar at all**, which is the `r == 0.0` arm and is not the same as a
    /// bar at zero: the shipped `ItemSlot` layout gives `0x10000347` no `0x3B`, so a bar left up
    /// with `0x69 = 0` would draw its own frame over the icon of every empty container.
    ///
    /// A missing object is the caller's problem here: [`ItemListWidget::decorate`] runs this only
    /// for a slot that holds an object, which is exactly the guard.
    pub fn update_capacity_display(
        &mut self,
        ui: &mut UiSystem,
        items_capacity: i32,
        contained_items: i32,
    ) -> bool {
        let Some(h) = self.capacity_bar else {
            return false;
        };
        if !self.is_container || items_capacity < 1 {
            ui.set_visible(h, false);
            return true;
        }
        let r = meter_ratio(contained_items, items_capacity);
        if r == 0.0 {
            ui.set_visible(h, false);
            return true;
        }
        ui.set_visible(h, true);
        set_attr_float(ui, h, crate::bind::attr::METER_LEVEL, r.clamp(0.0, 1.0));
        true
    }

    /// The structure bar's display update.
    ///
    /// With no bar or no object it does nothing. A max structure of 0 hides the bar. Otherwise
    /// `r = structure / max structure` (as doubles); `r == 1.0` hides the bar, and anything else
    /// shows it with `0x69 = clamp(r, 0, 1)` as a float.
    ///
    /// The early-out is the **mirror image** of the capacity bar's: a container shows nothing when
    /// it is *empty*, and a structured item shows nothing when it is *whole*. Getting that pair
    /// the wrong way round draws a full bar on every undamaged lockpick in the pack.
    pub fn update_structure_display(
        &mut self,
        ui: &mut UiSystem,
        structure: u32,
        max_structure: u32,
    ) -> bool {
        let Some(h) = self.structure_bar else {
            return false;
        };
        if max_structure == 0 {
            ui.set_visible(h, false);
            return true;
        }
        let r = meter_ratio_u32(structure, max_structure);
        if r == 1.0 {
            ui.set_visible(h, false);
            return true;
        }
        ui.set_visible(h, true);
        set_attr_float(ui, h, crate::bind::attr::METER_LEVEL, r.clamp(0.0, 1.0));
        true
    }

    /// **The stack count a player actually sees.**
    ///
    /// A missing item id or object returns. Stack size zero becomes one; a stack above one selects
    /// the plural name and prefixes it with the signed count plus one space. The resulting text is
    /// assigned as the tooltip and the tooltip-present flag is set.
    ///
    /// The format is `"%d %s"` — *count then name*, no brackets.
    ///
    /// `NAME_PLURAL` takes the plural name and falls back to the name when that buffer is empty
    /// in the object-name read, which is what `plural` being `None` means.
    ///
    /// # This is the line that makes a slot hit-testable, and it must not be the only one
    ///
    /// The tooltip write's tail re-evaluates whether the element should be mouse-visible (it has
    /// a context menu or valid tooltip text) and sets its mouse visibility from that — which is
    /// how the radar becomes clickable over a blip. An item slot does **not**
    /// depend on that: `Self::post_init` makes it mouse-visible outright, and the visibility
    /// update ORs that explicit flag with the tooltip-derived one, so setting and clearing the
    /// tooltip moves the slot neither into nor out of the hit test.
    /// Asserted, because that is the easy thing to get wrong.
    ///
    /// The tooltip flag is property `0x4B` `UICore_Element_tooltip`, and the tooltip builder
    /// wants **both** it and non-empty text before it
    /// will build a tooltip element — so writing the text alone shows nothing on hover.
    pub fn update_tooltip(
        &mut self,
        ui: &mut UiSystem,
        name: &str,
        plural: Option<&str>,
        stack_size: u32,
    ) -> bool {
        if self.item.is_none() {
            return false;
        }
        let s = stack_size.max(1);
        let text = if s > 1 {
            // Through [`non_empty`], not a fourth hand-written copy of the same test: this line is
            // the one that would hide a divergence between the two
            // halves of the pack's early-out, by re-normalising on the way to the screen.
            let n = non_empty(plural).unwrap_or(name);
            format!("{s} {n}")
        } else {
            name.to_string()
        };
        self.tooltip = Some(text.clone());
        ui.set_tooltip(self.handle, Some(text));
        ui.set_attribute_bool(self.handle, dereth_ui::props::attr::TOOLTIP_ON, true);
        true
    }

    // ---- slot state ------------------------------------------------------------------------

    /// The ui item element's selected state write — the selection ring.
    ///
    /// A clear always goes through; a set only when the slot is selectable. Either way it stores
    /// `selected` and shows the selection ring exactly when `v != 0`.
    ///
    /// **The selectable flag gates only the *set*, never the *clear*.** A slot that
    /// [`ItemListWidget::handle_single_selection`] has made unselectable can still be deselected,
    /// which is
    /// exactly what that function does to the duplicates of a selected object before it selects
    /// the one under the pointer.
    ///
    /// Returns true when the ring element existed to write.
    pub fn set_selected_state(&mut self, ui: &mut UiSystem, selected: bool) -> bool {
        if selected && !self.selectable {
            return false;
        }
        self.selected = selected;
        let Some(h) = self.selected_ring else {
            return false;
        };
        ui.set_visible(h, selected);
        true
    }

    /// **The drop hint.**
    ///
    /// With no drop-hint child it does nothing; when the child is already in the requested state
    /// it does nothing; otherwise it sets the child's state.
    ///
    /// Both early-outs matter and are reproduced: a slot root without the child does nothing (the
    /// `ItemSlot` layout `0x21000037` carries it on every root, so this is unreachable in the
    /// shipped tree, as measured on the live tree), and re-setting the state a slot is
    /// already in raises no state write and therefore no media step. Returns whether a state
    /// write was actually issued, so a test can see the edge rather than only the end state.
    /// The "number of empty item slots" answer, off this slot's mirrored
    /// weenie fields.
    ///
    /// An items capacity of `-1` (unbounded) answers `-1`; otherwise it is the capacity minus
    /// the loose items in the object's inventory record, or the capacity when there is no record.
    ///
    /// **`-1` is *has room*, not *no room*.** [`ItemListWidget::drag_over`]'s test is `!= 0`, and
    /// the signed convention is what makes an unbounded container answer `INTO_CONTAINER`. The
    /// no-inventory-record arm needs no separate case here: an object with no inventory record
    /// carries `contained_items == 0`, so `cap - 0 == cap` is the client's answer.
    #[must_use]
    pub const fn num_empty_item_slots(&self) -> i64 {
        match self.items_capacity {
            dereth_rules::capacity::UNLIMITED => -1,
            cap => cap as i64 - self.contained_items as i64,
        }
    }

    pub fn set_drag_accept_state(&mut self, ui: &mut UiSystem, s: StateId) -> bool {
        let Some(h) = self.drag_accept else {
            return false;
        };
        if self.drag_accept_state == Some(s) {
            return false;
        }
        self.drag_accept_state = Some(s);
        ui.set_state(h, s);
        true
    }

    /// Set the slot's selectable flag and nothing else.
    pub const fn set_selectable_state(&mut self, selectable: bool) {
        self.selectable = selectable;
    }

    /// The ui item element's delayed shortcut num write — store the number and nothing else.
    ///
    /// The client calls it when a shortcut names an object the client has not seen:
    /// there is no weenie to write the number onto, so the slot keeps it and the slot update's
    /// "delayed number is not `-1`" arm pushes it into the weenie on the first pass where one
    /// exists, then resets the field to `-1`.
    pub const fn set_delayed_shortcut_num(&mut self, n: i32) {
        self.delayed_shortcut_num = n;
    }

    /// The frame on an **open** pack's icon.
    ///
    /// With no open-container frame it does nothing; otherwise it sets the frame's visibility
    /// only when that differs from `state`.
    ///
    /// The early-out on the element's *current* visibility is the client's, and it is why this
    /// returns false both for "no such child" and for "already in that state".
    ///
    /// See [`Self::open_container`] for why "no such child" is the answer on nineteen of the
    /// twenty shipped slot roots.
    pub fn set_open_container_state(&mut self, ui: &mut UiSystem, state: bool) -> bool {
        let Some(h) = self.open_container else {
            return false;
        };
        if ui.node(h).is_some_and(|n| n.region.flags.visible) == state {
            return false;
        }
        ui.set_visible(h, state);
        true
    }

    /// The ui item element's cooldown display update — **the cooldown wedge.**
    ///
    /// With no structure bar it does nothing (see the guard discussion below). Otherwise, when
    /// the object has a cooldown id `> 0` and the player's enchantment registry reports cooldown
    /// `id + 0x8000` as running with `remaining` seconds left, it is `on`, with
    /// `n = trunc(remaining / cooldown duration * 100.0 * 0.1 + 1.0)` when the duration is
    /// positive (else `n = 0`). Wedge *k* (10 %…90 %) is shown when `on && n == k`, and the
    /// 100 % wedge when `on && n >= 10`.
    ///
    /// **Exactly one wedge is up at a time.** Nine of the ten arms are an *equality*; only the
    /// tenth is a threshold. A rebuild that read the ten as cumulative layers
    /// — the obvious reading of the names — would light ten overlays at
    /// once on a fresh cooldown, and every one of them is a full-slot 32×32 image.
    ///
    /// **The index counts down.** `remaining / duration` starts at 1 and falls to 0, so
    /// `n = trunc(10 r + 1)` runs 11 → 1: the *100* wedge (`n >= 10`) at the start, the *10* wedge
    /// (`n == 1`) at the end, and nothing once the cooldown expires. The three constants are
    /// `100.0`, `0.1` and `1.0`, applied in that order and
    /// deliberately not folded, because `r/d*100.0*0.1` and `r/d*10.0` are not the same double.
    ///
    /// **The guard is on the structure bar, and that is a shipped bug rather than a mis-read.**
    /// The retail client tests the structure bar, not a cooldown wedge — a copy-and-paste from
    /// the structure display, whose first line is the same test.
    /// It is invisible in retail because every `ItemSlot` root carries both children, and it is
    /// reproduced here so that a slot root without a structure bar behaves as the client would.
    ///
    /// `remaining` is the registry's remaining-time answer: `None` means the registry said *not on
    /// cooldown* (or there is no registry), which is the `on = false` arm.
    ///
    /// Returns the index into [`child::COOLDOWN`] of the wedge left visible, or `None`.
    pub fn update_cooldown_display(
        &mut self,
        ui: &mut UiSystem,
        remaining: Option<f64>,
    ) -> Option<usize> {
        // The whole body is inside the structure-bar test.
        self.structure_bar?;
        let mut on = false;
        let mut n = 0_i32;
        if self.cooldown_id > 0 {
            if let Some(r) = remaining {
                if self.cooldown_duration > 0.0 {
                    // The client truncates toward zero; a raw `as i32` saturates instead.
                    n = dereth_primitives::num::to_i32_f64(
                        r / self.cooldown_duration * 100.0 * 0.1 + 1.0,
                    );
                }
                on = true;
            }
        }
        let mut up = None;
        for (i, h) in self.cooldown.iter().enumerate() {
            let Some(h) = *h else { continue };
            let k = i32::try_from(i).unwrap_or(0) + 1;
            let v = on && if i == 9 { n >= 10 } else { n == k };
            ui.set_visible(h, v);
            if v {
                up = Some(i);
            }
        }
        self.cooldown_wedge = up;
        up
    }

    /// The ui item element's heartbeat step, driven by
    /// the global-message handler's message 3.
    ///
    /// On message 3, once `now >= last heartbeat + interval`, it records `now` as the last
    /// heartbeat and runs the heartbeat.
    ///
    /// The heartbeat does nothing but run the cooldown-display update. So the heartbeat *is* the
    /// cooldown display and there is nothing else in it.
    ///
    /// This is the piece that makes a wedge move. Every other decoration in this file is redrawn
    /// when the panel's snapshot changes, and a cooldown ticking down changes no snapshot at all —
    /// so without a once-a-second pass the wedge would freeze on whichever step it was drawn at.
    ///
    /// Returns true when the interval had elapsed and the display was re-run.
    pub fn listen_to_global_message(
        &mut self,
        ui: &mut UiSystem,
        msg: u32,
        now: f64,
        remaining: Option<f64>,
    ) -> bool {
        if msg != HEARTBEAT_MESSAGE || self.last_heartbeat + HEARTBEAT_INTERVAL > now {
            return false;
        }
        self.last_heartbeat = now;
        self.update_cooldown_display(ui, remaining);
        true
    }

    /// The sell-state arm of the client's slot-update tail.
    ///
    /// When the object's sell state differs from the slot's stored copy, it shows the sell marker
    /// exactly when the state is non-zero and stores `(state != 0)`.
    ///
    /// Note the asymmetry the client actually has: the comparison is against the raw `int`, the
    /// stored copy is the **boolean**.
    ///
    /// The sell-state write's one caller in the retail client is
    /// the vendor panel's own sell-state write, and this build's counterpart is
    /// `add_item_to_sell`, following the vendor panel's item insert and its "the target list is
    /// element `0x100000CE`, so mark it for sale" step, reached by a drop on the sell list.
    /// The marker is the little crown-type image in the top corner, and it is drawn in the pack **and** in the
    /// sell window because the sell-state write ends in an item-attributes-changed broadcast,
    /// which every tile showing that object answers.
    pub fn set_sell_state(&mut self, ui: &mut UiSystem, v: bool) -> bool {
        if self.sell_state == v {
            return false;
        }
        if let Some(h) = self.sell_state_elem {
            ui.set_visible(h, v);
        }
        self.sell_state = v;
        true
    }

    /// The trade-state arm of the same tail, identical in shape.
    ///
    /// Its writers are the trade, salvage, and housing panels through their trade-state updates.
    /// Those windows now exist; the remaining seam is delivery of the matching state change to
    /// every tile that displays the object.
    pub fn set_trade_state(&mut self, ui: &mut UiSystem, v: bool) -> bool {
        if self.trade_state == v {
            return false;
        }
        if let Some(h) = self.trade_state_elem {
            ui.set_visible(h, v);
        }
        self.trade_state = v;
        true
    }
}

/// `a / b` as the client computes it — it converts
/// both `int`s to `double`, divides, and only then narrows to `float` for the attribute.
fn meter_ratio(a: i32, b: i32) -> f32 {
    if b == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // the client's own `(float)` narrowing
    {
        (f64::from(a) / f64::from(b)) as f32
    }
}

/// The same for two unsigned 32-bit operands, which the client converts as unsigned (a value
/// that converts negative is corrected back with `+ 4294967296.0`).
fn meter_ratio_u32(a: u32, b: u32) -> f32 {
    if b == 0 {
        return 0.0;
    }
    #[allow(clippy::cast_possible_truncation)] // the client's own `(float)` narrowing
    {
        (f64::from(a) / f64::from(b)) as f32
    }
}

/// Everything one slot's decoration pass reads off its object — [`ItemListWidget::decorate`]'s
/// input, and the whole of what the slot reads from its object.
///
/// It is owned rather than borrowed because the callback is a closure over a `&dyn GameView` and
/// the two names come out of it by reference; a slot is decorated at most once per changed frame,
/// so the two `String`s are cheaper than the lifetime.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct SlotInfo {
    /// The object description's numbers.
    pub decoration: crate::view::SlotDecoration,
    /// The object's name.
    pub name: String,
    /// The object's plural name, when it carries a non-empty one.
    pub plural_name: Option<String>,
    /// The client's out-parameter for this object's
    /// cooldown id `+ 0x8000`, or `None` when the call returned 0.
    ///
    /// It is a *separate* field from [`Self::decoration`] because it is not a fact about the
    /// object at all: it is a fact about the **player's own enchantment registry**, read at draw
    /// time, and it changes every frame while a cooldown runs. The cooldown id and
    /// duration are the object's and travel in the decoration.
    pub cooldown_remaining: Option<f64>,
}

/// The client's empty-buffer convention, as **one expression for the whole crate**.
///
/// The client answers `NAME_PLURAL` from the plural name and falls
/// back to the name when that buffer holds only its terminator, so an empty plural means *use the
/// singular* — never *the plural is the empty string*.
///
/// **Three call sites in this crate have to agree, and two of them are the two halves of one
/// early-out.** [`TileInfo::read`] fills a guard's memory, [`TileInfo::matches`] compares it, and
/// [`ItemSlot::update_tooltip`] draws the result. A divergence between the first two would make
/// the gate either flap open on every frame — flushing four lists, the drag ghost and the
/// selection ring, at a cost that is invisible on screen — or close permanently and freeze every
/// tooltip in the pack; and the third would **hide** the flap, because it re-normalises on the way
/// to the screen, so the picture would stay right while the work ran every frame. Both failures
/// are plausible, neither reddens anything, and the third makes the first two unobservable. So the
/// possibility is **removed** rather than tested: there is one expression, and all three call it.
const fn non_empty(s: Option<&str>) -> Option<&str> {
    match s {
        Some(p) if !p.is_empty() => Some(p),
        _ => None,
    }
}

/// [`non_empty`] applied at the seam — the plural name for one object.
pub(super) fn plural_name_of(view: &dyn crate::view::GameView, id: ObjectId) -> Option<&str> {
    non_empty(view.plural_name(id))
}
