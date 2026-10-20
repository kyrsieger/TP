use super::super::common::now;
use super::super::test_support::test_store;
use super::*;
use uuid::Uuid;

#[test]
fn old_groups_gain_persistent_order_and_new_groups_append() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("history.sqlite3")).unwrap();
    conn.execute_batch("CREATE TABLE groups (id INTEGER PRIMARY KEY, name TEXT NOT NULL UNIQUE); INSERT INTO groups(id,name) VALUES(8,'资料'),(2,'工作');").unwrap();
    drop(conn);
    let store = initialize(dir.clone()).unwrap();
    assert_eq!(
        snapshot(&store)
            .unwrap()
            .groups
            .iter()
            .map(|g| g.id)
            .collect::<Vec<_>>(),
        vec![2, 8]
    );
    {
        let conn = store.db.lock().unwrap();
        conn.execute_batch(
            "UPDATE groups SET position=0 WHERE id=8; UPDATE groups SET position=1 WHERE id=2;",
        )
        .unwrap();
        conn.execute(
            "INSERT INTO groups(name,position) SELECT ?1,COALESCE(MAX(position),-1)+1 FROM groups",
            ["新增"],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    assert_eq!(
        snapshot(&store)
            .unwrap()
            .groups
            .iter()
            .map(|g| g.name.as_str())
            .collect::<Vec<_>>(),
        vec!["资料", "工作", "新增"]
    );
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn font_choice_survives_database_restart() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        let mut config = settings(&conn).unwrap();
        config.font_family = "Microsoft YaHei".into();
        config.font_style = super::super::fonts::FontStyle {
            name: "Light".into(),
            family: "Microsoft YaHei Light".into(),
            weight: 290,
            ..Default::default()
        };
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    assert_eq!(
        settings(&store.db.lock().unwrap()).unwrap().font_family,
        "Microsoft YaHei"
    );
    let restored = settings(&store.db.lock().unwrap()).unwrap();
    assert_eq!(restored.font_style.name, "Light");
    assert_eq!(restored.font_style.weight, 290);
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn theme_migrates_old_settings_and_survives_restart() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        let mut old = serde_json::to_value(settings(&conn).unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("theme");
        old["maxOrdinary"] = 100.into();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [old.to_string()],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    {
        let conn = store.db.lock().unwrap();
        let mut config = settings(&conn).unwrap();
        assert_eq!(config.theme, "indigo");
        assert_eq!(config.max_ordinary, 100);
        config.theme = "forest-night".into();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    assert_eq!(
        settings(&store.db.lock().unwrap()).unwrap().theme,
        "forest-night"
    );
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn rename_shortcut_migrates_old_settings_and_survives_restart() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        let mut old = serde_json::to_value(settings(&conn).unwrap()).unwrap();
        old.as_object_mut().unwrap().remove("renameShortcut");
        old["maxOrdinary"] = 100.into();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [old.to_string()],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    {
        let conn = store.db.lock().unwrap();
        let mut config = settings(&conn).unwrap();
        assert_eq!(config.rename_shortcut, "F2");
        assert_eq!(config.max_ordinary, 100);
        config.rename_shortcut = "Control+Shift+R".into();
        config.validate().unwrap();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    assert_eq!(
        settings(&store.db.lock().unwrap()).unwrap().rename_shortcut,
        "Control+Shift+R"
    );
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn snapshot_identifies_existing_directories_and_preserves_captured_type() {
    let store = test_store();
    let dir = store.dir.join("archive.txt");
    fs::create_dir_all(&dir).unwrap();
    let path = dir.to_string_lossy().into_owned();
    let legacy = serde_json::json!([{ "original": path, "stored": null }]).to_string();
    let captured = serde_json::to_string(&vec![FileItem {
        original: path,
        stored: None,
        is_directory: true,
    }])
    .unwrap();
    {
        let conn = store.db.lock().unwrap();
        for (id, files) in [("legacy", legacy), ("captured", captured)] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,files,used_at) VALUES(?1,?1,'file',?2,1)",
                params![id, files],
            )
            .unwrap();
        }
    }
    let records = snapshot(&store).unwrap().records;
    assert!(records.iter().all(|record| record.files[0].is_directory));
    fs::remove_dir_all(&dir).unwrap();
    let records = snapshot(&store).unwrap().records;
    assert!(
        records
            .iter()
            .find(|record| record.id == "captured")
            .unwrap()
            .files[0]
            .is_directory
    );
    drop(store);
    fs::remove_dir_all(dir.parent().unwrap()).unwrap();
}

#[test]
fn existing_hex_text_becomes_color_without_losing_metadata() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        conn.execute("INSERT INTO groups(id,name) VALUES(1,'调色板')", [])
            .unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,body,used_at,favorite,group_id) VALUES('old',?1,'text','#AbC',?2,1,1)",
                params![hash("text", b"#AbC"), now()],
            ).unwrap();
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    let conn = store.db.lock().unwrap();
    let (kind, digest, favorite, group_id): (String, String, i64, i64) = conn
        .query_row(
            "SELECT kind,digest,favorite,group_id FROM records WHERE id='old'",
            [],
            |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
        )
        .unwrap();
    assert_eq!(kind, "color");
    assert_eq!(digest, hash("color", b"#AbC"));
    assert_eq!((favorite, group_id), (1, 1));
    drop(conn);
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn old_records_keep_creation_time_empty_after_migration() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("history.sqlite3")).unwrap();
    conn.execute_batch(
        "CREATE TABLE records (
            id TEXT PRIMARY KEY, digest TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '', files TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0, pinned_at INTEGER, group_id INTEGER,
            used_at INTEGER NOT NULL, expires_at INTEGER, storage_mode TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'ready', preview INTEGER NOT NULL DEFAULT 0
        );
        INSERT INTO records(id,digest,kind,used_at) VALUES('old','old','text',1000);",
    )
    .unwrap();
    drop(conn);
    let store = initialize(dir.clone()).unwrap();
    let records = snapshot(&store).unwrap().records;
    assert_eq!(records[0].created_at, None);
    assert_eq!(records[0].title, "");
    store.db.lock().unwrap().execute(
            "INSERT INTO records(id,digest,kind,used_at,created_at) VALUES('new','new','text',2000,2000)",
            [],
        ).unwrap();
    assert_eq!(snapshot(&store).unwrap().records[0].created_at, Some(2000));
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn migrate_flat_file_copies_flattens_numbered_copy_directories() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    let store = initialize(dir.clone()).unwrap();
    let files_dir = store.dir.join("assets").join("legacy").join("files");
    let old_first = files_dir.join("0").join("first.txt");
    let old_second = files_dir.join("1").join("first.txt");
    fs::create_dir_all(old_first.parent().unwrap()).unwrap();
    fs::create_dir_all(old_second.parent().unwrap()).unwrap();
    fs::write(&old_first, b"first").unwrap();
    fs::write(&old_second, b"second").unwrap();
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,files,storage_mode,used_at) VALUES('legacy','legacy','file',?1,'copy',1)",
                [serde_json::to_string(&[
                    FileItem {
                        original: "C:\\a\\first.txt".into(),
                        stored: Some(old_first.to_string_lossy().into_owned()),
                        is_directory: false,
                    },
                    FileItem {
                        original: "C:\\b\\first.txt".into(),
                        stored: Some(old_second.to_string_lossy().into_owned()),
                        is_directory: false,
                    },
                ])
                .unwrap()],
            )
            .unwrap();
    }
    drop(store);

    let store = initialize(dir.clone()).unwrap();
    let stored = {
        let conn = store.db.lock().unwrap();
        let json: String = conn
            .query_row("SELECT files FROM records WHERE id='legacy'", [], |r| {
                r.get(0)
            })
            .unwrap();
        serde_json::from_str::<Vec<FileItem>>(&json)
            .unwrap()
            .into_iter()
            .map(|file| PathBuf::from(file.stored.unwrap()))
            .collect::<Vec<_>>()
    };
    assert_eq!(stored[0], files_dir.join("first.txt"));
    assert_eq!(stored[1], files_dir.join("first (2).txt"));
    assert_eq!(stored[0].parent(), stored[1].parent());
    assert!(stored.iter().all(|path| path.is_file()));
    // 旧序号目录已清理，重复迁移是幂等的。
    assert!(!files_dir.join("0").exists());
    assert!(!files_dir.join("1").exists());
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    let again: Vec<FileItem> = {
        let conn = store.db.lock().unwrap();
        let json: String = conn
            .query_row("SELECT files FROM records WHERE id='legacy'", [], |r| {
                r.get(0)
            })
            .unwrap();
        serde_json::from_str(&json).unwrap()
    };
    assert_eq!(
        PathBuf::from(again[0].stored.as_ref().unwrap()),
        files_dir.join("first.txt")
    );
    let store_dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(store_dir).unwrap();
}

#[test]
fn existing_records_gain_nullable_pin_time() {
    let dir = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).unwrap();
    let conn = Connection::open(dir.join("history.sqlite3")).unwrap();
    conn.execute_batch(
        "CREATE TABLE records (
            id TEXT PRIMARY KEY, digest TEXT NOT NULL UNIQUE, kind TEXT NOT NULL,
            body TEXT NOT NULL DEFAULT '', files TEXT NOT NULL DEFAULT '[]',
            favorite INTEGER NOT NULL DEFAULT 0, group_id INTEGER,
            used_at INTEGER NOT NULL, expires_at INTEGER, storage_mode TEXT NOT NULL DEFAULT '',
            status TEXT NOT NULL DEFAULT 'ready', preview INTEGER NOT NULL DEFAULT 0
        ); INSERT INTO records(id,digest,kind,used_at) VALUES('old','old','text',1);",
    )
    .unwrap();
    drop(conn);

    let store = initialize(dir.clone()).unwrap();
    let records = snapshot(&store).unwrap().records;
    assert_eq!(records.len(), 1);
    assert_eq!(records[0].pinned_at, None);
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
