use crate::vpk::{Error, Result};
use flate2::read::ZlibDecoder;
use std::io::{Read, Take};

const PNG_SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
const MAX_IMAGE_DIMENSION: u32 = 16_384;
const MAX_RGBA_BYTES: usize = 256 * 1024 * 1024;
const MAX_PNG_DIMENSION: u32 = 4096;
const MAX_PNG_IDAT_BYTES: usize = 8 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Image {
    pub width: u32,
    pub height: u32,
    pub rgba: Vec<u8>,
}

/// Decode a single-mip compiled Source 2 texture to straight RGBA8.
///
/// Supported formats observed in the installed hero portrait census are
/// DXT5, embedded PNG, and raw RGBA8888. DXT1 is also supported for synthetic
/// compatibility coverage. Unsupported formats return a value-level error.
pub fn decode_vtex_c(bytes: &[u8]) -> Result<Image> {
    if bytes.len() < 16 {
        return Err(Error::InvalidTexture("resource header is truncated"));
    }
    let metadata_len = usize::try_from(u32_at(bytes, 0)?)
        .map_err(|_| Error::InvalidTexture("metadata length does not fit memory"))?;
    if metadata_len < 16 || metadata_len > bytes.len() {
        return Err(Error::InvalidTexture(
            "metadata length is outside the resource",
        ));
    }
    if u16_at(bytes, 4)? != 12 {
        return Err(Error::InvalidTexture("unsupported resource header version"));
    }
    let table_start = 8_usize
        .checked_add(
            usize::try_from(u32_at(bytes, 8)?)
                .map_err(|_| Error::InvalidTexture("block table offset does not fit memory"))?,
        )
        .ok_or(Error::InvalidTexture("block table offset overflow"))?;
    let block_count = usize::try_from(u32_at(bytes, 12)?)
        .map_err(|_| Error::InvalidTexture("block count does not fit memory"))?;
    let table_bytes = block_count
        .checked_mul(12)
        .ok_or(Error::InvalidTexture("block table length overflow"))?;
    let table_end = table_start
        .checked_add(table_bytes)
        .ok_or(Error::InvalidTexture("block table length overflow"))?;
    if block_count > 64 || table_start < 16 || table_end > metadata_len {
        return Err(Error::InvalidTexture("block table is outside metadata"));
    }

    let mut data_block = None;
    for index in 0..block_count {
        let entry = table_start + index * 12;
        let tag = bytes
            .get(entry..entry + 4)
            .ok_or(Error::InvalidTexture("block tag is truncated"))?;
        let relative_offset = usize::try_from(u32_at(bytes, entry + 4)?)
            .map_err(|_| Error::InvalidTexture("block offset does not fit memory"))?;
        // Source 2 resource offsets are relative to the offset field itself.
        let offset = (entry + 4)
            .checked_add(relative_offset)
            .ok_or(Error::InvalidTexture("block offset overflow"))?;
        let length = usize::try_from(u32_at(bytes, entry + 8)?)
            .map_err(|_| Error::InvalidTexture("block length does not fit memory"))?;
        let end = offset
            .checked_add(length)
            .ok_or(Error::InvalidTexture("block range overflow"))?;
        if end > metadata_len || (length > 0 && offset < table_end) {
            return Err(Error::InvalidTexture("block exceeds resource metadata"));
        }
        if tag == b"DATA" {
            if data_block.is_some() {
                return Err(Error::InvalidTexture("duplicate DATA block"));
            }
            data_block = Some((offset, length));
        }
    }
    let (data_offset, data_length) =
        data_block.ok_or(Error::InvalidTexture("DATA block is absent"))?;
    if data_length < 28 {
        return Err(Error::InvalidTexture(
            "DATA block is too short for VTEX metadata",
        ));
    }
    let texture_header = data_offset;
    let texture_header_end = texture_header
        .checked_add(28)
        .ok_or(Error::InvalidTexture("VTEX header length overflow"))?;
    let data_block_end = data_offset
        .checked_add(data_length)
        .ok_or(Error::InvalidTexture("DATA block range overflow"))?;
    if texture_header_end > data_block_end {
        return Err(Error::InvalidTexture("VTEX header exceeds DATA block"));
    }
    if u16_at(bytes, texture_header)? != 1 {
        return Err(Error::InvalidTexture("unsupported VTEX version"));
    }
    let width = u32::from(u16_at(bytes, texture_header + 20)?);
    let height = u32::from(u16_at(bytes, texture_header + 22)?);
    let depth = u16_at(bytes, texture_header + 24)?;
    let format = *bytes
        .get(texture_header + 26)
        .ok_or(Error::InvalidTexture("image format is truncated"))?;
    let mip_count = *bytes
        .get(texture_header + 27)
        .ok_or(Error::InvalidTexture("mip count is truncated"))?;
    if width == 0 || height == 0 || depth != 1 || mip_count != 1 {
        return Err(Error::InvalidTexture(
            "unsupported dimensions or mip layout",
        ));
    }
    let output_len = checked_rgba_len(width, height)?;
    let payload = bytes
        .get(metadata_len..)
        .ok_or(Error::InvalidTexture("texture payload is truncated"))?;

    let rgba = match format {
        1 => decode_dxt(payload, width, height, DxtKind::Dxt1)?,
        2 => decode_dxt(payload, width, height, DxtKind::Dxt5)?,
        16 | 18 if payload.starts_with(PNG_SIGNATURE) => decode_png_rgba(payload, width, height)?,
        28 => decode_raw_rgba(payload, output_len)?,
        other => return Err(Error::UnsupportedTextureFormat(other)),
    };

    if rgba.len() != output_len {
        return Err(Error::InvalidTexture("decoded byte count is inconsistent"));
    }
    Ok(Image {
        width,
        height,
        rgba,
    })
}

