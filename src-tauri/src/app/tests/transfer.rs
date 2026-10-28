use super::*;
use crate::app::{
    digest::hash,
    storage::{initialize, snapshot},
    test_support::test_store,
};

fn record(store: &Store, id: &str, kind: &str, body: &str, used: i64) {
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO records(id,digest,kind,body,used_at,created_at) VALUES(?1,?2,?3,?4,?5,?6)",
            params![id, hash(kind, id.as_bytes()), kind, body, used, used - 1000],
        )
        .unwrap();
}
fn pack(store: &Store) -> PathBuf {
    let path = store.dir.join(format!("test-{}.zip", Uuid::new_v4()));
    write_archive(&store.db.lock().unwrap(), &store.dir, &path).unwrap();
    path
}
fn restore(target: &Store, source: &Path, keep: bool, timestamp: i64) -> RestoreResult {
    let mut prepared = prepare_archive(&target.dir, source).unwrap();
    let mut conn = target.db.lock().unwrap();
    let preview = restore_preview(&prepared, &settings(&conn).unwrap(), timestamp).unwrap();
    replace_data(target, &mut conn, &mut prepared, keep, &preview, timestamp).unwrap()
}

#[test]
fn roundtrip_preserves_originals_organization_and_assets_without_settings_or_sensitive_data() {
    let source = test_store();
    let target = test_store();
    let timestamp = now();
    let body = "  原文 <script>alert('x')</script>\r\n \t";
    record(&source, "text", "text", body, timestamp);
    record(&source, "picture", "image", "", timestamp);
    record(&source, "secret", "text", "DO-NOT-EXPORT", timestamp);
    record(&target, "old", "text", "替换前", timestamp);
    fs::create_dir_all(source.dir.join("assets/picture")).unwrap();
    fs::write(
        source.dir.join("assets/picture/image.png"),
        b"original-image-bytes",
    )
    .unwrap();
    fs::create_dir_all(source.dir.join("assets/secret")).unwrap();
    fs::write(
        source.dir.join("assets/secret/secret.txt"),
        b"DO-NOT-EXPORT",
    )
    .unwrap();
    {
        let conn = source.db.lock().unwrap();
        conn.execute(
            "INSERT INTO groups(id,name,position) VALUES(7,'工作',2)",
            [],
        )
        .unwrap();
        conn.execute("UPDATE records SET title='自定义标题',favorite=1,group_id=7,pinned_at=123 WHERE id='text'", []).unwrap();
        conn.execute(
            "UPDATE records SET expires_at=?1 WHERE id='secret'",
            [timestamp + 10000],
        )
        .unwrap();
        let mut config = settings(&conn).unwrap();
        config.max_ordinary = 12;
        config.retention_days = 30;
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
    }
    let backup = pack(&source);
    let mut zip = ZipArchive::new(File::open(&backup).unwrap()).unwrap();
    let mut json = String::new();
    zip.by_name("records.json")
        .unwrap()
        .read_to_string(&mut json)
        .unwrap();
    assert!(!json.contains("DO-NOT-EXPORT"));
    assert!(!json.contains("maxOrdinary"));
    assert!(!json.contains("settings"));
    assert!(zip.by_name("assets/secret/secret.txt").is_err());
    match restore(&target, &backup, true, timestamp) {
        RestoreResult::Completed { records, .. } => {
            assert_eq!(records, 2);
            assert!(!target.dir.join("backups").exists());
        }
        _ => panic!(),
    }
    let current = snapshot(&target).unwrap();
    assert_eq!(current.settings.max_ordinary, 300);
    assert_eq!(current.settings.retention_days, 7);
    let text = current.records.iter().find(|r| r.id == "text").unwrap();
    assert_eq!(text.body, body);
    assert_eq!(text.title, "自定义标题");
    assert!(text.favorite);
    assert_eq!(text.group_id, Some(7));
    assert_eq!(text.pinned_at, Some(123));
    assert_eq!(text.created_at, Some(timestamp - 1000));
    assert_eq!(
        fs::read(target.dir.join("assets/picture/image.png")).unwrap(),
        b"original-image-bytes"
    );
    assert!(!target.dir.join("restore-journal.json").exists());
}

