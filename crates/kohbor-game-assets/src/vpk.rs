use std::collections::HashMap;
use std::fmt;
use std::fs::{self, File};
use std::io::{self, BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const VPK_SIGNATURE: u32 = 0x55aa_1234;
const VPK_VERSION: u32 = 2;
const VPK_HEADER_BYTES: u64 = 28;
const DIR_ARCHIVE_INDEX: u16 = 0x7fff;
const MAX_TREE_STRING_BYTES: usize = 4096;
const MAX_PATH_COMPONENT_BYTES: usize = 255;
const MAX_RETAINED_ENTRIES: usize = 100_000;
// Account for both the Entry path and its HashMap key, including duplicates.
const MAX_RETAINED_PATH_BYTES: usize = 16 * 1024 * 1024;
const MAX_ENTRY_BYTES: u64 = 256 * 1024 * 1024;
const HERO_ROOT: &str = "panorama/images/heroes/";

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug)]
pub enum Error {
    Io(io::Error),
    InvalidHeader(&'static str),
    UnsupportedVpkVersion(u32),
    InvalidTree(&'static str),
    EntryFromDifferentVpk,
    EntryOutOfBounds,
    EntryTooLarge,
    EntryCrcMismatch {
        path: String,
        expected: u32,
        actual: u32,
    },
    InvalidTexture(&'static str),
    UnsupportedTextureFormat(u8),
    InvalidPng(&'static str),
    AllocationFailed,
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::InvalidHeader(reason) => write!(f, "invalid VPK header: {reason}"),
            Self::UnsupportedVpkVersion(version) => {
                write!(f, "unsupported VPK version {version}; expected version 2")
            }
            Self::InvalidTree(reason) => write!(f, "invalid VPK tree: {reason}"),
            Self::EntryFromDifferentVpk => write!(f, "entry belongs to a different VPK"),
            Self::EntryOutOfBounds => write!(f, "VPK entry is outside its archive section"),
            Self::EntryTooLarge => write!(f, "VPK entry exceeds the 256 MiB read limit"),
            Self::EntryCrcMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "CRC mismatch for {path}: expected {expected:08x}, got {actual:08x}"
            ),
            Self::InvalidTexture(reason) => write!(f, "invalid VTEX resource: {reason}"),
            Self::UnsupportedTextureFormat(format) => {
                write!(f, "unsupported VTEX image format {format}")
            }
            Self::InvalidPng(reason) => write!(f, "invalid embedded PNG: {reason}"),
            Self::AllocationFailed => write!(f, "allocation size is unavailable"),
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            _ => None,
        }
    }
}

impl From<io::Error> for Error {
    fn from(value: io::Error) -> Self {
        Self::Io(value)
    }
}

#[derive(Clone, Debug)]
pub struct Entry {
    source_dir: Arc<PathBuf>,
    path: String,
    crc32: u32,
    preload_file_offset: u64,
    preload_len: u16,
    archive_index: u16,
    offset: u32,
    length: u32,
}

impl Entry {
    pub fn path(&self) -> &str {
        &self.path
    }

    pub fn encoded_len(&self) -> u64 {
        u64::from(self.preload_len) + u64::from(self.length)
    }
}

#[derive(Debug)]
pub struct Vpk {
    dir_path: Arc<PathBuf>,
    archive_prefix: String,
    tree_size: u32,
    data_start: u64,
    data_size: u32,
    hero_entries: HashMap<String, Entry>,
}

impl Vpk {
    /// Size of the on-disk tree streamed during open, excluding archive data.
    pub fn directory_tree_bytes(&self) -> u32 {
        self.tree_size
    }

    /// Number of hero portrait metadata records retained for fast lookup.
    pub fn indexed_portrait_count(&self) -> usize {
        self.hero_entries.len()
    }

