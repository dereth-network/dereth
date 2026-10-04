//! The desktop host: what the client shell is given on a desktop -- the `winit` window and the
//! system clock, the operating system's time zone and URL launch, the `cpal` sound output, the
//! system clipboard and the system cursors -- for the product `P`.

use std::marker::PhantomData;

use dereth_client_runtime::app::{Platform, StartupError};
use dereth_client_runtime::config::Config;
use dereth_client_shell::cursor::CursorImages;
use dereth_client_shell::platform::host::Host;

use crate::Product;

/// The desktop, as the client shell's host, for product `P`.
#[derive(Debug, Clone, Copy, Default)]
pub struct Desktop<P: Product>(PhantomData<P>);

impl<P: Product> Host for Desktop<P> {
    const BUILD_ID: &'static str = P::BUILD_ID;

    type Clipboard = crate::clipboard::Win32Clipboard;

    fn open_platform(
        cfg: &Config,
        events: crate::platform::window::WindowEvents,
    ) -> Result<Platform, StartupError> {
        crate::launch::open_platform::<P>(cfg, events)
    }

    fn local_utc_offset_secs(unix_secs: i64) -> i32 {
        crate::platform::local_utc_offset_secs(unix_secs)
    }

    fn launch_uri(url: &str) -> i32 {
        crate::launch::launch_uri(url)
    }

    fn install_default_output() {
        crate::audio::install_default_output();
    }

    fn clipboard() -> Self::Clipboard {
        crate::clipboard::Win32Clipboard
    }

    fn cursor_images(window: Option<isize>) -> Box<dyn CursorImages> {
        P::cursor_images(window)
    }

    /// The Windows font system; none elsewhere.
    fn classic_fonts() -> Option<std::sync::Arc<dyn dereth_classic_dat::fonts::FontSource>> {
        cfg!(windows).then(|| {
            std::sync::Arc::new(dereth_classic_gdi::fonts::SystemFonts)
                as std::sync::Arc<dyn dereth_classic_dat::fonts::FontSource>
        })
    }

    fn caret_blink_secs() -> f64 {
        crate::platform::window::caret_blink_time_seconds()
    }

    fn classic_help_book() -> Result<Option<Vec<u8>>, String> {
        let Some(path) = std::env::var_os("DERETH_CLASSIC_HELP_BOOK") else {
            return Ok(None);
        };
        read_help_book(std::path::Path::new(&path))
    }

    fn classic_welcome() -> String {
        std::env::var("DERETH_CLASSIC_WELCOME").unwrap_or_default()
    }

    fn movie_bytes(path: &std::path::Path) -> Option<Vec<u8>> {
        std::fs::read(path).ok()
    }
}

fn read_help_book(path: &std::path::Path) -> Result<Option<Vec<u8>>, String> {
    if !path.exists() {
        return Ok(None);
    }
    std::fs::read(path)
        .map(Some)
        .map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (the desktop supplies bytes and distinguishes absent from unreadable help).
    use super::*;

    #[test]
    fn help_loading_distinguishes_missing_unreadable_and_readable_resources() {
        let dir = std::env::temp_dir().join(format!("dereth-host-help-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("help.json");
        assert_eq!(read_help_book(&path).unwrap(), None);
        assert!(read_help_book(&dir).is_err());
        std::fs::write(&path, b"host supplied bytes").unwrap();
        assert_eq!(
            read_help_book(&path).unwrap(),
            Some(b"host supplied bytes".to_vec())
        );
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
