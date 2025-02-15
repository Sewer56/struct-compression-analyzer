use crate::schema::BitOrder;
use bitstream_io::{BigEndian, BitReader, BitWrite, BitWriter, LittleEndian};
use lossless_transform_utils::{
    entropy::code_length_of_histogram32,
    histogram::{histogram32_from_bytes, Histogram32},
};
use std::io::Cursor;

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

/// Wrapper around the `BitReader` type that allows it to be used with either endian.
pub enum BitReaderContainer<'a> {
    Msb(BitReader<Cursor<&'a [u8]>, BigEndian>),
    Lsb(BitReader<Cursor<&'a [u8]>, LittleEndian>),
}

/// Creates a [`BitReaderContainer`] instance based on the given [`BitOrder`].
///
/// # Arguments
///
/// * `data` - The data to create the bit reader from.
/// * `bit_order` - The endianness of the bit stream.
///
/// # Returns
/// A [`BitReaderContainer`] instance with the specified endianness.
pub fn create_bit_reader(data: &[u8], bit_order: BitOrder) -> BitReaderContainer<'_> {
    match bit_order {
        BitOrder::Default | BitOrder::Msb => {
            BitReaderContainer::Msb(BitReader::endian(Cursor::new(data), BigEndian))
        }
        BitOrder::Lsb => {
            BitReaderContainer::Lsb(BitReader::endian(Cursor::new(data), LittleEndian))
        }
    }
}

/// Tracks statistics about individual bits in a field
///
/// Maintains counts of zero and one values observed at each bit position
/// to support entropy calculations and bit distribution analysis.
pub enum BitWriterContainer {
    Msb(BitWriter<Cursor<Vec<u8>>, BigEndian>),
    Lsb(BitWriter<Cursor<Vec<u8>>, LittleEndian>),
}

/// Creates a [`BitWriterContainer`] instance based on the given [`BitOrder`].
///
/// # Arguments
///
/// * `bit_order` - The endianness of the bit stream.
///
/// # Returns
/// A [`BitWriterContainer`] instance with the specified endianness.
pub fn create_bit_writer(bit_order: BitOrder) -> BitWriterContainer {
    match bit_order {
        BitOrder::Default | BitOrder::Msb => {
            BitWriterContainer::Msb(BitWriter::endian(Cursor::new(Vec::new()), BigEndian))
        }
        BitOrder::Lsb => {
            BitWriterContainer::Lsb(BitWriter::endian(Cursor::new(Vec::new()), LittleEndian))
        }
    }
}

/// Creates a [`BitWriterContainer`] instance based on the given [`BitOrder`].
/// This copies the supplied data into a new buffer, which is then owned by the container.
///
/// # Arguments
///
/// * `data` - The data to create the bit reader from.
/// * `bit_order` - The endianness of the bit stream.
///
/// # Returns
/// A [`BitWriterContainer`] instance with the specified endianness.
pub fn create_bit_writer_with_owned_data(data: &[u8], bit_order: BitOrder) -> BitWriterContainer {
    match bit_order {
        BitOrder::Default | BitOrder::Msb => {
            let mut cursor = Cursor::new(data.to_vec());
            cursor.set_position(data.len() as u64);
            BitWriterContainer::Msb(BitWriter::endian(cursor, BigEndian))
        }
        BitOrder::Lsb => {
            let mut cursor = Cursor::new(data.to_vec());
            cursor.set_position(data.len() as u64);
            BitWriterContainer::Lsb(BitWriter::endian(cursor, LittleEndian))
        }
    }
}

/// Retrieves the buffer behind a [`BitWriterContainer`] instance.
///
/// # Arguments
///
/// * `writer` - The [`BitWriterContainer`] instance to retrieve the buffer from.
///
/// # Returns
/// A reference to the buffer behind the [`BitWriterContainer`] instance.
pub fn get_writer_buffer(writer: &mut BitWriterContainer) -> &[u8] {
    match writer {
        BitWriterContainer::Msb(writer) => {
            writer.byte_align().unwrap();
            writer.writer().unwrap().get_ref()
        }
        BitWriterContainer::Lsb(writer) => {
            writer.byte_align().unwrap();
            writer.writer().unwrap().get_ref()
        }
    }
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