    /// Open and stream-validate a VPK v2 directory file.
    ///
    /// Only hero-image entries are retained in memory. Other paths are searched
    /// by rescanning the bounded tree when `find` is called.
    pub fn open(dir_path: impl AsRef<Path>) -> Result<Self> {
        let canonical = fs::canonicalize(dir_path.as_ref())?;
        let mut file = File::open(&canonical)?;
        let file_len = file.metadata()?.len();
        let mut header = [0_u8; VPK_HEADER_BYTES as usize];
        file.read_exact(&mut header)?;

        let signature = read_u32(&header, 0)?;
        if signature != VPK_SIGNATURE {
            return Err(Error::InvalidHeader("signature mismatch"));
        }
        let version = read_u32(&header, 4)?;
        if version != VPK_VERSION {
            return Err(Error::UnsupportedVpkVersion(version));
        }
        let tree_size = read_u32(&header, 8)?;
        let data_size = read_u32(&header, 12)?;
        let archive_md5_size = read_u32(&header, 16)?;
        let other_md5_size = read_u32(&header, 20)?;
        let signature_size = read_u32(&header, 24)?;
        let data_start = VPK_HEADER_BYTES
            .checked_add(u64::from(tree_size))
            .ok_or(Error::InvalidHeader("tree offset overflow"))?;
        let required_len = data_start
            .checked_add(u64::from(data_size))
            .and_then(|n| n.checked_add(u64::from(archive_md5_size)))
            .and_then(|n| n.checked_add(u64::from(other_md5_size)))
            .and_then(|n| n.checked_add(u64::from(signature_size)))
            .ok_or(Error::InvalidHeader("section length overflow"))?;
        if required_len > file_len {
            return Err(Error::InvalidHeader("declared sections exceed file length"));
        }
        let archive_prefix = directory_archive_prefix(&canonical)?;
        let dir_path = Arc::new(canonical);
        let mut hero_entries = HashMap::new();
        let mut bounded = BoundedReader::new(file, u64::from(tree_size));
        parse_tree(&mut bounded, &dir_path, data_size, true, None, |entry| {
            hero_entries
                .try_reserve(1)
                .map_err(|_| Error::AllocationFailed)?;
            let mut key = String::new();
            key.try_reserve_exact(entry.path.len())
                .map_err(|_| Error::AllocationFailed)?;
            key.push_str(&entry.path);
            hero_entries.insert(key, entry);
            Ok(())
        })?;

        Ok(Self {
            dir_path,
            archive_prefix,
            tree_size,
            data_start,
            data_size,
            hero_entries,
        })
    }

    /// Find an entry by slash-separated VPK path, case-insensitively.
    pub fn find(&self, path: &str) -> Option<Entry> {
        if path.len() > MAX_TREE_STRING_BYTES + 2 * MAX_PATH_COMPONENT_BYTES + 2 {
            return None;
        }
        let query = normalize_path(path);
        if let Some(entry) = self.hero_entries.get(&query) {
            return Some(entry.clone());
        }

        let mut file = File::open(self.dir_path.as_path()).ok()?;
        file.seek(SeekFrom::Start(VPK_HEADER_BYTES)).ok()?;
        let bounded = BoundedReader::new(file, u64::from(self.tree_size));
        let mut found = None;
        parse_tree(
            bounded,
            &self.dir_path,
            self.data_size,
            false,
            Some(&query),
            |entry| {
                found = Some(entry);
                Ok(())
            },
        )
        .ok()?;
        found
    }

