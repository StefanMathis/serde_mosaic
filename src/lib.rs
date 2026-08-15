/*!
[`serde`]: https://serde.rs
[`typetag`]: https://docs.rs/typetag/latest/typetag/
[`serialize_with`]: https://serde.rs/field-attrs.html#serialize_with
[`deserialize_with`]: https://serde.rs/field-attrs.html#deserialize_with
[`DatabaseEntry`]: crate::database_manager::DatabaseEntry
[`DatabaseManager`]: crate::database_manager::DatabaseManager
[`DatabaseManager::file_ext`]: crate::database_manager::DatabaseManager::file_ext
[`serialize_link`]: crate::attributes::serialize_link
[`deserialize_link`]: crate::attributes::deserialize_link
[`serialize_arc_link`]: crate::attributes::serialize_arc_link
[`deserialize_arc_link`]: crate::attributes::deserialize_arc_link
[`SerdeYaml`]: crate::format::SerdeYaml
[`SerdeJson`]: crate::format::SerdeJson
[`Format`]: crate::format::Format
[`serde_json`]: https://docs.rs/serde_json/latest/serde_json/
[`yaml_serde`]: https://docs.rs/yaml_serde/latest/yaml_serde/

Composable serialization and deserialization for Rust structs.

 */
#![doc = include_str!("../docs/main.md")]
#![deny(missing_docs)]

pub mod attributes;
pub mod database_manager;
pub mod format;

pub use attributes::*;
pub use database_manager::*;
pub use format::*;

pub use serde;
