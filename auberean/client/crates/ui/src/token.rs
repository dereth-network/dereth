//! Tokens: the 32-bit hashes that name every element, state, attribute, element kind and message
//! of the interface. A layout record stores only tokens; the client hashes the names it looks for.

/// A hashed name.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct Token(pub u32);

impl Token {
    /// The token of a name: each byte's low seven bits are placed at the top of a word, the word is
    /// rotated right by seven more bits than the byte before it, and the results are xored.
    // D2-14275, D2-14276, D2-14277, D2-14278
    #[must_use]
    pub const fn of(name: &str) -> Self {
        let bytes = name.as_bytes();
        let mut hash = 0u32;
        let mut shift = 0u32;
        let mut i = 0;
        while i < bytes.len() {
            let v = (bytes[i] as u32) << 25;
            hash ^= v.rotate_right(shift);
            shift = (shift + 7) & 31;
            i += 1;
        }
        Self(hash)
    }

    /// The raw word.
    #[must_use]
    pub const fn raw(self) -> u32 {
        self.0
    }
}

impl std::fmt::Debug for Token {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "#{:08X}", self.0)
    }
}

/// The element kinds a layout names.
pub mod kind {
    use super::Token;
    pub const REFERENCE: Token = Token::of("reference");
    pub const FIELD: Token = Token::of("field");
    pub const BUTTON: Token = Token::of("button");
    pub const TEXT: Token = Token::of("text");
    pub const METER: Token = Token::of("meter");
    pub const LISTBOX: Token = Token::of("listbox");
    pub const LISTBOX_MULTI_SIZE: Token = Token::of("listboxmsize");
    pub const SCROLLBAR: Token = Token::of("scrollbar");
    pub const RESIZEBAR: Token = Token::of("resizebar");
    pub const DRAGBAR: Token = Token::of("dragbar");
    pub const VIEWPORT: Token = Token::of("viewport");
    pub const PANEL: Token = Token::of("panel");
    pub const PANEL_PAGE: Token = Token::of("panel_page");
    pub const MENU: Token = Token::of("menu");
    pub const TREE: Token = Token::of("tree");
}

/// State names.
pub mod state {
    use super::Token;
    /// Every element's base state: its attributes, media and children apply in every state.
    pub const STATELESS: Token = Token::of("stateless");
    pub const NORMAL: Token = Token::of("normal");
    pub const NORMAL_ROLLOVER: Token = Token::of("normal_rollover");
    pub const NORMAL_PRESSED: Token = Token::of("normal_pressed");
    pub const HIGHLIGHT: Token = Token::of("highlight");
    pub const HIGHLIGHT_ROLLOVER: Token = Token::of("highlight_rollover");
    pub const HIGHLIGHT_PRESSED: Token = Token::of("highlight_pressed");
    pub const GHOSTED: Token = Token::of("ghosted");
    /// A drag bar held down, and let go.
    pub const PRESSED: Token = Token::of("pressed");
    /// A container under a dragged item that would take it, or would not.
    pub const DRAG_ROLLOVER_ACCEPT: Token = Token::of("drag_rollover_accept");
    pub const DRAG_ROLLOVER_REJECT: Token = Token::of("drag_rollover_reject");
}

