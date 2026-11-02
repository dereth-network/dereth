//! The interface's records with the client's string downloads: the install's records as the host
//! gave them, and, once a client is here, string references written through its data cache (so a
//! string table the server sent is read as the files' own are; one the cache cannot write is
//! written from the host's records), and the text elements' string downloads started and followed
//! there for references the host's records cannot write.

use std::sync::{Arc, Mutex};

use auberean_assets::ui::{Font, UiLayout};
use auberean_client_sdk::runtime::strings::StringDownloads;
use auberean_client_sdk::text::StringInfo;
use auberean_ui::source::Image;
use auberean_ui::Assets;

/// The host's records, and the client's string downloads once one is here.
pub struct TextAssets {
    inner: Arc<dyn Assets + Send + Sync>,
    strings: Mutex<Option<StringDownloads>>,
}

impl std::fmt::Debug for TextAssets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TextAssets")
            .field("connected", &self.strings().is_some())
            .finish_non_exhaustive()
    }
}

impl TextAssets {
    /// The host's records, with no client yet.
    #[must_use]
    pub fn new(inner: Arc<dyn Assets + Send + Sync>) -> Self {
        Self {
            inner,
            strings: Mutex::new(None),
        }
    }

    /// The client's string downloads, from now on.
    pub fn connect(&self, strings: &StringDownloads) {
        if let Ok(mut s) = self.strings.lock() {
            *s = Some(strings.clone());
        }
    }

    /// The client's string downloads, when its data cache has files to read.
    fn strings(&self) -> Option<StringDownloads> {
        self.strings
            .lock()
            .ok()?
            .clone()
            .filter(StringDownloads::reads)
    }
}

fn text(units: &[u16]) -> String {
    String::from_utf16_lossy(units)
}

impl Assets for TextAssets {
    fn layout(&self, id: u32) -> Option<Arc<UiLayout>> {
        self.inner.layout(id)
    }

    fn image(&self, id: u32) -> Option<Arc<Image>> {
        self.inner.image(id)
    }

    fn font(&self, id: u32) -> Option<Arc<Font>> {
        self.inner.font(id)
    }

    fn string(&self, table: u32, token: u32) -> Option<String> {
        self.inner.string(table, token)
    }

    fn data_id_by_enum(&self, value: u32, mapping: u32) -> u32 {
        self.inner.data_id_by_enum(value, mapping)
    }

    fn text(&self, table: u32, token: u32) -> Option<String> {
        self.inner.text(table, token)
    }

    fn write(&self, info: &StringInfo) -> Option<String> {
        self.strings()
            .and_then(|s| s.write_rendered(info))
            .map(|u| text(&u))
            .or_else(|| self.inner.write(info))
    }

    fn write_valid(&self, info: &StringInfo) -> Option<String> {
        self.strings()
            .and_then(|s| s.write_valid(info))
            .map(|u| text(&u))
            .or_else(|| self.inner.write_valid(info))
    }

    fn element_text(&self, info: &StringInfo) -> Vec<u16> {
        self.strings()
            .and_then(|s| s.write_rendered(info))
            .unwrap_or_else(|| self.inner.element_text(info))
    }

    /// A reference the host's own records write is at hand; any other is the client's to ask
    /// for.
    fn start_string_download(&self, info: &StringInfo) -> Option<u32> {
        let s = self.strings()?;
        if self.inner.write(info).is_some() {
            return None;
        }
        s.start(info)
    }

    fn string_download_waiting(&self, download: u32) -> bool {
        self.strings().is_some_and(|s| s.waiting(download))
    }

    fn release_string_download(&self, download: u32) {
        if let Some(s) = self.strings() {
            s.release(download);
        }
    }
}
