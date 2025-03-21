//! Brute force optimization for LZ match and entropy multiplier parameters.
//!
//! This module provides functionality to find optimal values for the
//! [`lz_match_multiplier`] and [`entropy_multiplier`] parameters used in the
//! [`size_estimate`] function.
//!
//! It exposes two main optimization approaches:
//!
//! *   **Split comparisons:** Optimizes parameters for two groups being compared directly using the
//!     [`find_optimal_split_result_coefficients`] function. Results are returned as
//!     [`SplitComparisonOptimizationResult`].
//!
//! *   **Custom comparisons:** Optimizes parameters for custom groups with a variable number of
//!     comparisons against a baseline group using the [`find_optimal_custom_result_coefficients`]
//!     function. Results are returned as [`CustomComparisonOptimizationResult`].
//!
//! The main entry point for using this module is the [`optimize_and_apply_coefficients`] function,
//! which performs the optimization and applies the resulting coefficients to an existing
//! [`MergedAnalysisResults`] object in place.
//!
//! [`size_estimate`]: crate::utils::analyze_utils::size_estimate
//! [`lz_match_multiplier`]: crate::analyzer::SizeEstimationParameters::lz_match_multiplier
//! [`entropy_multiplier`]: crate::analyzer::SizeEstimationParameters::entropy_multiplier
//! [`find_optimal_split_result_coefficients`]: crate::brute_force::find_optimal_split_result_coefficients
//! [`find_optimal_custom_result_coefficients`]: crate::brute_force::find_optimal_custom_result_coefficients
//! [`SplitComparisonOptimizationResult`]: crate::brute_force::SplitComparisonOptimizationResult
//! [`CustomComparisonOptimizationResult`]: crate::brute_force::CustomComparisonOptimizationResult
//! [`optimize_and_apply_coefficients`]: crate::brute_force::optimize_and_apply_coefficients
//! [`MergedAnalysisResults`]: crate::results::merged_analysis_results::MergedAnalysisResults

pub mod brute_force_custom;
pub mod brute_force_split;
use crate::analyzer::SizeEstimationParameters;
use crate::comparison::{GroupComparisonMetrics, GroupDifference};
use crate::results::analysis_results::AnalysisResults;
use crate::utils::analyze_utils::size_estimate;
use brute_force_custom::{
    find_optimal_custom_result_coefficients, CustomComparisonOptimizationResult,
};
use brute_force_split::{
    find_optimal_split_result_coefficients, SplitComparisonOptimizationResult,
};
use rayon::prelude::*;

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
            min_lz_multiplier: 0.0001,
            max_lz_multiplier: 1.0,
            lz_step_size: 0.0001,
            min_entropy_multiplier: 1.0,
            max_entropy_multiplier: 1.75,
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
    num_lz_matches: u64,
    entropy: f64,
    // Actual Compression Stats
    zstd_size: u64,
    original_size: u64,
    // Coefficients to Test
    lz_match_multiplier: f64,
    entropy_multiplier: f64,
) -> f64 {
    // Calculate estimated size with current coefficients
    let estimated_size = size_estimate(SizeEstimationParameters {
        name: "",
        data_len: original_size as usize,
        data: None,
        num_lz_matches: num_lz_matches as usize,
        entropy,
        lz_match_multiplier,
        entropy_multiplier,
    });

    // Calculate error (difference between estimated and actual size)
    let error = ((estimated_size as f64) - (zstd_size as f64)).abs();

    // If the ratios are on the opposite side of 1.0
    // (i.e.) estimate thinks its worse, when its better, impose a 'killing'
    // penalty by giving it max error.
    let zstd_is_bigger = zstd_size > original_size;
    let estimate_is_bigger = estimated_size as u64 > original_size;
    if zstd_is_bigger != estimate_is_bigger {
        return f32::MAX as f64;
    }

    error
}

