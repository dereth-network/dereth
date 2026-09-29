//! The header checks: each packaged binary is the format and architecture of its target and
//! needs nothing the target's users may not have.
//!
//! - **Windows (PE):** x86-64, PE32+, the console subsystem, and no import of the Visual C++
//!   runtime (`VCRUNTIME*`, `MSVCP*`, the universal CRT's `api-ms-win-crt-*` or `ucrtbase`): the C
//!   runtime is linked statically. The server carries its icon resource.
//! - **Linux (ELF):** 64-bit, the target's machine and dynamic loader, only the C library's own
//!   shared objects as `DT_NEEDED`, and no glibc symbol version newer than the floor.
//! - **macOS (Mach-O):** 64-bit, the target's CPU, only system libraries (`/usr/lib`,
//!   `/System/Library`), a minimum macOS at or below the floor, and on Apple silicon a code
//!   signature (the linker's ad-hoc one; the system refuses to run an unsigned arm64 binary).
//!
//! The parsers read only what the checks need and treat every offset as untrusted.

use super::targets::{Arch, Os, Target, GLIBC_FLOOR, MACOS_FLOOR};

/// Which binary is checked: the server carries an icon resource on Windows; the importer none.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Role {
    Server,
    Import,
}

fn u16_at(b: &[u8], off: usize) -> Result<u16, String> {
    b.get(off..off + 2)
        .map(|s| u16::from_le_bytes([s[0], s[1]]))
        .ok_or_else(|| format!("truncated at {off:#x}"))
}

fn u32_at(b: &[u8], off: usize) -> Result<u32, String> {
    b.get(off..off + 4)
        .map(|s| u32::from_le_bytes([s[0], s[1], s[2], s[3]]))
        .ok_or_else(|| format!("truncated at {off:#x}"))
}

fn u64_at(b: &[u8], off: usize) -> Result<u64, String> {
    b.get(off..off + 8)
        .map(|s| u64::from_le_bytes([s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]]))
        .ok_or_else(|| format!("truncated at {off:#x}"))
}

fn cstr_at(b: &[u8], off: usize) -> Result<String, String> {
    let tail = b
        .get(off..)
        .ok_or_else(|| format!("string out of range at {off:#x}"))?;
    let end = tail
        .iter()
        .position(|&c| c == 0)
        .ok_or_else(|| format!("unterminated string at {off:#x}"))?;
    Ok(String::from_utf8_lossy(&tail[..end]).into_owned())
}

fn to_usize(v: u64) -> Result<usize, String> {
    usize::try_from(v).map_err(|_| format!("offset {v:#x} out of range"))
}

/// What the PE check reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pe {
    pub machine: u16,
    pub pe32_plus: bool,
    pub subsystem: u16,
    /// The imported DLLs, ordinary and delay-loaded, as named.
    pub imports: Vec<String>,
    /// The resource directory is non-empty.
    pub resources: bool,
}

pub const IMAGE_FILE_MACHINE_AMD64: u16 = 0x8664;
pub const IMAGE_FILE_MACHINE_ARM64: u16 = 0xAA64;
pub const IMAGE_SUBSYSTEM_WINDOWS_CUI: u16 = 3;

