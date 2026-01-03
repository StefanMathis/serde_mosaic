/*!
This module provides some simple deserialization converters for Serde:
* Writing `k*pi` with k being a floating-point value instead of using the exact value
* Specifying angles in degree rather than radians (`90 deg` instead of `1,570796326794896619`).

The converters are provided as functions which are compatible with the Serde deserialization function attribute
(`#[serde(deserialize_with = "function")]`). Simply annotate the field with the corresponding function when deriving `Deserialize`.
If `Deserialize` is implemented manually, the functions can be used in the manual implementation as well.
 */

use std::any::Any;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fmt;
use std::marker::PhantomData;
use std::sync::Arc;

use serde::de::{self, MapAccess};
use serde::ser;
use serde::{Deserialize, Serialize};

use crate::{ArcMap, ArcMapEntry, DatabaseEntry, DatabaseLink, LinkOrEntity, RwInfo, READ_CONTEXT, WRITE_CONTEXT};

pub fn serialize_dbm<E: DatabaseEntry, S: ser::Serializer>(
    instance: &E,
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
                            crate::file_checksum(file_path.as_path()),
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

pub fn serialize_opt_arc_dbm<E: DatabaseEntry, S: ser::Serializer>(
    instance: &Option<Arc<E>>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match instance {
        Some(inst) => return serialize_dbm(&**inst, serializer),
        None => return None::<Arc<E>>.serialize(serializer),
    }
}

pub fn serialize_arc_dbm<E: DatabaseEntry, S: ser::Serializer>(
    instance: &Arc<E>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    return serialize_dbm(&**instance, serializer);
}

pub fn deserialize_dbm<'de, D, E: DatabaseEntry>(deserializer: D) -> Result<E, D::Error>
where
    D: de::Deserializer<'de>,
{
    struct Visitor<E: DatabaseEntry> {
        phantom: PhantomData<E>,
    }

    impl<'de, E: DatabaseEntry> de::Visitor<'de> for Visitor<E> {
        type Value = E;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material or a DatabaseLink struct.")
        }

        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let link_or_instance: LinkOrEntity<E> =
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;

            let instance: E = match link_or_instance {
                LinkOrEntity::Entity(val) => val,
                LinkOrEntity::DatabaseLink(link) => {
                    // Read the deserialization context
                    let res: Result<E, std::io::Error>  = READ_CONTEXT.with(|thread_context| {
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
                                    dbm.full_path(E::folder_name(), &link.file_name)
                                };
                                if let Some(mismatch) = link.test_for_checksum_mismatch(file_path) {
                                    crate::RwInfo::push_checksum_mismatch(mismatch);
                                }

                                context.read(OsStr::new(&link.file_name))
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

pub fn deserialize_opt_arc_dbm<'de, D, E: DatabaseEntry + Send + Sync + 'static>(
    deserializer: D,
) -> Result<Option<Arc<E>>, D::Error>
where
    D: de::Deserializer<'de>,
{

    struct Visitor<E> {
        phantom: PhantomData<E>,
    }

    impl<'de, E: DatabaseEntry + Send + Sync + 'static> de::Visitor<'de> for Visitor<E> {
        type Value = Option<Arc<E>>;

        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material, a DatabaseLink or None.")
        }

        fn visit_some<D>(self, deserializer: D) -> Result<Self::Value, D::Error>
            where
                D: de::Deserializer<'de>, {
            let instance = deserialize_arc_dbm(deserializer)?;
            return Ok(Some(instance))
        }

        // We need to use F here as a generic for the error, because E is already taken
        fn visit_none<F>(self) -> Result<Self::Value, F>
            where
                F: de::Error, {
            return Ok(None);
        }
    }

    let deserialized_instance = deserializer.deserialize_option(Visitor {
        phantom: PhantomData,
    })?;

    return Ok(deserialized_instance);
}

pub fn deserialize_arc_dbm<'de, D, E: DatabaseEntry + Send + Sync + 'static>(
    deserializer: D,
) -> Result<Arc<E>, D::Error>
where
    D: de::Deserializer<'de>,
{

    fn read_arc_from_map<T: Send + Sync + DatabaseEntry + 'static>(
        arc_map: &mut ArcMap,
        link: &DatabaseLink,
    ) -> Option<Arc<T>> {
        match arc_map.get_mut(T::folder_name()) {
            Some(file_name_map) => {
    
                let mut remove_entry = false;
    
                // Check if the instance already exists as Arc in the arc_map. 
                let instance = file_name_map.get(OsStr::new(&link.file_name)).map(|checksum_arc| {
                    // If the checksum of checksum_arc is the same as the one of the link or no checksum exists in either the link or the
                    // pointer map, return the Arc. If both checksums exists but are not equal, delete the entry in the arc_map
                    // and deserialize the file directly.
                    let use_arc_instance = match checksum_arc.checksum {
                        Some(checksum_of_arc) => {
                            match link.file_checksum {
                                Some(checksum_of_file) => {
                                    checksum_of_arc == checksum_of_file
                                },
                                None => true,
                            }
                        },
                        None => true,
                    };
                    
                    if use_arc_instance {
                        checksum_arc.arc.clone().downcast::<T>().ok()
                    } else {    
                        remove_entry = true;
                        None
                    }  
                }).flatten();
    
                // An instance existed inside the map, but it failed the checksum test => Delete the map entry
                if remove_entry {
                    RwInfo::push_replaced_arc_map_entry(ArcMapEntry { folder: T::folder_name().to_os_string(), file: link.file_name.clone().into() });
                    let _ = file_name_map.remove(OsStr::new(&link.file_name));
                }
    
                return instance
            }
            None => return None,
        }
    }
    
    fn store_arc_in_map(
        arc_map: &mut ArcMap,
        link: &DatabaseLink,
        folder_name: &OsStr,
        instance: Arc<dyn Any + Send + Sync + 'static>,
    ) -> () {
        // Try to create the category hash map first (will fail if it exists already)
        if !arc_map.contains_key(folder_name) {
            arc_map.insert(folder_name.to_os_string(), HashMap::new());
        }
        let file_name_map = arc_map.get_mut(folder_name).unwrap(); // Must not fail since we just inserted the hash map in case it didn't exist yet.
        let checksum_arc = ArcWithFileChecksum { arc: instance, checksum: link.file_checksum };
        file_name_map.insert(link.file_name.clone().into(), checksum_arc);
        return;
    }
    
    struct VisitorArc<E> {
        phantom: PhantomData<E>,
    }
    
    impl<'de, E: DatabaseEntry + Send + Sync + 'static> de::Visitor<'de> for VisitorArc<E> {
        type Value = Arc<E>;
    
        fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
            formatter.write_str("either a Material or a DatabaseLink struct.")
        }
    
        fn visit_map<M>(self, visitor: M) -> Result<Self::Value, M::Error>
        where
            M: MapAccess<'de>,
        {
            let link_or_instance: LinkOrEntity<E> =
                Deserialize::deserialize(de::value::MapAccessDeserializer::new(visitor))?;
    
            let instance: Self::Value = match link_or_instance {
                LinkOrEntity::Entity(val) => Arc::new(val),
                LinkOrEntity::DatabaseLink(link) => {
                    // Read the deserialization context
                    let res: std::io::Result<Arc<E>> = READ_CONTEXT.with(|thread_context| {
                        match thread_context.get() {
                            Some(context) => {
                                /*
                                Check if the instance has already been deserialized by checking the arc_map
                                If yes, reuse the pointer. If no, read the instance from the database and store the pointer in the context
    
                                SAFETY: A ReadContext object is both created and destroyed within the function DatabaseManager::read_verbose.
                                This function takes a mutable reference to a DatabaseManager object. Therefore, the pointer is not dangling.
                                The only two places where a mutable reference is built from the pointer is in this function and in
                                ReadContext::read(). The lifetime of the references is chosen so that they do not alias.
                                */
                                if let Some(arc) = read_arc_from_map(&mut unsafe {&mut *context.database_manager}.arc_map, &link) {
                                    Ok(arc)
                                } else {
                                    // Since we arrived here, the instance is not stored in the pointer map => Perform a regular deserialization
                                    let instance: E = context.read(
                                        OsStr::new(&link.file_name),
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
                                        dbm.full_path(E::folder_name(), OsStr::new(&link.file_name))
                                    };
                                    if let Some(mismatch) = link.test_for_checksum_mismatch(file_path) {
                                        crate::RwInfo::push_checksum_mismatch(mismatch);
                                    }
    
                                    // Store the entry in the hash map
                                    let any_ptr = arc.clone() as Arc<dyn Any + Send + Sync + 'static>;
                                    store_arc_in_map(&mut unsafe {&mut *context.database_manager}.arc_map, &link, E::folder_name(), any_ptr);
    
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


pub fn deserialize_from_str<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::de::Deserializer<'de>,
    T: serde::de::DeserializeOwned + std::str::FromStr,
    <T as std::str::FromStr>::Err: std::fmt::Display,
{
    let string = String::deserialize(deserializer)?;
    return T::from_str(&string).map_err(serde::de::Error::custom);
}


#[derive(Debug, Clone)]
pub struct ArcWithFileChecksum {
    pub arc: Arc<dyn Any + Send + Sync + 'static>,
    pub checksum: Option<u32>,
}

impl ArcWithFileChecksum {
    pub fn new(arc: Arc<dyn Any + Send + Sync + 'static>) -> Self {
        return Self::from(arc);
    }
}

impl From<Arc<dyn Any + Send + Sync + 'static>> for ArcWithFileChecksum {
    fn from(value: Arc<dyn Any + Send + Sync + 'static>) -> Self {
        return Self { arc: value, checksum: None };
    }
}

impl From<ArcWithFileChecksum> for Arc<dyn Any + Send + Sync + 'static> {
    fn from(value: ArcWithFileChecksum) -> Self {
        return value.arc;
    }
}