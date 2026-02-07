/*!
This module contains various functions for integrating the database linking
functionality into the serialization / deserialization machinery of the
[`serde`] crate. 

These functions are primarily meant to be used together with the 
[`serialize_with`](https://serde.rs/field-attrs.html#serialize_with) and
[`deserialize_with`](https://serde.rs/field-attrs.html#deserialize_with)
attributes for the [`serde::Serialize`] and [`serde::Deserialize`] macros:

```
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
```

In order to use them, the field type needs to implement [`DatabaseEntry`].
When serializing and deserializing "directly" (e.g. with
`serde_yaml::to_string` provided by the
[`serde_yaml`](https://docs.rs/serde_yaml/latest/serde_yaml/) crate), these
functions are basically no-ops and behave as if the attribute wasn't there
(i.e., a "normal" serialization / deserialization of the field is performed).
However, if using [`DatabaseManager::write`](crate::DatabaseManager::write) the
a field annotated with one of the serialization functions is serialized
separatedly from its parent struct, leaving a "link" in the latter. Similarily,
when using [`DatabaseManager::read`](crate::DatabaseManager::read) and a "link"
is encountered during deserialization, the
[`DatabaseManager`](crate::DatabaseManager) attempts to read the field contents
from the linked file.

See the docstrings of [`serialize_link`] and [`deserialize_link`] for more. The
other functions within this module are basically variations of the former two
for optional and reference-counted fields.
 */

use std::any::{Any, TypeId};
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::{self, DeserializeOwned, MapAccess};
use serde::ser;
use serde::{Deserialize, Serialize};

use crate::{
    CacheEntry, Cache, DatabaseEntry, DatabaseLink, LinkOrEntity, READ_CONTEXT, WRITE_CONTEXT, type_name
};

/**
Serializes `instance` into a database if this function is called from
[`DatabaseManager::write`](crate::DatabaseManager::write) and returns a
serialized "link" (file name plus a checksum) pointing to the database entry. If
called from anywhere else, this function just performs "normal" serialization
of `instance` by forwarding to `instance.serialize(serializer)`.

This function is meant to be used with the
[`serialize_with`](https://serde.rs/field-attrs.html#serialize_with) attribute
from [serde]:

```
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

#[derive(Serialize, Deserialize)]
struct Shirt {
    owner: String,
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

When `Shirt` is serialized with
[`DatabaseManager::write`](crate::DatabaseManager::write), the `material` field
content is serialized separately and stored as a standalone entry in the
underlying database. The serialized representation of `Shirt` contains a "link"
to this entry which can be interpreted when deserializing via
[`DatabaseManager::read`](crate::DatabaseManager::read). See
[`deserialize_link`].

The link itself is the string returned by
[`DatabaseEntry::name(&instance)`](DatabaseEntry::name) and (optionally) a
checksum (hash) of the database entry. See the "Serialized representation"
section in README.md.
 */
pub fn serialize_link<T: DatabaseEntry + Serialize, S: ser::Serializer>(
    instance: &T,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    return WRITE_CONTEXT.with(|thread_context| {
        match thread_context.get() {
            Some(context) => {
                /*
                SAFETY: A WriteContext object is both created and destroyed within the function DatabaseManager::write_verbose.
                This function takes a reference to a WriteOptions object. Therefore, the pointer is not dangling.
                */
                let write_mode = {
                    let write_options = unsafe { &*context.write_options };
                    write_options.write_mode
                };

                match write_mode {
                    crate::WriteMode::Flat => return instance.serialize(serializer),
                    crate::WriteMode::Link => {
                        // Serialize the database entry itself
                        let file_path = match context.write(instance) {
                            Ok(file_path) => file_path,
                            Err(msg) => return Err(ser::Error::custom(msg)),
                        };

                        // Write link to the serializer
                        return DatabaseLink::new(
                            instance,
                            crate::checksum(file_path.as_path()),
                        )
                        .serialize(serializer);
                    }
                };
            }
            None => {
                // Serialize without a database manager
                return instance.serialize(serializer);
            }
        }
    });
}

/**
Like [`serialize_link`], but for an `Option<T>`. If the field is [`None`], no
separate database entry is created for `instance` and the link field in the
serialized representation of the parent struct is left empty.
 */
