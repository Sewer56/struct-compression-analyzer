//! Processes custom field transformations and group comparisons defined in schemas.
//!
//! This module implements analysis of user-defined field groupings and transformations,
//! allowing comparison of different field arrangements and bit layouts. Unlike
//! split comparisons which handle basic field reordering, this module supports
//! complex transformations like:
//!
//! - Bit padding and alignment
//! - Field slicing and partial reads
//! - Custom field grouping patterns
//!
//! # Core Types
//!
//! - [`GroupComparisonResult`]: Results from analyzing custom field groupings
//! - [`GroupComparisonError`]: Errors that can occur during group analysis
//!
//! # Example
//!
//! ```yaml
//! compare_groups:
//!   - name: convert_7_to_8_bit
//!     description: "Convert 7-bit colors to 8-bit by padding"
//!     baseline:
//!       - type: array      # Original 7-bit values
//!         field: color7
//!         bits: 7
//!     comparisons:
//!       padded_8bit:      # Padded to 8 bits
//!         - type: struct
//!           fields:
//!             - type: field
//!               field: color7
//!               bits: 7
//!             - type: padding
//!               bits: 1
//!               value: 0
//! ```
//!
//! Each comparison analyzes:
//! - Compression metrics (entropy, LZ matches)
//! - Size comparisons (original, estimated, actual zstd)
//! - Field-level statistics
//!
//! # Usage Notes
//!
//! - Baseline group serves as reference for comparisons
//! - Multiple comparison groups can be defined
//! - Field transformations are applied during analysis
//! - Bit padding and alignment can impact compression
//!
//! # Submodules
//!
//! - [`generate_bytes`]: Core byte stream generation from schemas
//! - [`test_helpers`]: Testing utilities (only in test builds)
//!
//! [`GroupComparisonResult`]: crate::comparison::compare_groups::GroupComparisonResult
//! [`GroupComparisonError`]: crate::comparison::compare_groups::GroupComparisonError
//! [`generate_bytes`]: crate::comparison::compare_groups::generate_bytes

pub mod generate_bytes;
#[cfg(test)]
pub(crate) mod test_helpers;

use super::{GroupComparisonMetrics, GroupDifference};
use crate::comparison::compare_groups::generate_bytes::generate_group_bytes;
use crate::schema::Schema;
use crate::{analyzer::AnalyzerFieldState, schema::CustomComparison};
use ahash::AHashMap;
use generate_bytes::GenerateBytesError;
use thiserror::Error;

