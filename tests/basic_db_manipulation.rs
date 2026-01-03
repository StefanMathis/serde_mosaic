use std::{ffi::OsStr, path::Path};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Bar(String);

impl DatabaseEntry for Bar {
    fn folder_name() -> &'static OsStr {
        OsStr::new("bar")
    }

    fn file_name(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

// ========================================================

#[test]
fn test_serialize_and_deserialize() {
    let relative_path = Path::new("tests/test_database");

    let mut path = std::env::current_dir().unwrap();
    path.push(relative_path);
    let mut dbm = DatabaseManager::from_path(path.to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let file_name = "this is a bar object";
    let bar = Bar(file_name.into());

    // Check that no subfolder currently exists
    let subfolder = path.clone().join(Bar::folder_name());
    assert!(!subfolder.exists());

    // Serialize bar, creating the corresponding subfolder in the process
    dbm.write(&bar, &WriteOptions::default()).unwrap();
    assert!(subfolder.exists());

    // Deserialize bar again
    let bar_de: Bar = dbm.read(file_name).unwrap();

    // Remove the file manually for the next test
    dbm.remove_by_name(Bar::folder_name(), file_name).unwrap();

    // The subfolder is now empty => it will be deleted
    dbm.remove_empty_subfolders().unwrap();
    assert!(!subfolder.exists());

    assert_eq!(bar, bar_de);
}
