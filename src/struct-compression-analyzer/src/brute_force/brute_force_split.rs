use super::{
    find_optimal_coefficients_for_metrics_parallel, BruteForceComparisonMetrics, BruteForceConfig,
    OptimizationResult,
};
use crate::results::analysis_results::AnalysisResults;

/// Result of a brute force optimization on a split comparison.
#[derive(Debug, Clone, Copy)]
pub struct SplitComparisonOptimizationResult {
    /// Optimal parameters for group 1
    pub group_1: OptimizationResult,
    /// Optimal parameters for group 2
    pub group_2: OptimizationResult,
}

/// Finds the optimal values for `lz_match_multiplier` and `entropy_multiplier` for all split
/// results within a given [`MergedAnalysisResults`] item.
///
/// # Arguments
///
/// * `merged_results` - Mutable reference to the [`MergedAnalysisResults`] containing the data.
///   This is where we pull the data from, and where we will update the results.
/// * `config` - Configuration for the optimization process (optional, uses default if [`None`])
pub fn find_optimal_split_result_coefficients(
    individual_results: &mut [AnalysisResults],
    config: Option<&BruteForceConfig>,
) -> Vec<(String, SplitComparisonOptimizationResult)> {
    let default_config = BruteForceConfig::default();
    let config = config.unwrap_or(&default_config);

    let mut results: Vec<(String, SplitComparisonOptimizationResult)> = Vec::new();

    for (comparison_idx, comparison) in individual_results[0].split_comparisons.iter().enumerate() {
        results.push((
            comparison.name.clone(),
            find_optimal_split_result_coefficients_for_comparison(
                comparison_idx,
                config,
                individual_results,
            ),
        ));
    }

    results
}

/// This function finds the optimal coefficients for both groups in a split comparison
fn find_optimal_split_result_coefficients_for_comparison(
    comparison_idx: usize,
    config: &BruteForceConfig,
    original_results: &[AnalysisResults], // guaranteed non-empty
) -> SplitComparisonOptimizationResult {
    // Find optimal coefficients for group 1
    let group1_metrics = extract_group1_metrics(comparison_idx, original_results);
    let group1_best = find_optimal_coefficients_for_metrics_parallel(&group1_metrics, config);

    // Find optimal coefficients for group 2
    let group2_metrics = extract_group2_metrics(comparison_idx, original_results);
    let group2_best = find_optimal_coefficients_for_metrics_parallel(&group2_metrics, config);

    SplitComparisonOptimizationResult {
        group_1: group1_best,
        group_2: group2_best,
    }
}

/// Extracts all the group 1 metrics from each [`AnalysisResults`], at a given comparison index.
/// Returns a boxed slice of all metrics.
fn extract_group1_metrics(
    comparison_idx: usize,
    original_results: &[AnalysisResults], // guaranteed non-empty
) -> Box<[BruteForceComparisonMetrics]> {
    original_results
        .iter()
        .map(|result| {
            result.split_comparisons[comparison_idx]
                .group1_metrics
                .into()
        })
        .collect()
}

/// Extracts all the group 2 metrics from each [`AnalysisResults`], at a given comparison index.
/// Returns a boxed slice of all metrics.
fn extract_group2_metrics(
    comparison_idx: usize,
    original_results: &[AnalysisResults], // guaranteed non-empty
) -> Box<[BruteForceComparisonMetrics]> {
    original_results
        .iter()
        .map(|result| {
            result.split_comparisons[comparison_idx]
                .group2_metrics
                .into()
        })
        .collect()
}

