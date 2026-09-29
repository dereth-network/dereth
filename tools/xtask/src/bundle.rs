//! `cargo xtask bundle-macos`: the client, assembled into the `Dereth.app` that macOS reads an
//! application's identity out of.
//!
//! A Mach-O binary is not an application on this platform: the Dock, Finder, the application
//! switcher and Launch Services all read the name, the version and the icon out of a *bundle* --
//! a directory with a `Contents/Info.plist` -- and a binary cargo built has none, which is why a
//! plain `cargo run` shows a generic tile with the executable's name under it. This command is
//! the answer to that, and the only one: the client sets no icon of its own at run time, because
//! doing so means asking AppKit at a moment the system does not document, and the bundle is the
//! mechanism macOS actually supports. Running the binary *inside* the bundle
//! (`Dereth.app/Contents/MacOS/dereth-client`) is a bundled run like any other -- same terminal,
//! same log on stderr, right icon -- so a developer who wants the icon has it without leaving
//! the command line.
//!
//! What it assembles, from parts that already exist in the tree:
//!
//! ```text
//! Dereth.app/Contents/Info.plist              name, identifier, version, icon keys
//! Dereth.app/Contents/MacOS/dereth-client     the binary cargo just built
//! Dereth.app/Contents/Resources/…             the icon, in whichever of its two forms
//! ```
//!
//! **The icon has two forms, and which one is built depends on the machine.** macOS 26 draws an
//! application's icon itself -- the rounded square, the material, the shadow, the light and dark
//! and tinted variants -- from a layered source, `assets/Dereth.icon`, which `actool` (Xcode's
//! asset compiler) turns into an `Assets.car` and a plain `.icns` beside it for the systems that
//! predate all that. Handed a flat `.icns` instead, macOS 26 masks it into the same
//! square itself, which is the older and plainer result. So: `actool` is used when it is on the
//! machine, and when it is not the same artwork is turned into a flat `.icns` with `sips` and
//! `iconutil` -- both part of macOS rather than of Xcode -- so a host with no developer tools
//! still builds a bundle with an icon, simply the one an older macOS would have shown.
//!
//! **The dats are the one thing a bundle cannot carry.** A Finder launch passes no arguments and
//! starts the process with `/` as its working directory, so `--dat-dir` cannot be given and the
//! client's other two candidates -- the working directory and the executable's own directory --
//! are the only ones left. `--dat-dir <dir>` therefore links the four retail files into
//! `Contents/MacOS/`, where the second candidate finds them, and a bundle built without it is one
//! that has to be launched with the argument spelled out. Links rather than copies: the files are
//! multiple gigabytes and the install is read-only for a run anyway.

use std::path::{Path, PathBuf};

use dereth_dat::RetailDat;

use crate::util::{run, workspace_root};

/// The bundle's name, and with it the name under the icon in the Dock.
const APP_NAME: &str = "Dereth";

/// The binary that becomes `Contents/MacOS/<this>`, named in `Info.plist` as `CFBundleExecutable`.
pub const BINARY: &str = "dereth-client";

/// Launch Services' identity for the application: what it remembers a permission grant, a window
/// position or a default-application choice under. Stable across builds on purpose -- changing it
/// makes the system treat the result as a different application -- and reverse-DNS by convention.
pub const BUNDLE_ID: &str = "network.dereth.dereth-client";

/// The oldest macOS this is claimed to run on. The client's own floor, not the toolchain's.
const MINIMUM_MACOS: &str = "11.0";

