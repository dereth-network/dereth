//! Behaviour: none (tooling: a static glTF scene read for a world builder)
//! One triangle in a scene written by the test: its part and frame in the client's axes, the plain
//! material every scene gets last, a resource outside the builder's folder refused, and a base
//! color picture tinted.

use std::path::{Path, PathBuf};

use base64::Engine as _;
use dereth_gltf::{base_color_pixels, scene, Material};

/// A folder of the test's own under the system's temporary folder, removed when dropped.
struct Folder(PathBuf);

impl Folder {
    fn new(tag: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("dereth-gltf-{tag}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        Self(dir)
    }

    fn write(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        let p = self.0.join(rel);
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(&p, bytes).unwrap();
        p
    }
}

impl Drop for Folder {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

/// Three corners of a right triangle in the scene's own axes (y up), as little-endian floats.
fn corners() -> Vec<u8> {
    [[0.0f32, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]]
        .iter()
        .flatten()
        .flat_map(|v| v.to_le_bytes())
        .collect()
}

/// A scene of one node, `name`, moved by `(1, 2, 3)`, whose mesh is the triangle in buffer `uri`.
fn gltf(name: &str, uri: &str) -> String {
    format!(
        r#"{{"asset":{{"version":"2.0"}},"scene":0,"scenes":[{{"nodes":[0]}}],
        "nodes":[{{"name":"{name}","mesh":0,"translation":[1,2,3]}}],
        "meshes":[{{"primitives":[{{"attributes":{{"POSITION":0}}}}]}}],
        "buffers":[{{"uri":"{uri}","byteLength":36}}],
        "bufferViews":[{{"buffer":0,"byteOffset":0,"byteLength":36}}],
        "accessors":[{{"bufferView":0,"componentType":5126,"count":3,"type":"VEC3",
        "min":[0,0,0],"max":[1,1,0]}}]}}"#
    )
}

#[test]
fn a_triangle_s_part_stands_in_the_client_s_axes_and_the_scene_ends_with_a_plain_material() {
    let f = Folder::new("triangle");
    let data = format!(
        "data:application/octet-stream;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(corners())
    );
    let path = f.write("models/arch.gltf", gltf("arch", &data).as_bytes());
    let s = scene(&f.0, &path).unwrap();
    assert_eq!(s.parts.len(), 1);
    let part = &s.parts[0];
    assert_eq!(part.name, "arch");
    let o = part.frame.origin;
    assert_eq!((o.x, o.y, o.z), (1.0, -3.0, 2.0), "(x, y, z) is (x, -z, y)");
    let corners: Vec<_> = part.world_vertices().map(|v| (v.x, v.y, v.z)).collect();
    assert_eq!(
        corners,
        [(1.0, -3.0, 2.0), (2.0, -3.0, 2.0), (1.0, -3.0, 3.0)]
    );
    let p = &part.primitives[0];
    assert_eq!(
        p.indices,
        [0, 1, 2],
        "unindexed triangles are numbered in order"
    );
    assert_eq!(
        p.material, 0,
        "the plain material, the scene having none of its own"
    );
    assert_eq!(
        s.materials,
        [Material {
            color: [1.0; 4],
            image: None,
            double_sided: false,
            opaque: true,
        }]
    );
}

#[test]
fn a_buffer_outside_the_builder_s_folder_is_refused() {
    let f = Folder::new("outside");
    f.write("outside.bin", &corners());
    let path = f.write(
        "pack/models/arch.gltf",
        gltf("arch", "../../outside.bin").as_bytes(),
    );
    let e = scene(&f.0.join("pack"), &path).unwrap_err();
    assert!(e.contains("inside its pack"), "{e}");
    f.write("pack/models/arch.bin", &corners());
    let path = f.write("pack/models/arch.gltf", gltf("arch", "arch.bin").as_bytes());
    assert_eq!(scene(&f.0.join("pack"), &path).unwrap().parts.len(), 1);
    assert!(scene(Path::new(&f.0.join("pack")), &f.0.join("pack/none.gltf")).is_err());
}

#[test]
fn a_base_color_picture_is_tinted_by_its_factor_and_kept_opaque_when_the_material_is() {
    let mut png = Vec::new();
    {
        let mut e = png::Encoder::new(&mut png, 1, 1);
        e.set_color(png::ColorType::Rgba);
        e.set_depth(png::BitDepth::Eight);
        let mut w = e.write_header().unwrap();
        w.write_image_data(&[255, 255, 255, 128]).unwrap();
    }
    let (w, h, px) = base_color_pixels(&png, [1.0, 0.0, 1.0, 1.0], false).unwrap();
    assert_eq!((w, h), (1, 1));
    assert_eq!(px, [255, 0, 255, 128], "blue, green, red, alpha");
    let (_, _, px) = base_color_pixels(&png, [1.0; 4], true).unwrap();
    assert_eq!(px[3], 255);
    assert!(base_color_pixels(b"not a picture", [1.0; 4], true)
        .unwrap_err()
        .contains("PNG"));
}
