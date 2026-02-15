/*!
This module contains the [`DatabaseManager`] type, which is the heartpiece of
this crate and enables composed serialization / deserialization via its
[`DatabaseManager::write`] and [`DatabaseManager::read`] methods. All other
types and traits within this module are used to support this functionality:
- The [`DatabaseEntry`] trait is used to specify the "name" keys which are used
to store and retrieve serialized types into / from the database.
- The [`Cache`] and [`CacheEntry`] types allow the reusage of reference-counted
database entries across multiple composed types (see also
[`deserialize_arc_link`](crate::attributes::deserialize_arc_link)).
- The [`DatabaseKey`] is used to interact with the database (e.g. check the
existence of entries, delete them etc.)
- The [`WriteOptions`] type and its components [`WriteMode`] and
[`NameCollisions`] allows customizing the behaviour when serializing
into the database with [`DatabaseManager::write`].
- [`WriteInfo`] and [`ReadInfo`] are returned by the verbose write / read
alternatives [`DatabaseManager::write_verbose`] and
[`DatabaseManager::read_verbose`]. They contain additional informations about
the writing / reading process.
 */

use std::any::{Any, TypeId};
use std::fmt::Debug;
use std::sync::Arc;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::{self, File, remove_file},
    io::{BufReader, Error, ErrorKind, Write},
    mem,
    path::{Path, PathBuf},
};

use deserialize_untagged_verbose_error::DeserializeUntaggedVerboseError;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use std::cell::{Cell, RefCell};

use crate::Format;

/**
Returns the "name" of a type as a string slice. This function uses
[`std::any::type_name`] under the hood, but only returns the last part of the
slice which is the actual file name. For example,
`std::any::type_name::<String>` might return `std::string::String`, whereas this
function always just returns `String`.

This function is used throughout this crate to derive the folder names within
a database created by a [`DatabaseManager`]. For example, if a type `Material`
is stored within a database in `/path/to/db`, the folder name for the file is
determined by calling `type_name::<Material>()`, resulting in the file path
`/path/to/db/Material/file_name`.
 */
pub fn type_name<T>() -> &'static str {
    let full_name = std::any::type_name::<T>();
    full_name
        .rsplit("::")
        .next()
        .expect("full type name has at least one entry")
}

/**
Trait which allows storing an object within a database.

This is the central trait of this crate. Objects implementing this trait can be
stored independently of their parent struct when the latter is written to a
database via the [`DatabaseManager::write`] method. Similarily, when reading
from a database using [`DatabaseManager::read`], database entries of
implementors can be used to deserialize parent structs which link to them.

Implementors of this trait must fulfill the following conditions:
1) They must implement [`serde::ser::Serialize`] and [`serde::de::Deserialize`].
2) The implementation block must be annotated with `#[typetag::serde]`.
3) They must provide a unique "name" which can be used to identify individual
instances of the implementing type within the database. This "name" should be
retourned by [`DatabaseEntry::name`].

The second requirement allows serializing / deserializing the implementor as a
trait object. This is necessary for the following reasons:
- The [`Format`] of the [`DatabaseManager`] is internally stored as a trait
object to make it non-generic. This in turn is required due to the usage of
thread-local variables which inject the [`DatabaseManager`] into the
serialization / deserialization process.
- The [`Cache`] must not be generic to allow for full flexibility (i.e. an
arbitrary number of different stored types).
 */
#[typetag::serde]
pub trait DatabaseEntry: Any {
    /**
    To be uniquely identifiable by a "link", each [`DatabaseEntry`] must provide
    its own "name". This name is used both as the link in the serialized
    representation of the parent struct and also to determine the file name
    where the actual field contents are stored.
     */
    fn name(&self) -> &OsStr;
}

/**
A cache for (type-erased) [`DatabaseEntry`] objects stored in an [`Arc`]
pointer.

One central feature of this crate is the reuse of reference-counted
[`DatabaseEntry`] objects during deserialization of other objects containing
them.

As an example, consider the definitions below:

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
If a database contains a file `Material/pure_cotton` and multiple `Shirt` files
which have a link to "pure_cotton", the same [`Arc`]-wrapped instance gets
reused when reading multiple `Shirt` instances using the same
[`DatabaseManager`]. The `tests` directory of the repository contains some
concrete examples.

The [`Cache`] type itself is created as part of a [`DatabaseManager`] and is
populated everytime an [`Arc`]-wrapped [`DatabaseEntry`] annotated with
[`deserialize_arc_link`](crate::attributes::deserialize_arc_link) or
[`deserialize_opt_arc_link`](crate::attributes::deserialize_opt_arc_link)
gets deserialized. The cache is accessible via [`DatabaseManager::cache`] and
can also be manually adjusted with [`DatabaseManager::cache_mut`] (see
[`CacheEntry::insert`] for an example).

The structure of the type is as follows:
- The inner [`HashMap`] contains type-erased instances
([`Arc<dyn DatabaseEntry>`]) whose key is their [`DatabaseEntry::name`]. All
instances have the same type.
- The outer [`HashMap`] uses the [`TypeId`] of the stored type as the key for
the corresponding inner [`HashMap`].

See also [`CacheEntry`].
 */
pub type Cache = HashMap<TypeId, HashMap<OsString, CacheEntry>>;