fn decode_raw_rgba(payload: &[u8], output_len: usize) -> Result<Vec<u8>> {
    let pixels = payload
        .get(..output_len)
        .ok_or(Error::InvalidTexture("RGBA8888 payload is truncated"))?;
    let mut output = Vec::new();
    output
        .try_reserve_exact(output_len)
        .map_err(|_| Error::AllocationFailed)?;
    output.extend_from_slice(pixels);
    Ok(output)
}

#[derive(Clone, Copy)]
enum DxtKind {
    Dxt1,
    Dxt5,
}

fn decode_dxt(payload: &[u8], width: u32, height: u32, kind: DxtKind) -> Result<Vec<u8>> {
    let block_bytes = match kind {
        DxtKind::Dxt1 => 8_usize,
        DxtKind::Dxt5 => 16_usize,
    };
    let columns = usize::try_from(width.div_ceil(4))
        .map_err(|_| Error::InvalidTexture("block width does not fit memory"))?;
    let rows = usize::try_from(height.div_ceil(4))
        .map_err(|_| Error::InvalidTexture("block height does not fit memory"))?;
    let block_count = columns
        .checked_mul(rows)
        .ok_or(Error::InvalidTexture("block count overflow"))?;
    let required = block_count
        .checked_mul(block_bytes)
        .ok_or(Error::InvalidTexture("compressed payload length overflow"))?;
    if payload.len() < required {
        return Err(Error::InvalidTexture("compressed block data is truncated"));
    }
    let output_len = checked_rgba_len(width, height)?;
    let mut output = zeroed_vec(output_len)?;
    let width_usize =
        usize::try_from(width).map_err(|_| Error::InvalidTexture("width does not fit memory"))?;
    let height_usize =
        usize::try_from(height).map_err(|_| Error::InvalidTexture("height does not fit memory"))?;

    for block_y in 0..rows {
        for block_x in 0..columns {
            let block_index = block_y
                .checked_mul(columns)
                .and_then(|v| v.checked_add(block_x))
                .ok_or(Error::InvalidTexture("block index overflow"))?;
            let start = block_index
                .checked_mul(block_bytes)
                .ok_or(Error::InvalidTexture("block offset overflow"))?;
            let block = payload
                .get(start..start + block_bytes)
                .ok_or(Error::InvalidTexture("compressed block is truncated"))?;
            let decoded = match kind {
                DxtKind::Dxt1 => decode_dxt1_block(block)?,
                DxtKind::Dxt5 => decode_dxt5_block(block)?,
            };
            for local_y in 0..4_usize {
                for local_x in 0..4_usize {
                    let x = block_x * 4 + local_x;
                    let y = block_y * 4 + local_y;
                    if x >= width_usize || y >= height_usize {
                        continue;
                    }
                    let source = (local_y * 4 + local_x) * 4;
                    let target = (y * width_usize + x) * 4;
                    output[target..target + 4].copy_from_slice(&decoded[source..source + 4]);
                }
            }
        }
    }
    Ok(output)
}

fn decode_dxt1_block(block: &[u8]) -> Result<[u8; 64]> {
    if block.len() < 8 {
        return Err(Error::InvalidTexture("DXT1 block is truncated"));
    }
    decode_color_block(block, false, None)
}

fn decode_dxt5_block(block: &[u8]) -> Result<[u8; 64]> {
    if block.len() < 16 {
        return Err(Error::InvalidTexture("DXT5 block is truncated"));
    }
    let alpha0 = block[0];
    let alpha1 = block[1];
    let mut alpha_codes = 0_u64;
    for (index, byte) in block[2..8].iter().enumerate() {
        alpha_codes |= u64::from(*byte) << (index * 8);
    }
    let alpha_table = alpha_palette(alpha0, alpha1);
    let alphas =
        std::array::from_fn::<_, 16, _>(|i| alpha_table[((alpha_codes >> (i * 3)) & 7) as usize]);
    decode_color_block(&block[8..16], true, Some(alphas))
}

fn decode_color_block(block: &[u8], dxt5_color: bool, alpha: Option<[u8; 16]>) -> Result<[u8; 64]> {
    if block.len() < 8 {
        return Err(Error::InvalidTexture("color block is truncated"));
    }
    let color0 = u16::from_le_bytes([block[0], block[1]]);
    let color1 = u16::from_le_bytes([block[2], block[3]]);
    let mut palette = [[0_u8; 4]; 4];
    palette[0] = rgb565(color0);
    palette[1] = rgb565(color1);
    if color0 > color1 || dxt5_color {
        palette[2] = std::array::from_fn(|component| {
            if component == 3 {
                255
            } else {
                ((2 * u16::from(palette[0][component]) + u16::from(palette[1][component])) / 3)
                    as u8
            }
        });
        palette[3] = std::array::from_fn(|component| {
            if component == 3 {
                255
            } else {
                ((u16::from(palette[0][component]) + 2 * u16::from(palette[1][component])) / 3)
                    as u8
            }
        });
    } else {
        palette[2] = std::array::from_fn(|component| {
            if component == 3 {
                255
            } else {
                ((u16::from(palette[0][component]) + u16::from(palette[1][component])) / 2) as u8
            }
        });
        palette[3] = [0, 0, 0, 0];
    }
    let color_codes = u32::from_le_bytes([block[4], block[5], block[6], block[7]]);
    let mut output = [0_u8; 64];
    for pixel in 0..16 {
        let mut color = palette[((color_codes >> (pixel * 2)) & 3) as usize];
        if let Some(alphas) = alpha {
            color[3] = alphas[pixel];
        }
        let at = pixel * 4;
        output[at..at + 4].copy_from_slice(&color);
    }
    Ok(output)
}

