//! Analyzes compression efficiency of different field arrangements in bit-packed structures.
//!
//! Compares compression metrics between different field groupings, primarily focusing on
//! interleaved vs. separated layouts (e.g., RGBRGBRGB vs. RRRGGGBBB).
//!
//! # Core Types
//!
//! - [`SplitComparisonResult`]: Results from comparing field arrangements
//! - [`FieldComparisonMetrics`]: Field-level compression statistics
//!
//! # Example
//!
//! ```yaml
//! split_groups:
//!   - name: colors
//!     group_1: [colors]                    # RGBRGBRGB
//!     group_2: [color_r, color_g, color_b] # RRRGGGBBB
//! ```
//!
//! Use [`make_split_comparison_result`] to generate comparison metrics for two field arrangements.
//!
//! Each comparison tracks:
//! - Entropy and LZ matches (data redundancy measures)
//! - Sizes (original, estimated compression, actual zstd compression)
//!
//! # Usage Notes
//!
//! - Ensure compared groups have equal total bits
//! - Field ordering can significantly impact compression
//! - zstd compression time dominates performance
//!
//! [`SplitComparisonResult`]: crate::comparison::split_comparison::SplitComparisonResult
//! [`FieldComparisonMetrics`]: crate::comparison::split_comparison::FieldComparisonMetrics
//! [`make_split_comparison_result`]: crate::comparison::split_comparison::make_split_comparison_result

use super::{GroupComparisonMetrics, GroupDifference};
use crate::{
    analyzer::{CompressionOptions, SizeEstimationParameters},
    results::FieldMetrics,
    utils::analyze_utils::{calculate_file_entropy, get_zstd_compressed_size},
};
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;

/// Calculates the compression statistics of two splits (of the same data) and
/// returns them as a [`SplitComparisonResult`] object. This can also be used for
/// generic two-way compares.
///
/// This function aggregates the comparison results for individual fields and
/// calculates overall statistics for the split comparison.
///
/// # Arguments
///
/// * `name` - The name of the group comparison.
/// * `description` - A description of the group comparison.
/// * `baseline_bytes` - The bytes of the baseline (original/reference) group.
/// * `split_bytes` - The bytes of the second (comparison) group.
/// * `baseline_comparison_metrics` - The metrics for the individual fields in the baseline (original/reference) group.
/// * `split_comparison_metrics` - The metrics for the individual fields in the second (comparison) group.
/// * `compression_options` - Compression options, zstd compression level, etc.
///
/// # Returns
///
/// A [`SplitComparisonResult`] struct containing the aggregated comparison results
/// and overall statistics.
pub fn make_split_comparison_result(
    name: String,
    description: String,
    baseline_bytes: &[u8],
    split_bytes: &[u8],
    baseline_comparison_metrics: Vec<FieldComparisonMetrics>,
    split_comparison_metrics: Vec<FieldComparisonMetrics>,
    compression_options: CompressionOptions,
) -> SplitComparisonResult {
    // Calculate entropy and LZ matches for both group sets.
    let entropy1 = calculate_file_entropy(baseline_bytes);
    let entropy2 = calculate_file_entropy(split_bytes);
    let lz_matches1 = estimate_num_lz_matches_fast(baseline_bytes);
    let lz_matches2 = estimate_num_lz_matches_fast(split_bytes);
    let estimated_size_1 = (compression_options.size_estimator_fn)(SizeEstimationParameters {
        data: baseline_bytes,
        num_lz_matches: lz_matches1,
        entropy: entropy1,
    });
    let estimated_size_2 = (compression_options.size_estimator_fn)(SizeEstimationParameters {
        data: split_bytes,
        num_lz_matches: lz_matches2,
        entropy: entropy2,
    });
    let actual_size_1 =
        get_zstd_compressed_size(baseline_bytes, compression_options.zstd_compression_level);
    let actual_size_2 =
        get_zstd_compressed_size(split_bytes, compression_options.zstd_compression_level);

    let group1_metrics = GroupComparisonMetrics {
        lz_matches: lz_matches1 as u64,
        entropy: entropy1,
        estimated_size: estimated_size_1 as u64,
        zstd_size: actual_size_1 as u64,
        original_size: baseline_bytes.len() as u64,
    };

    let group2_metrics = GroupComparisonMetrics {
        lz_matches: lz_matches2 as u64,
        entropy: entropy2,
        estimated_size: estimated_size_2 as u64,
        zstd_size: actual_size_2 as u64,
        original_size: split_bytes.len() as u64,
    };

    SplitComparisonResult {
        name,
        description,
        difference: GroupDifference::from_metrics(&group1_metrics, &group2_metrics),
        group1_metrics,
        group2_metrics,
        baseline_comparison_metrics,
        split_comparison_metrics,
    }
}

