//! The browser bindings of the front page's launcher: what its worker's script calls, and the calls
//! it answers.
//!
//! The worker owns everything asynchronous (the player's files, every fetch); this side is
//! synchronous and keeps the launcher's state between calls. A worker has one thread, so the state
//! lives in thread-locals.

use std::cell::RefCell;
use std::sync::Arc;

use dereth_web_store::{overlay, settings};
use wasm_bindgen::prelude::*;

use crate::front::Front;
use crate::plugin_sources::PageShelf;

#[wasm_bindgen]
extern "C" {
    /// Fill `buf` from `offset` of the file `file` the worker is reading a data set's files from
    /// (to say what each reports); `false` when it is too short.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethProbeRead)]
    fn probe_read(file: u32, offset: f64, buf: &mut [u8]) -> bool;

    /// The client's files as the page last saved them; empty for none.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethSettingsLoad)]
    fn settings_load() -> Vec<u8>;

    /// Keep the client's files: the whole store's bytes, written before the call returns.
    #[wasm_bindgen(js_namespace = globalThis, js_name = derethSettingsSave)]
    fn settings_save(bytes: &[u8]);

    #[wasm_bindgen(js_namespace = console, js_name = error)]
    fn console_error(message: &str);
}

thread_local! {
    static FRONT: RefCell<Front> = RefCell::new(Front::default());
    /// The front page's plugin sources and the plugins installed from them.
    static SHELF: RefCell<PageShelf> = RefCell::new(PageShelf::default());
}

/// A file of a data set the worker is reading, by its place in the worker's list.
#[derive(Debug, Clone, Copy)]
struct ProbeFile(u32);

impl dereth_dat::DatStorage for ProbeFile {
    fn read_exact_at(&self, offset: u64, buf: &mut [u8]) -> std::io::Result<()> {
        // A dat file is under 4 GiB, so the offset is exact as a JavaScript number.
        #[allow(clippy::cast_precision_loss)]
        let at = offset as f64;
        if probe_read(self.0, at, buf) {
            Ok(())
        } else {
            Err(std::io::ErrorKind::UnexpectedEof.into())
        }
    }
}

/// A whole, non-negative JavaScript number (a size, a time in seconds) as an integer; 0 for
/// anything else.
fn whole(x: f64) -> u64 {
    u64::try_from(dereth_primitives::num::to_i64_f64(x)).unwrap_or(0)
}

/// The page's clock, in whole seconds since the epoch.
fn now_secs() -> u64 {
    whole(js_sys::Date::now() / 1000.0)
}

fn js_err(e: impl std::fmt::Display) -> JsError {
    JsError::new(&e.to_string())
}

/// Report panics on the console rather than as an unexplained trap.
#[wasm_bindgen]
pub fn start() {
    std::panic::set_hook(Box::new(|info| console_error(&info.to_string())));
}

/// Keep the client's own files (its preferences, the overlay blocklist) in the page's settings
/// file from now on: the worker opened it before it asks anything of them.
#[wasm_bindgen(js_name = installSettings)]
pub fn install_settings() {
    settings::install(&settings_load(), Some(settings_save));
}

/// Why `url` cannot be the server the worker connects to, or nothing when it can: `wss://` to any
/// host, and `ws://` only to this machine (see [`crate::server_url`]).
#[wasm_bindgen(js_name = serverUrlProblem)]
#[must_use]
pub fn server_url_problem(url: &str) -> Option<String> {
    crate::server_url::problem(url)
}

// ---- the front page's launcher ---------------------------------------------------------------

/// Start reading the world list, by the desktop launcher's rule: the `Servers.xml` `named` (the
/// page's `list` parameter) alone when one is named, and otherwise the Dereth Network hub's list
/// (or the one `hub` names, the page's `hub` parameter), then the community's own. Answers the
/// first address to fetch.
#[wasm_bindgen(js_name = frontListStart)]
#[must_use]
pub fn front_list_start(named: Option<String>, hub: Option<String>) -> Option<String> {
    FRONT.with(|f| f.borrow_mut().list_start(named.as_deref(), hub.as_deref()))
}