    /// Read exactly one entry's preload and payload bytes, then verify its CRC.
    pub fn read(&self, entry: &Entry) -> Result<Vec<u8>> {
        if !Arc::ptr_eq(&self.dir_path, &entry.source_dir) {
            return Err(Error::EntryFromDifferentVpk);
        }
        let output_len = entry.encoded_len();
        if output_len > MAX_ENTRY_BYTES {
            return Err(Error::EntryTooLarge);
        }
        let payload_len = u64::from(entry.length);
        let (archive_path, archive_offset, archive_limit) =
            if entry.archive_index == DIR_ARCHIVE_INDEX {
                let end = u64::from(entry.offset)
                    .checked_add(payload_len)
                    .ok_or(Error::EntryOutOfBounds)?;
                if end > u64::from(self.data_size) {
                    return Err(Error::EntryOutOfBounds);
                }
                (
                    self.dir_path.as_path().to_path_buf(),
                    self.data_start,
                    Some(self.data_size),
                )
            } else {
                let path = self.archive_path(entry.archive_index)?;
                (path, 0, None)
            };

        let mut archive = File::open(&archive_path)?;
        let archive_len = archive.metadata()?.len();
        let absolute_offset = archive_offset
            .checked_add(u64::from(entry.offset))
            .ok_or(Error::EntryOutOfBounds)?;
        let end = absolute_offset
            .checked_add(payload_len)
            .ok_or(Error::EntryOutOfBounds)?;
        if end > archive_len {
            return Err(Error::EntryOutOfBounds);
        }
        if let Some(section_len) = archive_limit {
            let section_end = u64::from(entry.offset)
                .checked_add(payload_len)
                .ok_or(Error::EntryOutOfBounds)?;
            if section_end > u64::from(section_len) {
                return Err(Error::EntryOutOfBounds);
            }
        }
        let preload_len = usize::from(entry.preload_len);
        let mut bytes = Vec::new();
        bytes
            .try_reserve(
                preload_len
                    .checked_add(usize::try_from(payload_len).map_err(|_| Error::EntryTooLarge)?)
                    .ok_or(Error::EntryTooLarge)?,
            )
            .map_err(|_| Error::AllocationFailed)?;
        bytes.resize(preload_len, 0);
        if preload_len > 0 {
            let mut tree_file = File::open(self.dir_path.as_path())?;
            let preload_end = entry
                .preload_file_offset
                .checked_add(u64::from(entry.preload_len))
                .ok_or(Error::EntryOutOfBounds)?;
            if preload_end > self.data_start {
                return Err(Error::EntryOutOfBounds);
            }
            tree_file.seek(SeekFrom::Start(entry.preload_file_offset))?;
            tree_file.read_exact(&mut bytes[..preload_len])?;
        }
        let start = preload_len;
        let payload_len = usize::try_from(payload_len).map_err(|_| Error::EntryTooLarge)?;
        let total_len = start.checked_add(payload_len).ok_or(Error::EntryTooLarge)?;
        bytes.resize(total_len, 0);
        archive.seek(SeekFrom::Start(absolute_offset))?;
        archive.read_exact(&mut bytes[start..])?;

        let actual_crc = crc32(&bytes);
        if actual_crc != entry.crc32 {
            return Err(Error::EntryCrcMismatch {
                path: entry.path.clone(),
                expected: entry.crc32,
                actual: actual_crc,
            });
        }
        Ok(bytes)
    }

    fn archive_path(&self, index: u16) -> Result<PathBuf> {
        let parent = self
            .dir_path
            .parent()
            .ok_or(Error::InvalidHeader("directory archive has no parent"))?;
        Ok(parent.join(format!("{}_{index:03}.vpk", self.archive_prefix)))
    }
}

fn directory_archive_prefix(path: &Path) -> Result<String> {
    let stem = path
        .file_stem()
        .and_then(|value| value.to_str())
        .ok_or(Error::InvalidHeader("directory archive name is not UTF-8"))?;
    stem.strip_suffix("_dir")
        .map(str::to_owned)
        .ok_or(Error::InvalidHeader("expected a pakNN_dir.vpk file name"))
}

fn normalize_path(path: &str) -> String {
    path.replace('\\', "/").to_ascii_lowercase()
}

fn is_hero_portrait_parts(extension: &str, directory: &str, name: &str) -> bool {
    (directory == HERO_ROOT.trim_end_matches('/') || directory.starts_with(HERO_ROOT))
        && extension.eq_ignore_ascii_case("vtex_c")
        && name
            .get(.."npc_dota_hero_".len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("npc_dota_hero_"))
}

fn path_parts_match(target: &str, extension: &str, directory: &str, name: &str) -> bool {
    let (target_directory, file) = target.rsplit_once('/').unwrap_or(("", target));
    let (target_name, target_extension) = file.rsplit_once('.').unwrap_or((file, " "));
    directory == target_directory
        && name.eq_ignore_ascii_case(target_name)
        && extension.eq_ignore_ascii_case(target_extension)
}

struct BoundedReader<R> {
    inner: R,
    remaining: u64,
}

impl<R> BoundedReader<R> {
    fn new(inner: R, remaining: u64) -> Self {
        Self { inner, remaining }
    }
}

impl<R: Read> Read for BoundedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let allowed = self.remaining.min(buffer.len() as u64) as usize;
        let count = self.inner.read(&mut buffer[..allowed])?;
        self.remaining = self.remaining.saturating_sub(count as u64);
        Ok(count)
    }
}

struct CountingReader<R> {
    inner: R,
    bytes_read: u64,
}

impl<R> CountingReader<R> {
    fn new(inner: R) -> Self {
        Self {
            inner,
            bytes_read: 0,
        }
    }
}

impl<R: Read> Read for CountingReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let count = self.inner.read(buffer)?;
        self.bytes_read = self.bytes_read.saturating_add(count as u64);
        Ok(count)
    }
}

