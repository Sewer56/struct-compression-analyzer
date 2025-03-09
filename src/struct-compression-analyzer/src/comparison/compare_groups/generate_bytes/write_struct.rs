use super::{GenerateBytesError, GenerateBytesResult};
use crate::{
    analyzer::AnalyzerFieldState,
    schema::{GroupComponent, GroupComponentStruct},
    utils::analyze_utils::{bit_writer_to_reader, BitReaderContainer},
};
use ahash::AHashMap;
use bitstream_io::{BitWrite, BitWriter, Endianness};
use core::cell::UnsafeCell;
use std::io::{self};

/// Processes an [`GroupComponentStruct`], writing its output to a
/// provided [`BitWriter`].
///
/// # Arguments
/// * `field_states` - A mutable reference to a map of field stats.
/// * `writer` - The bit writer to write the array to.
/// * `array` - Contains info about the array to write.
pub(crate) fn write_struct<TWrite: io::Write, TEndian: Endianness>(
    field_states: &mut AHashMap<String, AnalyzerFieldState>,
    writer: &mut BitWriter<TWrite, TEndian>,
    strct_ref: &GroupComponentStruct,
) -> GenerateBytesResult<()> {
    // Clone the struct definition to avoid mutating the original
    let mut strct = strct_ref.clone();
    let field_states_unsafe = UnsafeCell::new(field_states);

    // Map field names to their bitstream readers
    let mut field_readers = AHashMap::<String, BitReaderContainer>::new();

    // Initialize readers for all fields used in the struct
    for field in &mut strct.fields {
        let field_name = match field {
            GroupComponent::Array(_) | GroupComponent::Struct(_) => {
                return Err(GenerateBytesError::UnsupportedNestedComponent)
            }
            GroupComponent::Field(field) => Some(field.field.clone()),
            GroupComponent::Skip(skip) => Some(skip.field.clone()),
            GroupComponent::Padding(_) => None,
        };

        if let Some(field_name) = field_name {
            let field_states = unsafe { (*field_states_unsafe.get()).get_mut(&field_name) }
                .ok_or_else(|| GenerateBytesError::FieldNotFound(field_name.clone()))?;

            // Convert field's writer to a reader for reading stored bits
            field_readers.insert(
                field_name.clone(),
                bit_writer_to_reader(&mut field_states.writer),
            );

            // Set default bits if not specified in schema
            if let GroupComponent::Field(field) = field {
                field.set_bits(field_states.lenbits);
            };
        }
    }

    // Process struct components in a loop until no more data
    loop {
        let mut read_anything = false;

        for field in &strct.fields {
            match field {
                GroupComponent::Array(_) | GroupComponent::Struct(_) => {
                    return Err(GenerateBytesError::UnsupportedNestedComponent)
                }
                GroupComponent::Padding(padding) => {
                    writer
                        .write(padding.bits as u32, padding.value)
                        .map_err(|e| GenerateBytesError::WriteError {
                            source: e,
                            context: "writing padding bits".into(),
                        })?;
                }
                GroupComponent::Field(field) => {
                    let reader = field_readers
                        .get_mut(&field.field)
                        .ok_or_else(|| GenerateBytesError::FieldNotFound(field.field.clone()))?;

                    // Attempt read from source field
                    let read_result = reader.read(field.bits);
                    match read_result {
                        Ok(value) => {
                            // Only write if we successfully read the value
                            writer.write(field.bits, value).map_err(|e| {
                                GenerateBytesError::WriteError {
                                    source: e,
                                    context: format!(
                                        "writing {}-bit field '{}'",
                                        field.bits, field.field
                                    ),
                                }
                            })?;
                            read_anything = true;
                        }
                        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                            // Field is exhausted, continue processing other components
                        }
                        Err(e) => {
                            return Err(GenerateBytesError::ReadError {
                                source: e,
                                context: format!(
                                    "reading {}-bit field '{}'",
                                    field.bits, field.field
                                ),
                            })
                        }
                    }
                }
                GroupComponent::Skip(skip) => {
                    let reader = field_readers
                        .get_mut(&skip.field)
                        .ok_or_else(|| GenerateBytesError::FieldNotFound(skip.field.clone()))?;

                    // Attempt seek operation
                    let seek_result = reader.seek_bits(io::SeekFrom::Current(skip.bits as i64));
                    match seek_result {
                        Ok(_) => read_anything = true,
                        Err(e) if e.kind() == io::ErrorKind::UnexpectedEof => {
                            // Field is exhausted, continue processing other components
                        }
                        Err(e) => {
                            return Err(GenerateBytesError::SeekError {
                                source: e,
                                operation: format!(
                                    "skipping {} bits in field '{}'",
                                    skip.bits, skip.field
                                ),
                            })
                        }
                    }
                }
            }
        }

        if !read_anything {
            return Ok(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::comparison::compare_groups::test_helpers::create_mock_field_states;
    use crate::comparison::compare_groups::test_helpers::TEST_FIELD_NAME;
    use crate::schema::default_entropy_multiplier;
    use crate::schema::default_lz_match_multiplier;
    use crate::schema::BitOrder;
    use crate::schema::GroupComponentField;
    use crate::schema::GroupComponentPadding;
    use crate::schema::GroupComponentSkip;
    use bitstream_io::{BigEndian, BitWriter, LittleEndian};
    use std::io::Cursor;

    fn single_field_struct_group_component(bits: u32) -> GroupComponentStruct {
        GroupComponentStruct {
            fields: vec![GroupComponent::Field(GroupComponentField {
                field: TEST_FIELD_NAME.to_string(),
                bits,
            })],
            lz_match_multiplier: default_lz_match_multiplier(),
            entropy_multiplier: default_entropy_multiplier(),
        }
    }

    #[test]
    fn field_can_round_trip_lsb() {
        let input_data = [
            0b0010_0001, // 1, 2
            0b1000_0100, // 4, 8
        ];
        let mut field_states = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();

        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_states,
            &mut writer,
            &single_field_struct_group_component(0), // inherit from field
        )
        .unwrap();

        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn field_can_round_trip_msb() {
        let input_data = [
            0b0001_0010, // 1, 2
            0b0100_1000, // 4, 8
        ];
        let mut field_states = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Msb,
            BitOrder::Msb,
        );
        let mut output = Vec::new();

        let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);
        write_struct(
            &mut field_states,
            &mut writer,
            &single_field_struct_group_component(0), // inherit from field
        )
        .unwrap();

        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn field_can_read_slices_with_skip() {
        let input_data = [
            0b0010_1101, // 00, 11
            0b0000_1100, // 00, 11
        ];

        let expected_output = [0b00_11_00_11];

        let mut field_states = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();

        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_states,
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
                lz_match_multiplier: default_lz_match_multiplier(),
                entropy_multiplier: default_entropy_multiplier(),
            },
        )
        .unwrap();

        assert_eq!(expected_output, output.as_slice());
    }

    #[test]
    fn padding_writes_correct_bits_lsb() {
        let mut field_states =
            create_mock_field_states(TEST_FIELD_NAME, &[], 0, BitOrder::Lsb, BitOrder::Lsb);
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);
        write_struct(
            &mut field_states,
            &mut writer,
            &GroupComponentStruct {
                fields: vec![GroupComponent::Padding(GroupComponentPadding {
                    bits: 4,
                    value: 0b1010,
                })],
                lz_match_multiplier: default_lz_match_multiplier(),
                entropy_multiplier: default_entropy_multiplier(),
            },
        )
        .unwrap();
        writer.byte_align().unwrap();
        writer.flush().unwrap();
        assert_eq!(output, [0b0000_1010]);
    }

    #[test]
    fn padding_writes_correct_bits_msb() {
        let mut field_states =
            create_mock_field_states(TEST_FIELD_NAME, &[], 0, BitOrder::Msb, BitOrder::Msb);
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);
        write_struct(
            &mut field_states,
            &mut writer,
            &GroupComponentStruct {
                fields: vec![GroupComponent::Padding(GroupComponentPadding {
                    bits: 4,
                    value: 0b1010,
                })],
                lz_match_multiplier: default_lz_match_multiplier(),
                entropy_multiplier: default_entropy_multiplier(),
            },
        )
        .unwrap();
        writer.byte_align().unwrap();
        writer.flush().unwrap();
        assert_eq!(output, [0b1010_0000]);
    }
}
