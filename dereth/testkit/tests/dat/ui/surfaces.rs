//! UI fixtures and scenarios for surfaces.

use super::*;
// =============================================================================================
// ui.surface.* -- where an element's drawing surface takes its size from
//
// The authored setting choosing where an element's surface takes its size from is one of four
// answers, not a yes/no.
// =============================================================================================

// The setting, and the four answers, named the way the scenarios below read them.

/// A whole raised screen, settled.
fn a_raised_screen(m: dereth_ui::UiMode) -> HeadlessClient {
    let mut c = HeadlessClient::new(ClientSpec::screen(m, 8));
    c.tick(1);
    c
}

/// The elements of a live tree that own a surface of their own -- the ones the walk stops at.
fn surface_owners(c: &HeadlessClient) -> Vec<ElemHandle> {
    let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
    every_element(ui)
        .into_iter()
        .filter(|h| ui.node(*h).is_some_and(|n| n.flags.should_own_object()))
        .collect()
}

// ---------------------------------------------------------------------------------------------
// ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default
// ---------------------------------------------------------------------------------------------

/// The sweep, with its denominator: "they all answered the same" is a result only if there were
/// elements to answer.
pub(super) fn every_element_of_a_live_screen_is_sized_from_its_own_box() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);

    let (elements, answers) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let all = every_element(ui);
        let mut answers: std::collections::BTreeMap<u32, usize> = std::collections::BTreeMap::new();
        for h in &all {
            *answers
                .entry(ui.current_ui_object_mode(*h).value())
                .or_default() += 1;
        }
        (all.len(), answers)
    };
    let a_whole_tree = elements > 100;
    // This screen authors the setting nowhere, so every element must land on the default the
    // readers carry -- and it is the element's own box, not "owns nothing".
    let every_one_takes_its_own_box =
        answers.keys().copied().collect::<Vec<u32>>() == vec![UiObjectMode::ElementSize.value()];
    let the_default_is_the_elements_own_box = UiObjectMode::default() == UiObjectMode::ElementSize;

    c.assert_behaviour(
        "ui.surface.every-element-of-a-live-screen-is-sized-from-its-own-box-by-default",
        move |_| a_whole_tree && every_one_takes_its_own_box && the_default_is_the_elements_own_box,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners
// ---------------------------------------------------------------------------------------------

/// Driven rather than hoped for: no raised screen need carry an element that owns no surface, so
/// the setting is written through the client's own attribute path and read back for that element
/// **and** for a child of it.
pub(super) fn an_element_owning_no_surface_is_stepped_over() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);

    // Something that owns a surface and has a child, so the walk has both an owner to find and
    // somewhere to walk from.
    let (parent, child) = {
        let owners = surface_owners(&c);
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        owners
            .into_iter()
            .find_map(|h| ui.children(h).first().map(|ch| (h, *ch)))
            .expect("this screen has a surface-owning element with a child")
    };

    let (before, the_owning_stopped, up, down) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        // The precondition: a write that changed nothing could not prove the walk.
        let before = shell.ui.current_ui_object_mode(parent) == UiObjectMode::ElementSize
            && shell.ui.current_ui_object_mode(child) == UiObjectMode::ElementSize
            && shell
                .ui
                .node(parent)
                .expect("live")
                .flags
                .should_own_object();

        shell
            .ui
            .set_attribute_enum(parent, attr::UI_OBJECT_MODE, UiObjectMode::NoObject.value());

        let the_owning_stopped = !shell
            .ui
            .node(parent)
            .expect("live")
            .flags
            .should_own_object()
            && shell.ui.node(parent).expect("live").region.object_mode == UiObjectMode::NoObject;
        (
            before,
            the_owning_stopped,
            shell.ui.current_ui_object_mode(parent),
            shell.ui.current_ui_object_mode(child),
        )
    };
    // The element that owns nothing is walked past: it is never the answer, and its child lands
    // on the very same ancestor it does.
    let stepped_over = up != UiObjectMode::NoObject && down != UiObjectMode::NoObject && up == down;

    c.assert_behaviour(
        "ui.surface.an-element-owning-none-is-stepped-over-and-the-answer-is-its-owners",
        move |_| before && the_owning_stopped && stepped_over,
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for
// ---------------------------------------------------------------------------------------------

/// An A/B with one field changed. A test on a picker alone could not tell "forced" from "the
/// default", since the forced answer is also the default -- so a twin of the same tree takes the
/// same authored setting and must keep it.
pub(super) fn a_colour_picker_is_sized_from_its_own_box_whatever_it_asks_for() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHARACTER_MANAGEMENT);

    let owners = surface_owners(&c);
    let one_owner = owners.len() == 1;
    let control = owners[0];
    let picker = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        *ui.children(control)
            .first()
            .expect("the root has a child to promote")
    };

    let (promoted, both_took_the_write, the_control_kept_it, picker_answer, control_answer) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        // The A/B needs two elements the walk stops at, so the second is promoted the same way
        // the maker of a layout root promotes one.
        shell.ui.set_should_own_object(picker, true);
        let promoted = shell
            .ui
            .node(picker)
            .expect("live")
            .flags
            .should_own_object();

        let mut both_took_the_write = true;
        for h in [control, picker] {
            shell
                .ui
                .set_attribute_enum(h, attr::UI_OBJECT_MODE, UiObjectMode::StateSize.value());
            both_took_the_write &=
                shell.ui.node(h).expect("live").region.object_mode == UiObjectMode::StateSize;
        }
        let the_control_kept_it =
            shell.ui.current_ui_object_mode(control) == UiObjectMode::StateSize;

        // The one field that differs between the two.
        shell.ui.node_mut(picker).expect("live").desc.ty = dereth_ui::factory::ty::COLOR_PICKER;
        (
            promoted,
            both_took_the_write,
            the_control_kept_it,
            shell.ui.current_ui_object_mode(picker),
            shell.ui.current_ui_object_mode(control),
        )
    };
    let the_picker_is_forced = picker_answer == UiObjectMode::ElementSize;
    let the_twin_is_untouched = control_answer == UiObjectMode::StateSize;

    c.assert_behaviour(
        "ui.surface.a-colour-picker-is-sized-from-its-own-box-whatever-the-layout-asks-for",
        move |_| {
            one_owner
                && promoted
                && both_took_the_write
                && the_control_kept_it
                && the_picker_is_forced
                && the_twin_is_untouched
        },
    );
    c.shutdown();
}

