//! Crafting for plugins: what the character's items make, asked of a world that answers it, and
//! the server's craft question about a plugin's own combine, answered by that plugin alone.

use dereth_plugin_host::Package;
use dereth_protocol::admin::{RecipeAnswerStatus, RecipeMatch};
use dereth_rules::weenie::item_type;
use dereth_testkit::behaviours::note_asserted;
use dereth_testkit::plugins::{PluginRun, ScriptedObject, SCRIPTED_PLAYER};

dereth_testkit::scenarios! {
    scenario_a_plugin_asks_the_world_what_its_items_make_once_a_second_at_most => a_plugin_asks_the_world_what_its_items_make_once_a_second_at_most ["plugins.actions.a-plugin-asks-the-world-what-its-items-make-once-a-second-at-most"],
    scenario_a_craft_question_about_a_plugins_own_combine_is_its_alone_to_answer => a_craft_question_about_a_plugins_own_combine_is_its_alone_to_answer ["plugins.actions.a-craft-question-about-a-plugins-own-combine-is-its-alone-to-answer"],
}

const HEADS: u32 = 0x8000_0F01;
const SHAFTS: u32 = 0x8000_0F02;

/// A plugin that asks what items make (`/ask <ids>`), combines two (`/combine <a> <b>`) and
/// answers its craft questions as told (`/answer yes|no|wait`), printing what it hears.
const CRAFTER: &str = r#"
local answer = nil
commands.add("ask", "", function(args)
	local ids = {}
	for w in string.gmatch(args or "", "%S+") do
		table.insert(ids, tonumber(w))
	end
	task.spawn(function()
		local ok, v = pcall(function()
			return craft.recipes(ids)
		end)
		if not ok then
			print("refused " .. tostring(v))
			return
		end
		local done, r = pawait(v)
		if not done then
			print("failed " .. tostring(r))
			return
		end
		for _, m in ipairs(r) do
			print(string.format("makes %s x%d from %d on %d at %.2f lost %s %s", m.makes.name,
				m.makes.count, m.item, m.target, m.chance, tostring(m.fails.item_lost),
				tostring(m.fails.target_lost)))
		end
		print("answered " .. #r)
	end)
end)
commands.add("combine", "", function(args)
	local a, b = string.match(args, "(%S+) (%S+)")
	task.spawn(function()
		pawait(act.use_on(tonumber(a), tonumber(b)))
	end)
end)
commands.add("answer", "", function(args)
	if args == "yes" then
		answer = true
	elseif args == "no" then
		answer = false
	else
		answer = nil
	end
end)
on("craft.confirm", function(e)
	print(string.format("asked %d %d on %d at %.2f", e.question, e.item, e.target, e.chance or -1))
	if answer ~= nil then
		task.spawn(function()
			local ok, why = pawait(act.confirm(e.question, answer))
			print("confirmed " .. tostring(ok) .. " " .. tostring(why))
		end)
	end
end)
"#;

/// Another plugin, which answers a question it was not told (`/guess <question>`) and prints any
/// it hears.
const OTHER_PLUGIN: &str = r#"
commands.add("guess", "", function(args)
	task.spawn(function()
		local ok, why = pawait(act.confirm(tonumber(args), true))
		print("guessed " .. tostring(ok) .. " " .. tostring(why))
	end)
end)
on("craft.confirm", function(e)
	print("asked " .. e.question)
end)
"#;

fn package(id: &str, main: &str) -> Package {
    let manifest = format!(
        "id = \"{id}\"\nname = \"{id}\"\nversion = \"1.0.0\"\napi = \"0.7\"\n\
         [permissions]\n\"auto.craft\" = \"Crafts.\"\n\"act.interactive\" = \"Commands.\"\n\
         [capabilities]\noptional = [\"craft.recipes\"]\n"
    );
    Package {
        origin: id.into(),
        files: [
            ("plugin.toml".to_owned(), manifest.into_bytes()),
            ("main.lua".to_owned(), main.as_bytes().to_vec()),
        ]
        .into(),
    }
}

/// Something in the pack used on something else there.
fn item(id: u32, name: &str) -> ScriptedObject {
    ScriptedObject {
        id,
        name: name.into(),
        obj_type: item_type::MISC,
        container: Some(SCRIPTED_PLAYER),
        stack: Some(1),
        useability: Some(0x08 | (0x08 << 16)),
        target_type: Some(item_type::MISC),
        ..ScriptedObject::default()
    }
}

/// A scripted world that allows automation, holding a bundle of heads and one of shafts, with
/// `plugins` loaded.
fn world(plugins: &[(&str, &str)]) -> PluginRun {
    let mut run = PluginRun::new();
    run.host().set_world_policy(
        dereth_plugin_host::policy_doc::parse(
            "policy_version = 1\n[automation]\ndefault = \"allow\"\n",
        )
        .expect("a policy"),
    );
    for (id, main) in plugins {
        run.load(package(id, main));
    }
    run.scripted_world(
        "Tester",
        vec![
            item(HEADS, "Bundle of Deadly Arrowheads"),
            item(SHAFTS, "Bundle of Arrowshafts"),
        ],
    );
    run.advance(0.5);
    run
}

fn said(run: &PluginRun, plugin: &str, line: &str) -> bool {
    run.console(plugin).iter().any(|l| l.contains(line))
}

/// Behaviour: plugins.actions.a-plugin-asks-the-world-what-its-items-make-once-a-second-at-most
fn a_plugin_asks_the_world_what_its_items_make_once_a_second_at_most() {
    const P: &str = "test.crafter";
    let mut run = world(&[(P, CRAFTER)]);
    let ask = format!("/ask {HEADS} {SHAFTS}");
    // A world that has not offered recipe answers is asked nothing.
    run.type_line(&ask);
    run.advance(0.5);
    assert!(said(&run, P, "failed no_recipes"), "{:#?}", run.console(P));
    assert!(run.recipe_queries().is_empty());
    // Offered: the query names the items, and the answer comes back as the plugin's.
    run.offer_recipes();
    run.type_line(&ask);
    run.advance(0.25);
    let queries = run.recipe_queries();
    assert_eq!(queries.len(), 1, "{queries:?}");
    assert_eq!(queries[0].1, vec![HEADS, SHAFTS]);
    // Asked again at once: it waits its second.
    run.type_line(&ask);
    run.advance(0.25);
    assert_eq!(run.recipe_queries().len(), 1);
    run.answer_recipes(
        queries[0].0,
        RecipeAnswerStatus::Answered,
        vec![RecipeMatch {
            source: HEADS,
            target: SHAFTS,
            recipe: 7,
            skill: 37,
            difficulty: 150,
            chance: 0.75,
            makes_wcid: 300,
            makes_name: "Deadly Arrow".into(),
            makes_count: 100,
            fails: RecipeMatch::FAIL_SOURCE | RecipeMatch::FAIL_TARGET,
        }],
    );
    run.advance(0.25);
    assert!(
        said(
            &run,
            P,
            &format!("makes Deadly Arrow x100 from {HEADS} on {SHAFTS} at 0.75 lost true true")
        ),
        "{:#?}",
        run.console(P)
    );
    assert!(said(&run, P, "answered 1"));
    // Its second up, the waiting query goes; a world that says too soon fails it.
    run.advance(1.0);
    let queries = run.recipe_queries();
    assert_eq!(queries.len(), 2, "{queries:?}");
    run.answer_recipes(queries[1].0, RecipeAnswerStatus::TooSoon, Vec::new());
    run.advance(0.25);
    assert!(said(&run, P, "failed too_soon"), "{:#?}", run.console(P));
    // An item that is not the character's is refused at the call.
    run.type_line("/ask 12345");
    run.advance(0.25);
    assert!(
        said(&run, P, "is not the character's"),
        "{:#?}",
        run.console(P)
    );
    assert_eq!(run.recipe_queries().len(), 2);
    note_asserted(
        "plugins.actions.a-plugin-asks-the-world-what-its-items-make-once-a-second-at-most",
    );
}

fn sent(run: &PluginRun, what: &str) -> bool {
    run.sent().iter().any(|s| s == what)
}

/// Behaviour: plugins.actions.a-craft-question-about-a-plugins-own-combine-is-its-alone-to-answer
fn a_craft_question_about_a_plugins_own_combine_is_its_alone_to_answer() {
    const P: &str = "test.crafter";
    const OTHER: &str = "test.other";
    let mut run = world(&[(P, CRAFTER), (OTHER, OTHER_PLUGIN)]);
    let chance = "You determine that you have a 38 percent chance to succeed.";
    // The plugin's own combine: its question, with the server's chance, is the plugin's alone,
    // and its answer goes to the server.
    run.type_line("/answer yes");
    run.type_line(&format!("/combine {HEADS} {SHAFTS}"));
    run.advance(0.5);
    assert!(
        sent(&run, &format!("use_with_target {HEADS:#x} {SHAFTS:#x}")),
        "{:?}",
        run.sent()
    );
    run.ask_craft(0x77, chance);
    run.advance(0.5);
    assert!(
        said(&run, P, &format!("asked 119 {HEADS} on {SHAFTS} at 0.38")),
        "{:#?}",
        run.console(P)
    );
    assert!(!said(&run, OTHER, "asked"), "{:#?}", run.console(OTHER));
    assert!(sent(&run, "confirm 5 0x77 yes"), "{:?}", run.sent());
    assert!(said(&run, P, "confirmed true"), "{:#?}", run.console(P));
    // Another plugin may not answer it; unanswered, it is answered No.
    run.type_line("/answer wait");
    run.type_line(&format!("/combine {HEADS} {SHAFTS}"));
    run.advance(0.5);
    run.ask_craft(0x78, chance);
    run.advance(0.5);
    assert!(said(&run, P, "asked 120"), "{:#?}", run.console(P));
    run.type_line("/guess 120");
    run.advance(0.5);
    assert!(
        said(&run, OTHER, "guessed false not_yours"),
        "{:#?}",
        run.console(OTHER)
    );
    assert!(!sent(&run, "confirm 5 0x78 no"));
    run.advance(11.0);
    assert!(sent(&run, "confirm 5 0x78 no"), "{:?}", run.sent());
    // Long after the plugin's combine, a question is the player's: no plugin hears it, and it is
    // left to the player.
    run.advance(31.0);
    run.ask_craft(0x79, chance);
    run.advance(11.0);
    assert!(!said(&run, P, "asked 121"), "{:#?}", run.console(P));
    assert!(!run.sent().iter().any(|s| s.starts_with("confirm 5 0x79")));
    note_asserted(
        "plugins.actions.a-craft-question-about-a-plugins-own-combine-is-its-alone-to-answer",
    );
}
