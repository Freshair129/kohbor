//! Indexed ZIP container, format profile 1 (MEDIA-PACK-SPEC MP-03): strict
//! structure checks, bounded streaming extraction into staging, inventory
//! verification, media profile checks. Never writes outside the staging
//! directory it is given and never touches a registry.
//!
//! The container reader is this crate's own (not the `zip` crate): duplicate
//! records, extra fields, comments, hidden bytes and local/central header
//! disagreement must be rejected exactly. Inflate uses `flate2` only, always
//! through a bounded reader.

use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use flate2::read::DeflateDecoder;
use flate2::Crc;
use sha2::{Digest, Sha256};

use crate::catalog::Catalog;
use crate::descriptor::{self, ParsedDescriptor};
use crate::error::RejectReason;
use crate::fsutil::{has_unsafe_windows_segment, is_link_or_reparse_point, windows_path_key};
use crate::limits::Limits;
use crate::media::{self, WavInfo, WebmInfo};

const IO_CHUNK_BYTES: usize = 64 * 1024;
const EOCD_FIXED_BYTES: u64 = 22;
const EOCD_MAX_COMMENT_BYTES: u64 = u16::MAX as u64;
const CENTRAL_FIXED_BYTES: u64 = 46;
const LOCAL_FIXED_BYTES: u64 = 30;
const EOCD_SIGNATURE: &[u8] = b"PK\x05\x06";
const CENTRAL_SIGNATURE: &[u8] = b"PK\x01\x02";
const LOCAL_SIGNATURE: &[u8] = b"PK\x03\x04";
const ZIP64_EOCD_SIGNATURE: &[u8] = b"PK\x06\x06";
const ZIP64_LOCATOR_SIGNATURE: &[u8] = b"PK\x06\x07";

#[derive(Debug, Clone, Copy)]
struct EndRecord {
    offset: u64,
    disk_number: u16,
    central_disk: u16,
    disk_entries: u16,
    total_entries: u16,
    central_size: u32,
    central_offset: u32,
    comment_length: u16,
}

#[derive(Debug)]
struct ZipEntryRecord {
    raw_name: Vec<u8>,
    name: String,
    version_needed: u16,
    flags: u16,
    method: u16,
    time: u16,
    date: u16,
    crc32: u32,
    compressed_size: u32,
    expanded_size: u32,
    extra_length: u16,
    comment_length: u16,
    disk_start: u16,
    external_attributes: u32,
    local_offset: u32,
    data_offset: u64,
}

struct BoundedReader<R> {
    inner: R,
    remaining: u64,
}

impl<R> BoundedReader<R> {
    fn new(inner: R, limit: u64) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<R: Read> Read for BoundedReader<R> {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 || buffer.is_empty() {
            return Ok(0);
        }
        let requested = u64::try_from(buffer.len()).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bounded read length overflow")
        })?;
        let allowed = usize::try_from(self.remaining.min(requested)).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidInput, "bounded read length overflow")
        })?;
        let destination = buffer.get_mut(..allowed).ok_or_else(|| {
            io::Error::new(io::ErrorKind::InvalidInput, "bounded read exceeds buffer")
        })?;
        let count = self.inner.read(destination)?;
        let count = u64::try_from(count).map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "bounded read count overflow")
        })?;
        self.remaining = self.remaining.checked_sub(count).ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "bounded read count exceeded limit",
            )
        })?;
        usize::try_from(count)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "bounded read count overflow"))
    }
}

fn read_u16(bytes: &[u8], offset: usize) -> Option<u16> {
    let end = offset.checked_add(2)?;
    let value = bytes.get(offset..end)?;
    let array: [u8; 2] = value.try_into().ok()?;
    Some(u16::from_le_bytes(array))
}

fn read_u32(bytes: &[u8], offset: usize) -> Option<u32> {
    let end = offset.checked_add(4)?;
    let value = bytes.get(offset..end)?;
    let array: [u8; 4] = value.try_into().ok()?;
    Some(u32::from_le_bytes(array))
}

fn checked_end(offset: u64, length: u64) -> Result<u64, RejectReason> {
    offset.checked_add(length).ok_or(RejectReason::NotAZip)
}

fn read_exact_at(
    file: &mut File,
    offset: u64,
    buffer: &mut [u8],
    file_length: u64,
) -> Result<(), RejectReason> {
    let count = u64::try_from(buffer.len()).map_err(|_| RejectReason::NotAZip)?;
    let end = checked_end(offset, count)?;
    if end > file_length {
        return Err(RejectReason::NotAZip);
    }
    file.seek(SeekFrom::Start(offset))
        .map_err(|_| RejectReason::SourceUnavailable)?;
    match file.read_exact(buffer) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == io::ErrorKind::UnexpectedEof => Err(RejectReason::NotAZip),
        Err(_) => Err(RejectReason::SourceUnavailable),
    }
}

