use super::{calculate_error, BruteForceConfig, OptimizationResult};
use crate::results::{
    analysis_results::AnalysisResults, merged_analysis_results::MergedAnalysisResults,
};

/// Result of a brute force optimization on a custom comparison.
#[derive(Debug, Clone)]
pub struct CustomComparisonOptimizationResult {
    /// Optimal parameters for the baseline group
    pub baseline: OptimizationResult,
    /// Optimal parameters for each comparison group
    pub comparisons: Box<[OptimizationResult]>,
}

/// Finds the optimal values for `lz_match_multiplier` and `entropy_multiplier` for all custom
/// comparison results within a given [`MergedAnalysisResults`] item.
///
/// # Arguments
///
/// * `merged_results` - Mutable reference to the [`MergedAnalysisResults`] containing the data.
///   This is where we pull the data from, and where we will update the results.
/// * `config` - Configuration for the optimization process (optional, uses default if [`None`])
pub fn find_optimal_custom_result_coefficients(
    merged_results: &mut MergedAnalysisResults,
    config: Option<&BruteForceConfig>,
) -> Vec<(String, CustomComparisonOptimizationResult)> {
    // Use a reference to the default config if None is provided
    let default_config = BruteForceConfig::default();
    let config = config.unwrap_or(&default_config);

    let mut results: Vec<(String, CustomComparisonOptimizationResult)> = Vec::new();

    for (comparison_idx, comparison) in merged_results.custom_comparisons.iter().enumerate() {
        results.push((
            comparison.name.clone(),
            find_optimal_custom_result_coefficients_for_comparison(
                comparison_idx,
                config,
                &merged_results.original_results,
            ),
        ));
    }

    results
}

/// This function finds the optimal coefficients for the baseline and each comparison group in a custom comparison
fn find_optimal_custom_result_coefficients_for_comparison(
    comparison_idx: usize,
    config: &BruteForceConfig,
    original_results: &[AnalysisResults], // guaranteed non-empty
) -> CustomComparisonOptimizationResult {
    let mut baseline_best = OptimizationResult::default();
    let mut min_error_baseline = f64::MAX;

    // Get the first result to determine how many comparison groups exist
    let first_result = &original_results[0].custom_comparisons[comparison_idx];
    let num_comparisons = first_result.group_metrics.len();

    // Initialize comparison group optimization results
    let mut comparison_bests = vec![OptimizationResult::default(); num_comparisons];
    let mut min_error_comparisons = vec![f64::MAX; num_comparisons].into_boxed_slice();

    let mut lz_multiplier = config.min_lz_multiplier;
    while lz_multiplier <= config.max_lz_multiplier {
        let mut entropy_multiplier = config.min_entropy_multiplier;
        while entropy_multiplier <= config.max_entropy_multiplier {
            // With the given coefficients, calculate the error for baseline and all comparison groups
            let errors = calculate_error_for_all_results(
                original_results,
                comparison_idx,
                lz_multiplier,
                entropy_multiplier,
            );

            // Check if the baseline error is better than the current best
            if errors.0 < min_error_baseline {
                baseline_best = OptimizationResult {
                    lz_match_multiplier: lz_multiplier,
                    entropy_multiplier,
                };

                min_error_baseline = errors.0;
            }

            // Check if any comparison group errors are better than their current best
            for (i, &error) in errors.1.iter().enumerate() {
                if error < min_error_comparisons[i] {
                    comparison_bests[i] = OptimizationResult {
                        lz_match_multiplier: lz_multiplier,
                        entropy_multiplier,
                    };

                    min_error_comparisons[i] = error;
                }
            }

            entropy_multiplier += config.entropy_step_size;
        }

        lz_multiplier += config.lz_step_size;
    }

    CustomComparisonOptimizationResult {
        baseline: baseline_best,
        comparisons: comparison_bests.into_boxed_slice(),
    }
}