pub fn serialize_opt_link<T: DatabaseEntry + Serialize, S: ser::Serializer>(
    instance: &Option<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match instance {
        Some(inst) => return serialize_link(inst, serializer),
        None => return None::<T>.serialize(serializer),
    }
}

/**
Like [`serialize_link`], but for an `Arc<T>`. This function just forwards to
[`serialize_link`].
 */
pub fn serialize_arc_link<T: DatabaseEntry + Serialize, S: ser::Serializer>(
    instance: &Arc<T>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    return serialize_link(&**instance, serializer);
}

/**
Like [`serialize_opt_link`], but for an `Option<Arc<T>>`. This function just
forwards to [`serialize_link`] if `instance` is [`Some`], otherwise [`None`]
is serialized.
 */
pub fn serialize_opt_arc_link<T: DatabaseEntry + Serialize, S: ser::Serializer>(
    instance: &Option<Arc<T>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match instance {
        Some(inst) => return serialize_link(&**inst, serializer),
        None => return None::<Arc<T>>.serialize(serializer),
    }
}

/**
Deserializes `instance` from a database if this function is called from
[`DatabaseManager::read`](crate::DatabaseManager::read) and returns the
deserialized instance. If called from anywhere else, this function just performs
"normal" deserialization of `instance` by forwarding to
`deserializer.deserialize()`.

This function is meant to be used with the
[`deserialize_with`](https://serde.rs/field-attrs.html#deserialize_with)
attribute from [serde]:

```
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

#[derive(Serialize, Deserialize)]
struct Shirt {
    owner: String,
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
```

When `Shirt` is deserialized with
[`DatabaseManager::read`](crate::DatabaseManager::read) and the serialized
representation of the `material` field contains a "link", the actual
serialized representation of `material` is looked up in the database by
following the link. Then, `material` is deserialized separately and put into
the `Shirt` instance. If the `material` field of the serialized `Shirt`
representation already contains a serialized `Material` representation,
deserialization happens as usual and the database is not accessed.

See the "Serialized representation" section in README.md for more information
regarding the serialized representation of links.
 */
pub fn deserialize_link<'de, D, T: DatabaseEntry + DeserializeOwned>(
    deserializer: D,
) -> Result<T, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct Visitor<T: DatabaseEntry> {
        phantom: PhantomData<T>,
    }

    impl<'de, T: DatabaseEntry + DeserializeOwned> de::Visitor<'de> for Visitor<T> {
        type Value = T;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material or a DatabaseLink struct.")
        }

        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let link_or_instance: LinkOrEntity<T> =
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;

            let instance: T = match link_or_instance {
                LinkOrEntity::Entity(val) => {
                    val
                }
                LinkOrEntity::DatabaseLink(link) => {
                    // Read the deserialization context
                    let res: Result<T, std::io::Error>  = READ_CONTEXT.with(|thread_context| {
                        match thread_context.get() {
                            Some(context) => {
                                /*
                                If the link has a checksum, assert that the file is "in sync" with the link. See the documentation of 
                                DatabaseLink::test_for_checksum_mismatch for more information.

                                SAFETY: A ReadContext object is both created and destroyed within the function DatabaseManager::read_verbose.
                                This function takes a mutable reference to a DatabaseManager object. Therefore, the pointer is not dangling.
                                The only two places where a mutable reference is built from the pointer is in this function and in
                                ReadContext::read(). The lifetime of the references is chosen so that they do not alias.
                                */
                                let file_path = {
                                    let dbm = unsafe {&mut *context.database_manager};
                                    dbm.full_path_unchecked((type_name::<T>(), &link.name))
                                };
                                if let Some(mismatch) = link.test_for_checksum_mismatch(file_path) {
                                    crate::RwInfo::log_checksum_mismatch(mismatch);
                                }

                                context.read(OsStr::new(&link.name))
                            },
                            None => {
                                Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "No database manager has been set. Therefore, it is not possible to resolve links.".to_string(),
                                ))
                            }
                        }
                    });

                    match res {
                        Ok(val) => val,
                        Err(msg) => return Err(de::Error::custom(msg)),
                    }
                }
            };
            return Ok(instance);
        }
    }
    deserializer.deserialize_map(Visitor {
        phantom: PhantomData,
    })
}

/**
Like [`deserialize_link`], but for an `Option<T>`. If the "link" in the
serialized representation of `T` is empty (string is empty), `Option<T>` is
deserialized into [`None`]. Otherwise, [`deserialize_link`] is called.
 */
