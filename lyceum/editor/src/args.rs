//! The command line.
//!
//! ```text
//! lyceum-editor <world.toml> [--dats <dir>] [--pack <name>] [--landblock <XXYY>]
//!               [--weenies <sql> | --no-weenies] [--schema <json>] [--thumbnails <dir>]
//!               [--radius <n>]
//!               [--empyrean <dir>] [--live-dir <dir>] [--live]
//!               [--client <exe>]
//!               [--renderer vulkan|d3d12|wgpu]
//!               [--headless] [--size <w>x<h>] [--frames <n>] [--script <file>]
//!               [--capture-at <frame>:<png>]...
//! ```
//!
//! - `--dats`: the player's data files (the later set); the folder the client would find otherwise.
//! - `--pack`: the pack edits are written into; the world's last otherwise.
//! - `--landblock`: where the camera starts; the first landblock the world's packs edit otherwise.
//! - `--weenies`: ACE SQL (an export or a dump) naming the server's own weenies' setups and names,
//!   so a spawn of one is drawn and the model browser names the models they are drawn with. The
//!   newest world database `empyrean-import fetch` keeps in the user's cache folder otherwise, read
//!   while the editor runs; `--no-weenies` reads none.
//! - `--schema`: the server's names for the weenie forms, as `empyrean-import schema` writes them;
//!   otherwise written by that tool when it stands beside the editor, else the forms show numbers.
//! - `--thumbnails`: the folder the model browser's pictures are kept in; the user's cache folder
//!   otherwise.
//! - `--radius`: how many landblocks either side are drawn (2 by default).
//! - `--empyrean`: the folder of the server's programs (`empyrean-server`, `empyrean-import`), for
//!   the editor's own world and the forms' names; beside the editor otherwise.
//! - `--live-dir`: the folder the editor's own world runs in, kept when it ends; a new temporary
//!   folder, removed after, otherwise. Its world database is made from the server's SQL that
//!   `--weenies` names (or the one in the user's cache). `--live` starts that world as the editor
//!   opens.
//! - `--client`: the Dereth client the Play button starts; `dereth-client` beside the editor
//!   otherwise.
//! - `--headless`: no window; the frames are drawn off screen on the machine's graphics card.
//!   `--frames` ends the run, `--script` runs a list of edits and pointer events by frame
//!   ([`crate::script`]), and `--capture-at` writes the frame drawn then as a PNG.

use std::path::PathBuf;

/// The command line, read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Args {
    pub world: PathBuf,
    pub dats: Option<PathBuf>,
    pub pack: Option<String>,
    pub landblock: Option<u16>,
    pub weenies: Option<PathBuf>,
    /// The server's names for the weenie forms (`empyrean-import schema`'s file).
    pub schema: Option<PathBuf>,
    /// Whether to read the server database in the user's cache when `weenies` names none.
    pub server_database: bool,
    pub thumbnails: Option<PathBuf>,
    pub radius: u32,
    /// The editor's own world: the server's programs, its world database, its folder, whether it
    /// starts at once, and the client that plays it.
    pub empyrean: Option<PathBuf>,
    pub live_dir: Option<PathBuf>,
    pub live: bool,
    pub client: Option<PathBuf>,
    /// The file the editor's text is read from, in place of the one beside its settings.
    pub text: Option<PathBuf>,
    pub renderer: Option<String>,
    pub headless: bool,
    pub size: (u32, u32),
    pub frames: Option<u64>,
    pub script: Option<PathBuf>,
    pub captures: Vec<(u64, PathBuf)>,
}

/// The usage line, for a command line that does not read.
pub const USAGE: &str = "\
lyceum-editor <world.toml> [--dats <dir>] [--pack <name>] [--landblock <XXYY>]
              [--weenies <sql> | --no-weenies] [--schema <json>] [--thumbnails <dir>]
              [--radius <n>] [--empyrean <dir>] [--live-dir <dir>] [--live]
              [--client <exe>] [--text <toml>] [--renderer vulkan|d3d12|wgpu] [--headless]
              [--size <w>x<h>]
              [--frames <n>] [--script <file>] [--capture-at <frame>:<png>]...";

