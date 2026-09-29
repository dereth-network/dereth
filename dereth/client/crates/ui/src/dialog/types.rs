//! The seven dialog element types, 0x13–0x19.
//!
//! Every dialog subclass implements the
//! same three steps: creation, `set_data` (read the caller's properties into the child text
//! elements) and the element-message handler (watch for element message **1** from the known
//! child ids, write the answer into the dialog's data under property 0x92, close the dialog by
//! context, and stop processing the message).
//!
//! The dialog *element* here is deliberately thin: the factory ([`super::factory`]) owns the
//! queues and the answer, and the host owns the element creation, so what is left on the element is
//! the message watch and the modality flag.

use crate::dialog::base::{AnswerRole, DialogKind};
use crate::element::{Element, ElementMessageListenResult as R};
use crate::msg::element::id as msgid;
use crate::props::{attr, PropertyCollection, PropertyValue};
use crate::{ElemCtx, ElementDesc, ElementId, ElementMessage, LayoutDesc};

/// A dialog element.
#[derive(Debug)]
pub struct DialogElement {
    pub kind: DialogKind,
    /// The dialog context, set by the factory once the element exists.
    pub context: u64,
    /// The element id of the button that was pressed.
    pub answer: Option<ElementId>,
    /// Which arm that button was — see [`AnswerRole`].
    pub answer_role: Option<AnswerRole>,
    /// The string the *accept* arm harvested out of
    /// [`DialogKind::text_child`], for the two kinds that have a box; `Some("")` when the dialog
    /// was **cancelled**, which is writing an empty property `0x9C` and
    /// is a different thing from `None` (no answer yet).
    pub answer_text: Option<String>,
    /// The two menu kinds record the selected index: the accept arm reads the menu
    /// named by [`DialogKind::menu_child`], and the cancel arm
    /// records **-1** without consulting it at all; the selected index
    /// is asked for only on the accept arm. `None` = no answer yet.
    ///
    /// `-1` from the accept arm is a real answer and not an error: it is what
    /// the menu's selected-index query returns for a menu whose list box holds no selection, and
    /// the menu dialog's cancel writes the same literal into `0xA4`.
    pub answer_index: Option<i32>,
    /// Property 0xAC.
    pub modal: bool,
}

impl DialogElement {
    #[must_use]
    pub const fn new(kind: DialogKind) -> Self {
        Self {
            kind,
            context: 0,
            answer: None,
            answer_role: None,
            answer_text: None,
            answer_index: None,
            modal: false,
        }
    }

    /// The children whose element message 1 this kind treats as an answer, as an ordered
    /// **(accept, cancel)** pair.
    ///
    /// Each subclass has its own pair; answering the confirmation dialog's `0x17`/`0x19` for
    /// every kind would leave five of the seven kinds watching ids their own subclass never
    /// sends. The per-subclass table lives on [`DialogKind::answer_children`], where the
    /// screens that raise a dialog can consult the same rows.
    #[must_use]
    pub const fn answer_children(&self) -> (Option<ElementId>, Option<ElementId>) {
        self.kind.answer_children()
    }

    /// `set_data` — the part of it that belongs to the element rather than to the factory.
    ///
    /// Every subclass first applies the base dialog data (modal and timeout, which the
    /// factory's [`crate::dialog::Dialog`] already models) and then puts its **own** button
    /// captions on its own buttons: the confirmation dialog reads `0x90` onto
    /// child `0x17` and `0x91` onto `0x19`, the confirmation-text-input dialog
    /// reads `0x9A` onto `0x2E` and `0x9B` onto `0x2F`, and so on down
    /// [`DialogKind::caption_properties`].
    ///
    /// A property the caller did not supply leaves the shipped layout's own caption alone, which
    /// is what the client's property lookup does when the key is absent.
    pub fn set_data(&mut self, ctx: &mut ElemCtx<'_>, data: &PropertyCollection) {
        let (accept_id, cancel_id) = self.kind.answer_children();
        let (accept_prop, cancel_prop) = self.kind.caption_properties();
        for (id, prop) in [(accept_id, accept_prop), (cancel_id, cancel_prop)] {
            let (Some(id), Some(prop)) = (id, prop) else {
                continue;
            };
            // The client's property is a `StringInfo`; a host that has already resolved one hands
            // in a plain `String`. Both are accepted, and anything else is left alone.
            let text = match data.get(prop) {
                Some(PropertyValue::String(s) | PropertyValue::StringToken(s)) => Some(s.clone()),
                Some(PropertyValue::StringInfo(si)) => {
                    crate::text::element_text::resolve_string_info(ctx, si)
                }
                _ => None,
            };
            let (Some(text), Some(child)) = (text, ctx.ui.get_child_recursive(ctx.me, id)) else {
                continue;
            };
            if let Some(t) = ctx.ui.text_element_mut(child) {
                t.set_text(&text);
            }
        }
        self.set_menu_data(ctx, data);
    }