pub fn parse_pe(b: &[u8]) -> Result<Pe, String> {
    if b.get(..2) != Some(b"MZ") {
        return Err("not a PE file (no MZ header)".to_owned());
    }
    let pe = u32_at(b, 0x3C)? as usize;
    if b.get(pe..pe + 4) != Some(b"PE\0\0") {
        return Err("not a PE file (no PE signature)".to_owned());
    }
    let coff = pe + 4;
    let machine = u16_at(b, coff)?;
    let sections = u16_at(b, coff + 2)? as usize;
    let opt_size = u16_at(b, coff + 16)? as usize;
    let opt = coff + 20;
    let magic = u16_at(b, opt)?;
    let pe32_plus = magic == 0x20B;
    let subsystem = u16_at(b, opt + 68)?;
    let (count_off, dirs_off) = if pe32_plus { (108, 112) } else { (92, 96) };
    let dir_count = u32_at(b, opt + count_off)? as usize;
    let dir = |i: usize| -> Result<(u32, u32), String> {
        if i >= dir_count {
            return Ok((0, 0));
        }
        Ok((
            u32_at(b, opt + dirs_off + i * 8)?,
            u32_at(b, opt + dirs_off + i * 8 + 4)?,
        ))
    };
    let table = opt + opt_size;
    let mut map = Vec::with_capacity(sections);
    for i in 0..sections {
        let s = table + i * 40;
        let vsize = u32_at(b, s + 8)?;
        let va = u32_at(b, s + 12)?;
        let raw_size = u32_at(b, s + 16)?;
        let raw_ptr = u32_at(b, s + 20)?;
        map.push((va, vsize.max(raw_size), raw_ptr));
    }
    let rva = |r: u32| -> Result<usize, String> {
        map.iter()
            .find(|(va, size, _)| r >= *va && r - *va < *size)
            .map(|(va, _, ptr)| (*ptr + (r - *va)) as usize)
            .ok_or_else(|| format!("RVA {r:#x} is in no section"))
    };
    let mut imports = Vec::new();
    let (import_rva, _) = dir(1)?;
    if import_rva != 0 {
        let mut d = rva(import_rva)?;
        loop {
            let name_rva = u32_at(b, d + 12)?;
            if name_rva == 0 && u32_at(b, d)? == 0 {
                break;
            }
            imports.push(cstr_at(b, rva(name_rva)?)?);
            d += 20;
            if imports.len() > 4096 {
                return Err("an import table without an end".to_owned());
            }
        }
    }
    let (delay_rva, _) = dir(13)?;
    if delay_rva != 0 {
        let mut d = rva(delay_rva)?;
        loop {
            let name_rva = u32_at(b, d + 4)?;
            if name_rva == 0 {
                break;
            }
            imports.push(cstr_at(b, rva(name_rva)?)?);
            d += 32;
            if imports.len() > 4096 {
                return Err("a delay-import table without an end".to_owned());
            }
        }
    }
    let (_, resource_size) = dir(2)?;
    Ok(Pe {
        machine,
        pe32_plus,
        subsystem,
        imports,
        resources: resource_size != 0,
    })
}

/// A DLL that belongs to the Visual C++ runtime rather than to Windows itself.
pub fn is_c_runtime_dll(name: &str) -> bool {
    let n = name.to_ascii_lowercase();
    n.starts_with("vcruntime")
        || n.starts_with("msvcp")
        || n.starts_with("api-ms-win-crt-")
        || n == "ucrtbase.dll"
}

pub fn check_pe(pe: &Pe, target: Target, role: Role) -> Vec<String> {
    let mut problems = Vec::new();
    let want = match target.arch {
        Arch::X86_64 => IMAGE_FILE_MACHINE_AMD64,
        Arch::Aarch64 => IMAGE_FILE_MACHINE_ARM64,
    };
    if pe.machine != want {
        problems.push(format!("machine {:#06x}, expected {want:#06x}", pe.machine));
    }
    if !pe.pe32_plus {
        problems.push("not PE32+ (64-bit)".to_owned());
    }
    if pe.subsystem != IMAGE_SUBSYSTEM_WINDOWS_CUI {
        problems.push(format!(
            "subsystem {}, expected the console ({IMAGE_SUBSYSTEM_WINDOWS_CUI})",
            pe.subsystem
        ));
    }
    let runtime: Vec<&str> = pe
        .imports
        .iter()
        .filter(|i| is_c_runtime_dll(i))
        .map(String::as_str)
        .collect();
    if !runtime.is_empty() {
        problems.push(format!(
            "imports the Visual C++ runtime ({}); the C runtime must be linked statically \
             (`-C target-feature=+crt-static`)",
            runtime.join(", ")
        ));
    }
    if role == Role::Server && !pe.resources {
        problems.push("no resources: the server's icon is missing".to_owned());
    }
    problems
}

