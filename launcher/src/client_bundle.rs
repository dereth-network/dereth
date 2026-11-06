//! A game client's own application bundle, on macOS.
//!
//! macOS names and draws a running program by the bundle around it: the `.app` folder its executable
//! is in, and that folder's `Info.plist`. A client copied out of the launcher's bundle needs one of
//! its own, or the Dock shows a folder's icon; a client unpacked from a game's release has the
//! launcher's, so the Dock would show the launcher's. So the launcher makes the bundle a client's
//! own ([`dress`]) wherever it installs one: that client's name, and that client's icon, which the
//! launcher carries for each game (`tauri.macos.conf.json`).
//!
//! Elsewhere there is nothing to do: the callers ask only on macOS. These are plain file operations,
//! so the tests run everywhere.

use std::path::{Path, PathBuf};

use dereth_launch::components::Product;

/// Where a launcher's bundle keeps each client's icon, relative to the launcher's folder.
const ICONS: &str = "../Resources/clients";

/// What's left in a game's release file that is the launcher's, not the client's.
const THE_LAUNCHERS: [&str; 7] = [
    "MacOS/dereth",
    "_CodeSignature",
    "Resources/Assets.car",
    "Resources/Network.icns",
    "Resources/auberean_friends.dll",
    "Resources/clients",
    "Resources/launcher-defaults.json",
];

/// The `.app` folder of the program at `executable`, when it is in one.
#[must_use]
pub fn app_of(executable: &Path) -> Option<PathBuf> {
    let macos = executable.parent()?;
    let contents = macos.parent()?;
    let app = contents.parent()?;
    (macos.ends_with("MacOS")
        && contents.ends_with("Contents")
        && app.extension().is_some_and(|e| e == "app"))
    .then(|| app.to_owned())
}

/// The bundle's identifier: a client's own, so macOS keeps it apart from the launcher.
const fn identifier(product: Product) -> &'static str {
    match product {
        Product::Dereth => "network.dereth.dereth-client",
        Product::Auberean => "network.dereth.auberean",
        Product::Empyrean | Product::Lyceum | Product::Yalain | Product::Athenaeum => {
            "network.dereth.program"
        }
    }
}

/// The `Info.plist` of `product`'s client at `version`. `composed`: the bundle holds the compiled
/// layered icon macOS 26 draws, as well as the `.icns` every earlier one reads.
fn info_plist(product: Product, version: &str, icon: bool, composed: bool) -> String {
    let title = product.title();
    let mut icon_keys = String::new();
    if icon {
        icon_keys.push_str(&format!(
            "\n\t<key>CFBundleIconFile</key>\n\t<string>{title}</string>"
        ));
    }
    if composed {
        icon_keys.push_str(&format!(
            "\n\t<key>CFBundleIconName</key>\n\t<string>{title}</string>"
        ));
    }
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>{title}</string>
	<key>CFBundleDisplayName</key>
	<string>{title}</string>
	<key>CFBundleIdentifier</key>
	<string>{id}</string>
	<key>CFBundleExecutable</key>
	<string>{program}</string>{icon_keys}
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>11.0</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.role-playing-games</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#,
        id = identifier(product),
        program = product.program(),
    )
}

/// Whether the bundle around `executable` isn't the client's own yet: a release's, which is the
/// launcher's. Not in a bundle at all is nothing to make one of.
#[must_use]
pub fn needs_dressing(executable: &Path, product: Product) -> bool {
    let Some(app) = app_of(executable) else {
        return false;
    };
    let wanted = format!("<string>{}</string>", identifier(product));
    !std::fs::read_to_string(app.join("Contents/Info.plist")).is_ok_and(|p| p.contains(&wanted))
}

