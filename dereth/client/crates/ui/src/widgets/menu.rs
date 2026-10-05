use super::*;
use crate::ElementId;

/// Attribute **2** — the list box's element id inside the popup.
/// The menu's initialisation reads enum attribute 2, finds that descendant of the popup and
/// requires it to be type 5.
pub const ATTR_LIST_BOX: u32 = 2;
/// Attribute **6** — the popup's root element id inside the layout attribute **7** names.
/// Popup construction reads the layout from attribute 7, falls back to the menu's own layout,
/// reads the root id from attribute 6, and creates that root from the selected layout.
pub const ATTR_POPUP_ROOT: u32 = 6;
/// Attribute **7** — the layout the popup root is built out of, a `DataFile`.
/// Popup construction falls back to the menu's own layout when it is absent.
pub const ATTR_POPUP_LAYOUT: u32 = 7;
/// Attribute **9** — the element id one row is created from.
/// The insert-text-item reads it as an enum attribute.
pub const ATTR_ITEM_ELEMENT: u32 = 9;
/// Attribute **10** — the layout that row template lives in, with the same fall-back to the
/// menu's own layout.
pub const ATTR_ITEM_LAYOUT: u32 = 10;
/// Attribute **1** — an initial selected item id, read during menu initialization and skipped
/// when zero.
pub const ATTR_INITIAL_SELECTION: u32 = 1;
/// Attribute **3** — centres the popup on the menu instead of aligning
/// their left edges.
pub const ATTR_POPUP_CENTRED: u32 = 3;
/// Attribute **5** — puts the popup **above** the menu rather than below
/// it. The chat window's talk-focus menu `0x10000014` carries `5 = true`, which is why its
/// fourteen rows rise out of the bottom of the screen.
pub const ATTR_POPUP_ABOVE: u32 = 5;
/// Attribute **8** — the `TextElement` inside the menu that the selection
/// copies the chosen row's pre-parsed text into.
pub const ATTR_SELECTION_TEXT: u32 = 8;
/// Attribute **0x0E** — the open flag, written by the open and close operations.
/// It is `UICore_Button_toggled`, which is how an open drop-down draws pressed.
pub const ATTR_OPEN: u32 = 0x0E;

#[derive(Debug, Default)]
pub struct Menu {
    /// **A menu is a button, which is a text element** — the drop-down shows the
    /// chosen row's text on its own face.
    pub button: super::button::Button,
    /// The menu is open.
    pub open: bool,
    /// The popup — the element [`make_popup`] created, a **root** of the manager and not a
    /// child of the menu.
    pub popup: Option<ElemHandle>,
    /// The list box — the `ListBox` [`initialize_popup`] found inside the popup.
    pub list_box: Option<ElemHandle>,
    /// Attribute [`ATTR_POPUP_ROOT`]'s value: which element of the popup layout to build.
    pub popup_id: Option<ElementId>,
    /// Attribute [`ATTR_LIST_BOX`]'s value: which element inside the popup is the list.
    pub list_box_id: Option<ElementId>,
    /// Attribute [`ATTR_ITEM_ELEMENT`]'s value: the row template's element id.
    pub item_id: Option<ElementId>,
    /// Attribute [`ATTR_SELECTION_TEXT`]'s value.
    pub selection_text_id: Option<ElementId>,
    /// Attribute [`ATTR_POPUP_LAYOUT`]'s value.
    pub popup_layout: Option<crate::DataId>,
    /// Attribute [`ATTR_ITEM_LAYOUT`]'s value.
    pub item_layout: Option<crate::DataId>,
    /// The list box's x and y borders — the popup's size less the list box's,
    /// latched by [`initialize_popup`] and added back by [`recalculate_popup_size`].
    pub border: (i32, i32),
}

pub fn create(_l: &crate::LayoutDesc, _d: &crate::ElementDesc) -> Box<dyn Element> {
    Box::new(Menu::default())
}

/// The menu's selected-index query — the index of the selected item within the list's rows, or
/// **-1** when there is no list, no selection, or no matching row.
///
/// **The `-1`s are the point, not the fallback.** The confirmation-menu dialog's cancel arm
/// never calls this at all — its answer is primed with `-1` and the query runs only on the
/// accept arm — and both menu subclasses' cancel operations write a literal `-1`.
/// A menu with no list box answering -1 is therefore
/// indistinguishable, by design, from a menu whose player chose nothing.
///
/// The list box is written by [`initialize_popup`], which is the initialisation's second
/// half and runs after [`make_popup`]; until both have run this answers -1.
#[must_use]
pub fn selected_index(ui: &UiSystem, me: ElemHandle) -> i32 {
    let Some(list_h) = list_box_handle(ui, me) else {
        return -1;
    };
    let Some(list) = ui.node(list_h).and_then(|n| {
        n.behaviour
            .as_ref()?
            .as_any()?
            .downcast_ref::<super::listbox::ListBox>()
    }) else {
        return -1;
    };
    list.selected
        .and_then(|i| i32::try_from(i).ok())
        .unwrap_or(-1)
}

