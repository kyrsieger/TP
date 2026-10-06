use super::super::storage::snapshot;
use super::super::test_support::test_store;
use super::*;
use std::fs;

#[test]
fn batch_actions_change_only_selected_records_and_protect_them_before_cleanup() {
    let store = test_store();
    let ids = vec!["a".to_string(), "b".to_string()];
    {
        let conn = store.db.lock().unwrap();
        conn.execute("INSERT INTO groups(id,name) VALUES(1,'项目')", [])
            .unwrap();
        for id in ["a", "b", "c"] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at) VALUES(?1,?1,'text',?2)",
                params![
                    id,
                    if id == "c" {
                        now()
                    } else {
                        now() - 8 * 86_400_000
                    }
                ],
            )
            .unwrap();
        }
        apply_batch(
            &conn,
            &store.dir,
            &ids,
            &BatchOperation::SetFavorite { favorite: true },
        )
        .unwrap();
    }
    clean(&store).unwrap();
    {
        let conn = store.db.lock().unwrap();
        apply_batch(
            &conn,
            &store.dir,
            &ids,
            &BatchOperation::SetPinned { pinned: true },
        )
        .unwrap();
        let move_group = serde_json::from_value::<BatchOperation>(
            serde_json::json!({ "kind": "move_group", "groupId": 1 }),
        )
        .unwrap();
        apply_batch(&conn, &store.dir, &ids, &move_group).unwrap();
        let rows = conn
            .prepare("SELECT id,favorite,pinned_at IS NOT NULL,group_id FROM records ORDER BY id")
            .unwrap()
            .query_map([], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, bool>(1)?,
                    row.get::<_, bool>(2)?,
                    row.get::<_, Option<i64>>(3)?,
                ))
            })
            .unwrap()
            .collect::<std::result::Result<Vec<_>, _>>()
            .unwrap();
        assert_eq!(
            rows,
            vec![
                ("a".into(), true, true, Some(1)),
                ("b".into(), true, true, Some(1)),
                ("c".into(), false, false, None)
            ]
        );
        apply_batch(&conn, &store.dir, &ids, &BatchOperation::Delete).unwrap();
    }
    assert_eq!(
        snapshot(&store)
            .unwrap()
            .records
            .iter()
            .map(|record| record.id.as_str())
            .collect::<Vec<_>>(),
        vec!["c"]
    );
    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn pinned_records_sort_by_pin_time_and_unpin_restores_use_order() {
    let store = test_store();
    {
        let conn = store.db.lock().unwrap();
        for (id, used_at) in [("new", 3000), ("middle", 2000), ("old", 1000)] {
            conn.execute(
                "INSERT INTO records(id,digest,kind,used_at) VALUES(?1,?1,'text',?2)",
                params![id, used_at],
            )
            .unwrap();
        }
        set_pinned(&conn, "old", true).unwrap();
        set_pinned(&conn, "middle", true).unwrap();
    }
    let records = snapshot(&store).unwrap().records;
    assert_eq!(
        records.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(),
        vec!["middle", "old", "new"]
    );
    assert!(records[0].pinned_at > records[1].pinned_at);

    set_pinned(&store.db.lock().unwrap(), "middle", false).unwrap();
    assert_eq!(
        snapshot(&store)
            .unwrap()
            .records
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["old", "new", "middle"]
    );
    set_pinned(&store.db.lock().unwrap(), "old", false).unwrap();
    assert_eq!(
        snapshot(&store)
            .unwrap()
            .records
            .iter()
            .map(|r| r.id.as_str())
            .collect::<Vec<_>>(),
        vec!["new", "middle", "old"]
    );

    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}
