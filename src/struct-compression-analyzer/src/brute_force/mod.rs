//! Brute force optimization for LZ match and entropy multiplier parameters.
//!
//! This module provides functionality to find optimal values for the
//! [`lz_match_multiplier`] and [`entropy_multiplier`] parameters used in the
//! [`size_estimate`] function.
//!
//! [`size_estimate`]: crate::utils::analyze_utils::size_estimate
//! [`lz_match_multiplier`]: crate::analyzer::SizeEstimationParameters::lz_match_multiplier
//! [`entropy_multiplier`]: crate::analyzer::SizeEstimationParameters::entropy_multiplier

pub mod brute_force_custom;
pub mod brute_force_split;
use brute_force_custom::{
    find_optimal_custom_result_coefficients, CustomComparisonOptimizationResult,
};
use brute_force_split::{
    find_optimal_split_result_coefficients, SplitComparisonOptimizationResult,
};

use crate::analyzer::SizeEstimationParameters;
use crate::comparison::{GroupComparisonMetrics, GroupDifference};
use crate::results::merged_analysis_results::MergedAnalysisResults;
use crate::utils::analyze_utils::size_estimate;

/// Configuration for the brute force optimization process.
#[derive(Debug, Clone)]
pub struct BruteForceConfig {
    /// Minimum value for LZ match multiplier
    pub min_lz_multiplier: f64,
    /// Maximum value for LZ match multiplier
    pub max_lz_multiplier: f64,
    /// Step size for LZ match multiplier
    pub lz_step_size: f64,
    /// Minimum value for entropy multiplier
    pub min_entropy_multiplier: f64,
    /// Maximum value for entropy multiplier
    pub max_entropy_multiplier: f64,
    /// Step size for entropy multiplier
    pub entropy_step_size: f64,
}

impl Default for BruteForceConfig {
    fn default() -> Self {
        Self {
            min_lz_multiplier: 0.001,
            max_lz_multiplier: 1.0,
            lz_step_size: 0.001,
            min_entropy_multiplier: 1.0,
            max_entropy_multiplier: 1.2,
            entropy_step_size: 0.001,
        }
    }
}

/// Result of a brute force optimization.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptimizationResult {
    /// Optimized LZ match multiplier
    pub lz_match_multiplier: f64,
    /// Optimized entropy multiplier
    pub entropy_multiplier: f64,
}

/// Calculates the error for a given set of LZ match and entropy multipliers.
///
/// # Arguments
///
/// * `num_lz_matches` - The number of LZ matches in the input
/// * `entropy` - The estimated entropy of the input
/// * `zstd_size` - The ZSTD compressed size of the input
/// * `original_size` - The original size of the input
/// * `lz_match_multiplier` - The current LZ match multiplier
/// * `entropy_multiplier` - The current entropy multiplier
///
/// # Returns
///
/// The error for the tested parameters (difference between estimated and actual size).
#[inline(always)]
pub(crate) fn calculate_error(
    // Compression Estimator Params
    num_lz_matches: usize,
    entropy: f64,
    // Actual Compression Stats
    zstd_size: u64,
    original_size: usize,
    // Coefficients to Test
    lz_match_multiplier: f64,
    entropy_multiplier: f64,
) -> f64 {
    // Calculate estimated size with current coefficients
    let estimated_size = size_estimate(SizeEstimationParameters {
        name: "",
        data_len: original_size,
        data: None,
        num_lz_matches,
        entropy,
        lz_match_multiplier,
        entropy_multiplier,
    });

    // Calculate error (difference between estimated and actual size)
    ((estimated_size as f64) - (zstd_size as f64)).abs()
}

/// Optimizes and applies coefficients to a [`MergedAnalysisResults`] object.
///
/// This function:
/// 1. Finds optimal coefficients for all split comparisons
/// 2. Finds optimal coefficients for all custom comparisons
/// 3. Updates estimated sizes in both the merged results and original analysis results
///
/// # Arguments
///
/// * `merged_results` - The merged analysis results to optimize and update
/// * `config` - Optional configuration for the brute force optimization process
///
/// # Returns
///
/// A tuple of optimization result vectors for split and custom comparisons
#[allow(clippy::type_complexity)]
pub fn optimize_and_apply_coefficients(
    merged_results: &mut MergedAnalysisResults,
    config: Option<&BruteForceConfig>,
) -> (
    Vec<(String, SplitComparisonOptimizationResult)>,
    Vec<(String, CustomComparisonOptimizationResult)>,
) {
    // Find optimal coefficients for split comparisons
    let split_optimization_results = find_optimal_split_result_coefficients(merged_results, config);

    // Find optimal coefficients for custom comparisons
    let custom_optimization_results =
        find_optimal_custom_result_coefficients(merged_results, config);

    // Update the merged results with the optimized coefficients
    apply_optimized_coefficients(
        merged_results,
        &split_optimization_results,
        &custom_optimization_results,
    );

    (split_optimization_results, custom_optimization_results)
}

