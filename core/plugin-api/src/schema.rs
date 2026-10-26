//! The plugin API's schema: every host operation a plugin reaches, with its typed parameters and
//! result. The WIT description of the compiled-plugin interface ([`wit`]) and the Luau type
//! definitions of the Lua functions that are these operations one to one ([`luau_definitions`])
//! are both generated from it, so neither can drift from the other or from the host.
//!
//! A Lua plugin reaches the operations through the Lua-level API (its namespaces, promises and
//! libraries, `docs/plugins/api.md`); a compiled plugin imports them as the WIT interface `host`
//! and exports `init` and `deliver`. Values that are tables in Lua (an object's fields, an event's
//! data, a UI tree) cross the WIT boundary as JSON text, the type `value`.

use std::fmt::Write as _;

/// A parameter's or a result's type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Ty {
    Bool,
    /// A 64-bit signed integer.
    Int,
    /// A 64-bit float.
    Num,
    Str,
    /// Any value, as JSON text across the WIT boundary.
    Value,
    /// Absent (Lua `nil`), or the inner type.
    Opt(&'static Ty),
}

impl Ty {
    /// The type as the WIT interface spells it.
    #[must_use]
    pub fn wit(self) -> String {
        wit_ty(self)
    }

    /// The type as the Luau definitions spell it.
    #[must_use]
    pub fn luau(self) -> String {
        luau_ty(self)
    }
}

/// One parameter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Param {
    pub name: &'static str,
    pub ty: Ty,
}

/// One host operation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Op {
    /// The operation's name, `namespace.name`.
    pub name: &'static str,
    /// The Lua function that is this operation one to one, when there is one.
    pub lua: Option<&'static str>,
    pub params: &'static [Param],
    /// `None` when it answers nothing.
    pub result: Option<Ty>,
    /// What it does, for the generated descriptions.
    pub doc: &'static str,
}

const fn p(name: &'static str, ty: Ty) -> Param {
    Param { name, ty }
}

const OPT_STR: Ty = Ty::Opt(&Ty::Str);
const OPT_INT: Ty = Ty::Opt(&Ty::Int);
const ID: Param = p("object", Ty::Int);

/// The WIT package the interface is published as; its version is the API version.
pub const WIT_PACKAGE: &str = "dereth:plugin";

/// The API version of the schema, as the WIT package's version.
pub const WIT_VERSION: &str = "0.7.0";

