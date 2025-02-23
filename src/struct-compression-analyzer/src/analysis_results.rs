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
//! use struct_compression_analyzer::analysis_results::AnalysisResults;
//! use struct_compression_analyzer::analyzer::AnalysisOptions;
//!
//! fn analyze_data(schema: &Schema, data: &[u8]) -> AnalysisResults {
//!     let options = AnalysisOptions { zstd_compression_level: 7 };
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

use crate::analyzer::AnalyzerFieldState;
use crate::analyzer::BitStats;
use crate::analyzer::SchemaAnalyzer;
use crate::comparison::compare_groups::analyze_custom_comparisons;
use crate::comparison::compare_groups::GroupComparisonError;
use crate::comparison::compare_groups::GroupComparisonResult;
use crate::comparison::split_comparison::make_split_comparison_result;
use crate::comparison::split_comparison::FieldComparisonMetrics;
use crate::comparison::split_comparison::SplitComparisonResult;
use crate::schema::BitOrder;
use crate::schema::Metadata;
use crate::schema::Schema;
use crate::schema::SplitComparison;
use crate::utils::analyze_utils::calculate_file_entropy;
use crate::utils::analyze_utils::get_writer_buffer;
use crate::utils::analyze_utils::get_zstd_compressed_size;
use crate::utils::analyze_utils::size_estimate;
use crate::utils::constants::CHILD_MARKER;
use ahash::{AHashMap, HashMapExt};
use derive_more::derive::FromStr;
use lossless_transform_utils::match_estimator::estimate_num_lz_matches_fast;
use rayon::iter::{IntoParallelRefMutIterator, ParallelIterator};
use rustc_hash::FxHashMap;
use thiserror::Error;

/// Final computed metrics for output
#[derive(Clone)]
pub struct AnalysisResults {
    /// Schema name
    pub schema_metadata: Metadata,

    /// Entropy of the whole file
    pub file_entropy: f64,

    /// LZ compression matches in the file
    pub file_lz_matches: usize,

    /// Estimated size of the compressed data from our estimator
    pub estimated_file_size: usize,

    /// Actual size of the compressed data when compressed with zstandard
    pub zstd_file_size: usize,

    /// Original size of the uncompressed data
    pub original_size: usize,

    /// Field path → computed metrics
    /// This is a map of `full_path` to [`FieldMetrics`], such that we
    /// can easily merge the results of different fields down the road.
    pub per_field: AHashMap<String, FieldMetrics>,

    /// Split comparison results
    pub split_comparisons: Vec<SplitComparisonResult>,