/// Applies the optimized coefficients to the merged results and original files.
///
/// # Arguments
///
/// * `merged_results` - The merged analysis results to update
/// * `split_optimization_results` - The optimization results for split comparisons
/// * `custom_optimization_results` - The optimization results for custom comparisons
fn apply_optimized_coefficients(
    merged_results: &mut MergedAnalysisResults,
    split_optimization_results: &[(String, SplitComparisonOptimizationResult)],
    custom_optimization_results: &[(String, CustomComparisonOptimizationResult)],
) {
    // Update split comparisons in merged results
    for (split_idx, comparison) in merged_results.split_comparisons.iter_mut().enumerate() {
        let optimization_result = &split_optimization_results[split_idx].1;

        // Update group 1 metrics
        update_group_metrics(
            &mut comparison.group1_metrics,
            optimization_result.group_1.lz_match_multiplier,
            optimization_result.group_1.entropy_multiplier,
        );

        // Update group 2 metrics
        update_group_metrics(
            &mut comparison.group2_metrics,
            optimization_result.group_2.lz_match_multiplier,
            optimization_result.group_2.entropy_multiplier,
        );

        // Update group difference
        update_group_difference(
            &comparison.group1_metrics,
            &comparison.group2_metrics,
            &mut comparison.difference,
        );
    }

    // Update custom comparisons in merged results
    for (custom_idx, comparison) in merged_results.custom_comparisons.iter_mut().enumerate() {
        let optimization_result = &custom_optimization_results[custom_idx].1;

        // Update baseline metrics
        update_group_metrics(
            &mut comparison.baseline_metrics,
            optimization_result.baseline.lz_match_multiplier,
            optimization_result.baseline.entropy_multiplier,
        );

        // Update comparison group metrics
        for (group_idx, group_metrics) in comparison.group_metrics.iter_mut().enumerate() {
            update_group_metrics(
                group_metrics,
                optimization_result.comparisons[group_idx].lz_match_multiplier,
                optimization_result.comparisons[group_idx].entropy_multiplier,
            );
        }

        // Update group differences
        for (group_idx, difference) in comparison.differences.iter_mut().enumerate() {
            update_group_difference(
                &comparison.baseline_metrics,
                &comparison.group_metrics[group_idx],
                difference,
            );
        }
    }

    // Update each original analysis result
    for result in &mut merged_results.original_results {
        // Update split comparisons in original results
        for (split_idx, comparison) in result.split_comparisons.iter_mut().enumerate() {
            let optimization_result = &split_optimization_results[split_idx].1;

            // Update group 1 metrics
            update_group_metrics(
                &mut comparison.group1_metrics,
                optimization_result.group_1.lz_match_multiplier,
                optimization_result.group_1.entropy_multiplier,
            );

            // Update group 2 metrics
            update_group_metrics(
                &mut comparison.group2_metrics,
                optimization_result.group_2.lz_match_multiplier,
                optimization_result.group_2.entropy_multiplier,
            );

            // Update group difference
            update_group_difference(
                &comparison.group1_metrics,
                &comparison.group2_metrics,
                &mut comparison.difference,
            );
        }

        // Update custom comparisons in original results
        for (custom_idx, comparison) in result.custom_comparisons.iter_mut().enumerate() {
            let optimization_result = &custom_optimization_results[custom_idx].1;

            // Update baseline metrics
            update_group_metrics(
                &mut comparison.baseline_metrics,
                optimization_result.baseline.lz_match_multiplier,
                optimization_result.baseline.entropy_multiplier,
            );

            // Update comparison group metrics
            for (group_idx, group_metrics) in comparison.group_metrics.iter_mut().enumerate() {
                update_group_metrics(
                    group_metrics,
                    optimization_result.comparisons[group_idx].lz_match_multiplier,
                    optimization_result.comparisons[group_idx].entropy_multiplier,
                );
            }

            // Update group differences
            for (group_idx, difference) in comparison.differences.iter_mut().enumerate() {
                update_group_difference(
                    &comparison.baseline_metrics,
                    &comparison.group_metrics[group_idx],
                    difference,
                );
            }
        }
    }
}