/// What fetching the address last given brought: its `body`, or the `error` that kept it from
/// coming. Answers the next address to fetch, or nothing once the reading is over.
#[wasm_bindgen(js_name = frontListNext)]
#[must_use]
pub fn front_list_next(body: Option<String>, error: Option<String>) -> Option<String> {
    let fetched = body
        .map(String::into_bytes)
        .ok_or_else(|| error.unwrap_or_else(|| "no answer".to_owned()));
    FRONT.with(|f| f.borrow_mut().list_next(fetched))
}

/// The list read becomes the world list. Answers, as JSON, how many worlds it lists, where it came
/// from and the day's copy (fetched at `now`, seconds since the epoch) for the page to keep.
///
/// # Errors
/// No source answered with its list; the worlds read before stand.
#[wasm_bindgen(js_name = frontListDone)]
pub fn front_list_done(now: f64) -> Result<String, JsError> {
    FRONT
        .with(|f| f.borrow_mut().list_done(whole(now)))
        .map_err(js_err)
}

// ---- the front page's plugin sources -------------------------------------------------------

/// A download for the page to make, as JSON: `{ url, limit }`.
fn download_json(d: &dereth_launch::plugins::Download) -> String {
    serde_json::json!({ "url": d.url, "limit": d.limit }).to_string()
}

/// Where the page keeps its plugin sources and the record of what it installed, in the origin's
/// private storage, in that order: the worker reads them for [`front_plugins_open`].
#[wasm_bindgen(js_name = frontPluginRecords)]
#[must_use]
pub fn front_plugin_records() -> Vec<String> {
    vec![
        crate::plugin_sources::SOURCES_PATH.to_owned(),
        crate::plugin_sources::INSTALLED_PATH.to_owned(),
    ]
}

/// Take the plugin sources and the installed record as the page kept them (`undefined` for one it
/// does not have).
#[wasm_bindgen(js_name = frontPluginsOpen)]
pub fn front_plugins_open(sources: Option<Vec<u8>>, installed: Option<Vec<u8>>) {
    SHELF.with(|s| *s.borrow_mut() = PageShelf::open(sources, installed));
}

/// What the page is to write now, in order: each `[path, bytes]`, with `null` bytes for a file to
/// delete.
#[wasm_bindgen(js_name = frontPluginWrites)]
#[must_use]
pub fn front_plugin_writes() -> js_sys::Array {
    let out = js_sys::Array::new();
    for (path, bytes) in SHELF.with(|s| s.borrow_mut().take_writes()) {
        let pair = js_sys::Array::new();
        pair.push(&JsValue::from_str(&path));
        pair.push(&bytes.map_or(JsValue::NULL, |b| {
            js_sys::Uint8Array::from(b.as_slice()).into()
        }));
        out.push(&pair);
    }
    out
}

/// Where the page keeps the zip of plugin `id` installed from a source, in the origin's private
/// storage: the page tells those apart from the plugins the player added by hand.
#[wasm_bindgen(js_name = frontPluginZipPath)]
#[must_use]
pub fn front_plugin_zip_path(id: &str) -> String {
    crate::plugin_sources::zip_path(id)
}

/// The plugin sources and the plugins, as the page shows them, as JSON.
#[wasm_bindgen(js_name = frontPlugins)]
#[must_use]
pub fn front_plugins() -> String {
    SHELF.with(|s| s.borrow().view().to_string())
}

/// Add the plugin source at `url`; the page has the player confirm it first. Answers the download of its list, which the page makes next, as JSON (`{ url, limit }`).
///
/// # Errors
/// The address was refused, in words for the page.
#[wasm_bindgen(js_name = frontPluginAddSource)]
pub fn front_plugin_add_source(url: &str) -> Result<String, JsError> {
    SHELF
        .with(|s| s.borrow_mut().add_source(url))
        .map(|d| download_json(&d))
        .map_err(js_err)
}

/// What fetching the list of the source just added brought: its `body`, the `error` its host
/// answered with, or neither when the page could not reach it. Answers what to tell the player.
#[wasm_bindgen(js_name = frontPluginSourceRead)]
#[must_use]
pub fn front_plugin_source_read(url: &str, body: Option<Vec<u8>>, error: Option<String>) -> String {
    SHELF.with(|s| s.borrow_mut().source_read(url, body, error, now_secs()))
}

/// Remove the plugin source at `url`; what was installed from it stays, with no updates.
///
/// # Errors
/// Dereth's own source, or not a source.
#[wasm_bindgen(js_name = frontPluginRemoveSource)]
pub fn front_plugin_remove_source(url: &str) -> Result<(), JsError> {
    SHELF
        .with(|s| s.borrow_mut().remove_source(url))
        .map_err(js_err)
}