fn hash_open_file(file: &mut File, file_length: u64) -> Result<String, RejectReason> {
    file.seek(SeekFrom::Start(0))
        .map_err(|_| RejectReason::SourceUnavailable)?;
    let mut input = BoundedReader::new(file, file_length);
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; IO_CHUNK_BYTES];
    let mut read_total = 0u64;
    loop {
        let count = input
            .read(&mut buffer)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| RejectReason::NotAZip)?;
        read_total = read_total
            .checked_add(count_u64)
            .ok_or(RejectReason::NotAZip)?;
        let bytes = buffer.get(..count).ok_or(RejectReason::NotAZip)?;
        hasher.update(bytes);
    }
    if read_total != file_length {
        return Err(RejectReason::NotAZip);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn parse_end_record(bytes: &[u8], offset: u64) -> Option<EndRecord> {
    if bytes.get(..4)? != EOCD_SIGNATURE {
        return None;
    }
    Some(EndRecord {
        offset,
        disk_number: read_u16(bytes, 4)?,
        central_disk: read_u16(bytes, 6)?,
        disk_entries: read_u16(bytes, 8)?,
        total_entries: read_u16(bytes, 10)?,
        central_size: read_u32(bytes, 12)?,
        central_offset: read_u32(bytes, 16)?,
        comment_length: read_u16(bytes, 20)?,
    })
}

fn eocd_candidate_is_plausible(record: EndRecord) -> bool {
    record.disk_number == 0
        && record.central_disk == 0
        && record.disk_entries == record.total_entries
        && u64::from(record.central_offset)
            .checked_add(u64::from(record.central_size))
            .is_some_and(|central_end| central_end <= record.offset)
}

fn find_end_record(file: &mut File, file_length: u64) -> Result<EndRecord, RejectReason> {
    if file_length < EOCD_FIXED_BYTES {
        return Err(RejectReason::NotAZip);
    }

    let max_tail = EOCD_FIXED_BYTES
        .checked_add(EOCD_MAX_COMMENT_BYTES)
        .ok_or(RejectReason::NotAZip)?;
    let tail_length = file_length.min(max_tail);
    let tail_start = file_length
        .checked_sub(tail_length)
        .ok_or(RejectReason::NotAZip)?;
    let tail_size = usize::try_from(tail_length).map_err(|_| RejectReason::NotAZip)?;
    let mut tail = Vec::new();
    tail.try_reserve_exact(tail_size)
        .map_err(|_| RejectReason::NotAZip)?;
    tail.resize(tail_size, 0);
    read_exact_at(file, tail_start, &mut tail, file_length)?;

    if tail_size >= usize::try_from(EOCD_FIXED_BYTES).map_err(|_| RejectReason::NotAZip)? {
        let last_start = tail_size
            .checked_sub(usize::try_from(EOCD_FIXED_BYTES).map_err(|_| RejectReason::NotAZip)?)
            .ok_or(RejectReason::NotAZip)?;
        for index in (0..=last_start).rev() {
            let Some(fixed_end) = index.checked_add(22) else {
                continue;
            };
            let Some(fixed) = tail.get(index..fixed_end) else {
                continue;
            };
            let offset = tail_start
                .checked_add(u64::try_from(index).map_err(|_| RejectReason::NotAZip)?)
                .ok_or(RejectReason::NotAZip)?;
            let Some(record) = parse_end_record(fixed, offset) else {
                continue;
            };
            let end = checked_end(
                offset,
                EOCD_FIXED_BYTES
                    .checked_add(u64::from(record.comment_length))
                    .ok_or(RejectReason::NotAZip)?,
            )?;
            if !eocd_candidate_is_plausible(record) {
                continue;
            }
            if end < file_length {
                return Err(RejectReason::HiddenBytes);
            }
            if end > file_length {
                continue;
            }
            if record.comment_length != 0 {
                return Err(RejectReason::ArchiveComment);
            }
            return Ok(record);
        }
    }

    // A valid record followed by bytes is a hidden suffix. Search earlier blocks
    // only after the common exact-tail path has failed.
    let mut block_end = tail_start;
    let mut higher_prefix = Vec::new();
    let prefix_count = tail.len().min(21);
    if let Some(prefix) = tail.get(..prefix_count) {
        higher_prefix.extend_from_slice(prefix);
    }
    while block_end > 0 {
        let block_start = if block_end > IO_CHUNK_BYTES as u64 {
            block_end
                .checked_sub(IO_CHUNK_BYTES as u64)
                .ok_or(RejectReason::NotAZip)?
        } else {
            0
        };
        let block_length = block_end
            .checked_sub(block_start)
            .ok_or(RejectReason::NotAZip)?;
        let block_size = usize::try_from(block_length).map_err(|_| RejectReason::NotAZip)?;
        let mut block = Vec::new();
        block
            .try_reserve_exact(block_size)
            .map_err(|_| RejectReason::NotAZip)?;
        block.resize(block_size, 0);
        read_exact_at(file, block_start, &mut block, file_length)?;
        let mut combined = block;
        combined.extend_from_slice(&higher_prefix);

        for index in (0..combined.len()).rev() {
            if index >= block_size {
                continue;
            }
            let Some(fixed_end) = index.checked_add(22) else {
                continue;
            };
            let Some(fixed) = combined.get(index..fixed_end) else {
                continue;
            };
            let offset = block_start
                .checked_add(u64::try_from(index).map_err(|_| RejectReason::NotAZip)?)
                .ok_or(RejectReason::NotAZip)?;
            let Some(record) = parse_end_record(fixed, offset) else {
                continue;
            };
            let record_end = checked_end(
                offset,
                EOCD_FIXED_BYTES
                    .checked_add(u64::from(record.comment_length))
                    .ok_or(RejectReason::NotAZip)?,
            )?;
            if record_end < file_length && eocd_candidate_is_plausible(record) {
                return Err(RejectReason::HiddenBytes);
            }
        }

        higher_prefix.clear();
        let prefix_count = combined.len().min(21);
        if let Some(prefix) = combined.get(..prefix_count) {
            higher_prefix.extend_from_slice(prefix);
        }
        block_end = block_start;
    }
    Err(RejectReason::NotAZip)
}

fn parse_central_directory(
    file: &mut File,
    end_record: EndRecord,
    file_length: u64,
    entry_limit: usize,
) -> Result<(Vec<ZipEntryRecord>, u64), RejectReason> {
    if end_record.disk_number != 0
        || end_record.central_disk != 0
        || end_record.disk_entries != end_record.total_entries
    {
        return Err(RejectReason::NotAZip);
    }
    if end_record.total_entries == u16::MAX
        || end_record.central_size == u32::MAX
        || end_record.central_offset == u32::MAX
    {
        return Err(RejectReason::NotAZip);
    }
    if usize::from(end_record.total_entries) > entry_limit {
        return Err(RejectReason::TooManyEntries);
    }
    let central_start = u64::from(end_record.central_offset);
    let central_size = u64::from(end_record.central_size);
    let central_end = checked_end(central_start, central_size)?;
    if central_end > file_length {
        return Err(RejectReason::NotAZip);
    }
    if central_end != end_record.offset {
        if central_end < end_record.offset {
            if central_end
                .checked_add(4)
                .is_some_and(|value| value <= end_record.offset)
            {
                let mut marker = [0u8; 4];
                read_exact_at(file, central_end, &mut marker, file_length)?;
                if marker.as_slice() == ZIP64_EOCD_SIGNATURE {
                    return Err(RejectReason::NotAZip);
                }
            }
            if let Some(locator_offset) = end_record.offset.checked_sub(20) {
                if locator_offset >= central_end
                    && locator_offset
                        .checked_add(4)
                        .is_some_and(|value| value <= end_record.offset)
                {
                    let mut marker = [0u8; 4];
                    read_exact_at(file, locator_offset, &mut marker, file_length)?;
                    if marker.as_slice() == ZIP64_LOCATOR_SIGNATURE {
                        return Err(RejectReason::NotAZip);
                    }
                }
            }
            return Err(RejectReason::HiddenBytes);
        }
        return Err(RejectReason::NotAZip);
    }

    let expected_count = usize::from(end_record.total_entries);
    let mut entries = Vec::new();
    entries
        .try_reserve_exact(expected_count)
        .map_err(|_| RejectReason::NotAZip)?;
    let mut cursor = central_start;
    for _ in 0..expected_count {
        let fixed_end = checked_end(cursor, CENTRAL_FIXED_BYTES)?;
        if fixed_end > central_end {
            return Err(RejectReason::NotAZip);
        }
        let mut fixed = [0u8; 46];
        read_exact_at(file, cursor, &mut fixed, file_length)?;
        if fixed.get(..4) != Some(CENTRAL_SIGNATURE) {
            return Err(RejectReason::NotAZip);
        }
        let version_needed = read_u16(&fixed, 6).ok_or(RejectReason::NotAZip)?;
        let flags = read_u16(&fixed, 8).ok_or(RejectReason::NotAZip)?;
        let method = read_u16(&fixed, 10).ok_or(RejectReason::NotAZip)?;
        let time = read_u16(&fixed, 12).ok_or(RejectReason::NotAZip)?;
        let date = read_u16(&fixed, 14).ok_or(RejectReason::NotAZip)?;
        let crc32 = read_u32(&fixed, 16).ok_or(RejectReason::NotAZip)?;
        let compressed_size = read_u32(&fixed, 20).ok_or(RejectReason::NotAZip)?;
        let expanded_size = read_u32(&fixed, 24).ok_or(RejectReason::NotAZip)?;
        let name_length = read_u16(&fixed, 28).ok_or(RejectReason::NotAZip)?;
        let extra_length = read_u16(&fixed, 30).ok_or(RejectReason::NotAZip)?;
        let comment_length = read_u16(&fixed, 32).ok_or(RejectReason::NotAZip)?;
        let disk_start = read_u16(&fixed, 34).ok_or(RejectReason::NotAZip)?;
        let external_attributes = read_u32(&fixed, 38).ok_or(RejectReason::NotAZip)?;
        let local_offset = read_u32(&fixed, 42).ok_or(RejectReason::NotAZip)?;

        let variable_length = u64::from(name_length)
            .checked_add(u64::from(extra_length))
            .and_then(|length| length.checked_add(u64::from(comment_length)))
            .ok_or(RejectReason::NotAZip)?;
        let record_end = checked_end(fixed_end, variable_length)?;
        if record_end > central_end {
            return Err(RejectReason::NotAZip);
        }
        let name_length_usize = usize::from(name_length);
        let mut raw_name = Vec::new();
        raw_name
            .try_reserve_exact(name_length_usize)
            .map_err(|_| RejectReason::NotAZip)?;
        raw_name.resize(name_length_usize, 0);
        read_exact_at(file, fixed_end, &mut raw_name, file_length)?;

        entries.push(ZipEntryRecord {
            raw_name,
            name: String::new(),
            version_needed,
            flags,
            method,
            time,
            date,
            crc32,
            compressed_size,
            expanded_size,
            extra_length,
            comment_length,
            disk_start,
            external_attributes,
            local_offset,
            data_offset: 0,
        });
        cursor = record_end;
    }
    if cursor != central_end {
        return Err(RejectReason::HiddenBytes);
    }
    Ok((entries, central_start))
}

fn validate_archive_path(raw_name: &[u8]) -> Result<String, RejectReason> {
    if raw_name.is_empty() || raw_name.len() > 240 || !raw_name.is_ascii() {
        return Err(RejectReason::UnsafePath);
    }
    let name = std::str::from_utf8(raw_name).map_err(|_| RejectReason::UnsafePath)?;
    if has_unsafe_windows_segment(name) {
        return Err(RejectReason::UnsafePath);
    }
    let mut bytes = raw_name.iter();
    let first = bytes.next().ok_or(RejectReason::UnsafePath)?;
    if !(first.is_ascii_alphanumeric() || *first == b'_')
        || !bytes
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(*byte, b'_' | b'.' | b'/' | b'-'))
    {
        return Err(RejectReason::UnsafePath);
    }
    for segment in name.split('/') {
        if segment.is_empty() || segment == "." || segment == ".." || segment.starts_with('.') {
            return Err(RejectReason::UnsafePath);
        }
    }
    Ok(name.to_owned())
}