/// Describes an error that occurred while computing a group comparison.
#[derive(Error, Debug)]
pub enum GroupComparisonError {
    #[error("Failed to generate group bytes: {0}")]
    BytesGeneration(#[from] GenerateBytesError),

    #[error("Mismatched number of byte slices and group names. Slices {slices} != {names} Names")]
    InvalidItemCount { slices: usize, names: usize },

    #[error("Invalid comparison configuration: {0}")]
    InvalidConfiguration(String),
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
    /// Creates comparison results from precomputed group bytes
    ///
    /// Arguments:
    /// * `name` - The name of the comparison (copied from schema)
    /// * `description` - The description of the comparison (copied from schema)
    /// * `baseline_bytes` - The bytes of the baseline (original/reference) group.
    /// * `comparison_byte_slices` - The bytes of the comparison groups.
    /// * `group_names` - The names of the comparison groups in order they were specified in the schema.
    pub fn from_custom_comparison<T: AsRef<[u8]>>(
        name: String,
        description: String,
        baseline_bytes: &[u8],
        comparison_byte_slices: &[T],
        group_names: &[String],
    ) -> Result<Self, GroupComparisonError> {
        if comparison_byte_slices.len() != group_names.len() {
            return Err(GroupComparisonError::InvalidItemCount {
                slices: comparison_byte_slices.len(),
                names: group_names.len(),
            });
        }

        // Calculate baseline metrics
        let baseline_metrics = GroupComparisonMetrics::from_bytes(baseline_bytes);

        // Process comparison groups
        let mut group_metrics = Vec::with_capacity(comparison_byte_slices.len());
        let mut differences = Vec::with_capacity(comparison_byte_slices.len());
        let mut names = Vec::with_capacity(comparison_byte_slices.len());
        for group_name in group_names {
            names.push(group_name.clone());
        }

        for comparison in comparison_byte_slices {
            let metrics = GroupComparisonMetrics::from_bytes(comparison.as_ref());
            differences.push(GroupDifference::from_metrics(&baseline_metrics, &metrics));
            group_metrics.push(metrics);
        }

        Ok(Self {
            name,
            description,
            baseline_metrics,
            group_names: names,
            group_metrics,
            differences,
        })
    }
}

/// Analyzes a single custom comparison defined in the [`Schema`].
/// This is an internal API.
///
/// # Arguments
///
/// * `comparison` - The comparison to analyze
/// * `field_stats` - Mutable reference to field statistics map
///
/// # Returns
///
/// A single [`GroupComparisonResult`] containing metrics for the passed in comparison
pub(crate) fn process_single_comparison(
    comparison: &CustomComparison,
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
) -> Result<GroupComparisonResult, GroupComparisonError> {
    // Generate baseline bytes with error context
    let baseline_bytes = generate_group_bytes(&comparison.baseline, field_stats).map_err(|e| {
        GroupComparisonError::InvalidConfiguration(format!(
            "Comparison '{}' baseline error: {}. This is indicative of a configuration error.",
            comparison.name, e
        ))
    })?;

    // Generate comparison group bytes in schema order
    let mut comparison_bytes = Vec::new();
    let mut group_names = Vec::new();

    for (group_name, components) in &comparison.comparisons {
        let bytes = generate_group_bytes(components, field_stats).map_err(|e| {
            GroupComparisonError::InvalidConfiguration(format!(
                "Comparison '{}' group '{}' error: {}. This is indicative of a configuration error.",
                comparison.name, group_name, e
            ))
        })?;

        comparison_bytes.push(bytes);
        group_names.push(group_name.clone());
    }

    GroupComparisonResult::from_custom_comparison(
        comparison.name.clone(),
        comparison.description.clone(),
        &baseline_bytes,
        &comparison_bytes,
        &group_names,
    )
}

/// Analyzes all custom comparisons defined in the [`Schema`].
/// This is an internal API.
///
/// # Arguments
///
/// * `schema` - Reference to loaded schema definition
/// * `field_stats` - Mutable reference to field statistics map
///
/// # Returns
///
/// Vector of [`GroupComparisonResult`] containing metrics for all configured comparisons
pub(crate) fn analyze_custom_comparisons(
    schema: &Schema,
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
) -> Result<Vec<GroupComparisonResult>, GroupComparisonError> {
    schema
        .analysis
        .compare_groups
        .iter()
        .map(|comparison| process_single_comparison(comparison, field_stats))
        .collect()
}

#[cfg(test)]
mod from_custom_comparison_tests {
    use super::*;
    use crate::comparison::compare_groups::test_helpers::create_mock_field_states;
    use crate::comparison::compare_groups::test_helpers::TEST_FIELD_NAME;
    use crate::schema::BitOrder;
    use crate::schema::GroupComponent;
    use crate::schema::GroupComponentArray;
    use indexmap::IndexMap;

    #[test]
    fn from_custom_comparison_basic() {
        let input_data = [0b1010_1010, 0b0101_0101];
        let mut field_stats = create_mock_field_states(
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

        let result = process_single_comparison(&comparison, &mut field_stats).unwrap();

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
        let mut field_stats = create_mock_field_states(
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

        let result = process_single_comparison(&comparison, &mut field_stats).unwrap();

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

    #[test]
    fn invalid_configuration_error() {
        let invalid_comparison = CustomComparison {
            name: "invalid_comp".to_string(),
            description: "Invalid comparison".to_string(),
            baseline: vec![GroupComponent::Array(GroupComponentArray {
                field: "nonexistent_field".to_string(), // Field doesn't exist
                offset: 0,
                bits: 8,
            })],
            comparisons: IndexMap::new(),
        };

        let mut field_stats = AHashMap::new();
        let result = process_single_comparison(&invalid_comparison, &mut field_stats);

        assert!(matches!(
            result,
            Err(GroupComparisonError::InvalidConfiguration(msg))
                if msg.contains("Comparison 'invalid_comp' baseline error")
                && msg.contains("Field 'nonexistent_field' not found")
        ));
    }

    #[test]
    fn errors_on_mismatched_group_count() {
        let result = GroupComparisonResult::from_custom_comparison(
            "test".into(),
            "test".into(),
            &[],
            &[&[1u8], &[2u8]],
            &["group1".into()],
        );

        assert!(matches!(
            result,
            Err(GroupComparisonError::InvalidItemCount {
                slices: 2,
                names: 1
            })
        ));
    }
}