/// Read `argv` (without the program's name).
///
/// # Errors
/// A command line that does not read, with the reason.
pub fn parse(argv: &[String]) -> Result<Args, String> {
    let mut a = Args {
        world: PathBuf::new(),
        dats: None,
        pack: None,
        landblock: None,
        weenies: None,
        schema: None,
        server_database: true,
        thumbnails: None,
        radius: 2,
        empyrean: None,
        live_dir: None,
        live: false,
        client: None,
        text: None,
        renderer: None,
        headless: false,
        size: (1280, 800),
        frames: None,
        script: None,
        captures: Vec::new(),
    };
    let mut world = None;
    let mut it = argv.iter();
    while let Some(arg) = it.next() {
        let mut value = |name: &str| {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} takes a value"))
        };
        match arg.as_str() {
            "--dats" => a.dats = Some(PathBuf::from(value(arg)?)),
            "--pack" => a.pack = Some(value(arg)?),
            "--landblock" => {
                let v = value(arg)?;
                a.landblock = Some(
                    u16::from_str_radix(&v, 16)
                        .ok()
                        .filter(|_| v.len() == 4)
                        .ok_or_else(|| format!("{v:?} is not a landblock: four hex digits"))?,
                );
            }
            "--weenies" => a.weenies = Some(PathBuf::from(value(arg)?)),
            "--no-weenies" => a.server_database = false,
            "--schema" => a.schema = Some(PathBuf::from(value(arg)?)),
            "--thumbnails" => a.thumbnails = Some(PathBuf::from(value(arg)?)),
            "--radius" => {
                let v = value(arg)?;
                a.radius = v
                    .parse()
                    .ok()
                    .filter(|r| (1..=8).contains(r))
                    .ok_or_else(|| format!("--radius is 1 to 8, not {v}"))?;
            }
            "--empyrean" => a.empyrean = Some(PathBuf::from(value(arg)?)),
            "--live-dir" => a.live_dir = Some(PathBuf::from(value(arg)?)),
            "--live" => a.live = true,
            "--client" => a.client = Some(PathBuf::from(value(arg)?)),
            "--text" => a.text = Some(PathBuf::from(value(arg)?)),
            "--renderer" => a.renderer = Some(value(arg)?),
            "--headless" => a.headless = true,
            "--size" => {
                let v = value(arg)?;
                let (w, h) = v
                    .split_once('x')
                    .and_then(|(w, h)| Some((w.parse().ok()?, h.parse().ok()?)))
                    .filter(|(w, h): &(u32, u32)| {
                        (64..=8192).contains(w) && (64..=8192).contains(h)
                    })
                    .ok_or_else(|| format!("--size is <width>x<height>, not {v}"))?;
                a.size = (w, h);
            }
            "--frames" => {
                let v = value(arg)?;
                a.frames = Some(v.parse().map_err(|_| format!("--frames {v}"))?);
            }
            "--script" => a.script = Some(PathBuf::from(value(arg)?)),
            "--capture-at" => {
                let v = value(arg)?;
                let (n, path) = v
                    .split_once(':')
                    .and_then(|(n, p)| Some((n.parse::<u64>().ok()?, p)))
                    .filter(|(_, p)| !p.is_empty())
                    .ok_or_else(|| format!("--capture-at is <frame>:<png>, not {v}"))?;
                a.captures.push((n, PathBuf::from(path)));
            }
            other if other.starts_with("--") => return Err(format!("{other} is not a switch")),
            other => {
                if world.replace(PathBuf::from(other)).is_some() {
                    return Err("one world file".into());
                }
            }
        }
    }
    a.world = world.ok_or("which world: lyceum-editor <world.toml>")?;
    if !a.headless && (a.frames.is_some() || a.script.is_some() || !a.captures.is_empty()) {
        return Err("--frames, --script and --capture-at are for a --headless run".into());
    }
    Ok(a)
}

impl Args {
    /// The client's own command line for the runtime under the editor: offline, no body, no game
    /// interface, no sound, the dats named. The overlay folder is the editor's own, set once the
    /// world is open.
    #[must_use]
    pub fn runtime_argv(&self, landblock: u16) -> Vec<String> {
        let mut v: Vec<String> = [
            "--no-connect",
            "--no-character",
            "--no-ui",
            "--no-sound",
            "--landblock",
        ]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
        v.push(format!("{landblock:04X}"));
        v.push("--land-radius".into());
        v.push((self.radius + 2).to_string());
        v.push("--scenery-radius".into());
        v.push(self.radius.to_string());
        if let Some(d) = &self.dats {
            v.push("--dat-dir".into());
            v.push(d.display().to_string());
        }
        if let Some(r) = &self.renderer {
            v.push("--renderer".into());
            v.push(r.clone());
        }
        if self.headless {
            v.push("--headless".into());
        }
        v
    }
}

/// The runtime's configuration for `args`, as a scripted run or a test makes it: no settings
/// folder, nothing installed. A windowed run takes `dereth_desktop::start`'s instead.
///
/// # Errors
/// A command line the client does not read.
pub fn runtime_config(args: &Args) -> Result<dereth_client_runtime::config::Config, String> {
    let argv = args.runtime_argv(args.landblock.unwrap_or(0xA9B4));
    dereth_client_runtime::config::Config::from_args_and_prefs_at(&argv, std::path::Path::new(""))
        .map_err(|e| e.detail)
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: the world editor's command line)
    use super::*;

    fn args(s: &[&str]) -> Result<Args, String> {
        parse(&s.iter().map(|a| (*a).to_owned()).collect::<Vec<_>>())
    }

    #[test]
    fn a_scripted_run_reads_its_frames_captures_and_size() {
        let a = args(&[
            "world.toml",
            "--headless",
            "--size",
            "640x480",
            "--frames",
            "30",
            "--capture-at",
            "10:a.png",
            "--capture-at",
            "30:b.png",
            "--landblock",
            "A9B4",
        ])
        .unwrap();
        assert_eq!(a.world, PathBuf::from("world.toml"));
        assert_eq!(a.size, (640, 480));
        assert_eq!(a.frames, Some(30));
        assert_eq!(
            a.captures,
            [(10, PathBuf::from("a.png")), (30, PathBuf::from("b.png"))]
        );
        assert_eq!(a.landblock, Some(0xA9B4));
    }

    #[test]
    fn a_window_takes_no_script_and_a_switch_it_does_not_know_is_refused() {
        assert!(args(&["w.toml", "--frames", "3"]).is_err());
        assert!(args(&["w.toml", "--fly"]).is_err());
        assert!(args(&[]).is_err());
    }

    #[test]
    fn the_editors_own_world_reads_its_programs_folder_and_client() {
        let a = args(&[
            "w.toml",
            "--empyrean",
            "server",
            "--live-dir",
            "live",
            "--live",
            "--client",
            "dereth-client.exe",
        ])
        .unwrap();
        assert_eq!(a.empyrean, Some(PathBuf::from("server")));
        assert_eq!(a.live_dir, Some(PathBuf::from("live")));
        assert!(a.live);
        assert_eq!(a.client, Some(PathBuf::from("dereth-client.exe")));
        let b = args(&["w.toml"]).unwrap();
        assert!(!b.live && b.empyrean.is_none() && b.live_dir.is_none());
    }
}