pub fn deserialize_opt_link<
    'de,
    D,
    T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned,
>(
    deserializer: D,
) -> Result<Option<T>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct Visitor<T> {
        phantom: PhantomData<T>,
    }

    impl<'de, T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned> de::Visitor<'de>
        for Visitor<T>
    {
        type Value = Option<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material, a DatabaseLink or None.")
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            let instance = deserialize_link(deserializer)?;
            return Ok(Some(instance));
        }

        // We need to use F here as a generic for the error, because E is already taken
        fn visit_none<F>(self) -> Result<Self::Value, F>
        where
            F: de::Error,
        {
            return Ok(None);
        }
    }

    let deserialized_instance = deserializer.deserialize_option(Visitor {
        phantom: PhantomData,
    })?;

    return Ok(deserialized_instance);
}

/**
Similar to [`deserialize_link`], but for an `Arc<T>`. This function first checks
[`DatabaseManager::cache`](crate::DatabaseManager::cache) to see if there is
already a deserialized [`Arc<T>`] with the same name and checksum as in the "link"
available. If so, that pointer is simply returned. Otherwise, the field is
deserialized like in [`deserialize_link`] and wrapped in an `Arc<T>`. This
pointer is then stored in the database manager cache and returned as well.

Using this mechanism, it is possible to share instances of components over
multiple composed structs. Since the cache can also be manipulated directly
via [`DatabaseManager::cache_mut`](crate::DatabaseManager::cache_mut), it is
even possible to store an `Arc<T>` there and then reuse it during
[`DatabaseManager::read`](crate::DatabaseManager::read) when deserializing a
composed struct where the `Arc<T>` field has the same name as the link.
See [`Cache`] for more.
 */
