//! Generates byte streams from schema-defined field groups for compression analysis.
//!
//! This module handles the core functionality of converting schema field group definitions
//! into analyzable byte streams. It provides specialized handling for different group
//! component types and manages bit-level operations for field transformations.
//!
//! # Core Types
//!
//! - [`GenerateBytesError`]: Comprehensive error handling for byte generation
//! - [`GenerateBytesResult`]: Type alias for Result with GenerateBytesError
//!
//! # Internal Functions
//!
//! Two primary internal functions handle byte generation:
//!
//! - `generate_group_bytes`: Creates a Vec<u8> from group components
//! - `generate_output_for_compare_groups_entry`: Writes directly to a provided bitstream
//!
//! # Component Types
//!
//! The module handles two primary component types:
//!
//! - Arrays: Sequential field values with optional bit slicing
//! - Structs: Grouped fields with padding and alignment
//!
//! # Error Handling
//!
//! Comprehensive error handling covers:
//! - Field lookup failures
//! - Bit alignment issues
//! - Read/write operations
//! - Invalid component configurations
//!
//! # Implementation Notes
//!
//! - Handles both MSB and LSB bit ordering
//! - Supports partial field reads via offset/bits
//!
//! # Submodules
//!
//! - [`write_array`]: Array component processing
//! - [`write_struct`]: Struct component processing
//!
//! [`GenerateBytesError`]: crate::comparison::compare_groups::generate_bytes::GenerateBytesError
//! [`GenerateBytesResult`]: crate::comparison::compare_groups::generate_bytes::GenerateBytesResult
//! [`write_array`]: crate::comparison::compare_groups::generate_bytes::write_array
//! [`write_struct`]: crate::comparison::compare_groups::generate_bytes::write_struct
use thiserror::Error;
mod write_array;
mod write_struct;

pub(crate) type GenerateBytesResult<T> = std::result::Result<T, GenerateBytesError>;
use crate::comparison::compare_groups::generate_bytes::write_array::write_array;
use crate::comparison::compare_groups::generate_bytes::write_struct::write_struct;
use crate::{analyzer::AnalyzerFieldState, schema::GroupComponent};
use ahash::AHashMap;
use bitstream_io::{BigEndian, BitWrite, BitWriter, Endianness};
use std::io::Cursor;

/// Errors that can occur while generating bytes from a schema for analysis
#[derive(Error, Debug)]
pub enum GenerateBytesError {
    #[error("Invalid component type - {0}")]
    InvalidComponentType(String),

    #[error("Failed to byte align writer: {0}")]
    ByteAlignmentFailed(#[source] std::io::Error),

    #[error("Field '{0}' not found in field stats")]
    FieldNotFound(String),

    #[error("Read error while {context}: {source}")]
    ReadError {
        #[source]
        source: std::io::Error,
        context: String,
    },

    #[error("Write error while {context}: {source}")]
    WriteError {
        #[source]
        source: std::io::Error,
        context: String,
    },

    #[error("Seek error during {operation}: {source}")]
    SeekError {
        #[source]
        source: std::io::Error,
        operation: String,
    },

    #[error("Nested structure contains unsupported component type. Nested arrays and structs are not allowed within structs.")]
    UnsupportedNestedComponent,
}

/// Processes group components and writes them to a bitstream writer
///
/// # Parameters
/// - `field_stats`: Mutable reference to field statistics map
/// - `writer`: Bitstream writer implementing `std::io::Write`
/// - `components`: Slice of group components to process
///
/// # Panics
/// - If encountering any component type other than Array or Struct
pub(crate) fn generate_output_for_compare_groups_entry<
    TWrite: std::io::Write,
    TEndian: Endianness,
>(
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
    writer: &mut BitWriter<TWrite, TEndian>,
    components: &[GroupComponent],
) -> GenerateBytesResult<()> {
    for component in components {
        match component {
            GroupComponent::Array(array) => write_array(field_stats, writer, array)?,
            GroupComponent::Struct(struct_) => write_struct(field_stats, writer, struct_)?,
            _ => {
                return Err(GenerateBytesError::InvalidComponentType(
                    "Only arrays and structs are allowed at top level".into(),
                ))
            }
        }
    }
    Ok(())
}

pub(crate) fn generate_group_bytes(
    components: &[GroupComponent],
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
) -> GenerateBytesResult<Vec<u8>> {
    let mut output = Vec::new();
    let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);

    generate_output_for_compare_groups_entry(field_stats, &mut writer, components)?;
    writer
        .byte_align()
        .map_err(GenerateBytesError::ByteAlignmentFailed)?;
    Ok(output)
}

#[cfg(test)]
mod generate_output_tests {
    use super::*;
    use crate::comparison::compare_groups::test_helpers::create_mock_field_states;
    use crate::comparison::compare_groups::test_helpers::TEST_FIELD_NAME;
    use crate::schema::BitOrder;
    use crate::schema::GroupComponentArray;
    use crate::schema::GroupComponentField;
    use crate::schema::GroupComponentStruct;
    use ahash::AHashMap;
    use bitstream_io::{BitWriter, LittleEndian};
    use std::io::Cursor;

    #[test]
    fn can_write_array_component() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);

        let components = vec![GroupComponent::Array(GroupComponentArray {
            field: TEST_FIELD_NAME.to_string(),
            offset: 0,
            bits: 4,
        })];

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components)
            .unwrap();
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_write_struct_component() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);

        let components = vec![GroupComponent::Struct(GroupComponentStruct {
            fields: vec![GroupComponent::Field(GroupComponentField {
                field: TEST_FIELD_NAME.to_string(),
                bits: 4,
            })],
        })];

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components)
            .unwrap();
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_write_multiple_components() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_states(
            TEST_FIELD_NAME,
            &input_data,
            4,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);

        let components = vec![
            GroupComponent::Array(GroupComponentArray {
                field: TEST_FIELD_NAME.to_string(),
                offset: 0,
                bits: 4,
            }),
            GroupComponent::Struct(GroupComponentStruct {
                fields: vec![GroupComponent::Field(GroupComponentField {
                    field: TEST_FIELD_NAME.to_string(),
                    bits: 4,
                })],
            }),
        ];

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components)
            .unwrap();
        assert_eq!(
            &[input_data[0], input_data[1], input_data[0], input_data[1]],
            output.as_slice()
        );
    }

    #[test]
    #[should_panic]
    fn panics_on_invalid_component_type() {
        let mut field_stats = AHashMap::new();
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);

        let components = vec![GroupComponent::Field(GroupComponentField {
            field: TEST_FIELD_NAME.to_string(),
            bits: 4,
        })];

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components)
            .unwrap();
    }
}