    /// Custom group comparison results from schema-defined comparisons
    pub custom_comparisons: Vec<GroupComparisonResult>,
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

/// Given a [`SchemaAnalyzer`] which has ingested all of the data to be calculated, via
/// the [`SchemaAnalyzer::add_entry`] function, compute the analysis results.
///
/// This returns the results for all of the per-field metrics, as well as computing the
/// various schema defined groups, such as 'split' groups and 'compare' groups.
pub fn compute_analysis_results(
    analyzer: &mut SchemaAnalyzer,
) -> Result<AnalysisResults, ComputeAnalysisResultsError> {
    // First calculate file entropy
    let file_entropy = calculate_file_entropy(&analyzer.entries);
    let file_lz_matches = estimate_num_lz_matches_fast(&analyzer.entries);

    // Then calculate per-field entropy and lz matches
    let mut field_metrics: AHashMap<String, FieldMetrics> = AHashMap::new();

    for stats in &mut analyzer.field_states.values_mut() {
        let writer_buffer = get_writer_buffer(&mut stats.writer);
        let entropy = calculate_file_entropy(writer_buffer);
        let lz_matches = estimate_num_lz_matches_fast(writer_buffer);
        let estimated_size = size_estimate(writer_buffer, lz_matches, entropy);
        let actual_size = get_zstd_compressed_size(writer_buffer);

        // reduce memory usage from leftover analyzer.
        stats.value_counts.shrink_to_fit();
        field_metrics.insert(
            stats.full_path.clone(),
            FieldMetrics {
                name: stats.name.clone(),
                full_path: stats.full_path.clone(),
                entropy,
                lz_matches,
                bit_counts: stats.bit_counts.clone(),
                value_counts: stats.value_counts.clone(),
                depth: stats.depth,
                count: stats.count,
                lenbits: stats.lenbits,
                bit_order: stats.bit_order,
                estimated_size,
                zstd_size: actual_size,
                original_size: writer_buffer.len(),
            },
        );
    }

    // Process split group comparisons
    let split_comparisons = calc_split_comparisons(
        &mut analyzer.field_states,
        &analyzer.schema.analysis.split_groups,
        &field_metrics,
    );

    // Process custom group comparisons
    let custom_comparisons =
        analyze_custom_comparisons(analyzer.schema, &mut analyzer.field_states)?;

    Ok(AnalysisResults {
        file_entropy,
        file_lz_matches,
        per_field: field_metrics,
        schema_metadata: analyzer.schema.metadata.clone(),
        estimated_file_size: size_estimate(&analyzer.entries, file_lz_matches, file_entropy),
        zstd_file_size: get_zstd_compressed_size(&analyzer.entries),
        original_size: analyzer.entries.len(),
        split_comparisons,
        custom_comparisons,
    })
}

/// Calculates the comparison results between a series of field splits.
///
/// This function takes the [`SchemaAnalyzer`]'s intermediate state, that is, the
/// state of each field (containing the data for each field), a list of split comparisons
/// to make, and the individual metrics (results) for each field.
///
/// This then computes the comparison results for each split.
///
/// # Remarks
/// This API is for internal use. It may change without notice.
///
/// # Arguments
/// * `field_stats` - The current field states (analyzer working state)
/// * `comparisons` - A slice of [`SplitComparison`] objects defining the splits to compare.
/// * `field_metrics` - A reference to a hash map of field metrics.
///
/// # Returns
/// A vector of [`SplitComparisonResult`] objects containing the comparison results.
///
/// [`SchemaAnalyzer`]: crate::analyzer::SchemaAnalyzer
fn calc_split_comparisons(
    field_stats: &mut AHashMap<String, AnalyzerFieldState>,
    comparisons: &[SplitComparison],
    field_metrics: &AHashMap<String, FieldMetrics>,
) -> Vec<SplitComparisonResult> {
    let mut split_comparisons = Vec::new();
    for comparison in comparisons {
        let mut group1_bytes: Vec<u8> = Vec::new();
        let mut group2_bytes: Vec<u8> = Vec::new();

        // Sum up bytes for group 1
        for name in &comparison.group_1 {
            if let Some(stats) = field_stats.get_mut(name) {
                group1_bytes.extend_from_slice(get_writer_buffer(&mut stats.writer));
            }
        }

        // Sum up bytes for group 2
        for name in &comparison.group_2 {
            if let Some(stats) = field_stats.get_mut(name) {
                group2_bytes.extend_from_slice(get_writer_buffer(&mut stats.writer));
            }
        }

        let mut group1_field_metrics: Vec<FieldComparisonMetrics> = Vec::new();
        let mut group2_field_metrics: Vec<FieldComparisonMetrics> = Vec::new();
        for path in &comparison.group_1 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group1_field_metrics.push(metrics.1.clone().into());
            }
        }
        for path in &comparison.group_2 {
            if let Some(metrics) = field_metrics.iter().find(|(_k, v)| v.name == *path) {
                group2_field_metrics.push(metrics.1.clone().into());
            }
        }

        split_comparisons.push(make_split_comparison_result(
            comparison.name.clone(),
            comparison.description.clone(),
            &group1_bytes,
            &group2_bytes,
            group1_field_metrics,
            group2_field_metrics,
        ));
    }
    split_comparisons
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
        get_parent_path(&self.full_path)
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

pub(crate) fn get_parent_path(field_path: &str) -> Option<&str> {
    field_path.rsplit_once(CHILD_MARKER).map(|(p, _)| p)
}

#[derive(Debug, Clone, Copy, Default, FromStr)]
pub enum PrintFormat {
    #[default]
    Detailed,
    Concise,
}