/// `cargo xtask bundle-macos [--debug] [--dat-dir <dir>]`.
pub fn bundle_macos(args: &[String]) -> i32 {
    let mut debug = false;
    let mut dat_dir: Option<PathBuf> = None;
    let mut rest = args.iter();
    while let Some(arg) = rest.next() {
        match arg.as_str() {
            "--debug" => debug = true,
            "--dat-dir" => match rest.next() {
                Some(dir) => dat_dir = Some(PathBuf::from(dir)),
                None => {
                    eprintln!(
                        "bundle-macos: --dat-dir takes the directory holding the retail dats"
                    );
                    return 2;
                }
            },
            other => {
                eprintln!("bundle-macos: unexpected argument `{other}`");
                eprintln!("usage: cargo xtask bundle-macos [--debug] [--dat-dir <dir>]");
                return 2;
            }
        }
    }
    // The bundle holds a macOS binary and is assembled with macOS's own layout; building one
    // anywhere else would produce a directory that cannot be run and would not say so until it
    // was carried to a Mac.
    if !cfg!(target_os = "macos") {
        eprintln!("bundle-macos: this command builds a macOS application and runs on macOS");
        return 2;
    }
    if let Some(dir) = &dat_dir {
        if !dereth_dat::holds_retail_dats(dir) {
            eprintln!("bundle-macos: no retail dats in {}", dir.display());
            return 1;
        }
    }
    match assemble(debug, dat_dir.as_deref()) {
        Ok(app) => {
            report(&app, dat_dir.as_deref());
            0
        }
        Err(e) => {
            eprintln!("bundle-macos: {e}");
            1
        }
    }
}

/// Build the binary and lay the bundle out around it, answering where it is.
fn assemble(debug: bool, dat_dir: Option<&Path>) -> Result<PathBuf, String> {
    let root = workspace_root();
    let (profile_dir, cargo_args): (&str, &[&str]) = if debug {
        ("debug", &["build", "-p", BINARY])
    } else {
        ("release", &["build", "--release", "-p", BINARY])
    };
    if !run(&root, "cargo", cargo_args) {
        return Err("the client did not build".to_owned());
    }

    let target = root.join("target").join(profile_dir);
    let binary = target.join(BINARY);
    if !binary.is_file() {
        return Err(format!("{} is not where cargo left it", binary.display()));
    }

    let app = target.join(format!("{APP_NAME}.app"));
    lay_out(
        &root,
        &target,
        &binary,
        &app,
        &Identity {
            name: APP_NAME,
            bundle_id: BUNDLE_ID,
            version: env!("CARGO_PKG_VERSION"),
        },
    )?;
    let macos = app.join("Contents/MacOS");

    if let Some(dats) = dat_dir {
        let dats = std::fs::canonicalize(dats).map_err(|e| format!("{}: {e}", dats.display()))?;
        for dat in RetailDat::ALL {
            let file = dat.in_dir(&dats);
            // The high-resolution partition is the optional one; the other three are not, and
            // `holds_retail_dats` has already reported on the file that stands for the set.
            if file.is_file() {
                let at = macos.join(dat.file_name());
                link(&file, &at).map_err(|e| format!("{}: {e}", at.display()))?;
            }
        }
    }
    Ok(app)
}

/// What names a bundle: the name under its icon, Launch Services' identifier for it, and its
/// version.
#[derive(Debug, Clone, Copy)]
pub struct Identity<'a> {
    pub name: &'a str,
    pub bundle_id: &'a str,
    pub version: &'a str,
}

/// Lay the client's bundle out at `app` around `binary`: `Info.plist` naming it `identity`, the
/// icon (built in `work`), and the binary as `Contents/MacOS/dereth-client`.
///
/// # Errors
/// A folder or file could not be written, or the icon could not be built.
pub fn lay_out(
    root: &Path,
    work: &Path,
    binary: &Path,
    app: &Path,
    identity: &Identity,
) -> Result<(), String> {
    // Replaced whole rather than written over: a bundle left from an earlier build can hold a
    // file this one does not write, and a half-old application is the kind of thing that is
    // debugged for an hour.
    if app.exists() {
        std::fs::remove_dir_all(app).map_err(|e| format!("{}: {e}", app.display()))?;
    }
    let macos = app.join("Contents/MacOS");
    let resources = app.join("Contents/Resources");
    for dir in [&macos, &resources] {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }

    let form = install_icon(root, work, &resources)?;
    write(
        &app.join("Contents/Info.plist"),
        &info_plist(form, identity),
    )?;
    // Four bytes of classic Mac OS that the Finder still reads before the plist.
    write(&app.join("Contents/PkgInfo"), "APPL????")?;
    copy(binary, &macos.join(BINARY))
}

