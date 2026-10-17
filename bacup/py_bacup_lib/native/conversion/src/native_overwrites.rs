//! FO76 native assets, "replace FO4's matching materials": the files a run wrote over FO4's paths
//! are recorded, so a later run without the option removes them instead of leaving FO76 files
//! shadowing FO4's. Only recorded files are ever removed.

use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

const MARKER: &str = "debug/fo4_overwrite_outputs.txt";

pub type OutputSnapshot = HashMap<String, Option<SystemTime>>;

fn data_path(mod_root: &Path, key: &str) -> PathBuf {
    mod_root
        .join("data")
        .join(key.replace('/', std::path::MAIN_SEPARATOR_STR))
}

fn modified(path: &Path) -> Option<SystemTime> {
    std::fs::metadata(path).ok()?.modified().ok()
}

fn recorded(mod_root: &Path) -> BTreeSet<String> {
    std::fs::read_to_string(mod_root.join(MARKER))
        .unwrap_or_default()
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_owned)
        .collect()
}

fn write_recorded(mod_root: &Path, keys: &BTreeSet<String>) -> std::io::Result<()> {
    let path = mod_root.join(MARKER);
    if keys.is_empty() {
        return match std::fs::remove_file(&path) {
            Err(error) if error.kind() != std::io::ErrorKind::NotFound => Err(error),
            _ => Ok(()),
        };
    }
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut text = keys.iter().cloned().collect::<Vec<_>>().join("\n");
    text.push('\n');
    std::fs::write(path, text)
}

/// Removes recorded overwrites that are no longer in `active`; returns how many files went.
pub fn remove_stale_overwrites(
    mod_root: &Path,
    active: &HashSet<String>,
) -> std::io::Result<usize> {
    let recorded = recorded(mod_root);
    let mut removed = 0;
    for key in recorded.iter().filter(|key| !active.contains(*key)) {
        let path = data_path(mod_root, key);
        if path.is_file() {
            std::fs::remove_file(path)?;
            removed += 1;
        }
    }
    let kept = recorded
        .into_iter()
        .filter(|key| active.contains(key))
        .collect();
    write_recorded(mod_root, &kept)?;
    Ok(removed)
}

/// Taken before a phase writes, so `record_written_overwrites` can tell which files it wrote.
pub fn snapshot_overwrites(mod_root: &Path, active: &HashSet<String>) -> OutputSnapshot {
    active
        .iter()
        .map(|key| (key.clone(), modified(&data_path(mod_root, key))))
        .collect()
}

pub fn record_written_overwrites(mod_root: &Path, before: &OutputSnapshot) -> std::io::Result<()> {
    let mut keys = recorded(mod_root);
    for (key, was) in before {
        let now = modified(&data_path(mod_root, key));
        if now.is_some() && now != *was {
            keys.insert(key.clone());
        }
    }
    write_recorded(mod_root, &keys)
}

/// Phase entry: drops overwrites the run no longer makes, then snapshots the ones it may write.
pub(crate) fn begin_phase(ctx: &crate::phase::PhaseCtx<'_>, phase: &'static str) -> OutputSnapshot {
    let active = &ctx.run.native_overwrite_paths;
    let message = match remove_stale_overwrites(ctx.mod_path, active) {
        Ok(0) => None,
        Ok(removed) => Some((
            crate::phase::LogLevel::Info,
            format!("removed {removed} FO76 file(s) a previous run wrote over FO4's"),
        )),
        Err(error) => Some((
            crate::phase::LogLevel::Warn,
            format!("removing previous FO4 overwrites: {error}"),
        )),
    };
    if let Some((level, message)) = message {
        let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level,
            message,
        });
    }
    snapshot_overwrites(ctx.mod_path, active)
}

pub(crate) fn finish_phase(
    ctx: &crate::phase::PhaseCtx<'_>,
    phase: &'static str,
    before: &OutputSnapshot,
) {
    if let Err(error) = record_written_overwrites(ctx.mod_path, before) {
        let _ = ctx.run.event_tx.try_send(crate::phase::PhaseEvent::Log {
            phase,
            level: crate::phase::LogLevel::Warn,
            message: format!("recording FO4 overwrites: {error}"),
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(mod_root: &Path, key: &str, bytes: &[u8]) {
        let path = data_path(mod_root, key);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }

    #[test]
    fn a_run_without_the_option_removes_only_the_overwrites_a_run_wrote() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let active: HashSet<String> = [
            "materials/rocks/rock01.bgsm",
            "textures/rocks/rock01_d.dds",
            "textures/rocks/untouched_d.dds",
        ]
        .into_iter()
        .map(str::to_owned)
        .collect();
        write(root, "textures/rocks/untouched_d.dds", b"already here");

        assert_eq!(remove_stale_overwrites(root, &active).unwrap(), 0);
        let before = snapshot_overwrites(root, &active);
        write(root, "materials/rocks/rock01.bgsm", b"fo76");
        write(root, "textures/rocks/rock01_d.dds", b"fo76");
        record_written_overwrites(root, &before).unwrap();

        let still_active: HashSet<String> = ["textures/rocks/rock01_d.dds".to_owned()].into();
        assert_eq!(remove_stale_overwrites(root, &still_active).unwrap(), 1);
        assert!(!data_path(root, "materials/rocks/rock01.bgsm").exists());
        assert!(data_path(root, "textures/rocks/rock01_d.dds").exists());

        assert_eq!(remove_stale_overwrites(root, &HashSet::new()).unwrap(), 1);
        assert!(!data_path(root, "textures/rocks/rock01_d.dds").exists());
        assert!(data_path(root, "textures/rocks/untouched_d.dds").exists());
        assert!(!root.join(MARKER).exists());
    }
}
