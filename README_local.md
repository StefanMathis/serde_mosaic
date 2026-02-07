serde_mosaic
============

[`serde`]: https://serde.rs
[`typetag`]: https://docs.rs/typetag/latest/typetag/
[`serialize_with`]: https://serde.rs/field-attrs.html#serialize_with
[`deserialize_with`]: https://serde.rs/field-attrs.html#deserialize_with
[`DatabaseEntry`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/database_manager/trait.DatabaseEntry.html
[`DatabaseManager`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/database_manager/struct.DatabaseManager.html
[`DatabaseManager::file_ext`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/database_manager/struct.DatabaseManager.html#method.file_ext
[`serialize_link`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/attributes/fn.serialize_link.html
[`deserialize_link`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/attributes/fn.deserialize_link.html
[`serialize_arc_link`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/attributes/fn.serialize_arc_link.html
[`deserialize_arc_link`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/attributes/fn.deserialize_arc_link.html
[`SerdeYaml`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/format/struct.SerdeYaml.html
[`SerdeJson`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/format/struct.SerdeJson.html
[`Format`]: https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/format/trait.Format.html
[`serde_json`]: https://docs.rs/serde_json/latest/serde_json/
[`serde_yaml`]: https://docs.rs/serde_yaml/latest/serde_yaml/

Composable serialization and deserialization for Rust structs.

This crate allows a composed struct to be serialized into the serialized forms
of its individual components. Likewise, a composed struct can be deserialized
from multiple serialized component forms. This enables sharing serialized
components across multiple composed structs --even of different types -- and 
reduces duplication when the serialized data is stored in a database.

Currently, only a file-system based database type is available, but the concept
can be easily expanded for other database types (e.g. an in-memory database).
Please open an issue on
[Github](https://github.com/StefanMathis/serde_mosaic.git) if needed.

This crate builds on Serde but is not affiliated with the Serde project.

# An introductory example

Suppose we have a `Shirt` type. Shirts of the same type share the same material,
but can have different owners and sizes:

```rust
struct Material {
    name: String,
    // A value in percent, the rest of the material is linen
    cotton_content: f64,
}

struct Shirt {
    owner: String,
    material: Material,
    size: usize
}
```

We now want to serialize different shirt instances made from the same material
and store them in a database. Clearly, it would be a waste to store the
serialized form of the material multiple times as part of the `Shirt`
serialization. This is where `serde_mosaic` comes into play: It provides the
functions [`serialize_link`] and [`deserialize_link`] which can be used in
conjunction with the [`serialize_with`] and [`deserialize_with`] field
attributes of the [`serde`] crate to mark the `Material` component of the
`Shirt` for composed serialization / deserialization. To provide a unique
identifier of the component, the [`DatabaseEntry`] trait needs to be implemented
for `Material`. The `#[typetag::serde]` macro from the
[`typetag`](https://crates.io/crates/typetag) crate is also needed:

```rust

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

#[derive(Serialize, Deserialize)]
struct Shirt {
    owner: String,
    #[serde(deserialize_with = "deserialize_link")]
    #[serde(serialize_with = "serialize_link")]
    material: Material,
    size: usize
}

#[typetag::serde]
impl DatabaseEntry for Shirt {
    fn name(&self) -> &OsStr {
        self.owner.as_ref()
    }
}
```

Now, the location of the database in the file system and its format must be
specified. `serde_mosaic` provides multiple predefined formats such as
[`SerdeYaml`] or [`SerdeJson`], but it is also possible to define your own
format by implementing the [`Format`] trait. For the example, let's stick with
[`SerdeYaml`]:

```rust,no_run
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

#[derive(Serialize, Deserialize)]
struct Shirt {
    owner: String,
    #[serde(deserialize_with = "deserialize_link")]
    #[serde(serialize_with = "serialize_link")]
    material: Material,
    size: usize
}

#[typetag::serde]
impl DatabaseEntry for Shirt {
    fn name(&self) -> &OsStr {
        self.owner.as_ref()
    }
}

let pure_cotton = Material {
    name: "pure_cotton".into(),
    cotton_content: 100.0,
};

let mikes_shirt = Shirt {
    owner: "mike".into(),
    material: pure_cotton.clone(),
    size: 40
};

let joes_shirt = Shirt {
    owner: "joe".into(),
    material: pure_cotton.clone(),
    size: 38
};

let mut dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists or can be created");

// Now serialize the shirt representations. `WriteOptions` allows you to detail
// how the actual representation looks like
let write_options = WriteOptions::default();
dbm.write(&mikes_shirt, &write_options).expect("serialization and writing succeeds");
dbm.write(&joes_shirt, &write_options).expect("serialization and writing succeeds");
```

This creates the following files:
```ignore
/path/to/db/Material/pure_cotton.yaml
/path/to/db/Shirt/joe.yaml
/path/to/db/Shirt/mike.yaml
```

The files `joe.yaml` and `mike.yaml` do not contain the serialized
representation of `pure_cotton`, but only a link to `pure_cotton.yaml`. When
deserializing `joe.yaml`, the [`DatabaseManager`] interprets the link,
deserializes `pure_cotton.yaml` and puts the resulting `Material` into the
`material` field of `Shirt`:

```rust,ignore
let mut dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists");
let joes_shirt: Shirt = dbm.read("joe").expect("file exists");
```

"Normal" serialization and deserialization without a [`DatabaseManager`] is
still possible - the attribute functions [`serialize_link`] and
[`deserialize_link`] are no-ops in such a case, serialization and
deserialization works as expected.

# Reference-counted components

If the same `Material` should be shared between different `Shirt`s not just in
the database, but also within memory, a common Rust pattern is to use a
reference counter such as `Arc`. `serde_mosaic` supports this with the
[`serialize_arc_link`] and [`deserialize_arc_link`] functions:

```rust
use std::ffi::OsStr;
use std::sync::Arc;

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

#[derive(Serialize, Deserialize)]
struct Shirt {
    owner: String,
    #[serde(deserialize_with = "deserialize_arc_link")]
    #[serde(serialize_with = "serialize_arc_link")]
    material: Arc<Material>,
    size: usize
}

#[typetag::serde]
impl DatabaseEntry for Shirt {
    fn name(&self) -> &OsStr {
        self.owner.as_ref()
    }
}
```

The [`DatabaseManager`] maintains a cache of `Arc<Material>` instances which
have already been deserialized before. If the database manager encounters a
cached link / file name in a second `Shirt` it is currently deserializing, it
reuses the cached instance by cloning the `Arc` pointer and inserting the clone
in the newly deserialized `Shirt`.

# Optional fields

It is also possible to have optional fields used for composition:

```rust
use std::ffi::OsStr;
use std::sync::Arc;

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

#[derive(Serialize, Deserialize)]
struct FancyShirt {
    owner: String,
    #[serde(deserialize_with = "deserialize_opt_arc_link")]
    #[serde(serialize_with = "serialize_opt_arc_link")]
    cuff_material: Option<Arc<Material>>,
    #[serde(deserialize_with = "deserialize_opt_link")]
    #[serde(serialize_with = "serialize_opt_link")]
    collar_material: Option<Material>,
    size: usize
}

#[typetag::serde]
impl DatabaseEntry for FancyShirt {
    fn name(&self) -> &OsStr {
        self.owner.as_ref()
    }
}
```

When the optional field is empty, the link in the serialized representation is
simply empty as well.

# Serialized representation

As mentioned before, the serialized representation of a composed struct contains
a "link" instead of the actual field contents. For example, the
yaml-representation of `Shirt` created by the [`DatabaseManager`] looks like
this:

```yaml
---
Shirt:
  name: mike
  material:
    name: pure_cotton
    checksum: 94637245
  size: 40
```

The "link" consists of the two fields `name` and `checksum`. The `name` field
tells the [`DatabaseManager`] to look for a file "pure_cotton.yaml" (the file
extension is derived from [`DatabaseManager::file_ext`]) and deserialize that
file; the resulting `Material` instance is then put into `Shirt`. The `checksum`
field is optional and should be omitted when creating a database entry manually.
The number is a hash which is used to check if a file changed during the
lifetime of a [`DatabaseManager`]. This avoids a stale cache for
reference-counted components.

One difference to the "standard" yaml-representation of `Shirt` is the fact that
the type is stated at the very top of the hierarchy. This is necessary because
internally, `Shirt` is serialized as a [`DatabaseEntry`] trait object via
[typetag] (which in turn is necessary to allow for arbitrary [`Format`]s). Since
[typetag] treats trait objects as enum variants, the "variant name" (which is
the type name) needs to be stated explictly.

Creating a database entry for a pure cotton shirt manually could look like this:

`/path/to/db/Shirt/sarah.yaml`
```yaml
---
Shirt:
  name: sarah
  material:
    name: pure_cotton
  size: 39
```

`/path/to/db/Material/pure_cotton.yaml`
```yaml
---
Material:
  name: pure_cotton
  cotton_content: 100
```

# Predefined database formats

This crate offers several predefined [`Format`]s which are gates behind feature
flags.

## JSON

Enabling the `serde_json` feature provides the [`SerdeJson`] database format.
This format uses the [`serde_json`] crate for serializing and deserializing the
database entries.

## YAML

Enabling the `serde_yaml` feature provides the [`SerdeYaml`] database format.
This format uses the [`serde_yaml`] crate for serializing and deserializing the
database entries.

# Examples in the `/tests` directory

The repository contains a fully-fledged database within `test/test_database` as
well as various examples for reading from and writing to that database in
`tests`. I tried very hard to make these tests as self-explanatory as possible,
but please open an issue on
[Github](https://github.com/StefanMathis/serde_mosaic.git) if help is needed.

- `tests/basic_db_manipulation.rs`: Interaction with the database via the
[`DatabaseManager`] (e.g. checking if an entry already exists, clearing database
entries based on their name etc.)
- `tests/read.rs`: Deserializing composed structs from the database, with
examples for `Arc` (incl. in-memory sharing), `Option` and nested composed
structs.
- `tests/serialize_and_deserialize.rs`: Serializing and deserializing structs
with the `.._link` attributes without a [`DatabaseManager`] (i.e. "normal"
[serde] behaviour).
- `tests/utilities.rs`: Definition of the structs used within the tests.
- `tests/write_and_read.rs`: Serializing to and serialization from the database,
basically a composition of `tests/read.rs` and `tests/write.rs`
- `tests/write.rs`: Serializing composed structs into the database, with
examples for `Arc`, `Option` and nested composed structs.

It is recommended to first check out `tests/write.rs` and `tests/read.rs` to
understand how to work with this crate.

# Documentation

The full API documentation is available at
[https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/](https://docs.rs/serde_mosaic/0.1.0/serde_mosaic/).