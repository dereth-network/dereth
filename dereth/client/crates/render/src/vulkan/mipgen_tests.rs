use super::*;

// Shared source is instantiated with this backend's types and assertion helpers.
#[allow(clippy::duplicate_mod)]
#[path = "../tests/mipgen.rs"]
mod shared;

fn mip_test_device(width: u32, height: u32) -> Option<Gpu> {
    test_device(width, height)
}

fn generated_levels_message(source: crate::PixelFormatId) -> String {
    format!("a synthetic {source:?} texture decoded by the production path must receive generated mip levels")
}

const LAYOUT_MESSAGE: &str = "readback reports actual device layout, not source provenance";

/// Behaviour: rendering.textures.runtime-mips-preserve-channels-and-keep-their-owners-alive
#[test]
fn runtime_mips_preserve_every_channel_and_handle_rectangles_and_odd_extents() {
    crate::device_thread::run(move || {
        shared::runtime_mips_preserve_every_channel_and_handle_rectangles_and_odd_extents();
    });
}

#[test]
fn runtime_mips_leave_provided_bc_and_non_imgtex_uploads_unchanged() {
    crate::device_thread::run(move || {
        shared::runtime_mips_leave_provided_bc_and_non_imgtex_uploads_unchanged();
    });
}

/// Behaviour: rendering.textures.runtime-mips-preserve-channels-and-keep-their-owners-alive
#[test]
fn runtime_mips_keep_cache_links_and_open_frame_resources_alive() {
    crate::device_thread::run(move || {
        shared::runtime_mips_keep_cache_links_and_open_frame_resources_alive();
    });
}

#[test]
fn compressed_mips_preserve_subresources_owners_and_open_frame_links() {
    crate::device_thread::run(move || {
        shared::compressed_mips_preserve_subresources_owners_and_open_frame_links();
    });
}

#[test]
fn compressed_bc2_mips_preserve_subresources_owners_and_open_frame_links() {
    crate::device_thread::run(move || {
        shared::compressed_bc2_mips_preserve_subresources_owners_and_open_frame_links();
    });
}

#[test]
fn compressed_bc3_mips_preserve_subresources_owners_and_open_frame_links() {
    crate::device_thread::run(move || {
        shared::compressed_bc3_mips_preserve_subresources_owners_and_open_frame_links();
    });
}

#[test]
fn premultiplied_mips_dxt2_source_decoder_resource_and_minified_draw() {
    crate::device_thread::run(move || {
        shared::premultiplied_mips_dxt2_source_decoder_resource_and_minified_draw();
    });
}

#[test]
fn premultiplied_mips_dxt4_source_decoder_resource_and_minified_draw() {
    crate::device_thread::run(move || {
        shared::premultiplied_mips_dxt4_source_decoder_resource_and_minified_draw();
    });
}

#[test]
fn premultiplied_mips_dxt2_owners_and_open_frame_links() {
    crate::device_thread::run(move || {
        shared::premultiplied_mips_dxt2_owners_and_open_frame_links();
    });
}

#[test]
fn premultiplied_mips_dxt4_owners_and_open_frame_links() {
    crate::device_thread::run(move || {
        shared::premultiplied_mips_dxt4_owners_and_open_frame_links();
    });
}
