//! The engine against a recording host: the bindings reach the host with the right operations, and
//! the sandbox, the budget and the memory cap hold.

use std::collections::BTreeMap;

use dereth_plugin_api::{
    CallError, Event, HostCalls, Limits, ScriptEngine, ScriptErrorKind, ScriptInstance, Value,
};

use super::LuauEngine;

/// A host that answers the few operations the tests need and records every call.
#[derive(Default)]
struct Recorder {
    calls: Vec<(String, Vec<Value>)>,
    console: Vec<String>,
    modules: BTreeMap<String, String>,
    store: BTreeMap<String, Value>,
    next_timer: i64,
}

impl HostCalls for Recorder {
    fn call(&mut self, op: &str, args: &[Value]) -> Result<Value, CallError> {
        self.calls.push((op.to_owned(), args.to_vec()));
        match op {
            "task.timer" => {
                self.next_timer += 1;
                Ok(Value::Int(self.next_timer))
            }
            "store.set" => {
                let key = args
                    .get(1)
                    .and_then(Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                self.store
                    .insert(key, args.get(2).cloned().unwrap_or_default());
                Ok(Value::Nil)
            }
            "store.get" => Ok(self
                .store
                .get(args.get(1).and_then(Value::as_str).unwrap_or_default())
                .cloned()
                .unwrap_or_default()),
            "time.clock" => Ok(Value::Num(12.5)),
            // Object 0xDEAD: the thread set aside for a while, as the system may do in the middle
            // of an entry.
            "object.exists" if args.first().and_then(Value::as_i64) == Some(0xDEAD) => {
                std::thread::sleep(std::time::Duration::from_millis(10));
                Ok(Value::Bool(false))
            }
            "player.get" if args.first().and_then(Value::as_str) == Some("name") => {
                Ok(Value::from("Tester"))
            }
            "commands.add" => Err(CallError::Failed(
                "commands.add is not available here".into(),
            )),
            _ => Ok(Value::Nil),
        }
    }

    fn console(&mut self, line: &str) {
        self.console.push(line.to_owned());
    }

    fn module_source(&mut self, name: &str) -> Option<String> {
        self.modules.get(name).cloned()
    }
}

impl Recorder {
    fn ops(&self) -> Vec<&str> {
        self.calls.iter().map(|(op, _)| op.as_str()).collect()
    }
}

fn vm() -> Box<dyn ScriptInstance> {
    LuauEngine
        .instantiate(Limits {
            memory_bytes: 8 << 20,
            call_budget_ms: 50.0,
            profile: false,
        })
        .expect("vm")
}

#[test]
fn a_handler_registered_with_on_hears_its_event_with_the_argument_table() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        r#"on("chat.line", function(e) print("heard", e.text, e.window) end)"#,
    )
    .expect("load");
    assert_eq!(host.ops(), ["events.subscribe"]);
    vm.deliver(
        &mut host,
        &Event::new(
            "chat.line",
            Value::map().with("text", "hello").with("window", 1_i64),
        ),
    )
    .expect("deliver");
    assert_eq!(host.console, ["heard\thello\t1"]);
}

#[test]
fn a_failing_handler_is_reported_with_its_place_and_the_others_still_run() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "on('x', function() error('first') end)\non('x', function() print('second ran') end)",
    )
    .expect("load");
    let err = vm
        .deliver(&mut host, &Event::new("x", Value::map()))
        .expect_err("fails");
    assert_eq!(err.kind, ScriptErrorKind::Runtime);
    assert!(
        err.message.contains("main.lua:1") && err.message.contains("first"),
        "{}",
        err.message
    );
    assert_eq!(host.console, ["second ran"]);
}