#[test]
fn reference_paths_and_missing_copies_are_preserved_without_reading_user_originals() {
    let source = test_store();
    let target = test_store();
    let timestamp = now();
    record(&source, "reference", "video", "", timestamp);
    record(&source, "copy", "file", "", timestamp);
    let original = source.dir.join("outside-user-file.mp4");
    fs::write(&original, b"NOT-AN-APP-ASSET").unwrap();
    let copied = source.dir.join("assets/copy/files/nested");
    fs::create_dir_all(&copied).unwrap();
    fs::write(copied.join("完整文件.txt"), "文件内容").unwrap();
    let missing = source.dir.join("assets/copy/files/missing.txt");
    {
        let conn = source.db.lock().unwrap();
        conn.execute(
            "UPDATE records SET files=?1,storage_mode='reference' WHERE id='reference'",
            [serde_json::to_string(&vec![FileItem {
                original: original.to_string_lossy().into(),
                stored: None,
                is_directory: false,
            }])
            .unwrap()],
        )
        .unwrap();
        conn.execute(
            "UPDATE records SET files=?1,storage_mode='copy' WHERE id='copy'",
            [serde_json::to_string(&vec![
                FileItem {
                    original: "用户目录".into(),
                    stored: Some(copied.to_string_lossy().into()),
                    is_directory: true,
                },
                FileItem {
                    original: "已丢失文件".into(),
                    stored: Some(missing.to_string_lossy().into()),
                    is_directory: false,
                },
            ])
            .unwrap()],
        )
        .unwrap();
    }
    let backup = pack(&source);
    let prepared = prepare_archive(&target.dir, &backup).unwrap();
    assert!(!prepared
        .manifest
        .assets
        .iter()
        .any(|asset| asset.path.contains("outside-user")));
    assert!(prepared
        .manifest
        .warnings
        .iter()
        .any(|w| w.contains("仅包含原路径")));
    assert!(prepared
        .manifest
        .warnings
        .iter()
        .any(|w| w.contains("文件已不可用")));
    restore(&target, &backup, true, timestamp);
    let records = snapshot(&target).unwrap().records;
    let copy = records.iter().find(|r| r.id == "copy").unwrap();
    assert_eq!(copy.status, "missing");
    assert!(copy.files[0]
        .stored
        .as_ref()
        .unwrap()
        .starts_with(target.dir.to_str().unwrap()));
    assert_eq!(
        fs::read_to_string(target.dir.join("assets/copy/files/nested/完整文件.txt")).unwrap(),
        "文件内容"
    );
    assert_eq!(fs::read(&original).unwrap(), b"NOT-AN-APP-ASSET");
}

#[test]
fn unified_export_is_readable_escaped_and_can_be_imported() {
    let source = test_store();
    record(
        &source,
        "text",
        "text",
        " <script>alert(1)</script>\n ",
        now(),
    );
    let path = pack(&source);
    let mut zip = ZipArchive::new(File::open(&path).unwrap()).unwrap();
    let mut html = String::new();
    zip.by_name("index.html")
        .unwrap()
        .read_to_string(&mut html)
        .unwrap();
    assert!(html.contains("&lt;script&gt;alert(1)&lt;/script&gt;"));
    assert!(!html.contains("<script>"));
    assert!(html.contains("default-src 'none'"));
    assert!(zip.by_name("records.json").is_ok());
    assert!(zip.by_name("manifest.json").is_err());
    let mut json = String::new();
    zip.by_name("records.json")
        .unwrap()
        .read_to_string(&mut json)
        .unwrap();
    let metadata: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(metadata.get("format").is_none());
    let target = test_store();
    restore(&target, &path, true, now());
    assert_eq!(
        snapshot(&target).unwrap().records[0].body,
        " <script>alert(1)</script>\n "
    );
}

#[test]
fn capacity_grows_above_500_before_expiration_policy_and_never_shrinks() {
    let source = test_store();
    let target = test_store();
    let timestamp = now();
    for index in 0..600 {
        record(&source, &format!("row-{index}"), "text", "", timestamp);
    }
    record(&source, "favorite", "text", "", 0);
    source
        .db
        .lock()
        .unwrap()
        .execute("UPDATE records SET favorite=1 WHERE id='favorite'", [])
        .unwrap();
    let backup = pack(&source);
    restore(&target, &backup, false, timestamp);
    assert_eq!(
        settings(&target.db.lock().unwrap()).unwrap().max_ordinary,
        700
    );
    assert_eq!(snapshot(&target).unwrap().records.len(), 601);
    assert!(crate::app::cleanup::cleanup_ids(
        &target.db.lock().unwrap(),
        &snapshot(&source).unwrap().settings,
        timestamp
    )
    .is_ok());
    let small = test_store();
    record(&small, "small", "text", "", timestamp);
    restore(&target, &pack(&small), false, timestamp);
    assert_eq!(
        settings(&target.db.lock().unwrap()).unwrap().max_ordinary,
        700
    );
    settings(&target.db.lock().unwrap())
        .unwrap()
        .validate()
        .unwrap();
}

