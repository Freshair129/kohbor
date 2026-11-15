use kohbor_account::{RecoveryMarker, SecretStore, StoreError};
use std::fs::{self, File, Metadata, OpenOptions};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, SocketAddrV4, TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

#[cfg(feature = "demo")]
#[path = "account_fake_native.rs"]
mod s1c_native_fake;

#[cfg(feature = "demo")]
pub(crate) use s1c_native_fake::{
    __cmd__account_fake_native, __tauri_command_name_account_fake_native, account_fake_native,
};

const MARKER_FILE: &str = "recovery-marker.bin";
const SESSION_FILE: &str = "account-session.bin";
const LOCK_FILE: &str = "account-storage.lock";
const MAGIC: &[u8; 8] = b"KOHBORM\0";
const SESSION_MAGIC: &[u8; 8] = b"KOHBORS\0";
const SESSION_TOMBSTONE: &[u8; 9] = b"KOHBORX\0\x01";
const FORMAT_VERSION: u8 = 1;
const MAX_ACCOUNT_ID_BYTES: usize = 128;
const MAX_MARKER_PLAINTEXT_BYTES: usize = 8 + 1 + 1 + 2 + MAX_ACCOUNT_ID_BYTES;
const MAX_PLAINTEXT_BYTES: usize = 256 * 1024;
const MAX_PROTECTED_BYTES: usize = 512 * 1024;
const MAX_SESSION_LIST_ITEMS: usize = 4096;
const MAX_TOKEN_BYTES: usize = 16 * 1024;
const MAX_ITEM_LABEL_BYTES: usize = 512;
const MAX_CALLBACK_HEADER_BYTES: usize = 8 * 1024;
const MAX_CALLBACK_HEADER_FIELDS: usize = 64;
const MAX_CALLBACK_LINE_BYTES: usize = 2 * 1024;
const MAX_CALLBACK_PARAMETER_BYTES: usize = 4 * 1024;
const MAX_BROWSER_SIGNIN_TIMEOUT: Duration = Duration::from_secs(5 * 60);
const CALLBACK_ACCEPT_POLL: Duration = Duration::from_millis(10);
const CALLBACK_READ_POLL: Duration = Duration::from_millis(25);
const CALLBACK_WRITE_TIMEOUT: Duration = Duration::from_millis(250);

/// File-backed storage for the durable account recovery marker.
///
/// This is only an atomic replacement for the marker file. The host that
/// eventually owns the complete secret store must serialize marker and
/// session operations together, including across concurrent clients. An
/// atomic rename of this one file does not provide that transaction.
#[cfg(test)]
pub struct RecoveryMarkerFile {
    root: PathBuf,
}

#[cfg(test)]
impl RecoveryMarkerFile {
    pub fn new(root: &Path) -> Self {
        Self {
            root: root.to_owned(),
        }
    }

    pub fn load(&self) -> Result<RecoveryMarker, StoreError> {
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) if is_plain_directory(&metadata) => {}
            Ok(_) => return Err(StoreError::Unreadable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RecoveryMarker::Clear)
            }
            Err(_) => return Err(StoreError::Unreadable),
        }

        let path = self.root.join(MARKER_FILE);
        let file = match open_marker_file(&path) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(RecoveryMarker::Clear)
            }
            Err(_) => return Err(StoreError::Unreadable),
        };

        let metadata = file.metadata().map_err(|_| StoreError::Unreadable)?;
        if !is_plain_file(&metadata) || metadata.len() > MAX_PROTECTED_BYTES as u64 {
            return Err(StoreError::Unreadable);
        }

        // Bound the actual read too, in case the file grows after metadata().
        let mut protected = Vec::with_capacity(metadata.len() as usize);
        file.take((MAX_PROTECTED_BYTES + 1) as u64)
            .read_to_end(&mut protected)
            .map_err(|_| StoreError::Unreadable)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(StoreError::Unreadable);
        }

        let plaintext = unprotect_current_user(&protected).ok_or(StoreError::Unreadable)?;
        decode_marker(&plaintext).ok_or(StoreError::Unreadable)
    }

    /// Persist one marker as a versioned DPAPI blob and atomically replace the
    /// previous file. This method does not serialize against session writes.
    pub fn commit(&self, marker: &RecoveryMarker) -> Result<(), StoreError> {
        match fs::symlink_metadata(&self.root) {
            Ok(metadata) if is_plain_directory(&metadata) => {}
            _ => return Err(StoreError::Unwritable),
        }

        let plaintext = encode_marker(marker).ok_or(StoreError::Unwritable)?;
        let protected = protect_current_user(&plaintext).ok_or(StoreError::Unwritable)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(StoreError::Unwritable);
        }

        let destination = self.root.join(MARKER_FILE);
        ensure_replaceable_destination(&destination).map_err(|_| StoreError::Unwritable)?;

        let (temporary_path, mut temporary_file) =
            create_unique_sibling(&self.root).map_err(|_| StoreError::Unwritable)?;
        let mut cleanup = OwnedTemporaryFile::new(temporary_path.clone());

        let write_result = temporary_file
            .write_all(&protected)
            .and_then(|()| temporary_file.sync_all());
        drop(temporary_file);
        write_result.map_err(|_| StoreError::Unwritable)?;

        replace_file_atomically(&temporary_path, &destination)
            .map_err(|_| StoreError::Unwritable)?;
        cleanup.disarm();
        Ok(())
    }
}

fn encode_marker(marker: &RecoveryMarker) -> Option<Vec<u8>> {
    let mut encoded = Vec::with_capacity(MAX_MARKER_PLAINTEXT_BYTES);
    encoded.extend_from_slice(MAGIC);
    encoded.push(FORMAT_VERSION);
    match marker {
        RecoveryMarker::Clear => encoded.push(0),
        RecoveryMarker::DeletePending { account_id } => {
            append_account_id(&mut encoded, 1, account_id)?;
        }
        RecoveryMarker::WipePending { account_id } => {
            append_account_id(&mut encoded, 2, account_id)?;
        }
    }
    Some(encoded)
}

fn append_account_id(encoded: &mut Vec<u8>, tag: u8, account_id: &str) -> Option<()> {
    let bytes = account_id.as_bytes();
    if !valid_account_id(bytes) {
        return None;
    }
    encoded.push(tag);
    encoded.extend_from_slice(&(bytes.len() as u16).to_le_bytes());
    encoded.extend_from_slice(bytes);
    Some(())
}

fn decode_marker(encoded: &[u8]) -> Option<RecoveryMarker> {
    if encoded.len() < 10 || encoded.len() > MAX_MARKER_PLAINTEXT_BYTES || &encoded[..8] != MAGIC {
        return None;
    }
    if encoded[8] != FORMAT_VERSION {
        return None;
    }

    match encoded[9] {
        0 if encoded.len() == 10 => Some(RecoveryMarker::Clear),
        tag @ (1 | 2) if encoded.len() >= 12 => {
            let id_len = u16::from_le_bytes([encoded[10], encoded[11]]) as usize;
            if id_len == 0 || id_len > MAX_ACCOUNT_ID_BYTES || encoded.len() != 12 + id_len {
                return None;
            }
            let bytes = &encoded[12..];
            if !valid_account_id(bytes) {
                return None;
            }
            let account_id = String::from_utf8(bytes.to_vec()).ok()?;
            match tag {
                1 => Some(RecoveryMarker::DeletePending { account_id }),
                2 => Some(RecoveryMarker::WipePending { account_id }),
                _ => None,
            }
        }
        _ => None,
    }
}

fn valid_account_id(bytes: &[u8]) -> bool {
    !bytes.is_empty()
        && bytes.len() <= MAX_ACCOUNT_ID_BYTES
        && bytes
            .iter()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'-' | b'_' | b'.'))
}

fn is_plain_directory(metadata: &Metadata) -> bool {
    if !metadata.is_dir() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn is_plain_file(metadata: &Metadata) -> bool {
    if !metadata.is_file() || metadata.file_type().is_symlink() {
        return false;
    }
    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0000_0400;
        metadata.file_attributes() & FILE_ATTRIBUTE_REPARSE_POINT == 0
    }
    #[cfg(not(windows))]
    {
        true
    }
}

fn ensure_replaceable_destination(path: &Path) -> std::io::Result<()> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if is_plain_file(&metadata) => Ok(()),
        Ok(_) => Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "marker destination is not a regular file",
        )),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(error),
    }
}

fn create_unique_sibling(root: &Path) -> std::io::Result<(PathBuf, File)> {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);

    for _ in 0..128 {
        let sequence = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
        let name = format!(".recovery-marker-{}-{sequence}.tmp", std::process::id());
        let path = root.join(name);
        match OpenOptions::new().write(true).create_new(true).open(&path) {
            Ok(file) => return Ok((path, file)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error),
        }
    }
    Err(std::io::Error::new(
        std::io::ErrorKind::AlreadyExists,
        "could not allocate a unique marker temporary file",
    ))
}

struct OwnedTemporaryFile {
    path: Option<PathBuf>,
}

impl OwnedTemporaryFile {
    fn new(path: PathBuf) -> Self {
        Self { path: Some(path) }
    }

    fn disarm(&mut self) {
        self.path = None;
    }
}

impl Drop for OwnedTemporaryFile {
    fn drop(&mut self) {
        if let Some(path) = self.path.take() {
            // This is the exact sibling created with create_new by this call;
            // never enumerate or remove another writer's temporary files.
            let _ = fs::remove_file(path);
        }
    }
}

#[cfg(windows)]
fn open_marker_file(path: &Path) -> std::io::Result<File> {
    use std::os::windows::fs::OpenOptionsExt;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    OpenOptions::new()
        .read(true)
        .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
        .open(path)
}

#[cfg(not(windows))]
fn open_marker_file(path: &Path) -> std::io::Result<File> {
    File::open(path)
}

#[cfg(windows)]
fn replace_file_atomically(source: &Path, destination: &Path) -> std::io::Result<()> {
    use std::os::windows::ffi::OsStrExt;

    const MOVEFILE_REPLACE_EXISTING: u32 = 0x0000_0001;
    const MOVEFILE_WRITE_THROUGH: u32 = 0x0000_0008;

    fn wide_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path contains an embedded NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn MoveFileExW(existing: *const u16, new: *const u16, flags: u32) -> i32;
        fn GetLastError() -> u32;
    }

    let source_wide = wide_path(source)?;
    let destination_wide = wide_path(destination)?;
    // Both names are siblings, so this is a same-volume rename. Omitting
    // MOVEFILE_COPY_ALLOWED makes any unexpected cross-volume case fail.
    let succeeded = unsafe {
        MoveFileExW(
            source_wide.as_ptr(),
            destination_wide.as_ptr(),
            MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH,
        )
    };
    if succeeded != 0 {
        Ok(())
    } else {
        let error_code = unsafe { GetLastError() } as i32;
        Err(std::io::Error::from_raw_os_error(error_code))
    }
}