/// Every host operation, in the order the WIT interface lists them.
#[rustfmt::skip]
pub const OPS: &[Op] = &[
    Op { name: "console.print", lua: None, params: &[p("text", Ty::Str)], result: None,
         doc: "A line for the plugin's own console." },
    Op { name: "events.subscribe", lua: None, params: &[p("event", Ty::Str)], result: None,
         doc: "Hear an event by name from now on (delivered to `deliver`)." },
    Op { name: "events.unsubscribe", lua: None, params: &[p("event", Ty::Str)], result: None,
         doc: "Stop hearing an event." },
    Op { name: "chat.print", lua: Some("chat.print"), params: &[p("text", Ty::Str), p("style", OPT_STR)], result: None,
         doc: "Print a line in the chat window, for the player only; `style` is a chat kind." },
    Op { name: "chat.hide", lua: None, params: &[p("line", Ty::Int)], result: None,
         doc: "Hide an incoming chat line (by its `chat.line` event's `line`); needs `chat.filter`." },
    Op { name: "chat.send", lua: Some("chat.say"), params: &[p("text", Ty::Str)], result: None,
         doc: "Say a line, or type a command, as the player would in the chat bar." },
    Op { name: "commands.add", lua: None, params: &[p("verb", Ty::Str), p("help", Ty::Str)], result: None,
         doc: "Claim a typed `/verb`; the line arrives as a `command` event." },
    Op { name: "commands.remove", lua: Some("commands.remove"), params: &[p("verb", Ty::Str)], result: None,
         doc: "Give up a claimed `/verb`." },
    Op { name: "task.timer", lua: None, params: &[p("ms", Ty::Num), p("repeat", Ty::Bool)], result: Some(Ty::Int),
         doc: "Start a timer; each time it is due a `task.timer` event with its id arrives." },
    Op { name: "task.cancel", lua: Some("task.cancel"), params: &[p("timer", Ty::Int)], result: None,
         doc: "Cancel a timer." },
    Op { name: "task.error", lua: None, params: &[p("message", Ty::Str)], result: None,
         doc: "Report an error the plugin caught, to its console." },
    Op { name: "store.get", lua: None, params: &[p("scope", Ty::Str), p("key", Ty::Str)], result: Some(Ty::Value),
         doc: "A value kept in a store scope (`global`, `world` or `character`)." },
    Op { name: "store.set", lua: None, params: &[p("scope", Ty::Str), p("key", Ty::Str), p("data", Ty::Value)], result: None,
         doc: "Keep a value in a store scope; `null` removes it." },
    Op { name: "player.get", lua: None, params: &[p("field", Ty::Str)], result: Some(Ty::Value),
         doc: "A field of the player's character (`name`, `health`, `xp`, `position`, ...)." },
    Op { name: "player.int", lua: Some("game.player_int"), params: &[p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "The character's own integer property `key`, or null." },
    Op { name: "object.exists", lua: None, params: &[ID], result: Some(Ty::Bool),
         doc: "Whether the client knows the object." },
    Op { name: "object.get", lua: None, params: &[ID, p("field", Ty::Str)], result: Some(Ty::Value),
         doc: "A field of an object (`name`, `class`, `container`, `position`, ...)." },
    Op { name: "object.distance", lua: None, params: &[ID], result: Some(Ty::Value),
         doc: "The object's distance from the character in metres, or null." },
    Op { name: "object.int", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "An integer property of an object, or null." },
    Op { name: "object.float", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "A float property of an object, or null." },
    Op { name: "object.string", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "A string property of an object, or null." },
    Op { name: "object.bool", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "A boolean property of an object, or null." },
    Op { name: "object.int64", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "A 64-bit integer property of an object, or null." },
    Op { name: "object.did", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "A data-id property of an object (a number naming game data), or null." },
    Op { name: "object.iid", lua: None, params: &[ID, p("key", Ty::Int)], result: Some(Ty::Value),
         doc: "An instance-id property of an object (another object's id), or null." },
    Op { name: "object.health", lua: None, params: &[ID], result: Some(Ty::Value),
         doc: "What the client knows of an object's health (`current`, `max`, `fraction`), or null." },
    Op { name: "object.appraisal", lua: None, params: &[ID], result: Some(Ty::Value),
         doc: "The last appraisal of an object, or null." },
    Op { name: "object.properties", lua: None, params: &[ID, p("type", OPT_STR)], result: Some(Ty::Value),
         doc: "Every property the client knows of an object (`ints`, `int64s`, `floats`, `strings`, `bools`, `dids`, `iids`, each by number), or one type's, or null." },
    Op { name: "inventory.items", lua: None, params: &[], result: Some(Ty::Value),
         doc: "The ids of the items the character holds." },
    Op { name: "inventory.open_container", lua: None, params: &[], result: Some(Ty::Value),
         doc: "The container the character has open, or null." },
    Op { name: "inventory.free_slots", lua: None, params: &[], result: Some(Ty::Value),
         doc: "Free slots in the main pack, or null." },
    Op { name: "inventory.packs", lua: None, params: &[], result: Some(Ty::Value),
         doc: "The character's packs in the order the main pack shows them, the main pack first: `id`, `name`, `capacity`, `used` each, or null." },
    Op { name: "vendor.info", lua: Some("vendor.info"), params: &[], result: Some(Ty::Value),
         doc: "The open vendor, or null." },
    Op { name: "trade.info", lua: Some("trade.info"), params: &[], result: Some(Ty::Value),
         doc: "The open trade, or null." },
    Op { name: "magic.spell", lua: Some("magic.spell"), params: &[p("spell", Ty::Int)], result: Some(Ty::Value),
         doc: "A spell of the spell table, or null." },
    Op { name: "magic.known", lua: Some("magic.known"), params: &[p("from", OPT_INT), p("count", OPT_INT)], result: Some(Ty::Value),
         doc: "The character's spell book, in id order: every spell it knows that the spell table has, or `count` of them from the `from`th (from 1)." },
    Op { name: "files.package", lua: Some("files.package"), params: &[p("path", Ty::Str)], result: Some(OPT_STR),
         doc: "A text file of the plugin's own package." },
    Op { name: "files.package_list", lua: Some("files.package_list"), params: &[], result: Some(Ty::Value),
         doc: "Every file of the plugin's own package, by path." },
    Op { name: "act.request", lua: None, params: &[p("action", Ty::Str), p("args", Ty::Value)], result: Some(OPT_INT),
         doc: "Ask for an action (`act.use`, `combat.attack`, `nav.move_to`, `net.fetch`, ...); answers its id, and its end arrives as `action.completed` or `action.failed`." },
    Op { name: "nav.owner", lua: Some("nav.owner"), params: &[], result: Some(OPT_STR),
         doc: "The plugin moving the character, or none." },
    Op { name: "nav.journey", lua: Some("nav.journey"), params: &[], result: Some(Ty::Value),
         doc: "The journey this plugin has under way (its legs, the leg being carried out, where that leg goes, the way still to go, the goal), or null." },
    Op { name: "nav.show_journey", lua: Some("nav.show_journey"), params: &[p("on", Ty::Bool)], result: Some(Ty::Bool),
         doc: "Show or hide the host's drawing of this plugin's journey (the arrow, the way on the map and radar); false when it has none." },
    Op { name: "nav.known_portals", lua: Some("nav.known_portals"), params: &[], result: Some(Ty::Value),
         doc: "The portals this world's store knows: where each stands and sends, how it is known (learned, published, hint), its uses, failures and level range, and why it was retired." },
    Op { name: "nav.recalls", lua: Some("nav.recalls"), params: &[], result: Some(Ty::Value),
         doc: "Where the character's recalls have sent it: each recall (`spell <id>`, `lifestone`, `marketplace`) and its landing." },
    Op { name: "nav.sight", lua: None, params: &[p("target", Ty::Value), p("from", Ty::Opt(&Ty::Value))], result: Some(Ty::Opt(&Ty::Bool)),
         doc: "Whether a straight line from the character (or `from`) to an object or a place is clear of the world's walls, ground and the objects that stop a body, a metre above each end; null where the host cannot tell." },
    Op { name: "meta.parse", lua: None, params: &[p("text", Ty::Str)], result: Some(Ty::Value),
         doc: "A VTank meta file's text, read." },
    Op { name: "files.data", lua: Some("files.data"), params: &[p("path", Ty::Str)], result: Some(OPT_STR),
         doc: "A text file of the plugin's data folder (the player's files for it), or null." },
    Op { name: "files.data_list", lua: Some("files.data_list"), params: &[], result: Some(Ty::Value),
         doc: "Every file in the plugin's data folder, by path." },
    Op { name: "files.save_data", lua: Some("files.save_data"), params: &[p("path", Ty::Str), p("text", Ty::Str)], result: Some(Ty::Bool),
         doc: "Save a text file to the plugin's data folder (at most 4 MiB, 1000 files)." },
    Op { name: "profile.usd", lua: None, params: &[p("text", Ty::Str)], result: Some(Ty::Value),
         doc: "A VTank settings profile's text, read: each table by name, as rows of columns." },
    Op { name: "nav.parse", lua: None, params: &[p("text", Ty::Str)], result: Some(Ty::Value),
         doc: "A VTank route file's text, read." },
    Op { name: "text.match", lua: Some("regex.match"), params: &[p("pattern", Ty::Str), p("text", Ty::Str), p("ignore-case", Ty::Bool)], result: Some(Ty::Value),
         doc: "The first match of a pattern in a text, with its groups, or null." },
    Op { name: "session.info", lua: Some("session.info"), params: &[], result: Some(Ty::Value),
         doc: "The session: its phase, the character list, portal space." },
    Op { name: "peers.send", lua: None, params: &[p("channel", Ty::Str), p("data", Ty::Value)], result: Some(Ty::Value),
         doc: "Send a message to this plugin's copies in the player's other clients." },
    Op { name: "overlay.add", lua: None, params: &[p("kind", Ty::Str), p("marker", Ty::Value)], result: Some(Ty::Value),
         doc: "Add a marker (`world`, `radar`, `map` or `arrow`); answers its id." },
    Op { name: "overlay.remove", lua: None, params: &[p("marker", Ty::Int)], result: Some(Ty::Bool),
         doc: "Remove a marker." },
    Op { name: "overlay.clear", lua: None, params: &[p("kind", OPT_STR)], result: None,
         doc: "Remove every marker of a kind, or all." },
    Op { name: "data.landblock", lua: None, params: &[p("landblock", Ty::Int)], result: Some(Ty::Value),
         doc: "A landblock of the game's data (`0xXXYY`, or a cell within it): its 9 by 9 heights, terrain and roads, and its cells, or null." },
    Op { name: "data.cell", lua: None, params: &[p("cell", Ty::Int)], result: Some(Ty::Value),
         doc: "A dungeon or building cell of the game's data: its place, its portals and the cells seen from it, its floor's outline and its box, or null." },
    Op { name: "retail_ui.set", lua: None, params: &[p("panel", Ty::Str), p("state", Ty::Value)], result: None,
         doc: "Hide or show (`hidden`) and move (`x`, `y`) one of the game's own panels." },
    Op { name: "retail_ui.reset", lua: None, params: &[p("panel", Ty::Str)], result: None,
         doc: "Give one of the game's own panels back as the player had it." },
    Op { name: "services.offer", lua: None, params: &[p("service", Ty::Str), p("allow", Ty::Opt(&Ty::Value))], result: None,
         doc: "Offer a service to the other plugins in this client (only those `allow` lists, a list of plugin ids, when given); its calls arrive as `service.call` events." },
    Op { name: "services.withdraw", lua: None, params: &[p("service", Ty::Str)], result: None,
         doc: "Stop offering a service." },
    Op { name: "services.reply", lua: None, params: &[p("call", Ty::Int), p("data", Ty::Value), p("error", OPT_STR)], result: Some(Ty::Bool),
         doc: "Answer a `service.call`: its value, or the error it failed with." },
    Op { name: "services.list", lua: Some("services.list"), params: &[], result: Some(Ty::Value),
         doc: "The services this plugin may call: `{ plugin, service }` each." },
    Op { name: "policy.info", lua: Some("policy.info"), params: &[], result: Some(Ty::Value),
         doc: "The world's plugin policy." },
    Op { name: "loot.load", lua: None, params: &[p("text", Ty::Str)], result: Some(Ty::Int),
         doc: "Read a VTank loot profile; answers its handle." },
    Op { name: "loot.rules", lua: None, params: &[p("profile", Ty::Int)], result: Some(Ty::Value),
         doc: "A loaded loot profile's rules." },
    Op { name: "loot.classify", lua: None, params: &[p("profile", Ty::Int), ID], result: Some(Ty::Value),
         doc: "The rule an item meets first, or null." },
    Op { name: "loot.needs_appraisal", lua: None, params: &[p("profile", Ty::Int), ID], result: Some(Ty::Bool),
         doc: "Whether an item must be appraised before it can be classified." },
    Op { name: "loot.salvage_combine", lua: None, params: &[p("profile", Ty::Int)], result: Some(Ty::Value),
         doc: "A loaded loot profile's salvage combining: the workmanship ranges by material, or null." },
    Op { name: "permissions.activation", lua: Some("permissions.activation"), params: &[], result: Some(Ty::Num),
         doc: "Seconds left of the activation the player's last gesture gave." },
    Op { name: "permissions.has", lua: Some("permissions.has"), params: &[p("permission", Ty::Str)], result: Some(Ty::Bool),
         doc: "Whether the plugin may use what a permission covers: every one but net.http, which its manifest must name." },
    Op { name: "world.find", lua: None, params: &[p("filter", Ty::Value)], result: Some(Ty::Value),
         doc: "The ids of the objects a filter matches (by name, class, type, container, wielder, distance and property conditions)." },
    Op { name: "world.selection", lua: None, params: &[], result: Some(Ty::Value),
         doc: "The selected object, or null." },
    Op { name: "ui.window", lua: None, params: &[p("tree", Ty::Value)], result: None,
         doc: "Declare (or replace) a window from its UI tree; a new one opens only while the plugin answers the player." },
    Op { name: "ui.hud", lua: None, params: &[p("tree", Ty::Value)], result: None,
         doc: "Declare (or replace) a HUD from its UI tree; a new one opens only while the plugin answers the player." },
    Op { name: "ui.get", lua: None, params: &[p("window", Ty::Str), p("node", Ty::Str), p("property", Ty::Str)], result: Some(Ty::Value),
         doc: "A property of a node of a declared window." },
    Op { name: "ui.set", lua: None, params: &[p("window", Ty::Str), p("node", Ty::Str), p("property", Ty::Str), p("data", Ty::Value)], result: None,
         doc: "Set a property of a node of a declared window." },
    Op { name: "ui.close", lua: None, params: &[p("window", Ty::Str)], result: None,
         doc: "Close a declared window." },
    Op { name: "ui.show", lua: None, params: &[p("window", Ty::Str)], result: None,
         doc: "Show a declared window again, while the plugin answers the player; otherwise it stays closed." },
    Op { name: "ui.toast", lua: Some("ui.toast"), params: &[p("text", Ty::Str), p("seconds", Ty::Opt(&Ty::Num))], result: None,
         doc: "Show a line briefly at the top of the screen with the plugin's name (1 to 30 seconds, 5 when left out; three at once a plugin)." },
    Op { name: "ui.status", lua: Some("ui.status"), params: &[p("status", Ty::Str)], result: None,
         doc: "What the plugin bar shows the plugin doing: running, paused or idle." },
    Op { name: "host.info", lua: None, params: &[], result: Some(Ty::Value),
         doc: "The host's name and API version." },
    Op { name: "host.has", lua: None, params: &[p("capability", Ty::Str)], result: Some(Ty::Bool),
         doc: "Whether the host offers a capability." },
    Op { name: "time.clock", lua: None, params: &[], result: Some(Ty::Num),
         doc: "Seconds since the plugin loaded, on the frame clock." },
    Op { name: "time.unix", lua: None, params: &[], result: Some(Ty::Int),
         doc: "The game's time, as Unix seconds." },
];

impl Op {
    /// The operation's WIT function name: `chat.print` is `chat-print`.
    #[must_use]
    pub fn wit_name(&self) -> String {
        self.name.replace(['.', '_'], "-")
    }
}

/// The operation a WIT function name stands for.
#[must_use]
pub fn by_wit_name(name: &str) -> Option<&'static Op> {
    OPS.iter().find(|o| o.wit_name() == name)
}

