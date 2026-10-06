use super::super::model::Record;
use super::super::storage::snapshot;
use super::super::test_support::test_store;
use super::*;

#[test]
fn cleanup_keeps_favorites_and_grouped_records() {
    let store = test_store();
    {
        let conn = store.db.lock().unwrap();
        conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
            .unwrap();
        for (id, favorite, group_id, expires_at) in [
            ("old", 0, None, None),
            ("sensitive", 0, None, Some(now() - 1)),
            ("favorite", 1, None, Some(now() - 1)),
            ("grouped", 0, Some(1), Some(now() - 1)),
        ] {
            conn.execute("INSERT INTO records(id,digest,kind,used_at,favorite,group_id,expires_at) VALUES(?1,?1,'text',?2,?3,?4,?5)",
                    params![id, now() - 8 * 86_400_000, favorite, group_id, expires_at]).unwrap();
        }
    }
    assert!(clean(&store).unwrap());
    let mut remaining: Vec<String> = snapshot(&store)
        .unwrap()
        .records
        .into_iter()
        .map(|r| r.id)
        .collect();
    remaining.sort();
    assert_eq!(remaining, vec!["favorite", "grouped"]);
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn count_limit_only_applies_to_ordinary_records() {
    let store = test_store();
    {
        let conn = store.db.lock().unwrap();
        let mut value = settings(&conn).unwrap();
        value.max_ordinary = 1;
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&value).unwrap()],
        )
        .unwrap();
        let base = now();
        for (id, age, favorite) in [("new", 1, 0), ("old", 2, 0), ("saved", 3, 1)] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at,favorite) VALUES(?1,?1,'text',?2,?3)",
                params![id, base - age * 1000, favorite],
            )
            .unwrap();
        }
    }
    assert!(clean(&store).unwrap());
    let remaining: Vec<String> = snapshot(&store)
        .unwrap()
        .records
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(remaining, vec!["new", "saved"]);
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_cleanup_confirmation_counts_deletions_without_applying_settings() {
    let store = test_store();
    let timestamp = now();
    {
        let conn = store.db.lock().unwrap();
        conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
            .unwrap();
        for (id, used_at, favorite, group_id, expires_at) in [
            ("new", timestamp - 1000, 0, None, None),
            ("second", timestamp - 2000, 0, None, None),
            ("third", timestamp - 3000, 0, None, None),
            ("old", timestamp - 2 * 86_400_000, 0, None, None),
            ("expired", timestamp - 500, 0, None, Some(timestamp)),
            ("favorite", timestamp - 31 * 86_400_000, 1, None, None),
            ("grouped", timestamp - 31 * 86_400_000, 0, Some(1), None),
        ] {
            conn.execute("INSERT INTO records(id,digest,kind,used_at,favorite,group_id,expires_at) VALUES(?1,?1,'text',?2,?3,?4,?5)",
                    params![id, used_at, favorite, group_id, expires_at]).unwrap();
        }
        let original = settings(&conn).unwrap();
        assert_eq!(
            settings_cleanup_confirmation(&conn, &original, timestamp, 0).unwrap(),
            None
        );
        let mut value = original.clone();
        value.retention_days = 1;
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 0).unwrap(),
            Some(2)
        );
        value.max_ordinary = 2;
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 0).unwrap(),
            Some(3)
        );
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 3).unwrap(),
            None
        );
        assert_eq!(settings(&conn).unwrap().max_ordinary, original.max_ordinary);
        assert_eq!(
            settings(&conn).unwrap().retention_days,
            original.retention_days
        );
        assert_eq!(
            conn.query_row("SELECT count(*) FROM records", [], |r| r.get::<_, usize>(0))
                .unwrap(),
            7
        );

        conn.execute(
            "INSERT INTO records(id,digest,kind,used_at) VALUES('added','added','text',?1)",
            [timestamp],
        )
        .unwrap();
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 3).unwrap(),
            Some(4)
        );
        conn.execute("UPDATE records SET favorite=1 WHERE id='third'", [])
            .unwrap();
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 3).unwrap(),
            None
        );
    }
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn settings_changes_without_deletions_do_not_require_confirmation() {
    let store = test_store();
    let timestamp = now();
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
            "INSERT INTO records(id,digest,kind,used_at) VALUES('recent','recent','text',?1)",
            [timestamp],
        )
        .unwrap();
        let mut value = settings(&conn).unwrap();
        value.max_ordinary = 1;
        value.retention_days = 1;
        assert_eq!(
            settings_cleanup_confirmation(&conn, &value, timestamp, 0).unwrap(),
            None
        );
    }
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retention_uses_last_use_time_for_every_supported_duration() {
    let store = test_store();
    let timestamp = now();
    for days in [1, 3, 5, 7, 30] {
        let cutoff = timestamp - i64::from(days) * 86_400_000;
        {
            let conn = store.db.lock().unwrap();
            conn.execute("DELETE FROM records", []).unwrap();
            conn.execute("INSERT OR IGNORE INTO groups(id,name) VALUES(1,'项目')", [])
                .unwrap();
            let mut value = settings(&conn).unwrap();
            value.retention_days = days;
            conn.execute(
                "UPDATE settings SET value=?1 WHERE key='main'",
                [serde_json::to_string(&value).unwrap()],
            )
            .unwrap();
            for (id, used_at, favorite, group_id, pinned_at, expires_at) in [
                ("old", cutoff - 1, 0, None, None, None),
                ("boundary", cutoff, 0, None, None, None),
                ("recent", cutoff + 1, 0, None, None, None),
                ("reused", timestamp, 0, None, None, None),
                ("pinned", cutoff - 1, 0, None, Some(timestamp), None),
                ("favorite", cutoff - 1, 1, None, None, Some(timestamp)),
                ("grouped", cutoff - 1, 0, Some(1), None, Some(timestamp)),
            ] {
                conn.execute("INSERT INTO records(id,digest,kind,used_at,created_at,favorite,group_id,pinned_at,expires_at) VALUES(?1,?1,'text',?2,?3,?4,?5,?6,?7)",
                        params![id, used_at, cutoff - 86_400_000, favorite, group_id, pinned_at, expires_at]).unwrap();
            }
        }
        assert!(clean_at(&store, timestamp).unwrap());
        let mut remaining: Vec<String> = snapshot(&store)
            .unwrap()
            .records
            .into_iter()
            .map(|r| r.id)
            .collect();
        remaining.sort();
        assert_eq!(
            remaining,
            vec!["boundary", "favorite", "grouped", "recent", "reused"],
            "retention: {days} days"
        );
        assert!(!clean_at(&store, timestamp).unwrap());
    }
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn count_limit_keeps_latest_ordinary_records_at_supported_limits() {
    let store = test_store();
    let timestamp = now();
    for limit in [1, 300, 500] {
        {
            let conn = store.db.lock().unwrap();
            conn.execute("DELETE FROM records", []).unwrap();
            conn.execute("INSERT OR IGNORE INTO groups(id,name) VALUES(1,'项目')", [])
                .unwrap();
            let mut value = settings(&conn).unwrap();
            value.max_ordinary = limit;
            conn.execute(
                "UPDATE settings SET value=?1 WHERE key='main'",
                [serde_json::to_string(&value).unwrap()],
            )
            .unwrap();
            for age in 0..=limit {
                let id = format!("record-{age}");
                conn.execute("INSERT INTO records(id,digest,kind,used_at,pinned_at) VALUES(?1,?1,'text',?2,?3)",
                        params![id, timestamp - i64::from(age), (age == limit).then_some(timestamp)]).unwrap();
            }
            for (id, favorite, group_id) in [("favorite", 1, None), ("grouped", 0, Some(1))] {
                conn.execute("INSERT INTO records(id,digest,kind,used_at,favorite,group_id) VALUES(?1,?1,'text',?2,?3,?4)",
                        params![id, timestamp - 31 * 86_400_000, favorite, group_id]).unwrap();
            }
        }
        assert!(clean_at(&store, timestamp).unwrap());
        let records = snapshot(&store).unwrap().records;
        let ordinary: Vec<&Record> = records
            .iter()
            .filter(|r| !r.favorite && r.group_id.is_none())
            .collect();
        assert_eq!(ordinary.len(), usize::from(limit));
        assert!(ordinary
            .iter()
            .all(|r| r.used_at > timestamp - i64::from(limit)));
        assert_eq!(records.len(), usize::from(limit) + 2);
        assert!(!clean_at(&store, timestamp).unwrap());
    }
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn count_limit_does_not_count_expired_sensitive_records() {
    let store = test_store();
    {
        let conn = store.db.lock().unwrap();
        let mut value = settings(&conn).unwrap();
        value.max_ordinary = 2;
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&value).unwrap()],
        )
        .unwrap();
        let base = now();
        for (id, age, expires_at) in [
            ("expired", 1, Some(base - 1)),
            ("new", 2, None),
            ("older", 3, None),
            ("oldest", 4, None),
        ] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at,expires_at) VALUES(?1,?1,'text',?2,?3)",
                params![id, base - age * 1000, expires_at],
            )
            .unwrap();
        }
    }
    assert!(clean(&store).unwrap());
    let remaining: Vec<String> = snapshot(&store)
        .unwrap()
        .records
        .into_iter()
        .map(|r| r.id)
        .collect();
    assert_eq!(remaining, vec!["new", "older"]);
    assert!(!clean(&store).unwrap());
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn clear_cache_removes_ordinary_records_and_orphan_assets() {
    let store = test_store();
    let assets = store.dir.join("assets");
    let source = store.dir.join("original.txt");
    fs::write(&source, b"original").unwrap();
    for id in [
        "image", "video", "file", "favorite", "grouped", "both", "shared", "orphan",
    ] {
        fs::create_dir_all(assets.join(id)).unwrap();
        fs::write(assets.join(id).join("data.bin"), id).unwrap();
    }
    fs::write(assets.join("orphan.bin"), b"orphan").unwrap();
    {
        let conn = store.db.lock().unwrap();
        conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
            .unwrap();
        for (id, kind, favorite, group_id, stored_id) in [
            ("text", "text", 0, None, None),
            ("image", "image", 0, None, None),
            ("video", "video", 0, None, Some("video")),
            ("file", "file", 0, None, Some("file")),
            ("favorite", "image", 1, None, None),
            ("grouped", "file", 0, Some(1), Some("grouped")),
            ("both", "video", 1, Some(1), Some("both")),
            ("shared", "file", 0, None, Some("shared")),
            ("shared-owner", "file", 1, None, Some("shared")),
        ] {
            let files = stored_id.map_or_else(
                || "[]".to_string(),
                |owner| {
                    serde_json::to_string(&vec![FileItem {
                        original: source.to_string_lossy().into_owned(),
                        stored: Some(
                            assets
                                .join(owner)
                                .join("data.bin")
                                .to_string_lossy()
                                .into_owned(),
                        ),
                        is_directory: false,
                    }])
                    .unwrap()
                },
            );
            conn.execute("INSERT INTO records(id,digest,kind,files,used_at,favorite,group_id,storage_mode) VALUES(?1,?1,?2,?3,?4,?5,?6,'copy')",
                    params![id, kind, files, now(), favorite, group_id]).unwrap();
        }
    }
    assert_eq!(cache_size_data(&store).unwrap(), 26);
    assert_eq!(clear_cache_data(&store).unwrap(), 5);
    assert_eq!(cache_size_data(&store).unwrap(), 0);
    let mut remaining: Vec<String> = snapshot(&store)
        .unwrap()
        .records
        .into_iter()
        .map(|r| r.id)
        .collect();
    remaining.sort();
    assert_eq!(
        remaining,
        vec!["both", "favorite", "grouped", "shared-owner"]
    );
    for id in ["image", "video", "file", "orphan"] {
        assert!(!assets.join(id).exists());
    }
    assert!(!assets.join("orphan.bin").exists());
    for id in ["favorite", "grouped", "both", "shared"] {
        assert!(assets.join(id).join("data.bin").exists());
    }
    assert!(source.exists());
    assert!(store.dir.join("history.sqlite3").exists());
    assert_eq!(
        settings(&store.db.lock().unwrap()).unwrap().max_ordinary,
        300
    );
    fs::write(assets.join("late-orphan.bin"), b"orphan").unwrap();
    assert_eq!(cache_size_data(&store).unwrap(), 6);
    assert_eq!(clear_cache_data(&store).unwrap(), 0);
    assert_eq!(cache_size_data(&store).unwrap(), 0);
    assert!(!assets.join("late-orphan.bin").exists());
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
