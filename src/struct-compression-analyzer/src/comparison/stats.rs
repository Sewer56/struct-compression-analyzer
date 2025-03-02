//! Statistical functions for analyzing compression metrics.
//!
//! This module provides functionality for calculating and analyzing statistical
//! measures related to compression ratios and other metrics.
//!
//! # Types
//!
//! - [`Stats`]: Container for a complete set of statistical measures including
//!   quartiles, mean, median, IQR, min/max, and sample count.
//!
//! # Functions
//!
//! ## Core Statistics
//!
//! - [`calculate_stats`]: Calculate comprehensive statistics for an array of values
//! - [`calculate_percentile`]: Helper function to calculate a specific percentile
//! - [`format_stats`]: Format statistics as a human-readable string
//!
//! ## ZSTD Compression Ratio Statistics
//!
//! - [`calculate_zstd_ratio_stats`]: Statistics for ZSTD ratios in split comparisons
//! - [`calculate_custom_zstd_ratio_stats`]: Statistics for ZSTD ratios in custom comparisons
//!
//! # Statistical Measures
//!
//! The module provides calculation of:
//! - Interquartile Range (IQR)
//! - Percentile ranges (Q1, median, Q3)
//! - Minimum and maximum values
//! - Mean (average)
//! - Sample count

use crate::{plot::calc_ratio_f64, results::analysis_results::AnalysisResults};
use core::cmp::Ordering;

/// Statistics for a set of numeric values.
#[derive(Debug, Clone, Copy)]
pub struct Stats {
    /// Minimum value
    pub min: f64,
    /// First quartile (25th percentile)
    pub q1: f64,
    /// Median (50th percentile)
    pub median: f64,
    /// Third quartile (75th percentile)
    pub q3: f64,
    /// Maximum value
    pub max: f64,
    /// Interquartile range (IQR = Q3 - Q1)
    pub iqr: f64,
    /// Mean (average) value
    pub mean: f64,
    /// Sample size
    pub count: usize,
}

/// Calculate statistics for an array of values.
///
/// This function calculates various statistics including min, max, quartiles,
/// interquartile range (IQR), and mean.
///
/// # Arguments
///
/// * `values` - Slice of values to analyze
///
/// # Returns
///
/// A [`Stats`] struct containing the calculated statistics
pub fn calculate_stats(values: &[f64]) -> Option<Stats> {
    let count = values.len();
    if count == 0 {
        return None;
    }

    let mut sorted_values = values.to_vec();
    sorted_values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let min = sorted_values[0];
    let max = sorted_values[count - 1];

    // Calculate mean
    let sum: f64 = sorted_values.iter().sum();
    let mean = sum / count as f64;

    // Calculate median and quartiles
    let median = calculate_percentile(&sorted_values, 0.5);
    let q1 = calculate_percentile(&sorted_values, 0.25);
    let q3 = calculate_percentile(&sorted_values, 0.75);
    let iqr = q3 - q1;

    Some(Stats {
        min,
        q1,
        median,
        q3,
        max,
        iqr,
        mean,
        count,
    })
}

/// Calculate a specific percentile of values.
///
/// # Arguments
///
/// * `sorted_values` - Sorted slice of values
/// * `percentile` - Percentile to calculate (0.0 to 1.0)
///
/// # Returns
///
/// The value at the specified percentile
fn calculate_percentile(sorted_values: &[f64], percentile: f64) -> f64 {
    let count = sorted_values.len();
    if count == 0 {
        return 0.0;
    }

    let index = percentile * (count - 1) as f64;
    let lower_idx = index.floor() as usize;
    let upper_idx = index.ceil() as usize;

    if lower_idx == upper_idx {
        sorted_values[lower_idx]
    } else {
        let weight = index - lower_idx as f64;
        sorted_values[lower_idx] * (1.0 - weight) + sorted_values[upper_idx] * weight
    }
}

/// Calculate ZSTD ratio statistics between two groups in split comparison.
///
/// This function calculates the ZSTD compression ratio statistics between
/// group1_metrics and group2_metrics using the results array.
///
/// # Arguments
///
/// * `results` - Slice of analysis results
/// * `comparison_index` - Index of the comparison to analyze
///
/// # Returns
///
/// Optional [`Stats`] struct containing the ratio statistics, or [`None`] if there are no results
pub fn calculate_zstd_ratio_stats(
    results: &[AnalysisResults],
    comparison_index: usize,
) -> Option<Stats> {
    let ratios: Vec<f64> = results
        .iter()
        .filter_map(|result| {
            result
                .split_comparisons
                .get(comparison_index)
                .map(|comparison| {
                    calc_ratio_f64(
                        comparison.group2_metrics.zstd_size,
                        comparison.group1_metrics.zstd_size,
                    )
                })
        })
        .collect();

    calculate_stats(&ratios)
}

/// Calculate ZSTD ratio statistics between two groups in custom comparison.
///
/// This function calculates the ZSTD compression ratio statistics between
/// a specific group in group_metrics and the baseline metrics.
///
/// # Arguments
///
/// * `results` - Slice of analysis results
/// * `comparison_index` - Index of the custom comparison to analyze
/// * `group_index` - Index of the group within group_metrics to compare with baseline
///
/// # Returns
///
/// Optional [`Stats`] struct containing the ratio statistics, or [`None`] if there are no results
pub fn calculate_custom_zstd_ratio_stats(
    results: &[AnalysisResults],
    comparison_index: usize,
    group_index: usize,
) -> Option<Stats> {
    let ratios: Vec<f64> = results
        .iter()
        .filter_map(|result| {
            if let Some(comparison) = result.custom_comparisons.get(comparison_index) {
                // Only include results where the group_index is valid
                comparison
                    .group_metrics
                    .get(group_index)
                    .map(|group_metrics| {
                        calc_ratio_f64(
                            group_metrics.zstd_size,
                            comparison.baseline_metrics.zstd_size,
                        )
                    })
            } else {
                None
            }
        })
        .collect();

    calculate_stats(&ratios)
}

/// Format statistics as a string.
///
/// # Arguments
///
/// * `stats` - The statistics to format
///
/// # Returns
///
/// A formatted string representation of the statistics
pub fn format_stats(stats: &Stats) -> String {
    format!(
        "min: {:.3}, Q1: {:.3}, median: {:.3}, Q3: {:.3}, max: {:.3}, IQR: {:.3}, mean: {:.3} (n={})",
        stats.min, stats.q1, stats.median, stats.q3, stats.max, stats.iqr, stats.mean, stats.count
    )
}