/// The words of WIT itself: a name spelled as one is written with a `%` before it.
const WIT_KEYWORDS: &[&str] = &[
    "as",
    "async",
    "bool",
    "borrow",
    "char",
    "constructor",
    "enum",
    "export",
    "f32",
    "f64",
    "flags",
    "from",
    "func",
    "future",
    "import",
    "include",
    "interface",
    "list",
    "option",
    "own",
    "package",
    "record",
    "resource",
    "result",
    "s16",
    "s32",
    "s64",
    "s8",
    "static",
    "stream",
    "string",
    "tuple",
    "type",
    "u16",
    "u32",
    "u64",
    "u8",
    "use",
    "variant",
    "with",
    "world",
];

/// `name` as a WIT identifier: as it is, or with a `%` before it where it is one of WIT's words.
fn wit_ident(name: &str) -> String {
    if WIT_KEYWORDS.contains(&name) {
        format!("%{name}")
    } else {
        name.to_owned()
    }
}

fn wit_ty(t: Ty) -> String {
    match t {
        Ty::Bool => "bool".into(),
        Ty::Int => "s64".into(),
        Ty::Num => "f64".into(),
        Ty::Str => "string".into(),
        Ty::Value => "value".into(),
        Ty::Opt(inner) => format!("option<{}>", wit_ty(*inner)),
    }
}

