//! The targets Empyrean is released for, and which host can build which.

/// An operating system, of a target or of the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    Linux,
    Mac,
}

impl Os {
    /// The host this runs on.
    pub fn host() -> Option<Self> {
        match std::env::consts::OS {
            "windows" => Some(Self::Windows),
            "linux" => Some(Self::Linux),
            "macos" => Some(Self::Mac),
            _ => None,
        }
    }
}

/// A processor architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Arch {
    X86_64,
    Aarch64,
}

/// One release target.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Target {
    pub triple: &'static str,
    pub os: Os,
    pub arch: Arch,
}

/// The oldest glibc a Linux release runs on: the binaries are linked against its symbols
/// (`cargo zigbuild --target <triple>.2.28`) and the header check holds them to it.
pub const GLIBC_FLOOR: (u32, u32) = (2, 28);

/// The oldest macOS a macOS release claims (`MACOSX_DEPLOYMENT_TARGET`); the header check holds
/// the binaries' minimum to it.
pub const MACOS_FLOOR: (u32, u32) = (11, 0);

/// Every target Empyrean is released for.
pub const TARGETS: &[Target] = &[
    Target {
        triple: "x86_64-pc-windows-msvc",
        os: Os::Windows,
        arch: Arch::X86_64,
    },
    Target {
        triple: "x86_64-unknown-linux-gnu",
        os: Os::Linux,
        arch: Arch::X86_64,
    },
    Target {
        triple: "aarch64-unknown-linux-gnu",
        os: Os::Linux,
        arch: Arch::Aarch64,
    },
    Target {
        triple: "aarch64-apple-darwin",
        os: Os::Mac,
        arch: Arch::Aarch64,
    },
    Target {
        triple: "x86_64-apple-darwin",
        os: Os::Mac,
        arch: Arch::X86_64,
    },
];

/// The release target named `triple`.
pub fn find(triple: &str) -> Result<Target, String> {
    TARGETS
        .iter()
        .copied()
        .find(|t| t.triple == triple)
        .ok_or_else(|| {
            let known: Vec<&str> = TARGETS.iter().map(|t| t.triple).collect();
            format!(
                "`{triple}` is not an Empyrean release target; the targets are {}",
                known.join(", ")
            )
        })
}

impl Target {
    /// The executable suffix of the target.
    pub fn exe_suffix(self) -> &'static str {
        if self.os == Os::Windows {
            ".exe"
        } else {
            ""
        }
    }

    /// The archive format: `.zip` on Windows, `.tar.gz` elsewhere (it keeps the executable bit).
    pub fn archive_ext(self) -> &'static str {
        if self.os == Os::Windows {
            "zip"
        } else {
            "tar.gz"
        }
    }

    /// The archive's file name, `empyrean-<version>-<target>.<ext>`; its one top-level folder is
    /// the same without the extension.
    pub fn archive_name(self, version: &str) -> String {
        format!("{}.{}", self.archive_root(version), self.archive_ext())
    }

    /// The archive's top-level folder, `empyrean-<version>-<target>`.
    pub fn archive_root(self, version: &str) -> String {
        format!("empyrean-{version}-{}", self.triple)
    }

    /// What `--target` is given to the builder: a Linux target carries the glibc floor for
    /// `cargo zigbuild`.
    pub fn build_target_arg(self) -> String {
        if self.os == Os::Linux {
            format!("{}.{}.{}", self.triple, GLIBC_FLOOR.0, GLIBC_FLOOR.1)
        } else {
            self.triple.to_owned()
        }
    }
}

/// How a target is built.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Builder {
    /// `cargo build`, with the host's own toolchain and linker.
    Cargo,
    /// `cargo zigbuild`, which links against a chosen glibc.
    Zigbuild,
}

/// What the host offers a build.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Host {
    pub os: Option<Os>,
    /// `cargo zigbuild` runs.
    pub zigbuild: bool,
    /// `zig` runs (cargo-zigbuild's linker).
    pub zig: bool,
    /// The installed Rust targets, when rustup could say.
    pub installed: Option<Vec<String>>,
}

/// How this host builds `target`, or why it cannot.
///
/// Windows builds on Windows and macOS on macOS, each with its own linker and system libraries;
/// Linux builds anywhere `cargo zigbuild` and `zig` run, since it is the glibc floor that decides
/// where a Linux binary runs and only zig can link against an older one.
pub fn plan(target: Target, host: &Host) -> Result<Builder, String> {
    let builder = match target.os {
        Os::Windows if host.os == Some(Os::Windows) => Builder::Cargo,
        Os::Windows => {
            return Err(format!(
                "{} builds on a Windows host only (the MSVC linker and the Windows SDK)",
                target.triple
            ))
        }
        Os::Mac if host.os == Some(Os::Mac) => Builder::Cargo,
        Os::Mac => {
            return Err(format!(
                "{} builds on a macOS host only (Apple's linker and SDK)",
                target.triple
            ))
        }
        Os::Linux => {
            let mut missing = Vec::new();
            if !host.zigbuild {
                missing.push("cargo-zigbuild (`cargo install --locked cargo-zigbuild`)");
            }
            if !host.zig {
                missing
                    .push("zig on PATH (https://ziglang.org/download/, or `pip install ziglang`)");
            }
            if !missing.is_empty() {
                return Err(format!(
                    "{} is linked against glibc {}.{} with cargo zigbuild; this host lacks {}",
                    target.triple,
                    GLIBC_FLOOR.0,
                    GLIBC_FLOOR.1,
                    missing.join(" and ")
                ));
            }
            Builder::Zigbuild
        }
    };
    if let Some(installed) = &host.installed {
        if !installed.iter().any(|t| t == target.triple) {
            return Err(format!(
                "the Rust standard library for {0} is not installed: `rustup target add {0}`",
                target.triple
            ));
        }
    }
    Ok(builder)
}
