//! The desktop's URL launch, as the host performs it.
//!
//! The support-ticket buttons hand a URL to the desktop's registered handler and read back the
//! launch result, whose signed `> 32` test decides whether the error dialog is requested. The
//! launch itself is an operating-system call, so the host installs it here, the way it installs
//! the local-time shift in [`crate::platform::clock`].

/// The host's launch, installed once.
static URI_LAUNCHER: std::sync::OnceLock<fn(&str) -> i32> = std::sync::OnceLock::new();

/// Install the host's URL launch. The first installation stands; later ones are the same function
/// and are ignored.
pub fn install_uri_launcher(f: fn(&str) -> i32) {
    let _ = URI_LAUNCHER.set(f);
}

/// Hand `url` to the desktop and answer the launch result: greater than 32 when the handler was
/// started. `0` before the host has installed its launch, which is the answer the host gives when
/// the launcher refuses the call.
#[must_use]
pub fn launch_uri(url: &str) -> i32 {
    URI_LAUNCHER.get().map_or(0, |f| f(url))
}
