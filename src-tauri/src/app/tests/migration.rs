use super::super::storage::{initialize, settings, snapshot};
use super::*;

#[test]
fn migration_preserves_wal_records_settings_and_rebases_only_owned_copies() {
    let root = std::env::temp_dir().join(format!("tp-migrate-test-{}", uuid::Uuid::new_v4()));
    let source = root.join("com.tp.clipboard");
    let target = root.join("TP");
    let store = initialize(source.clone()).unwrap();
    let assets = source.join("assets/r/files");
    fs::create_dir_all(&assets).unwrap();
    fs::write(assets.join("example.txt"), "attachment").unwrap();
    let original = root.join("original.txt").to_string_lossy().into_owned();
    let stored = assets.join("example.txt").to_string_lossy().into_owned();
    {
        let conn = store.db.lock().unwrap();
        conn.execute_batch(
            "PRAGMA wal_autocheckpoint=0; INSERT INTO groups(id,name,position) VALUES(1,'test',1)",
        )
        .unwrap();
        conn.execute("INSERT INTO records(id,digest,kind,files,favorite,group_id,used_at,storage_mode) VALUES('r','identity','file',?1,1,1,123,'copy')", [serde_json::json!([{"original":original,"stored":stored,"extra":"preserved"}]).to_string()]).unwrap();
    }
    migrate_data_directory(&source, &target).unwrap();
    let migrated = initialize(target.clone()).unwrap();
    let state = snapshot(&migrated).unwrap();
    assert_eq!(state.records.len(), 1);
    assert_eq!(state.groups[0].name, "test");
    assert!(state.records[0].favorite);
    assert_eq!(state.records[0].group_id, Some(1));
    assert_eq!(state.records[0].used_at, 123);
    assert_eq!(state.records[0].files[0].original, original);
    assert_eq!(
        Path::new(state.records[0].files[0].stored.as_deref().unwrap()),
        target.join("assets/r/files/example.txt")
    );
    assert_eq!(
        fs::read_to_string(target.join("assets/r/files/example.txt")).unwrap(),
        "attachment"
    );
    assert_eq!(
        settings(&store.db.lock().unwrap()).unwrap().shortcut,
        settings(&migrated.db.lock().unwrap()).unwrap().shortcut
    );
    let conn = migrated.db.lock().unwrap();
    let (digest, files): (String, String) = conn
        .query_row("SELECT digest,files FROM records WHERE id='r'", [], |r| {
            Ok((r.get(0)?, r.get(1)?))
        })
        .unwrap();
    assert_eq!(digest, "identity");
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&files).unwrap()[0]["extra"],
        "preserved"
    );
    drop(conn);
    // Running again must not import late changes from the legacy database.
    store
        .db
        .lock()
        .unwrap()
        .execute("DELETE FROM records", [])
        .unwrap();
    migrate_data_directory(&source, &target).unwrap();
    assert_eq!(snapshot(&migrated).unwrap().records.len(), 1);
    drop(migrated);
    drop(store);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn migration_without_legacy_data_does_not_create_target() {
    let root = std::env::temp_dir().join(format!("tp-migrate-test-{}", uuid::Uuid::new_v4()));
    migrate_data_directory(&root.join("com.tp.clipboard"), &root.join("TP")).unwrap();
    assert!(!root.exists());
}

#[test]
fn migration_does_not_overwrite_nonempty_target_and_leaves_no_partial_import() {
    let root = std::env::temp_dir().join(format!("tp-migrate-test-{}", uuid::Uuid::new_v4()));
    let source = root.join("com.tp.clipboard");
    let target = root.join("TP");
    let store = initialize(source.clone()).unwrap();
    fs::create_dir(&target).unwrap();
    fs::write(target.join("keep"), "existing").unwrap();
    assert!(migrate_data_directory(&source, &target).is_err());
    assert!(!target.join("history.sqlite3").exists());
    fs::remove_file(target.join("keep")).unwrap();
    // Malformed JSON must fail without publishing an incomplete target.
    store.db.lock().unwrap().execute("INSERT INTO records(id,digest,kind,files,used_at) VALUES('bad','bad','file','invalid',1)", []).unwrap();
    assert!(migrate_data_directory(&source, &target).is_err());
    assert_eq!(fs::read_dir(&target).unwrap().count(), 0);
    assert_eq!(fs::read_dir(&root).unwrap().count(), 2);
    drop(store);
    fs::remove_dir_all(root).unwrap();
}