fn parse_tree<R, F>(
    reader: R,
    source_dir: &Arc<PathBuf>,
    data_size: u32,
    capture_hero_paths: bool,
    target_path: Option<&str>,
    mut visit: F,
) -> Result<()>
where
    R: Read,
    F: FnMut(Entry) -> Result<()>,
{
    let mut reader = BufReader::new(CountingReader::new(reader));
    let mut retained_entries = 0_usize;
    let mut retained_path_bytes = 0_usize;
    loop {
        let extension = read_tree_string(&mut reader, MAX_PATH_COMPONENT_BYTES)?;
        if extension.is_empty() {
            break;
        }
        validate_component(&extension)?;
        loop {
            let directory = read_tree_string(&mut reader, MAX_TREE_STRING_BYTES)?;
            if directory.is_empty() {
                break;
            }
            for component in directory.split(['/', '\\']) {
                validate_component(component)?;
            }
            // Normalize the shared prefix once, not once for every short record.
            let directory = if directory == " " {
                String::new()
            } else {
                normalize_path(&directory).trim_end_matches('/').to_owned()
            };
            loop {
                let name = read_tree_string(&mut reader, MAX_PATH_COMPONENT_BYTES)?;
                if name.is_empty() {
                    break;
                }
                validate_component(&name)?;
                let mut record = [0_u8; 18];
                reader
                    .read_exact(&mut record)
                    .map_err(|_| Error::InvalidTree("entry record truncated"))?;
                let crc = read_u32(&record, 0)?;
                let preload_len = read_u16(&record, 4)?;
                let archive_index = read_u16(&record, 6)?;
                let offset = read_u32(&record, 8)?;
                let length = read_u32(&record, 12)?;
                let terminator = read_u16(&record, 16)?;
                if terminator != 0xffff {
                    return Err(Error::InvalidTree("entry terminator mismatch"));
                }
                if archive_index > DIR_ARCHIVE_INDEX {
                    return Err(Error::InvalidTree("archive index exceeds the VPK range"));
                }
                let end = u64::from(offset)
                    .checked_add(u64::from(length))
                    .ok_or(Error::InvalidTree("entry range overflow"))?;
                if archive_index == DIR_ARCHIVE_INDEX && end > u64::from(data_size) {
                    return Err(Error::InvalidTree("directory entry exceeds data section"));
                }
                // Filter by the already bounded components before allocating a full path.
                let capture = (capture_hero_paths
                    && is_hero_portrait_parts(&extension, &directory, &name))
                    || target_path.is_some_and(|target| {
                        path_parts_match(target, &extension, &directory, &name)
                    });
                let consumed = reader
                    .get_ref()
                    .bytes_read
                    .checked_sub(reader.buffer().len() as u64)
                    .ok_or(Error::InvalidTree("tree cursor underflow"))?;
                let preload_file_offset = VPK_HEADER_BYTES
                    .checked_add(consumed)
                    .ok_or(Error::InvalidTree("preload offset overflow"))?;
                skip_exact(&mut reader, usize::from(preload_len))?;
                if capture {
                    if retained_entries >= MAX_RETAINED_ENTRIES {
                        return Err(Error::InvalidTree("retained entry limit exceeded"));
                    }
                    let path_len = composed_path_len(&extension, &directory, &name)?;
                    retained_path_bytes = path_len
                        .checked_mul(2)
                        .and_then(|bytes| retained_path_bytes.checked_add(bytes))
                        .filter(|bytes| *bytes <= MAX_RETAINED_PATH_BYTES)
                        .ok_or(Error::InvalidTree("retained path byte limit exceeded"))?;
                    retained_entries += 1;
                    let path = compose_path(&extension, &directory, &name, path_len)?;
                    visit(Entry {
                        source_dir: Arc::clone(source_dir),
                        path,
                        crc32: crc,
                        preload_file_offset,
                        preload_len,
                        archive_index,
                        offset,
                        length,
                    })?;
                }
            }
        }
    }
    Ok(())
}

fn composed_path_len(extension: &str, directory: &str, name: &str) -> Result<usize> {
    name.len()
        .checked_add(if directory.is_empty() {
            0
        } else {
            directory.len() + 1
        })
        .and_then(|len| {
            len.checked_add(if extension == " " {
                0
            } else {
                extension.len() + 1
            })
        })
        .ok_or(Error::InvalidTree("path length overflow"))
}