/// What the ELF check reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Elf {
    pub machine: u16,
    pub interp: Option<String>,
    pub needed: Vec<String>,
    /// Every `GLIBC_x.y[.z]` version the binary requires.
    pub glibc: Vec<(u32, u32, u32)>,
}

pub const EM_X86_64: u16 = 62;
pub const EM_AARCH64: u16 = 183;

const PT_LOAD: u32 = 1;
const PT_DYNAMIC: u32 = 2;
const PT_INTERP: u32 = 3;
const DT_NULL: u64 = 0;
const DT_NEEDED: u64 = 1;
const DT_STRTAB: u64 = 5;
const DT_VERNEED: u64 = 0x6fff_fffe;
const DT_VERNEEDNUM: u64 = 0x6fff_ffff;

pub fn parse_elf(b: &[u8]) -> Result<Elf, String> {
    if b.get(..4) != Some(b"\x7fELF") {
        return Err("not an ELF file".to_owned());
    }
    if b.get(4) != Some(&2) {
        return Err("not a 64-bit ELF file".to_owned());
    }
    if b.get(5) != Some(&1) {
        return Err("not a little-endian ELF file".to_owned());
    }
    let machine = u16_at(b, 18)?;
    let phoff = to_usize(u64_at(b, 32)?)?;
    let phentsize = u16_at(b, 54)? as usize;
    let phnum = u16_at(b, 56)? as usize;
    let mut loads = Vec::new();
    let mut interp = None;
    let mut dynamic = None;
    for i in 0..phnum {
        let p = phoff + i * phentsize;
        let kind = u32_at(b, p)?;
        let offset = u64_at(b, p + 8)?;
        let vaddr = u64_at(b, p + 16)?;
        let filesz = u64_at(b, p + 32)?;
        match kind {
            PT_LOAD => loads.push((vaddr, filesz, offset)),
            PT_INTERP => interp = Some(cstr_at(b, to_usize(offset)?)?),
            PT_DYNAMIC => dynamic = Some((offset, filesz)),
            _ => {}
        }
    }
    let at = |v: u64| -> Result<usize, String> {
        loads
            .iter()
            .find(|(va, size, _)| v >= *va && v - *va < *size)
            .map(|(va, _, off)| to_usize(off + (v - va)))
            .unwrap_or_else(|| Err(format!("address {v:#x} is in no loaded segment")))
    };
    let mut needed = Vec::new();
    let mut glibc = Vec::new();
    if let Some((offset, size)) = dynamic {
        let mut needed_offs = Vec::new();
        let (mut strtab, mut verneed, mut verneednum) = (None, None, 0u64);
        let start = to_usize(offset)?;
        for i in 0..to_usize(size / 16)? {
            let e = start + i * 16;
            let tag = u64_at(b, e)?;
            let val = u64_at(b, e + 8)?;
            match tag {
                DT_NULL => break,
                DT_NEEDED => needed_offs.push(val),
                DT_STRTAB => strtab = Some(val),
                DT_VERNEED => verneed = Some(val),
                DT_VERNEEDNUM => verneednum = val,
                _ => {}
            }
        }
        let strtab = match strtab {
            Some(v) => at(v)?,
            None if needed_offs.is_empty() => 0,
            None => return Err("DT_NEEDED without a string table".to_owned()),
        };
        for off in needed_offs {
            needed.push(cstr_at(b, strtab + to_usize(off)?)?);
        }
        if let Some(v) = verneed {
            let mut entry = at(v)?;
            for _ in 0..verneednum {
                let count = u16_at(b, entry + 2)?;
                let aux_off = u32_at(b, entry + 8)? as usize;
                let next = u32_at(b, entry + 12)? as usize;
                let mut aux = entry + aux_off;
                for _ in 0..count {
                    let name = cstr_at(b, strtab + u32_at(b, aux + 8)? as usize)?;
                    if let Some(v) = parse_glibc(&name) {
                        glibc.push(v);
                    }
                    let aux_next = u32_at(b, aux + 12)? as usize;
                    if aux_next == 0 {
                        break;
                    }
                    aux += aux_next;
                }
                if next == 0 {
                    break;
                }
                entry += next;
            }
        }
    }
    Ok(Elf {
        machine,
        interp,
        needed,
        glibc,
    })
}

