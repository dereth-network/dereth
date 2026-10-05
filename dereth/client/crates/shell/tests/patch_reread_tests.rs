//! After a data patch, the interfaces draw the patched records: a record of the classic portal
//! patched through the data-patch path is the one the classic interface's art reads, though the
//! interface was built over the files before the patch.

use super::*;
use crate::{app::CoreApp, platform::host::NullHost};
use dereth_dat::overlay::OverlayDir;
use dereth_primitives::DataId;
use dereth_protocol::admin::DddData;

/// The record patched: the classic character screen's left panel.
const PATCHED: u32 = dereth_classic_ui::art::CHARACTER_PANEL;
/// The record whose picture it is patched to: another full-screen picture of the same portal.
const REPLACEMENT: u32 = 0x0600_1239;

struct Client {
    app: CoreApp<NullHost>,
    shell: ClientShell<NullHost>,
    art: std::sync::Arc<dereth_classic_ui::art::ClassicArt>,
}

impl Client {
    /// The client over a world drawn from the files before Throne of Destiny, its overlay kept in
    /// `overlay` (empty), with the classic interface built over the files as they open.
    fn new(state: &std::path::Path, overlay: &std::path::Path) -> Self {
        let cfg = dereth_client_runtime::config::Config {
            headless: true,
            connect: false,
            sound: false,
            world: false,
            width: 800,
            height: 600,
            dat_dir: dereth_dat::testing::both_sets_dir(),
            world_base: Some(dereth_primitives::ContainerEra::Classic),
            overlay_dat_dir: Some(overlay.to_path_buf()),
            preferences_file: state.join("prefs.ini"),
            ..Default::default()
        };
        let mut app = CoreApp::<NullHost>::bring_up_with_store(
            cfg,
            None,
            |_| Ok(dereth_client_runtime::app::Platform::headless(800, 600)),
            |_, _, _, _| Ok(Box::new(crate::present::NullPresentation::new(800, 600))),
        )
        .expect("both dat sets and a device-free presentation");
        let mut shell =
            ClientShell::with_window_events(app.window.raw_handle(), Default::default());
        app.start_shell(&mut shell).expect("real input and UI");
        struct Fonts;
        impl dereth_classic_dat::fonts::FontSource for Fonts {
            fn rasterize(
                &self,
                _: &dereth_classic_dat::fonts::FontSpec,
            ) -> Result<dereth_classic_dat::fonts::FontAtlas, String> {
                Ok(Default::default())
            }
        }
        // As the classic interface is brought up: its portal is the world's, as it opened.
        let portal = dereth_classic_dat::ClassicPortal::of_store(&app.store)
            .expect("the world's own portal is the classic one");
        let art = std::sync::Arc::new(
            dereth_classic_ui::art::ClassicArt::new(portal, &Fonts).expect("the art"),
        );
        let mut ui = dereth_classic_ui::runtime::ClassicUi::new(
            dereth_classic_ui::resources::Resources::new(
                std::sync::Arc::clone(&art),
                Err("World creation tables unavailable".into()),
                None,
            ),
            dereth_classic_ui::art::ClassicPaths {
                state: state.join("classic"),
            },
            dereth_classic_ui::panels::factory,
            (800, 600),
        );
        ui.start(&mut app.ui_context())
            .expect("the classic interface starts");
        shell.classic.ui = Some(ui);
        let mut c = Self { app, shell, art };
        c.frame();
        c
    }

    fn frame(&mut self) {
        assert!(self.app.frame(&mut self.shell));
    }

    /// Patch `id` of the portal to `payload` through the data-patch path, and end the patch as
    /// `DDD_EndDDD` does.
    fn patch(&mut self, id: u32, payload: &[u8]) {
        let iteration = self
            .app
            .store
            .portal()
            .entry(DataId(id))
            .map_or(0, |e| e.iteration)
            + 1;
        let m = DddData {
            dat_file_type: 0,
            dat_file_id: 1,
            resource_type: 0x0C,
            resource_id: id,
            iteration,
            compressed: 0,
            version: 1,
            data_size: u32::try_from(payload.len()).expect("small") + 4,
            data: payload.to_vec(),
        };
        let (outcome, _) = self.app.ddd.on_data(&m);
        assert!(outcome.wrote(), "the patch is written: {outcome:?}");
        self.app.ddd_invalidation = Some(self.app.ddd.on_end());
        self.app.invalidate_after_ddd();
    }
}

/// Behaviour: net.dat-patch.every-view-reads-the-patched-records
#[test]
#[cfg_attr(
    not(feature = "retail-dats"),
    ignore = "reads retail and classic interface data"
)]
fn the_classic_interface_draws_a_portal_record_patched_after_it_was_built() {
    let state =
        dereth_dat::testing::ScratchDir::new("patch-reread-state").expect("a scratch directory");
    let overlay = dereth_dat::testing::ScratchDir::new("patch-reread").expect("a scratch folder");
    let overlay = overlay.path().join("overlay");
    OverlayDir::new(&overlay).expect("an overlay folder");
    let mut c = Client::new(state.path(), &overlay);

    let before = c.art.image(PATCHED).expect("the 2005 panel decodes");
    // The replacement's picture, as a record of the patched id: a picture record names itself.
    let mut replacement = c
        .app
        .store
        .portal()
        .read(DataId(REPLACEMENT))
        .expect("the replacement record");
    replacement[..4].copy_from_slice(&PATCHED.to_le_bytes());
    let want = c.art.image(REPLACEMENT).expect("the replacement decodes");
    assert!(*before != *want, "the two pictures differ");

    let generation = c.app.store_generation();
    let assets = std::sync::Arc::clone(&c.app.anim_assets);
    c.patch(PATCHED, &replacement);
    assert_eq!(
        c.app.store_generation(),
        generation + 1,
        "the files reopened"
    );
    assert!(
        c.app.store.portal().read(DataId(PATCHED)).expect("reads") == replacement,
        "the reopened files read the patched record"
    );
    assert!(
        !std::sync::Arc::ptr_eq(&assets, &c.app.anim_assets),
        "the preview spaces' assets are built over the reopened files"
    );

    c.frame();
    let after = c.art.image(PATCHED).expect("the patched panel decodes");
    assert!(
        *after == *want,
        "the classic interface's art reads the patched record, not the one it decoded before"
    );
    assert!(
        c.art.portal().get(PATCHED) == Some(replacement),
        "the art reads the reopened files"
    );
}