#[test]
fn an_endless_loop_is_stopped_by_the_budget_and_the_vm_keeps_working() {
    let mut host = Recorder::default();
    let mut vm = vm();
    let err = vm
        .load(
            &mut host,
            "main.lua",
            "local n = 0\nwhile true do n = n + 1 end",
        )
        .expect_err("stopped");
    assert_eq!(err.kind, ScriptErrorKind::Budget);
    // The overrun names where the script was, so its author can find the long work.
    assert!(err.message.contains("main.lua:2"), "{}", err.message);
    // A handler that swallows the error inside its own pcall is still stopped.
    vm.load(
        &mut host,
        "again.lua",
        "on('spin', function() while true do pcall(function() while true do end end) end end)",
    )
    .expect("load");
    let err = vm
        .deliver(&mut host, &Event::new("spin", Value::map()))
        .expect_err("stopped");
    assert_eq!(err.kind, ScriptErrorKind::Budget);
    vm.load(&mut host, "after.lua", "print('still here')")
        .expect("usable");
    assert_eq!(host.console, ["still here"]);
}

/// Behaviour: none (a unit check of the budget's measure; `core/budget` pins the budget's
/// behaviour).
#[test]
fn time_the_host_spends_answering_is_the_plugins() {
    let mut host = Recorder::default();
    let mut vm = LuauEngine
        .instantiate(Limits {
            memory_bytes: 8 << 20,
            call_budget_ms: 2.0,
            profile: false,
        })
        .expect("vm");
    // The recorder takes at least 10 ms to answer for object 0xDEAD. With hardly any work of the
    // plugin's own, a few such calls are past the load's deadline (its budgets, 50 ms) by more
    // than any stall: the entry is stopped after seven or eight at most, not run on for fifty.
    let err = vm
        .load(
            &mut host,
            "slow.lua",
            "local n = 0 for i = 1, 50 do n = n + 1 local _ = game.world:get(0xDEAD) end print('done', n)",
        )
        .expect_err("over its budget");
    assert_eq!(err.kind, ScriptErrorKind::Budget);
    assert!(host.console.is_empty());
    let slow = host
        .calls
        .iter()
        .filter(|(op, _)| op == "object.exists")
        .count();
    assert!((1..=8).contains(&slow), "{slow} slow calls answered");
}

/// Behaviour: none (a unit check of the budget's grace; `core/budget` pins the budget's
/// behaviour).
#[test]
fn an_overrun_a_task_catches_is_noted_by_it_and_still_counted() {
    let mut host = Recorder::default();
    let mut vm = vm();
    let err = vm
        .load(
            &mut host,
            "main.lua",
            "task.spawn(function()\n\
             local ok, why = pcall(function() while true do end end)\n\
             print('caught', ok, string.find(tostring(why), 'time budget') ~= nil)\n\
             end)",
        )
        .expect_err("still an overrun");
    assert_eq!(err.kind, ScriptErrorKind::Budget);
    assert_eq!(host.console, ["caught\tfalse\ttrue"]);
}

/// Behaviour: none (a unit check of the budget's measure; `core/budget` pins the budget's
/// behaviour).
#[test]
fn a_short_entry_the_system_stalls_is_not_over_its_budget() {
    let mut host = Recorder::default();
    let mut vm = LuauEngine
        .instantiate(Limits {
            memory_bytes: 8 << 20,
            call_budget_ms: 2.0,
            profile: false,
        })
        .expect("vm");
    // Ten milliseconds of wall time pass inside a short entry: well past a 2 ms budget, and still
    // not an overrun, because the entry did almost no work.
    vm.load(
        &mut host,
        "stalled.lua",
        "local n = 0 for i = 1, 100 do n = n + i end local _ = game.world:get(0xDEAD) print('done', n)",
    )
    .expect("a stalled short entry completes");
    assert_eq!(host.console, ["done\t5050"]);
}

#[test]
fn allocating_past_the_memory_cap_fails_as_out_of_memory() {
    let mut host = Recorder::default();
    let mut vm = vm();
    let err = vm
        .load(
            &mut host,
            "main.lua",
            "local t = {} for i = 1, 1e8 do t[i] = string.rep('x', 64) .. i end",
        )
        .expect_err("capped");
    assert_eq!(err.kind, ScriptErrorKind::Memory);
    vm.load(&mut host, "after.lua", "print(1 + 1)")
        .expect("usable");
}

