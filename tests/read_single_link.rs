use std::{ffi::OsStr, path::Path};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct MyStruct {
    file_name: String,
    #[serde(deserialize_with = "deserialize_dbm")]
    my_number: NumberWrapper,
}

impl DatabaseEntry for MyStruct {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("my_struct")
    }
}

#[derive(Clone, Serialize, Deserialize, Debug)]
#[serde(deny_unknown_fields)]
struct NumberWrapper {
    val: f64,
    file_name: String,
}

impl DatabaseEntry for NumberWrapper {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("number_wrapper")
    }
}

#[test]
fn test_read_from_database_link() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let resolved_struct: MyStruct = dbm.read("my_struct_single_db_link_read").unwrap();
    assert_eq!(resolved_struct.my_number.val, 42.0);
}

#[test]
fn test_read_from_database_link_check_err_msg() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let err = dbm
        .read::<MyStruct, _>("my_struct_single_db_link_malformed_read")
        .unwrap_err();
    let err_msg = err.to_string();
    assert!(err_msg.contains("my_struct_single_db_link_malformed_read.yaml"));
    assert!(err_msg.contains("number_wrapper_malformed.yaml"));
    println!("{err_msg}");
}

#[test]
fn test_read_from_database_flat() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();
    let resolved_struct: MyStruct = dbm.read("my_struct_flat_read").unwrap();
    assert_eq!(resolved_struct.my_number.val, 42.0);
}
