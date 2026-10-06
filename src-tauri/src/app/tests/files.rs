use super::super::storage::snapshot;
use super::super::test_support::test_store;
use super::*;
use uuid::Uuid;

#[test]
fn record_directory_selects_all_paths_by_folder() {
    let root = std::env::temp_dir().join(format!("tp-test-{}", Uuid::new_v4()));
    let first_dir = root.join("first");
    let second_dir = root.join("second");
    fs::create_dir_all(&first_dir).unwrap();
    fs::create_dir_all(&second_dir).unwrap();
    let first = first_dir.join("first.txt");
    let another = first_dir.join("another.txt");
    let second = second_dir.join("second.txt");
    for path in [&first, &another, &second] {
        fs::write(path, b"original").unwrap();
    }
    let files = [first.clone(), another.clone(), second.clone()];
    let grouped = paths_by_directory(&files).unwrap();
    assert_eq!(grouped.get(&first_dir), Some(&vec![first, another]));
    assert_eq!(grouped.get(&second_dir), Some(&vec![second.clone()]));
    assert!(paths_by_directory(&[]).is_err());
    fs::remove_file(second).unwrap();
    assert!(paths_by_directory(&files).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn record_action_paths_prefer_original_and_fall_back_to_file_copy() {
    let store = test_store();
    let original = store.dir.join("original.png");
    let stored = store
        .dir
        .join("assets")
        .join("image")
        .join("files")
        .join("original.png");
    fs::write(&original, b"original").unwrap();
    fs::create_dir_all(stored.parent().unwrap()).unwrap();
    fs::write(&stored, b"copy").unwrap();
    let files = serde_json::to_string(&[FileItem {
        original: original.to_string_lossy().into_owned(),
        stored: Some(stored.to_string_lossy().into_owned()),
        is_directory: false,
    }])
    .unwrap();
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,files,used_at) VALUES('image','image','image',?1,1)",
                [files],
            )
            .unwrap();
    }
    assert_eq!(
        record_action_paths(&store, "image", false).unwrap(),
        vec![original.clone()]
    );
    fs::remove_file(&original).unwrap();
    assert_eq!(
        record_action_paths(&store, "image", false).unwrap(),
        vec![stored.clone()]
    );
    assert_eq!(
        record_action_paths(&store, "image", true).unwrap(),
        vec![stored]
    );

    let screenshot = store
        .dir
        .join("assets")
        .join("screenshot")
        .join("image.png");
    fs::create_dir_all(screenshot.parent().unwrap()).unwrap();
    fs::write(&screenshot, b"screenshot").unwrap();
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,used_at) VALUES('screenshot','screenshot','image',1)",
                [],
            )
            .unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,files,used_at) VALUES('directory','directory','file',?1,1)",
                [serde_json::to_string(&[FileItem {
                    original: store.dir.join("folder").to_string_lossy().into_owned(),
                    stored: None,
                    is_directory: true,
                }])
                .unwrap()],
            )
            .unwrap();
    }
    assert_eq!(
        record_action_paths(&store, "screenshot", false).unwrap(),
        vec![screenshot]
    );
    assert!(record_action_paths(&store, "directory", false).is_err());

    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn direct_image_records_expose_their_image_copy_path() {
    let store = test_store();
    let screenshot = store
        .dir
        .join("assets")
        .join("screenshot")
        .join("image.png");
    fs::create_dir_all(screenshot.parent().unwrap()).unwrap();
    fs::write(&screenshot, b"screenshot").unwrap();
    let file_items = serde_json::to_string(&[FileItem {
        original: store
            .dir
            .join("original.png")
            .to_string_lossy()
            .into_owned(),
        stored: None,
        is_directory: false,
    }])
    .unwrap();
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,used_at) VALUES('screenshot','screenshot','image',2)",
                [],
            )
            .unwrap();
        conn.execute(
                "INSERT INTO records(id,digest,kind,files,used_at) VALUES('image','image','image',?1,1)",
                [file_items],
            )
            .unwrap();
    }
    let records = snapshot(&store).unwrap().records;
    let find = |id: &str| records.iter().find(|record| record.id == id).unwrap();
    // 直接复制的图片没有原文件，悬停预览使用本工具保存的图片副本路径。
    assert_eq!(
        find("screenshot").asset_path.as_deref(),
        Some(screenshot.to_string_lossy().as_ref())
    );
    // 有文件项的图片记录仍使用原路径，不额外返回副本路径。
    assert_eq!(find("image").asset_path, None);

    let dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(dir).unwrap();
}

#[test]
fn save_files_places_the_whole_group_in_one_directory_without_overwriting() {
    let store = test_store();
    let source = store.dir.join("source");
    let first = source.join("first.txt");
    let second = source.join("nested").join("first.txt");
    fs::create_dir_all(second.parent().unwrap()).unwrap();
    fs::write(&first, b"first").unwrap();
    fs::write(&second, b"second").unwrap();
    let files = vec![
        FileItem {
            original: first.to_string_lossy().into_owned(),
            stored: None,
            is_directory: false,
        },
        FileItem {
            original: second.to_string_lossy().into_owned(),
            stored: None,
            is_directory: false,
        },
    ];
    let id = "group";
    {
        let conn = store.db.lock().unwrap();
        conn.execute(
            "INSERT INTO records(id,digest,kind,status,used_at) VALUES(?1,?1,'file','pending',1)",
            [id],
        )
        .unwrap();
    }
    let dir = store.dir.join("assets").join(id).join("files");
    // 直接复用 save_files 的落盘逻辑，不启动后台线程。
    let saved = {
        let mut files = files.clone();
        fs::create_dir_all(&dir).unwrap();
        let mut taken = HashSet::<OsString>::new();
        for file in files.iter_mut() {
            let name = Path::new(&file.original).file_name().unwrap();
            let target = dir.join(unique_file_name(name, &mut taken));
            copy_path(Path::new(&file.original), &target).unwrap();
            file.stored = Some(target.to_string_lossy().into_owned());
        }
        files
    };
    let stored: Vec<PathBuf> = saved
        .iter()
        .map(|file| PathBuf::from(file.stored.as_ref().unwrap()))
        .collect();
    assert_eq!(
        stored[0].parent(),
        stored[1].parent(),
        "整组副本必须落在同一个目录下"
    );
    assert_eq!(stored[0].file_name().unwrap(), "first.txt");
    assert_eq!(stored[1].file_name().unwrap(), "first (2).txt");

    let store_dir = store.dir.clone();
    drop(store);
    fs::remove_dir_all(store_dir).unwrap();
}