/// The list box — the `ListBox` [`initialize_popup`] resolved inside the popup.
///
/// `None` is the client's null list box, which every menu member answers -1 / false /
/// nothing to. It is **not** a recursive lookup of
/// attribute 2 below the *menu*: the initialisation looks
/// below the **popup**, and the popup is a root
/// element rather than a child of the menu, so a lookup below the menu
/// could only ever succeed on a hand-built tree.
#[must_use]
pub fn list_box_handle(ui: &UiSystem, me: ElemHandle) -> Option<ElemHandle> {
    let menu = ui
        .node(me)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<Menu>()?;
    let list = menu.list_box?;
    ui.node(list).map(|_| list)
}

/// The popup root [`make_popup`] created, or `None`.
#[must_use]
pub fn popup_handle(ui: &UiSystem, me: ElemHandle) -> Option<ElemHandle> {
    let menu = ui
        .node(me)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<Menu>()?;
    let popup = menu.popup?;
    ui.node(popup).map(|_| popup)
}

fn with_menu<T>(ui: &UiSystem, me: ElemHandle, f: impl FnOnce(&Menu) -> T) -> Option<T> {
    Some(f(ui
        .node(me)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<Menu>()?))
}

fn with_menu_mut<T>(
    ui: &mut UiSystem,
    me: ElemHandle,
    f: impl FnOnce(&mut Menu) -> T,
) -> Option<T> {
    Some(f(ui
        .node_mut(me)?
        .behaviour
        .as_mut()?
        .as_any_mut()?
        .downcast_mut::<Menu>()?))
}

/// Lift the `Menu` **out of its arena slot** for the duration of `f`, exactly as
/// [`crate::UiSystem`] does before it calls a widget's own handler.
///
/// **This is why every operation below has a `&mut Menu` form.** A free function
/// that reads the menu back out of `ui` cannot be called *from inside* the menu's own
/// `listen_to_element_message`: the slot is empty for the duration of that call, so
/// `list_box_handle` answers `None` and the whole body silently does nothing — the menu
/// receives the selection and raises no `MENU_CHOSEN` — and it fails **silently** unless a
/// test asserts that the receiving side actually observes the notice.
fn with_taken<T>(
    ui: &mut UiSystem,
    me: ElemHandle,
    f: impl FnOnce(&mut Menu, &mut UiSystem) -> T,
) -> Option<T> {
    let mut b = ui.take_behaviour(me)?;
    let out = b
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<Menu>())
        .map(|m| f(m, ui));
    ui.put_behaviour(me, b);
    out
}

fn with_list_mut<T>(
    ui: &mut UiSystem,
    list: ElemHandle,
    f: impl FnOnce(&mut super::listbox::ListBox, &mut UiSystem) -> T,
) -> Option<T> {
    let mut b = ui.take_behaviour(list)?;
    let out = b
        .as_any_mut()
        .and_then(|a| a.downcast_mut::<super::listbox::ListBox>())
        .map(|l| f(l, ui));
    ui.put_behaviour(list, b);
    out
}

/// The menu's popup construction — build the drop-down's popup out of the layout
/// attribute [`ATTR_POPUP_LAYOUT`] names.
///
/// An existing popup is queued for deletion. The client resolves layout attribute 7 with the
/// menu's layout as fallback, reads root id attribute 6, and creates a root. A created popup
/// starts hidden with activatable `0x33` and activate-on-show `0x34` set, is stored on the menu,
/// and registers the menu for its element messages.
///
/// The visibility write is `false`, and the two bool attributes `0x33` and `0x34` are both
/// written `true`.
///
/// **The popup is a root element, not a child of the menu.** That is what lets a drop-down
/// draw over the window below it, and it is why the menu has to *register* for the popup's
/// element messages: nothing would otherwise bubble from the list box back to the menu.
///
/// Returns the popup, or `None` when the layout or the element id is missing — the client's
/// two silent returns.
pub fn make_popup(
    ui: &mut UiSystem,
    assets: &dyn dereth_primitives::AssetSource,
    me: ElemHandle,
) -> Option<ElemHandle> {
    if let Some(old) = with_menu_mut(ui, me, |m| m.popup.take()).flatten() {
        ui.add_to_delete_queue(old);
    }
    let own_layout = ui.node(me)?.layout_did;
    let (declared, root_id) = with_menu(ui, me, |m| (m.popup_layout, m.popup_id))?;
    let root_id = root_id?;
    let popup = declared
        .and_then(|did| ui.create_root_by_data_id(assets, did, root_id).ok())
        .or_else(|| ui.create_root_by_data_id(assets, own_layout, root_id).ok())?;
    ui.initialize_tree(popup);
    ui.set_visible(popup, false);
    ui.set_attribute_bool(popup, crate::props::attr::ACTIVATABLE, true);
    ui.set_attribute_bool(popup, crate::props::attr::ACTIVATE_ON_SHOW, true);
    with_menu_mut(ui, me, |m| m.popup = Some(popup));
    ui.register_for_element_messages(popup, crate::ListenerId::Element(me));
    Some(popup)
}