/// Calculates the error for a given set of LZ match and entropy multipliers.
/// This returns the sum of all of the errors for all results in &[AnalysisResults].
///
/// # Arguments
///
/// * `analysis_results` - The [`AnalysisResults`] to calculate the error total for
/// * `comparison_idx` - The index of the custom comparison to calculate the error for
/// * `lz_match_multiplier` - The current LZ match multiplier
/// * `entropy_multiplier` - The current entropy multiplier
///
/// # Returns
///
/// A tuple, with the first value containing the sum of all of the errors for the baseline group,
/// and the second value containing a vector of errors for each comparison group.
#[inline(always)]
fn calculate_error_for_all_results(
    analysis_results: &[AnalysisResults],
    comparison_idx: usize,
    lz_match_multiplier: f64,
    entropy_multiplier: f64,
) -> (f64, Box<[f64]>) {
    let mut baseline_total_error = 0.0f64;

    // Initialize the comparison group errors
    let first_result = &analysis_results[0].custom_comparisons[comparison_idx];
    let num_comparisons = first_result.group_metrics.len();
    let mut comparison_total_errors = vec![0.0f64; num_comparisons].into_boxed_slice();

    for result in analysis_results {
        let comparison = &result.custom_comparisons[comparison_idx];

        // Calculate error for baseline
        baseline_total_error += calculate_error(
            comparison.baseline_metrics.lz_matches,
            comparison.baseline_metrics.entropy,
            comparison.baseline_metrics.zstd_size,
            comparison.baseline_metrics.original_size,
            lz_match_multiplier,
            entropy_multiplier,
        );

        // Calculate error for each comparison group
        for (i, metrics) in comparison.group_metrics.iter().enumerate() {
            comparison_total_errors[i] += calculate_error(
                metrics.lz_matches,
                metrics.entropy,
                metrics.zstd_size,
                metrics.original_size,
                lz_match_multiplier,
                entropy_multiplier,
            );
        }
    }

    (baseline_total_error, comparison_total_errors)
}

