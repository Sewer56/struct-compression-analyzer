use super::{GenerateBytesError, GenerateBytesResult};
use crate::utils::{
    analyze_utils::{get_writer_buffer, BitWriterContainer},
    bitstream_ext::BitReaderExt,
};
use crate::{analyzer::AnalyzerFieldState, schema::GroupComponentArray};
use ahash::AHashMap;
use bitstream_io::{BigEndian, BitRead, BitReader, BitWrite, BitWriter, Endianness, LittleEndian};
use std::io::{self, Cursor, SeekFrom};

/// Processes an [`GroupComponentArray`], writing its output to a
/// provided [`BitWriter`].
///
/// # Arguments
/// * `bit_order` - The bit order to use when reading the data.
///   This is normally inherited from the schema root.
/// * `field_stats` - A mutable reference to a map of field stats.
/// * `writer` - The bit writer to write the array to.
/// * `array` - Contains info about the array to write.
pub(crate) fn write_array<TWrite: io::Write, TEndian: Endianness>(
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
    writer: &mut BitWriter<TWrite, TEndian>,
    array: &GroupComponentArray,
) -> GenerateBytesResult<()> {
    let field = field_stats
        .get_mut(&array.field)
        .ok_or_else(|| GenerateBytesError::FieldNotFound(array.field.clone()))?;

    let bits: u32 = array.get_bits(field);
    let offset = array.offset;
    let field_len = field.lenbits;
    match &field.writer {
        BitWriterContainer::Msb(_) => {
            let bytes = get_writer_buffer(&mut field.writer);
            let mut reader = BitReader::endian(Cursor::new(bytes), BigEndian);
            write_array_inner(&mut reader, bits, offset, field_len, writer)
        }
        BitWriterContainer::Lsb(_) => {
            let bytes = get_writer_buffer(&mut field.writer);
            let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
            write_array_inner(&mut reader, bits, offset, field_len, writer)
        }
    }
}

/// Processes an array component by reading bits from a field's stored data
/// and writing them to the output writer according to array configuration.
///
/// Handles both MSB and LSB bit orders by creating appropriate readers
/// from the field's stored bitstream data.
fn write_array_inner<
    TWrite: io::Write,
    TEndian: Endianness,
    TReader: io::Read + io::Seek,
    TReaderEndian: Endianness,
>(
    reader: &mut BitReader<TReader, TReaderEndian>,
    bits: u32,
    offset: u32,
    field_len: u32,
    writer: &mut BitWriter<TWrite, TEndian>,
) -> GenerateBytesResult<()> {
    // Loop until we run out of bits in the source field data
    loop {
        // Calculate ending position before reading to maintain alignment
        let ending_pos = reader
            .position_in_bits()
            .map_err(|e| GenerateBytesError::SeekError {
                source: e,
                operation: "getting array position".into(),
            })?
            + field_len as u64;

        // Check remaining bits before attempting read
        let remaining = reader
            .remaining_bits()
            .map_err(|e| GenerateBytesError::SeekError {
                source: e,
                operation: "checking remaining bits".into(),
            })?;

        if remaining < field_len as u64 {
            return Ok(());
        }

        // Seek to the array element offset
        reader
            .seek_bits(SeekFrom::Current(offset as i64))
            .map_err(|e| GenerateBytesError::SeekError {
                source: e,
                operation: format!("seeking to array offset {}", offset),
            })?;

        // Read the actual value from the source bitstream
        let value = reader
            .read::<u64>(bits)
            .map_err(|e| GenerateBytesError::ReadError {
                source: e,
                context: format!("reading {bits}-bit array element"),
            })?;

        // Write the value to the output stream
        writer
            .write::<u64>(bits, value)
            .map_err(|e| GenerateBytesError::WriteError {
                source: e,
                context: format!("writing {bits}-bit array element"),
            })?;

        // Return to calculated end position for next iteration
        reader.seek_bits(SeekFrom::Start(ending_pos)).map_err(|e| {
            GenerateBytesError::SeekError {
                source: e,
                operation: format!("seeking to array end position {}", ending_pos),
            }
        })?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison::compare_groups::test_helpers::create_mock_field_states;
    use crate::comparison::compare_groups::test_helpers::TEST_FIELD_NAME;
    use crate::schema::BitOrder;
    use bitstream_io::BitWriter;
    use std::io::Cursor;

    fn test_array_group_component(offset: u32, bits: u32) -> GroupComponentArray {
        GroupComponentArray {
            field: TEST_FIELD_NAME.to_string(),
            offset,
            bits,
        }
    }

    #[test]
    fn can_round_trip_lsb() {
        // Binary fields: LSB first (rightmost bit is first)
        let input_data = [
            0b0010_0001, // 1, 2
            0b1000_0100, // 4, 8
        ];
        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();

        // Write using LSB
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_array(
            &mut field_stats,
            &mut writer,
            &test_array_group_component(0, 4), // inherit bit count from field
        )
        .unwrap();

        // Read back written data
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_round_trip_msb() {
        // Binary fields: MSB first (rightmost bit is first)
        let input_data = [
            0b0001_0010, // 1, 2
            0b0100_1000, // 4, 8
        ];
        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Msb,
            BitOrder::Msb,
        );
        let mut output = Vec::new();

        // Write using MSB
        let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);
        write_array(
            &mut field_stats,
            &mut writer,
            &test_array_group_component(0, 0), // inherit bit count from field
        )
        .unwrap();

        // Read back written data
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_read_slices() {
        // Binary fields: LSB first (rightmost bit is first)
        let input_data = [
            0b0010_1101, // 00, 11
            0b0000_1100, // 00, 11
        ];
        // Note: Regardless of the slice read however, after each read is done, the stream will be advanced to the
        // next field.
        let expected_output = [
            0b00_11_00_11, // 00, 11, 00, 11
        ]; // rest is dropped, because we offset by 2

        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();

        // Write using LSB
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_array(
            &mut field_stats,
            &mut writer,
            &test_array_group_component(2, 2), // only upper 2 bits.
        )
        .unwrap();

        // Read back written data
        assert_eq!(expected_output, output.as_slice());
    }
}