fn validate_central_entries(entries: &mut [ZipEntryRecord]) -> Result<(), RejectReason> {
    let Some(first) = entries.first() else {
        return Err(RejectReason::DescriptorNotFirst);
    };
    if first.raw_name.as_slice() != b"descriptor.json" {
        return Err(RejectReason::DescriptorNotFirst);
    }

    for entry in entries.iter() {
        if entry.time != 0 || entry.date != 33 || entry.external_attributes != (0o100644u32 << 16) {
            return Err(RejectReason::HeaderMismatch);
        }
    }

    for entry in entries.iter_mut() {
        entry.name = validate_archive_path(&entry.raw_name)?;
    }

    for index in 0..entries.len() {
        let Some(entry) = entries.get(index) else {
            return Err(RejectReason::NotAZip);
        };
        if entries
            .iter()
            .take(index)
            .any(|previous| windows_path_key(&previous.name) == windows_path_key(&entry.name))
        {
            return Err(RejectReason::DuplicateEntry);
        }
    }

    for pair in entries.windows(2).skip(1) {
        let Some(previous) = pair.first() else {
            return Err(RejectReason::NotAZip);
        };
        let Some(next) = pair.get(1) else {
            return Err(RejectReason::NotAZip);
        };
        if previous.raw_name > next.raw_name {
            return Err(RejectReason::HeaderMismatch);
        }
    }
    Ok(())
}