/// The client's popup-dependent half, run after [`make_popup`].
///
/// After creating the popup, the client reads list id attribute 2 and finds that list beneath
/// the popup. If available, selection attribute 1 seeds the selected row. It initializes the
/// menu and stores the popup-minus-list width and height as borders.
///
/// It is split out because `Element::initialize` in this crate has no asset source and cannot
/// call [`make_popup`] (see `env::create_and_add_root_element` for why `UiSystem` holds none).
/// The host calls [`make_popup`] and then this; everything after is the client's order.
pub fn initialize_popup(ui: &mut UiSystem, me: ElemHandle) -> Option<ElemHandle> {
    let popup = popup_handle(ui, me)?;
    let list_id = with_menu(ui, me, |m| m.list_box_id)??;
    let list = ui.get_child_recursive(popup, list_id).filter(|h| {
        ui.node(*h)
            .is_some_and(|n| n.ty() == crate::factory::ty::LISTBOX)
    });
    with_menu_mut(ui, me, |m| m.list_box = list);
    let list = list?;
    let pb = ui.screen_box(popup);
    let lb = ui.screen_box(list);
    with_menu_mut(ui, me, |m| {
        m.border = (pb.width() - lb.width(), pb.height() - lb.height());
    });
    Some(list)
}

/// The menu's insert-text-item — create one row and put it in the list at
/// `index`.
///
/// It reads row element id attribute 9 and resolves layout attribute 10 with the menu's layout
/// as fallback. It creates that child, requires a text element, sets the caption, and inserts it
/// at `index`; any failed step deletes or rejects the row and returns nothing.
///
/// The list's own insert is the two lines that make a row usable:
/// make the row mouse-visible, and then
/// the list box's insert. Without the first the row is drawn and cannot
/// be clicked.
///
/// The caption is taken as an already-resolved string rather than as a `StringInfo`, because
/// every caller in this build resolves through the host's string service before it gets here
/// (see `dereth_ui_screens::chat::mainchat::label`).
pub fn insert_text_item(
    ui: &mut UiSystem,
    assets: &dyn dereth_primitives::AssetSource,
    me: ElemHandle,
    text: &str,
    index: usize,
) -> Option<ElemHandle> {
    let list = list_box_handle(ui, me)?;
    let own_layout = ui.node(me)?.layout_did;
    let (declared, item_id) = with_menu(ui, me, |m| (m.item_layout, m.item_id))?;
    let item_id = item_id?;
    let item = declared
        .and_then(|did| ui.create_child_by_data_id(assets, list, did, item_id).ok())
        .or_else(|| {
            ui.create_child_by_data_id(assets, list, own_layout, item_id)
                .ok()
        })?;
    ui.initialize_tree(item);
    if ui.text_element_mut(item).is_none() {
        // The type-`0x0C` check failed: the description named something that is not a
        // `TextElement`, and the client destroys the element again.
        ui.remove_and_delete_root(item);
        return None;
    }
    if let Some(t) = ui.text_element_mut(item) {
        t.set_text(text);
    }
    // The menu's insert, which makes the row mouse-visible first.
    ui.set_mouse_visible(item, true);
    let ok = with_list_mut(ui, list, |l, ui| l.insert_item(ui, list, item, index)).unwrap_or(false);
    if !ok {
        ui.remove_and_delete_root(item);
        return None;
    }
    // The client sets the layout dirty bit `0x200`, and its own layout pass consumes it on the
    // next frame. This crate has no layout pass
    // (see the layout-pass note), so this method makes the two calls that pass would make:
    // it places the rows and grows the popup to hold them. Doing it per
    // insert rather than per batch is the same answer and keeps the caller free of it.
    layout_items(ui, me);
    Some(item)
}