#[cfg(not(windows))]
fn replace_file_atomically(_source: &Path, _destination: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "recovery marker storage requires Windows DPAPI",
    ))
}

#[cfg(windows)]
fn protect_current_user(plaintext: &[u8]) -> Option<Vec<u8>> {
    use std::ffi::c_void;
    use std::ptr;

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x0000_0001;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptProtectData(
            data_in: *const DataBlob,
            description: *const u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    struct LocalAllocation(*mut c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = LocalFree(self.0);
                }
            }
        }
    }

    if plaintext.is_empty() || plaintext.len() > MAX_PLAINTEXT_BYTES {
        return None;
    }
    let data_in = DataBlob {
        cb_data: u32::try_from(plaintext.len()).ok()?,
        pb_data: plaintext.as_ptr() as *mut u8,
    };
    let mut data_out = DataBlob {
        cb_data: 0,
        pb_data: ptr::null_mut(),
    };
    let succeeded = unsafe {
        CryptProtectData(
            &data_in,
            ptr::null(),
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut data_out,
        )
    };
    let allocation = LocalAllocation(data_out.pb_data.cast());
    if succeeded == 0
        || allocation.0.is_null()
        || data_out.cb_data == 0
        || data_out.cb_data as usize > MAX_PROTECTED_BYTES
    {
        return None;
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec() };
    Some(bytes)
}

#[cfg(not(windows))]
fn protect_current_user(_plaintext: &[u8]) -> Option<Vec<u8>> {
    None
}

#[cfg(windows)]
fn unprotect_current_user(protected: &[u8]) -> Option<Vec<u8>> {
    use std::ffi::c_void;
    use std::ptr;

    const CRYPTPROTECT_UI_FORBIDDEN: u32 = 0x0000_0001;

    #[repr(C)]
    struct DataBlob {
        cb_data: u32,
        pb_data: *mut u8,
    }

    #[link(name = "crypt32")]
    unsafe extern "system" {
        fn CryptUnprotectData(
            data_in: *const DataBlob,
            description: *mut *mut u16,
            optional_entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            data_out: *mut DataBlob,
        ) -> i32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    struct LocalAllocation(*mut c_void);
    impl Drop for LocalAllocation {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = LocalFree(self.0);
                }
            }
        }
    }

    if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
        return None;
    }
    let data_in = DataBlob {
        cb_data: u32::try_from(protected.len()).ok()?,
        pb_data: protected.as_ptr() as *mut u8,
    };
    let mut data_out = DataBlob {
        cb_data: 0,
        pb_data: ptr::null_mut(),
    };
    let mut description = ptr::null_mut();
    let succeeded = unsafe {
        CryptUnprotectData(
            &data_in,
            &mut description,
            ptr::null(),
            ptr::null_mut(),
            ptr::null_mut(),
            CRYPTPROTECT_UI_FORBIDDEN,
            &mut data_out,
        )
    };
    let data_allocation = LocalAllocation(data_out.pb_data.cast());
    let description_allocation = LocalAllocation(description.cast());
    if succeeded == 0
        || data_allocation.0.is_null()
        || data_out.cb_data == 0
        || data_out.cb_data as usize > MAX_PLAINTEXT_BYTES
    {
        return None;
    }
    let bytes =
        unsafe { std::slice::from_raw_parts(data_out.pb_data, data_out.cb_data as usize).to_vec() };
    drop(description_allocation);
    Some(bytes)
}

#[cfg(not(windows))]
fn unprotect_current_user(_protected: &[u8]) -> Option<Vec<u8>> {
    None
}

struct StorageRoot {
    path: PathBuf,
    // These real handles omit FILE_SHARE_DELETE from the volume root through
    // the leaf. They keep every path component from being renamed/replaced.
    _directory_handles: Vec<File>,
}

pub struct AccountStorage {
    inner: Arc<StorageRoot>,
}

pub struct AccountStorageGuard {
    inner: Arc<StorageRoot>,
    // Opening the stable lock file with share mode zero is the cross-process
    // try-lock. The handle also denies delete/rename for its entire lifetime.
    _lock: File,
    // This one-use permit exists only in the guard that durably created a new
    // DeletePending record. A fresh guard must use host adjudication instead.
    in_flight_delete: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeleteResolution {
    Unknown,
    Committed,
    NotCommitted,
}

impl AccountStorage {
    pub fn open(root: &Path) -> Result<Self, StoreError> {
        #[cfg(windows)]
        {
            let (root, components) = trusted_storage_path(root).map_err(|error| {
                trace_storage_open_error("trusted_path_or_drive", &error);
                StoreError::Unreadable
            })?;
            let directory_handles =
                win32::pin_directories(&root, &components).map_err(|error| {
                    trace_storage_open_error("directory_pinning", &error);
                    StoreError::Unreadable
                })?;
            Ok(Self {
                inner: Arc::new(StorageRoot {
                    path: root,
                    _directory_handles: directory_handles,
                }),
            })
        }
        #[cfg(not(windows))]
        {
            let _ = root;
            Err(StoreError::Unreadable)
        }
    }

    pub fn try_lock(&self) -> Result<AccountStorageGuard, StoreError> {
        #[cfg(windows)]
        {
            let lock = win32::open_lock(&self.inner.path.join(LOCK_FILE))
                .map_err(|_| StoreError::Unwritable)?;
            Ok(AccountStorageGuard {
                inner: Arc::clone(&self.inner),
                _lock: lock,
                in_flight_delete: None,
            })
        }
        #[cfg(not(windows))]
        {
            Err(StoreError::Unwritable)
        }
    }
}

#[cfg(test)]
fn trace_storage_open_error(stage: &str, error: &std::io::Error) {
    match error.raw_os_error() {
        Some(code) => eprintln!(
            "account_storage_open_failure stage={stage} kind={:?} win32={code}",
            error.kind()
        ),
        None => eprintln!(
            "account_storage_open_failure stage={stage} kind={:?} detail={error}",
            error.kind()
        ),
    }
}

#[cfg(not(test))]
fn trace_storage_open_error(_stage: &str, _error: &std::io::Error) {}

impl AccountStorageGuard {
    /// Apply only a trusted, host-side adjudication. This method never performs
    /// broker I/O and never retries an ambiguous delete. A persisted
    /// WipePending record is already committed authority for local erasure.
    /// Future broker integration must pass an authenticated resolution here,
    /// while holding this guard for the whole Client operation.
    pub fn resolve_pending_delete(
        &mut self,
        account_id: &str,
        resolution: DeleteResolution,
    ) -> Result<(), StoreError> {
        if !valid_account_id(account_id.as_bytes()) {
            return Err(StoreError::Unwritable);
        }
        let marker = self.read_marker()?;
        match marker {
            RecoveryMarker::DeletePending {
                account_id: pending,
            } if pending == account_id => match resolution {
                DeleteResolution::Unknown => {
                    self.in_flight_delete = None;
                    Ok(())
                }
                DeleteResolution::NotCommitted => {
                    self.require_saved_session(&pending)?;
                    self.write_marker_raw(&RecoveryMarker::Clear)?;
                    self.in_flight_delete = None;
                    Ok(())
                }
                DeleteResolution::Committed => {
                    self.require_saved_session(&pending)?;
                    self.write_marker_raw(&RecoveryMarker::WipePending {
                        account_id: pending.clone(),
                    })?;
                    self.wipe_account(&pending)?;
                    self.require_no_saved_session()?;
                    self.write_marker_raw(&RecoveryMarker::Clear)?;
                    self.in_flight_delete = None;
                    Ok(())
                }
            },
            RecoveryMarker::WipePending {
                account_id: pending,
            } if pending == account_id => match resolution {
                DeleteResolution::Unknown => {
                    self.in_flight_delete = None;
                    Ok(())
                }
                DeleteResolution::NotCommitted => Err(StoreError::Unwritable),
                DeleteResolution::Committed => {
                    if let Some(session) = self.read_session_file()? {
                        if session.account_id != pending {
                            return Err(StoreError::Unwritable);
                        }
                    }
                    self.wipe_account(&pending)?;
                    self.require_no_saved_session()?;
                    self.write_marker_raw(&RecoveryMarker::Clear)?;
                    self.in_flight_delete = None;
                    Ok(())
                }
            },
            RecoveryMarker::DeletePending { .. } | RecoveryMarker::WipePending { .. } => {
                Err(StoreError::Unwritable)
            }
            RecoveryMarker::Clear => Err(StoreError::Unwritable),
        }
    }

    fn read_marker(&self) -> Result<RecoveryMarker, StoreError> {
        match read_protected_file(&self.inner, MARKER_FILE, MAX_MARKER_PLAINTEXT_BYTES) {
            Ok(None) => Ok(RecoveryMarker::Clear),
            Ok(Some(plaintext)) => decode_marker(&plaintext).ok_or(StoreError::Unreadable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(RecoveryMarker::Clear),
            Err(_) => Err(StoreError::Unreadable),
        }
    }

    fn write_marker_raw(&self, marker: &RecoveryMarker) -> Result<(), StoreError> {
        let plaintext = encode_marker(marker).ok_or(StoreError::Unwritable)?;
        let protected = protect_current_user(&plaintext).ok_or(StoreError::Unwritable)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(StoreError::Unwritable);
        }
        commit_protected_file(&self.inner, MARKER_FILE, &protected)
            .map_err(|_| StoreError::Unwritable)
    }

    fn read_session_file(&self) -> Result<Option<kohbor_account::PersistedSession>, StoreError> {
        match read_protected_file(&self.inner, SESSION_FILE, MAX_PLAINTEXT_BYTES) {
            Ok(None) => Ok(None),
            Ok(Some(plaintext)) if plaintext.as_slice() == SESSION_TOMBSTONE => Ok(None),
            Ok(Some(plaintext)) => decode_session(&plaintext)
                .map(Some)
                .ok_or(StoreError::Unreadable),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(_) => Err(StoreError::Unreadable),
        }
    }

    // The bounded protected bytes are the revision: tombstones and re-encryption
    // can change even when the logical session remains absent or identical.
    fn read_session_file_revision(&self) -> Result<Option<Vec<u8>>, StoreError> {
        match read_protected_file_with_ciphertext(&self.inner, SESSION_FILE, MAX_PLAINTEXT_BYTES) {
            Ok(None) => Ok(None),
            Ok(Some((plaintext, protected)))
                if plaintext.as_slice() == SESSION_TOMBSTONE
                    || decode_session(&plaintext).is_some() =>
            {
                Ok(Some(protected))
            }
            Ok(Some(_)) | Err(_) => Err(StoreError::Unreadable),
        }
    }

