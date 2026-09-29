//! A server build that cannot start: it reports this build's facts (`--release-facts`) as the real
//! server does, and fails whatever else it is asked, before it is ready. The updater's end-to-end
//! tests publish it as a new release to see a failed health check rolled back.

use std::process::ExitCode;

fn main() -> ExitCode {
    if std::env::args().nth(1).as_deref() == Some("--release-facts") {
        println!(
            "{}",
            empyrean_server::update::release::Facts::this_build().to_json()
        );
        return ExitCode::SUCCESS;
    }
    eprintln!("This build cannot start.");
    ExitCode::FAILURE
}
