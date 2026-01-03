use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs::{self, remove_file, File},
    io::{BufReader, Error, ErrorKind, Write},
    mem,
    path::{Path, PathBuf},
};

use serde::{de::DeserializeOwned, Deserialize, Serialize};
use deserialize_untagged_verbose_error::DeserializeUntaggedVerboseError;

use std::cell::{Cell, RefCell};

use crate::ArcWithFileChecksum;

/**
Trait which enables the storage of the implementing instance in a database which is controlled by a DatabaseManager.
 */
pub trait DatabaseEntry: Serialize + DeserializeOwned + Sized + std::fmt::Debug {
    /**
    Returns the name of the folder where instances of `Self` should be stored.
     */
    fn folder_name() -> &'static OsStr;

    /**
    Returns the name of the file (without extension) in which the serialized representation of `Self` should be stored.
     */
    fn file_name(&self) -> &OsStr;
}

/// Outer map key equals DatabaseEntry::folder_name(), inner map key equals DatabaseEntry::file_name(&self)
pub(crate) type ArcMap = HashMap<OsString, HashMap<OsString, ArcWithFileChecksum>>;

/**
TODO: Explain that DBM is an abbreviation for DatabaseManager.

All methods which manipulate files take a mutable reference to the DBM. This is done in order
to avoid race conditions when operating multi-threaded.
 */
#[derive(Clone)]
pub struct DatabaseManager {
    dir: PathBuf,
    format: DatabaseFormat,
    pub arc_map: ArcMap,
}