    fn require_saved_session(&self, account_id: &str) -> Result<(), StoreError> {
        match self.read_session_file()? {
            Some(session) if session.account_id == account_id => Ok(()),
            _ => Err(StoreError::Unwritable),
        }
    }

    fn require_no_saved_session(&self) -> Result<(), StoreError> {
        if self.read_session_file()?.is_none() {
            Ok(())
        } else {
            Err(StoreError::Unwritable)
        }
    }

    fn write_session_file(
        &self,
        session: &kohbor_account::PersistedSession,
    ) -> Result<(), StoreError> {
        let plaintext = encode_session(session).ok_or(StoreError::Unwritable)?;
        let protected = protect_current_user(&plaintext).ok_or(StoreError::Unwritable)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(StoreError::Unwritable);
        }
        commit_protected_file(&self.inner, SESSION_FILE, &protected)
            .map_err(|_| StoreError::Unwritable)
    }

    fn write_session_tombstone(&self) -> Result<(), StoreError> {
        let protected = protect_current_user(SESSION_TOMBSTONE).ok_or(StoreError::Unwritable)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(StoreError::Unwritable);
        }
        commit_protected_file(&self.inner, SESSION_FILE, &protected)
            .map_err(|_| StoreError::Unwritable)
    }
}

impl kohbor_account::SecretStore for AccountStorageGuard {
    fn load_recovery_marker(&self) -> Result<RecoveryMarker, StoreError> {
        self.read_marker()
    }

    fn write_recovery_marker(&mut self, marker: &RecoveryMarker) -> Result<(), StoreError> {
        let current = self.read_marker()?;
        let mut new_delete_authority = None;
        let mut consume_delete_authority = false;
        let allowed = match (&current, marker) {
            (RecoveryMarker::Clear, RecoveryMarker::Clear) => true,
            (RecoveryMarker::Clear, RecoveryMarker::DeletePending { account_id }) => {
                let matches = valid_account_id(account_id.as_bytes())
                    && self
                        .read_session_file()?
                        .is_some_and(|session| session.account_id == *account_id);
                if matches {
                    new_delete_authority = Some(account_id.clone());
                }
                matches
            }
            (RecoveryMarker::Clear, RecoveryMarker::WipePending { account_id }) => {
                valid_account_id(account_id.as_bytes())
                    && self
                        .read_session_file()?
                        .is_some_and(|session| session.account_id == *account_id)
            }
            (
                RecoveryMarker::DeletePending {
                    account_id: current,
                },
                RecoveryMarker::WipePending { account_id: next },
            ) => {
                let authorized =
                    self.in_flight_delete.as_deref() == Some(current.as_str()) && current == next;
                if authorized {
                    self.require_saved_session(current)?;
                    consume_delete_authority = true;
                }
                authorized
            }
            (
                RecoveryMarker::DeletePending {
                    account_id: current,
                },
                RecoveryMarker::Clear,
            ) => {
                let authorized = self.in_flight_delete.as_deref() == Some(current.as_str());
                if authorized {
                    self.require_saved_session(current)?;
                    consume_delete_authority = true;
                }
                authorized
            }
            (
                RecoveryMarker::WipePending {
                    account_id: current,
                },
                RecoveryMarker::WipePending { account_id: next },
            ) => current == next,
            (RecoveryMarker::WipePending { .. }, RecoveryMarker::Clear) => {
                self.require_no_saved_session()?;
                true
            }
            _ => false,
        };
        if !allowed {
            return Err(StoreError::Unwritable);
        }
        self.write_marker_raw(marker)?;
        if let Some(account_id) = new_delete_authority {
            self.in_flight_delete = Some(account_id);
        }
        if consume_delete_authority {
            self.in_flight_delete = None;
        }
        Ok(())
    }

    fn load_session(&self) -> Result<Option<kohbor_account::PersistedSession>, StoreError> {
        match self.read_marker()? {
            RecoveryMarker::Clear => self.read_session_file(),
            RecoveryMarker::DeletePending { .. } | RecoveryMarker::WipePending { .. } => Ok(None),
        }
    }

    fn write_session(
        &mut self,
        session: &kohbor_account::PersistedSession,
    ) -> Result<(), StoreError> {
        if !matches!(self.read_marker()?, RecoveryMarker::Clear)
            || encode_session(session).is_none()
        {
            return Err(StoreError::Unwritable);
        }
        if let Some(existing) = self.read_session_file()? {
            if existing.account_id != session.account_id {
                return Err(StoreError::Unwritable);
            }
        }
        self.write_session_file(session)
    }

    fn write_friends(&mut self, account_id: &str, friends: &[String]) -> Result<(), StoreError> {
        if !valid_account_id(account_id.as_bytes())
            || !matches!(self.read_marker()?, RecoveryMarker::Clear)
        {
            return Err(StoreError::Unwritable);
        }
        let mut session = self.read_session_file()?.ok_or(StoreError::Unwritable)?;
        if session.account_id != account_id || !valid_id_list(friends) {
            return Err(StoreError::Unwritable);
        }
        session.friends = friends.to_vec();
        self.write_session_file(&session)
    }

    fn wipe_account(&mut self, account_id: &str) -> Result<(), StoreError> {
        if !valid_account_id(account_id.as_bytes()) {
            return Err(StoreError::Unwritable);
        }
        match self.read_marker()? {
            RecoveryMarker::WipePending {
                account_id: pending,
            } if pending == account_id => {}
            RecoveryMarker::Clear => {
                if self.read_session_file()?.is_some() {
                    return Err(StoreError::Unwritable);
                }
                return Ok(());
            }
            _ => return Err(StoreError::Unwritable),
        }

        let Some(session) = self.read_session_file()? else {
            return Ok(());
        };
        if session.account_id != account_id {
            return Err(StoreError::Unwritable);
        }
        // A DPAPI-protected tombstone is atomically replaced with write-through
        // before WipePending can be cleared. A crash leaves either the old
        // protected session plus WipePending, or the durable tombstone.
        self.write_session_tombstone()?;
        if self.read_session_file()?.is_some() {
            return Err(StoreError::Unwritable);
        }
        Ok(())
    }
}

fn valid_id_list(values: &[String]) -> bool {
    values.len() <= MAX_SESSION_LIST_ITEMS
        && values
            .iter()
            .all(|value| valid_account_id(value.as_bytes()))
}

fn valid_secret_text(value: &str, maximum: usize) -> bool {
    !value.is_empty()
        && value.len() <= maximum
        && value.chars().all(|character| !character.is_control())
}

fn valid_item_labels(values: &[String]) -> bool {
    values.len() <= MAX_SESSION_LIST_ITEMS
        && values.iter().all(|value| {
            !value.is_empty()
                && value.len() <= MAX_ITEM_LABEL_BYTES
                && value.chars().all(|character| !character.is_control())
        })
}

fn encode_session(session: &kohbor_account::PersistedSession) -> Option<Vec<u8>> {
    if !valid_account_id(session.account_id.as_bytes())
        || !valid_secret_text(&session.refresh_token, MAX_TOKEN_BYTES)
        || !valid_secret_text(&session.device_key, MAX_TOKEN_BYTES)
        || !valid_id_list(&session.friends)
        || !valid_id_list(&session.pairing_ids)
        || !valid_item_labels(&session.private_item_labels)
    {
        return None;
    }

    let mut encoded = Vec::with_capacity(512);
    encoded.extend_from_slice(SESSION_MAGIC);
    encoded.push(FORMAT_VERSION);
    append_session_text(&mut encoded, &session.account_id, MAX_ACCOUNT_ID_BYTES)?;
    append_session_text(&mut encoded, &session.refresh_token, MAX_TOKEN_BYTES)?;
    append_session_text(&mut encoded, &session.device_key, MAX_TOKEN_BYTES)?;
    append_session_list(
        &mut encoded,
        &session.friends,
        MAX_SESSION_LIST_ITEMS,
        MAX_ACCOUNT_ID_BYTES,
    )?;
    append_session_list(
        &mut encoded,
        &session.pairing_ids,
        MAX_SESSION_LIST_ITEMS,
        MAX_ACCOUNT_ID_BYTES,
    )?;
    append_session_list(
        &mut encoded,
        &session.private_item_labels,
        MAX_SESSION_LIST_ITEMS,
        MAX_ITEM_LABEL_BYTES,
    )?;
    (encoded.len() <= MAX_PLAINTEXT_BYTES).then_some(encoded)
}

fn append_session_text(encoded: &mut Vec<u8>, value: &str, maximum: usize) -> Option<()> {
    let bytes = value.as_bytes();
    if bytes.is_empty() || bytes.len() > maximum {
        return None;
    }
    encoded.extend_from_slice(&u32::try_from(bytes.len()).ok()?.to_le_bytes());
    encoded.extend_from_slice(bytes);
    Some(())
}

fn append_session_list(
    encoded: &mut Vec<u8>,
    values: &[String],
    maximum_count: usize,
    maximum_item_bytes: usize,
) -> Option<()> {
    if values.len() > maximum_count {
        return None;
    }
    encoded.extend_from_slice(&u16::try_from(values.len()).ok()?.to_le_bytes());
    for value in values {
        append_session_text(encoded, value, maximum_item_bytes)?;
        if encoded.len() > MAX_PLAINTEXT_BYTES {
            return None;
        }
    }
    Some(())
}

fn decode_session(encoded: &[u8]) -> Option<kohbor_account::PersistedSession> {
    if encoded.len() < SESSION_MAGIC.len() + 1
        || encoded.len() > MAX_PLAINTEXT_BYTES
        || &encoded[..SESSION_MAGIC.len()] != SESSION_MAGIC
        || encoded[SESSION_MAGIC.len()] != FORMAT_VERSION
    {
        return None;
    }
    let mut offset = SESSION_MAGIC.len() + 1;
    let account_id = read_session_text(encoded, &mut offset, MAX_ACCOUNT_ID_BYTES)?;
    let refresh_token = read_session_text(encoded, &mut offset, MAX_TOKEN_BYTES)?;
    let device_key = read_session_text(encoded, &mut offset, MAX_TOKEN_BYTES)?;
    let friends = read_session_list(
        encoded,
        &mut offset,
        MAX_SESSION_LIST_ITEMS,
        MAX_ACCOUNT_ID_BYTES,
    )?;
    let pairing_ids = read_session_list(
        encoded,
        &mut offset,
        MAX_SESSION_LIST_ITEMS,
        MAX_ACCOUNT_ID_BYTES,
    )?;
    let private_item_labels = read_session_list(
        encoded,
        &mut offset,
        MAX_SESSION_LIST_ITEMS,
        MAX_ITEM_LABEL_BYTES,
    )?;
    if offset != encoded.len() {
        return None;
    }
    let session = kohbor_account::PersistedSession {
        account_id,
        refresh_token,
        device_key,
        friends,
        pairing_ids,
        private_item_labels,
    };
    encode_session(&session).map(|_| session)
}

