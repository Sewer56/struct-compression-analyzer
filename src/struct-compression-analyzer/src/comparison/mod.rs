//! Core comparison structures for storing the results of group comparisons.
//!
//! The module is split into two specialized submodules:
//!
//! - [`split_comparison`]: Easy comparison of 'splitting' structs.
//!     - e.g. interleaved (RGBRGBRGB) vs. separated fields (RRRGGGBB)
//! - [`compare_groups`]: Comparison of more custom field transformations and analysis
//! - [`stats`]: Additional statistics for comparing groups
//!
//! # Types
//!
//! - [`GroupComparisonMetrics`]: Collects compression metrics (LZ matches, entropy, sizes)
//! - [`GroupDifference`]: Tracks metric differences between two field groups
//!
//! # Example
//!
//! ```no_run
//! use struct_compression_analyzer::comparison::*;
//! use struct_compression_analyzer::analyzer::CompressionOptions;
//!
//! fn calculate_example(baseline_data: &[u8], comparison_data: &[u8]) {
//!     let options = CompressionOptions::default();
//!     let baseline = GroupComparisonMetrics::from_bytes(&baseline_data, "name_a", options);
//!     let comparison = GroupComparisonMetrics::from_bytes(&comparison_data, "name_b", options);
//!
//!     // Compare the difference
//!     let difference = GroupDifference::from_metrics(&baseline, &comparison);
//! }
//! ```
//!
//! [`split_comparison`]: self::split_comparison
//! [`compare_groups`]: self::compare_groups
//! [`stats`]: self::stats
//! [`GroupComparisonMetrics`]: GroupComparisonMetrics
//! [`GroupDifference`]: GroupDifference

use crate::{
    analyzer::{CompressionOptions, SizeEstimationParameters},
    utils::analyze_utils::{calculate_file_entropy, get_zstd_compressed_size},
};
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;

pub mod compare_groups;
pub mod split_comparison;
pub mod stats;

/// The statistics for a given group of fields.
/// This can be a group created by the [`split_comparison`] module, the
/// [`compare_groups`] module or any other piece of code that compares multiple sets of bytes.
#[derive(Clone, Default, Debug, PartialEq, Copy)]
pub struct GroupComparisonMetrics {
    /// Number of total LZ matches
    pub lz_matches: u64,
    /// Amount of entropy in the input data set
    pub entropy: f64,
    /// Size estimated by the size estimator function.
    pub estimated_size: u64,
    /// Size compressed by zstd.
    pub zstd_size: u64,
    /// Size of the original data.
    pub original_size: u64,
}

/// Represents the difference between 2 groups of fields.
/// For the raw values of a single group, see [`GroupComparisonMetrics`].
///
/// This can be used for representing the difference between either splits, or any two arbitrary
/// groups of analyzed bytes. Usually this is the difference between a result and a baseline.
#[derive(PartialEq, Debug, Clone, Copy, Default)]
pub struct GroupDifference {
    /// The difference in LZ matches.
    pub lz_matches: i64,
    /// The difference in entropy
    pub entropy: f64,
    /// Difference in estimated size using the user
    /// provided estimate function.
    pub estimated_size: i64,
    /// Difference in zstd compressed size
    pub zstd_size: i64,
    /// Difference in original size
    pub original_size: i64,
}

impl GroupComparisonMetrics {
    /// Calculates group comparison metrics for a given byte slice.
    ///
    /// This function computes various metrics such as entropy, LZ matches, estimated size,
    /// and Zstandard compressed size, which are used for comparing different compression strategies.
    ///
    /// # Arguments
    /// * `bytes` - A slice of bytes representing the data to analyze.
    /// * `group_name` - The name of the group being analyzed.
    /// * `compression_options` - Compression options, zstd compression level, etc.
    ///
    /// # Returns
    /// A [`GroupComparisonMetrics`] struct containing the computed metrics.
    pub fn from_bytes(
        bytes: &[u8],
        group_name: &str,
        compression_options: CompressionOptions,
    ) -> Self {
        let entropy = calculate_file_entropy(bytes);
        let lz_matches = estimate_num_lz_matches_fast(bytes) as u64;
        let estimated_size = (compression_options.size_estimator_fn)(SizeEstimationParameters {
            name: group_name,
            data: Some(bytes),
            data_len: bytes.len(),
            num_lz_matches: lz_matches as usize,
            entropy,
            lz_match_multiplier: compression_options.lz_match_multiplier,
            entropy_multiplier: compression_options.entropy_multiplier,
        }) as u64;
        let zstd_size = get_zstd_compressed_size(bytes, compression_options.zstd_compression_level);

        GroupComparisonMetrics {
            lz_matches,
            entropy,
            estimated_size,
            zstd_size,
            original_size: bytes.len() as u64,
        }
    }
}

impl GroupDifference {
    /// Creates a new GroupDifference by comparing two sets of metrics
    ///
    /// # Arguments
    /// * `baseline` - The baseline metrics to compare against
    /// * `comparison` - The metrics to compare with the baseline
    ///
    /// # Returns
    /// A new [`GroupDifference`] containing the calculated differences
    pub fn from_metrics(
        baseline: &GroupComparisonMetrics,
        comparison: &GroupComparisonMetrics,
    ) -> Self {
        GroupDifference {
            lz_matches: comparison.lz_matches as i64 - baseline.lz_matches as i64,
            entropy: comparison.entropy - baseline.entropy,
            estimated_size: comparison.estimated_size as i64 - baseline.estimated_size as i64,
            zstd_size: comparison.zstd_size as i64 - baseline.zstd_size as i64,
            original_size: comparison.original_size as i64 - baseline.original_size as i64,
        }
    }
}
