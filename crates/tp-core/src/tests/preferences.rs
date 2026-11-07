use super::*;
use crate::test_support::{initialize, test_store};
use std::fs;

#[test]
fn independent_field_edits_merge_without_reverting_geometry_or_prior_choices() {
    let store = test_store();
    save_patch(
        &store,
        None,
        |value| value.retain_search_input = false,
        |_, _| Ok(()),
    )
    .unwrap();
    {
        let conn = store.db.lock().unwrap();
        persist_panel_geometry(
            &conn,
            PanelGeometry {
                panel_x: Some(50.),
                panel_y: Some(70.),
                panel_width: 480.,
                panel_height: 640.,
            },
        )
        .unwrap();
    }
    save_patch(
        &store,
        None,
        |value| value.retain_type_filter = false,
        |_, next| {
            assert!(!next.retain_search_input);
            assert_eq!(next.panel_x, Some(50.));
            assert_eq!(next.panel_width, 480.);
            Ok(())
        },
    )
    .unwrap();
    let value = store.snapshot().unwrap().settings;
    assert!(!value.retain_search_input && !value.retain_type_filter);
    assert_eq!(value.panel_height, 640.);
}

#[test]
fn saving_requires_current_cleanup_count_and_preserves_protected_records() {
    let store = test_store();
    let timestamp = now();
    {
        let conn = store.db.lock().unwrap();
        for (id, favorite) in [("old", false), ("new", false), ("protected", true)] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at,favorite) VALUES(?1,?1,'text',?2,?3)",
                rusqlite::params![id, timestamp, favorite],
            )
            .unwrap();
        }
    }
    let mut value = store.snapshot().unwrap().settings;
    value.max_ordinary = 1;
    assert!(matches!(
        save(&store, &value, None, |_, _| panic!(
            "confirmation must precede host changes"
        ))
        .unwrap(),
        SaveOutcome::ConfirmationRequired { count: 1 }
    ));
    store
        .db
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO records(id,digest,kind,used_at) VALUES('arrived','arrived','text',?1)",
            [timestamp],
        )
        .unwrap();
    assert!(matches!(
        save(&store, &value, Some(1), |_, _| panic!(
            "new arrivals need renewed confirmation"
        ))
        .unwrap(),
        SaveOutcome::ConfirmationRequired { count: 2 }
    ));
    assert!(matches!(
        save(&store, &value, Some(2), |_, _| Ok(())).unwrap(),
        SaveOutcome::Saved { removed: 2, .. }
    ));
    let snapshot = store.snapshot().unwrap();
    assert_eq!(snapshot.records.len(), 2);
    assert!(snapshot.records.iter().any(|r| r.id == "protected"));
    assert_eq!(snapshot.settings.max_ordinary, 1);
}

#[test]
fn failed_host_changes_restore_previous_preferences_without_deleting_records() {
    let store = test_store();
    let original = store.snapshot().unwrap().settings;
    let mut next = original.clone();
    next.always_on_top = !original.always_on_top;
    next.theme = "forest-mist".into();
    next.max_ordinary = 1;
    {
        let conn = store.db.lock().unwrap();
        for id in ["first", "second"] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at) VALUES(?1,?1,'text',?2)",
                rusqlite::params![id, now()],
            )
            .unwrap();
        }
    }
    let mut applied = Vec::new();
    assert!(save(&store, &next, Some(1), |_, value| {
        applied.push(value.always_on_top);
        if value.always_on_top == next.always_on_top {
            Err("host failed".into())
        } else {
            Ok(())
        }
    })
    .is_err());
    assert_eq!(applied, [next.always_on_top, original.always_on_top]);
    let saved = store.snapshot().unwrap().settings;
    assert_eq!(saved.always_on_top, original.always_on_top);
    assert_eq!(saved.theme, original.theme);
    assert_eq!(saved.max_ordinary, original.max_ordinary);
    assert_eq!(store.snapshot().unwrap().records.len(), 2);
    store
        .db
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only=ON")
        .unwrap();
    assert!(save(&store, &next, Some(1), |_, _| panic!(
        "failed database writes cannot alter host"
    ))
    .is_err());
}

