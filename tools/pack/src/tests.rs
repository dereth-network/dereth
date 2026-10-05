//! Behaviour: none (tooling: the pack tool's pictures, records, manifest and container)

use super::*;
use dereth_dat::client_layer::ClientLayer;

fn png_bytes(width: u32, height: u32, colour: png::ColorType, data: &[u8]) -> Vec<u8> {
    let mut out = Vec::new();
    {
        let mut e = png::Encoder::new(&mut out, width, height);
        e.set_color(colour);
        e.set_depth(png::BitDepth::Eight);
        if colour == png::ColorType::Indexed {
            e.set_palette(vec![10, 20, 30, 200, 100, 50]);
        }
        let mut w = e.write_header().unwrap();
        w.write_image_data(data).unwrap();
    }
    out
}

fn gradient(width: u32, height: u32) -> Vec<u8> {
    (0..width * height)
        .flat_map(|i| {
            let (x, y) = (i % width, i / width);
            [x as u8 * 7, y as u8 * 5, (x ^ y) as u8]
        })
        .collect()
}

#[test]
fn a_picture_round_trips_through_both_record_formats() {
    let rgb = gradient(32, 32);
    let picture = read_png(
        Path::new("g.png"),
        &png_bytes(32, 32, png::ColorType::Rgb, &rgb),
        [0; 3],
    )
    .unwrap();
    assert_eq!((picture.width, picture.height), (32, 32));
    assert_eq!(picture.rgb, rgb);
    assert!(!picture.flattened);
    let id = DataId(0x0600_708F);
    let record = encode(id, &picture, Format::Rgb);
    assert_eq!(
        record.len(),
        12 + 32 * 32 * 3,
        "an id, a width, a height, the pixels"
    );
    assert_eq!(&record[..4], &id.raw().to_le_bytes());
    assert_eq!(&record[12..15], &rgb[..3], "red, green, blue");
    assert_eq!(decode(&record, Format::Rgb), Some((id, picture.clone())));
    let surface = encode(id, &picture, Format::Surface);
    assert_eq!(surface.len(), 24 + 32 * 32 * 3);
    assert_eq!(
        &surface[24..27],
        &[rgb[2], rgb[1], rgb[0]],
        "blue, green, red"
    );
    assert_eq!(decode(&surface, Format::Surface), Some((id, picture)));
    assert_eq!(decode(&record[..record.len() - 1], Format::Rgb), None);
}

#[test]
fn transparency_is_laid_on_the_background_and_an_indexed_picture_is_expanded() {
    // One opaque red pixel, one half-transparent white, one fully transparent.
    let rgba = [255, 0, 0, 255, 255, 255, 255, 128, 9, 9, 9, 0];
    let p = read_png(
        Path::new("a.png"),
        &png_bytes(3, 1, png::ColorType::Rgba, &rgba),
        [0, 0, 100],
    )
    .unwrap();
    assert!(p.flattened);
    assert_eq!(p.rgb, [255, 0, 0, 128, 128, 178, 0, 0, 100]);
    let p = read_png(
        Path::new("i.png"),
        &png_bytes(2, 1, png::ColorType::Indexed, &[1, 0]),
        [0; 3],
    )
    .unwrap();
    assert_eq!(p.rgb, [200, 100, 50, 10, 20, 30]);
    assert!(!p.flattened);
}

