//! The client's chat failure-event handler, re-exported.
//!
//! Every production item -- the `ARMS` table, `Arm`, `arm_for`, `handle_failure_event` and the
//! named `WeenieError` codes -- lives in [`dereth_client_contract::chat::failure`], because
//! `dereth_client_shell::hud` turns nine inbound error events into chat lines through it and nothing in
//! it names a `dereth_ui` type. See that module for what it is.
//!
//! The tests live here: four of them assert the arm's chat type against
//! [`crate::chat::colors::color_for_type`] and the main window's default filter through
//! [`crate::chat::interface::ChatInterface`], both of which are presentation. They read the table
//! through the glob below.
pub use dereth_client_contract::chat::failure::*;

#[cfg(test)]
mod tests {
    use super::*;

    /// The two channel lines are the sentences retail prints.
    #[test]
    fn the_two_channel_lines_are_the_sentences_retail_prints() {
        let m = handle_failure_event(YOU_HAVE_ENTERED_THE_CHANNEL, "Trade").expect("an arm");
        assert_eq!(m.body, "You have entered the Trade channel.\n");
        assert_eq!(
            handle_failure_event(0x051B, "LFG").expect("an arm").body,
            "You have entered the LFG channel.\n"
        );
        assert_eq!(
            handle_failure_event(0x051C, "Trade").expect("an arm").body,
            "You have left the Trade channel.\n"
        );
    }

    /// Oracle: both arms add the line to the scroll with chat type **0**, which the
    /// colour table leaves at its green fill. This is the assertion that makes the line the same
    /// colour retail draws it; a `LocalError` (`0x1A`) here would come out bright red and would be
    /// filtered out of the main window entirely.
    #[test]
    fn both_arms_are_chat_type_zero_which_is_the_green_default() {
        let m = handle_failure_event(YOU_HAVE_ENTERED_THE_CHANNEL, "Trade").expect("an arm");
        assert_eq!(m.ty, 0);
        assert_eq!(super::super::colors::color_for_type(m.ty).hex, 0x80FF7F);
        assert_eq!(
            m.window, 0,
            "a broadcast, subject to every window's own filter"
        );
        assert!(m.prefix.is_none(), "no speaker");
        // …and the main window's default filter does **not** mask type 0 out, which is the other
        // half of it reaching the pane a player is looking at.
        let w = super::super::interface::ChatInterface::new(super::super::interface::window::MAIN);
        assert!(w.type_is_active(m.ty));
    }

    /// An unknown code is dropped rather than invented.
    #[test]
    fn an_unknown_code_is_dropped_rather_than_invented() {
        assert!(handle_failure_event(0x0000, "x").is_none());
        assert!(
            handle_failure_event(0x0016, "x").is_none(),
            "one below the first band"
        );
        assert!(
            handle_failure_event(0x0594, "x").is_none(),
            "one past the last band"
        );
        assert!(
            handle_failure_event(0xFFFF_FFFF, "x").is_none(),
            "no band can reach it"
        );
        let m = handle_failure_event(0x051A, "x").expect("0x51A is a real arm");
        assert_eq!(m.body, "Only the original owner may use that item's magic.");
        assert_eq!(m.ty, LOCAL_ERROR_CHAT_TYPE);
    }