/// The download to make to install plugin `id` from `source`, or with no source to update it from
/// its own, as JSON (`{ url, limit }`).
///
/// # Errors
/// It cannot be installed or updated here, in words for the page.
#[wasm_bindgen(js_name = frontPluginPlan)]
pub fn front_plugin_plan(source: Option<String>, id: &str) -> Result<String, JsError> {
    SHELF
        .with(|s| s.borrow().plan(source.as_deref(), id))
        .map(|d| download_json(&d))
        .map_err(js_err)
}

/// What fetching the download [`front_plugin_plan`] named brought, as for a source's list: the
/// plugin installed or updated with it. Answers what to tell the player.
///
/// # Errors
/// Nothing came, or the release was refused, in words for the page.
#[wasm_bindgen(js_name = frontPluginTake)]
pub fn front_plugin_take(
    source: Option<String>,
    id: &str,
    body: Option<Vec<u8>>,
    error: Option<String>,
) -> Result<String, JsError> {
    SHELF
        .with(|s| s.borrow_mut().take(source.as_deref(), id, body, error))
        .map_err(js_err)
}

/// Remove the installed plugin `id`.
///
/// # Errors
/// Not installed.
#[wasm_bindgen(js_name = frontPluginRemove)]
pub fn front_plugin_remove(id: &str) -> Result<(), JsError> {
    SHELF.with(|s| s.borrow_mut().remove(id)).map_err(js_err)
}

/// When plugin updates apply without the player: `off`, `notify`, `first_party` or `all`.
///
/// # Errors
/// Not one of those.
#[wasm_bindgen(js_name = frontPluginUpdates)]
pub fn front_plugin_updates(setting: &str) -> Result<(), JsError> {
    SHELF
        .with(|s| s.borrow_mut().set_updates(setting))
        .map_err(js_err)
}

/// Start reading every plugin source and applying the updates that apply on their own: the first
/// download to make, as JSON (`{ url, limit }`), or nothing.
#[wasm_bindgen(js_name = frontPluginRefreshStart)]
#[must_use]
pub fn front_plugin_refresh_start() -> Option<String> {
    SHELF.with(|s| s.borrow_mut().refresh_start().map(|d| download_json(&d)))
}

/// What the refresh's last download brought, as for a source's list: the next download, or
/// nothing once the refresh is over.
#[wasm_bindgen(js_name = frontPluginRefreshNext)]
#[must_use]
pub fn front_plugin_refresh_next(body: Option<Vec<u8>>, error: Option<String>) -> Option<String> {
    SHELF.with(|s| {
        s.borrow_mut()
            .refresh_next(body, error, now_secs())
            .map(|d| download_json(&d))
    })
}

/// What the refresh came to, a line each (empty when nothing is new), as JSON: `{ said }`.
#[wasm_bindgen(js_name = frontPluginRefreshDone)]
#[must_use]
pub fn front_plugin_refresh_done() -> String {
    SHELF.with(|s| serde_json::json!({ "said": s.borrow_mut().refresh_done() }).to_string())
}

/// Take the day's copy of the world list, as the page kept it, for the list read from the sources
/// the page names (`named`, its `list` parameter; `hub`, its `hub` parameter); answers how many
/// worlds it lists.
///
/// # Errors
/// The copy does not read, or was read from other sources; the worlds read before stand.
#[wasm_bindgen(js_name = frontListCopy)]
pub fn front_list_copy(
    copy: &str,
    named: Option<String>,
    hub: Option<String>,
) -> Result<u32, JsError> {
    let n = FRONT
        .with(|f| {
            f.borrow_mut()
                .set_copy(copy, named.as_deref(), hub.as_deref())
        })
        .map_err(js_err)?;
    Ok(u32::try_from(n).unwrap_or(u32::MAX))
}

/// Whether the world list is to be read at `now` (seconds since the epoch), as the page opens: no
/// copy of it is shown, or the copy is not the day's.
#[wasm_bindgen(js_name = frontListWanted)]
#[must_use]
pub fn front_list_wanted(now: f64) -> bool {
    FRONT.with(|f| f.borrow().list_wanted(whole(now)))
}