fn compose_path(extension: &str, directory: &str, name: &str, path_len: usize) -> Result<String> {
    let mut path = String::new();
    path.try_reserve_exact(path_len)
        .map_err(|_| Error::AllocationFailed)?;
    if !directory.is_empty() {
        path.push_str(directory);
        path.push('/');
    }
    path.push_str(name);
    if extension != " " {
        path.push('.');
        path.push_str(extension);
    }
    path.make_ascii_lowercase();
    Ok(path)
}

fn validate_component(component: &str) -> Result<()> {
    if component.len() > MAX_PATH_COMPONENT_BYTES || component.contains(['/', '\\']) {
        return Err(Error::InvalidTree("invalid or oversized path component"));
    }
    Ok(())
}

fn read_tree_string(reader: &mut impl Read, max_bytes: usize) -> Result<String> {
    let mut bytes = Vec::new();
    loop {
        let mut byte = [0_u8; 1];
        reader
            .read_exact(&mut byte)
            .map_err(|_| Error::InvalidTree("string or terminator truncated"))?;
        if byte[0] == 0 {
            break;
        }
        if bytes.len() >= max_bytes {
            return Err(Error::InvalidTree("tree string exceeds its byte limit"));
        }
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).map_err(|_| Error::InvalidTree("tree string is not UTF-8"))
}

fn skip_exact(reader: &mut impl Read, mut count: usize) -> Result<()> {
    let mut scratch = [0_u8; 8192];
    while count > 0 {
        let step = count.min(scratch.len());
        reader
            .read_exact(&mut scratch[..step])
            .map_err(|_| Error::InvalidTree("preload bytes truncated"))?;
        count -= step;
    }
    Ok(())
}