    /// The menu half of `set_data`, for the two kinds that have one.
    ///
    /// The menu dialog's `set_data` and the confirmation-menu dialog's both
    /// first find the menu descendant by id, cast it to a menu, and then run two
    /// arms over the caller's collection, in this order:
    ///
    /// 1. **the rows** — property `0xA0` / `0xA6`, an `Array` walked entry by entry into
    ///    the menu's insert-text-item;
    /// 2. **the initial selection** — property `0xA4` / `0xAB` as an integer, through
    ///    selecting item `n` when it exists.
    ///
    /// **Step 1 is not implemented here**; it belongs with
    /// menu-item creation and its text-item insertion. Without it a dialog raised from the
    /// shipped `Dialog` layout has an empty menu and the
    /// selected-index read answers **-1** — the client's own answer for that state, not a
    /// substitute for one. Step 2 is implemented, and it is a no-op against an empty menu for
    /// exactly the reason step 1 gives: an item past the end is null and
    /// selecting null clears the selection.
    fn set_menu_data(&mut self, ctx: &mut ElemCtx<'_>, data: &PropertyCollection) {
        let Some(menu_id) = self.kind.menu_child() else {
            return;
        };
        let Some(menu) = ctx.ui.get_child_recursive(ctx.me, menu_id) else {
            return;
        };
        let Some(key) = self.kind.answer_property() else {
            return;
        };
        let Some(index) = data
            .get_int(key)
            .or_else(|| data.get_enum(key).and_then(|v| i32::try_from(v).ok()))
        else {
            return;
        };
        crate::widgets::menu::set_selected_index(ctx.ui, menu, index);
    }

    /// The answer as the caller's collection would carry it: the key from
    /// [`DialogKind::answer_property`] and the value that subclass writes under it.
    ///
    /// `Message` and `Wait` return `None` because neither writes anything at all — they only
    /// close the dialog.
    ///
    /// `Menu` and `ConfirmationMenu` answer too: [`DialogKind::menu_child`] names the menu element
    /// each subclass resolves, and the answer is
    /// [`crate::UiSystem::menu_selected_index`] on it. So **five of the seven kinds answer, which
    /// is all five that write anything**.
    #[must_use]
    pub fn answer_property(&self) -> Option<(u32, PropertyValue)> {
        let key = self.kind.answer_property()?;
        let value = match self.kind {
            // The two text kinds answer with the string, empty when cancelled.
            DialogKind::ConfirmationTextInput | DialogKind::TextInput => {
                PropertyValue::String(self.answer_text.clone()?)
            }
            // Confirmation compares answer id 0x17 and stores equality as a Boolean; raw `answer` remains
            // available to older screen callers, but property 0x92 is Boolean, including programmatic cancel.
            DialogKind::Confirmation => {
                PropertyValue::Bool(self.answer_role? == AnswerRole::Accept)
            }
            // The two menu kinds answer with the menu's selected index, or -1.
            DialogKind::Menu | DialogKind::ConfirmationMenu => {
                PropertyValue::Integer(self.answer_index?)
            }
            DialogKind::Message | DialogKind::Wait => return None,
        };
        Some((key, value))
    }