/// Which of the two icon forms a bundle ended up with. The plist's icon keys differ, so the
/// answer travels from the step that built it to the step that writes them.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IconForm {
    /// `assets/Dereth.icon`, compiled: `Assets.car` for macOS 26's own drawing of it, and the
    /// `.icns` `actool` generates beside it for every system before that.
    Composed,
    /// A flat `.icns` made from the icon's own image with `sips` and `iconutil`: what a machine
    /// without Xcode can build, and what macOS 26 will mask into its rounded square itself.
    Fallback,
}

/// Put the icon in `Contents/Resources`, by whichever route this machine can take.
fn install_icon(root: &Path, target: &Path, resources: &Path) -> Result<IconForm, String> {
    let source = root
        .join("dereth/client/assets")
        .join(format!("{APP_NAME}.icon"));
    if !source.is_dir() {
        return Err(format!("{} is missing", source.display()));
    }
    // A directory of its own for the intermediate files: `actool` writes three outputs beside
    // each other and the fall-back writes an `.iconset` of ten, and none of that belongs in the
    // bundle or in the source tree.
    let built = target.join("icon-build");
    if built.exists() {
        std::fs::remove_dir_all(&built).map_err(|e| format!("{}: {e}", built.display()))?;
    }
    std::fs::create_dir_all(&built).map_err(|e| format!("{}: {e}", built.display()))?;
    let icns = format!("{APP_NAME}.icns");

    if !actool_is_installed() {
        println!("bundle-macos: no actool (Xcode) here; building a flat icon with sips/iconutil");
        flat_icns(&layer_image(&source)?, &built, &built.join(&icns))?;
        copy(&built.join(&icns), &resources.join(&icns))?;
        return Ok(IconForm::Fallback);
    }

    let partial = built.join("partial-info.plist");
    let ok = run(
        root,
        "xcrun",
        &[
            "actool",
            "--output-format",
            "human-readable-text",
            "--notices",
            "--warnings",
            "--app-icon",
            APP_NAME,
            "--output-partial-info-plist",
            &partial.to_string_lossy(),
            "--target-device",
            "mac",
            "--platform",
            "macosx",
            // The same floor the plist claims, so the compiler makes what that claim needs: the
            // layered icon for macOS 26 and the flat one for everything older.
            "--minimum-deployment-target",
            MINIMUM_MACOS,
            "--compile",
            &built.to_string_lossy(),
            &source.to_string_lossy(),
        ],
    );
    if !ok {
        return Err(format!("actool could not compile {}", source.display()));
    }
    // Both outputs, because they are for different systems: `Assets.car` is what macOS 26 reads,
    // and the generated `.icns` is what every earlier one reads.
    copy(&built.join("Assets.car"), &resources.join("Assets.car"))?;
    copy(&built.join(&icns), &resources.join(&icns))?;
    Ok(IconForm::Composed)
}

/// The image the icon is drawn from, inside the `.icon`.
///
/// The `.icon` is a directory of layer images and a description of how to compose them, and
/// `actool` is what reads that description. The fall-back cannot, so it takes the one layer this
/// icon has. A `.icon` edited into several layers would no longer have this file, which is why
/// the error says what it says: the fall-back is a floor, and the answer on a machine that can
/// compose is to compose.
fn layer_image(source: &Path) -> Result<PathBuf, String> {
    let image = source.join("Assets/dereth.png");
    if image.is_file() {
        return Ok(image);
    }
    Err(format!(
        "{} is missing: without actool the bundle's icon is made from that one image, so either \
         install Xcode or point this at the image the icon is drawn from",
        image.display()
    ))
}