fn rgb565(value: u16) -> [u8; 4] {
    let red = ((value >> 11) & 0x1f) as u8;
    let green = ((value >> 5) & 0x3f) as u8;
    let blue = (value & 0x1f) as u8;
    [
        (red << 3) | (red >> 2),
        (green << 2) | (green >> 4),
        (blue << 3) | (blue >> 2),
        255,
    ]
}

fn alpha_palette(alpha0: u8, alpha1: u8) -> [u8; 8] {
    let mut values = [0_u8; 8];
    values[0] = alpha0;
    values[1] = alpha1;
    if alpha0 > alpha1 {
        for index in 1..=6 {
            values[index + 1] = (((7 - index) as u16 * u16::from(alpha0)
                + index as u16 * u16::from(alpha1))
                / 7) as u8;
        }
    } else {
        for index in 1..=4 {
            values[index + 1] = (((5 - index) as u16 * u16::from(alpha0)
                + index as u16 * u16::from(alpha1))
                / 5) as u8;
        }
        values[6] = 0;
        values[7] = 255;
    }
    values
}

fn decode_png_rgba(bytes: &[u8], expected_width: u32, expected_height: u32) -> Result<Vec<u8>> {
    // This API can receive an arbitrary slice, without passing through Vpk::read.
    // Bound dimensions before collecting any chunks or allocating decode buffers.
    check_png_dimensions(expected_width, expected_height)?;
    if !bytes.starts_with(PNG_SIGNATURE) {
        return Err(Error::InvalidPng("signature mismatch"));
    }
    let mut position = PNG_SIGNATURE.len();
    let mut header = None;
    let mut palette: Option<Vec<u8>> = None;
    let mut transparency: Option<Vec<u8>> = None;
    let mut compressed = Vec::new();
    let mut ended = false;
    let mut saw_idat = false;
    let mut idat_ended = false;
    while position < bytes.len() {
        let length = usize::try_from(u32_be_at(bytes, position)?)
            .map_err(|_| Error::InvalidPng("chunk length does not fit memory"))?;
        let type_at = position
            .checked_add(4)
            .ok_or(Error::InvalidPng("chunk offset overflow"))?;
        let data_at = type_at
            .checked_add(4)
            .ok_or(Error::InvalidPng("chunk offset overflow"))?;
        let data_end = data_at
            .checked_add(length)
            .ok_or(Error::InvalidPng("chunk length overflow"))?;
        let crc_end = data_end
            .checked_add(4)
            .ok_or(Error::InvalidPng("chunk CRC offset overflow"))?;
        if crc_end > bytes.len() {
            return Err(Error::InvalidPng("chunk exceeds encoded image"));
        }
        let chunk_type = &bytes[type_at..data_at];
        let chunk_data = &bytes[data_at..data_end];
        if header.is_none() && chunk_type != b"IHDR" {
            return Err(Error::InvalidPng("IHDR must be first"));
        }
        if chunk_type == b"IDAT"
            && compressed
                .len()
                .checked_add(length)
                .is_none_or(|total| total > MAX_PNG_IDAT_BYTES)
        {
            return Err(Error::InvalidPng("compressed IDAT byte limit exceeded"));
        }
        let expected_crc = u32_be_at(bytes, data_end)?;
        if png_crc32(&bytes[type_at..data_end]) != expected_crc {
            return Err(Error::InvalidPng("chunk CRC mismatch"));
        }
        if saw_idat && chunk_type != b"IDAT" {
            idat_ended = true;
        }
        match chunk_type {
            b"IHDR" => {
                if header.is_some() || chunk_data.len() != 13 {
                    return Err(Error::InvalidPng("invalid or duplicate IHDR"));
                }
                let width = u32::from_be_bytes([
                    chunk_data[0],
                    chunk_data[1],
                    chunk_data[2],
                    chunk_data[3],
                ]);
                let height = u32::from_be_bytes([
                    chunk_data[4],
                    chunk_data[5],
                    chunk_data[6],
                    chunk_data[7],
                ]);
                check_png_dimensions(width, height)?;
                if width != expected_width || height != expected_height {
                    return Err(Error::InvalidPng("IHDR dimensions disagree with VTEX"));
                }
                let bit_depth = chunk_data[8];
                let color_type = chunk_data[9];
                if bit_depth != 8 || !matches!(color_type, 0 | 2 | 3 | 4 | 6) {
                    return Err(Error::InvalidPng("unsupported bit depth or color type"));
                }
                if chunk_data[10] != 0 || chunk_data[11] != 0 || chunk_data[12] != 0 {
                    return Err(Error::InvalidPng(
                        "unsupported PNG compression, filter, or interlace",
                    ));
                }
                header = Some((width, height, color_type));
            }
            b"PLTE" => {
                if palette.is_some()
                    || chunk_data.is_empty()
                    || !chunk_data.len().is_multiple_of(3)
                    || chunk_data.len() > 768
                {
                    return Err(Error::InvalidPng("invalid palette"));
                }
                palette = Some(copy_bytes(chunk_data)?);
            }
            b"tRNS" => {
                if transparency.is_some() || chunk_data.len() > 768 {
                    return Err(Error::InvalidPng("invalid transparency chunk"));
                }
                transparency = Some(copy_bytes(chunk_data)?);
            }
            b"IDAT" => {
                if idat_ended {
                    return Err(Error::InvalidPng("IDAT chunks are not consecutive"));
                }
                saw_idat = true;
                compressed
                    .try_reserve_exact(chunk_data.len())
                    .map_err(|_| Error::AllocationFailed)?;
                compressed.extend_from_slice(chunk_data);
            }
            b"IEND" => {
                if !chunk_data.is_empty() {
                    return Err(Error::InvalidPng("IEND has data"));
                }
                ended = true;
                position = crc_end;
                break;
            }
            _ => {
                let critical = chunk_type
                    .first()
                    .is_some_and(|byte| byte.is_ascii_uppercase());
                if critical {
                    return Err(Error::InvalidPng("unknown critical chunk"));
                }
            }
        }
        position = crc_end;
    }
    if !ended || position != bytes.len() || !saw_idat {
        return Err(Error::InvalidPng("missing IEND, IDAT, or trailing data"));
    }
    let (width, height, color_type) = header.ok_or(Error::InvalidPng("IHDR is absent"))?;
    if color_type == 3 && palette.is_none() {
        return Err(Error::InvalidPng("palette image lacks PLTE"));
    }
    let channels = match color_type {
        0 | 3 => 1_usize,
        2 => 3,
        4 => 2,
        6 => 4,
        _ => return Err(Error::InvalidPng("unsupported color type")),
    };
    let width = usize::try_from(width).map_err(|_| Error::InvalidPng("width overflow"))?;
    let height = usize::try_from(height).map_err(|_| Error::InvalidPng("height overflow"))?;
    let row_len = width
        .checked_mul(channels)
        .ok_or(Error::InvalidPng("row size overflow"))?;
    let filtered_row_len = row_len
        .checked_add(1)
        .ok_or(Error::InvalidPng("row size overflow"))?;
    let filtered_len = filtered_row_len
        .checked_mul(height)
        .ok_or(Error::InvalidPng("inflated image size overflow"))?;
    if filtered_len > MAX_RGBA_BYTES {
        return Err(Error::InvalidPng("image exceeds the decode size limit"));
    }
    let mut inflated = Vec::new();
    inflated
        .try_reserve_exact(filtered_len)
        .map_err(|_| Error::AllocationFailed)?;
    let decoder = ZlibDecoder::new(compressed.as_slice());
    let limit = u64::try_from(filtered_len)
        .map_err(|_| Error::InvalidPng("inflated image size overflow"))?
        .checked_add(1)
        .ok_or(Error::InvalidPng("inflated image size overflow"))?;
    let mut limited: Take<ZlibDecoder<&[u8]>> = decoder.take(limit);
    limited.read_to_end(&mut inflated)?;
    if inflated.len() != filtered_len {
        return Err(Error::InvalidPng("inflated size disagrees with dimensions"));
    }

    let decoded_len = row_len
        .checked_mul(height)
        .ok_or(Error::InvalidPng("decoded image size overflow"))?;
    let mut scanlines = zeroed_vec(decoded_len)?;
    for y in 0..height {
        let filtered_start = y * filtered_row_len;
        let filter = inflated[filtered_start];
        if filter > 4 {
            return Err(Error::InvalidPng("unknown scanline filter"));
        }
        let row_start = y * row_len;
        for x in 0..row_len {
            let raw = inflated[filtered_start + 1 + x];
            let left = if x >= channels {
                scanlines[row_start + x - channels]
            } else {
                0
            };
            let above = if y > 0 {
                scanlines[row_start + x - row_len]
            } else {
                0
            };
            let upper_left = if y > 0 && x >= channels {
                scanlines[row_start + x - row_len - channels]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
                4 => paeth(left, above, upper_left),
                _ => return Err(Error::InvalidPng("unknown scanline filter")),
            };
            scanlines[row_start + x] = raw.wrapping_add(predictor);
        }
    }

    let output_len = checked_rgba_len(expected_width, expected_height)?;
    let mut rgba = zeroed_vec(output_len)?;
    for pixel in 0..width * height {
        let src = pixel * channels;
        let dst = pixel * 4;
        match color_type {
            0 => {
                let gray = scanlines[src];
                let alpha = transparency
                    .as_ref()
                    .filter(|chunk| chunk.len() == 2)
                    .map(|chunk| {
                        let transparent = u16::from_be_bytes([chunk[0], chunk[1]]) as u8;
                        if gray == transparent {
                            0
                        } else {
                            255
                        }
                    })
                    .unwrap_or(255);
                rgba[dst..dst + 4].copy_from_slice(&[gray, gray, gray, alpha]);
            }
            2 => {
                let red = scanlines[src];
                let green = scanlines[src + 1];
                let blue = scanlines[src + 2];
                let alpha = match transparency.as_ref().filter(|chunk| chunk.len() == 6) {
                    Some(chunk) => {
                        let transparent = [chunk[1], chunk[3], chunk[5]];
                        if [red, green, blue] == transparent {
                            0
                        } else {
                            255
                        }
                    }
                    None => 255,
                };
                rgba[dst..dst + 4].copy_from_slice(&[red, green, blue, alpha]);
            }
            3 => {
                let index = usize::from(scanlines[src]);
                let colors = palette
                    .as_ref()
                    .ok_or(Error::InvalidPng("palette image lacks PLTE"))?;
                let color = index
                    .checked_mul(3)
                    .ok_or(Error::InvalidPng("palette index overflow"))?;
                let rgb = colors
                    .get(color..color + 3)
                    .ok_or(Error::InvalidPng("palette index is out of range"))?;
                let alpha = transparency
                    .as_ref()
                    .and_then(|values| values.get(index))
                    .copied()
                    .unwrap_or(255);
                rgba[dst..dst + 4].copy_from_slice(&[rgb[0], rgb[1], rgb[2], alpha]);
            }
            4 => {
                let gray = scanlines[src];
                rgba[dst..dst + 4].copy_from_slice(&[gray, gray, gray, scanlines[src + 1]]);
            }
            6 => rgba[dst..dst + 4].copy_from_slice(&scanlines[src..src + 4]),
            _ => return Err(Error::InvalidPng("unsupported color type")),
        }
    }
    Ok(rgba)
}