/**
A [`Cache`] entry containing the cached instance itself (within its
[`CacheEntry::arc`] field) and optionally the [`CacheEntry::checksum`] of the
instance.

This struct is usually created within
[`deserialize_arc_link`](crate::attributes::deserialize_arc_link), but can also
be created manually to modify the [`Cache`] of a [`DatabaseManager`]. This can
be useful to force the reuse of an existing [`DatabaseEntry`] instance within
a composed struct. As an example, assume that the following database entry
should be deserialized:

```yaml
---
Shirt:
  name: mike
  material:
    name: pure_cotton
    checksum: 1234114
  size: 40
```

Adding a `Material` instance whose [`DatabaseEntry::name`] function returns
`pure_cotton` to the [`Cache`] of the [`DatabaseManager`] now forces reuse of
that instance when deserializing "mike"s `Shirt`. Any checksum within the link
is simply ignored when using a manually created [`CacheEntry`].

```no_run
use std::ffi::OsStr;
use std::sync::Arc;

use serde::{Serialize, Deserialize};
use serde_mosaic::*;

#[derive(Serialize, Deserialize)]
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

let pure_cotton = Arc::new(Material {
    name: "pure_cotton".into(),
    cotton_content: 100.0,
});

let mut dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists");
CacheEntry::insert(dbm.cache_mut(), pure_cotton);
```
*/
#[derive(Clone)]
pub struct CacheEntry {
    /**
    The pointer to the underlying cached [`DatabaseEntry`]. It is type-erased so
    a [`Cache`] can store an arbitrary number of types. When reading from the
    cache inside
    [`deserialize_arc_link`](crate::attributes::deserialize_arc_link), the
    concrete type is known and the [`Any`] trait object can be safely downcast.
     */
    pub arc: Arc<dyn DatabaseEntry + Send + Sync + 'static>,
    /**
    If a [`CacheEntry`] is created within
    [`deserialize_arc_link`](crate::attributes::deserialize_arc_link), the
    checksum of the created file is stored within the link left in the parent
    struct. This is used during deserialization to see if a cached instance can
    be used or whether the actual file should be deserialized. When manually
    creating a [`CacheEntry`], this field is set to [`None`].
     */
    pub checksum: Option<u32>,
}

impl CacheEntry {
    /**
    Creates a new [`CacheEntry`] from a type-erased [`DatabaseEntry`]. This
    method is useful when manually adding elements to a [`Cache`], although
    the convenience method [`CacheEntry::insert`] should be considered instead.
     */
    pub fn new(arc: Arc<dyn DatabaseEntry + Send + Sync + 'static>) -> Self {
        return Self::from(arc);
    }

    /**
    Creates a new instance of [`CacheEntry`] from a [`DatabaseEntry`] and puts
    it into the given [`Cache`]. For the "outer" [`HashMap`] of the [`Cache`],
    the [`TypeId`] of `T` is used as key. [`DatabaseEntry::name`] then returns
    the inner key. If there is already an entry for the inner key, the new one
    is inserted and the old one is returned.

    This is a static function of [`CacheEntry`] rather than a [`Cache`] method,
    because the latter is just a type alias, hence defining a new method for
    [`Cache`] is not possible (without implementing a custom trait).

    # Examples

    ```
    use std::ffi::OsStr;
    use std::sync::Arc;

    use serde::{Serialize, Deserialize};
    use serde_mosaic::*;

    #[derive(Serialize, Deserialize)]
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

    let pure_cotton = Arc::new(Material {
        name: "pure_cotton".into(),
        cotton_content: 100.0,
    });

    let mut cache = Cache::new();

    // Insert into the empty cache
    assert_eq!(cache.len(), 0);
    assert!(CacheEntry::insert(&mut cache, pure_cotton.clone()).is_none());

    // Now insert the instance again. The old one is returned.
    assert_eq!(cache.len(), 1);
    assert!(CacheEntry::insert(&mut cache, pure_cotton).is_some());
    ```
     */
    pub fn insert<T: DatabaseEntry + Send + Sync>(
        cache: &mut Cache,
        instance: Arc<T>,
    ) -> Option<Arc<T>> {
        let type_id = TypeId::of::<T>();
        let name = instance.name().to_owned();
        match cache.get_mut(&type_id) {
            Some(subcache) => {
                let old_entry = subcache.insert(name, CacheEntry::new(instance))?;
                let any_arc = old_entry.arc as Arc<dyn Any + Send + Sync + 'static>;
                return any_arc.downcast().ok();
            }
            None => {
                let mut subcache = HashMap::new();
                subcache.insert(name, CacheEntry::new(instance));
                cache.insert(type_id, subcache);
                return None;
            }
        }
    }
}

impl From<Arc<dyn DatabaseEntry + Send + Sync + 'static>> for CacheEntry {
    fn from(value: Arc<dyn DatabaseEntry + Send + Sync + 'static>) -> Self {
        return Self {
            arc: value,
            checksum: None,
        };
    }
}

impl From<CacheEntry> for Arc<dyn Any + Send + Sync + 'static> {
    fn from(value: CacheEntry) -> Self {
        return value.arc;
    }
}

/**
This struct is used to access database entries via a [`DatabaseManager`]. It
contains the folder (typename) where a file containing the contents of an entry
is stored.

This struct is usually not created manually, but via one of its [`From`]
implementations. For example, every `T` implementing [`DatabaseEntry`] has a
blanket [`From<&T>`] implementation for [`DatabaseKey`]. It is also possible to
create it from a tuple of any two types implementing [`AsRef<OsStr>`]. The first
tuple element is interpreted as [`DatabaseKey::type_name`], the second as
[`DatabaseKey::name`].

# Examples

```no_run
use std::ffi::OsStr;

use serde::{Serialize, Deserialize};
use serde_mosaic::*;

#[derive(Serialize, Deserialize)]
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

let mut dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists");

assert!(!dbm.exists(&pure_cotton));
assert!(!dbm.exists(("Material", "pure_cotton")));

let write_options = WriteOptions::default();
dbm.write(&pure_cotton, &write_options).expect("serialization and writing succeeds");

assert!(dbm.exists(&pure_cotton));
assert!(dbm.exists(("Material", "pure_cotton")));
```
 */
pub struct DatabaseKey<'a> {
    /**
    The name of the folder where all database entries for a type `T` are stored.
    It is equivalent to the string returned by [`type_name`]. For example, for
    the type `Material` from the struct docstring, the folder name is simply
    "Material".
     */
    pub type_name: &'a OsStr,
    /**
    This is the "name" returned by [`DatabaseEntry::name`]. The file generated
    for a [`DatabaseEntry`] has this name (plus the file extension defined by
    [`Format::file_ext`]).
     */
    pub name: &'a OsStr,
}

