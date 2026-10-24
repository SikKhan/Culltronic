//! Space/non-English paths must round-trip through scanning, storage and export.

#![expect(clippy::unwrap_used)]

use std::sync::atomic::AtomicBool;

use culltronic_core::{Db, Label, ScanOptions, export_files, scan_folder};

#[test]
fn unicode_pair_identity_and_labels_survive_reopen_and_export() {
    let dir = tempfile::tempdir().unwrap();
    let source = dir.path().join("旅行 photos");
    let destination = dir.path().join("精选 export");
    let profile = dir.path().join("用户 profile/cullr/index.db");
    std::fs::create_dir_all(&source).unwrap();
    std::fs::create_dir_all(&destination).unwrap();
    let raw = source.join("照片 01.CR3");
    let jpeg = source.join("照片 01.JPG");
    std::fs::write(&raw, b"RAW placeholder").unwrap();
    std::fs::write(&jpeg, b"JPEG placeholder").unwrap();
    let options = ScanOptions::default();
    let scanned = scan_folder(&source, options).unwrap();
    assert_eq!(scanned.len(), 1);
    assert_eq!(scanned[0].jpeg_rel_path, Some("照片 01.JPG".into()));
    let db = Db::open(&profile).unwrap();
    let entries = db.sync_scan(&source, &scanned, options).unwrap();
    let id = entries[0].id;
    db.set_label(id, Label::Green).unwrap();
    db.set_rotation(id, 1).unwrap();
    db.touch_root(&source, 42).unwrap();
    db.kv_set("auto_advance", "true").unwrap();
    drop(db);
    let reopened = Db::open(&profile).unwrap();
    let fresh = scan_folder(&source, options).unwrap();
    let entries = reopened.sync_scan(&source, &fresh, options).unwrap();
    assert_eq!(entries[0].id, id);
    assert_eq!(entries[0].label, Label::Green);
    assert_eq!(entries[0].rot_cw, 1);
    assert_eq!(reopened.kv_get("auto_advance").unwrap().as_deref(), Some("true"));
    assert_eq!(reopened.recent_roots(1).unwrap(), vec![source]);
    let report = export_files(&[raw, jpeg], &destination, &AtomicBool::new(false), |_| {}).unwrap();
    assert_eq!(report.copied, 2);
    assert!(report.failures.is_empty());
    assert_eq!(
        std::fs::read(destination.join("照片 01.CR3")).unwrap(),
        b"RAW placeholder"
    );
    assert_eq!(
        std::fs::read(destination.join("照片 01.JPG")).unwrap(),
        b"JPEG placeholder"
    );
}