// ---------------------------------------------------------------------------------------------
// ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks
// ---------------------------------------------------------------------------------------------

/// The arm no raised screen reaches: a shipped layout root that authors "owns nothing". Driven
/// against a root of the same mechanism that authors nothing at all, because a test that only
/// ever raises the first could not tell "the authored setting took it away" from "roots never get
/// one".
pub(super) fn a_shipped_root_that_owns_no_surface_is_left_without_one() {
    let mut c = a_raised_screen(dereth_ui::framework::mode::CHAR_GEN);
    let store = c.dat_store().expect("the retail dats are open").clone();

    let (authored, the_bit_was_taken_away, owns_nothing, the_walk_lands_on_the_default) = {
        let shell = c.app_mut().ui_mut().expect("the UI shell is up");
        let zero = shell
            .ui
            .create_root_by_data_id(
                &*store,
                dereth_primitives::DataId(0x2100_0001),
                dereth_ui::ElementId(0x1000_0419),
            )
            .expect("the shipped layout's root element is created");
        let authored = shell
            .ui
            .node(zero)
            .expect("live")
            .merged_properties()
            .get_enum(attr::UI_OBJECT_MODE)
            == Some(UiObjectMode::NoObject.value());
        (
            authored,
            !shell.ui.node(zero).expect("live").flags.should_own_object(),
            shell.ui.node(zero).expect("live").region.object_mode == UiObjectMode::NoObject,
            shell.ui.current_ui_object_mode(zero) == UiObjectMode::ElementSize,
        )
    };

    // The control: a root the screen raised for itself, through the very same maker.
    let (control_authors_nothing, the_control_kept_its_surface) = {
        let ui = &c.view().expect_app().ui().expect("the UI shell is up").ui;
        let t = every_element(ui)
            .into_iter()
            .find(|h| {
                *h != ui.root()
                    && ui.node(*h).is_some_and(|n| {
                        n.flags.is_root_element()
                            && n.merged_properties()
                                .get_enum(attr::UI_OBJECT_MODE)
                                .is_none()
                    })
            })
            .expect("the screen raised a root of its own, or there is no control");
        (true, ui.node(t).expect("live").flags.should_own_object())
    };

    c.assert_behaviour(
        "ui.surface.a-shipped-root-that-owns-none-is-left-without-one-though-the-maker-asks",
        move |_| {
            authored
                && the_bit_was_taken_away
                && owns_nothing
                && the_walk_lands_on_the_default
                && control_authors_nothing
                && the_control_kept_its_surface
        },
    );
    c.shutdown();
}