impl<'a, T: DatabaseEntry> From<&'a T> for DatabaseKey<'a> {
    fn from(value: &'a T) -> Self {
        return Self {
            type_name: OsStr::new(type_name::<T>()),
            name: value.name(),
        };
    }
}

impl<'a, A, B> From<(&'a A, &'a B)> for DatabaseKey<'a>
where
    A: AsRef<OsStr> + ?Sized,
    B: AsRef<OsStr> + ?Sized,
{
    fn from(value: (&'a A, &'a B)) -> Self {
        Self {
            type_name: value.0.as_ref(),
            name: value.1.as_ref(),
        }
    }
}

impl<'a> From<[&'a OsStr; 2]> for DatabaseKey<'a> {
    fn from(value: [&'a OsStr; 2]) -> Self {
        return Self {
            type_name: value[0],
            name: value[1],
        };
    }
}

impl<'a> From<[&'a str; 2]> for DatabaseKey<'a> {
    fn from(value: [&'a str; 2]) -> Self {
        return Self {
            type_name: OsStr::new(value[0]),
            name: OsStr::new(value[1]),
        };
    }
}

/**
A manager for a file-system database.

A "database" is essentially just an arbitrary directory in the file system with
subdirectories for specific implementors of [`DatabaseEntry`]. Those
subdirectories are named after the implementor (see [`type_name`]) and stores
files with the serialized representations of individual instances. The
directories are created on a as-needed base by the [`DatabaseManager`].

The following code defines a database at the path `/path/to/db` (the "database
root") and stores two composed [`DatabaseEntry`] implementors within it.

```no_run
use std::ffi::OsStr;

use serde::{Serialize, Deserialize};
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
    #[serde(serialize_with = "serialize_link")]
    #[serde(deserialize_with = "deserialize_link")]
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
dbm.write(&mikes_shirt, &WriteOptions::default()).expect("serialization and writing succeeds");
dbm.write(&joes_shirt, &WriteOptions::default()).expect("serialization and writing succeeds");
```
The first `dbm.write` creates a directory `/path/to/db/Shirt` (if it didn't
exist yet) and a file `/path/to/db/Shirt/mike.yaml` where the content of the
variable `mikes_shirt` is stored. Additionally, since `pure_cotton` should be
linked during serialization and deserialization, a second file
`/path/to/db/Material/pure_cotton.yaml` (and the corresponding directory) is
created, which contains the serialized representation of `pure_cotton`.

The second `dbm.write` now creates a file `/path/to/db/Shirt/joe.yaml`. Since
`/path/to/db/Material/pure_cotton.yaml` already exists, no additional file is
created.

The `DatabaseManager` holds the path to the database directory `/path/to/db`,
the database format [`SerdeYaml`](crate::format::SerdeYaml) and a [`Cache`] for
reference-counted instances (see the docstring of [`Cache`] for more). It is
therefore cheap to create new `DatabaseManager` instances.

All methods which manipulate files (e.g. [`read`](DatabaseManager::read),
[`write`](DatabaseManager::write) or [`remove`](DatabaseManager::remove) take a
mutable reference of `self`. This is done in order to prevent race conditions
when operating multi-threaded. If it is necessary to use a [`DatabaseManager`]
in multiple threads at once, consider using a [`Mutex`](std::sync::Mutex) lock
or creating one manager instance per thread (although this prevents sharing the
[`Cache`] over the different threads).
 */
#[derive(Clone)]
pub struct DatabaseManager {
    dir: PathBuf,
    format: Box<dyn Format>,
    cache: Cache,
}

impl DatabaseManager {
    /**
    Creates a new instance of `Self` with the given `path` and `format`. If the
    path does not exist in the file system, this function tries to create it.
    Returns a [`std::io::Error`] if the directory cannot be used as a database
    (e.g. due to permission issues).

    # Examples

    ```no_run
    use serde_mosaic::*;

    let dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists or can be created");
    ```
    */
    pub fn new<P, F>(path: P, format: F) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
        F: Format + 'static,
    {
        return Self::with_boxed_format(path, Box::new(format));
    }

    /**
    Like [`DatabaseManager::new`], but takes a boxed [`Format`] trait object.
    [`DatabaseManager::new`] boxes its `format` argument and then calls this
    function. If the `format` is already available as a boxed trait object
    (e.g. because it has been taken from another [`DatabaseManager`] instance),
    using this function skips an allocation.
     */
    pub fn with_boxed_format<P>(path: P, format: Box<dyn Format>) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
    {
        // Check if the directory already exist. If not, try to create it
        let mut dir = PathBuf::new();
        dir.push(&path);
        if !dir.exists() {
            fs::create_dir(&dir).map_err(|err: Error| {
                Error::new(
                    err.kind(),
                    format!("Could not create directory {}", dir.display()),
                )
            })?;
        }
        return Self::open_with_boxed_format(path, format);
    }

    /**
    Like [`DatabaseManager::new`], but returns an error if the specified `path`
    does not exist.
     */
    pub fn open<P, F>(path: P, format: F) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
        F: Format + 'static,
    {
        return Self::open_with_boxed_format(path, Box::new(format));
    }

    /**
    Like [`DatabaseManager::open`], but takes a boxed [`Format`] instead of
    being generic. See [`DatabaseManager::with_boxed_format`] for details.
     */
    pub fn open_with_boxed_format<P>(path: P, format: Box<dyn Format>) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut dir = PathBuf::new();
        dir.push(path);

