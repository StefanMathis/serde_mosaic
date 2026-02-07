/*!
This module contains the [`Format`] trait, which is used by a
[`DatabaseManager`](crate::DatabaseManager) to serialize / deserialize
[`DatabaseEntry`] instances. See the the trait docstring for more.

Additionally, it also contains the following predefined implementors of
[`Format`]:
- [`SerdeJson`]
- [`SerdeYaml`]
*/

use std::error::Error;
use std::ffi::OsStr;

use dyn_clone::DynClone;

use crate::DatabaseEntry;

/**
A trait defining the serialization / deserialization strategy used by a
[`DatabaseManager`](crate::DatabaseManager).

Implementors of this trait are used to construct
[`DatabaseManager`](crate::DatabaseManager) instances. Within the database
manager, the [`serialize`](Format::serialize) and
[`deserialize`](Format::deserialize) methods are used whenever the manager needs
to serialize / deserialize a struct or one of its components (e.g. when it
encounters a struct field annotated by one of the "link" attributes from
[`attributes`](crate::attributes)).

Besides the predefined [`SerdeJson`] and [`SerdeYaml`] implementors, it is very
easy to define a custom [`Format`] based on one of the various serialization /
deserialization crates available. See the method docstrings for more. The
implementations for the predefined types are also very simple (6 LoC per type)
and can be used as examples.

Because a [`DatabaseManager`](crate::DatabaseManager) must be cloneable, any
implementor of this trait must implement [`Clone`] as well.
 */
pub trait Format: DynClone + std::any::Any {
    /**
    Returns the file extension used within the database. This extension is added
    to any files created by the [`DatabaseManager`](crate::DatabaseManager) and
    in return is expected when reading a file. For example, if this function
    returns "yaml" and [`DatabaseEntry::name`] returns "basket", the file
    created inside the database is "basket.yaml".

    An empty string means that no file extension shold be used (the file name
    for the previous example would then be "basket").
     */
    fn file_ext(&self) -> &OsStr;

    /**
    Serializes a [`DatabaseEntry`] trait object into a serialized bytes
    representation.

    Creating a new [`DatabaseManager`](crate::DatabaseManager) requires
    specifying the data format of the underlying database via a [`Format`] trait
    object. This method of the given trait object is used by the database
    manager when it needs to serialize a struct (either the originally given
    input struct or one of its components annotated by one of the "link"
    attributes from [`attributes`](crate::attributes)).

    This function uses trait objects both for the input and the error because
    the implementation strategy used for
    [`DatabaseManager`](crate::DatabaseManager) does not allow the usage of
    generics.

    If an ASCII serializer / deserializer is used, sometimes only a `to_string`
    method is available. In this case, it is recommended to convert the
    resulting [`String`] with [`String::into_bytes`]. For example, the
    predefined [`SerdeYaml`] and [`SerdeJson`] formats do just that:

    ```
    use std::ffi::OsStr;

    use serde::{Deserialize, Serialize};
    use serde_mosaic::*;

    #[derive(Serialize, Deserialize, Clone)]
    struct Material {
        name: String,
        cotton_content: f64,
    }

    #[typetag::serde]
    impl DatabaseEntry for Material {
        fn name(&self) -> &OsStr {
            self.name.as_ref()
        }
    }

    let pure_cotton = Material {
        name: "pure_cotton".into(),
        cotton_content: 100.0,
    };

    let format = SerdeYaml {};
    let bytes = format.serialize(&pure_cotton).expect("must succeed");
    let reconstructed_string = String::from_utf8(bytes).
        expect("is valid utf8 because the bytes come from a string");
    ```
     */
    fn serialize(&self, value: &dyn DatabaseEntry)
    -> Result<Vec<u8>, Box<dyn Error + Send + Sync>>;