/// Take the launcher's state record as the page kept it (empty for none).
///
/// # Errors
/// A record that does not read.
#[wasm_bindgen(js_name = frontSetState)]
pub fn front_set_state(json: &str) -> Result<(), JsError> {
    FRONT
        .with(|f| f.borrow_mut().set_state(json))
        .map_err(js_err)
}

/// The launcher's state record, for the page to keep.
#[wasm_bindgen(js_name = frontState)]
#[must_use]
pub fn front_state() -> String {
    FRONT.with(|f| f.borrow().state_json())
}

/// A world's status document, as fetched; answers whether it was one.
#[wasm_bindgen(js_name = frontStatus)]
#[must_use]
pub fn front_status(slug: &str, body: &[u8]) -> bool {
    FRONT.with(|f| f.borrow_mut().set_status(slug, body))
}

/// A world's status document did not answer.
#[wasm_bindgen(js_name = frontStatusFailed)]
pub fn front_status_failed(slug: &str) {
    FRONT.with(|f| f.borrow_mut().status_failed(slug));
}

/// Start reading the hub's list (the network's, or the one `hub` names) for each world's status,
/// when a read is due at `now` (seconds since the epoch): about a minute after the last, while the
/// worlds shown are the hub's list's and the page names no list of its own (`named`, its `list`
/// parameter). Answers the first address to fetch; nothing when no read is due.
#[wasm_bindgen(js_name = frontHubStart)]
#[must_use]
pub fn front_hub_start(named: Option<String>, hub: Option<String>, now: f64) -> Option<String> {
    FRONT.with(|f| {
        f.borrow_mut()
            .hub_start(named.as_deref(), hub.as_deref(), whole(now))
    })
}

/// What fetching the address last given for the status brought: its `body`, or the `error` that
/// kept it from coming. Answers the next address to fetch, or nothing once the read is over.
#[wasm_bindgen(js_name = frontHubNext)]
#[must_use]
pub fn front_hub_next(body: Option<String>, error: Option<String>) -> Option<String> {
    let fetched = body
        .map(String::into_bytes)
        .ok_or_else(|| error.unwrap_or_else(|| "no answer".to_owned()));
    FRONT.with(|f| f.borrow_mut().hub_next(fetched))
}

/// The status read is over, at `now` (seconds since the epoch). Answers whether it brought the
/// hub's word; a read that failed changes nothing.
#[wasm_bindgen(js_name = frontHubDone)]
#[must_use]
pub fn front_hub_done(now: f64) -> bool {
    FRONT.with(|f| f.borrow_mut().hub_done(whole(now)))
}

/// Every world as it is shown now, with the eras and systems on offer, as JSON.
#[wasm_bindgen(js_name = frontWorlds)]
#[must_use]
pub fn front_worlds() -> String {
    FRONT.with(|f| f.borrow().worlds(now_secs()).to_string())
}

/// One world's page as it is shown now, as JSON: [`Front::world`]. `None` for a world not shown.
#[wasm_bindgen(js_name = frontWorld)]
#[must_use]
pub fn front_world(slug: &str) -> Option<String> {
    FRONT.with(|f| f.borrow().world(slug, now_secs()).map(|v| v.to_string()))
}

/// Add a server by host and port; answers its slug.
///
/// # Errors
/// What is wrong with the host or port, for the player.
#[wasm_bindgen(js_name = frontAddWorld)]
pub fn front_add_world(
    game: &str,
    name: &str,
    host: &str,
    port: &str,
    ruleset: &str,
    emulator: &str,
) -> Result<String, JsError> {
    FRONT
        .with(|f| {
            f.borrow_mut()
                .add_world_for(game, name, host, port, ruleset, emulator)
        })
        .map_err(js_err)
}

/// The URL the server the player added as `slug` was added by.
#[wasm_bindgen(js_name = frontAddedBy)]
pub fn front_added_by(slug: &str, url: &str) {
    FRONT.with(|f| f.borrow_mut().added_by(slug, url));
}

/// Take a server the player added off the list.
#[wasm_bindgen(js_name = frontRemoveWorld)]
pub fn front_remove_world(slug: &str) {
    FRONT.with(|f| f.borrow_mut().remove_world(slug));
}