    /// The menu dialog's cancel and the confirmation-menu dialog's
    /// — both write the integer `-1` under `0xA4` / `0xAB` and then close the dialog by its
    /// context, with the `-1` a literal.
    ///
    /// This is the arm a *timeout*, a *replace*, and a factory reset take, which is not the
    /// same as the cancel **button** (`0x23`) — that one runs through
    /// [`Element::listen_to_element_message`] and reaches the same value by the other road.
    pub fn cancel_dialog(&mut self) {
        if self.kind.menu_child().is_some() {
            self.answer_index = Some(-1);
        }
        if self.kind.text_child().is_some() {
            self.answer_text = Some(String::new());
        }
        self.answer_role = Some(AnswerRole::Cancel);
    }
}

impl Element for DialogElement {
    /// A dialog cast, which is how a framework reads the answer back off the element
    /// it created. The confirmation dialog's message handler writes
    /// the answer into the caller's `PropertyCollection` and then closes the dialog through the
    /// factory, which hands the caller's data to the completion callback.
    /// Older screen hosts read [`Self::answer`] directly. The App targeted-dialog host reads
    /// [`Self::answer_property`] and invokes its retained callback at the delivery boundary.
    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_dialog_mut(&mut self) -> Option<&mut Self> {
        Some(self)
    }

    fn on_set_attribute(&mut self, ctx: &mut ElemCtx<'_>, id: u32, v: Option<&PropertyValue>) {
        if id == attr::DIALOG_MODAL {
            self.modal = matches!(v, Some(PropertyValue::Bool(true)));
            // "modal" is exactly blocking clicks plus bring-to-front; there is no modal loop.
            if let Some(n) = ctx.ui.node_mut(ctx.me) {
                n.region.flags.block_clicks = self.modal;
            }
            if self.modal {
                ctx.ui.bring_to_front(ctx.me);
            }
        }
    }

    fn listen_to_element_message(&mut self, ctx: &mut ElemCtx<'_>, m: &ElementMessage) -> R {
        // Every confirmation dialog watches element message **1**, which is
        // the button *click* -- see [`msgid::BUTTON_CLICKED`]'s correction note.
        if m.id != msgid::BUTTON_CLICKED {
            // The base dialog's message handler: on 0x24 (resized) targeted at itself,
            // re-centre the popup.
            if m.id == msgid::RESIZED && m.source == ctx.me {
                ctx.ui.bring_to_front(ctx.me);
            }
            return R::Default;
        }
        let Some(role) = self.kind.answer_role(m.source_id) else {
            return R::Default;
        };
        self.answer = Some(m.source_id);
        self.answer_role = Some(role);
        // **The harvest.** The text-input dialog's `0x2E` arm reads child `0x2C`
        // as text into property `0x9C`; the confirmation-menu dialog does the same with `0x2B`
        // into `0x98`.
        // The `0x2F` arm instead cancels, which writes an **empty**
        // `0x9C` -- so a cancel is `Some("")` and never `None`, and the difference is the whole of
        // what stops a *Cancel* deleting a character on the char-select screen.
        if let Some(box_id) = self.kind.text_child() {
            self.answer_text = Some(match role {
                AnswerRole::Accept => ctx
                    .ui
                    .get_child_recursive(ctx.me, box_id)
                    .and_then(|c| ctx.ui.text_element_mut(c))
                    .map_or_else(String::new, |t| t.glyphs.inq_text(false)),
                AnswerRole::Cancel => String::new(),
            });
        }
        // **The menu harvest**, and it is the same shape as the text one above with
        // the asymmetry in a different place. The menu dialog's message handler's
        // `0x1E` arm writes the menu's selected index under `0xA4`;
        // the confirmation-menu dialog starts its answer at `-1`
        // and reads the selected index **only** on the accept arm, so its `0x23`
        // arm writes `-1` having never looked at the menu. A menu that is empty answers -1 by the
        // same route, which is the selected-index query's own answer and not a stand-in:
        // until the dialog menu's rows are filled (see `set_menu_data`), every dialog raised
        // from the shipped layout is in exactly that state.
        if let Some(menu_id) = self.kind.menu_child() {
            self.answer_index = Some(match role {
                AnswerRole::Accept => ctx
                    .ui
                    .get_child_recursive(ctx.me, menu_id)
                    .map_or(-1, |m| ctx.ui.menu_selected_index(m)),
                AnswerRole::Cancel => -1,
            });
        }
        R::StopProcessing
    }
}

