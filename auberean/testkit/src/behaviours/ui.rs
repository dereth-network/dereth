//! The interface: elements built from the layout records, the pointer, the compositor, and the
//! game's panels on them.
//!
//! A row claims what the September 2013 client's interface does.

use super::{Behaviour, Evidence, Tier, RETAIL, THIS_CLIENT, TOOLING};

/// This subject's rows, in id order.
pub static ROWS: &[Behaviour] = &[
    behaviour! {
        id: "ui.book.a-book-shows-its-pages-and-turns-them",
        says: "A book the scripts open shows its title and its first page, each page laid out a \
               whole number of the text's pages and the count at most a thousand; the slider's \
               arrow turns a page, and a press on the picture turns to the next page and closes \
               the book after the last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BOOK"),
        station: "auberean-ui-game::dat::play::book::a_book_shows_its_pages_and_turns_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.book.a-books-text-splits-into-pages-and-headings",
        says: "A book's text splits at each <BR> into a page; a heading between <H> and </H> goes \
               beside its page with the line after it dropped, a page without one has a lone line \
               there, and what follows the last <BR> is no page.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-BOOK-PAGES"),
        station: "auberean-ui-game::dat::play::book::a_books_text_splits_into_pages_and_headings",
        tier: Tier::Dat,
    },
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
        id: "ui.camera.mouse-look-first-person-and-the-arrows",
        says: "Mouse look comes on while its key is held and its letting go switches it over; its \
               switch turns it on and off; the crosshair shows only in mouse look; the left button \
               runs forward while mouse look is wanted; the first person key switches first person; \
               the arrows ask to turn and move at 1 and 0, and nothing while a text element or a list \
               has the keyboard.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CAMERA"),
        station: "auberean-ui-game::dat::play::standing::mouse_look_first_person_and_the_arrows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.charselect.a-row-past-the-accounts-limit-is-not-a-deleted-one",
        says: "A character's time until it is purged reads nothing for a row past the number the \
               account may have, so it lists and enters as a living one; a row holding the invalid \
               id has no character.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-LIMIT"),
        station: "auberean-ui-game::cpu::flow::a_row_past_the_accounts_limit_is_not_a_deleted_one",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.charselect.an-empty-list-leaves-enter-and-delete-ghosted",
        says: "With no character on the account, creation, the controls and the credits are \
               offered once the list arrives, but Enter and Delete stay ghosted: no row is \
               selected to light them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-EMPTY"),
        station: "auberean-ui-game::dat::screens::pregame::an_empty_list_leaves_enter_and_delete_ghosted",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.charselect.create-options-and-credits-wait-and-go-on",
        says: "The character list's Create, Options and Credits each put please wait up, ghost \
               the buttons and ask for their screen: creation's hall, the key map, the credits.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-SCREENS"),
        station: "auberean-ui-game::dat::screens::pregame::create_options_and_credits_wait_and_go_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.charselect.delete-asks-first",
        says: "The character list's Delete asks first, naming the character, the buttons ghosted \
               while it waits; yes deletes that character, no lights the buttons that never wait \
               and shows the list again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-DELETE"),
        station: "auberean-ui-game::dat::screens::pregame::delete_asks_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.charselect.the-buttons-follow-the-selected-character",
        says: "The character list's buttons stay ghosted until the characters arrive; then a \
               living character can be entered or deleted, a deleted one only restored, and \
               creation is offered while slots remain; Enter puts please wait up, ghosts the \
               buttons and asks for the world and the play screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT"),
        station: "auberean-ui-game::dat::screens::pregame::the_buttons_follow_the_selected_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.charselect.the-carousel-turns-to-the-selected-row",
        says: "Character select's carousel has an arm per slot, the selected row's at the front: \
               the list comes back on its row at once, another row selected turns it an arm's \
               step at a time, a second and a half each, the shorter way round, and a press on a \
               character's figure selects its row.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHARSELECT-CAROUSEL"),
        station: "auberean-ui-game::dat::screens::pregame::the_carousel_turns_to_the_selected_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.a-line-before-the-first-options-is-not-shown",
        says: "A line the scripts write before they set the options is taken by what the windows show then: the place's status told to a new character right after its description never appears, as the windows show nothing until the options are set; the lines after the options do.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-LINE-BEFORE-OPTIONS"),
        station: "auberean-ui-game::dat::play::chat::a_line_before_the_first_options_is_not_shown",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.a-line-waits-in-the-log-for-its-string-tables",
        says: "A chat line that came as a string reference whose table the data lacks waits in the \
               log until the table has come, then shows as written from it; the lines after it \
               wait behind it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-CHAT"),
        station: "auberean-ui-game::dat::play::chat::a_line_waits_in_the_log_for_its_string_tables_and_the_lines_after_it_wait_behind_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.a-tell-asked-for-goes-in-the-entry",
        says: "A tell the player's scripts ask for puts the tell command with the name in the                entry of the window keys are for, selected, and starts typing there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-ASSISTED-TELL"),
        station: "auberean-ui-game::dat::play::chat::a_tell_asked_for_goes_in_the_entry",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.each-error-line-plays-the-warning-sound",
        says: "An error line stands in the line of messages with its warning sound, once per line: two error lines as a character enters play it twice.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-WARNING-SOUND"),
        station: "auberean-ui-game::dat::play::chat::each_error_line_plays_the_warning_sound",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.each-window-shows-the-kinds-its-filter-holds-in-their-colors",
        says: "The chat panel makes four windows, the first shown and the others hidden; once the chat options are read the first entry holds the prompt and a devoted window's entry its channel's name; each window shows the kinds of text its filter holds, each line whole in its kind's color; an error line also stands in the line of messages at the screen's top for the six seconds its row says.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-WINDOWS"),
        station: "auberean-ui-game::dat::play::chat::each_window_shows_the_kinds_its_filter_holds_in_their_colors",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-buttons-and-keys-show-the-other-windows-and-a-devoted-window-speaks-on-its-channel",
        says: "The first window's buttons and the windows' keys show and hide the other windows; a line for a hidden window changes its button's look, and a kind marked to pop up shows the window; a line typed in a devoted window goes as its channel's command.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-DEVOTED"),
        station: "auberean-ui-game::dat::play::chat::the_buttons_and_keys_show_the_other_windows_and_a_devoted_window_speaks_on_its_channel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-entry-is-labelled-with-the-chat-strings",
        says: "The chat's strings are written as the client writes them: the assisted tell is the command's mark, \"tell\", the name and a comma; the reply command's names start with \"reply\".",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-STRINGS"),
        station: "auberean-ui-game::dat::play::chat::the_entry_is_labelled_with_the_chat_strings",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-entry-takes-a-line-and-hands-it-to-the-command-line",
        says: "Enter starts typing (the prompt goes) and hands the line typed to the player's command line, the entry emptied; / and ; start a command and an alias with their mark; Escape stops typing at the next tick, the text kept; the arrows ask for the lines typed, with Ctrl the tells sent and with Shift the tells answered, only while typing, and what comes back fills the entry.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-ENTRY"),
        station: "auberean-ui-game::dat::play::chat::the_entry_takes_a_line_and_hands_it_to_the_command_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-entry-types-in-the-chat-entrys-color",
        says: "What is typed in a chat window's entry is drawn in the chat entry's font and color \
               from the chat options (a new player's: white), not in the entry's own red, the \
               error lines' color; a window devoted to a channel types in that channel's color.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-ENTRY-COLOR"),
        station: "auberean-ui-game::dat::play::chat::the_entry_types_in_the_chat_entrys_color",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-lines-written-with-the-options-show-by-those-options",
        says: "As a character enters, its scripts set its options and write the lines about what it could not reach in one go; the windows take those lines by the options the scripts left, whether the client read them before the frame or only at the next: the first window shows the errors and the standard lines, and the errors stand in the line of messages at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-ENTRY-LINES"),
        station: "auberean-ui-game::dat::play::chat::the_lines_written_with_the_options_show_by_those_options",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.the-lock-holds-the-log-and-the-slider-fades-the-window",
        says: "The lock button locks the log against new lines until pressed again; the options button shows the opacity slider, which fades the whole window and keeps the opacity with the player's options when let go.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CHAT-LOCK"),
        station: "auberean-ui-game::dat::play::chat::the_lock_holds_the_log_and_the_slider_fades_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.chat.with-alt-held-the-chat-window-takes-none-of-its-keys",
        says: "A chat window takes its keys (the arrows through the lines typed, the tells sent and \
               the tells answered, the page keys, Home, End and Escape) only with Alt up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CCAMERA-CHAT-ALT"),
        station: "auberean-interface::dat::screens::alt_reaches_the_interface_from_either_key_and_goes_with_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.a-tall-window-grows-the-list-and-not-the-head",
        says: "On a window taller than 768, the head of a craft skill's categories or a \
               category's recipes keeps its height, the craft skill's display goes down with the \
               page's foot and the list takes the rest, and a press on a row reaches the row.",
        since: THIS_CLIENT,
        divergence: "CD2-craft-page-list-takes-a-tall-window",
        evidence: Evidence::Private("AC2-EVID-UI-CRAFT-TALL"),
        station: "auberean-ui-game::dat::play::craft::a_tall_window_grows_the_list_and_not_the_head",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.recipe-and-ingredient-names-wait-for-their-string-tables",
        says: "The ingredient panel gives its texts (the recipe's name, its cost and charges, its \
               ingredients' names, the craft button's label and tip) to their text elements as \
               string references: one whose string tables the data lacks shows once they have \
               come. The recipe's name is then colored, written again from its element's text, \
               so a name still waiting stays empty.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-INGREDIENTS"),
        station: "auberean-ui-game::dat::play::ingredients::recipe_and_ingredient_names_wait_for_their_string_tables",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.the-crafting-page-goes-from-craft-skills-to-categories-to-recipes",
        says: "The crafting tab shows the craft skills, each its name (disabled while its use \
               earns less than the usual experience) and rating (buffed when its level stands \
               above its own); a craft skill's forward button selects it and shows it at the head \
               with its display and a row per category of its recipes in order of their numbers; \
               a category's forward button shows its recipes that are shown (one not yet usable \
               again disabled); the examination's request for a recipe marks its row; a recipe \
               pressed twice is opened; the raise button raises the craft skill; the back button \
               goes up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CRAFT-PAGE"),
        station: "auberean-ui-game::dat::play::craft::the_crafting_page_goes_from_craft_skills_to_categories_to_recipes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.the-ingredient-panel-takes-items-and-makes-the-recipe",
        says: "The scripts open the ingredient panel on a recipe: its name, cost, a slot per \
               ingredient it takes at its last spinner value. An item dropped on a slot is asked to \
               go in, and shows there once the scripts say it went; fill-in puts the first fitting \
               item in each empty slot that fills itself in; turning the spinner asks for the \
               recipe there and brings its slots; the craft button asks to make the recipe from the \
               items in the slots, and with no charges left the panel closes when it is done.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INGREDIENTS"),
        station: "auberean-ui-game::dat::play::ingredients::the_ingredient_panel_takes_items_and_makes_the_recipe",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.the-makings-end-reads-the-charges-of-that-frame",
        says: "A making's end and a panel opening read the recipe as it stands that frame, by recipe whatever the spinner, so the last charge used closes the panel though the answer came with the notice.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CRAFT-DONE-CHARGES"),
        station: "auberean-ui-game::dat::play::ingredients::the_makings_end_reads_the_charges_of_that_frame",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.the-page-stands-as-laid-out-up-to-a-height-of-768",
        says: "At a window height of 768 and below the crafting page is its layout's size, and \
               the head, the list and the craft skill's display stand where the layout puts them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CRAFT-LAID-OUT"),
        station: "auberean-ui-game::dat::play::craft::the_page_stands_as_laid_out_up_to_a_height_of_768",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.craft.the-spinner-writes-a-changed-value-unsigned",
        says: "The ingredient panel's spinner shows its value signed when the panel opens and \
               unsigned once changed (a negative value written as its 32-bit word, of which the \
               field shows three characters), written only when the field reads otherwise (both \
               read as unsigned words); a typed value is read as the C library reads a number in \
               base 0, a leading 0 octal.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SPINNER-TEXT"),
        station: "auberean-ui-game::dat::play::ingredients::the_spinner_writes_a_changed_value_unsigned",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.creation.an-admin-client-offers-the-admin-character",
        says: "A client started as an administrator's shows the administrator's button beside \
               Cancel and Enter on the creation screen; lit, the character is asked for as an \
               administrator's. A player's client never shows it.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UI-ADMIN-CLIENT"),
        station: "auberean-ui-game::dat::screens::pregame::an_admin_client_offers_the_admin_character",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.creation.the-appearance-shows-the-avatars-rows-and-shapes-it",
        says: "The appearance shows the chosen avatar's rows (a kind its race lacks gone), each \
               labelled, its slider and buttons' tooltips naming it, at the avatar's stop; a \
               page's button shows its rows and frames the head on the head's page; a slider sets \
               the avatar's value; Randomize asks for a page's or every page's; dragging over the \
               figure turns the view; Back goes to the hall; Next puts the name's panel up, \
               Cancel takes it down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREATION-APPEARANCE"),
        station: "auberean-ui-game::dat::screens::pregame::the_appearance_shows_the_avatars_rows_and_shapes_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.creation.the-hall-describes-the-avatar-under-the-pointer-and-a-press-chooses-it",
        says: "Over an avatar in creation's hall, the description names its race and sex and the \
               tooltip says a press chooses it; a press chooses it and, its leaving behavior \
               done, please wait goes up and its appearance comes next; Cancel goes back to the \
               list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREATION-HALL"),
        station: "auberean-ui-game::dat::screens::pregame::the_hall_describes_the_avatar_under_the_pointer_and_a_press_chooses_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.creation.the-name-is-asked-with-cancel-and-enter",
        says: "The name is asked with Cancel and Enter, the buttons the player's client shows; \
               the entry shows its hint and takes no keys until pressed, its hint goes then, it \
               takes only a name's letters and at most sixteen; Enter asks for the character, the \
               server's refusal shows under the name and lights the buttons, and made, it enters \
               the world with it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAME-BUTTONS"),
        station: "auberean-ui-game::dat::screens::pregame::the_name_is_asked_with_cancel_and_enter",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.creation.the-whole-figure-view-stays-as-the-height-changes",
        says: "The appearance screen's view of the whole figure stays put as the height is \
               chosen, so the figure grows and shrinks in it; the view close up moves with \
               the height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREATION-CAMERA"),
        station: "auberean-interface::dat::screens::the_whole_figure_view_stays_as_the_height_changes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.credits.ctrl-and-alt-held-open-the-other-credits",
        says: "Credits pressed with Ctrl and Alt held and Shift not shows the other set of the \
               credits' picture and text fields; otherwise the usual set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREDITS-ALT"),
        station: "auberean-interface::dat::screens::ctrl_and_alt_held_open_the_other_credits",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.credits.the-text-scrolls-up-and-the-credits-end-by-themselves",
        says: "The credits scroll their text up from under the screen's foot until the last line \
               has gone over its top (about four minutes: the field's duration for a screen and a \
               line's worth of scrolling), then close by themselves, please wait up and the \
               character list next; an interface key closes them at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREDITS"),
        station: "auberean-ui-game::dat::screens::pregame::the_text_scrolls_up_and_the_credits_end_by_themselves",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.credits.they-stop-the-song-and-open-on-their-second-picture",
        says: "The credits stop the song playing as they open, once, and show the second of \
               their pictures first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CREDITS-OPEN"),
        station: "auberean-ui-game::dat::screens::pregame::they_stop_the_song_and_open_on_their_second_picture",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.datapatch.a-failed-connection-says-its-errors-own-line",
        says: "A login refused, or a login server letting its connection go, with an error leads \
               from connecting to the disconnected screen with that error's own line (the server \
               full); an address the client cannot understand names the host; an error with no \
               line of its own leaves the screen as it is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-CONNECT-FAILED"),
        station: "auberean-ui-game::dat::screens::pregame::a_failed_connection_says_its_errors_own_line",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.datapatch.connecting-says-how-it-goes-and-the-list-leads-on",
        says: "The connecting screen connects when it comes up and says so at its foot, writes \
               the connection's progress there, leads to the disconnected screen with the reason \
               when the connection fails, follows the data downloaded on its meter, and once the \
               account's characters are listed goes on to the character list, or into the world \
               as the character the client was started with.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DATAPATCH"),
        station: "auberean-ui-game::dat::screens::pregame::connecting_says_how_it_goes_and_the_list_leads_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.datapatch.the-characters-named-on-the-line-are-asked-after-once",
        says: "The character named to play on the command line goes before the one named to make \
               and play, which stands in when none is named to play; both are forgotten once the \
               first list comes; a name the list lacks, or a deleted character's, leads to the \
               character list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DATAPATCH-NAMED"),
        station: "auberean-ui-game::dat::screens::pregame::the_characters_named_on_the_line_are_asked_after_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.a-message-the-scripts-ask-for-first-goes-up-at-once",
        says: "The scripts' message says whether it goes up at once: one that does goes up in \
               place of the dialog up, which waits at the head of the line and comes back when it \
               closes; one that does not waits its turn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DIALOG-MESSAGE-FIRST"),
        station: "auberean-ui-game::dat::play::dialogs::a_message_the_scripts_ask_for_first_goes_up_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.a-message-waits-for-its-string-table",
        says: "A dialog asked for with a string reference (the scripts' message) gives it to its \
               text element as one: while the string tables it reads are on their way the dialog \
               is up with no text, which shows once they have come (the client's error message \
               for a table refused).",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-DIALOGS"),
        station: "auberean-ui-game::dat::play::dialogs::a_message_waits_for_its_string_table",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.a-timed-question-counts-down-and-answers-no",
        says: "A timed question says how many of its fifteen seconds are left, counting down each second, and answers itself no once they have run out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DIALOG-TIMED"),
        station: "auberean-ui-game::dat::play::dialogs::a_timed_question_counts_down_and_answers_no",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.an-offer-to-bring-the-player-back-asks-by-name",
        says: "Someone offering to bring the player back asks the question, naming them, in place of \n               death's own dialog; an offer with no name reads as no offer.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RESURRECT-OFFER"),
        station: "auberean-ui-game::dat::play::dialogs::an_offer_to_bring_the_player_back_asks_by_name",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.one-dialog-is-up-at-a-time-and-the-rest-wait-their-turn",
        says: "One dialog is up at a time: another waits its turn, one asked for first goes up at once and the one it displaced waits at the head of the line; a dialog closed by its number leaves the line, and when the one up closes the next goes up. Every dialog of the world keeps mouse look off from being asked for until it goes. A question's yes answers yes and its other button no; a message's and death's button answer them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DIALOG-QUEUE"),
        station: "auberean-ui-game::dat::play::dialogs::one_dialog_is_up_at_a_time_and_the_rest_wait_their_turn",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.the-answers-reach-the-panel-that-asked",
        says: "A dialog's answer reaches every panel and the one that asked acts on it: death's dialog brings the player back, a question of the player's scripts answered yes is answered with what it carried, the journal's question whether to give a quest up answered yes gives it up. Mouse look waits while a dialog of the world is up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DIALOG-ANSWERS"),
        station: "auberean-ui-game::dat::play::dialogs::the_answers_reach_the_panel_that_asked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.dialogs.the-use-key-answers-a-dialog-as-it-goes-up",
        says: "An element that takes the keyboard on showing takes it as it is made: a message's or a question's button holds the keyboard when the dialog goes up, so the use key answers it, unless the use key is already down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ACTIVATE-ON-FIRST-SHOW"),
        station: "auberean-ui-game::dat::play::dialogs::the_use_key_answers_a_dialog_as_it_goes_up",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.disconnected.the-reason-shows-and-quit-leaves",
        says: "The disconnected screen shows why the session ended, and its button leaves the \
               game.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DISCONNECTED"),
        station: "auberean-ui-game::dat::screens::pregame::the_reason_shows_and_quit_leaves",
        tier: Tier::Dat,
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
        id: "ui.drag.one-long-move-drags-what-the-press-was-on",
        says: "A press on an item followed by one long move, the pointer leaving the item in a single step, drags that item: what the press was on, not what the pointer has come to.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAG-LONG-MOVE"),
        station: "auberean-ui::cpu::framework::one_long_move_drags_what_the_press_was_on",
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
        id: "ui.draw.a-fill-reaching-off-its-element-paints-the-edge-it-passes",
        says: "A rectangle painted over an element (the radar's blips) is a window onto the                element's own picture pressed onto it: its left and top edges held at 0 or more,                its right and bottom at the picture's at most and at its first column and row at                least, a window with no width or height kept one texel inside its far edge; so                one reaching past an edge paints along that edge and none lands outside.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-FILL-WINDOW"),
        station: "auberean-ui::cpu::compositor::a_fill_reaching_off_its_element_paints_the_edge_it_passes",
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
        id: "ui.draw.a-resize-line-outlines-the-new-size-until-release",
        says: "A window with a resize line keeps its picture while it is resized: a white outline                one texel wide shows the new place and size, and the window takes them on release.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RESIZE-OUTLINE"),
        station: "auberean-ui::cpu::compositor::a_resize_line_outlines_the_new_size_until_release",
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
        id: "ui.draw.a-top-level-element-without-a-surface-shows-nothing",
        says: "A top-level element marked no_surface_object is drawn into no picture of its own: \
               neither its picture nor its children's reach the screen, and the pointer still finds \
               it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-NO-SURFACE"),
        station: "auberean-ui::cpu::compositor::a_top_level_element_without_a_surface_shows_nothing",
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
        id: "ui.draw.each-layer-starts-clear-of-the-one-before",
        says: "Each top-level element starts from a clear layer, whatever the elements drawn before                it left on theirs.",
        since: TOOLING,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-LAYER-CLEAR"),
        station: "auberean-ui::cpu::compositor::each_layer_starts_clear_of_the_one_before",
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
        id: "ui.draw.of-equal-depths-the-first-on-the-screen-is-drawn-in-front",
        says: "Of two top-level elements at one depth the first put on the screen is drawn in front, and the pointer meets it first.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAW-EQUAL-DEPTHS"),
        station: "auberean-ui::cpu::compositor::of_equal_depths_the_first_on_the_screen_is_drawn_in_front",
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
        id: "ui.element.a-placed-element-stays-off-no-edge-of-the-screen",
        says: "A top-level element placed past the screen's left or top edge comes back to \
               it, as one past the right or bottom edge does; a child is not held to the \
               screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PLACE-ON-SCREEN"),
        station: "auberean-ui::cpu::elements::a_placed_element_stays_off_no_edge_of_the_screen",
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
        id: "ui.element.a-window-resized-stops-at-the-screens-edges",
        says: "A window resized by its grips stops at the edges of the screen: an edge \
               dragged past one stands there, and its other edges stay where they are.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RESIZE-ON-SCREEN"),
        station: "auberean-ui::cpu::elements::a_window_resized_stops_at_the_screens_edges",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.element.an-elements-object-stands-where-its-top-level-element-stands",
        says: "The object an element is drawn with stands where its top-level element stands on the \
               screen: a child deep in a window answers the window's left and top, as the radar \
               places its dial by it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OBJECT-ORIGIN"),
        station: "auberean-ui::cpu::elements::an_elements_object_stands_where_its_top_level_element_stands",
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
        id: "ui.element.matching-attributes-goes-on-into-the-children-and-their-text",
        says: "Matching one element's attributes onto another (as a dragged copy is given the \
               original's) goes on into the children: each child gives the other's first child made \
               from the same description every attribute it lacks, a child the other lacks ending \
               it there, and a text element gives the other its text when theirs differ.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MATCH-ATTRIBUTES"),
        station: "auberean-ui::cpu::elements::matching_attributes_goes_on_into_the_children_and_their_text",
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
        id: "ui.element.the-use-key-held-keeps-a-shown-element-off-the-keyboard",
        says: "An element shown with activate_on_show takes the keyboard, unless the use key is \
               held.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-USE-HELD"),
        station: "auberean-ui::cpu::text::the_use_key_held_keeps_a_shown_element_off_the_keyboard",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.examination.a-countdown-shows-the-time-left-the-clock-keeps-current",
        says: "A countdown line of an examination profile shows the time left until its end (a game \
               time) in the string table's words, with the journal's least months, days and minutes \
               (1, 1 and 60), and the clock keeps them current; the next profile takes it away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EXAMINATION-COUNTDOWN"),
        station: "auberean-ui-game::dat::play::examination::a_countdown_shows_the_time_left_the_clock_keeps_current",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.examination.a-line-waiting-on-its-string-table-empties-the-window-when-it-comes",
        says: "A profile's line given as a string reference whose string tables the data lacks is \
               laid out empty, taking the rest of its row (on a new line when the row holds more \
               than its indent); when they come, or are refused, the window empties to its \
               waiting row until the next profile.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-EXAMINATION"),
        station: "auberean-ui-game::dat::play::examination::a_line_waiting_on_its_string_table_empties_the_window_when_it_comes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.examination.an-effect-line-lays-out-the-effects-own-lines",
        says: "An effect line of a profile lays out the effect's own lines in its place, the \
               effects among them left out, as the window's other lines are laid out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EFFECT-LINES"),
        station: "auberean-ui-game::dat::play::examination::an_effect_line_lays_out_the_effects_own_lines",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.examination.the-tabbed-window-asks-for-the-tab-pressed",
        says: "The player's options can make the examination window the tabbed one: it asks to examine with the first tab and shows its tab buttons, each active when asked for, inactive when the profile has lines for it, else disabled; pressing a tab that is not disabled (Ctrl adds or takes it away) asks to examine with that tab's tabs.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EXAMINATION-TABS"),
        station: "auberean-ui-game::dat::play::examination::the_tabbed_window_asks_for_the_tab_pressed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.examination.the-window-lays-out-a-profile-line-by-line",
        says: "The examination window, shown, comes to the front of the toolbar's windows (depth 10; another shown after it pushes it back one, to between 11 and 18) and asks to examine with its tabs (every tab: it starts as the classic window) and, hidden, to stop; it shows a waiting row until the profile comes, then lays the profile's lines on the tabs asked for out in one row: text from the left, wrapping to an indented new line when it does not fit, a break to a new line at the left, each indent 16 pixels in, pictures as wide as they are, the row as tall as its lines.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EXAMINATION-LAYOUT"),
        station: "auberean-ui-game::dat::play::examination::the_examination_window_lays_out_a_profile_line_by_line",
        tier: Tier::Dat,
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
        id: "ui.flow.each-refusal-reason-leads-to-its-screen",
        says: "Each reason the server gives for refusing a character leads to its screen: a deleted \
               character and the failed entries back to the character list, a refused login, a \
               crash, a log-off, the account's troubles, a full server and a booted character to the \
               disconnected screen, any other reason to none.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-REFUSAL-SCREENS"),
        station: "auberean-ui-game::cpu::flow::each_refusal_reason_leads_to_its_screen",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.flow.the-game-flow-starts-on-the-intro-and-follows-the-servers-reasons",
        says: "The game's flow starts on the opening screen with the data's default cursor; a reason \
               the server gives for refusing or ending a character leads to the character list or the \
               disconnected screen with the reason's message, or the server's own words when they \
               are valid (a boot's line); a lost connection asks for the disconnected screen \
               unless it is up or asked for; the screenshot key, let go, takes a screenshot.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-GAME-FLOW"),
        station: "auberean-ui-game::dat::play::standing::the_game_flow_starts_on_the_intro_and_follows_the_servers_reasons",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.frame.a-new-kept-place-is-hidden-off-the-screen",
        says: "A new kept place, before an element's place is read into it, is hidden with every \
               corner at -3, off the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME3-KEPT-PLACE"),
        station: "auberean-ui::cpu::framework::a_new_kept_place_is_hidden_off_the_screen",
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
        id: "ui.gameplay.the-layout-commands-name-their-file",
        says: "The scripts' commands to write and read the panels' places ask for the file they \
               name; with no name, the file of this character, world and screen: \
               UI-character-world-width-height.txt.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LAYOUT-COMMANDS"),
        station: "auberean-ui-game::dat::play::standing::the_layout_commands_name_their_file",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.gameplay.the-layout-file-keeps-the-windows-places",
        says: "The windows' and chat windows' places are written a line each as a tag, X, Y, W and H, \
               and a line read back gives its window that size, then that place; a line naming no \
               kept window is passed over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LAYOUT-FILE"),
        station: "auberean-ui-game::dat::play::standing::the_layout_file_keeps_the_windows_places",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.gameplay.the-log-out-key-ends-the-session-at-the-next-tick",
        says: "The log-out key ends the session at the next tick of the clock, once: a please-wait \
               dialog, the panels gone, the character logged off.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-END-SESSION"),
        station: "auberean-ui-game::dat::play::standing::the_log_out_key_ends_the_session_at_the_next_tick",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.gameplay.the-panels-keys-show-and-hide-them",
        says: "The toolbar windows' keys show and hide their windows; the toolbar's key hides the \
               toolbar, its shortcut bar and its windows and shows them again; the status bar's and \
               the radar's keys theirs; the key for the whole interface hides every panel and shows \
               them again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-GAMEPLAY-KEYS"),
        station: "auberean-ui-game::dat::play::standing::the_panels_keys_show_and_hide_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.gameplay.the-panels-places-are-kept-and-put-back",
        says: "A panel's place changed, the play screen asks once for the panels' places to be \
               kept with the player's options and sent to the server (not the game options); the \
               options coming again ask for the panels to be put back from them, and the display \
               set up again asks for them as the player's interface keeps them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-KEEP-PLACES"),
        station: "auberean-ui-game::dat::play::radar::the_panels_places_are_kept_when_moved_and_put_back_when_the_options_come",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.hud.the-toolbar-sits-on-the-bottom-edge",
        says: "The play screen makes no panel until the player is in the world; then its status bar, \
               toolbar with the shortcut bar, chat window and radar show, the toolbar on the screen's \
               foot covering 38 pixels of it, its dismount and consignment buttons hidden, the seven \
               toolbar windows made and hidden, the mouse look crosshair hidden.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLBAR"),
        station: "auberean-ui-game::dat::play::standing::the_toolbar_sits_on_the_bottom_edge",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.hud.vitals-read-current-over-greatest-and-focus-is-a-heros",
        says: "The vitals read current over greatest for health and vigor; anyone but a hero sees \
               the focus bar's ornament in its place, without numbers.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-VITALS"),
        station: "auberean-ui-game::dat::play::standing::vitals_read_current_over_greatest_and_focus_is_a_heros",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.intro.the-pictures-pass-and-a-key-or-a-press-goes-on",
        says: "The game opens on its pictures, three of them four seconds each, then goes on to \
               connecting; a key pressed or a press let go goes on at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INTRO"),
        station: "auberean-ui-game::dat::screens::pregame::the_pictures_pass_and_a_key_or_a_press_goes_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.a-dragged-item-shows-its-icon-once-on-a-larger-screen",
        says: "On a screen larger than the layouts', an item dragged out of the pack is shown once, in its cell's size with its corner at the pointer; its icon is not repeated beside it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DRAG-AREA-ITEM"),
        station: "auberean-ui-game::dat::play::inventory::a_dragged_item_shows_its_icon_once_on_a_larger_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.a-drop-in-the-world-waits-for-its-own-answer",
        says: "A second item dropped in the world goes to what the world answers for it, not \
               to what the last search found.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-DROP-ANSWER"),
        station: "auberean-ui-game::dat::play::inventory::a_drop_in_the_world_waits_for_its_own_answer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.a-press-on-the-doll-waits-for-its-own-answer",
        says: "A press on the paper doll's figure waits for its own search: what the last \
               press found is not selected, and what this one finds is.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PAPER-DOLL-CLICK-ANSWER"),
        station: "auberean-ui-game::dat::play::inventory::a_press_on_the_doll_waits_for_its_own_answer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-item-coming-back-as-another-leaves-shows",
        says: "An item added to the pack at the slot past its last as another leaves in the same frame shows in its slot, though by the scripts' answers after the frame the pack holds no more than before.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-SWAP-SHOWS"),
        station: "auberean-ui-game::dat::play::inventory::an_item_coming_back_as_another_leaves_shows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-item-dropped-in-the-world-goes-to-what-lies-there",
        says: "An item of the pack dragged over the world and let go asks what lies under the \
               pointer there; once the world answers it is moved to what was found: to no object \
               when the ground or the player itself lay there (put down), else into that object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-WORLD-DROP"),
        station: "auberean-ui-game::dat::play::inventory::an_item_dropped_in_the_world_goes_to_what_lies_there",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-item-dropped-on-a-cell-moves-there",
        says: "An item pressed in the pack is selected; dragged onto another cell of its pack it is \
               moved to that slot, no further than just past the last item (a slot sooner once it \
               has left its own); a stack asks the stack splitter first, which shows the item's \
               name and the whole stack, refuses more than that saying the most, and moves the \
               amount given.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-DROP"),
        station: "auberean-ui-game::dat::play::inventory::an_item_dropped_on_a_cell_moves_there",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-item-from-the-world-goes-only-into-the-pack",
        says: "An item pulled from the world comes from no container: let go over an open \
               chest's cell it stays where it is; only the player's own pack takes it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-WORLD-DROP-PACK"),
        station: "auberean-ui-game::dat::play::inventory::an_item_from_the_world_goes_only_into_the_pack",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-item-given-as-the-pack-is-sent-shows-once",
        says: "An item given as the pack is sent again shows once: the contents laid out from the scripts' answers after the frame's notices already hold it, so its own notice adds nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-GIFT-ONCE"),
        station: "auberean-ui-game::dat::play::inventory::an_item_given_as_the_pack_is_sent_shows_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.an-opened-container-shows-its-contents",
        says: "When the scripts say a container opened for the player, its window shows its \
               contents; take-all asks to take everything; the close button closes it for the \
               player and hides it; when the scripts say it closed, it hides.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-EXTERNAL-CONTAINER"),
        station: "auberean-ui-game::dat::play::inventory::an_opened_container_shows_its_contents",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-pack-shows-its-items-in-their-slots",
        says: "The inventory window shows by its key; its main pack's first cells hold the items \
               the scripts list for it, each with its icon (its look's layers laid together when it                has them) and its name as the tooltip, its icon \
               carrying the item, the pack and the slot; the four packs start each after the one \
               before; a tab shows its pack, the close button hides the window.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY"),
        station: "auberean-ui-game::dat::play::inventory::the_pack_shows_its_items_in_their_slots",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-paper-doll-finds-the-part-under-the-pointer",
        says: "A search of the paper doll's figure runs straight along its scene's view from the \
               pointer (taken from the viewport's corner) and answers with the figure met first \
               and the kind its mesh names there: the head's mesh, the torso's, nothing beside \
               the figure; in the figure's box between its parts, the figure on no mesh.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PAPER-DOLL-PICK"),
        station: "auberean-interface::dat::screens::the_paper_doll_finds_the_part_under_the_pointer",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-paper-doll-keeps-its-look-while-mounted",
        says: "The paper doll's figure follows the player's look on foot and keeps the look it had while the player is mounted, so it never shows the mount.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DOLL-MOUNTED"),
        station: "auberean-interface::dat::screens::the_paper_doll_keeps_its_look_while_mounted",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-paper-doll-shows-and-wears-items",
        says: "The paper doll shows a worn item in its place's slot and its wear on the meter \
               beside it (the meters of empty places hidden); an item from the pack dropped on a \
               slot is worn there; the money bag shows the money the scripts say the player has.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PAPER-DOLL"),
        station: "auberean-ui-game::dat::play::inventory::the_paper_doll_shows_and_wears_items",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-paper-doll-shows-the-player-seen-straight-on",
        says: "The paper doll shows the player's figure as the player looks, still in its first \
               pose, seen straight on by its scene's camera: the camera sees the figure's drawn \
               height (less for the races drawn shorter) and the figure stands half that height \
               below it, its feet at the viewport's foot; hidden with the inventory.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PAPER-DOLL-FIGURE"),
        station: "auberean-interface::dat::screens::the_paper_doll_shows_the_player_seen_straight_on",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-scripts-notices-keep-the-pack-current",
        says: "The scripts' notices about the pack put an added item in the cell of its slot, \
               leave a removed item's cell empty at the end, and show an item pending while a \
               request about it is under way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-INVENTORY-NOTICES"),
        station: "auberean-ui-game::dat::play::inventory::the_scripts_notices_keep_the_pack_current",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-stack-amount-is-written-signed",
        says: "The stack splitter's entry holds the amount offered written as a signed number \
               (`%d`), so an amount from 2^31 up shows negative.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STACK-AMOUNT-TEXT"),
        station: "auberean-ui-game::dat::play::inventory::the_stack_amount_is_written_signed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.inventory.the-world-pulled-on-drags-the-selection-not-a-pack-item",
        says: "A press on the world held and moved drags the selection's icon (the object selected \
               in the world, wherever it lies), not an item of the pack the pointer then passes \
               over; let go over another cell of the pack, the selection is asked to be put \
               there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-WORLD-DRAG"),
        station: "auberean-ui-game::dat::play::inventory::the_world_pulled_on_drags_the_selection_not_a_pack_item",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.keyboard.a-key-is-bound-with-the-first-modifier-held",
        says: "A key on the key map is bound with the first of the modifiers held, left Shift, right Shift, left Ctrl, right Ctrl, left and right told apart.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-KEYBOARD-MODIFIERS"),
        station: "auberean-ui-game::dat::screens::pregame::a_key_is_bound_with_the_first_modifier_held",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.keyboard.the-key-map-shows-each-actions-keys-and-rebinds-them",
        says: "The key map shows each action that has a name, its name and its two keys (a key \
               held with Ctrl or Shift written with it); a key cell pressed waits for a key: one \
               the game keeps for an action without a name is refused, one bound to another \
               action asks first and moves; Apply hands the client the maps; a preset asks and \
               puts its maps in place; Back with changes asks, then please wait and the character \
               list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-KEYBOARD"),
        station: "auberean-ui-game::dat::screens::pregame::the_key_map_shows_each_actions_keys_and_rebinds_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.layout.a-layout-is-read-again-once-its-last-element-goes",
        says: "A layout is let go once the last element made from it goes: loading it again reads \
               its record anew, and elements made after a changed record follow the new one; while \
               an element of it stands it is not read again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LAYOUT-RELEASE"),
        station: "auberean-ui::cpu::elements::a_layout_is_read_again_once_its_last_element_goes",
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
        id: "ui.list.a-row-taken-out-stands-nowhere",
        says: "A row taken out of a list has no parent until it is put somewhere: it is not drawn                or pressed where it stood, and the rows after it move up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-ROW-OUT"),
        station: "auberean-ui::cpu::lists::a_row_taken_out_of_a_list_stands_nowhere",
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
        id: "ui.list.an-emptied-list-keeps-its-cell-and-a-row-past-the-end-is-refused",
        says: "A list emptied row by row keeps its cells' size and one cell of sheet, so the \
               view it stood at stays; inserting past its end, or before a row not its own, \
               adds nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-EMPTY-AND-INSERT"),
        station: "auberean-ui::cpu::lists::an_emptied_list_keeps_its_cell_and_a_row_past_the_end_is_refused",
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
        id: "ui.list.selecting-by-drag-stays-on-the-row-pressed",
        says: "A list that selects by dragging, pressed on a row, selects it and listens for its \
               top-level element's own pointer moves, which the manager never sends: moving onto \
               another row with the button held leaves the pressed row selected, and the release \
               undoes the listening.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-LIST-DRAG-SELECT"),
        station: "auberean-ui::cpu::lists::selecting_by_drag_stays_on_the_row_pressed",
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
        id: "ui.manager.elements-by-name-and-removal",
        says: "An element is found by name, the oldest first, or within one top-level element; an \
               element removed is found no more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ELEMENTS-BY-NAME"),
        station: "auberean-ui::cpu::framework::elements_by_name_and_removal",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.map.a-click-zooms-in-and-a-right-click-goes-back-out",
        says: "A click on the world map shows the closer map of the 128 land cells around the \
               place clicked; an edge's button moves it nine cells (three with Shift) each time it \
               hears the press, three times for a click; a right click goes back to the world map \
               at the next tick.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MAP-DETAIL"),
        station: "auberean-ui-game::dat::play::map::a_click_zooms_in_and_a_right_click_goes_back_out",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.map.markers-come-and-go-by-kind",
        says: "A marker put on the map shows with its words and map coordinates in its tooltip; \
               another of its kind too near one already there is not put; a kind keeps its latest \
               ten; a marker is taken off by its object, a kind cleared at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MAP-MARKERS"),
        station: "auberean-ui-game::dat::play::map::markers_come_and_go_by_kind",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.map.the-regions-overviews-open-from-their-buttons",
        says: "A region's button on the world map opens its overview map in that region's state; \
               its close button takes it down.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MAP-OVERVIEWS"),
        station: "auberean-ui-game::dat::play::map::the_regions_overviews_open_from_their_buttons",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.map.the-world-map-fits-the-regions-picture-and-marks-the-player",
        says: "The map opens on the world map: the region's picture fitted inside its frame, the \
               player's mark where the player stands, the player's map coordinates and the game's \
               date written.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MAP"),
        station: "auberean-ui-game::dat::play::map::the_world_map_fits_the_regions_picture_and_marks_the_player",
        tier: Tier::Dat,
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
        id: "ui.media.an-icons-layers-mix-by-their-own-alpha",
        says: "An icon's layer goes over those below by its own alpha: none leaves the texel, whole replaces it and its alpha, between mixes the colors and keeps the alpha below. A layer of another size is stretched to fit, and one of alpha alone adds nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ICON-LAYER-BLIT"),
        station: "auberean-ui::cpu::media::an_icons_layers_mix_by_their_own_alpha",
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
        id: "ui.menu.an-open-menu-closes-as-its-window-hides",
        says: "An open menu closes as the window it stands in hides: its popup, its own \
               top-level element, does not stay on the screen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-MENU-HIDES"),
        station: "auberean-ui::cpu::lists::an_open_menu_closes_as_its_window_hides",
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
        id: "ui.meter.a-moving-fill-slides-keeping-its-own-size",
        says: "A meter that moves its fills slides each whole, keeping the size the layout \
               gave it; a left fill stands the part of the meter past the position before the \
               meter, cut to a whole pixel toward the meter.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-METER-MOVING-FILL"),
        station: "auberean-ui::cpu::lists::a_moving_fill_slides_keeping_its_own_size",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.names.a-name-at-the-screens-side-runs-off-it",
        says: "A name over a head near the screen's side runs off the edge, cut there, not pushed back in.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-RUN-OFF"),
        station: "auberean-ui-game::dat::play::names::a_name_at_the_screens_side_runs_off_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.a-name-moves-off-another-the-shortest-way",
        says: "Two boxes overlap when they share a texel; a name's box lying over another is moved off it the shortest way still allowed (left and up stopping two texels short, as the client measures them) and may not then go straight back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-PLACING"),
        station: "auberean-ui-game::dat::play::names::a_name_moves_off_another_the_shortest_way",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.a-renamed-object-is-named-anew-once-its-string-table-comes",
        says: "An object's name, as its scripts give it, goes to the name over its head and to \
               the selection's name as a string reference; when the scripts say its radar look \
               changed (as they do when the server renames it) both are made again from the new \
               name, which shows once the string tables it reads have come.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-NAMES"),
        station: "auberean-ui-game::dat::play::names::a_renamed_object_is_named_anew_once_its_string_table_comes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.a-selection-faded-out-of-sight-is-not-marked",
        says: "A selection its animation has faded out of sight is neither marked nor named: its \
               marks and name hide until it can be seen again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-FADED"),
        station: "auberean-ui-game::dat::play::names::a_selection_faded_out_of_sight_is_not_marked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.an-arrow-points-to-a-selection-off-the-screen",
        says: "When no corner of the selection's box is on the screen, the corner marks hide and an arrow on the screen's edge points the way to it (behind the camera: at the foot, pointing down); a selection beyond the greatest selection distance (400 meters) is no longer selected.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-OFFSCREEN"),
        station: "auberean-ui-game::dat::play::names::an_arrow_points_to_a_selection_off_the_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.an-item-selected-in-the-pack-is-marked-once-put-down",
        says: "An item selected in the pack and then put down is named and marked as soon as it                lies in the world, with no new selection.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-FROM-PACK"),
        station: "auberean-ui-game::dat::play::names::an_item_selected_in_the_pack_is_marked_once_put_down",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.an-item-that-changes-is-named-anew",
        says: "An item changing, its quantity among others, names it anew: the name over its head as well as the selection's, so a stack's count does not go stale.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ITEM-UPDATE-NAMES"),
        station: "auberean-ui-game::dat::play::names::an_item_that_changes_is_named_anew",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.damage-text-fades-out-over-its-life",
        says: "A hit's damage text fades out over its life: whole as it begins, about half gone half way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAMAGE-TEXT-FADE"),
        station: "auberean-ui-game::dat::play::names::damage_text_fades_out_over_its_life",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.damage-text-is-drawn-at-its-scale",
        says: "A hit's damage text is drawn at its scale, and stands over the object by the size it is drawn at.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-DAMAGE-TEXT-SCALE"),
        station: "auberean-ui-game::dat::play::names::damage_text_is_drawn_at_its_scale",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.damage-text-rises-over-the-struck-object-and-goes",
        says: "A hit's damage text stands over the struck object's box in the color the scripts \
               gave it and rises three of its own heights over its duration, then goes; texts at \
               once are moved off each other as names are.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DAMAGE-TEXT"),
        station: "auberean-ui-game::dat::play::names::damage_text_rises_over_the_struck_object_and_goes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.names-over-heads-stand-clear-of-each-other",
        says: "The names' key shows a name over every object on the radar within the radar's range, each moved off the names already placed; a press on a name selects its object; the key again hides them, the player's option kept each time.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-HEADS"),
        station: "auberean-ui-game::dat::play::names::names_over_heads_stand_clear_of_each_other",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.no-names-stand-over-portal-space",
        says: "While portal space is drawn, which lasts past the server's putting the player \
               back, no name, damage or selection mark stands over it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-PORTAL"),
        station: "auberean-ui-game::dat::play::names::no_names_stand_over_portal_space",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.placed-after-the-world-the-marks-take-its-camera",
        says: "Placed after the world's update, the names' clock only asks for a placing: the marks stay where the last camera put them until the placing is made with this frame's camera, once for each time the clock asked.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-FRAME-ORDER"),
        station: "auberean-ui-game::dat::play::names::placed_after_the_world_the_marks_take_its_camera",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.names.the-selection-is-named-and-marked-over-its-box",
        says: "The selection's name stands centered over the box it fills on the screen in its radar color, its health and vigor meters under the name once the scripts send its vitals, and four marks stand off the box's corners, each turned on from the last by a quarter, a half and three quarters more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NAMES-SELECTION"),
        station: "auberean-ui-game::dat::play::names::the_selection_is_named_and_marked_over_its_box",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.notices.handlers-hear-in-the-order-they-registered",
        says: "A notice is handed to its handlers in the order they registered; a handler \
               registered again for the same notice stays where it was; one unregistered from a \
               notice no longer hears it, and one that goes hears nothing more.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-NOTICE-REGISTRY"),
        station: "auberean-ui-game::cpu::notices::handlers_hear_a_notice_in_the_order_they_registered",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.notices.registrations-name-the-scripts-notices-by-their-natives",
        says: "A panel's registrations, written by the notices' own numbers, name a notice the \
               player's scripts send by the native that sends it and the client's own by its \
               number; the examination window's opening by both.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-NOTICE-NAMES"),
        station: "auberean-ui-game::cpu::notices::registrations_name_the_scripts_notices_by_their_natives",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.notices.the-play-screens-notices-reach-their-handlers-in-the-order-they-registered",
        says: "The play screen's panels register for their notices as the play screen makes them, \
               in that order, and the play screen registers for some of its own as it is made and \
               the rest once its panels are made, after them: a notice reaches those who registered \
               for it, in that order, and no other.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-NOTICE-ORDER"),
        station: "auberean-ui-game::dat::play::standing::the_play_screens_notices_reach_their_handlers_in_the_order_they_registered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.a-blessed-preference-is-read-from-the-file-once-and-written-back",
        says: "A preference blessed is copied into the bank and read from the file by its names \
               (its default when absent) as applied, and the owner holds the bank's value; \
               blessed again it takes the new values without the file, its handler and monitor \
               told; every preference is written back by its names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREFERENCE-BANK"),
        station: "auberean-ui-game::cpu::preferences::a_blessed_preference_is_read_from_the_file_once_and_written_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.options.a-flip-of-full-screen-goes-through-its-preference",
        says: "Full screen flipped goes through its preference: the host told, the page told so its \
               row shows it, marked applied, so the page sees nothing to apply.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-FLIP"),
        station: "auberean-interface::dat::preferences::a_flip_of_full_screen_goes_through_its_preference",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.a-number-preference-is-written-as-percent-f",
        says: "A number preference is written as the C library writes `%f`: its first seventeen \
               significant digits, zeros after them, and six places, a tie rounding up.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREF-FLOAT-TEXT"),
        station: "auberean-ui-game::cpu::preferences::a_number_preference_is_written_as_percent_f",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.options.a-preference-compares-and-copies-by-kind",
        says: "Two preferences compare by their names, kind and same-kind values; a preference \
               copies only another of its kind, and a copy's applied value is a new one's.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREFERENCE-COMPARE"),
        station: "auberean-ui-game::cpu::preferences::a_preference_compares_and_copies_by_kind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.options.a-resized-window-is-the-displays-size",
        says: "While the game is a window, a window the player resizes is the display's size: its \
               row shows it and keeping keeps it. In full screen the window's size says nothing of \
               the preference.",
        since: THIS_CLIENT,
        divergence: "CD2-the-window-can-be-any-size",
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-RESIZE"),
        station: "auberean-interface::dat::preferences::a_resized_window_is_the_displays_size",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.apply-asks-first-then-sets-every-chat-option-and-the-filters-last",
        says: "The chat page's Apply asks first, then sets every chat option section by section (the \n \
               devoted channels, each kind's font size, color and popup flag), then each window's \n \
               filter as its boxes say (the administrators' and the log's kept as they were); the \n \
               options are read again and sent to the server to keep.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-CHAT-APPLY"),
        station: "auberean-ui-game::dat::play::chat::apply_asks_first_then_sets_every_chat_option_and_the_filters_last",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.assistance-lists-other-last",
        says: "The assistance page's kinds of call read with each new row put first: Other last.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ASSISTANCE-ORDER"),
        station: "auberean-ui-game::dat::play::options::assistance_lists_other_last",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.assistance-other-asks-a-description",
        says: "The assistance page's Other is a kind of call that asks for a description, so Send                doesn't pass it over.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-ASSISTANCE-OTHER"),
        station: "auberean-ui-game::dat::play::options::assistance_other_asks_a_description",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.closing-a-section-leaves-the-pages-scroll-widget-long",
        says: "The page's bar shows no widget when first opened with every section closed; a \
               section opened makes the widget the share in view; closed again, every row showing, \
               the bar is disabled and the widget stays, the bar's whole length.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-SECTION-SCROLL-WIDGET"),
        station: "auberean-ui-game::dat::play::options::closing_a_section_leaves_the_pages_scroll_widget_long",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.each-preference-reads-and-writes-its-value-as-the-file-keeps-it",
        says: "Each preference kind reads and writes its value as the preferences file keeps it: a \
               yes-or-no as 1 or 0, a number only when the whole text is one (six places \
               written), a choice by number or by its name in its enumeration, a text as it is; a \
               new number is a half over 0 to 1.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREFERENCE-KINDS"),
        station: "auberean-ui-game::cpu::preferences::each_preference_reads_and_writes_its_value_as_the_file_keeps_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.options.hidden-with-changes-the-window-asks-to-apply-them",
        says: "The options window hidden with an option changed and not applied asks whether to apply it; no puts back what was applied.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-CONFIRM"),
        station: "auberean-ui-game::dat::play::options::hidden_with_changes_the_window_asks_to_apply_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.leave-world-and-quit-ask-first",
        says: "The options window's Leave World and Quit ask first, the commands ghosted while a question is up; yes to leaving ends the session at the next tick; yes to quitting sends the options to keep and ends the game.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-COMMANDS"),
        station: "auberean-ui-game::dat::play::options::leave_world_and_quit_ask_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.recall-and-default-ask-first-and-put-back-the-applied-or-the-defaults",
        says: "The chat page's Recall and Default each ask first: Recall puts back what was applied, \n \
               Default puts a new player's chat options in and applies them; the options window \n \
               hidden with a chat option changed asks whether to apply it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-CHAT-RECALL"),
        station: "auberean-ui-game::dat::play::chat::recall_and_default_ask_first_and_put_back_the_applied_or_the_defaults",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-bank-tells-the-page-and-the-handlers-what-they-hold",
        says: "A section's monitor is listed its preferences on attaching and when one comes or \
               goes; applying hands each handler its changed preferences, applied; submitting \
               hands them unapplied; recalling puts back the section and hands each its own; \
               synchronizing applies without telling anyone.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREFERENCE-BANK-TELLS"),
        station: "auberean-ui-game::cpu::preferences::the_bank_tells_the_page_and_the_handlers_what_they_hold",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.options.the-bars-rows-set-by-hand-are-kept-at-once",
        says: "The shortcut bar resized by hand tells the options window its rows: they are set and the options sent at once, no other panel told, and the character page shows them applied.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-ROWS"),
        station: "auberean-ui-game::dat::play::options::the_bars_rows_set_by_hand_are_kept_at_once",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-character-page-shows-the-players-options-and-applies-the-changed-ones",
        says: "The character page shows the player's options as the client read them (melting without a question as its opposite, the radar's filters from its mask, the bar's rows, others' damage range) with Apply and Recall ghosted until one changes; Apply sets the changed options in the page's order, tells the other panels and sends the options to the server.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS"),
        station: "auberean-ui-game::dat::play::options::the_character_page_shows_the_players_options_and_applies_the_changed_ones",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-chat-page-lists-each-kind-with-a-new-players-options-as-defaults",
        says: "The chat page has a section for every kind of text but the administrators': its font \n \
               size, font color, the four windows' boxes and its popup box (the chat entry only \n \
               size and color, with no large size); the devoted windows' channels are named from \n \
               the kinds' list by place, so the general, trade and region channels show under the \n \
               player-killer, general and trade channels' names. A new player's chat options are \n \
               the defaults and the page shows the player's own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-OPTIONS-CHAT-PAGE"),
        station: "auberean-ui-game::dat::play::chat::the_chat_page_lists_each_kind_with_a_new_players_options_as_defaults",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-client-page-lists-the-cameras-preferences-and-keeps-them",
        says: "The client page lists the camera's eight camera preferences, three mouse and three \
               avatar ones, read from the file by the data's names; a camera preference applied \
               reaches the camera, and keeping writes every preference by its names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PREFERENCE-CAMERA"),
        station: "auberean-interface::dat::preferences::the_client_page_lists_the_cameras_preferences_and_keeps_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-client-page-shows-the-preferences-the-client-lists",
        says: "The client page, opened, asks the client to list its eight sections' preferences; a \
               listing the client gives fills the section it names with a row for each preference, \
               and the same listing is taken in once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-CLIENT-LISTING"),
        station: "auberean-ui-game::dat::play::options::the_client_page_shows_the_preferences_the_client_lists",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-client-page-shows-the-sizes-and-the-clients-own-changes",
        says: "A preference labeled in words of its own shows them on its row's menu, and a change \
               the client made itself shows on the row that holds it, with nothing left for the \
               page to apply.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-PAGE"),
        station: "auberean-ui-game::dat::play::options::the_client_page_shows_the_sizes_and_the_clients_own_changes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-client-pages-other-section-sets-tooltips",
        says: "The client page's last section lists the tooltip delay, from none to ten seconds, and \
               whether tooltips show, read from the preferences file by the data's names; applied, \
               each reaches the interface, and keeping writes them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-TOOLTIP-PAGE"),
        station: "auberean-interface::dat::preferences::the_client_pages_other_section_sets_tooltips",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-command-line-forces-the-top-gore-and-blood",
        says: "The command line's maxgore and greenblood stand over the language's top gore level and                blood color: the effects start at them and nothing above them is offered.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-FORCED-GORE"),
        station: "auberean-interface::dat::preferences::the_command_line_forces_the_top_gore_and_blood",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-effects-section-chooses-what-keyed-effects-play",
        says: "The options' effects section lists how much gore, blood's color and steps raising particles, by what the language allows, at its top levels by default, and tells the effects as they change.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-FX-CHANNEL-PREFS"),
        station: "auberean-interface::dat::preferences::the_effects_section_chooses_what_keyed_effects_play",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-rendering-question-keeps-or-puts-back-and-leaves-the-rows",
        says: "Applying a changed rendering preference submits it and asks, in words that count \
               their seconds, whether to keep it; no puts it back and keeps the file, the row left \
               as it was set, so Apply and Revert stay lit and Apply tries it again; yes keeps it \
               and they go dark.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-QUESTION"),
        station: "auberean-ui-game::dat::play::options::the_rendering_question_keeps_or_puts_back_and_leaves_the_rows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-rendering-section-lists-the-displays-sizes-and-the-devices-preferences",
        says: "The rendering section holds the display's size and five yes-or-no preferences, in \
               the device's order. The sizes are those the display can take from 800 by 600 up, \
               with 1024 by 768 always there and the default, each named by its numbers; the \
               defaults are full screen and waiting for the display's refresh.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-LISTS"),
        station: "auberean-interface::dat::preferences::the_rendering_section_lists_the_displays_sizes_and_the_devices_preferences",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.options.the-rendering-section-reaches-the-host-when-submitted-and-is-kept",
        says: "What the player sets in the rendering section reaches the host only when submitted, \
               and again when put back; kept, each goes to the file by the data's names, the size \
               as one number, its width above its height.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-OPTIONS-RENDERING-HOST"),
        station: "auberean-interface::dat::preferences::the_rendering_section_reaches_the_host_when_submitted_and_is_kept",
        tier: Tier::Dat,
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
        id: "ui.panels.help-buttons-do-not-show",
        says: "The panels' help buttons do not show, on the panels made with the play screen and \
               on those made when they first open.",
        since: THIS_CLIENT,
        divergence: "CD2-help-buttons-do-not-show",
        evidence: Evidence::Private("AC2-EVID-UI-HELP-HIDDEN"),
        station: "auberean-ui-game::dat::play::help::the_panels_help_buttons_do_not_show",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pet.the-mode-buttons-step-from-the-mode-asked",
        says: "The mode buttons step from the mode last asked, not the one the pet has taken: \
               Next twice before the pet answers is two steps on, and Previous then one back.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PET-MODE-STEP"),
        station: "auberean-ui-game::dat::play::pet::the_mode_buttons_step_from_the_mode_asked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pet.the-modes-turn-in-order-and-names-lose-their-owner",
        says: "A pet's mode turns through the nine modes in order and back round (one not among \
               them goes to the first going on and the last going back); a pet's name loses its \
               owner's part in parentheses.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PET-MODES"),
        station: "auberean-ui-game::dat::play::pet::the_modes_turn_in_order_and_names_lose_their_owner",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pet.the-options-bit-hides-the-pet-panel",
        says: "A pet shows the panel on a cleared profile; the bit of the player's options \
               set hides it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PET-OPTION-BIT"),
        station: "auberean-ui-game::dat::play::pet::the_options_bit_hides_the_pet_panel",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pet.the-pet-panel-lists-pets-and-commands-them",
        says: "With the player's option on, a pet the scripts add shows in its row and the panel \
               with its commands lit; the next-mode button shows the next mode at once and asks \
               for it within five seconds and again every five until the pet takes it; attack \
               commands the pets at the selection; a pet under attack shows unsafe until five \
               seconds after; with no pets the panel hides.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PET"),
        station: "auberean-ui-game::dat::play::pet::the_pet_panel_lists_pets_and_commands_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.pointer.a-press-taken-in-front-is-not-held-behind",
        says: "A press taken by an element in front stops there: the top-level elements behind                it never hear it nor hold the button, so the next press on one of them alone is                its own, pressed on the topmost.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-PRESS-STOPS"),
        station: "auberean-ui::cpu::pointer::a_press_taken_in_front_is_not_held_behind",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.pointer.a-turned-element-is-found-by-its-unturned-rectangle",
        says: "A turned top-level element is drawn turned but found by the pointer in its \n               unturned rectangle.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TURNED-POINTER"),
        station: "auberean-ui::cpu::pointer::a_turned_element_is_found_by_its_unturned_rectangle",
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
        id: "ui.pointer.the-pointer-passes-through-a-tooltip",
        says: "A tooltip takes no part in the pointer: over one, moves, clicks and the wheel reach what lies beneath.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TOOLTIP-POINTER"),
        station: "auberean-ui::cpu::text::the_pointer_passes_through_a_tooltip",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.quests.a-quest-that-changed-is-marked-in-its-row",
        says: "A quest that changes has its row's name and time marked in the changed look, the                others keeping theirs; the whole list read at the session's start marks nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-QUEST-CHANGED-MARK"),
        station: "auberean-ui-game::dat::play::examination::a_quest_that_changed_is_marked_in_its_row",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.a-quests-time-left-is-written-in-words-the-clock-keeps-current",
        says: "A quest's time left is written in the string table's words for how much is left: \
               above a month, months and days; above a day, days and hours; above sixty minutes, \
               hours and minutes; else minutes and seconds. The clock keeps the words current, \
               each looked at again when its smallest unit runs out, until the time is out.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-QUEST-TIME"),
        station: "auberean-ui-game::dat::play::examination::a_quests_time_left_is_written_in_words_the_clock_keeps_current",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.the-journal-keeps-quests-under-way-and-finished",
        says: "The quest journal reads the player's quests at the session's start: those under way on one page and those completed, failed or cancelled on the other, newest given first; a quest that changes becomes the open one (its text row, and a cancel row when it may be given up) and moves to the page of its status; while the window is hidden its button shows that something changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-QUEST-JOURNAL"),
        station: "auberean-ui-game::dat::play::examination::the_journal_keeps_quests_under_way_and_finished",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.the-journal-scrolls-to-the-quest-that-changed",
        says: "The quest that changed comes into view: with more quests than the list shows, \
               the list scrolls to it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-JOURNAL-SCROLL"),
        station: "auberean-ui-game::dat::play::examination::the_journal_scrolls_to_the_quest_that_changed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.the-quest-button-keeps-its-look-as-it-opens-the-window",
        says: "The quest window's button opens it and stays on the toolbar: shown, the window \
               puts its button in its unchanged look, the picture it shows when nothing changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-QUEST-BUTTON"),
        station: "auberean-ui-game::dat::play::examination::the_quest_button_keeps_its_look_as_it_opens_the_window",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.the-quest-window-stands-on-the-quests-under-way-until-a-scene-is-added",
        says: "At the session's start the quest window stands on the quests under way (the whole \
               list and the whole story change no page); a scene added to the story while it is \
               hidden turns it to the story and marks its button changed, as a new character's \
               first entry does.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-QUEST-WINDOW-AT-ENTRY"),
        station: "auberean-ui-game::dat::play::examination::the_quest_window_stands_on_the_quests_under_way_until_a_scene_is_added",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.quests.the-story-page-lists-the-acts-and-opens-the-one-changed",
        says: "The story page lays out the visible acts, in order, each with its scenes' icons \
               (the hidden ones left out), completed when the scene may be played again, else not; \
               an act the scripts say changed while the page is not shown marks the window's \
               button, is open with its scenes' rows and its name marked; pressing an act's row \
               opens or closes it, and a completed scene's row pressed twice plays it again.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STORY-PAGE"),
        station: "auberean-ui-game::dat::play::examination::the_story_page_lists_the_acts_and_opens_the_one_changed",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.radar.a-radar-kept-hidden-comes-back-hidden",
        says: "The radar's kept places put back with its field hidden hide the whole radar, dial and all, so it does not come back as a bare dial.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-RADAR-KEPT-HIDDEN"),
        station: "auberean-ui-game::dat::play::radar::a_radar_kept_hidden_comes_back_hidden",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.radar.the-blip-under-the-pointer-names-and-selects-its-object",
        says: "The blip within four pixels of the pointer names its object in the radar's tooltip (none, an empty one), and a press on the radar selects that object.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RADAR-TOOLTIP"),
        station: "auberean-ui-game::dat::play::radar::the_blip_under_the_pointer_names_and_selects_its_object",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.radar.the-dial-turns-and-the-blips-show-what-is-around",
        says: "The radar's dial stands on the radar's corner turned counterclockwise by the player's heading; each object the scripts put on the radar within its range (50 meters inside, 75 outside) is a blip of its shape at its place from the player, the way the player faces up, scaled from the range to the radar's radius; the selection's blip is bracketed; nothing beyond the range is drawn.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RADAR-BLIPS"),
        station: "auberean-ui-game::dat::play::radar::the_dial_turns_and_the_blips_show_what_is_around",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.radar.the-groove-level-adds-up-what-is-in-range",
        says: "The objects around with a groove level add theirs while within the radar's range; the sum is the music's groove level, told when it changes.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RADAR-GROOVE"),
        station: "auberean-ui-game::dat::play::radar::the_groove_level_adds_up_what_is_in_range",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.radar.the-place-line-reads-inside-or-the-map-coordinates",
        says: "The radar's place line reads Inside in an interior cell and outdoors the cell's map coordinates, north or south then east or west to a tenth; the coordinates' button closes and opens it, its tooltip saying which, the option kept.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-RADAR-PLACE"),
        station: "auberean-ui-game::dat::play::radar::the_place_line_reads_inside_or_the_map_coordinates",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.scrollbar.a-list-showing-all-its-rows-leaves-the-widget-long",
        says: "A list that comes to show all its rows disables its bar and leaves the widget as it \
               was laid out just before, long and showing; its rows laid out again, it stays; its \
               window updated, which lays the disabled bar out afresh, takes the widget away.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SCROLLBAR-DISABLED-WIDGET"),
        station: "auberean-ui::cpu::lists::a_list_showing_all_its_rows_leaves_the_widget_long",
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
        id: "ui.scrollbar.the-wheel-leaves-other-windows-alone",
        says: "The wheel over another window, over nothing, or over a window whose bar is hidden leaves a window's list where it is.",
        since: THIS_CLIENT,
        divergence: "CD2-wheel-scrolls-the-window",
        evidence: Evidence::Private("AC2-EVID-UI-WHEEL-OTHERS"),
        station: "auberean-ui::cpu::lists::the_wheel_leaves_other_windows_alone",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.scrollbar.the-wheel-scrolls-the-window-under-the-pointer",
        says: "The wheel over a window's ground, or over something in it that takes no wheel, turns the window's nearest bar.",
        since: THIS_CLIENT,
        divergence: "CD2-wheel-scrolls-the-window",
        evidence: Evidence::Private("AC2-EVID-UI-WHEEL-WINDOW"),
        station: "auberean-ui::cpu::lists::the_wheel_scrolls_the_window_under_the_pointer",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.selection.a-press-on-the-world-after-a-button-still-selects",
        says: "A press taken by a panel in front (a toolbar button clicked, opening its window)                leaves the field over the world untouched: the next press on the world selects                what lies under it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-AFTER-BUTTON"),
        station: "auberean-ui-game::dat::play::selection::a_press_on_the_world_after_a_button_still_selects",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection.a-press-on-the-world-selects-what-lies-under-it",
        says: "The selection's field lies over the whole screen behind every panel; a left press on it asks the world what lies under the pointer and at the next tick after the answer selects it (nothing, when nothing was there); an object that may not be selected is passed over unless the player's kind may select anything; a double press also uses it, unless the player holds it; a right press on an object asks for the examination panel.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-PRESS"),
        station: "auberean-ui-game::dat::play::selection::a_press_on_the_world_selects_what_lies_under_it",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection.attacking-with-auto-target-selects-the-nearest-foe-first",
        says: "The war button, with the player targeting automatically, selects the nearest foe \
               before it asks the player's scripts to attack; without, it only asks to attack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CGAME4-AUTO-TARGET"),
        station: "auberean-ui-game::dat::play::selection::attacking_with_auto_target_selects_the_nearest_foe_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection.the-selection-keys-pick-by-kind-and-distance",
        says: "The selection keys pick by kind among the objects within the radar's range (indoors the shorter), ordered by distance along the ground plus a fifth more per meter up or down, then by id: the next past the selection, the previous before it, round again past either end, the nearest; a foe may be attacked and is no fellow; what a body holds is found by the item keys, at the body; the player's own key selects it and then the last selection; the last attacker's key selects the last to attack the player; Escape selects nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-KEYS"),
        station: "auberean-ui-game::dat::play::selection::the_selection_keys_pick_by_kind_and_distance",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.selection.the-targeting-notice-shows-the-usage-cursor",
        says: "The scripts' targeting notice shows the selection field's usage cursor over the world while an item waits for its target, and puts the cursor back after.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SELECTION-CURSOR"),
        station: "auberean-ui-game::dat::play::selection::the_targeting_notice_shows_the_usage_cursor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shop.a-catalog-lists-its-shops-and-enters-one",
        says: "A catalog the scripts show lists its header and a row per shop with its name and \
               how many things it sells; a shop pressed twice is entered; the consignments button \
               asks for the player's consignments; close leaves the catalog.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHOP-CATALOG"),
        station: "auberean-ui-game::dat::play::shop::a_catalog_lists_its_shops_and_enters_one",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shop.a-shop-shows-its-sales-and-buys-them",
        says: "A shop the scripts show lists a row per sale with what is left (no end, sold out, \
               restricted); a sale pressed is selected; purchase buys one when only one may be \
               bought, asks how many when more may, and tells the player when none may (a message \
               that goes up at once); the purchase question is answered; close leaves the shop.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHOP"),
        station: "auberean-ui-game::dat::play::shop::a_shop_shows_its_sales_and_buys_them",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shop.consignments-are-collected-put-up-and-restocked",
        says: "The player's consignments show a row each and an empty row; a sold one is \
               collected for its coins; an item dropped on a consignment of its kind asks how \
               many and whether to restock it at its cost; the new-sale window takes an item, how \
               many and a price, and puts it up after its question.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-CONSIGNMENTS"),
        station: "auberean-ui-game::dat::play::shop::consignments_are_collected_put_up_and_restocked",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shop.part-of-a-stack-is-named-for-its-amount",
        says: "Part of a stack is called by its name for that many, which the scripts answer: the new-sale window shows it once it comes and its question says it; a restock's question waits for it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-SHOP-AMOUNT-NAME"),
        station: "auberean-ui-game::dat::play::shop::part_of_a_stack_is_named_for_its_amount",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.a-moved-slot-shows-its-icon-where-it-went",
        says: "A slot dragged onto another shows its skill's icon and tooltip there, and the slot it left shows none, once the player's scripts hold the moved shortcuts; onto a slot holding another skill it replaces that one.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-MOVED"),
        station: "auberean-ui-game::dat::play::bar::a_moved_slot_shows_its_icon_where_it_went",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.a-slot-keeps-its-icon-while-pressed-and-dragged",
        says: "A slot pressed keeps its icon, and the copy dragged with the pointer shows the same icon.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-PRESSED"),
        station: "auberean-ui-game::dat::play::bar::a_slot_keeps_its_icon_while_pressed_and_dragged",
        tier: Tier::Dat,
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
        id: "ui.shortcuts.an-item-slot-shows-the-icon-its-look-makes",
        says: "An item dragged from the pack onto an empty slot is put there; once the player's scripts hold it, the slot shows the icon the item's look makes (its layers laid together), the item's name as its tooltip, and its number.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-ITEM-ICON"),
        station: "auberean-ui-game::dat::play::bar::an_item_slot_shows_the_icon_its_look_makes",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.dragging-moves-and-clears-shortcuts",
        says: "A slot dragged onto another moves its shortcut there and empties its own; one dragged off the bar is emptied; a skill the player no longer has trained is taken off the bar when the scripts say the skill changed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-DRAG"),
        station: "auberean-ui-game::dat::play::bar::dragging_moves_and_clears_shortcuts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.keys-and-clicks-use-slots-and-sets-move-the-view",
        says: "A slot's key and a left click on it use the slot; a right click examines what it holds; Ctrl with a digit shows that set, whose slots the keys then use; the next and previous set keys move the view; the attack key asks to attack.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-USE"),
        station: "auberean-ui-game::dat::play::bar::keys_and_clicks_use_slots_and_sets_move_the_view",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.the-attack-buttons-follow-the-scripts-notice",
        says: "The war button asks the player's scripts to start attacking and the buttons go on \
               showing the state the player is in; the scripts' notice of the attack state ghosts the \
               button for that state.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-ATTACK-NOTICE"),
        station: "auberean-ui-game::dat::play::standing::the_attack_buttons_follow_the_scripts_notice",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.shortcuts.the-slots-show-the-scripts-icons-and-numbers",
        says: "The shortcut bar holds ten sets of ten slots for one row; a slot holding a skill shows the icon and tooltip the player's scripts give for the slot, and its number in its corner while slots are numbered; an empty slot shows neither and cannot be dragged; a skill the player cannot use with what it carries is shadowed; the set's number shows by the arrows and the scripts are told the set.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SHORTCUT-SLOTS"),
        station: "auberean-ui-game::dat::play::bar::the_slots_show_the_scripts_icons_and_numbers",
        tier: Tier::Dat,
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
        id: "ui.skills.a-panel-is-laid-out-as-a-tree-and-a-skill-trained-through-its-question",
        says: "The skill panel the examination asks for is laid out as a tree, each node at its place hung from its parent: a trained skill shows its level (marked when a buff moves it) and whether it may be raised, an untrained one whether it may be trained; the root buttons follow the race's root panels. Pressing a node asks the examination for it; the examination's skill selects it and the foot shows what training costs against the skill credits; training a skill whose training cannot be undone asks first, and yes trains it. A node opening a panel loads it, and the back node returns to the panel left.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SKILL-TREE"),
        station: "auberean-ui-game::dat::play::skills::a_panel_is_laid_out_as_a_tree_and_a_skill_trained_through_its_question",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.skills.the-foot-shows-the-experience-the-points-notice-carries",
        says: "The scripts' notice of the player's points carries the experience to spend high word first, as a 64-bit number sits below a notice's receiver; the skill window's foot shows the number with both words in it, not its words swapped.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SKILL-POINTS-NOTICE"),
        station: "auberean-ui-game::dat::play::skills::the_foot_shows_the_experience_the_points_notice_carries",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.skills.the-trees-scroll-bar-shows-no-thumb-while-the-whole-tree-shows",
        says: "While the whole skill tree is in view its scroll bar is disabled and shows only its \
               track and arrows, no thumb, from the moment the window first shows: the tree finds \
               the scroll bar it names as the window is made.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TREE-SCROLLBAR"),
        station: "auberean-ui-game::dat::play::skills::the_trees_scroll_bar_shows_no_thumb_while_the_whole_tree_shows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.a-fellows-row-follows-its-notices-and-keys-select-fellows",
        says: "A fellow's row's bars follow its notices; the keys for the first nine fellows \
               select them by place in the list.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-FELLOWS-ROW"),
        station: "auberean-ui-game::dat::play::social::a_fellows_row_follows_its_notices_and_keys_select_fellows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.a-fellowship-lists-its-fellows-the-leader-first",
        says: "A fellowship described puts the page in its fellowship state with its name and a \
               row per fellow, the leader moved to the top and each row named with its place; the \
               leader's buttons are ghosted for a player who does not lead; the small panel gets \
               its rows too; each fellow but the player is put on the map.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-FELLOWS"),
        station: "auberean-ui-game::dat::play::social::a_fellowship_lists_its_fellows_the_leader_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.a-place-offered-in-a-fellowship-is-asked-about",
        says: "Someone offering the player a place in a fellowship is asked about with the \
               leader's and the fellowship's names; yes takes the place, no turns it down with its \
               reason.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-FELLOWSHIP-OFFER"),
        station: "auberean-ui-game::dat::play::social::a_place_offered_in_a_fellowship_is_asked_about",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.breaking-and-swearing-ask-first",
        says: "Breaking with the player's patron asks first and, answered yes, breaks and shows \
               the player again; an oath offered is asked about and answered; Swear, lit when the \
               player has no patron and another player is selected, offers the player's oath.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-ALLEGIANCE-OATH"),
        station: "auberean-ui-game::dat::play::social::breaking_and_swearing_ask_first",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.the-allegiance-page-shows-a-member-its-patron-and-vassals",
        says: "The allegiance page asks for the player's profile as its tab opens; a profile shows \
               the allegiance's name, size and monarch, the member with its patron, each vassal \
               and the message of the day; a vassal's Down asks for that vassal's profile; a \
               profile with no member says the player has no allegiance.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-ALLEGIANCE"),
        station: "auberean-ui-game::dat::play::social::the_allegiance_page_shows_a_member_its_patron_and_vassals",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.the-fellowship-page-asks-for-a-new-fellowship",
        says: "With no fellowship the page offers to make one: the looting menu holds the five \
               ways with the second chosen; Create asks for a fellowship with the name entered, \
               the experience setting as the experience button is not lit, and the way chosen.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-FELLOWSHIP"),
        station: "auberean-ui-game::dat::play::social::the_fellowship_page_asks_for_a_new_fellowship",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.social.the-friends-page-lists-friends-by-state",
        says: "The chat service's friends are listed online (lit by their state) and offline under \
               their headings; a friend's row selected lights Remove, another player selected \
               lights Add, and each asks the chat service; the state menu sets the player's state \
               with its emote.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-SOCIAL-FRIENDS"),
        station: "auberean-ui-game::dat::play::social::the_friends_page_lists_friends_by_state",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.statusbar.a-missed-pulse-runs-the-bar-out-over-the-time-left",
        says: "A pulsed effect that misses its pulse runs its bar out over what is left of its time at that moment.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-PULSE-MISSED-CLOCK"),
        station: "auberean-ui-game::dat::play::bar::a_missed_pulse_runs_the_bar_out_over_the_time_left",
        tier: Tier::Dat,
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
        id: "ui.statusbar.the-latency-graph-option-draws-the-bandwidth-over-the-network-indicator",
        says: "With the player's latency graph option on, the network indicator's tooltip gives the \
               bytes in and out each second and a graph is blended over the indicator inside its \
               border: a column a measure, the bytes sent a second down from the top and those \
               received up from the foot, each bar as long as the eighth of the bytes takes halvings \
               to come down to one, the texel past its end part of the way to its color; the dark \
               ground is clear for the measure at which the indicator's state changes; with the \
               option off, no graph.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-NET-GRAPH"),
        station: "auberean-ui-game::dat::play::bar::the_latency_graph_option_draws_the_bandwidth_over_the_network_indicator",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.statusbar.vitals-indicators-and-effects-follow-the-scripts",
        says: "The status bar shows the vitals the player's scripts say as current over greatest, a hero's focus with its numbers; a click on a bar hides its numbers and its tooltip says a click shows them; the conflict and vitae indicators follow the scripts; each effect the scripts list shows as an icon with its name, gone once no longer listed.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STATUSBAR"),
        station: "auberean-ui-game::dat::play::bar::vitals_indicators_and_effects_follow_the_scripts",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.story.a-doors-scene-ends-when-the-player-has-arrived",
        says: "A scene played on the way through a door stays over the screen until the player's \
               body has been sent through and stands at the other side with its cells loaded; \
               the door's scene ending does not end it. Then the story goes, the world shows and the                scripts hear the scenes are done. Portal space does not show on the way.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-DOOR-ARRIVAL"),
        station: "auberean-interface::dat::screens::a_doors_scene_ends_when_the_player_has_arrived",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.story.a-subtitle-waits-for-its-string-table",
        says: "A story step's valid subtitle is given to its frame's text as a string reference: \
               when the data lacks the table its entry's text is in, the text waits until the \
               table has come, then shows the subtitle.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-STORY"),
        station: "auberean-ui-game::dat::play::story::a_subtitle_waits_for_its_string_table_then_shows",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.story.the-hide-all-key-waits-for-a-story",
        says: "While a story is up, the key that hides the whole interface does nothing.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-HIDE-ALL-GATE"),
        station: "auberean-ui-game::dat::play::story::the_hide_all_key_waits_for_a_story",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.story.the-shortcut-strip-shows-in-front-of-the-storys-background",
        says: "The story's background stands at the shortcut strip's depth and came after it, so the strip is drawn in front of it, the only part of the play screen that shows through, and the pointer meets it there.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STORY-STRIP"),
        station: "auberean-ui-game::dat::play::story::the_shortcut_strip_shows_in_front_of_the_storys_background",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.story.the-storys-scenes-play-over-the-screen",
        says: "The scripts' scenes put the story over the screen; once its background has faded \
               in the world hides and the playable scenes' steps play, each picture in the frame \
               whose turn it is with its music, voice and subtitle, for its own time or five \
               seconds; after the last the world shows and the scripts hear which scenes were \
               played. Escape cuts a story short; a portal's scene waits for the player to \
               arrive.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-STORY"),
        station: "auberean-ui-game::dat::play::story::the_storys_scenes_play_over_the_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.text.a-character-the-font-lacks-switches-to-a-fallback-font",
        says: "A string with a character the text element's font lacks switches the element to the \
               first of the two fallback fonts the font mapping names (the Unicode font, then the \
               second) that holds every character of it; when neither does, or the element has no \
               font, the font stays. The fallback lasts for that string: the next string, a typed \
               character or a measured string starts from the element's own font.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-FALLBACK-FONT"),
        station: "auberean-ui::cpu::text::a_character_the_font_lacks_switches_to_a_fallback_font",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.a-one-line-entry-keeps-the-caret-in-view",
        says: "A one-line entry keeps the caret in view: the view stays while the caret is within \
               the width past it, then stands one width before the caret, and going back past its \
               start puts the view at the caret.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-HORIZONTAL-VIEW"),
        station: "auberean-ui::cpu::text::a_one_line_entry_keeps_the_caret_in_view",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.a-reference-waits-for-its-string-tables",
        says: "A string reference given to a text element whose string tables the data lacks is \
               added at the element's tick once they have come, and the element says its download \
               is done; what is added after it waits behind it, in order; a reference whose \
               tables are at hand is added at once.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-ELEMENT"),
        station: "auberean-ui::cpu::text::a_reference_waits_for_its_string_tables_and_what_follows_waits_behind_it",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.an-element-with-no-font-starts-in-the-default-font",
        says: "A text element with no font of its own starts in the default font the font mapping \
               names.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-DEFAULT-FONT"),
        station: "auberean-ui::cpu::text::an_element_with_no_font_starts_in_the_default_font",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.an-entry-of-the-string-table-is-a-reference",
        says: "An element's entry of its string table is added as a string reference: written as \
               the client writes one, waiting for its tables if the data lacks them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-ENTRY-REFERENCE"),
        station: "auberean-ui::cpu::text::an_entry_of_the_string_table_is_a_reference",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.an-entry-taking-the-keyboard-turns-the-input-method-on",
        says: "An editable text element that takes the keyboard turns the host's input method on \
               over the part of the element that shows, in the element's font and color; giving the \
               keyboard up turns it off, and an element marked no_IME never turns it on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TEXT-IME"),
        station: "auberean-ui::cpu::text::an_entry_taking_the_keyboard_turns_the_input_method_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.text.dropped-references-let-their-downloads-go",
        says: "A text element's waiting strings are dropped when its text is replaced and when it \
               goes, each letting go of the string download it waits on.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-ELEMENT-DROPPED"),
        station: "auberean-ui::cpu::text::a_text_element_lets_go_of_its_downloads_when_its_text_is_replaced_or_it_goes",
        tier: Tier::Cpu,
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
        id: "ui.text.the-input-methods-composition-shows-at-the-caret",
        says: "What the input method is composing shows at the caret of the entry that has it on, \
               underlined, and isn't part of the entry's text; what it writes is typed as any text.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-IME-COMPOSE"),
        station: "auberean-ui::cpu::text::the_input_methods_composition_shows_at_the_caret",
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
        id: "ui.toolbar.a-windows-button-shows-and-hides-it-and-its-tooltip-says-which",
        says: "A toolbar window's button shows its window and, pressed again, hides it; its tooltip \
               is the window's string with Show or Hide as the window is; the window's close button \
               hides it; a window does not hide another.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLBAR-WINDOWS"),
        station: "auberean-ui-game::dat::play::standing::a_windows_button_shows_and_hides_it_and_its_tooltip_says_which",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.toolbar.the-windows-keep-their-width-on-a-larger-screen",
        says: "On a screen larger than the layout's the toolbar windows keep their width: the map, \
               skills, quests and social windows reach down to the screen's foot, no taller than a \
               thousand pixels; the options and inventory windows stand in the foot's corners; the \
               examination window stays where the layout puts it.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLBAR-WINDOW-SIZES"),
        station: "auberean-ui-game::dat::play::standing::the_windows_keep_their_width_on_a_larger_screen",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.toolbar.the-wings-follow-the-mount-and-the-sales",
        says: "The scripts' notices that the player is mounted and has a sale waiting show the \
               dismount and consignment buttons and set their wings' states, the consignment button's \
               alert sounding; pressed, the buttons ask to dismount and to show the sales; the \
               notices' opposite hides them.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLBAR-WINGS"),
        station: "auberean-ui-game::dat::play::standing::the_wings_follow_the_mount_and_the_sales",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.tooltip.a-tip-given-as-a-reference-waits-for-its-string-tables",
        says: "An element's own tip given as a string reference goes to the tooltip's text \
               element as one: while the string tables it reads are on their way the tooltip is \
               up with no text, which shows once they have come.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-STRDL-TOOLTIP"),
        station: "auberean-ui::cpu::text::a_tip_given_as_a_reference_waits_for_its_string_tables",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.tooltip.a-tooltip-goes-with-the-window-its-owner-lies-in",
        says: "A window hidden takes the tooltip of an element in it, the pointer unmoved.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TOOLTIP-HIDDEN-WINDOW"),
        station: "auberean-ui::cpu::text::a_tooltip_goes_with_the_window_its_owner_lies_in",
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
        id: "ui.tooltip.the-left-button-down-anywhere-holds-a-tooltip-back",
        says: "With the left button down anywhere, on an element or over the world, no tooltip shows.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-TOOLTIP-LEFT-DOWN"),
        station: "auberean-ui::cpu::text::the_left_button_down_over_the_world_holds_a_tooltip_back",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.tooltip.the-players-delay-and-switch-rule-tooltips",
        says: "The player's tooltip delay is how long the pointer rests before a tooltip shows, one \
               shows at once when the delay comes down to the rest so far; switched off, none shows \
               and the one up goes; switched back on, the element under the pointer shows its own.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-CSTREAM-TOOLTIP-OPTIONS"),
        station: "auberean-ui::cpu::text::the_players_tooltip_switch_turns_them_off_and_back_on",
        tier: Tier::Cpu,
    },
    behaviour! {
        id: "ui.trade.an-offered-item-is-named-for-the-number-offered",
        says: "An offered item's tooltip names it for the number offered, not the number its                owner holds.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TRADE-OFFERED-NAME"),
        station: "auberean-ui-game::dat::play::trade::an_offered_item_is_named_for_the_number_offered",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.trade.an-offered-item-shows-the-selection-and-the-targeting-cursor",
        says: "An offered item's icon, on either side of a trade, takes the selected look while it is the player's selection, and the cursor its element names while an item waits to be used on a target.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TRADE-OFFERED-ITEMS"),
        station: "auberean-ui-game::dat::play::trade::an_offered_item_shows_the_selection_and_the_targeting_cursor",
        tier: Tier::Dat,
    },
    behaviour! {
        id: "ui.trade.the-trade-window-offers-accepts-and-closes",
        says: "A trade the partner asks for puts up a question naming them, and yes opens it; the \
               window shows the partner's name; an item dragged from the pack onto the player's \
               list is offered (a stack asks how many); each side's offers show; accept is asked \
               and ghosted, and a side's acceptance shows until cleared; closed by the partner, \
               the window says so for its delay, then hides.",
        since: RETAIL,
        evidence: Evidence::Private("AC2-EVID-UI-TRADE"),
        station: "auberean-ui-game::dat::play::trade::the_trade_window_offers_accepts_and_closes",
        tier: Tier::Dat,
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