        if dir.exists() {
            return Ok(Self {
                dir,
                format,
                cache: Default::default(),
            });
        } else {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Could not find directory {}", dir.display()),
            ));
        }
    }

    /**
    Returns a reference to the [`Path`] used as the database root of `self`.

    # Examples

    ```no_run
    use std::path::Path;
    use serde_mosaic::*;

    let dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists or can be created");
    assert_eq!(dbm.dir(), Path::new("/path/to/db"));
    ```
     */
    pub fn dir(&self) -> &Path {
        return self.dir.as_path();
    }

    /**
    Returns a reference to the underlying [`Format`] of the database.

    Since the [`Format`] is internally stored as a trait object, this function
    returns a reference to that trait object as well. The trait bounds of
    [`Format`] guarantee that any implementor also implements the [`Any`] trait
    and can therefore be downcasted to the concrete type.

    # Examples

    ```no_run
    use std::any::Any;
    use serde_mosaic::*;

    let dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists or can be created");
    let format_ref = dbm.data_format() as &dyn Any; // Possible since Rust 1.86
    assert!(format_ref.downcast_ref::<SerdeYaml>().is_some());
    ```
     */
    pub fn data_format(&self) -> &dyn Format {
        return &*self.format;
    }

    /**
    Returns the file extension used by `self` to write and read files.

    This function is a shorthand for `dbm.data_format().file_ext()`.
     */
    pub fn file_ext(&self) -> &OsStr {
        return self.format.file_ext();
    }

    /**
    Returns the checksum of a database file specified by the given `key`. If
    the file doesn't exist, this function returns `None`.
     */
    pub fn checksum<'a, T: Into<DatabaseKey<'a>>>(&self, key: T) -> Option<u32> {
        return checksum(&self.full_path_unchecked(key));
    }

    /**
    Removes all empty subfolders within the database path `self.dir()`.

    Be aware that the [`DatabaseManager`] doesn't know which folders belong to
    the database and which folders do not. For example, the following snippet
    would remove an empty folder `/path/to/db/foo`, even though it wasn't
    created by the database manager:

    ```no_run
    use std::path::PathBuf;
    use serde_mosaic::*;

    let unrelated_dir = PathBuf::from("/path/to/db/foo");

    assert!(unrelated_dir.exists());
    assert!(unrelated_dir.read_dir().expect("read permissions available").next().is_none());

    let mut dbm = DatabaseManager::new("/path/to/db", SerdeYaml).expect("directory exists or can be created");

    assert!(unrelated_dir.exists());
    assert!(unrelated_dir.read_dir().expect("read permissions available").next().is_none());

    dbm.remove_empty_subfolders();

    assert!(!unrelated_dir.exists());
    ```
     */
    pub fn remove_empty_subfolders(&mut self) -> std::io::Result<()> {
        fn remove_priv(path: &Path) -> std::io::Result<()> {
            let reader = path.read_dir()?;
            for folder in reader {
                let dir_entry = folder?;

                // Check if the folder is empty
                let path = dir_entry.path();

                // Check if the folder is empty:
                // https://stackoverflow.com/questions/56744383/how-would-i-check-if-a-directory-is-empty-in-rust
                if path.read_dir()?.next().is_none() {
                    std::fs::remove_dir_all(path)?;
                }
            }
            return Ok(());
        }

        // =====================================================================

        remove_priv(self.dir())?;
        return Ok(());
    }

    /**
    Tries to remove the specified database file from the database.

    This function essentially derives the file path from the given `key` with
    [`DatabaseManager::full_path`] and then tries to delete the file. If the
    file doesn't exist or can't be removed, this function returns an error.

    Be aware that the [`DatabaseManager`] does not know which files "belong" to
    the database - if a file fitting the naming scheme has been created in an
    unrelated way, it will still be removed.
     */
    pub fn remove<'a, T: Into<DatabaseKey<'a>>>(&mut self, key: T) -> std::io::Result<()> {
        let file_path = self.full_path_unchecked(key);
        if file_path.exists() {
            return std::fs::remove_file(&file_path).map_err(|err| {
                Error::new(
                    err.kind(),
                    format!("Could not remove file {}: {}", file_path.display(), err),
                )
            });
        } else {
            return Ok(());
        }
    }

    /**
    Searches through all direct subfolders (non-recursively) of `self.dir()` and
    removes all files with the given file name whose file extension matches that
    of `self.file_ext`. Similar to [`DatabaseManager::remove`], this function
    does not discriminate between files which were created by `self` and files
    which were created by something else.
     */
    pub fn remove_all<O: AsRef<OsStr>>(&mut self, name: O) -> std::io::Result<()> {
        fn remove_all_inner(dbm: &mut DatabaseManager, name: &OsStr) -> std::io::Result<()> {
            let mut file_with_ext = name.to_os_string();
            if !dbm.file_ext().is_empty() {
                file_with_ext.push(".");
                file_with_ext.push(dbm.file_ext());
            }

            let paths = fs::read_dir(dbm.dir())?;

            // Iterate through all folders of the database
            for path in paths {
                if let Ok(dir) = path {
                    let file_path = dir.path().join(&file_with_ext);
                    if file_path.exists() {
                        std::fs::remove_file(&file_path)?;
                    }
                }
            }

            return Ok(());
        }
        return remove_all_inner(self, name.as_ref());
    }

    /**
    Checks if the database has an entry for the given `key`.

    Under the hood, this function calls `self.full_path(key).is_some()`.
     */
    pub fn exists<'a, T: Into<DatabaseKey<'a>>>(&self, key: T) -> bool {
        return self.full_path(key).is_some();
    }

    /**
    Returns the full path of the database entry specified by `key`, if the entry
    exist. If not, returns `None`.
     */
    pub fn full_path<'a, T: Into<DatabaseKey<'a>>>(&self, key: T) -> Option<PathBuf> {
        let path = self.full_path_unchecked(key);
        if path.exists() {
            return Some(path);
        } else {
            return None;
        }
    }

    pub(crate) fn full_path_unchecked<'a, T: Into<DatabaseKey<'a>>>(&self, key: T) -> PathBuf {
        let key: DatabaseKey = key.into();
        let mut file_with_ext = OsStr::new(&key.name).to_os_string();
        if !self.file_ext().is_empty() {
            file_with_ext.push(".");
            file_with_ext.push(self.file_ext());
        }
        return self
            .dir()
            .join(OsStr::new(&key.type_name))
            .join(file_with_ext);
    }

    /**
    Returns a reference to the [`Cache`] used within `self`.
     */
    pub fn cache(&self) -> &Cache {
        return &self.cache;
    }

    /**
    Returns a mutable reference to the [`Cache`] used within `self`. This can
    be used to manually add entries to the [`Cache`]. See the docstrings of
    [`Cache`] and [`CacheEntry`].
     */
    pub fn cache_mut(&mut self) -> &mut Cache {
        return &mut self.cache;
    }

    // ====================================================================
    // Serialization

    /**
    Serializes the given `instance` into the database according to the given
    [`WriteOptions`]. If successfull, the path to the written file is returned.

    This is the central function to store new entries within the database. As
    outlined in the docstring of [`DatabaseManager`], calling this function
    can actually result in multiple files being written, if `instance` is
    composed of other [`DatabaseEntry`] implementor instances which are
    annotated with one of the "link"
    [attributes for serialization](crate::attributes) (depending on the
    [`WriteMode`] of [`WriteOptions`]). Using serialization functions from other
    packages (as e.g. `serde_yaml::to_string`) bypasses the entire linking
    machinery of this crate and just creates the expected serialized
    representations.
    */
    pub fn write<T: DatabaseEntry>(
        &mut self,
        instance: &T,
        write_options: &WriteOptions,
    ) -> std::io::Result<PathBuf> {
        return self
            .write_verbose_log(instance, write_options, false)
            .map(|arg| arg.0);
    }

    /**
    Like [`DatabaseManager::write`], but returns additional [`WriteInfo`] in
    case writing to the database was successfull.

    The [`WriteInfo`] contains the following information:
    - Which files were created new.
    - Which existing files have been overwritten.

    These results heavily depend on the settings within [`WriteOptions`], see
    its docstring for more.
     */
    pub fn write_verbose<T: DatabaseEntry>(
        &mut self,
        instance: &T,
        write_options: &WriteOptions,
    ) -> std::io::Result<(PathBuf, WriteInfo)> {
        return self.write_verbose_log(instance, write_options, true);
    }

    fn write_verbose_log<T: DatabaseEntry>(
        &mut self,
        instance: &T,
        write_options: &WriteOptions,
        log: bool,
    ) -> std::io::Result<(PathBuf, WriteInfo)> {
        let result = WRITE_CONTEXT.with(|thread_context| {
            // Context only exist for the duration of this function call.
            let context = WriteContext::new(self, write_options, log);

            // Set the thread context
            thread_context.set(Some(context.clone()));

            let result = context.write(instance);

            // Remove the thread context
            thread_context.set(None);

            result
        });

        // Get writing metadata
        let write_info = RwInfo::take_write_info();

        match result {
            Ok(path_buf) => return Ok((path_buf, write_info)),
            Err(err) => return Err(err),
        }
    }

    // ====================================================================
    // Deserialization

    /**
    Deserializes an instance of `T` stored within the file with the given `name`
    from the database and returns it.

    This function first derives the full file path name by concatenating
    `self.dir()`, the name of `T` (see [`type_name`]) and by combining `name`
    and `self.file_ext` to the file name. If this file exists, its content is
    then deserialized using [`Format::deserialize`] of `self.data_format()`.
    Any encountered links are resolved by reading the corresponding files and
    storing the resulting object within the created `T` instance.

    Like [`DatabaseManager::write`], using this function is mandatory in order
    to read files with links in them. Using serialization functions from other
    packages (as e.g. `serde_yaml::from_str`) bypasses the entire linking
    machinery of this crate and will result in failure if any links are stored
    within the files.
    */
    pub fn read<T: DatabaseEntry, O: AsRef<OsStr>>(&mut self, name: O) -> std::io::Result<T> {
        return self.read_verbose(name).map(|arg| arg.0);
    }

    /**
    Like [`DatabaseManager::read`], but returns additional [`ReadInfo`] in case
    reading from the database was successfull.

    The [`ReadInfo`] contains all [`ChecksumMismatch`]es which happened when a
    link contained a checksum which didn't match the linked file. If such a
    mismatch occurs, the file is still read and its contents are deserialized
    and replace the link regardless. Therefore, this information is useful to
    check if a linked file was changed since the creation of the link (e.g. in
    order to determine whether the returned instance of `T` should be used or
    not).
     */
    pub fn read_verbose<T: DatabaseEntry, O: AsRef<OsStr>>(
        &mut self,
        name: O,
    ) -> std::io::Result<(T, ReadInfo)> {
        return self.read_verbose_log(name, true);
    }

    fn read_verbose_log<T: DatabaseEntry, O: AsRef<OsStr>>(
        &mut self,
        name: O,
        log: bool,
    ) -> std::io::Result<(T, ReadInfo)> {
        let result = READ_CONTEXT.with(|thread_context| {
            // Context only exist for the duration of this function call.
            let context = ReadContext::new(self, log);

            // Set the thread context
            thread_context.set(Some(context.clone()));

            let result = context.read(name.as_ref());

            // Remove the thread context
            thread_context.set(None);

            result
        });

        // Get reading metadata
        let read_info = RwInfo::take_read_info();

        match result {
            Ok(instance) => return Ok((instance, read_info)),
            Err(err) => return Err(err),
        }
    }

    /**
    Deserializes the given string using [`Format::deserialize`] from
    `self.data_format()` and resolves any encountered links using the underlying
    database.

    This function behaves similarily to [`DatabaseManager::read`], except that
    the starting point is not a file from the database, but `str` instead.
     */
    pub fn from_str<T: DeserializeOwned + 'static, S: AsRef<str>>(
        &mut self,
        str: S,
    ) -> std::io::Result<T> {
        READ_CONTEXT.with(|thread_context| {
            // Context only exist for the duration of this function call.
            let context = ReadContext::new(self, false);

            // Set the thread context
            thread_context.set(Some(context.clone()));

            let dbm = unsafe { &mut *context.database_manager };

            let result = match dbm.format.deserialize(str.as_ref().as_bytes()) {
                Ok(val) => {
                    let val = val as Box<dyn Any>;
                    match val.downcast() {
                        Ok(val) => *val,
                        Err(_) => {
                            return Err(std::io::Error::new(
                                std::io::ErrorKind::InvalidData,
                                format!("type is not {}", type_name::<T>()),
                            ));
                        }
                    }
                }
                Err(msg) => Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    msg.to_string(),
                )),
            };
            // Remove the thread context
            thread_context.set(None);

            result
        })
    }
}

