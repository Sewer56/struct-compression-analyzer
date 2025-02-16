use crate::{
    analyze_utils::{bit_writer_to_reader, BitReaderContainer},
    analyzer::FieldStats,
    schema::{GroupComponent, GroupComponentStruct},
};
use ahash::AHashMap;
use bitstream_io::{BitWrite, BitWriter, Endianness};
use core::cell::UnsafeCell;
use std::io::{self};

/// Processes an [`GroupComponentStruct`], writing its output to a
/// provided [`BitWriter`].
///
/// # Arguments
/// * `field_stats` - A mutable reference to a map of field stats.
/// * `writer` - The bit writer to write the array to.
/// * `array` - Contains info about the array to write.
pub fn write_struct<TWrite: io::Write, TEndian: Endianness>(
    field_stats: &mut AHashMap<String, FieldStats>,
    writer: &mut BitWriter<TWrite, TEndian>,
    strct_ref: &GroupComponentStruct,
) {
    // Note: We mutate the struct here, by making number of bits inherited from schema fields,
    // so we clone as to keep the original `strct_ref` unmodified
    let mut strct = strct_ref.clone();
    let field_stats_unsafe = UnsafeCell::new(field_stats);

    // The state of each field reader.
    let mut field_readers = AHashMap::<String, BitReaderContainer>::new();

    // Map only the fields used in the struct, which will reduce the
    // hashmap size a bit, improving perf.
    for field in &mut strct.fields {
        let field_name = match field {
            GroupComponent::Array(_) | GroupComponent::Struct(_) => {
                panic!("Arrays and Structs inside structs are not supported")
            }
            GroupComponent::Field(field) => Some(field.field.clone()),
            GroupComponent::Skip(skip) => Some(skip.field.clone()),
            GroupComponent::Padding(_) => None,
        };

        // If we got a key and field_name, process it
        if let Some(field_name) = field_name {
            // SAFETY: UNDEFINED BEHAVIOUR BELOW.
            //         WE'RE BYPASSING BORROW CHECKER BECAUSE WE MANUALLY 'KNOW' WE CAN BORROW HERE
            //         BASED ON LIFETIME OF `field_states` being lower than `field_stats`
            let field_stats = unsafe { (*field_stats_unsafe.get()).get_mut(&field_name).unwrap() };
            field_readers.insert(field_name, bit_writer_to_reader(&mut field_stats.writer));

            // Populate the number of bits in the field.
            // This is ugly, this entire loop is, but it's fairly concise, at least.
            if let GroupComponent::Field(field) = field {
                field.set_bits(field_stats.lenbits);
            };
        }
    }
    field_readers.shrink_to_fit();

    // Run the struct in a loop.
    loop {
        // Note: It is true that we may insert some padding here when other fields are not inserted.
        //       That is okay. 1 byte is considered within margin of error.
        let mut read_anything = false;

        for field in &strct.fields {
            match field {
                GroupComponent::Array(_) | GroupComponent::Struct(_) => {
                    panic!("Arrays and Structs inside structs are not supported")
                }
                GroupComponent::Padding(_padding) => {
                    writer.write(_padding.bits as u32, _padding.value).unwrap()
                } // no-op
                GroupComponent::Field(field) => {
                    let reader = field_readers.get_mut(&field.field).unwrap();
                    let value = reader.read(field.bits);
                    if value.is_ok() {
                        writer.write(field.bits, value.unwrap()).unwrap();
                        read_anything = true;
                    }

                    // If read failed, then we reached end of stream, most likely.
                }
                GroupComponent::Skip(skip) => {
                    let reader = field_readers.get_mut(&skip.field).unwrap();
                    let seek_result = reader.seek_bits(io::SeekFrom::Current(skip.bits as i64));
                    if seek_result.is_ok() {
                        read_anything = true;
                    }
                }
            }
        }

        if !read_anything {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compare_groups::test_helpers::{create_mock_field_stats, TEST_FIELD_NAME},
        schema::{BitOrder, GroupComponentField, GroupComponentPadding, GroupComponentSkip},
    };
    use bitstream_io::{BigEndian, BitWriter, LittleEndian};
    use std::io::Cursor;

    fn single_field_struct_group_component(bits: u32) -> GroupComponentStruct {
        GroupComponentStruct {
            fields: vec![GroupComponent::Field(GroupComponentField {
                field: TEST_FIELD_NAME.to_string(),
                bits,
            })],
        }
    }

    #[test]
    fn field_can_round_trip_lsb() {
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

        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_stats,
            &mut writer,
            &single_field_struct_group_component(0), // inherit from field
        );

        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn field_can_round_trip_msb() {
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

        let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);
        write_struct(
            &mut field_stats,
            &mut writer,
            &single_field_struct_group_component(0), // inherit from field
        );

        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn field_can_read_slices_with_skip() {
        let input_data = [
            0b0010_1101, // 00, 11
            0b0000_1100, // 00, 11
        ];

        let expected_output = [0b00_11_00_11];

        let mut field_stats = create_mock_field_stats(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();

        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_stats,
            &mut writer,
            &GroupComponentStruct {
                fields: vec![
                    GroupComponent::Skip(GroupComponentSkip {
                        field: TEST_FIELD_NAME.to_string(),
                        bits: 2, // skip 2 bits
                    }),
                    GroupComponent::Field(GroupComponentField {
                        field: TEST_FIELD_NAME.to_string(),
                        bits: 2, // read 2 bits
                    }),
                ],
            },
        );

        assert_eq!(expected_output, output.as_slice());
    }

    #[test]
    fn padding_writes_correct_bits_lsb() {
        let mut field_stats =
            create_mock_field_stats(TEST_FIELD_NAME, &[], 0, BitOrder::Lsb, BitOrder::Lsb);
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_stats,
            &mut writer,
            &GroupComponentStruct {
                fields: vec![GroupComponent::Padding(GroupComponentPadding {
                    bits: 4,
                    value: 0b1010,
                })],
            },
        );
        writer.byte_align().unwrap();
        writer.flush().unwrap();
        assert_eq!(output, [0b0000_1010]);
    }

    #[test]
    fn padding_writes_correct_bits_msb() {
        let mut field_stats =
            create_mock_field_stats(TEST_FIELD_NAME, &[], 0, BitOrder::Msb, BitOrder::Msb);
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);
        write_struct(
            &mut field_stats,
            &mut writer,
            &GroupComponentStruct {
                fields: vec![GroupComponent::Padding(GroupComponentPadding {
                    bits: 4,
                    value: 0b1010,
                })],
            },
        );
        writer.byte_align().unwrap();
        writer.flush().unwrap();
        assert_eq!(output, [0b1010_0000]);
    }
}