    /// The fellowship arms are the literals and the type the bytes carry.
    #[test]
    fn the_fellowship_arms_are_the_literals_and_the_type_the_bytes_carry() {
        let open = handle_failure_event(IS_NOW_OPEN_FELLOWSHIP, "Of The ring").expect("an arm");
        assert_eq!(
            open.body,
            "Of The ring is now an open fellowship; anyone may recruit new members.\n"
        );
        assert_eq!(open.ty, 0);
        let closed = handle_failure_event(IS_NOW_CLOSED_FELLOWSHIP, "Of The ring").expect("an arm");
        assert_eq!(closed.body, "Of The ring is now a closed fellowship.\n");
        assert_eq!(closed.ty, 0);
        let lead = handle_failure_event(IS_NOW_LEADER_OF_FELLOWSHIP, "Caius").expect("an arm");
        assert_eq!(lead.body, "Caius is now the leader of this fellowship.\n");
        assert_eq!(lead.ty, 0);
        let passed =
            handle_failure_event(YOU_HAVE_PASSED_FELLOWSHIP_LEADERSHIP_TO, "Ash").expect("an arm");
        assert_eq!(
            passed.body,
            "You have passed leadership of the fellowship to Ash\n"
        );
        assert_eq!(passed.ty, 0);
        let locked = handle_failure_event(LOCKED_FELLOWSHIP_CANNOT_RECRUIT, "Bex").expect("an arm");
        assert_eq!(
            locked.body,
            "This fellowship is locked; Bex cannot be recruited into the fellowship.\n"
        );
        assert_eq!(locked.ty, 0);
        // The `0x1a` arms of the `0x40a..0x489` band and the two bare-code ones.
        for (code, text) in [
            (
                YOU_MUST_BE_LEADER_OF_FELLOWSHIP,
                "You must be the leader of a Fellowship",
            ),
            (YOUR_FELLOWSHIP_IS_FULL, "Your Fellowship is full"),
            (
                FELLOWSHIP_NAME_IS_NOT_PERMITTED,
                "That Fellowship name is not permitted",
            ),
            (
                YOU_DO_NOT_BELONG_TO_A_FELLOWSHIP,
                "You do not belong to a Fellowship.",
            ),
            (
                YOUR_OFFER_OF_ALLEGIANCE_WAS_IGNORED,
                "Your offer of Allegiance has been ignored.",
            ),
        ] {
            let m = handle_failure_event(code, "").expect("an arm");
            assert_eq!(m.body, text);
            assert_eq!(m.ty, LOCAL_ERROR_CHAT_TYPE, "{code:#x}");
        }
        assert_eq!(
            handle_failure_event(OLTHOI_CANNOT_JOIN_FELLOWSHIP, "")
                .expect("an arm")
                .ty,
            7
        );
        // Code `0x417` uses the two-level dispatch based at `0x40A`, and `0x51D` the direct
        // table based at `0x4E9`; both entries resolve to the no-message default arm.
        assert!(
            handle_failure_event(0x0417, "").is_none(),
            "FellowshipIgnoringRequests: silent"
        );
        assert!(
            handle_failure_event(0x051D, "").is_none(),
            "TurbineChatIsEnabled: silent"
        );
        assert!(
            handle_failure_event(0x04DB, "").is_none(),
            "FellowshipDeclined: silent"
        );
    }

    /// **A-F19.** Both the literal and the
    /// chat type it is added to the scroll with are recovered, so this asserts the pair rather
    /// than the sentence alone. `MAGIC` is light blue and `LOCAL_ERROR` red; printing either at type `0`
    /// would be a green line, which is what a one-constant fix would have produced.
    #[test]
    fn the_portal_refusal_is_the_sentence_and_the_colour_retail_prints() {
        let m = handle_failure_event(YOU_MUST_COMPLETE_QUEST_TO_USE_PORTAL, "").expect("an arm");
        assert_eq!(
            m.body,
            "You must complete a quest to interact with that portal.\n"
        );
        assert_eq!(m.ty, 7, "added with chat type 7 -- MAGIC, not Broadcast");
        // Light blue, not the green fill a type-0 line would take.
        assert_eq!(super::super::colors::color_for_type(m.ty).hex, 0x3FBFFF);

        let c = handle_failure_event(ACTION_CANCELLED, "").expect("an arm");
        assert_eq!(c.body, "Action cancelled!");
        assert_eq!(c.ty, 0x1A, "added with chat type 0x1a -- LOCAL_ERROR");
        assert_eq!(super::super::colors::color_for_type(c.ty).hex, 0xFF0000);

        // ...and the two arms that were here before still carry `0`, so the constant this unit
        // re-documented did not change value for anything that already read it.
        assert_eq!(
            handle_failure_event(YOU_HAVE_ENTERED_THE_CHANNEL, "Trade")
                .expect("an arm")
                .ty,
            FAILURE_EVENT_CHAT_TYPE
        );
    }

    /// You have moved too far is the sentence and the surface retail uses.
    #[test]
    fn you_have_moved_too_far_is_the_sentence_and_the_surface_retail_uses() {
        let m = handle_failure_event(YOU_HAVE_MOVED_TOO_FAR, "").expect("0x498 has an arm");
        assert_eq!(m.body, "You have moved too far!");
        assert_eq!(
            m.ty, LOCAL_ERROR_CHAT_TYPE,
            "the owner's line is chat type 0x1A"
        );
        assert_eq!(super::super::colors::color_for_type(m.ty).hex, 0xFF0000);
        let w = super::super::interface::ChatInterface::new(super::super::interface::window::MAIN);
        assert!(
            !w.type_is_active(m.ty),
            "0xFBFFFFFF has bit 26 clear: the scrollback drops it"
        );
    }