#[test]
fn the_sandbox_keeps_libraries_read_only_and_plugins_apart() {
    let mut host = Recorder::default();
    let mut a = vm();
    let mut b = vm();
    assert!(a.load(&mut host, "a.lua", "string.upper = nil").is_err());
    assert!(a.load(&mut host, "a.lua", "game.player = 1").is_err());
    a.load(&mut host, "a.lua", "shared = 'from a'")
        .expect("own global");
    b.load(
        &mut host,
        "b.lua",
        "print(shared, loadstring, io, os.execute)",
    )
    .expect("load");
    assert_eq!(host.console, ["nil\tnil\tnil\tnil"]);
}

#[test]
fn require_loads_only_modules_from_the_plugins_own_package_once() {
    let mut host = Recorder::default();
    host.modules.insert(
        "lib.util".into(),
        "print('loading') return { twice = function(x) return x * 2 end }".into(),
    );
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "local u = require('lib.util') local v = require('lib.util') print(u.twice(21), u == v)",
    )
    .expect("load");
    assert_eq!(host.console, ["loading", "42\ttrue"]);
    let err = vm
        .load(&mut host, "main.lua", "require('socket')")
        .expect_err("missing");
    assert!(
        err.message.contains("not in this plugin's package"),
        "{}",
        err.message
    );
}

#[test]
fn timers_run_their_function_and_wake_a_sleeping_task() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "task.after(100, function() print('after') end)\n\
         task.spawn(function() print('before sleep') task.sleep(50) print('woke') end)",
    )
    .expect("load");
    assert_eq!(host.console, ["before sleep"]);
    // Timer 2 is the sleep; timer 1 the `after`.
    vm.deliver(
        &mut host,
        &Event::new("task.timer", Value::map().with("id", 2_i64)),
    )
    .expect("wake");
    vm.deliver(
        &mut host,
        &Event::new("task.timer", Value::map().with("id", 1_i64)),
    )
    .expect("after");
    vm.deliver(
        &mut host,
        &Event::new("task.timer", Value::map().with("id", 1_i64)),
    )
    .expect("once only");
    assert_eq!(host.console, ["before sleep", "woke", "after"]);
}

#[test]
fn values_cross_to_the_host_as_lists_and_maps_and_come_back() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "store.character.log = { 'a', 'b', { n = 3, ok = true } }\n\
         local back = store.character.log\n\
         print(#back, back[3].n, back[3].ok, math.type == nil)",
    )
    .expect("load");
    assert_eq!(
        host.store.get("log"),
        Some(&Value::List(vec![
            Value::from("a"),
            Value::from("b"),
            Value::map().with("n", 3_i64).with("ok", true)
        ]))
    );
    assert_eq!(host.console, ["3\t3\ttrue\ttrue"]);
    assert!(vm
        .load(&mut host, "bad.lua", "store.character.f = print")
        .is_err());
}

#[test]
fn game_reads_and_the_clock_come_from_the_host() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "print(game.player.name) print(os.clock())",
    )
    .expect("load");
    assert_eq!(host.console, ["Tester", "12.5"]);
}

#[test]
fn a_host_call_that_fails_is_a_lua_error_with_its_reason() {
    let mut host = Recorder::default();
    let mut vm = vm();
    let err = vm
        .load(
            &mut host,
            "main.lua",
            "commands.add('x', 'help', function() end)",
        )
        .expect_err("refused");
    assert_eq!(err.kind, ScriptErrorKind::Runtime);
    assert!(
        err.message.contains("commands.add is not available here"),
        "{}",
        err.message
    );
}

#[test]
fn unload_hands_the_plugins_state_to_the_next_loads_reload_hook() {
    let mut host = Recorder::default();
    let mut old = vm();
    old.load(
        &mut host,
        "main.lua",
        "count = 41 function on_unload() return { count = count + 1 } end",
    )
    .expect("load");
    let state = old
        .deliver(&mut host, &Event::new("plugin.unload", Value::Nil))
        .expect("unload");
    assert_eq!(state, Value::map().with("count", 42_i64));
    let mut new = vm();
    new.load(
        &mut host,
        "main.lua",
        "function on_reload(s) print('resumed at', s.count) end",
    )
    .expect("load");
    new.deliver(&mut host, &Event::new("plugin.reload", state))
        .expect("reload");
    assert_eq!(host.console, ["resumed at\t42"]);
}

