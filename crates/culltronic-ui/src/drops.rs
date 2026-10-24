//! External file drops: validate the batch before changing the mounted folder.

use std::collections::HashSet;
use std::path::PathBuf;

use culltronic_core::{PhotoEntry, PhotoId};

/// Map both pair members to one tile, preserving the dropped order for focus.
pub(crate) fn photo_ids(entries: &[PhotoEntry], files: &[PathBuf]) -> Result<Vec<PhotoId>, String> {
    let mut ids = Vec::new();
    for file in files {
        let entry = entries.iter().find(|entry| {
            entry.rel_path == *file || entry.jpeg_rel_path.as_ref() == Some(file)
        }).ok_or_else(|| format!("{} is unavailable, unsupported, or has no matching RAW photo.", file.display()))?;
        if !ids.contains(&entry.id) {
            ids.push(entry.id);
        }
    }
    Ok(ids)
}

/// A folder to open and original file names to resolve after scanning.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct DropRequest {
    pub root: PathBuf,
    pub files: Vec<PathBuf>,
}

/// Reject ambiguous batches as a whole; a rejected drop must not lose selection.
pub(crate) fn resolve(paths: Vec<PathBuf>) -> Result<DropRequest, String> {
    if paths.is_empty() {
        return Err("Drop a folder or RAW photos from one folder.".into());
    }
    if paths.len() == 1 && paths[0].is_dir() {
        return Ok(DropRequest { root: paths[0].clone(), files: Vec::new() });
    }
    let mut root = None;
    let mut files = Vec::new();
    let mut seen = HashSet::new();
    for path in paths {
        let metadata = path.metadata().map_err(|_| format!("Cannot access {}.", path.display()))?;
        if !metadata.is_file() {
            return Err("Drop one folder, or photos from one folder; do not mix folders and files.".into());
        }
        let extension = path.extension().unwrap_or_default().as_encoded_bytes();
        if !culltronic_core::is_supported_extension(extension)
            && !extension.eq_ignore_ascii_case(b"jpg")
            && !extension.eq_ignore_ascii_case(b"jpeg")
        {
            return Err(format!("Unsupported file: {}. Drop RAW photos or their companion JPEGs.", path.display()));
        }
        let parent = path.parent().ok_or("Cannot determine the photo's folder.")?;
        if root.as_deref().is_some_and(|root| root != parent) {
            return Err("The dropped photos must come from the same folder.".into());
        }
        root = Some(parent.to_owned());
        let file = path.file_name().ok_or("Cannot determine the photo's filename.")?;
        if seen.insert(file.to_owned()) {
            files.push(PathBuf::from(file));
        }
    }
    Ok(DropRequest { root: root.ok_or("No photos were dropped.")?, files })
}

#[cfg(test)]
mod tests {
    #![expect(clippy::unwrap_used)]
    use super::*;

    #[test]
    fn accepts_folder_and_unicode_photos_and_deduplicates() {
        let dir = tempfile::tempdir().unwrap();
        let photo = dir.path().join("旅行 photo.CR3");
        std::fs::write(&photo, b"raw").unwrap();
        assert!(resolve(vec![dir.path().into()]).unwrap().files.is_empty());
        assert_eq!(resolve(vec![photo.clone(), photo]).unwrap().files, vec![PathBuf::from("旅行 photo.CR3")]);
    }

    #[test]
    fn rejects_mixed_folders_unsupported_and_missing_files() {
        let a = tempfile::tempdir().unwrap();
        let b = tempfile::tempdir().unwrap();
        let first = a.path().join("a.NEF");
        let second = b.path().join("b.ARW");
        std::fs::write(&first, b"raw").unwrap();
        std::fs::write(&second, b"raw").unwrap();
        assert!(resolve(vec![first.clone(), second]).is_err());
        assert!(resolve(vec![first, a.path().into()]).is_err());
        let text = a.path().join("notes.txt");
        std::fs::write(&text, b"x").unwrap();
        assert!(resolve(vec![text]).is_err());
        assert!(resolve(vec![a.path().join("missing.CR3")]).is_err());
        assert!(resolve(Vec::new()).is_err());
    }
}
