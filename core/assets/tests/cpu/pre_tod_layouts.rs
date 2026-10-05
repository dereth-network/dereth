//! Behaviour: none (checks formats, geometry, numeric contracts or repository tools)
//! Records in the layouts from before Throne of Destiny read into the same values as the later
//! layouts: the widened XP list, the image textures, the leading ids and the older alignment.
//! Fixture: records built byte by byte in the test.

use dereth_assets::material::PFID_P8;
use dereth_assets::{
    Decode, EnvCell, LanguageString, RenderSurface, Surface, SurfaceTexture, XpTable,
};
use dereth_dat::ContainerEra;
use dereth_primitives::DataId;

fn words(ws: &[u32]) -> Vec<u8> {
    ws.iter().flat_map(|w| w.to_le_bytes()).collect()
}

#[test]
fn a_pre_tod_xp_table_widens_the_level_list_from_u32() {
    // id, five maxima (1 each), then six two-entry lists.
    let mut b = words(&[0x0E00_0018, 1, 1, 1, 1, 1]);
    b.extend(words(&[0, 10, 0, 20, 0, 30, 0, 40]));
    b.extend(words(&[0, 4_286_609_098]));
    b.extend(words(&[0, 5]));
    let xp = XpTable::decode_payload_in(ContainerEra::Classic, DataId(0x0E00_0018), &b).unwrap();
    assert_eq!(xp.level_xp, [0, 4_286_609_098]);
    assert_eq!(xp.level_credits, [0, 5]);
    // The later layout reads the level list as u64 and runs out.
    assert!(XpTable::decode_payload(DataId(0x0E00_0018), &b).is_err());
}

#[test]
fn a_pre_tod_surface_starts_with_its_own_id() {
    let b = words(&[0x0800_0003, 1, 0xFF80_4020, 0, 0x3F80_0000, 0]);
    let s = Surface::decode_payload_in(ContainerEra::Classic, DataId(0x0800_0003), &b).unwrap();
    assert_eq!(s.surface_type, 1);
    assert_eq!(s.color_value, Some(0xFF80_4020));
    assert_eq!(s.luminosity, 1.0);
}

/// A 2x2 palette-indexed image texture: the pixels, the palette, then alignment.
#[test]
fn a_pre_tod_image_texture_is_its_own_level_and_reads_as_an_image() {
    let id = DataId(0x0500_0010);
    let mut b = words(&[id.raw(), 2, 2, 2]);
    b.extend([1, 2, 3, 4]);
    b.extend(words(&[0x0400_0007]));
    let t = SurfaceTexture::decode_payload_in(ContainerEra::Classic, id, &b).unwrap();
    assert_eq!(t.source_levels, [id]);
    let rs = RenderSurface::from_classic_texture(id, &b).unwrap();
    assert_eq!((rs.width, rs.height, rs.format), (2, 2, PFID_P8));
    assert_eq!(rs.payload(&b), Some(&[1u8, 2, 3, 4][..]));
    assert_eq!(rs.default_palette_id, Some(DataId(0x0400_0007)));
    // An unknown image type is refused.
    let mut bad = b.clone();
    bad[4] = 5;
    assert!(RenderSurface::from_classic_texture(id, &bad).is_err());
}

#[test]
fn a_pre_tod_render_surface_is_rgb_bytes_with_no_header_fields() {
    let id = DataId(0x0600_0001);
    let mut b = words(&[id.raw(), 1, 2]);
    b.extend([10, 20, 30, 40, 50, 60]);
    let rs = RenderSurface::decode_payload_in(ContainerEra::Classic, id, &b).unwrap();
    assert_eq!(
        (rs.width, rs.height, rs.image_size, rs.format),
        (1, 2, 6, 242)
    );
    assert_eq!(rs.payload(&b), Some(&[10u8, 20, 30, 40, 50, 60][..]));
}

#[test]
fn a_pre_tod_string_is_a_padded_u16_length_string() {
    let id = DataId(0x3100_0001);
    let mut b = words(&[id.raw()]);
    b.extend(5u16.to_le_bytes());
    b.extend(b"Hello");
    b.push(0); // padding to four
    let s = LanguageString::decode_payload_in(ContainerEra::Classic, id, &b).unwrap();
    assert_eq!(s.text, "Hello");
}

/// Flags, the cell id (no id before the flags), one surface then alignment, the environment and
/// cell structure, the frame, no portals, one visible cell then alignment.
#[test]
fn a_pre_tod_env_cell_starts_with_flags_and_aligns_after_its_lists() {
    let id = DataId(0x0163_0100);
    let mut b = words(&[0, id.raw()]);
    b.extend([1u8, 0]);
    b.extend(1u16.to_le_bytes());
    b.extend(0x0123u16.to_le_bytes());
    b.extend([0, 0]); // alignment after the surface list
    b.extend(0x0045u16.to_le_bytes());
    b.extend(2u16.to_le_bytes());
    b.extend(words(&[0, 0, 0, 0x3F80_0000, 0, 0, 0]));
    b.extend(0x0101u16.to_le_bytes());
    b.extend([0, 0]); // alignment after the visible cells
    let cell = EnvCell::decode_payload_in(ContainerEra::Classic, id, &b).unwrap();
    assert_eq!(cell.id, id);
    assert_eq!(cell.surfaces, [DataId(0x0800_0123)]);
    assert_eq!(cell.environment, DataId(0x0D00_0045));
    assert_eq!(cell.cell_struct, 2);
    assert_eq!(cell.visible_cells, [0x0101]);
    assert!(EnvCell::decode_payload(id, &b).is_err());
}