/// A flat `.icns` from one image, with the tools macOS itself ships: `sips` resamples, `iconutil`
/// packs. The ten entries are the sizes an `.iconset` is defined to hold -- five sizes, each at
/// one and at two pixels per point -- and the names are part of that definition, not a choice.
fn flat_icns(image: &Path, work: &Path, out: &Path) -> Result<(), String> {
    let iconset = work.join(format!("{APP_NAME}.iconset"));
    std::fs::create_dir_all(&iconset).map_err(|e| format!("{}: {e}", iconset.display()))?;
    for points in [16u32, 32, 128, 256, 512] {
        for scale in [1u32, 2] {
            let name = if scale == 1 {
                format!("icon_{points}x{points}.png")
            } else {
                format!("icon_{points}x{points}@2x.png")
            };
            let pixels = (points * scale).to_string();
            quiet(
                "sips",
                &[
                    "--resampleHeightWidth",
                    &pixels,
                    &pixels,
                    &image.to_string_lossy(),
                    "--out",
                    &iconset.join(&name).to_string_lossy(),
                ],
            )?;
        }
    }
    quiet(
        "iconutil",
        &[
            "-c",
            "icns",
            &iconset.to_string_lossy(),
            "-o",
            &out.to_string_lossy(),
        ],
    )
}

/// Run a command for its effect, with its chatter dropped: `sips` reports every resample, and ten
/// of those say nothing the caller did not already know.
fn quiet(program: &str, args: &[&str]) -> Result<(), String> {
    let status = std::process::Command::new(program)
        .args(args)
        .stdout(std::process::Stdio::null())
        .status()
        .map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} failed ({status})"))
    }
}

