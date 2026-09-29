//! A child that faults on purpose is reported (code, access kind, touched address, module) and
//! still dies with the same exception code.
//! Fixture: a child process that raises a controlled exception.

#![cfg(windows)]

const SELF_TEST: &str = "DERETH_TEST_EXCEPTION_FILTER_SELF_TEST";

/// The child half: the one test the re-exec'd child is told to run. A no-op in an ordinary run of
/// this suite, which is what keeps it from taking the parent down with it.
#[test]
fn the_child_faults_on_purpose() {
    crash_if_asked();
}

/// Installs the filter and faults, or returns immediately if the parent did not ask.
fn crash_if_asked() {
    if std::env::var_os(SELF_TEST).is_none() {
        return;
    }
    dereth_render::debug::install_exception_filter();
    // A write through a null pointer: `ExceptionInformation[0] == 1` (write) and `[1] == 0`, which
    // is the exact shape the report claims to decode.
    let p: *mut u32 = std::ptr::null_mut();
    // SAFETY: deliberately unsound. This is the fault under test; the process is expected to die
    // here and the parent asserts that it did.
    unsafe { std::ptr::write_volatile(p, 0xDEAD_BEEF) };
    unreachable!("the null write must not be reachable past the fault");
}

/// Run this binary again with the crash requested, and return `(exit code, stderr)`.
fn run_the_crashing_child() -> (u32, String) {
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let out = std::process::Command::new(exe)
        .env(SELF_TEST, "1")
        // One test, exactly, and the one that faults. `--test-threads=1` keeps the fault on the
        // main thread's child rather than in a libtest worker, so the report the parent reads is
        // the simplest possible one.
        .arg("--exact")
        .arg("presentation::exception_filter::the_child_faults_on_purpose")
        .arg("--test-threads=1")
        .output()
        .expect("the child runs");
    let code = out.status.code().unwrap_or(0) as u32;
    (code, String::from_utf8_lossy(&out.stderr).into_owned())
}

/// Behaviour: presentation.crash.a-fault-is-named-and-still-kills-the-process
/// The whole point: a fault names itself instead of vanishing, **and** is still fatal with the
/// same code. Both halves in one test, because either alone would be satisfied by a build that
/// gets the other wrong.
#[test]
fn a_fault_is_reported_and_still_kills_the_process() {
    let (code, stderr) = run_the_crashing_child();

    assert_eq!(
        code, 0xC000_0005,
        "the child must still die of the access violation; the filter reports, it does not \
         handle. stderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("dere: unhandled exception"),
        "the exception filter must write a report.\
         \nstderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("0xC0000005") && stderr.contains("ACCESS_VIOLATION"),
        "the report must name the code both ways: the hex a shell shows and the name a \
         reader recognises.\nstderr was:\n{stderr}"
    );
}

/// The access-violation arm decodes `ExceptionInformation`. A null *write* must say so: "write of
/// 0x0" is a null-deref through a small offset and points at a different bug from a read of a
/// recycled-looking address, which is the first thing anyone reading a crash wants to know.
#[test]
fn the_report_decodes_the_access_kind_and_the_touched_address() {
    let (_, stderr) = run_the_crashing_child();
    assert!(
        stderr.contains("access    write of 0x0000000000000000"),
        "the write through null was not decoded from ExceptionInformation.\nstderr was:\n{stderr}"
    );
}

/// The half of the report that survives an unusable PDB. `module+RVA` needs only the loader, and
/// resolves offline afterwards; a symbolised backtrace needs `dbghelp` to be in a usable state on
/// a thread that has just faulted, which is not guaranteed.
#[test]
fn the_report_locates_the_fault_in_a_module() {
    let (_, stderr) = run_the_crashing_child();
    let exe = std::env::current_exe().expect("the test binary knows its own path");
    let me = exe
        .file_stem()
        .expect("the binary has a file name")
        .to_string_lossy()
        .into_owned();
    assert!(
        stderr.contains(&format!("module    {me}")),
        "the faulting address was not resolved to this test binary ({me}).\nstderr was:\n{stderr}"
    );
    assert!(
        stderr.contains("backtrace (the faulting thread"),
        "no backtrace was attempted.\nstderr was:\n{stderr}"
    );
}
