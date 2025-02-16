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

use crate::schema::Schema;
use crate::split_comparisons::GroupComparisonMetrics;
use crate::split_comparisons::GroupDifference;
use crate::{
    analyzer::FieldStats,
    schema::{CustomComparison, GroupComponent},
};
use ahash::AHashMap;
use bitstream_io::BitWrite;
use bitstream_io::{BigEndian, BitWriter, Endianness};
use std::io::Cursor;
use write_array::write_array;
use write_struct::write_struct;

use crate::analyze_utils::{calculate_file_entropy, get_zstd_compressed_size, size_estimate};
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;

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

/// Analyzes all custom comparisons defined in the schema
///
/// # Arguments
///
/// * `schema` - Reference to loaded schema definition
/// * `field_stats` - Mutable reference to field statistics map
/// * `field_metrics` - Immutable reference to field analysis metrics
///
/// # Returns
///
/// Vector of [`GroupComparisonResult`] containing metrics for all configured comparisons
pub fn analyze_custom_comparisons(
    schema: &Schema,
    field_stats: &mut AHashMap<String, FieldStats>,
) -> Vec<GroupComparisonResult> {
    schema
        .analysis
        .compare_groups
        .iter()
        .map(|comparison| GroupComparisonResult::from_custom_comparison(comparison, field_stats))
        .collect()
}

fn generate_group_bytes(
    components: &[GroupComponent],
    field_stats: &mut AHashMap<String, FieldStats>,
) -> Vec<u8> {
    let mut output = Vec::new();
    let mut writer = BitWriter::endian(Cursor::new(&mut output), BigEndian);

    generate_output_for_compare_groups_entry(field_stats, &mut writer, components);
    writer.byte_align().unwrap();
    output
}

fn calculate_group_metrics(bytes: &[u8]) -> GroupComparisonMetrics {
    let entropy = calculate_file_entropy(bytes);
    let lz_matches = estimate_num_lz_matches_fast(bytes) as u64;
    let estimated_size = size_estimate(bytes, lz_matches as usize, entropy) as u64;
    let zstd_size = get_zstd_compressed_size(bytes) as u64;

    GroupComparisonMetrics {
        lz_matches,
        entropy,
        estimated_size,
        zstd_size,
        original_size: bytes.len() as u64,
    }
}

/// Contains the result of comparing custom field groupings defined in the schema.
#[derive(Clone)]
pub struct GroupComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// Metrics for the baseline group.
    pub baseline_metrics: GroupComparisonMetrics,
    /// Names of the comparison groups in order they were specified in the schema
    pub group_names: Vec<String>,
    /// Metrics for the comparison groups in schema order
    pub group_metrics: Vec<GroupComparisonMetrics>,
    /// Comparison between other groups and first (baseline) group.
    pub differences: Vec<GroupDifference>,
}

impl GroupComparisonResult {
    /// Creates comparison results for a single custom comparison definition from the schema.
    ///
    /// # Arguments
    ///
    /// * `comparison` - Custom group comparison configuration from schema
    /// * `field_stats` - Mutable reference to field statistics map
    /// * `field_metrics` - Immutable reference to field analysis metrics
    ///
    /// # Panics
    ///
    /// Will panic if called with invalid group components that can't be processed
    pub fn from_custom_comparison(
        comparison: &CustomComparison,
        field_stats: &mut AHashMap<String, FieldStats>,
    ) -> Self {
        let mut results = Vec::new();
        let mut differences = Vec::new();
        let mut group_names = Vec::new();

        // Process baseline group first
        let baseline_bytes = generate_group_bytes(&comparison.baseline, field_stats);
        let baseline_metrics = calculate_group_metrics(&baseline_bytes);

        // Process each comparison group
        for (group_name, components) in &comparison.comparisons {
            group_names.push(group_name.clone());
            let comparison_bytes = generate_group_bytes(components, field_stats);
            let comparison_metrics = calculate_group_metrics(&comparison_bytes);

            // Calculate differences
            differences.push(GroupDifference {
                lz_matches: comparison_metrics.lz_matches as i64
                    - baseline_metrics.lz_matches as i64,
                entropy: comparison_metrics.entropy - baseline_metrics.entropy,
                estimated_size: comparison_metrics.estimated_size as i64
                    - baseline_metrics.estimated_size as i64,
                zstd_size: comparison_metrics.zstd_size as i64 - baseline_metrics.zstd_size as i64,
                original_size: comparison_metrics.original_size as i64
                    - baseline_metrics.original_size as i64,
            });

            results.push(GroupComparisonMetrics {
                lz_matches: comparison_metrics.lz_matches,
                entropy: comparison_metrics.entropy,
                estimated_size: comparison_metrics.estimated_size,
                zstd_size: comparison_metrics.zstd_size,
                original_size: comparison_metrics.original_size,
            });
        }

        GroupComparisonResult {
            name: comparison.name.clone(),
            description: comparison.description.clone(),
            baseline_metrics: GroupComparisonMetrics {
                lz_matches: baseline_metrics.lz_matches,
                entropy: baseline_metrics.entropy,
                estimated_size: baseline_metrics.estimated_size,
                zstd_size: baseline_metrics.zstd_size,
                original_size: baseline_metrics.original_size,
            },
            group_names,
            group_metrics: results,
            differences,
        }
    }
}

