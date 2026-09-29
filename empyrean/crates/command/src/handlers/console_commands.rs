// Ported from ACE (ACEmulator), AGPL-3.0: Source/ACE.Server/Command/Handlers/ConsoleCommands.cs
//! Port of `Source/ACE.Server/Command/Handlers/ConsoleCommands.cs`.
//!
//! Console-only commands (`CommandHandlerFlag.ConsoleInvoke`): they write with
//! `Console.WriteLine` ([`console_write_line`]). The dat exports reach empyrean-dat's pointers.
//!
//! Not ACE: `config-write` ([`handle_config_write`]) writes the configuration in use as
//! `empyrean.toml`.

use empyrean_common::config_manager::ConfigManager;
use empyrean_common::dotnet::format;
use empyrean_common::not_ported;
use empyrean_entity::enums::AccessLevel;
use empyrean_net::SessionId;
use empyrean_world::World;

use crate::command_handler_attribute::CommandHandlerAttribute;
use crate::command_handler_flag::CommandHandlerFlag;
use crate::command_handler_info::{CommandHandlerInfo, NamedHandler};
use crate::command_manager::console_write_line;
use crate::command_parameter_helpers::dotnet_parse;
use crate::handler;

/// This file's `[CommandHandler]` decorations, in declaration order.
#[must_use]
pub fn command_handlers() -> Vec<CommandHandlerInfo> {
    let a = AccessLevel::Admin;
    let c = CommandHandlerFlag::ConsoleInvoke;
    let dir = "<export-directory-without-spaces>";
    let rows: [(CommandHandlerAttribute, NamedHandler); 9] = [
        (
            CommandHandlerAttribute::with_count(
                "version",
                a,
                c,
                0,
                "Show server version information.",
                "",
            ),
            handler!(show_version),
        ),
        (
            CommandHandlerAttribute::with_count(
                "exit",
                a,
                c,
                0,
                "Shut down server immediately.",
                "",
            ),
            handler!(exit),
        ),
        (
            CommandHandlerAttribute::with_count(
                "cell-export",
                a,
                c,
                1,
                "Export contents of CELL DAT file.",
                dir,
            ),
            handler!(export_cell_dat_contents),
        ),
        (
            CommandHandlerAttribute::with_count(
                "portal-export",
                a,
                c,
                1,
                "Export contents of PORTAL DAT file.",
                dir,
            ),
            handler!(export_portal_dat_contents),
        ),
        (
            CommandHandlerAttribute::with_count(
                "highres-export",
                a,
                c,
                1,
                "Export contents of client_highres.dat file.",
                dir,
            ),
            handler!(export_highres_dat_contents),
        ),
        (
            CommandHandlerAttribute::with_count(
                "language-export",
                a,
                c,
                1,
                "Export contents of client_local_English.dat file.",
                dir,
            ),
            handler!(export_language_dat_contents),
        ),
        (
            CommandHandlerAttribute::with_count("wave-export", a, c, 0, "Export Wave Files", ""),
            handler!(export_wave_files),
        ),
        (
            CommandHandlerAttribute::with_count(
                "image-export",
                a,
                c,
                0,
                "Export Texture/Image Files",
                "",
            ),
            handler!(export_image_file),
        ),
        // Not ACE: not in ACE's registry.
        (
            CommandHandlerAttribute::with_count(
                "config-write",
                a,
                c,
                0,
                CONFIG_WRITE_DESCRIPTION,
                CONFIG_WRITE_USAGE,
            ),
            handler!(handle_config_write),
        ),
    ];
    rows.into_iter()
        .map(|(attribute, (handler, handler_name))| CommandHandlerInfo {
            handler,
            handler_name,
            attribute,
        })
        .collect()
}

// ACE: ConsoleCommands.ShowVersion
/// `version`: show server version information.
pub fn show_version(w: &mut World, _session: Option<SessionId>, _parameters: &[String]) {
    let msg = crate::handlers::player_commands::server_build_info_get_version_info(w);
    console_write_line(&msg);
}

// ACE: ConsoleCommands.Exit
/// `exit`: shut down server immediately.
pub fn exit(w: &mut World, session: Option<SessionId>, parameters: &[String]) {
    admin_shard_commands_shutdown_server_now(w, session, parameters);
}