fn read_u16(bytes: &[u8], offset: usize) -> Result<u16> {
    let slice = bytes
        .get(
            offset
                ..offset
                    .checked_add(2)
                    .ok_or(Error::InvalidHeader("offset overflow"))?,
        )
        .ok_or(Error::InvalidHeader("truncated integer"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn read_u32(bytes: &[u8], offset: usize) -> Result<u32> {
    let slice = bytes
        .get(
            offset
                ..offset
                    .checked_add(4)
                    .ok_or(Error::InvalidHeader("offset overflow"))?,
        )
        .ok_or(Error::InvalidHeader("truncated integer"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

pub(crate) fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0_u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0_u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

#[cfg(test)]
mod tests {
    use super::{crc32, Error, Vpk, DIR_ARCHIVE_INDEX, VPK_HEADER_BYTES, VPK_SIGNATURE};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDir(PathBuf);

    impl TempDir {
        fn new() -> Self {
            let nonce = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0);
            static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
            let unique = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
            let path = std::env::temp_dir().join(format!(
                "kohbor-game-assets-{}-{nonce}-{unique}",
                std::process::id()
            ));
            fs::create_dir_all(&path).expect("create synthetic VPK directory");
            Self(path)
        }

        fn path(&self) -> &Path {
            &self.0
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn crc32_matches_the_standard_check_vector() {
        assert_eq!(crc32(b"123456789"), 0xcbf4_3926);
    }

    #[test]
    fn reads_preload_and_payload_from_external_and_directory_archives() {
        let temp = TempDir::new();
        let (directory, external) = synthetic_vpk();
        fs::write(temp.path().join("pak01_dir.vpk"), &directory)
            .expect("write synthetic directory archive");
        fs::write(temp.path().join("pak01_000.vpk"), &external)
            .expect("write synthetic numbered archive");

        let vpk = Vpk::open(temp.path().join("pak01_dir.vpk")).expect("open synthetic VPK");
        assert_eq!(vpk.indexed_portrait_count(), 1);
        assert_eq!(
            vpk.directory_tree_bytes() as usize,
            directory.len() - 28 - 5
        );
        let hero = vpk
            .find("PANORAMA\\IMAGES\\HEROES\\NPC_DOTA_HERO_LION_PNG.VTEX_C")
            .expect("find hero entry case-insensitively");
        assert_eq!(
            hero.path(),
            "panorama/images/heroes/npc_dota_hero_lion_png.vtex_c"
        );
        assert_eq!(vpk.read(&hero).expect("read external entry"), b"preraw");

        let local = vpk.find("readme.txt").expect("find directory-file entry");
        assert_eq!(vpk.read(&local).expect("read directory entry"), b"local");
    }

    #[test]
    fn rejects_truncated_and_corrupted_synthetic_directory_files() {
        let temp = TempDir::new();
        let (directory, external) = synthetic_vpk();
        let path = temp.path().join("pak01_dir.vpk");
        fs::write(temp.path().join("pak01_000.vpk"), external)
            .expect("write synthetic numbered archive");

        for end in 0..directory.len() {
            fs::write(&path, &directory[..end]).expect("write truncated synthetic VPK");
            assert!(Vpk::open(&path).is_err(), "accepted VPK prefix {end}");
        }

        let mut corrupt_tree = directory.clone();
        corrupt_tree[8..12].copy_from_slice(&1_u32.to_le_bytes());
        fs::write(&path, corrupt_tree).expect("write corrupt synthetic VPK");
        assert!(matches!(Vpk::open(&path), Err(Error::InvalidTree(_))));
    }

    #[test]
    fn reports_crc_and_archive_bounds_errors_as_values() {
        let temp = TempDir::new();
        let (directory, mut external) = synthetic_vpk();
        fs::write(temp.path().join("pak01_dir.vpk"), directory)
            .expect("write synthetic directory archive");
        let archive_path = temp.path().join("pak01_000.vpk");
        fs::write(&archive_path, &external).expect("write synthetic numbered archive");
        let vpk = Vpk::open(temp.path().join("pak01_dir.vpk")).expect("open synthetic VPK");
        let entry = vpk
            .find("panorama/images/heroes/npc_dota_hero_lion_png.vtex_c")
            .expect("find synthetic hero");

        external[3] ^= 1;
        fs::write(&archive_path, &external).expect("corrupt synthetic payload");
        assert!(matches!(
            vpk.read(&entry),
            Err(Error::EntryCrcMismatch { .. })
        ));

        fs::write(&archive_path, b"short").expect("truncate synthetic archive");
        assert!(matches!(vpk.read(&entry), Err(Error::EntryOutOfBounds)));
    }

    #[test]
    fn malformed_records_and_foreign_entries_are_rejected() {
        let temp = TempDir::new();
        let (directory, external) = synthetic_vpk();
        let path = temp.path().join("pak01_dir.vpk");
        fs::write(temp.path().join("pak01_000.vpk"), external).expect("synthetic archive");
        let record = 28 + b"txt\0 \0readme\0".len();
        for (offset, bytes) in [
            (record + 16, vec![0, 0]),
            (record + 6, vec![0xff, 0xff]),
            (record + 8, u32::MAX.to_le_bytes().to_vec()),
        ] {
            let mut corrupt = directory.clone();
            corrupt[offset..offset + bytes.len()].copy_from_slice(&bytes);
            fs::write(&path, corrupt).expect("synthetic malformed directory");
            assert!(Vpk::open(&path).is_err());
        }
        fs::write(&path, directory).expect("synthetic valid directory");
        let first = Vpk::open(&path).expect("first reader");
        let second = Vpk::open(&path).expect("second reader");
        let entry = first.find("readme.txt").expect("synthetic entry");
        assert!(matches!(
            second.read(&entry),
            Err(Error::EntryFromDifferentVpk)
        ));
    }

    #[test]
    fn index_rejects_repeated_long_prefix_amplification() {
        let temp = TempDir::new();
        let prefix = format!(
            "panorama/images/heroes/{}",
            vec!["x".repeat(250); 14].join("/")
        );
        let bytes = synthetic_index_vpk(&prefix, "vtex_c", 2_500, None);
        let path = temp.path().join("pak01_dir.vpk");
        fs::write(&path, bytes).expect("synthetic amplified index");
        assert!(matches!(
            Vpk::open(path),
            Err(Error::InvalidTree("retained path byte limit exceeded"))
        ));
    }

    #[test]
    fn index_rejects_excessive_entry_count() {
        let temp = TempDir::new();
        let bytes = synthetic_index_vpk("panorama/images/heroes", "vtex_c", 100_001, None);
        let path = temp.path().join("pak01_dir.vpk");
        fs::write(&path, bytes).expect("synthetic excessive index");
        assert!(matches!(
            Vpk::open(path),
            Err(Error::InvalidTree("retained entry limit exceeded"))
        ));
    }

    #[test]
    fn index_rejects_oversize_path_components() {
        let temp = TempDir::new();
        let path = temp.path().join("pak01_dir.vpk");
        let long = "x".repeat(256);
        for (directory, extension, name) in [
            (
                format!("panorama/images/heroes/{long}"),
                "vtex_c".to_owned(),
                None,
            ),
            ("panorama/images/heroes".to_owned(), long.clone(), None),
            (
                "panorama/images/heroes".to_owned(),
                "vtex_c".to_owned(),
                Some(long.as_str()),
            ),
        ] {
            fs::write(&path, synthetic_index_vpk(&directory, &extension, 1, name))
                .expect("synthetic long component");
            assert!(matches!(Vpk::open(&path), Err(Error::InvalidTree(_))));
        }
    }

    #[test]
    fn index_skips_unrelated_long_prefix_records() {
        let temp = TempDir::new();
        let prefix = format!(
            "panorama/images/heroes_other/{}",
            vec!["x".repeat(250); 14].join("/")
        );
        let path = temp.path().join("pak01_dir.vpk");
        fs::write(&path, synthetic_index_vpk(&prefix, "vtex_c", 2_500, None))
            .expect("synthetic unrelated records");
        let vpk =
            Vpk::open(&path).expect("unrelated records must not consume retained path budget");
        assert_eq!(vpk.indexed_portrait_count(), 0);
        assert!(vpk
            .find(&format!("{prefix}/npc_dota_hero_0.vtex_c"))
            .is_some());
    }

    fn synthetic_index_vpk(
        directory: &str,
        extension: &str,
        count: usize,
        name: Option<&str>,
    ) -> Vec<u8> {
        let mut tree = Vec::new();
        write_cstr(&mut tree, extension);
        write_cstr(&mut tree, directory);
        for index in 0..count {
            write_cstr(&mut tree, name.unwrap_or(&format!("npc_dota_hero_{index}")));
            write_record(&mut tree, &[], &[], 0, 0);
        }
        tree.extend_from_slice(&[0, 0, 0]);
        let mut bytes = vec![0; 28];
        bytes[..4].copy_from_slice(&VPK_SIGNATURE.to_le_bytes());
        bytes[4..8].copy_from_slice(&2_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&(tree.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&tree);
        bytes
    }

    fn synthetic_vpk() -> (Vec<u8>, Vec<u8>) {
        let local_payload = b"local";
        let external_preload = b"pre";
        let external_payload = b"raw";
        let mut tree = Vec::new();

        write_cstr(&mut tree, "txt");
        write_cstr(&mut tree, " ");
        write_cstr(&mut tree, "readme");
        write_record(&mut tree, local_payload, &[], DIR_ARCHIVE_INDEX, 0);
        write_cstr(&mut tree, "");
        write_cstr(&mut tree, "");

        write_cstr(&mut tree, "vtex_c");
        write_cstr(&mut tree, "panorama/images/heroes");
        write_cstr(&mut tree, "npc_dota_hero_lion_png");
        write_record(&mut tree, external_payload, external_preload, 0, 3);
        write_cstr(&mut tree, "");
        write_cstr(&mut tree, "");
        write_cstr(&mut tree, "");

        let mut directory = vec![0_u8; VPK_HEADER_BYTES as usize];
        directory[0..4].copy_from_slice(&VPK_SIGNATURE.to_le_bytes());
        directory[4..8].copy_from_slice(&2_u32.to_le_bytes());
        directory[8..12].copy_from_slice(&(tree.len() as u32).to_le_bytes());
        directory[12..16].copy_from_slice(&(local_payload.len() as u32).to_le_bytes());
        directory.extend_from_slice(&tree);
        directory.extend_from_slice(local_payload);

        let mut external = b"---".to_vec();
        external.extend_from_slice(external_payload);
        (directory, external)
    }

    fn write_record(
        tree: &mut Vec<u8>,
        payload: &[u8],
        preload: &[u8],
        archive_index: u16,
        offset: u32,
    ) {
        let mut whole = preload.to_vec();
        whole.extend_from_slice(payload);
        tree.extend_from_slice(&crc32(&whole).to_le_bytes());
        tree.extend_from_slice(&(preload.len() as u16).to_le_bytes());
        tree.extend_from_slice(&archive_index.to_le_bytes());
        tree.extend_from_slice(&offset.to_le_bytes());
        tree.extend_from_slice(&(payload.len() as u32).to_le_bytes());
        tree.extend_from_slice(&0xffff_u16.to_le_bytes());
        tree.extend_from_slice(preload);
    }

    fn write_cstr(target: &mut Vec<u8>, value: &str) {
        target.extend_from_slice(value.as_bytes());
        target.push(0);
    }
}