impl From<DatabaseManager> for Box<dyn Format> {
    fn from(value: DatabaseManager) -> Self {
        return value.format;
    }
}

impl From<DatabaseManager> for Cache {
    fn from(value: DatabaseManager) -> Self {
        return value.cache;
    }
}

// ========================================================================================================

#[derive(Clone, Copy)]
pub(crate) struct WriteContext {
    log: bool,
    pub(crate) database_manager: *mut DatabaseManager,
    pub(crate) write_options: *const WriteOptions,
}

thread_local!(pub(crate) static WRITE_CONTEXT: Cell<Option<WriteContext>> = Cell::new(None));

impl WriteContext {
    pub(crate) fn new(
        database_manager: &mut DatabaseManager,
        write_options: &WriteOptions,
        log: bool,
    ) -> Self {
        return Self {
            database_manager: std::ptr::from_mut(database_manager),
            write_options: std::ptr::from_ref(write_options),
            log,
        };
    }

    pub(crate) fn write<T: DatabaseEntry>(&self, instance: &T) -> std::io::Result<PathBuf> {
        // Enable / disable logging
        RwInfo::set_log(self.log);

        /*
        SAFETY: A WriteContext object is both created and destroyed within the function DatabaseManager::write_verbose.
        This function takes a mutable reference to a DatabaseManager. Therefore, the pointer is not dangling
        during the lifetime of the WriteContext. To avoid aliasing, we need to make sure that the mutable
        reference only exists AFTER serializing instance with self.data_format.to_string(instance), since this function
        could end up calling WriteContext::write again.

        The same is true for WriteOptions, but here we don't need to worry about aliasing.
         */
        let dbm = unsafe { &mut *self.database_manager }; // Casting from a *mut
        let write_options = unsafe { &*self.write_options }; // Casting from a *

        // Serialize self into a string. During the call of this function, no &mut
        // DatabaseManager must exist, since to_string could end up calling
        // Self::write, which would lead to aliasing mutable pointers.
        let data = dbm
            .format
            .serialize(instance)
            .map_err(|err| std::io::Error::new(ErrorKind::Other, err))?;

        let mut name = write_options.name(instance);
        if !dbm.file_ext().is_empty() {
            name.push(".");
            name.push(dbm.file_ext());
        }

        // If the folder for the file is missing, create it
        let folder_dir = dbm.dir().join(type_name::<T>());
        if !folder_dir.exists() {
            std::fs::create_dir_all(&folder_dir)?;
        }

        // Adjust the file name, if necessary
        let full_file_path = folder_dir.join(name);
        let file_exists = full_file_path.exists();

        let file_path = match write_options.name_collisions {
            NameCollisions::Overwrite => {
                if file_exists {
                    RwInfo::log_overwritten_file_path(full_file_path.clone());
                } else {
                    RwInfo::log_created_file_path(full_file_path.clone());
                }
                full_file_path
            }
            NameCollisions::KeepExisting => {
                // If the file already exists, do nothing
                if file_exists {
                    RwInfo::log_kept_file_path(full_file_path.clone());
                    return Ok(full_file_path);
                } else {
                    RwInfo::log_created_file_path(full_file_path.clone());
                    full_file_path
                }
            }
            NameCollisions::AdjustName => {
                // Check if a file `name` already exists within folder_dir. If
                // that is the case, find a new file name which isn't used yet.
                if file_exists {
                    let mut counter = 0;
                    let mut trial_file_path: PathBuf;
                    loop {
                        let mut name = write_options.name(instance);
                        name.push(&format!("_{}", counter));
                        if !dbm.file_ext().is_empty() {
                            name.push(".");
                            name.push(dbm.file_ext());
                        }
                        trial_file_path = folder_dir.join(name);
                        if !trial_file_path.exists() {
                            break;
                        }
                        counter += 1;
                    }
                    RwInfo::log_created_file_path(trial_file_path.clone());
                    trial_file_path
                } else {
                    RwInfo::log_created_file_path(full_file_path.clone());
                    full_file_path
                }
            }
        };

        // Create the corresponding file
        let mut file = File::create(&file_path).map_err(|err| {
            Error::new(
                err.kind(),
                format!("Could not create file {}", file_path.display()),
            )
        })?;

        // Store the serialized data in the file
        match file.write_all(&data) {
            Ok(_) => {
                return Ok(file_path);
            }
            Err(err) => {
                // Cleanup: Remove the file
                remove_file(&file_path)?;
                return Err(err);
            }
        };
    }
}