/// The layout update's tail for a menu whose rows changed: place the rows, tell the
/// scrollable how big its paper now is, and grow the popup to match. Returns
/// `(cols, rows)`.
///
/// The three steps are one function in the client — the layout update ends in the
/// scrollable-area resize, whose `0x32` broadcast reaches
/// the popup-size recalculation through the menu's element-message listener. **That
/// broadcast cannot do the work here**: it arrives while the list's behaviour is out of its
/// arena slot, so the menu's `0x32` arm reads a paper size of `(0, 0)` and shrinks the popup
/// to nothing — a popup box of `(5,576)-(4,577)`, zero wide, with
/// fourteen correctly placed rows inside it. So the resize is driven from here, where both
/// objects are in their slots, and the `0x32` arm is kept because it is the client's own.
pub fn layout_items(ui: &mut UiSystem, me: ElemHandle) -> (i32, i32) {
    let Some(list) = list_box_handle(ui, me) else {
        return (0, 0);
    };
    let grid = with_list_mut(ui, list, |l, ui| {
        let g = l.update_layout(ui, list);
        l.refresh_scroll(ui, list);
        g
    })
    .unwrap_or((0, 0));
    recalculate_popup_size(ui, me);
    grid
}

/// Behavior: insert a text item at the list box's item count,
/// i.e. append.
///
/// The client dereferences the list box and then reads its item count with **no null check
/// on either**, so a menu with no list box faults; here it answers `None`, which is the same
/// "no row was made" the caller has to handle anyway.
///
/// This is the client's only row-making call, fourteen times.
pub fn add_text_item(
    ui: &mut UiSystem,
    assets: &dyn dereth_primitives::AssetSource,
    me: ElemHandle,
    text: &str,
) -> Option<ElemHandle> {
    let n = num_items(ui, me);
    insert_text_item(ui, assets, me, text, n)
}

/// Behavior: flush the list box, or nothing when there is none.
pub fn flush(ui: &mut UiSystem, me: ElemHandle) {
    let Some(list) = list_box_handle(ui, me) else {
        return;
    };
    with_list_mut(ui, list, |l, ui| l.flush(ui));
}

/// Behavior: the list box's item count, **0** with no
/// list box.
#[must_use]
pub fn num_items(ui: &UiSystem, me: ElemHandle) -> usize {
    let Some(list) = list_box_handle(ui, me) else {
        return 0;
    };
    ui.node(list)
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<super::listbox::ListBox>()
        })
        .map_or(0, |l| l.items.len())
}

/// The menu's item accessor — the list box's `get_item(i)`.
#[must_use]
pub fn get_item(ui: &UiSystem, me: ElemHandle, index: usize) -> Option<ElemHandle> {
    let list = list_box_handle(ui, me)?;
    ui.node(list)
        .and_then(|n| {
            n.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<super::listbox::ListBox>()
        })
        .and_then(|l| l.get_item(index))
}

/// The menu's selected-item setter, whole:
///
/// ```text
/// if list_box: list_box.set_selected_item(item, broadcast)
///              new_selection(broadcast)
/// ```
///
/// `None` clears the selection, and the main chat window's
/// message-7 arm calls it exactly that way — with no item and `false` — so that
/// the menu does not keep a highlight on the row the player just chose. The trailing
/// new-selection step is what puts the chosen row's text on the menu's own face, and it is the
/// **only** caller of the free [`new_selection`]: the client's other one is
/// the element-message listener's message-4 case, which cannot use it (see `with_taken`).
pub fn set_selected_item(
    ui: &mut UiSystem,
    me: ElemHandle,
    item: Option<ElemHandle>,
    broadcast: bool,
) {
    let Some(list) = list_box_handle(ui, me) else {
        return;
    };
    // **`broadcast` is passed through.** The client's own body passes
    // the one flag to both the list box's selected-item setter and the new-selection step.
    // Dropping it here would make the main chat window's clearing call (no item, `false`)
    // still raise the list's own `4`.
    with_list_mut(ui, list, |l, ui| {
        l.set_selected_item(ui, list, item, broadcast)
    });
    new_selection(ui, me, broadcast);
}