// ACE: ConsoleCommands.ExportCellDatContents
/// `cell-export <export-directory-without-spaces>`.
pub fn export_cell_dat_contents(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    // Not ACE's (a fix): a wrong parameter count prints the usage and
    // stops; ACE went on, so with more than one parameter the export still ran into the first
    // (and with none it threw).
    if parameters.len() != 1 {
        console_write_line("cell-export <export-directory-without-spaces>");
        return;
    }

    let export_dir = &parameters[0];

    console_write_line(&format!(
        "Exporting cell.dat contents to {export_dir}.  This can take longer than an hour."
    ));
    w.dats.cell_dat().extract_landblock_contents(export_dir);
    console_write_line(&format!("Export of cell.dat to {export_dir} complete."));
}

// ACE: ConsoleCommands.ExportPortalDatContents
/// `portal-export <export-directory-without-spaces>`.
pub fn export_portal_dat_contents(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    // Not ACE's (a fix): as cell-export, a wrong parameter count
    // prints the usage and stops.
    if parameters.len() != 1 {
        console_write_line("portal-export <export-directory-without-spaces>");
        return;
    }

    let export_dir = &parameters[0];

    console_write_line(&format!(
        "Exporting portal.dat contents to {export_dir}.  This will take a while."
    ));
    w.dats
        .portal_dat()
        .extract_categorized_portal_contents(export_dir);
    console_write_line(&format!("Export of portal.dat to {export_dir} complete."));
}

// ACE: ConsoleCommands.ExportHighresDatContents
/// `highres-export <export-directory-without-spaces>`.
pub fn export_highres_dat_contents(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    let Some(high_res) = w.dats.high_res_dat() else {
        console_write_line("client_highres.dat file was not loaded.");
        return;
    };
    // Not ACE's (a fix): as cell-export, a wrong parameter count
    // prints the usage and stops.
    if parameters.len() != 1 {
        console_write_line("highres-export <export-directory-without-spaces>");
        return;
    }

    let export_dir = &parameters[0];

    console_write_line(&format!(
        "Exporting client_highres.dat contents to {export_dir}.  This will take a while."
    ));
    high_res.extract_categorized_portal_contents(export_dir);
    console_write_line(&format!(
        "Export of client_highres.dat to {export_dir} complete."
    ));
}

// ACE: ConsoleCommands.ExportLanguageDatContents
/// `language-export <export-directory-without-spaces>`.
///
/// DIVERGE: empyrean-dat always opens the language dat, so ACE's null check (whose message wrongly
/// names client_highres.dat) cannot fire.
pub fn export_language_dat_contents(
    w: &mut World,
    _session: Option<SessionId>,
    parameters: &[String],
) {
    // Not ACE's (a fix): as cell-export, a wrong parameter count
    // prints the usage and stops.
    if parameters.len() != 1 {
        console_write_line("language-export <export-directory-without-spaces>");
        return;
    }

    let export_dir = &parameters[0];

    console_write_line(&format!(
        "Exporting client_local_English.dat contents to {export_dir}.  This will take a while."
    ));
    w.dats
        .language_dat()
        .extract_categorized_portal_contents(export_dir);
    console_write_line(&format!(
        "Export of client_local_English.dat to {export_dir} complete."
    ));
}

// ACE: ConsoleCommands.ExportWaveFiles
/// `wave-export <export-directory-without-spaces>`: export all wav files to a specific directory.
pub fn export_wave_files(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    if parameters.len() != 1 {
        console_write_line("wave-export <export-directory-without-spaces>");
        return;
    }

    let export_dir = &parameters[0];

    console_write_line(&format!(
        "Exporting portal.dat WAV files to {export_dir}.  This may take a while."
    ));
    for entry in w.dats.portal_dat().all_files() {
        if dat_file_is_wave(entry) {
            wave_export_wave(w, entry, export_dir);
        }
    }
    console_write_line(&format!("Export to {export_dir} complete."));
}