#[derive(Clone, Copy)]
pub(crate) struct ReadContext {
    log: bool,
    pub(crate) database_manager: *mut DatabaseManager,
}

thread_local!(pub(crate) static READ_CONTEXT: Cell<Option<ReadContext>> = Cell::new(None));

impl ReadContext {
    pub(crate) fn new(database_manager: &mut DatabaseManager, log: bool) -> Self {
        return Self {
            log,
            database_manager: std::ptr::from_mut(database_manager),
        };
    }

    pub(crate) fn read<T: DatabaseEntry>(&self, name: &OsStr) -> std::io::Result<T> {
        // Enable / disable logging
        RwInfo::set_log(self.log);

        /*
        SAFETY: A WriteContext object is both created and destroyed within the function DatabaseManager::read_verbose.
        This function takes a mutable reference to a DatabaseManager. Therefore, the pointer is not dangling
        during the lifetime of the WriteContext. To avoid aliasing, we need to make sure that the mutable
        reference does not exist anymore when calling self.data_format.from_reader(instance), since this function
        could end up calling WriteContext::read again.
         */
        let dbm = unsafe { &mut *self.database_manager };
        let file_path = dbm.full_path_unchecked((type_name::<T>(), name));

        if !file_path.exists() {
            return Err(Error::new(
                std::io::ErrorKind::NotFound,
                format!("Could not find file {}", file_path.display()),
            ));
        }

        // Reading from the cache failed => read directly from the file
        let data = fs::read(file_path.as_path())?;

        match dbm.format.deserialize(&data) {
            Ok(val) => {
                let val = val as Box<dyn Any>;
                match val.downcast::<T>() {
                    Ok(val) => Ok(*val),
                    Err(_) => {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidData,
                            format!("type is not {}", type_name::<T>()),
                        ));
                    }
                }
            }
            Err(err) => {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    err.to_string(),
                ));
            }
        }
    }
}

