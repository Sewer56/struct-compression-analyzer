//! Analyzes and processes final analysis results for bit-packed data structures.
//!
//! This module handles the final stage of analysis, computing metrics and statistics
//! from processed bit-packed data. It provides comprehensive analysis capabilities
//! including entropy calculations, LZ compression analysis, and field-level statistics.
//!
//! # Core Types
//!
//! - [`AnalysisResults`]: Top-level container for all analysis results
//! - [`FieldMetrics`]: Detailed metrics for individual fields
//! - [`PrintFormat`]: Output formatting options for result presentation
//!
//! # Key Features
//!
//! - Field-level and file-level entropy analysis
//! - LZ compression match detection
//! - Size estimation and actual compression metrics
//! - Bit distribution statistics
//! - Value frequency analysis
//! - Split comparison results
//!
//! # Public APIs
//!
//! Key types and functions for users of this module:
//!
//! ## Types
//!
//! - [`AnalysisResults`]: Primary container for analysis output
//!   - [`AnalysisResults::print()`]: Display results in console
//!   - [`AnalysisResults::try_merge_many()`]: Combine multiple analysis results
//!   - [`AnalysisResults::as_field_metrics()`]: Convert file statistics to field metrics
//!
//! - [`FieldMetrics`]: Per-field analysis data
//!   - [`FieldMetrics::parent_path()`]: Get path of parent field
//!   - [`FieldMetrics::parent_metrics_or()`]: Get metrics of parent field
//!   - [`FieldMetrics::sorted_value_counts()`]: Get sorted value frequencies
//!
//! ## Functions
//!
//! - [`compute_analysis_results()`]: Generate analysis from [`SchemaAnalyzer`]
//!
//! # Example
//!
//! ```no_run
//! use struct_compression_analyzer::{analyzer::SchemaAnalyzer, schema::Schema};
//! use struct_compression_analyzer::results::analysis_results::AnalysisResults;
//! use struct_compression_analyzer::analyzer::CompressionOptions;
//!
//! fn analyze_data(schema: &Schema, data: &[u8]) -> AnalysisResults {
//!     let options = CompressionOptions::default();
//!     let mut analyzer = SchemaAnalyzer::new(schema, options);
//!     analyzer.add_entry(data);
//!     analyzer.generate_results().unwrap()
//! }
//! ```
//!
//! # Output Formats
//!
//! Results can be displayed in two formats (console):
//!
//! - [`Detailed`]: Comprehensive analysis with full metrics
//! - [`Concise`]: Condensed summary of key statistics
//!
//! Groups of results (multiple files) can also be displayed via one of the
//! other modules.
//!
//! - [`CSV`]: CSV representation of results. Export to spreadsheets.
//! - [`Plot`]: Generate plots of results.
//!
//! # Field Metrics
//!
//! For each field, the analysis computes:
//!
//! - Shannon entropy in bits
//! - LZ compression matches
//! - Bit-level distribution
//! - Value frequency counts
//! - Size estimates (original, compressed, estimated)
//!
//! Fields can be analyzed individually or merged for group analysis.
//!
//! # Implementation Notes
//!
//! - Handles both MSB and LSB bit ordering
//! - Supports nested field hierarchies
//! - Provides parent/child relationship tracking
//! - Implements efficient metric merging for group analysis
//!
//! [`AnalysisResults`]: crate::analysis_results::AnalysisResults
//! [`FieldMetrics`]: crate::analysis_results::FieldMetrics
//! [`PrintFormat`]: crate::analysis_results::PrintFormat
//! [`Detailed`]: crate::analysis_results::PrintFormat::Detailed
//! [`Concise`]: crate::analysis_results::PrintFormat::Concise
//! [`CSV`]: crate::csv
//! [`Plot`]: crate::plot

pub mod analysis_results;
pub mod merged_analysis_results;

use crate::analyzer::BitStats;
use crate::comparison::compare_groups::GroupComparisonError;
use crate::results::analysis_results::AnalysisResults;
use crate::schema::BitOrder;
use crate::utils::constants::CHILD_MARKER;
use derive_more::FromStr;
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Error type for when merging analysis results fails.
#[derive(Debug, Error)]
pub enum AnalysisMergeError {
    #[error(
        "Number of bit counts did not match while merging `bit_counts`.
This indicates inconsistent input data, or merging of results that were computed differently."
    )]
    BitCountsDontMatch,

    #[error("Field length mismatch: {0} != {1}. This indicates inconsistent, different or incorrect input data.")]
    FieldLengthMismatch(u32, u32),
}