/// `GLIBC_2.28` → (2, 28, 0); anything else (`GLIBC_PRIVATE`, `GCC_3.0`) → none.
pub fn parse_glibc(name: &str) -> Option<(u32, u32, u32)> {
    let v = name.strip_prefix("GLIBC_")?;
    let mut parts = v.split('.').map(|p| p.parse::<u32>().ok());
    let major = parts.next()??;
    let minor = parts.next()??;
    let patch = match parts.next() {
        Some(p) => p?,
        None => 0,
    };
    Some((major, minor, patch))
}

/// The shared objects a Linux release may need: the C library's own.
pub const ALLOWED_NEEDED: &[&str] = &[
    "libc.so.6",
    "libm.so.6",
    "libgcc_s.so.1",
    "libpthread.so.0",
    "libdl.so.2",
    "librt.so.1",
    "ld-linux-x86-64.so.2",
    "ld-linux-aarch64.so.1",
];

/// The dynamic loader of a Linux target.
pub fn linux_interp(arch: Arch) -> &'static str {
    match arch {
        Arch::X86_64 => "/lib64/ld-linux-x86-64.so.2",
        Arch::Aarch64 => "/lib/ld-linux-aarch64.so.1",
    }
}

pub fn check_elf(elf: &Elf, target: Target) -> Vec<String> {
    let mut problems = Vec::new();
    let want = match target.arch {
        Arch::X86_64 => EM_X86_64,
        Arch::Aarch64 => EM_AARCH64,
    };
    if elf.machine != want {
        problems.push(format!("machine {}, expected {want}", elf.machine));
    }
    let interp = linux_interp(target.arch);
    if elf.interp.as_deref() != Some(interp) {
        problems.push(format!(
            "dynamic loader {}, expected {interp}",
            elf.interp.as_deref().unwrap_or("(none)")
        ));
    }
    let extra: Vec<&str> = elf
        .needed
        .iter()
        .filter(|n| !ALLOWED_NEEDED.contains(&n.as_str()))
        .map(String::as_str)
        .collect();
    if !extra.is_empty() {
        problems.push(format!(
            "needs shared objects beyond the C library's: {}",
            extra.join(", ")
        ));
    }
    if let Some(newest) = elf.glibc.iter().max() {
        if (newest.0, newest.1) > GLIBC_FLOOR {
            problems.push(format!(
                "requires GLIBC_{} (newest), above the {}.{} floor",
                version_text(*newest),
                GLIBC_FLOOR.0,
                GLIBC_FLOOR.1
            ));
        }
    } else {
        problems.push(
            "requires no glibc symbol version: not linked the way the release links".to_owned(),
        );
    }
    problems
}

fn version_text((a, b, c): (u32, u32, u32)) -> String {
    if c == 0 {
        format!("{a}.{b}")
    } else {
        format!("{a}.{b}.{c}")
    }
}

/// What the Mach-O check reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MachO {
    pub cputype: u32,
    pub dylibs: Vec<String>,
    pub signed: bool,
    /// The minimum macOS (`LC_BUILD_VERSION` or `LC_VERSION_MIN_MACOSX`).
    pub min_os: Option<(u32, u32, u32)>,
}

pub const CPU_TYPE_X86_64: u32 = 0x0100_0007;
pub const CPU_TYPE_ARM64: u32 = 0x0100_000C;
const MH_MAGIC_64: u32 = 0xFEED_FACF;
const LC_LOAD_DYLIB: u32 = 0xC;
const LC_LOAD_WEAK_DYLIB: u32 = 0x8000_0018;
const LC_REEXPORT_DYLIB: u32 = 0x8000_001F;
const LC_CODE_SIGNATURE: u32 = 0x1D;
const LC_VERSION_MIN_MACOSX: u32 = 0x24;
const LC_BUILD_VERSION: u32 = 0x32;