thread_local!(static RW_INFO: RefCell<RwInfo> = RefCell::new(RwInfo::default()));

#[derive(Default)]
pub(crate) struct RwInfo {
    log: bool,
    overwritten_files: Vec<PathBuf>,
    kept_files: Vec<PathBuf>,
    created_files: Vec<PathBuf>,
    checksum_mismatch: Vec<ChecksumMismatch>,
}

impl RwInfo {
    fn set_log(log: bool) {
        RW_INFO.with(|f| {
            let rw_info = &mut *f.borrow_mut();
            rw_info.log = log;
        });
    }

    fn take_write_info() -> WriteInfo {
        return RW_INFO.with(|f| {
            let rw_info = &mut *f.borrow_mut();
            return WriteInfo {
                overwritten_files: mem::replace(&mut rw_info.overwritten_files, Vec::new()),
                created_files: mem::replace(&mut rw_info.created_files, Vec::new()),
                kept_files: mem::replace(&mut rw_info.kept_files, Vec::new()),
            };
        });
    }

    fn take_read_info() -> ReadInfo {
        return RW_INFO.with(|f| {
            let rw_info = &mut *f.borrow_mut();
            return ReadInfo {
                checksum_mismatch: mem::replace(&mut rw_info.checksum_mismatch, Vec::new()),
            };
        });
    }

    fn log_overwritten_file_path(path: PathBuf) {
        RW_INFO.with(|f| {
            let mut borrowed = f.borrow_mut();
            if borrowed.log {
                borrowed.overwritten_files.push(path);
            }
        });
    }

    fn log_created_file_path(path: PathBuf) {
        RW_INFO.with(|f| {
            let mut borrowed = f.borrow_mut();
            if borrowed.log {
                borrowed.created_files.push(path);
            }
        });
    }

    fn log_kept_file_path(path: PathBuf) {
        RW_INFO.with(|f| {
            let mut borrowed = f.borrow_mut();
            if borrowed.log {
                borrowed.kept_files.push(path);
            }
        });
    }

    pub(crate) fn log_checksum_mismatch(val: ChecksumMismatch) {
        RW_INFO.with(|f| {
            let mut borrowed = f.borrow_mut();
            if borrowed.log {
                borrowed.checksum_mismatch.push(val);
            }
        });
    }
}

// Linked entries
// ======================================================

#[derive(DeserializeUntaggedVerboseError, Debug)]
pub(crate) enum LinkOrEntity<T> {
    DatabaseLink(DatabaseLink),
    Entity(T),
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DatabaseLink {
    pub name: String,
    #[serde(default)]
    pub checksum: Option<u32>,
}

impl DatabaseLink {
    pub(crate) fn new<T: DatabaseEntry>(instance: &T, checksum: Option<u32>) -> Self {
        DatabaseLink {
            name: instance.name().to_string_lossy().to_string(),
            checksum,
        }
    }

    /**
    A problem with links is the "silent" manipulation of files. Consider the following example:
    Struct A contains another struct of type B. Through the use of the annotation deserialize_link (or deserialize_arc_link),
    struct A is stored as two distinct files (one for B and one for A containing a link to B). Now the file containing B is
    changed (e.g. by changing some field value of B). Reading the file of A therefore does not result in the same struct
    which was serialized.

    To mitigate this problem, a link may store the checksum of the file containing B as an optional field.
    This optional field is always populated when serializing A with the DatabaseManager. When the checksum of the link
    does not equal the checksum of file B during deserialization, the checksum mismatch is documented in the ReadInfo
    struct which is returned by DatabaseManager::read_verbose. However, the deserialization itself does not fail even
    though the file of B has been changed (because the indirect change to A through the file of B might have been intentional).
     */
    pub(crate) fn test_for_checksum_mismatch(
        &self,
        file_path: PathBuf,
    ) -> Option<ChecksumMismatch> {
        let checksum_cached_in_link = self.checksum?;
        let checksum_loaded_file = checksum(file_path.as_path())?;
        return Some(ChecksumMismatch {
            checksum_cached_in_link,
            checksum_loaded_file,
            file_path,
        });
    }
}

/*
    Serialize the given instance into the database managed by self, using the specified link mode. Return the path to the resulting file.
    The file is saved with the file name returned by the `DatabaseEntry::name` method. If a file of the same name already exists, it is
    overwritten unless `overwrite` is set to false. In the latter case, `_x` is appended to the string returned by `DatabaseEntry::name`,
    where x is the first free number (no name collision).
*/

/**
Options to modify the behaviour of [`DatabaseManager::write`]. See the
individual fields for details.
 */
#[derive(Debug, Clone)]
pub struct WriteOptions {
    /**
    Specifies the behaviour when [`DatabaseManager::write`] attempts to write
    a file which already exists. See [`NameCollisions`] for more.

    Defaults to [`NameCollisions::KeepExisting`].
     */
    pub name_collisions: NameCollisions,
    /**
    Specifies the [`WriteMode`] when a link attribute is encountered. See
    [`WriteMode`] for more.

    Defaults to [`WriteMode::Link`].
     */
    pub write_mode: WriteMode,
    /**
    This map allows modifying the names of the written files. For example,
    if a file `pure_cotton` (+ file extension) should be written, but the map
    contains a key-value pair `pure_cotton: 100percent_cotton`, then a file
    `100percent_cotton` (+ file extension) will be written instead. Any links
    to this file which are created in other files also then link to the
    `100percent_cotton` file.

    Defaults to an empty [`HashMap`].
     */
    pub alias: HashMap<OsString, OsString>,
}

impl WriteOptions {
    fn name<T: DatabaseEntry>(&self, instance: &T) -> OsString {
        return self
            .alias
            .get(instance.name())
            .map(|string| string.as_os_str())
            .unwrap_or(instance.name())
            .to_os_string();
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            name_collisions: Default::default(),
            write_mode: Default::default(),
            alias: Default::default(),
        }
    }
}