macro_rules! dialog_ctor {
    ($name:ident, $kind:expr) => {
        pub fn $name(_l: &LayoutDesc, _d: &ElementDesc) -> Box<dyn Element> {
            Box::new(DialogElement::new($kind))
        }
    };
}

dialog_ctor!(create_confirmation, DialogKind::Confirmation);
dialog_ctor!(create_confirmation_menu, DialogKind::ConfirmationMenu);
dialog_ctor!(
    create_confirmation_text_input,
    DialogKind::ConfirmationTextInput
);
dialog_ctor!(create_menu, DialogKind::Menu);
dialog_ctor!(create_message, DialogKind::Message);
dialog_ctor!(create_text_input, DialogKind::TextInput);
dialog_ctor!(create_wait, DialogKind::Wait);

/// A subclass's `set_data` on a live dialog element — the caller's collection in, the button
/// captions out onto the subclass's own buttons.
///
/// The dialog factory calls `set_data` on the element it has just created,
/// immediately before `update_popup_size_and_position`. Returns `false` when `h` is not a dialog.
pub fn set_dialog_data(
    ui: &mut crate::UiSystem,
    h: crate::ElemHandle,
    data: &PropertyCollection,
) -> bool {
    let Some(mut b) = ui.take_behaviour(h) else {
        return false;
    };
    let applied = match b.as_dialog_mut() {
        Some(d) => {
            d.set_data(&mut ElemCtx { ui, me: h }, data);
            true
        }
        None => false,
    };
    ui.put_behaviour(h, b);
    applied
}

/// The [`DialogElement`] on a live element, or `None` when the element is not a dialog.
#[must_use]
pub fn dialog_element(ui: &crate::UiSystem, h: crate::ElemHandle) -> Option<&DialogElement> {
    ui.node(h)?
        .behaviour
        .as_ref()?
        .as_any()?
        .downcast_ref::<DialogElement>()
}

/// The constructor for one kind, for the factory registration in
/// [`crate::factory::register_engine_classes`].
#[must_use]
pub fn ctor_for(kind: DialogKind) -> crate::ElementCtor {
    match kind {
        DialogKind::Confirmation => create_confirmation,
        DialogKind::ConfirmationMenu => create_confirmation_menu,
        DialogKind::ConfirmationTextInput => create_confirmation_text_input,
        DialogKind::Menu => create_menu,
        DialogKind::Message => create_message,
        DialogKind::TextInput => create_text_input,
        DialogKind::Wait => create_wait,
    }
}

#[cfg(test)]
mod confirmation_bool_tests {
    use super::*;

    #[test]
    fn property92_is_boolean_while_raw_button_identity_remains_available() {
        let mut d = DialogElement::new(DialogKind::Confirmation);
        assert_eq!(d.answer_property(), None);
        d.answer = Some(ElementId(0x17));
        d.answer_role = Some(AnswerRole::Accept);
        assert_eq!(d.answer_property(), Some((0x92, PropertyValue::Bool(true))));
        assert_eq!(d.answer, Some(ElementId(0x17)));
        d.answer = Some(ElementId(0x19));
        d.answer_role = Some(AnswerRole::Cancel);
        assert_eq!(
            d.answer_property(),
            Some((0x92, PropertyValue::Bool(false)))
        );
        let mut cancelled = DialogElement::new(DialogKind::Confirmation);
        cancelled.cancel_dialog();
        assert_eq!(
            cancelled.answer_property(),
            Some((0x92, PropertyValue::Bool(false)))
        );
        assert_eq!(
            cancelled.answer, None,
            "programmatic cancel is not a fake button click"
        );
    }
}
