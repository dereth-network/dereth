//! The top-level exception filter the client installs at start-up.
//!
//! # What retail does, and why this build needs it
//!
//! **The shipped retail client installs no exception filter at all.** It carries one, installed
//! only when debug flag `0x0200` is set, and the very first thing it does at start-up clears that
//! flag, so a fault in retail ends the process with whatever the operating system makes of it and
//! no report from the client. This filter is therefore a development affordance of this client
//! (recorded as a tooling row, not as a divergence a player meets), modelled on the dormant one.
//!
//! The dormant filter installs once — only while no previous filter has been saved yet —
//! keeping the filter `SetUnhandledExceptionFilter` hands back. The filter then, in order:
//!
//! 1. clears the floating-point status (`_clearfp`);
//! 2. passes `0x80000003` straight to the previous filter;
//! 3. if it is already handling an exception, runs the crash cleaners and chains (re-entry guard);
//! 4. marks itself as handling an exception;
//! 5. runs a `0xC00000FD` on a backup stack, when one is available;
//! 6. otherwise builds a report context from the exception pointers and **reports the exception**.
//!
//! The load-bearing part is the last step: with the filter installed, a fault is **reported**; it
//! does not vanish. This build had no filter at all, so an access violation in a test binary produced exit
//! `0xC0000005`, no libtest `test result:` line, and nothing else — a crash that reads as silence,
//! which is this project's most-repeated failure mode. Two suites sat in a merge record as "no
//! result" on exactly that basis.
//!
//! # What is transcribed and what is not
//!
//! Transcribed: install-once (only while no previous filter is saved), saving and chaining to the
//! previous filter, the re-entry guard, passing `0x80000003` (a debug break with no debugger
//! attached) straight through untouched, and treating `0xC00000FD` specially because the
//! faulting thread has no stack left to walk.
//!
//! Not transcribed: the exception reporter's Watson upload, the crash-dump worker thread, the
//! crash-cleaner pass, the 4 MiB emergency-memory probe and the backup stack. Those serve a shipped
//! consumer product with a crash-reporting server behind it. What this build needs from the same
//! seam is the fault named on stderr.
//!
//! # This never swallows a crash
//!
//! The filter always returns `EXCEPTION_CONTINUE_SEARCH`, so the process still dies and the exit
//! code is still the exception code. A named crash is worth far more than a quiet suite; a
//! *swallowed* one is worth less than either, because it converts a fault into a wrong answer.

#[cfg(windows)]
pub use win32::install_exception_filter;

/// The portable build has no `SetUnhandledExceptionFilter`; a fault is whatever the OS makes of
/// it (a core dump, a signal report from the shell), which is already a name rather than silence.
#[cfg(not(windows))]
pub fn install_exception_filter() {}

#[cfg(windows)]
mod win32 {
    use std::sync::atomic::{AtomicBool, AtomicPtr, Ordering};

    use windows::Win32::Foundation::{HMODULE, MAX_PATH};
    use windows::Win32::System::Diagnostics::Debug::{
        SetUnhandledExceptionFilter, EXCEPTION_POINTERS, LPTOP_LEVEL_EXCEPTION_FILTER,
    };
    use windows::Win32::System::LibraryLoader::{
        GetModuleFileNameW, GetModuleHandleExW, GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS,
        GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
    };

    /// Retail's saved previous filter: the filter that was installed before ours, chained to rather than
    /// dropped. Null when there was none, which is the usual case for a test binary.
    static OLD_FILTER: AtomicPtr<core::ffi::c_void> = AtomicPtr::new(std::ptr::null_mut());

    /// Retail's install-once test (no previous filter saved yet), kept separately because a *successfully*
    /// installed filter can legitimately have recorded a null predecessor.
    static INSTALLED: AtomicBool = AtomicBool::new(false);

    /// Retail's re-entry flag. A fault raised *inside* the filter must not recurse into
    /// it; retail chains to the old filter instead, and so does this.
    static IN_HANDLER: AtomicBool = AtomicBool::new(false);