/// Open the menu.
///
/// ```text
/// if open or no list box or the list box has no items or no popup: return
/// above = bool attribute 5;  centred = bool attribute 3
/// x = screen.x0
/// if centred: x -= (popup.width - width) / 2
/// y = above ? screen.y0 - popup.height : screen.y1
/// move the popup to (x, y) in screen space
/// make the popup visible
/// open = true; bool attribute 0x0E = true; broadcast message 8 with (0, 0)
/// ```
///
/// **An empty item list is a hard return**: a talk-focus menu with no rows does not open, so
/// clicking the Chat tab does nothing at all rather than opening an empty box. Returns whether
/// it opened.
pub fn open(ui: &mut UiSystem, me: ElemHandle) -> bool {
    with_taken(ui, me, |m, ui| m.open_now(ui, me)).unwrap_or(false)
}

/// Behavior: hide the popup, clear the open flag and `0x0E`, broadcast
/// **9**. Returns whether it was open.
pub fn close(ui: &mut UiSystem, me: ElemHandle) -> bool {
    with_taken(ui, me, |m, ui| m.close_now(ui, me)).unwrap_or(false)
}

/// Behavior: copy the chosen row's text onto the menu's own
/// face and broadcast **7**.
///
/// With a list present, it reads text-child attribute 8 and copies the selected row's text to
/// that child, or clears it when nothing is selected. When `broadcast` is true it sends message
/// 7 with the selected row's element id and handle, or two zeroes for no selection.
///
/// `p2` is the **element**, which is the only thing that distinguishes one row from another:
/// every row of one menu is created from the same description, so `p1` is the same id for all
/// of them. The client reads `p2` for exactly
/// that reason.
pub fn new_selection(ui: &mut UiSystem, me: ElemHandle, broadcast: bool) {
    with_taken(ui, me, |m, ui| m.new_selection_now(ui, me, broadcast));
}

/// Behavior: grow the popup to the rows it holds.
///
/// With both list and popup present, it starts from the popup size. A list pinned at both
/// horizontal edges replaces width with scrollable width plus the saved border; a list pinned
/// at both vertical edges does the same for height. The popup is resized to that result.
///
/// The four `== 1` comparisons read the list's left, top, right, and bottom anchoring modes: the
/// popup only follows the list on an axis where the list is pinned at **both** ends and
/// therefore cannot stretch on its own. The measured scrollable width and height supply the
/// replacement dimensions.
///
/// It is reached from element message `0x32`, the client's own row-change broadcast, so adding
/// rows resizes the popup
/// without anyone asking.
pub fn recalculate_popup_size(ui: &mut UiSystem, me: ElemHandle) {
    with_taken(ui, me, |m, ui| m.recalculate_popup_size_now(ui, me));
}

impl Menu {
    /// The list box's item count, off the fields this object already holds.
    fn items_len(&self, ui: &UiSystem) -> usize {
        let Some(list) = self.list_box else { return 0 };
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .map_or(0, |l| l.items.len())
    }