    /**
    Deserializes a [`DatabaseEntry`] trait object from a serialized bytes
    representation.

    Creating a new [`DatabaseManager`](crate::DatabaseManager) requires
    specifying the data format of the underlying database via a [`Format`] trait
    object. This method of the given trait object is used by the database
    manager when it needs to deserialize a struct (either the output struct
    or one of its components annotated by one of the "link" attributes from
    [`attributes`](crate::attributes)).

    This function uses trait objects both for the output and the error
    because the implementation strategy used for
    [`DatabaseManager`](crate::DatabaseManager) does not allow the usage of
    generics.

    If an ASCII serializer / deserializer is used, sometimes only a
    `from_string` method is available. In this case, it is recommended to
    convert the given bytes slice into a [`str`] with [`str::from_utf8`].
    For example, the predefined [`SerdeYaml`] and [`SerdeJson`] formats do just
    that:

    ```
    use std::ffi::OsStr;
    use std::any::Any;

    use serde::{Deserialize, Serialize};
    use serde_mosaic::*;

    #[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
    struct Cloth {
        name: String,
        cotton_content: f64,
    }

    #[typetag::serde]
    impl DatabaseEntry for Cloth {
        fn name(&self) -> &OsStr {
            self.name.as_ref()
        }
    }

    let pure_cotton = Cloth {
        name: "pure_cotton".into(),
        cotton_content: 100.0,
    };

    let format = SerdeYaml {};
    let bytes = format.serialize(&pure_cotton).expect("must succeed");
    let boxed_mat = format.deserialize(&bytes).expect("must succeed") as Box<dyn Any>;
    let reconstructed_mat: Cloth = *boxed_mat.downcast().expect("is material");
    assert_eq!(pure_cotton, reconstructed_mat);
    ```
     */
    fn deserialize(
        &self,
        bytes: &[u8],
    ) -> Result<Box<dyn DatabaseEntry>, Box<dyn Error + Send + Sync>>;
}

dyn_clone::clone_trait_object!(Format);

/**
A [`Format`] which uses [`serde_yaml`] for its implementation of
[`Format::serialize`] and [`Format::deserialize`]. The file extension is "yaml".

This is a zero-sized struct which does not contain any data, it is purely used
as a "marker" to tell a [`DatabaseManager`](crate::DatabaseManager) how a
[`DatabaseEntry`] should be serialized / deserialized and which file extension
should be used.
 */
#[cfg(feature = "serde_yaml")]
#[derive(Clone, Copy, Debug)]
pub struct SerdeYaml;

#[cfg(feature = "serde_yaml")]
impl Format for SerdeYaml {
    fn file_ext(&self) -> &OsStr {
        return OsStr::new("yaml");
    }

    fn serialize(
        &self,
        value: &dyn DatabaseEntry,
    ) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let value = serde_yaml::to_string(value)?;
        return Ok(value.into_bytes());
    }

    fn deserialize(
        &self,
        bytes: &[u8],
    ) -> Result<Box<dyn DatabaseEntry>, Box<dyn Error + Send + Sync>> {
        let str = std::str::from_utf8(bytes)?;
        let value = serde_yaml::from_str(str)?;
        return Ok(value);
    }
}

/**
A [`Format`] which uses [`serde_json`] for its implementation of
[`Format::serialize`] and [`Format::deserialize`]. The file extension is "json".

This is a zero-sized struct which does not contain any data, it is purely used
as a "marker" to tell a [`DatabaseManager`](crate::DatabaseManager) how a
[`DatabaseEntry`] should be serialized / deserialized and which file extension
should be used.
 */
#[cfg(feature = "serde_json")]
#[derive(Clone, Copy, Debug)]
pub struct SerdeJson;

#[cfg(feature = "serde_json")]
impl Format for SerdeJson {
    fn file_ext(&self) -> &OsStr {
        return OsStr::new("json");
    }

    fn serialize(
        &self,
        value: &dyn DatabaseEntry,
    ) -> Result<Vec<u8>, Box<dyn Error + Send + Sync>> {
        let value = serde_json::to_string(value)?;
        return Ok(value.into_bytes());
    }

    fn deserialize(
        &self,
        bytes: &[u8],
    ) -> Result<Box<dyn DatabaseEntry>, Box<dyn Error + Send + Sync>> {
        let str = std::str::from_utf8(bytes)?;
        let value = serde_json::from_str(str)?;
        return Ok(value);
    }
}
