//! Behaviour: none (checks data formats, fixture conformance or host contracts)
//! The default 1252 table and the host NLS arm agree over ASCII; the default names code page 1252
//! and widens 0x80/0x93/0x9F correctly.
//! Fixture: recorded messages and synthetic state or packets.

use dereth_client_model::HostText;

/// The default table and the host nls arm agree over ascii.
#[test]
fn the_default_table_and_the_host_nls_arm_agree_over_ascii() {
    let table = HostText::default();
    let units: Vec<u16> = (0x20u16..0x7F).collect();
    let bytes: Vec<u8> = (0x20u8..0x7F).collect();

    assert_eq!(table.narrow(&units).as_deref(), Some(&bytes[..]));
    assert_eq!(table.widen(&bytes).as_deref(), Some(&units[..]));
    assert_eq!(table.english_ws_field(&units).as_deref(), Some(&bytes[..]));

    #[cfg(windows)]
    {
        // `dereth_primitives::text::to_spstring` is `WideCharToMultiByte(CP_ACP, 0, ...)` on this machine.
        assert_eq!(
            dereth_primitives::text::to_spstring(&units)
                .map(|narrow| narrow.bytes)
                .ok()
                .as_deref(),
            table.narrow(&units).as_deref(),
        );
        assert_eq!(
            dereth_primitives::text::to_wpstring(&bytes).ok().as_deref(),
            table.widen(&bytes).as_deref(),
        );
        assert_eq!(
            dereth_primitives::text::english_ws_field(&units)
                .ok()
                .as_deref(),
            table.english_ws_field(&units).as_deref(),
        );
    }
}

/// The default is the table and not a stub, and it reports the code page it actually is. A
/// `HostText` whose `acp` lied would send `turbine.rs` bytes in one encoding and label them
/// another.
#[test]
fn the_default_names_the_code_page_it_converts_in() {
    assert_eq!(HostText::default().acp(), 1252);
    // 0x80..=0x9F is where 1252 differs from Latin-1, so it is where a wrong table shows.
    assert_eq!(
        HostText::default().widen(b"\x80\x93\x9f"),
        Some(vec![0x20ac, 0x201c, 0x178])
    );
}