/// Make the bundle around `executable` the client `product`'s own, at `version`: what's the
/// launcher's goes, the icon from the launcher in `launcher_dir` goes in, and `Info.plist` is written
/// last. Not in a bundle is left alone. A launcher without the icon makes a bundle without one.
///
/// # Errors
/// A file that could not be changed. The client still runs; it's only drawn wrongly.
pub fn dress(
    executable: &Path,
    product: Product,
    version: &str,
    launcher_dir: &Path,
) -> Result<(), String> {
    let Some(app) = app_of(executable) else {
        return Ok(());
    };
    let contents = app.join("Contents");
    let failed = |p: &Path, e: std::io::Error| format!("{}: {e}", p.display());
    for name in THE_LAUNCHERS {
        let path = contents.join(name);
        let gone = match std::fs::symlink_metadata(&path) {
            Err(_) => Ok(()),
            Ok(m) if m.is_dir() => std::fs::remove_dir_all(&path),
            Ok(_) => std::fs::remove_file(&path),
        };
        gone.map_err(|e| failed(&path, e))?;
    }
    let title = product.title();
    let resources = contents.join("Resources");
    std::fs::create_dir_all(&resources).map_err(|e| failed(&resources, e))?;
    let icons = launcher_dir.join(ICONS).join(title);
    let found = |name: &str| -> Result<bool, String> {
        let from = icons.join(name);
        if !from.is_file() {
            return Ok(false);
        }
        let to = resources.join(name);
        std::fs::copy(&from, &to).map_err(|e| failed(&to, e))?;
        Ok(true)
    };
    let icon = found(&format!("{title}.icns"))?;
    let composed = found("Assets.car")?;
    let pkg = contents.join("PkgInfo");
    std::fs::write(&pkg, "APPL????").map_err(|e| failed(&pkg, e))?;
    let plist = contents.join("Info.plist");
    std::fs::write(&plist, info_plist(product, version, icon, composed))
        .map_err(|e| failed(&plist, e))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launcher's folder with `product`'s icon in its bundle's resources, as its build lays it out.
    fn launcher_with_icon(root: &Path, product: Product) -> PathBuf {
        let here = root.join("Launcher.app/Contents/MacOS");
        let icons = here.join(ICONS).join(product.title());
        std::fs::create_dir_all(&here).unwrap();
        std::fs::create_dir_all(&icons).unwrap();
        std::fs::write(icons.join(format!("{}.icns", product.title())), b"flat").unwrap();
        std::fs::write(icons.join("Assets.car"), b"layered").unwrap();
        here
    }

    fn file(path: &Path, bytes: &[u8]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    /// The Dock draws a client by this bundle: it has to be its own game's name and icon. The
    /// owner's report that started this was a folder's icon, from a copy that had no bundle.
    #[test]
    fn a_copied_client_gets_a_bundle_with_its_own_name_and_icon() {
        let temp = tempfile::tempdir().unwrap();
        let here = launcher_with_icon(temp.path(), Product::Dereth);
        let exe = temp.path().join("Dereth.app/Contents/MacOS/dereth-client");
        file(&exe, b"client");

        assert!(needs_dressing(&exe, Product::Dereth));
        dress(&exe, Product::Dereth, "0.4.9", &here).unwrap();
        assert!(!needs_dressing(&exe, Product::Dereth));

        let contents = temp.path().join("Dereth.app/Contents");
        let plist = std::fs::read_to_string(contents.join("Info.plist")).unwrap();
        for wanted in [
            "<key>CFBundleExecutable</key>\n\t<string>dereth-client</string>",
            "<key>CFBundleName</key>\n\t<string>Dereth</string>",
            "<key>CFBundleIconFile</key>\n\t<string>Dereth</string>",
            "<key>CFBundleIconName</key>\n\t<string>Dereth</string>",
            "<key>CFBundleShortVersionString</key>\n\t<string>0.4.9</string>",
            "<key>NSHighResolutionCapable</key>\n\t<true/>",
        ] {
            assert!(plist.contains(wanted), "{wanted}\n{plist}");
        }
        assert!(!plist.contains("Network"), "{plist}");
        assert_eq!(
            std::fs::read(contents.join("Resources/Dereth.icns")).unwrap(),
            b"flat"
        );
        assert_eq!(
            std::fs::read(contents.join("Resources/Assets.car")).unwrap(),
            b"layered"
        );
    }

    /// Each game's client wears its own game's name and icon, never the other's.
    #[test]
    fn auberean_s_client_is_named_and_drawn_as_auberean() {
        let temp = tempfile::tempdir().unwrap();
        let here = launcher_with_icon(temp.path(), Product::Auberean);
        let exe = temp.path().join("A.app/Contents/MacOS/auberean");
        file(&exe, b"client");
        dress(&exe, Product::Auberean, "0.2.0", &here).unwrap();
        let contents = temp.path().join("A.app/Contents");
        let plist = std::fs::read_to_string(contents.join("Info.plist")).unwrap();
        assert!(plist.contains("<string>Auberean</string>"), "{plist}");
        assert!(plist.contains("<string>auberean</string>"), "{plist}");
        assert!(!plist.contains("Dereth"), "{plist}");
        assert!(contents.join("Resources/Auberean.icns").is_file());
    }

    /// A client unpacked from a game's release comes in the launcher's bundle. The launcher's parts
    /// go, so the bundle isn't the launcher's in any way, and the client's parts all stay.
    #[test]
    fn a_downloaded_client_s_bundle_stops_being_the_launcher_s() {
        let temp = tempfile::tempdir().unwrap();
        let here = launcher_with_icon(temp.path(), Product::Dereth);
        let contents = temp.path().join("Dereth.app/Contents");
        let exe = contents.join("MacOS/dereth-client");
        file(&exe, b"client");
        file(&contents.join("MacOS/dereth"), b"launcher");
        file(&contents.join("MacOS/dereth-plugin"), b"tool");
        file(&contents.join("Frameworks/libMoltenVK.dylib"), b"library");
        file(&contents.join("Info.plist"), b"the launcher's");
        file(&contents.join("_CodeSignature/CodeResources"), b"seal");
        file(&contents.join("Resources/Network.icns"), b"network");
        file(&contents.join("Resources/Assets.car"), b"network layered");
        file(&contents.join("Resources/clients/Dereth/Dereth.icns"), b"x");
        file(&contents.join("Resources/launcher-defaults.json"), b"{}");
        file(&contents.join("Resources/auberean_friends.dll"), b"library");
        file(&contents.join("Resources/NOTICE.txt"), b"notices");

        assert!(needs_dressing(&exe, Product::Dereth));
        dress(&exe, Product::Dereth, "0.4.9", &here).unwrap();

        for gone in [
            "MacOS/dereth",
            "_CodeSignature",
            "Resources/Network.icns",
            "Resources/auberean_friends.dll",
            "Resources/clients",
            "Resources/launcher-defaults.json",
        ] {
            assert!(!contents.join(gone).exists(), "{gone}");
        }
        for kept in [
            "MacOS/dereth-client",
            "MacOS/dereth-plugin",
            "Frameworks/libMoltenVK.dylib",
            "Resources/NOTICE.txt",
        ] {
            assert!(contents.join(kept).is_file(), "{kept}");
        }
        assert_eq!(
            std::fs::read(contents.join("Resources/Assets.car")).unwrap(),
            b"layered"
        );
    }

    /// A launcher built with no icons (no Xcode) still names the client; its plist promises no icon
    /// it doesn't hold.
    #[test]
    fn a_launcher_without_the_icon_makes_a_bundle_that_names_none() {
        let temp = tempfile::tempdir().unwrap();
        let exe = temp.path().join("Dereth.app/Contents/MacOS/dereth-client");
        file(&exe, b"client");
        dress(&exe, Product::Dereth, "0.4.9", temp.path()).unwrap();
        let plist =
            std::fs::read_to_string(temp.path().join("Dereth.app/Contents/Info.plist")).unwrap();
        assert!(plist.contains("<string>dereth-client</string>"), "{plist}");
        assert!(!plist.contains("CFBundleIcon"), "{plist}");
    }

    /// A program that isn't in a `.app` has nothing to dress, and nothing is written around it.
    #[test]
    fn a_client_outside_a_bundle_is_left_alone() {
        let temp = tempfile::tempdir().unwrap();
        for exe in [
            temp.path().join("0.4.9/dereth-client"),
            temp.path().join("0.4.9/Contents/MacOS/dereth-client"),
        ] {
            file(&exe, b"client");
            assert_eq!(app_of(&exe), None);
            assert!(!needs_dressing(&exe, Product::Dereth));
            dress(&exe, Product::Dereth, "0.4.9", temp.path()).unwrap();
        }
        assert!(!temp.path().join("0.4.9/Contents/Info.plist").exists());
    }
}