impl AnalysisResults {
    /// Merge multiple [`AnalysisResults`] objects into one.
    /// This is useful when analyzing multiple files or groups of fields.
    /// With this you can 'aggregate' results over a large data set.
    pub fn try_merge_many(&mut self, others: &[Self]) -> Result<(), AnalysisMergeError> {
        self.per_field
            .par_iter_mut()
            .try_for_each(|(full_path, field_metrics)| {
                // Get all matching `full_path` from all other elements as vec
                let matches: Vec<&FieldMetrics> = others
                    .iter()
                    .flat_map(|results| results.per_field.get(full_path))
                    .collect();

                field_metrics.try_merge_many(&matches)
            })?;

        // Merge the file entropy and LZ matches
        self.file_entropy = (self.file_entropy
            + others.iter().map(|m| m.file_entropy).sum::<f64>())
            / (others.len() + 1) as f64;
        self.file_lz_matches = (self.file_lz_matches
            + others.iter().map(|m| m.file_lz_matches).sum::<usize>())
            / (others.len() + 1);

        Ok(())
    }

    /// Converts the file level statistics into a [`FieldMetrics`] object
    /// which can be used for comparison with parent in places such as the
    /// print function.
    pub fn as_field_metrics(&self) -> FieldMetrics {
        FieldMetrics {
            name: String::new(),
            full_path: String::new(),
            depth: 0,
            zstd_size: self.zstd_file_size,
            estimated_size: self.estimated_file_size,
            original_size: self.original_size,
            count: 0,
            lenbits: 0,
            entropy: self.file_entropy,
            lz_matches: self.file_lz_matches,
            bit_counts: Vec::new(),
            bit_order: BitOrder::Default,
            value_counts: FxHashMap::new(),
        }
    }

    pub fn print(&self, schema: &Schema, format: PrintFormat, skip_misc_stats: bool) {
        match format {
            PrintFormat::Detailed => {
                self.print_detailed(schema, &self.as_field_metrics(), skip_misc_stats)
            }
            PrintFormat::Concise => {
                self.print_concise(schema, &self.as_field_metrics(), skip_misc_stats)
            }
        }
    }

    fn print_detailed(&self, schema: &Schema, file_metrics: &FieldMetrics, skip_misc_stats: bool) {
        println!("Schema: {}", self.schema_metadata.name);
        println!("Description: {}", self.schema_metadata.description);
        println!("File Entropy: {:.2} bits", self.file_entropy);
        println!("File LZ Matches: {}", self.file_lz_matches);
        println!("File Original Size: {}", self.original_size);
        println!("File Compressed Size: {}", self.zstd_file_size);
        println!("File Estimated Size: {}", self.estimated_file_size);
        println!("\nPer-field Metrics (in schema order):");

        // Iterate through schema-defined fields in order
        for field_path in schema.ordered_field_and_group_paths() {
            self.detailed_print_field(file_metrics, &field_path);
        }

        println!("\nSplit Group Comparisons:");
        for comparison in &self.split_comparisons {
            detailed_print_comparison(comparison);
        }

        println!("\nCustom Group Comparisons:");
        for comparison in &self.custom_comparisons {
            concise_print_custom_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(&field_path);
            }
        }
    }

    fn detailed_print_field(&self, file_metrics: &FieldMetrics, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            // Indent based on field depth to show hierarchy
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_or(self, file_metrics);

            // Calculate percentages
            println!(
                "{}{}: {:.2} bit entropy, {} LZ 3 Byte matches ({:.2}%)",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64)
            );
            let padding = format!("{}{}", indent, field.name).len() + 2; // +2 for ": "
            println!(
                "{:padding$}Sizes: Estimated/ZStandard -16/Original: {}/{}/{} ({:.2}%/{:.2}%/{:.2}%)",
                "",
                field.estimated_size,
                field.zstd_size,
                field.original_size,
                calculate_percentage(
                    field.estimated_size as f64,
                    parent_stats.estimated_size as f64
                ),
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(
                    field.original_size as f64,
                    parent_stats.original_size as f64
                )
            );
            println!(
                "{:padding$}{} bit, {} unique values, {:?}",
                "",
                field.lenbits,
                field.value_counts.len(),
                field.bit_order
            );
        }
    }

    fn print_concise(&self, schema: &Schema, file_metrics: &FieldMetrics, skip_misc_stats: bool) {
        println!("Schema: {}", self.schema_metadata.name);
        println!(
            "File: {:.2}bpb, {} LZ, {}/{}/{} ({:.2}%/{:.2}%/{:.2}%) (est/zstd/orig)",
            self.file_entropy,
            self.file_lz_matches,
            self.estimated_file_size,
            self.zstd_file_size,
            self.original_size,
            calculate_percentage(self.estimated_file_size as f64, self.original_size as f64),
            calculate_percentage(self.zstd_file_size as f64, self.original_size as f64),
            100.0
        );

        println!("\nField Metrics:");
        for field_path in schema.ordered_field_and_group_paths() {
            self.concise_print_field(file_metrics, &field_path);
        }

        println!("\nSplit Group Comparisons:");
        for comparison in &self.split_comparisons {
            concise_print_split_comparison(comparison);
        }

        println!("\nCustom Group Comparisons:");
        for comparison in &self.custom_comparisons {
            concise_print_custom_comparison(comparison);
        }

        if !skip_misc_stats {
            println!("\nField Value Stats: [as `value: probability %`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_value_stats(&field_path);
            }

            println!("\nField Bit Stats: [as `(zeros/ones) (percentage %)`]");
            for field_path in schema.ordered_field_and_group_paths() {
                self.concise_print_field_bit_stats(&field_path);
            }
        }
    }

    fn concise_print_field(&self, file_metrics: &FieldMetrics, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            let indent = "  ".repeat(field.depth);
            let parent_stats = field.parent_metrics_or(self, file_metrics);

            println!(
                "{}{}: {:.2}bpb, {} LZ ({:.2}%), {}/{}/{} ({:.2}%/{:.2}%/{:.2}%) (est/zstd/orig), {}bit",
                indent,
                field.name,
                field.entropy,
                field.lz_matches,
                calculate_percentage(field.lz_matches as f64, parent_stats.lz_matches as f64),
                field.estimated_size,
                field.zstd_size,
                field.original_size,
                calculate_percentage(field.estimated_size as f64, parent_stats.estimated_size as f64),
                calculate_percentage(field.zstd_size as f64, parent_stats.zstd_size as f64),
                calculate_percentage(field.original_size as f64, parent_stats.original_size as f64),
                field.lenbits
            );
        }
    }

    fn concise_print_field_value_stats(&self, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_value_stats(field);
        }
    }

    fn concise_print_field_bit_stats(&self, field_path: &str) {
        if let Some(field) = self.per_field.get(field_path) {
            print_field_metrics_bit_stats(field);
        }
    }
}

