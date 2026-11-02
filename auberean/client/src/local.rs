//! The machine's clock and the player's files for the game's scripts: the local time from the
//! system, and files kept in the settings folder by bare names (the September 2013 client keeps
//! them beside itself).

use std::io::Write as _;
use std::path::PathBuf;

use auberean_client_sdk::script::local::{bare_name, LocalSystem, LocalTime};
use chrono::{Datelike as _, Timelike as _};

/// The settings folder as the scripts' files' home.
#[derive(Debug)]
pub struct SettingsFolder {
    pub dir: PathBuf,
}

impl SettingsFolder {
    /// The path of a file by a bare name; none for any other name.
    fn path(&self, file: &str) -> Option<PathBuf> {
        bare_name(file).then(|| self.dir.join(file))
    }
}

impl LocalSystem for SettingsFolder {
    fn folder(&self) -> Option<String> {
        Some(self.dir.display().to_string())
    }

    fn local_time(&self) -> Option<LocalTime> {
        let now = chrono::Local::now();
        Some(LocalTime {
            year: u16::try_from(now.year()).ok()?,
            month: u8::try_from(now.month()).ok()?,
            day: u8::try_from(now.day()).ok()?,
            weekday: u8::try_from(now.weekday().num_days_from_sunday()).ok()?,
            hour: u8::try_from(now.hour()).ok()?,
            minute: u8::try_from(now.minute()).ok()?,
            second: u8::try_from(now.second()).ok()?,
            millisecond: u16::try_from(now.timestamp_subsec_millis().min(999)).ok()?,
        })
    }

    fn real_time(&self) -> Option<i64> {
        Some(chrono::Utc::now().timestamp())
    }

    fn append_line(&mut self, file: &str, bytes: &[u8]) -> bool {
        let Some(path) = self.path(file) else {
            return false;
        };
        let _ = std::fs::create_dir_all(&self.dir);
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .and_then(|mut f| f.write_all(bytes))
            .map_err(|e| tracing::warn!("adding to {file}: {e}"))
            .is_ok()
    }

    fn write_file(&mut self, file: &str, bytes: &[u8]) -> bool {
        let Some(path) = self.path(file) else {
            return false;
        };
        let _ = std::fs::create_dir_all(&self.dir);
        std::fs::write(path, bytes)
            .map_err(|e| tracing::warn!("writing {file}: {e}"))
            .is_ok()
    }

    fn read_file(&mut self, file: &str) -> Option<Vec<u8>> {
        std::fs::read(self.path(file)?).ok()
    }
}