/// Print optimization results in a user-friendly format.
///
/// # Arguments
///
/// * `results` - Vector of (comparison name, CustomComparisonOptimizationResult) tuples
pub fn print_optimization_results(results: &[(String, CustomComparisonOptimizationResult)]) {
    println!("\n=== Custom Comparison Parameter Optimization Results ===");
    println!("Comparison Name | Group | LZ Multiplier | Entropy Multiplier |");
    println!("----------------|-------|---------------|--------------------|");

    for (name, result) in results {
        println!(
            "{:<16}|{:<7}|{:<15.3}|{:<20.3}|",
            name, "BASE", result.baseline.lz_match_multiplier, result.baseline.entropy_multiplier
        );

        for (i, comparison) in result.comparisons.iter().enumerate() {
            println!(
                "{:<16}|{:<7}|{:<15.3}|{:<20.3}|",
                "", i, comparison.lz_match_multiplier, comparison.entropy_multiplier
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use ahash::AHashMap;

    use super::*;
    use crate::{
        comparison::compare_groups::GroupComparisonResult,
        comparison::{GroupComparisonMetrics, GroupDifference},
    };

    // Helper function to create a mock GroupComparisonResult for testing
    #[allow(clippy::too_many_arguments)]
    fn create_mock_group_comparison_result(
        name: &str,
        baseline_lz_matches: u64,
        baseline_entropy: f64,
        baseline_zstd_size: u64,
        baseline_original_size: u64,
        comparison_group_count: usize,
        comparison_lz_matches: u64,
        comparison_entropy: f64,
        comparison_zstd_size: u64,
        comparison_original_size: u64,
    ) -> GroupComparisonResult {
        let baseline_metrics = GroupComparisonMetrics {
            lz_matches: baseline_lz_matches,
            entropy: baseline_entropy,
            estimated_size: 0, // Not used in testing
            zstd_size: baseline_zstd_size,
            original_size: baseline_original_size,
        };

        let mut group_names = Vec::with_capacity(comparison_group_count);
        let mut group_metrics = Vec::with_capacity(comparison_group_count);
        let mut differences = Vec::with_capacity(comparison_group_count);

        for i in 0..comparison_group_count {
            group_names.push(format!("group_{}", i));

            let metrics = GroupComparisonMetrics {
                lz_matches: comparison_lz_matches,
                entropy: comparison_entropy,
                estimated_size: 0, // Not used in testing
                zstd_size: comparison_zstd_size,
                original_size: comparison_original_size,
            };

            group_metrics.push(metrics);
            differences.push(GroupDifference::from_metrics(&baseline_metrics, &metrics));
        }

        GroupComparisonResult {
            name: name.to_string(),
            description: "Test comparison".to_string(),
            baseline_metrics,
            group_names,
            group_metrics,
            differences,
        }
    }

    // Helper function to create a mock AnalysisResults with a custom comparison
    #[allow(clippy::too_many_arguments)]
    fn create_mock_analysis_results_with_custom(
        comparison_name: &str,
        baseline_lz_matches: u64,
        baseline_entropy: f64,
        baseline_zstd_size: u64,
        baseline_original_size: u64,
        comparison_0_group_count: usize,
        comparison_0_lz_matches: u64,
        comparison_0_entropy: f64,
        comparison_0_zstd_size: u64,
        comparison_0_original_size: u64,
    ) -> AnalysisResults {
        let custom_comparison = create_mock_group_comparison_result(
            comparison_name,
            baseline_lz_matches,
            baseline_entropy,
            baseline_zstd_size,
            baseline_original_size,
            comparison_0_group_count,
            comparison_0_lz_matches,
            comparison_0_entropy,
            comparison_0_zstd_size,
            comparison_0_original_size,
        );

        AnalysisResults {
            schema_metadata: crate::schema::Metadata {
                name: "Test Schema".to_string(),
                description: "Test Schema Description".to_string(),
            },
            file_entropy: 0.0,
            file_lz_matches: 0,
            zstd_file_size: 0,
            original_size: 0,
            per_field: AHashMap::new(),
            split_comparisons: Vec::new(),
            custom_comparisons: vec![custom_comparison],
        }
    }

    #[test]
    fn can_find_optimal_custom_result_coefficients() {
        // Create a mock analysis results with a custom comparison
        let analysis_results1 = create_mock_analysis_results_with_custom(
            "test_comparison",
            100, // baseline
            1.0,
            110,
            1000,
            2, // comparison_group_count
            210,
            1.6,
            230,
            1000,
        );

        // Create a merged analysis results from the mock analysis results
        let mut merged_results = MergedAnalysisResults::new(&analysis_results1);
        let config = BruteForceConfig::default();

        // Find optimal coefficients
        let optimal_results =
            find_optimal_custom_result_coefficients(&mut merged_results, Some(&config));

        // Verify results
        assert_eq!(optimal_results.len(), 1);
        assert_eq!(optimal_results[0].0, "test_comparison");

        // Verify there are results for baseline and 2 comparison groups
        // Check baseline values are within config range
        assert!(optimal_results[0].1.baseline.lz_match_multiplier >= config.min_lz_multiplier);
        assert!(optimal_results[0].1.baseline.lz_match_multiplier <= config.max_lz_multiplier);
        assert!(optimal_results[0].1.baseline.entropy_multiplier >= config.min_entropy_multiplier);
        assert!(optimal_results[0].1.baseline.entropy_multiplier <= config.max_entropy_multiplier);

        // Check we have the right number of comparison groups
        assert_eq!(optimal_results[0].1.comparisons.len(), 2);

        // Check first comparison group values are within config range
        let comparisons = &optimal_results[0].1.comparisons;
        assert!(comparisons[0].lz_match_multiplier >= config.min_lz_multiplier);
        assert!(comparisons[0].lz_match_multiplier <= config.max_lz_multiplier);
        assert!(comparisons[0].entropy_multiplier >= config.min_entropy_multiplier);
        assert!(comparisons[0].entropy_multiplier <= config.max_entropy_multiplier);

        // Check second comparison group values are within config range
        assert!(comparisons[1].lz_match_multiplier >= config.min_lz_multiplier);
        assert!(comparisons[1].lz_match_multiplier <= config.max_lz_multiplier);
        assert!(comparisons[1].entropy_multiplier >= config.min_entropy_multiplier);
        assert!(comparisons[1].entropy_multiplier <= config.max_entropy_multiplier);

        // Assert the error is below 5 (known correct threshold)
        let original_results = vec![analysis_results1];
        let (baseline_error, comparison_errors) = calculate_error_for_all_results(
            &original_results,
            0,
            optimal_results[0].1.baseline.lz_match_multiplier,
            optimal_results[0].1.baseline.entropy_multiplier,
        );
        assert!(
            baseline_error < 5.0,
            "Baseline error {} should be less than 5.0",
            baseline_error
        );

        // Check errors for each comparison group
        for (i, &error) in comparison_errors.iter().enumerate() {
            let (_, comparison_errors) = calculate_error_for_all_results(
                &original_results,
                0,
                optimal_results[0].1.comparisons[i].lz_match_multiplier,
                optimal_results[0].1.comparisons[i].entropy_multiplier,
            );

            assert!(
                comparison_errors[i] < 5.0,
                "Comparison group {} error {} should be less than 5.0",
                i,
                error
            );
        }
    }

    #[test]
    fn handles_empty_custom_results() {
        // Create empty analysis results
        let analysis_results = AnalysisResults::default();

        // Create a merged analysis results with no custom comparisons
        let mut merged_results = MergedAnalysisResults::new(&analysis_results);

        // Find optimal coefficients
        let optimal_results = find_optimal_custom_result_coefficients(&mut merged_results, None);

        // Verify results are empty
        assert!(optimal_results.is_empty());
    }
}