#[test]
fn a_manifest_reads_and_refuses_what_it_cannot_mean() {
    let m = Manifest::parse(
        "over portal.dat\nout layer.dat  # here\nbackground 0D1115\n\
         0x06006BF0 rgb b.png\n0x0600708F\trgb\ta.png\t000000\n",
    )
    .unwrap();
    assert_eq!(m.over, ContainerEra::PreTod);
    assert_eq!(m.background, [0x0D, 0x11, 0x15]);
    assert_eq!(
        m.entries.iter().map(|e| e.id).collect::<Vec<_>>(),
        [DataId(0x0600_6BF0), DataId(0x0600_708F)],
        "ascending"
    );
    assert_eq!(m.entries[1].background, Some([0; 3]));
    for bad in [
        "out x\n0x06000001 rgb a.png",
        "over portal.dat\n0x06000001 rgb a.png",
        "over portal.dat\nout x\n0x06000001 surface a.png",
        "over portal.dat\nout x\n0x06000001 rgb a.png\n0x06000001 rgb b.png",
        "over portal.dat\nout x\n0x05000001 rgb a.png",
        "over cell.dat\nout x",
        "over portal.dat\nout x\nsomething else",
    ] {
        assert!(Manifest::parse(bad).is_err(), "{bad}");
    }
}

#[test]
fn a_pack_writes_the_same_container_every_time_and_it_reads_back_as_a_client_layer() {
    let dir = std::env::temp_dir().join(format!("dereth-pack-test-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("a.png"),
        png_bytes(32, 32, png::ColorType::Rgb, &gradient(32, 32)),
    )
    .unwrap();
    std::fs::write(
        dir.join("b.png"),
        png_bytes(34, 27, png::ColorType::Rgb, &gradient(34, 27)),
    )
    .unwrap();
    let manifest = dir.join("pack.tsv");
    std::fs::write(
        &manifest,
        "over portal.dat\nout layer.dat\n0x06000002 rgb b.png\n0x06000001 rgb a.png\n",
    )
    .unwrap();
    let packed = pack(&manifest, None).unwrap();
    assert_eq!(packed.ids, [DataId(0x0600_0001), DataId(0x0600_0002)]);
    assert!(
        is_current(&manifest).unwrap(),
        "the same input, the same bytes"
    );
    let layer = ClientLayer::from_file(dereth_dat::DatFile::open(&packed.out).unwrap()).unwrap();
    assert_eq!(layer.era(), ContainerEra::PreTod);
    let (id, picture) = decode(&layer.read(DataId(0x0600_0002)).unwrap(), Format::Rgb).unwrap();
    assert_eq!(id, DataId(0x0600_0002));
    assert_eq!((picture.width, picture.height), (34, 27));
    assert_eq!(picture.rgb, gradient(34, 27));
    // A changed picture is a stale container.
    std::fs::write(
        dir.join("a.png"),
        png_bytes(32, 32, png::ColorType::Rgb, &[7; 32 * 32 * 3]),
    )
    .unwrap();
    assert!(!is_current(&manifest).unwrap());
    let _ = std::fs::remove_dir_all(&dir);
}

/// The client's own records over the older portal, as the client builds them in.
fn classic_manifest() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../core/client-runtime/assets/classic-portal/pack.tsv")
}

#[test]
fn the_classic_portal_layer_the_client_builds_in_is_what_its_manifest_makes() {
    let manifest = classic_manifest();
    assert!(
        is_current(&manifest).unwrap(),
        "the container is stale: cargo run -p dereth-pack -- {}",
        manifest.display()
    );
    // Each record holds its picture's pixels.
    let text = std::fs::read_to_string(&manifest).unwrap();
    let m = Manifest::parse(&text).unwrap();
    let dir = manifest.parent().unwrap();
    let layer =
        ClientLayer::from_file(dereth_dat::DatFile::open(&dir.join(&m.out)).unwrap()).unwrap();
    assert_eq!(
        layer.ids(),
        m.entries.iter().map(|e| e.id).collect::<Vec<_>>()
    );
    for e in &m.entries {
        let path = dir.join(&e.picture);
        let png = read_png(&path, &std::fs::read(&path).unwrap(), m.background).unwrap();
        let (id, picture) = decode(&layer.read(e.id).unwrap(), e.format).unwrap();
        assert_eq!(id, e.id);
        assert_eq!(picture.rgb, png.rgb, "{}", e.picture.display());
    }
}