#[test]
fn expired_records_can_be_cleaned_or_kept_by_updating_only_last_use() {
    let source = test_store();
    let timestamp = now();
    record(&source, "expired", "text", "", timestamp - 90 * 86400000);
    record(&source, "protected", "text", "", timestamp - 90 * 86400000);
    source
        .db
        .lock()
        .unwrap()
        .execute("UPDATE records SET favorite=1 WHERE id='protected'", [])
        .unwrap();
    let backup = pack(&source);
    let remove = test_store();
    let keep = test_store();
    restore(&remove, &backup, false, timestamp);
    assert_eq!(snapshot(&remove).unwrap().records.len(), 1);
    restore(&keep, &backup, true, timestamp);
    let current = snapshot(&keep).unwrap();
    let expired = current.records.iter().find(|r| r.id == "expired").unwrap();
    assert_eq!(expired.used_at, timestamp);
    assert_eq!(expired.created_at, Some(timestamp - 90 * 86400000 - 1000));
    let protected = current
        .records
        .iter()
        .find(|r| r.id == "protected")
        .unwrap();
    assert_eq!(protected.used_at, timestamp - 90 * 86400000);
    assert!(crate::app::cleanup::cleanup_ids(
        &keep.db.lock().unwrap(),
        &current.settings,
        timestamp
    )
    .unwrap()
    .is_empty());
}

fn write_custom(path: &Path, manifest: &Manifest, name: &str, bytes: &[u8]) {
    let mut writer = ZipWriter::new(File::create(path).unwrap());
    writer.start_file("records.json", zip_options()).unwrap();
    writer
        .write_all(&serde_json::to_vec(manifest).unwrap())
        .unwrap();
    writer.start_file("index.html", zip_options()).unwrap();
    writer
        .write_all(b"<!doctype html><title>TP</title>")
        .unwrap();
    writer.start_file(name, zip_options()).unwrap();
    writer.write_all(bytes).unwrap();
    writer.finish().unwrap();
}

#[test]
fn corrupt_hash_traversal_and_future_version_are_rejected_before_replacement() {
    let source = test_store();
    let target = test_store();
    record(&source, "image", "image", "", now());
    record(&target, "keep", "text", "原数据", now());
    let (mut manifest, _) = collect_manifest(&source.db.lock().unwrap(), &source.dir).unwrap();
    let path = source.dir.join("invalid.zip");
    manifest.assets.push(Asset {
        path: "assets/image/image.png".into(),
        directory: false,
        size: 3,
        sha256: "0".repeat(64),
    });
    write_custom(&path, &manifest, "assets/image/image.png", b"bad");
    assert!(prepare_archive(&target.dir, &path)
        .err()
        .unwrap()
        .contains("校验失败"));
    manifest.assets[0].path = "assets/image/../../outside".into();
    write_custom(&path, &manifest, "assets/image/../../outside", b"bad");
    assert!(prepare_archive(&target.dir, &path).is_err());
    manifest.version = 999;
    assert!(validate_manifest(&manifest).is_err());
    assert_eq!(snapshot(&target).unwrap().records[0].body, "原数据");
}

#[test]
fn database_failure_rolls_back_assets_records_and_settings() {
    let source = test_store();
    let target = test_store();
    let timestamp = now();
    record(&source, "new", "text", "新数据", timestamp);
    record(&target, "old", "text", "原数据", timestamp);
    fs::create_dir_all(target.dir.join("assets/old")).unwrap();
    fs::write(target.dir.join("assets/old/file.txt"), "原附件").unwrap();
    let mut prepared = prepare_archive(&target.dir, &pack(&source)).unwrap();
    let mut conn = target.db.lock().unwrap();
    let preview = restore_preview(&prepared, &settings(&conn).unwrap(), timestamp).unwrap();
    conn.execute_batch("CREATE TRIGGER reject_restore BEFORE INSERT ON records BEGIN SELECT RAISE(ABORT,'failure injection'); END;").unwrap();
    assert!(replace_data(&target, &mut conn, &mut prepared, true, &preview, timestamp).is_err());
    drop(conn);
    assert_eq!(snapshot(&target).unwrap().records[0].body, "原数据");
    assert_eq!(
        fs::read_to_string(target.dir.join("assets/old/file.txt")).unwrap(),
        "原附件"
    );
    assert!(!target.dir.join("restore-journal.json").exists());
    assert!(!target.dir.join("backups").exists());
}

