//! Strict KTX2 subset: six 2-D layers, complete mip chain, no supercompression.
use std::ops::Range;

pub struct Texture<'a> {
    pub bytes: &'a [u8],
    pub edge: u32,
    pub vk_format: u32,
    pub levels: Vec<Range<usize>>,
    pub gpu_bytes: u64,
}

impl<'a> Texture<'a> {
    pub fn parse(
        bytes: &'a [u8],
        expected_edge: u32,
        expected_format: u32,
    ) -> Result<Self, String> {
        let error = || "invalid or unsupported terrain KTX2".to_string();
        if bytes.len() < 80 || &bytes[..12] != b"\xABKTX 20\xBB\r\n\x1A\n" {
            return Err(error());
        }
        let u32_at = |i| u32::from_le_bytes(bytes[i..i + 4].try_into().unwrap());
        let u64_at = |i| u64::from_le_bytes(bytes[i..i + 8].try_into().unwrap());
        let edge = u32_at(20);
        let format = u32_at(12);
        if ![2048, 4096].contains(&edge)
            || edge != expected_edge
            || format != expected_format
            || u32_at(16) != 1
            || u32_at(24) != edge
            || u32_at(28) != 0
            || u32_at(32) != 6
            || u32_at(36) != 1
            || u32_at(40) != edge.ilog2() + 1
            || u32_at(44) != 0
            || u64_at(64) != 0
            || u64_at(72) != 0
        {
            return Err(error());
        }
        let count = u32_at(40) as usize;
        let index_end = 80 + count * 24;
        if bytes.len() < index_end {
            return Err(error());
        }
        let dfd_start = u32_at(48) as usize;
        let dfd_end = dfd_start
            .checked_add(u32_at(52) as usize)
            .ok_or_else(error)?;
        if dfd_start < index_end || dfd_end > bytes.len() || dfd_end - dfd_start < 28 {
            return Err(error());
        }
        let mut levels = Vec::new();
        let mut gpu_bytes = 0;
        for level in 0..count {
            let size = (edge >> level).max(1) as u64;
            let expected = match format {
                37 | 43 => size * size * 4 * 6,
                131 | 132 => size.div_ceil(4).pow(2) * 8 * 6,
                137 | 141 => size.div_ceil(4).pow(2) * 16 * 6,
                _ => return Err(error()),
            };
            let offset = u64_at(80 + level * 24);
            let length = u64_at(88 + level * 24);
            let end = offset.checked_add(length).ok_or_else(error)?;
            if length != expected
                || u64_at(96 + level * 24) != expected
                || offset < dfd_end as u64
                || offset % 8 != 0
                || end > bytes.len() as u64
                || levels
                    .iter()
                    .any(|r: &Range<usize>| offset < r.end as u64 && end > r.start as u64)
            {
                return Err(error());
            }
            levels.push(offset as usize..end as usize);
            gpu_bytes += expected;
        }
        Ok(Self {
            bytes,
            edge,
            vk_format: format,
            levels,
            gpu_bytes,
        })
    }
}
