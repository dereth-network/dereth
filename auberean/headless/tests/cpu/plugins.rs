//! The headless client's plugin lines over the plugin host, with the sample plugin read from its
//! folder and frames made by hand.

use std::cell::Cell;
use std::path::PathBuf;
use std::time::Duration;

use auberean_client_sdk::contract::event::{EffectEvent, GameEvent};
use auberean_client_sdk::plugins::{MemoryStorage, PluginHost, VmClock};
use auberean_client_sdk::{HookFrame, Phase};
use auberean_headless::plugins::{host, line, DirSource};

/// A clock that stands still: no entry takes any time.
#[derive(Debug, Default)]
struct Still(Cell<Duration>);

impl VmClock for Still {
    fn now(&self) -> Duration {
        self.0.get()
    }
}

fn sample() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../plugins/effects")
}

fn frame(h: &mut PluginHost, t: f64, events: &[GameEvent]) {
    let mut f = HookFrame::new(events, t, t, Phase::World);
    h.step(&mut f);
}

fn printed(h: &mut PluginHost) -> Vec<String> {
    h.take_printed().into_iter().map(|l| l.text).collect()
}

/// The headless client's plugin lines load the sample from its folder, list it, carry a typed
/// command to it and press its window's button, and unload it; the sample hears the effects put
/// on and taken off and counts them.
///
/// Behaviour: plugins.headless.plugin-lines-load-list-command-and-unload
#[test]
fn the_plugin_lines_load_the_sample_list_it_run_its_command_and_unload_it() {
    let mut h = host(
        Box::<Still>::default(),
        Box::<MemoryStorage>::default(),
        DirSource::default(),
    );
    let dir = sample();
    assert!(line(&mut h, &format!("plugin load {}", dir.display()))
        .expect("loads")
        .is_empty());
    frame(&mut h, 0.0, &[]);
    assert_eq!(printed(&mut h), ["Effects is loaded."]);
    assert_eq!(
        line(&mut h, "plugin list").expect("lists"),
        ["example.effects 0.1.0 running active"]
    );
    frame(
        &mut h,
        0.1,
        &[
            GameEvent::Effect(EffectEvent::Added { effect: 12 }),
            GameEvent::Effect(EffectEvent::Added { effect: 30 }),
            GameEvent::Effect(EffectEvent::Removed { effect: 12 }),
        ],
    );
    assert_eq!(
        printed(&mut h),
        ["Effect 12 is on.", "Effect 30 is on.", "Effect 12 is off."]
    );
    assert!(line(&mut h, "/effects").expect("claimed").is_empty());
    assert!(line(&mut h, "/nobody").is_err());
    frame(&mut h, 0.2, &[]);
    assert_eq!(printed(&mut h), ["1 on now; 2 put on, 1 taken off."]);
    let dump = line(&mut h, "ui dump example.effects main").expect("a window");
    assert_eq!(dump[0], "example.effects main (open)");
    line(&mut h, "ui click example.effects main reset").expect("a button");
    frame(&mut h, 0.3, &[]);
    let w = h.window("example.effects", "main").expect("the window");
    assert_eq!(
        w.tree.find("counts").expect("label").text("text"),
        "1 on now; 0 put on, 0 taken off."
    );
    assert!(line(&mut h, "plugin console example.effects").is_ok());
    assert!(line(&mut h, "plugin unload example.effects")
        .expect("unloads")
        .is_empty());
    frame(&mut h, 0.4, &[]);
    assert_eq!(printed(&mut h), ["Effects is unloaded."]);
    assert!(line(&mut h, "plugin list").expect("lists").is_empty());
    assert!(line(&mut h, "plugin dance").is_err());
}