/**
During the write process, [`DatabaseManager::write`] may attempt to overwrite
files which already exist. This enum specifies the behaviour in such a case.
*/
#[derive(Debug, Clone, Copy, Default)]
pub enum NameCollisions {
    /**
    Overwrite the existing file
     */
    Overwrite,
    #[default]
    /**
    Keep the existing file and link to it. No new file is created.
     */
    KeepExisting,
    /**
    Keep the existing file and create a new file with a modified name. If a link
    is being created, it links to the new file. The modification scheme is as
    follows:
    1) Append "_0" to the file name and check if that name is taken as well.
    2) If that is the case, add 1 to the number at the end and check if that
    name is also taken.
    3) Repeat 2) until an available name has been found, save the file then
    under the available name.
    For example, if set to false and attempting to write `pure_cotton` from
    the [`DatabaseManager`] docstring four times, the following files would be
    created:
    - `/path/to/db/Material/pure_cotton.yaml`
    - `/path/to/db/Material/pure_cotton_0.yaml`
    - `/path/to/db/Material/pure_cotton_1.yaml`
    - `/path/to/db/Material/pure_cotton_2.yaml`
     */
    AdjustName,
}

/**
Specifies the serialization behaviour when encountering a link during a
[`DatabaseManager::write`] call.
 */
#[derive(Debug, Clone, Copy, Default)]
pub enum WriteMode {
    /**
    Any links are ignored and the entire object is serialized into a single
    file. This is the same behaviour as if the object would have been serialized
    without using a [`DatabaseManager`] at all.
     */
    Flat,
    #[default]
    /**
    If a field with a "link" attribute is encountered, a separate database entry
    is created for it as described in [`DatabaseManager::write`].

    This is the default mode.
     */
    Link,
}

/**
This struct is returned by [`DatabaseManager::read_verbose`] and contains
information about the reading procedure within its fields.
 */
#[derive(Debug, Clone)]
pub struct ReadInfo {
    /**
    A vector of all [`ChecksumMismatch`]es which happened when reading a linked
    file. If the checksum listed within a link did not match that of the linked
    file, the file is still read, but the mismatch is stored within this vector
    for inspection. See the docstring of [`ChecksumMismatch`] for more.
     */
    pub checksum_mismatch: Vec<ChecksumMismatch>,
}

/**
This struct is returned by [`DatabaseManager::write_verbose`] and contains
information about the writing procedure within its fields.
 */
#[derive(Debug, Clone)]
pub struct WriteInfo {
    /**
    A list of all files which have been created anew during the call to
    [`DatabaseManager::write_verbose`].
     */
    pub created_files: Vec<PathBuf>,
    /**
    If the [`WriteOptions::name_collisions`] field is set to
    [`NameCollisions::KeepExisting`] and the database manager attempts to create
    a file which already exists, the old file is not overwritten and no new file
    is created. The paths of these files are listed within this field.
     */
    pub kept_files: Vec<PathBuf>,
    /**
    If the [`WriteOptions::name_collisions`] field is set to
    [`NameCollisions::Overwrite`] and the database manager attempts to create
    a file which already exists, the old file is overwritten. The paths of all
    overwritten files are listed within this field.
     */
    pub overwritten_files: Vec<PathBuf>,
}

/**
Information about a checksum mismatch.

A checksum is an [`u32`] integer derived from the contents of a file using
[`adler32::adler32`] (see also the [`checksum`] function). When deserializing
a link which contains a checksum and the contents of the linked file do not
match that checksum, a checksum mismatch occurs. The file is still deserialized
and the resulting type is used to replace the link. However, sometimes it might
be necessary to inspect the file in question. This struct holds the checksum
which was stored in the link, the checksum of the linked file contents and the
path to the linked file and is returned as part of [`ReadInfo`] when using
[`DatabaseManager::read_verbose`]. If the link does not contain a checksum
(usually the case for manually created links), a checksum mismatch cannot occur
by definition.
 */
#[derive(Debug, Clone)]
pub struct ChecksumMismatch {
    /**
    The checksum value stored in the link.
     */
    pub checksum_cached_in_link: u32,
    /**
    The checksum value of the file contents in [`ChecksumMismatch::file_path`].
     */
    pub checksum_loaded_file: u32,
    /**
    Path to the file where the mismatch occurred.
     */
    pub file_path: PathBuf,
}

/**
Calculates the checksum of the file contents at the given `path` using
[`adler32::adler32`].

This function can be used to determine the checksum of a file outside of this
crate (e.g. when a link is written manually). If there is no file at the given
`path`, [`None`] is returned.
 */
pub fn checksum(path: &Path) -> Option<u32> {
    let f = File::open(path).ok()?;
    let reader = BufReader::new(f);
    return adler32::adler32(reader).ok();
}