/// The result of comparing 2 arbitrary groups of fields based on the schema.
#[derive(Clone, Default)]
pub struct SplitComparisonResult {
    /// The name of the group comparison. (Copied from schema)
    pub name: String,
    /// A description of the group comparison. (Copied from schema)
    pub description: String,
    /// The metrics for the first group.
    pub group1_metrics: GroupComparisonMetrics,
    /// The metrics for the second group.
    pub group2_metrics: GroupComparisonMetrics,
    /// Comparison between group 2 and group 1.
    pub difference: GroupDifference,
    /// The statistics for the individual fields of the baseline group.
    pub baseline_comparison_metrics: Vec<FieldComparisonMetrics>,
    /// The statistics for the individual fields of the split group.
    pub split_comparison_metrics: Vec<FieldComparisonMetrics>,
}

/// Helper functions around [`SplitComparisonResult`]
impl SplitComparisonResult {
    /// Ratio between the max and min entropy of the baseline fields.
    pub fn baseline_max_entropy_diff_ratio(&self) -> f64 {
        calculate_max_entropy_diff_ratio(&self.baseline_comparison_metrics)
    }

    /// Maximum difference between the entropy of the baseline fields.
    pub fn baseline_max_entropy_diff(&self) -> f64 {
        calculate_max_entropy_diff(&self.baseline_comparison_metrics)
    }

    /// Maximum difference between the entropy of the split fields.
    pub fn split_max_entropy_diff(&self) -> f64 {
        calculate_max_entropy_diff(&self.split_comparison_metrics)
    }

    /// Ratio between the max and min entropy of the split fields.
    pub fn split_max_entropy_diff_ratio(&self) -> f64 {
        calculate_max_entropy_diff_ratio(&self.split_comparison_metrics)
    }
}

/// Represents the statistics for the individual fields which were used
/// to create the individual combined group or split.
///
/// i.e. This is the info for the fields that were used to create the final
/// combined group or split.
///
/// This is useful when dumping
/// extra info about the fields.
#[derive(PartialEq, Debug, Clone, Copy, Default)]
pub struct FieldComparisonMetrics {
    /// LZ compression matches in the field
    pub lz_matches: usize,
    /// Shannon entropy in bits
    pub entropy: f64,
}

/// Converts a [`FieldMetrics`] object into a [`FieldComparisonMetrics`] object.
impl From<FieldMetrics> for FieldComparisonMetrics {
    fn from(value: FieldMetrics) -> Self {
        Self {
            entropy: value.entropy,
            lz_matches: value.lz_matches,
        }
    }
}

fn calculate_max_entropy_diff(results: &[FieldComparisonMetrics]) -> f64 {
    let entropy_values: Vec<f64> = results.iter().map(|m| m.entropy).collect();
    if entropy_values.len() < 2 {
        0.0
    } else {
        let max = entropy_values
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let min = entropy_values
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        max - min
    }
}

fn calculate_max_entropy_diff_ratio(results: &[FieldComparisonMetrics]) -> f64 {
    let entropy_values: Vec<f64> = results.iter().map(|m| m.entropy).collect();
    if entropy_values.len() < 2 {
        0.0
    } else {
        let max = entropy_values
            .iter()
            .max_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        let min = entropy_values
            .iter()
            .min_by(|a, b| a.partial_cmp(b).unwrap())
            .unwrap();
        if *min == 0.0 {
            return 0.0;
        }
        max / min
    }
}