fn luau_ty(t: Ty) -> String {
    match t {
        Ty::Bool => "boolean".into(),
        Ty::Int | Ty::Num => "number".into(),
        Ty::Str => "string".into(),
        Ty::Value => "any".into(),
        Ty::Opt(inner) => format!("{}?", luau_ty(*inner)),
    }
}

/// The WIT description of the compiled-plugin interface: the package, the `host` interface (one
/// function per operation, each answering `result<T, string>`, the error being what a Lua plugin
/// would see raised) and the world `plugin` a plugin's component targets.
#[must_use]
pub fn wit() -> String {
    let mut s = String::new();
    let _ = writeln!(
        s,
        "// The Dereth plugin interface for compiled plugins (WebAssembly components).\n\
         // Generated from the plugin API's schema; the Lua definitions come from the same schema.\n\
         // Do not edit: regenerate it (`docs/plugins/wasm.md` says how).\n\n\
         package {WIT_PACKAGE}@{WIT_VERSION};\n\n\
         /// The host: every operation a plugin reaches. An operation that fails (a missing\n\
         /// permission, wrong arguments) answers `err` with the reason.\n\
         interface host {{\n    \
         /// Any value, as JSON text: a table, an array, a string, a number, a boolean or null.\n    \
         type value = string;"
    );
    for op in OPS {
        let params: Vec<String> = op
            .params
            .iter()
            .map(|p| format!("{}: {}", wit_ident(p.name), wit_ty(p.ty)))
            .collect();
        let result = op.result.map_or_else(|| "_".to_owned(), wit_ty);
        let _ = writeln!(
            s,
            "\n    /// {}\n    {}: func({}) -> result<{}, string>;",
            op.doc,
            op.wit_name(),
            params.join(", "),
            result
        );
    }
    let _ = writeln!(
        s,
        "}}\n\n\
         /// A compiled plugin.\n\
         world plugin {{\n    \
         import host;\n\n    \
         /// The plugin's start: what a Lua plugin's entry file does at its top level (subscribe to\n    \
         /// events, claim commands, declare windows).\n    \
         export init: func() -> result<_, string>;\n\n    \
         /// One event: its name and its data (JSON). Answers a value (JSON; `null` for none), which\n    \
         /// for `plugin.unload` is the state handed to the next load.\n    \
         export deliver: func(event: string, data: string) -> result<string, string>;\n\
         }}"
    );
    s
}