fn validate_local_headers(
    file: &mut File,
    entries: &mut [ZipEntryRecord],
    central_start: u64,
    file_length: u64,
) -> Result<(), RejectReason> {
    let mut expected_offset = 0u64;
    for entry in entries.iter_mut() {
        if entry.disk_start != 0 {
            return Err(RejectReason::NotAZip);
        }
        if entry.compressed_size == u32::MAX
            || entry.expanded_size == u32::MAX
            || entry.local_offset == u32::MAX
            || entry.version_needed >= 45
        {
            return Err(RejectReason::NotAZip);
        }
        if u64::from(entry.local_offset) != expected_offset {
            return Err(RejectReason::HiddenBytes);
        }
        let local_offset = u64::from(entry.local_offset);
        let fixed_end = checked_end(local_offset, LOCAL_FIXED_BYTES)?;
        if fixed_end > file_length {
            return Err(RejectReason::NotAZip);
        }
        if fixed_end > central_start {
            return Err(RejectReason::HiddenBytes);
        }
        let mut fixed = [0u8; 30];
        read_exact_at(file, local_offset, &mut fixed, file_length)?;
        if fixed.get(..4) != Some(LOCAL_SIGNATURE) {
            return Err(RejectReason::NotAZip);
        }
        let version_needed = read_u16(&fixed, 4).ok_or(RejectReason::NotAZip)?;
        let flags = read_u16(&fixed, 6).ok_or(RejectReason::NotAZip)?;
        let method = read_u16(&fixed, 8).ok_or(RejectReason::NotAZip)?;
        let time = read_u16(&fixed, 10).ok_or(RejectReason::NotAZip)?;
        let date = read_u16(&fixed, 12).ok_or(RejectReason::NotAZip)?;
        let crc32 = read_u32(&fixed, 14).ok_or(RejectReason::NotAZip)?;
        let compressed_size = read_u32(&fixed, 18).ok_or(RejectReason::NotAZip)?;
        let expanded_size = read_u32(&fixed, 22).ok_or(RejectReason::NotAZip)?;
        let name_length = read_u16(&fixed, 26).ok_or(RejectReason::NotAZip)?;
        let extra_length = read_u16(&fixed, 28).ok_or(RejectReason::NotAZip)?;
        if compressed_size == u32::MAX || expanded_size == u32::MAX || version_needed >= 45 {
            return Err(RejectReason::NotAZip);
        }

        let variable_start = fixed_end;
        let local_name_end = checked_end(variable_start, u64::from(name_length))?;
        let variable_end = checked_end(local_name_end, u64::from(extra_length))?;
        if variable_end > file_length {
            return Err(RejectReason::NotAZip);
        }
        if variable_end > central_start {
            return Err(RejectReason::HiddenBytes);
        }
        let name_length_usize = usize::from(name_length);
        let mut local_name = Vec::new();
        local_name
            .try_reserve_exact(name_length_usize)
            .map_err(|_| RejectReason::NotAZip)?;
        local_name.resize(name_length_usize, 0);
        read_exact_at(file, variable_start, &mut local_name, file_length)?;

        if extra_length != 0 {
            return Err(RejectReason::UnexpectedExtraField);
        }
        if local_name != entry.raw_name
            || usize::from(name_length) != entry.raw_name.len()
            || version_needed != entry.version_needed
            || flags != entry.flags
            || method != entry.method
            || time != entry.time
            || date != entry.date
            || crc32 != entry.crc32
            || compressed_size != entry.compressed_size
            || expanded_size != entry.expanded_size
        {
            return Err(RejectReason::HeaderMismatch);
        }
        if entry.flags & 0x0008 != 0 {
            return Err(RejectReason::HeaderMismatch);
        }

        let data_end = checked_end(variable_end, u64::from(entry.compressed_size))?;
        if data_end > file_length {
            return Err(RejectReason::NotAZip);
        }
        if data_end > central_start {
            return Err(RejectReason::HiddenBytes);
        }
        entry.data_offset = variable_end;
        expected_offset = data_end;
    }
    if expected_offset != central_start {
        return Err(RejectReason::HiddenBytes);
    }
    Ok(())
}