fn read_session_text(encoded: &[u8], offset: &mut usize, maximum: usize) -> Option<String> {
    let length_bytes = encoded.get(*offset..offset.checked_add(4)?)?;
    let length = u32::from_le_bytes(length_bytes.try_into().ok()?) as usize;
    *offset += 4;
    if length == 0 || length > maximum {
        return None;
    }
    let bytes = encoded.get(*offset..offset.checked_add(length)?)?;
    *offset += length;
    String::from_utf8(bytes.to_vec()).ok()
}

fn read_session_list(
    encoded: &[u8],
    offset: &mut usize,
    maximum_count: usize,
    maximum_item_bytes: usize,
) -> Option<Vec<String>> {
    let count_bytes = encoded.get(*offset..offset.checked_add(2)?)?;
    let count = u16::from_le_bytes(count_bytes.try_into().ok()?) as usize;
    *offset += 2;
    if count > maximum_count {
        return None;
    }
    let mut values = Vec::with_capacity(count);
    for _ in 0..count {
        values.push(read_session_text(encoded, offset, maximum_item_bytes)?);
    }
    Some(values)
}

#[cfg(windows)]
fn trusted_storage_path(path: &Path) -> std::io::Result<(PathBuf, Vec<std::ffi::OsString>)> {
    use std::os::windows::ffi::{OsStrExt, OsStringExt};
    let wide: Vec<u16> = path.as_os_str().encode_wide().collect();
    if wide.len() < 4
        || wide[0] > 0x7f
        || !(wide[0] as u8).is_ascii_alphabetic()
        || wide[1] != b':' as u16
        || wide[2] != b'\\' as u16
        || wide.contains(&0)
        || wide.contains(&(b'/' as u16))
    {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "storage root must be a local absolute drive path",
        ));
    }
    let mut components = Vec::new();
    for component in wide[3..].split(|unit| *unit == b'\\' as u16) {
        if component.is_empty()
            || component == [b'.' as u16]
            || component == [b'.' as u16, b'.' as u16]
            || component.len() > 255
            || component
                .iter()
                .any(|unit| *unit < 0x20 || *unit == b':' as u16)
            || component
                .last()
                .is_some_and(|unit| *unit == b'.' as u16 || *unit == b' ' as u16)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "invalid storage root component",
            ));
        }
        components.push(std::ffi::OsString::from_wide(component));
    }
    if components.is_empty() || win32::drive_type(&wide[..3])? != win32::DRIVE_FIXED {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "storage root must be on a fixed local disk",
        ));
    }
    let mut normalized = std::ffi::OsString::from_wide(&wide);
    // Accept one trailing separator only by rejecting it above. Preserve the
    // caller's Unicode path spelling; each component is pinned separately.
    let path = PathBuf::from(std::mem::take(&mut normalized));
    Ok((path, components))
}

#[cfg(not(windows))]
fn trusted_storage_path(_path: &Path) -> std::io::Result<(PathBuf, Vec<std::ffi::OsString>)> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Windows local storage required",
    ))
}

fn read_protected_file(
    root: &StorageRoot,
    name: &str,
    maximum_plaintext: usize,
) -> std::io::Result<Option<Vec<u8>>> {
    read_protected_file_with_ciphertext(root, name, maximum_plaintext)
        .map(|file| file.map(|(plaintext, _protected)| plaintext))
}

fn read_protected_file_with_ciphertext(
    root: &StorageRoot,
    name: &str,
    maximum_plaintext: usize,
) -> std::io::Result<Option<(Vec<u8>, Vec<u8>)>> {
    #[cfg(windows)]
    {
        let file = match win32::open_regular_file(
            &root.path.join(name),
            win32::GENERIC_READ,
            win32::FILE_SHARE_READ,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        let length = file.metadata()?.len();
        if length == 0 || length > MAX_PROTECTED_BYTES as u64 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected file size is invalid",
            ));
        }
        let mut protected = Vec::with_capacity(length as usize);
        file.take((MAX_PROTECTED_BYTES + 1) as u64)
            .read_to_end(&mut protected)?;
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "protected file size is invalid",
            ));
        }
        let plaintext = unprotect_current_user(&protected)
            .filter(|bytes| !bytes.is_empty() && bytes.len() <= maximum_plaintext)
            .ok_or_else(|| {
                std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "protected file is unreadable",
                )
            })?;
        Ok(Some((plaintext, protected)))
    }
    #[cfg(not(windows))]
    {
        let _ = (root, name, maximum_plaintext);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows DPAPI storage required",
        ))
    }
}

fn commit_protected_file(root: &StorageRoot, name: &str, protected: &[u8]) -> std::io::Result<()> {
    #[cfg(windows)]
    {
        if protected.is_empty() || protected.len() > MAX_PROTECTED_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "protected file size is invalid",
            ));
        }
        let destination = root.path.join(name);
        win32::ensure_replaceable_file(&destination)?;
        let (temporary_path, mut temporary_file, identity) = win32::create_temporary(&root.path)?;
        let mut cleanup = win32::OwnedStorageTemporary::new(temporary_path.clone(), identity);
        temporary_file.write_all(protected)?;
        temporary_file.sync_all()?;
        drop(temporary_file);
        win32::ensure_same_plain_file(&temporary_path, identity)?;
        win32::ensure_replaceable_file(&destination)?;
        replace_file_atomically(&temporary_path, &destination)?;
        cleanup.disarm();
        win32::ensure_replaceable_file(&destination)?;
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = (root, name, protected);
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "Windows DPAPI storage required",
        ))
    }
}

