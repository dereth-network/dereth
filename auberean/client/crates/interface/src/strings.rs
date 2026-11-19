//! The interface's records with the client's strings: the install's records as the host
//! gave them, and, once a client is here, string references written through its data cache (so a
//! string table the server sent is read as the files' own are; one the cache cannot write is
//! written from the host's records), and the text elements' string downloads started and followed
//! there for references whose string tables the cache lacks.

use std::sync::{Arc, Mutex};

use auberean_assets::ui::{Font, UiLayout};
use auberean_client_sdk::text::{CachedStrings, StringInfo};
use auberean_ui::source::Image;
use auberean_ui::Assets;

/// The host's records, and the client's strings once one is here.
pub struct TextAssets {
    inner: Arc<dyn Assets + Send + Sync>,
    strings: Mutex<Option<Arc<dyn CachedStrings>>>,
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

    /// The client's strings, from now on.
    pub fn connect(&self, strings: &dyn CachedStrings) {
        if let Ok(mut s) = self.strings.lock() {
            *s = Some(strings.share());
        }
    }

    /// The client's strings, when its data cache has files to read.
    fn strings(&self) -> Option<Arc<dyn CachedStrings>> {
        self.strings.lock().ok()?.clone().filter(|s| s.reads())
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

    /// The client's data cache says what is at hand: a reference whose string tables are all
    /// there (its own, every nested reference's, and the private tables their entries name) asks
    /// for nothing, and any other asks for what is missing. The host's records cannot say: they
    /// write a reference whose nested reference they lack, the error message in its place.
    fn start_string_download(&self, info: &StringInfo) -> Option<u32> {
        self.strings()?.start(info)
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
