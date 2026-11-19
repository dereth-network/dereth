//! The plugins' windows and HUDs: the plugin bar opening and closing them, a window drawn with its
//! nodes, a press on a button asked of the plugin, and its close box.
//!
//! Behaviour: none (this client's own interface, not a behaviour of the retail client)

use std::collections::BTreeMap;

use auberean_client_sdk::plugin_api::{NodeKind, UiNode, Value};
use auberean_horizon::ui::plugins::PluginSeat;
use auberean_shell::interface::Request;
use auberean_shell::plugins::PluginWindow;

use crate::{centre, in_world, Harness};

fn node(
    kind: NodeKind,
    id: Option<&str>,
    props: &[(&str, Value)],
    children: Vec<UiNode>,
) -> UiNode {
    UiNode {
        kind,
        id: id.map(str::to_owned),
        props: props
            .iter()
            .map(|(k, v)| ((*k).to_owned(), v.clone()))
            .collect::<BTreeMap<_, _>>(),
        children,
    }
}

/// The sample's window: a line of text and a button.
fn hello(open: bool) -> PluginWindow {
    PluginWindow {
        plugin: "example.hello".into(),
        window: "main".into(),
        tree: node(
            NodeKind::Window,
            Some("main"),
            &[("title", Value::Str("Hello".into()))],
            vec![
                node(
                    NodeKind::Label,
                    Some("where"),
                    &[("text", Value::Str("In the world.".into()))],
                    Vec::new(),
                ),
                node(
                    NodeKind::Button,
                    Some("wave"),
                    &[("text", Value::Str("Wave".into()))],
                    Vec::new(),
                ),
            ],
        ),
        open,
        revision: 1,
    }
}

fn hud() -> PluginWindow {
    PluginWindow {
        plugin: "example.hello".into(),
        window: "status".into(),
        tree: node(
            NodeKind::Hud,
            Some("status"),
            &[("anchor", Value::Str("top_right".into()))],
            vec![node(
                NodeKind::Label,
                Some("line"),
                &[("text", Value::Str("Ready".into()))],
                Vec::new(),
            )],
        ),
        open: true,
        revision: 1,
    }
}

#[test]
fn the_plugin_bar_has_a_button_for_each_window_and_a_press_opens_or_closes_it() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.plugins.windows = vec![hello(false), hud()];
    h.frame(&s);
    let k = h.k();
    let button = PluginSeat::bar_button(k, 0);
    assert!(h.ui.covers(centre(button).0, centre(button).1));
    assert!(
        !h.ui
            .covers(centre(PluginSeat::bar_button(k, 1)).0, centre(button).1),
        "a HUD has no button"
    );
    h.ui.plugins.windows = vec![hello(false)];
    let out = h.click(&s, centre(button));
    assert_eq!(
        out.requests,
        vec![Request::PluginWindow {
            plugin: "example.hello".into(),
            window: "main".into(),
            open: true
        }]
    );
    h.ui.plugins.windows = vec![hello(true)];
    let out = h.click(&s, centre(button));
    assert!(out.requests.contains(&Request::PluginWindow {
        plugin: "example.hello".into(),
        window: "main".into(),
        open: false
    }));
}

#[test]
fn an_open_window_draws_its_nodes_and_a_press_on_its_button_is_the_plugins() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.plugins.windows = vec![hello(true)];
    h.frame(&s);
    let r = *h.ui.plugins.rects().first().expect("drawn");
    assert!(
        h.ui.covers(centre(r).0, centre(r).1),
        "the window answers the pointer"
    );
    let line =
        h.ui.plugins
            .node_rect("example.hello", "main", "where")
            .expect("the label");
    let wave =
        h.ui.plugins
            .node_rect("example.hello", "main", "wave")
            .expect("the button");
    assert!(line.bottom() <= wave.y + 0.5, "the label over the button");
    assert!(
        r.contains(centre(wave).0, centre(wave).1),
        "inside the window"
    );
    h.ui.plugins.windows = vec![hello(true)];
    let out = h.click(&s, centre(wave));
    assert_eq!(
        out.requests,
        vec![Request::PluginClick {
            plugin: "example.hello".into(),
            window: "main".into(),
            node: "wave".into()
        }]
    );
    // Closed, it is not drawn and answers nothing.
    h.ui.plugins.windows = vec![hello(false)];
    h.frame(&s);
    assert!(h.ui.plugins.rects().is_empty());
    assert!(!h.ui.covers(centre(wave).0, centre(wave).1));
}

#[test]
fn a_hud_stands_at_its_anchor_with_no_frame() {
    let mut h = Harness::new();
    let s = in_world();
    h.ui.plugins.windows = vec![hud()];
    h.frame(&s);
    let line =
        h.ui.plugins
            .node_rect("example.hello", "status", "line")
            .expect("drawn");
    assert!(line.x > h.screen.0 / 2.0, "at the right");
    assert!(line.y < h.screen.1 / 2.0, "at the top");
    assert!(h.ui.plugins.rects().is_empty(), "no window frame");
}