/// Attribute names.
pub mod attr {
    use super::Token;
    pub const REFERENT: Token = Token::of("referent");
    pub const HIDE: Token = Token::of("hide");
    pub const MOUSE_INVISIBLE: Token = Token::of("mouse_invisible");
    pub const SIZE_IMAGE_TO_OWNER: Token = Token::of("size_image_to_owner");
    /// A top-level element drawn into no picture of its own: it takes the pointer over its
    /// rectangle but shows nothing.
    pub const NO_SURFACE_OBJECT: Token = Token::of("no_surface_object");
    pub const ALPHA_VALUE: Token = Token::of("alpha_value");
    // Buttons.
    pub const GHOSTED: Token = Token::of("ghosted");
    pub const HIGHLIGHTED: Token = Token::of("highlighted");
    pub const ROLLOVER: Token = Token::of("rollover");
    pub const RADIO_BUTTON: Token = Token::of("radio_button");
    pub const BOOLEAN_BUTTON: Token = Token::of("boolean_button");
    pub const BROADCAST_WHEN_GHOSTED: Token = Token::of("broadcast_when_ghosted");
    pub const MOUSE_EVENT_PASSTHROUGH: Token = Token::of("mouse_event_passthrough");
    /// A button that repeats its click while held: first after `hot_click_first_interval`
    /// seconds, then every `hot_click_interval`.
    pub const HOT_BUTTON: Token = Token::of("hot_button");
    pub const HOT_CLICK_FIRST_INTERVAL: Token = Token::of("hot_click_first_interval");
    pub const HOT_CLICK_INTERVAL: Token = Token::of("hot_click_interval");
    // Every element.
    pub const ACTIVATABLE: Token = Token::of("activatable");
    pub const ACTIVATE_ORDER: Token = Token::of("activate_order");
    pub const ACTIVATE_ON_SHOW: Token = Token::of("activate_on_show");
    pub const TOOLTIPS_ON: Token = Token::of("tooltips_on");
    pub const TOOLTIP_ID: Token = Token::of("tooltip_ID");
    pub const TOOLTIP_ENTRY: Token = Token::of("tooltip_entry");
    pub const TOOLTIP_TEXT_ID: Token = Token::of("tooltip_text_ID");
    /// A moved element stays wholly on the screen.
    pub const DISPLAY_CONSTRAINED: Token = Token::of("display_constrained");
    /// An element that takes dragged items, and whether it refuses them.
    pub const CONTAINER: Token = Token::of("container");
    pub const DISALLOW_DRAG_IN: Token = Token::of("disallow_drag_in");
    pub const DISALLOW_DRAG_OUT: Token = Token::of("disallow_drag_out");
    pub const DRAGABLE: Token = Token::of("dragable");
    /// A field that shows whether it would take the item dragged over it.
    pub const DRAG_ROLLOVER: Token = Token::of("drag_rollover");
    /// Resizing shows a line around the new size and resizes only when let go.
    pub const RESIZE_LINE: Token = Token::of("resize_line");
    pub const MIN_WIDTH: Token = Token::of("min_width");
    pub const MAX_WIDTH: Token = Token::of("max_width");
    pub const MIN_HEIGHT: Token = Token::of("min_height");
    pub const MAX_HEIGHT: Token = Token::of("max_height");
    /// The element's place is kept for the character.
    pub const SAVE_LOC: Token = Token::of("save_loc");
    /// The element says when it moves or changes size.
    pub const BROADCAST_LOC_CHANGE: Token = Token::of("broadcast_loc_change");
    // Drag bars.
    pub const DISABLED: Token = Token::of("disabled");
    pub const HIDE_WHEN_DISABLED: Token = Token::of("hide_when_disabled");
    pub const ALLOW_UPDATING: Token = Token::of("allow_updating");
    // Text.
    pub const ENTRY: Token = Token::of("entry");
    pub const FONT: Token = Token::of("font");
    pub const FONT_COLOR_RED: Token = Token::of("font_color_red");
    pub const FONT_COLOR_GREEN: Token = Token::of("font_color_green");
    pub const FONT_COLOR_BLUE: Token = Token::of("font_color_blue");
    pub const FONT_OUTLINE: Token = Token::of("font_outline");
    pub const CENTER: Token = Token::of("center");
    pub const RIGHT_JUSTIFY: Token = Token::of("right_justify");
    pub const CENTER_VERTICALLY: Token = Token::of("center_vertically");
    pub const ONE_LINE: Token = Token::of("one_line");
    pub const LEFT_MARGIN: Token = Token::of("left_margin");
    pub const RIGHT_MARGIN: Token = Token::of("right_margin");
    pub const TOP_MARGIN: Token = Token::of("top_margin");
    pub const BOTTOM_MARGIN: Token = Token::of("bottom_margin");
    pub const EDITABLE: Token = Token::of("editable");
    pub const SELECTABLE: Token = Token::of("selectable");
    pub const MAX_CHARS: Token = Token::of("max_chars");
    // Meters.
    pub const GOAL_POSITION: Token = Token::of("goal_position");
    pub const POSITION: Token = Token::of("position");
    pub const MOVE_FILL: Token = Token::of("move_fill");
    pub const LEFT_FILL: Token = Token::of("left_fill");
    pub const RIGHT_FILL: Token = Token::of("right_fill");
    // List boxes.
    pub const ITEM_NORMAL_STATE: Token = Token::of("item_normal_state");
    pub const ITEM_SELECTED_STATE: Token = Token::of("item_selected_state");
    pub const SELECTED_ITEM_STATE_CHANGE: Token = Token::of("selected_item_state_change");
    pub const MAX_COLUMNS: Token = Token::of("max_columns");
    pub const VERTICAL_SCROLLBAR: Token = Token::of("vertical_scrollbar");
    // Scroll bars.
    pub const WIDGET: Token = Token::of("widget");
    pub const PROPORTIONAL: Token = Token::of("proportional");
    pub const VERTICAL: Token = Token::of("vertical");
    pub const X_WIDGET_SIZE: Token = Token::of("x_widget_size");
    pub const Y_WIDGET_SIZE: Token = Token::of("y_widget_size");
    pub const MIN_WIDGET_WIDTH: Token = Token::of("min_widget_width");
    pub const MIN_WIDGET_HEIGHT: Token = Token::of("min_widget_height");
    pub const X_POSITION: Token = Token::of("x_position");
    pub const Y_POSITION: Token = Token::of("y_position");
    /// A scroll bar that snaps to `num_stop_locations` evenly spaced stops.
    pub const STOP_LOCATIONS: Token = Token::of("stop_locations");
    pub const NUM_STOP_LOCATIONS: Token = Token::of("num_stop_locations");
    /// A scroll bar's buttons that move it one stop, and whether they do.
    pub const BUTTONS_INTERACT: Token = Token::of("buttons_interact");
    pub const LEFT_BUTTON: Token = Token::of("left_button");
    pub const RIGHT_BUTTON: Token = Token::of("right_button");
    // Viewports.
    pub const UISCENE: Token = Token::of("uiscene");
}