/// Updates a [`GroupComparisonMetrics`] struct with new coefficient values.
///
/// # Arguments
///
/// * `metrics` - The metrics to update
/// * `lz_match_multiplier` - The new LZ match multiplier
/// * `entropy_multiplier` - The new entropy multiplier
fn update_group_metrics(
    metrics: &mut GroupComparisonMetrics,
    lz_match_multiplier: f64,
    entropy_multiplier: f64,
) {
    // Recalculate estimated size with the optimized parameters
    let estimated_size = size_estimate(SizeEstimationParameters {
        name: "",
        data_len: metrics.original_size as usize,
        data: None,
        num_lz_matches: metrics.lz_matches as usize,
        entropy: metrics.entropy,
        lz_match_multiplier,
        entropy_multiplier,
    });

    // Update the estimated size
    metrics.estimated_size = estimated_size as u64;
}

/// Updates a [`GroupDifference`] struct with recalculated values.
fn update_group_difference(
    group1_metrics: &GroupComparisonMetrics,
    group2_metrics: &GroupComparisonMetrics,
    difference: &mut GroupDifference,
) {
    difference.estimated_size =
        group2_metrics.estimated_size as i64 - group1_metrics.estimated_size as i64;
}

/// Prints formatted optimization results for both split and custom comparisons.
///
/// # Arguments
///
/// * `split_results` - Optimization results for split comparisons
/// * `custom_results` - Optimization results for custom comparisons
pub fn print_all_optimization_results(
    split_results: &[(String, SplitComparisonOptimizationResult)],
    custom_results: &[(String, CustomComparisonOptimizationResult)],
) {
    brute_force_split::print_optimization_results(split_results);
    brute_force_custom::print_optimization_results(custom_results);
}

