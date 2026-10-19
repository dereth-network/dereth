//! Behaviour: none (tooling: the world editor)
//! The editor's plain modules are unit-tested where they are; this binary holds what needs more
//! than one of them.

#[test]
fn the_usage_names_the_scripted_mode() {
    assert!(lyceum_editor::args::USAGE.contains("--script"));
}