// ACE: ConsoleCommands.ExportImageFile
/// `image-export <export-directory-without-spaces> [id]`: export all texture/image files to a
/// specific directory.
pub fn export_image_file(w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let syntax = "image-export <export-directory-without-spaces> [id]";
    if parameters.is_empty() {
        console_write_line(syntax);
        return;
    }

    let export_dir = &parameters[0];
    if export_dir.is_empty() || !std::path::Path::new(export_dir).is_dir() {
        console_write_line(syntax);
        return;
    }

    if parameters.len() > 1 {
        let parsed = if let Some(hex) = parameters[1].strip_prefix("0x") {
            dotnet_parse::uint_try_parse_hex(hex)
        } else {
            dotnet_parse::uint_try_parse(&parameters[1])
        };
        let Some(image_id) = parsed else {
            console_write_line(syntax);
            return;
        };

        texture_export_texture(w, false, image_id, export_dir);

        console_write_line(&format!(
            "Exported {} to {export_dir}.",
            format(image_id, "X8")
        ));
    } else {
        let mut portal_files = 0;
        let mut highres_files = 0;
        console_write_line(&format!("Exporting client_portal.dat textures and images to {export_dir}.  This may take a while."));
        for entry in w.dats.portal_dat().all_files() {
            if dat_file_is_texture(entry) {
                texture_export_texture(w, false, entry, export_dir);
                portal_files += 1;
            }
        }
        console_write_line(&format!(
            "Exported {portal_files} total files from client_portal.dat to {export_dir}."
        ));

        if let Some(high_res) = w.dats.high_res_dat() {
            for entry in high_res.all_files() {
                if dat_file_is_texture(entry) {
                    texture_export_texture(w, true, entry, export_dir);
                    highres_files += 1;
                }
            }
            console_write_line(&format!(
                "Exported {highres_files} total files from client_highres.dat to {export_dir}."
            ));
        }
        let total_files = portal_files + highres_files;
        console_write_line(&format!(
            "Exported {total_files} total files to {export_dir}."
        ));
    }
}

/// `config-write`'s description.
pub const CONFIG_WRITE_DESCRIPTION: &str =
    "Write the configuration in use as empyrean.toml (Empyrean; not in ACE).";

/// `config-write`'s usage.
pub const CONFIG_WRITE_USAGE: &str = "[path, default ./empyrean.toml] [-f to overwrite]";

/// `config-write`'s default path.
pub const CONFIG_WRITE_DEFAULT_PATH: &str = "./empyrean.toml";

/// Not ACE: `config-write [path] [-f]`: writes the configuration in use (`ConfigManager.Config`)
/// as a commented `empyrean.toml`, with the writer `--write-config` uses. It never overwrites an
/// existing file unless `-f` is given, and replies with the path written.
pub fn handle_config_write(_w: &mut World, _session: Option<SessionId>, parameters: &[String]) {
    let usage = format!("config-write {CONFIG_WRITE_USAGE}");
    let force = parameters.iter().any(|p| p == "-f");
    let paths: Vec<&String> = parameters.iter().filter(|p| *p != "-f").collect();
    if paths.len() > 1 {
        console_write_line(&usage);
        return;
    }
    let path = std::path::PathBuf::from(
        paths
            .first()
            .map_or(CONFIG_WRITE_DEFAULT_PATH, |p| p.as_str()),
    );
    let shown = std::path::absolute(&path).unwrap_or_else(|_| path.clone());

    match empyrean_common::toml_config::write_toml_file(&ConfigManager::config(), &path, force) {
        Ok(()) => console_write_line(&format!(
            "Wrote the configuration in use to {}.",
            shown.display()
        )),
        Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => console_write_line(&format!(
            "{} already exists. Use -f to overwrite it: config-write {} -f",
            shown.display(),
            path.display()
        )),
        Err(e) => console_write_line(&format!("Unable to write {}: {e}", shown.display())),
    }
}

// ---------------------------------------------------------------------------------------------
// Pointers to members that are not ported yet.
// ---------------------------------------------------------------------------------------------

/// `AdminShardCommands.ShutdownServerNow(session, parameters)`.
fn admin_shard_commands_shutdown_server_now(
    w: &mut World,
    session: Option<SessionId>,
    parameters: &[String],
) {
    crate::handlers::admin_shard_commands::shutdown_server_now(w, session, parameters);
}

/// `entry.Value.GetFileType(DatDatabaseType.Portal) == DatFileType.Wave`.
fn dat_file_is_wave(_file_id: u32) -> bool {
    not_ported!("ACE: DatFile.GetFileType");
    false
}

/// `entry.Value.GetFileType(DatDatabaseType.Portal) == DatFileType.Texture`.
fn dat_file_is_texture(_file_id: u32) -> bool {
    not_ported!("ACE: DatFile.GetFileType");
    false
}

/// `DatManager.PortalDat.ReadFromDat<Wave>(id).ExportWave(exportDir)`.
fn wave_export_wave(_w: &World, _file_id: u32, _export_dir: &str) {
    not_ported!("ACE: Wave.ExportWave");
}

/// `DatManager.{PortalDat|HighResDat}.ReadFromDat<Texture>(id).ExportTexture(exportDir)`.
fn texture_export_texture(_w: &World, _high_res: bool, _file_id: u32, _export_dir: &str) {
    not_ported!("ACE: Texture.ExportTexture");
}
