//! The interface: elements built from the layout records, the pointer, the compositor, and the
//! game's panels on them.
//!
//! A row claims what the September 2013 client's interface does.

use super::{Behaviour, Evidence, Tier, RETAIL, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "ui.button.a-button-shows-its-normal-state-before-the-pointer-touches-it",
        says: "A button its layout names by reference, whose own first state is the plain one, is in its normal state and draws its picture as soon as it is made: the ghosted, highlighted or rollover flags its template sets bring its state up to date.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BUTTON-FIRST-STATE"),
        station: "auberean-ui::dat::layouts::a_button_shows_its_normal_state_before_the_pointer_touches_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.button.a-ghosted-button-never-clicks",
        says: "A ghosted button shows its ghosted look whatever the pointer does, and a press and \
               release on it sends no click.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BUTTON-GHOSTED"),
        station: "auberean-ui::cpu::pointer::a_ghosted_button_never_clicks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.button.a-release-on-the-button-clicks",
        says: "A button rolls over under the pointer, shows pressed while held, and clicks when the \
               press is released on it; released off it, it sends no click.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BUTTON-CLICK"),
        station: "auberean-ui::cpu::pointer::a_release_on_the_button_clicks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.drag.an-item-dragged-is-taken-by-a-container-or-comes-back",
        says: "An element held with the left button and moved more than a few pixels (or held \
               a third of a second) is dragged out of its container as a copy that follows the \
               pointer, carrying what the element was given; let go over a container it is taken \
               in, over anything else it comes back. The copy dropped stays, hidden, until the next \
               drag starts.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAG-DROP"),
        station: "auberean-ui::cpu::framework::an_item_dragged_is_taken_by_a_container_or_comes_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.drag.the-dragged-copy-shows-in-its-items-size",
        says: "The copy dragged with the pointer is shown in the size of the item it was copied from, its corner at the pointer: on a screen larger than its layout's, a copy that grows with the screen shows nothing past that size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAG-AREA"),
        station: "auberean-ui::cpu::framework::the_dragged_copy_shows_in_its_items_size",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.a-centered-picture-is-drawn-once-in-the-middle",
        says: "A picture its element asks to center is drawn once in the element's middle, half the \
               room left over before it, neither repeated nor stretched even when the element asks to \
               fit it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-CENTERED"),
        station: "auberean-ui::cpu::compositor::a_centered_picture_is_drawn_once_in_the_middle",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.a-picture-of-its-own-laid-over-the-scene-matches-drawing-over-it",
        says: "The interface drawn into a transparent picture of its own and laid over the scene by that picture's alpha matches the interface drawn straight over the scene.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-OWN-PICTURE"),
        station: "auberean-ui::cpu::compositor::a_picture_of_its_own_laid_over_the_scene_matches_drawing_over_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.a-small-picture-repeats-unless-sized-to-its-element",
        says: "A picture smaller than its element repeats across it from the element's top left; \
               one the element asks to fit to it is stretched over it instead.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-TILE"),
        station: "auberean-ui::cpu::compositor::a_small_picture_repeats_unless_sized_to_its_element",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.blended-pictures-keep-the-layers-alpha-copied-ones-replace-it",
        says: "Pictures of draw mode 1 and 2 blend over what the element's layer already holds and \
               keep the layer's alpha; mode 0 copies its texels, alpha included.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-BLEND"),
        station: "auberean-ui::cpu::compositor::blended_pictures_keep_the_layers_alpha_copied_ones_replace_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.mode-three-fades-by-the-elements-alpha",
        says: "A picture of draw mode 3 is copied and faded by its element's transparency.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-FADE"),
        station: "auberean-ui::cpu::compositor::mode_three_fades_by_the_elements_alpha",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.the-nearest-top-level-element-is-drawn-last",
        says: "Top-level elements are laid over each other from the deepest, the nearest drawn \
               last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-DEPTH"),
        station: "auberean-ui::cpu::compositor::the_nearest_top_level_element_is_drawn_last",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.draw.the-revision-moves-only-with-a-change",
        says: "The element tree's revision moves when anything drawn may have changed and stays put otherwise.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UI-REVISION"),
        station: "auberean-ui::cpu::compositor::the_revision_moves_only_with_a_change",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.a-child-keeps-its-own-picture-through-its-parents-states",
        says: "A child the layout made, given a picture of its own, stays itself when its parent changes state: it keeps its picture and goes back to the layout's description.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHILD-SWITCHED"),
        station: "auberean-ui::cpu::elements::a_child_keeps_its_own_picture_through_its_parents_states",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.a-kept-place-comes-back-at-the-same-share-of-the-screen",
        says: "An element's kept place is its corners in the screen's clip space less a pixel's \
               share; put back on a screen of another size it stands at the same share of the \
               screen, to the pixel the conversion's rounding gives.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LOCATION"),
        station: "auberean-ui::cpu::media::a_kept_place_comes_back_at_the_same_share_of_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.a-reference-is-its-template-placed-where-the-reference-stands",
        says: "An element made from a reference takes its kind from the template it names, its \
               place from where the layout puts the reference, and its children from both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-REFERENCE"),
        station: "auberean-ui::cpu::elements::a_reference_is_its_template_placed_where_the_reference_stands",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.a-state-brings-its-children-and-passes-itself-on",
        says: "Entering a state makes the children it describes and drops those of the state left; \
               a state marked to pass itself on puts every child into the same state.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STATE"),
        station: "auberean-ui::cpu::elements::a_state_brings_its_children_and_passes_itself_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.a-top-level-element-is-kept-on-the-screen",
        says: "A top-level element reaching past the screen's bottom or right edge is moved back \
               onto the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ON-SCREEN"),
        station: "auberean-ui::cpu::elements::a_top_level_element_is_kept_on_the_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.edges-follow-the-screen-as-it-grows",
        says: "On a screen larger than its layout's, each edge keeps its distance from the side it \
               follows, or centers the element.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EDGES"),
        station: "auberean-ui::cpu::elements::edges_follow_the_screen_as_it_grows",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.edges-grow-it-no-larger-than-its-greatest-size",
        says: "The size an element's edges give it on a larger screen is held within its least and \
               greatest width and height, as any resize is: a toolbar window keeps its width and \
               grows no taller than its greatest height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SIZE-LIMITS"),
        station: "auberean-ui::cpu::elements::edges_grow_it_no_larger_than_its_greatest_size",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.lower-depth-is-nearer-and-the-newest-of-equals",
        says: "An element's children are kept nearest first: the lower depth in front, and of equal \
               depths the one made later.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHILD-ORDER"),
        station: "auberean-ui::cpu::elements::lower_depth_is_nearer_and_the_newest_of_equals",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.one-name-holds-a-value-of-each-type",
        says: "An element keeps its attributes in a table per type, so one name may hold a \
               value of each type; within one change of state the first value given for a name \
               wins, whatever its type.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ATTRIBUTE-TABLES"),
        station: "auberean-ui::cpu::media::one_name_holds_a_value_of_each_type",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.the-first-value-given-wins",
        says: "An element's attributes take the first value given: the reference's before the \
               template's, the current state's before the base state's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ATTRIBUTES"),
        station: "auberean-ui::cpu::elements::the_first_value_given_wins",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.flow.a-screen-asked-for-comes-up-at-the-next-tick",
        says: "A screen asked for comes up at the next tick of the clock: the screen up is hidden and \
               closed, the new one made, given the text it was asked with, and shown.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-FLOW"),
        station: "auberean-ui::cpu::framework::a_screen_asked_for_comes_up_at_the_next_tick",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.frame.a-panel-shows-only-while-its-parents-do",
        says: "A panel's framework shows only while it and every framework above it show; showing a \
               framework tells each child to show again by its own flag; a screen's top removes \
               the elements it made when it goes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-FRAME-SHOWN"),
        station: "auberean-ui::cpu::framework::a_panel_shows_only_while_its_parents_do",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.frame.kept-places-come-back",
        says: "A window whose place is kept is put back where the player left it; one whose edges stay \
               where placed keeps its place when the display is set up again at another size.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-KEPT-PLACES"),
        station: "auberean-ui::cpu::framework::kept_places_come_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.layout.a-screen-finds-its-layout-by-number",
        says: "A screen finds its layout record by its layout number through the data's \n               layout mapping.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LAYOUT-BY-NUMBER"),
        station: "auberean-ui::dat::layouts::a_screen_finds_its_layout_by_number",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.layout.every-retail-layout-builds",
        says: "Every top-level element of every full-screen interface layout of the September 2013 \
               install builds into an element tree and draws.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LAYOUTS"),
        station: "auberean-ui::dat::layouts::every_retail_layout_builds",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.layout.the-character-list-buttons-stand-where-retail-draws-them",
        says: "The character list's buttons stand where the retail screen draws them, 142 by 35 \
               each, the first at (800, 300).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-PLACES"),
        station: "auberean-ui::dat::layouts::the_character_list_buttons_stand_where_retail_draws_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.list.a-pressed-row-is-selected",
        says: "A list's rows stack from its top; pressing one selects it, giving it the list's \
               selected state and the row selected before the normal state.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST"),
        station: "auberean-ui::cpu::pointer::a_pressed_row_is_selected",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.list.a-smooth-list-glides-to-its-goal-over-its-movement-time",
        says: "A smooth list glides to a goal view: half way after half its movement time, there at the \
               end, the goal then forgotten.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-SMOOTH"),
        station: "auberean-ui::cpu::lists::a_smooth_list_glides_to_its_goal_over_its_movement_time",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.list.arrow-buttons-scroll-a-row-and-ghost-at-the-ends",
        says: "A list's arrow buttons scroll it a row at a time to its last full view; the up arrow is \
               ghosted at the top and the down arrow at the bottom.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-ARROWS"),
        station: "auberean-ui::cpu::lists::arrow_buttons_scroll_a_row_and_ghost_at_the_ends",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.list.the-wheel-scrolls-a-list-through-its-scroll-bar",
        says: "The wheel over a list goes to its scroll bar, which tells the list to scroll a line; the \
               bar's position follows the view.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-WHEEL"),
        station: "auberean-ui::cpu::lists::the_wheel_scrolls_a_list_through_its_scroll_bar",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.listener.registrations-take-effect-in-order",
        says: "A listener hears an element's message only once registered for it; an undoing takes \
               effect at the next delivery; a global message registered twice needs two undoings.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LISTENERS"),
        station: "auberean-ui::cpu::framework::registrations_take_effect_in_order",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.manager.elements-by-name-and-removal-at-the-next-tick",
        says: "An element is found by name, the oldest first, or within one top-level element; an \
               element asked to go is hidden at once and removed at the next tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ELEMENTS-BY-NAME"),
        station: "auberean-ui::cpu::framework::elements_by_name_and_removal_at_the_next_tick",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.media.a-pause-holds-the-list-and-a-jump-plays-it-again",
        says: "A pause holds a state's media list until its rolled length is over, then starts \
               over; a jump plays the list again from the item it names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MEDIA-PAUSE-JUMP"),
        station: "auberean-ui::cpu::media::a_pause_holds_the_list_and_a_jump_plays_it_again",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.media.a-top-level-element-fades-as-it-enters-and-leaves-its-states",
        says: "A top-level element runs the fades of its states: as it is made (from no state) \
               and as it enters a state, and as it leaves one, when it stays in the old state \
               until the fade is done; it is drawn at the alpha the fade reached.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-FADES"),
        station: "auberean-ui::cpu::media::a_top_level_element_fades_as_it_enters_and_leaves_its_states",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.media.an-animation-shows-the-frame-its-time-reached-then-the-list-moves-on",
        says: "An animation shows the frame its time has reached, the time since it started \
               over one frame's share of its length, and holds the media list until its last \
               frame shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MEDIA-ANIMATION"),
        station: "auberean-ui::cpu::media::an_animation_shows_the_frame_its_time_reached_then_the_list_moves_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.media.messages-are-said-and-states-change",
        says: "A message item makes the element say its message, and a state item puts the \
               element in another state once the change under way is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MEDIA-MESSAGE-STATE"),
        station: "auberean-ui::cpu::media::messages_are_said_and_states_change",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.media.sounds-go-to-the-host-and-cursors-to-their-element",
        says: "A sound item asks the host to play its sound when its state plays; a cursor item \
               gives its element a cursor, shown while the element is topmost under the \
               pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MEDIA-SOUND-CURSOR"),
        station: "auberean-ui::cpu::media::sounds_go_to_the_host_and_cursors_to_their_element",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.menu.a-menu-opens-its-list-and-closes-on-a-choice",
        says: "A menu opens its popup under itself sized to its rows; choosing a row selects it, the menu \
               says what was chosen, and letting go on the list closes the popup.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MENU"),
        station: "auberean-ui::cpu::lists::a_menu_opens_its_list_and_closes_on_a_choice",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.menu.a-menu-shows-its-choice-in-its-display",
        says: "A menu's selection display shows the chosen row's text, whether the player or the \
               program chose it, and is emptied when nothing is chosen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MENU-DISPLAY"),
        station: "auberean-ui::cpu::lists::a_menu_shows_its_choice_in_its_display",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.meter.a-meter-takes-its-goal-at-once-or-smoothly",
        says: "A meter takes a goal position at once, its fill following; a smooth meter moves toward it \
               over its movement time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-METER-GOAL"),
        station: "auberean-ui::cpu::lists::a_meter_takes_its_goal_at_once_or_smoothly",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.panel.opening-a-tab-closes-the-others",
        says: "A panel's tabs each make their page beside them and show it only while open; pressing a \
               closed tab opens it and closes the one that was open; a closed tab rolls over under \
               the pointer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PANEL-TABS"),
        station: "auberean-ui::cpu::framework::opening_a_tab_closes_the_others",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.pointer.mouse-invisible-elements-let-the-pointer-through",
        says: "An element marked invisible to the mouse lets the pointer through to what is beneath \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MOUSE-INVISIBLE"),
        station: "auberean-ui::cpu::pointer::mouse_invisible_elements_let_the_pointer_through",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.scrollbar.dragging-the-widget-scrolls-its-list",
        says: "Dragging a scroll bar's widget moves the bar to the widget's share of the way, and its list \
               takes that share of its hidden rows, kept on a row's edge.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SCROLLBAR-DRAG"),
        station: "auberean-ui::cpu::lists::dragging_the_widget_scrolls_its_list",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.shortcuts.an-item-on-the-bar-is-named-and-shown-as-the-pack-shows-it",
        says: "At each recorded log-off, the first items of the player's pack put on an empty slot of the bar are named and shown on the bar as the pack shows them: the name for the item's quantity and the icon its scripts give.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-ITEM-LOOK"),
        station: "auberean-client-runtime::corpus::toolbar::an_item_on_the_bar_is_named_and_shown_as_the_pack_shows_it",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "ui.shortcuts.the-toolbar-from-the-scripts-names-and-shows-each-slot",
        says: "Each time a recorded session's options came again, the toolbar asked of the player's scripts holds the player's shortcuts slot for slot; every skill slot gets the skill's name and an icon, every empty slot the same empty slot's tooltip and no icon; every skill a slot holds is trained; the height is 1 to 5 rows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-TOOLBAR"),
        station: "auberean-client-runtime::corpus::toolbar::the_toolbar_from_the_scripts_names_and_shows_each_slot",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "ui.statusbar.the-effects-from-the-scripts-are-the-players-effects",
        says: "Each time a recorded session's scripts said an effect on the player changed, the effects the player's scripts list for the effects panel each have a number, a name and an icon, no number twice, and each was among the player's effects when the session started or was added by the server since, and was not taken off since.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EFFECT-LIST"),
        station: "auberean-client-runtime::corpus::effects::the_effects_from_the_scripts_are_the_effects_the_server_gave_the_player",
        tier: Tier::Corpus,
    },
    behaviour! {
        id: "ui.text.strings-take-the-elements-numbered-font-colors",
        says: "A string added to a text element in one of its numbered colors takes that color's \
               red, green and blue attributes, one in the element's own color its plain font color \
               attributes; a channel the element lacks reads as full.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-FONTS"),
        station: "auberean-ui::cpu::text::strings_take_the_elements_numbered_font_colors",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.the-caret-shows-steadily-then-blinks",
        says: "An editable entry's caret shows steadily for a second and a half after it moves, \
               then blinks, a second on and a second off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-CARET"),
        station: "auberean-ui::cpu::text::the_caret_shows_steadily_then_blinks",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.the-view-scrolls-by-page-and-to-either-end",
        says: "A text taller than its element scrolls to its end with the last lines at the foot, \
               a page at a time within the text, and back home.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-VIEW"),
        station: "auberean-ui::cpu::text::the_view_scrolls_by_page_and_to_either_end",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.typing-and-editing-keys-work-at-the-caret",
        says: "An editable entry pressed takes the keyboard; typing goes in at the caret up to its \
               greatest length; the arrows, Home and End move the caret, Backspace and Delete drop \
               a character, Shift with an arrow selects and typing replaces the selection, Ctrl+C \
               copies and Ctrl+V pastes over it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-EDIT"),
        station: "auberean-ui::cpu::text::typing_and_editing_keys_work_at_the_caret",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.tooltip.a-tooltip-shows-once-the-pointer-rests-and-goes-when-it-leaves",
        says: "Once the pointer has rested a quarter of a second on an element that shows tooltips, \
               its tooltip template comes up in front of everything with the element's tip text, \
               sized to the text, past the cursor; it goes when the pointer moves to another \
               element.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLTIP"),
        station: "auberean-ui::cpu::text::a_tooltip_shows_once_the_pointer_rests_and_goes_when_it_leaves",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.tree.a-nodes-lines-run-from-its-side-to-each-childs",
        says: "A tree node's lines run from the middle of one of its sides to the middle of \
               each child's facing side, unless the elements name their points; a node taken \
               away leaves its parents' children.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TREE-NODES"),
        station: "auberean-ui::cpu::media::a_nodes_lines_run_from_its_side_to_each_childs",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.tree.children-hang-from-their-parents-by-lines",
        says: "A tree hangs each node from the node that names it as a child and draws a line from the \
               parent's connection point to the child's, in its line color.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TREE"),
        station: "auberean-ui::cpu::lists::children_hang_from_their_parents_by_lines",
        tier: Tier::Cpu,
    },
];