fn validate_entry_profile(entries: &[ZipEntryRecord]) -> Result<(), RejectReason> {
    for entry in entries {
        if entry.flags & (0x0001 | 0x0040 | 0x2000) != 0 {
            return Err(RejectReason::EncryptedEntry);
        }
        if entry.method != 0 && entry.method != 8 {
            return Err(RejectReason::UnsupportedCompression);
        }
        if entry.extra_length != 0 || entry.comment_length != 0 {
            return Err(RejectReason::UnexpectedExtraField);
        }
        if entry.version_needed >= 45 {
            return Err(RejectReason::NotAZip);
        }
        if entry.version_needed > 20 {
            return Err(RejectReason::HeaderMismatch);
        }
    }
    Ok(())
}

fn inspect_existing_parent(path: &Path) -> Result<bool, RejectReason> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            if is_link_or_reparse_point(&metadata) || !metadata.is_dir() {
                return Err(RejectReason::DuplicateEntry);
            }
            Ok(true)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(RejectReason::SourceUnavailable),
    }
}

fn check_staging_destination(staging_dir: &Path, relative: &str) -> Result<(), RejectReason> {
    let mut current = staging_dir.to_path_buf();
    let mut segments = relative.split('/').peekable();
    while let Some(segment) = segments.next() {
        current.push(segment);
        if segments.peek().is_some() {
            if !inspect_existing_parent(&current)? {
                return Ok(());
            }
        } else {
            match fs::symlink_metadata(&current) {
                Ok(_) => return Err(RejectReason::DuplicateEntry),
                Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
                Err(_) => return Err(RejectReason::SourceUnavailable),
            }
        }
    }
    Err(RejectReason::UnsafePath)
}

fn create_staging_file(staging_dir: &Path, relative: &str) -> Result<File, RejectReason> {
    let mut current = staging_dir.to_path_buf();
    let mut segments = relative.split('/').peekable();
    while let Some(segment) = segments.next() {
        current.push(segment);
        if segments.peek().is_some() {
            match inspect_existing_parent(&current)? {
                true => {}
                false => match fs::create_dir(&current) {
                    Ok(()) => {}
                    Err(error) if error.kind() == io::ErrorKind::AlreadyExists => {
                        if !inspect_existing_parent(&current)? {
                            return Err(RejectReason::SourceUnavailable);
                        }
                    }
                    Err(_) => return Err(RejectReason::SourceUnavailable),
                },
            }
        } else {
            return OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&current)
                .map_err(|error| {
                    if error.kind() == io::ErrorKind::AlreadyExists {
                        RejectReason::DuplicateEntry
                    } else {
                        RejectReason::SourceUnavailable
                    }
                });
        }
    }
    Err(RejectReason::UnsafePath)
}

struct ExpandedResult {
    size_bytes: u64,
    sha256: String,
    crc32: u32,
    captured: Option<Vec<u8>>,
}

fn copy_expanded<R: Read>(
    reader: &mut R,
    destination: &mut File,
    entry_limit: u64,
    declared_size: u64,
    aggregate: &mut u64,
    aggregate_limit: u64,
    capture: bool,
) -> Result<ExpandedResult, RejectReason> {
    let mut hasher = Sha256::new();
    let mut crc = Crc::new();
    let mut size_bytes = 0u64;
    let mut captured = if capture { Some(Vec::new()) } else { None };
    let mut buffer = [0u8; IO_CHUNK_BYTES];

    loop {
        let entry_remaining = entry_limit
            .checked_sub(size_bytes)
            .ok_or(RejectReason::EntryTooLarge)?;
        let aggregate_remaining = aggregate_limit
            .checked_sub(*aggregate)
            .ok_or(RejectReason::ExpansionLimit)?;
        let allowed = entry_remaining.min(aggregate_remaining);
        if allowed == 0 {
            let mut probe = [0u8; 1];
            let count = reader.read(&mut probe).map_err(map_inflate_error)?;
            if count == 0 {
                break;
            }
            if entry_remaining == 0 {
                return Err(RejectReason::EntryTooLarge);
            }
            return Err(RejectReason::ExpansionLimit);
        }

        let capacity = usize::try_from(allowed.min(IO_CHUNK_BYTES as u64))
            .map_err(|_| RejectReason::ExpansionLimit)?;
        let output = buffer
            .get_mut(..capacity)
            .ok_or(RejectReason::ExpansionLimit)?;
        let count = reader.read(output).map_err(map_inflate_error)?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| RejectReason::ExpansionLimit)?;
        let new_entry_size = size_bytes
            .checked_add(count_u64)
            .ok_or(RejectReason::EntryTooLarge)?;
        let new_aggregate = aggregate
            .checked_add(count_u64)
            .ok_or(RejectReason::ExpansionLimit)?;
        if new_entry_size > entry_limit {
            return Err(RejectReason::EntryTooLarge);
        }
        if new_aggregate > aggregate_limit {
            return Err(RejectReason::ExpansionLimit);
        }
        let bytes = buffer.get(..count).ok_or(RejectReason::ExpansionLimit)?;
        destination
            .write_all(bytes)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        hasher.update(bytes);
        crc.update(bytes);
        if let Some(contents) = captured.as_mut() {
            contents
                .try_reserve(count)
                .map_err(|_| RejectReason::EntryTooLarge)?;
            contents.extend_from_slice(bytes);
        }
        size_bytes = new_entry_size;
        *aggregate = new_aggregate;
    }

    if size_bytes != declared_size {
        return Err(RejectReason::ExpansionLimit);
    }
    Ok(ExpandedResult {
        size_bytes,
        sha256: format!("{:x}", hasher.finalize()),
        crc32: crc.sum(),
        captured,
    })
}