/// Optimizes and applies coefficients to a slice of [`AnalysisResults`] objects.
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
    merged_results: &mut [AnalysisResults],
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
/// * `individual_results` - The analysis results to update
/// * `split_optimization_results` - The optimization results for split comparisons
/// * `custom_optimization_results` - The optimization results for custom comparisons
pub fn apply_optimized_coefficients(
    individual_results: &mut [AnalysisResults],
    split_optimization_results: &[(String, SplitComparisonOptimizationResult)],
    custom_optimization_results: &[(String, CustomComparisonOptimizationResult)],
) {
    // Update split comparisons in merged results
    for (split_idx, comparison) in individual_results[0]
        .split_comparisons
        .iter_mut()
        .enumerate()
    {
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
    for (custom_idx, comparison) in individual_results[0]
        .custom_comparisons
        .iter_mut()
        .enumerate()
    {
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
    for result in individual_results {
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
/// * `writer` - The writer to print results to
/// * `split_results` - Optimization results for split comparisons
/// * `custom_results` - Optimization results for custom comparisons
pub fn print_all_optimization_results<W: std::io::Write>(
    writer: &mut W,
    split_results: &[(String, SplitComparisonOptimizationResult)],
    custom_results: &[(String, CustomComparisonOptimizationResult)],
) -> std::io::Result<()> {
    brute_force_split::print_optimization_results(writer, split_results)?;
    brute_force_custom::print_optimization_results(writer, custom_results)?;
    Ok(())
}

/// Optimized, reduced form of [`GroupComparisonMetrics`],
/// meant for storing only the fields used during brute forcing.
#[derive(Clone, Default, Debug, PartialEq, Copy)]
pub(crate) struct BruteForceComparisonMetrics {
    /// Number of total LZ matches
    pub lz_matches: u64,
    /// Amount of entropy in the input data set
    pub entropy: f64,
    /// Size compressed by zstd.
    pub zstd_size: u64,
    /// Size of the original data.
    pub original_size: u64,
}

impl From<GroupComparisonMetrics> for BruteForceComparisonMetrics {
    fn from(value: GroupComparisonMetrics) -> Self {
        BruteForceComparisonMetrics {
            lz_matches: value.lz_matches,
            entropy: value.entropy,
            zstd_size: value.zstd_size,
            original_size: value.original_size,
        }
    }
}

/// Finds the optimal coefficients (lz_match_multiplier and entropy_multiplier) for a given
/// set of metrics by running a brute force optimization. This runs in parallel on all threads.
///
/// # Arguments
///
/// * `metrics` - The metrics to find optimal coefficients for
/// * `config` - Configuration for the optimization process
///
/// # Returns
///
/// The optimal [`OptimizationResult`] containing the best coefficients
pub(crate) fn find_optimal_coefficients_for_metrics_parallel(
    metrics: &[BruteForceComparisonMetrics],
    config: &BruteForceConfig,
) -> OptimizationResult {
    // Determine how to split the lz range
    let num_chunks = rayon::current_num_threads();
    let lz_range = config.max_lz_multiplier - config.min_lz_multiplier;
    let chunk_size = lz_range / num_chunks as f64;

    // Create chunks for parallel processing
    let mut chunks = Vec::with_capacity(num_chunks);
    for x in 0..num_chunks {
        let start = config.min_lz_multiplier + (x as f64 * chunk_size);
        let end = if x == num_chunks - 1 {
            config.max_lz_multiplier
        } else {
            config.min_lz_multiplier + ((x + 1) as f64 * chunk_size)
        };

        chunks.push((start, end));
    }

    // Process chunks in parallel
    let results: Vec<_> = chunks
        .par_iter()
        .map(|(start, end)| {
            find_optimal_coefficients_for_metrics(
                metrics,
                &BruteForceConfig {
                    min_lz_multiplier: *start,
                    max_lz_multiplier: *end,
                    min_entropy_multiplier: config.min_entropy_multiplier,
                    max_entropy_multiplier: config.max_entropy_multiplier,
                    entropy_step_size: config.entropy_step_size,
                    lz_step_size: config.lz_step_size,
                },
            )
        })
        .collect();

    // Find the overall best result using a simple for loop
    let mut best_result = OptimizationResult::default();
    let mut min_error = f64::MAX;
    for (result, error) in results {
        if error < min_error {
            min_error = error;
            best_result = result;
        }
    }

    best_result
}

/// Finds the optimal coefficients (lz_match_multiplier and entropy_multiplier) for a given
/// set of metrics by running a brute force optimization.
///
/// # Arguments
///
/// * `metrics` - The metrics to find optimal coefficients for
/// * `config` - Configuration for the optimization process
///
/// # Returns
///
/// The optimal [`OptimizationResult`] containing the best coefficients,
/// and the minimum error found for this best result.
pub(crate) fn find_optimal_coefficients_for_metrics(
    metrics: &[BruteForceComparisonMetrics],
    config: &BruteForceConfig,
) -> (OptimizationResult, f64) {
    let mut best_result = OptimizationResult::default();
    let mut min_error = f64::MAX;

    let mut lz_multiplier = config.min_lz_multiplier;
    while lz_multiplier <= config.max_lz_multiplier {
        let mut entropy_multiplier = config.min_entropy_multiplier;
        while entropy_multiplier <= config.max_entropy_multiplier {
            // Calculate the error with the given coefficients
            let error =
                calculate_error_for_bruteforce_metrics(metrics, lz_multiplier, entropy_multiplier);

            // Update if better than current best
            if error < min_error {
                best_result = OptimizationResult {
                    lz_match_multiplier: lz_multiplier,
                    entropy_multiplier,
                };

                min_error = error;
            }

            entropy_multiplier += config.entropy_step_size;
        }

        lz_multiplier += config.lz_step_size;
    }

    (best_result, min_error)
}

/// Calculates the error for a given set of metrics with specified coefficients.
/// This returns the sum of all errors for all results in the metrics slice.
///
/// # Arguments
///
/// * `metrics` - The metrics to calculate the error for
/// * `lz_match_multiplier` - The LZ match multiplier to test
/// * `entropy_multiplier` - The entropy multiplier to test
///
/// # Returns
///
/// The sum of all errors for the given metrics with the specified coefficients
#[inline(always)]
pub(crate) fn calculate_error_for_bruteforce_metrics(
    metrics: &[BruteForceComparisonMetrics],
    lz_match_multiplier: f64,
    entropy_multiplier: f64,
) -> f64 {
    let mut total_error = 0.0f64;

    for result in metrics {
        total_error += calculate_error(
            result.lz_matches,
            result.entropy,
            result.zstd_size,
            result.original_size,
            lz_match_multiplier,
            entropy_multiplier,
        );
    }

    total_error
}

/// These tests are crap, they weren't written by a human, after all.
#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        comparison::{
            compare_groups::GroupComparisonResult, split_comparison::SplitComparisonResult,
        },
        results::analysis_results::AnalysisResults,
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

    /// Creates a simple mock AnalysisResults for testing
    fn create_mock_results() -> AnalysisResults {
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

        // Create mock original analysis results
        let schema_metadata = Metadata {
            name: TEST_SCHEMA_NAME.to_string(),
            description: TEST_SCHEMA_DESC.to_string(),
        };
        AnalysisResults {
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

        // Create mock result
        let mut results = vec![create_mock_results()];

        // Get references to the split and custom comparisons for cleaner code
        let split_comparison = &results[0].split_comparisons[0];
        let custom_comparison = &results[0].custom_comparisons[0];

        // Save the original estimated sizes for comparison
        let original_split_estimated_size_g1 = split_comparison.group1_metrics.estimated_size;
        let original_split_estimated_size_g2 = split_comparison.group2_metrics.estimated_size;
        let original_custom_estimated_size_baseline =
            custom_comparison.baseline_metrics.estimated_size;
        let original_custom_estimated_size_group =
            custom_comparison.group_metrics[0].estimated_size;

        // Run the optimization
        let (split_results, custom_results) =
            optimize_and_apply_coefficients(&mut results, Some(&config));

        // After optimization, get references again as the merged_results was mutated
        let split_comparison = &results[0].split_comparisons[0];
        let custom_comparison = &results[0].custom_comparisons[0];

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
            results[0].split_comparisons[0]
                .group1_metrics
                .estimated_size,
            original_split_estimated_size_g1
        );
        assert_ne!(
            results[0].custom_comparisons[0]
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