    /// The arm table is sorted unique and the size the ladder produces.
    #[test]
    fn the_arm_table_is_sorted_unique_and_the_size_the_ladder_produces() {
        assert_eq!(ARMS.len(), 344, "the band ladder reaches exactly 344 codes");
        for w in ARMS.windows(2) {
            assert!(
                w[0].0 < w[1].0,
                "{:#x} then {:#x}: not sorted/unique",
                w[0].0,
                w[1].0
            );
        }
        assert_eq!(ARMS[0].0, 0x0017, "the lowest failure band starts at 0x17");
        assert_eq!(
            ARMS[ARMS.len() - 1].0,
            0x0593,
            "0x4E9 + 0xAA is the top band's last slot"
        );
        let mut broadcast = 0;
        let mut magic = 0;
        let mut local = 0;
        let mut silent = 0;
        for &(code, ty, arm) in &ARMS {
            match ty {
                BROADCAST_CHAT_TYPE => broadcast += 1,
                MAGIC_CHAT_TYPE => magic += 1,
                LOCAL_ERROR_CHAT_TYPE => local += 1,
                other => panic!("{code:#x}: chat type {other:#x} is not one of retail's three"),
            }
            if arm == Arm::Silent {
                silent += 1;
                assert!(arm.render("x").is_none(), "{code:#x}: Silent draws no line");
            } else {
                let body = arm
                    .render("x")
                    .expect("a non-Silent arm always composes a line");
                assert!(
                    !body.is_empty(),
                    "{code:#x}: an arm with an empty line is a bad row"
                );
            }
        }
        // The handler's own histogram, counted per arm in retail.
        assert_eq!((broadcast, magic, local), (162, 59, 123));
        assert_eq!(silent, 3, "the three abuse-report codes");
    }

    /// Each composition shape is the one the arm performs.
    #[test]
    fn each_composition_shape_is_the_one_the_arm_performs() {
        // `Lit` -- the client, the server's string unused even when it carries one.
        assert_eq!(
            handle_failure_event(0x001D, "ignored")
                .expect("an arm")
                .body,
            "You're too busy!"
        );
        // `Fmt`, one `%s`.
        assert_eq!(
            handle_failure_event(0x051B, "Trade").expect("an arm").body,
            "You have entered the Trade channel.\n"
        );
        // `Fmt`, two `%s` -- the client pushes the same pointer twice, so both are the same word.
        assert_eq!(
            handle_failure_event(0x0052, "Frundi").expect("an arm").body,
            "You fail to affect Frundi because Frundi is not a player killer!\n"
        );
        // `Suffix` -- the client: `s = text; s += lit`.
        assert_eq!(
            handle_failure_event(0x002B, "Bael'Zharon")
                .expect("an arm")
                .body,
            "Bael'Zharon cannot carry anymore.\n"
        );
        // `Wrap`.
        assert_eq!(
            handle_failure_event(0x0518, "Bex").expect("an arm").body,
            "This fellowship is locked; Bex cannot be recruited into the fellowship.\n"
        );
        // `SuffixMid` -- the three chained `operator+`.
        assert_eq!(
            handle_failure_event(0x04F8, "Frundi").expect("an arm").body,
            "Frundi fails to affect you because you are not the same sort of player killer as \
             Frundi!\n"
        );
        // `Text` -- the client adds the server's string straight to the scroll.
        let afk = handle_failure_event(0x055E, "Frundi is away").expect("an arm");
        assert_eq!(afk.body, "Frundi is away");
        assert_eq!(afk.ty, BROADCAST_CHAT_TYPE);
        // `FmtOr` -- the client substitutes `L"item"` when the server's string is empty.
        assert_eq!(
            handle_failure_event(0x04BF, "").expect("an arm").body,
            "The item was not suitable for salvaging."
        );
        assert_eq!(
            handle_failure_event(0x04BF, "Iron Key")
                .expect("an arm")
                .body,
            "The Iron Key was not suitable for salvaging."
        );
        // `Silent` -- the client raises its abuse-report-response notice and draws nothing.
        for code in [0x04B8, 0x04B9, 0x04BA] {
            assert!(
                handle_failure_event(code, "x").is_none(),
                "{code:#x} draws no chat line"
            );
            assert!(
                arm_for(code).is_some(),
                "{code:#x} still has an arm -- it just draws nothing"
            );
        }
    }

    /// The three jump refusals carry the global text.
    #[test]
    fn the_three_jump_refusals_carry_the_globals_text() {
        assert_eq!(
            handle_failure_event(0x0048, "").expect("an arm").body,
            "You can't jump from this position"
        );
        assert_eq!(
            handle_failure_event(0x0024, "").expect("an arm").body,
            "You can't jump while in the air"
        );
        assert_eq!(
            handle_failure_event(0x0049, "").expect("an arm").body,
            "You're too loaded down to jump"
        );
        for code in [0x0024, 0x0048, 0x0049] {
            assert_eq!(
                handle_failure_event(code, "").expect("an arm").ty,
                LOCAL_ERROR_CHAT_TYPE
            );
        }
    }
}