    /// The body of [`open`], callable while this `Menu` is out of its slot.
    pub fn open_now(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
        let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
            return false;
        };
        if self.open || self.list_box.is_none() || self.items_len(ui) == 0 {
            return false;
        }
        let props = ui.node(me).map(crate::ElementNode::merged_properties);
        let above = props
            .as_ref()
            .and_then(|p| p.get_bool(ATTR_POPUP_ABOVE))
            .unwrap_or(false);
        let centred = props
            .as_ref()
            .and_then(|p| p.get_bool(ATTR_POPUP_CENTRED))
            .unwrap_or(false);
        let mine = ui.screen_box(me);
        let pb = ui.screen_box(popup);
        let mut x = mine.x0;
        if centred {
            x -= (pb.width() - mine.width()) / 2;
        }
        let y = if above {
            mine.y0 - pb.height()
        } else {
            mine.y1 + 1
        };
        ui.move_to(popup, x, y);
        ui.set_visible(popup, true);
        self.open = true;
        ui.set_attribute_bool(me, ATTR_OPEN, true);
        ui.broadcast_element_message(me, msgid::MENU_OPENED, 0, 0);
        true
    }

    /// The body of [`close`], callable while this `Menu` is out of its slot.
    pub fn close_now(&mut self, ui: &mut UiSystem, me: ElemHandle) -> bool {
        if !self.open {
            return false;
        }
        let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
            return false;
        };
        ui.set_visible(popup, false);
        self.open = false;
        ui.set_attribute_bool(me, ATTR_OPEN, false);
        ui.broadcast_element_message(me, msgid::MENU_CLOSED, 0, 0);
        true
    }

    /// The list box's selected item, read out of the list's behaviour.
    ///
    /// **Answers `None` while the list box is itself mid-dispatch**, because its behaviour is
    /// then out of its arena slot — which is exactly when message 4 arrives. That is what
    /// [`Menu::new_selection_with`]'s hint is for; getting it wrong produces a
    /// `MENU_CHOSEN` with `p2 = 0`: a message that says "a row was chosen" and does not
    /// say which.
    fn selected_item(&self, ui: &UiSystem) -> Option<ElemHandle> {
        let list = self.list_box?;
        ui.node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .and_then(|l| l.selected.and_then(|i| l.items.get(i).copied()))
    }

    /// The body of [`new_selection`], callable while this `Menu` is out of its slot — which is
    /// where the client calls it from: the element-message listener's message-4 case.
    pub fn new_selection_now(&mut self, ui: &mut UiSystem, me: ElemHandle, broadcast: bool) {
        let selected = self.selected_item(ui);
        self.new_selection_with(ui, me, selected, broadcast);
    }

    /// [`Menu::new_selection_now`] with the selected item supplied rather than read.
    ///
    /// Message 4 carries it in `p2` (`ListBox`'s own broadcast: `p1 = index`,
    /// `p2 = the item`), which is the only place it can be read from while the list is
    /// dispatching.
    pub fn new_selection_with(
        &mut self,
        ui: &mut UiSystem,
        me: ElemHandle,
        selected: Option<ElemHandle>,
        broadcast: bool,
    ) {
        if self.list_box.is_none() {
            return;
        }
        if let Some(text_id) = self.selection_text_id {
            if let Some(face) = ui.get_child_recursive(me, text_id) {
                // The pre-parsed text is the tagged form, so a row carrying a link keeps it.
                let chosen: Option<String> =
                    selected.and_then(|h| ui.text_element_mut(h).map(|t| t.glyphs.inq_text(true)));
                if let Some(t) = ui.text_element_mut(face) {
                    // Clear all text when there is no selection — the client's else arm.
                    t.set_text(chosen.as_deref().unwrap_or(""));
                }
            }
        }
        if !broadcast {
            return;
        }
        match selected {
            Some(h) => {
                let id = ui.node(h).map_or(0, |n| n.element_id().0);
                ui.broadcast_element_message(me, msgid::MENU_CHOSEN, id, h.raw());
            }
            None => ui.broadcast_element_message(me, msgid::MENU_CHOSEN, 0, 0),
        }
    }

    /// The body of [`recalculate_popup_size`], callable while this `Menu` is out of its slot.
    pub fn recalculate_popup_size_now(&mut self, ui: &mut UiSystem, me: ElemHandle) {
        let _ = me;
        let Some(popup) = self.popup.filter(|p| ui.node(*p).is_some()) else {
            return;
        };
        let Some(list) = self.list_box else { return };
        let Some(edges) = ui.node(list).map(|n| n.desc.edges) else {
            return;
        };
        let paper = ui
            .node(list)
            .and_then(|n| {
                n.behaviour
                    .as_ref()?
                    .as_any()?
                    .downcast_ref::<super::listbox::ListBox>()
            })
            .map(|l| (l.scroll.width, l.scroll.height));
        let Some((pw, ph)) = paper else { return };
        let pb = ui.screen_box(popup);
        let pinned = |a: crate::layout::EdgeMode, b: crate::layout::EdgeMode| {
            a == crate::layout::EdgeMode::AnchorStart && b == crate::layout::EdgeMode::AnchorStart
        };
        let w = if pinned(edges.left, edges.right) {
            pw + self.border.0
        } else {
            pb.width()
        };
        let h = if pinned(edges.top, edges.bottom) {
            ph + self.border.1
        } else {
            pb.height()
        };
        ui.resize_to(popup, w, h);
        // `resize_to` cascades `update_for_parent_size_change` through the popup tree. A menu
        // row is cloned from one layout description, so an anchored row template can put every
        // clone back at that description's Y during the cascade. Retail does not make this
        // second call directly in the popup-size recalculation: the insert set dirty bit 0x200
        // and a later layout consumption (also explicit in the item-index-at-point query)
        // re-places the rows. This crate has no global layout pass (see `layout_items`), so
        // consume the equivalent Rust scheduling point here. The paper dimensions are already
        // current, making the nested 0x32 path a no-op while this Menu is lifted.
        with_list_mut(ui, list, |l, ui| {
            l.update_layout(ui, list);
            l.refresh_scroll(ui, list);
        });
    }
}