/// The Luau type definitions (for a Luau language server) of the Lua functions that are host
/// operations one to one, grouped by namespace. The rest of the Lua API (events, promises,
/// handles, the libraries) is Lua itself and described in `docs/plugins/api.md`.
#[must_use]
pub fn luau_definitions() -> String {
    let mut by_ns: std::collections::BTreeMap<&str, Vec<(&str, &Op)>> = Default::default();
    for op in OPS {
        if let Some((ns, name)) = op.lua.and_then(|l| l.split_once('.')) {
            by_ns.entry(ns).or_default().push((name, op));
        }
    }
    let mut s = String::from(
        "-- Luau definitions of the Dereth plugin API's functions that are host operations one to\n\
         -- one. Generated from the plugin API's schema, the same schema as the WIT interface.\n\
         -- Do not edit: regenerate it (`docs/plugins/wasm.md` says how).\n",
    );
    for (ns, fns) in by_ns {
        let _ = writeln!(s, "\ndeclare {ns}: {{");
        for (name, op) in fns {
            let params: Vec<String> = op
                .params
                .iter()
                .map(|p| format!("{}: {}", p.name.replace('-', "_"), luau_ty(p.ty)))
                .collect();
            let result = op.result.map_or_else(|| "()".to_owned(), luau_ty);
            let _ = writeln!(
                s,
                "\t-- {}\n\t{name}: ({}) -> {result},",
                op.doc,
                params.join(", ")
            );
        }
        s.push_str("}\n");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::{by_wit_name, wit, OPS};

    #[test]
    fn every_operation_has_a_distinct_wit_name_and_few_enough_flat_parameters() {
        let mut seen = std::collections::BTreeSet::new();
        for op in OPS {
            assert!(seen.insert(op.wit_name()), "{} twice", op.name);
            assert_eq!(by_wit_name(&op.wit_name()).map(|o| o.name), Some(op.name));
            // Each parameter flattens to at most three core values; sixteen is the most an import
            // takes directly.
            assert!(
                op.params.len() * 3 < 16,
                "{} has too many parameters",
                op.name
            );
        }
        assert!(wit().contains(
            "chat-print: func(text: string, style: option<string>) -> result<_, string>;"
        ));
        assert!(
            wit().contains("magic-known: func(%from: option<s64>, count: option<s64>)"),
            "a parameter named as one of WIT's words is written with a %"
        );
    }
}