impl DatabaseManager {
    pub fn read_or_create<P>(path: P, format: DatabaseFormat) -> std::io::Result<Self>
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
        return Self::from_path(path, format);
    }

    pub fn from_path<P>(path: P, format: DatabaseFormat) -> std::io::Result<Self>
    where
        P: AsRef<Path>,
    {
        let mut dir = PathBuf::new();
        dir.push(path);

        if dir.exists() {
            return Ok(Self {
                dir,
                format,
                arc_map: Default::default(),
            });
        } else {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("Could not find directory {}", dir.display()),
            ));
        }
    }

    /**
    Return a reference to the underlaying path of the database manager.
     */
    pub fn dir(&self) -> &Path {
        return self.dir.as_path();
    }

    pub fn data_format(&self) -> DatabaseFormat {
        return self.format;
    }

    pub fn set_data_format(&mut self, format: DatabaseFormat) {
        self.format = format;
    }

    pub fn file_ext(&self) -> &OsStr {
        return self.format.file_ext();
    }

    pub fn file_checksum<T: AsRef<OsStr>, O: AsRef<OsStr>>(
        &self,
        folder_name: T,
        file_name: O,
    ) -> Option<u32> {
        return file_checksum(&self.full_path(folder_name, file_name));
    }

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
    Try to remove the specified file from the database.
     */
    pub fn remove_by_instance<E: DatabaseEntry>(&mut self, instance: &E) -> std::io::Result<()> {
        return self.remove_by_name(E::folder_name(), instance.file_name());
    }

    /**
    Try to remove the specified file from the database.
     */
    pub fn remove_by_name<T: AsRef<OsStr>, O: AsRef<OsStr>>(
        &mut self,
        folder_name: T,
        file_name: O,
    ) -> std::io::Result<()> {
        let file_path = self.full_path(folder_name, file_name);
        if file_path.exists() {
            return std::fs::remove_file(&file_path).map_err(|err| {
                Error::new(
                    err.kind(),
                    format!("Could not remove file {}", file_path.display()),
                )
            });
        } else {
            return Ok(());
        }
    }

    /**
    Search through all folders of the database and remove all files with the given file name(s)
     */
    pub fn remove_all<O: AsRef<OsStr>>(&mut self, file_name: O) -> std::io::Result<()> {
        fn remove_all_inner(dbm: &mut DatabaseManager, file_name: &OsStr) -> std::io::Result<()> {
            let mut file_with_ext = file_name.to_os_string();
            file_with_ext.push(".");
            file_with_ext.push(dbm.file_ext());

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
        return remove_all_inner(self, file_name.as_ref());
    }

    /**
    Check if the database has an entry with the given file name.
     */
    pub fn exists<T: AsRef<OsStr>, O: AsRef<OsStr>>(&self, folder_name: T, file_name: O) -> bool {
        return self.full_path(folder_name, file_name).exists();
    }

    /**
    Return the full path of the database entry, if it exists
     */
    pub fn full_path<T: AsRef<OsStr>, O: AsRef<OsStr>>(
        &self,
        folder_name: T,
        file_name: O,
    ) -> PathBuf {
        let mut file_with_ext = OsStr::new(&file_name).to_os_string();
        file_with_ext.push(".");
        file_with_ext.push(self.file_ext());
        return self
            .dir()
            .join(OsStr::new(&folder_name))
            .join(file_with_ext);
    }

    pub fn arc_map(&self) -> &ArcMap {
        return &self.arc_map;
    }

    pub fn arc_map_mut(&mut self) -> &mut ArcMap {
        return &mut self.arc_map;
    }

    // ====================================================================
    // Serialization

    /**
    Serialize the given instance into the database managed by self, using the specified link mode. Return the path to the resulting file.
    The file is saved with the file name returned by the `DatabaseEntry::file_name` method. If a file of the same name already exists, it is
    overwritten unless `overwrite` is set to false. In the latter case, `_x` is appended to the string returned by `DatabaseEntry::file_name`,
    where x is the first free number (no name collision).
    */
    pub fn write<E: DatabaseEntry>(
        &mut self,
        instance: &E,
        write_options: &WriteOptions,
    ) -> std::io::Result<PathBuf> {
        return self.write_verbose(instance, write_options).map(|arg| arg.0);
    }

    pub fn write_verbose<E: DatabaseEntry>(
        &mut self,
        instance: &E,
        write_options: &WriteOptions,
    ) -> std::io::Result<(PathBuf, WriteInfo)> {
        let result = WRITE_CONTEXT.with(|thread_context| {
            // Context only exist for the duration of this function call.
            let context = WriteContext::new(self, write_options);

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
    Try to read the instance defined by `file_name` from the database managed by self.
    */
    pub fn read<E: DatabaseEntry, O: AsRef<OsStr>>(&mut self, file_name: O) -> std::io::Result<E> {
        return self.read_verbose(file_name).map(|arg| arg.0);
    }

    pub fn read_verbose<E: DatabaseEntry, O: AsRef<OsStr>>(
        &mut self,
        file_name: O,
    ) -> std::io::Result<(E, ReadInfo)> {
        fn read_verbose_inner<E: DatabaseEntry>(
            dbm: &mut DatabaseManager,
            file_name: &OsStr,
        ) -> std::io::Result<(E, ReadInfo)> {
            let result = READ_CONTEXT.with(|thread_context| {
                // Context only exist for the duration of this function call.
                let context = ReadContext::new(dbm);

                // Set the thread context
                thread_context.set(Some(context.clone()));

                let result = context.read(file_name);

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
        return read_verbose_inner(self, file_name.as_ref());
    }

    /**
    Create an instance of type `E` from the string `data`. If `data` contains links, resolve them with `self`.
     */
    pub fn from_str<E: DeserializeOwned, S: AsRef<str>>(&mut self, str: S) -> std::io::Result<E> {
        fn from_str_inner<E: DeserializeOwned>(
            dbm: &mut DatabaseManager,
            str: &str,
        ) -> std::io::Result<E> {
            READ_CONTEXT.with(|thread_context| {
                // Context only exist for the duration of this function call.
                let context = ReadContext::new(dbm);

                // Set the thread context
                thread_context.set(Some(context.clone()));

                let result = match context.data_format.from_str(str) {
                    Ok(val) => Ok(val),
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
        return from_str_inner(self, str.as_ref());
    }
}

// ========================================================================================================

#[derive(Clone, Copy)]
pub(crate) struct WriteContext {
    pub(crate) database_manager: *mut DatabaseManager,
    pub(crate) write_options: *const WriteOptions,
    pub(crate) data_format: DatabaseFormat,
}

thread_local!(pub(crate) static WRITE_CONTEXT: Cell<Option<WriteContext>> = Cell::new(None));

impl WriteContext {
    pub(crate) fn new(
        database_manager: &mut DatabaseManager,
        write_options: &WriteOptions,
    ) -> Self {
        let data_format = database_manager.data_format();
        return Self {
            database_manager: std::ptr::from_mut(database_manager),
            write_options: std::ptr::from_ref(write_options),
            data_format,
        };
    }

    pub(crate) fn write<E: DatabaseEntry>(&self, instance: &E) -> std::io::Result<PathBuf> {
        // Serialize self into a string. During the call of this function, no &mut DatabaseManager must exist,
        // since to_string could end up calling Self::write, which would lead to aliasing mutable pointers.
        let data = self.data_format.to_string(instance)?;

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

        let mut file_name = write_options.file_name(instance);
        file_name.push(".");
        file_name.push(dbm.file_ext());

        // If the folder for the file is missing, create it
        let folder_dir = dbm.dir().join(E::folder_name());
        if !folder_dir.exists() {
            std::fs::create_dir_all(&folder_dir)?;
        }

        // Adjust the file name, if necessary
        let full_file_path = folder_dir.join(file_name);
        let file_exists = full_file_path.exists();

        let file_path = if write_options.overwrite {
            full_file_path
        } else {
            // Check if a file `file_name` already exists within folder_dir
            if file_exists {
                let mut counter = 0;
                let mut trial_file_path: PathBuf;
                loop {
                    let mut file_name = write_options.file_name(instance);
                    file_name.push(&format!("_{}", counter));
                    file_name.push(".");
                    file_name.push(dbm.file_ext());
                    trial_file_path = folder_dir.join(file_name);
                    if !trial_file_path.exists() {
                        break;
                    }
                    counter += 1;
                }
                trial_file_path
            } else {
                full_file_path
            }
        };

        // Log whether the file is newly created or whether it overrides an existing file
        if write_options.overwrite && file_exists {
            RwInfo::push_overwritten_file_path(file_path.clone());
        } else {
            // If we're not in overwrite mode, a new file is created anyway
            RwInfo::push_created_file_path(file_path.clone());
        }

        // Create the corresponding file
        let mut file = File::create(&file_path).map_err(|err| {
            Error::new(
                err.kind(),
                format!("Could not create file {}", file_path.display()),
            )
        })?;

        // Store the serialized data in the file
        match file.write_all(data.as_bytes()) {
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
    pub(crate) database_manager: *mut DatabaseManager,
    pub(crate) data_format: DatabaseFormat,
}

thread_local!(pub(crate) static READ_CONTEXT: Cell<Option<ReadContext>> = Cell::new(None));

impl ReadContext {
    pub(crate) fn new(database_manager: &mut DatabaseManager) -> Self {
        let data_format = database_manager.data_format();
        return Self {
            database_manager: std::ptr::from_mut(database_manager),
            data_format,
        };
    }

    pub(crate) fn read<E: DatabaseEntry>(&self, file_name: &OsStr) -> std::io::Result<E> {
        /*
        SAFETY: A WriteContext object is both created and destroyed within the function DatabaseManager::read_verbose.
        This function takes a mutable reference to a DatabaseManager. Therefore, the pointer is not dangling
        during the lifetime of the WriteContext. To avoid aliasing, we need to make sure that the mutable
        reference does not exist anymore when calling self.data_format.from_reader(instance), since this function
        could end up calling WriteContext::read again.
         */
        let file_path = {
            let dbm = unsafe { &mut *self.database_manager };
            dbm.full_path(E::folder_name(), file_name)
        };

        if !file_path.exists() {
            return Err(Error::new(
                std::io::ErrorKind::NotFound,
                format!("Could not find file {}", file_path.display()),
            ));
        }

        // Reading from the cache failed => read directly from the file
        let f = File::open(file_path.as_path())?;
        let reader = BufReader::new(f);
        let instance = self.data_format.from_reader(reader).map_err(|msg| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                format!(
                    "\nReading file {} failed:\n    {msg}\n",
                    file_path.as_os_str().to_string_lossy()
                ),
            )
        })?;
        return Ok(instance);
    }
}

thread_local!(static RW_INFO: RefCell<RwInfo> = RefCell::new(RwInfo::default()));

#[derive(Default)]
pub(crate) struct RwInfo {
    overwritten_files: Vec<PathBuf>,
    created_files: Vec<PathBuf>,
    checksum_mismatch: Vec<ChecksumMismatch>,
    replaced_arc_map_entry: Vec<ArcMapEntry>,
}

impl RwInfo {
    fn take_write_info() -> WriteInfo {
        return RW_INFO.with(|f| {
            let rw_info = &mut *f.borrow_mut();
            return WriteInfo {
                overwritten_files: mem::replace(&mut rw_info.overwritten_files, Vec::new()),
                created_files: mem::replace(&mut rw_info.created_files, Vec::new()),
            };
        });
    }

    fn take_read_info() -> ReadInfo {
        return RW_INFO.with(|f| {
            let rw_info = &mut *f.borrow_mut();
            return ReadInfo {
                checksum_mismatch: mem::replace(&mut rw_info.checksum_mismatch, Vec::new()),
                replaced_arc_map_entry: mem::replace(
                    &mut rw_info.replaced_arc_map_entry,
                    Vec::new(),
                ),
            };
        });
    }

    fn push_overwritten_file_path(path: PathBuf) {
        RW_INFO.with(|f| {
            f.borrow_mut().overwritten_files.push(path);
        });
    }

    fn push_created_file_path(path: PathBuf) {
        RW_INFO.with(|f| {
            f.borrow_mut().created_files.push(path);
        });
    }

    pub(crate) fn push_checksum_mismatch(val: ChecksumMismatch) {
        RW_INFO.with(|f| {
            f.borrow_mut().checksum_mismatch.push(val);
        });
    }

    pub(crate) fn push_replaced_arc_map_entry(val: ArcMapEntry) {
        RW_INFO.with(|f| {
            f.borrow_mut().replaced_arc_map_entry.push(val);
        });
    }
}

// Linked entries
// ======================================================

#[derive(DeserializeUntaggedVerboseError, Debug)]
pub enum LinkOrEntity<E> {
    DatabaseLink(DatabaseLink),
    Entity(E),
}

impl<E> LinkOrEntity<E> {
    pub fn unwrap(self) -> E {
        match self {
            LinkOrEntity::DatabaseLink(_) => {
                panic!("The link needs to be resolved before unwrapping.")
            }
            LinkOrEntity::Entity(instance) => instance,
        }
    }
}

#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DatabaseLink {
    pub file_name: String,
    #[serde(default)]
    pub file_checksum: Option<u32>,
}

impl DatabaseLink {
    pub fn new<T: DatabaseEntry>(instance: &T, file_checksum: Option<u32>) -> Self {
        DatabaseLink {
            file_name: instance.file_name().to_string_lossy().to_string(),
            file_checksum,
        }
    }

    /**
    A problem with links is the "silent" manipulation of files. Consider the following example:
    Struct A contains another struct of type B. Through the use of the annotation deserialize_dbm (or deserialize_arc_dbm),
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
        let checksum_cached_in_link = self.file_checksum?;
        let checksum_loaded_file = file_checksum(file_path.as_path())?;
        return Some(ChecksumMismatch {
            checksum_cached_in_link,
            checksum_loaded_file,
            file_path,
        });
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DatabaseFormat {
    #[cfg(feature = "json")]
    Json,
    #[cfg(feature = "yaml")]
    Yaml,
}

impl DatabaseFormat {
    pub fn file_ext(&self) -> &OsStr {
        match self {
            #[cfg(feature = "json")]
            DatabaseFormat::Json => OsStr::new("json"),
            #[cfg(feature = "yaml")]
            DatabaseFormat::Yaml => OsStr::new("yaml"),
        }
    }

    fn from_reader<E: DatabaseEntry>(&self, reader: BufReader<File>) -> Result<E, String> {
        match self {
            #[cfg(feature = "json")]
            DatabaseFormat::Json => {
                let _: E = match serde_json::from_reader(reader) {
                    Ok(val) => {
                        return Ok(val);
                    }
                    Err(msg) => {
                        // Manual conversion into an IO error, since the Error type of serde_yaml does not implement From<Error> for io::Error
                        return Err(msg.to_string());
                    }
                };
            }
            #[cfg(feature = "yaml")]
            DatabaseFormat::Yaml => {
                let _: E = match serde_yaml::from_reader(reader) {
                    Ok(val) => {
                        return Ok(val);
                    }
                    Err(msg) => {
                        // Manual conversion into an IO error, since the Error type of serde_yaml does not implement From<Error> for io::Error
                        return Err(msg.to_string());
                    }
                };
            }
        }
    }

    fn from_str<E: DeserializeOwned>(&self, str: &str) -> std::io::Result<E> {
        match self {
            #[cfg(feature = "json")]
            DatabaseFormat::Json => {
                let _: E = match serde_json::from_str(str) {
                    Ok(val) => {
                        return Ok(val);
                    }
                    Err(msg) => {
                        // Manual conversion into an IO error, since the Error type of serde_yaml does not implement From<Error> for io::Error
                        return Err(Error::new(ErrorKind::InvalidData, msg.to_string()));
                    }
                };
            }
            #[cfg(feature = "yaml")]
            DatabaseFormat::Yaml => {
                let _: E = match serde_yaml::from_str(str) {
                    Ok(val) => {
                        return Ok(val);
                    }
                    Err(msg) => {
                        // Manual conversion into an IO error, since the Error type of serde_yaml does not implement From<Error> for io::Error
                        return Err(Error::new(ErrorKind::InvalidData, msg.to_string()));
                    }
                };
            }
        }
    }

    fn to_string<E: DatabaseEntry>(&self, instance: &E) -> std::io::Result<String> {
        match self {
            #[cfg(feature = "json")]
            DatabaseFormat::Json => {
                return serde_json::to_string(instance).map_err(|msg| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string())
                })
            }
            #[cfg(feature = "yaml")]
            DatabaseFormat::Yaml => {
                return serde_yaml::to_string(instance).map_err(|msg| {
                    std::io::Error::new(std::io::ErrorKind::InvalidData, msg.to_string())
                })
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct WriteOptions {
    pub overwrite: bool,
    pub write_mode: WriteMode,
    pub alias: HashMap<OsString, OsString>,
}

impl WriteOptions {
    fn file_name<E: DatabaseEntry>(&self, instance: &E) -> OsString {
        return self
            .alias
            .get(instance.file_name())
            .map(|string| string.as_os_str())
            .unwrap_or(instance.file_name())
            .to_os_string();
    }
}

impl Default for WriteOptions {
    fn default() -> Self {
        Self {
            overwrite: Default::default(),
            write_mode: Default::default(),
            alias: Default::default(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ReadInfo {
    pub checksum_mismatch: Vec<ChecksumMismatch>,
    pub replaced_arc_map_entry: Vec<ArcMapEntry>,
}

#[derive(Debug, Clone)]
pub struct WriteInfo {
    pub overwritten_files: Vec<PathBuf>,
    pub created_files: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Default)]
pub enum WriteMode {
    Flat,
    #[default]
    Link,
}

#[derive(Debug, Clone)]
pub struct ChecksumMismatch {
    pub checksum_cached_in_link: u32,
    pub checksum_loaded_file: u32,
    pub file_path: PathBuf,
}

pub fn file_checksum(path: &Path) -> Option<u32> {
    let f = File::open(path).ok()?;
    let reader = BufReader::new(f);
    return adler32::adler32(reader).ok();
}

#[derive(Debug, Clone)]
pub struct ArcMapEntry {
    pub folder: OsString,
    pub file: OsString,
}
