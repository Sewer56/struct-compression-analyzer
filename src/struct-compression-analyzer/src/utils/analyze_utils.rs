//! Utility functions for analyzing and processing bit-packed data.
//!
//! This module provides low-level utilities for:
//! - Size estimation and compression
//! - Bit manipulation and ordering
//! - Bitstream reader/writer creation and management
//!
//! # Core Functions
//!
//! - [`size_estimate`]: Estimates compressed data size based on LZ matches and entropy
//! - [`get_zstd_compressed_size`]: Calculates actual compressed size using zstandard
//! - [`calculate_file_entropy`]: Computes Shannon entropy of input data
//! - [`reverse_bits`]: Reverses bits in a u64 value
//!
//! # Bitstream Utilities
//!
//! - [`create_bit_reader`]: Creates a [`BitReaderContainer`] with specified endianness
//! - [`create_bit_writer`]: Creates a [`BitWriterContainer`] with specified endianness
//! - [`create_bit_writer_with_owned_data`]: Creates writer containing copied data
//! - [`get_writer_buffer`]: Retrieves underlying buffer from a writer
//! - [`bit_writer_to_reader`]: Converts a writer into a reader
//!
//! # Types
//!
//! - [`BitReaderContainer`]: Wrapper around bit readers supporting both endians
//! - [`BitWriterContainer`]: Wrapper around bit writers supporting both endians

use crate::{analyzer::SizeEstimationParameters, schema::BitOrder};
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter, LittleEndian};
use lossless_transform_utils::{
    entropy::code_length_of_histogram32,
    histogram::{histogram32_from_bytes, Histogram32},
};
use std::io::{self, Cursor, SeekFrom};

/// Estimate size of a compressed data based on precalculated LZ matches and entropy
///
/// # Arguments
///
/// * `params` - [`SizeEstimationParameters`] containing:
///     * `data_len` - The uncompressed data length
///     * `num_lz_matches` - The number of LZ matches
///     * `entropy` - The estimated entropy of the data
///     * `lz_match_multiplier` - Multiplier for LZ matches
///     * `entropy_multiplier` - Multiplier for entropy
///
/// # Returns
///
/// This is a rough estimation based on very limited testing on DXT1, only, you'll want to
/// replace this function with something more suitable for your use case, possibly.
pub fn size_estimate(params: SizeEstimationParameters) -> usize {
    // Calculate expected bytes after LZ
    let bytes_after_lz =
        params.data_len - (params.num_lz_matches as f64 * params.lz_match_multiplier) as usize;

    // Calculate expected bits and convert to bytes
    (bytes_after_lz as f64 * params.entropy * params.entropy_multiplier).ceil() as usize / 8
}

/// Determines the actual size of the compressed data by compressing with a realistic compressor.
pub fn get_zstd_compressed_size(data: &[u8], level: i32) -> u64 {
    zstd::bulk::compress(data, level)
        .ok()
        .map(|compressed| compressed.len())
        .unwrap() as u64
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

impl BitReaderContainer<'_> {
    pub fn read(&mut self, bits: u32) -> io::Result<u64> {
        match self {
            BitReaderContainer::Msb(reader) => reader.read(bits),
            BitReaderContainer::Lsb(reader) => reader.read(bits),
        }
    }

    pub fn seek_bits(&mut self, seekfrom: SeekFrom) -> io::Result<u64> {
        match self {
            BitReaderContainer::Msb(reader) => reader.seek_bits(seekfrom),
            BitReaderContainer::Lsb(reader) => reader.seek_bits(seekfrom),
        }
    }
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

/// Converts a [`BitWriterContainer`] instance into a [`BitReaderContainer`] instance.
///
/// # Arguments
///
/// * `writer` - The [`BitWriterContainer`] instance to convert.
///
/// # Returns
/// A [`BitReaderContainer`] instance containing the same data as the input [`BitWriterContainer`].
pub fn bit_writer_to_reader(writer: &mut BitWriterContainer) -> BitReaderContainer<'_> {
    match writer {
        BitWriterContainer::Msb(writer) => {
            writer.byte_align().unwrap();
            let array = writer.writer().unwrap().get_ref();
            BitReaderContainer::Msb(BitReader::endian(Cursor::new(array), BigEndian))
        }
        BitWriterContainer::Lsb(writer) => {
            writer.byte_align().unwrap();
            let array = writer.writer().unwrap().get_ref();
            BitReaderContainer::Lsb(BitReader::endian(Cursor::new(array), LittleEndian))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zstd_compression_estimate() {
        let data = b"This is a test string that should compress well with zstandard zstandard zstandard zstandard zstandard zstandard";
        let compressed_size = get_zstd_compressed_size(data, 16);
        assert!(compressed_size < data.len() as u64);
    }
}