/// The menu's item accessor followed by its selected-item setter — the pair
/// the menu dialog's `set_data` runs for property `0xA4` and
/// the confirmation-menu dialog's runs for `0xAB`.
///
/// **The dialog's *initial* selection and its answer are the same property key.** The item
/// accessor answers none for an index past the end and selecting none clears the selection,
/// so an out-of-range seed leaves the selected index at -1 rather than at the old row; a menu
/// with no list box ignores the call entirely.
pub fn set_selected_index(ui: &mut UiSystem, me: ElemHandle, index: i32) {
    let Some(list_h) = list_box_handle(ui, me) else {
        return;
    };
    let n = ui
        .node(list_h)
        .and_then(|nd| {
            nd.behaviour
                .as_ref()?
                .as_any()?
                .downcast_ref::<super::listbox::ListBox>()
        })
        .map_or(0, |l| l.items.len());
    match usize::try_from(index).ok().filter(|i| *i < n) {
        Some(i) => {
            if let Some(mut b) = ui.take_behaviour(list_h) {
                if let Some(l) = b
                    .as_any_mut()
                    .and_then(|a| a.downcast_mut::<super::listbox::ListBox>())
                {
                    l.select(ui, list_h, i);
                }
                ui.put_behaviour(list_h, b);
            }
        }
        None => {
            if let Some(l) = ui.node_mut(list_h).and_then(|nd| {
                nd.behaviour
                    .as_mut()?
                    .as_any_mut()?
                    .downcast_mut::<super::listbox::ListBox>()
            }) {
                l.selected = None;
            }
        }
    }
}

