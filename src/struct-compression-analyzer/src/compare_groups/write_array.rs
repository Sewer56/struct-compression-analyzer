use crate::{
    analyze_utils::get_writer_buffer,
    analyzer::FieldStats,
    bitstream_ext::BitReaderExt,
    schema::{BitOrder, GroupComponentArray},
};
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
pub fn write_array<TWrite: io::Write, TEndian: Endianness>(
    bit_order: BitOrder,
    field_stats: &mut AHashMap<String, FieldStats>,
    writer: &mut BitWriter<TWrite, TEndian>,
    array: &GroupComponentArray,
) {
    // Gets the field details for the given field
    let field = field_stats
        .get_mut(&array.field)
        .expect("Field not found in field stats");

    let bits: u32 = array.get_bits(field);
    let offset = array.offset;
    let field_len = field.lenbits;
    let bytes = get_writer_buffer(&mut field.writer);
    if bit_order == BitOrder::Lsb {
        let mut reader = BitReader::endian(Cursor::new(bytes), LittleEndian);
        write_array_inner(&mut reader, bits, offset, field_len, writer);
    } else {
        let mut reader = BitReader::endian(Cursor::new(bytes), BigEndian);
        write_array_inner(&mut reader, bits, offset, field_len, writer);
    };
}

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
) {
    loop {
        // Position after reading the field.
        let ending_pos = reader.position_in_bits().unwrap() + field_len as u64;

        // Check if there's enough data to read
        if reader.remaining_bits().unwrap_or(0) < field_len as u64 {
            return;
        }

        // Seek to offset to read from.
        reader.seek_bits(SeekFrom::Current(offset as i64)).unwrap();

        // Read the bits
        let value = reader.read::<u64>(bits).unwrap();

        // Write the bits
        writer.write::<u64>(bits, value).unwrap();

        // Seek to loop end
        reader.seek_bits(SeekFrom::Start(ending_pos)).unwrap();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compare_groups::test_helpers::{create_mock_field_stats, TEST_FIELD_NAME};
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
        let mut field_stats = create_mock_field_stats(
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
            BitOrder::Lsb,
            &mut field_stats,
            &mut writer,
            &test_array_group_component(0, 4), // inherit bit count from field
        );

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
        let mut field_stats = create_mock_field_stats(
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
            BitOrder::Msb,
            &mut field_stats,
            &mut writer,
            &test_array_group_component(0, 0), // inherit bit count from field
        );

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

        let mut field_stats = create_mock_field_stats(
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
            BitOrder::Lsb,
            &mut field_stats,
            &mut writer,
            &test_array_group_component(2, 2), // only upper 2 bits.
        );

        // Read back written data
        assert_eq!(expected_output, output.as_slice());
    }
}
