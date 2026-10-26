//! A plugin's window laid out as text by the headless client: `ui layout` prints each open window's
//! nodes with their places and sizes, each character a cell.
//! Fixture: the shipped retail DAT records, and a plugin package written to a scratch folder.

use dereth_headless::run::Options;
use dereth_headless::{parse, run};

/// Behaviour: plugins.ui.the-layout-as-text-shows-each-nodes-place-and-size
/// A window of a label and a list, opened from the plugin bar and resized by the player to 30
/// cells by 10 lines: the label keeps its line, and the list (the window's last list, growing)
/// takes the rest, its one column as wide as the list less its rim and gaps.
#[test]
fn ui_layout_prints_each_open_windows_nodes_places_and_sizes() {
    dereth_client_sdk::dat::testing::require_dats();
    let dir = std::env::temp_dir().join(format!("dereth-headless-layout-{}", std::process::id()));
    let package = dir.join("layout");
    std::fs::create_dir_all(&package).expect("a scratch folder");
    std::fs::write(
        package.join("plugin.toml"),
        "id = \"test.layout\"\nname = \"Layout\"\nversion = \"1.0.0\"\napi = \"0.7\"\ncontexts = [\"pregame\", \"world\"]\n",
    )
    .expect("the manifest");
    std::fs::write(
        package.join("main.lua"),
        "ui.window{ id = \"main\", title = \"Layout\", resizable = true,\n\
         \tui.label{ id = \"head\", text = \"Log\" },\n\
         \tui.list{ id = \"log\", columns = { \"Line\" }, rows = { { \"one\" } } },\n\
         }\n",
    )
    .expect("the script");
    let opts = Options {
        dat_dir: dereth_client_sdk::dat::testing::dat_dir(),
        plugins: vec![package.clone()],
        ..Options::default()
    };
    let commands = parse(
        "tick 2\nui bar test.layout\nui resize test.layout main 30 10\ntick 1\nui layout\nquit\n",
    )
    .expect("the script parses");
    let mut out: Vec<u8> = Vec::new();
    run(&commands, &opts, &mut out).expect("the run finishes");
    let text = String::from_utf8(out).expect("text");
    let _ = std::fs::remove_dir_all(&dir);
    let laid: Vec<&str> = text
        .lines()
        .skip_while(|l| *l != "window test.layout main")
        .take(4)
        .collect();
    assert_eq!(
        laid,
        [
            "window test.layout main",
            "  window #main 210x160",
            "    label #head 0,0 210x16",
            "    list #log 0,19 210x141 columns=200 rows=6",
        ],
        "{text}"
    );
}
