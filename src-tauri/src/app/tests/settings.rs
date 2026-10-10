use super::*;
use crate::app::{storage::initialize, test_support::test_store};
use std::fs;

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