fn map_inflate_error(error: io::Error) -> RejectReason {
    match error.kind() {
        io::ErrorKind::UnexpectedEof | io::ErrorKind::InvalidData | io::ErrorKind::InvalidInput => {
            RejectReason::NotAZip
        }
        _ => RejectReason::SourceUnavailable,
    }
}

fn extract_entry(
    source: &mut File,
    staging_dir: &Path,
    entry: &ZipEntryRecord,
    is_descriptor: bool,
    limits: Limits,
    aggregate: &mut u64,
) -> Result<ExpandedResult, StageError> {
    let entry_limit = if is_descriptor {
        limits.descriptor_bytes
    } else {
        limits.payload_bytes
    };
    let declared_size = u64::from(entry.expanded_size);
    if declared_size > entry_limit {
        return Err(RejectReason::EntryTooLarge.into());
    }
    let mut destination = create_staging_file(staging_dir, &entry.name)?;
    let extracted = (|| -> Result<ExpandedResult, RejectReason> {
        source
            .seek(SeekFrom::Start(entry.data_offset))
            .map_err(|_| RejectReason::SourceUnavailable)?;
        let compressed_size = u64::from(entry.compressed_size);
        let result = if entry.method == 0 {
            let mut bounded = BoundedReader::new(source, compressed_size);
            copy_expanded(
                &mut bounded,
                &mut destination,
                entry_limit,
                declared_size,
                aggregate,
                limits.expanded_bytes,
                is_descriptor,
            )?
        } else if entry.method == 8 {
            let bounded = BoundedReader::new(source, compressed_size);
            let mut decoder = DeflateDecoder::new(bounded);
            let result = copy_expanded(
                &mut decoder,
                &mut destination,
                entry_limit,
                declared_size,
                aggregate,
                limits.expanded_bytes,
                is_descriptor,
            )?;
            if decoder.total_in() != compressed_size {
                return Err(RejectReason::NotAZip);
            }
            result
        } else {
            return Err(RejectReason::UnsupportedCompression);
        };
        if result.crc32 != entry.crc32 {
            return Err(RejectReason::HeaderMismatch);
        }
        Ok(result)
    })();
    destination.sync_all().map_err(StageError::Io)?;
    extracted.map_err(StageError::Rejected)
}

#[derive(Debug)]
pub(crate) enum StageError {
    Rejected(RejectReason),
    Io(io::Error),
}

impl From<RejectReason> for StageError {
    fn from(reason: RejectReason) -> Self {
        StageError::Rejected(reason)
    }
}

fn read_staged_media(
    staging_dir: &Path,
    relative: &str,
    limit: u64,
) -> Result<Vec<u8>, RejectReason> {
    let path = staged_path(staging_dir, relative);
    let root_metadata =
        fs::symlink_metadata(staging_dir).map_err(|_| RejectReason::SourceUnavailable)?;
    if is_link_or_reparse_point(&root_metadata) || !root_metadata.is_dir() {
        return Err(RejectReason::SourceUnavailable);
    }
    let components = relative.split('/').collect::<Vec<_>>();
    let mut current = staging_dir.to_path_buf();
    let mut metadata = root_metadata;
    for (index, segment) in components.iter().enumerate() {
        current.push(segment);
        metadata = fs::symlink_metadata(&current).map_err(|_| RejectReason::SourceUnavailable)?;
        if is_link_or_reparse_point(&metadata) {
            return Err(RejectReason::SourceUnavailable);
        }
        if index + 1 < components.len() && !metadata.is_dir() {
            return Err(RejectReason::SourceUnavailable);
        }
    }
    if !metadata.is_file() {
        return Err(RejectReason::SourceUnavailable);
    }
    if metadata.len() > limit {
        return Err(RejectReason::EntryTooLarge);
    }
    let capacity = usize::try_from(metadata.len()).map_err(|_| RejectReason::EntryTooLarge)?;
    let mut contents = Vec::new();
    contents
        .try_reserve_exact(capacity)
        .map_err(|_| RejectReason::EntryTooLarge)?;
    let mut file = File::open(path).map_err(|_| RejectReason::SourceUnavailable)?;
    let mut buffer = [0u8; IO_CHUNK_BYTES];
    let mut total = 0u64;
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        if count == 0 {
            break;
        }
        let count_u64 = u64::try_from(count).map_err(|_| RejectReason::EntryTooLarge)?;
        total = total
            .checked_add(count_u64)
            .ok_or(RejectReason::EntryTooLarge)?;
        if total > limit {
            return Err(RejectReason::EntryTooLarge);
        }
        let bytes = buffer.get(..count).ok_or(RejectReason::EntryTooLarge)?;
        contents.extend_from_slice(bytes);
    }
    if total != metadata.len() {
        return Err(RejectReason::SourceUnavailable);
    }
    Ok(contents)
}

