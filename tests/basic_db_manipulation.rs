use std::{any::Any, ffi::OsStr, path::Path};

use serde::{Deserialize, Serialize};
use serde_mosaic::*;

#[derive(Serialize, Deserialize, PartialEq, Debug)]
struct Bar(String);

#[typetag::serde]
impl DatabaseEntry for Bar {
    fn name(&self) -> &OsStr {
        OsStr::new(&self.0)
    }
}

// ========================================================

#[test]
fn test_serialize_and_deserialize() {
    let relative_path = Path::new("tests/test_database");

    let mut path = std::env::current_dir().unwrap();
    path.push(relative_path);
    let mut dbm = DatabaseManager::open(path.to_path_buf(), SerdeYaml).unwrap();

    let name = "this is a bar object";
    let bar = Bar(name.into());

    // Check that no subfolder currently exists
    let subfolder = path.clone().join(type_name::<Bar>());
    assert!(!subfolder.exists());

    // Serialize bar, creating the corresponding subfolder in the process
    dbm.write(&bar, &WriteOptions::default()).unwrap();
    assert!(subfolder.exists());

    // Deserialize bar again
    let bar_de: Bar = dbm.read(name).unwrap();

    // Remove the file manually for the next test
    dbm.remove((type_name::<Bar>(), name)).unwrap();

    // The subfolder is now empty => it will be deleted
    dbm.remove_empty_subfolders().unwrap();
    assert!(!subfolder.exists());

    assert_eq!(bar, bar_de);
}

#[test]
fn test_format_readout() {
    let dbm = DatabaseManager::new("tests/test_database", SerdeYaml)
        .expect("directory exists or can be created");
    let format_ref = dbm.data_format() as &dyn Any; // Possible since Rust 1.86
    assert!(format_ref.downcast_ref::<SerdeYaml>().is_some());
}