#[cfg(windows)]
mod win32 {
    use super::*;
    use std::ffi::c_void;
    use std::os::windows::{
        ffi::{OsStrExt, OsStringExt},
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle},
    };
    use std::ptr;

    pub const DRIVE_FIXED: u32 = 3;
    pub const GENERIC_READ: u32 = 0x8000_0000;
    const DELETE: u32 = 0x0001_0000;
    const READ_CONTROL: u32 = 0x0002_0000;
    const FILE_READ_ATTRIBUTES: u32 = 0x0080;
    pub(super) const FILE_SHARE_READ: u32 = 0x0000_0001;
    const FILE_SHARE_WRITE: u32 = 0x0000_0002;
    const OPEN_EXISTING: u32 = 3;
    const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
    const FILE_FLAG_OPEN_REPARSE_POINT: u32 = 0x0020_0000;
    const FILE_ATTRIBUTE_DIRECTORY: u32 = 0x0010;
    const FILE_ATTRIBUTE_REPARSE_POINT: u32 = 0x0400;
    const FILE_ATTRIBUTE_DEVICE: u32 = 0x0040;
    const FILE_ALL_ACCESS: u32 = 0x001f_01ff;
    const OBJECT_INHERIT_ACE: u32 = 0x01;
    const CONTAINER_INHERIT_ACE: u32 = 0x02;
    const ACL_REVISION: u32 = 2;
    const SECURITY_DESCRIPTOR_REVISION: u32 = 1;
    const OWNER_SECURITY_INFORMATION: u32 = 0x0000_0001;
    const DACL_SECURITY_INFORMATION: u32 = 0x0000_0004;
    const MAX_SECURITY_DESCRIPTOR_BYTES: usize = 1024 * 1024;
    const SE_DACL_PRESENT: u16 = 0x0004;
    const SE_DACL_PROTECTED: u16 = 0x1000;
    const ACL_SIZE_INFORMATION: u32 = 2;
    const ACCESS_ALLOWED_ACE_TYPE: u8 = 0;
    const ACCESS_DENIED_ACE_TYPE: u8 = 1;
    const INHERITED_ACE: u8 = 0x10;
    const TOKEN_QUERY: u32 = 0x0008;
    const TOKEN_USER: u32 = 1;
    const FILE_DISPOSITION_INFO_CLASS: u32 = 4;

    #[repr(C)]
    struct ByHandleFileInformation {
        attributes: u32,
        creation_low: u32,
        creation_high: u32,
        access_low: u32,
        access_high: u32,
        write_low: u32,
        write_high: u32,
        volume_serial: u32,
        size_high: u32,
        size_low: u32,
        link_count: u32,
        file_index_high: u32,
        file_index_low: u32,
    }

    #[repr(C)]
    struct AclSizeInformation {
        ace_count: u32,
        bytes_in_use: u32,
        bytes_free: u32,
    }

    #[repr(C)]
    struct AceHeader {
        ace_type: u8,
        ace_flags: u8,
        ace_size: u16,
    }

    #[repr(C)]
    struct SidAndAttributes {
        sid: *mut c_void,
        attributes: u32,
    }

    #[repr(C)]
    struct FileDispositionInfo {
        delete_file: u8,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn CreateFileW(
            file_name: *const u16,
            desired_access: u32,
            share_mode: u32,
            security_attributes: *mut c_void,
            creation_disposition: u32,
            flags_and_attributes: u32,
            template_file: *mut c_void,
        ) -> *mut c_void;
        fn CreateDirectoryW(path: *const u16, security_attributes: *mut c_void) -> i32;
        fn GetDriveTypeW(root_path_name: *const u16) -> u32;
        fn GetCurrentProcess() -> *mut c_void;
        fn CloseHandle(handle: *mut c_void) -> i32;
        fn GetFileInformationByHandle(
            handle: *mut c_void,
            info: *mut ByHandleFileInformation,
        ) -> i32;
        fn SetFileInformationByHandle(
            handle: *mut c_void,
            file_information_class: u32,
            file_information: *const c_void,
            buffer_size: u32,
        ) -> i32;
        fn LocalFree(memory: *mut c_void) -> *mut c_void;
    }

    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: *mut c_void, access: u32, token: *mut *mut c_void) -> i32;
        fn GetTokenInformation(
            token: *mut c_void,
            token_information_class: u32,
            token_information: *mut c_void,
            token_information_length: u32,
            return_length: *mut u32,
        ) -> i32;
        fn GetKernelObjectSecurity(
            handle: *mut c_void,
            requested_information: u32,
            security_descriptor: *mut c_void,
            length: u32,
            length_needed: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorOwner(
            security_descriptor: *mut c_void,
            owner: *mut *mut c_void,
            owner_defaulted: *mut i32,
        ) -> i32;
        fn GetSecurityDescriptorControl(
            security_descriptor: *mut c_void,
            control: *mut u16,
            revision: *mut u32,
        ) -> i32;
        fn GetSecurityDescriptorDacl(
            security_descriptor: *mut c_void,
            dacl_present: *mut i32,
            dacl: *mut *mut c_void,
            dacl_defaulted: *mut i32,
        ) -> i32;
        fn GetAclInformation(
            acl: *mut c_void,
            acl_information: *mut c_void,
            acl_information_length: u32,
            acl_information_class: u32,
        ) -> i32;
        fn GetAce(acl: *mut c_void, ace_index: u32, ace: *mut *mut c_void) -> i32;
        fn InitializeAcl(acl: *mut c_void, acl_length: u32, acl_revision: u32) -> i32;
        fn AddAccessAllowedAceEx(
            acl: *mut c_void,
            acl_revision: u32,
            ace_flags: u32,
            access_mask: u32,
            sid: *mut c_void,
        ) -> i32;
        fn GetLengthSid(sid: *mut c_void) -> u32;
        fn InitializeSecurityDescriptor(security_descriptor: *mut c_void, revision: u32) -> i32;
        fn SetSecurityDescriptorDacl(
            security_descriptor: *mut c_void,
            dacl_present: i32,
            dacl: *mut c_void,
            dacl_defaulted: i32,
        ) -> i32;
        fn SetSecurityDescriptorControl(
            security_descriptor: *mut c_void,
            control_bits_of_interest: u16,
            control_bits_to_set: u16,
        ) -> i32;
        fn EqualSid(sid1: *mut c_void, sid2: *mut c_void) -> i32;
        fn ConvertStringSidToSidW(string_sid: *const u16, sid: *mut *mut c_void) -> i32;
    }

    #[derive(Clone, Copy, PartialEq, Eq)]
    pub struct FileIdentity {
        volume: u32,
        index_high: u32,
        index_low: u32,
    }

    struct TokenUser {
        handle: *mut c_void,
        buffer: Vec<usize>,
    }
    impl Drop for TokenUser {
        fn drop(&mut self) {
            unsafe {
                let _ = CloseHandle(self.handle);
            }
        }
    }
    impl TokenUser {
        fn sid(&self) -> *mut c_void {
            unsafe { (*(self.buffer.as_ptr() as *const SidAndAttributes)).sid }
        }
    }

    struct LocalSid(*mut c_void);
    impl Drop for LocalSid {
        fn drop(&mut self) {
            if !self.0.is_null() {
                unsafe {
                    let _ = LocalFree(self.0);
                }
            }
        }
    }

    pub struct OwnedStorageTemporary {
        path: Option<PathBuf>,
        identity: FileIdentity,
    }
    impl OwnedStorageTemporary {
        pub fn new(path: PathBuf, identity: FileIdentity) -> Self {
            Self {
                path: Some(path),
                identity,
            }
        }
        pub fn disarm(&mut self) {
            self.path = None;
        }
    }
    impl Drop for OwnedStorageTemporary {
        fn drop(&mut self) {
            if let Some(path) = self.path.take() {
                let _ = remove_if_same(&path, self.identity);
            }
        }
    }

    fn wide_path(path: &Path) -> std::io::Result<Vec<u16>> {
        let mut wide: Vec<u16> = path.as_os_str().encode_wide().collect();
        if wide.contains(&0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "path contains an embedded NUL",
            ));
        }
        wide.push(0);
        Ok(wide)
    }

    pub fn drive_type(root: &[u16]) -> std::io::Result<u32> {
        let mut wide = root.to_vec();
        wide.push(0);
        let kind = unsafe { GetDriveTypeW(wide.as_ptr()) };
        if kind == 0 {
            let error = std::io::Error::last_os_error();
            trace_storage_open_error("drive_type", &error);
            Err(error)
        } else {
            Ok(kind)
        }
    }

    fn open_handle(
        path: &Path,
        access: u32,
        share: u32,
        disposition: u32,
        flags: u32,
    ) -> std::io::Result<File> {
        let wide = wide_path(path)?;
        let handle = unsafe {
            CreateFileW(
                wide.as_ptr(),
                access,
                share,
                ptr::null_mut(),
                disposition,
                flags,
                ptr::null_mut(),
            )
        };
        if handle as isize == -1 {
            return Err(std::io::Error::last_os_error());
        }
        if handle.is_null() {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { File::from_raw_handle(handle) })
    }

    fn file_information(file: &File) -> std::io::Result<ByHandleFileInformation> {
        let mut info = std::mem::MaybeUninit::<ByHandleFileInformation>::zeroed();
        if unsafe { GetFileInformationByHandle(file.as_raw_handle(), info.as_mut_ptr()) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(unsafe { info.assume_init() })
    }

    fn file_identity(info: &ByHandleFileInformation) -> FileIdentity {
        FileIdentity {
            volume: info.volume_serial,
            index_high: info.file_index_high,
            index_low: info.file_index_low,
        }
    }

    fn validate_plain_directory(file: &File) -> std::io::Result<ByHandleFileInformation> {
        let info = file_information(file)?;
        if info.attributes & FILE_ATTRIBUTE_DIRECTORY == 0
            || info.attributes & (FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DEVICE) != 0
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "storage ancestor is not a plain directory",
            ));
        }
        Ok(info)
    }

    pub fn pin_directories(
        root: &Path,
        components: &[std::ffi::OsString],
    ) -> std::io::Result<Vec<File>> {
        pin_directories_with_policy(root, components, true)
    }

    fn pin_ancestors(root: &Path, components: &[std::ffi::OsString]) -> std::io::Result<Vec<File>> {
        pin_directories_with_policy(root, components, false)
    }

    fn pin_directories_with_policy(
        root: &Path,
        components: &[std::ffi::OsString],
        require_private_leaf: bool,
    ) -> std::io::Result<Vec<File>> {
        if components.is_empty() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "storage path must name an existing directory",
            ));
        }
        let drive = root.as_os_str().encode_wide().take(2).collect::<Vec<_>>();
        let mut drive_root = std::ffi::OsString::from_wide(&[drive[0], b':' as u16, b'\\' as u16]);
        let mut current = PathBuf::from(std::mem::take(&mut drive_root));
        let mut handles = Vec::with_capacity(components.len() + 1);
        let root_handle = open_handle(
            &current,
            FILE_READ_ATTRIBUTES | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )
        .inspect_err(|error| trace_storage_open_error("volume_root_handle", error))?;
        let root_info = validate_plain_directory(&root_handle)
            .inspect_err(|error| trace_storage_open_error("volume_root_validation", error))?;
        let volume = root_info.volume_serial;
        handles.push(root_handle);
        for (index, component) in components.iter().enumerate() {
            current.push(component);
            let handle = open_handle(
                &current,
                FILE_READ_ATTRIBUTES | READ_CONTROL,
                FILE_SHARE_READ | FILE_SHARE_WRITE,
                OPEN_EXISTING,
                FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
            )
            .inspect_err(|error| trace_storage_open_error("ancestor_or_leaf_handle", error))?;
            let info = validate_plain_directory(&handle).inspect_err(|error| {
                trace_storage_open_error("ancestor_or_leaf_validation", error)
            })?;
            if info.volume_serial != volume {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "storage path crosses a volume boundary",
                ));
            }
            if require_private_leaf && index + 1 == components.len() {
                validate_security(&handle, true)
                    .inspect_err(|error| trace_storage_open_error("leaf_owner_or_dacl", error))?;
            }
            handles.push(handle);
        }
        Ok(handles)
    }

    fn open_private_directory(path: &Path) -> std::io::Result<File> {
        let file = open_handle(
            path,
            FILE_READ_ATTRIBUTES | READ_CONTROL,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            OPEN_EXISTING,
            FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        validate_plain_directory(&file)?;
        validate_security(&file, true)?;
        Ok(file)
    }

    fn create_private_directory(path: &Path) -> std::io::Result<()> {
        #[repr(C)]
        struct SecurityAttributes {
            length: u32,
            security_descriptor: *mut c_void,
            inherit_handle: i32,
        }

        let current = current_user()?;
        let system = sid_from_text("S-1-5-18")?;
        let admins = sid_from_text("S-1-5-32-544")?;
        let sids = [current.sid(), admins.0, system.0];
        let mut acl_bytes = 8usize;
        for sid in sids {
            let sid_length = unsafe { GetLengthSid(sid) } as usize;
            if sid_length == 0 {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "current token contains an invalid SID",
                ));
            }
            acl_bytes = acl_bytes.checked_add(8 + sid_length).ok_or_else(|| {
                std::io::Error::new(std::io::ErrorKind::InvalidData, "DACL size overflow")
            })?;
        }
        let acl_length = u32::try_from(acl_bytes).map_err(|_| {
            std::io::Error::new(std::io::ErrorKind::InvalidData, "DACL size overflow")
        })?;
        let acl_words = acl_bytes.div_ceil(std::mem::size_of::<usize>());
        let mut acl_storage = vec![0usize; acl_words];
        let acl = acl_storage.as_mut_ptr().cast::<c_void>();
        if unsafe { InitializeAcl(acl, acl_length, ACL_REVISION) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        for sid in sids {
            if unsafe {
                AddAccessAllowedAceEx(
                    acl,
                    ACL_REVISION,
                    OBJECT_INHERIT_ACE | CONTAINER_INHERIT_ACE,
                    FILE_ALL_ACCESS,
                    sid,
                )
            } == 0
            {
                return Err(std::io::Error::last_os_error());
            }
        }

        // SECURITY_DESCRIPTOR is opaque here; this aligned buffer exceeds
        // SECURITY_DESCRIPTOR_MIN_LENGTH on both supported Windows widths.
        let mut descriptor_storage = [0usize; 16];
        let descriptor = descriptor_storage.as_mut_ptr().cast::<c_void>();
        if unsafe { InitializeSecurityDescriptor(descriptor, SECURITY_DESCRIPTOR_REVISION) } == 0
            || unsafe { SetSecurityDescriptorDacl(descriptor, 1, acl, 0) } == 0
            || unsafe {
                SetSecurityDescriptorControl(descriptor, SE_DACL_PROTECTED, SE_DACL_PROTECTED)
            } == 0
        {
            return Err(std::io::Error::last_os_error());
        }

        let mut attributes = SecurityAttributes {
            length: std::mem::size_of::<SecurityAttributes>() as u32,
            security_descriptor: descriptor,
            inherit_handle: 0,
        };
        let wide = wide_path(path)?;
        if unsafe {
            CreateDirectoryW(
                wide.as_ptr(),
                (&mut attributes as *mut SecurityAttributes).cast(),
            )
        } != 0
        {
            Ok(())
        } else {
            Err(std::io::Error::last_os_error())
        }
    }

    pub fn pin_or_create_account_directory(
        app_data_root: &Path,
        components: &[std::ffi::OsString],
    ) -> std::io::Result<(PathBuf, Vec<File>)> {
        // These handles pin the entire existing app-data path before the only
        // creation call. OPEN_REPARSE_POINT plus attribute validation rejects
        // junctions without following them; omitting FILE_SHARE_DELETE keeps
        // the checked path components from being replaced while we work.
        let mut directory_handles = pin_ancestors(app_data_root, components)?;
        let parent = directory_handles.last().ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "app-data root is not pinned",
            )
        })?;
        let parent_volume = file_information(parent)?.volume_serial;
        let account_path = app_data_root.join("account");

        let account_handle = match open_private_directory(&account_path) {
            Ok(handle) => handle,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                match create_private_directory(&account_path) {
                    Ok(()) => {}
                    // Another creator may have won the atomic create race.
                    // Treat its entry like any pre-existing child and validate
                    // it below without repairing or replacing it.
                    Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
                    Err(error) => return Err(error),
                }
                open_private_directory(&account_path)?
            }
            Err(error) => return Err(error),
        };
        if file_information(&account_handle)?.volume_serial != parent_volume {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "account storage crosses a volume boundary",
            ));
        }
        directory_handles.push(account_handle);
        Ok((account_path, directory_handles))
    }

    pub fn open_lock(path: &Path) -> std::io::Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .share_mode(0)
            .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
            .open(path)?;
        let info = file_information(&file)?;
        if info.attributes
            & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DEVICE)
            != 0
            || info.link_count != 1
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "storage lock must be a single-link plain file",
            ));
        }
        validate_security(&file, false)?;
        file.sync_all()?;
        Ok(file)
    }

    pub fn open_regular_file(path: &Path, access: u32, share: u32) -> std::io::Result<File> {
        open_checked_file(path, access, share)
    }

    fn open_checked_file(path: &Path, access: u32, share: u32) -> std::io::Result<File> {
        let file = open_handle(
            path,
            access | READ_CONTROL | FILE_READ_ATTRIBUTES,
            share,
            OPEN_EXISTING,
            FILE_FLAG_OPEN_REPARSE_POINT,
        )?;
        let info = file_information(&file)?;
        if info.attributes
            & (FILE_ATTRIBUTE_DIRECTORY | FILE_ATTRIBUTE_REPARSE_POINT | FILE_ATTRIBUTE_DEVICE)
            != 0
            || info.link_count != 1
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "storage entry must be a single-link plain file",
            ));
        }
        validate_security(&file, false)?;
        Ok(file)
    }

    pub fn ensure_replaceable_file(path: &Path) -> std::io::Result<()> {
        match open_checked_file(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE) {
            Ok(_file) => Ok(()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(error) => Err(error),
        }
    }

    pub fn ensure_same_plain_file(path: &Path, expected: FileIdentity) -> std::io::Result<()> {
        let file = open_checked_file(path, GENERIC_READ, FILE_SHARE_READ | FILE_SHARE_WRITE)?;
        if file_identity(&file_information(&file)?) != expected {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "temporary file identity changed",
            ));
        }
        Ok(())
    }

    pub fn create_temporary(root: &Path) -> std::io::Result<(PathBuf, File, FileIdentity)> {
        use std::sync::atomic::{AtomicU64, Ordering};
        static NEXT_TEMPORARY_ID: AtomicU64 = AtomicU64::new(0);
        for _ in 0..128 {
            let sequence = NEXT_TEMPORARY_ID.fetch_add(1, Ordering::Relaxed);
            let path = root.join(format!(
                ".account-storage-{}-{sequence}.tmp",
                std::process::id()
            ));
            match OpenOptions::new()
                .read(true)
                .write(true)
                .create_new(true)
                .share_mode(0)
                .custom_flags(FILE_FLAG_OPEN_REPARSE_POINT)
                .open(&path)
            {
                Ok(file) => {
                    let info = file_information(&file)?;
                    if info.attributes
                        & (FILE_ATTRIBUTE_DIRECTORY
                            | FILE_ATTRIBUTE_REPARSE_POINT
                            | FILE_ATTRIBUTE_DEVICE)
                        != 0
                        || info.link_count != 1
                    {
                        return Err(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "temporary file is not a single-link regular file",
                        ));
                    }
                    validate_security(&file, false)?;
                    return Ok((path, file, file_identity(&info)));
                }
                Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(std::io::Error::new(
            std::io::ErrorKind::AlreadyExists,
            "could not allocate a storage temporary file",
        ))
    }

    fn remove_if_same(path: &Path, expected: FileIdentity) -> std::io::Result<()> {
        let file = match open_checked_file(
            path,
            DELETE | GENERIC_READ,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(error) => return Err(error),
        };
        let info = file_information(&file)?;
        if file_identity(&info) != expected || info.link_count != 1 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "temporary entry changed before cleanup",
            ));
        }
        set_delete_disposition(&file)
    }

    fn set_delete_disposition(file: &File) -> std::io::Result<()> {
        let disposition = FileDispositionInfo { delete_file: 1 };
        let succeeded = unsafe {
            SetFileInformationByHandle(
                file.as_raw_handle(),
                FILE_DISPOSITION_INFO_CLASS,
                &disposition as *const _ as *const c_void,
                std::mem::size_of::<FileDispositionInfo>() as u32,
            )
        };
        if succeeded == 0 {
            Err(std::io::Error::last_os_error())
        } else {
            Ok(())
        }
    }

    fn current_user() -> std::io::Result<TokenUser> {
        let mut handle = ptr::null_mut();
        if unsafe { OpenProcessToken(GetCurrentProcess(), TOKEN_QUERY, &mut handle) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        let mut needed = 0;
        unsafe {
            let _ = GetTokenInformation(handle, TOKEN_USER, ptr::null_mut(), 0, &mut needed);
        }
        if needed == 0 {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(std::io::Error::last_os_error());
        }
        let mut buffer = vec![0usize; (needed as usize).div_ceil(std::mem::size_of::<usize>())];
        if unsafe {
            GetTokenInformation(
                handle,
                TOKEN_USER,
                buffer.as_mut_ptr().cast(),
                needed,
                &mut needed,
            )
        } == 0
        {
            unsafe {
                let _ = CloseHandle(handle);
            }
            return Err(std::io::Error::last_os_error());
        }
        Ok(TokenUser { handle, buffer })
    }

    fn sid_from_text(value: &str) -> std::io::Result<LocalSid> {
        let wide: Vec<u16> = value.encode_utf16().chain(Some(0)).collect();
        let mut sid = ptr::null_mut();
        if unsafe { ConvertStringSidToSidW(wide.as_ptr(), &mut sid) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        Ok(LocalSid(sid))
    }

    fn trusted_sid(
        sid: *mut c_void,
        current_user: *mut c_void,
        system: *mut c_void,
        admins: *mut c_void,
    ) -> bool {
        unsafe {
            EqualSid(sid, current_user) != 0
                || EqualSid(sid, system) != 0
                || EqualSid(sid, admins) != 0
        }
    }

    fn validate_security(file: &File, require_protected_dacl: bool) -> std::io::Result<()> {
        let current = current_user()?;
        let system = sid_from_text("S-1-5-18")?;
        let admins = sid_from_text("S-1-5-32-544")?;
        let requested = OWNER_SECURITY_INFORMATION | DACL_SECURITY_INFORMATION;
        let mut needed = 0u32;
        let first_query = unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                requested,
                ptr::null_mut(),
                0,
                &mut needed,
            )
        };
        if needed == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if first_query != 0 || needed as usize > MAX_SECURITY_DESCRIPTOR_BYTES {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "security descriptor size is invalid",
            ));
        }
        let word_count = (needed as usize).div_ceil(std::mem::size_of::<usize>());
        let mut descriptor_buffer = vec![0usize; word_count];
        let descriptor_capacity = word_count * std::mem::size_of::<usize>();
        let mut actual = 0u32;
        if unsafe {
            GetKernelObjectSecurity(
                file.as_raw_handle(),
                requested,
                descriptor_buffer.as_mut_ptr().cast(),
                u32::try_from(descriptor_capacity).map_err(|_| {
                    std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        "security descriptor size is invalid",
                    )
                })?,
                &mut actual,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if actual == 0 || actual as usize > descriptor_capacity {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidData,
                "security descriptor length is invalid",
            ));
        }
        let descriptor = descriptor_buffer.as_mut_ptr().cast::<c_void>();
        let mut owner = ptr::null_mut();
        let mut owner_defaulted = 0;
        if unsafe { GetSecurityDescriptorOwner(descriptor, &mut owner, &mut owner_defaulted) } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        if owner.is_null() || !trusted_sid(owner, current.sid(), system.0, admins.0) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "storage owner is not trusted",
            ));
        }
        let mut control = 0u16;
        let mut revision = 0u32;
        if unsafe { GetSecurityDescriptorControl(descriptor, &mut control, &mut revision) } == 0 {
            return Err(std::io::Error::last_os_error());
        }
        if require_protected_dacl
            && (control & SE_DACL_PRESENT == 0 || control & SE_DACL_PROTECTED == 0)
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "storage leaf DACL is not protected",
            ));
        }
        let mut dacl_present = 0;
        let mut descriptor_dacl = ptr::null_mut();
        let mut defaulted = 0;
        if unsafe {
            GetSecurityDescriptorDacl(
                descriptor,
                &mut dacl_present,
                &mut descriptor_dacl,
                &mut defaulted,
            )
        } == 0
            || dacl_present == 0
            || descriptor_dacl.is_null()
        {
            return Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "storage DACL is missing or invalid",
            ));
        }
        let mut acl_info = AclSizeInformation {
            ace_count: 0,
            bytes_in_use: 0,
            bytes_free: 0,
        };
        if unsafe {
            GetAclInformation(
                descriptor_dacl,
                &mut acl_info as *mut _ as *mut c_void,
                std::mem::size_of::<AclSizeInformation>() as u32,
                ACL_SIZE_INFORMATION,
            )
        } == 0
        {
            return Err(std::io::Error::last_os_error());
        }
        for index in 0..acl_info.ace_count {
            let mut ace = ptr::null_mut();
            if unsafe { GetAce(descriptor_dacl, index, &mut ace) } == 0 || ace.is_null() {
                return Err(std::io::Error::last_os_error());
            }
            let header = unsafe { ptr::read_unaligned(ace as *const AceHeader) };
            if header.ace_size < 8
                || (require_protected_dacl && header.ace_flags & INHERITED_ACE != 0)
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "storage DACL contains an invalid or inherited ACE",
                ));
            }
            if header.ace_type != ACCESS_ALLOWED_ACE_TYPE
                && header.ace_type != ACCESS_DENIED_ACE_TYPE
            {
                return Err(std::io::Error::new(
                    std::io::ErrorKind::PermissionDenied,
                    "storage DACL contains an unsupported ACE",
                ));
            }
            if header.ace_type == ACCESS_ALLOWED_ACE_TYPE {
                let sid = unsafe { ace.cast::<u8>().add(8) as *mut c_void };
                if !trusted_sid(sid, current.sid(), system.0, admins.0) {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::PermissionDenied,
                        "storage DACL grants access to an untrusted principal",
                    ));
                }
            }
        }
        Ok(())
    }
}