fn inspect_media(
    parsed: &ParsedDescriptor,
    staging_dir: &Path,
    limits: Limits,
) -> Result<(), RejectReason> {
    let mut wav_cache: HashMap<String, WavInfo> = HashMap::new();
    let mut webm_cache: HashMap<String, WebmInfo> = HashMap::new();
    for binding in &parsed.descriptor.bindings {
        let mixed_info = if let Some(info) = wav_cache.get(&binding.mixed_audio_path) {
            *info
        } else {
            let bytes =
                read_staged_media(staging_dir, &binding.mixed_audio_path, limits.payload_bytes)?;
            let info = media::inspect_wav(&bytes, RejectReason::MixedAudioInvalid)?;
            wav_cache.insert(binding.mixed_audio_path.clone(), info);
            info
        };
        let identity_info = if let Some(path) = binding.identity_audio_path.as_deref() {
            Some(if let Some(info) = wav_cache.get(path) {
                *info
            } else {
                let bytes = read_staged_media(staging_dir, path, limits.payload_bytes)?;
                let info = media::inspect_wav(&bytes, RejectReason::IdentityAudioInvalid)?;
                wav_cache.insert(path.to_owned(), info);
                info
            })
        } else {
            None
        };
        if mixed_info.duration_ms != binding.duration_ms {
            return Err(RejectReason::AudioDurationMismatch);
        }
        if identity_info.is_some_and(|info| info.samples != mixed_info.samples) {
            return Err(RejectReason::IdentityAudioLengthMismatch);
        }

        let visual_info = if let Some(info) = webm_cache.get(&binding.silent_visual_path) {
            *info
        } else {
            let bytes = read_staged_media(
                staging_dir,
                &binding.silent_visual_path,
                limits.payload_bytes,
            )?;
            let info = media::inspect_webm(&bytes)?;
            webm_cache.insert(binding.silent_visual_path.clone(), info);
            info
        };
        if let Some(duration_ms) = visual_info.duration_ms {
            let minimum = u64::from(binding.duration_ms).saturating_sub(34);
            let maximum = u64::from(binding.duration_ms)
                .checked_add(u64::from(binding.visual_tail_ms))
                .and_then(|value| value.checked_add(34))
                .ok_or(RejectReason::VisualDurationMismatch)?;
            if duration_ms < minimum || duration_ms > maximum {
                return Err(RejectReason::VisualDurationMismatch);
            }
        }
    }
    Ok(())
}

/// One verified payload in the staging directory.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedFile {
    /// Path inside the package (also the relative path under the staging dir).
    pub path: String,
    pub size_bytes: u64,
    pub sha256: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StagedPackage {
    pub parsed: ParsedDescriptor,
    /// SHA-256 of the whole container, computed here, never read from the package.
    pub container_sha256: String,
    /// `descriptor.json` is written to the staging dir root; payloads keep their paths.
    pub files: Vec<StagedFile>,
}

/// Read, validate and stage the container at `source` into `staging_dir`
/// (created by the caller, empty). All checks complete before the function
/// returns `Ok`; on `Err` the caller discards the staging directory (this
/// function may leave partial files in it, never elsewhere).
///
/// Container rules (each failure is its own `RejectReason`):
/// - size > `limits.container_bytes` -> `ContainerTooLarge` (checked from the
///   file size before reading); not a ZIP / no end record / truncated -> `NotAZip`;
///   bytes before the first local header, gaps between records, or anything after
///   the end record -> `HiddenBytes`; a non-empty archive comment -> `ArchiveComment`.
/// - entry count > `limits.payload_count + 1` -> `TooManyEntries`.
/// - first entry is not `descriptor.json` -> `DescriptorNotFirst`; the entries
///   after it must be in ASCII byte order of their names, otherwise
///   `HeaderMismatch`; timestamps other than 1980-01-01 00:00:00 or permissions
///   other than regular-file 0644 are also `HeaderMismatch`.
/// - duplicate record names (exact or case-insensitive) -> `DuplicateEntry`;
///   unsafe name (see `descriptor::parse_descriptor` step 4, also absolute,
///   UNC, backslash, `..`, trailing slash/directory entries) -> `UnsafePath`.
/// - general-purpose flag bit 0 (encryption) or a strong-encryption flag ->
///   `EncryptedEntry`; method other than 0 (Stored) or 8 (Deflate) ->
///   `UnsupportedCompression`; a non-empty extra field or per-entry comment ->
///   `UnexpectedExtraField`; local header name/sizes/method/CRC/flags disagreeing
///   with the central directory -> `HeaderMismatch`; ZIP64 structures are not
///   admitted (`NotAZip`).
/// - Streaming extraction caps: an entry's declared or actual expanded size >
///   `limits.payload_bytes` (descriptor: `limits.descriptor_bytes`) -> `EntryTooLarge`;
///   the aggregate of all expanded bytes > `limits.expanded_bytes`, or an entry
///   whose actual expanded length differs from its header while inflating
///   (decompression bomb) -> `ExpansionLimit`. Inflate stops at the cap; it never
///   buffers an entry wholly in memory beyond the media inspection step.
/// - Descriptor: `parse_descriptor` on entry 0.
/// - Inventory: every `files[]` entry must exist in the container
///   (`DanglingReference`), every non-descriptor container entry must be in
///   `files[]` (`UndeclaredPayload`), the actual expanded size must equal
///   `size_bytes` (`SizeMismatch`) and the SHA-256 must equal `sha256`
///   (`DigestMismatch`). Roles `source_asset` and `font` are inventoried but not
///   media-inspected.
/// - Media (EVENT-AUDIO-SPEC section 6): `inspect_wav` on every binding's mixed
///   audio (`MixedAudioInvalid`) and identity audio (`IdentityAudioInvalid`);
///   mixed `duration_ms` (floor of samples * 1000 / 48,000) must equal the
///   binding's `duration_ms` (`AudioDurationMismatch`); identity sample count
///   must equal the mixed sample count (`IdentityAudioLengthMismatch`);
///   `inspect_webm` on the silent visual (`VisualInvalid`, `VisualHasAudioTrack`);
///   when the visual declares a duration it must lie in
///   `[duration_ms, duration_ms + visual_tail_ms]` plus 34 ms (one frame at
///   30 fps) tolerance on both sides (`VisualDurationMismatch`); a binding with
///   an inert event id is still verified.
///
/// A corrupt mixed audio is never replaced by a valid identity audio: the pair
/// is rejected (MEDIA-004).
pub fn stage_package(
    source: &Path,
    staging_dir: &Path,
    catalog: &Catalog,
    limits: &Limits,
) -> Result<StagedPackage, RejectReason> {
    stage_package_inner(source, staging_dir, catalog, limits).map_err(|error| match error {
        StageError::Rejected(reason) => reason,
        StageError::Io(_) => RejectReason::SourceUnavailable,
    })
}