/// Behaviour: none (a tool for plugin authors: the profile's own bookkeeping).
#[test]
fn a_profile_counts_entries_host_calls_and_the_lines_that_ran() {
    let mut host = Recorder::default();
    let mut vm = super::LuauInstance::new(Limits {
        profile: true,
        call_budget_ms: 1000.0,
        ..Limits::default()
    })
    .expect("vm");
    vm.load(
        &mut host,
        "busy.lua",
        "local n = 0\nfor i = 1, 200000 do n = n + i % 7 end\nlocal _ = game.world:get(0xDEAD)\nprint(n)",
    )
    .expect("runs");
    let profile = vm.profile.as_ref().expect("profiling");
    let table = profile.table(web_time::Duration::from_secs(1), 0);
    assert!(table.contains("load"), "{table}");
    assert!(table.contains("object.exists"), "{table}");
    assert!(table.contains("busy.lua:2"), "{table}");
}

/// Behaviour: none (a unit check of the bindings' cost: the game is the frame's for an entry).
#[test]
fn a_player_field_read_twice_in_an_entry_is_asked_of_the_host_once() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        "on('look', function() print(game.player.name, game.player.name, game.player.level, game.player.level) end)",
    )
    .expect("load");
    // Nothing of the game is read when the code is loaded: a path such as `game.player.name` is
    // read where it runs, each time (a compiler may otherwise read it once, at load, and keep it).
    assert!(
        host.calls.iter().all(|(op, _)| op != "player.get"),
        "{:?}",
        host.calls
    );
    vm.deliver(&mut host, &Event::new("look", Value::map()))
        .expect("delivered");
    vm.deliver(&mut host, &Event::new("look", Value::map()))
        .expect("delivered");
    let asked = |key: &str| {
        host.calls
            .iter()
            .filter(|(op, args)| {
                op == "player.get" && args.first().and_then(Value::as_str) == Some(key)
            })
            .count()
    };
    // Once an entry each: a field the host answers nil for is not asked again either.
    assert_eq!((asked("name"), asked("level")), (2, 2), "{:?}", host.calls);
    assert_eq!(
        host.console,
        ["Tester\tTester\tnil\tnil", "Tester\tTester\tnil\tnil"]
    );
}

/// Behaviour: none (a unit check of the copy into the host: a value's size is counted as it is
/// copied, not after).
#[test]
fn a_value_that_shares_its_parts_is_counted_for_each_and_one_too_large_is_refused() {
    let mut host = Recorder::default();
    let mut vm = vm();
    vm.load(
        &mut host,
        "main.lua",
        r#"
        local t = "x"
        for i = 1, 19 do t = { t, t } end
        local ok, err = pcall(function() store.global.k = t end)
        print("shared parts", ok, string.find(tostring(err), "too large", 1, true) ~= nil)
        local c = {}
        c.me = c
        ok, err = pcall(function() store.global.k = c end)
        print("itself", ok, string.find(tostring(err), "holds itself", 1, true) ~= nil)
        local s = { 1, 2, 3 }
        print("twice", (pcall(function() store.global.k = { a = s, b = s } end)))
        print(string.rep("y", 1048576))
        "#,
    )
    .expect("load");
    assert_eq!(
        host.console[..3],
        [
            "shared parts\tfalse\ttrue",
            "itself\tfalse\ttrue",
            "twice\ttrue"
        ]
    );
    assert_eq!(
        host.store.get("k"),
        Some(
            &Value::map()
                .with(
                    "a",
                    Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
                )
                .with(
                    "b",
                    Value::List(vec![Value::Int(1), Value::Int(2), Value::Int(3)])
                )
        )
    );
    assert!(
        host.console[3].len() <= dereth_plugin_api::engine::CONSOLE_LINE_BYTES,
        "a console line is cut: {} bytes",
        host.console[3].len()
    );
}
