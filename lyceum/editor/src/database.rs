//! Fetch the server's world database into its user cache, away from the frame loop.

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver, TryRecvError};

/// The import tool's existing fetch command, for the world database of `era`. It chooses the
/// pinned database release.
#[must_use]
pub fn command(import: &Path, cache: &Path, era: dereth_primitives::EraId) -> Command {
    let mut cmd = Command::new(import);
    cmd.arg("fetch");
    if era == dereth_primitives::EraId::Infiltration {
        cmd.args(["--world", "16py"]);
    }
    cmd.arg("--dir").arg(cache).stdin(Stdio::null());
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x0800_0000);
    }
    cmd
}

/// A download in progress. The frame loop reads completion without waiting.
#[derive(Debug)]
pub struct Fetch {
    rx: Receiver<Result<PathBuf, String>>,
}

impl Fetch {
    /// Start the bundled importer, then find the SQL file it left in the cache.
    ///
    /// # Errors
    /// The worker thread could not start.
    pub fn start(
        import: PathBuf,
        cache: PathBuf,
        era: dereth_primitives::EraId,
    ) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel();
        std::thread::Builder::new()
            .name("world database".into())
            .spawn(move || {
                let result = command(&import, &cache, era)
                    .output()
                    .map_err(|e| e.to_string())
                    .and_then(|output| {
                        if !output.status.success() {
                            return Err(String::from_utf8_lossy(&output.stderr).trim().to_owned());
                        }
                        crate::catalog::newest_database_for(
                            &cache,
                            lyceum_foundry::export::Era::of(era),
                        )
                        .ok_or_else(|| "the download left no world database in the cache".into())
                    });
                let _ = tx.send(result);
            })
            .map_err(|e| e.to_string())?;
        Ok(Self { rx })
    }

    /// A completion event, or `None` while the worker is running.
    #[must_use]
    pub fn poll(&self) -> Option<Result<PathBuf, String>> {
        match self.rx.try_recv() {
            Ok(result) => Some(result),
            Err(TryRecvError::Empty) => None,
            Err(TryRecvError::Disconnected) => {
                Some(Err("the database download ended without a result".into()))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Behaviour: none (tooling: database fetch arguments and event-driven completion)
    use super::*;

    #[test]
    fn fetching_uses_the_named_importer_and_cache_without_running_a_server() {
        let (import, cache) = (
            Path::new("bin/empyrean-import"),
            Path::new("cache/world-database"),
        );
        let cmd = command(import, cache, dereth_primitives::EraId::Eor);
        assert_eq!(cmd.get_program(), "bin/empyrean-import");
        let args: Vec<_> = cmd.get_args().map(|s| s.to_str().unwrap()).collect();
        assert_eq!(args, ["fetch", "--dir", "cache/world-database"]);
        // An Infiltration world's is the February 2005 world database.
        let cmd = command(import, cache, dereth_primitives::EraId::Infiltration);
        let args: Vec<_> = cmd.get_args().map(|s| s.to_str().unwrap()).collect();
        assert_eq!(
            args,
            ["fetch", "--world", "16py", "--dir", "cache/world-database"]
        );
    }

    #[test]
    fn completion_arrives_as_an_event_and_a_lost_worker_is_an_error() {
        let (tx, rx) = mpsc::channel();
        let fetch = Fetch { rx };
        assert!(fetch.poll().is_none());
        let path = PathBuf::from("cache/world.sql");
        tx.send(Ok(path.clone())).unwrap();
        assert_eq!(fetch.poll(), Some(Ok(path)));
        tx.send(Err("download refused".into())).unwrap();
        assert_eq!(fetch.poll(), Some(Err("download refused".into())));
        drop(tx);
        assert!(fetch.poll().unwrap().is_err());
    }
}
