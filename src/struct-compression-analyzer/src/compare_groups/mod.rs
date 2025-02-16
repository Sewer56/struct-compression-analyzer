//! Module for comparing and writing structured data groups
//!
//! Provides functionality for processing arrays and structs as compression groups,
//! including bitstream writing implementations and comparison metrics.
//!
//! The module contains:
//! - `write_array`: Array-specific compression logic
//! - `write_struct`: Struct field compression implementation
//! - Core coordination logic in [`write_components`]

#[cfg(test)]
pub(crate) mod test_helpers;
pub mod write_array;
pub mod write_struct;

use crate::split_comparisons::GroupComparisonMetrics;
use crate::split_comparisons::GroupDifference;
use crate::{analyzer::FieldStats, schema::GroupComponent};
use ahash::AHashMap;
use bitstream_io::{BitWriter, Endianness};
use write_array::write_array;
use write_struct::write_struct;

/// Processes group components and writes them to a bitstream writer
///
/// # Parameters
/// - `field_stats`: Mutable reference to field statistics map
/// - `writer`: Bitstream writer implementing `std::io::Write`
/// - `components`: Slice of group components to process
///
/// # Panics
/// - If encountering any component type other than Array or Struct
pub fn generate_output_for_compare_groups_entry<TWrite: std::io::Write, TEndian: Endianness>(
    field_stats: &mut AHashMap<String, FieldStats>,
    writer: &mut BitWriter<TWrite, TEndian>,
    components: &[GroupComponent],
) {
    for component in components {
        match component {
            GroupComponent::Array(array) => write_array(field_stats, writer, array),
            GroupComponent::Struct(struct_) => write_struct(field_stats, writer, struct_),
            _ => panic!("Invalid top-level component type - only arrays and structs are allowed"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        compare_groups::test_helpers::{create_mock_field_stats, TEST_FIELD_NAME},
        schema::{BitOrder, GroupComponentArray, GroupComponentField, GroupComponentStruct},
    };
    use bitstream_io::{BitWriter, LittleEndian};
    use std::io::Cursor;

    #[test]
    fn can_write_array_component() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_stats(
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

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components);
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_write_struct_component() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_stats(
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

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components);
        assert_eq!(input_data, output.as_slice());
    }

    #[test]
    fn can_write_multiple_components() {
        let input_data = [0b0010_0001, 0b1000_0100];
        let mut field_stats = create_mock_field_stats(
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

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components);
        assert_eq!(
            &[input_data[0], input_data[1], input_data[0], input_data[1]],
            output.as_slice()
        );
    }

    #[test]
    #[should_panic(expected = "Invalid top-level component type")]
    fn panics_on_invalid_component_type() {
        let mut field_stats = AHashMap::new();
        let mut output = Vec::new();
        let mut writer = BitWriter::endian(Cursor::new(&mut output), LittleEndian);

        let components = vec![GroupComponent::Field(GroupComponentField {
            field: TEST_FIELD_NAME.to_string(),
            bits: 4,
        })];

        generate_output_for_compare_groups_entry(&mut field_stats, &mut writer, &components);
    }
}

/// The result of comparing 2 arbitrary groups of fields based on the schema.
#[derive(Clone)]
pub struct GroupComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// Metrics for the baseline group.
    pub baseline_metrics: GroupComparisonMetrics,
    /// Metrics for the other groups.
    pub group_metrics: Vec<GroupComparisonMetrics>,
    /// Comparison between other groups and first (baseline) group.
    pub differences: Vec<GroupDifference>,
}
