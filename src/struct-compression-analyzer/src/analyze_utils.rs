use lossless_transform_utils::{
    entropy::code_length_of_histogram32,
    histogram::{histogram32_from_bytes, Histogram32},
};

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
/// That is, zstandard at level 16.
pub fn get_zstd_compressed_size(data: &[u8]) -> usize {
    zstd::bulk::compress(data, 16)
        .ok()
        .map(|compressed| compressed.len())
        .unwrap()
}

/// Calculates the entropy of a given input
pub fn calculate_file_entropy(bytes: &[u8]) -> f64 {
    let mut histogram = Histogram32::default();
    histogram32_from_bytes(bytes, &mut histogram);
    code_length_of_histogram32(&histogram, bytes.len() as u64)
}

/// Reverses the bits of a u64 value
/// # Arguments
/// * `max_bits` - The number of bits to reverse
/// * `bits` - The bits to reverse
///
/// # Returns
/// The reversed bits
pub fn reverse_bits(max_bits: u32, bits: u64) -> u64 {
    let mut reversed_bits = 0u64;
    for x in 0..max_bits {
        if bits & (1 << x) != 0 {
            reversed_bits |= 1 << (max_bits - 1 - x);
        }
    }
    reversed_bits
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