pub fn parse_macho(b: &[u8]) -> Result<MachO, String> {
    if u32_at(b, 0)? != MH_MAGIC_64 {
        return Err("not a 64-bit Mach-O file (or a universal one)".to_owned());
    }
    let cputype = u32_at(b, 4)?;
    let ncmds = u32_at(b, 16)?;
    let mut at = 32usize;
    let mut dylibs = Vec::new();
    let mut signed = false;
    let mut min_os = None;
    for _ in 0..ncmds {
        let cmd = u32_at(b, at)?;
        let size = u32_at(b, at + 4)? as usize;
        if size < 8 {
            return Err(format!("a load command of {size} bytes"));
        }
        match cmd {
            LC_LOAD_DYLIB | LC_LOAD_WEAK_DYLIB | LC_REEXPORT_DYLIB => {
                let name = u32_at(b, at + 8)? as usize;
                dylibs.push(cstr_at(b, at + name)?);
            }
            LC_CODE_SIGNATURE => signed = true,
            LC_BUILD_VERSION => min_os = Some(decode_version(u32_at(b, at + 12)?)),
            LC_VERSION_MIN_MACOSX => min_os = Some(decode_version(u32_at(b, at + 8)?)),
            _ => {}
        }
        at += size;
    }
    Ok(MachO {
        cputype,
        dylibs,
        signed,
        min_os,
    })
}

/// Mach-O's `xxxx.yy.zz` nibble-packed version.
fn decode_version(v: u32) -> (u32, u32, u32) {
    (v >> 16, (v >> 8) & 0xFF, v & 0xFF)
}

pub fn check_macho(m: &MachO, target: Target) -> Vec<String> {
    let mut problems = Vec::new();
    let want = match target.arch {
        Arch::X86_64 => CPU_TYPE_X86_64,
        Arch::Aarch64 => CPU_TYPE_ARM64,
    };
    if m.cputype != want {
        problems.push(format!("CPU type {:#x}, expected {want:#x}", m.cputype));
    }
    let foreign: Vec<&str> = m
        .dylibs
        .iter()
        .filter(|d| !(d.starts_with("/usr/lib/") || d.starts_with("/System/Library/")))
        .map(String::as_str)
        .collect();
    if !foreign.is_empty() {
        problems.push(format!(
            "links libraries outside the system: {}",
            foreign.join(", ")
        ));
    }
    match m.min_os {
        Some(v) if (v.0, v.1) > MACOS_FLOOR => problems.push(format!(
            "minimum macOS {}, above the {}.{} floor",
            version_text(v),
            MACOS_FLOOR.0,
            MACOS_FLOOR.1
        )),
        Some(_) => {}
        None => problems.push("no minimum macOS version".to_owned()),
    }
    if target.arch == Arch::Aarch64 && !m.signed {
        problems.push("no code signature: macOS will not run an unsigned arm64 binary".to_owned());
    }
    problems
}

/// The windowed subsystem: a program that makes its own window and no console.
pub const IMAGE_SUBSYSTEM_WINDOWS_GUI: u16 = 2;