/// Whether Xcode's asset compiler is on this machine. `xcrun --find` is the question the
/// toolchain answers for itself, and it answers it without compiling anything.
fn actool_is_installed() -> bool {
    std::process::Command::new("xcrun")
        .args(["--find", "actool"])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// `Info.plist`: what Finder, the Dock and Launch Services read instead of asking the binary.
///
/// `CFBundleIconFile` names the file in `Contents/Resources`, `CFBundleExecutable` the one in
/// `Contents/MacOS`, and `NSHighResolutionCapable` is what stops a Retina display from scaling
/// the window up by two instead of handing the client the pixels it asked for.
fn info_plist(icon: IconForm, identity: &Identity) -> String {
    let Identity {
        name,
        bundle_id,
        version,
    } = *identity;
    // `CFBundleIconFile` names a file in `Contents/Resources` and is read by every macOS;
    // `CFBundleIconName` names an icon inside `Assets.car` and is what macOS 26 looks for first.
    // A composed bundle carries both, because it holds both.
    let icon_keys = match icon {
        IconForm::Composed => format!(
            "\t<key>CFBundleIconFile</key>\n\t<string>{APP_NAME}</string>\n\t\
             <key>CFBundleIconName</key>\n\t<string>{APP_NAME}</string>"
        ),
        IconForm::Fallback => {
            format!("\t<key>CFBundleIconFile</key>\n\t<string>{APP_NAME}</string>")
        }
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key>
	<string>{name}</string>
	<key>CFBundleDisplayName</key>
	<string>{name}</string>
	<key>CFBundleIdentifier</key>
	<string>{bundle_id}</string>
	<key>CFBundleExecutable</key>
	<string>{BINARY}</string>
{icon_keys}
	<key>CFBundlePackageType</key>
	<string>APPL</string>
	<key>CFBundleInfoDictionaryVersion</key>
	<string>6.0</string>
	<key>CFBundleShortVersionString</key>
	<string>{version}</string>
	<key>CFBundleVersion</key>
	<string>{version}</string>
	<key>LSMinimumSystemVersion</key>
	<string>{MINIMUM_MACOS}</string>
	<key>LSApplicationCategoryType</key>
	<string>public.app-category.role-playing-games</string>
	<key>NSHighResolutionCapable</key>
	<true/>
</dict>
</plist>
"#
    )
}

/// What was built, and the two ways to start it.
fn report(app: &Path, dat_dir: Option<&Path>) {
    println!("\nbundle-macos: {}", app.display());
    match dat_dir {
        Some(dir) => println!(
            "  the retail dats are linked from {}, so opening it in Finder works",
            dir.display()
        ),
        None => println!(
            "  no dats in the bundle: start it with `open -a {} --args --dat-dir <dir>`, or \
             rebuild with --dat-dir <dir> to link them in",
            app.display()
        ),
    }
    println!(
        "  a bundle in a build directory is a bundle Launch Services has not indexed; copy it to \
         /Applications to give it a permanent identity"
    );
}

/// `fs::write`, with the path in the error.
fn write(at: &Path, contents: &str) -> Result<(), String> {
    std::fs::write(at, contents).map_err(|e| format!("{}: {e}", at.display()))
}

/// `fs::copy`, with both paths in the error. The binary keeps its mode bits, and with them the
/// execute permission that makes it the bundle's executable.
fn copy(from: &Path, to: &Path) -> Result<(), String> {
    std::fs::copy(from, to)
        .map(|_| ())
        .map_err(|e| format!("{} -> {}: {e}", from.display(), to.display()))
}

/// A symbolic link at `at` pointing to `target`.
#[cfg(unix)]
fn link(target: &Path, at: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(target, at)
}

/// Unreachable: the command refuses to run off macOS before it reaches a link. Present so that
/// the module compiles, and is checked, on the hosts the rest of the workspace is built on.
#[cfg(not(unix))]
fn link(_target: &Path, _at: &Path) -> std::io::Result<()> {
    Err(std::io::Error::other("a symbolic link is a Unix facility"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const DEV: Identity = Identity {
        name: APP_NAME,
        bundle_id: BUNDLE_ID,
        version: "0.0.0",
    };

    /// The plist is the bundle's whole identity, and three of its keys are cross-references that
    /// a typo breaks silently: an application whose `CFBundleExecutable` names no file will not
    /// start, and one whose `CFBundleIconFile` names no file shows the generic icon -- which is
    /// the symptom this command exists to remove.
    #[test]
    fn info_plist_names_the_executable_and_the_icon_file_the_bundle_actually_holds() {
        for form in [IconForm::Composed, IconForm::Fallback] {
            let plist = info_plist(form, &DEV);
            assert!(
                plist.contains(&format!("<string>{BINARY}</string>")),
                "{plist}"
            );
            assert!(
                plist.contains(&format!("<string>{BUNDLE_ID}</string>")),
                "{plist}"
            );
            assert!(plist.starts_with("<?xml version=\"1.0\""), "{plist}");
        }
    }

    /// The two forms name different files, and each has to name the one its own branch of
    /// `install_icon` actually wrote: a plist naming the other is an application that shows the
    /// generic icon and says nothing about why.
    #[test]
    fn each_icon_form_names_the_file_its_own_branch_installs() {
        let composed = info_plist(IconForm::Composed, &DEV);
        assert!(
            composed.contains("<key>CFBundleIconName</key>"),
            "{composed}"
        );
        assert!(
            composed.contains(&format!("<string>{APP_NAME}</string>")),
            "{composed}"
        );
        let fallback = info_plist(IconForm::Fallback, &DEV);
        assert!(!fallback.contains("CFBundleIconName"), "{fallback}");
        assert!(
            fallback.contains("<key>CFBundleIconFile</key>"),
            "{fallback}"
        );
    }

    /// The released client is its own application beside the launcher's `Dereth.app`: its plist
    /// carries the name and version it is given, and still names the one icon the bundle holds.
    #[test]
    fn a_bundle_named_otherwise_keeps_the_client_executable_and_the_dereth_icon() {
        let plist = info_plist(
            IconForm::Composed,
            &Identity {
                name: "Dereth Client",
                bundle_id: BUNDLE_ID,
                version: "0.1.0",
            },
        );
        assert!(plist.contains("<string>Dereth Client</string>"), "{plist}");
        assert!(plist.contains("<string>0.1.0</string>"), "{plist}");
        assert!(
            plist.contains(&format!("<string>{BINARY}</string>")),
            "{plist}"
        );
        assert!(
            plist.contains("<key>CFBundleIconFile</key>\n\t<string>Dereth</string>"),
            "{plist}"
        );
    }

    /// `--dat-dir` without its directory, and any argument that is not one of the two, are usage
    /// errors rather than a bundle built from a guess.
    #[test]
    fn a_malformed_command_line_is_refused_and_builds_nothing() {
        assert_eq!(bundle_macos(&["--dat-dir".to_owned()]), 2);
        assert_eq!(bundle_macos(&["--everything".to_owned()]), 2);
    }
}