#[test]
fn reinstall_restores_enabled_preference_and_refreshes_executable_path() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        save_autostart(&conn, true, false, |_| Ok(())).unwrap();
        // 普通设置的保存不能覆盖开机启动选择。
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&Settings::default()).unwrap()],
        )
        .unwrap();
    }
    drop(store);
    // 重装保留数据库，但卸载已删除系统启动项。
    let store = initialize(dir.clone()).unwrap();
    {
        let conn = store.db.lock().unwrap();
        let mut applied = Vec::new();
        restore_autostart(&conn, false, |value| {
            applied.push(value);
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, vec![true]);
        // 已存在的启动项也需要刷新路径。
        restore_autostart(&conn, true, |value| {
            applied.push(value);
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, vec![true, true]);
        assert_eq!(saved_autostart(&conn).unwrap(), Some(true));
    }
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn disabled_preference_survives_reinstall_without_deleting_missing_entry() {
    let store = test_store();
    let dir = store.dir.clone();
    save_autostart(&store.db.lock().unwrap(), false, true, |_| Ok(())).unwrap();
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    {
        let conn = store.db.lock().unwrap();
        restore_autostart(&conn, false, |_| panic!("不应删除不存在的系统启动项")).unwrap();
        let mut applied = Vec::new();
        restore_autostart(&conn, true, |value| {
            applied.push(value);
            Ok(())
        })
        .unwrap();
        assert_eq!(applied, vec![false]);
        assert_eq!(saved_autostart(&conn).unwrap(), Some(false));
    }
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn legacy_install_adopts_current_state_without_changing_system() {
    for current in [false, true] {
        let store = test_store();
        let dir = store.dir.clone();
        {
            let conn = store.db.lock().unwrap();
            assert_eq!(saved_autostart(&conn).unwrap(), None);
            restore_autostart(&conn, current, |_| panic!("迁移不应改变系统状态")).unwrap();
            assert_eq!(saved_autostart(&conn).unwrap(), Some(current));
        }
        drop(store);
        fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn system_failure_rolls_back_preference_and_restoration_can_retry() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        write_autostart(&conn, true).unwrap();
        let mut applied = Vec::new();
        let result = save_autostart(&conn, false, true, |value| {
            applied.push(value);
            if value {
                Ok(())
            } else {
                Err("系统写入失败".into())
            }
        });
        assert!(result.is_err());
        assert_eq!(applied, vec![false, true]);
        assert_eq!(saved_autostart(&conn).unwrap(), Some(true));
        assert!(restore_autostart(&conn, false, |_| Err("恢复失败".into())).is_err());
        assert_eq!(saved_autostart(&conn).unwrap(), Some(true));
        restore_autostart(&conn, false, |value| {
            assert!(value);
            Ok(())
        })
        .unwrap();
    }
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn database_failure_does_not_apply_system_change() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        conn.execute_batch("PRAGMA query_only=ON").unwrap();
        assert!(save_autostart(&conn, true, false, |_| panic!(
            "数据库写入失败时不应更改系统状态"
        ))
        .is_err());
        assert_eq!(saved_autostart(&conn).unwrap(), None);
    }
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn geometry_save_merges_into_latest_preferences_without_reverting_the_clipboard_switch() {
    let store = test_store();
    let dir = store.dir.clone();
    {
        let conn = store.db.lock().unwrap();
        let mut config = settings(&conn).unwrap();
        config.remove_trailing_newlines = true;
        config.theme = "forest-mist".into();
        conn.execute(
            "UPDATE settings SET value=?1 WHERE key='main'",
            [serde_json::to_string(&config).unwrap()],
        )
        .unwrap();
        persist_panel_geometry(
            &conn,
            PanelGeometry {
                panel_x: Some(100.0),
                panel_y: Some(200.0),
                panel_width: 480.0,
                panel_height: 650.0,
            },
        )
        .unwrap();
        let saved = settings(&conn).unwrap();
        assert!(saved.remove_trailing_newlines);
        assert_eq!(saved.theme, "forest-mist");
        assert_eq!(saved.panel_width, 480.0);
        assert!(persist_panel_geometry(
            &conn,
            PanelGeometry {
                panel_x: None,
                panel_y: None,
                panel_width: 20.0,
                panel_height: 650.0
            }
        )
        .is_err());
        assert_eq!(settings(&conn).unwrap().panel_width, 480.0);
    }
    drop(store);
    let store = initialize(dir.clone()).unwrap();
    assert!(
        settings(&store.db.lock().unwrap())
            .unwrap()
            .remove_trailing_newlines
    );
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
