use std::{ffi::OsStr, path::Path, sync::Arc};

use shared_arc_db::*;
use serde::{Deserialize, Serialize};

#[derive(Clone, Serialize, Deserialize, Debug)]
struct NumberSingleWrapper {
    file_name: String,
    val: f64,
}

impl DatabaseEntry for NumberSingleWrapper {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("number_wrapper")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NumberDoubleWrapper {
    file_name: String,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_wrapper: Arc<NumberSingleWrapper>,
}

impl DatabaseEntry for NumberDoubleWrapper {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("number_wrapper")
    }
}

#[derive(Serialize, Deserialize, Debug, Clone)]
struct NumberTripleWrapper {
    file_name: String,
    #[serde(
        serialize_with = "serialize_arc_dbm",
        deserialize_with = "deserialize_arc_dbm"
    )]
    my_wrapper: Arc<NumberDoubleWrapper>,
}

impl DatabaseEntry for NumberTripleWrapper {
    fn file_name(&self) -> &OsStr {
        self.file_name.as_ref()
    }

    fn folder_name() -> &'static OsStr {
        OsStr::new("number_wrapper")
    }
}

#[test]
fn test_read_from_database_triple_wrapped() {
    let path_db = "tests/test_database";
    let mut dbm =
        DatabaseManager::from_path(Path::new(path_db).to_path_buf(), DatabaseFormat::Yaml).unwrap();

    let resolved_struct: NumberTripleWrapper = dbm.read("number_triple_wrapper").unwrap();
    assert_eq!(42.0, resolved_struct.my_wrapper.my_wrapper.val);
}
