//! The three shipped input payloads, read from the retail dats under `$DERETH_TEST_DAT_DIR`.
//!
//! The `ActionMap` (`0x26000000`) and both master keymaps (`0x14000000`,
//! `DefaultMap 0x14000002`), each the record exactly as the dat stores it -- id word included --
//! which is what `ActionMap::read` and `MasterInputMap::read` take. Read once per binary.

use std::sync::OnceLock;

use dereth_primitives::{AssetSource, DataId};

pub(crate) const ACTIONMAP: DataId = DataId(0x2600_0000);
pub(crate) const KEYMAP_GM: DataId = DataId(0x1400_0000);
pub(crate) const KEYMAP_DEFAULT: DataId = DataId(0x1400_0002);

pub(crate) struct Payloads {
    pub(crate) actionmap: Vec<u8>,
    pub(crate) keymap_gm: Vec<u8>,
    pub(crate) keymap_default: Vec<u8>,
}

/// The payloads, off the retail dats. **No skip**: absent dats are a failure.
pub(crate) fn shipped() -> &'static Payloads {
    static CELL: OnceLock<Payloads> = OnceLock::new();
    CELL.get_or_init(|| {
        let store = dereth_dat::testing::open_store_or_fail();
        let read = |id: DataId| {
            store
                .read(id)
                .unwrap_or_else(|e| panic!("{:#010X} from the retail dats: {e}", id.0))
        };
        Payloads {
            actionmap: read(ACTIONMAP),
            keymap_gm: read(KEYMAP_GM),
            keymap_default: read(KEYMAP_DEFAULT),
        }
    })
}