pub(crate) fn stage_package_for_store(
    source: &Path,
    staging_dir: &Path,
    catalog: &Catalog,
    limits: &Limits,
) -> Result<StagedPackage, StageError> {
    stage_package_inner(source, staging_dir, catalog, limits)
}

fn stage_package_inner(
    source: &Path,
    staging_dir: &Path,
    catalog: &Catalog,
    limits: &Limits,
) -> Result<StagedPackage, StageError> {
    let limits = limits.clamped();
    let mut source_file = File::open(source).map_err(|_| RejectReason::SourceUnavailable)?;
    let metadata = source_file
        .metadata()
        .map_err(|_| RejectReason::SourceUnavailable)?;
    let file_length = metadata.len();
    if file_length > limits.container_bytes {
        return Err(RejectReason::ContainerTooLarge.into());
    }

    let root_metadata =
        fs::symlink_metadata(staging_dir).map_err(|_| RejectReason::SourceUnavailable)?;
    if is_link_or_reparse_point(&root_metadata) || !root_metadata.is_dir() {
        return Err(RejectReason::SourceUnavailable.into());
    }

    let container_sha256 = hash_open_file(&mut source_file, file_length)?;
    let end_record = find_end_record(&mut source_file, file_length)?;
    if end_record.comment_length != 0 {
        return Err(RejectReason::ArchiveComment.into());
    }
    let maximum_entries = limits
        .payload_count
        .checked_add(1)
        .ok_or(RejectReason::TooManyEntries)?;
    let (mut entries, central_start) =
        parse_central_directory(&mut source_file, end_record, file_length, maximum_entries)?;
    validate_central_entries(&mut entries)?;
    validate_entry_profile(&entries)?;
    validate_local_headers(&mut source_file, &mut entries, central_start, file_length)?;

    for (index, entry) in entries.iter().enumerate() {
        let entry_limit = if index == 0 {
            limits.descriptor_bytes
        } else {
            limits.payload_bytes
        };
        if u64::from(entry.expanded_size) > entry_limit {
            return Err(RejectReason::EntryTooLarge.into());
        }
    }

    for entry in &entries {
        check_staging_destination(staging_dir, &entry.name)?;
    }

    let mut expanded_total = 0u64;
    let first = entries.first().ok_or(RejectReason::DescriptorNotFirst)?;
    let descriptor_result = extract_entry(
        &mut source_file,
        staging_dir,
        first,
        true,
        limits,
        &mut expanded_total,
    )?;
    let descriptor_bytes = descriptor_result
        .captured
        .ok_or(RejectReason::DescriptorSyntax)?;
    let parsed = descriptor::parse_descriptor(&descriptor_bytes, catalog, &limits)?;

    let payload_entries = entries.get(1..).ok_or(RejectReason::NotAZip)?;
    for inventory in &parsed.descriptor.files {
        if !payload_entries
            .iter()
            .any(|entry| entry.name == inventory.path)
        {
            return Err(RejectReason::DanglingReference.into());
        }
    }
    for entry in payload_entries {
        if !parsed
            .descriptor
            .files
            .iter()
            .any(|inventory| inventory.path == entry.name)
        {
            return Err(RejectReason::UndeclaredPayload.into());
        }
    }

    let mut staged_files = Vec::new();
    staged_files
        .try_reserve_exact(parsed.descriptor.files.len())
        .map_err(|_| RejectReason::EntryTooLarge)?;
    for entry in payload_entries {
        let expanded = extract_entry(
            &mut source_file,
            staging_dir,
            entry,
            false,
            limits,
            &mut expanded_total,
        )?;
        let inventory = parsed
            .descriptor
            .files
            .iter()
            .find(|inventory| inventory.path == entry.name)
            .ok_or(RejectReason::UndeclaredPayload)?;
        if expanded.size_bytes != inventory.size_bytes {
            return Err(RejectReason::SizeMismatch.into());
        }
        if expanded.sha256 != inventory.sha256 {
            return Err(RejectReason::DigestMismatch.into());
        }
        staged_files.push(StagedFile {
            path: entry.name.clone(),
            size_bytes: expanded.size_bytes,
            sha256: expanded.sha256,
        });
    }

    inspect_media(&parsed, staging_dir, limits)?;
    Ok(StagedPackage {
        parsed,
        container_sha256,
        files: staged_files,
    })
}

/// SHA-256 of a file, streamed (lowercase hex). `SourceUnavailable` when it
/// cannot be read.
pub fn sha256_file(path: &Path) -> Result<String, RejectReason> {
    let mut file = File::open(path).map_err(|_| RejectReason::SourceUnavailable)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; IO_CHUNK_BYTES];
    loop {
        let count = file
            .read(&mut buffer)
            .map_err(|_| RejectReason::SourceUnavailable)?;
        if count == 0 {
            break;
        }
        let bytes = buffer.get(..count).ok_or(RejectReason::SourceUnavailable)?;
        hasher.update(bytes);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

/// Convenience for tests and the authoring tool: the staging path of a staged
/// payload.
pub fn staged_path(staging_dir: &Path, package_path: &str) -> PathBuf {
    staging_dir.join(package_path)
}