/// Print optimization results in a user-friendly format.
///
/// # Arguments
///
/// * `results` - Vector of (comparison name, OptimizationResult) tuples
pub fn print_optimization_results(results: &[(String, SplitComparisonOptimizationResult)]) {
    println!("=== Split Comparison Parameter Optimization Results ===");
    println!("Comparison Name               | Group | LZ Multiplier | Entropy Multiplier |");
    println!("------------------------------|-------|---------------|--------------------|");

    for (name, result) in results {
        println!(
            "{:<30}|{:<7}|{:<15.4}|{:<20.4}|",
            name, "G1", result.group_1.lz_match_multiplier, result.group_1.entropy_multiplier
        );
        println!(
            "{:<30}|{:<7}|{:<15.4}|{:<20.4}|",
            "", "G2", result.group_2.lz_match_multiplier, result.group_2.entropy_multiplier
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        brute_force::calculate_error_for_bruteforce_metrics,
        comparison::{
            split_comparison::SplitComparisonResult, GroupComparisonMetrics, GroupDifference,
        },
        results::analysis_results::AnalysisResults,
    };

    /// Creates a simple mock AnalysisResults instance for testing
    #[allow(clippy::too_many_arguments)]
    fn create_mock_analysis_results(
        group1_lz_matches: u64,
        group1_entropy: f64,
        group1_zstd_size: u64,
        group1_original_size: u64,
        group2_lz_matches: u64,
        group2_entropy: f64,
        group2_zstd_size: u64,
        group2_original_size: u64,
    ) -> AnalysisResults {
        let group1_metrics = GroupComparisonMetrics {
            lz_matches: group1_lz_matches,
            entropy: group1_entropy,
            estimated_size: 0, // Not used in optimization
            zstd_size: group1_zstd_size,
            original_size: group1_original_size,
        };

        let group2_metrics = GroupComparisonMetrics {
            lz_matches: group2_lz_matches,
            entropy: group2_entropy,
            estimated_size: 0, // Not used in optimization
            zstd_size: group2_zstd_size,
            original_size: group2_original_size,
        };

        let difference = GroupDifference::from_metrics(&group1_metrics, &group2_metrics);

        let split_comparison = SplitComparisonResult {
            name: "test_comparison".to_string(),
            description: "Test comparison for optimization".to_string(),
            group1_metrics,
            group2_metrics,
            difference,
            baseline_comparison_metrics: vec![],
            split_comparison_metrics: vec![],
        };

        AnalysisResults {
            split_comparisons: vec![split_comparison],
            ..Default::default()
        }
    }

    #[test]
    fn can_find_optimal_split_result_coefficients() {
        let config = BruteForceConfig::default();

        // Create two mock analysis results with the same split comparison
        let results1 = create_mock_analysis_results(
            100, 1.0, 110, 1000, // Group 1
            200, 1.5, 220, 1000, // Group 2
        );

        let results2 = create_mock_analysis_results(
            110, 1.1, 120, 1000, // Group 1
            210, 1.6, 230, 1000, // Group 2
        );

        let original_results = vec![results1, results2];

        // Call the function we're testing
        let result = find_optimal_split_result_coefficients_for_comparison(
            0, // First comparison
            &config,
            &original_results,
        );

        // Assert that the result has reasonable values within our configured ranges
        assert!(result.group_1.lz_match_multiplier >= config.min_lz_multiplier);
        assert!(result.group_1.lz_match_multiplier <= config.max_lz_multiplier);
        assert!(result.group_1.entropy_multiplier >= config.min_entropy_multiplier);
        assert!(result.group_1.entropy_multiplier <= config.max_entropy_multiplier);

        assert!(result.group_2.lz_match_multiplier >= config.min_lz_multiplier);
        assert!(result.group_2.lz_match_multiplier <= config.max_lz_multiplier);
        assert!(result.group_2.entropy_multiplier >= config.min_entropy_multiplier);
        assert!(result.group_2.entropy_multiplier <= config.max_entropy_multiplier);

        // Assert the error is below 5 (known correct assumption)
        let group1_metrics = extract_group1_metrics(0, &original_results);
        let group1_error = calculate_error_for_bruteforce_metrics(
            &group1_metrics,
            result.group_1.lz_match_multiplier,
            result.group_1.entropy_multiplier,
        );
        assert!(group1_error < 5.0);

        let group2_metrics = extract_group2_metrics(0, &original_results);
        let group2_error = calculate_error_for_bruteforce_metrics(
            &group2_metrics,
            result.group_2.lz_match_multiplier,
            result.group_2.entropy_multiplier,
        );
        assert!(group2_error < 5.0);
    }

    #[test]
    fn handles_empty_split_results() {
        // Test the function with an empty results array
        let config = BruteForceConfig::default();
        let empty_results: Vec<AnalysisResults> = vec![];

        let result =
            find_optimal_split_result_coefficients_for_comparison(0, &config, &empty_results);

        // Should return default values when no results are provided
        assert_eq!(result.group_1.lz_match_multiplier, config.min_lz_multiplier);
        assert_eq!(
            result.group_1.entropy_multiplier,
            config.min_entropy_multiplier
        );
        assert_eq!(result.group_2.lz_match_multiplier, config.min_lz_multiplier);
        assert_eq!(
            result.group_2.entropy_multiplier,
            config.min_entropy_multiplier
        );
    }
}
