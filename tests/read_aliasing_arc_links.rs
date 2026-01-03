use std::{ffi::OsStr, path::Path, sync::Arc};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};
use std::ptr;

#[derive(Serialize, Deserialize, Debug)]
struct MyStruct {
    file_name: String,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_first_arc_number: Arc<NumberWrapper>,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_second_arc_number: Arc<NumberWrapper>,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_third_arc_number: Arc<NumberWrapper>,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_fourth_arc_number: Arc<NumberWrapper>,
}

impl DatabaseEntry for MyStruct {
    fn file_name(&self) -> &OsStr {
        return self.file_name.as_ref();
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
        &self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("number_wrapper")
    }
}

/**
Test reading two fields which link to the same database field. The resulting Arc pointers should point to the same memory location, since the fields should be aliasing.
 */
#[test]
fn test_read_from_database() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let resolved_struct: MyStruct = dbm.read("my_struct_multiple_aliasing_db_links").unwrap();

    // Check that the pointers of fields 1-3 are aliasing
    assert!(ptr::eq(
        &*resolved_struct.my_first_arc_number,
        &*resolved_struct.my_second_arc_number
    ));
    assert!(ptr::eq(
        &*resolved_struct.my_first_arc_number,
        &*resolved_struct.my_third_arc_number
    ));
    assert!(ptr::eq(
        &*resolved_struct.my_second_arc_number,
        &*resolved_struct.my_third_arc_number
    ));

    // But field 4 is not aliasing with the other three!
    assert!(!ptr::eq(
        &*resolved_struct.my_first_arc_number,
        &*resolved_struct.my_fourth_arc_number
    ));
    assert!(!ptr::eq(
        &*resolved_struct.my_second_arc_number,
        &*resolved_struct.my_fourth_arc_number
    ));
    assert!(!ptr::eq(
        &*resolved_struct.my_third_arc_number,
        &*resolved_struct.my_fourth_arc_number
    ));
}
