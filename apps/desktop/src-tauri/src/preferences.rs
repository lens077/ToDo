//! Launch-to-launch desktop preferences.
//!
//! Only the last opened project lives here. It sits next to the Capture inbox
//! in `~/.todo/` because both are personal, per-machine state that never enters
//! a repository. A missing or corrupt file simply means "nothing remembered":
//! the desktop must still launch and let the user pick a project.

use std::{
    fs,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct DesktopPreferences {
    /// Path to the task file (or its directory) that was open when the desktop
    /// last wrote preferences.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_project: Option<PathBuf>,
}

pub fn default_path() -> Option<PathBuf> {
    todo_core::global_capture_path()
        .ok()
        .and_then(|inbox| inbox.parent().map(|dir| dir.join("desktop.json")))
}

pub fn read(path: &Path) -> DesktopPreferences {
    fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

pub fn write(path: &Path, preferences: &DesktopPreferences) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let encoded = serde_json::to_vec_pretty(preferences).map_err(std::io::Error::other)?;
    // Write-then-rename so a crash mid-write leaves the previous file intact.
    let temp = path.with_extension("json.tmp");
    fs::write(&temp, encoded)?;
    fs::rename(&temp, path)
}

pub fn remember_project(path: Option<&Path>, project: &Path) {
    let Some(path) = path else { return };
    let mut preferences = read(path);
    if preferences.last_project.as_deref() == Some(project) {
        return;
    }
    preferences.last_project = Some(project.to_path_buf());
    // Failing to remember is not worth interrupting the user who just opened a
    // project successfully; the next launch simply starts empty.
    let _ = write(path, &preferences);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_and_tolerates_garbage() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("nested").join("desktop.json");
        assert_eq!(read(&file), DesktopPreferences::default());

        remember_project(Some(&file), Path::new("/repo/TODO.md"));
        assert_eq!(
            read(&file).last_project.as_deref(),
            Some(Path::new("/repo/TODO.md"))
        );

        fs::write(&file, b"{not json").unwrap();
        assert_eq!(read(&file), DesktopPreferences::default());
    }
}