// Helper function to calculate percentage
fn calculate_percentage(child: f64, parent: f64) -> f64 {
    if parent == 0.0 {
        0.0
    } else {
        (child / parent) * 100.0
    }
}

fn detailed_print_comparison(comparison: &SplitComparisonResult) {
    concise_print_split_comparison(comparison);
}

fn print_field_metrics_value_stats(field: &FieldMetrics) {
    // Print field name with indent
    let indent = "  ".repeat(field.depth);
    println!("{}{} ({} bits)", indent, field.name, field.lenbits);

    // Print value statistics
    let counts = field.sorted_value_counts();
    if !counts.is_empty() {
        let total_values: u64 = counts.iter().map(|(_, &c)| c).sum();
        for (val, &count) in counts.iter().take(5) {
            let pct = (count as f32 / total_values as f32) * 100.0;
            println!("{}    {}: {:.1}%", indent, val, pct);
        }
    }
}

fn print_field_metrics_bit_stats(field: &FieldMetrics) {
    let indent = "  ".repeat(field.depth);
    println!("{}{} ({} bits)", indent, field.name, field.lenbits);

    // If we didn't collect the bits, skip printing.
    if field.bit_counts.len() != field.lenbits as usize {
        return;
    }

    for i in 0..field.lenbits {
        let bit_stats = &field.bit_counts[i as usize];
        let total = bit_stats.zeros + bit_stats.ones;
        let percentage = if total > 0 {
            (bit_stats.ones as f64 / total as f64) * 100.0
        } else {
            0.0
        };
        println!(
            "{}  Bit {}: ({}/{}) ({:.1}%)",
            indent, i, bit_stats.zeros, bit_stats.ones, percentage
        );
    }
}