/// Host-only account storage. It retains the pinned directory handles for the
/// lifetime of the storage root and obtains a new cross-process guard for
/// every operation.
pub struct AccountHost {
    storage: AccountStorage,
}

pub struct ConfirmedDeleteStatus {
    pub account_id: String,
    pub resolution: DeleteResolution,
}
pub trait DeleteStatusBroker {
    fn confirmed_delete_status(
        &mut self,
        account_id: &str,
    ) -> Result<ConfirmedDeleteStatus, kohbor_account::BrokerError>;
}
impl AccountHost {
    /// Open the one fixed `account` child of an already-existing app-data
    /// directory. Provisioning is deliberately non-recursive: if the trusted
    /// app root is missing or unsafe, account storage stays unavailable.
    pub fn open(app_data_root: &Path) -> Result<Self, StoreError> {
        let (root, components) =
            trusted_storage_path(app_data_root).map_err(|_| StoreError::Unreadable)?;

        #[cfg(windows)]
        {
            let (path, directory_handles) =
                win32::pin_or_create_account_directory(&root, &components)
                    .map_err(|_| StoreError::Unreadable)?;
            Ok(Self {
                storage: AccountStorage {
                    inner: Arc::new(StorageRoot {
                        path,
                        _directory_handles: directory_handles,
                    }),
                },
            })
        }

        #[cfg(not(windows))]
        {
            let _ = (root, components);
            Err(StoreError::Unreadable)
        }
    }

