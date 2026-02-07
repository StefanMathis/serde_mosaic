/*!
Functions and structs used in the other integration tests.
 */

#![allow(dead_code)]

use std::{ffi::OsStr, path::Path, sync::Arc};

use serde::{Deserialize, Serialize};
use serde_mosaic::*;

#[derive(Clone, Serialize, Deserialize, Debug, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: usize,
    pub name: String,
}

#[typetag::serde]
impl DatabaseEntry for Material {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cup {
    pub name: String,
    #[serde(deserialize_with = "deserialize_link")]
    #[serde(serialize_with = "serialize_link")]
    pub material: Material,
}

#[typetag::serde]
impl DatabaseEntry for Cup {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shovel {
    pub name: String,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub shaft: Arc<Material>,
    #[serde(deserialize_with = "deserialize_link")]
    #[serde(serialize_with = "serialize_link")]
    pub blade: Material,
}

#[typetag::serde]
impl DatabaseEntry for Shovel {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct User {
    pub name: String,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub shovel: Arc<Shovel>,
}

#[typetag::serde]
impl DatabaseEntry for User {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Stool {
    pub name: String,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub leg_1: Arc<Material>,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub leg_2: Arc<Material>,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub leg_3: Arc<Material>,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    pub seat: Arc<Material>,
}

#[typetag::serde]
impl DatabaseEntry for Stool {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Shelf {
    pub name: String,
    #[serde(deserialize_with = "deserialize_opt_arc_link")]
    #[serde(serialize_with = "serialize_opt_arc_link")]
    pub shovel: Option<Arc<Shovel>>,
}

#[typetag::serde]
impl DatabaseEntry for Shelf {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct Cupboard {
    pub name: String,
    #[serde(deserialize_with = "deserialize_opt_link")]
    #[serde(serialize_with = "serialize_opt_link")]
    pub cup: Option<Cup>,
}

#[typetag::serde]
impl DatabaseEntry for Cupboard {
    fn name(&self) -> &OsStr {
        self.name.as_ref()
    }
}

pub fn test_database() -> DatabaseManager {
    let path_db = "tests/test_database";
    return DatabaseManager::open(Path::new(path_db).to_path_buf(), SerdeYaml).unwrap();
}