/// The messages an element sends its listeners.
pub mod msg {
    use super::Token;
    /// A press on the element, the topmost under the pointer.
    pub const MOUSE_PRESS_TOP: Token = Token::of("mouse_press_top");
    /// A press on the element (every element under the pointer the press reaches), and a button
    /// pressed.
    pub const MOUSE_PRESS: Token = Token::of("mouse_press");
    /// The same message a button sends when pressed.
    pub const BUTTON_PRESS: Token = MOUSE_PRESS;
    /// A release on the element, and on the topmost.
    pub const MOUSE_RELEASE: Token = Token::of("mouse_release");
    pub const MOUSE_RELEASE_TOP: Token = Token::of("mouse_release_top");
    /// The pointer moved over the element, and over the topmost.
    pub const MOUSE_MOVE: Token = Token::of("mouse_move");
    pub const MOUSE_MOVE_TOP: Token = Token::of("mouse_move_top");
    /// The pointer came over the element or left it (the parameter says which), and became or
    /// stopped being the topmost under the pointer.
    pub const MOUSE_OVER: Token = Token::of("mouse_over");
    pub const MOUSE_OVER_TOP: Token = Token::of("mouse_over_top");
    /// A held hot button's repeated click.
    pub const HOT_CLICK: Token = Token::of("hot_click");
    /// A button's highlight turned on or off (the parameter).
    pub const HIGHLIGHT_STATE_CHANGE: Token = Token::of("highlight_state_change");
    /// An element moved or resized by the player.
    pub const LOCATION_CHANGED: Token = Token::of("location_changed");
    /// An element came to show or stopped showing (the parameter says which).
    pub const SET_SHOW_STATE: Token = Token::of("set_show_state");
    /// A button released with the pointer still on it: a click.
    pub const BUTTON_RELEASE_ON: Token = Token::of("mouse_release_on");
    /// A button released with the pointer off it.
    pub const BUTTON_RELEASE_OFF: Token = Token::of("mouse_release_off");
    /// A list box's selection changed.
    pub const NEW_SELECTION: Token = Token::of("new_selection");
    /// A scroll bar moved.
    pub const NEW_POSITION: Token = Token::of("new_position");
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the hash checked against names the client is known to use)
    use super::Token;

    #[test]
    fn hashes_match_the_names_the_client_uses() {
        assert_eq!(Token::of("HealthField").0, 0xB894_6705);
        assert_eq!(Token::of("stateless").0, 0x5CE1_FCAA);
        assert_eq!(Token::of("referent").0, 0xBCE0_425E);
        assert_eq!(Token::of("reference").0, 0xBCE0_5594);
        assert_eq!(Token::of("hide").0, 0xD1A7_2650);
    }
}