    pub fn with_operation<R>(
        &self,
        operation: impl FnOnce(&mut AccountStorageGuard) -> R,
    ) -> Result<R, StoreError> {
        let mut guard = self.storage.try_lock()?;
        Ok(operation(&mut guard))
    }

    /// Resolve only a durable pending delete, and keep the storage lock held
    /// across the trusted broker status query and any local reconciliation.
    /// An ambiguous or mismatched result leaves the durable marker in place.
    pub fn reconcile_pending_delete(
        &self,
        broker: &mut impl DeleteStatusBroker,
    ) -> Result<(), StoreError> {
        self.with_operation(|guard| {
            match guard.load_recovery_marker()? {
                RecoveryMarker::Clear => Ok(()),
                RecoveryMarker::WipePending { account_id } => {
                    // WipePending is already local authorization to retry the
                    // erase. Do not contact the broker or repeat deletion.
                    guard.wipe_account(&account_id)?;
                    guard.write_recovery_marker(&RecoveryMarker::Clear)
                }
                RecoveryMarker::DeletePending { account_id } => {
                    let status = broker
                        .confirmed_delete_status(&account_id)
                        .map_err(|_| StoreError::Unreadable)?;
                    if status.account_id != account_id {
                        return Err(StoreError::Unreadable);
                    }
                    match status.resolution {
                        DeleteResolution::Unknown => Ok(()),
                        resolution => guard.resolve_pending_delete(&account_id, resolution),
                    }
                }
            }
        })?
    }
}

/// Lazily opens account storage for host-side callers. Keeping only the app
/// data path in managed state avoids storage I/O during Tauri setup and keeps
/// account-storage failures independent from core startup.
#[cfg(feature = "demo")]
pub struct DesktopAccountHost {
    app_data_root: PathBuf,
}

#[cfg(feature = "demo")]
impl DesktopAccountHost {
    pub fn new(app_data_root: PathBuf) -> Self {
        Self { app_data_root }
    }

    pub fn with_operation<R>(
        &self,
        operation: impl FnOnce(&mut AccountStorageGuard) -> R,
    ) -> Result<R, StoreError> {
        AccountHost::open(&self.app_data_root)?.with_operation(operation)
    }

    pub fn reconcile_pending_delete(
        &self,
        broker: &mut impl DeleteStatusBroker,
    ) -> Result<(), StoreError> {
        AccountHost::open(&self.app_data_root)?.reconcile_pending_delete(broker)
    }

    /// Delegate to the same loopback host while keeping account storage lazy.
    /// Preflight the request before opening storage so live-match and already
    /// cancelled requests do not provision or touch the account directory.
    pub fn sign_in_via_browser(
        &self,
        client: &mut kohbor_account::Client,
        request: BrowserSignInRequest<'_>,
        clock: &impl kohbor_account::Clock,
        provider: &mut impl kohbor_account::Provider,
        broker: &mut impl kohbor_account::Broker,
        launch: impl FnOnce(
            &kohbor_account::AuthorizationRequest,
            SocketAddrV4,
        ) -> Result<(), BrowserSignInError>,
    ) -> Result<kohbor_account::CallbackOutcome, BrowserSignInError> {
        if request.in_live_match {
            return Err(BrowserSignInError::LiveMatch);
        }
        if request.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(BrowserSignInError::Cancelled);
        }
        if request.timeout.is_zero() {
            return Err(BrowserSignInError::Timeout);
        }
        let host = AccountHost::open(&self.app_data_root).map_err(|_| BrowserSignInError::Io)?;
        host.sign_in_via_browser(client, request, clock, provider, broker, launch)
    }
}

pub struct BrowserSignInRequest<'a> {
    pub material: kohbor_account::AuthMaterial,
    pub service: &'a kohbor_account::ServiceConfig,
    pub in_live_match: bool,
    pub timeout: std::time::Duration,
    pub cancelled: &'a std::sync::atomic::AtomicBool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrowserSignInError {
    Unavailable,
    LiveMatch,
    Cancelled,
    Timeout,
    InvalidCallback,
    BrowserLaunch,
    Io,
}

impl AccountHost {
    pub fn sign_in_via_browser(
        &self,
        client: &mut kohbor_account::Client,
        request: BrowserSignInRequest<'_>,
        clock: &impl kohbor_account::Clock,
        provider: &mut impl kohbor_account::Provider,
        broker: &mut impl kohbor_account::Broker,
        launch: impl FnOnce(
            &kohbor_account::AuthorizationRequest,
            SocketAddrV4,
        ) -> Result<(), BrowserSignInError>,
    ) -> Result<kohbor_account::CallbackOutcome, BrowserSignInError> {
        if request.in_live_match {
            return Err(BrowserSignInError::LiveMatch);
        }
        if request.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            return Err(BrowserSignInError::Cancelled);
        }
        if request.timeout.is_zero() {
            return Err(BrowserSignInError::Timeout);
        }

        let timeout = request.timeout.min(MAX_BROWSER_SIGNIN_TIMEOUT);

        let (auth, session_revision) = self
            .with_operation(|guard| {
                let auth = client
                    .begin_handoff(request.material, clock, guard)
                    .map_err(map_begin_handoff_error)?;
                let revision = match guard.read_session_file_revision() {
                    Ok(revision) => revision,
                    Err(_) => {
                        client.cancel_handoff();
                        return Err(BrowserSignInError::Io);
                    }
                };
                Ok((auth, revision))
            })
            .map_err(|_| BrowserSignInError::Io)??;

        let listener = match TcpListener::bind(SocketAddrV4::new(Ipv4Addr::LOCALHOST, 0)) {
            Ok(listener) => listener,
            Err(_) => {
                client.cancel_handoff();
                return Err(BrowserSignInError::Io);
            }
        };
        if listener.set_nonblocking(true).is_err() {
            client.cancel_handoff();
            return Err(BrowserSignInError::Io);
        }
        let address = match listener.local_addr() {
            Ok(address) => match address {
                std::net::SocketAddr::V4(address) => address,
                std::net::SocketAddr::V6(_) => {
                    client.cancel_handoff();
                    return Err(BrowserSignInError::Io);
                }
            },
            Err(_) => {
                client.cancel_handoff();
                return Err(BrowserSignInError::Io);
            }
        };

        if request.cancelled.load(std::sync::atomic::Ordering::Acquire) {
            drop(listener);
            client.cancel_handoff();
            return Err(BrowserSignInError::Cancelled);
        }
        // Storage admission and listener setup can include platform calls.
        // Start the bounded browser wait only when dispatch is ready, while
        // retaining the caller's maximum five-minute window.
        let deadline = Instant::now() + timeout;
        if let Err(error) = check_browser_wait(request.cancelled, deadline) {
            drop(listener);
            client.cancel_handoff();
            return Err(error);
        }
        if let Err(error) = launch(&auth, address) {
            drop(listener);
            client.cancel_handoff();
            return Err(error);
        }

        let mut stream = match accept_one_callback_peer(listener, request.cancelled, deadline) {
            Ok(stream) => stream,
            Err(error) => {
                client.cancel_handoff();
                return Err(error);
            }
        };

        let callback = match read_callback_request(&mut stream, request.cancelled, deadline) {
            Ok(callback) => callback,
            Err(error) => {
                let _ = write_callback_response(&mut stream, Some(error));
                client.cancel_handoff();
                return Err(error);
            }
        };

        if let Err(error) = check_browser_wait(request.cancelled, deadline) {
            let _ = write_callback_response(&mut stream, Some(error));
            client.cancel_handoff();
            return Err(error);
        }

        // The response is fixed text. It deliberately contains no callback
        // values, provider data, or credentials.
        if callback.state != auth.state {
            let _ = write_callback_response(&mut stream, Some(BrowserSignInError::InvalidCallback));
            client.cancel_handoff();
            return Err(BrowserSignInError::InvalidCallback);
        }
        if let Err(error) = write_callback_response(&mut stream, None) {
            client.cancel_handoff();
            return Err(error);
        }
        drop(stream);

        // The browser wait held no storage guard. Acquire a new guard and keep
        // it across the complete pure-client callback, provider exchange,
        // broker sign-in, recovery recheck, and session persistence.
        let callback_result = self.with_operation(|guard| {
            check_browser_wait(request.cancelled, deadline)?;
            // Keep session contents unread while recovery is pending or unreadable.
            if guard
                .load_recovery_marker()
                .map_err(|_| BrowserSignInError::Io)?
                == RecoveryMarker::Clear
                && guard
                    .read_session_file_revision()
                    .map_err(|_| BrowserSignInError::Io)?
                    != session_revision
            {
                return Err(BrowserSignInError::Cancelled);
            }
            Ok(client.deliver_callback(
                (&callback.state, &callback.code),
                request.service,
                clock,
                provider,
                broker,
                guard,
            ))
        });