fn check_png_dimensions(width: u32, height: u32) -> Result<()> {
    if width > MAX_PNG_DIMENSION || height > MAX_PNG_DIMENSION {
        return Err(Error::InvalidPng("PNG dimensions exceed 4096"));
    }
    if width == 0 || height == 0 {
        return Err(Error::InvalidPng("PNG dimensions must be positive"));
    }
    Ok(())
}

fn paeth(left: u8, above: u8, upper_left: u8) -> u8 {
    let left = i32::from(left);
    let above = i32::from(above);
    let upper_left = i32::from(upper_left);
    let estimate = left + above - upper_left;
    let left_distance = (estimate - left).abs();
    let above_distance = (estimate - above).abs();
    let upper_left_distance = (estimate - upper_left).abs();
    if left_distance <= above_distance && left_distance <= upper_left_distance {
        left as u8
    } else if above_distance <= upper_left_distance {
        above as u8
    } else {
        upper_left as u8
    }
}

fn checked_rgba_len(width: u32, height: u32) -> Result<usize> {
    if width == 0 || height == 0 || width > MAX_IMAGE_DIMENSION || height > MAX_IMAGE_DIMENSION {
        return Err(Error::InvalidTexture(
            "dimensions exceed the supported range",
        ));
    }
    let len = usize::try_from(width)
        .ok()
        .and_then(|w| usize::try_from(height).ok().and_then(|h| w.checked_mul(h)))
        .and_then(|pixels| pixels.checked_mul(4))
        .ok_or(Error::InvalidTexture("RGBA size overflow"))?;
    if len > MAX_RGBA_BYTES {
        return Err(Error::InvalidTexture(
            "RGBA image exceeds the 256 MiB limit",
        ));
    }
    Ok(len)
}

