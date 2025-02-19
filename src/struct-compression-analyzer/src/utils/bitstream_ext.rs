use bitstream_io::{BitReader, Endianness};
use std::io::{self, Read, Seek, SeekFrom};

/// Extension trait for BitReader to add remaining bits functionality
pub(crate) trait BitReaderExt {
    /// Returns the number of bits remaining from the current position
    fn remaining_bits(&mut self) -> io::Result<u64>;
}

impl<R, E> BitReaderExt for BitReader<R, E>
where
    R: Read + Seek,
    E: Endianness,
{
    fn remaining_bits(&mut self) -> io::Result<u64> {
        // Store current position
        let current_pos = self.position_in_bits()?;

        // Get total size in bits
        let total_bits = self.seek_bits(SeekFrom::End(0))?;

        // Restore original position
        self.seek_bits(SeekFrom::Start(current_pos))?;

        // Calculate remaining bits
        Ok(total_bits - current_pos)
    }
}
