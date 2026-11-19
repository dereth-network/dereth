//! What the tests share: packages made from text, a host running Auberean's bindings, a clock the
//! test moves, stores the test can read, and frames made by hand.

use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::time::Duration;

use auberean_client_contract::event::GameEvent;
use auberean_client_contract::session::Phase;
use auberean_client_contract::view::View;
use auberean_client_contract::Action;
use auberean_client_runtime::frame_hook::HookFrame;
use auberean_plugin_host::{bindings, HostConfig, Package, PluginHost, PluginStorage, VmClock};
use dereth_plugin_api::Limits;
use dereth_plugin_lua::{Bindings, LuauEngine};

/// Auberean's bindings for the Luau engine.
pub const AUBEREAN: Bindings = Bindings {
    name: bindings::NAME,
    layer: bindings::LAYER,
    libraries: &[],
};

/// A package of `main.lua` and a manifest for `game` with `extra` lines.
pub fn package(id: &str, game: &str, extra: &str, main: &str) -> Package {
    let manifest = format!(
        "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.7\"\ngame = \"{game}\"\n{extra}"
    );
    let mut files = BTreeMap::new();
    files.insert("plugin.toml".to_owned(), manifest.into_bytes());
    files.insert("main.lua".to_owned(), main.as_bytes().to_vec());
    Package {
        origin: id.to_owned(),
        files,
    }
}

/// A clock that moves on by `step` each time it is read, or only when the test moves it.
#[derive(Debug, Default)]
pub struct TestClock {
    pub now: Cell<Duration>,
    pub step: Cell<Duration>,
}

impl VmClock for TestClock {
    fn now(&self) -> Duration {
        let t = self.now.get() + self.step.get();
        self.now.set(t);
        t
    }
}

/// A shared clock, so the test keeps a hand on it.
#[derive(Debug, Clone, Default)]
pub struct Shared(pub Rc<TestClock>);

impl VmClock for Shared {
    fn now(&self) -> Duration {
        self.0.now()
    }
}

/// Stores the test can read after the host wrote them.
#[derive(Debug, Clone, Default)]
pub struct Stores(pub Rc<RefCell<BTreeMap<(String, String), String>>>);

impl PluginStorage for Stores {
    fn load(&mut self, plugin: &str, scope: &str) -> Option<String> {
        self.0
            .borrow()
            .get(&(plugin.to_owned(), scope.to_owned()))
            .cloned()
    }

    fn save(&mut self, plugin: &str, scope: &str, text: &str) {
        self.0
            .borrow_mut()
            .insert((plugin.to_owned(), scope.to_owned()), text.to_owned());
    }
}

/// The limits the tests run with: an entry's budget of a minute guards against a hang only, so
/// the machine's clock decides nothing.
pub fn limits() -> Limits {
    Limits {
        call_budget_ms: 60_000.0,
        ..Limits::default()
    }
}

/// A host with Auberean's Luau engine, the clock and the stores given.
pub fn host_with(clock: Shared, stores: Stores) -> PluginHost {
    let mut config = HostConfig::new("test-host", "1.0");
    config.limits = limits();
    PluginHost::new(config, Box::new(clock), Box::new(stores))
        .with_engine(Box::new(LuauEngine::new(AUBEREAN)))
}

/// A host whose clock stands still, with stores in memory.
pub fn host() -> PluginHost {
    host_with(Shared::default(), Stores::default())
}

/// One frame at `t` in `phase` with `events`: the actions the plugins asked for.
pub fn frame(host: &mut PluginHost, t: f64, phase: Phase, events: &[GameEvent]) -> Vec<Action> {
    let mut f = HookFrame::new(events, t, t, phase);
    host.step(&mut f);
    f.take_actions()
}

/// One frame as [`frame`] makes it, carrying `view`.
pub fn frame_seeing(
    host: &mut PluginHost,
    t: f64,
    phase: Phase,
    events: &[GameEvent],
    view: &View,
) -> Vec<Action> {
    let mut f = HookFrame::new(events, t, t, phase).with_view(view);
    host.step(&mut f);
    f.take_actions()
}

/// The texts of the lines printed since last asked, each `name: text` or the host's own text.
pub fn printed(host: &mut PluginHost) -> Vec<String> {
    host.take_printed()
        .into_iter()
        .map(|l| match l.from {
            Some(name) => format!("{name}: {}", l.text),
            None => l.text,
        })
        .collect()
}