        match callback_result {
            Err(_) => {
                client.cancel_handoff();
                Err(BrowserSignInError::Io)
            }
            Ok(Err(error)) => {
                client.cancel_handoff();
                Err(error)
            }
            Ok(Ok(kohbor_account::CallbackOutcome::Accepted)) => {
                Ok(kohbor_account::CallbackOutcome::Accepted)
            }
            Ok(Ok(kohbor_account::CallbackOutcome::Rejected)) => {
                Err(BrowserSignInError::InvalidCallback)
            }
            Ok(Ok(kohbor_account::CallbackOutcome::Failed)) => Err(BrowserSignInError::Unavailable),
        }
    }
}

#[derive(Debug)]
struct BrowserCallback {
    code: String,
    state: String,
}

fn map_begin_handoff_error(outcome: kohbor_account::CallbackOutcome) -> BrowserSignInError {
    match outcome {
        kohbor_account::CallbackOutcome::Rejected => BrowserSignInError::InvalidCallback,
        kohbor_account::CallbackOutcome::Failed => BrowserSignInError::Unavailable,
        kohbor_account::CallbackOutcome::Accepted => BrowserSignInError::Unavailable,
    }
}

fn check_browser_wait(
    cancelled: &std::sync::atomic::AtomicBool,
    deadline: Instant,
) -> Result<(), BrowserSignInError> {
    if cancelled.load(std::sync::atomic::Ordering::Acquire) {
        return Err(BrowserSignInError::Cancelled);
    }
    if Instant::now() >= deadline {
        return Err(BrowserSignInError::Timeout);
    }
    Ok(())
}

fn accept_one_callback_peer(
    listener: TcpListener,
    cancelled: &std::sync::atomic::AtomicBool,
    deadline: Instant,
) -> Result<TcpStream, BrowserSignInError> {
    loop {
        check_browser_wait(cancelled, deadline)?;
        match listener.accept() {
            Ok((stream, _peer)) => {
                // This endpoint is strictly one-shot. Close it before reading
                // or parsing the accepted request.
                drop(listener);
                stream
                    .set_nonblocking(false)
                    .map_err(|_| BrowserSignInError::Io)?;
                return Ok(stream);
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                let remaining = deadline.saturating_duration_since(Instant::now());
                std::thread::sleep(CALLBACK_ACCEPT_POLL.min(remaining));
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => return Err(BrowserSignInError::Io),
        }
    }
}

fn read_callback_request(
    stream: &mut TcpStream,
    cancelled: &std::sync::atomic::AtomicBool,
    deadline: Instant,
) -> Result<BrowserCallback, BrowserSignInError> {
    let mut bytes = Vec::with_capacity(1024);
    let mut chunk = [0u8; 1024];

    loop {
        check_browser_wait(cancelled, deadline)?;
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            return Err(BrowserSignInError::Timeout);
        }
        stream
            .set_read_timeout(Some(CALLBACK_READ_POLL.min(remaining)))
            .map_err(|_| BrowserSignInError::Io)?;

        match stream.read(&mut chunk) {
            Ok(0) => return Err(BrowserSignInError::InvalidCallback),
            Ok(count) => {
                if bytes.len().saturating_add(count) > MAX_CALLBACK_HEADER_BYTES {
                    return Err(BrowserSignInError::InvalidCallback);
                }
                bytes.extend_from_slice(&chunk[..count]);
                if has_malformed_line_endings(&bytes) {
                    return Err(BrowserSignInError::InvalidCallback);
                }
                if let Some(end) = find_header_end(&bytes) {
                    if end + 4 != bytes.len() {
                        return Err(BrowserSignInError::InvalidCallback);
                    }
                    return parse_callback_request(&bytes[..end]);
                }
                if bytes.len() == MAX_CALLBACK_HEADER_BYTES {
                    return Err(BrowserSignInError::InvalidCallback);
                }
            }
            Err(error)
                if matches!(
                    error.kind(),
                    std::io::ErrorKind::WouldBlock
                        | std::io::ErrorKind::TimedOut
                        | std::io::ErrorKind::Interrupted
                ) =>
            {
                check_browser_wait(cancelled, deadline)?;
            }
            Err(_) => return Err(BrowserSignInError::Io),
        }
    }
}

fn has_malformed_line_endings(bytes: &[u8]) -> bool {
    for (index, byte) in bytes.iter().enumerate() {
        if *byte == b'\n' && (index == 0 || bytes[index - 1] != b'\r') {
            return true;
        }
        if *byte == b'\r' && index + 1 < bytes.len() && bytes[index + 1] != b'\n' {
            return true;
        }
    }
    false
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn parse_callback_request(header: &[u8]) -> Result<BrowserCallback, BrowserSignInError> {
    let text = std::str::from_utf8(header).map_err(|_| BrowserSignInError::InvalidCallback)?;
    let mut lines = text.split("\r\n");
    let request_line = lines.next().ok_or(BrowserSignInError::InvalidCallback)?;
    if request_line.len() > MAX_CALLBACK_LINE_BYTES || !request_line.is_ascii() {
        return Err(BrowserSignInError::InvalidCallback);
    }
    let mut fields = request_line.split(' ');
    let method = fields.next().ok_or(BrowserSignInError::InvalidCallback)?;
    let target = fields.next().ok_or(BrowserSignInError::InvalidCallback)?;
    let version = fields.next().ok_or(BrowserSignInError::InvalidCallback)?;
    if method != "GET" || version != "HTTP/1.1" || fields.next().is_some() {
        return Err(BrowserSignInError::InvalidCallback);
    }
    let (path, query) = target
        .split_once('?')
        .ok_or(BrowserSignInError::InvalidCallback)?;
    if path != "/callback" || query.is_empty() || target.contains('#') {
        return Err(BrowserSignInError::InvalidCallback);
    }

    let mut seen_headers: Vec<String> = Vec::new();
    let mut header_count = 0usize;
    let mut host_seen = false;
    for line in lines {
        if line.is_empty()
            || line.len() > MAX_CALLBACK_LINE_BYTES
            || line.starts_with(' ')
            || line.starts_with('\t')
        {
            return Err(BrowserSignInError::InvalidCallback);
        }
        header_count += 1;
        if header_count > MAX_CALLBACK_HEADER_FIELDS {
            return Err(BrowserSignInError::InvalidCallback);
        }
        let (name, value) = line
            .split_once(':')
            .ok_or(BrowserSignInError::InvalidCallback)?;
        if name.is_empty() || !name.bytes().all(is_http_token_byte) {
            return Err(BrowserSignInError::InvalidCallback);
        }
        if value
            .chars()
            .any(|character| character.is_control() && character != '\t')
        {
            return Err(BrowserSignInError::InvalidCallback);
        }
        let normalized_name = name.to_ascii_lowercase();
        if seen_headers.iter().any(|seen| seen == &normalized_name) {
            return Err(BrowserSignInError::InvalidCallback);
        }
        seen_headers.push(normalized_name.clone());
        let trimmed_value = value.trim_matches(|character| character == ' ' || character == '\t');
        if normalized_name == "host" {
            if trimmed_value.is_empty() {
                return Err(BrowserSignInError::InvalidCallback);
            }
            host_seen = true;
        }
        if matches!(
            normalized_name.as_str(),
            "content-length" | "transfer-encoding"
        ) {
            return Err(BrowserSignInError::InvalidCallback);
        }
    }
    if !host_seen {
        return Err(BrowserSignInError::InvalidCallback);
    }

    parse_callback_query(query)
}

fn is_http_token_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

fn parse_callback_query(query: &str) -> Result<BrowserCallback, BrowserSignInError> {
    let mut code = None;
    let mut state = None;
    for pair in query.split('&') {
        if pair.is_empty() {
            return Err(BrowserSignInError::InvalidCallback);
        }
        let (raw_name, raw_value) = pair
            .split_once('=')
            .ok_or(BrowserSignInError::InvalidCallback)?;
        if raw_name.is_empty() {
            return Err(BrowserSignInError::InvalidCallback);
        }
        let name = decode_query_component(raw_name)?;
        let value = decode_query_component(raw_value)?;
        if value.is_empty() || value.chars().any(char::is_control) {
            return Err(BrowserSignInError::InvalidCallback);
        }
        match name.as_str() {
            "code" if code.is_none() => code = Some(value),
            "state" if state.is_none() => state = Some(value),
            _ => return Err(BrowserSignInError::InvalidCallback),
        }
    }
    Ok(BrowserCallback {
        code: code.ok_or(BrowserSignInError::InvalidCallback)?,
        state: state.ok_or(BrowserSignInError::InvalidCallback)?,
    })
}

fn decode_query_component(value: &str) -> Result<String, BrowserSignInError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0usize;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                if index + 2 >= bytes.len() {
                    return Err(BrowserSignInError::InvalidCallback);
                }
                let high =
                    hex_value(bytes[index + 1]).ok_or(BrowserSignInError::InvalidCallback)?;
                let low = hex_value(bytes[index + 2]).ok_or(BrowserSignInError::InvalidCallback)?;
                decoded.push((high << 4) | low);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
        if decoded.len() > MAX_CALLBACK_PARAMETER_BYTES {
            return Err(BrowserSignInError::InvalidCallback);
        }
    }
    String::from_utf8(decoded).map_err(|_| BrowserSignInError::InvalidCallback)
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn write_callback_response(
    stream: &mut TcpStream,
    error: Option<BrowserSignInError>,
) -> Result<(), BrowserSignInError> {
    const RECEIVED_BODY: &[u8] = b"Sign-in request received. Return to Kohbor.";
    const REJECTED_BODY: &[u8] = b"This sign-in request could not be accepted. Return to Kohbor.";
    let (status, body) = match error {
        None => ("200 OK", RECEIVED_BODY),
        Some(BrowserSignInError::Timeout) => ("408 Request Timeout", REJECTED_BODY),
        Some(_) => ("400 Bad Request", REJECTED_BODY),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\nCache-Control: no-store\r\nPragma: no-cache\r\nReferrer-Policy: no-referrer\r\nX-Content-Type-Options: nosniff\r\nContent-Security-Policy: default-src 'none'\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        body.len()
    );
    stream
        .set_write_timeout(Some(CALLBACK_WRITE_TIMEOUT))
        .map_err(|_| BrowserSignInError::Io)?;
    stream
        .write_all(head.as_bytes())
        .and_then(|()| stream.write_all(body))
        .map_err(|_| BrowserSignInError::Io)
}