#[test]
fn pending_workers_block_backup_instead_of_exporting_partial_files() {
    let store = test_store();
    record(&store, "pending", "file", "", now());
    store
        .db
        .lock()
        .unwrap()
        .execute("UPDATE records SET status='pending' WHERE id='pending'", [])
        .unwrap();
    assert!(collect_manifest(&store.db.lock().unwrap(), &store.dir)
        .err()
        .unwrap()
        .contains("待保存"));
}

#[test]
fn startup_removes_interrupted_inspection_without_touching_current_data() {
    let source = test_store();
    let target = test_store();
    record(&source, "image", "image", "", now());
    fs::create_dir_all(source.dir.join("assets/image")).unwrap();
    fs::write(source.dir.join("assets/image/image.png"), b"image").unwrap();
    record(&target, "old", "text", "原数据", now());
    fs::write(target.dir.join("assets/old.txt"), "原附件").unwrap();
    let mut prepared = prepare_archive(&target.dir, &pack(&source)).unwrap();
    let abandoned = prepared.workspace.path.clone();
    assert!(abandoned.join("assets/image/image.png").exists());
    // Leave the same on-disk state as a process terminated before confirmation.
    prepared.workspace.persistent = true;
    drop(prepared);
    assert!(!target.dir.join("restore-journal.json").exists());
    let dir = target.dir.clone();
    drop(target);
    let reopened = initialize(dir.clone()).unwrap();
    assert!(!abandoned.exists());
    assert_eq!(snapshot(&reopened).unwrap().records[0].body, "原数据");
    assert_eq!(
        fs::read_to_string(dir.join("assets/old.txt")).unwrap(),
        "原附件"
    );
}

#[test]
fn failed_startup_recovery_preserves_workspaces_for_retry() {
    let store = test_store();
    let mut stage = Workspace::new(&store.dir).unwrap();
    stage.persistent = true;
    fs::write(stage.path.join("attachment.txt"), b"keep").unwrap();
    fs::write(
        store.dir.join("restore-journal.json"),
        serde_json::to_vec(&Journal {
            stage: stage.path.file_name().unwrap().to_str().unwrap().into(),
        })
        .unwrap(),
    )
    .unwrap();
    let dir = store.dir.clone();
    drop(store);
    // A missing rollback database must stop recovery before the sweep.
    assert!(initialize(dir.clone()).is_err());
    assert!(dir.join("restore-journal.json").exists());
    assert_eq!(
        fs::read(stage.path.join("attachment.txt")).unwrap(),
        b"keep"
    );
}

#[test]
fn startup_recovers_uncommitted_restore_and_finishes_committed_restore() {
    for committed in [false, true] {
        let store = test_store();
        record(&store, "old", "text", "原数据", now());
        let mut stage = Workspace::new(&store.dir).unwrap();
        stage.persistent = true;
        let mut abandoned = Workspace::new(&store.dir).unwrap();
        abandoned.persistent = true;
        fs::write(abandoned.path.join("partial.bin"), b"partial").unwrap();
        let id = stage
            .path
            .file_name()
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();
        {
            let conn = store.db.lock().unwrap();
            conn.execute(
                "VACUUM INTO ?1",
                [stage.path.join("previous.sqlite3").to_str().unwrap()],
            )
            .unwrap();
            conn.execute_batch(
                "DELETE FROM records; CREATE TABLE transfer_commits(id TEXT PRIMARY KEY);",
            )
            .unwrap();
            if committed {
                conn.execute("INSERT INTO transfer_commits(id) VALUES(?1)", [&id])
                    .unwrap();
            }
        }
        fs::write(store.dir.join("assets/old.txt"), "原附件").unwrap();
        fs::rename(store.dir.join("assets"), stage.path.join("previous-assets")).unwrap();
        fs::create_dir(store.dir.join("assets")).unwrap();
        fs::write(store.dir.join("assets/new.txt"), "新附件").unwrap();
        fs::write(
            store.dir.join("restore-journal.json"),
            serde_json::to_vec(&Journal { stage: id }).unwrap(),
        )
        .unwrap();
        let dir = store.dir.clone();
        drop(store);
        let reopened = initialize(dir.clone()).unwrap();
        if committed {
            assert!(snapshot(&reopened).unwrap().records.is_empty());
            assert!(dir.join("assets/new.txt").exists());
        } else {
            assert_eq!(snapshot(&reopened).unwrap().records[0].body, "原数据");
            assert!(dir.join("assets/old.txt").exists());
        }
        assert!(!dir.join("restore-journal.json").exists());
        assert!(!stage.path.exists());
        assert!(!abandoned.path.exists());
    }
}