#[cfg(test)]
mod generate_output_tests {
    use super::*;
    use crate::{
        compare_groups::test_helpers::{create_mock_field_stats, TEST_FIELD_NAME},
        schema::{
            BitOrder, GroupComponent, GroupComponentArray, GroupComponentField,
            GroupComponentStruct,
        },
    };
    use ahash::AHashMap;
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

#[cfg(test)]
mod from_custom_comparison_tests {
    use indexmap::IndexMap;

    use super::*;
    use crate::{
        compare_groups::test_helpers::{create_mock_field_stats, TEST_FIELD_NAME},
        schema::{BitOrder, CustomComparison, GroupComponent, GroupComponentArray},
    };

    #[test]
    fn from_custom_comparison_basic() {
        let input_data = [0b1010_1010, 0b0101_0101];
        let mut field_stats = create_mock_field_stats(
            TEST_FIELD_NAME,
            &input_data,
            8,
            BitOrder::Lsb,
            BitOrder::Lsb,
        );

        let comparison = CustomComparison {
            name: "test_comp".to_string(),
            description: "test comparison".to_string(),
            baseline: vec![GroupComponent::Array(GroupComponentArray {
                field: TEST_FIELD_NAME.to_string(),
                offset: 0,
                bits: 8,
            })],
            comparisons: {
                let mut map = IndexMap::new();
                map.insert(
                    "comp1".to_string(),
                    vec![GroupComponent::Array(GroupComponentArray {
                        field: TEST_FIELD_NAME.to_string(),
                        offset: 0,
                        bits: 4,
                    })],
                );
                map
            },
        };

        let result = GroupComparisonResult::from_custom_comparison(&comparison, &mut field_stats);

        // Note: The 'zstd' and 'estimated size' numbers may randomly break with parameter changes.
        //       This is OK, we hardcoded them here for sanity test only.
        // Validate baseline metrics
        assert_eq!(result.baseline_metrics.original_size, 2); // input_data
        assert_eq!(result.baseline_metrics.zstd_size, 11); // Zstd has overhead
        assert_eq!(result.baseline_metrics.estimated_size, 0); // Arbitrary, and can change.
        assert_eq!(result.baseline_metrics.entropy, 1.0); // 2 different bytes == entropy of 1

        // Validate comparison group
        assert_eq!(result.group_names, vec!["comp1"]);
        let comp_metrics = &result.group_metrics[0];
        assert_eq!(comp_metrics.original_size, 1); // Half the data.
        assert_eq!(comp_metrics.zstd_size, 10); // Zstd has overhead
        assert_eq!(comp_metrics.entropy, 0.0); // One byte == entropy of 0

        // Validate differences
        let diff = &result.differences[0];
        assert_eq!(diff.original_size, -1);
        assert_eq!(diff.zstd_size, -1);
        assert_eq!(diff.entropy, -1.0);
    }

    #[test]
    fn from_custom_comparison_multiple_groups() {
        let input_data = [0b1111_0000];
        let mut field_stats = create_mock_field_stats(
            TEST_FIELD_NAME,
            &input_data,
            8,
            BitOrder::Msb,
            BitOrder::Msb,
        );

        let comparison = CustomComparison {
            name: "multi_group".to_string(),
            description: String::new(),
            baseline: vec![GroupComponent::Array(GroupComponentArray {
                field: TEST_FIELD_NAME.to_string(),
                offset: 0,
                bits: 8,
            })],
            comparisons: {
                let mut map = IndexMap::new();
                map.insert(
                    "half_bits".to_string(),
                    vec![GroupComponent::Array(GroupComponentArray {
                        field: TEST_FIELD_NAME.to_string(),
                        offset: 0,
                        bits: 4,
                    })],
                );
                map.insert(
                    "full_bits".to_string(),
                    vec![GroupComponent::Array(GroupComponentArray {
                        field: TEST_FIELD_NAME.to_string(),
                        offset: 0,
                        bits: 8,
                    })],
                );
                map
            },
        };

        let result = GroupComparisonResult::from_custom_comparison(&comparison, &mut field_stats);

        assert_eq!(result.group_names, vec!["half_bits", "full_bits"]);
        assert_eq!(result.differences.len(), 2);

        // Note: The 'zstd' and 'estimated size' numbers may randomly break with parameter changes.
        //       This is OK, we hardcoded them here for sanity test only.
        // First comparison group differences
        // Estimated size is equal.
        assert!(result.differences[0].estimated_size <= 0);

        // Second comparison should match baseline
        assert_eq!(result.differences[1].estimated_size, 0);
        assert_eq!(result.differences[1].original_size, 0);
        assert_eq!(result.differences[1].zstd_size, 0);
        assert_eq!(result.differences[1].entropy, 0.0);
    }
}