    /// The exception codes worth naming rather than printing as a bare hex word. `0xC0000005` arrives
    /// through a shell as `-1073741819`, which reads as noise and was skipped over more than once.
    fn code_name(code: u32) -> &'static str {
        match code {
            0xC000_0005 => "ACCESS_VIOLATION",
            0xC000_001D => "ILLEGAL_INSTRUCTION",
            0xC000_0025 => "NONCONTINUABLE_EXCEPTION",
            0xC000_008C => "ARRAY_BOUNDS_EXCEEDED",
            0xC000_0090 => "FLT_INVALID_OPERATION",
            0xC000_0094 => "INT_DIVIDE_BY_ZERO",
            0xC000_0096 => "PRIV_INSTRUCTION",
            0xC000_00FD => "STACK_OVERFLOW",
            0xC000_0374 => "HEAP_CORRUPTION",
            0xC000_0409 => "STACK_BUFFER_OVERRUN",
            0x8000_0003 => "BREAKPOINT",
            0x8000_0004 => "SINGLE_STEP",
            _ => "unknown",
        }
    }

    /// The module an address falls in, and its offset within that module.
    ///
    /// A symbolised backtrace is the better answer and is printed too, but it depends on a readable
    /// PDB and on `dbghelp` being in a usable state, neither of which is guaranteed on a thread that
    /// has just faulted. `module+RVA` needs only the loader, and can be resolved offline afterwards.
    fn module_of(address: usize) -> Option<(String, usize)> {
        let mut module = HMODULE::default();
        // SAFETY: `GetModuleHandleExW` with FROM_ADDRESS takes the address as an opaque LPCWSTR and
        // does not dereference it; UNCHANGED_REFCOUNT means the returned handle needs no FreeLibrary.
        // A failure leaves `module` as the default and is reported as "no module".
        let got = unsafe {
            GetModuleHandleExW(
                GET_MODULE_HANDLE_EX_FLAG_FROM_ADDRESS
                    | GET_MODULE_HANDLE_EX_FLAG_UNCHANGED_REFCOUNT,
                windows::core::PCWSTR(address as *const u16),
                &mut module,
            )
        };
        if got.is_err() || module.is_invalid() {
            return None;
        }
        let mut buf = [0u16; MAX_PATH as usize];
        // SAFETY: `buf` is a live MAX_PATH-element buffer and the handle is valid; the call writes at
        // most `buf.len()` UTF-16 units and returns how many it wrote.
        let n = unsafe { GetModuleFileNameW(Some(module), &mut buf) } as usize;
        if n == 0 {
            return None;
        }
        let path = String::from_utf16_lossy(&buf[..n.min(buf.len())]);
        let name = path.rsplit(['\\', '/']).next().unwrap_or(&path).to_string();
        Some((name, address.wrapping_sub(module.0 as usize)))
    }

    /// The filter itself, as the operating system calls it.
    ///
    /// # Safety
    /// Called by the operating system on the faulting thread with a valid `EXCEPTION_POINTERS`.
    unsafe extern "system" fn filter(info: *const EXCEPTION_POINTERS) -> i32 {
        // `EXCEPTION_CONTINUE_SEARCH`. Retail's filter reports and then lets the process die; so does
        // this one, so the exit code the caller sees is still the exception code.
        const CONTINUE_SEARCH: i32 = 0;

        if info.is_null() {
            return CONTINUE_SEARCH;
        }
        // SAFETY: non-null, and the OS guarantees `ExceptionRecord` points at a live record for the
        // duration of the filter call.
        let record = unsafe { (*info).ExceptionRecord };
        if record.is_null() {
            return CONTINUE_SEARCH;
        }
        // SAFETY: as above.
        let (code, address, params, n_params) = unsafe {
            (
                (*record).ExceptionCode.0 as u32,
                (*record).ExceptionAddress as usize,
                (*record).ExceptionInformation,
                (*record).NumberParameters as usize,
            )
        };

        // Retail's first arm: a breakpoint with no debugger attached is not the debug layer's business.
        if code == 0x8000_0003 {
            return CONTINUE_SEARCH;
        }
        // Retail's re-entry flag: a fault inside the filter chains out rather than
        // recursing. `swap` makes the test and the set one operation.
        if IN_HANDLER.swap(true, Ordering::SeqCst) {
            return CONTINUE_SEARCH;
        }

        let mut out = String::with_capacity(1024);
        out.push_str("\n=== dere: unhandled exception ===\n");
        out.push_str(&format!(
            "  code      0x{code:08X}  {}\n  at        0x{address:016X}\n",
            code_name(code)
        ));
        if let Some((module, rva)) = module_of(address) {
            out.push_str(&format!("  module    {module}+0x{rva:X}\n"));
        } else {
            out.push_str("  module    <not in any loaded module>\n");
        }
        // For an access violation the record carries what kind of access it was and which address was
        // touched, which is usually the whole diagnosis: a small value is a null-deref through an
        // offset, a recycled-looking pointer is a use-after-free.
        if code == 0xC000_0005 && n_params >= 2 {
            let kind = match params[0] {
                0 => "read",
                1 => "write",
                8 => "execute (DEP)",
                _ => "?",
            };
            out.push_str(&format!("  access    {kind} of 0x{:016X}\n", params[1]));
        }
        out.push_str(&format!("  thread    {:?}\n", std::thread::current().id()));
        if let Some(name) = std::thread::current().name() {
            out.push_str(&format!("  test      {name}\n"));
        }

        // Retail sends `0xC00000FD` to an alternate stack because the faulting thread has none left.
        // This build has no backup stack, so it declines to walk one instead of faulting again inside
        // the filter -- the guard above would then swallow the report entirely.
        if code == 0xC000_00FD {
            out.push_str(
                "  backtrace declined: the stack is exhausted (retail runs this arm on a\n",
            );
            out.push_str("             backup stack; this build has none)\n");
        } else {
            out.push_str("  backtrace (the faulting thread, innermost first):\n");
            out.push_str(&format!("{}\n", std::backtrace::Backtrace::force_capture()));
        }
        out.push_str("=== the process now dies with the code above ===\n");

        // Write straight to the handle. `eprint!` is fine here, but a locked stderr that some other
        // thread already holds would deadlock a process that is already dying.
        {
            use std::io::Write as _;
            let mut err = std::io::stderr();
            let _ = err.write_all(out.as_bytes());
            let _ = err.flush();
        }

        // Chain to whatever was installed before us, exactly as retail's arms do.
        let old = OLD_FILTER.load(Ordering::SeqCst);
        if !old.is_null() {
            // SAFETY: `old` is whatever `SetUnhandledExceptionFilter` handed back, which is either null
            // or a valid top-level filter with this exact signature.
            let old: unsafe extern "system" fn(*const EXCEPTION_POINTERS) -> i32 =
                unsafe { std::mem::transmute(old) };
            IN_HANDLER.store(false, Ordering::SeqCst);
            // SAFETY: the pointers are the ones the OS handed us and are live for this call.
            return unsafe { old(info) };
        }
        IN_HANDLER.store(false, Ordering::SeqCst);
        CONTINUE_SEARCH
    }

    /// Install the filter. Idempotent, as the client's own "already installed" test makes it: a
    /// process that builds several `App`s installs one filter.
    ///
    /// Safe to call from anywhere, including from a crate that forbids `unsafe`.
    pub fn install_exception_filter() {
        if INSTALLED.swap(true, Ordering::SeqCst) {
            return;
        }
        let f: LPTOP_LEVEL_EXCEPTION_FILTER = Some(filter);
        // SAFETY: `SetUnhandledExceptionFilter` takes a function pointer and returns the previous one;
        // there are no borrowed pointers, and `filter` is a `fn` item that lives for the whole program.
        let old = unsafe { SetUnhandledExceptionFilter(f) };
        OLD_FILTER.store(
            old.map_or(std::ptr::null_mut(), |p| p as *mut core::ffi::c_void),
            Ordering::SeqCst,
        );
    }

    #[cfg(test)]
    mod tests {
        use super::*;

        /// The install is idempotent, which is retail's install-once arm. A process that
        /// builds several `App`s must not chain the filter to itself -- that is an infinite recursion
        /// on the first fault, which would turn a named crash back into silence.
        #[test]
        fn installing_twice_installs_once() {
            install_exception_filter();
            let after_first = OLD_FILTER.load(Ordering::SeqCst);
            install_exception_filter();
            assert_eq!(
            after_first,
            OLD_FILTER.load(Ordering::SeqCst),
            "the second install must not overwrite the saved previous filter with our own filter"
        );
            assert!(INSTALLED.load(Ordering::SeqCst));
        }

        /// Every code the filter prints by name is one a fault in this build has actually produced or
        /// plausibly will; the unknown arm still prints the hex rather than dropping it.
        #[test]
        fn the_access_violation_code_is_named() {
            assert_eq!(code_name(0xC000_0005), "ACCESS_VIOLATION");
            assert_eq!(code_name(0xC000_00FD), "STACK_OVERFLOW");
            assert_eq!(code_name(0x8000_0003), "BREAKPOINT");
            assert_eq!(code_name(0x1234_5678), "unknown");
        }

        /// `module_of` resolves an address inside the test binary itself to that binary and a non-zero
        /// offset. This is the half of the report that survives an unusable PDB.
        #[test]
        fn an_address_in_this_binary_resolves_to_this_binary() {
            let here = the_access_violation_code_is_named as *const () as usize;
            let (name, rva) = module_of(here).expect("this function is inside a loaded module");
            assert!(name.to_ascii_lowercase().ends_with(".exe"), "got {name}");
            assert!(rva > 0, "an RVA into the image is never zero");
        }
    }
}
