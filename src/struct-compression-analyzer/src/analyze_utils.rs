/// Estimate size of a compressed data based on precalculated LZ matches and entropy
///
/// Arguments:
/// * `data` - The uncompressed data
/// * `num_lz_matches` - The number of LZ matches
/// * `entropy` - The estimated entropy of the data
///
/// Returns: The estimated size of the compressed data in bytes
pub fn size_estimate(data: &[u8], num_lz_matches: usize, entropy: f64) -> usize {
    // Calculate expected bytes after LZ
    let bytes_after_lz = data.len() - (num_lz_matches as f64 * 0.375f64) as usize;

    // Calculate expected bits and convert to bytes
    (bytes_after_lz as f64 * entropy).ceil() as usize / 8
}

/// Determines the actual size of the compressed data by compressing with a realistic compressor.
/// That is, zstandard at level 9.
pub fn get_zstd_compressed_size(data: &[u8]) -> usize {
    zstd::bulk::compress(data, 9)
        .ok()
        .map(|compressed| compressed.len())
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_compression_estimate() {
        let data = b"This is a test string that should compress well with zstandard zstandard zstandard zstandard zstandard zstandard";
        let compressed_size = get_zstd_compressed_size(data);
        assert!(compressed_size < data.len());
    }
}