pub fn deserialize_arc_link<'de, D, T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned>(
    deserializer: D,
) -> Result<Arc<T>, D::Error>
where
    D: de::Deserializer<'de>,
{
    fn read_cache<T: Send + Sync + DatabaseEntry + 'static>(
        cache: &mut Cache,
        link: &DatabaseLink,
    ) -> Option<Arc<T>> {
        match cache.get_mut(&TypeId::of::<T>()) {
            Some(name_map) => {
                let mut remove_entry = false;

                // Check if the instance already exists as Arc in the cache.
                let instance = name_map
                    .get(OsStr::new(&link.name))
                    .map(|checksum_arc| {
                        // If the checksum of checksum_arc is the same as the one of the link or no checksum exists in either the link or the
                        // pointer map, return the Arc. If both checksums exists but are not equal, delete the entry in the cache
                        // and deserialize the file directly.
                        let use_arc_instance = match checksum_arc.checksum {
                            Some(checksum_of_arc) => match link.checksum {
                                Some(checksum_of_file) => checksum_of_arc == checksum_of_file,
                                None => true,
                            },
                            None => true,
                        };

                        if use_arc_instance {
                            let arc_any = checksum_arc.arc.clone() as Arc<dyn Any + Send +Sync>;
                            arc_any.downcast::<T>().ok()
                        } else {
                            remove_entry = true;
                            None
                        }
                    })
                    .flatten();

                // An instance existed inside the map, but it failed the checksum test => Delete the map entry
                if remove_entry {
                    let _ = name_map.remove(OsStr::new(&link.name));
                }

                return instance;
            }
            None => return None,
        }
    }

    fn write_cache<T: Send + Sync + DatabaseEntry + 'static>(
        cache: &mut Cache,
        link: &DatabaseLink,
        instance: Arc<dyn DatabaseEntry + Send + Sync + 'static>,
    ) -> () {
        // Try to create the category hash map first (will fail if it exists already)
        if !cache.contains_key(&TypeId::of::<T>()) {
            cache.insert(TypeId::of::<T>(), HashMap::new());
        }
        let name_map = cache.get_mut(&TypeId::of::<T>()).unwrap(); // Must not fail since we just inserted the hash map in case it didn't exist yet.
        let checksum_arc = CacheEntry {
            arc: instance,
            checksum: link.checksum,
        };
        name_map.insert(link.name.clone().into(), checksum_arc);
        return;
    }

    struct VisitorArc<T> {
        phantom: PhantomData<T>,
    }

    impl<'de, T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned> de::Visitor<'de>
        for VisitorArc<T>
    {
        type Value = Arc<T>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter
                .write_str("either a type implementing DatabaseEntry or a DatabaseLink struct.")
        }

        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let link_or_instance: LinkOrEntity<T> =
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;

            let instance: Self::Value = match link_or_instance {
                LinkOrEntity::Entity(val) => {
                    Arc::new(val)
                }
                LinkOrEntity::DatabaseLink(link) => {
                    // Read the deserialization context
                    let res: std::io::Result<Arc<T>> = READ_CONTEXT.with(|thread_context| {
                        match thread_context.get() {
                            Some(context) => {
                                /*
                                Check if the instance has already been deserialized by checking the cache
                                If yes, reuse the pointer. If no, read the instance from the database and store the pointer in the context
    
                                SAFETY: A ReadContext object is both created and destroyed within the function DatabaseManager::read_verbose.
                                This function takes a mutable reference to a DatabaseManager object. Therefore, the pointer is not dangling.
                                The only two places where a mutable reference is built from the pointer is in this function and in
                                ReadContext::read(). The lifetime of the references is chosen so that they do not alias.
                                */
                                if let Some(arc) = read_cache(&mut unsafe {&mut *context.database_manager}.cache_mut(), &link) {
                                    Ok(arc)
                                } else {
                                    // Since we arrived here, the instance is not stored in the pointer map => Perform a regular deserialization
                                    let instance: T = context.read(
                                        OsStr::new(&link.name),
                                    )?;
                                    let arc = Arc::new(instance);
    
                                    /*
                                    If the link has a checksum, assert that the file is "in sync" with the link. See the documentation of 
                                    DatabaseLink::test_for_checksum_mismatch for more information.
    
                                    SAFETY: A ReadContext object is both created and destroyed within the function DatabaseManager::read_verbose.
                                    This function takes a mutable reference to a DatabaseManager object. Therefore, the pointer is not dangling.
                                    The only two places where a mutable reference is built from the pointer is in this function and in
                                    ReadContext::read(). The lifetime of the references is chosen so that they do not alias.
                                    */
                                    let file_path = {
                                        let dbm = unsafe {&mut *context.database_manager};
                                        dbm.full_path_unchecked((type_name::<T>(), &link.name))
                                    };
                                    if let Some(mismatch) = link.test_for_checksum_mismatch(file_path) {
                                        crate::RwInfo::log_checksum_mismatch(mismatch);
                                    }
    
                                    // Store the entry in the hash map
                                    write_cache::<T>(&mut unsafe {&mut *context.database_manager}.cache_mut(), &link, arc.clone());
    
                                    // Return the pointer
                                    Ok(arc)
                                }                                
                            },
                            None => {
                                Err(std::io::Error::new(
                                    std::io::ErrorKind::Other,
                                    "No database manager has been set. Therefore, it is not possible to resolve links.".to_string(),
                                ))
                            }
                        }
                    });

                    match res {
                        Ok(val) => val,
                        Err(msg) => return Err(de::Error::custom(msg)),
                    }
                }
            };
            return Ok(instance);
        }
    }

    let deserialized_instance = deserializer.deserialize_map(VisitorArc {
        phantom: PhantomData,
    })?;

    return Ok(deserialized_instance);
}

/**
Like [`deserialize_arc_link`], but for `Option<Arc<T>>`. This function just
forwards to [`deserialize_arc_link`] if the link is not empty, otherwise
[`None`] is returned.
 */
pub fn deserialize_opt_arc_link<
    'de,
    D,
    T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned,
>(
    deserializer: D,
) -> Result<Option<Arc<T>>, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct Visitor<T> {
        phantom: PhantomData<T>,
    }

    impl<'de, T: DatabaseEntry + Send + Sync + 'static + DeserializeOwned> de::Visitor<'de>
        for Visitor<T>
    {
        type Value = Option<Arc<T>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material, a DatabaseLink or None.")
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
        where
            D: de::Deserializer<'de>,
        {
            let instance = deserialize_arc_link(deserializer)?;
            return Ok(Some(instance));
        }

        // We need to use F here as a generic for the error, because E is already taken
        fn visit_none<F>(self) -> Result<Self::Value, F>
        where
            F: de::Error,
        {
            return Ok(None);
        }
    }

    let deserialized_instance = deserializer.deserialize_option(Visitor {
        phantom: PhantomData,
    })?;

    return Ok(deserialized_instance);
}