fn concise_print_custom_comparison(comparison: &GroupComparisonResult) {
    let base_lz = comparison.baseline_metrics.lz_matches;
    let base_entropy = comparison.baseline_metrics.entropy;
    let base_est = comparison.baseline_metrics.estimated_size;
    let base_zstd = comparison.baseline_metrics.zstd_size;
    let base_size = comparison.baseline_metrics.original_size;

    println!("  {}: {}", comparison.name, comparison.description);
    println!("    Base Group:");
    println!("      Size: {}", base_size);
    println!("      LZ, Entropy: ({}, {:.2})", base_lz, base_entropy);
    println!("      Est/Zstd: ({}/{})", base_est, base_zstd);

    for (i, (group_name, metrics)) in comparison
        .group_names
        .iter()
        .zip(&comparison.group_metrics)
        .enumerate()
    {
        let comp_lz = metrics.lz_matches;
        let comp_entropy = metrics.entropy;
        let comp_est = metrics.estimated_size;
        let comp_zstd = metrics.zstd_size;
        let comp_size = metrics.original_size;

        let ratio_est = calculate_percentage(comp_est as f64, base_est as f64);
        let ratio_zstd = calculate_percentage(comp_zstd as f64, base_zstd as f64);
        let diff_est = comparison.differences[i].estimated_size;
        let diff_zstd = comparison.differences[i].zstd_size;

        println!("\n    {} Group:", group_name);
        println!("      Size: {}", comp_size);
        println!("      LZ, Entropy: ({}, {:.2})", comp_lz, comp_entropy);
        println!("      Est/Zstd: ({}/{})", comp_est, comp_zstd);
        println!(
            "      Ratio (est/zstd): ({:.1}%/{:.1}%)",
            ratio_est, ratio_zstd
        );
        println!("      Diff (est/zstd): ({}/{})", diff_est, diff_zstd);

        if base_size != comp_size {
            println!("      [WARNING!!] Sizes of base and comparison groups don't match!! They may vary by a few bytes due to padding.");
            println!("      [WARNING!!] However if they vary extremely, your groups may be incorrect. base: {}, {}: {}", base_size, group_name, comp_size);
        }
    }
}

fn concise_print_split_comparison(comparison: &SplitComparisonResult) {
    let base_lz = comparison.group1_metrics.lz_matches;
    let size_orig = comparison.group1_metrics.original_size;
    let size_comp = comparison.group2_metrics.original_size;
    let base_entropy = comparison.group1_metrics.entropy;

    let base_est = comparison.group1_metrics.estimated_size;
    let base_zstd = comparison.group1_metrics.zstd_size;

    let comp_lz = comparison.group2_metrics.lz_matches;
    let comp_entropy = comparison.group2_metrics.entropy;

    let comp_est = comparison.group2_metrics.estimated_size;
    let comp_zstd = comparison.group2_metrics.zstd_size;

    let ratio_est = calculate_percentage(comp_est as f64, base_est as f64);
    let ratio_zstd = calculate_percentage(comp_zstd as f64, base_zstd as f64);

    let diff_est = comparison.difference.estimated_size;
    let diff_zstd = comparison.difference.zstd_size;

    println!("  {}: {}", comparison.name, comparison.description);
    println!("    Original Size: {}", size_orig);
    println!("    Base LZ, Entropy: ({}, {:.2}):", base_lz, base_entropy);
    println!("    Comp LZ, Entropy: ({}, {:.2}):", comp_lz, comp_entropy);
    println!(
        "    Base Group LZ, Entropy: ({:?}, {:?})",
        comparison
            .baseline_comparison_metrics
            .iter()
            .map(|m| m.lz_matches)
            .collect::<Vec<_>>(),
        comparison
            .baseline_comparison_metrics
            .iter()
            .map(|m| format!("{:.2}", m.entropy))
            .collect::<Vec<_>>()
    );
    println!(
        "    Comp Group LZ, Entropy: ({:?}, {:?})",
        comparison
            .split_comparison_metrics
            .iter()
            .map(|m| m.lz_matches)
            .collect::<Vec<_>>(),
        comparison
            .split_comparison_metrics
            .iter()
            .map(|m| format!("{:.2}", m.entropy))
            .collect::<Vec<_>>()
    );

    println!("    Base (est/zstd): ({}/{})", base_est, base_zstd);
    println!("    Comp (est/zstd): ({}/{})", comp_est, comp_zstd);
    println!("    Ratio (est/zstd): ({}/{})", ratio_est, ratio_zstd);
    println!("    Diff (est/zstd): ({}/{})", diff_est, diff_zstd);

    if size_orig != size_comp {
        println!("    [WARNING!!] Sizes of both groups in bytes don't match!! They may vary by a few bytes due to padding.");
        println!("    [WARNING!!] However if they vary extremely, your groups may be incorrect. group1: {}, group2: {}", size_orig, size_comp);
    }
}
