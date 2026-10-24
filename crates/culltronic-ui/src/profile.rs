//! Windows window-state storage; Linux retains eframe's existing location.

#[cfg(any(windows, test))]
use std::path::Path;

/// Keep window state beside the existing local index/cache without renaming it.
pub(crate) fn window_state_path() -> anyhow::Result<Option<std::path::PathBuf>> {
    #[cfg(windows)]
    {
        let root = dirs::cache_dir()
            .ok_or_else(|| anyhow::anyhow!("Cannot locate Local AppData"))?
            .join("cullr");
        std::fs::create_dir_all(&root)?;
        let path = root.join("window.ron");
        if let Some(old) = eframe::storage_dir("Culltronic") {
            migrate_window_state(&old.join("app.ron"), &path)?;
        }
        Ok(Some(path))
    }
    #[cfg(not(windows))]
    Ok(None)
}

/// Copy once, atomically and without replacing newer local state; retain backup.
#[cfg(any(windows, test))]
fn migrate_window_state(old: &Path, new: &Path) -> anyhow::Result<()> {
    if new.try_exists()? || !old.try_exists()? {
        return Ok(());
    }
    let parent = new
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Invalid window state path"))?;
    std::fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    std::io::copy(&mut std::fs::File::open(old)?, &mut temp)?;
    temp.as_file().sync_all()?;
    match temp.persist_noclobber(new) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == std::io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error.into()),
    }
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn migrates_once_preserving_old_and_new_state() {
        let dir = tempfile::tempdir().unwrap();
        let old = dir.path().join("roaming.ron");
        let new = dir.path().join("cullr/window.ron");
        migrate_window_state(&old, &new).unwrap();
        assert!(!new.exists());
        std::fs::write(&old, b"old geometry").unwrap();
        migrate_window_state(&old, &new).unwrap();
        assert_eq!(std::fs::read(&new).unwrap(), b"old geometry");
        assert!(old.exists());
        std::fs::write(&new, b"new geometry").unwrap();
        migrate_window_state(&old, &new).unwrap();
        assert_eq!(std::fs::read(&new).unwrap(), b"new geometry");
    }
}