/// Error type for when something goes wrong when computing the final analysis results.
#[derive(Debug, Error)]
pub enum ComputeAnalysisResultsError {
    #[error(transparent)]
    GroupComparisonError(#[from] GroupComparisonError),
}

/// Complete analysis metrics for a single field
#[derive(Clone)]
pub struct FieldMetrics {
    /// Name of the field or group
    pub name: String,
    /// Name of the full path to the field or group
    pub full_path: String,
    /// The depth of the field in the group/field chain.
    pub depth: usize,
    /// Total number of observed values
    pub count: u64,
    /// Length of the field or group in bits.
    pub lenbits: u32,
    /// Shannon entropy in bits
    pub entropy: f64,
    /// LZ compression matches in the field
    pub lz_matches: usize,
    /// Bit-level statistics. Index of tuple is bit offset.
    pub bit_counts: Vec<BitStats>,
    /// The order of the bits within the field
    pub bit_order: BitOrder,
    /// Value → occurrence count
    /// Count of occurrences for each observed value.
    pub value_counts: FxHashMap<u64, u64>,
    /// Estimated size of the compressed data from our estimator
    pub estimated_size: usize,
    /// Actual size of the compressed data when compressed with zstandard
    pub zstd_size: usize,
    /// Original size of the data before compression
    pub original_size: usize,
}

impl FieldMetrics {
    /// Merge multiple [`FieldMetrics`] objects into one.
    /// This gives you an 'aggregate' result over a large data set.
    ///
    /// # Arguments
    ///
    /// * `self` - The object to merge into.
    /// * `other` - The object to merge from.
    pub fn try_merge_many(&mut self, others: &[&Self]) -> Result<(), AnalysisMergeError> {
        // Validate compatible field configurations
        for other in others {
            if self.lenbits != other.lenbits {
                return Err(AnalysisMergeError::FieldLengthMismatch(
                    self.lenbits,
                    other.lenbits,
                ));
            }
        }

        let total_items = others.len() + 1;
        let mut total_count = self.count;
        let mut total_entropy = self.entropy;
        let mut total_lz_matches = self.lz_matches;
        let mut total_estimated_size = self.estimated_size;
        let mut total_zstd_size = self.zstd_size;
        let mut total_original_size = self.original_size;

        for metrics in others {
            total_count += metrics.count;
            total_entropy += metrics.entropy;
            total_lz_matches += metrics.lz_matches;
            total_estimated_size += metrics.estimated_size;
            total_zstd_size += metrics.zstd_size;
            total_original_size += metrics.original_size;
        }

        self.count = total_count;
        self.entropy = total_entropy / total_items as f64;
        self.lz_matches = total_lz_matches / total_items;
        self.estimated_size = total_estimated_size / total_items;
        self.zstd_size = total_zstd_size / total_items;
        self.original_size = total_original_size / total_items;

        self.merge_bit_stats_and_value_counts(others)?;
        Ok(())
    }

    fn merge_bit_stats_and_value_counts(
        &mut self,
        others: &[&Self],
    ) -> Result<(), AnalysisMergeError> {
        let bit_counts = &mut self.bit_counts;
        let value_counts = &mut self.value_counts;

        for other in others {
            // Validate bit counts length
            if bit_counts.len() != other.bit_counts.len() {
                return Err(AnalysisMergeError::BitCountsDontMatch);
            }

            for (bit_offset, bit_stats) in other.bit_counts.iter().enumerate() {
                let current = bit_counts
                    .get_mut(bit_offset)
                    .ok_or(AnalysisMergeError::BitCountsDontMatch)?;
                current.ones += bit_stats.ones;
                current.zeros += bit_stats.zeros;
            }

            // Add value counts from others into self
            for (value, count) in &other.value_counts {
                *value_counts.entry(*value).or_insert(0) += count;
            }
        }
        Ok(())
    }

    /// Returns the parent path of the current field.
    /// The parent path is the part of the full path before the last dot.
    pub fn parent_path(&self) -> Option<&str> {
        self.full_path.rsplit_once(CHILD_MARKER).map(|(p, _)| p)
    }

    /// Returns the [`FieldMetrics`] object for the parent of the current field.
    /// Returns `None` if there is no parent.
    pub fn parent_metrics_or<'a>(
        &self,
        results: &'a AnalysisResults,
        optb: &'a FieldMetrics,
    ) -> &'a FieldMetrics {
        let parent_path = self.parent_path();
        let parent_stats = parent_path
            .and_then(|p| results.per_field.get(p))
            .unwrap_or(optb);
        parent_stats
    }

    /// Get sorted value counts descending (value, count)
    pub fn sorted_value_counts(&self) -> Vec<(&u64, &u64)> {
        let mut counts: Vec<_> = self.value_counts.iter().collect();
        counts.sort_by(|a, b| b.1.cmp(a.1));
        counts
    }
}

#[derive(Debug, Clone, Copy, Default, FromStr)]
pub enum PrintFormat {
    #[default]
    Detailed,
    Concise,
}