/// Check one of Dereth's binaries (the launcher or the client) for `target`: a one-line
/// description for the manifest, or every problem found.
///
/// - **Windows:** x86-64 PE32+, the windowed subsystem, the icon resource, and no import of the
///   Visual C++ runtime.
/// - **Linux:** the target's machine and dynamic loader, and no glibc symbol newer than
///   `glibc_floor`. The shared objects are not limited: the launcher links the desktop's web view
///   and the client loads its graphics and window libraries, which every desktop has.
/// - **macOS:** as the server's binaries: the CPU, only system libraries, the minimum macOS, and
///   a code signature on Apple silicon.
pub fn check_app_binary(
    bytes: &[u8],
    target: Target,
    glibc_floor: (u32, u32),
) -> Result<String, Vec<String>> {
    match target.os {
        Os::Windows => {
            let pe = parse_pe(bytes).map_err(|e| vec![e])?;
            let mut problems: Vec<String> = check_pe(&pe, target, Role::Import)
                .into_iter()
                .filter(|p| !p.starts_with("subsystem"))
                .collect();
            if pe.subsystem != IMAGE_SUBSYSTEM_WINDOWS_GUI {
                problems.push(format!(
                    "subsystem {}, expected the windowed one ({IMAGE_SUBSYSTEM_WINDOWS_GUI})",
                    pe.subsystem
                ));
            }
            if !pe.resources {
                problems.push("no resources: the icon is missing".to_owned());
            }
            if !problems.is_empty() {
                return Err(problems);
            }
            Ok(format!(
                "PE32+ machine {:#06x}, windowed; imports {}; resources",
                pe.machine,
                pe.imports.join(", ")
            ))
        }
        Os::Linux => {
            let elf = parse_elf(bytes).map_err(|e| vec![e])?;
            let mut problems = Vec::new();
            let want = match target.arch {
                Arch::X86_64 => EM_X86_64,
                Arch::Aarch64 => EM_AARCH64,
            };
            if elf.machine != want {
                problems.push(format!("machine {}, expected {want}", elf.machine));
            }
            let interp = linux_interp(target.arch);
            if elf.interp.as_deref() != Some(interp) {
                problems.push(format!(
                    "dynamic loader {}, expected {interp}",
                    elf.interp.as_deref().unwrap_or("(none)")
                ));
            }
            let newest = elf.glibc.iter().max().copied();
            if let Some(n) = newest.filter(|n| (n.0, n.1) > glibc_floor) {
                problems.push(format!(
                    "requires GLIBC_{} (newest), above the {}.{} floor",
                    version_text(n),
                    glibc_floor.0,
                    glibc_floor.1
                ));
            }
            if !problems.is_empty() {
                return Err(problems);
            }
            Ok(format!(
                "ELF64 machine {}; loader {}; needs {}; newest GLIBC_{}",
                elf.machine,
                elf.interp.as_deref().unwrap_or("(none)"),
                elf.needed.join(", "),
                version_text(newest.unwrap_or_default())
            ))
        }
        Os::Mac => check_binary(bytes, target, Role::Import),
    }
}

/// Parse and check one binary for `target`: a one-line description for the manifest, or every
/// problem found.
pub fn check_binary(bytes: &[u8], target: Target, role: Role) -> Result<String, Vec<String>> {
    match target.os {
        Os::Windows => {
            let pe = parse_pe(bytes).map_err(|e| vec![e])?;
            let problems = check_pe(&pe, target, role);
            if !problems.is_empty() {
                return Err(problems);
            }
            Ok(format!(
                "PE32+ machine {:#06x}, console; imports {}{}",
                pe.machine,
                pe.imports.join(", "),
                if pe.resources { "; resources" } else { "" }
            ))
        }
        Os::Linux => {
            let elf = parse_elf(bytes).map_err(|e| vec![e])?;
            let problems = check_elf(&elf, target);
            if !problems.is_empty() {
                return Err(problems);
            }
            let newest = elf.glibc.iter().max().copied().unwrap_or_default();
            Ok(format!(
                "ELF64 machine {}; loader {}; needs {}; newest GLIBC_{}",
                elf.machine,
                elf.interp.as_deref().unwrap_or("(none)"),
                elf.needed.join(", "),
                version_text(newest)
            ))
        }
        Os::Mac => {
            let m = parse_macho(bytes).map_err(|e| vec![e])?;
            let problems = check_macho(&m, target);
            if !problems.is_empty() {
                return Err(problems);
            }
            Ok(format!(
                "Mach-O CPU {:#x}; minimum macOS {}; {}; links {}",
                m.cputype,
                version_text(m.min_os.unwrap_or_default()),
                if m.signed { "signed" } else { "unsigned" },
                m.dylibs.join(", ")
            ))
        }
    }
}