/// These tests are crap, they weren't written by a human, after all.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comparison::{
            compare_groups::GroupComparisonResult, split_comparison::SplitComparisonResult,
        },
        results::{
            analysis_results::AnalysisResults, merged_analysis_results::MergedSplitComparisonResult,
        },
        schema::Metadata,
    };
    use ahash::AHashMap;

    // Constants for test data
    const TEST_NAME_SPLIT: &str = "Test Split";
    const TEST_DESC_SPLIT: &str = "Test Split Description";
    const TEST_NAME_CUSTOM: &str = "Test Custom";
    const TEST_DESC_CUSTOM: &str = "Test Custom Description";
    const TEST_GROUP_NAME: &str = "Test Group";
    const TEST_SCHEMA_NAME: &str = "Test Schema";
    const TEST_SCHEMA_DESC: &str = "Test Schema Description";

    // Constants for metrics values
    const GROUP1_LZ_MATCHES: u64 = 100;
    const GROUP1_ENTROPY: f64 = 5.0;
    const GROUP1_ESTIMATED_SIZE: u64 = 1000;
    const GROUP1_ZSTD_SIZE: u64 = 800;
    const GROUP1_ORIGINAL_SIZE: u64 = 2000;

    const GROUP2_LZ_MATCHES: u64 = 150;
    const GROUP2_ENTROPY: f64 = 4.0;
    const GROUP2_ESTIMATED_SIZE: u64 = 900;
    const GROUP2_ZSTD_SIZE: u64 = 700;
    const GROUP2_ORIGINAL_SIZE: u64 = 1800;

    const DIFF_LZ_MATCHES: i64 = 50;
    const DIFF_ENTROPY: f64 = -1.0;
    const DIFF_ESTIMATED_SIZE: i64 = -100;
    const DIFF_ZSTD_SIZE: i64 = -100;
    const DIFF_ORIGINAL_SIZE: i64 = -200;

    // Constants for brute force config
    const TEST_MIN_LZ: f64 = 0.01;
    const TEST_MAX_LZ: f64 = 0.05;
    const TEST_LZ_STEP: f64 = 0.02;
    const TEST_MIN_ENTROPY: f64 = 1.0;
    const TEST_MAX_ENTROPY: f64 = 1.1;
    const TEST_ENTROPY_STEP: f64 = 0.05;

    /// Creates a simple mock MergedAnalysisResults for testing
    fn create_mock_merged_results() -> MergedAnalysisResults {
        // Create a simple split comparison result
        let group1_metrics = GroupComparisonMetrics {
            lz_matches: GROUP1_LZ_MATCHES,
            entropy: GROUP1_ENTROPY,
            estimated_size: GROUP1_ESTIMATED_SIZE,
            zstd_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
        };

        let group2_metrics = GroupComparisonMetrics {
            lz_matches: GROUP2_LZ_MATCHES,
            entropy: GROUP2_ENTROPY,
            estimated_size: GROUP2_ESTIMATED_SIZE,
            zstd_size: GROUP2_ZSTD_SIZE,
            original_size: GROUP2_ORIGINAL_SIZE,
        };

        let difference = GroupDifference {
            lz_matches: DIFF_LZ_MATCHES,
            entropy: DIFF_ENTROPY,
            estimated_size: DIFF_ESTIMATED_SIZE,
            zstd_size: DIFF_ZSTD_SIZE,
            original_size: DIFF_ORIGINAL_SIZE,
        };

        let split_comparison = MergedSplitComparisonResult {
            name: TEST_NAME_SPLIT.to_string(),
            description: TEST_DESC_SPLIT.to_string(),
            group1_metrics,
            group2_metrics,
            difference,
            baseline_comparison_metrics: Vec::new(),
            split_comparison_metrics: Vec::new(),
        };

        // Create a simple custom comparison result
        let baseline_metrics = GroupComparisonMetrics {
            lz_matches: GROUP1_LZ_MATCHES,
            entropy: GROUP1_ENTROPY,
            estimated_size: GROUP1_ESTIMATED_SIZE,
            zstd_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
        };

        let group_metrics = vec![GroupComparisonMetrics {
            lz_matches: GROUP2_LZ_MATCHES,
            entropy: GROUP2_ENTROPY,
            estimated_size: GROUP2_ESTIMATED_SIZE,
            zstd_size: GROUP2_ZSTD_SIZE,
            original_size: GROUP2_ORIGINAL_SIZE,
        }];

        let group_difference = GroupDifference {
            lz_matches: DIFF_LZ_MATCHES,
            entropy: DIFF_ENTROPY,
            estimated_size: DIFF_ESTIMATED_SIZE,
            zstd_size: DIFF_ZSTD_SIZE,
            original_size: DIFF_ORIGINAL_SIZE,
        };

        let custom_comparison = GroupComparisonResult {
            name: TEST_NAME_CUSTOM.to_string(),
            description: TEST_DESC_CUSTOM.to_string(),
            baseline_metrics,
            group_metrics: group_metrics.clone(),
            group_names: vec![TEST_GROUP_NAME.to_string()],
            differences: vec![group_difference],
        };

        // Create mock original analysis results
        let schema_metadata = Metadata {
            name: TEST_SCHEMA_NAME.to_string(),
            description: TEST_SCHEMA_DESC.to_string(),
        };
        let original_result = AnalysisResults {
            schema_metadata: schema_metadata.clone(),
            file_entropy: GROUP1_ENTROPY,
            file_lz_matches: GROUP1_LZ_MATCHES,
            zstd_file_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
            per_field: AHashMap::new(),
            split_comparisons: vec![SplitComparisonResult {
                name: TEST_NAME_SPLIT.to_string(),
                description: TEST_DESC_SPLIT.to_string(),
                group1_metrics,
                group2_metrics,
                difference,
                baseline_comparison_metrics: Vec::new(),
                split_comparison_metrics: Vec::new(),
            }],
            custom_comparisons: vec![GroupComparisonResult {
                name: TEST_NAME_CUSTOM.to_string(),
                description: TEST_DESC_CUSTOM.to_string(),
                baseline_metrics,
                group_metrics: group_metrics.clone(),
                group_names: vec![TEST_GROUP_NAME.to_string()],
                differences: vec![group_difference],
            }],
        };

        // Create the merged results
        MergedAnalysisResults {
            schema_metadata,
            file_entropy: GROUP1_ENTROPY,
            file_lz_matches: GROUP1_LZ_MATCHES,
            zstd_file_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
            merged_file_count: 1,
            per_field: AHashMap::new(),
            split_comparisons: vec![split_comparison],
            custom_comparisons: vec![custom_comparison],
            original_results: vec![original_result],
        }
    }

    #[test]
    fn can_optimize_and_apply_coefficients() {
        // Create a simple BruteForceConfig with a narrow range for quick testing
        let config = BruteForceConfig {
            min_lz_multiplier: TEST_MIN_LZ,
            max_lz_multiplier: TEST_MAX_LZ,
            lz_step_size: TEST_LZ_STEP,
            min_entropy_multiplier: TEST_MIN_ENTROPY,
            max_entropy_multiplier: TEST_MAX_ENTROPY,
            entropy_step_size: TEST_ENTROPY_STEP,
        };

        // Create mock merged results
        let mut merged_results = create_mock_merged_results();

        // Get references to the split and custom comparisons for cleaner code
        let split_comparison = &merged_results.split_comparisons[0];
        let custom_comparison = &merged_results.custom_comparisons[0];

        // Save the original estimated sizes for comparison
        let original_split_estimated_size_g1 = split_comparison.group1_metrics.estimated_size;
        let original_split_estimated_size_g2 = split_comparison.group2_metrics.estimated_size;
        let original_custom_estimated_size_baseline =
            custom_comparison.baseline_metrics.estimated_size;
        let original_custom_estimated_size_group =
            custom_comparison.group_metrics[0].estimated_size;

        // Run the optimization
        let (split_results, custom_results) =
            optimize_and_apply_coefficients(&mut merged_results, Some(&config));

        // After optimization, get references again as the merged_results was mutated
        let split_comparison = &merged_results.split_comparisons[0];
        let custom_comparison = &merged_results.custom_comparisons[0];
        let original_result = &merged_results.original_results[0];

        // Verify split optimization results
        assert!(!split_results.is_empty());
        assert_eq!(split_results[0].0, TEST_NAME_SPLIT);

        // Verify that the coefficients were applied and estimated sizes were updated
        assert_ne!(
            split_comparison.group1_metrics.estimated_size,
            original_split_estimated_size_g1
        );
        assert_ne!(
            split_comparison.group2_metrics.estimated_size,
            original_split_estimated_size_g2
        );

        // Verify custom optimization results
        assert!(!custom_results.is_empty());
        assert_eq!(custom_results[0].0, TEST_NAME_CUSTOM);

        // Verify that the coefficients were applied and estimated sizes were updated
        assert_ne!(
            custom_comparison.baseline_metrics.estimated_size,
            original_custom_estimated_size_baseline
        );
        assert_ne!(
            custom_comparison.group_metrics[0].estimated_size,
            original_custom_estimated_size_group
        );

        // Verify that original results were also updated
        assert_ne!(
            original_result.split_comparisons[0]
                .group1_metrics
                .estimated_size,
            original_split_estimated_size_g1
        );
        assert_ne!(
            original_result.custom_comparisons[0]
                .baseline_metrics
                .estimated_size,
            original_custom_estimated_size_baseline
        );
    }

    #[test]
    fn can_update_group_metrics() {
        // Create metrics with test constants
        let mut metrics = GroupComparisonMetrics {
            lz_matches: GROUP1_LZ_MATCHES,
            entropy: GROUP1_ENTROPY,
            estimated_size: GROUP1_ESTIMATED_SIZE,
            zstd_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
        };

        let original_estimated_size = metrics.estimated_size;

        // Update with different coefficients
        update_group_metrics(&mut metrics, TEST_MIN_LZ * 2.0, TEST_MIN_ENTROPY + 0.05);

        // Verify that the estimated size was updated
        assert_ne!(metrics.estimated_size, original_estimated_size);

        // Verify that the other fields remain unchanged
        assert_eq!(metrics.lz_matches, GROUP1_LZ_MATCHES);
        assert_eq!(metrics.entropy, GROUP1_ENTROPY);
        assert_eq!(metrics.zstd_size, GROUP1_ZSTD_SIZE);
        assert_eq!(metrics.original_size, GROUP1_ORIGINAL_SIZE);
    }

    #[test]
    fn can_calculate_group_difference() {
        // Create test groups using constants
        let group1_metrics = GroupComparisonMetrics {
            lz_matches: GROUP1_LZ_MATCHES,
            entropy: GROUP1_ENTROPY,
            estimated_size: GROUP1_ESTIMATED_SIZE,
            zstd_size: GROUP1_ZSTD_SIZE,
            original_size: GROUP1_ORIGINAL_SIZE,
        };

        let group2_metrics = GroupComparisonMetrics {
            lz_matches: GROUP2_LZ_MATCHES,
            entropy: GROUP2_ENTROPY,
            estimated_size: GROUP2_ESTIMATED_SIZE,
            zstd_size: GROUP2_ZSTD_SIZE,
            original_size: GROUP2_ORIGINAL_SIZE,
        };

        let mut difference = GroupDifference {
            lz_matches: 0,     // Will be updated
            entropy: 0.0,      // Will be updated
            estimated_size: 0, // Will be updated
            zstd_size: 0,      // Will be updated
            original_size: 0,  // Will be updated
        };

        // Update the difference using our function
        update_group_difference(&group1_metrics, &group2_metrics, &mut difference);

        // Verify that the estimated_size field was updated correctly
        assert_eq!(difference.estimated_size, DIFF_ESTIMATED_SIZE);

        // Calculate expected values for other fields (if they were updated by update_group_difference)
        // For now, we're only testing estimated_size since that's all our function updates
    }
}