#[test]
fn duplicate_directory_alias_cannot_hide_an_unverified_attachment() {
    let source = test_store();
    record(&source, "image", "image", "", now());
    let (mut manifest, _) = collect_manifest(&source.db.lock().unwrap(), &source.dir).unwrap();
    manifest.assets = vec![
        Asset {
            path: "assets/image".into(),
            directory: true,
            size: 0,
            sha256: String::new(),
        },
        Asset {
            path: "assets/image/image.png".into(),
            directory: false,
            size: 3,
            sha256: format!("{:x}", Sha256::digest(b"png")),
        },
    ];
    let path = source.dir.join("alias.zip");
    let mut writer = ZipWriter::new(File::create(&path).unwrap());
    writer.start_file("records.json", zip_options()).unwrap();
    writer
        .write_all(&serde_json::to_vec(&manifest).unwrap())
        .unwrap();
    writer.start_file("index.html", zip_options()).unwrap();
    writer
        .write_all(b"<!doctype html><title>TP</title>")
        .unwrap();
    writer
        .add_directory("assets/image/", zip_options())
        .unwrap();
    writer
        .add_directory("assets/image//", zip_options())
        .unwrap();
    writer.finish().unwrap();
    assert!(prepare_archive(&source.dir, &path).is_err());
}

#[test]
fn archive_time_is_readable_in_local_time_including_leading_zeroes() {
    let date = chrono::DateTime::parse_from_rfc3339("2026-10-28T20:49:11+08:00").unwrap();
    assert_eq!(filename_time(date), "20261028-204911");
    let date = chrono::DateTime::parse_from_rfc3339("2026-01-02T03:04:05-05:00").unwrap();
    assert_eq!(filename_time(date), "20260102-030405");
}

#[test]
fn case_alias_record_ids_are_rejected_before_any_windows_file_operations() {
    let source = test_store();
    record(&source, "A", "image", "", now());
    record(&source, "a", "text", "", 0);
    let (manifest, _) = collect_manifest(&source.db.lock().unwrap(), &source.dir).unwrap();
    assert!(validate_manifest(&manifest).is_err());
}

#[test]
fn import_does_not_create_or_depend_on_automatic_backups() {
    let source = test_store();
    let target = test_store();
    let timestamp = now();
    record(&source, "new", "text", "新数据", timestamp);
    record(&target, "old", "text", "原数据", timestamp);
    let mut prepared = prepare_archive(&target.dir, &pack(&source)).unwrap();
    fs::write(target.dir.join("backups"), b"not-a-directory").unwrap();
    let mut conn = target.db.lock().unwrap();
    let preview = restore_preview(&prepared, &settings(&conn).unwrap(), timestamp).unwrap();
    assert!(replace_data(&target, &mut conn, &mut prepared, true, &preview, timestamp).is_ok());
    drop(conn);
    assert_eq!(snapshot(&target).unwrap().records[0].body, "新数据");
    assert!(!target.dir.join("restore-journal.json").exists());
}

#[test]
fn interrupted_rollback_freezes_reads_and_writes_until_restart_recovery() {
    let store = test_store();
    record(&store, "old", "text", "原数据", now());
    store
        .recovery_required
        .store(true, std::sync::atomic::Ordering::SeqCst);
    assert!(store.read_data().is_err());
    assert!(store.write_data().is_err());
    assert!(snapshot(&store).is_err());
    let dir = store.dir.clone();
    drop(store);
    let reopened = initialize(dir).unwrap();
    assert_eq!(snapshot(&reopened).unwrap().records[0].body, "原数据");
}
