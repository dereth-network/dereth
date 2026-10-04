//! UI fixtures and scenarios for screens.

use super::*;
// ---------------------------------------------------------------------------------------------
// ui.screen-rebuild.*
// ---------------------------------------------------------------------------------------------

/// Putting the same screen up again builds it fresh, and what the old one was told is gone.
pub(super) fn putting_the_same_screen_up_again_builds_it_fresh() {
    use dereth_client_model::combat::PowerBarMode;
    use dereth_ui_screens::bind::{attr, attr_float};

    /// The bars the shipped tree carries, and the meter inside each.
    fn bars(c: &HeadlessClient) -> Vec<(dereth_ui::ElemHandle, dereth_ui::ElemHandle)> {
        c.view()
            .expect_app()
            .hud()
            .panels
            .power_bar
            .bars
            .iter()
            .map(|b| {
                (
                    b.element.expect("the shipped bar"),
                    b.bound.get("bar").expect("the shipped meter"),
                )
            })
            .collect()
    }

    /// What each bar is showing.
    fn showing(c: &HeadlessClient) -> Vec<(bool, dereth_ui::StateId, Option<f32>)> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        bars(c)
            .into_iter()
            .map(|(element, meter)| {
                let n = ui.node(element).expect("the bar is live");
                (
                    n.region.flags.visible,
                    n.state,
                    attr_float(ui, meter, attr::METER_LEVEL),
                )
            })
            .collect()
    }

    /// The one bar that is on the notice lists -- the only one a message can reach.
    fn subscriber(c: &HeadlessClient) -> Vec<(bool, dereth_ui::StateId, Option<f32>)> {
        let app = c.view().expect_app();
        let ui = &app.ui().expect("the UI shell is up").ui;
        app.hud()
            .panels
            .power_bar
            .bars
            .iter()
            .filter(|b| b.registered)
            .map(|b| {
                let element = b.element.expect("the shipped bar");
                let meter = b.bound.get("bar").expect("the shipped meter");
                let n = ui.node(element).expect("the bar is live");
                (
                    n.region.flags.visible,
                    n.state,
                    attr_float(ui, meter, attr::METER_LEVEL),
                )
            })
            .collect()
    }

    fn charge(c: &mut HeadlessClient, mode: PowerBarMode, level: f32) {
        let combat = &mut c.objects_mut().world.combat;
        combat.begin_power_bar(mode, false, 0);
        combat.set_power_bar_level(level);
    }

    let mut c = HeadlessClient::new(ClientSpec::gameplay(4));
    // The state the shipped layout builds, read before anything is told to any of it.
    let pristine = showing(&c);
    let old_bars = bars(&c);
    let two_bars = old_bars.len() == 2;

    // The positive control: the old widgets really do take a charge.
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.25);
    c.tick(1);
    let old_took_it = subscriber(&c)
        .iter()
        .all(|(v, _, l)| *v && *l == Some(0.25));

    // Now a charge sent while the old widgets are still there, and the same screen queued again.
    let switches = c.view().expect_app().ui().expect("shell").flow.switches;
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.9);
    c.app_mut()
        .queue_ui_mode(dereth_ui::framework::mode::GAME_PLAY);
    c.tick(1);

    let rebuilt = {
        let app = c.view().expect_app();
        let shell = app.ui().expect("shell");
        shell.flow.current_mode() == Some(dereth_ui::framework::mode::GAME_PLAY)
            && shell.flow.switches == switches + 1
            // Every one of the old elements is gone rather than reused.
            && old_bars.iter().all(|(b, m)| shell.ui.node(*b).is_none() && shell.ui.node(*m).is_none())
    };
    let new_bars = bars(&c);
    let rebound = new_bars != old_bars;
    // The new widgets came up in the state the layout gives them, not in the state the old ones
    // were left in: the charge sent while the old ones existed does not reach them.
    let fresh = showing(&c) == pristine;

    // An unchanged frame neither rebuilds again nor replays what was dropped.
    c.tick(1);
    let settled = bars(&c) == new_bars
        && showing(&c) == pristine
        && c.view().expect_app().ui().expect("shell").flow.switches == switches + 1;

    // ...and the new widgets take a new charge.
    charge(&mut c, PowerBarMode::AdvancedCombat, 0.5);
    c.tick(1);
    let new_took_it = subscriber(&c).iter().all(|(v, _, l)| *v && *l == Some(0.5));

    c.assert_behaviour(
        "ui.screen-rebuild.putting-the-same-screen-up-again-builds-it-fresh",
        move |_| two_bars && old_took_it && rebuilt && rebound && fresh && settled && new_took_it,
    );
    c.shutdown();
}
