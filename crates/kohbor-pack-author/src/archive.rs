use flate2::write::DeflateEncoder;
use flate2::Compression;
use std::io::Write;

const DOS_DATE_1980_01_01: u16 = 33;
const REGULAR_FILE_0644: u32 = 0o100644u32 << 16;

pub(crate) struct ArchiveEntry<'a> {
    pub path: &'a str,
    pub bytes: &'a [u8],
    pub method: u16,
}

pub(crate) fn write_zip(entries: &[ArchiveEntry<'_>]) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut central = Vec::new();
    for entry in entries {
        let name = entry.path.as_bytes();
        let name_len = u16::try_from(name.len()).map_err(|_| "ZIP path is too long".to_owned())?;
        let expanded_size = u32::try_from(entry.bytes.len())
            .map_err(|_| "ZIP entry exceeds the classic ZIP size limit".to_owned())?;
        let local_offset = u32::try_from(output.len())
            .map_err(|_| "ZIP output exceeds the classic ZIP size limit".to_owned())?;
        let crc = crc32(entry.bytes);
        let compressed = match entry.method {
            0 => entry.bytes.to_vec(),
            8 => {
                let mut encoder = DeflateEncoder::new(Vec::new(), Compression::new(6));
                encoder
                    .write_all(entry.bytes)
                    .map_err(|error| format!("Deflate failed: {error}"))?;
                encoder
                    .finish()
                    .map_err(|error| format!("Deflate finish failed: {error}"))?
            }
            _ => return Err("unsupported ZIP method".to_owned()),
        };
        let compressed_size = u32::try_from(compressed.len())
            .map_err(|_| "compressed ZIP entry exceeds the classic ZIP size limit".to_owned())?;
        let local_entry_size = 30usize
            .checked_add(name.len())
            .and_then(|size| size.checked_add(compressed.len()))
            .ok_or_else(|| "ZIP output size overflow".to_owned())?;
        let central_entry_size = 46usize
            .checked_add(name.len())
            .ok_or_else(|| "ZIP central directory size overflow".to_owned())?;
        let projected_size = output
            .len()
            .checked_add(local_entry_size)
            .and_then(|size| size.checked_add(central.len()))
            .and_then(|size| size.checked_add(central_entry_size))
            .and_then(|size| size.checked_add(22))
            .ok_or_else(|| "ZIP output size overflow".to_owned())?;
        if projected_size > 512 * 1024 * 1024 {
            return Err("ZIP output exceeds the package 512 MiB limit".to_owned());
        }

        put_u32(&mut output, 0x0403_4b50);
        put_u16(&mut output, 20);
        put_u16(&mut output, 0);
        put_u16(&mut output, entry.method);
        put_u16(&mut output, 0);
        put_u16(&mut output, DOS_DATE_1980_01_01);
        put_u32(&mut output, crc);
        put_u32(&mut output, compressed_size);
        put_u32(&mut output, expanded_size);
        put_u16(&mut output, name_len);
        put_u16(&mut output, 0);
        output.extend_from_slice(name);
        output.extend_from_slice(&compressed);

        put_u32(&mut central, 0x0201_4b50);
        put_u16(&mut central, 0x0314);
        put_u16(&mut central, 20);
        put_u16(&mut central, 0);
        put_u16(&mut central, entry.method);
        put_u16(&mut central, 0);
        put_u16(&mut central, DOS_DATE_1980_01_01);
        put_u32(&mut central, crc);
        put_u32(&mut central, compressed_size);
        put_u32(&mut central, expanded_size);
        put_u16(&mut central, name_len);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u16(&mut central, 0);
        put_u32(&mut central, REGULAR_FILE_0644);
        put_u32(&mut central, local_offset);
        central.extend_from_slice(name);
    }

    let entries_count = u16::try_from(entries.len())
        .map_err(|_| "too many ZIP entries for the classic ZIP profile".to_owned())?;
    let central_offset = u32::try_from(output.len())
        .map_err(|_| "ZIP output exceeds the classic ZIP size limit".to_owned())?;
    let central_size = u32::try_from(central.len())
        .map_err(|_| "ZIP central directory exceeds the classic ZIP size limit".to_owned())?;
    let end_offset = output
        .len()
        .checked_add(central.len())
        .ok_or_else(|| "ZIP output size overflow".to_owned())?;
    let final_size = end_offset
        .checked_add(22)
        .ok_or_else(|| "ZIP output size overflow".to_owned())?;
    if final_size > 512 * 1024 * 1024 {
        return Err("ZIP output exceeds the package 512 MiB limit".to_owned());
    }
    output.extend_from_slice(&central);
    put_u32(&mut output, 0x0605_4b50);
    put_u16(&mut output, 0);
    put_u16(&mut output, 0);
    put_u16(&mut output, entries_count);
    put_u16(&mut output, entries_count);
    put_u32(&mut output, central_size);
    put_u32(&mut output, central_offset);
    put_u16(&mut output, 0);
    Ok(output)
}

fn crc32(bytes: &[u8]) -> u32 {
    let mut crc = !0u32;
    for byte in bytes {
        crc ^= u32::from(*byte);
        for _ in 0..8 {
            let mask = 0u32.wrapping_sub(crc & 1);
            crc = (crc >> 1) ^ (0xedb8_8320 & mask);
        }
    }
    !crc
}

fn put_u16(out: &mut Vec<u8>, value: u16) {
    out.extend_from_slice(&value.to_le_bytes());
}

fn put_u32(out: &mut Vec<u8>, value: u32) {
    out.extend_from_slice(&value.to_le_bytes());
}