fn zeroed_vec(length: usize) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(length)
        .map_err(|_| Error::AllocationFailed)?;
    bytes.resize(length, 0);
    Ok(bytes)
}

fn copy_bytes(source: &[u8]) -> Result<Vec<u8>> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(source.len())
        .map_err(|_| Error::AllocationFailed)?;
    bytes.extend_from_slice(source);
    Ok(bytes)
}

fn u16_at(bytes: &[u8], offset: usize) -> Result<u16> {
    let end = offset
        .checked_add(2)
        .ok_or(Error::InvalidTexture("integer offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or(Error::InvalidTexture("integer is truncated"))?;
    Ok(u16::from_le_bytes([slice[0], slice[1]]))
}

fn u32_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(Error::InvalidTexture("integer offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or(Error::InvalidTexture("integer is truncated"))?;
    Ok(u32::from_le_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

fn u32_be_at(bytes: &[u8], offset: usize) -> Result<u32> {
    let end = offset
        .checked_add(4)
        .ok_or(Error::InvalidPng("integer offset overflow"))?;
    let slice = bytes
        .get(offset..end)
        .ok_or(Error::InvalidPng("integer is truncated"))?;
    Ok(u32::from_be_bytes([slice[0], slice[1], slice[2], slice[3]]))
}

pub(crate) fn png_crc32(bytes: &[u8]) -> u32 {
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
    use super::{decode_vtex_c, png_crc32};
    use crate::Error;
    use flate2::write::ZlibEncoder;
    use flate2::Compression;
    use std::io::Write;

    #[test]
    fn resource_block_offsets_are_relative_to_their_offset_fields() {
        for table_start in [16_usize, 32] {
            let metadata_len = 124_usize;
            let texture_start = 96_usize;
            let mut resource = vec![0_u8; metadata_len];
            resource[..4].copy_from_slice(&(metadata_len as u32).to_le_bytes());
            resource[4..8].copy_from_slice(&0x0001_000c_u32.to_le_bytes());
            resource[8..12].copy_from_slice(&((table_start - 8) as u32).to_le_bytes());
            resource[12..16].copy_from_slice(&1_u32.to_le_bytes());
            resource[table_start..table_start + 4].copy_from_slice(b"DATA");
            resource[table_start + 4..table_start + 8]
                .copy_from_slice(&((texture_start - table_start - 4) as u32).to_le_bytes());
            resource[table_start + 8..table_start + 12].copy_from_slice(&28_u32.to_le_bytes());
            resource[texture_start..texture_start + 2].copy_from_slice(&1_u16.to_le_bytes());
            resource[texture_start + 20..texture_start + 22].copy_from_slice(&4_u16.to_le_bytes());
            resource[texture_start + 22..texture_start + 24].copy_from_slice(&4_u16.to_le_bytes());
            resource[texture_start + 24..texture_start + 26].copy_from_slice(&1_u16.to_le_bytes());
            resource[texture_start + 26] = 1;
            resource[texture_start + 27] = 1;
            resource.extend_from_slice(&[0x00, 0xf8, 0, 0, 0, 0, 0, 0]);
            assert!(
                decode_vtex_c(&resource).is_ok(),
                "relative block layout rejected"
            );
        }
    }

    #[test]
    fn decodes_synthetic_dxt1_block_to_rgba8() {
        let block = [0x00, 0xf8, 0x00, 0x00, 0, 0, 0, 0];
        let resource = make_vtex(4, 4, 1, &block);
        let image = decode_vtex_c(&resource).expect("synthetic DXT1 should decode");
        assert_eq!((image.width, image.height), (4, 4));
        assert_eq!(image.rgba.len(), 4 * 4 * 4);
        assert!(image
            .rgba
            .chunks_exact(4)
            .all(|pixel| { pixel[0] == 255 && pixel[1] == 0 && pixel[2] == 0 && pixel[3] == 255 }));
    }

    #[test]
    fn decodes_synthetic_dxt5_block_to_rgba8() {
        let mut block = [0_u8; 16];
        block[0] = 255;
        block[1] = 0;
        block[8..10].copy_from_slice(&0x001f_u16.to_le_bytes());
        let resource = make_vtex(4, 4, 2, &block);
        let image = decode_vtex_c(&resource).expect("synthetic DXT5 should decode");
        assert!(image
            .rgba
            .chunks_exact(4)
            .all(|pixel| { pixel[0] == 0 && pixel[1] == 0 && pixel[2] == 255 && pixel[3] == 255 }));
    }

    #[test]
    fn dxt5_alpha_interpolation_covers_both_endpoint_orders() {
        for (endpoints, expected) in [
            ([255_u8, 0], [255, 0, 218, 182, 145, 109, 72, 36]),
            ([6, 12], [6, 12, 7, 8, 9, 10, 0, 255]),
        ] {
            let mut block = [0_u8; 16];
            block[..2].copy_from_slice(&endpoints);
            let codes = (0..16).fold(0_u64, |bits, pixel| bits | ((pixel % 8) << (pixel * 3)));
            block[2..8].copy_from_slice(&codes.to_le_bytes()[..6]);
            block[8..10].copy_from_slice(&0xf800_u16.to_le_bytes());
            let image = decode_vtex_c(&make_vtex(4, 4, 2, &block)).expect("synthetic alpha block");
            assert!(
                image
                    .rgba
                    .chunks_exact(4)
                    .enumerate()
                    .all(|(index, pixel)| pixel[3] == expected[index % 8]),
                "alpha interpolation differs"
            );
        }
    }

    #[test]
    fn dxt1_transparency_crops_partial_edge_blocks() {
        let block = [0, 0, 0, 0xf8, 0xff, 0xff, 0xff, 0xff];
        let image = decode_vtex_c(&make_vtex(3, 2, 1, &block)).expect("synthetic edge block");
        assert_eq!(image.rgba.len(), 24);
        assert!(
            image.rgba.iter().all(|value| *value == 0),
            "transparent pixels differ"
        );
    }

    #[test]
    fn unsupported_mips_versions_and_noise_return_errors() {
        let valid = make_vtex(4, 4, 1, &[0; 8]);
        for (at, replacement) in [(4, 0xff), (96, 0xff), (123, 2)] {
            let mut corrupt = valid.clone();
            corrupt[at] = replacement;
            assert!(decode_vtex_c(&corrupt).is_err());
        }
        for length in 0..256 {
            for value in [0_u8, 0xff, 0x55, 0xaa] {
                assert!(decode_vtex_c(&vec![value; length]).is_err());
            }
        }
    }

    #[test]
    fn decodes_synthetic_rgba8888_payload() {
        let pixels = [255, 0, 0, 255, 0, 255, 0, 128];
        let resource = make_vtex(2, 1, 28, &pixels);
        let image = decode_vtex_c(&resource).expect("synthetic RGBA should decode");
        assert!(image.rgba[..] == pixels[..], "RGBA8888 pixels differ");
    }

    #[test]
    fn decodes_synthetic_embedded_png_payload() {
        let pixels = [
            0, 255, 0, 0, 255, // filter byte and an opaque red pixel
            0, 0, 255, 0, 128, // filter byte and a half-alpha green pixel
        ];
        let png = make_png(1, 2, 6, &pixels);
        let resource = make_vtex(1, 2, 18, &png);
        let image = decode_vtex_c(&resource).expect("synthetic PNG should decode");
        assert!(
            image.rgba.as_slice() == [255, 0, 0, 255, 0, 255, 0, 128],
            "embedded PNG pixels differ"
        );
    }

    #[test]
    fn png_rejects_declared_dimensions_over_4096() {
        for (width, height) in [(4097_u16, 1_u16), (1, 4097)] {
            let filtered = vec![0; (usize::from(width) * 4 + 1) * usize::from(height)];
            let png = make_png(u32::from(width), u32::from(height), 6, &filtered);
            assert!(matches!(
                decode_vtex_c(&make_vtex(width, height, 16, &png)),
                Err(Error::InvalidPng("PNG dimensions exceed 4096"))
            ));
        }
        let huge_header = make_png(u32::MAX, 1, 6, &[0, 0, 0, 0, 255]);
        assert!(matches!(
            decode_vtex_c(&make_vtex(1, 1, 16, &huge_header)),
            Err(Error::InvalidPng("PNG dimensions exceed 4096"))
        ));
    }

    #[test]
    fn png_accepts_the_dimension_limit() {
        for (width, height) in [(4096_u16, 1_u16), (1, 4096)] {
            let filtered = vec![0; (usize::from(width) * 4 + 1) * usize::from(height)];
            let png = make_png(u32::from(width), u32::from(height), 6, &filtered);
            let image = decode_vtex_c(&make_vtex(width, height, 16, &png))
                .expect("PNG at dimension boundary");
            assert_eq!(
                image.rgba.len(),
                usize::from(width) * usize::from(height) * 4
            );
        }
    }

    #[test]
    fn png_rejects_total_idat_over_eight_mib_across_chunks() {
        let mut png = make_png(1, 1, 6, &[0, 0, 0, 0, 255]);
        png.truncate(png.len() - 12);
        let padding = vec![0; 4 * 1024 * 1024];
        push_chunk(&mut png, b"IDAT", &padding);
        push_chunk(&mut png, b"IDAT", &padding);
        push_chunk(&mut png, b"IEND", &[]);
        assert!(matches!(
            decode_vtex_c(&make_vtex(1, 1, 18, &png)),
            Err(Error::InvalidPng("compressed IDAT byte limit exceeded"))
        ));
    }

    #[test]
    fn png_requires_ihdr_before_allocating_chunk_data() {
        let png = make_png(1, 1, 6, &[0, 0, 0, 0, 255]);
        let mut reordered = png[..8].to_vec();
        reordered.extend_from_slice(&png[33..png.len() - 12]);
        reordered.extend_from_slice(&png[8..33]);
        reordered.extend_from_slice(&png[png.len() - 12..]);
        assert!(matches!(
            decode_vtex_c(&make_vtex(1, 1, 16, &reordered)),
            Err(Error::InvalidPng("IHDR must be first"))
        ));
    }

    #[test]
    fn decodes_png_scanline_filters_and_palette_alpha() {
        let rows = [
            [255, 0, 0, 255, 0, 255, 0, 255],
            [0, 0, 255, 255, 255, 255, 0, 255],
            [255, 255, 255, 255, 0, 0, 0, 255],
            [12, 34, 56, 78, 90, 123, 45, 67],
            [1, 2, 3, 4, 5, 6, 7, 8],
        ];
        let mut filtered = Vec::new();
        let mut previous = [0_u8; 8];
        for (index, row) in rows.iter().enumerate() {
            let filter = index as u8;
            filtered.push(filter);
            filtered.extend_from_slice(&encode_filter_row(filter, row, &previous, 4));
            previous = *row;
        }
        let png = make_png(2, 5, 6, &filtered);
        let resource = make_vtex(2, 5, 16, &png);
        let image = decode_vtex_c(&resource).expect("all synthetic PNG filters should decode");
        assert!(
            image.rgba == rows.into_iter().flatten().collect::<Vec<u8>>(),
            "filtered PNG pixels differ"
        );

        let palette_png = make_palette_png();
        let palette_resource = make_vtex(2, 1, 18, &palette_png);
        let palette_image =
            decode_vtex_c(&palette_resource).expect("synthetic palette PNG should decode");
        assert!(
            palette_image.rgba.as_slice() == [255, 0, 0, 255, 0, 255, 0, 128],
            "palette PNG pixels differ"
        );
    }

    #[test]
    fn rejects_truncated_and_corrupt_synthetic_textures_without_panicking() {
        let valid = make_vtex(4, 4, 1, &[0x00, 0xf8, 0, 0, 0, 0, 0, 0]);
        for end in 0..valid.len() {
            assert!(
                decode_vtex_c(&valid[..end]).is_err(),
                "accepted prefix {end}"
            );
        }

        let mut bad_header = valid.clone();
        bad_header[..4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_vtex_c(&bad_header).is_err());

        let mut bad_table = valid.clone();
        bad_table[12..16].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_vtex_c(&bad_table).is_err());

        let mut bad_block = valid.clone();
        bad_block[20..24].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(decode_vtex_c(&bad_block).is_err());

        let short_dxt = make_vtex(4, 4, 1, &[0; 7]);
        assert!(decode_vtex_c(&short_dxt).is_err());
    }

    #[test]
    fn rejects_png_chunk_crc_and_truncated_stream() {
        let raw = [0, 1, 2, 3, 255];
        let png = make_png(1, 1, 6, &raw);
        let valid = make_vtex(1, 1, 16, &png);
        assert!(decode_vtex_c(&valid).is_ok());

        let mut corrupt_png = png.clone();
        let idat_crc = corrupt_png.len() - 16;
        corrupt_png[idat_crc] ^= 0x40;
        let corrupt = make_vtex(1, 1, 16, &corrupt_png);
        assert!(matches!(decode_vtex_c(&corrupt), Err(Error::InvalidPng(_))));

        let truncated = make_vtex(1, 1, 16, &png[..png.len() - 1]);
        assert!(decode_vtex_c(&truncated).is_err());
    }

    #[test]
    fn synthetic_bc7_mode_six_known_answers() {
        for color in [[255, 127, 63, 255], [0, 0, 254, 128]] {
            let block = bc7_mode6_solid(color);
            let decoded = decode_bc7_mode6_for_test(&block).expect("valid mode six block");
            assert!(decoded
                .chunks_exact(4)
                .all(|pixel| pixel.iter().copied().eq(color)));
        }
    }

    fn make_vtex(width: u16, height: u16, format: u8, payload: &[u8]) -> Vec<u8> {
        let metadata_len = 124_u32;
        let mut bytes = vec![0_u8; metadata_len as usize];
        bytes[0..4].copy_from_slice(&metadata_len.to_le_bytes());
        bytes[4..8].copy_from_slice(&0x0001_000c_u32.to_le_bytes());
        bytes[8..12].copy_from_slice(&8_u32.to_le_bytes());
        bytes[12..16].copy_from_slice(&2_u32.to_le_bytes());
        bytes[16..20].copy_from_slice(b"RED2");
        bytes[20..24].copy_from_slice(&20_u32.to_le_bytes());
        bytes[24..28].copy_from_slice(&24_u32.to_le_bytes());
        bytes[28..32].copy_from_slice(b"DATA");
        bytes[32..36].copy_from_slice(&64_u32.to_le_bytes());
        bytes[36..40].copy_from_slice(&28_u32.to_le_bytes());

        let texture = 96;
        bytes[texture..texture + 2].copy_from_slice(&1_u16.to_le_bytes());
        bytes[texture + 20..texture + 22].copy_from_slice(&width.to_le_bytes());
        bytes[texture + 22..texture + 24].copy_from_slice(&height.to_le_bytes());
        bytes[texture + 24..texture + 26].copy_from_slice(&1_u16.to_le_bytes());
        bytes[texture + 26] = format;
        bytes[texture + 27] = 1;
        bytes.extend_from_slice(payload);
        bytes
    }

    fn make_png(width: u32, height: u32, color_type: u8, filtered_rows: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(filtered_rows)
            .expect("write synthetic scanlines");
        let compressed = encoder.finish().expect("finish synthetic zlib stream");

        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, color_type, 0, 0, 0]);
        push_chunk(&mut png, b"IHDR", &ihdr);
        push_chunk(&mut png, b"IDAT", &compressed);
        push_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn make_palette_png() -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
        encoder
            .write_all(&[0, 0, 1])
            .expect("write synthetic palette scanline");
        let compressed = encoder.finish().expect("finish synthetic palette stream");
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        let mut ihdr = Vec::new();
        ihdr.extend_from_slice(&2_u32.to_be_bytes());
        ihdr.extend_from_slice(&1_u32.to_be_bytes());
        ihdr.extend_from_slice(&[8, 3, 0, 0, 0]);
        push_chunk(&mut png, b"IHDR", &ihdr);
        push_chunk(&mut png, b"PLTE", &[255, 0, 0, 0, 255, 0]);
        push_chunk(&mut png, b"tRNS", &[255, 128]);
        push_chunk(&mut png, b"IDAT", &compressed);
        push_chunk(&mut png, b"IEND", &[]);
        png
    }

    fn encode_filter_row(
        filter: u8,
        row: &[u8],
        previous: &[u8],
        bytes_per_pixel: usize,
    ) -> Vec<u8> {
        let mut encoded = Vec::with_capacity(row.len());
        for index in 0..row.len() {
            let left = if index >= bytes_per_pixel {
                row[index - bytes_per_pixel]
            } else {
                0
            };
            let above = previous[index];
            let upper_left = if index >= bytes_per_pixel {
                previous[index - bytes_per_pixel]
            } else {
                0
            };
            let predictor = match filter {
                0 => 0,
                1 => left,
                2 => above,
                3 => ((u16::from(left) + u16::from(above)) / 2) as u8,
                4 => super::paeth(left, above, upper_left),
                _ => 0,
            };
            encoded.push(row[index].wrapping_sub(predictor));
        }
        encoded
    }

    fn push_chunk(png: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
        png.extend_from_slice(&(data.len() as u32).to_be_bytes());
        let start = png.len();
        png.extend_from_slice(kind);
        png.extend_from_slice(data);
        let crc = png_crc32(&png[start..]);
        png.extend_from_slice(&crc.to_be_bytes());
    }

    // Test-local vector construction only. The production decoder has no BC7 support.
    fn bc7_mode6_solid(color: [u8; 4]) -> [u8; 16] {
        let mut bits = Vec::with_capacity(128);
        for bit in 0..7 {
            bits.push(u8::from(bit == 6));
        }
        for component in color {
            let quantized = component >> 1;
            for _endpoint in 0..2 {
                for bit in 0..7 {
                    bits.push((quantized >> bit) & 1);
                }
            }
        }
        bits.push(color[0] & 1);
        bits.push(color[0] & 1);
        bits.extend(std::iter::repeat_n(0, 63));
        let mut block = [0_u8; 16];
        for (index, bit) in bits.into_iter().take(128).enumerate() {
            block[index / 8] |= bit << (index % 8);
        }
        block
    }

    // Test-local mode-6 reference helper, never used by production decode_vtex_c.
    // Passing these constructed vectors does not establish runtime BC7 support.
    fn decode_bc7_mode6_for_test(block: &[u8; 16]) -> Option<[u8; 64]> {
        let mut bit = 0_usize;
        let mut mode = None;
        for index in 0..8 {
            let value = read_bit(block, &mut bit)?;
            if value != 0 {
                mode = Some(index);
                break;
            }
        }
        if mode != Some(6) {
            return None;
        }
        let mut endpoints = [[0_u8; 4]; 2];
        let [endpoint0, endpoint1] = &mut endpoints;
        for (value0, value1) in endpoint0.iter_mut().zip(endpoint1.iter_mut()) {
            *value0 = read_bits(block, &mut bit, 7)? as u8;
            *value1 = read_bits(block, &mut bit, 7)? as u8;
        }
        let p0 = read_bit(block, &mut bit)? as u8;
        let p1 = read_bit(block, &mut bit)? as u8;
        for (endpoint, pbit) in endpoints.iter_mut().zip([p0, p1]) {
            for channel in endpoint {
                *channel = (*channel << 1) | pbit;
            }
        }
        let weights = [
            0_u16, 4, 9, 13, 17, 21, 26, 30, 34, 38, 43, 47, 51, 55, 60, 64,
        ];
        let mut rgba = [0_u8; 64];
        for pixel in 0..16 {
            let index = read_bits(block, &mut bit, if pixel == 0 { 3 } else { 4 })? as usize;
            let weight = weights[index];
            for channel in 0..4 {
                rgba[pixel * 4 + channel] = (((64 - weight) * u16::from(endpoints[0][channel])
                    + weight * u16::from(endpoints[1][channel])
                    + 32)
                    >> 6) as u8;
            }
        }
        Some(rgba)
    }

    fn read_bit(block: &[u8; 16], offset: &mut usize) -> Option<u32> {
        if *offset >= 128 {
            return None;
        }
        let result = u32::from((block[*offset / 8] >> (*offset % 8)) & 1);
        *offset += 1;
        Some(result)
    }

    fn read_bits(block: &[u8; 16], offset: &mut usize, count: usize) -> Option<u32> {
        let mut result = 0_u32;
        for index in 0..count {
            result |= read_bit(block, offset)? << index;
        }
        Some(result)
    }
}
