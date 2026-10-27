//! The plugin template the author's tool writes (`dereth-plugin new`, from
//! `tools/plugin/template`): its plugin, filled in as the tool fills it, loads in this client's
//! host in a scripted world, says hello in its window, and answers its command.
//!
//! Behaviour: none (the author's template, checked so that a new plugin starts from one that runs)

use dereth_plugin_api::ApiVersion;
use dereth_plugin_host::Package;
use dereth_testkit::plugins::PluginRun;

const ID: &str = "example.hello";

/// The template's plugin folder, its placeholders filled as `dereth-plugin new` fills them for a
/// plugin named Hello.
fn template_plugin() -> Package {
    let dir = dereth_testkit::conformance::workspace_root().join("tools/plugin/template");
    let filled = |file: &str| {
        std::fs::read_to_string(dir.join(file))
            .unwrap_or_else(|e| panic!("{file}: {e}"))
            .replace("{{id}}", ID)
            .replace("{{name}}", "Hello")
            .replace("{{api}}", &ApiVersion::CURRENT.to_string())
            .replace("{{command}}", "hello")
            .replace("{{homepage}}", "")
            .into_bytes()
    };
    Package {
        origin: "hello".into(),
        files: ["plugin.toml", "main.lua", "CHANGES.md"]
            .into_iter()
            .map(|f| (f.to_owned(), filled(f)))
            .collect(),
    }
}

#[test]
fn the_template_plugin_loads_says_hello_in_its_window_and_answers_its_command() {
    let mut run = PluginRun::new();
    run.load(template_plugin());
    run.scripted_world("Tester", Vec::new());
    run.advance(0.5);
    assert_eq!(run.status(ID), "running", "{:#?}", run.console(ID));
    assert!(run.ui_dump().contains("Hello!"), "{}", run.ui_dump());
    run.click(ID, "main.wave");
    assert!(run.ui_dump().contains("Hello again!"), "{}", run.ui_dump());
    run.type_line("/hello");
    assert!(
        run.printed()
            .iter()
            .any(|l| l.contains("Hello from Hello.")),
        "{:?}",
        run.printed()
    );
}