impl Element for Menu {
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    /// The type-6 downcast, mutably — what [`make_popup`] and [`initialize_popup`] write the
    /// popup and the list box through. Without it both would be silent no-ops.
    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }

    /// The original mouse-visibility query always returns true for the five mouse-driven
    /// element types: button, menu, dragbar, resizebar, and scrollbar.
    fn should_be_mouse_visible(&self) -> bool {
        true
    }

    /// The original menu shares button, text, and scrolling mouse-down behavior, so pressing a
    /// menu moves the focus element.
    fn takes_focus_on_press(&self) -> bool {
        true
    }

    /// The client's two element-id reads, taken here rather than
    /// in `post_init` for the same reason `ListBox` takes `0x71`/`0x72` here: the
    /// value arrives through initialisation step 4's `on_set_attribute` sweep, one call earlier
    /// than `post_init`, and it is the same value out of the same collection.
    ///
    /// Without these reads a `Menu` could not open and could not report a selection.
    /// The shipped menus carry both keys: layout `0x21000043`'s `0x1000035B` has
    /// `2 = 0x10000360` (the `ListBox` inside the popup) and `6 = 0x1000035F` (the
    /// popup root), with `7 = 0x21000043` naming its own layout.
    ///
    /// Popup creation itself — creating that popup root out of the layout attribute 7
    /// names — is **not** done here: it needs an asset source and this crate's element manager
    /// has none. It is the free function [`make_popup`], which the host
    /// calls with the asset source it already holds, followed by [`initialize_popup`]. These
    /// six ids are what both of them read.
    fn on_set_attribute(
        &mut self,
        ctx: &mut ElemCtx<'_>,
        id: u32,
        v: Option<&crate::PropertyValue>,
    ) {
        let element = match v {
            Some(crate::PropertyValue::Enum(e)) => Some(ElementId(*e)),
            Some(crate::PropertyValue::Integer(i)) => u32::try_from(*i).ok().map(ElementId),
            _ => None,
        };
        let did = match v {
            Some(crate::PropertyValue::DataFile(d)) => Some(*d),
            _ => None,
        };
        match id {
            ATTR_LIST_BOX => self.list_box_id = element,
            ATTR_POPUP_ROOT => self.popup_id = element,
            ATTR_ITEM_ELEMENT => self.item_id = element,
            ATTR_SELECTION_TEXT => self.selection_text_id = element,
            ATTR_POPUP_LAYOUT => self.popup_layout = did,
            ATTR_ITEM_LAYOUT => self.item_layout = did,
            _ => {}
        }
        self.button.on_set_attribute(ctx, id, v);
    }

    fn as_text_mut(&mut self) -> Option<&mut crate::text::TextElement> {
        self.button.as_text_mut()
    }

    /// The client's tail, in its order:
    ///
    /// ```text
    /// if popup: stop listening to the popup's element messages
    /// if open and popup: hide the popup; open = false
    ///                    bool attribute 0x0E = false
    ///                    broadcast message 9 with (0, 0)
    /// if popup: queue the popup for deletion; popup = none
    /// ```
    ///
    /// The close-shaped middle is spelled out inline in the destructor rather than being a
    /// call to the close, which is why it is written out here too.
    fn on_destroy(&mut self, ctx: &mut ElemCtx<'_>) {
        let Some(popup) = self.popup.take() else {
            return;
        };
        let me = ctx.me;
        ctx.ui
            .unregister_from_element(popup, crate::ListenerId::Element(me));
        if self.open {
            ctx.ui.set_visible(popup, false);
            self.open = false;
            ctx.ui.set_attribute_bool(me, ATTR_OPEN, false);
            ctx.ui
                .broadcast_element_message(me, msgid::MENU_CLOSED, 0, 0);
        }
        self.list_box = None;
        ctx.ui.add_to_delete_queue(popup);
    }

    fn compose_text(&self, screen: crate::Box2D) -> Vec<crate::text::PlacedGlyph> {
        self.button.compose_text(screen)
    }

    /// Delegated for the same reason [`Self::compose_text`] is:
    /// the original button, menu, and scrollbar share text-control outline behavior. This
    /// rebuild delegates the same element outline state through composition.
    ///
    /// Without this the outline would be drawn for a plain label and silently not
    /// for a button carrying the same attribute. 16 shipped elements are exactly that case.
    fn text_outline_color(&self) -> Option<u32> {
        self.button.text_outline_color()
    }

    /// Delegated for the same reason the two above are: the selection
    /// belongs to the original shared text behavior, so its color-inversion draw arm applies
    /// to this element too.
    fn selection_boxes(&self, screen: crate::Box2D) -> Vec<crate::Box2D> {
        self.button.selection_boxes(screen)
    }

    /// Delegated with the three above: the draw's caret arm is
    /// `TextElement`'s, so it is this widget's too.
    fn caret(&self, screen: crate::Box2D) -> Option<(crate::Box2D, u32)> {
        self.button.caret(screen)
    }

    /// The menu's element-message handler, in the client's own order.
    ///
    /// ```text
    /// from the popup:
    ///     0x2A while open and the pointer is not over the menu: close; return Stop
    /// from the list box:
    ///     4:    new_selection(true); close          // a row was picked
    ///     0x32: recalculate_popup_size; return Stop // the paper grew
    ///     0x43: close                               // row activated
    ///     any other: return Stop
    ///     return Stop
    /// from a type-5 element that is the list box: ...row rollover states...
    /// from the menu itself:
    ///     1:    close if open, else open; return Stop
    ///     0x1B: update the state;             return Stop
    /// otherwise: the button base's handler
    /// ```
    ///
    /// The open/close toggle is message **1**
    /// (`BUTTON_CLICKED`), not `0x19` — a menu is a `Button` and its own click
    /// arrives the way every other button's does — and this arm **chains to the button**,
    /// which is what raises that 1 in the first place. Without the chain a `Menu` would be
    /// the one button in the build that never ran the inherited click path.
    ///
    /// The `0x2A` arm is the client's click-away close: the popup is a root element, so
    /// clicking anywhere else deactivates it and the menu shuts.
    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        // Every call below is the `_now` form: this object is **out of its arena slot** for
        // the duration of this function, so the free functions cannot see it. See
        // [`with_taken`].
        let me = ctx.me;
        if Some(m.source) == self.popup {
            let over_top = ctx
                .ui
                .node(me)
                .is_some_and(|n| n.region.flags.mouse_over_top);
            if m.id == msgid::DEACTIVATED && self.open && !over_top {
                self.close_now(ctx.ui, me);
                return R::StopProcessing;
            }
            return R::Default;
        }
        if Some(m.source) == self.list_box {
            match m.id {
                msgid::LIST_SELECTION_CHANGED => {
                    // `p2` is the selected item; the list cannot be asked for it here.
                    let picked = (m.p2 != 0).then(|| ElemHandle::from_raw(m.p2));
                    self.new_selection_with(ctx.ui, me, picked, true);
                    self.close_now(ctx.ui, me);
                }
                msgid::SCROLL_OFFSET => self.recalculate_popup_size_now(ctx.ui, me),
                msgid::LIST_ITEM_ACTIVATED => {
                    self.close_now(ctx.ui, me);
                }
                _ => {}
            }
            return R::StopProcessing;
        }
        if m.source == ctx.me {
            if m.id == msgid::BUTTON_CLICKED {
                if self.open {
                    self.close_now(ctx.ui, me);
                } else {
                    self.open_now(ctx.ui, me);
                }
                return R::StopProcessing;
            }
            if m.id == msgid::MOUSE_OVER_TOP {
                // The menu's state update: bool attribute `0x0E` = the open flag.
                let open = self.open;
                ctx.ui.set_attribute_bool(me, ATTR_OPEN, open);
                return R::StopProcessing;
            }
        }
        self.button.listen_to_element_message(ctx, m)
    }
}
