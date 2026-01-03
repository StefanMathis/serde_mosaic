use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    path::Path,
};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug, Clone)]
#[serde(deny_unknown_fields)]
struct MyStruct {
    file_name: String,
    #[serde(serialize_with = "serialize_dbm")]
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
fn test_write_to_database_flat() {
    let my_struct = MyStruct {
        file_name: "my_struct_flat_overwrite".to_string(),
        my_number: NumberWrapper {
            val: 12.0,
            file_name: "number_wrapper_overwrite".to_string(),
        },
    };

    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let _ = dbm.remove_by_name(MyStruct::folder_name(), my_struct.file_name());

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Flat;
    write_options.overwrite = true;

    let (_, write_info) = dbm.write_verbose(&my_struct, &write_options).unwrap();
    assert_eq!(write_info.overwritten_files.len(), 0);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new("my_struct_flat_overwrite")
    );

    let _ = dbm.remove_by_name(MyStruct::folder_name(), my_struct.file_name());
}

#[test]
fn test_write_to_database_linked() {
    let my_struct = MyStruct {
        file_name: "my_struct_link_overwrite".to_string(),
        my_number: NumberWrapper {
            val: 12.0,
            file_name: "number_wrapper_overwrite".to_string(),
        },
    };

    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let _ = dbm.remove_by_name(MyStruct::folder_name(), my_struct.file_name());

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;

    let (_, write_info) = dbm.write_verbose(&my_struct, &write_options).unwrap();
    assert_eq!(write_info.overwritten_files.len(), 1);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.overwritten_files[0].file_stem().unwrap(),
        OsStr::new("number_wrapper_overwrite")
    );
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new("my_struct_link_overwrite")
    );
}

#[test]
fn test_write_to_database_with_alias() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    // Cleanup before test
    let _ = dbm.remove_by_name(MyStruct::folder_name(), "new_struct_name");
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "new_number_name");

    let my_struct = MyStruct {
        file_name: "replace_me".to_string(),
        my_number: NumberWrapper {
            val: 12.0,
            file_name: "replace_me_as_well".to_string(),
        },
    };

    let mut alias: HashMap<OsString, OsString> = HashMap::new();
    alias.insert(
        OsStr::new("replace_me").to_os_string(),
        OsStr::new("new_struct_name").to_os_string(),
    );
    alias.insert(
        OsStr::new("replace_me_as_well").to_os_string(),
        OsStr::new("new_number_name").to_os_string(),
    );

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;
    write_options.alias = alias;

    let (_, write_info) = dbm.write_verbose(&my_struct, &write_options).unwrap();

    assert_eq!(write_info.created_files.len(), 2);
    assert_eq!(
        write_info.created_files[0].file_stem().unwrap(),
        OsStr::new("new_number_name")
    );
    assert_eq!(
        write_info.created_files[1].file_stem().unwrap(),
        OsStr::new("new_struct_name")
    );

    // The original file names are not used in the database, but the aliases are
    assert!(!dbm.exists(MyStruct::folder_name(), &my_struct.file_name));
    assert!(!dbm.exists(NumberWrapper::folder_name(), &my_struct.my_number.file_name));
    assert!(dbm.exists(MyStruct::folder_name(), "new_struct_name"));
    assert!(dbm.exists(NumberWrapper::folder_name(), "new_number_name"));

    // Cleanup
    let _ = dbm.remove_by_name(MyStruct::folder_name(), "new_struct_name");
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "new_number_name");
}

#[test]
fn test_write_to_database_wo_overwrite() {
    let my_number = NumberWrapper {
        val: 12.0,
        file_name: "number_wrapper".to_string(),
    };

    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    // Remove any leftover files from the last test
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_0");
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_1");
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_2");

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = false;

    // Stringify the written file and check the input
    let (file_path_0, write_info) = dbm.write_verbose(&my_number, &write_options).unwrap();

    // File is newly created
    assert_eq!(write_info.overwritten_files.len(), 0);
    assert_eq!(write_info.created_files.len(), 1);
    assert_eq!(
        write_info.created_files[0].file_name().unwrap(),
        OsStr::new("number_wrapper_0.yaml")
    );
    assert!(file_path_0.to_string_lossy().contains("number_wrapper_0"));

    let file_path_1 = dbm.write(&my_number, &write_options).unwrap();
    assert!(file_path_1.to_string_lossy().contains("number_wrapper_1"));

    let file_path_2 = dbm.write(&my_number, &write_options).unwrap();
    assert!(file_path_2.to_string_lossy().contains("number_wrapper_2"));

    assert!(file_path_0.exists());
    assert!(file_path_1.exists());
    assert!(file_path_2.exists());
    dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_0")
        .unwrap();
    dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_1")
        .unwrap();
    dbm.remove_by_name(NumberWrapper::folder_name(), "number_wrapper_2")
        .unwrap();

    // Check that the file does not exist
    assert!(!file_path_0.exists());
    assert!(!file_path_1.exists());
    assert!(!file_path_2.exists());
}

#[test]
fn test_remove_all() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    // Cleanup before test
    let _ = dbm.remove_by_name(MyStruct::folder_name(), "remove_all");
    let _ = dbm.remove_by_name(NumberWrapper::folder_name(), "remove_all");

    assert!(!dbm.exists(MyStruct::folder_name(), "remove_all"));
    assert!(!dbm.exists(NumberWrapper::folder_name(), "remove_all"));

    let my_struct = MyStruct {
        file_name: "remove_all".to_string(),
        my_number: NumberWrapper {
            val: 12.0,
            file_name: "remove_all".to_string(),
        },
    };

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;

    // Stringify the written file and check the input
    let _ = dbm.write(&my_struct, &write_options).unwrap();

    assert!(dbm.exists(MyStruct::folder_name(), "remove_all"));
    assert!(dbm.exists(NumberWrapper::folder_name(), "remove_all"));

    dbm.remove_all("remove_all").unwrap();

    assert!(!dbm.exists(MyStruct::folder_name(), "remove_all"));
    assert!(!dbm.exists(NumberWrapper::folder_name(), "remove_all"));
}
