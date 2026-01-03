use std::{ffi::OsStr, path::Path, ptr, sync::Arc};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug)]
struct MyStruct {
    file_name: String,
    #[serde(
        serialize_with = "serialize_opt_arc_dbm",
        deserialize_with = "deserialize_opt_arc_dbm"
    )]
    my_arc_number: Option<Arc<NumberWrapper>>,
}

impl DatabaseEntry for MyStruct {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("my_struct")
    }
}

#[derive(Serialize, Deserialize, Debug)]
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
fn test_write_to_database() {
    let my_struct = MyStruct {
        file_name: "my_struct_opt_arc_write".to_string(),
        my_arc_number: Some(Arc::new(NumberWrapper {
            val: 12.0,
            file_name: "arc_number_opt".to_string(),
        })),
    };

    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let path_string = format!(
        "{}/{}/{}.yaml",
        path_db,
        MyStruct::folder_name().to_str().unwrap(),
        my_struct.file_name().to_str().unwrap()
    );

    let file_path = Path::new(&path_string);

    let path_string_arc_number_wrapper = format!(
        "{}/{}/{}.yaml",
        path_db,
        NumberWrapper::folder_name().to_str().unwrap(),
        my_struct
            .my_arc_number
            .as_ref()
            .unwrap()
            .file_name()
            .to_str()
            .unwrap()
    );

    let file_path_arc_number_wrapper = Path::new(&path_string_arc_number_wrapper);

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;

    // Stringify the written file and check the input
    dbm.write(&my_struct, &write_options).unwrap();

    assert!(file_path.exists());
    assert!(file_path_arc_number_wrapper.exists());

    dbm.remove_by_name(MyStruct::folder_name(), my_struct.file_name())
        .unwrap();
    dbm.remove_by_name(
        NumberWrapper::folder_name(),
        my_struct.my_arc_number.as_ref().unwrap().file_name(),
    )
    .unwrap();

    // Check that the file does not exist
    assert!(!file_path.exists());
    assert!(!file_path_arc_number_wrapper.exists());
}

#[test]
fn test_read_from_database() {
    {
        let path_db = "tests/test_database";
        let mut dbm =
            DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml)
                .unwrap();
        let resolved_struct: MyStruct = dbm.read("my_struct_arc_none").unwrap();
        assert!(resolved_struct.my_arc_number.is_none());
    }

    {
        let path_db = "tests/test_database";
        let mut dbm =
            DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml)
                .unwrap();
        let resolved_struct: MyStruct = dbm.read("my_struct_arc_some").unwrap();
        assert_eq!(resolved_struct.my_arc_number.unwrap().val, 21.0);
    }
}

/**
Create multiple instanced of MyStruct, which share the same my_arc_number
 */
#[test]
fn test_read_and_reuse() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    // Write a test file to make sure that the checksum is up-to-date
    let my_struct = MyStruct {
        file_name: "my_struct_reuse_opt".to_string(),
        my_arc_number: Some(Arc::new(NumberWrapper {
            val: 12.0,
            file_name: "number_wrapper_reuse_arc_opt".to_string(),
        })),
    };

    // Cleanup
    dbm.remove_by_instance(&my_struct).unwrap();
    dbm.remove_by_instance(&**my_struct.my_arc_number.as_ref().unwrap())
        .unwrap();

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;

    dbm.write(&my_struct, &write_options).unwrap();

    assert_eq!(dbm.arc_map().len(), 0);
    let first_struct: MyStruct = dbm.read(my_struct.file_name()).unwrap();
    assert_eq!(dbm.arc_map().len(), 1);
    let second_struct: MyStruct = dbm.read(my_struct.file_name()).unwrap();

    // These two instances share the same pointer
    assert!(ptr::eq(
        &**first_struct.my_arc_number.as_ref().unwrap(),
        &**second_struct.my_arc_number.as_ref().unwrap()
    ));

    // Manipulate the checksum so the arc does not get reused anymore.
    // This is the inverted case of the usual problem that the file has been manipulated during the lifetime of the database manager.
    {
        let inner_map = dbm
            .arc_map_mut()
            .get_mut(NumberWrapper::folder_name())
            .unwrap();
        let reference = inner_map
            .get_mut(my_struct.my_arc_number.as_ref().unwrap().file_name())
            .unwrap();
        reference.checksum = Some(reference.checksum.unwrap() + 1);
    }

    let third_struct: MyStruct = dbm.read(my_struct.file_name()).unwrap();

    // No pointer sharing, since the checksum was not identical
    assert!(!ptr::eq(
        &**first_struct.my_arc_number.as_ref().unwrap(),
        &**third_struct.my_arc_number.as_ref().unwrap()
    ));

    // Cleanup
    dbm.remove_by_instance(&my_struct).unwrap();
    dbm.remove_by_instance(&**my_struct.my_arc_number.as_ref().unwrap())
        .unwrap();
}