/// A start on a world, recorded now as the desktop launcher records one (JSON: `account`,
/// `dat_set_id`, `classic_set_id`).
///
/// # Errors
/// Choices that do not read.
#[wasm_bindgen(js_name = frontRecordLaunch)]
pub fn front_record_launch(slug: &str, prefs: &str) -> Result<(), JsError> {
    FRONT
        .with(|f| f.borrow_mut().record_launch(slug, prefs, now_secs()))
        .map_err(js_err)
}

/// Pin a combination to Home (JSON, a favourite); answers its id.
///
/// # Errors
/// A favourite that does not read.
#[wasm_bindgen(js_name = frontSaveFavourite)]
pub fn front_save_favourite(favourite: &str) -> Result<String, JsError> {
    FRONT
        .with(|f| f.borrow_mut().save_favourite(favourite))
        .map_err(js_err)
}

/// Unpin the favourite `id`.
#[wasm_bindgen(js_name = frontRemoveFavourite)]
pub fn front_remove_favourite(id: &str) {
    FRONT.with(|f| f.borrow_mut().remove_favourite(id));
}

/// Star the world `slug` in the world list, or take its star away.
#[wasm_bindgen(js_name = frontSetWorldFavorite)]
pub fn front_set_world_favorite(slug: &str, favorite: bool) {
    FRONT.with(|f| f.borrow_mut().set_world_favorite(slug, favorite));
}

/// Forget the account `username` on world `slug`.
#[wasm_bindgen(js_name = frontForgetAccount)]
pub fn front_forget_account(slug: &str, username: &str) {
    FRONT.with(|f| f.borrow_mut().forget_account(slug, username));
}

/// Forget every account.
#[wasm_bindgen(js_name = frontForgetAllAccounts)]
pub fn front_forget_all_accounts() {
    FRONT.with(|f| f.borrow_mut().forget_all_accounts());
}

/// Make the data set `id` the default of its kind.
///
/// # Errors
/// No such set.
#[wasm_bindgen(js_name = frontSetDefaultSet)]
pub fn front_set_default_set(id: &str) -> Result<(), JsError> {
    FRONT
        .with(|f| f.borrow_mut().set_default_set(id))
        .map_err(js_err)
}

/// The interface the client starts in and the ones on offer, as JSON, as the desktop launcher's
/// page has them: `{ chosen, offers: [{ name, line }] }`, `chosen` null while the kept profile
/// names none.
#[wasm_bindgen(js_name = frontInterface)]
#[must_use]
pub fn front_interface() -> String {
    serde_json::json!({
        "chosen": settings::interface().map(|i| i.label()),
        "offers": dereth_launch::interface::offers(),
    })
    .to_string()
}

/// Name the interface the client starts in, by its name in any case.
///
/// # Errors
/// No interface of that name, or a profile the store will not take.
#[wasm_bindgen(js_name = frontSetInterface)]
pub fn front_set_interface(name: &str) -> Result<(), JsError> {
    let choice = dereth_launch::interface::by_name(name)
        .ok_or_else(|| JsError::new(&format!("{name} is not an interface the client has.")))?;
    settings::set_interface(choice).map_err(js_err)
}

/// This build's version: Dereth's.
#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    env!("CARGO_PKG_VERSION").to_owned()
}

/// Read the files the worker holds open for a data set kept in the browser's folder `folder`:
/// `names[i]` (of `sizes[i]` bytes) is read through the worker's file `i`. Answers the ids of the
/// folder's sets.
#[wasm_bindgen(js_name = frontAddSets)]
#[must_use]
pub fn front_add_sets(folder: &str, names: Vec<String>, sizes: Vec<f64>) -> Vec<String> {
    let files = names
        .into_iter()
        .zip(sizes)
        .enumerate()
        .map(|(i, (name, size))| {
            let storage: Box<dyn dereth_dat::DatStorage> =
                Box::new(ProbeFile(u32::try_from(i).unwrap_or(u32::MAX)));
            (name, whole(size), storage)
        })
        .collect();
    FRONT.with(|f| f.borrow_mut().add_sets(folder, files))
}

