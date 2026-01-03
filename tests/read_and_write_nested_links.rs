use std::{ffi::OsStr, path::Path};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
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

#[derive(Serialize, Deserialize, Debug, Clone)]
struct MyStruct {
    #[serde(serialize_with = "serialize_dbm", deserialize_with = "deserialize_dbm")]
    my_number: NumberWrapper,
}

impl DatabaseEntry for MyStruct {
    fn file_name(&self) -> &OsStr {
        OsStr::new("placeholder")
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("my_struct")
    }
}

// Wrapped struct
#[derive(Serialize, Deserialize, Debug)]
struct MyStructWrapper {
    file_name: String,
    my_struct: MyStruct,
}

// Wrapped struct
#[derive(Serialize, Deserialize, Debug)]
struct MyStructDoubleWrapper {
    file_name: String,
    my_struct_wrapper: MyStructWrapper,
    #[serde(serialize_with = "serialize_dbm", deserialize_with = "deserialize_dbm")]
    my_second_number: NumberWrapper,
}

impl DatabaseEntry for MyStructDoubleWrapper {
    fn file_name(&self) -> &OsStr {
        OsStr::new(&self.file_name)
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("my_struct")
    }
}

fn create_wrapper() -> MyStructDoubleWrapper {
    let my_struct = MyStruct {
        my_number: NumberWrapper {
            val: 12.0,
            file_name: "number_wrapper_nested_first".to_string(),
        },
    };

    let wrapper = MyStructWrapper {
        file_name: "test_wrapper_nested".to_string(),
        my_struct,
    };

    let wrapper = MyStructDoubleWrapper {
        file_name: "test_wrapper_doubly_nested".to_string(),
        my_struct_wrapper: wrapper,
        my_second_number: NumberWrapper {
            val: 21.0,
            file_name: "number_wrapper_nested_second".to_string(),
        },
    };
    return wrapper;
}

#[test]
fn test_write_doubly_wrapped_to_database() {
    let wrapper = create_wrapper();

    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let path_string = format!(
        "{}/{}/{}.yaml",
        path_db,
        MyStruct::folder_name().to_str().unwrap(),
        wrapper.file_name().to_str().unwrap()
    );
    let file_path = Path::new(&path_string);

    let path_string = format!(
        "{}/{}/{}.yaml",
        path_db,
        NumberWrapper::folder_name().to_str().unwrap(),
        wrapper
            .my_struct_wrapper
            .my_struct
            .my_number
            .file_name()
            .to_str()
            .unwrap()
    );
    let file_path_number_wrapper_nested_first = Path::new(&path_string);

    let path_string = format!(
        "{}/{}/{}.yaml",
        path_db,
        NumberWrapper::folder_name().to_str().unwrap(),
        wrapper.my_second_number.file_name().to_str().unwrap()
    );
    let file_path_number_wrapper_nested_second = Path::new(&path_string);

    let mut write_options = WriteOptions::default();
    write_options.write_mode = WriteMode::Link;
    write_options.overwrite = true;

    // Stringify the written file and check the input
    dbm.write(&wrapper, &write_options).unwrap();

    assert!(file_path.exists());
    assert!(file_path_number_wrapper_nested_first.exists());
    assert!(file_path_number_wrapper_nested_second.exists());
    dbm.remove_by_instance(&wrapper).unwrap();
    dbm.remove_by_instance(&wrapper.my_struct_wrapper.my_struct.my_number)
        .unwrap();
    dbm.remove_by_instance(&wrapper.my_second_number).unwrap();

    // Check that the file does not exist
    assert!(!file_path.exists());
    assert!(!file_path_number_wrapper_nested_first.exists());
    assert!(!file_path_number_wrapper_nested_second.exists());
}

#[test]
fn test_read_from_database_doubly_wrapped() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let resolved_struct: MyStructDoubleWrapper = dbm
        .read("my_struct_with_linked_fields_doubly_wrapped")
        .unwrap();
    assert_eq!(
        resolved_struct.my_struct_wrapper.my_struct.my_number.val,
        42.0
    );
    assert_eq!(resolved_struct.my_second_number.val, 42.0);
}

#[test]
fn test_from_str() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let data =
    "---\nfile_name: example_name\nmy_struct_wrapper:\n  file_name: test_wrapper_nested\n  my_struct:\n    my_number:\n      file_name: number_wrapper\nmy_second_number:\n  file_name: number_wrapper\n";

    let resolved_struct: MyStructDoubleWrapper = dbm.from_str(data).unwrap();
    assert_eq!(
        resolved_struct.my_struct_wrapper.my_struct.my_number.val,
        42.0
    );
    assert_eq!(resolved_struct.my_second_number.val, 42.0);
}