/// Read the files the worker holds open for a data set of Asheron's Call 2's kept in the browser's
/// folder `folder`, as [`front_add_sets`] reads Asheron's Call's. Answers its set's id.
#[wasm_bindgen(js_name = frontAddAubereanSets)]
#[must_use]
pub fn front_add_auberean_sets(folder: &str, names: Vec<String>, sizes: Vec<f64>) -> Vec<String> {
    let files = names
        .into_iter()
        .zip(sizes)
        .enumerate()
        .map(|(i, (name, size))| {
            let storage: Box<dyn dereth_dat::DatStorage> =
                Box::new(ProbeFile(u32::try_from(i).unwrap_or(u32::MAX)));
            (name, whole(size), storage)
        })
        .collect();
    FRONT.with(|f| f.borrow_mut().add_auberean_sets(folder, files))
}

/// The names of the files of a data set of Asheron's Call 2's.
#[wasm_bindgen(js_name = frontAubereanFiles)]
#[must_use]
pub fn front_auberean_files() -> Vec<String> {
    dereth_launch::auberean::set_file_names()
        .into_iter()
        .map(str::to_owned)
        .collect()
}

/// Why files named `names` are not a data set of Asheron's Call 2's, in words for the player, or
/// nothing when they are.
#[wasm_bindgen(js_name = frontAubereanSetProblem)]
#[must_use]
#[allow(clippy::needless_pass_by_value)]
pub fn front_auberean_set_problem(names: Vec<String>) -> Option<String> {
    dereth_launch::auberean::set_problem(names.iter().map(String::as_str))
}

/// The default data set of Asheron's Call 2's check, as JSON: `{ ok, summary }`.
#[wasm_bindgen(js_name = frontAubereanCheck)]
#[must_use]
pub fn front_auberean_check() -> String {
    FRONT.with(|f| f.borrow().auberean_check().to_string())
}

/// Keep the realm the page shows, which it opens in next time.
#[wasm_bindgen(js_name = frontSetRealm)]
pub fn front_set_realm(realm: &str) {
    FRONT.with(|f| f.borrow_mut().set_realm(realm));
}

/// Forget the sets kept in the browser's folder `folder`.
#[wasm_bindgen(js_name = frontRemoveSets)]
pub fn front_remove_sets(folder: &str) {
    FRONT.with(|f| f.borrow_mut().remove_sets(folder));
}

/// The folder a world's overlay is kept in, from its address (`host:port`).
#[wasm_bindgen(js_name = overlayFolder)]
#[must_use]
pub fn overlay_folder(address: &str) -> String {
    format!("overlays/{}", crate::front::overlay_folder(address))
}

// ---- the world's overlay -----------------------------------------------------------------------

/// The worker now holds the overlay folder `folder` open (empty for none).
#[wasm_bindgen(js_name = holdOverlay)]
pub fn hold_overlay(folder: &str) {
    overlay::hold(folder);
}

/// What the folder the worker holds open keeps, as JSON: the world its overlay belongs to, the set
/// it was made against and its containers; `null` when the worker holds none.
#[wasm_bindgen(js_name = overlayInfo)]
#[must_use]
pub fn overlay_info() -> String {
    let Some(folder) = overlay::BrowserFolder::held() else {
        return "null".into();
    };
    match dereth_dat::overlay::OverlayDir::in_folder(Arc::new(folder)) {
        Ok(dir) => serde_json::json!({
            "folder": dir.path().display().to_string(),
            "world_key": dir.world_key(),
            "base": dir.base_era().map(|e| match e {
                dereth_primitives::ContainerEra::Modern => "modern",
                dereth_primitives::ContainerEra::Classic => "classic",
            }),
            "containers": dir
                .containers()
                .iter()
                .filter_map(|p| p.file_name().map(|n| n.to_string_lossy().into_owned()))
                .collect::<Vec<_>>(),
        })
        .to_string(),
        Err(e) => serde_json::json!({ "error": e.to_string() }).to_string(),
    }
}

/// The worlds whose overlays the player refuses: the client's overlay blocklist, kept with its
/// settings.
#[wasm_bindgen(js_name = overlayBlocklist)]
#[must_use]
pub fn overlay_blocklist() -> Vec<String> {
    settings::blocklist().into_iter().collect()
}

/// Refuse the overlay of the world `key` (as the world names itself), or take it off the list.
///
/// # Errors
/// The settings file will not take it.
#[wasm_bindgen(js_name = setOverlayBlocked)]
pub fn set_overlay_blocked(key: &str, blocked: bool) -> Result<(), JsError> {
    settings::set_blocked(key, blocked).map_err(js_err)
}
